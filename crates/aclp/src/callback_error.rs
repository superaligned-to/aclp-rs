use std::error::Error;

/// A callback failure after admission, reported to the server's diagnostics hook.
///
/// Requests answer these with JSON-RPC `-32603` and never expose the detail to the caller.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CallbackError {
    /// The callback returned an error.
    #[error("ACLP callback for {method} failed")]
    Failed {
        /// The event method.
        method: &'static str,
        /// The callback's error.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The callback panicked or was cancelled by runtime shutdown.
    #[error("ACLP callback for {method} panicked or was cancelled")]
    Aborted {
        /// The event method.
        method: &'static str,
    },
    /// The typed parameters rejected schema-valid input, which indicates a generator defect.
    #[error("ACLP parameters for {method} do not fit their typed view")]
    Params {
        /// The event method.
        method: &'static str,
        /// The deserialization error.
        #[source]
        source: serde_json::Error,
    },
    /// The result failed to serialize, did not satisfy its schema, or exceeded 1 MiB.
    #[error("ACLP callback result for {method} does not satisfy its schema")]
    InvalidResult {
        /// The event method.
        method: &'static str,
    },
}
