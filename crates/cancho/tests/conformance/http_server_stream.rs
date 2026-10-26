//! `packages/http-server`'s `stream` on a socket server (`docs/http-server.md` section 14): the program
//! `tests/programs/server_stream.cho` holds `GET /stream` and sends it one event every 100 ms, chunked, then ends it; `GET /flood`
//! is offered 4,007-byte chunks until the connection's answer room will not take one whole. The server is built from the package's
//! source (`HTTP_SERVER_SOURCE` names another file, as `scripts/http_server_stream_mutants.py` does); the store is checked in
//! `stream_through_the_store`.

use super::*;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

struct Streamer {
    dir: PathBuf,
    child: std::process::Child,
    port: u16,
}

impl Streamer {
    fn start(tag: &str, paths: Vec<PathBuf>, extra: &[&str]) -> Streamer {
        let (dir, exe) = build_example_paths(tag, &paths, "server_stream");
        let mut attempts = 0;
        loop {
            let port = free_port();
            let mut child = Command::new(&exe)
                .arg(port.to_string())
                .args(extra)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("the compiled server runs");
            std::thread::sleep(Duration::from_millis(300));
            attempts += 1;
            if attempts < 5 && matches!(child.try_wait(), Ok(Some(_))) {
                continue;
            }
            return Streamer { dir, child, port };
        }
    }

    fn from_source(tag: &str) -> Streamer {
        Streamer::from_source_with(tag, &[])
    }

    fn from_source_with(tag: &str, extra: &[&str]) -> Streamer {
        let source = std::env::var("HTTP_SERVER_SOURCE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| repo_root().join("packages/http-server/server.cho"));
        Streamer::start(
            tag,
            vec![repo_root().join("tests/programs/server_stream.cho"), source],
            extra,
        )
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

    /// One short request on a connection of its own; the body of the answer.
    fn get(&self, path: &str) -> String {
        let mut s = self.client();
        s.write_all(
            format!("GET {path} HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n").as_bytes(),
        )
        .unwrap();
        let mut all = Vec::new();
        s.read_to_end(&mut all).unwrap();
        let text = String::from_utf8_lossy(&all).into_owned();
        text.split("\r\n\r\n").nth(1).unwrap_or("").to_string()
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Reads until `needle` has arrived or `limit` has passed; the bytes read and the time each read finished.
fn read_until(
    s: &mut TcpStream,
    needle: &[u8],
    limit: Duration,
) -> (Vec<u8>, Vec<(Instant, usize)>) {
    s.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    let end = Instant::now() + limit;
    let mut got = Vec::new();
    let mut marks = Vec::new();
    let mut chunk = [0u8; 8192];
    while Instant::now() < end && !got.windows(needle.len()).any(|w| w == needle) {
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                got.extend(&chunk[..n]);
                marks.push((Instant::now(), got.len()));
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {}
            Err(e) => panic!("{e}"),
        }
    }
    (got, marks)
}

fn assert_stream_of_five(text: &str) {
    let head =
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n";
    assert!(text.starts_with(head), "head: {text:?}");
    let mut want = String::from(head);
    for n in 1..=5 {
        want.push_str(&format!("9\r\ndata: {n}\n\n\r\n"));
    }
    want.push_str("0\r\n\r\n");
    assert_eq!(text, want);
}

#[test]
fn events_arrive_while_the_stream_is_open_and_others_are_served() {
    let server = Streamer::from_source("hs-stream-events");
    let mut a = server.client();
    a.write_all(b"GET /stream HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
    let began = Instant::now();
    // Another connection is answered while the stream is open.
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(server.get("/plain"), "/plain");
    let (got, marks) = read_until(&mut a, b"0\r\n\r\n", Duration::from_secs(10));
    assert_stream_of_five(&String::from_utf8_lossy(&got));
    // The events were not all written at the end: the reads that carried them are spread over time (five events 100 ms apart).
    let first = marks.first().unwrap().0;
    let last = marks.last().unwrap().0;
    assert!(
        last - first >= Duration::from_millis(250) && marks.len() >= 3,
        "the stream arrived in one piece: {} reads over {:?}",
        marks.len(),
        last - first
    );
    assert!(last - began < Duration::from_secs(5));
    // The ticket of the stream that is over takes nothing (the request is not held any more), and nothing was written to its connection.
    assert_eq!(server.get("/stale"), "-1");
    assert_eq!(server.get("/spare"), "0");
    // The response is over and the connection is still good: the next request on it is answered.
    a.write_all(b"GET /again HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n").unwrap();
    let mut rest = Vec::new();
    a.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    a.read_to_end(&mut rest).unwrap();
    assert!(
        String::from_utf8_lossy(&rest).ends_with("/again"),
        "{:?}",
        String::from_utf8_lossy(&rest)
    );
}

#[test]
fn a_client_that_leaves_ends_its_stream_and_the_loop_goes_on() {
    let server = Streamer::from_source("hs-stream-leaves");
    let mut a = server.client();
    a.write_all(b"GET /stream HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
    let (got, _) = read_until(&mut a, b"data: 1", Duration::from_secs(5));
    assert!(String::from_utf8_lossy(&got).contains("data: 1"));
    drop(a);
    std::thread::sleep(Duration::from_millis(900));
    // `stream` answered -1 for the ticket of the connection that left (once), and the loop answers others.
    assert_eq!(server.get("/gone"), "1");
    assert_eq!(server.get("/plain"), "/plain");
    // A second stream still works afterwards.
    let mut b = server.client();
    b.write_all(b"GET /stream HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
    let (got, _) = read_until(&mut b, b"0\r\n\r\n", Duration::from_secs(10));
    assert_stream_of_five(&String::from_utf8_lossy(&got));
}

#[test]
fn a_ticket_of_a_connection_that_left_does_not_reach_the_one_that_took_its_slot() {
    let server = Streamer::from_source("hs-stream-generation");
    let mut a = server.client();
    a.write_all(b"GET /stream HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
    let (got, _) = read_until(&mut a, b"data: 1", Duration::from_secs(5));
    assert!(String::from_utf8_lossy(&got).contains("data: 1"));
    drop(a);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(server.get("/gone"), "1");
    // Another stream, in the slot the first one left; while it runs, the old ticket is offered a byte.
    let mut b = server.client();
    b.write_all(b"GET /stream HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(server.get("/stale"), "-1");
    let (got, _) = read_until(&mut b, b"0\r\n\r\n", Duration::from_secs(10));
    assert_stream_of_five(&String::from_utf8_lossy(&got));
}

#[test]
fn a_request_refused_while_it_streams_is_given_no_more_bytes() {
    // The program streams to a request whose declared body never arrives, with a stall timer of 250 ms: the request is refused at 250 ms, and an event
    // offered after that must not be added to the answer that is about to be replaced by the refusal and the end of the connection.
    let server = Streamer::from_source_with("hs-stream-refused", &["250"]);
    let mut a = server.client();
    a.write_all(b"GET /stream HTTP/1.1\r\nHost: t\r\nContent-Length: 10\r\n\r\n").unwrap();
    let began = Instant::now();
    let (got, _) = read_until(&mut a, b"0\r\n\r\n", Duration::from_secs(3));
    let text = String::from_utf8_lossy(&got).into_owned();
    assert!(began.elapsed() < Duration::from_secs(3), "{text:?}");
    let events = text.matches("data: ").count();
    assert!((1..=3).contains(&events), "{events} events before the refusal: {text:?}");
    assert!(!text.contains("0\r\n\r\n"), "the stream was ended properly: {text:?}");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(server.get("/gone"), "1");
}

#[test]
fn a_reader_that_never_reads_is_never_given_half_a_message() {
    let server = Streamer::from_source("hs-stream-flood");
    let mut a = server.client();
    a.write_all(b"GET /flood HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
    // `a` reads nothing. The program offers a 4,007-byte chunk every turn while the room takes it whole; the first turn it does not, the stream is ended.
    let began = Instant::now();
    let stats = loop {
        let s = server.get("/stats");
        if !s.contains("refused=0") {
            break s;
        }
        assert!(began.elapsed() < Duration::from_secs(30), "the stream was never ended: {s}");
        std::thread::sleep(Duration::from_millis(100));
    };
    eprintln!("measured: the stream was ended after {:?} with {stats}", began.elapsed());
    let taken: usize =
        stats.trim_start_matches("taken=").split(' ').next().unwrap().parse().unwrap();
    // The program offered the chunk anyway, as an application that ignores `space` would: it took what the room held, less than a chunk.
    let blind: usize = stats.rsplit("blind=").next().unwrap().parse().unwrap();
    assert!(blind < 4007, "a refused offer took {blind} bytes");
    assert_eq!(stats, format!("taken={taken} refused=1 blind={blind}"));
    assert_eq!(taken % 4007, 0, "a message was cut: {taken} bytes taken");
    assert!(taken > 0);
    // What the reader gets when it finally reads is the head and whole chunks, nothing cut.
    let head_len =
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n"
            .len();
    a.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    let mut got = Vec::<u8>::new();
    let mut buf = vec![0u8; 65536];
    let end = Instant::now() + Duration::from_secs(20);
    let mut closed = false;
    while !closed && Instant::now() < end {
        match a.read(&mut buf) {
            Ok(0) => closed = true,
            Ok(n) => got.extend(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {}
            Err(e) => panic!("{e}"),
        }
    }
    // The server ended the connection of the reader it could not keep up with (`answer` of the end of the body did not fit the full room).
    assert!(closed, "the connection of a reader that fell behind is still open");
    let text = &got[head_len..];
    // What arrived is the stream from its start, whole chunks, and at most the last one cut by the end of the connection: nothing else, nothing out of place.
    let mut chunk = b"fa0\r\n".to_vec();
    chunk.extend(std::iter::repeat_n(b'x', 4000));
    chunk.extend(b"\r\n");
    let whole = text.len() / 4007;
    for i in 0..whole {
        assert_eq!(&text[i * 4007..(i + 1) * 4007], &chunk[..], "chunk {i} is not whole");
    }
    let tail = &text[whole * 4007..];
    assert_eq!(tail, &chunk[..tail.len()]);
    // What was waiting in the answer room when the connection was ended (at most its size, 65,536 bytes) never reached the reader.
    let lost = (taken + blind) as i64 - text.len() as i64;
    eprintln!(
        "measured: {} bytes taken, {blind} more offered blind, {} received, {lost} lost with the connection",
        taken,
        text.len()
    );
    assert!((0..=65536).contains(&lost), "{lost} bytes were lost");
    assert_eq!(server.get("/plain"), "/plain");
}

#[test]
fn stream_through_the_store() {
    let mut paths = vec![repo_root().join("tests/programs/server_stream.cho")];
    paths.extend(fetch_net_dependencies(
        "hs-stream-store",
        &[("tests/programs/server_stream.lock", "packages/http-server/.cancho-vcs")],
    ));
    let server = Streamer::start("hs-stream-store-run", paths, &[]);
    let mut a = server.client();
    a.write_all(b"GET /stream HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
    let (got, _) = read_until(&mut a, b"0\r\n\r\n", Duration::from_secs(10));
    assert_stream_of_five(&String::from_utf8_lossy(&got));
}
