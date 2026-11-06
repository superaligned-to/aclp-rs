use std::cell::Cell;
use std::fmt;

use serde::de::{DeserializeSeed, Deserializer, Error as _, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

pub(crate) const MAX_BODY_BYTES: usize = 1_048_576;

// The root has depth zero; each nested value adds one level.
pub(crate) const MAX_JSON_DEPTH: usize = 64;

// Count containers and scalar values once each; object keys do not count.
pub(crate) const MAX_JSON_NODES: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum JsonError {
    #[error("JSON body exceeds 1 MiB")]
    TooLarge,
    #[error("JSON complexity limit exceeded")]
    TooComplex,
    #[error("invalid JSON: {0}")]
    Syntax(String),
}

#[derive(Default)]
struct Budget {
    nodes: Cell<usize>,
    exceeded: Cell<bool>,
}

struct Checked<'a> {
    depth: usize,
    budget: &'a Budget,
}

impl Checked<'_> {
    fn child(&self) -> Self {
        Checked {
            depth: self.depth + 1,
            budget: self.budget,
        }
    }
}

impl<'de> DeserializeSeed<'de> for Checked<'_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        let nodes = self.budget.nodes.get() + 1;

        self.budget.nodes.set(nodes);

        if self.depth > MAX_JSON_DEPTH || nodes > MAX_JSON_NODES {
            self.budget.exceeded.set(true);
            return Err(D::Error::custom(JsonError::TooComplex));
        }

        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Checked<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();

        while let Some(item) = seq.next_element_seed(self.child())? {
            items.push(item);
        }

        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut object = Map::new();

        while let Some(key) = map.next_key::<String>()? {
            if object.contains_key(&key) {
                return Err(A::Error::custom("duplicate JSON object key"));
            }

            let val = map.next_value_seed(self.child())?;

            object.insert(key, val);
        }

        Ok(Value::Object(object))
    }
}

/// Decodes one strict JSON value: UTF-8, unique keys, finite numbers and bounded size and nesting.
pub(crate) fn decode(bytes: &[u8]) -> Result<Value, JsonError> {
    if bytes.len() > MAX_BODY_BYTES {
        return Err(JsonError::TooLarge);
    }

    let budget = Budget::default();

    let mut de = serde_json::Deserializer::from_slice(bytes);

    let val = Checked {
        depth: 0,
        budget: &budget,
    }
    .deserialize(&mut de)
    .map_err(|err| {
        if budget.exceeded.get() {
            JsonError::TooComplex
        } else {
            JsonError::Syntax(err.to_string())
        }
    })?;

    de.end().map_err(|err| JsonError::Syntax(err.to_string()))?;

    Ok(val)
}

fn check(val: &Value, depth: usize, nodes: &mut usize) -> Result<(), JsonError> {
    *nodes += 1;

    if depth > MAX_JSON_DEPTH || *nodes > MAX_JSON_NODES {
        return Err(JsonError::TooComplex);
    }

    match val {
        Value::Array(items) => items.iter().try_for_each(|v| check(v, depth + 1, nodes)),
        Value::Object(object) => object.values().try_for_each(|v| check(v, depth + 1, nodes)),
        _ => Ok(()),
    }
}

/// Checks that a value fits the decoder's bounds, so any peer can read it back.
pub(crate) fn check_bounds(val: &Value) -> Result<(), JsonError> {
    check(val, 0, &mut 0)
}

pub(crate) fn encode(val: &Value) -> Result<Vec<u8>, JsonError> {
    check_bounds(val)?;

    let bytes = serde_json::to_vec(val).map_err(|err| JsonError::Syntax(err.to_string()))?;

    if bytes.len() > MAX_BODY_BYTES {
        return Err(JsonError::TooLarge);
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests;
