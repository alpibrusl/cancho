//! The M0 conformance harness.
//!
//! Every fixture under `tests/accept/` must compile, run, and produce the
//! output its header declares. Every fixture under `tests/reject/` must be
//! refused, with the message its header declares.
//!
//! The header syntax is a comment the compiler ignores:
//!
//! ```text
//! //~ STDIN <a line fed to the program>          (default: nothing)
//! //~ STDOUT <a line the program must print>
//! //~ STDERR <a line it must print as a diagnostic>  (default: none)
//! //~ EXIT <the status it must exit with>      (default 0)
//! //~ ERROR <a substring the refusal must contain>
//! //~ RULE <the rule tag the refusal must carry>
//! ```
//!
//! `STDIN` arrived with `docs/standard-input.md`: a fixture that reads
//! input needs input to be tested with, and every runner here fed a
//! program nothing. It is read from the same header as the rest, so the
//! accept walker and the example walker got it at the same moment.
//!
//! `STDERR` arrived with `docs/standard-error.md` and is checked even
//! when a fixture declares none, which is the half that matters: a
//! program writing to the wrong stream now fails a test rather than
//! passing one quietly, and that is what the two directives being
//! separate is *for* (§1.1).
//!
//! Adding a rule to the language means adding a fixture here. M2 says every
//! rule needs a must-reject fixture (#1, #2); the discipline starts at M0, when
//! it is cheap.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod benchmarks;
mod binary32;

mod compile_time;
mod conn_peer;
mod corpus;
mod crypto_builtins;
mod devices;

mod duplication;
mod f32_text;
mod float_text;
mod floats;
mod formatting;
mod global_state;
mod http;
mod http_fetch_nb;
mod https_hello;
mod json;
mod mathfn;

mod selfhost;
mod sha256_builtins;
mod sockets;

mod tls;
mod tls_auth;
mod tls_client_auth;
mod tls_echo;
mod tls_echo_fixed;
mod tls_fuzz;
mod tls_pool;
mod tls_server;
mod vcs;
mod vcs_remote;
mod vcs_std;
mod vcs_sync;
mod wasm_target;
mod wide_multiply;

const BIN: &str = env!("CARGO_BIN_EXE_cancho");

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the workspace root is two levels above this crate")
}

/// A loopback port the operating system says is free right now -- shared
/// by `net.rs`, which binds a real socket to a
/// port CI cannot be trusted to leave idle at a hard-coded number.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("a free loopback port")
        .local_addr()
        .expect("a bound address")
        .port()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cancho-conformance-{tag}"));
    make_removable(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a writable temporary directory");
    dir
}

/// A test that made a directory with mode 0 (`directory_mkdir`) and stopped before it said so leaves one that
/// `remove_dir_all` cannot read, so the next run found it there and its own `mkdir` of that name answered
/// `EEXIST`. Give every directory under `dir` back its owner's permissions first.
fn make_removable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::symlink_metadata(dir) else { return };
    if !meta.is_dir() {
        return;
    }
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            make_removable(&entry.path());
        }
    }
}

fn fixtures(kind: &str) -> Vec<PathBuf> {
    let dir = repo_root().join("tests").join(kind);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read `{}`: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "cho"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no fixtures in `{}`", dir.display());
    paths
}

/// Read `cancho authority --output json` for one program.
fn authority_json(source: &str, tag: &str) -> String {
    let dir = scratch(tag);
    let path = dir.join("program.cho");
    std::fs::write(&path, source).expect("a writable fixture");
    let out = Command::new(BIN)
        .args([
            "authority".as_ref(),
            "--std".as_ref(),
            "--output".as_ref(),
            "json".as_ref(),
            path.as_os_str(),
        ])
        .output()
        .expect("the compiler runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let _ = std::fs::remove_dir_all(&dir);
    text
}
