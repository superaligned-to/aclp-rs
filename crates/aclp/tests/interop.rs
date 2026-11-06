//! Interoperability with the TypeScript SDK; run with `cargo xtask interop <aclp-ts checkout>`.

mod common;

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use aclp::events::{
    AgentFinishedParams, AgentRequestedParams, AgentStartedParams, CatalogChangedParams,
    ConfigurationChangedParams, ContextCompactionFinishedParams, ContextCompactionRequestedParams,
    ContextSourceLoadedParams, ErrorOccurredParams, HandoffFinishedParams, HandoffRequestedParams,
    InputSubmittedParams, InteractionRequestedParams, InteractionResolvedParams,
    MessageEmittedParams, MessageEmittingParams, ModelChangedParams, ModelFinishedParams,
    ModelRequestedParams, NotificationCreatedParams, PermissionRequestedParams,
    PermissionResolvedParams, PlanUpdatedParams, ResourceChangedParams,
    RunCancellationRequestedParams, RunFinishedParams, RunFinishingParams, RunPausedParams,
    RunResumedParams, RunStartedParams, SessionEndedParams, SessionStartedParams,
    TaskCreatedParams, TaskFinishedParams, TaskUpdatedParams, ToolBatchFinishedParams,
    ToolFinishedParams, ToolRequestedParams, ToolStartedParams, WorkspaceChangedParams,
};
use aclp::{Callbacks, Client, ClientError, ClientOptions, Event, ResultSchema, ServerOptions};
use common::{ContextReply, Raw, context_schema, exchange, request_bytes, start};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

const AUTH: &str = "Bearer interop";

const MAX_BODY_BYTES: usize = 1_048_576;

macro_rules! events {
    ($apply:ident) => {
        $apply!(
            SessionStartedParams,
            SessionEndedParams,
            InputSubmittedParams,
            RunStartedParams,
            RunFinishingParams,
            RunFinishedParams,
            ToolRequestedParams,
            ToolStartedParams,
            ToolFinishedParams,
            ContextCompactionRequestedParams,
            ContextCompactionFinishedParams,
            ContextSourceLoadedParams,
            CatalogChangedParams,
            ModelRequestedParams,
            ModelFinishedParams,
            PermissionRequestedParams,
            PermissionResolvedParams,
            InteractionRequestedParams,
            InteractionResolvedParams,
            AgentRequestedParams,
            AgentStartedParams,
            AgentFinishedParams,
            HandoffRequestedParams,
            HandoffFinishedParams,
            MessageEmittingParams,
            MessageEmittedParams,
            RunCancellationRequestedParams,
            RunPausedParams,
            RunResumedParams,
            ToolBatchFinishedParams,
            PlanUpdatedParams,
            TaskCreatedParams,
            TaskUpdatedParams,
            TaskFinishedParams,
            WorkspaceChangedParams,
            ResourceChangedParams,
            ConfigurationChangedParams,
            ModelChangedParams,
            NotificationCreatedParams,
            ErrorOccurredParams
        )
    };
}

fn examples() -> Vec<(String, Value)> {
    serde_json::from_str(include_str!("fixtures/event-examples.json")).expect("fixture parses")
}

fn example<P: Event>() -> P {
    let (_, params) = examples()
        .into_iter()
        .find(|(m, _)| m == P::METHOD)
        .expect("fixture covers the method");

    serde_json::from_value(params).expect("typed fixture")
}

fn methods() -> BTreeSet<String> {
    examples().into_iter().map(|(m, _)| m).collect()
}

fn strict() -> ResultSchema<Value> {
    ResultSchema::new(json!({
        "type": "object",
        "properties": { "ok": { "type": "boolean" } },
        "required": ["ok"],
    }))
    .expect("compiles")
}

fn notify(client: &Client, method: &str, params: Value) {
    macro_rules! send {
        ($($t:ident),*) => {
            $(
                if method == <$t as Event>::METHOD {
                    let typed: $t = serde_json::from_value(params).expect("typed fixture");

                    client.notify(&typed).expect("enqueued");
                    return;
                }
            )*
        };
    }

    events!(send);
    panic!("unknown method {method}");
}

fn observing(tx: &UnboundedSender<&'static str>) -> Callbacks {
    let mut callbacks = Callbacks::new();

    macro_rules! observe {
        ($($t:ident),*) => {
            $(
                let sink = tx.clone();

                callbacks = callbacks.notification(move |_: $t| {
                    let _ = sink.send(<$t as Event>::METHOD);

                    async { Ok::<_, Infallible>(()) }
                });
            )*
        };
    }

    events!(observe);

    let sink = tx.clone();

    let callbacks = callbacks.request(&context_schema(), move |input: InputSubmittedParams| {
        let _ = sink.send(InputSubmittedParams::METHOD);

        async move {
            Ok::<_, Infallible>(ContextReply {
                context: vec![format!("rs:{}", input.content.len())],
            })
        }
    });

    let sink = tx.clone();

    callbacks.request(&strict(), move |_: SessionStartedParams| {
        let _ = sink.send(SessionStartedParams::METHOD);

        async { Ok::<_, Infallible>(json!({})) }
    })
}

struct Peer {
    _child: Child,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Peer {
    async fn spawn(args: &[&str]) -> Peer {
        let sdk = std::env::var("ACLP_TS")
            .expect("ACLP_TS names an aclp-ts checkout; use cargo xtask interop");

        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/interop/peer.mjs");

        let mut child = Command::new("node")
            .arg(script)
            .arg(args[0])
            .arg(PathBuf::from(sdk))
            .args(&args[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("node starts");

        let stdout = child.stdout.take().expect("piped stdout");

        Peer {
            _child: child,
            lines: BufReader::new(stdout).lines(),
        }
    }

    async fn line(&mut self) -> Value {
        let line = tokio::time::timeout(Duration::from_secs(20), self.lines.next_line())
            .await
            .expect("peer answers in time")
            .expect("peer stdout readable")
            .expect("peer still running");

        serde_json::from_str(&line).expect("peer writes JSON lines")
    }

    async fn port(&mut self) -> u16 {
        let port = self.line().await["port"]
            .as_u64()
            .expect("peer reports its port");

        u16::try_from(port).expect("TCP port")
    }
}

#[tokio::test]
#[ignore = "needs Node and an aclp-ts build; run cargo xtask interop"]
async fn typescript_server_accepts_every_rust_event() {
    let mut ts = Peer::spawn(&["server"]).await;

    let url = format!("http://127.0.0.1:{}/events", ts.port().await);

    let client = Client::new(ClientOptions::new(&url).authorization(AUTH)).expect("client config");

    for (method, params) in examples() {
        notify(&client, &method, params);
    }

    client.flush().await.expect("every notification admitted");

    let mut seen = BTreeSet::new();

    while seen.len() < methods().len() {
        let received = ts.line().await;

        seen.insert(
            received["received"]
                .as_str()
                .expect("received method")
                .to_owned(),
        );
    }

    assert_eq!(seen, methods());

    let reply = client
        .request(&example::<InputSubmittedParams>(), &context_schema())
        .await
        .expect("context reply");

    assert_eq!(reply.context, ["ts:1"]);

    let invalid = client
        .request(&example::<SessionStartedParams>(), &strict())
        .await;

    assert!(
        matches!(invalid, Err(ClientError::Callback { code: -32603, .. })),
        "{invalid:?}"
    );

    let missing = client
        .request(&example::<ToolFinishedParams>(), &context_schema())
        .await;

    assert!(
        matches!(missing, Err(ClientError::Callback { code: -32601, .. })),
        "{missing:?}"
    );

    let intruder = Client::new(
        ClientOptions::new(&url)
            .authorization("Bearer wrong")
            .on_error(|_| {}),
    )
    .expect("client config");

    intruder
        .notify(&example::<SessionEndedParams>())
        .expect("enqueued");

    let failures = intruder
        .flush()
        .await
        .expect_err("unauthorized")
        .into_failures();

    assert!(
        matches!(failures.as_slice(), [ClientError::Status(401)]),
        "{failures:?}"
    );
}

#[tokio::test]
#[ignore = "needs Node and an aclp-ts build; run cargo xtask interop"]
async fn rust_server_accepts_every_typescript_event() {
    let (tx, mut rx) = unbounded_channel();

    let server = start(observing(&tx), ServerOptions::new().authorization(AUTH)).await;

    let mut ts = Peer::spawn(&["client", &server.url()]).await;

    let summary = ts.line().await;

    assert_eq!(
        summary,
        json!({
            "flushed": null,
            "reply": { "context": ["rs:1"] },
            "invalidResult": "ACLP callback failed (-32603): Internal error",
            "notificationOnly": "ACLP callback failed (-32601): Method not found",
            "unauthorized": ["ACLP notification was not admitted (HTTP 401)."],
        })
    );

    let mut seen = BTreeSet::new();

    while seen.len() < methods().len() {
        let method = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("callbacks run")
            .expect("server keeps the channel");

        seen.insert(method.to_owned());
    }

    assert_eq!(seen, methods());
    server.stop().await;
}

type Case<'a> = (&'a str, &'a str, Vec<(&'a str, &'a str)>, String);

#[derive(Debug, PartialEq, Eq)]
enum Body {
    Empty,
    Error(i64),
    Result,
}

fn outcome(raw: &Raw) -> (u16, Body, Option<String>) {
    let body = if raw.body.is_empty() {
        Body::Empty
    } else {
        let msg = raw.json();

        match msg["error"]["code"].as_i64() {
            Some(code) => Body::Error(code),
            None if msg.get("result").is_some() => Body::Result,
            None => panic!("unexpected body {msg}"),
        }
    };

    (
        raw.status,
        body,
        raw.header("www-authenticate").map(str::to_owned),
    )
}

#[tokio::test]
#[ignore = "needs Node and an aclp-ts build; run cargo xtask interop"]
async fn servers_agree_on_status_codes_and_json_rpc_errors() {
    let mut ts = Peer::spawn(&["server"]).await;

    let ts_port = ts.port().await;

    let (tx, _rx) = unbounded_channel();

    let rs = start(observing(&tx), ServerOptions::new().authorization(AUTH)).await;

    let with = |mut msg: Value, key: &str, val: Value| {
        msg[key] = val;
        msg.to_string()
    };

    let input = json!({ "jsonrpc": "2.0", "method": "input.submitted", "params": examples()[2].1, "id": "r" });

    let tool = json!({ "jsonrpc": "2.0", "method": "tool.finished", "params": examples()[8].1 });

    let auth = ("Authorization", AUTH);

    let oversized = "x".repeat(MAX_BODY_BYTES + 1);

    let cases: Vec<Case<'_>> = vec![
        ("valid request", "POST", vec![auth], input.to_string()),
        ("valid notification", "POST", vec![auth], tool.to_string()),
        ("missing credentials", "POST", vec![], tool.to_string()),
        ("wrong credentials", "POST", vec![("Authorization", "Bearer wrong")], tool.to_string()),
        ("foreign host", "POST", vec![auth, ("Host", "example.com")], tool.to_string()),
        ("browser origin", "POST", vec![auth, ("Origin", "https://example.com")], tool.to_string()),
        ("wrong method", "GET", vec![auth], String::new()),
        ("wrong media type", "POST", vec![auth, ("Content-Type", "text/plain")], tool.to_string()),
        ("oversized body", "POST", vec![auth], oversized),
        ("parse error", "POST", vec![auth], "{".to_owned()),
        ("invalid envelope", "POST", vec![auth], with(input.clone(), "extra", json!(true))),
        ("unknown request method", "POST", vec![auth], with(input.clone(), "method", json!("unknown.event"))),
        ("unknown notification method", "POST", vec![auth], with(tool.clone(), "method", json!("unknown.event"))),
        ("invalid request params", "POST", vec![auth], with(input.clone(), "params", json!({}))),
        ("invalid notification params", "POST", vec![auth], with(tool.clone(), "params", json!({}))),
        ("notification-only request", "POST", vec![auth], with(tool.clone(), "id", json!("r"))),
        (
            "result outside schema",
            "POST",
            vec![auth],
            json!({ "jsonrpc": "2.0", "method": "session.started", "params": examples()[0].1, "id": "r" }).to_string(),
        ),
    ];

    for (name, method, headers, body) in cases {
        let ts_raw = exchange(
            ts_port,
            &request_bytes(ts_port, method, &headers, body.as_bytes()),
        )
        .await;

        let rs_raw = exchange(
            rs.port,
            &request_bytes(rs.port, method, &headers, body.as_bytes()),
        )
        .await;

        assert_eq!(outcome(&rs_raw), outcome(&ts_raw), "{name}");
    }

    rs.stop().await;
}
