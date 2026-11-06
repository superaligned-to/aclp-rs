//! Connection limits, timeouts, overload and shutdown behaviour over real sockets.

mod common;

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use aclp::events::{InputSubmittedParams, ToolFinishedParams};
use aclp::{Callbacks, Client, ClientError, ClientOptions, ServerOptions};
use common::{ContextReply, context_schema, input, input_message, post, start, tool_message};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, Semaphore};

async fn closed_within(stream: &mut TcpStream, limit: Duration) -> bool {
    let mut buf = [0; 1024];

    let deadline = Instant::now() + limit;

    loop {
        match tokio::time::timeout_at(deadline.into(), stream.read(&mut buf)).await {
            Err(_) => return false,
            Ok(Ok(0) | Err(_)) => return true,
            Ok(Ok(_)) => {}
        }
    }
}

fn counting(calls: &Arc<AtomicUsize>, hold: Duration) -> Callbacks {
    let (request, notification) = (Arc::clone(calls), Arc::clone(calls));

    Callbacks::new()
        .request(&context_schema(), move |_: InputSubmittedParams| {
            let calls = Arc::clone(&request);

            async move {
                tokio::time::sleep(hold).await;
                calls.fetch_add(1, Ordering::SeqCst);
                Ok::<_, Infallible>(ContextReply {
                    context: Vec::new(),
                })
            }
        })
        .notification(move |_: ToolFinishedParams| {
            let calls = Arc::clone(&notification);

            async move {
                tokio::time::sleep(hold).await;
                calls.fetch_add(1, Ordering::SeqCst);
                Ok::<_, Infallible>(())
            }
        })
}

#[tokio::test]
async fn a_partial_request_head_is_closed_after_the_header_timeout() {
    let server = start(Callbacks::new(), ServerOptions::new()).await;

    let mut stream = TcpStream::connect(("127.0.0.1", server.port))
        .await
        .expect("connect");

    let started = Instant::now();

    stream
        .write_all(
            format!(
                "POST /events HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n",
                server.port
            )
            .as_bytes(),
        )
        .await
        .expect("write");

    assert!(
        closed_within(&mut stream, Duration::from_secs(8)).await,
        "connection stays open"
    );
    assert!(
        started.elapsed() >= Duration::from_secs(4),
        "closed after {:?}",
        started.elapsed()
    );
    server.stop().await;
}

#[tokio::test]
async fn connections_beyond_sixty_four_are_closed_while_the_rest_stay_usable() {
    let server = start(
        counting(&Arc::new(AtomicUsize::new(0)), Duration::ZERO),
        ServerOptions::new(),
    )
    .await;

    let mut idle = Vec::new();

    for _ in 0..64 {
        idle.push(
            TcpStream::connect(("127.0.0.1", server.port))
                .await
                .expect("connect"),
        );
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut extra = TcpStream::connect(("127.0.0.1", server.port))
        .await
        .expect("connect");

    assert!(
        closed_within(&mut extra, Duration::from_secs(1)).await,
        "65th connection is closed"
    );

    let mut first = idle.remove(0);

    let body = tool_message().to_string();

    first
        .write_all(&common::request_bytes(
            server.port,
            "POST",
            &[],
            body.as_bytes(),
        ))
        .await
        .expect("write");

    let mut reply = Vec::new();

    first.read_to_end(&mut reply).await.expect("read");

    assert!(
        String::from_utf8_lossy(&reply).starts_with("HTTP/1.1 202"),
        "{}",
        String::from_utf8_lossy(&reply)
    );
    drop(idle);
    server.stop().await;
}

#[tokio::test]
async fn oversized_request_headers_are_rejected_before_callbacks() {
    let calls = Arc::new(AtomicUsize::new(0));

    let server = start(counting(&calls, Duration::ZERO), ServerOptions::new()).await;

    let filler = "x".repeat(20 * 1024);

    let reply = post(
        server.port,
        tool_message().to_string(),
        &[("X-Filler", &filler)],
    )
    .await;

    assert_eq!(reply.status, 431);
    assert_eq!(
        post(server.port, tool_message().to_string(), &[])
            .await
            .status,
        202
    );
    server.stop().await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn overload_answers_429_and_releases_every_slot() {
    let calls = Arc::new(AtomicUsize::new(0));

    let server = start(
        counting(&calls, Duration::from_millis(50)),
        ServerOptions::new(),
    )
    .await;

    let port = server.port;

    let gate = Arc::new(Semaphore::new(48));

    let mut tasks = Vec::new();

    for idx in 0..200 {
        let gate = Arc::clone(&gate);

        tasks.push(tokio::spawn(async move {
            let _permit = gate.acquire_owned().await.expect("gate open");

            let body = if idx % 2 == 0 {
                input_message(&format!("input-{idx}")).to_string()
            } else {
                tool_message().to_string()
            };

            post(port, body, &[]).await.status
        }));
    }

    let mut statuses = Vec::new();

    for task in tasks {
        statuses.push(task.await.expect("caller task"));
    }

    assert!(
        statuses.iter().all(|s| [200, 202, 429].contains(s)),
        "{statuses:?}"
    );
    assert!(
        statuses.contains(&429),
        "48 callers over 32 slots must overload"
    );

    let admitted = statuses.iter().filter(|s| **s != 429).count();

    assert_eq!(
        post(port, input_message("after").to_string(), &[])
            .await
            .status,
        200
    );
    server.stop().await;
    assert_eq!(calls.load(Ordering::SeqCst), admitted + 1);
}

async fn holding_server(release: Arc<Notify>, waiting: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");

    let url = format!("http://{}/events", listener.local_addr().expect("address"));

    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let (release, waiting) = (Arc::clone(&release), Arc::clone(&waiting));

            tokio::spawn(async move {
                let mut buf = Vec::new();

                let mut chunk = [0; 4096];

                while !String::from_utf8_lossy(&buf).contains("\"id\"") || !buf.ends_with(b"}") {
                    match stream.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }

                let text = String::from_utf8_lossy(&buf).into_owned();

                let body = text.split_once("\r\n\r\n").map_or("", |(_, b)| b);

                let id =
                    serde_json::from_str::<serde_json::Value>(body).expect("request JSON")["id"]
                        .clone();

                let notified = release.notified();

                waiting.fetch_add(1, Ordering::SeqCst);
                notified.await;

                let reply =
                    serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": { "context": [] } })
                        .to_string();

                let res = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );

                let _ = stream.write_all(res.as_bytes()).await;
            });
        }
    });

    url
}

#[tokio::test]
async fn the_client_runs_sixty_four_requests_and_refuses_the_next() {
    let release = Arc::new(Notify::new());

    let waiting = Arc::new(AtomicUsize::new(0));

    let url = holding_server(Arc::clone(&release), Arc::clone(&waiting)).await;

    let client = Client::new(ClientOptions::new(url).timeout(Duration::from_secs(10)))
        .expect("client config");

    let mut tasks = Vec::new();

    for _ in 0..64 {
        let client = client.clone();

        tasks.push(tokio::spawn(async move {
            client.request(&input(), &context_schema()).await
        }));
    }

    while waiting.load(Ordering::SeqCst) < 64 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let refused = client.request(&input(), &context_schema()).await;

    assert!(matches!(refused, Err(ClientError::Capacity)), "{refused:?}");
    release.notify_waiters();

    for task in tasks {
        assert!(task.await.expect("request task").is_ok());
    }

    assert!(matches!(client.notify(&common::tool()), Ok(())));
}

#[tokio::test]
async fn shutdown_waits_for_an_in_flight_request_callback() {
    let entered = Arc::new(Notify::new());

    let done = Arc::new(AtomicBool::new(false));

    let (e, d) = (Arc::clone(&entered), Arc::clone(&done));

    let callbacks = Callbacks::new().request(&context_schema(), move |_: InputSubmittedParams| {
        let (e, d) = (Arc::clone(&e), Arc::clone(&d));

        async move {
            e.notify_one();
            tokio::time::sleep(Duration::from_millis(300)).await;
            d.store(true, Ordering::SeqCst);
            Ok::<_, Infallible>(ContextReply {
                context: Vec::new(),
            })
        }
    });

    let server = start(callbacks, ServerOptions::new()).await;

    let port = server.port;

    let caller = tokio::spawn(async move {
        post(port, input_message("slow").to_string(), &[])
            .await
            .status
    });

    entered.notified().await;
    server.stop().await;
    assert!(
        done.load(Ordering::SeqCst),
        "serve returned before the callback finished"
    );
    assert_eq!(caller.await.expect("caller task"), 200);
}
