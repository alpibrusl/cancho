//! `examples/api/` (`docs/server.md`): the server loop over real sockets.
//!
//! Every test starts the compiled server on a port the operating system
//! said was free, talks to it from this process, and kills it. The things
//! worth testing about a server are not the routes (those are `std.route`'s
//! tests) but what a loop around them can get wrong: a request split across
//! segments, several in one, a client that goes silent, a client that
//! vanishes, and a refusal that must close the connection.

use super::*;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

struct Server {
    child: std::process::Child,
    port: u16,
    dir: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn start(tag: &str, extra: &[&str]) -> Server {
    let (dir, exe) = build_example_paths(tag, &[repo_root().join("examples/api/api.ls")], "api");
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let child = Command::new(&exe)
        .arg(port.to_string())
        .args(extra)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the compiled server runs");
    let server = Server { child, port, dir };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(_) => return server,
            Err(e) if Instant::now() < deadline => {
                let _ = e;
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("the server never listened on {port}: {e}"),
        }
    }
}

fn connect(server: &Server) -> TcpStream {
    let stream = TcpStream::connect(("127.0.0.1", server.port)).expect("a connection");
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    stream
}

struct Response {
    status: u16,
    head: String,
    body: String,
}

/// One response off `stream`, read by its `Content-Length`; `None` at EOF.
fn read_response(stream: &mut TcpStream, carry: &mut Vec<u8>) -> Option<Response> {
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(end) = carry.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&carry[..end]).to_string();
            let length = head
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length: "))
                .map(|v| v.parse::<usize>().unwrap())
                .expect("every response declares its length");
            while carry.len() < end + 4 + length {
                let n = stream.read(&mut chunk).expect("the rest of the body");
                assert!(n > 0, "the connection closed inside a body");
                carry.extend(&chunk[..n]);
            }
            let body = String::from_utf8_lossy(&carry[end + 4..end + 4 + length]).to_string();
            carry.drain(..end + 4 + length);
            let status = head.split(' ').nth(1).unwrap().parse().unwrap();
            return Some(Response { status, head, body });
        }
        match stream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => carry.extend(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => return None,
            Err(e) => panic!("reading a response: {e}"),
        }
    }
}

fn ask(stream: &mut TcpStream, request: &str) -> Response {
    stream.write_all(request.as_bytes()).unwrap();
    read_response(stream, &mut Vec::new()).expect("an answer")
}

fn get(path: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: t\r\n\r\n")
}

fn post(path: &str, body: &str) -> String {
    format!("POST {path} HTTP/1.1\r\nHost: t\r\nContent-Length: {}\r\n\r\n{body}", body.len())
}

#[test]
fn the_routes_answer() {
    let server = start("api-routes", &[]);
    let mut s = connect(&server);
    let r = ask(&mut s, &get("/health"));
    assert_eq!((r.status, r.body.as_str()), (200, "{\"ok\":true}"));
    assert!(r.head.contains("Content-Type: application/json"));
    assert!(r.head.contains("Connection: keep-alive"));
    let r = ask(&mut s, &get("/users/42"));
    assert_eq!((r.status, r.body.as_str()), (200, "{\"id\":42,\"name\":\"user-42\"}"));
    let r = ask(&mut s, &get("/users/abc"));
    assert_eq!((r.status, r.body.as_str()), (400, "{\"error\":\"id must be a number\"}"));
    let r = ask(&mut s, &get("/users/"));
    assert_eq!(r.status, 404);
    let r = ask(&mut s, &get("/search?q=a%20b+c%C3%A9"));
    assert_eq!((r.status, r.body.as_str()), (200, "{\"q\":\"a b cé\",\"length\":7}"));
    let r = ask(&mut s, &get("/search"));
    assert_eq!((r.status, r.body.as_str()), (400, "{\"error\":\"q is required\"}"));
    let r = ask(&mut s, &get("/search?q=%zz"));
    assert_eq!(r.status, 400);
    let r = ask(&mut s, &post("/add", "{\"a\": 40, \"b\": 2}"));
    assert_eq!((r.status, r.body.as_str()), (200, "{\"sum\":42}"));
    let r = ask(&mut s, &post("/add", "{\"a\": 1.5, \"b\": 2}"));
    assert_eq!((r.status, r.body.as_str()), (422, "{\"error\":\"a and b must be integers\"}"));
    let r = ask(&mut s, &post("/add", "{\"a\": 1"));
    assert_eq!(r.status, 422);
    let r = ask(&mut s, &post("/health", ""));
    assert_eq!((r.status, r.body.as_str()), (405, "{\"error\":\"method not allowed\"}"));
    // RFC 9110 §15.5.6: a 405 says what the path does allow, and a path with two
    // methods says both.
    assert!(r.head.contains("\r\nAllow: GET\r\n") || r.head.ends_with("Allow: GET"), "{}", r.head);
    let r = ask(&mut s, &get("/add"));
    assert_eq!(r.status, 405);
    assert!(r.head.contains("Allow: POST"), "{}", r.head);
    // A 404 has none: no route has the path.
    let r = ask(&mut s, &get("/nothing"));
    assert!(!r.head.contains("Allow:"), "{}", r.head);
    let r = ask(&mut s, &get("/nothing"));
    assert_eq!((r.status, r.body.as_str()), (404, "{\"error\":\"not found\"}"));
}

#[test]
fn one_connection_serves_many_requests_and_pipelined_ones_in_order() {
    let server = start("api-keepalive", &[]);
    let mut s = connect(&server);
    for i in 0..60 {
        let r = ask(&mut s, &get(&format!("/users/{i}")));
        assert_eq!(r.body, format!("{{\"id\":{i},\"name\":\"user-{i}\"}}"));
    }
    // Thirty requests in one write: thirty answers, in order.
    let mut all = String::new();
    for i in 0..30 {
        all.push_str(&get(&format!("/users/{}", 1000 + i)));
    }
    s.write_all(all.as_bytes()).unwrap();
    let mut carry = Vec::new();
    for i in 0..30 {
        let r = read_response(&mut s, &mut carry).expect("an answer");
        assert_eq!(r.body, format!("{{\"id\":{},\"name\":\"user-{}\"}}", 1000 + i, 1000 + i));
    }
}

#[test]
fn a_request_split_across_segments_is_answered_once_whole() {
    let server = start("api-split", &[]);
    let mut s = connect(&server);
    s.set_nodelay(true).unwrap();
    let body = "{\"a\": 5, \"b\": 6}";
    let request = post("/add", body);
    // Cut it inside the request line, inside a header, at the blank line, and
    // inside the body: each piece is its own segment.
    let cuts = [5, 22, request.len() - body.len() - 2, request.len() - 6];
    let mut from = 0;
    for cut in cuts {
        s.write_all(&request.as_bytes()[from..cut]).unwrap();
        std::thread::sleep(Duration::from_millis(40));
        from = cut;
    }
    s.write_all(&request.as_bytes()[from..]).unwrap();
    let r = read_response(&mut s, &mut Vec::new()).expect("an answer");
    assert_eq!((r.status, r.body.as_str()), (200, "{\"sum\":11}"));
    // And the connection is still good.
    assert_eq!(ask(&mut s, &get("/health")).status, 200);
}

#[test]
fn the_start_of_the_next_request_after_a_whole_one_is_kept_and_completed() {
    // One read holding a whole request and the front of the next: the first is
    // answered, the rest is moved to the start of the buffer, and the other
    // half, arriving later, completes it. Without that move the second request
    // is parsed as its own tail.
    let server = start("api-carry", &[]);
    let mut s = connect(&server);
    s.set_nodelay(true).unwrap();
    let mut carry = Vec::new();
    for round in 0..20 {
        let first = get(&format!("/users/{round}"));
        let second = post("/add", &format!("{{\"a\": {round}, \"b\": 100}}"));
        let cut = 11 + round % 7;
        let mut together = first.clone().into_bytes();
        together.extend(&second.as_bytes()[..cut]);
        s.write_all(&together).unwrap();
        let r = read_response(&mut s, &mut carry).expect("the first answer");
        assert_eq!(r.body, format!("{{\"id\":{round},\"name\":\"user-{round}\"}}"));
        std::thread::sleep(Duration::from_millis(30));
        s.write_all(&second.as_bytes()[cut..]).unwrap();
        let r = read_response(&mut s, &mut carry).expect("the second answer");
        assert_eq!(r.body, format!("{{\"sum\":{}}}", round + 100), "round {round}");
    }
}

#[test]
fn closing_a_connection_moves_the_last_one_with_its_buffered_bytes() {
    // Connections are kept in a dense array and a closed one is replaced by
    // the last. The last may be holding the front of a request; it must still
    // be holding it afterwards, and still be the same client's.
    let server = start("api-swap", &[]);
    for round in 0..30 {
        let early = connect(&server);
        let mut late = connect(&server);
        late.set_nodelay(true).unwrap();
        let request = get(&format!("/users/{round}"));
        late.write_all(&request.as_bytes()[..12]).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        // `early` goes: the server moves `late` into its slot.
        drop(early);
        std::thread::sleep(Duration::from_millis(30));
        late.write_all(&request.as_bytes()[12..]).unwrap();
        let r = read_response(&mut late, &mut Vec::new()).expect("an answer");
        assert_eq!(
            r.body,
            format!("{{\"id\":{round},\"name\":\"user-{round}\"}}"),
            "round {round}"
        );
    }
}

#[test]
fn what_cannot_be_trusted_is_refused_and_the_connection_closes() {
    let server = start("api-refuse", &[]);
    let cases: Vec<(&str, String, u16)> = vec![
        ("both lengths", "POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 4\r\nTransfer-Encoding: chunked\r\n\r\n{}{}".into(), 400),
        ("line folding", "GET /health HTTP/1.1\r\nHost: t\r\nX: a\r\n b\r\n\r\n".into(), 400),
        ("space before colon", "GET /health HTTP/1.1\r\nHost : t\r\n\r\n".into(), 400),
        ("no host", "GET /health HTTP/1.1\r\n\r\n".into(), 400),
        ("garbage method", "G@T /health HTTP/1.1\r\nHost: t\r\n\r\n".into(), 400),
        ("version", "GET /health HTTP/2.0\r\nHost: t\r\n\r\n".into(), 400),
        ("two lengths", "POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 2\r\nContent-Length: 3\r\n\r\n{}".into(), 400),
        ("chunked", "POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n".into(), 501),
        ("body too large", "POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 100000\r\n\r\n".into(), 413),
        ("head too large", format!("GET /health HTTP/1.1\r\nHost: t\r\nX: {}\r\n\r\n", "a".repeat(20_000)), 431),
    ];
    for (name, request, status) in cases {
        let mut s = connect(&server);
        // The big ones may be refused before the whole request is written.
        let _ = s.write_all(request.as_bytes());
        let mut carry = Vec::new();
        let r = read_response(&mut s, &mut carry).unwrap_or_else(|| panic!("{name}: no answer"));
        assert_eq!(r.status, status, "{name}: {}", r.body);
        assert!(r.head.contains("Connection: close"), "{name}");
        assert!(r.body.starts_with("{\"error\":"), "{name}");
        // Closed: the next read is the end of the stream (or a reset, if the
        // server closed with the rest of a large request unread).
        assert!(read_response(&mut s, &mut carry).is_none(), "{name}: still open");
    }
    // None of that hurt the server.
    assert_eq!(ask(&mut connect(&server), &get("/health")).status, 200);
}

#[test]
fn connection_close_is_honoured() {
    let server = start("api-close", &[]);
    let mut s = connect(&server);
    let r = ask(&mut s, "GET /health HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n");
    assert_eq!(r.status, 200);
    assert!(r.head.contains("Connection: close"));
    assert!(read_response(&mut s, &mut Vec::new()).is_none());
    // HTTP/1.0 closes unless it asks to stay.
    let mut s = connect(&server);
    s.write_all(b"GET /health HTTP/1.0\r\n\r\n").unwrap();
    let r = read_response(&mut s, &mut Vec::new()).expect("an answer");
    assert!(r.head.contains("Connection: close"));
    assert!(read_response(&mut s, &mut Vec::new()).is_none());
}

#[test]
fn a_silent_client_does_not_hold_up_the_others() {
    let server = start("api-silent", &[]);
    // Fifty connections that open and say nothing, one that sends half a head.
    let idle: Vec<TcpStream> = (0..50).map(|_| connect(&server)).collect();
    let mut half = connect(&server);
    half.write_all(b"GET /health HTT").unwrap();
    // Everyone else is served promptly all the while.
    let started = Instant::now();
    for i in 0..40 {
        assert_eq!(ask(&mut connect(&server), &get(&format!("/users/{i}"))).status, 200);
    }
    assert!(started.elapsed() < Duration::from_secs(3), "the loop waited on a silent client");
    drop(idle);
    drop(half);
}

#[test]
fn many_clients_at_once_each_get_their_own_answers() {
    let server = start("api-many", &[]);
    let port = server.port;
    let threads: Vec<_> = (0..64)
        .map(|t| {
            std::thread::spawn(move || {
                let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
                s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
                let mut carry = Vec::new();
                for i in 0..40 {
                    let id = t * 1000 + i;
                    let request = if i % 4 == 3 {
                        post("/add", &format!("{{\"a\": {id}, \"b\": 1}}"))
                    } else {
                        get(&format!("/users/{id}"))
                    };
                    s.write_all(request.as_bytes()).unwrap();
                    let r = read_response(&mut s, &mut carry).expect("an answer");
                    let want = if i % 4 == 3 {
                        format!("{{\"sum\":{}}}", id + 1)
                    } else {
                        format!("{{\"id\":{id},\"name\":\"user-{id}\"}}")
                    };
                    assert_eq!(r.body, want, "client {t} request {i}");
                }
            })
        })
        .collect();
    for t in threads {
        t.join().expect("a client saw a wrong answer");
    }
}

#[test]
fn clients_that_vanish_do_not_hurt_the_server() {
    let server = start("api-vanish", &[]);
    for round in 0..150 {
        let mut s = connect(&server);
        match round % 3 {
            // Ask and leave without reading: the answer is written to a
            // closed socket, which would be SIGPIPE.
            0 => {
                let _ = s.write_all(get("/health").as_bytes());
            }
            // Leave in the middle of a head.
            1 => {
                let _ = s.write_all(b"GET /hea");
            }
            // Leave in the middle of a body.
            _ => {
                let _ = s.write_all(
                    b"POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 50\r\n\r\n{\"a\"",
                );
            }
        }
        drop(s);
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(ask(&mut connect(&server), &get("/health")).status, 200);
}

#[test]
fn an_idle_connection_is_closed_after_the_timeout() {
    // `api <port> reuseport 1`: one second of silence.
    let server = start("api-idle", &["0", "1"]);
    let mut quiet = connect(&server);
    let mut busy = connect(&server);
    let started = Instant::now();
    // The busy one keeps talking and is never closed.
    for _ in 0..5 {
        assert_eq!(ask(&mut busy, &get("/health")).status, 200);
        std::thread::sleep(Duration::from_millis(600));
    }
    // The quiet one has been closed in the meantime.
    assert!(read_response(&mut quiet, &mut Vec::new()).is_none(), "an idle connection stayed open");
    assert!(started.elapsed() < Duration::from_secs(15));
    assert_eq!(ask(&mut busy, &get("/health")).status, 200);
}

// ---- writes that do not block ---------------------------------------------

/// How many bytes the kernel will queue on a loopback connection whose far end
/// never reads: a writer that is never refused until the buffers are full, and
/// then refused. Linux holds a few megabytes; macOS may hold far more, and the
/// tests that need a client to be *stalled* must send more than that, so they
/// ask rather than assume.
fn loopback_capacity() -> usize {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let writer = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let (_reader, _) = listener.accept().unwrap();
    writer.set_nonblocking(true).unwrap();
    let chunk = vec![0u8; 65536];
    let mut total = 0usize;
    let mut refused = 0;
    let mut writer = writer;
    // Refused twice in a row, 150 ms apart, means full: the first refusal can be
    // the buffer waiting to grow.
    while refused < 2 && total < 1 << 30 {
        match writer.write(&chunk) {
            Ok(n) => {
                total += n;
                refused = 0;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                refused += 1;
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(e) => panic!("measuring the loopback buffers: {e}"),
        }
    }
    total
}

/// How many 32 KiB answers it takes to be sure of filling `capacity` twice over,
/// and at least a thousand.
fn requests_to_stall(capacity: usize) -> usize {
    (2 * capacity / 32768 + 200).max(1000)
}

/// `count` requests for blobs, as one string to write in one go.
fn blob_requests(sizes: impl Iterator<Item = usize>) -> String {
    sizes.map(|n| get(&format!("/blob/{n}"))).collect()
}

#[test]
fn a_client_that_stops_reading_stalls_only_itself() {
    // Enough requests for 32 KiB each that the answers are more than twice what
    // the kernel will hold for a client that reads none of it. A server that
    // *blocks* in `write` stops dead there; this one parks the rest of that
    // connection's answers and serves everyone else.
    let capacity = loopback_capacity();
    let count = requests_to_stall(capacity);
    let server = start("api-stall", &[]);
    let mut slow = connect(&server);
    slow.write_all(blob_requests((0..count).map(|_| 32768)).as_bytes()).unwrap();
    std::thread::sleep(Duration::from_millis(500));

    let started = Instant::now();
    for i in 0..30 {
        let mut other = connect(&server);
        other.write_all(get(&format!("/users/{i}")).as_bytes()).unwrap();
        let r = read_response(&mut other, &mut Vec::new()).unwrap_or_else(|| {
            panic!("request {i} on another connection got no answer ({count} requests, loopback holds {capacity} bytes)")
        });
        assert_eq!(r.status, 200);
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the loop waited on a client that was not reading"
    );

    // And the stalled client loses nothing: it now reads every answer, whole
    // and in order.
    let mut carry = Vec::new();
    for i in 0..count {
        let r = read_response(&mut slow, &mut carry).unwrap_or_else(|| {
            panic!("answer {i} of {count} missing (loopback holds {capacity} bytes)")
        });
        assert_eq!(r.status, 200, "answer {i}");
        assert_eq!(r.body.len(), 32768, "answer {i} of {count}");
    }
}

#[test]
fn a_slow_reader_gets_every_byte_in_the_right_order() {
    // Answers of a thousand different sizes, read a little at a time. Each time
    // the kernel takes only part of one, the rest waits in the connection's
    // output buffer and is sent when `poll` says there is room: a byte
    // dropped, repeated or reordered there changes a length.
    // `1500` is the fourth argument: no `send` is handed more than that, so
    // every answer is sent in pieces and the rest of a partial one has to be
    // resumed. On Linux loopback a `send` is otherwise whole or refused and
    // that path never runs.
    let server = start("api-slow", &["0", "9", "1500"]);
    let sizes: Vec<usize> = (0..1000).map(|i| 1 + (i * 7919) % 32768).collect();
    let mut slow = connect(&server);
    slow.write_all(blob_requests(sizes.iter().copied()).as_bytes()).unwrap();
    let mut carry = Vec::new();
    for (i, &size) in sizes.iter().enumerate() {
        let r =
            read_response(&mut slow, &mut carry).unwrap_or_else(|| panic!("answer {i} missing"));
        assert_eq!((r.status, r.body.len()), (200, size), "answer {i}");
        assert!(
            r.body.bytes().enumerate().all(|(at, b)| b == b'a' + (at % 26) as u8),
            "answer {i} has the wrong bytes"
        );
        if i % 50 == 0 {
            std::thread::sleep(Duration::from_millis(20));
            // Others are served in the middle of it.
            assert_eq!(ask(&mut connect(&server), &get("/health")).status, 200);
        }
    }
}

#[test]
fn a_client_that_never_reads_is_closed_after_the_idle_timeout() {
    // Two seconds without progress. The connection has output waiting and no
    // `POLLIN` interest, so nothing it sends can keep it alive.
    let capacity = loopback_capacity();
    let count = requests_to_stall(capacity);
    let server = start("api-never-reads", &["0", "2"]);
    let mut s = connect(&server);
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    s.write_all(blob_requests((0..count).map(|_| 32768)).as_bytes()).unwrap();
    std::thread::sleep(Duration::from_secs(5));
    // Whatever the kernel was holding comes out, and then the connection ends
    // -- by end of stream, or by a reset, which is what closing a socket with
    // unread input sends. It must not still be open.
    let mut total = 0usize;
    let mut buf = vec![0u8; 65536];
    loop {
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                break;
            }
            Err(e) => {
                panic!("the connection was still open after the timeout ({total} bytes read): {e}")
            }
        }
    }
    assert!(
        total < count * 32768,
        "every answer ({count} of them, {total} bytes) was delivered to a client that was not reading; loopback holds {capacity} bytes"
    );
    assert_eq!(ask(&mut connect(&server), &get("/health")).status, 200);
}

#[test]
fn a_connection_that_is_to_close_closes_only_once_its_answer_has_gone() {
    // `Connection: close` on a 20 KB answer, with `send` limited to 1500 bytes
    // at a time: the answer goes out in pieces over several `poll` rounds, and
    // the connection must stay open until the last of them -- closing at the
    // first partial send would cut the answer short.
    let server = start("api-close-large", &["0", "30", "1500"]);
    let mut s = connect(&server);
    s.write_all(b"GET /blob/20000 HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n").unwrap();
    let mut carry = Vec::new();
    let r = read_response(&mut s, &mut carry).expect("an answer");
    assert_eq!((r.status, r.body.len()), (200, 20000));
    assert!(r.head.contains("Connection: close"));
    assert!(read_response(&mut s, &mut carry).is_none(), "the connection stayed open");
    // And so does a refusal that is bigger than one piece: 20,000 bytes of header.
    let mut s = connect(&server);
    let _ = s.write_all(
        format!("GET /health HTTP/1.1\r\nHost: t\r\nX: {}\r\n\r\n", "a".repeat(20_000)).as_bytes(),
    );
    let r = read_response(&mut s, &mut Vec::new()).expect("a refusal");
    assert_eq!(r.status, 431);
}

#[test]
fn the_server_announces_its_port_on_standard_error() {
    // On the unbuffered stream: a piped standard output is held until the
    // process ends, and a server that announces itself only then has not
    // announced itself. No send flag is announced any more -- there is none:
    // `conn_write` picks the system's way of not waiting (`native-sockets.md`).
    let (dir, exe) =
        build_example_paths("api-flag", &[repo_root().join("examples/api/api.ls")], "api");
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut child = Command::new(&exe)
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the compiled server runs");
    let mut line = String::new();
    {
        use std::io::BufRead;
        let mut out = std::io::BufReader::new(child.stderr.take().unwrap());
        out.read_line(&mut line).expect("a first line");
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(line.trim(), format!("listening on {port}"));
}

#[test]
fn the_server_holds_no_foreign_authority() {
    // The point of `docs/native-sockets.md`: the same server that reported
    // `ffi("libc")` -- "may call anything in libc" -- now reports exactly what
    // it does. The port is an argument, so `net_in` names none; that is said
    // rather than hidden.
    let source = std::fs::read_to_string(repo_root().join("examples/api/api.ls")).unwrap();
    let json = authority_json(&source, "api-authority");
    assert!(!json.contains("\"ffi\""), "no foreign code anywhere:\n{json}");
    for label in ["net_in", "conn_accept", "conn_read", "conn_write", "poll", "clock", "heap"] {
        assert!(json.contains(&format!("\"name\": \"{label}\"")), "`{label}` is reported:\n{json}");
    }
}
