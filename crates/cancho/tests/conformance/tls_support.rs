//! What `http_fetch_nb.rs` still takes from the TLS test files that moved to cancho (`docs/gate.md` G4, wave 4: `tests/commands/tls*.sh`
//! and `https_hello.sh`): the log of a server's standard output, a server killed when its test ends, a build, the test identity's files,
//! the package's file list and the HTTPS example's. The modules keep the names the old files had (`tls_echo`, `tls_server`,
//! `https_hello`), so that file builds unchanged. When `http_fetch_nb.rs` moves, this file goes with it (and the three `use` lines of `main.rs`).

use super::*;

pub(super) mod tls_echo {
    use super::*;
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    /// A server's output, line by line as it is written.
    pub(in super::super) struct Log {
        pub(in super::super) lines: Mutex<Vec<String>>,
        pub(in super::super) more: Condvar,
    }

    impl Log {
        pub(in super::super) fn wait(&self, what: &str, count: usize) -> Vec<String> {
            let mut lines = self.lines.lock().unwrap();
            let end = std::time::Instant::now() + Duration::from_secs(30);
            loop {
                let found: Vec<String> =
                    lines.iter().filter(|l| l.contains(what)).cloned().collect();
                if found.len() >= count {
                    return found;
                }
                let left = end.saturating_duration_since(std::time::Instant::now());
                assert!(!left.is_zero(), "waited for {count} `{what}`: {lines:?}");
                lines = self.more.wait_timeout(lines, left).unwrap().0;
            }
        }
    }

    /// The server, killed if the test fails before it stops it: an orphan would hold the test's standard error open, and the run with it.
    pub(in super::super) struct Running(pub(in super::super) std::process::Child);

    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    pub(in super::super) fn build(dir: &Path, name: &str, files: &[PathBuf]) -> PathBuf {
        let exe = dir.join(name);
        let out = Command::new(BIN)
            .args(["build", "--std", "--backend", "llvm"])
            .args(files)
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("the compiler runs");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        exe
    }

    pub(in super::super) fn install(identity: &str, into: &Path, files: &[&str]) {
        let from = repo_root().join("tests/vectors/tls/echo").join(identity);
        for f in files {
            std::fs::copy(from.join(f), into.join(f)).unwrap();
        }
    }
}

pub(super) mod tls_server {
    use super::*;

    /// The package's files: the engine, the client it is built on, the server, and `packages/x509` with the key parser.
    pub(in super::super) fn package_files() -> Vec<PathBuf> {
        let tls = [
            "tls.cho",
            "record.cho",
            "message.cho",
            "slot.cho",
            "cident.cho",
            "client12.cho",
            "client.cho",
            "hello.cho",
            "identity.cho",
            "ticket.cho",
            "clientauth.cho",
            "server.cho",
            "saved.cho",
        ];
        let x509 = ["verify.cho", "names.cho", "x509.cho", "key.cho"];
        let mut files: Vec<PathBuf> =
            tls.iter().map(|f| repo_root().join("packages/tls").join(f)).collect();
        files.extend(x509.iter().map(|f| repo_root().join("packages/x509").join(f)));
        files
    }
}

pub(super) mod https_hello {
    use super::*;

    pub(in super::super) fn example_files() -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = ["hello.cho", "loop.cho", "app.cho"]
            .iter()
            .map(|f| repo_root().join("examples/https_hello").join(f))
            .collect();
        for f in ["front.cho", "identity.cho"] {
            files.push(repo_root().join("examples/tls_echo").join(f));
        }
        files.push(repo_root().join("packages/http-server/server.cho"));
        files.extend(super::tls_server::package_files());
        files
    }
}
