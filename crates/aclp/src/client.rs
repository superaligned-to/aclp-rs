use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use hyper::body::Bytes;
use hyper::header::{AUTHORIZATION, CONTENT_TYPE, HOST, HeaderMap, HeaderValue};
use hyper::{Request, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio::time::{Instant, timeout_at};
use uuid::Uuid;

use crate::client_error::{ClientError, FlushError};
use crate::config_error::ConfigError;
use crate::event::Event;
use crate::json::{self, MAX_BODY_BYTES};
use crate::result_schema::ResultSchema;
use crate::validation;

const MAX_PENDING: usize = 64;

const MAX_HEADER_BYTES: usize = 16_384;

const MAX_TIMEOUT: Duration = Duration::from_millis(2_147_483_647);

type ErrorHook = Arc<dyn Fn(&ClientError) + Send + Sync>;

/// Client configuration: a loopback endpoint, optional credentials, a deadline and a diagnostics hook.
#[derive(Clone)]
pub struct ClientOptions {
    url: String,
    authorization: Option<String>,
    timeout: Duration,
    on_error: Option<ErrorHook>,
}

impl ClientOptions {
    /// Targets `url`, which must be `http://` on `127.0.0.1`, `[::1]` or `localhost`.
    pub fn new(url: impl Into<String>) -> Self {
        ClientOptions {
            url: url.into(),
            authorization: None,
            timeout: Duration::from_secs(5),
            on_error: None,
        }
    }

    /// Sends this exact `Authorization` header value with every call.
    #[must_use]
    pub fn authorization(mut self, val: impl Into<String>) -> Self {
        self.authorization = Some(val.into());
        self
    }

    /// Sets the per-call deadline, which starts before local validation; the default is five seconds.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Receives each notification delivery failure; without it failures are logged with `tracing`.
    #[must_use]
    pub fn on_error(mut self, hook: impl Fn(&ClientError) + Send + Sync + 'static) -> Self {
        self.on_error = Some(Arc::new(hook));
        self
    }
}

impl fmt::Debug for ClientOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientOptions")
            .field("url", &self.url)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "<redacted>"),
            )
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Endpoint {
    addr: SocketAddr,
    host: HeaderValue,
    path: String,
}

#[derive(Debug, Default)]
struct State {
    active: usize,
    pending: usize,
    failures: Vec<ClientError>,
}

struct Inner {
    endpoint: Endpoint,
    authorization: Option<HeaderValue>,
    timeout: Duration,
    on_error: Option<ErrorHook>,
    state: Mutex<State>,
    drained: Notify,
}

/// Sends typed ACLP lifecycle events to one loopback callback server.
///
/// Clones share capacity, pending notifications and recorded failures. Requests are never retried
/// or redirected. Each call uses its own HTTP/1.1 connection.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("endpoint", &self.inner.endpoint.addr)
            .field("timeout", &self.inner.timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    Request,
    Notification,
}

struct Slot {
    inner: Arc<Inner>,
    call: Call,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let drained = {
            let mut state = self.inner.state();

            state.active -= 1;

            if self.call == Call::Notification {
                state.pending -= 1;
            }

            state.pending == 0
        };

        if drained {
            self.inner.drained.notify_waiters();
        }
    }
}

struct Reply {
    status: StatusCode,
    body: Bytes,
}

fn endpoint(url: &str) -> Result<Endpoint, ConfigError> {
    let uri: Uri = url.parse().map_err(|_| ConfigError::Endpoint)?;

    let authority = uri.authority().ok_or(ConfigError::Endpoint)?;

    let ip = match authority.host() {
        "127.0.0.1" | "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        "[::1]" => IpAddr::V6(Ipv6Addr::LOCALHOST),
        _ => return Err(ConfigError::Endpoint),
    };

    if uri.scheme_str() != Some("http") || authority.as_str().contains('@') || url.contains('#') {
        return Err(ConfigError::Endpoint);
    }

    Ok(Endpoint {
        addr: SocketAddr::new(ip, authority.port_u16().unwrap_or(80)),
        host: HeaderValue::from_str(authority.as_str()).map_err(|_| ConfigError::Endpoint)?,
        path: uri.path_and_query().map_or("/", |p| p.as_str()).to_owned(),
    })
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
}

impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn reserve(self: &Arc<Self>, call: Call) -> Result<Slot, ClientError> {
        let mut state = self.state();

        if state.active >= MAX_PENDING {
            return Err(ClientError::Capacity);
        }

        state.active += 1;

        if call == Call::Notification {
            state.pending += 1;
        }

        Ok(Slot {
            inner: Arc::clone(self),
            call,
        })
    }

    fn prepare<P: Event>(&self, params: &P, id: Option<&str>) -> Result<Vec<u8>, ClientError> {
        let invalid = ClientError::InvalidParams { method: P::METHOD };

        let params = serde_json::to_value(params).map_err(|_| invalid)?;

        let mut msg = json!({ "jsonrpc": "2.0", "method": P::METHOD, "params": params });

        if let Some(id) = id {
            msg["id"] = Value::from(id);
        }

        let body = json::encode(&msg).map_err(|_| ClientError::TooLarge)?;

        if !validation::validate_params(P::METHOD, &msg["params"]) {
            return Err(ClientError::InvalidParams { method: P::METHOD });
        }

        Ok(body)
    }

    async fn post(&self, body: Vec<u8>, deadline: Instant) -> Result<Reply, ClientError> {
        timeout_at(deadline, self.exchange(body))
            .await
            .unwrap_or(Err(ClientError::Deadline))
    }

    async fn exchange(&self, body: Vec<u8>) -> Result<Reply, ClientError> {
        let stream = TcpStream::connect(self.endpoint.addr)
            .await
            .map_err(ClientError::transport)?;

        let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
            .max_buf_size(MAX_HEADER_BYTES)
            .handshake(TokioIo::new(stream))
            .await
            .map_err(ClientError::transport)?;

        let mut req = Request::post(&self.endpoint.path)
            .header(HOST, &self.endpoint.host)
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(body)))
            .map_err(ClientError::transport)?;

        if let Some(authorization) = &self.authorization {
            req.headers_mut()
                .insert(AUTHORIZATION, authorization.clone());
        }

        let work = async move {
            let res = sender
                .send_request(req)
                .await
                .map_err(ClientError::transport)?;

            let status = res.status();

            if status == StatusCode::SWITCHING_PROTOCOLS {
                return Err(ClientError::Upgrade);
            }

            if status == StatusCode::OK && !is_json(res.headers()) {
                return Err(ClientError::ContentType);
            }

            let body = Limited::new(res.into_body(), MAX_BODY_BYTES)
                .collect()
                .await
                .map_err(|err| {
                    if err.is::<LengthLimitError>() {
                        ClientError::ResponseTooLarge
                    } else {
                        ClientError::Transport(err)
                    }
                })?
                .to_bytes();

            Ok(Reply { status, body })
        };

        let (res, _) = tokio::join!(work, conn);

        res
    }

    fn fail(&self, err: ClientError) {
        match &self.on_error {
            Some(hook) => hook(&err),
            None => tracing::warn!(error = %err, "ACLP notification delivery failed"),
        }

        let mut state = self.state();

        if state.failures.len() < MAX_PENDING {
            state.failures.push(err);
        }
    }
}

impl Client {
    /// Creates a client for one loopback endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] for a non-loopback or credential-bearing URL, a zero or overlong
    /// timeout, or an authorization value that cannot be an HTTP header.
    pub fn new(opts: ClientOptions) -> Result<Self, ConfigError> {
        if opts.timeout.is_zero() || opts.timeout > MAX_TIMEOUT {
            return Err(ConfigError::Timeout);
        }

        let authorization = opts
            .authorization
            .map(|v| HeaderValue::from_str(&v).map_err(|_| ConfigError::Authorization))
            .transpose()?
            .map(|mut v| {
                v.set_sensitive(true);
                v
            });

        validation::warm();

        Ok(Client {
            inner: Arc::new(Inner {
                endpoint: endpoint(&opts.url)?,
                authorization,
                timeout: opts.timeout,
                on_error: opts.on_error,
                state: Mutex::new(State::default()),
                drained: Notify::new(),
            }),
        })
    }

    /// Sends a request and returns its result once it matches `result`.
    ///
    /// The deadline covers validation, the HTTP exchange and result checking.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] for local capacity or validation failures, transport failures, the
    /// deadline, a non-JSON-RPC or mismatched reply, a callback error, or a result outside `result`.
    pub async fn request<P: Event, T: DeserializeOwned>(
        &self,
        params: &P,
        result: &ResultSchema<T>,
    ) -> Result<T, ClientError> {
        let deadline = Instant::now() + self.inner.timeout;

        let id = Uuid::new_v4().to_string();

        let _slot = self.inner.reserve(Call::Request)?;

        let body = self.inner.prepare(params, Some(&id))?;

        let reply = self.inner.post(body, deadline).await?;

        if reply.status != StatusCode::OK {
            return Err(ClientError::Status(reply.status.as_u16()));
        }

        let mut msg = json::decode(&reply.body).map_err(|_| ClientError::InvalidResponse)?;

        if !validation::is_response(&msg) || msg["id"] != id.as_str() {
            return Err(ClientError::InvalidResponse);
        }

        if let Some(err) = msg.get("error") {
            return Err(ClientError::Callback {
                code: err["code"].as_i64().unwrap_or_default(),
                message: err["message"].as_str().unwrap_or_default().to_owned(),
            });
        }

        let val = msg["result"].take();

        if !result.is_valid(&val) {
            return Err(ClientError::InvalidResult);
        }

        let typed = T::deserialize(val).map_err(|_| ClientError::InvalidResult)?;

        if Instant::now() >= deadline {
            return Err(ClientError::Deadline);
        }

        Ok(typed)
    }

    /// Validates and enqueues a notification, then returns without awaiting delivery.
    ///
    /// Delivery failures reach the diagnostics hook and the next [`Client::flush`].
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::Capacity`], [`ClientError::InvalidParams`] or
    /// [`ClientError::TooLarge`] before anything is sent.
    ///
    /// # Panics
    ///
    /// Panics when called outside a Tokio runtime, like [`tokio::spawn`].
    pub fn notify<P: Event>(&self, params: &P) -> Result<(), ClientError> {
        let deadline = Instant::now() + self.inner.timeout;

        let slot = self.inner.reserve(Call::Notification)?;

        let body = self.inner.prepare(params, None)?;

        let inner = Arc::clone(&self.inner);

        tokio::spawn(async move {
            let admitted = inner.post(body, deadline).await.and_then(|reply| {
                if reply.status == StatusCode::ACCEPTED && reply.body.is_empty() {
                    Ok(())
                } else {
                    Err(ClientError::Status(reply.status.as_u16()))
                }
            });

            if let Err(err) = admitted {
                inner.fail(err);
            }

            drop(slot);
        });

        Ok(())
    }

    /// Waits for every outstanding notification admission attempt, including ones queued meanwhile.
    ///
    /// Success confirms server admission, not callback completion. Call it before process exit;
    /// accepted notifications can still be lost if the server restarts.
    ///
    /// # Errors
    ///
    /// Returns [`FlushError`] with the delivery failures recorded since the previous flush.
    pub async fn flush(&self) -> Result<(), FlushError> {
        loop {
            let drained = self.inner.drained.notified();

            if self.inner.state().pending == 0 {
                break;
            }

            drained.await;
        }

        let failures = std::mem::take(&mut self.inner.state().failures);

        if failures.is_empty() {
            Ok(())
        } else {
            Err(FlushError::new(failures))
        }
    }
}
