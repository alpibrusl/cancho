//! `packages/http-server/` (`docs/http-server.md`): the parts of its contract
//! `examples/api` does not exercise, because `api` follows the rule that
//! `wait` is followed by `next` until `-1`.

use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// The package's one fetched file, and the program that calls `next` once per
/// `wait`.
fn one_per_round(tag: &str) -> (PathBuf, PathBuf) {
    let mut paths = vec![repo_root().join("tests/programs/server_one_per_round.ls")];
    paths.extend(fetch_net_dependencies(
        tag,
        &[("examples/api/server.lock", "packages/http-server/.lex-sys-vcs")],
    ));
    build_example_paths(tag, &paths, "server")
}

fn read_until(stream: &mut TcpStream, expect: usize) -> String {
    let mut got = Vec::new();
    let mut chunk = [0u8; 4096];
    let deadline = Instant::now() + Duration::from_secs(10);
    while got.len() < expect {
        assert!(
            Instant::now() < deadline,
            "only {} of {expect} bytes: {:?}",
            got.len(),
            String::from_utf8_lossy(&got)
        );
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => got.extend(&chunk[..n]),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => panic!("{e}"),
        }
    }
    String::from_utf8_lossy(&got).into_owned()
}

#[test]
fn requests_a_caller_did_not_get_to_are_not_lost() {
    let (dir, exe) = one_per_round("hs-one-per-round");
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut child = Command::new(&exe)
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the compiled server runs");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut clients = Vec::new();
    while clients.len() < 4 {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(s) => {
                s.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
                clients.push(s);
            }
            Err(e) => {
                assert!(Instant::now() < deadline, "never listened: {e}");
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    // Four connections, each pipelining six requests in one write, each path
    // naming the client and its place: the server hands over one request per
    // round, so three of the four connections are left unvisited every time.
    let mut expected = Vec::new();
    for (i, stream) in clients.iter_mut().enumerate() {
        let mut request = String::new();
        let mut want = String::new();
        for j in 0..6 {
            let path = format!("/c{i}r{j}");
            request.push_str(&format!("GET {path} HTTP/1.1\r\nHost: t\r\n\r\n"));
            want.push_str(&format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{path}",
                path.len()
            ));
        }
        stream.write_all(request.as_bytes()).unwrap();
        expected.push(want);
    }
    for (stream, want) in clients.iter_mut().zip(&expected) {
        let got = read_until(stream, want.len());
        assert_eq!(&got, want);
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_checked_in_store_still_resolves_and_holds_this_source() {
    let store = repo_root().join("packages/http-server/.lex-sys-vcs");
    let out = Command::new(BIN).args(["vcs", "resolve"]).arg(&store).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // The store holds exactly the file in the tree: a source edited without
    // re-publishing would leave consumers on the old one.
    let source =
        std::fs::read_to_string(repo_root().join("packages/http-server/server.ls")).unwrap();
    let mut found = false;
    for entry in std::fs::read_dir(store.join("sources")).unwrap() {
        found |= std::fs::read_to_string(entry.unwrap().path()).unwrap() == source;
    }
    assert!(
        found,
        "packages/http-server/server.ls is not what the store published; re-run `vcs publish --std`"
    );
}
