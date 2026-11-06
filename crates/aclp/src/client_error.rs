use std::error::Error;

/// A failed request, notification admission attempt or local delivery precondition.
///
/// No failure permits a guarded harness continuation; the caller keeps it blocked.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClientError {
    /// 64 requests or notification attempts are already outstanding.
    #[error("ACLP client capacity exhausted")]
    Capacity,
    /// The parameters do not satisfy the event's schema or semantic rules.
    #[error("invalid ACLP parameters for {method}")]
    InvalidParams {
        /// The event method.
        method: &'static str,
    },
    /// The encoded message exceeds the protocol's size or nesting bounds.
    #[error("ACLP message exceeds protocol bounds")]
    TooLarge,
    /// The deadline passed before a valid reply was received and checked.
    #[error("ACLP request deadline exceeded")]
    Deadline,
    /// The connection failed or closed before a complete response.
    #[error("ACLP transport failed")]
    Transport(#[source] Box<dyn Error + Send + Sync>),
    /// The server answered with an HTTP protocol upgrade.
    #[error("ACLP does not support HTTP protocol upgrades")]
    Upgrade,
    /// A request did not receive HTTP 200, or a notification an empty HTTP 202.
    #[error("ACLP call was not accepted (HTTP {0})")]
    Status(u16),
    /// A result response lacked `Content-Type: application/json`.
    #[error("ACLP result requires Content-Type application/json")]
    ContentType,
    /// The response body exceeds 1 MiB.
    #[error("ACLP response exceeds 1 MiB")]
    ResponseTooLarge,
    /// The response is not valid JSON-RPC or does not echo the request id.
    #[error("invalid or mismatched ACLP response")]
    InvalidResponse,
    /// The callback answered with a JSON-RPC error.
    #[error("ACLP callback failed ({code}): {message}")]
    Callback {
        /// The JSON-RPC error code.
        code: i64,
        /// The server's error message.
        message: String,
    },
    /// The result does not match the installed result schema or its typed view.
    #[error("ACLP callback result does not match its installed schema")]
    InvalidResult,
}

impl ClientError {
    pub(crate) fn transport(err: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        ClientError::Transport(err.into())
    }
}

/// Notification deliveries that failed since the previous flush.
#[derive(Debug, thiserror::Error)]
#[error("ACLP notification delivery failed ({} failures)", failures.len())]
pub struct FlushError {
    failures: Vec<ClientError>,
}

impl FlushError {
    pub(crate) fn new(failures: Vec<ClientError>) -> Self {
        FlushError { failures }
    }

    /// The individual delivery failures, oldest first, at most 64.
    pub fn failures(&self) -> &[ClientError] {
        &self.failures
    }

    /// Takes the individual delivery failures.
    pub fn into_failures(self) -> Vec<ClientError> {
        self.failures
    }
}
