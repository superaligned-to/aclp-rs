use std::collections::HashMap;
use std::sync::LazyLock;

use jsonschema::Validator;
use serde_json::{Value, json};

use crate::events::DEFINITIONS;
use crate::local_schema;
use crate::semantics;

const MAX_ID_BYTES: usize = 128;

struct Validators {
    envelope: Validator,
    response: Validator,
    params: HashMap<&'static str, Validator>,
}

static VALIDATORS: LazyLock<Validators> = LazyLock::new(|| {
    let root: Value =
        serde_json::from_str(include_str!("../schema.json")).expect("the pinned schema is JSON");

    let compile = |refs: Value| {
        let mut schema = root.clone();

        if let Some(object) = schema.as_object_mut() {
            object.remove("oneOf");
            object.insert("allOf".to_owned(), refs);
        }

        local_schema::draft_07()
            .build(&schema)
            .expect("the pinned schema compiles")
    };

    let reference = |name: &str| json!({ "$ref": format!("#/definitions/{name}") });

    Validators {
        envelope: compile(json!([reference("InvocationEnvelope")])),
        response: compile(json!([{
            "oneOf": [reference("SuccessResponse"), reference("ErrorResponse")],
        }])),
        params: DEFINITIONS
            .iter()
            .map(|(method, def)| (*method, compile(json!([reference(def)]))))
            .collect(),
    }
});

/// Compiles the protocol validators now rather than on the first message.
pub(crate) fn warm() {
    LazyLock::force(&VALIDATORS);
}

fn bounded_id(val: &Value) -> bool {
    match val.get("id") {
        None | Some(Value::Null) => true,
        Some(Value::String(id)) => id.len() <= MAX_ID_BYTES,
        Some(_) => false,
    }
}

pub(crate) fn is_envelope(val: &Value) -> bool {
    val.is_object() && bounded_id(val) && VALIDATORS.envelope.is_valid(val)
}

pub(crate) fn is_response(val: &Value) -> bool {
    val.is_object() && bounded_id(val) && VALIDATORS.response.is_valid(val)
}

pub(crate) fn is_event_method(method: &str) -> bool {
    VALIDATORS.params.contains_key(method)
}

pub(crate) fn validate_params(method: &str, params: &Value) -> bool {
    VALIDATORS
        .params
        .get(method)
        .is_some_and(|v| v.is_valid(params))
        && semantics::validate(method, params)
}

pub(crate) fn request_id(val: &Value) -> Option<&str> {
    val.get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= MAX_ID_BYTES)
}

pub(crate) fn is_notification(val: &Value) -> bool {
    val.get("jsonrpc").and_then(Value::as_str) == Some("2.0")
        && val
            .get("method")
            .and_then(Value::as_str)
            .is_some_and(|m| !m.is_empty())
        && val.get("id").is_none()
}

#[cfg(test)]
mod tests;
