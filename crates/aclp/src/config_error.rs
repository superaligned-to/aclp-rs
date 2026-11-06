/// Invalid client or server configuration, detected before any I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The URL is not `http` or `https`, carries credentials or a fragment, or needs the `tls` feature.
    #[error("ACLP requires an HTTP or HTTPS endpoint without URL credentials or a fragment")]
    Endpoint,
    /// The client timeout is zero or longer than 2,147,483,647 milliseconds.
    #[error("ACLP timeout must be positive and at most 2147483647 ms")]
    Timeout,
    /// The authorization value is not an HTTP scheme followed by a credential on one line.
    #[error("authorization must contain an HTTP scheme and credential")]
    Authorization,
    /// The client identity does not match `ClientInfo` or cannot be an HTTP header.
    #[error("ACLP client identity must match ClientInfo and be printable ASCII")]
    Identity,
    /// The CA bundle holds no usable PEM certificate.
    #[error("ACLP CA bundle has no usable PEM certificate")]
    Certificate,
    /// A public host name is invalid, or public hosts are configured without authorization.
    #[error("ACLP public hosts must be plain host names and require authorization")]
    Hosts,
    /// The configured discovery document does not satisfy `DiscoveryDocument`.
    #[error("ACLP discovery document is invalid")]
    Discovery,
}
