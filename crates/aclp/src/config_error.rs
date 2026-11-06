/// Invalid client or server configuration, detected before any I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The client URL is not a loopback HTTP endpoint without credentials or a fragment.
    #[error("ACLP requires a loopback HTTP endpoint without URL credentials or a fragment")]
    Endpoint,
    /// The client timeout is zero or longer than 2,147,483,647 milliseconds.
    #[error("ACLP timeout must be positive and at most 2147483647 ms")]
    Timeout,
    /// The authorization value is not an HTTP scheme followed by a credential on one line.
    #[error("authorization must contain an HTTP scheme and credential")]
    Authorization,
}
