use std::path::Path;
use std::process::Command;

use crate::Failure;

const STEPS: [&[&str]; 8] = [
    &["fmt", "--all", "--check"],
    &[
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--locked",
        "--",
        "-D",
        "warnings",
    ],
    &[
        "clippy",
        "--package",
        "aclp",
        "--no-default-features",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ],
    &["doc", "--workspace", "--no-deps", "--locked"],
    &["xtask", "generate", "--check"],
    &["xtask", "guard"],
    &["test", "--workspace", "--locked"],
    &["deny", "--locked", "check"],
];

#[expect(clippy::print_stdout, reason = "the gate echoes each step it runs")]
pub(crate) fn run(root: &Path) -> Result<(), Failure> {
    for step in STEPS {
        println!("cargo {}", step.join(" "));

        let status = Command::new(env!("CARGO"))
            .args(step)
            .current_dir(root)
            .env("RUSTDOCFLAGS", "-D warnings")
            .status()?;

        if !status.success() {
            return Err(format!("cargo {} failed", step.join(" ")).into());
        }
    }

    Ok(())
}
