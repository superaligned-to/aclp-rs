use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use http_body_util::{BodyExt, LengthLimitError, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HOST, HeaderValue, ORIGIN};
use hyper::{Method, Request, StatusCode};
use sha2::{Digest, Sha256};

use crate::json::MAX_BODY_BYTES;

const READ_TIMEOUT: Duration = Duration::from_secs(5);

const HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];

/// An HTTP-level rejection answered with an empty body and a closed connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HttpFailure(pub(crate) StatusCode);

/// A configured `Authorization` value, compared by digest so timing reveals neither content nor length.
#[derive(Clone)]
pub(crate) struct Credential {
    digest: [u8; 32],
    pub(crate) scheme: String,
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn is_token(scheme: &str) -> bool {
    !scheme.is_empty()
        && scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+.^_`|~-".contains(&b))
}

impl Credential {
    pub(crate) fn parse(val: &str) -> Option<Self> {
        let (scheme, rest) = val.split_once(' ')?;

        let credential = rest.trim_start_matches(' ');

        let starts = credential
            .chars()
            .next()
            .is_some_and(|c| !c.is_whitespace());

        if !is_token(scheme) || !starts || credential.contains(['\r', '\n']) {
            return None;
        }

        Some(Credential {
            digest: digest(val.as_bytes()),
            scheme: scheme.to_owned(),
        })
    }

    fn matches(&self, supplied: &[u8]) -> bool {
        let supplied = digest(supplied);

        self.digest
            .iter()
            .zip(supplied.iter())
            .fold(0, |acc, (x, y)| acc | (x ^ y))
            == 0
    }
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4 == Ipv4Addr::LOCALHOST,
        IpAddr::V6(v6) => {
            v6 == Ipv6Addr::LOCALHOST || v6.to_ipv4_mapped() == Some(Ipv4Addr::LOCALHOST)
        }
    }
}

fn host_matches(val: &HeaderValue, local_port: u16) -> bool {
    let Ok(val) = val.to_str() else {
        return false;
    };

    let (host, port) = match val.strip_prefix('[') {
        Some(literal) => match literal.split_once(']') {
            Some((addr, "")) => (format!("[{addr}]"), None),
            Some((addr, after)) => match after.strip_prefix(':') {
                Some(port) => (format!("[{addr}]"), Some(port)),
                None => return false,
            },
            None => return false,
        },
        None => match val.split_once(':') {
            Some((host, port)) => (host.to_owned(), Some(port)),
            None => (val.to_owned(), None),
        },
    };

    let port = match port {
        None => Some(80),
        Some(p) if p.starts_with('0') || !p.bytes().all(|b| b.is_ascii_digit()) => None,
        Some(p) => p.parse::<u16>().ok(),
    };

    HOSTS.iter().any(|h| host.eq_ignore_ascii_case(h)) && port == Some(local_port)
}

fn is_json(val: Option<&HeaderValue>) -> bool {
    val.and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
}

/// Checks peers, request target, origin, credentials, method, media type and declared size.
pub(crate) fn validate(
    req: &Request<Incoming>,
    peer: SocketAddr,
    local: SocketAddr,
    credential: Option<&Credential>,
) -> Result<(), HttpFailure> {
    let forbidden = Err(HttpFailure(StatusCode::FORBIDDEN));

    if !is_loopback(peer.ip()) || !is_loopback(local.ip()) {
        return forbidden;
    }

    let headers = req.headers();

    let mut hosts = headers.get_all(HOST).iter();

    let host_ok =
        hosts.next().is_some_and(|h| host_matches(h, local.port())) && hosts.next().is_none();

    let path = req.uri().path();

    let origin_form =
        req.uri().authority().is_none() && path.starts_with('/') && !path.starts_with("//");

    if !host_ok || !origin_form || headers.contains_key(ORIGIN) {
        return forbidden;
    }

    if let Some(credential) = credential {
        let mut supplied = headers.get_all(AUTHORIZATION).iter();

        let first = supplied.next().map_or(&b""[..], HeaderValue::as_bytes);

        if !credential.matches(first) || supplied.next().is_some() {
            return Err(HttpFailure(StatusCode::UNAUTHORIZED));
        }
    }

    if req.method() != Method::POST {
        return Err(HttpFailure(StatusCode::METHOD_NOT_ALLOWED));
    }

    if !is_json(headers.get(CONTENT_TYPE)) {
        return Err(HttpFailure(StatusCode::UNSUPPORTED_MEDIA_TYPE));
    }

    let too_large = headers
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.parse::<usize>().map_or(true, |n| n > MAX_BODY_BYTES));

    if too_large {
        return Err(HttpFailure(StatusCode::PAYLOAD_TOO_LARGE));
    }

    Ok(())
}

/// Reads at most 1 MiB within five seconds of the request head.
pub(crate) async fn read_body(body: Incoming) -> Result<Bytes, HttpFailure> {
    match tokio::time::timeout(READ_TIMEOUT, Limited::new(body, MAX_BODY_BYTES).collect()).await {
        Err(_) => Err(HttpFailure(StatusCode::REQUEST_TIMEOUT)),
        Ok(Err(err)) if err.is::<LengthLimitError>() => {
            Err(HttpFailure(StatusCode::PAYLOAD_TOO_LARGE))
        }
        Ok(Err(_)) => Err(HttpFailure(StatusCode::BAD_REQUEST)),
        Ok(Ok(collected)) => Ok(collected.to_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_must_name_loopback_and_the_listening_port() {
        let ok = |v: &str, port| host_matches(&HeaderValue::from_str(v).expect("header"), port);

        assert!(ok("127.0.0.1:8787", 8787));
        assert!(ok("LOCALHOST:8787", 8787));
        assert!(ok("[::1]:8787", 8787));
        assert!(ok("localhost", 80));
        assert!(!ok("localhost", 8787));
        assert!(!ok("localhost:8788", 8787));
        assert!(!ok("localhost:08787", 8787));
        assert!(!ok("example.com:8787", 8787));
        assert!(!ok("127.0.0.2:8787", 8787));
        assert!(!ok("[::1]8787", 8787));
        assert!(!ok("user@localhost:8787", 8787));
        assert!(!ok("localhost:8787/x", 8787));
    }

    #[test]
    fn authorization_needs_a_scheme_and_a_single_line_credential() {
        assert_eq!(
            Credential::parse("Bearer token").map(|c| c.scheme),
            Some("Bearer".to_owned())
        );
        assert!(Credential::parse("Bearer   token with spaces").is_some());

        for invalid in [
            "missing-scheme",
            "Bearer ",
            "Bearer credential\r\nHost: attacker",
            "Bad(scheme) x",
            " Bearer x",
        ] {
            assert!(Credential::parse(invalid).is_none(), "{invalid:?}");
        }
    }

    #[test]
    fn credentials_match_only_the_exact_value() {
        let credential = Credential::parse("Bearer token").expect("valid");

        assert!(credential.matches(b"Bearer token"));
        assert!(!credential.matches(b"Bearer token2"));
        assert!(!credential.matches(b""));
    }

    #[test]
    fn loopback_includes_ipv4_mapped_localhost_only() {
        assert!(is_loopback("127.0.0.1".parse().expect("ip")));
        assert!(is_loopback("::1".parse().expect("ip")));
        assert!(is_loopback("::ffff:127.0.0.1".parse().expect("ip")));
        assert!(!is_loopback("127.0.0.2".parse().expect("ip")));
        assert!(!is_loopback("10.0.0.1".parse().expect("ip")));
    }
}
