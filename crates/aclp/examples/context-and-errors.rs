//! Injects context before a model request and counts errors in observed tool output.

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use aclp::events::{
    ContentBlock, InputSubmittedParams, ObservedToolCall, Payload, ToolCompletion,
    ToolFinishedParams,
};
use aclp::{Callbacks, Client, ClientOptions, ResultSchema, Server, ServerOptions};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::{Notify, oneshot};

#[derive(Debug, Serialize, Deserialize)]
struct ContextReply {
    context: Vec<String>,
}

fn texts(content: &[ContentBlock]) -> impl Iterator<Item = &str> {
    content.iter().filter_map(|block| match block {
        ContentBlock::Text { text } => Some(text.as_str()),
        _ => None,
    })
}

fn output(outcome: &ToolCompletion) -> Option<&Payload> {
    match outcome {
        ToolCompletion::Succeeded { output }
        | ToolCompletion::Failed { output, .. }
        | ToolCompletion::Cancelled { output, .. }
        | ToolCompletion::Unknown { output, .. } => output.as_ref(),
        ToolCompletion::Denied { .. } => None,
    }
}

#[tokio::main]
#[expect(clippy::print_stdout, reason = "the example reports what it observed")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let context = ResultSchema::<ContextReply>::new(json!({
        "type": "object",
        "properties": { "context": { "type": "array", "items": { "type": "string" } } },
        "required": ["context"],
        "additionalProperties": false,
    }))?;

    let errors = Arc::new(AtomicUsize::new(0));

    let observed = Arc::new(Notify::new());

    let (counter, done) = (Arc::clone(&errors), Arc::clone(&observed));

    let callbacks = Callbacks::new()
        .request(&context, |input: InputSubmittedParams| async move {
            let class = texts(&input.content).any(|text| text.contains("class"));

            let context = if class {
                vec![
                    "Prefer plain structs, free functions and traits over class hierarchies."
                        .to_owned(),
                ]
            } else {
                Vec::new()
            };

            Ok::<_, Infallible>(ContextReply { context })
        })
        .notification(move |finished: ToolFinishedParams| {
            let content = output(&finished.outcome).map_or(&[][..], |p| p.content.as_slice());

            let found = texts(content)
                .map(|text| text.matches("error").count())
                .sum();

            counter.fetch_add(found, Ordering::SeqCst);
            done.notify_one();

            async { Ok::<_, Infallible>(()) }
        });

    let listener = TcpListener::bind("127.0.0.1:0").await?;

    let url = format!("http://{}/events", listener.local_addr()?);

    let (stop, stopped) = oneshot::channel::<()>();

    let server = tokio::spawn(Server::new(callbacks, ServerOptions::new())?.serve(
        listener,
        async {
            let _ = stopped.await;
        },
    ));

    let client = Client::new(ClientOptions::new(url))?;

    let input = InputSubmittedParams {
        session_id: "example-session".to_owned(),
        run_id: None,
        agent_id: None,
        content: vec![ContentBlock::Text {
            text: "Create a Rust class for parsing configuration.".to_owned(),
        }],
        structured_content: None,
    };

    let reply = client.request(&input, &context).await?;

    println!(
        "Context to apply before the model request: {:?}",
        reply.context
    );

    client.notify(&ToolFinishedParams {
        session_id: "example-session".to_owned(),
        run_id: None,
        agent_id: None,
        call: ObservedToolCall {
            id: None,
            name: "shell".to_owned(),
            arguments: Some(json!({ "command": "cargo test" })),
        },
        outcome: ToolCompletion::Failed {
            error: None,
            output: Some(Payload {
                content: vec![ContentBlock::Text {
                    text: "error: missing name\nerror: missing return type".to_owned(),
                }],
                structured_content: None,
            }),
        },
    })?;

    client.flush().await?;
    observed.notified().await;
    println!("Observed errors: {}", errors.load(Ordering::SeqCst));

    let _ = stop.send(());
    server.await?;
    Ok(())
}
