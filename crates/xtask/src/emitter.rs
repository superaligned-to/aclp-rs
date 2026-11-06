use std::collections::BTreeSet;
use std::fmt::Write as _;

use serde_json::{Map, Value};

use crate::Failure;

const ENVELOPES: [&str; 7] = [
    "RequestEnvelope",
    "NotificationEnvelope",
    "InvocationEnvelope",
    "StandardInvocation",
    "SuccessResponse",
    "RpcError",
    "ErrorResponse",
];

const RENAMES: [(&str, &str); 6] = [
    ("DiscoveryAuthMethods", "AuthMethod"),
    ("CatalogChangedCatalog", "Catalog"),
    ("NotificationCreatedLevel", "NotificationLevel"),
    ("OperationResourceAccess", "ResourceAccess"),
    ("ResourceChangedChange", "ResourceChange"),
    ("WorkspaceChangedChange", "WorkspaceChange"),
];

const HEADER: &str = "//! Standard ACLP event parameters and discovery types generated from the pinned protocol schema.
//!
//! Regenerate with `cargo xtask generate`; the gate rejects hand edits.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Number, Value};

use crate::event::{Event, sealed::Sealed};

fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}
";

struct Generator<'a> {
    defs: &'a Map<String, Value>,
    inlined: BTreeSet<String>,
    out: String,
    pending: Vec<(String, Vec<String>)>,
    emitted: BTreeSet<String>,
}

pub(crate) fn generate(schema: &Value) -> Result<String, Failure> {
    let defs = schema
        .get("definitions")
        .and_then(Value::as_object)
        .ok_or("protocol schema has no definitions")?;

    let methods = methods(defs)?;

    let mut generator = Generator {
        defs,
        inlined: BTreeSet::new(),
        out: HEADER.to_owned(),
        pending: Vec::new(),
        emitted: BTreeSet::new(),
    };

    for def in defs.values() {
        for variant in def
            .get("oneOf")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(name) = ref_name(variant) {
                generator.inlined.insert(name.to_owned());
            }
        }
    }

    for (name, def) in defs {
        if ENVELOPES.contains(&name.as_str())
            || generator.inlined.contains(name)
            || generator.alias(def, name).is_some()
        {
            continue;
        }

        let method = methods
            .iter()
            .find(|(_, d)| d == name)
            .map(|(m, _)| m.as_str());

        generator.definition(name, def, method)?;
    }

    generator.events(&methods);

    Ok(generator.out)
}

fn methods(defs: &Map<String, Value>) -> Result<Vec<(String, String)>, Failure> {
    let branches = defs
        .get("StandardInvocation")
        .and_then(|d| d.get("allOf"))
        .and_then(|a| a.get(1))
        .and_then(|b| b.get("oneOf"))
        .and_then(Value::as_array)
        .ok_or("protocol schema has no standard dispatch")?;

    branches
        .iter()
        .map(|branch| {
            let method = branch
                .pointer("/properties/method/const")
                .and_then(Value::as_str);

            let params = branch.pointer("/properties/params").and_then(ref_name);

            match (method, params) {
                (Some(m), Some(p)) => Ok((m.to_owned(), p.to_owned())),
                _ => Err("invalid protocol method definition".into()),
            }
        })
        .collect()
}

fn ref_name(schema: &Value) -> Option<&str> {
    schema.get("$ref")?.as_str()?.strip_prefix("#/definitions/")
}

fn props(schema: &Value) -> Option<&Map<String, Value>> {
    schema.get("properties").and_then(Value::as_object)
}

fn required(schema: &Value) -> BTreeSet<&str> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

fn snake(name: &str) -> String {
    let mut out = String::new();

    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }

    out
}

fn pascal(name: &str) -> String {
    name.split(['_', '.'])
        .map(|part| {
            let mut chars = part.chars();

            chars
                .next()
                .map(|c| format!("{}{}", c.to_ascii_uppercase(), chars.as_str()))
                .unwrap_or_default()
        })
        .collect()
}

fn renamed(name: String) -> String {
    RENAMES
        .iter()
        .find(|(from, _)| *from == name)
        .map_or(name, |(_, to)| (*to).to_owned())
}

impl Generator<'_> {
    fn resolve<'s>(&'s self, schema: &'s Value) -> Result<&'s Value, Failure> {
        match ref_name(schema) {
            Some(name) => self
                .defs
                .get(name)
                .ok_or_else(|| format!("unresolved reference {name}").into()),
            None => Ok(schema),
        }
    }

    fn alias(&self, def: &Value, name: &str) -> Option<&'static str> {
        if name == "Schema" {
            return Some("Value");
        }

        match def.get("type").and_then(Value::as_str) {
            Some("string") => Some("String"),
            Some(_) => None,
            None if def.as_object().is_some_and(Map::is_empty) => Some("Value"),
            None => None,
        }
    }

    fn tag(&self, variants: &[Value]) -> Result<Option<String>, Failure> {
        let first = self.resolve(&variants[0])?;

        for key in props(first).into_iter().flat_map(Map::keys) {
            let mut tagged = true;

            for variant in variants {
                let constant = self
                    .resolve(variant)?
                    .pointer(&format!("/properties/{key}/const"));

                tagged &= constant.is_some_and(Value::is_string);
            }

            if tagged {
                return Ok(Some(key.clone()));
            }
        }

        Ok(None)
    }

    fn ty(&mut self, owner: &str, prop: &str, schema: &Value) -> Result<String, Failure> {
        if let Some(name) = ref_name(schema) {
            let def = self.resolve(schema)?;

            return Ok(self
                .alias(def, name)
                .map_or_else(|| name.to_owned(), str::to_owned));
        }

        if let Some(values) = schema.get("enum").and_then(Value::as_array) {
            let name = renamed(format!("{owner}{}", pascal(prop)));

            let values = values
                .iter()
                .map(|v| v.as_str().map(str::to_owned).ok_or("non-string enum value"))
                .collect::<Result<Vec<_>, _>>()?;

            self.pending.push((name.clone(), values));
            return Ok(name);
        }

        if schema.get("allOf").is_some() {
            return Ok("String".to_owned());
        }

        match schema.get("type").and_then(Value::as_str) {
            Some("string") => Ok("String".to_owned()),
            Some("integer") => Ok("Number".to_owned()),
            Some("array") => match schema.get("items") {
                Some(items) => Ok(format!("Vec<{}>", self.ty(owner, prop, items)?)),
                None => Ok("Vec<Value>".to_owned()),
            },
            None if props(schema).is_none() => Ok("Value".to_owned()),
            _ => Err(format!("unsupported schema at {owner}.{prop}").into()),
        }
    }

    fn fields(
        &mut self,
        owner: &str,
        schema: &Value,
        skip: &[&str],
        visibility: &str,
    ) -> Result<String, Failure> {
        let required = required(schema);

        let mut out = String::new();

        for (prop, field) in props(schema).into_iter().flatten() {
            if skip.contains(&prop.as_str()) {
                continue;
            }

            let ty = self.ty(owner, prop, field)?;

            let name = snake(prop);

            let mut attrs = Vec::new();

            if name != *prop {
                attrs.push(format!("rename = \"{prop}\""));
            }

            let ty = if required.contains(prop.as_str()) {
                ty
            } else {
                attrs.push("default".to_owned());

                if ty == "Value" {
                    attrs.push("deserialize_with = \"present\"".to_owned());
                }

                attrs.push("skip_serializing_if = \"Option::is_none\"".to_owned());
                format!("Option<{ty}>")
            };

            if !attrs.is_empty() {
                writeln!(out, "#[serde({})]", attrs.join(", "))?;
            }

            writeln!(out, "{visibility}{name}: {ty},")?;
        }

        Ok(out)
    }

    fn definition(&mut self, name: &str, def: &Value, method: Option<&str>) -> Result<(), Failure> {
        if let Some(method) = method {
            writeln!(self.out, "\n/// Parameters of the `{method}` event.")?;
        } else {
            self.out.push('\n');
        }

        let variants = def.get("oneOf").and_then(Value::as_array);

        match (props(def), variants) {
            (None, Some(variants)) => self.enumeration(name, variants)?,
            (Some(_), _) => self.structure(name, def)?,
            (None, None) => return Err(format!("unsupported definition {name}").into()),
        }

        self.flush()
    }

    fn structure(&mut self, name: &str, def: &Value) -> Result<(), Failure> {
        let owner = name.strip_suffix("Params").unwrap_or(name).to_owned();

        let variants = def
            .get("oneOf")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let tag = if variants.is_empty() {
            None
        } else {
            self.tag(&variants)?
        };

        let skip: Vec<String> = match &tag {
            Some(_) => variants
                .iter()
                .filter_map(props)
                .flat_map(|p| p.keys().cloned())
                .collect(),
            None => Vec::new(),
        };

        let skip: Vec<&str> = skip.iter().map(String::as_str).collect();

        let mut body = self.fields(&owner, def, &skip, "pub ")?;

        if let Some(tag) = &tag {
            let flattened = renamed(format!("{owner}{}", pascal(tag)));

            let parent: Vec<Value> = required(def).into_iter().map(Value::from).collect();

            let variants: Vec<Value> = variants
                .into_iter()
                .map(|mut variant| {
                    variant["required"] = Value::from(parent.clone());
                    variant
                })
                .collect();

            writeln!(body, "#[serde(flatten)]\npub {}: {flattened},", snake(tag))?;
            self.out.push_str(&format!(
                "#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub struct {name} {{\n{body}}}\n\n"
            ));
            return self.enumeration(&flattened, &variants);
        }

        writeln!(
            self.out,
            "#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub struct {name} {{\n{body}}}"
        )?;
        Ok(())
    }

    fn enumeration(&mut self, name: &str, variants: &[Value]) -> Result<(), Failure> {
        let tag = self.tag(variants)?;

        let mut body = String::new();

        for (idx, variant) in variants.iter().enumerate() {
            let variant = self.resolve(variant)?.clone();

            let (label, skip) = match &tag {
                Some(tag) => {
                    let value = variant
                        .pointer(&format!("/properties/{tag}/const"))
                        .and_then(Value::as_str)
                        .ok_or("missing tag constant")?;

                    writeln!(body, "#[serde(rename = \"{value}\")]")?;
                    (pascal(value), vec![tag.as_str()])
                }
                None => (pascal(&self.discriminant(variants, idx)?), Vec::new()),
            };

            let fields = self.fields(&format!("{name}{label}"), &variant, &skip, "")?;

            if fields.is_empty() {
                writeln!(body, "{label},")?;
            } else {
                writeln!(body, "{label} {{\n{fields}}},")?;
            }
        }

        let repr = match &tag {
            Some(tag) => format!("tag = \"{tag}\""),
            None => "untagged".to_owned(),
        };

        writeln!(
            self.out,
            "#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\n#[serde({repr})]\npub enum {name} {{\n{body}}}"
        )?;
        Ok(())
    }

    fn discriminant(&self, variants: &[Value], idx: usize) -> Result<String, Failure> {
        let own = self.resolve(&variants[idx])?;

        for key in required(own) {
            let mut shared = false;

            for (other, variant) in variants.iter().enumerate() {
                shared |= other != idx && required(self.resolve(variant)?).contains(key);
            }

            if !shared {
                return Ok(key.to_owned());
            }
        }

        Err("untagged variant has no distinguishing property".into())
    }

    fn flush(&mut self) -> Result<(), Failure> {
        for (name, values) in std::mem::take(&mut self.pending) {
            if !self.emitted.insert(name.clone()) {
                continue;
            }

            let mut body = String::new();

            for value in values {
                writeln!(body, "#[serde(rename = \"{value}\")]\n{},", pascal(&value))?;
            }

            writeln!(
                self.out,
                "\n#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]\npub enum {name} {{\n{body}}}"
            )?;
        }

        Ok(())
    }

    fn events(&mut self, methods: &[(String, String)]) {
        let mut table = String::new();

        let mut arms = String::new();

        for (method, def) in methods {
            self.out.push_str(&format!(
                "\nimpl Sealed for {def} {{}}\n\nimpl Event for {def} {{\n    const METHOD: &'static str = \"{method}\";\n}}\n"
            ));
            table.push_str(&format!("(\"{method}\", \"{def}\"),\n"));
            arms.push_str(&format!(
                "\"{method}\" => serde_json::from_value::<{def}>(params).and_then(serde_json::to_value),\n"
            ));
        }

        self.out.push_str(&format!(
            "\npub(crate) const DEFINITIONS: [(&str, &str); {}] = [\n{table}];\n",
            methods.len()
        ));
        self.out.push_str(&format!(
            "\n#[cfg(test)]\npub(crate) fn round_trip(method: &str, params: Value) -> serde_json::Result<Value> {{\n    match method {{\n{arms}        _ => Err(serde::de::Error::custom(\"unknown method\")),\n    }}\n}}\n"
        ));
    }
}
