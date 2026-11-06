//! Client and callback server for the Agent Context Lifecycle Protocol (ACLP).
//!
//! A [`Client`] sends typed lifecycle [`events`] over loopback HTTP as JSON-RPC requests or
//! notifications. A [`Server`] validates each message against the pinned protocol schema, invokes
//! the matching [`Callbacks`] entry and checks request results against their [`ResultSchema`].
//!
//! ```no_run
//! use std::convert::Infallible;
//!
//! use aclp::events::{ContentBlock, InputSubmittedParams};
//! use aclp::{Callbacks, Client, ClientOptions, ResultSchema, Server, ServerOptions};
//! use serde::{Deserialize, Serialize};
//! use serde_json::json;
//!
//! #[derive(Serialize, Deserialize)]
//! struct ContextReply {
//!     context: Vec<String>,
//! }
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let context = ResultSchema::<ContextReply>::new(json!({
//!     "type": "object",
//!     "properties": { "context": { "type": "array", "items": { "type": "string" } } },
//!     "required": ["context"],
//!     "additionalProperties": false,
//! }))?;
//!
//! let callbacks = Callbacks::new().request(&context, |params: InputSubmittedParams| async move {
//!     let mentions_class = params
//!         .content
//!         .iter()
//!         .any(|block| matches!(block, ContentBlock::Text { text } if text.contains("class")));
//!
//!     let context = if mentions_class {
//!         vec!["Prefer plain structs and free functions.".to_owned()]
//!     } else {
//!         Vec::new()
//!     };
//!
//!     Ok::<_, Infallible>(ContextReply { context })
//! });
//!
//! let listener = tokio::net::TcpListener::bind("127.0.0.1:8787").await?;
//!
//! let server = Server::new(callbacks, ServerOptions::new())?;
//!
//! tokio::spawn(server.serve(listener, std::future::pending()));
//!
//! let client = Client::new(ClientOptions::new("http://127.0.0.1:8787/events"))?;
//!
//! let input = InputSubmittedParams {
//!     session_id: "session-1".to_owned(),
//!     run_id: None,
//!     agent_id: None,
//!     content: vec![ContentBlock::Text { text: "Create a class.".to_owned() }],
//!     structured_content: None,
//! };
//!
//! let reply = client.request(&input, &context).await?;
//! # Ok(())
//! # }
//! ```

mod callback_error;
mod callbacks;
mod client;
mod client_error;
mod client_options;
mod config_error;
mod connection;
mod discover;
mod event;
#[expect(
    missing_docs,
    reason = "generated fields mirror the pinned schema, whose event catalog documents them"
)]
pub mod events;
mod json;
mod local_schema;
mod result_schema;
mod semantics;
mod server;
mod server_options;
mod transport;
mod validation;

pub use callback_error::CallbackError;
pub use callbacks::{CallContext, Callbacks};
pub use client::Client;
pub use client_error::{ClientError, FlushError};
pub use client_options::ClientOptions;
pub use config_error::ConfigError;
pub use discover::{DISCOVERY_PATH, DiscoverOptions, discover};
pub use event::Event;
pub use local_schema::SchemaError;
pub use result_schema::ResultSchema;
pub use server::Server;
pub use server_options::{Discovery, ServerOptions};
