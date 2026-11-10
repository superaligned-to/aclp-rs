use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::client_error::ClientError;
use crate::events::ClientInfo;
use crate::on_error::OnError;

pub(crate) type ErrorHook = Arc<dyn Fn(&ClientError) + Send + Sync>;

pub(crate) type AuthorizationFn =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = String> + Send>> + Send + Sync>;

#[derive(Clone)]
pub(crate) enum Authorization {
    Fixed(String),
    Dynamic(AuthorizationFn),
}

/// Client configuration: an endpoint, credentials, identity, consent, a deadline, a failure policy
/// and a diagnostics hook.
#[derive(Clone)]
pub struct ClientOptions {
    pub(crate) url: String,
    pub(crate) authorization: Option<Authorization>,
    pub(crate) timeout: Duration,
    pub(crate) on_error: Option<ErrorHook>,
    pub(crate) failure: Option<OnError>,
    pub(crate) client: Option<ClientInfo>,
    pub(crate) events: Option<HashSet<String>>,
    pub(crate) ca: Option<String>,
}

impl ClientOptions {
    /// Targets an `http://` or `https://` URL without credentials or a fragment.
    ///
    /// A non-loopback endpoint should use `https` and requires credentials on the server side.
    pub fn new(url: impl Into<String>) -> Self {
        ClientOptions {
            url: url.into(),
            authorization: None,
            timeout: Duration::from_secs(5),
            on_error: None,
            failure: None,
            client: None,
            events: None,
            ca: None,
        }
    }

    /// Sends this exact `Authorization` header value with every call.
    #[must_use]
    pub fn authorization(mut self, val: impl Into<String>) -> Self {
        self.authorization = Some(Authorization::Fixed(val.into()));
        self
    }

    /// Computes the `Authorization` header value before every call, for tokens that refresh.
    ///
    /// The time it takes counts against the call's deadline.
    #[must_use]
    pub fn authorization_with<F, Fut>(mut self, provider: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = String> + Send + 'static,
    {
        self.authorization = Some(Authorization::Dynamic(Arc::new(move || {
            Box::pin(provider())
        })));
        self
    }

    /// Identifies the client and harness to the server in the `ACLP-Client` header.
    #[must_use]
    pub fn client(mut self, client: ClientInfo) -> Self {
        self.client = Some(client);
        self
    }

    /// Limits sends to the methods the user consented to; `client.*` methods are always allowed.
    #[must_use]
    pub fn events<I, S>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.events = Some(methods.into_iter().map(Into::into).collect());
        self
    }

    /// Trusts these PEM certificates for `https` endpoints in addition to the bundled web roots.
    #[must_use]
    pub fn ca(mut self, pem: impl Into<String>) -> Self {
        self.ca = Some(pem.into());
        self
    }

    /// Sets the per-call deadline, which starts before local validation; the default is five seconds.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets what a failed request does to the continuation it guards, for every method.
    ///
    /// Without it each method takes [`OnError::default_for`].
    #[must_use]
    pub fn failure_policy(mut self, policy: OnError) -> Self {
        self.failure = Some(policy);
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
            .field("failure", &self.failure)
            .field("client", &self.client)
            .field("events", &self.events)
            .finish_non_exhaustive()
    }
}

fn product(name: &str, version: Option<&str>) -> String {
    match version {
        Some(version) => format!("{name}/{version}"),
        None => name.to_owned(),
    }
}

/// Renders `ClientInfo` as `ACLP-Client` product tokens: the client, then the harness.
pub(crate) fn client_header(client: &ClientInfo) -> String {
    let mut tokens = vec![product(&client.name, client.version.as_deref())];

    if let Some(harness) = &client.harness {
        tokens.push(product(&harness.name, harness.version.as_deref()));
    }

    tokens.join(" ")
}
