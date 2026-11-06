use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header::{ALLOW, CONNECTION, CONTENT_TYPE, HeaderValue, WWW_AUTHENTICATE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::callback_error::CallbackError;
use crate::callbacks::{Callback, Callbacks};
use crate::config_error::ConfigError;
use crate::json;
use crate::result_schema::ResultSchema;
use crate::transport::{self, Credential, HttpFailure};
use crate::validation;

const MAX_ACTIVE_CALLBACKS: usize = 32;

const MAX_CONNECTIONS: usize = 64;

const HEADER_TIMEOUT: Duration = Duration::from_secs(5);

const MAX_HEADER_BYTES: usize = 16_384;

// Persistent accept failures such as descriptor exhaustion would otherwise spin the accept loop.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

type ErrorHook = Arc<dyn Fn(&CallbackError) + Send + Sync>;

type Reply = Response<Full<Bytes>>;

/// Server configuration: optional required credentials and a diagnostics hook.
#[derive(Clone, Default)]
pub struct ServerOptions {
    authorization: Option<String>,
    on_error: Option<ErrorHook>,
}

impl ServerOptions {
    /// Creates options without credentials that log callback failures with `tracing`.
    pub fn new() -> Self {
        ServerOptions::default()
    }

    /// Requires this exact `Authorization` header value, such as `Bearer <token>`, before dispatch.
    #[must_use]
    pub fn authorization(mut self, val: impl Into<String>) -> Self {
        self.authorization = Some(val.into());
        self
    }

    /// Receives each callback failure after admission instead of the `tracing` log.
    #[must_use]
    pub fn on_error(mut self, hook: impl Fn(&CallbackError) + Send + Sync + 'static) -> Self {
        self.on_error = Some(Arc::new(hook));
        self
    }
}

impl fmt::Debug for ServerOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerOptions")
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "<redacted>"),
            )
            .finish_non_exhaustive()
    }
}

struct Inner {
    callbacks: HashMap<&'static str, Callback>,
    credential: Option<Credential>,
    on_error: Option<ErrorHook>,
    capacity: Arc<Semaphore>,
    tasks: TaskTracker,
}

/// An ACLP callback server for loopback HTTP/1.1 connections.
///
/// The server validates every message before a callback runs. At most 32 callbacks run at once;
/// a slot stays reserved until its callback completes, even when the caller has gone away.
pub struct Server {
    inner: Arc<Inner>,
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Server")
            .field("methods", &self.inner.callbacks.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

fn empty(status: StatusCode) -> Reply {
    let mut res = Response::new(Full::default());

    *res.status_mut() = status;
    res
}

fn json_reply(status: StatusCode, body: Vec<u8>) -> Reply {
    let mut res = Response::new(Full::new(Bytes::from(body)));

    *res.status_mut() = status;
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    res
}

fn rpc_error(status: StatusCode, id: Option<&str>, code: i64, message: &str) -> Reply {
    let body = json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } });

    json_reply(status, body.to_string().into_bytes())
}

fn invalid(id: Option<&str>, code: i64, message: &str) -> Reply {
    match id {
        Some(id) => rpc_error(StatusCode::OK, Some(id), code, message),
        None => empty(StatusCode::BAD_REQUEST),
    }
}

impl Inner {
    fn report(&self, err: &CallbackError) {
        match &self.on_error {
            Some(hook) => hook(err),
            None => tracing::warn!(error = %err, "ACLP callback failed"),
        }
    }

    fn reject(&self, HttpFailure(status): HttpFailure) -> Reply {
        let mut res = empty(status);

        let headers = res.headers_mut();

        headers.insert(CONNECTION, HeaderValue::from_static("close"));

        if status == StatusCode::METHOD_NOT_ALLOWED {
            headers.insert(ALLOW, HeaderValue::from_static("POST"));
        }

        let challenge = self
            .credential
            .as_ref()
            .and_then(|c| HeaderValue::from_str(&format!("{} realm=\"aclp\"", c.scheme)).ok());

        if let (StatusCode::UNAUTHORIZED, Some(challenge)) = (status, challenge) {
            headers.insert(WWW_AUTHENTICATE, challenge);
        }

        res
    }

    fn spawn(&self, call: Invocation<'_>) -> JoinHandle<Result<Value, CallbackError>> {
        let Invocation {
            cb,
            method,
            params,
            permit,
        } = call;

        let running = tokio::spawn((cb.handle)(params));

        self.tasks.spawn(async move {
            let res = running
                .await
                .unwrap_or(Err(CallbackError::Aborted { method }));

            drop(permit);
            res
        })
    }

    async fn receive(
        self: Arc<Self>,
        req: Request<Incoming>,
        peer: SocketAddr,
        local: SocketAddr,
    ) -> Reply {
        match self.admit(req, peer, local).await {
            Ok(res) => res,
            Err(failure) => self.reject(failure),
        }
    }

    async fn admit(
        self: &Arc<Self>,
        req: Request<Incoming>,
        peer: SocketAddr,
        local: SocketAddr,
    ) -> Result<Reply, HttpFailure> {
        transport::validate(&req, peer, local, self.credential.as_ref())?;

        let permit = Arc::clone(&self.capacity)
            .try_acquire_owned()
            .map_err(|_| HttpFailure(StatusCode::TOO_MANY_REQUESTS))?;

        let body = transport::read_body(req.into_body()).await?;

        let Ok(mut msg) = json::decode(&body) else {
            return Ok(rpc_error(
                StatusCode::BAD_REQUEST,
                None,
                -32700,
                "Parse error",
            ));
        };

        if !validation::is_envelope(&msg) {
            if validation::is_notification(&msg) {
                return Ok(empty(StatusCode::BAD_REQUEST));
            }

            let id = validation::request_id(&msg);

            return Ok(rpc_error(
                StatusCode::BAD_REQUEST,
                id,
                -32600,
                "Invalid request",
            ));
        }

        let id = msg.get("id").and_then(Value::as_str).map(str::to_owned);

        let method = msg["method"].as_str().unwrap_or_default();

        if !validation::is_event_method(method) {
            return Ok(invalid(id.as_deref(), -32601, "Method not found"));
        }

        if !validation::validate_params(method, &msg["params"]) {
            return Ok(invalid(id.as_deref(), -32602, "Invalid params"));
        }

        let Some((&method, cb)) = self.callbacks.get_key_value(method) else {
            return Ok(invalid(id.as_deref(), -32601, "Method not found"));
        };

        let call = Invocation {
            cb,
            method,
            params: msg["params"].take(),
            permit,
        };

        Ok(match (id, &cb.result) {
            (None, _) => self.notify(call),
            (Some(id), Some(result)) => self.request(call, result, &id).await,
            (Some(id), None) => rpc_error(StatusCode::OK, Some(&id), -32601, "Method not found"),
        })
    }

    fn notify(self: &Arc<Self>, call: Invocation<'_>) -> Reply {
        let running = self.spawn(call);

        let inner = Arc::clone(self);

        self.tasks.spawn(async move {
            if let Ok(Err(err)) = running.await {
                inner.report(&err);
            }
        });

        empty(StatusCode::ACCEPTED)
    }

    async fn request(&self, call: Invocation<'_>, result: &ResultSchema<Value>, id: &str) -> Reply {
        let method = call.method;

        let outcome = self
            .spawn(call)
            .await
            .unwrap_or(Err(CallbackError::Aborted { method }));

        let body = outcome.and_then(|val| {
            if !result.is_valid(&val) {
                return Err(CallbackError::InvalidResult { method });
            }

            json::encode(&json!({ "jsonrpc": "2.0", "id": id, "result": val }))
                .map_err(|_| CallbackError::InvalidResult { method })
        });

        match body {
            Ok(body) => json_reply(StatusCode::OK, body),
            Err(err) => {
                self.report(&err);
                rpc_error(StatusCode::OK, Some(id), -32603, "Internal error")
            }
        }
    }
}

struct Invocation<'a> {
    cb: &'a Callback,
    method: &'static str,
    params: Value,
    permit: OwnedSemaphorePermit,
}

async fn connection(
    inner: Arc<Inner>,
    stream: TcpStream,
    shutdown: CancellationToken,
    _slot: OwnedSemaphorePermit,
) {
    let (Ok(peer), Ok(local)) = (stream.peer_addr(), stream.local_addr()) else {
        return;
    };

    let service = service_fn(move |req| {
        let inner = Arc::clone(&inner);

        async move { Ok::<_, Infallible>(inner.receive(req, peer, local).await) }
    });

    let conn = http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_TIMEOUT)
        .max_buf_size(MAX_HEADER_BYTES)
        .serve_connection(TokioIo::new(stream), service);

    tokio::pin!(conn);

    let res = tokio::select! {
        res = conn.as_mut() => res,
        () = shutdown.cancelled() => {
            conn.as_mut().graceful_shutdown();
            conn.await
        }
    };

    if let Err(err) = res {
        tracing::debug!(error = %err, "ACLP connection closed with an error");
    }
}

impl Server {
    /// Creates a server for `callbacks`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Authorization`] when the configured authorization is not an HTTP
    /// scheme followed by a single-line credential.
    pub fn new(callbacks: Callbacks, opts: ServerOptions) -> Result<Self, ConfigError> {
        let credential = opts
            .authorization
            .map(|v| Credential::parse(&v).ok_or(ConfigError::Authorization))
            .transpose()?;

        validation::warm();

        Ok(Server {
            inner: Arc::new(Inner {
                callbacks: callbacks.map,
                credential,
                on_error: opts.on_error,
                capacity: Arc::new(Semaphore::new(MAX_ACTIVE_CALLBACKS)),
                tasks: TaskTracker::new(),
            }),
        })
    }

    /// Accepts connections on `listener` until `shutdown` completes, then drains.
    ///
    /// Bind the listener to a loopback address; requests from other peers are rejected regardless.
    /// After `shutdown`, the server stops accepting, lets open connections finish their current
    /// exchange, and waits for every admitted callback, including notifications, to complete.
    /// Connections beyond 64 are closed on accept.
    pub async fn serve(self, listener: TcpListener, shutdown: impl Future<Output = ()>) {
        let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));

        let stop = CancellationToken::new();

        tokio::pin!(shutdown);

        loop {
            let accepted = tokio::select! {
                () = &mut shutdown => break,
                accepted = listener.accept() => accepted,
            };

            let stream = match accepted {
                Ok((stream, _)) => stream,
                Err(err) => {
                    tracing::warn!(error = %err, "ACLP accept failed");
                    tokio::time::sleep(ACCEPT_BACKOFF).await;
                    continue;
                }
            };

            let Ok(slot) = Arc::clone(&slots).try_acquire_owned() else {
                continue;
            };

            let inner = Arc::clone(&self.inner);

            self.inner
                .tasks
                .spawn(connection(inner, stream, stop.clone(), slot));
        }

        stop.cancel();
        self.inner.tasks.close();
        self.inner.tasks.wait().await;
    }
}
