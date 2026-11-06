//! Server behaviour over real loopback HTTP connections.

mod common;

use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aclp::events::{InputSubmittedParams, ToolFinishedParams};
use aclp::{CallbackError, Callbacks, ServerOptions};
use common::{ContextReply, context_schema, input_message, post, start, tool_message};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::{Notify, Semaphore};

const MAX_BODY_BYTES: usize = 1_048_576;

fn counting(calls: &Arc<AtomicUsize>) -> Callbacks {
    let request = Arc::clone(calls);

    let notification = Arc::clone(calls);

    Callbacks::new()
        .request(&context_schema(), move |_: InputSubmittedParams| {
            request.fetch_add(1, Ordering::SeqCst);

            async {
                Ok::<_, Infallible>(ContextReply {
                    context: Vec::new(),
                })
            }
        })
        .notification(move |_: ToolFinishedParams| {
            notification.fetch_add(1, Ordering::SeqCst);

            async { Ok::<_, Infallible>(()) }
        })
}

#[tokio::test]
async fn requests_return_a_validated_result_with_the_matching_id() {
    let callbacks = Callbacks::new().request(
        &context_schema(),
        |params: InputSubmittedParams| async move {
            Ok::<_, Infallible>(ContextReply {
                context: vec![params.content.len().to_string()],
            })
        },
    );

    let server = start(callbacks, ServerOptions::new()).await;

    let reply = post(server.port, input_message("input-1").to_string(), &[]).await;

    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-type"), Some("application/json"));
    assert_eq!(
        reply.json(),
        json!({ "jsonrpc": "2.0", "id": "input-1", "result": { "context": ["1"] } })
    );
    server.stop().await;
}

#[tokio::test]
async fn notifications_acknowledge_admission_without_awaiting_the_callback() {
    let entered = Arc::new(Notify::new());

    let release = Arc::new(Semaphore::new(0));

    let finished = Arc::new(AtomicUsize::new(0));

    let (e, r, f) = (
        Arc::clone(&entered),
        Arc::clone(&release),
        Arc::clone(&finished),
    );

    let callbacks = Callbacks::new().notification(move |_: ToolFinishedParams| {
        let (e, r, f) = (Arc::clone(&e), Arc::clone(&r), Arc::clone(&f));

        async move {
            e.notify_one();
            let _permit = r.acquire().await;
            f.fetch_add(1, Ordering::SeqCst);
            Ok::<_, Infallible>(())
        }
    });

    let server = start(callbacks, ServerOptions::new()).await;

    let reply = post(server.port, tool_message().to_string(), &[]).await;

    assert_eq!((reply.status, reply.body.as_str()), (202, ""));
    entered.notified().await;
    assert_eq!(finished.load(Ordering::SeqCst), 0);
    release.add_permits(1);
    server.stop().await;
    assert_eq!(
        finished.load(Ordering::SeqCst),
        1,
        "shutdown waits for admitted callbacks"
    );
}

#[tokio::test]
async fn callback_failures_invalid_results_and_oversized_results_become_internal_errors() {
    let cases: [fn() -> Result<Value, String>; 4] = [
        || Err("private failure detail".to_owned()),
        || Ok(json!({ "context": "invalid" })),
        || Ok(json!({ "context": ["x".repeat(MAX_BODY_BYTES)] })),
        || panic!("private panic detail"),
    ];

    let schema =
        aclp::ResultSchema::<Value>::new(context_schema().schema().clone()).expect("compiles");

    for (idx, case) in cases.into_iter().enumerate() {
        let errors = Arc::new(Mutex::new(Vec::new()));

        let sink = Arc::clone(&errors);

        let opts = ServerOptions::new().on_error(move |err: &CallbackError| {
            sink.lock().expect("lock").push(err.to_string());
        });

        let callbacks = Callbacks::new().request(
            &schema,
            move |_: InputSubmittedParams| async move { case() },
        );

        let server = start(callbacks, opts).await;

        let reply = post(server.port, input_message("input-1").to_string(), &[]).await;

        assert_eq!(reply.status, 200, "case {idx}");
        assert_eq!(
            reply.json(),
            json!({ "jsonrpc": "2.0", "id": "input-1", "error": { "code": -32603, "message": "Internal error" } }),
            "case {idx}"
        );
        assert!(!reply.body.contains("private"), "case {idx}");
        assert_eq!(errors.lock().expect("lock").len(), 1, "case {idx}");
        server.stop().await;
    }
}

#[tokio::test]
async fn invalid_messages_never_invoke_callbacks_and_notification_errors_have_empty_bodies() {
    let calls = Arc::new(AtomicUsize::new(0));

    let server = start(counting(&calls), ServerOptions::new()).await;

    let with = |mut msg: Value, key: &str, val: Value| {
        msg[key] = val;
        msg.to_string().into_bytes()
    };

    let cases: Vec<(Vec<u8>, u16, Option<i64>)> = vec![
        (b"{".to_vec(), 400, Some(-32700)),
        (vec![255], 400, Some(-32700)),
        (
            br#"{"jsonrpc":"2.0","id":"x","id":"y","method":"input.submitted","params":{}}"#
                .to_vec(),
            400,
            Some(-32700),
        ),
        (
            with(input_message("i"), "extra", json!(true)),
            400,
            Some(-32600),
        ),
        (
            with(input_message("i"), "id", Value::from("é".repeat(65))),
            400,
            Some(-32600),
        ),
        (
            with(input_message("i"), "method", json!("unknown.event")),
            200,
            Some(-32601),
        ),
        (
            with(input_message("i"), "params", json!({})),
            200,
            Some(-32602),
        ),
        (
            with(tool_message(), "method", json!("unknown.event")),
            400,
            None,
        ),
        (with(tool_message(), "params", json!({})), 400, None),
        (with(tool_message(), "extra", json!(true)), 400, None),
    ];

    for (body, status, code) in cases {
        let reply = post(server.port, &body, &[]).await;

        let label = String::from_utf8_lossy(&body).into_owned();

        assert_eq!(reply.status, status, "{label}");

        match code {
            Some(code) => assert_eq!(reply.json()["error"]["code"], code, "{label}"),
            None => assert_eq!(reply.body, "", "{label}"),
        }
    }

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    server.stop().await;
}

#[tokio::test]
async fn notification_only_callbacks_reject_requests_before_executing() {
    let calls = Arc::new(AtomicUsize::new(0));

    let counter = Arc::clone(&calls);

    let callbacks = Callbacks::new().notification(move |_: ToolFinishedParams| {
        counter.fetch_add(1, Ordering::SeqCst);

        async { Ok::<_, Infallible>(()) }
    });

    let server = start(callbacks, ServerOptions::new()).await;

    let mut msg = tool_message();

    msg["id"] = json!("request-1");

    let reply = post(server.port, msg.to_string(), &[]).await;

    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.json(),
        json!({ "jsonrpc": "2.0", "id": "request-1", "error": { "code": -32601, "message": "Method not found" } })
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    server.stop().await;
}

#[tokio::test]
async fn authorization_and_loopback_host_checks_precede_dispatch() {
    let calls = Arc::new(AtomicUsize::new(0));

    let opts = ServerOptions::new().authorization("Bearer local-test-credential");

    let server = start(counting(&calls), opts).await;

    let body = tool_message().to_string();

    let token = ("Authorization", "Bearer local-test-credential");

    let denied = post(server.port, &body, &[]).await;

    assert_eq!((denied.status, denied.body.as_str()), (401, ""));
    assert_eq!(
        denied.header("www-authenticate"),
        Some(r#"Bearer realm="aclp""#)
    );
    assert_eq!(
        post(server.port, &body, &[("Authorization", "Bearer wrong")])
            .await
            .status,
        401
    );
    assert_eq!(post(server.port, &body, &[token, token]).await.status, 401);
    assert_eq!(
        post(server.port, &body, &[token, ("Host", "example.com")])
            .await
            .status,
        403
    );
    assert_eq!(
        post(
            server.port,
            &body,
            &[token, ("Origin", "https://example.com")]
        )
        .await
        .status,
        403
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(post(server.port, &body, &[token]).await.status, 202);
    server.stop().await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    for invalid in ["missing-scheme", "Bearer credential\r\nHost: attacker"] {
        let opts = ServerOptions::new().authorization(invalid);

        assert_eq!(
            aclp::Server::new(Callbacks::new(), opts).err(),
            Some(aclp::ConfigError::Authorization)
        );
    }
}

#[tokio::test]
async fn http_method_target_content_type_and_body_size_are_bounded_before_callbacks() {
    let server = start(Callbacks::new(), ServerOptions::new()).await;

    let port = server.port;

    let get = common::exchange(port, &common::request_bytes(port, "GET", &[], b"")).await;

    assert_eq!((get.status, get.header("allow")), (405, Some("POST")));
    assert_eq!(
        post(port, "{}", &[("Content-Type", "text/plain")])
            .await
            .status,
        415
    );
    assert_eq!(
        post(port, "x".repeat(MAX_BODY_BYTES + 1), &[]).await.status,
        413
    );

    let absolute = format!(
        "POST http://127.0.0.1:{port}/events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
    );

    assert_eq!(
        common::exchange(port, absolute.as_bytes()).await.status,
        403
    );

    let chunk = "x".repeat(MAX_BODY_BYTES);

    let chunked = format!(
        "POST /events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{chunk}\r\n1\r\nx\r\n0\r\n\r\n",
        chunk.len()
    );

    assert_eq!(common::exchange(port, chunked.as_bytes()).await.status, 413);
    server.stop().await;
}

#[tokio::test]
async fn notification_callback_failures_remain_diagnostics() {
    let (tx, rx) = tokio::sync::oneshot::channel();

    let tx = Mutex::new(Some(tx));

    let opts = ServerOptions::new().on_error(move |err: &CallbackError| {
        if let Some(tx) = tx.lock().expect("lock").take() {
            let _ = tx.send(std::error::Error::source(err).map(ToString::to_string));
        }
    });

    let callbacks = Callbacks::new()
        .notification(|_: ToolFinishedParams| async { Err::<(), _>("observation failed") });

    let server = start(callbacks, opts).await;

    let reply = post(server.port, tool_message().to_string(), &[]).await;

    assert_eq!((reply.status, reply.body.as_str()), (202, ""));
    assert_eq!(
        rx.await.expect("reported"),
        Some("observation failed".to_owned())
    );
    server.stop().await;
}

#[tokio::test]
async fn callback_capacity_remains_reserved_after_callers_disconnect() {
    let entered = Arc::new(AtomicUsize::new(0));

    let admitted = Arc::new(Notify::new());

    let release = Arc::new(Semaphore::new(0));

    let (e, a, r) = (
        Arc::clone(&entered),
        Arc::clone(&admitted),
        Arc::clone(&release),
    );

    let callbacks = Callbacks::new().request(&context_schema(), move |_: InputSubmittedParams| {
        let (e, a, r) = (Arc::clone(&e), Arc::clone(&a), Arc::clone(&r));

        async move {
            if e.fetch_add(1, Ordering::SeqCst) + 1 == 32 {
                a.notify_one();
            }

            let _permit = r.acquire().await;
            Ok::<_, Infallible>(ContextReply {
                context: Vec::new(),
            })
        }
    });

    let server = start(callbacks, ServerOptions::new()).await;

    let mut callers = Vec::new();

    for idx in 0..32 {
        let mut stream = TcpStream::connect(("127.0.0.1", server.port))
            .await
            .expect("connect");

        let body = input_message(&format!("input-{idx}")).to_string();

        stream
            .write_all(&common::request_bytes(
                server.port,
                "POST",
                &[],
                body.as_bytes(),
            ))
            .await
            .expect("write");
        callers.push(stream);
    }

    admitted.notified().await;
    drop(callers);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        post(server.port, input_message("late").to_string(), &[])
            .await
            .status,
        429
    );
    release.add_permits(32);

    let mut status = 0;

    for _ in 0..100 {
        status = post(server.port, input_message("after").to_string(), &[])
            .await
            .status;

        if status == 200 {
            break;
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(status, 200);
    assert_eq!(entered.load(Ordering::SeqCst), 33);
    server.stop().await;
}

#[tokio::test]
async fn incomplete_bodies_time_out() {
    let server = start(Callbacks::new(), ServerOptions::new()).await;

    let head = format!(
        "POST /events HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{{",
        server.port
    );

    let reply = tokio::time::timeout(
        Duration::from_secs(8),
        common::exchange(server.port, head.as_bytes()),
    )
    .await
    .expect("server answers before the test deadline");

    assert_eq!(reply.status, 408);
    server.stop().await;
}
