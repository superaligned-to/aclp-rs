//! Standard ACLP event parameters generated from the pinned protocol schema.
//!
//! Regenerate with `cargo xtask generate`; the gate rejects hand edits.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Number, Value};

use crate::event::{Event, sealed::Sealed};

fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddedResource {
    Text {
        uri: String,
        #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        text: String,
    },
    Blob {
        uri: String,
        #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        blob: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "audio")]
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource_link")]
    ResourceLink {
        uri: String,
        name: String,
        #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    #[serde(rename = "resource")]
    Resource { resource: EmbeddedResource },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    pub content: Vec<ContentBlock>,
    #[serde(
        rename = "structuredContent",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub structured_content: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputPayload {
    pub content: Vec<ContentBlock>,
    #[serde(
        rename = "structuredContent",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub structured_content: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Completion {
    #[serde(rename = "succeeded")]
    Succeeded {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Payload>,
    },
    #[serde(rename = "failed")]
    Failed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<Failure>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Payload>,
    },
    #[serde(rename = "cancelled")]
    Cancelled {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Payload>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum RunCompletion {
    #[serde(rename = "succeeded")]
    Succeeded {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Payload>,
    },
    #[serde(rename = "failed")]
    Failed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<Failure>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Payload>,
    },
    #[serde(rename = "cancelled")]
    Cancelled {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Payload>,
    },
    #[serde(rename = "interrupted")]
    Interrupted {
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Payload>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ToolCompletion {
    #[serde(rename = "succeeded")]
    Succeeded {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Payload>,
    },
    #[serde(rename = "failed")]
    Failed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<Failure>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Payload>,
    },
    #[serde(rename = "cancelled")]
    Cancelled {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Payload>,
    },
    #[serde(rename = "denied")]
    Denied {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    #[serde(rename = "unknown")]
    Unknown {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<Payload>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedToolCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub arguments: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposedToolCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelOutput {
    pub content: Vec<ContentBlock>,
    #[serde(
        rename = "structuredContent",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub structured_content: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calls: Option<Vec<ProposedToolCall>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ModelCompletion {
    #[serde(rename = "succeeded")]
    Succeeded {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<ModelOutput>,
    },
    #[serde(rename = "failed")]
    Failed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<Failure>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<ModelOutput>,
    },
    #[serde(rename = "cancelled")]
    Cancelled {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<ModelOutput>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(
        rename = "outputSchema",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub output_schema: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub kind: SourceKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceKind {
    #[serde(rename = "skill")]
    Skill,
    #[serde(rename = "instructions")]
    Instructions,
    #[serde(rename = "memory")]
    Memory,
    #[serde(rename = "resource")]
    Resource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: Vec<ContentBlock>,
    #[serde(
        rename = "structuredContent",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub structured_content: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calls: Option<Vec<ProposedToolCall>>,
    #[serde(rename = "callId", default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelMessageRole {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "developer")]
    Developer,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInput {
    pub messages: Vec<ModelMessage>,
    pub view: ModelInputView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelInputView {
    #[serde(rename = "complete")]
    Complete,
    #[serde(rename = "partial")]
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(
        rename = "inputTokens",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub input_tokens: Option<Number>,
    #[serde(
        rename = "outputTokens",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub output_tokens: Option<Number>,
    #[serde(
        rename = "cachedInputTokens",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cached_input_tokens: Option<Number>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Operation {
    #[serde(rename = "tool")]
    Tool { call: ProposedToolCall },
    #[serde(rename = "resource")]
    Resource { uri: String, access: ResourceAccess },
    #[serde(rename = "network")]
    Network { origin: String },
    #[serde(rename = "capability")]
    Capability { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceAccess {
    #[serde(rename = "read")]
    Read,
    #[serde(rename = "write")]
    Write,
    #[serde(rename = "execute")]
    Execute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionOption {
    pub id: String,
    pub label: String,
    pub decision: PermissionOptionDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<PermissionOptionScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PermissionOptionDecision {
    #[serde(rename = "allow")]
    Allow,
    #[serde(rename = "deny")]
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PermissionOptionScope {
    #[serde(rename = "once")]
    Once,
    #[serde(rename = "session")]
    Session,
    #[serde(rename = "persistent")]
    Persistent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Interaction {
    #[serde(rename = "question")]
    Question { message: String },
    #[serde(rename = "form")]
    Form { message: String, schema: Value },
    #[serde(rename = "url")]
    Url { message: String, url: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Answer {
    #[serde(rename = "accepted")]
    Accepted { value: Value },
    #[serde(rename = "declined")]
    Declined,
    #[serde(rename = "cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum UrlAnswer {
    #[serde(rename = "accepted")]
    Accepted,
    #[serde(rename = "declined")]
    Declined,
    #[serde(rename = "cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub content: Vec<ContentBlock>,
    #[serde(
        rename = "structuredContent",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub structured_content: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolFinishedItem {
    pub call: ObservedToolCall,
    pub outcome: ToolCompletion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    pub status: PlanStepStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlanStepStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub status: TaskSnapshotStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskSnapshotStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "blocked")]
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRef {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRef {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRef {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingChange {
    pub key: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<Value>,
}

/// Parameters of the `session.started` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStartedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(
        rename = "parentSessionId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub parent_session_id: Option<String>,
}

/// Parameters of the `session.ended` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEndedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Parameters of the `input.submitted` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSubmittedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub content: Vec<ContentBlock>,
    #[serde(
        rename = "structuredContent",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub structured_content: Option<Value>,
}

/// Parameters of the `run.started` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStartedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputPayload>,
    #[serde(
        rename = "resumesRunId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub resumes_run_id: Option<String>,
}

/// Parameters of the `run.finishing` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFinishingParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Payload>,
}

/// Parameters of the `run.finished` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFinishedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub outcome: RunCompletion,
}

/// Parameters of the `tool.requested` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRequestedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub call: ProposedToolCall,
}

/// Parameters of the `tool.started` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolStartedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub call: ObservedToolCall,
}

/// Parameters of the `tool.finished` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolFinishedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub call: ObservedToolCall,
    pub outcome: ToolCompletion,
}

/// Parameters of the `context.compaction.requested` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCompactionRequestedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Parameters of the `context.compaction.finished` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCompactionFinishedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub outcome: Completion,
}

/// Parameters of the `context.source.loaded` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextSourceLoadedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Payload>,
}

/// Parameters of the `catalog.changed` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogChangedParams {
    #[serde(rename = "sessionId", default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(flatten)]
    pub catalog: Catalog,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "catalog")]
pub enum Catalog {
    #[serde(rename = "tools")]
    Tools { entries: Vec<ToolDefinition> },
    #[serde(rename = "skills")]
    Skills { entries: Vec<SkillDefinition> },
}

/// Parameters of the `model.requested` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequestedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<ModelInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(
        rename = "modelCallId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub model_call_id: Option<String>,
}

/// Parameters of the `model.finished` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFinishedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    #[serde(
        rename = "modelCallId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub model_call_id: Option<String>,
    pub outcome: ModelCompletion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// Parameters of the `permission.requested` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRequestedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub operation: Operation,
    pub options: Vec<PermissionOption>,
}

/// Parameters of the `permission.resolved` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionResolvedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub operation: Operation,
    pub decision: PermissionResolvedDecision,
    #[serde(rename = "optionId", default, skip_serializing_if = "Option::is_none")]
    pub option_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PermissionResolvedDecision {
    #[serde(rename = "allowed")]
    Allowed,
    #[serde(rename = "denied")]
    Denied,
    #[serde(rename = "cancelled")]
    Cancelled,
}

/// Parameters of the `interaction.requested` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionRequestedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub interaction: Interaction,
}

/// Parameters of the `interaction.resolved` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionResolvedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub interaction: Interaction,
    pub response: Value,
}

/// Parameters of the `agent.requested` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRequestedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub agent: AgentRef,
    #[serde(
        rename = "childRunId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub child_run_id: Option<String>,
    pub input: InputPayload,
}

/// Parameters of the `agent.started` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStartedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub agent: AgentRef,
    #[serde(
        rename = "childRunId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub child_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputPayload>,
}

/// Parameters of the `agent.finished` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFinishedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub agent: AgentRef,
    #[serde(
        rename = "childRunId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub child_run_id: Option<String>,
    pub outcome: RunCompletion,
}

/// Parameters of the `handoff.requested` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffRequestedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub from: AgentRef,
    pub to: AgentRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputPayload>,
}

/// Parameters of the `handoff.finished` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffFinishedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub from: AgentRef,
    pub to: AgentRef,
    pub outcome: Completion,
}

/// Parameters of the `message.emitting` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageEmittingParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub message: Message,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient: Option<AgentRef>,
}

/// Parameters of the `message.emitted` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageEmittedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub message: Message,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient: Option<AgentRef>,
}

/// Parameters of the `run.cancellation.requested` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCancellationRequestedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Parameters of the `run.paused` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunPausedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(
        rename = "interactionId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub interaction_id: Option<String>,
}

/// Parameters of the `run.resumed` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunResumedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(
        rename = "interactionId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub interaction_id: Option<String>,
}

/// Parameters of the `tool.batch.finished` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBatchFinishedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(rename = "batchId", default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    pub calls: Vec<ToolFinishedItem>,
}

/// Parameters of the `plan.updated` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanUpdatedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub steps: Vec<PlanStep>,
}

/// Parameters of the `task.created` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCreatedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub task: TaskSnapshot,
}

/// Parameters of the `task.updated` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskUpdatedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub task: TaskSnapshot,
}

/// Parameters of the `task.finished` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskFinishedParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub task: TaskRef,
    pub outcome: Completion,
}

/// Parameters of the `workspace.changed` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceChangedParams {
    #[serde(rename = "sessionId", default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub workspace: WorkspaceRef,
    pub change: WorkspaceChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkspaceChange {
    #[serde(rename = "opened")]
    Opened,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "updated")]
    Updated,
}

/// Parameters of the `resource.changed` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceChangedParams {
    #[serde(rename = "sessionId", default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub resource: ResourceRef,
    pub change: ResourceChange,
    #[serde(
        rename = "previousUri",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_uri: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceChange {
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "modified")]
    Modified,
    #[serde(rename = "deleted")]
    Deleted,
    #[serde(rename = "renamed")]
    Renamed,
}

/// Parameters of the `configuration.changed` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigurationChangedParams {
    #[serde(rename = "sessionId", default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub changes: Vec<SettingChange>,
}

/// Parameters of the `model.changed` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelChangedParams {
    #[serde(rename = "sessionId", default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub model: ModelRef,
    #[serde(
        rename = "previousModel",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_model: Option<ModelRef>,
}

/// Parameters of the `notification.created` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationCreatedParams {
    #[serde(rename = "sessionId", default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub level: NotificationLevel,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NotificationLevel {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "error")]
    Error,
}

/// Parameters of the `error.occurred` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorOccurredParams {
    #[serde(rename = "sessionId", default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "runId", default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(rename = "agentId", default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub error: Failure,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<Operation>,
}

impl Sealed for SessionStartedParams {}

impl Event for SessionStartedParams {
    const METHOD: &'static str = "session.started";
}

impl Sealed for SessionEndedParams {}

impl Event for SessionEndedParams {
    const METHOD: &'static str = "session.ended";
}

impl Sealed for InputSubmittedParams {}

impl Event for InputSubmittedParams {
    const METHOD: &'static str = "input.submitted";
}

impl Sealed for RunStartedParams {}

impl Event for RunStartedParams {
    const METHOD: &'static str = "run.started";
}

impl Sealed for RunFinishingParams {}

impl Event for RunFinishingParams {
    const METHOD: &'static str = "run.finishing";
}

impl Sealed for RunFinishedParams {}

impl Event for RunFinishedParams {
    const METHOD: &'static str = "run.finished";
}

impl Sealed for ToolRequestedParams {}

impl Event for ToolRequestedParams {
    const METHOD: &'static str = "tool.requested";
}

impl Sealed for ToolStartedParams {}

impl Event for ToolStartedParams {
    const METHOD: &'static str = "tool.started";
}

impl Sealed for ToolFinishedParams {}

impl Event for ToolFinishedParams {
    const METHOD: &'static str = "tool.finished";
}

impl Sealed for ContextCompactionRequestedParams {}

impl Event for ContextCompactionRequestedParams {
    const METHOD: &'static str = "context.compaction.requested";
}

impl Sealed for ContextCompactionFinishedParams {}

impl Event for ContextCompactionFinishedParams {
    const METHOD: &'static str = "context.compaction.finished";
}

impl Sealed for ContextSourceLoadedParams {}

impl Event for ContextSourceLoadedParams {
    const METHOD: &'static str = "context.source.loaded";
}

impl Sealed for CatalogChangedParams {}

impl Event for CatalogChangedParams {
    const METHOD: &'static str = "catalog.changed";
}

impl Sealed for ModelRequestedParams {}

impl Event for ModelRequestedParams {
    const METHOD: &'static str = "model.requested";
}

impl Sealed for ModelFinishedParams {}

impl Event for ModelFinishedParams {
    const METHOD: &'static str = "model.finished";
}

impl Sealed for PermissionRequestedParams {}

impl Event for PermissionRequestedParams {
    const METHOD: &'static str = "permission.requested";
}

impl Sealed for PermissionResolvedParams {}

impl Event for PermissionResolvedParams {
    const METHOD: &'static str = "permission.resolved";
}

impl Sealed for InteractionRequestedParams {}

impl Event for InteractionRequestedParams {
    const METHOD: &'static str = "interaction.requested";
}

impl Sealed for InteractionResolvedParams {}

impl Event for InteractionResolvedParams {
    const METHOD: &'static str = "interaction.resolved";
}

impl Sealed for AgentRequestedParams {}

impl Event for AgentRequestedParams {
    const METHOD: &'static str = "agent.requested";
}

impl Sealed for AgentStartedParams {}

impl Event for AgentStartedParams {
    const METHOD: &'static str = "agent.started";
}

impl Sealed for AgentFinishedParams {}

impl Event for AgentFinishedParams {
    const METHOD: &'static str = "agent.finished";
}

impl Sealed for HandoffRequestedParams {}

impl Event for HandoffRequestedParams {
    const METHOD: &'static str = "handoff.requested";
}

impl Sealed for HandoffFinishedParams {}

impl Event for HandoffFinishedParams {
    const METHOD: &'static str = "handoff.finished";
}

impl Sealed for MessageEmittingParams {}

impl Event for MessageEmittingParams {
    const METHOD: &'static str = "message.emitting";
}

impl Sealed for MessageEmittedParams {}

impl Event for MessageEmittedParams {
    const METHOD: &'static str = "message.emitted";
}

impl Sealed for RunCancellationRequestedParams {}

impl Event for RunCancellationRequestedParams {
    const METHOD: &'static str = "run.cancellation.requested";
}

impl Sealed for RunPausedParams {}

impl Event for RunPausedParams {
    const METHOD: &'static str = "run.paused";
}

impl Sealed for RunResumedParams {}

impl Event for RunResumedParams {
    const METHOD: &'static str = "run.resumed";
}

impl Sealed for ToolBatchFinishedParams {}

impl Event for ToolBatchFinishedParams {
    const METHOD: &'static str = "tool.batch.finished";
}

impl Sealed for PlanUpdatedParams {}

impl Event for PlanUpdatedParams {
    const METHOD: &'static str = "plan.updated";
}

impl Sealed for TaskCreatedParams {}

impl Event for TaskCreatedParams {
    const METHOD: &'static str = "task.created";
}

impl Sealed for TaskUpdatedParams {}

impl Event for TaskUpdatedParams {
    const METHOD: &'static str = "task.updated";
}

impl Sealed for TaskFinishedParams {}

impl Event for TaskFinishedParams {
    const METHOD: &'static str = "task.finished";
}

impl Sealed for WorkspaceChangedParams {}

impl Event for WorkspaceChangedParams {
    const METHOD: &'static str = "workspace.changed";
}

impl Sealed for ResourceChangedParams {}

impl Event for ResourceChangedParams {
    const METHOD: &'static str = "resource.changed";
}

impl Sealed for ConfigurationChangedParams {}

impl Event for ConfigurationChangedParams {
    const METHOD: &'static str = "configuration.changed";
}

impl Sealed for ModelChangedParams {}

impl Event for ModelChangedParams {
    const METHOD: &'static str = "model.changed";
}

impl Sealed for NotificationCreatedParams {}

impl Event for NotificationCreatedParams {
    const METHOD: &'static str = "notification.created";
}

impl Sealed for ErrorOccurredParams {}

impl Event for ErrorOccurredParams {
    const METHOD: &'static str = "error.occurred";
}

pub(crate) const DEFINITIONS: [(&str, &str); 40] = [
    ("session.started", "SessionStartedParams"),
    ("session.ended", "SessionEndedParams"),
    ("input.submitted", "InputSubmittedParams"),
    ("run.started", "RunStartedParams"),
    ("run.finishing", "RunFinishingParams"),
    ("run.finished", "RunFinishedParams"),
    ("tool.requested", "ToolRequestedParams"),
    ("tool.started", "ToolStartedParams"),
    ("tool.finished", "ToolFinishedParams"),
    (
        "context.compaction.requested",
        "ContextCompactionRequestedParams",
    ),
    (
        "context.compaction.finished",
        "ContextCompactionFinishedParams",
    ),
    ("context.source.loaded", "ContextSourceLoadedParams"),
    ("catalog.changed", "CatalogChangedParams"),
    ("model.requested", "ModelRequestedParams"),
    ("model.finished", "ModelFinishedParams"),
    ("permission.requested", "PermissionRequestedParams"),
    ("permission.resolved", "PermissionResolvedParams"),
    ("interaction.requested", "InteractionRequestedParams"),
    ("interaction.resolved", "InteractionResolvedParams"),
    ("agent.requested", "AgentRequestedParams"),
    ("agent.started", "AgentStartedParams"),
    ("agent.finished", "AgentFinishedParams"),
    ("handoff.requested", "HandoffRequestedParams"),
    ("handoff.finished", "HandoffFinishedParams"),
    ("message.emitting", "MessageEmittingParams"),
    ("message.emitted", "MessageEmittedParams"),
    (
        "run.cancellation.requested",
        "RunCancellationRequestedParams",
    ),
    ("run.paused", "RunPausedParams"),
    ("run.resumed", "RunResumedParams"),
    ("tool.batch.finished", "ToolBatchFinishedParams"),
    ("plan.updated", "PlanUpdatedParams"),
    ("task.created", "TaskCreatedParams"),
    ("task.updated", "TaskUpdatedParams"),
    ("task.finished", "TaskFinishedParams"),
    ("workspace.changed", "WorkspaceChangedParams"),
    ("resource.changed", "ResourceChangedParams"),
    ("configuration.changed", "ConfigurationChangedParams"),
    ("model.changed", "ModelChangedParams"),
    ("notification.created", "NotificationCreatedParams"),
    ("error.occurred", "ErrorOccurredParams"),
];

#[cfg(test)]
pub(crate) fn round_trip(method: &str, params: Value) -> serde_json::Result<Value> {
    match method {
        "session.started" => {
            serde_json::from_value::<SessionStartedParams>(params).and_then(serde_json::to_value)
        }
        "session.ended" => {
            serde_json::from_value::<SessionEndedParams>(params).and_then(serde_json::to_value)
        }
        "input.submitted" => {
            serde_json::from_value::<InputSubmittedParams>(params).and_then(serde_json::to_value)
        }
        "run.started" => {
            serde_json::from_value::<RunStartedParams>(params).and_then(serde_json::to_value)
        }
        "run.finishing" => {
            serde_json::from_value::<RunFinishingParams>(params).and_then(serde_json::to_value)
        }
        "run.finished" => {
            serde_json::from_value::<RunFinishedParams>(params).and_then(serde_json::to_value)
        }
        "tool.requested" => {
            serde_json::from_value::<ToolRequestedParams>(params).and_then(serde_json::to_value)
        }
        "tool.started" => {
            serde_json::from_value::<ToolStartedParams>(params).and_then(serde_json::to_value)
        }
        "tool.finished" => {
            serde_json::from_value::<ToolFinishedParams>(params).and_then(serde_json::to_value)
        }
        "context.compaction.requested" => {
            serde_json::from_value::<ContextCompactionRequestedParams>(params)
                .and_then(serde_json::to_value)
        }
        "context.compaction.finished" => {
            serde_json::from_value::<ContextCompactionFinishedParams>(params)
                .and_then(serde_json::to_value)
        }
        "context.source.loaded" => serde_json::from_value::<ContextSourceLoadedParams>(params)
            .and_then(serde_json::to_value),
        "catalog.changed" => {
            serde_json::from_value::<CatalogChangedParams>(params).and_then(serde_json::to_value)
        }
        "model.requested" => {
            serde_json::from_value::<ModelRequestedParams>(params).and_then(serde_json::to_value)
        }
        "model.finished" => {
            serde_json::from_value::<ModelFinishedParams>(params).and_then(serde_json::to_value)
        }
        "permission.requested" => serde_json::from_value::<PermissionRequestedParams>(params)
            .and_then(serde_json::to_value),
        "permission.resolved" => serde_json::from_value::<PermissionResolvedParams>(params)
            .and_then(serde_json::to_value),
        "interaction.requested" => serde_json::from_value::<InteractionRequestedParams>(params)
            .and_then(serde_json::to_value),
        "interaction.resolved" => serde_json::from_value::<InteractionResolvedParams>(params)
            .and_then(serde_json::to_value),
        "agent.requested" => {
            serde_json::from_value::<AgentRequestedParams>(params).and_then(serde_json::to_value)
        }
        "agent.started" => {
            serde_json::from_value::<AgentStartedParams>(params).and_then(serde_json::to_value)
        }
        "agent.finished" => {
            serde_json::from_value::<AgentFinishedParams>(params).and_then(serde_json::to_value)
        }
        "handoff.requested" => {
            serde_json::from_value::<HandoffRequestedParams>(params).and_then(serde_json::to_value)
        }
        "handoff.finished" => {
            serde_json::from_value::<HandoffFinishedParams>(params).and_then(serde_json::to_value)
        }
        "message.emitting" => {
            serde_json::from_value::<MessageEmittingParams>(params).and_then(serde_json::to_value)
        }
        "message.emitted" => {
            serde_json::from_value::<MessageEmittedParams>(params).and_then(serde_json::to_value)
        }
        "run.cancellation.requested" => {
            serde_json::from_value::<RunCancellationRequestedParams>(params)
                .and_then(serde_json::to_value)
        }
        "run.paused" => {
            serde_json::from_value::<RunPausedParams>(params).and_then(serde_json::to_value)
        }
        "run.resumed" => {
            serde_json::from_value::<RunResumedParams>(params).and_then(serde_json::to_value)
        }
        "tool.batch.finished" => {
            serde_json::from_value::<ToolBatchFinishedParams>(params).and_then(serde_json::to_value)
        }
        "plan.updated" => {
            serde_json::from_value::<PlanUpdatedParams>(params).and_then(serde_json::to_value)
        }
        "task.created" => {
            serde_json::from_value::<TaskCreatedParams>(params).and_then(serde_json::to_value)
        }
        "task.updated" => {
            serde_json::from_value::<TaskUpdatedParams>(params).and_then(serde_json::to_value)
        }
        "task.finished" => {
            serde_json::from_value::<TaskFinishedParams>(params).and_then(serde_json::to_value)
        }
        "workspace.changed" => {
            serde_json::from_value::<WorkspaceChangedParams>(params).and_then(serde_json::to_value)
        }
        "resource.changed" => {
            serde_json::from_value::<ResourceChangedParams>(params).and_then(serde_json::to_value)
        }
        "configuration.changed" => serde_json::from_value::<ConfigurationChangedParams>(params)
            .and_then(serde_json::to_value),
        "model.changed" => {
            serde_json::from_value::<ModelChangedParams>(params).and_then(serde_json::to_value)
        }
        "notification.created" => serde_json::from_value::<NotificationCreatedParams>(params)
            .and_then(serde_json::to_value),
        "error.occurred" => {
            serde_json::from_value::<ErrorOccurredParams>(params).and_then(serde_json::to_value)
        }
        _ => Err(serde::de::Error::custom("unknown method")),
    }
}
