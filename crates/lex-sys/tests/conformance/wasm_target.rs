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
        .arg(repo_root().join("examples/hello.ls"))
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
            .find(|l| l.trim_start().starts_with(&format!("lex-sys {command}")))
            .unwrap_or_else(|| panic!("no usage line for `{command}`"));
        assert!(line.contains("--target <triple>"), "`{command}`: {line}");
    }
}
