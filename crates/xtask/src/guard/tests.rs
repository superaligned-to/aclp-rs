use std::collections::BTreeSet;

use super::*;

fn tree() -> Vec<SourceFile> {
    REQUIRED
        .iter()
        .map(|path| SourceFile {
            path: (*path).to_owned(),
            content: String::new(),
        })
        .collect()
}

fn names(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn allowed() -> BTreeSet<String> {
    RUNTIME_DEPENDENCIES
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect()
}

fn module(lines: usize) -> SourceFile {
    SourceFile {
        path: "crates/aclp/src/big.rs".to_owned(),
        content: "x\n\n".repeat(lines),
    }
}

#[test]
fn ceilings_keep_their_documented_values() {
    assert_eq!(MAX_MODULE_LINES, 400);
    assert_eq!(MAX_RUNTIME_DEPENDENCIES, 12);
}

#[test]
fn complete_tree_with_reviewed_dependencies_passes() {
    let errors = check_structure(&tree(), &allowed(), &names(&BUILD_SCRIPTS));

    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn missing_boundary_fails() {
    let mut files = tree();

    files.retain(|f| f.path != "crates/aclp/src/server.rs");

    let errors = check_structure(&files, &allowed(), &BTreeSet::new());

    assert_eq!(
        errors,
        ["required SDK boundary is missing: crates/aclp/src/server.rs"]
    );
}

#[test]
fn unreviewed_or_excess_dependencies_fail() {
    let mut deps = allowed();

    deps.insert("left-pad".to_owned());

    let errors = check_structure(&tree(), &deps, &BTreeSet::new());

    assert_eq!(
        errors,
        [
            "runtime dependencies exceed 12",
            "runtime dependency has no recorded reason: left-pad",
        ]
    );
}

#[test]
fn unreviewed_build_script_fails() {
    let errors = check_structure(&tree(), &allowed(), &names(&["openssl-sys"]));

    assert_eq!(
        errors,
        ["dependency build script is not reviewed: openssl-sys"]
    );
}

#[test]
fn module_ceiling_counts_only_non_blank_lines() {
    let mut files = tree();

    files.push(module(MAX_MODULE_LINES));

    assert!(check_structure(&files, &allowed(), &BTreeSet::new()).is_empty());

    files.push(SourceFile {
        path: "crates/aclp/src/over.rs".to_owned(),
        ..module(MAX_MODULE_LINES + 1)
    });

    assert_eq!(
        check_structure(&files, &allowed(), &BTreeSet::new()),
        ["crates/aclp/src/over.rs: 401 non-blank lines exceed 400"]
    );
}

#[test]
fn generated_module_is_exempt_from_the_ceiling() {
    let mut files = tree();

    files.retain(|f| f.path != GENERATED);
    files.push(SourceFile {
        path: GENERATED.to_owned(),
        ..module(MAX_MODULE_LINES + 1)
    });

    assert!(check_structure(&files, &allowed(), &BTreeSet::new()).is_empty());
}
