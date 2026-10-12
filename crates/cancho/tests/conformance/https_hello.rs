//! `examples/https_hello` (`docs/http-server.md` §11): an HTTPS server made of the TLS
//! engine (`packages/tls`) and the byte-fed `http.server` (`packages/http-server`), built
//! and run against `packages/tls`'s own client (`tests/programs/tls_many.cho`), so no TLS
//! library outside this repository is needed. `scripts/https_hello_test.py` is the same
//! server against `openssl s_client`, curl and Python's `ssl`/`http.client`: keep-alive,
//! pipelining, 200 connections, a large body, a client that stops reading (CI's
//! `tls-assurance` job runs it).

use super::tls_echo::{Log, Running, build, install, signal};
use super::*;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Condvar, Mutex};

pub(super) fn example_files() -> Vec<PathBuf> {
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

/// The report, pinned, as the echo's is: no foreign code, and the same labels. The HTTP
/// server adds none (it is all `conn_*` and `poll`, which the loop has), so a label that
/// appears here is the example, or the package, gaining authority.
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

/// `conc` connections of `tls_many` at once: each verifies the chain against the test CA
/// and the name, sends `GET / HTTP/1.0` padded to 16,384 bytes (a full request buffer) and
/// reads until the server's close_notify. An HTTP/1.0 request is answered `Connection:
/// close` and closed, so the answer is the same 99 bytes for every one:
/// `HTTP/1.1 200 OK`, three headers, and `hello over TLS`.
fn clients(exe: &Path, port: u16, conc: usize) {
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
    let body = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 15\r\nConnection: close\r\n\r\nhello over TLS\n";
    let sizes: Vec<&str> = lines[..conc].iter().map(|l| l.split(' ').nth(3).unwrap()).collect();
    assert!(sizes.iter().all(|n| *n == body.len().to_string()), "every answer whole: {lines:?}");
    let hashes: Vec<&str> = lines[..conc].iter().map(|l| l.split(' ').nth(4).unwrap()).collect();
    assert!(hashes.iter().all(|h| *h == hashes[0]), "every answer the same bytes: {lines:?}");
}

/// The example end to end on one thread: 64 connections at once with at most 4 handshakes
/// in progress, each answered and closed with close_notify; a reload (`SIGHUP`) that takes,
/// and one that is refused and leaves the renewed identity serving; then a stop
/// (`SIGTERM`), exit status 0.
#[test]
fn the_server_answers_many_clients_reloads_and_stops_cleanly() {
    let dir = scratch("https-hello");
    let hello = build(&dir, "https_hello", &example_files());
    let mut many_files = vec![repo_root().join("tests/programs/tls_many.cho")];
    many_files.extend(super::tls_server::package_files());
    let many = build(&dir, "tls_many", &many_files);
    let certs = dir.join("certs");
    std::fs::create_dir_all(&certs).unwrap();
    install("first", &certs, &["chain.pem", "key.pem", "names"]);

    let port = free_port();
    let mut server = Running(
        Command::new(&hello)
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
    log.wait(" closed ok ", 64);

    install("renewed", &certs, &["chain.pem", "key.pem"]);
    signal(server.0.id(), "HUP");
    log.wait("reload 0 ok", 1);
    install("first", &certs, &["chain.pem"]);
    signal(server.0.id(), "HUP");
    log.wait("reload 0 refused tls-server-key-mismatch", 1);
    clients(&many, port, 8);
    log.wait(" closed ok ", 72);

    signal(server.0.id(), "TERM");
    let status = server.0.wait().unwrap();
    reader.join().unwrap();
    log.wait("stopping", 1);
    assert_eq!(status.code(), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}
