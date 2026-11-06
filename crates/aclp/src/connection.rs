use std::fmt;

use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use hyper::body::Bytes;
use hyper::header::{CONTENT_TYPE, HOST, HeaderMap, HeaderValue};
use hyper::{Method, Request, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use crate::client_error::ClientError;
use crate::config_error::ConfigError;
use crate::json::MAX_BODY_BYTES;

const MAX_HEADER_BYTES: usize = 16_384;

#[cfg(feature = "tls")]
#[derive(Clone)]
struct Tls {
    connector: tokio_rustls::TlsConnector,
    name: rustls::pki_types::ServerName<'static>,
}

/// One parsed `http` or `https` target: where to connect and what to send as Host and path.
#[derive(Clone)]
pub(crate) struct Endpoint {
    connect: String,
    port: u16,
    authority: HeaderValue,
    path: String,
    #[cfg(feature = "tls")]
    tls: Option<Tls>,
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("authority", &self.authority)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

pub(crate) struct Reply {
    pub(crate) status: StatusCode,
    pub(crate) json: bool,
    pub(crate) body: Bytes,
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
}

#[cfg(feature = "tls")]
fn tls(host: &str, ca: Option<&str>) -> Result<Tls, ConfigError> {
    use std::sync::Arc;

    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, ServerName};

    let mut roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };

    if let Some(pem) = ca {
        let certs = CertificateDer::pem_slice_iter(pem.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ConfigError::Certificate)?;

        if certs.is_empty() {
            return Err(ConfigError::Certificate);
        }

        for cert in certs {
            roots.add(cert).map_err(|_| ConfigError::Certificate)?;
        }
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut cfg = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| ConfigError::Certificate)?
        .with_root_certificates(roots)
        .with_no_client_auth();

    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];

    let name = ServerName::try_from(host.to_owned()).map_err(|_| ConfigError::Endpoint)?;

    Ok(Tls {
        connector: tokio_rustls::TlsConnector::from(Arc::new(cfg)),
        name,
    })
}

impl Endpoint {
    /// Parses an `http` or `https` URL without credentials or a fragment.
    pub(crate) fn parse(url: &str, ca: Option<&str>) -> Result<Self, ConfigError> {
        let uri: Uri = url.parse().map_err(|_| ConfigError::Endpoint)?;

        let authority = uri.authority().ok_or(ConfigError::Endpoint)?;

        let secure = match uri.scheme_str() {
            Some("http") => false,
            Some("https") => true,
            _ => return Err(ConfigError::Endpoint),
        };

        if authority.as_str().contains('@') || url.contains('#') || authority.host().is_empty() {
            return Err(ConfigError::Endpoint);
        }

        let host = authority
            .host()
            .trim_start_matches('[')
            .trim_end_matches(']');

        let connect = if host == "localhost" {
            "127.0.0.1"
        } else {
            host
        };

        #[cfg(feature = "tls")]
        let tls = secure.then(|| tls(host, ca)).transpose()?;

        #[cfg(not(feature = "tls"))]
        if secure || ca.is_some() {
            return Err(ConfigError::Endpoint);
        }

        Ok(Endpoint {
            connect: connect.to_owned(),
            port: authority
                .port_u16()
                .unwrap_or(if secure { 443 } else { 80 }),
            authority: HeaderValue::from_str(authority.as_str())
                .map_err(|_| ConfigError::Endpoint)?,
            path: uri.path_and_query().map_or("/", |p| p.as_str()).to_owned(),
            #[cfg(feature = "tls")]
            tls,
        })
    }

    /// The same origin with another path, used for discovery.
    pub(crate) fn with_path(&self, path: &str) -> Self {
        Endpoint {
            path: path.to_owned(),
            ..self.clone()
        }
    }

    /// Sends one request on a fresh connection and reads at most 1 MiB of response body.
    pub(crate) async fn exchange(
        &self,
        method: Method,
        mut headers: HeaderMap,
        body: Bytes,
    ) -> Result<Reply, ClientError> {
        let stream = TcpStream::connect((self.connect.as_str(), self.port))
            .await
            .map_err(ClientError::transport)?;

        headers.insert(HOST, self.authority.clone());

        let mut req = Request::builder()
            .method(method)
            .uri(&self.path)
            .body(Full::new(body))
            .map_err(ClientError::transport)?;

        *req.headers_mut() = headers;

        #[cfg(feature = "tls")]
        if let Some(tls) = &self.tls {
            let stream = tls
                .connector
                .connect(tls.name.clone(), stream)
                .await
                .map_err(ClientError::transport)?;

            return send(TokioIo::new(stream), req).await;
        }

        send(TokioIo::new(stream), req).await
    }
}

async fn send<I>(io: I, req: Request<Full<Bytes>>) -> Result<Reply, ClientError>
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
        .max_buf_size(MAX_HEADER_BYTES)
        .handshake(io)
        .await
        .map_err(ClientError::transport)?;

    let work = async move {
        let res = sender
            .send_request(req)
            .await
            .map_err(ClientError::transport)?;

        let status = res.status();

        if status == StatusCode::SWITCHING_PROTOCOLS {
            return Err(ClientError::Upgrade);
        }

        let json = is_json(res.headers());

        let body = Limited::new(res.into_body(), MAX_BODY_BYTES)
            .collect()
            .await
            .map_err(|err| {
                if err.is::<LengthLimitError>() {
                    ClientError::ResponseTooLarge
                } else {
                    ClientError::Transport(err)
                }
            })?
            .to_bytes();

        Ok(Reply { status, json, body })
    };

    let (res, _) = tokio::join!(work, conn);

    res
}
