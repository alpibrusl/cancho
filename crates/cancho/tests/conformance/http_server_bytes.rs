//! `packages/http-server`'s byte-fed mode (`docs/http-server.md` §11): the server opened with
//! `open_bytes`, fed by a program that owns no socket. The tests are cancho's own
//! (`tests/packages/http_server_bytes_test.cho`, `cancho test`), written in the language
//! they test, and run against the package's source on both backends; the socket path's own
//! are `api.rs` and `http_server.rs`, unchanged.

use super::*;

/// How many `fn test_*` the file holds: a test deleted or renamed away is a red build here,
/// not a quiet pass of fewer.
const TESTS: usize = 19;

#[test]
fn the_byte_fed_tests_pass_on_both_backends() {
    let file = repo_root().join("tests/packages/http_server_bytes_test.cho");
    let source = std::fs::read_to_string(&file).unwrap();
    assert_eq!(source.matches("\nfn test_").count(), TESTS, "the count above is stale");
    for backend in ["llvm", "cranelift"] {
        let out = Command::new(BIN)
            .args(["test", "--std", "--backend", backend])
            .arg(repo_root().join("packages/http-server/server.cho"))
            .arg(&file)
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
