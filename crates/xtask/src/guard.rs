use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::Failure;

pub(crate) const MAX_MODULE_LINES: usize = 400;

// Every entry states the need it serves; adding a runtime dependency means adding a reason here.
pub(crate) const RUNTIME_DEPENDENCIES: [(&str, &str); 15] = [
    (
        "http-body-util",
        "bounded request and response body collection",
    ),
    (
        "hyper",
        "HTTP/1.1 client and server with header read limits",
    ),
    ("hyper-util", "Tokio I/O and timer adapters for hyper"),
    (
        "jsonschema",
        "Draft-07 validation with format checks and offline references",
    ),
    (
        "rustls",
        "HTTPS for remote endpoints, behind the default tls feature",
    ),
    ("serde", "typed event parameters and results"),
    ("serde_json", "JSON values with preserved member order"),
    ("sha2", "length-hiding credential comparison"),
    ("thiserror", "library error types"),
    ("tokio", "async sockets, deadlines and capacity semaphores"),
    ("tokio-rustls", "async TLS streams for the HTTPS client"),
    (
        "tokio-util",
        "callback task tracking and shutdown cancellation",
    ),
    ("tracing", "default diagnostics sink"),
    ("uuid", "fresh random request ids"),
    (
        "webpki-roots",
        "bundled web PKI roots, so HTTPS works without system certificate access",
    ),
];

pub(crate) const MAX_RUNTIME_DEPENDENCIES: usize = RUNTIME_DEPENDENCIES.len();

// Build scripts run arbitrary code at compile time; each crate in the runtime graph that has one is reviewed here.
pub(crate) const BUILD_SCRIPTS: [&str; 30] = [
    "ahash",
    "getrandom",
    "httparse",
    "jsonschema-value",
    "libc",
    "num-traits",
    "parking_lot_core",
    "proc-macro2",
    "quote",
    "ref-cast",
    "ring",
    "rustls",
    "serde",
    "serde_core",
    "serde_json",
    "thiserror",
    "unicode-general-category",
    "wasm-bindgen",
    "wasm-bindgen-shared",
    "windows_aarch64_gnullvm",
    "windows_aarch64_msvc",
    "windows_i686_gnu",
    "windows_i686_gnullvm",
    "windows_i686_msvc",
    "windows_x86_64_gnu",
    "windows_x86_64_gnullvm",
    "windows_x86_64_msvc",
    "wit-bindgen",
    "zerocopy",
    "zmij",
];

pub(crate) const REQUIRED: [&str; 7] = [
    "protocol-source.json",
    "crates/aclp/schema.json",
    "crates/aclp/src/lib.rs",
    "crates/aclp/src/client.rs",
    "crates/aclp/src/server.rs",
    "crates/aclp/src/events.rs",
    "crates/aclp/tests/fixtures/event-examples.json",
];

pub(crate) const GENERATED: &str = "crates/aclp/src/events.rs";

#[derive(Debug)]
pub(crate) struct SourceFile {
    pub(crate) path: String,
    pub(crate) content: String,
}

pub(crate) fn check_structure(
    files: &[SourceFile],
    dependencies: &BTreeSet<String>,
    build_scripts: &BTreeSet<String>,
) -> Vec<String> {
    let mut errors = Vec::new();

    let names: BTreeSet<&str> = files.iter().map(|f| f.path.as_str()).collect();

    for path in REQUIRED {
        if !names.contains(path) {
            errors.push(format!("required SDK boundary is missing: {path}"));
        }
    }

    if dependencies.len() > MAX_RUNTIME_DEPENDENCIES {
        errors.push(format!(
            "runtime dependencies exceed {MAX_RUNTIME_DEPENDENCIES}"
        ));
    }

    for dep in dependencies {
        if !RUNTIME_DEPENDENCIES.iter().any(|(name, _)| name == dep) {
            errors.push(format!("runtime dependency has no recorded reason: {dep}"));
        }
    }

    for name in build_scripts {
        if !BUILD_SCRIPTS.contains(&name.as_str()) {
            errors.push(format!("dependency build script is not reviewed: {name}"));
        }
    }

    for file in files {
        if !file.path.ends_with(".rs") || file.path == GENERATED {
            continue;
        }

        // Count non-blank lines after rustfmt, including comments; blank lines carry grouping and never count.
        let lines = file
            .content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count();

        if lines > MAX_MODULE_LINES {
            errors.push(format!(
                "{}: {lines} non-blank lines exceed {MAX_MODULE_LINES}",
                file.path
            ));
        }
    }

    errors
}

fn source_files(root: &Path, dir: &Path, out: &mut Vec<SourceFile>) -> Result<(), Failure> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;

        let path = entry.path();

        let name = entry.file_name();

        if [".git", "target"].iter().any(|skip| name == *skip) {
            continue;
        }

        if entry.file_type()?.is_dir() {
            source_files(root, &path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs" || e == "json") {
            out.push(SourceFile {
                path: path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                content: std::fs::read_to_string(&path)?,
            });
        }
    }

    Ok(())
}

fn metadata(root: &Path) -> Result<Value, Failure> {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(root)
        .output()?;

    if !output.status.success() {
        return Err("cargo metadata failed".into());
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}

fn normal(dep: &Value) -> bool {
    dep["dep_kinds"]
        .as_array()
        .is_some_and(|kinds| kinds.iter().any(|k| k["kind"].is_null()))
}

fn runtime_graph(meta: &Value) -> Result<(BTreeSet<String>, BTreeSet<String>), Failure> {
    let packages: BTreeMap<&str, &Value> = meta["packages"]
        .as_array()
        .ok_or("metadata has no packages")?
        .iter()
        .filter_map(|p| Some((p["id"].as_str()?, p)))
        .collect();

    let nodes: BTreeMap<&str, &Value> = meta["resolve"]["nodes"]
        .as_array()
        .ok_or("metadata has no resolve graph")?
        .iter()
        .filter_map(|n| Some((n["id"].as_str()?, n)))
        .collect();

    let root = packages
        .values()
        .find(|p| p["name"] == "aclp")
        .and_then(|p| p["id"].as_str())
        .ok_or("metadata has no aclp package")?;

    let direct = nodes[root]["deps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| normal(d))
        .filter_map(|d| {
            packages.get(d["pkg"].as_str()?)?["name"]
                .as_str()
                .map(str::to_owned)
        })
        .collect();

    let mut seen = BTreeSet::new();

    let mut stack = vec![root];

    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }

        for dep in nodes[id]["deps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| normal(d))
        {
            stack.extend(dep["pkg"].as_str());
        }
    }

    let scripts = seen
        .iter()
        .filter(|id| **id != root)
        .filter_map(|id| packages.get(id))
        .filter(|p| {
            p["targets"].as_array().into_iter().flatten().any(|t| {
                t["kind"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|k| k == "custom-build")
            })
        })
        .filter_map(|p| p["name"].as_str().map(str::to_owned))
        .collect();

    Ok((direct, scripts))
}

#[expect(clippy::print_stdout, reason = "the guard confirms a passing run")]
pub(crate) fn run(root: &Path) -> Result<(), Failure> {
    let mut files = Vec::new();

    source_files(root, root, &mut files)?;

    let (dependencies, scripts) = runtime_graph(&metadata(root)?)?;

    let errors = check_structure(&files, &dependencies, &scripts);

    if !errors.is_empty() {
        return Err(errors.join("\n").into());
    }

    println!("repository structure, module ceilings and dependency checks passed");
    Ok(())
}

#[cfg(test)]
mod tests;
