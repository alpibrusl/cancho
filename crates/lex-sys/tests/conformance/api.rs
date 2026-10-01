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
    let fetched = fetch_net_sockets(&format!("{tag}-fetch"), "examples/api/net.lock");
    let (dir, exe) =
        build_example_paths(tag, &[repo_root().join("examples/api/api.ls"), fetched], "api");
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
