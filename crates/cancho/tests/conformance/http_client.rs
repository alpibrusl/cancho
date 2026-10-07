//! `packages/http-client` (`docs/http-client.md`): a non-blocking HTTP/1.1 client with no socket in it. The tests are cancho's
//! own (`tests/packages/http_client_*.cho`, `cancho test`), written in the language they test and run against the package's
//! source on both backends: a caller's loop played by the test, and an upstream played by hand. `scripts/http_client_test.py`
//! is the same client, through `examples/http_fetch_nb`, against real servers over plain TCP and TLS (CI's `tls-assurance`
//! job), and `scripts/http_client_mutants.py` breaks the package one site at a time and watches these tests fail.

use super::*;

/// How many `fn test_*` the files hold: a test deleted or renamed away is a red build here, not a quiet pass of fewer.
const TESTS: usize = 111;

const TEST_FILES: [&str; 7] = [
    "http_client_harness.cho",
    "http_client_test.cho",
    "http_client_pool_test.cho",
    "http_client_time_test.cho",
    "http_client_wire_test.cho",
    "http_client_flow_test.cho",
    "http_client_edge_test.cho",
];

fn package_files() -> Vec<PathBuf> {
    ["slot.cho", "wire.cho", "client.cho"]
        .iter()
        .map(|f| repo_root().join("packages/http-client").join(f))
        .collect()
}

#[test]
fn the_client_tests_pass_on_both_backends() {
    let files: Vec<PathBuf> =
        TEST_FILES.iter().map(|f| repo_root().join("tests/packages").join(f)).collect();
    let counted: usize = files
        .iter()
        .map(|f| {
            let source = std::fs::read_to_string(f).unwrap();
            source.matches("\nfn test_").count() + source.matches("\npub fn test_").count()
        })
        .sum();
    assert_eq!(counted, TESTS, "the count above is stale");
    for backend in ["llvm", "cranelift"] {
        let out = Command::new(BIN)
            .args(["test", "--std", "--backend", backend])
            .args(package_files())
            .args(&files)
            .output()
            .expect("the compiler runs");
        let text = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{backend}:\n{text}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(text.contains(&format!("test result: ok. {TESTS} passed; 0 failed")), "{text}");
    }
}

/// The package performs no I/O, reads no clock and owns no socket: a program that makes a request, takes its bytes, gives a
/// response and reads the body reports the heap (the client's buffers) and the console (the program printing what it saw),
/// and nothing else -- no network, no clock, no foreign code. The program is `tests/programs/http_client_bytes.cho`; its
/// output is the request as the client wrote it and the body of a chunked response given in pieces of three bytes.
#[test]
fn the_package_reaches_the_heap_and_nothing_else() {
    let program = repo_root().join("tests/programs/http_client_bytes.cho");
    let out = Command::new(BIN)
        .arg("authority")
        .args(package_files())
        .arg(&program)
        .args(["--std", "--output", "json"])
        .output()
        .expect("the compiler runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let json = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(json.trim_start().starts_with("{\n  \"bounded\": true,"), "{json}");
    assert!(json.contains("\"effects\": [\"heap\", \"io_write\"]"), "{json}");
    assert!(json.contains("\"foreign_symbols\": []"), "{json}");
    for backend in ["llvm", "cranelift"] {
        let out = Command::new(BIN)
            .args(["run", "--std", "--backend", backend])
            .args(package_files())
            .arg(&program)
            .output()
            .expect("the compiler runs");
        assert_eq!(
            out.status.code(),
            Some(0),
            "{backend}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "GET /hello HTTP/1.1\r\nHost: example.test\r\nAccept: text/plain\r\n\r\nhello world\n",
            "{backend}"
        );
    }
}
