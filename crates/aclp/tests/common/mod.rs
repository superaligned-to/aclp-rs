#![allow(
    dead_code,
    reason = "each test binary uses a different subset of these helpers"
)]

use std::convert::Infallible;
use std::time::Duration;

use aclp::events::{
    ContentBlock, InputSubmittedParams, ObservedToolCall, ToolCompletion, ToolFinishedParams,
};
use aclp::{Callbacks, ResultSchema, Server, ServerOptions};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ContextReply {
    pub context: Vec<String>,
}

pub(crate) fn context_schema() -> ResultSchema<ContextReply> {
    ResultSchema::new(json!({
        "type": "object",
        "properties": { "context": { "type": "array", "items": { "type": "string" } } },
        "required": ["context"],
        "additionalProperties": false,
    }))
    .expect("context schema compiles")
}

pub(crate) fn input() -> InputSubmittedParams {
    InputSubmittedParams {
        session_id: "session-1".to_owned(),
        run_id: None,
        agent_id: None,
        content: vec![ContentBlock::Text {
            text: "Create a Rust class.".to_owned(),
        }],
        structured_content: None,
    }
}

pub(crate) fn tool() -> ToolFinishedParams {
    ToolFinishedParams {
        session_id: "session-1".to_owned(),
        run_id: None,
        agent_id: None,
        call: ObservedToolCall {
            id: None,
            name: "shell".to_owned(),
            arguments: None,
        },
        outcome: ToolCompletion::Succeeded { output: None },
    }
}

pub(crate) fn input_message(id: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "input.submitted",
        "params": { "sessionId": "session-1", "content": [{ "type": "text", "text": "class" }] },
        "id": id,
    })
}

pub(crate) fn tool_message() -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "tool.finished",
        "params": { "sessionId": "session-1", "call": { "name": "shell" }, "outcome": { "status": "succeeded" } },
    })
}

pub(crate) fn constant(reply: ContextReply) -> Callbacks {
    Callbacks::new().request(&context_schema(), move |_: InputSubmittedParams| {
        let reply = reply.clone();

        async move { Ok::<_, Infallible>(reply) }
    })
}

pub(crate) struct Running {
    pub port: u16,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl Running {
    pub(crate) fn url(&self) -> String {
        format!("http://127.0.0.1:{}/events", self.port)
    }

    pub(crate) async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }

        tokio::time::timeout(Duration::from_secs(10), &mut self.task)
            .await
            .expect("server drains")
            .expect("server task completes");
    }
}

pub(crate) async fn start(callbacks: Callbacks, opts: ServerOptions) -> Running {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");

    let port = listener.local_addr().expect("address").port();

    let server = Server::new(callbacks, opts).expect("server config");

    let (stop, stopped) = oneshot::channel::<()>();

    let task = tokio::spawn(server.serve(listener, async move {
        let _ = stopped.await;
    }));

    Running {
        port,
        stop: Some(stop),
        task,
    }
}

#[derive(Debug)]
pub(crate) struct Raw {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Raw {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub(crate) fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("JSON body")
    }
}

pub(crate) async fn exchange(port: u16, request: &[u8]) -> Raw {
    let stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");

    let (mut rx, mut tx) = stream.into_split();

    let request = request.to_vec();

    let writer = tokio::spawn(async move {
        let _ = tx.write_all(&request).await;

        tx
    });

    let mut buf = Vec::new();

    let _ = rx.read_to_end(&mut buf).await;

    drop(writer.await);

    let text = String::from_utf8_lossy(&buf);

    let (head, body) = text.split_once("\r\n\r\n").expect("complete response");

    let mut lines = head.lines();

    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .expect("status line");

    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();

    Raw {
        status,
        headers,
        body: body.to_owned(),
    }
}

pub(crate) fn request_bytes(
    port: u16,
    method: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Vec<u8> {
    let mut head = format!(
        "{method} /events HTTP/1.1\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );

    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        head.push_str(&format!("Host: 127.0.0.1:{port}\r\n"));
    }

    if !headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
    {
        head.push_str("Content-Type: application/json\r\n");
    }

    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }

    head.push_str("\r\n");

    let mut out = head.into_bytes();

    out.extend_from_slice(body);
    out
}

pub(crate) async fn post(port: u16, body: impl AsRef<[u8]>, headers: &[(&str, &str)]) -> Raw {
    exchange(port, &request_bytes(port, "POST", headers, body.as_ref())).await
}
