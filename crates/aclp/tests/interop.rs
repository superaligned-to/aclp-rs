//! Interoperability with the TypeScript SDK; run with `cargo xtask interop <aclp-ts checkout>`.

mod common;
#[path = "interop/harness.rs"]
mod harness;

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::time::Duration;

use aclp::events::{
    AgentFinishedParams, AgentRequestedParams, AgentStartedParams, CatalogChangedParams,
    ClientInfo, ClientRegisteredParams, ClientUnregisteredParams, ClientUpdatedParams,
    ConfigurationChangedParams, ContextCompactionFinishedParams, ContextCompactionRequestedParams,
    ContextSourceLoadedParams, ErrorOccurredParams, HandoffFinishedParams, HandoffRequestedParams,
    InputSubmittedParams, InteractionRequestedParams, InteractionResolvedParams,
    MessageEmittedParams, MessageEmittingParams, ModelChangedParams, ModelFinishedParams,
    ModelRequestedParams, NotificationCreatedParams, PermissionRequestedParams,
    PermissionResolvedParams, PlanUpdatedParams, ResourceChangedParams,
    RunCancellationRequestedParams, RunFinishedParams, RunFinishingParams, RunPausedParams,
    RunResumedParams, RunStartedParams, SessionEndedParams, SessionStartedParams, SoftwareRef,
    TaskCreatedParams, TaskFinishedParams, TaskUpdatedParams, ToolBatchFinishedParams,
    ToolFinishedParams, ToolRequestedParams, ToolStartedParams, WorkspaceChangedParams,
};
use aclp::{
    CallContext, Callbacks, Client, ClientError, ClientOptions, DiscoverOptions, Discovery, Event,
    ResultSchema, ServerOptions, discover,
};
use common::{ContextReply, Raw, context_schema, exchange, request_bytes, start};
use harness::Peer;
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

const AUTH: &str = "Bearer interop";

const MAX_BODY_BYTES: usize = 1_048_576;

const PUBLIC_HOST: &str = "aclp.example.com";

type Seen = (&'static str, Option<String>);

fn identity() -> ClientInfo {
    ClientInfo {
        name: "interop".to_owned(),
        version: Some("1.0.0".to_owned()),
        harness: Some(SoftwareRef {
            name: "rust".to_owned(),
            version: Some("1".to_owned()),
        }),
    }
}

fn server_options() -> ServerOptions {
    ServerOptions::new()
        .authorization(AUTH)
        .hosts([PUBLIC_HOST])
        .discovery(Discovery::new("https://aclp.example.com/events").name("Interop"))
}

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
            ErrorOccurredParams,
            ClientRegisteredParams,
            ClientUpdatedParams,
            ClientUnregisteredParams
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

fn observing(tx: &UnboundedSender<Seen>) -> Callbacks {
    let mut callbacks = Callbacks::new();

    macro_rules! observe {
        ($($t:ident),*) => {
            $(
                let sink = tx.clone();

                callbacks = callbacks.notification_with(move |_: $t, ctx: CallContext| {
                    let _ = sink.send((<$t as Event>::METHOD, ctx.client));

                    async { Ok::<_, Infallible>(()) }
                });
            )*
        };
    }

    events!(observe);

    let sink = tx.clone();

    let callbacks = callbacks.request_with(
        &context_schema(),
        move |input: InputSubmittedParams, ctx: CallContext| {
            let _ = sink.send((InputSubmittedParams::METHOD, ctx.client));

            async move {
                Ok::<_, Infallible>(ContextReply {
                    context: vec![format!("rs:{}", input.content.len())],
                })
            }
        },
    );

    let sink = tx.clone();

    callbacks.request_with(
        &strict(),
        move |_: SessionStartedParams, ctx: CallContext| {
            let _ = sink.send((SessionStartedParams::METHOD, ctx.client));

            async { Ok::<_, Infallible>(json!({})) }
        },
    )
}

#[tokio::test]
#[ignore = "needs Node and an aclp-ts build; run cargo xtask interop"]
async fn typescript_server_accepts_every_rust_event() {
    let mut ts = Peer::spawn(&["server"]).await;

    let url = format!("http://127.0.0.1:{}/events", ts.port().await);

    let client = Client::new(
        ClientOptions::new(&url)
            .authorization(AUTH)
            .client(identity()),
    )
    .expect("client config");

    for batch in examples().chunks(16) {
        for (method, params) in batch {
            notify(&client, method, params.clone());
        }

        client.flush().await.expect("every notification admitted");
    }

    let mut seen = BTreeSet::new();

    while seen.len() < methods().len() {
        let received = ts.line().await;

        assert_eq!(received["client"], "interop/1.0.0 rust/1", "{received}");
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

    let server = start(observing(&tx), server_options()).await;

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
        let (method, client) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("callbacks run")
            .expect("server keeps the channel");

        assert_eq!(client.as_deref(), Some("interop/1.0.0 node/26"), "{method}");
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

    let rs = start(observing(&tx), server_options()).await;

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
        ("named public host", "POST", vec![auth, ("Host", PUBLIC_HOST)], tool.to_string()),
        ("named public host with port", "POST", vec![auth, ("Host", "aclp.example.com:443")], tool.to_string()),
        ("named public host without credentials", "POST", vec![("Host", PUBLIC_HOST)], tool.to_string()),
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

fn discovery_request(port: u16) -> Vec<u8> {
    format!("GET /.well-known/aclp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n")
        .into_bytes()
}

#[tokio::test]
#[ignore = "needs Node and an aclp-ts build; run cargo xtask interop"]
async fn discovery_documents_agree_across_sdks() {
    let mut ts = Peer::spawn(&["server"]).await;

    let ts_port = ts.port().await;

    let (tx, _rx) = unbounded_channel();

    let rs = start(observing(&tx), server_options()).await;

    let from_ts = discover(
        &format!("http://127.0.0.1:{ts_port}"),
        DiscoverOptions::new(),
    )
    .await
    .expect("Rust client reads the TypeScript document");

    let mut reader = Peer::spawn(&["discover", &format!("http://127.0.0.1:{}", rs.port)]).await;

    let from_rs = reader.line().await;

    assert_eq!(
        serde_json::to_value(&from_ts).expect("document JSON"),
        from_rs
    );
    assert_eq!(from_ts.events.len(), methods().len());
    assert_eq!(from_ts.requests, ["input.submitted", "session.started"]);

    let ts_raw = exchange(ts_port, &discovery_request(ts_port)).await;

    let rs_raw = exchange(rs.port, &discovery_request(rs.port)).await;

    assert_eq!(
        (rs_raw.status, rs_raw.json()),
        (ts_raw.status, ts_raw.json())
    );
    rs.stop().await;
}
