use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use hyper::body::Bytes;
use serde_json::{Value, json};

use crate::callback_error::CallbackError;
use crate::callbacks::Callback;
use crate::config_error::ConfigError;
use crate::events::DiscoveryAuth;
use crate::json;
use crate::transport::Credential;
use crate::validation;

pub(crate) type ErrorHook = Arc<dyn Fn(&CallbackError) + Send + Sync>;

/// Server configuration: credentials, public host names, discovery and a diagnostics hook.
#[derive(Clone, Default)]
pub struct ServerOptions {
    pub(crate) authorization: Option<String>,
    pub(crate) on_error: Option<ErrorHook>,
    pub(crate) hosts: Vec<String>,
    pub(crate) discovery: Option<Discovery>,
}

/// What `GET /.well-known/aclp` publishes besides the registered methods.
#[derive(Debug, Clone)]
pub struct Discovery {
    endpoint: String,
    name: Option<String>,
    auth: Option<DiscoveryAuth>,
}

impl Discovery {
    /// Publishes `endpoint`, the absolute URL clients post ACLP messages to.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Discovery {
            endpoint: endpoint.into(),
            name: None,
            auth: None,
        }
    }

    /// Sets the display name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Replaces the default auth description, which lists `token` exactly when authorization is set.
    #[must_use]
    pub fn auth(mut self, auth: DiscoveryAuth) -> Self {
        self.auth = Some(auth);
        self
    }
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

    /// Also serves these public host names, from any peer and port, for deployments behind TLS.
    ///
    /// Public hosts require [`ServerOptions::authorization`]; loopback names stay loopback-only.
    #[must_use]
    pub fn hosts<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.hosts = names.into_iter().map(Into::into).collect();
        self
    }

    /// Serves a discovery document for the registered callbacks at `/.well-known/aclp`.
    #[must_use]
    pub fn discovery(mut self, discovery: Discovery) -> Self {
        self.discovery = Some(discovery);
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

pub(crate) fn discovery_document(
    callbacks: &HashMap<&'static str, Callback>,
    discovery: &Discovery,
    credential: Option<&Credential>,
) -> Result<Bytes, ConfigError> {
    let mut events: Vec<&str> = callbacks.keys().copied().collect();

    events.sort_unstable();

    let requests: Vec<&str> = events
        .iter()
        .copied()
        .filter(|m| callbacks.get(m).is_some_and(|cb| cb.result.is_some()))
        .collect();

    let auth = match &discovery.auth {
        Some(auth) => serde_json::to_value(auth).map_err(|_| ConfigError::Discovery)?,
        None if credential.is_some() => json!({ "methods": ["token"] }),
        None => json!({ "methods": [] }),
    };

    let mut doc = json!({ "endpoint": discovery.endpoint, "auth": auth, "events": events, "requests": requests });

    if let Some(name) = &discovery.name {
        doc["name"] = Value::from(name.as_str());
    }

    if !validation::is_discovery_document(&doc) {
        return Err(ConfigError::Discovery);
    }

    json::encode(&doc)
        .map(Bytes::from)
        .map_err(|_| ConfigError::Discovery)
}
