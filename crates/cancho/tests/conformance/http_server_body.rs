//! `packages/http-server`'s streaming request bodies and `Expect: 100-continue` (`docs/http-server.md` §12): a server opened with
//! `open_bytes` and `limits`, fed by a program that owns no socket. The tests are cancho's own, written in the language they test
//! (`tests/packages/http_server_body_test.cho` and `http_server_body_flow_test.cho`, with their helpers in `body_support.cho`), run
//! with `cancho test` against the package's source on both backends. The socket path's behaviour for a server that never calls
//! `limits` is `api.rs`'s and `http_server.rs`'s, unchanged; the byte-fed mode's is `http_server_bytes.rs`'s.

use super::*;

/// (file, how many `fn test_*` it holds): a test deleted or renamed away is a red build here, not a quiet pass of fewer.
const FILES: [(&str, usize); 2] = [("http_server_body_test.cho", 7), ("http_server_body_flow_test.cho", 13)];

#[test]
fn the_streaming_body_tests_pass_on_both_backends() {
    for (name, tests) in FILES {
        let file = repo_root().join("tests/packages").join(name);
        let source = std::fs::read_to_string(&file).unwrap();
        assert_eq!(source.matches("\nfn test_").count(), tests, "{name}: the count above is stale");
        for backend in ["llvm", "cranelift"] {
            let out = Command::new(BIN)
                .args(["test", "--std", "--backend", backend])
                .arg(repo_root().join("packages/http-server/server.cho"))
                .arg(repo_root().join("tests/packages/body_support.cho"))
                .arg(&file)
                .output()
                .expect("the compiler runs");
            let text = String::from_utf8_lossy(&out.stdout);
            assert_eq!(out.status.code(), Some(0), "{name} on {backend}:\n{text}\n{}", String::from_utf8_lossy(&out.stderr));
            assert!(text.contains(&format!("test result: ok. {tests} passed; 0 failed")), "{name} on {backend}: {text}");
        }
    }
}
