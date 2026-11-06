use serde_json::{Value, json};

use super::*;
use crate::events::round_trip;
use crate::result_schema::ResultSchema;

fn examples() -> Vec<(String, Value)> {
    serde_json::from_str(include_str!("../../tests/fixtures/event-examples.json"))
        .expect("fixture parses")
}

#[test]
fn all_forty_methods_accept_their_canonical_parameters() {
    let examples = examples();

    assert_eq!(DEFINITIONS.len(), 40);
    assert_eq!(examples.len(), 40);

    for (method, params) in examples {
        assert!(is_event_method(&method), "{method}");
        assert!(validate_params(&method, &params), "{method}");
        assert!(
            !validate_params(&method, &json!({ "unexpected": true })),
            "{method}"
        );
    }
}

#[test]
fn typed_parameters_round_trip_without_changing_values() {
    for (method, params) in examples() {
        let typed = round_trip(&method, params.clone()).expect("typed view accepts valid params");

        assert_eq!(typed, params, "{method}");
    }

    let present_null = json!({ "sessionId": "s", "content": [], "structuredContent": null });

    assert!(validate_params("input.submitted", &present_null));
    assert_eq!(
        round_trip("input.submitted", present_null.clone()).ok(),
        Some(present_null)
    );

    let float_count = json!({
        "sessionId": "s",
        "outcome": { "status": "succeeded" },
        "usage": { "inputTokens": 1.0 },
    });

    assert!(validate_params("model.finished", &float_count));
    assert_eq!(
        round_trip("model.finished", float_count.clone()).ok(),
        Some(float_count)
    );
}

#[test]
fn generic_envelopes_classify_unsupported_methods_before_lookup() {
    let msg = json!({ "jsonrpc": "2.0", "method": "custom.method", "params": {}, "id": "r" });

    assert!(is_envelope(&msg));
    assert!(!is_event_method("custom.method"));

    let with = |key: &str, val: Value| {
        let mut msg = msg.clone();

        msg[key] = val;
        msg
    };

    assert!(!is_envelope(&with("id", Value::from("é".repeat(65)))));
    assert!(is_envelope(&with("id", Value::from("é".repeat(64)))));
    assert!(!is_envelope(&with("id", Value::Null)));
    assert!(!is_envelope(&with("id", json!(1))));
    assert!(!is_envelope(&with("extra", json!(true))));
    assert!(!is_envelope(&with("method", json!("rpc.discover"))));
    assert!(!is_envelope(&json!([msg])));
}

#[test]
fn responses_require_a_matching_shape_and_exactly_one_outcome() {
    assert!(is_response(
        &json!({ "jsonrpc": "2.0", "id": "r", "result": null })
    ));
    assert!(!is_response(&json!({
        "jsonrpc": "2.0",
        "id": "r",
        "result": null,
        "error": { "code": -32603, "message": "Failed" },
    })));
    assert!(!is_response(&json!({
        "jsonrpc": "2.0",
        "id": "r",
        "error": { "code": -32700, "message": "Parse error" },
    })));
    assert!(is_response(&json!({
        "jsonrpc": "2.0",
        "id": null,
        "error": { "code": -32700, "message": "Parse error" },
    })));
}

#[test]
fn binary_content_must_use_canonical_base64_outside_structured_data() {
    let image = |data: &str| json!({ "sessionId": "s", "content": [{ "type": "image", "mimeType": "image/png", "data": data }] });

    assert!(validate_params("input.submitted", &image("AA==")));
    assert!(!validate_params("input.submitted", &image("AB==")));
    assert!(validate_params(
        "input.submitted",
        &json!({ "sessionId": "s", "content": [], "structuredContent": { "type": "image", "data": "AB==" } }),
    ));
}

#[test]
fn permission_origins_and_offered_identities_have_semantic_constraints() {
    let options = json!([{ "id": "allow", "label": "Allow", "decision": "allow" }]);

    let network = |origin: &str| json!({ "sessionId": "s", "operation": { "kind": "network", "origin": origin }, "options": options });

    assert!(validate_params(
        "permission.requested",
        &network("https://example.com/")
    ));

    for origin in [
        "https://example.com/path",
        "https://example.com/?",
        "https://example.com/a/..",
        "https://example.com/%2e",
        "https:example.com",
        "https://@example.com",
    ] {
        assert!(
            !validate_params("permission.requested", &network(origin)),
            "{origin}"
        );
    }

    let duplicated = json!({
        "sessionId": "s",
        "operation": { "kind": "capability", "name": "network" },
        "options": [
            { "id": "allow", "label": "Allow", "decision": "allow" },
            { "id": "allow", "label": "Other", "decision": "deny" },
        ],
    });

    assert!(!validate_params("permission.requested", &duplicated));
}

#[test]
fn catalogs_plans_settings_and_batch_calls_require_unique_identities() {
    let cases = [
        (
            "catalog.changed",
            json!({ "catalog": "skills", "entries": [{ "name": "x" }, { "name": "x", "description": "other" }] }),
        ),
        (
            "plan.updated",
            json!({ "sessionId": "s", "steps": [
            { "id": "x", "title": "A", "status": "pending" },
            { "id": "x", "title": "B", "status": "pending" },
        ] }),
        ),
        (
            "configuration.changed",
            json!({ "changes": [{ "key": "x", "value": 1 }, { "key": "x", "value": 2 }] }),
        ),
        (
            "tool.batch.finished",
            json!({ "sessionId": "s", "calls": [
            { "call": { "id": "x", "name": "a" }, "outcome": { "status": "unknown" } },
            { "call": { "id": "x", "name": "b" }, "outcome": { "status": "unknown" } },
        ] }),
        ),
    ];

    for (method, params) in cases {
        assert!(!validate_params(method, &params), "{method}");
    }
}

#[test]
fn embedded_schemas_resolve_locally_and_accepted_form_answers_satisfy_them() {
    let resolved = |value: Value| {
        json!({
            "sessionId": "s",
            "interaction": { "kind": "form", "message": "Choose count", "schema": { "type": "integer", "minimum": 1 } },
            "response": { "status": "accepted", "value": value },
        })
    };

    assert!(validate_params("interaction.resolved", &resolved(json!(1))));
    assert!(!validate_params(
        "interaction.resolved",
        &resolved(json!(0))
    ));

    let tools = |schema: Value| json!({ "catalog": "tools", "entries": [{ "name": "x", "inputSchema": schema }] });

    assert!(!validate_params(
        "catalog.changed",
        &tools(json!({ "$ref": "https://example.invalid/schema" }))
    ));
    assert!(!validate_params(
        "catalog.changed",
        &tools(json!({ "$ref": "#/definitions/missing" }))
    ));
    assert!(validate_params(
        "catalog.changed",
        &tools(json!({ "$schema": "https://json-schema.org/draft-07/schema#", "type": "object" })),
    ));
}

#[test]
fn result_schemas_validate_without_coercion_or_defaults() {
    let result = ResultSchema::<Value>::new(json!({
        "type": "object",
        "properties": { "count": { "type": "number", "default": 1 } },
        "required": ["count"],
        "additionalProperties": false,
    }))
    .expect("compiles");

    let missing = json!({});

    assert!(result.is_valid(&json!({ "count": 1 })));
    assert!(!result.is_valid(&json!({ "count": "1" })));
    assert!(!result.is_valid(&missing));
    assert_eq!(missing, json!({}));
    assert!(
        ResultSchema::<String>::new(
            json!({ "type": "string", "$ref": "https://example.invalid/schema" })
        )
        .is_err()
    );
}

#[test]
fn notification_classification_requires_an_absent_id() {
    assert!(is_notification(
        &json!({ "jsonrpc": "2.0", "method": "x", "params": 1 })
    ));
    assert!(!is_notification(
        &json!({ "jsonrpc": "2.0", "method": "x", "id": null })
    ));
    assert!(!is_notification(
        &json!({ "jsonrpc": "1.0", "method": "x" })
    ));
    assert_eq!(request_id(&json!({ "id": "r" })), Some("r"));
    assert_eq!(request_id(&json!({ "id": "" })), None);
    assert_eq!(request_id(&json!({ "id": "x".repeat(129) })), None);
}
