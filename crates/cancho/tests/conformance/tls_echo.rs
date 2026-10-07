//! `examples/tls_echo` (`docs/tls-server.md` §8, step 3): the TLS echo server
//! the broker and the gateway copy, built and run against `packages/tls`'s
//! own client (`tests/programs/tls_many.cho`), so no TLS library outside this
//! repository is needed. `scripts/tls_echo_test.py` is the same server
//! against `openssl s_client` and Python's `ssl`, and its cost (CI's
//! `tls-assurance` job runs it).

use super::*;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

fn example_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = ["tls_echo.cho", "echo.cho", "front.cho", "identity.cho"]
        .iter()
        .map(|f| repo_root().join("examples/tls_echo").join(f))
        .collect();
    files.extend(super::tls_server::package_files());
    files
}

/// The report, pinned: what the example can reach is the review of it a
/// supervisor reads, so a label that appears or goes is a red build. No
/// foreign code (`bounded`), the files read through a directory handle
/// (`dir_read`, `file_read`) after one path each at start (`fs_read("")`:
/// the operator names the directory, so there is no literal to narrow to,
/// `docs/agent-toolbox.md` §2.2; `examples/tls_echo_fixed` fixes the directory
/// and reports two literal paths instead, `tls_echo_fixed.rs`), and the three
/// signals it claims.
#[test]
fn the_example_reports_a_bounded_authority_and_no_foreign_code() {
    let out = Command::new(BIN)
        .arg("authority")
        .args(example_files())
        .args(["--std", "--output", "json"])
        .output()
        .expect("the compiler runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let json = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(json.trim_start().starts_with("{\n  \"bounded\": true,"), "{json}");
    assert!(json.contains("\"foreign_symbols\": []"), "{json}");
    let labels: Vec<String> = json
        .lines()
        .filter_map(|l| l.trim().strip_prefix("{ \"name\": "))
        .map(|l| l.split(", \"bounded\"").next().unwrap().to_string())
        .collect();
    let want = [
        "\"args\", \"argument\": null",
        "\"clock\", \"argument\": null",
        "\"conn_accept\", \"argument\": null",
        "\"conn_read\", \"argument\": null",
        "\"conn_write\", \"argument\": null",
        "\"dir_read\", \"argument\": null",
        "\"err_write\", \"argument\": null",
        "\"file_read\", \"argument\": null",
        "\"fs_read\", \"argument\": \"\"",
        "\"heap\", \"argument\": null",
        "\"io_write\", \"argument\": null",
        "\"net_in\", \"argument\": \"\"",
        "\"poll\", \"argument\": null",
        "\"signals\", \"argument\": \"HUP,INT,TERM\"",
        "\"signals_read\", \"argument\": null",
    ];
    assert_eq!(labels, want, "{json}");
}

/// `tls_echo`'s output, line by line as it is written.
pub(super) struct Log {
    pub(super) lines: Mutex<Vec<String>>,
    pub(super) more: Condvar,
}

impl Log {
    pub(super) fn wait(&self, what: &str, count: usize) -> Vec<String> {
        let mut lines = self.lines.lock().unwrap();
        let end = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let found: Vec<String> = lines.iter().filter(|l| l.contains(what)).cloned().collect();
            if found.len() >= count {
                return found;
            }
            let left = end.saturating_duration_since(std::time::Instant::now());
            assert!(!left.is_zero(), "waited for {count} `{what}`: {lines:?}");
            lines = self.more.wait_timeout(lines, left).unwrap().0;
        }
    }
}

/// The server, killed if the test fails before it stops it: an orphan would
/// hold the test's standard error open, and the run with it.
pub(super) struct Running(pub(super) std::process::Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) fn build(dir: &Path, name: &str, files: &[PathBuf]) -> PathBuf {
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

pub(super) fn install(identity: &str, into: &Path, files: &[&str]) {
    let from = repo_root().join("tests/vectors/tls/echo").join(identity);
    for f in files {
        std::fs::copy(from.join(f), into.join(f)).unwrap();
    }
}

pub(super) fn signal(pid: u32, name: &str) {
    let ok = Command::new("kill").args([&format!("-{name}"), &pid.to_string()]).status().unwrap();
    assert!(ok.success(), "kill -{name}");
}

/// `conc` connections of `tls_many` at once: each verifies the chain
/// against the test CA and the name, sends 16,384 bytes and reads until the
/// server's close_notify, which the echo sends once `--idle` passes with
/// nothing more. Every connection must end `ok` with all 16,384 bytes back,
/// the same for every one.
pub(super) fn clients(exe: &Path, port: u16, conc: usize) {
    let ca = std::fs::read(repo_root().join("tests/vectors/tls/echo/ca.pem")).unwrap();
    let mut child = Command::new(exe)
        .args(["127.0.0.1", &port.to_string(), "echo.lex-sys.test", &conc.to_string(), "65536"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&ca).unwrap();
    let out = child.wait_with_output().unwrap();
    let lines: Vec<String> =
        String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    assert_eq!(lines.last().cloned(), Some(format!("done ok={conc} failed=0")), "{lines:?}");
    let echoed: Vec<&str> = lines[..conc].iter().map(|l| l.split(' ').nth(3).unwrap()).collect();
    assert!(echoed.iter().all(|n| *n == "16384"), "every request echoed whole: {lines:?}");
    let hashes: Vec<&str> = lines[..conc].iter().map(|l| l.split(' ').nth(4).unwrap()).collect();
    assert!(hashes.iter().all(|h| *h == hashes[0]), "every echo the same bytes: {lines:?}");
}

/// The example end to end on one thread: 64 connections at once with at
/// most 4 handshakes in progress (the rest queued, `docs/tls-server.md` §7),
/// every byte echoed and every connection closed with close_notify when
/// idle; a reload (`SIGHUP`) that takes, and one that is refused and leaves
/// the renewed identity serving; then a stop (`SIGTERM`), exit status 0.
#[test]
fn the_echo_serves_many_connections_reloads_and_stops_cleanly() {
    let dir = scratch("tls-echo");
    let echo = build(&dir, "tls_echo", &example_files());
    let mut many_files = vec![repo_root().join("tests/programs/tls_many.cho")];
    many_files.extend(super::tls_server::package_files());
    let many = build(&dir, "tls_many", &many_files);
    let certs = dir.join("certs");
    std::fs::create_dir_all(&certs).unwrap();
    install("first", &certs, &["chain.pem", "key.pem", "names"]);

    let port = free_port();
    let mut server = Running(
        Command::new(&echo)
            .args(["--port", &port.to_string(), "--dir"])
            .arg(&certs)
            .args(["--idle", "1000", "--handshakes", "4", "--connections", "64"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let log = Arc::new(Log { lines: Mutex::new(Vec::new()), more: Condvar::new() });
    let stdout = server.0.stdout.take().unwrap();
    let writer = Arc::clone(&log);
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            writer.lines.lock().unwrap().push(line.unwrap());
            writer.more.notify_all();
        }
    });
    log.wait("listening", 1);

    clients(&many, port, 64);
    let up = log.wait(" established suite=", 64);
    assert!(up.iter().all(|l| l.contains(" sni=echo.lex-sys.test ")), "{up:?}");
    log.wait(" closed idle ", 64);

    install("renewed", &certs, &["chain.pem", "key.pem"]);
    signal(server.0.id(), "HUP");
    log.wait("reload 0 ok", 1);
    install("first", &certs, &["chain.pem"]);
    signal(server.0.id(), "HUP");
    log.wait("reload 0 refused tls-server-key-mismatch", 1);
    clients(&many, port, 8);
    log.wait(" closed idle ", 72);

    signal(server.0.id(), "TERM");
    let status = server.0.wait().unwrap();
    reader.join().unwrap();
    log.wait("stopping", 1);
    assert_eq!(status.code(), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}
