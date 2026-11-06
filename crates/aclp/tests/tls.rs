//! HTTPS delivery to a server trusted through a custom CA.

#![cfg(feature = "tls")]

mod common;

use std::sync::Arc;

use aclp::{Client, ClientError, ClientOptions};
use common::{ContextReply, context_schema, input, tool};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

struct Tls {
    url: String,
    ca: String,
    methods: UnboundedReceiver<String>,
}

fn reply(body: &str) -> Vec<u8> {
    let msg: Value = serde_json::from_str(body).expect("request JSON");

    let Some(id) = msg.get("id") else {
        return b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec();
    };

    let body = json!({ "jsonrpc": "2.0", "id": id, "result": { "context": ["tls"] } }).to_string();

    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

async fn serve() -> Tls {
    let issued =
        rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("certificate");

    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(issued.signing_key.serialize_der()));

    let cfg = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("protocol versions")
    .with_no_client_auth()
    .with_single_cert(vec![CertificateDer::from(issued.cert.der().to_vec())], key)
    .expect("server config");

    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");

    let port = listener.local_addr().expect("address").port();

    let (tx, methods) = unbounded_channel();

    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let Ok(mut tls) = acceptor.accept(stream).await else {
                continue;
            };

            let mut buf = Vec::new();

            let mut chunk = [0; 4096];

            let body = loop {
                let n = tls.read(&mut chunk).await.unwrap_or(0);

                buf.extend_from_slice(&chunk[..n]);

                let text = String::from_utf8_lossy(&buf).into_owned();

                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let len = head
                        .lines()
                        .find_map(|l| {
                            l.strip_prefix("content-length: ")
                                .or_else(|| l.strip_prefix("Content-Length: "))
                        })
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);

                    if body.len() >= len || n == 0 {
                        break body.to_owned();
                    }
                }

                if n == 0 {
                    break String::new();
                }
            };

            let method = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|m| m["method"].as_str().map(str::to_owned));

            let _ = tx.send(method.unwrap_or_default());
            let _ = tls.write_all(&reply(&body)).await;
            let _ = tls.shutdown().await;
        }
    });

    Tls {
        url: format!("https://localhost:{port}/events"),
        ca: issued.cert.pem(),
        methods,
    }
}

#[tokio::test]
async fn requests_and_notifications_reach_an_https_server_trusted_through_a_custom_ca() {
    let mut server = serve().await;

    let client =
        Client::new(ClientOptions::new(&server.url).ca(&server.ca)).expect("client config");

    let reply: ContextReply = client
        .request(&input(), &context_schema())
        .await
        .expect("TLS reply");

    assert_eq!(reply.context, ["tls"]);
    client.notify(&tool()).expect("enqueued");
    client.flush().await.expect("admitted");
    assert_eq!(
        server.methods.recv().await.as_deref(),
        Some("input.submitted")
    );
    assert_eq!(
        server.methods.recv().await.as_deref(),
        Some("tool.finished")
    );

    let untrusted = Client::new(ClientOptions::new(&server.url)).expect("client config");

    let err = untrusted
        .request(&input(), &context_schema())
        .await
        .expect_err("unknown issuer");

    assert!(matches!(err, ClientError::Transport(_)), "{err:?}");
}

#[test]
fn a_ca_bundle_without_certificates_is_rejected() {
    let opts = ClientOptions::new("https://localhost/events").ca("not a certificate");

    assert_eq!(
        Client::new(opts).err(),
        Some(aclp::ConfigError::Certificate)
    );
}
