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
    let mut paths = vec![repo_root().join("tests/programs/server_one_per_round.cho")];
    paths.extend(fetch_net_dependencies(
        tag,
        &[("examples/api/server.lock", "packages/http-server/.cancho-vcs")],
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
    let store = repo_root().join("packages/http-server/.cancho-vcs");
    let out = Command::new(BIN).args(["vcs", "resolve"]).arg(&store).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // The store holds exactly the file in the tree: a source edited without
    // re-publishing would leave consumers on the old one.
    let source =
        std::fs::read_to_string(repo_root().join("packages/http-server/server.cho")).unwrap();
    let mut found = false;
    for entry in std::fs::read_dir(store.join("sources")).unwrap() {
        found |= std::fs::read_to_string(entry.unwrap().path()).unwrap() == source;
    }
    assert!(
        found,
        "packages/http-server/server.cho is not what the store published; re-run `vcs publish --std`"
    );
}

#[test]
fn replies_of_any_type_and_bodiless_ones_keep_the_connection_framed() {
    // `reply_as` writes the content type it is given; `reply_empty` writes a `204`
    // with no `Content-Length` and no `Content-Type` (RFC 9110 §15.3.5), so the
    // client must find the end of it at the blank line. Between two ordinary
    // answers on one keep-alive connection, a wrong frame would shift every
    // answer after it.
    let (dir, exe) = one_per_round("hs-reply-kinds");
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut child = Command::new(&exe)
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the compiled server runs");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut s = loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(s) => break s,
            Err(e) => {
                assert!(Instant::now() < deadline, "never listened: {e}");
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    };
    s.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
    let get = |p: &str| format!("GET {p} HTTP/1.1\r\nHost: t\r\n\r\n");
    s.write_all(format!("{}{}{}{}", get("/a"), get("/empty"), get("/typed"), get("/b")).as_bytes())
        .unwrap();
    let want = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\n/a\
                HTTP/1.1 204 No Content\r\nConnection: keep-alive\r\n\r\n\
                HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 2\r\nConnection: keep-alive\r\nX-Test: 1\r\n\r\nhi\
                HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\n/b";
    assert_eq!(read_until(&mut s, want.len()), want);
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `tests/programs/server_hold.cho`, running: a server on a port of its own that has
/// connected to the returned peer (the test's end of a connection the server watches in its
/// own poller), and the connection to write to.
struct Held {
    dir: PathBuf,
    child: std::process::Child,
    port: u16,
    peer: TcpStream,
}

impl Held {
    fn start(tag: &str) -> Held {
        Held::start_with(tag, 16384)
    }

    /// With the given buffer size, which sets how many connections the server has room for
    /// (256 MiB of buffers in all).
    fn start_with(tag: &str, size: usize) -> Held {
        let mut paths = vec![repo_root().join("tests/programs/server_hold.cho")];
        paths.extend(fetch_net_dependencies(
            tag,
            &[("tests/programs/server_hold.lock", "packages/http-server/.cancho-vcs")],
        ));
        let (dir, exe) = build_example_paths(tag, &paths, "server_hold");
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer_port = listener.local_addr().unwrap().port();
        let child = Command::new(&exe)
            .args([port.to_string(), peer_port.to_string(), size.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the compiled server runs");
        let (peer, _) = listener.accept().unwrap();
        Held { dir, child, port, peer }
    }

    fn client(&self) -> TcpStream {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match TcpStream::connect(("127.0.0.1", self.port)) {
                Ok(s) => {
                    s.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
                    return s;
                }
                Err(e) => {
                    assert!(Instant::now() < deadline, "never listened: {e}");
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }

    /// One byte written to the peer connection: one held request let go, oldest first.
    fn release(&mut self) {
        self.peer.write_all(b"x").unwrap();
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn get(path: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: t\r\n\r\n")
}

fn ok(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",
        body.len()
    )
}

/// Nothing more arrives within the read timeout.
fn silent(stream: &mut TcpStream) {
    let mut chunk = [0u8; 256];
    match stream.read(&mut chunk) {
        Err(e)
            if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut => {}
        other => panic!("expected silence, got {other:?}"),
    }
}

#[test]
fn a_held_request_does_not_stop_the_loop_and_what_follows_it_waits_its_turn() {
    let mut h = Held::start("hs-hold");
    let mut a = h.client();
    let mut b = h.client();
    // `a` asks for a held request and pipelines a second behind it.
    a.write_all(format!("{}{}", get("/hold"), get("/after")).as_bytes()).unwrap();
    // The loop is not stopped: another client is answered meanwhile, more than once.
    for path in ["/one", "/two", "/three"] {
        b.write_all(get(path).as_bytes()).unwrap();
        assert_eq!(read_until(&mut b, ok(path).len()), ok(path));
    }
    // `a` has heard nothing: not the held answer, and not the one behind it.
    silent(&mut a);
    // Another request arriving on it meanwhile queues behind them, unanswered.
    a.write_all(get("/late").as_bytes()).unwrap();
    silent(&mut a);
    h.release();
    // The held one is answered, then the ones that were waiting behind it, in order.
    let want = format!("{}{}{}", ok("released"), ok("/after"), ok("/late"));
    assert_eq!(read_until(&mut a, want.len()), want);
    // The connection is still good for another request.
    a.write_all(get("/again").as_bytes()).unwrap();
    assert_eq!(read_until(&mut a, ok("/again").len()), ok("/again"));
    // And nothing was held twice: a release with nothing held finds no ticket to refuse. (A
    // request handed out again while it was held would leave a second ticket for it.)
    h.release();
    std::thread::sleep(Duration::from_millis(200));
    a.write_all(get("/stale").as_bytes()).unwrap();
    assert_eq!(read_until(&mut a, ok("0").len()), ok("0"));
}

#[test]
fn held_requests_are_released_one_each_and_oldest_first() {
    let mut h = Held::start("hs-hold-order");
    let mut clients: Vec<TcpStream> = (0..3).map(|_| h.client()).collect();
    for (i, c) in clients.iter_mut().enumerate() {
        c.write_all(get("/hold").as_bytes()).unwrap();
        // Spaced, so the order they were held in is the order they were sent.
        std::thread::sleep(Duration::from_millis(100));
        assert!(i < 3);
    }
    h.release();
    assert_eq!(read_until(&mut clients[0], ok("released").len()), ok("released"));
    silent(&mut clients[1]);
    silent(&mut clients[2]);
    h.release();
    assert_eq!(read_until(&mut clients[1], ok("released").len()), ok("released"));
    silent(&mut clients[2]);
    h.release();
    assert_eq!(read_until(&mut clients[2], ok("released").len()), ok("released"));
}

#[test]
fn a_held_connection_with_a_full_buffer_waits_without_reading_and_loses_nothing() {
    let mut h = Held::start("hs-hold-full");
    let mut a = h.client();
    // Behind the held request, far more than the server's 16 KiB buffer holds: it stops
    // reading, the rest waits in the kernel, and nothing is dropped or refused.
    let mut request = get("/hold");
    let mut want = String::new();
    for i in 0..2500 {
        request.push_str(&get(&format!("/p{i:04}")));
        want.push_str(&ok(&format!("/p{i:04}")));
    }
    assert!(request.len() > 3 * 16384);
    let writer = {
        let mut w = a.try_clone().unwrap();
        std::thread::spawn(move || w.write_all(request.as_bytes()).unwrap())
    };
    std::thread::sleep(Duration::from_millis(500));
    silent(&mut a);
    h.release();
    let want = format!("{}{}", ok("released"), want);
    assert_eq!(read_until(&mut a, want.len()), want);
    writer.join().unwrap();
}

#[test]
fn answering_more_than_the_table_has_rooms_in_one_round_keeps_every_connection_queued() {
    // Buffers of 64 MiB leave room for 4 connections, so a ready queue of 4. One client
    // pipelines twelve requests, each held and answered at once: every answer puts the
    // connection back in the queue behind the one just taken, and by the fifth the queue's
    // end has been passed unless what was taken is made room of.
    let h = Held::start_with("hs-hold-queue", 64 << 20);
    let mut a = h.client();
    let mut b = h.client();
    a.write_all(get("/instant").repeat(12).as_bytes()).unwrap();
    b.write_all(get("/instant").repeat(12).as_bytes()).unwrap();
    let want = ok("instant").repeat(12);
    assert_eq!(read_until(&mut a, want.len()), want);
    assert_eq!(read_until(&mut b, want.len()), want);
}

#[test]
fn a_held_request_is_not_the_idle_timeouts_to_close() {
    let mut h = Held::start("hs-hold-idle");
    let mut a = h.client();
    a.write_all(get("/hold").as_bytes()).unwrap();
    // The server closes a connection quiet for 9 seconds; this one is quiet because its
    // answer is the application's to give.
    std::thread::sleep(Duration::from_secs(12));
    h.release();
    assert_eq!(read_until(&mut a, ok("released").len()), ok("released"));
}

#[test]
fn a_ticket_answers_once_and_never_reaches_a_connection_that_took_the_slot() {
    let mut h = Held::start("hs-hold-stale");
    let mut a = h.client();
    // Another connection keeps the lower slots occupied differently from `a`'s.
    let mut b = h.client();
    a.write_all(get("/hold").as_bytes()).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    h.release();
    assert_eq!(read_until(&mut a, ok("released").len()), ok("released"));
    // The ticket (the first handed out, 0) has been used: answering it again on the same
    // connection is refused, and only the request that asked is told anything.
    b.write_all(get("/replay/0").as_bytes()).unwrap();
    assert_eq!(read_until(&mut b, ok("1").len()), ok("1"));
    silent(&mut a);
    // The connection goes away and another takes its place in the server's table, holding
    // a request of its own.
    drop(a);
    std::thread::sleep(Duration::from_millis(300));
    let mut c = h.client();
    c.write_all(get("/hold").as_bytes()).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    // The old ticket now names a slot that is in use and held -- by someone else. It is
    // refused, and `c` is not answered by it.
    b.write_all(get("/replay/0").as_bytes()).unwrap();
    assert_eq!(read_until(&mut b, ok("2").len()), ok("2"));
    silent(&mut c);
    h.release();
    assert_eq!(read_until(&mut c, ok("released").len()), ok("released"));
}
