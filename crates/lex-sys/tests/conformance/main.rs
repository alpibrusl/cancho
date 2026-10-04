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

mod aead;
mod agent_cli;
mod agent_tools;
mod api;
mod arguments;
mod authority;
mod backends;
mod benchmarks;
mod checked_output;
mod compile_time;
mod corpus;
mod differential;
mod directory_handles;
mod directory_listing;
mod directory_writes;
mod docs;
mod duplication;
mod ecdh;
mod ecdsa;
mod file_writes;
mod filesystem;
mod floats;
mod foreign_authority;
mod formatting;
mod gcm;
mod http;
mod http_server;
mod identity;
mod io;
mod json;
mod kdf;
mod mathfn;
mod memory;
mod modules;
mod net;
mod ports;
mod project;
mod refusals;
mod release;
mod rsa;
mod signals;
mod sockets;
mod testing;
mod tls;
mod traps;
mod vcs;
mod vcs_remote;
mod vcs_std;
mod x25519;
mod x509;
mod x509_verify;
mod zeroed_slices;

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

/// Fetch several locked dependencies fresh, one `vcs fetch` per
/// `(lock, store)` pair, all into the *same* scratch directory, and hand
/// back every distinct file written across all of them -- composed with
/// no new tooling, the same way `examples/fetch/fetch.ls` composes two
/// real packages (`net.sockets` and `net.connect`) at its own command
/// line. Every test that builds or authority-checks a program that
/// imports a fetched package calls this first, re-verifying every pin
/// the same way `vcs resolve` always does rather than trusting a
/// checked-in copy.
///
/// One shared directory, not one per pair: `vcs fetch` writes each file
/// as `<source_hash>.ls`, so two pairs whose closures overlap (`net.
/// connect` and `http.response` now both transitively require `net.
/// sockets`, `docs/package-system.md` §6) write the *same* path twice
/// with identical content rather than the same content at two different
/// paths -- the latter is what `build`/`check` refuse as a duplicate
/// declaration.
fn fetch_net_dependencies(tag: &str, pairs: &[(&str, &str)]) -> Vec<PathBuf> {
    let dir = scratch(&format!("{tag}-fetch"));
    for (lock, store) in pairs {
        let fetch = Command::new(BIN)
            .args([
                "vcs".as_ref(),
                "fetch".as_ref(),
                "--lock".as_ref(),
                repo_root().join(lock).as_os_str(),
                "--store".as_ref(),
                repo_root().join(store).as_os_str(),
                "-o".as_ref(),
                dir.as_os_str(),
            ])
            .output()
            .expect("the compiler runs");
        assert!(fetch.status.success(), "{}", String::from_utf8_lossy(&fetch.stderr));
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("fetch wrote its output directory") {
        let path = entry.expect("a readable directory entry").path();
        if path.extension().is_some_and(|e| e == "ls") {
            files.push(path);
        }
    }
    files
}

/// `net.sockets` (`packages/net-sockets/`), this repository's first real
/// `lex-sys-vcs` package (`docs/package-system.md` §6), fetched fresh
/// and handed back as the one file `vcs fetch` wrote. Every test that
/// builds or authority-checks `examples/serve/serve.ls` or
/// `examples/results_stub/results_stub.ls` -- both of which `import
/// net.sockets` rather than duplicating its declarations -- calls this.
fn fetch_net_sockets(tag: &str, lock_relative: &str) -> PathBuf {
    let mut files =
        fetch_net_dependencies(tag, &[(lock_relative, "packages/net-sockets/.lex-sys-vcs")]);
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

/// `examples/fetch/fetch.ls`: `net.connect` (extended with
/// `octets_of`/`port_of`/`connect_to`) and `http.response`
/// (`send_all`/`status_of`, the fifth real package and the
/// client-side mirror of `http.request`, `docs/package-system.md` §6)
/// -- both now transitively require `net.sockets` too, so no separate
/// `net.sockets` lock is needed, the same reason `build_collect` below
/// dropped its own.
fn fetch_dependencies(tag: &str) -> Vec<PathBuf> {
    fetch_net_dependencies(
        tag,
        &[
            ("examples/fetch/connect.lock", "packages/net-connect/.lex-sys-vcs"),
            ("examples/fetch/response.lock", "packages/http-response/.lex-sys-vcs"),
        ],
    )
}

fn build_fetch(tag: &str) -> (PathBuf, PathBuf) {
    let mut paths = vec![repo_root().join("examples/fetch/fetch.ls")];
    paths.extend(fetch_dependencies(&format!("{tag}-fetch")));
    build_example_paths(tag, &paths, "fetch")
}

fn authority_of_fetch(tag: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut paths = vec![repo_root().join("examples/fetch/fetch.ls")];
    paths.extend(fetch_dependencies(&format!("{tag}-fetch")));
    authority_of_paths(&paths)
}

/// `examples/report/report.ls`: outbound, the same shape as `fetch.ls`
/// (`net.connect` and `http.response`).
fn fetch_report_dependencies(tag: &str) -> Vec<PathBuf> {
    fetch_net_dependencies(
        tag,
        &[
            ("examples/report/connect.lock", "packages/net-connect/.lex-sys-vcs"),
            ("examples/report/response.lock", "packages/http-response/.lex-sys-vcs"),
        ],
    )
}

fn build_report(tag: &str) -> (PathBuf, PathBuf) {
    let mut paths = vec![repo_root().join("examples/report/report.ls")];
    paths.extend(fetch_report_dependencies(&format!("{tag}-fetch")));
    build_example_paths(tag, &paths, "report")
}

/// `examples/collect/collect.ls`: `http.request` (`packages/http-request/
/// request.ls`, `docs/package-system.md` §4.6), the fourth real package
/// and the first that itself depends on a package. One fetch is enough:
/// a store is always exactly one file (`vcs publish` takes one input),
/// so fetching any of `http.request`'s own declarations transitively
/// writes the *whole* `net.sockets` file too -- every direct
/// `sockets.*` call `collect.ls` still makes resolves from the same
/// fetched file, with no separate `net.sockets` lock needed.
fn build_collect(tag: &str) -> (PathBuf, PathBuf) {
    let fetched = fetch_net_dependencies(
        &format!("{tag}-fetch"),
        &[("examples/collect/request.lock", "packages/http-request/.lex-sys-vcs")],
    );
    let mut paths = vec![repo_root().join("examples/collect/collect.ls")];
    paths.extend(fetched);
    build_example_paths(tag, &paths, "collect")
}

/// `examples/vsock/vsock.ls`: outbound, `net.sockets`/`net.connect` the
/// same shape as `fetch.ls`/`report.ls`, plus `agent.wire`
/// (`packages/agent-wire/wire.ls`, `docs/package-system.md` §6) for the
/// `AgentViewMsg` decoder it shares with `agent_guest.ls`. No dedicated
/// `build_vsock`: unlike the other four migrated files, nothing here
/// spawns `vsock` as a running process (`docs/vsock.md`'s own note that
/// a real `AF_VSOCK` round trip stays untested in this sandbox) -- only
/// the LLVM-backend build below needs its fetched dependencies.
fn fetch_vsock_dependencies(tag: &str) -> Vec<PathBuf> {
    fetch_net_dependencies(
        tag,
        &[
            ("examples/vsock/net.lock", "packages/net-sockets/.lex-sys-vcs"),
            ("examples/vsock/connect.lock", "packages/net-connect/.lex-sys-vcs"),
            ("examples/vsock/wire.lock", "packages/agent-wire/.lex-sys-vcs"),
        ],
    )
}

/// `examples/agent_guest/agent_guest.ls`: outbound, `net.connect` and
/// `http.response` the same shape as `fetch.ls`/`report.ls`, plus
/// `agent.wire` for the same decoder `vsock.ls`'s own header once
/// copied by hand.
fn fetch_agent_guest_dependencies(tag: &str) -> Vec<PathBuf> {
    fetch_net_dependencies(
        tag,
        &[
            ("examples/agent_guest/connect.lock", "packages/net-connect/.lex-sys-vcs"),
            ("examples/agent_guest/response.lock", "packages/http-response/.lex-sys-vcs"),
            ("examples/agent_guest/wire.lock", "packages/agent-wire/.lex-sys-vcs"),
        ],
    )
}

fn build_agent_guest(tag: &str) -> (PathBuf, PathBuf) {
    let mut paths = vec![repo_root().join("examples/agent_guest/agent_guest.ls")];
    paths.extend(fetch_agent_guest_dependencies(&format!("{tag}-fetch")));
    build_example_paths(tag, &paths, "agent_guest")
}

/// `examples/agent_supervisor/agent_supervisor.ls`: `http.request`, the
/// same one-fetch shape `build_collect` uses and for the same reason.
fn build_agent_supervisor(tag: &str) -> (PathBuf, PathBuf) {
    let fetched = fetch_net_dependencies(
        &format!("{tag}-fetch"),
        &[("examples/agent_supervisor/request.lock", "packages/http-request/.lex-sys-vcs")],
    );
    let mut paths = vec![repo_root().join("examples/agent_supervisor/agent_supervisor.ls")];
    paths.extend(fetched);
    build_example_paths(tag, &paths, "agent_supervisor")
}
