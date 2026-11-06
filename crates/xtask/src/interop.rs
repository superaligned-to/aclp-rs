use std::path::Path;
use std::process::Command;

use crate::Failure;

pub(crate) fn run(root: &Path, sdk: &str) -> Result<(), Failure> {
    let sdk = Path::new(sdk).canonicalize()?;

    if !sdk.join("dist/index.js").is_file() {
        return Err(format!(
            "{} has no dist/index.js; run pnpm build there",
            sdk.display()
        )
        .into());
    }

    let status = Command::new(env!("CARGO"))
        .args([
            "test",
            "-p",
            "aclp",
            "--test",
            "interop",
            "--locked",
            "--",
            "--ignored",
        ])
        .current_dir(root)
        .env("ACLP_TS", sdk)
        .status()?;

    if !status.success() {
        return Err("interop tests failed".into());
    }

    Ok(())
}
