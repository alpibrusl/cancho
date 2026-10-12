//! `examples/http_fetch_nb` (`docs/http-client.md` §6): several URLs fetched at once on one thread, through `packages/http-client`
//! and `fetch_io.cho` (the sockets, the TLS engine and the poller the package leaves to its caller). Built here and run against a
//! server written in this file (plain TCP, keep-alive, a length and chunked) and against `examples/https_hello`, the repository's
//! own TLS server, so no program outside the repository is needed. `scripts/http_client_test.py` is the same program against
//! Python's `ssl`, nginx and a hostile server, and `scripts/http_client_mutants.py` breaks the package under its tests (CI's
//! `tls-assurance` job runs both).

use super::tls_echo::{Log, Running, build, install};
use super::*;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

fn example_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = ["fetch.cho", "fetch_io.cho"]
        .iter()
        .map(|f| repo_root().join("examples/http_fetch_nb").join(f))
        .collect();
    files.extend(
        ["slot.cho", "wire.cho", "client.cho"]
            .iter()
            .map(|f| repo_root().join("packages/http-client").join(f)),
    );
    files.extend(super::tls_server::package_files());
    files
}

/// The report, pinned, as the TLS echo's is: what the example can reach is the review of it a supervisor reads, so a label that
/// appears or goes is a red build. No foreign code (`bounded`); the network only outbound (`net_out`, with no host named: the
/// URLs are the operator's); the sockets and the poller; the clock; the console and standard input (the roots); and one path,
/// `/dev/urandom`, for the TLS engine's entropy. No file is written and nothing is listened on.
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
        "\"conn_read\", \"argument\": null",
        "\"conn_write\", \"argument\": null",
        "\"err_write\", \"argument\": null",
        "\"fs_read\", \"argument\": \"/dev/urandom\"",
        "\"heap\", \"argument\": null",
        "\"io_read\", \"argument\": null",
        "\"io_write\", \"argument\": null",
        "\"net_out\", \"argument\": \"\"",
        "\"poll\", \"argument\": null",
    ];
    assert_eq!(labels, want, "{json}");
}

/// `sha256("hello")` and `sha256("hello world")`.
const HELLO: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
const HELLO_WORLD: &str = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

/// A keep-alive HTTP/1.1 server on a thread: `/a` answers `hello` with a length, `/c` answers `hello world` in two chunks, and
/// it counts the connections it was given.
fn serve(listener: TcpListener, connections: Arc<AtomicUsize>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { return };
        connections.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut out = stream;
            loop {
                let mut target = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if target.is_empty() {
                        target = line.split(' ').nth(1).unwrap_or("/").to_string();
                    }
                    if line == "\r\n" {
                        break;
                    }
                }
                let answer: &[u8] = if target == "/a" {
                    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello"
                } else if target == "/c" {
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n6\r\nhello \r\n5\r\nworld\r\n0\r\n\r\n"
                } else {
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"
                };
                if std::io::Write::write_all(&mut out, answer).is_err() {
                    return;
                }
            }
        });
    }
}

/// The lines `http_fetch_nb` printed: `lane status bytes sha reuse`, then `done ...`.
fn run(exe: &Path, args: &[String], stdin: Option<&[u8]>) -> (Option<i32>, Vec<String>) {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    if let Some(bytes) = stdin {
        input.write_all(bytes).unwrap();
    }
    drop(input);
    let out = child.wait_with_output().unwrap();
    let lines = String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    (out.status.code(), lines)
}

#[test]
fn it_fetches_over_plain_tcp_on_connections_it_keeps() {
    let dir = scratch("http-fetch-plain");
    let exe = build(&dir, "http_fetch_nb", &example_files());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let connections = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&connections);
    std::thread::spawn(move || serve(listener, counted));

    let url = |path: &str| format!("http://127.0.0.1:{port}{path}");
    let (status, lines) = run(&exe, &["--repeat".into(), "4".into(), url("/a"), url("/c")], None);
    assert_eq!(status, Some(0), "{lines:?}");
    assert_eq!(
        lines.last().unwrap(),
        "done ok=8 failed=0 connects=2 reuses=6 retries=0",
        "{lines:?}"
    );
    let want = |lane: &str, bytes: &str, sha: &str| {
        lines.iter().filter(|l| l.starts_with(&format!("{lane} 200 {bytes} {sha} "))).count()
    };
    assert_eq!(want("0", "5", HELLO), 4, "{lines:?}");
    assert_eq!(want("1", "11", HELLO_WORLD), 4, "{lines:?}");
    assert_eq!(lines.iter().filter(|l| l.ends_with(" new")).count(), 2, "{lines:?}");
    assert_eq!(lines.iter().filter(|l| l.ends_with(" reused")).count(), 6, "{lines:?}");
    assert_eq!(connections.load(Ordering::SeqCst), 2);

    // A 404 is a response, not a failure; a closed port is `client.connect`; a bad command line is refused.
    let (status, lines) = run(&exe, &[url("/missing")], None);
    assert_eq!(status, Some(0), "{lines:?}");
    assert!(lines[0].starts_with("0 404 0 "), "{lines:?}");
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let (status, lines) = run(&exe, &[format!("http://127.0.0.1:{closed}/")], None);
    assert_eq!(status, Some(1), "{lines:?}");
    assert_eq!(lines[0], "0 failed client.connect", "{lines:?}");
    let (status, _) = run(&exe, &["http://not-an-address.test/".into()], None);
    assert_eq!(status, Some(2));
    let (status, _) = run(&exe, &[], None);
    assert_eq!(status, Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn it_fetches_over_tls_from_https_hello_verifying_the_name_and_closing_with_close_notify() {
    let dir = scratch("http-fetch-tls");
    let fetch = build(&dir, "http_fetch_nb", &example_files());
    let hello = build(&dir, "https_hello", &super::https_hello::example_files());
    let certs = dir.join("certs");
    std::fs::create_dir_all(&certs).unwrap();
    install("first", &certs, &["chain.pem", "key.pem", "names"]);
    let port = free_port();
    let mut server = Running(
        Command::new(&hello)
            .args(["--port", &port.to_string(), "--dir"])
            .arg(&certs)
            .args(["--idle", "60000", "--connections", "16"])
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

    let ca = std::fs::read(repo_root().join("tests/vectors/tls/echo/ca.pem")).unwrap();
    let name = "echo.lex-sys.test";
    let url = |path: &str| format!("https://{name}:{port}{path}");
    let resolve = ["--resolve".to_string(), format!("{name}=127.0.0.1")];
    let mut args = resolve.to_vec();
    args.extend([
        "--repeat".into(),
        "3".into(),
        url("/hello/n1"),
        url("/big/100000"),
        url("/hello/n2"),
    ]);
    let (status, lines) = run(&fetch, &args, Some(&ca));
    assert_eq!(status, Some(0), "{lines:?}");
    assert_eq!(
        lines.last().unwrap(),
        "done ok=9 failed=0 connects=3 reuses=6 retries=0",
        "{lines:?}"
    );
    assert_eq!(lines.iter().filter(|l| l.starts_with("1 200 100000 ")).count(), 3, "{lines:?}");
    let hashes: Vec<&str> = lines
        .iter()
        .filter(|l| l.starts_with("1 200 "))
        .map(|l| l.split(' ').nth(3).unwrap())
        .collect();
    assert!(hashes.iter().all(|h| *h == hashes[0]), "the same bytes every time: {lines:?}");
    // Three handshakes for nine requests, and each connection was ended with close_notify (the pool is closed before the exit).
    let up = log.wait(" established suite=", 3);
    assert!(up.iter().all(|l| l.contains(&format!(" sni={name} "))), "{up:?}");
    log.wait(" closed ok ", 3);

    // The name is checked: a certificate for `echo.lex-sys.test` is refused for any other name, and so are the wrong roots.
    let other = [
        "--resolve".to_string(),
        "other.example=127.0.0.1".to_string(),
        format!("https://other.example:{port}/hello/x"),
    ];
    let (status, lines) = run(&fetch, &other, Some(&ca));
    assert_eq!(status, Some(1), "{lines:?}");
    assert_eq!(lines[0], "0 failed client.tls", "{lines:?}");
    let (status, lines) = run(
        &fetch,
        &[resolve[0].clone(), resolve[1].clone(), url("/hello/x")],
        Some(b"not a pem\n"),
    );
    assert_eq!(
        status,
        Some(2),
        "a trust store with no root is refused before anything is dialled: {lines:?}"
    );

    let _ = server.0.kill();
    let _ = server.0.wait();
    let _ = reader.join();
    let _ = std::fs::remove_dir_all(&dir);
}
