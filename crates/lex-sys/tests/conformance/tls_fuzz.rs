//! The fuzzing corpus as a regression test (`docs/tls-assurance.md` §3.5):
//! every input committed under `tests/vectors/fuzz/<harness>/`, which is
//! AFL++'s minimised queue from the campaign §3.4 reports, run through its
//! harness (`tests/programs/fuzz_<harness>.ls`) on both backends. Each must
//! exit 0: a trap ends the process with a signal, which is the failure the
//! fuzzer looked for. AFL++ instrumented only the LLVM backend's code, so
//! the Cranelift run is the one place its code meets these inputs.

use super::json::feed;
use super::*;

const PACKAGES: [&str; 8] = [
    "packages/tls/record.ls",
    "packages/tls/message.ls",
    "packages/tls/slot.ls",
    "packages/tls/client12.ls",
    "packages/tls/client.ls",
    "packages/x509/verify.ls",
    "packages/x509/names.ls",
    "packages/x509/x509.ls",
];

fn replay(harness: &str) {
    let dir = repo_root().join("tests/vectors/fuzz").join(harness);
    let mut inputs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("a directory entry").path())
        .collect();
    inputs.sort();
    assert!(!inputs.is_empty(), "{}: no inputs", dir.display());
    for backend in ["llvm", "cranelift"] {
        let out = scratch(&format!("tls-fuzz-{harness}-{backend}"));
        let exe = out.join(harness);
        let build = Command::new(BIN)
            .args(["build", "--std", "--backend", backend])
            .arg(repo_root().join(format!("tests/programs/fuzz_{harness}.ls")))
            .arg(repo_root().join("tests/programs/fuzz_common.ls"))
            .arg(repo_root().join("tests/programs/fuzz_fixture.ls"))
            .args(PACKAGES.map(|f| repo_root().join(f)))
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("the compiler runs");
        assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
        for input in &inputs {
            let ran = feed(&exe, &std::fs::read(input).expect("the input reads"));
            assert_eq!(
                ran.status.code(),
                Some(0),
                "{backend}: {} ends with {:?}",
                input.display(),
                ran.status
            );
        }
    }
}

#[test]
fn der_corpus_never_traps() {
    replay("der");
}

#[test]
fn chain_corpus_never_traps() {
    replay("chain");
}

#[test]
fn messages_corpus_never_traps() {
    replay("messages");
}

#[test]
fn client_corpus_never_traps() {
    replay("client");
}

#[test]
fn flight_corpus_never_traps() {
    replay("flight");
}
