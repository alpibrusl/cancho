//! `--target` (`docs/wasm.md`): the parts that need no wasm toolchain.
//!
//! Building and running a `wasm32-wasip1` program needs a clang with the wasm
//! target, `wasm-ld`, a wasi-libc sysroot and wasmtime, none of which CI is
//! assumed to have. What does not need them -- that the flag is refused where
//! it means nothing, and that the module the backend emits for the target has
//! the shape wasi-libc requires -- is pinned here. The end-to-end map is
//! `scripts/wasm_coverage.py`.

use super::*;

fn refused_with_usage(args: &[&str]) -> String {
    let output = Command::new(BIN)
        .args(args)
        .arg(repo_root().join("examples/hello.cho"))
        .output()
        .expect("the compiler runs");
    assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_target_needs_the_llvm_backend() {
    let said =
        refused_with_usage(&["check", "--backend", "cranelift", "--target", "wasm32-wasip1"]);
    assert!(said.contains("--target"), "{said}");
    assert!(said.contains("llvm"), "{said}");
}

#[test]
fn an_unknown_target_is_a_usage_error() {
    let said = refused_with_usage(&["build", "--target", "not-a-triple"]);
    assert!(said.contains("not-a-triple"), "{said}");
}

#[test]
fn test_does_not_take_a_target_yet() {
    // Ignoring it would test the host build and report it as the wasm one.
    let said = refused_with_usage(&["test", "--target", "wasm32-wasip1"]);
    assert!(said.contains("--target"), "{said}");
}

#[test]
fn a_target_is_in_the_usage_text() {
    let help = Command::new(BIN).arg("--help").output().expect("the compiler runs");
    let help = String::from_utf8_lossy(&help.stdout);
    for command in ["build", "check", "run"] {
        let line = help
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("cancho {command}")))
            .unwrap_or_else(|| panic!("no usage line for `{command}`"));
        assert!(line.contains("--target <triple>"), "`{command}`: {line}");
    }
}

/// W0.2 (`docs/wasm.md`): what WASI preview 1 cannot do is refused, with the
/// rule `unsupported-on-target`, at the function that reaches it, before any
/// code is generated -- so this needs no wasm toolchain.
///
/// Each of these is a real accept fixture, so the same program is fine for the
/// host: the refusal is a property of the target.
#[test]
fn what_wasi_cannot_do_is_a_located_refusal_not_a_trap_or_a_link_error() {
    for (fixture, function, builtin, noun) in [
        ("spawn_join", "main", "spawn", "threads"),
        ("fork_heap_workers", "pair", "fork_heap", "threads"),
        ("connect_a_refused_address", "probe", "connect", "sockets"),
        ("signals_claim", "main", "signals_watch", "signals"),
        ("process_spawn", "run", "pipe_open", "processes and pipes"),
    ] {
        let path = repo_root().join(format!("tests/accept/{fixture}.cho"));

        let host = Command::new(BIN).arg("check").arg(&path).arg("--std").output().unwrap();
        assert!(host.status.success(), "{fixture} should check for the host");

        let wasm = Command::new(BIN)
            .args(["check", "--std", "--output", "json", "--target", "wasm32-wasip1"])
            .arg(&path)
            .output()
            .expect("the compiler runs");
        assert_eq!(wasm.status.code(), Some(1), "{fixture}");
        let body = String::from_utf8_lossy(&wasm.stdout);
        assert!(body.contains("\"rule\": \"unsupported-on-target\""), "{fixture}: {body}");
        assert!(body.contains(&format!("`{function}` uses `{builtin}`")), "{fixture}: {body}");
        assert!(body.contains(&format!("{noun} do not exist on `wasm32-wasip1`")), "{body}");
        assert!(body.contains("\"line\""), "{fixture}: a refusal is located: {body}");
    }
}

#[test]
fn build_refuses_too_not_only_check() {
    let out = Command::new(BIN)
        .args(["build", "--std", "--target", "wasm32-wasip1", "-o"])
        .arg(scratch("wasm-refused").join("out"))
        .arg(repo_root().join("tests/accept/spawn_join.cho"))
        .output()
        .expect("the compiler runs");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("threads do not exist on `wasm32-wasip1`"), "{said}");
}

/// W2b (`docs/wasm.md`): `authority --target wasm32-wasip1` says what the row licenses
/// a WASI module to import. Text only, so CI runs it; that the *numbers* are what real
/// modules import is `scripts/wasm_authority_check.py` (110 of 110 on this commit).
#[test]
fn authority_reports_what_a_wasi_module_may_import() {
    let authority = |program: &str, extra: &[&str]| -> String {
        let out = Command::new(BIN)
            .args(["authority", "--std"])
            .args(extra)
            .arg(repo_root().join(program))
            .output()
            .expect("the compiler runs");
        assert!(out.status.success(), "{program}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    // A console filter's row is `[heap, io_read, io_write]`, and licenses exactly the two
    // calls the console is written against (W2a), and how to exit (`proc_exit`, allowed to
    // every program and required of none).
    let json = authority(
        "tests/programs/json_roundtrip.cho",
        &["--target", "wasm32-wasip1", "--output", "json"],
    );
    for line in [
        "\"target\": \"wasm32-wasip1\"",
        "\"required\": [\"fd_read\", \"fd_write\"]",
        "\"allowed\": [\"fd_read\", \"fd_write\", \"proc_exit\"]",
        "\"refused\": []",
        "\"unbounded\": []",
        "\"unknown\": []",
    ] {
        assert!(json.contains(line), "missing `{line}` in:\n{json}");
    }

    // A label is coarser than a builtin: a program that opens files may use more than it
    // must, and the report says both.
    let files = authority(
        "tests/accept/file_roundtrip.cho",
        &["--target", "wasm32-wasip1", "--output", "json"],
    );
    assert!(files.contains("\"path_open\""), "{files}");
    assert!(files.contains("\"path_rename\""), "allowed, though this program never renames");
    let required = files.lines().find(|l| l.contains("\"required\"")).expect("a required line");
    assert!(!required.contains("path_rename"), "{required}");

    // A program the target cannot do is reported as such, beside the program being refused.
    let refused = authority(
        "tests/accept/spawn_join.cho",
        &["--target", "wasm32-wasip1", "--output", "json"],
    );
    assert!(refused.contains("\"refused\": [\"conc\"]"), "{refused}");

    // The host's report is not touched: no `wasi` key without a target.
    let host = authority("tests/programs/json_roundtrip.cho", &["--output", "json"]);
    assert!(!host.contains("\"wasi\""), "{host}");
    // `functions` is still the last key, with no trailing comma (the count is the std's to
    // change, so only its shape is pinned).
    let mut tail = host.trim_end().lines().rev();
    assert_eq!(tail.next(), Some("}"), "{host}");
    let last = tail.next().expect("a last key");
    assert!(last.starts_with("  \"functions\": ") && !last.ends_with(','), "{last}");

    // And the prose says it too.
    let text = authority("tests/programs/json_roundtrip.cho", &["--target", "wasm32-wasip1"]);
    assert!(text.contains("a wasm32-wasip1 module built from this program"), "{text}");
    assert!(text.contains("must import fd_read fd_write"), "{text}");
}
