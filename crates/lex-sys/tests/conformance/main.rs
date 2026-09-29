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

mod agent_tools;
mod arguments;
mod authority;
mod backends;
mod benchmarks;
mod compile_time;
mod corpus;
mod differential;
mod docs;
mod filesystem;
mod floats;
mod identity;
mod io;
mod memory;
mod modules;
mod net;
mod ports;
mod refusals;
mod traps;
mod vcs;

const BIN: &str = env!("CARGO_BIN_EXE_lex-sys");

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the workspace root is two levels above this crate")
}

/// A loopback port the operating system says is free right now -- shared
/// by `net.rs` and `backends.rs`, both of which bind a real socket to a
/// port CI cannot be trusted to leave idle at a hard-coded number.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("a free loopback port")
        .local_addr()
        .expect("a bound address")
        .port()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lex-sys-conformance-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a writable temporary directory");
    dir
}

fn fixtures(kind: &str) -> Vec<PathBuf> {
    let dir = repo_root().join("tests").join(kind);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read `{}`: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "ls"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no fixtures in `{}`", dir.display());
    paths
}

/// Read `lex-sys authority --output json` for one program.
fn authority_json(source: &str, tag: &str) -> String {
    let dir = scratch(tag);
    let path = dir.join("program.ls");
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

/// Every result printed as an `int`, so one line format covers all three
/// result types -- and a float as its **bits**, so `-0.0` and `0.0`, and
/// two NaNs, cannot compare equal by accident.
fn shown(ret: &str, e: &str) -> String {
    match ret {
        "int" => e.to_owned(),
        "bool" => format!("b2i({e})"),
        _ => format!("bits_of({e})"),
    }
}

/// Read `authority --output json` for a program, as parsed fields.
///
/// Returns `(effect names, foreign symbols, labels as name=argument)`.
fn authority_of(relative: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    authority_of_paths(&[repo_root().join(relative)])
}

/// `authority_of`, over more than one file -- what a program that
/// `import`s a fetched package needs, since `net.sockets`
/// (`packages/net-sockets/`, `docs/package-system.md` §6) is not on the
/// command line by itself.
fn authority_of_paths(paths: &[PathBuf]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut command = Command::new(BIN);
    command.arg("authority");
    for path in paths {
        command.arg(path);
    }
    command.args(["--std", "--output", "json"]);
    let out = command.output().expect("the compiler runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).expect("the report is utf-8");

    // A small reader rather than a JSON dependency: this crate has none,
    // and the three fields below are flat arrays of strings and objects.
    fn array(text: &str, key: &str) -> Vec<String> {
        let Some(at) = text.find(&format!("\"{key}\"")) else { return Vec::new() };
        let rest = &text[at..];
        let Some(open) = rest.find('[') else { return Vec::new() };
        let Some(close) = rest[open..].find(']') else { return Vec::new() };
        let body = &rest[open + 1..open + close];
        body.split(',')
            .filter_map(|piece| {
                let piece = piece.trim();
                let start = piece.find('"')?;
                let end = piece[start + 1..].find('"')? + start + 1;
                Some(piece[start + 1..end].to_owned())
            })
            .collect()
    }

    // `labels` is objects, so read the pairs out of the same slice.
    let mut labels = Vec::new();
    if let Some(at) = text.find("\"labels\"") {
        let rest = &text[at..];
        if let (Some(open), Some(close)) = (rest.find('['), rest.find(']')) {
            for row in rest[open + 1..close].split('}') {
                let Some(name_at) = row.find("\"name\":") else { continue };
                let name: String = row[name_at + 7..]
                    .trim_start()
                    .trim_start_matches('"')
                    .chars()
                    .take_while(|c| *c != '"')
                    .collect();
                let argument = row.find("\"argument\":").map(|a| {
                    let tail = row[a + 11..].trim_start();
                    if tail.starts_with("null") {
                        String::new()
                    } else {
                        tail.trim_start_matches('"').chars().take_while(|c| *c != '"').collect()
                    }
                });
                match argument.as_deref() {
                    Some("") | None => labels.push(name),
                    Some(value) => labels.push(format!("{name}={value}")),
                }
            }
        }
    }

    (array(&text, "effects"), array(&text, "foreign_symbols"), labels)
}

fn build_example(tag: &str, relative: &str, binary: &str) -> (PathBuf, PathBuf) {
    build_example_paths(tag, &[repo_root().join(relative)], binary)
}

/// `build_example`, over more than one file -- what a program that
/// `import`s a fetched package needs.
fn build_example_paths(tag: &str, paths: &[PathBuf], binary: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(tag);
    let exe = dir.join(binary);
    let mut command = Command::new(BIN);
    command.arg("build");
    for path in paths {
        command.arg(path);
    }
    command.args(["--std".as_ref(), "-o".as_ref(), exe.as_os_str()]);
    let build = command.output().expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

/// `net.sockets` (`packages/net-sockets/`), this repository's first real
/// `lex-sys-vcs` package (`docs/package-system.md` §6), fetched fresh
/// into its own scratch directory and handed back as the one file `vcs
/// fetch` wrote there. Every test that builds or authority-checks
/// `examples/serve/serve.ls` or `examples/results_stub/results_stub.ls`
/// -- both of which `import net.sockets` rather than duplicating its
/// declarations -- calls this first, re-verifying the pin the same way
/// `vcs resolve` always does rather than trusting a checked-in copy.
fn fetch_net_sockets(tag: &str, lock_relative: &str) -> PathBuf {
    let dir = scratch(tag);
    let fetch = Command::new(BIN)
        .args([
            "vcs".as_ref(),
            "fetch".as_ref(),
            "--lock".as_ref(),
            repo_root().join(lock_relative).as_os_str(),
            "--store".as_ref(),
            repo_root().join("packages/net-sockets/.lex-sys-vcs").as_os_str(),
            "-o".as_ref(),
            dir.as_os_str(),
        ])
        .output()
        .expect("the compiler runs");
    assert!(fetch.status.success(), "{}", String::from_utf8_lossy(&fetch.stderr));
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fetch wrote its output directory")
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "ls"))
        .collect();
    assert_eq!(files.len(), 1, "`net.sockets` should fetch to exactly one file, found {files:?}");
    files.remove(0)
}

fn build_serve(tag: &str) -> (PathBuf, PathBuf) {
    let fetched = fetch_net_sockets(&format!("{tag}-fetch"), "examples/serve/net.lock");
    build_example_paths(tag, &[repo_root().join("examples/serve/serve.ls"), fetched], "serve")
}

fn authority_of_serve(tag: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let fetched = fetch_net_sockets(&format!("{tag}-fetch"), "examples/serve/net.lock");
    authority_of_paths(&[repo_root().join("examples/serve/serve.ls"), fetched])
}
