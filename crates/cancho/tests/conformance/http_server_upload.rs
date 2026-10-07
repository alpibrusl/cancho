//! `packages/http-server`'s streaming request bodies on the socket path (`docs/http-server.md` §12): the program
//! `tests/programs/server_upload.cho` serves uploads over real sockets, so `step`, `settle`, `wait` and the timers are
//! exercised as well as the byte-fed path `http_server_body.rs` covers without any. The server is built from the package's
//! source (not the store: the store is checked on its own, in `http_server.rs`).

use super::*;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

struct Upload {
    dir: PathBuf,
    child: std::process::Child,
    port: u16,
}

impl Upload {
    fn start(tag: &str, size: usize, max_body: usize, head_ms: u64, body_ms: u64) -> Upload {
        Upload::start_with(tag, size, max_body, head_ms, body_ms, 0)
    }

    /// `lazy` 1: the program calls `next` only every other time round its loop.
    fn start_with(
        tag: &str,
        size: usize,
        max_body: usize,
        head_ms: u64,
        body_ms: u64,
        lazy: u8,
    ) -> Upload {
        // (`scripts/http_server_body_mutants.py --socket` builds the server from a mutated copy of the source.)
        let source = std::env::var("HTTP_SERVER_SOURCE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| repo_root().join("packages/http-server/server.cho"));
        let paths = vec![repo_root().join("tests/programs/server_upload.cho"), source];
        let (dir, exe) = build_example_paths(tag, &paths, "server_upload");
        let port = free_port();
        let child = Command::new(&exe)
            .args([
                port.to_string(),
                size.to_string(),
                max_body.to_string(),
                head_ms.to_string(),
                body_ms.to_string(),
                lazy.to_string(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the compiled server runs");
        Upload { dir, child, port }
    }

    fn client(&self) -> TcpStream {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match TcpStream::connect(("127.0.0.1", self.port)) {
                Ok(s) => {
                    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
                    return s;
                }
                Err(e) => {
                    assert!(Instant::now() < deadline, "never listened: {e}");
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }
}

impl Drop for Upload {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The bytes of a test body: byte `i` of stream `seed`.
fn byte_at(i: usize, seed: usize) -> u8 {
    (97 + (i * 7 + i / 13 + seed) % 26) as u8
}

fn body_of(n: usize, seed: usize) -> Vec<u8> {
    (0..n).map(|i| byte_at(i, seed)).collect()
}

fn checksum(body: &[u8]) -> u64 {
    body.iter().fold(0u64, |c, &b| (c * 131 + b as u64) % 1_000_000_007)
}

fn tally(body: &[u8]) -> String {
    format!("n={} c={}", body.len(), checksum(body))
}

/// One response's head (to the blank line, inclusive), or what arrived before the connection ended.
fn read_head(s: &mut TcpStream) -> String {
    let mut got = Vec::new();
    let mut byte = [0u8; 1];
    while !got.ends_with(b"\r\n\r\n") {
        match s.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => got.push(byte[0]),
            Err(e) => panic!("{e}: after {:?}", String::from_utf8_lossy(&got)),
        }
    }
    String::from_utf8_lossy(&got).into_owned()
}

fn content_length(head: &str) -> usize {
    head.lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .map(|v| v.trim().parse().unwrap())
        .unwrap_or(0)
}

/// A response: its head and its body. A `100 Continue` is returned as it is, with an empty body.
fn read_response(s: &mut TcpStream) -> (String, String) {
    let head = read_head(s);
    let mut body = vec![0u8; content_length(&head)];
    s.read_exact(&mut body).unwrap_or_else(|e| panic!("{e}: after {head:?}"));
    (head, String::from_utf8_lossy(&body).into_owned())
}

/// Does the connection end (within the read timeout) with nothing more to read?
fn ends(s: &mut TcpStream) -> bool {
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let mut b = [0u8; 64];
    loop {
        match s.read(&mut b) {
            Ok(0) => return true,
            Ok(_) => {}
            Err(e) if matches!(e.kind(), ErrorKind::ConnectionReset | ErrorKind::BrokenPipe) => {
                return true;
            }
            Err(_) => return false,
        }
    }
}

fn post(path: &str, length: usize, extra: &str) -> String {
    format!("POST {path} HTTP/1.1\r\nHost: t\r\nContent-Length: {length}\r\n{extra}\r\n")
}

fn chunked(body: &[u8], piece: usize, ext: &str, trailers: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for c in body.chunks(piece) {
        out.extend(format!("{:x}{ext}\r\n", c.len()).bytes());
        out.extend(c);
        out.extend(b"\r\n");
    }
    out.extend(format!("0{ext}\r\n{trailers}\r\n").bytes());
    out
}

#[test]
fn uploads_arrive_byte_for_byte_over_sockets_with_100_continue_pipelining_and_keep_alive() {
    let server = Upload::start("hs-upload", 4096, 64 << 20, 2000, 2000);
    let mut s = server.client();
    // A length body of 3 MiB, the client waiting for `100 Continue`: the server answers it only because the application proceeds.
    let body = body_of(3 << 20, 1);
    s.write_all(post("/upload", body.len(), "Expect: 100-continue\r\n").as_bytes()).unwrap();
    assert!(read_head(&mut s).starts_with("HTTP/1.1 100 Continue\r\n\r\n"));
    s.write_all(&body).unwrap();
    let (head, text) = read_response(&mut s);
    assert!(head.starts_with("HTTP/1.1 200 "), "{head}");
    assert_eq!(text, tally(&body));
    // The same connection: chunked, with extensions and a trailer, in pieces the buffer (4096) does not line up with.
    let body = body_of(1 << 20, 2);
    s.write_all(b"POST /upload HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
    s.write_all(&chunked(&body, 777, ";k=v", "X-T: 1\r\n")).unwrap();
    assert_eq!(read_response(&mut s).1, tally(&body));
    // Without waiting: head and body in one write, and a request pipelined behind it in the same write.
    let body = body_of(100_000, 3);
    let mut all = post("/upload", body.len(), "").into_bytes();
    all.extend(&body);
    all.extend(b"GET /whole HTTP/1.1\r\nHost: t\r\n\r\nPOST /whole HTTP/1.1\r\nHost: t\r\nContent-Length: 5\r\n\r\nhello");
    s.write_all(&all).unwrap();
    assert_eq!(read_response(&mut s).1, tally(&body));
    assert_eq!(read_response(&mut s).1, "n=0 c=0");
    assert_eq!(read_response(&mut s).1, tally(b"hello"));
    // A body of nothing, and one that fits: not streamed at all.
    s.write_all(post("/upload", 0, "").as_bytes()).unwrap();
    assert_eq!(read_response(&mut s).1, "n=0 c=0");
    // Four uploads at the same time on four connections, each its own bytes: every one arrives whole, not mixed with its neighbours'.
    let port = server.port;
    let threads: Vec<_> = (0..4usize)
        .map(|i| {
            std::thread::spawn(move || {
                let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
                c.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
                let body = body_of((2 << 20) + i * 1001, 10 + i);
                c.write_all(post("/upload", body.len(), "").as_bytes()).unwrap();
                for part in body.chunks(7919) {
                    c.write_all(part).unwrap();
                }
                (read_response(&mut c).1, tally(&body))
            })
        })
        .collect();
    for t in threads {
        let (got, want) = t.join().unwrap();
        assert_eq!(got, want);
    }
    // Sizes around the buffer, on fresh connections, all at once.
    for n in [1usize, 4095, 4096, 4097, 8192, 65535, 65536, 65537] {
        let mut c = server.client();
        let body = body_of(n, n);
        let mut all = post("/upload", n, "").into_bytes();
        all.extend(&body);
        c.write_all(&all).unwrap();
        assert_eq!(read_response(&mut c).1, tally(&body), "{n} bytes");
    }
}

#[test]
fn a_refusal_before_the_body_sends_no_100_and_the_faults_are_refused_with_their_rules() {
    let server = Upload::start("hs-upload-refuse", 4096, 1 << 20, 2000, 2000);
    let mut s = server.client();
    // Refused on the head: no `100`, and the client never sent the body, so the connection goes on.
    s.write_all(post("/refuse", 500_000, "Expect: 100-continue\r\n").as_bytes()).unwrap();
    let (head, text) = read_response(&mut s);
    assert!(head.starts_with("HTTP/1.1 413 "), "{head}");
    assert!(text.contains("refused"));
    s.write_all(b"GET /whole HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
    assert_eq!(read_response(&mut s).1, "n=0 c=0");
    // The same without `Expect`, and a client that sends the body anyway: refused, and the connection closes.
    let mut c = server.client();
    c.write_all(post("/refuse", 1 << 20, "").as_bytes()).unwrap();
    c.write_all(&vec![b'x'; 5000]).unwrap();
    assert!(read_head(&mut c).starts_with("HTTP/1.1 413 "));
    // (The body is not part of the 413; what is left of it is read and dropped.)
    let _ = ends(&mut c);
    // A length over the server's maximum (1 MiB) is refused by the server, with its rule, before a `100`.
    let mut d = server.client();
    d.write_all(post("/upload", (1 << 20) + 1, "Expect: 100-continue\r\n").as_bytes()).unwrap();
    let (head, _) = read_response(&mut d);
    assert!(head.starts_with("HTTP/1.1 413 ") && head.contains("X-Rule: body.too-large"), "{head}");
    assert!(!head.contains("100 Continue"));
    assert!(ends(&mut d));
    // Another expectation: 417.
    let mut e = server.client();
    e.write_all(post("/upload", 10, "Expect: gimme\r\n").as_bytes()).unwrap();
    let (head, _) = read_response(&mut e);
    assert!(
        head.starts_with("HTTP/1.1 417 ") && head.contains("X-Rule: expect.unsupported"),
        "{head}"
    );
    // Bad chunking in the middle of an upload: 400 with the rule, and the server serves the next client.
    for (wire, rule) in [
        (&b"zz\r\n"[..], "body.chunk-size"),
        (&b"5\r\nhelloXX"[..], "body.chunk-framing"),
        (&b"0\r\nT: v\nU\r\n\r\n"[..], "body.trailer"),
        (&b"fffffff\r\n"[..], "body.too-large"),
    ] {
        let mut f = server.client();
        f.write_all(b"POST /upload HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n")
            .unwrap();
        f.write_all(wire).unwrap();
        let (head, _) = read_response(&mut f);
        assert!(head.contains(&format!("X-Rule: {rule}")), "{rule}: {head}");
        assert!(ends(&mut f));
    }
    // Answered before the body is read, and the client still sending: the answer comes, and the connection closes.
    let mut g = server.client();
    g.write_all(post("/early", 1 << 20, "").as_bytes()).unwrap();
    g.write_all(&vec![b'y'; 10_000]).unwrap();
    assert_eq!(read_response(&mut g).1, "early");
    assert!(ends(&mut g));
    let mut h = server.client();
    h.write_all(b"GET /whole HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
    assert_eq!(read_response(&mut h).1, "n=0 c=0");
}

#[test]
fn a_client_that_stalls_in_a_head_or_a_body_is_timed_out_and_the_others_are_served() {
    let server = Upload::start("hs-upload-timers", 4096, 1 << 20, 600, 600);
    // A head that never ends, and one that is trickled; a body that stops; all at once.
    let mut head = server.client();
    head.write_all(b"POST /upload HTTP/1.1\r\nHo").unwrap();
    let mut trickle = server.client();
    let mut stalled = server.client();
    stalled.write_all(post("/upload", 100_000, "").as_bytes()).unwrap();
    stalled.write_all(&vec![b'z'; 1000]).unwrap();
    let started = Instant::now();
    for byte in b"GET /t HTTP/1.1\r\nHost: t\r\n".iter().take(10) {
        let _ = trickle.write_all(&[*byte]);
        // The others are served throughout, whatever these three are doing.
        let t = Instant::now();
        let mut ok = server.client();
        ok.write_all(b"GET /whole HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
        assert_eq!(read_response(&mut ok).1, "n=0 c=0");
        assert!(t.elapsed() < Duration::from_millis(400), "{:?}", t.elapsed());
        std::thread::sleep(Duration::from_millis(100));
    }
    for (name, c, rule) in [
        ("head", &mut head, "timeout.head"),
        ("trickle", &mut trickle, "timeout.head"),
        ("body", &mut stalled, "timeout.body"),
    ] {
        let _ = c.set_read_timeout(Some(Duration::from_secs(5)));
        let (h, _) = read_response(c);
        assert!(
            h.starts_with("HTTP/1.1 408 ") && h.contains(&format!("X-Rule: {rule}")),
            "{name}: {h}"
        );
        assert!(ends(c), "{name}");
    }
    // Within a sane time of the deadline (600 ms) and not before it.
    assert!(
        started.elapsed() > Duration::from_millis(600)
            && started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_body_the_application_takes_slowly_stops_being_read_and_a_client_that_leaves_costs_nothing() {
    let server = Upload::start("hs-upload-slow", 4096, 1 << 30, 5000, 5000);
    // `/slow` takes 50 bytes a turn of 20 ms: the kernel's buffers fill and the client's writes block. What the server buffers is
    // 4096 bytes, so the client got no more than the sockets' own buffers in.
    let mut s = server.client();
    s.write_all(post("/slow", 256 << 20, "").as_bytes()).unwrap();
    s.set_nonblocking(true).unwrap();
    let chunk = vec![b'q'; 65536];
    let mut written = 0usize;
    let mut blocked_for = Duration::ZERO;
    let started = Instant::now();
    while blocked_for < Duration::from_millis(500) && written < (200 << 20) {
        match s.write(&chunk) {
            Ok(n) => {
                written += n;
                blocked_for = Duration::ZERO;
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
                blocked_for += Duration::from_millis(50);
            }
            Err(e) => panic!("{e}"),
        }
    }
    assert!(
        blocked_for >= Duration::from_millis(500),
        "never blocked: {written} bytes in {:?}",
        started.elapsed()
    );
    assert!(
        written < (64 << 20),
        "{written} bytes were taken by a server that takes 2,500 a second"
    );
    // The others are served meanwhile.
    let mut ok = server.client();
    ok.write_all(b"GET /whole HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
    assert_eq!(read_response(&mut ok).1, "n=0 c=0");
    // The client leaves in the middle: nothing is left behind. Twenty more do the same, and the server serves after.
    drop(s);
    for i in 0..20 {
        let mut c = server.client();
        c.write_all(post("/upload", 1 << 20, "").as_bytes()).unwrap();
        c.write_all(&body_of(10_000 + i, i)).unwrap();
        drop(c);
    }
    let mut ok = server.client();
    ok.write_all(post("/upload", 5, "").as_bytes()).unwrap();
    ok.write_all(b"hello").unwrap();
    assert_eq!(read_response(&mut ok).1, tally(b"hello"));
}

#[test]
fn a_refusal_found_while_reading_is_sent_even_when_the_application_is_slow_to_call_next() {
    // The program visits connections every tenth turn, so a fault `wait` finds waits through turns in which the client sends more.
    let server = Upload::start_with("hs-upload-lazy", 4096, 1 << 20, 2000, 2000, 1);
    for (wire, rule) in
        [(&b"zz\r\n"[..], "body.chunk-size"), (&b"5\r\nhelloXX"[..], "body.chunk-framing")]
    {
        let mut c = server.client();
        c.write_all(b"POST /upload HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n")
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        c.write_all(wire).unwrap();
        for _ in 0..20 {
            // More bytes, while the program is not looking.
            if c.write_all(b"0123456789").is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(3));
        }
        let (head, _) = read_response(&mut c);
        assert!(
            head.starts_with("HTTP/1.1 400 ") && head.contains(&format!("X-Rule: {rule}")),
            "{rule}: {head}"
        );
        assert!(ends(&mut c));
    }
    let mut ok = server.client();
    ok.write_all(post("/upload", 5, "").as_bytes()).unwrap();
    ok.write_all(b"hello").unwrap();
    assert_eq!(read_response(&mut ok).1, tally(b"hello"));
}

#[test]
fn a_connection_that_reads_never_writes_into_its_neighbours_buffer() {
    // Connection F (slot 0) uploads in small writes; connection S (slot 1) is slow and has a full buffer of untaken bytes. Whatever F's reads do
    // to the room `body_take` gave back, S's body arrives whole.
    let server = Upload::start("hs-upload-neighbour", 4096, 64 << 20, 5000, 5000);
    let mut f = server.client();
    f.write_all(post("/upload", 300_000, "").as_bytes()).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let mut s = server.client();
    let slow = body_of(6000, 99);
    s.write_all(post("/slow", slow.len(), "").as_bytes()).unwrap();
    s.write_all(&slow).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let fast = body_of(300_000, 7);
    for part in fast.chunks(1000) {
        f.write_all(part).unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(read_response(&mut f).1, tally(&fast));
    assert_eq!(read_response(&mut s).1, tally(&slow));
}
