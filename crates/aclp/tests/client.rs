//! Client behaviour against real loopback callback servers.

mod common;

use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aclp::events::{ContentBlock, InputSubmittedParams, SessionStartedParams, ToolFinishedParams};
use aclp::{Callbacks, Client, ClientError, ClientOptions, ConfigError, ServerOptions};
use common::{ContextReply, constant, context_schema, input, start, tool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

fn session() -> SessionStartedParams {
    SessionStartedParams {
        session_id: "session-1".to_owned(),
        run_id: None,
        agent_id: None,
        reason: None,
        parent_session_id: None,
    }
}

type Responder = Box<dyn Fn(&str) -> Vec<u8> + Send + Sync>;

type Expected = fn(&ClientError) -> bool;

fn assert_send<T: Send>(val: T) -> T {
    val
}

async fn fake(respond: impl Fn(&str) -> Vec<u8> + Send + Sync + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");

    let url = format!("http://{}/events", listener.local_addr().expect("address"));

    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0; 65_536];

            let n = stream.read(&mut buf).await.unwrap_or(0);

            let req = String::from_utf8_lossy(&buf[..n]).into_owned();

            let body = req.split_once("\r\n\r\n").map_or("", |(_, b)| b).to_owned();

            let _ = stream.write_all(&respond(&body)).await;
        }
    });

    url
}

fn response(status: &str, content_type: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

#[tokio::test]
async fn request_returns_schema_validated_context_from_a_real_callback_server() {
    let callbacks = Callbacks::new().request(
        &context_schema(),
        |params: InputSubmittedParams| async move {
            let class = params
                .content
                .iter()
                .any(|b| matches!(b, ContentBlock::Text { text } if text.contains("class")));

            let context = if class {
                vec!["Prefer plain structs.".to_owned()]
            } else {
                Vec::new()
            };

            Ok::<_, Infallible>(ContextReply { context })
        },
    );

    let server = start(callbacks, ServerOptions::new()).await;

    let client = Client::new(ClientOptions::new(server.url())).expect("client config");

    let reply = assert_send(client.request(&input(), &context_schema())).await;

    assert_eq!(
        reply.expect("valid reply").context,
        ["Prefer plain structs."]
    );
    server.stop().await;
}

#[tokio::test]
async fn flush_waits_for_admission_while_an_observation_callback_remains_unfinished() {
    let release = Arc::new(Semaphore::new(0));

    let finished = Arc::new(AtomicUsize::new(0));

    let (r, f) = (Arc::clone(&release), Arc::clone(&finished));

    let callbacks = Callbacks::new().notification(move |_: ToolFinishedParams| {
        let (r, f) = (Arc::clone(&r), Arc::clone(&f));

        async move {
            let _permit = r.acquire().await;
            f.fetch_add(1, Ordering::SeqCst);
            Ok::<_, Infallible>(())
        }
    });

    let server = start(callbacks, ServerOptions::new()).await;

    let client = Client::new(ClientOptions::new(server.url())).expect("client config");

    client.notify(&tool()).expect("enqueued");
    assert_send(client.flush()).await.expect("admitted");
    assert_eq!(finished.load(Ordering::SeqCst), 0);
    release.add_permits(1);
    server.stop().await;
    assert_eq!(finished.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn notification_rejection_reaches_diagnostics_and_flush() {
    let server = start(
        Callbacks::new(),
        ServerOptions::new().authorization("Bearer installed-token"),
    )
    .await;

    let diagnostics = Arc::new(Mutex::new(Vec::new()));

    let sink = Arc::clone(&diagnostics);

    let opts = ClientOptions::new(server.url())
        .on_error(move |err| sink.lock().expect("lock").push(err.to_string()));

    let client = Client::new(opts).expect("client config");

    client.notify(&session()).expect("enqueued");

    let failures = client.flush().await.expect_err("rejected").into_failures();

    assert!(
        matches!(failures.as_slice(), [ClientError::Status(401)]),
        "{failures:?}"
    );
    assert_eq!(diagnostics.lock().expect("lock").len(), 1);
    assert!(client.flush().await.is_ok());
    server.stop().await;
}

#[test]
fn client_rejects_remote_endpoints_and_invalid_timers() {
    for url in [
        "https://example.com/events",
        "http://example.com/events",
        "http://user:password@localhost/events",
        "http://localhost/events#fragment",
        "https://localhost/events",
        "localhost:8787",
    ] {
        assert_eq!(
            Client::new(ClientOptions::new(url)).err(),
            Some(ConfigError::Endpoint),
            "{url}"
        );
    }

    for timeout in [Duration::ZERO, Duration::from_millis(2_147_483_648)] {
        let opts = ClientOptions::new("http://localhost/events").timeout(timeout);

        assert_eq!(Client::new(opts).err(), Some(ConfigError::Timeout));
    }

    let opts = ClientOptions::new("http://[::1]:8787/events").authorization("Bearer x\r\nHost: y");

    assert_eq!(Client::new(opts).err(), Some(ConfigError::Authorization));
}

#[tokio::test]
async fn client_rejects_a_mismatched_id_a_result_outside_its_schema_and_non_json_results() {
    let echo = |id: Option<&str>, result: &str| {
        let id = id.map_or_else(|| "\"wrong-id\"".to_owned(), |id| format!("\"{id}\""));

        format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{result}}}"#)
    };

    let request_id = |body: &str| -> String {
        let msg: serde_json::Value = serde_json::from_str(body).expect("request JSON");

        msg["id"].as_str().expect("request id").to_owned()
    };

    let cases: Vec<(Responder, Expected)> = vec![
        (
            Box::new(move |_| {
                response(
                    "200 OK",
                    "application/json",
                    &echo(None, r#"{"context":[]}"#),
                )
            }),
            |e| matches!(e, ClientError::InvalidResponse),
        ),
        (
            Box::new(move |b| {
                response(
                    "200 OK",
                    "application/json",
                    &echo(Some(&request_id(b)), r#"{"context":"invalid"}"#),
                )
            }),
            |e| matches!(e, ClientError::InvalidResult),
        ),
        (
            Box::new(move |b| {
                response(
                    "200 OK",
                    "text/plain",
                    &echo(Some(&request_id(b)), r#"{"context":[]}"#),
                )
            }),
            |e| matches!(e, ClientError::ContentType),
        ),
        (
            Box::new(|_| response("500 Internal Server Error", "text/plain", "")),
            |e| matches!(e, ClientError::Status(500)),
        ),
    ];

    for (idx, (respond, expected)) in cases.into_iter().enumerate() {
        let client = Client::new(ClientOptions::new(fake(respond).await)).expect("client config");

        let err = client
            .request(&input(), &context_schema())
            .await
            .expect_err("rejected");

        assert!(expected(&err), "case {idx}: {err:?}");
    }
}

#[tokio::test]
async fn callback_errors_surface_their_code_and_message() {
    let callbacks = Callbacks::new().request(&context_schema(), |_: InputSubmittedParams| async {
        Err::<ContextReply, _>("hidden detail")
    });

    let server = start(callbacks, ServerOptions::new()).await;

    let client = Client::new(ClientOptions::new(server.url())).expect("client config");

    let err = client
        .request(&input(), &context_schema())
        .await
        .expect_err("callback failed");

    assert!(
        matches!(&err, ClientError::Callback { code: -32603, message } if message == "Internal error"),
        "{err:?}"
    );
    server.stop().await;
}

#[tokio::test]
async fn request_deadline_covers_the_whole_exchange() {
    let callbacks = Callbacks::new().request(&context_schema(), |_: InputSubmittedParams| async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok::<_, Infallible>(ContextReply {
            context: Vec::new(),
        })
    });

    let server = start(callbacks, ServerOptions::new()).await;

    let opts = ClientOptions::new(server.url()).timeout(Duration::from_millis(100));

    let client = Client::new(opts).expect("client config");

    let err = client
        .request(&input(), &context_schema())
        .await
        .expect_err("deadline");

    assert!(matches!(err, ClientError::Deadline), "{err:?}");
    server.stop().await;
}

#[tokio::test]
async fn invalid_parameters_fail_locally_before_any_io() {
    let client =
        Client::new(ClientOptions::new("http://127.0.0.1:1/events")).expect("client config");

    let mut params = input();

    params.content.clear();

    let err = client
        .request(&params, &context_schema())
        .await
        .expect_err("invalid");

    assert!(
        matches!(
            err,
            ClientError::InvalidParams {
                method: "input.submitted"
            }
        ),
        "{err:?}"
    );
    assert!(matches!(
        client.notify(&params),
        Err(ClientError::InvalidParams { .. })
    ));
    assert!(client.flush().await.is_ok());
}

#[tokio::test]
async fn client_bounds_notification_admission_attempts_before_dispatch() {
    let opts = ClientOptions::new("http://127.0.0.1:1/events").on_error(|_| {});

    let client = Client::new(opts).expect("client config");

    for _ in 0..64 {
        client.notify(&session()).expect("enqueued");
    }

    assert!(matches!(
        client.notify(&session()),
        Err(ClientError::Capacity)
    ));
    assert!(matches!(
        client.request(&input(), &context_schema()).await,
        Err(ClientError::Capacity)
    ));
    assert_eq!(
        client
            .flush()
            .await
            .expect_err("unreachable")
            .failures()
            .len(),
        64
    );
    client.notify(&session()).expect("capacity released");
    assert!(client.flush().await.is_err());
}

#[tokio::test]
async fn http_upgrades_fail_without_retaining_client_capacity() {
    let url = fake(|_| {
        b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n"
            .to_vec()
    })
    .await;

    let opts = ClientOptions::new(url)
        .timeout(Duration::from_millis(500))
        .on_error(|_| {});

    let client = Client::new(opts).expect("client config");

    let err = client
        .request(&input(), &context_schema())
        .await
        .expect_err("upgrade");

    assert!(matches!(err, ClientError::Upgrade), "{err:?}");

    for _ in 0..2 {
        for _ in 0..64 {
            client.notify(&session()).expect("enqueued");
        }

        let failures = client.flush().await.expect_err("upgrade").into_failures();

        assert!(
            failures.iter().all(|f| matches!(f, ClientError::Upgrade)),
            "{failures:?}"
        );
    }
}

#[tokio::test]
async fn clones_share_one_queue_and_capacity() {
    let server = start(
        constant(ContextReply {
            context: Vec::new(),
        }),
        ServerOptions::new(),
    )
    .await;

    let client = Client::new(ClientOptions::new(server.url())).expect("client config");

    let clone = client.clone();

    clone.notify(&tool()).expect("enqueued");

    let err = client
        .flush()
        .await
        .expect_err("tool.finished has no callback")
        .into_failures();

    assert!(
        matches!(err.as_slice(), [ClientError::Status(400)]),
        "{err:?}"
    );
    server.stop().await;
}
