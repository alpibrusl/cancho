//! What the conformance files that drive a program through standard input share: `build_driver`,
//! `feed` and the `Lcg` generator. Every test this file held (`std.json` against `serde_json`, the
//! parse and write drivers, the library tests) moved to cancho in G4 wave 4
//! (`tests/commands/json.sh`, `docs/gate.md`); what is left is used by `http.rs` and the `tls*.rs`
//! files, and goes with the last of them.

use super::*;
use std::io::Write;
use std::process::Stdio;

/// Build one of `tests/programs/` against the standard library.
pub(super) fn build_driver(tag: &str, program: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(tag);
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std"])
        .arg(repo_root().join("tests/programs").join(program))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

pub(super) fn feed(exe: &Path, input: &[u8]) -> std::process::Output {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the driver runs");
    // Written from a thread: a driver that answers before it has read
    // everything must not deadlock the pipe.
    let mut stdin = child.stdin.take().expect("a piped stdin");
    let bytes = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&bytes);
    });
    let out = child.wait_with_output().expect("the driver finishes");
    let _ = writer.join();
    out
}

/// A small deterministic generator, so a failure names a reproducible input.
pub(super) struct Lcg(pub(super) u64);
impl Lcg {
    pub(super) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 11
    }
    pub(super) fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}
