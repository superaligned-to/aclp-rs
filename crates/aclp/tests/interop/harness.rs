use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};

pub(crate) struct Peer {
    _child: Child,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Peer {
    pub(crate) async fn spawn(args: &[&str]) -> Peer {
        let sdk = std::env::var("ACLP_TS")
            .expect("ACLP_TS names an aclp-ts checkout; use cargo xtask interop");

        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/interop/peer.mjs");

        let mut child = Command::new("node")
            .arg(script)
            .arg(args[0])
            .arg(PathBuf::from(sdk))
            .args(&args[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("node starts");

        let stdout = child.stdout.take().expect("piped stdout");

        Peer {
            _child: child,
            lines: BufReader::new(stdout).lines(),
        }
    }

    pub(crate) async fn line(&mut self) -> Value {
        let line = tokio::time::timeout(Duration::from_secs(20), self.lines.next_line())
            .await
            .expect("peer answers in time")
            .expect("peer stdout readable")
            .expect("peer still running");

        serde_json::from_str(&line).expect("peer writes JSON lines")
    }

    pub(crate) async fn port(&mut self) -> u16 {
        let port = self.line().await["port"]
            .as_u64()
            .expect("peer reports its port");

        u16::try_from(port).expect("TCP port")
    }
}
