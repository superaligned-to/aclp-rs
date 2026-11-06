//! Repository tasks: protocol type generation, structural guards and the full gate.

mod emitter;
mod gate;
mod generate;
mod guard;
mod interop;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

type Failure = Box<dyn std::error::Error>;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_with_input(program: &str, args: &[&str], input: &str) -> Result<String, Failure> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    child
        .stdin
        .take()
        .ok_or("child process has no stdin")?
        .write_all(input.as_bytes())?;

    let output = child.wait_with_output()?;

    if !output.status.success() {
        return Err(format!("{program} failed with {}", output.status).into());
    }

    Ok(String::from_utf8(output.stdout)?)
}

#[expect(
    clippy::print_stderr,
    reason = "the task runner reports failures on stderr"
)]
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let root = root();

    let res = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["generate"] => generate::run(&root, generate::Mode::Write),
        ["generate", "--check"] => generate::run(&root, generate::Mode::Check),
        ["guard"] => guard::run(&root),
        ["check"] => gate::run(&root),
        ["interop", sdk] => interop::run(&root, sdk),
        _ => Err(
            "usage: cargo xtask <generate [--check] | guard | check | interop <aclp-ts checkout>>"
                .into(),
        ),
    };

    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
