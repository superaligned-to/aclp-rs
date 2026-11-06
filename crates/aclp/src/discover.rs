use std::time::Duration;

use hyper::Method;
use hyper::body::Bytes;
use hyper::header::{ACCEPT, HeaderMap, HeaderValue};

use crate::client_error::ClientError;
use crate::config_error::ConfigError;
use crate::connection::Endpoint;
use crate::events::DiscoveryDocument;
use crate::json;
use crate::validation;

/// The discovery path every ACLP server origin serves.
pub const DISCOVERY_PATH: &str = "/.well-known/aclp";

/// Options for [`discover`]: a deadline and extra trusted CA certificates.
#[derive(Debug, Clone)]
pub struct DiscoverOptions {
    timeout: Duration,
    ca: Option<String>,
}

impl Default for DiscoverOptions {
    fn default() -> Self {
        DiscoverOptions {
            timeout: Duration::from_secs(5),
            ca: None,
        }
    }
}

impl DiscoverOptions {
    /// Uses a five-second deadline and the bundled web roots.
    pub fn new() -> Self {
        DiscoverOptions::default()
    }

    /// Sets the deadline for the whole exchange.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Trusts these PEM certificates in addition to the bundled web roots.
    #[must_use]
    pub fn ca(mut self, pem: impl Into<String>) -> Self {
        self.ca = Some(pem.into());
        self
    }
}

/// Fetches and validates the discovery document at the origin of `url`.
///
/// # Errors
///
/// Returns [`ClientError`] for an unusable URL or CA, transport failures, the deadline, a status
/// other than 200, a non-JSON response or a document that fails `DiscoveryDocument` validation.
pub async fn discover(url: &str, opts: DiscoverOptions) -> Result<DiscoveryDocument, ClientError> {
    let endpoint = Endpoint::parse(url, opts.ca.as_deref())
        .map_err(|err: ConfigError| ClientError::transport(err))?
        .with_path(DISCOVERY_PATH);

    let mut headers = HeaderMap::new();

    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    let reply = tokio::time::timeout(
        opts.timeout,
        endpoint.exchange(Method::GET, headers, Bytes::new()),
    )
    .await
    .unwrap_or(Err(ClientError::Deadline))?;

    if reply.status != hyper::StatusCode::OK {
        return Err(ClientError::Status(reply.status.as_u16()));
    }

    if !reply.json {
        return Err(ClientError::ContentType);
    }

    let doc = json::decode(&reply.body).map_err(|_| ClientError::InvalidDiscovery)?;

    if !validation::is_discovery_document(&doc) {
        return Err(ClientError::InvalidDiscovery);
    }

    serde_json::from_value(doc).map_err(|_| ClientError::InvalidDiscovery)
}
