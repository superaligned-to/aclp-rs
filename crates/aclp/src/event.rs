use serde::Serialize;
use serde::de::DeserializeOwned;

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// A standard ACLP event whose parameters type names its method.
///
/// Implemented only for the generated types in [`crate::events`].
pub trait Event: sealed::Sealed + Serialize + DeserializeOwned + Send + 'static {
    /// The JSON-RPC method that carries this event.
    const METHOD: &'static str;
}
