use std::collections::HashSet;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use hyper::body::Bytes;
use hyper::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use hyper::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::Notify;
use tokio::time::{Instant, timeout_at};
use uuid::Uuid;

use crate::client_error::{ClientError, FlushError};
use crate::client_options::{
    Authorization, AuthorizationFn, ClientOptions, ErrorHook, client_header,
};
use crate::config_error::ConfigError;
use crate::connection::{Endpoint, Reply};
use crate::event::Event;
use crate::json;
use crate::result_schema::ResultSchema;
use crate::validation;

const MAX_PENDING: usize = 64;

const MAX_TIMEOUT: Duration = Duration::from_millis(2_147_483_647);

const ACLP_CLIENT: HeaderName = HeaderName::from_static("aclp-client");

enum Credential {
    Fixed(HeaderValue),
    Dynamic(AuthorizationFn),
}

#[derive(Debug, Default)]
struct State {
    active: usize,
    pending: usize,
    failures: Vec<ClientError>,
}

struct Inner {
    endpoint: Endpoint,
    credential: Option<Credential>,
    identity: Option<HeaderValue>,
    events: Option<HashSet<String>>,
    timeout: Duration,
    on_error: Option<ErrorHook>,
    state: Mutex<State>,
    drained: Notify,
}

/// Sends typed ACLP lifecycle events to one callback server.
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
            .field("endpoint", &self.inner.endpoint)
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

fn sensitive(val: &str) -> Option<HeaderValue> {
    let mut val = HeaderValue::from_str(val).ok()?;

    val.set_sensitive(true);
    Some(val)
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
        let consented = self
            .events
            .as_ref()
            .is_none_or(|events| events.contains(P::METHOD) || P::METHOD.starts_with("client."));

        if !consented {
            return Err(ClientError::NotConsented { method: P::METHOD });
        }

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

    async fn headers(&self) -> Result<HeaderMap, ClientError> {
        let mut headers = HeaderMap::new();

        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        match &self.credential {
            Some(Credential::Fixed(val)) => {
                headers.insert(AUTHORIZATION, val.clone());
            }
            Some(Credential::Dynamic(provider)) => {
                let val = sensitive(&provider().await).ok_or(ClientError::Authorization)?;

                headers.insert(AUTHORIZATION, val);
            }
            None => {}
        }

        if let Some(identity) = &self.identity {
            headers.insert(ACLP_CLIENT, identity.clone());
        }

        Ok(headers)
    }

    async fn post(&self, body: Vec<u8>, deadline: Instant) -> Result<Reply, ClientError> {
        let send = async {
            let headers = self.headers().await?;

            self.endpoint
                .exchange(Method::POST, headers, Bytes::from(body))
                .await
        };

        timeout_at(deadline, send)
            .await
            .unwrap_or(Err(ClientError::Deadline))
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
    /// Creates a client for one endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] for a URL that is not `http` or `https`, carries credentials or a
    /// fragment, or needs TLS without the `tls` feature; a zero or overlong timeout; an
    /// authorization value or client identity that cannot be an HTTP header; or an unusable CA.
    pub fn new(opts: ClientOptions) -> Result<Self, ConfigError> {
        if opts.timeout.is_zero() || opts.timeout > MAX_TIMEOUT {
            return Err(ConfigError::Timeout);
        }

        let credential = match opts.authorization {
            Some(Authorization::Fixed(val)) => Some(Credential::Fixed(
                sensitive(&val).ok_or(ConfigError::Authorization)?,
            )),
            Some(Authorization::Dynamic(provider)) => Some(Credential::Dynamic(provider)),
            None => None,
        };

        let identity = opts
            .client
            .map(|client| {
                let valid =
                    serde_json::to_value(&client).is_ok_and(|v| validation::is_client_info(&v));

                valid
                    .then(|| HeaderValue::from_str(&client_header(&client)).ok())
                    .flatten()
                    .ok_or(ConfigError::Identity)
            })
            .transpose()?;

        validation::warm();

        Ok(Client {
            inner: Arc::new(Inner {
                endpoint: Endpoint::parse(&opts.url, opts.ca.as_deref())?,
                credential,
                identity,
                events: opts.events,
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

        if !reply.json {
            return Err(ClientError::ContentType);
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
    /// Returns [`ClientError::Capacity`], [`ClientError::NotConsented`],
    /// [`ClientError::InvalidParams`] or [`ClientError::TooLarge`] before anything is sent.
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
