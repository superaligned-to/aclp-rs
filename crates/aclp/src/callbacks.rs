use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::callback_error::CallbackError;
use crate::event::Event;
use crate::result_schema::ResultSchema;

type Outcome = Pin<Box<dyn Future<Output = Result<Value, CallbackError>> + Send>>;

pub(crate) type Handler = Arc<dyn Fn(Value, CallContext) -> Outcome + Send + Sync>;

/// What the transport knows about one call besides its parameters.
///
/// The client identity is self-reported; authorize on credentials, never on it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CallContext {
    /// The raw `ACLP-Client` header, when present.
    pub client: Option<String>,
}

#[derive(Clone)]
pub(crate) struct Callback {
    pub(crate) handle: Handler,
    pub(crate) result: Option<ResultSchema<Value>>,
}

/// The callbacks a server invokes, at most one per event method.
///
/// A request callback also installs the result schema its replies must satisfy; a notification
/// callback serves only notifications. Registering a method again replaces its callback.
#[derive(Clone, Default)]
pub struct Callbacks {
    pub(crate) map: HashMap<&'static str, Callback>,
}

impl fmt::Debug for Callbacks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.map.keys()).finish()
    }
}

fn erase<P, T, F, Fut, E>(handle: F) -> Handler
where
    P: Event,
    T: Serialize,
    F: Fn(P, CallContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    E: Into<Box<dyn Error + Send + Sync>>,
{
    let method = P::METHOD;

    Arc::new(move |params: Value, ctx: CallContext| -> Outcome {
        let call = serde_json::from_value::<P>(params).map(|p| handle(p, ctx));

        Box::pin(async move {
            let call = call.map_err(|source| CallbackError::Params { method, source })?;

            let val = call.await.map_err(|err| CallbackError::Failed {
                method,
                source: err.into(),
            })?;

            serde_json::to_value(val).map_err(|_| CallbackError::InvalidResult { method })
        })
    })
}

impl Callbacks {
    /// Creates an empty set; the server answers every method as not found.
    pub fn new() -> Self {
        Callbacks::default()
    }

    /// Serves requests and notifications of `P`; requests reply with the result once it satisfies `result`.
    #[must_use]
    pub fn request<P, T, F, Fut, E>(self, result: &ResultSchema<T>, handle: F) -> Self
    where
        P: Event,
        T: Serialize,
        F: Fn(P) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        self.request_with(result, move |p, _| handle(p))
    }

    /// Like [`Callbacks::request`], and the callback also receives the call's [`CallContext`].
    #[must_use]
    pub fn request_with<P, T, F, Fut, E>(mut self, result: &ResultSchema<T>, handle: F) -> Self
    where
        P: Event,
        T: Serialize,
        F: Fn(P, CallContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        self.map.insert(
            P::METHOD,
            Callback {
                handle: erase(handle),
                result: Some(result.erase()),
            },
        );
        self
    }

    /// Serves notifications of `P`; requests for `P` are answered as method not found.
    #[must_use]
    pub fn notification<P, F, Fut, E>(self, handle: F) -> Self
    where
        P: Event,
        F: Fn(P) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        self.notification_with(move |p, _| handle(p))
    }

    /// Like [`Callbacks::notification`], and the callback also receives the call's [`CallContext`].
    #[must_use]
    pub fn notification_with<P, F, Fut, E>(mut self, handle: F) -> Self
    where
        P: Event,
        F: Fn(P, CallContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        self.map.insert(
            P::METHOD,
            Callback {
                handle: erase(handle),
                result: None,
            },
        );
        self
    }
}
