use std::path::Path;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::emitter::generate;
use crate::{Failure, run_with_input};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Write,
    Check,
}

pub(crate) fn run(root: &Path, mode: Mode) -> Result<(), Failure> {
    let schema_path = root.join("crates/aclp/schema.json");

    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("protocol-source.json"))?)?;

    let source = std::fs::read(&schema_path)?;

    let digest: String = Sha256::digest(&source)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    if manifest.get("schemaSha256").and_then(Value::as_str) != Some(digest.as_str()) {
        return Err("pinned protocol schema digest does not match protocol-source.json".into());
    }

    let schema: Value = serde_json::from_slice(&source)?;

    let code = generate(&schema)?;

    let formatted = run_with_input("rustfmt", &["--edition", "2024"], &code)?;

    let dst = root.join("crates/aclp/src/events.rs");

    if mode == Mode::Write {
        std::fs::write(dst, formatted)?;
        return Ok(());
    }

    if std::fs::read_to_string(dst)? != formatted {
        return Err("generated event types are stale; run cargo xtask generate".into());
    }

    Ok(())
}
