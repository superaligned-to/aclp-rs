use std::collections::HashSet;

use serde_json::Value;

use crate::local_schema;

const UNSTRUCTURED: [&str; 7] = [
    "schema",
    "inputSchema",
    "outputSchema",
    "arguments",
    "structuredContent",
    "value",
    "data",
];

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn unique<'a>(items: impl IntoIterator<Item = Option<&'a Value>>, key: &str) -> bool {
    let mut seen = HashSet::new();

    for item in items {
        let Some(object) = item.and_then(Value::as_object) else {
            return false;
        };

        if let Some(identity) = object.get(key)
            && !seen.insert(identity.to_string())
        {
            return false;
        }
    }

    true
}

fn unique_in(params: &Value, list: &str, key: &str) -> bool {
    params
        .get(list)
        .and_then(Value::as_array)
        .is_some_and(|items| unique(items.iter().map(Some), key))
}

pub(crate) fn canonical_base64(data: &str) -> bool {
    let bytes = data.as_bytes();

    if !bytes.len().is_multiple_of(4) {
        return false;
    }

    let body = data.trim_end_matches('=');

    let padding = bytes.len() - body.len();

    let Some(digits) = body
        .bytes()
        .map(|b| BASE64.iter().position(|c| *c == b))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };

    match (padding, digits.last()) {
        (0, _) | (_, None) => padding == 0,
        (1, Some(last)) => last & 0b11 == 0,
        (2, Some(last)) => last & 0b1111 == 0,
        _ => false,
    }
}

pub(crate) fn network_origin(origin: &str) -> bool {
    let lower = origin.to_ascii_lowercase();

    let Some(rest) = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    else {
        return false;
    };

    let authority = rest.strip_suffix('/').unwrap_or(rest);

    if authority.is_empty() || authority.contains(['/', '?', '#', '\\', '@']) {
        return false;
    }

    let port = match authority.strip_prefix('[') {
        Some(literal) => match literal.split_once(']') {
            Some((_, "")) => None,
            Some((_, after)) => Some(after.strip_prefix(':').unwrap_or(after)),
            None => return false,
        },
        None => authority.split_once(':').map(|(_, port)| port),
    };

    if port.is_some_and(|p| {
        p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) || p.parse::<u16>().is_err()
    }) {
        return false;
    }

    origin
        .parse::<hyper::Uri>()
        .is_ok_and(|uri| uri.host().is_some_and(|h| !h.is_empty() && h != "[]"))
}

fn embedded(schema: &Value) -> bool {
    (schema.is_boolean() || schema.is_object()) && local_schema::compile(schema).is_ok()
}

fn content(val: &Value) -> bool {
    if let Some(items) = val.as_array() {
        return items.iter().all(content);
    }

    let Some(object) = val.as_object() else {
        return true;
    };

    let text = |key: &str| object.get(key).and_then(Value::as_str);

    if matches!(text("type"), Some("image" | "audio"))
        && text("data").is_some_and(|d| !canonical_base64(d))
    {
        return false;
    }

    if text("kind") == Some("network") && text("origin").is_some_and(|o| !network_origin(o)) {
        return false;
    }

    for key in ["inputSchema", "outputSchema"] {
        if object.get(key).is_some_and(|s| !embedded(s)) {
            return false;
        }
    }

    if text("kind") == Some("form") && !object.get("schema").is_some_and(embedded) {
        return false;
    }

    for (key, child) in object {
        if UNSTRUCTURED.contains(&key.as_str()) {
            continue;
        }

        let blob = child.get("blob").and_then(Value::as_str);

        if key == "resource" && blob.is_some_and(|b| !canonical_base64(b)) {
            return false;
        }

        if !content(child) {
            return false;
        }
    }

    true
}

fn accepted_form(params: &Value) -> bool {
    let interaction = &params["interaction"];

    let response = &params["response"];

    if interaction["kind"] != "form" || response["status"] != "accepted" {
        return true;
    }

    local_schema::compile(&interaction["schema"]).is_ok_and(|v| v.is_valid(&response["value"]))
}

/// Applies the protocol's cross-field rules that JSON Schema cannot express.
pub(crate) fn validate(method: &str, params: &Value) -> bool {
    if !params.is_object() || !content(params) {
        return false;
    }

    match method {
        "catalog.changed" => unique_in(params, "entries", "name"),
        "permission.requested" => unique_in(params, "options", "id"),
        "plan.updated" => unique_in(params, "steps", "id"),
        "configuration.changed" => unique_in(params, "changes", "key"),
        "tool.batch.finished" => params["calls"]
            .as_array()
            .is_some_and(|calls| unique(calls.iter().map(|c| c.get("call")), "id")),
        "interaction.resolved" => accepted_form(params),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use proptest::collection::vec;
    use proptest::prelude::*;

    use super::*;

    fn alphabet() -> impl Strategy<Value = String> {
        vec(
            prop::sample::select(
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=-_ ".to_vec(),
            ),
            0..16,
        )
        .prop_map(|bytes| bytes.into_iter().map(char::from).collect())
    }

    proptest! {
        #[test]
        fn encoded_bytes_are_canonical(bytes in vec(any::<u8>(), 0..64)) {
            prop_assert!(canonical_base64(&STANDARD.encode(&bytes)));
        }

        #[test]
        fn canonical_base64_agrees_with_a_strict_decoder(data in alphabet()) {
            let oracle = STANDARD.decode(&data).is_ok_and(|bytes| STANDARD.encode(bytes) == data);

            prop_assert_eq!(canonical_base64(&data), oracle, "{:?}", data);
        }

        #[test]
        fn set_padding_bits_are_rejected(bytes in vec(any::<u8>(), 1..64)) {
            let encoded = STANDARD.encode(&bytes);

            prop_assume!(encoded.ends_with('='));

            let body = encoded.trim_end_matches('=');

            let last = body.bytes().last().and_then(|b| BASE64.iter().position(|c| *c == b)).expect("digit");

            let mut tampered = body[..body.len() - 1].to_owned();

            tampered.push(char::from(BASE64[last ^ 1]));
            tampered.push_str(&encoded[body.len()..]);

            prop_assert!(!canonical_base64(&tampered), "{}", tampered);
        }

        #[test]
        fn network_origin_never_panics(origin in any::<String>(), tail in any::<String>()) {
            let _ = network_origin(&origin);
            let _ = network_origin(&format!("https://{tail}"));
            let _ = network_origin(&format!("http://[{tail}"));
        }
    }

    #[test]
    fn base64_must_be_canonical() {
        for data in ["", "AA==", "AAA=", "AAAA", "+/+/"] {
            assert!(canonical_base64(data), "{data}");
        }

        for data in ["AB==", "AAB=", "A", "AA=", "AA=A", "A===", "AA-_", "AA\n=="] {
            assert!(!canonical_base64(data), "{data}");
        }
    }

    #[test]
    fn network_origins_have_only_a_scheme_host_and_port() {
        for origin in [
            "https://example.com",
            "https://example.com/",
            "http://example.com:8080/",
            "HTTPS://Example.com",
            "http://[::1]:80",
            "http://127.0.0.1",
        ] {
            assert!(network_origin(origin), "{origin}");
        }

        for origin in [
            "https://example.com/a/..",
            "https://example.com/%2e",
            "https:example.com",
            "https://@example.com",
            "https://user@example.com",
            "https://example.com/?",
            "https://example.com/#",
            "https://example.com:99999",
            "https://example.com:",
            "https://exa mple.com",
            "ftp://example.com",
            "https://",
            "http://[::1",
        ] {
            assert!(!network_origin(origin), "{origin}");
        }
    }
}
