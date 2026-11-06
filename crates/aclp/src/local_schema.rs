use jsonschema::{Draft, Validator};
use serde_json::Value;

use crate::json;

// Count schema containers and scalar values, including annotation data; exclude object keys.
pub(crate) const MAX_SCHEMA_NODES: usize = 2_048;

pub(crate) const MAX_SCHEMA_DEPTH: usize = 32;

const DRAFT_07: [&str; 4] = [
    "http://json-schema.org/draft-07/schema#",
    "http://json-schema.org/draft-07/schema",
    "https://json-schema.org/draft-07/schema#",
    "https://json-schema.org/draft-07/schema",
];

/// A JSON Schema that cannot be installed as a local Draft-07 schema.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    /// The schema exceeds the node or depth ceiling for installed schemas.
    #[error("JSON schema complexity limit exceeded")]
    TooComplex,
    /// The schema declares a `$schema` dialect other than Draft-07.
    #[error("JSON schema dialect {0} is not Draft-07")]
    Dialect(String),
    /// The schema is invalid or references a resource outside the document.
    #[error("invalid JSON schema: {0}")]
    Invalid(String),
}

fn complexity(val: &Value, depth: usize, nodes: &mut usize) -> Result<(), SchemaError> {
    *nodes += 1;

    if *nodes > MAX_SCHEMA_NODES || depth > MAX_SCHEMA_DEPTH {
        return Err(SchemaError::TooComplex);
    }

    match val {
        Value::Array(items) => items
            .iter()
            .try_for_each(|v| complexity(v, depth + 1, nodes)),
        Value::Object(object) => object
            .values()
            .try_for_each(|v| complexity(v, depth + 1, nodes)),
        _ => Ok(()),
    }
}

pub(crate) fn draft_07() -> jsonschema::ValidationOptions<'static> {
    jsonschema::options()
        .with_draft(Draft::Draft7)
        .should_validate_formats(true)
        .offline()
}

/// Compiles a bounded, self-contained Draft-07 schema without fetching references.
pub(crate) fn compile(schema: &Value) -> Result<Validator, SchemaError> {
    json::check_bounds(schema).map_err(|_| SchemaError::TooComplex)?;
    complexity(schema, 0, &mut 0)?;

    if let Some(dialect) = schema.get("$schema")
        && !dialect.as_str().is_some_and(|d| DRAFT_07.contains(&d))
    {
        return Err(SchemaError::Dialect(dialect.to_string()));
    }

    draft_07()
        .build(schema)
        .map_err(|err| SchemaError::Invalid(err.to_string()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn complexity_ceilings_keep_their_documented_values() {
        assert_eq!(MAX_SCHEMA_DEPTH, 32);
        assert_eq!(MAX_SCHEMA_NODES, 2_048);
    }

    #[test]
    fn schemas_over_the_node_or_depth_ceiling_are_rejected() {
        assert!(compile(&json!({ "type": "string" })).is_ok());

        let wide = json!({ "enum": (0..MAX_SCHEMA_NODES).collect::<Vec<_>>() });

        assert_eq!(compile(&wide).err(), Some(SchemaError::TooComplex));

        let mut nested = json!("value");

        for _ in 0..=MAX_SCHEMA_DEPTH {
            nested = json!([nested]);
        }

        assert_eq!(
            compile(&json!({ "const": nested })).err(),
            Some(SchemaError::TooComplex)
        );
    }

    #[test]
    fn local_references_compile_and_remote_ones_fail() {
        let validator = compile(&json!({
            "definitions": { "count": { "type": "integer" } },
            "$ref": "#/definitions/count",
        }))
        .expect("local reference compiles");

        assert!(validator.is_valid(&json!(1)));
        assert!(!validator.is_valid(&json!("1")));
        assert!(compile(&json!({ "$ref": "https://example.invalid/missing" })).is_err());
        assert!(compile(&json!({ "$ref": "#/definitions/missing" })).is_err());
    }

    #[test]
    fn only_draft_07_dialects_are_accepted() {
        for dialect in DRAFT_07 {
            assert!(compile(&json!({ "$schema": dialect })).is_ok(), "{dialect}");
        }

        let other = json!({ "$schema": "https://json-schema.org/draft/2020-12/schema" });

        assert!(matches!(compile(&other), Err(SchemaError::Dialect(_))));
    }
}
