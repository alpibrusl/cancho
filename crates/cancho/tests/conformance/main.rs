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

mod conn_peer;

mod http;
mod http_fetch_nb;
mod json;

mod selfhost;
mod tls_support;
// The TLS files moved to `tests/commands` (G4, wave 4); `http_fetch_nb` still names these three modules.
use tls_support::{https_hello, tls_echo, tls_server};

mod wasm_target;

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

/// Every `.cho` file under `dir`, recursively, skipping `.cancho-vcs` -- content-addressed package-store blobs, copies by design. It was in `duplication.rs`.
pub(crate) fn walk_ls_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries {
        let path = entry.expect("a readable directory entry").path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some(".cancho-vcs") {
                continue;
            }
            walk_ls_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "cho") {
            out.push(path);
        }
    }
}
