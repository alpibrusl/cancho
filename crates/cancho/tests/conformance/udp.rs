//! `docs/udp.md`: datagram sockets, over real sockets, on both backends. Every program is
//! `edition 5;` and declares no `extern fn` and holds no `Ffi`.

use super::sockets::{BACKENDS, build, dial_program, io_program, wait_for};
use super::*;
use std::net::UdpSocket;
use std::time::Duration;

/// Run `source` (a `run(bound, io)` program dialling `127.0.0.1:PORT`) on both backends against
/// `peer`, which gets the UDP socket the program talks to, already bound to `PORT`.
fn against_peer(
    tag: &str,
    source: &str,
    expect: i32,
    peer: impl Fn(UdpSocket) + Send + Clone + 'static,
) {
    for backend in BACKENDS {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let port = socket.local_addr().unwrap().port();
        let dir = scratch(&format!("udp-{tag}-{backend}"));
        let exe =
            build(&dir, tag, &dial_program(port, &format!("127.0.0.1:{port}"), source), backend);
        let peer = peer.clone();
        let thread = std::thread::spawn(move || peer(socket));
        let code = run_with_deadline(&exe, tag, backend);
        assert_eq!(code, Some(expect), "{backend}/{tag}");
        thread.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Run `exe`, killing it after 20 seconds: a mutant that makes a receive block for ever must fail
/// the test, not hang the suite.
fn run_with_deadline(exe: &Path, tag: &str, backend: &str) -> Option<i32> {
    let mut child = Command::new(exe).spawn().expect("the program runs");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait().expect("a waitable child") {
            return status.code();
        }
        if std::time::Instant::now() > deadline {
            child.kill().expect("a killable child");
            child.wait().expect("a reaped child");
            panic!("{backend}/{tag}: still running after 20 seconds");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The program sends `ping` and reads one datagram back, and exits `0` if it is `PING`.
const ECHO: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match udp_connect(n, "127.0.0.1", PORT) {
            UdpOpened::Ok(u) => {
                var sock = u;
                borrow mut sock as &!uh in {
                    match udp_send(uh, "ping") {
                        Sent::Wrote(w) => {
                            region a {
                                var buf = alloc_slice[a](16, byte_of(0));
                                match udp_recv(uh, buf) {
                                    Datagram::Got(k) => {
                                        if k == 4 && int_of(buf[0]) == 80 { status = 0; } else { status = 5; }
                                    }
                                    Datagram::Truncated(k) => { status = 6; }
                                    Datagram::Again => { status = 7; }
                                    Datagram::Failed(e) => { status = 8; }
                                }
                            }
                        }
                        Sent::Again => { status = 9; }
                        Sent::Failed(e) => { status = 10; }
                    }
                }
                udp_close(sock);
            }
            UdpOpened::Failed(e) => { status = 2; }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

#[test]
fn a_connected_socket_exchanges_a_datagram() {
    against_peer("echo", ECHO, 0, |socket| {
        let mut buf = [0u8; 16];
        let (n, from) = socket.recv_from(&mut buf).unwrap();
        socket.send_to(&buf[..n].to_ascii_uppercase(), from).unwrap();
    });
}

/// An empty datagram is a datagram: `Got(0)`, never an end-of-stream.
const EMPTY: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match udp_connect(n, "127.0.0.1", PORT) {
            UdpOpened::Ok(u) => {
                var sock = u;
                borrow mut sock as &!uh in {
                    udp_send(uh, "x");
                    region a {
                        var buf = alloc_slice[a](16, byte_of(0));
                        match udp_recv(uh, buf) {
                            Datagram::Got(k) => { if k == 0 { status = 0; } else { status = 5; } }
                            Datagram::Truncated(k) => { status = 6; }
                            Datagram::Again => { status = 7; }
                            Datagram::Failed(e) => { status = 8; }
                        }
                    }
                }
                udp_close(sock);
            }
            UdpOpened::Failed(e) => { status = 2; }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

#[test]
fn an_empty_datagram_is_got_zero() {
    against_peer("empty", EMPTY, 0, |socket| {
        let mut buf = [0u8; 16];
        let (_, from) = socket.recv_from(&mut buf).unwrap();
        socket.send_to(&[], from).unwrap();
    });
}

/// A 100-byte datagram into a 16-byte buffer is `Truncated`, never a `Got(16)` that looks whole.
/// Linux reports the real length; Darwin has no such flag, so there it reports the buffer's.
const TRUNCATED: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match udp_connect(n, "127.0.0.1", PORT) {
            UdpOpened::Ok(u) => {
                var sock = u;
                borrow mut sock as &!uh in {
                    udp_send(uh, "x");
                    region a {
                        var buf = alloc_slice[a](16, byte_of(0));
                        match udp_recv(uh, buf) {
                            Datagram::Got(k) => { status = 5; }
                            Datagram::Truncated(k) => { if k == EXPECT { status = 0; } else { status = 6; } }
                            Datagram::Again => { status = 7; }
                            Datagram::Failed(e) => { status = 8; }
                        }
                    }
                }
                udp_close(sock);
            }
            UdpOpened::Failed(e) => { status = 2; }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

#[test]
fn an_oversize_datagram_is_truncated_and_says_so() {
    let expect = if cfg!(target_os = "macos") { 16 } else { 100 };
    against_peer("truncated", &TRUNCATED.replace("EXPECT", &expect.to_string()), 0, |socket| {
        let mut buf = [0u8; 16];
        let (_, from) = socket.recv_from(&mut buf).unwrap();
        socket.send_to(&[7u8; 100], from).unwrap();
    });
}

/// The kernel drops a datagram from any source but the one the socket was connected to: a
/// stranger's `evil` arrives first and is never seen; the peer's `good` is what `udp_recv` reads.
const STRANGER: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match udp_connect(n, "127.0.0.1", PORT) {
            UdpOpened::Ok(u) => {
                var sock = u;
                borrow mut sock as &!uh in {
                    udp_send(uh, "x");
                    region a {
                        var buf = alloc_slice[a](16, byte_of(0));
                        match udp_recv(uh, buf) {
                            Datagram::Got(k) => { if k == 4 && int_of(buf[0]) == 103 { status = 0; } else { status = 5; } }
                            Datagram::Truncated(k) => { status = 6; }
                            Datagram::Again => { status = 7; }
                            Datagram::Failed(e) => { status = 8; }
                        }
                    }
                }
                udp_close(sock);
            }
            UdpOpened::Failed(e) => { status = 2; }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

#[test]
fn a_datagram_from_a_stranger_never_reaches_a_connected_socket() {
    against_peer("stranger", STRANGER, 0, |socket| {
        let mut buf = [0u8; 16];
        let (_, from) = socket.recv_from(&mut buf).unwrap();
        let stranger = UdpSocket::bind("127.0.0.1:0").unwrap();
        stranger.send_to(b"evil", from).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        socket.send_to(b"good", from).unwrap();
    });
}

/// A non-blocking receive with nothing waiting is `Again`.
const AGAIN: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match udp_connect(n, "127.0.0.1", PORT) {
            UdpOpened::Ok(u) => {
                var sock = u;
                borrow mut sock as &!uh in {
                    if udp_nonblocking(uh) != 0 {
                        status = 3;
                    } else {
                        region a {
                            var buf = alloc_slice[a](16, byte_of(0));
                            match udp_recv(uh, buf) {
                                Datagram::Got(k) => { status = 5; }
                                Datagram::Truncated(k) => { status = 6; }
                                Datagram::Again => { status = 0; }
                                Datagram::Failed(e) => { status = 8; }
                            }
                            var empty = alloc_slice[a](0, byte_of(0));
                            match udp_recv(uh, empty) {
                                Datagram::Failed(e) => { if e != 22 { status = 9; } }
                                Datagram::Got(k) => { status = 10; }
                                Datagram::Truncated(k) => { status = 11; }
                                Datagram::Again => { status = 12; }
                            }
                        }
                    }
                }
                udp_close(sock);
            }
            UdpOpened::Failed(e) => { status = 2; }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

#[test]
fn a_nonblocking_receive_with_nothing_waiting_is_again_and_an_empty_buffer_is_einval() {
    against_peer("again", AGAIN, 0, |_socket| {});
}

/// Two sockets get two kernel-chosen ports, both real (ephemeral ports are above 1023, which also
/// catches a port read from the wrong bytes of the address): the way to a fresh source port is a
/// fresh socket (`docs/udp.md` §5).
const PORTS: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match udp_connect(n, "127.0.0.1", PORT) {
            UdpOpened::Ok(a0) => {
                var first = a0;
                match udp_connect(n, "127.0.0.1", PORT) {
                    UdpOpened::Ok(b0) => {
                        var second = b0;
                        borrow first as &fa in {
                            borrow second as &sb in {
                                let p = udp_local_port(fa);
                                let q = udp_local_port(sb);
                                if p > 1023 && q > 1023 && p != q { status = 0; } else { status = 5; }
                            }
                        }
                        udp_close(second);
                    }
                    UdpOpened::Failed(e) => { status = 3; }
                }
                udp_close(first);
            }
            UdpOpened::Failed(e) => { status = 2; }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

#[test]
fn each_socket_gets_its_own_kernel_chosen_port() {
    against_peer("ports", PORTS, 0, |_socket| {});
}

/// A `Poller` reports a datagram socket readable once the peer has written.
const POLLED: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match udp_connect(n, "127.0.0.1", PORT) {
            UdpOpened::Ok(u) => {
                var sock = u;
                match poller_new() {
                    Polling::Ok(p0) => {
                        var poller = p0;
                        borrow mut sock as &!uh in {
                            borrow mut poller as &!ph in {
                                udp_send(uh, "x");
                                poller_add_udp(ph, uh, 7, 1);
                                region a {
                                    var ev = alloc_slice[a](4, 0);
                                    let ready = poller_wait(ph, ev, 5000);
                                    if ready < 1 || ev[0] != 7 {
                                        status = 3;
                                    } else {
                                        var buf = alloc_slice[a](16, byte_of(0));
                                        match udp_recv(uh, buf) {
                                            Datagram::Got(k) => { if k == 4 { status = 0; } else { status = 5; } }
                                            Datagram::Truncated(k) => { status = 6; }
                                            Datagram::Again => { status = 7; }
                                            Datagram::Failed(e) => { status = 8; }
                                        }
                                    }
                                }
                            }
                        }
                        poller_close(poller);
                    }
                    Polling::Failed(e) => { status = 2; }
                }
                udp_close(sock);
            }
            UdpOpened::Failed(e) => { status = 2; }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

#[test]
fn a_poller_reports_a_datagram_socket_readable() {
    against_peer("polled", POLLED, 0, |socket| {
        let mut buf = [0u8; 16];
        let (_, from) = socket.recv_from(&mut buf).unwrap();
        socket.send_to(b"pong", from).unwrap();
    });
}

const OUTSIDE_HOST: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    borrow bound as &n in {
        match udp_connect(n, "127.0.0.2", PORT) {
            UdpOpened::Ok(u) => { udp_close(u); }
            UdpOpened::Failed(e) => { }
        }
    }
    release(bound);
    release(io);
    return 1;
}
"#;

const OUTSIDE_PORT: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    borrow bound as &n in {
        match udp_connect(n, "127.0.0.1", 1) {
            UdpOpened::Ok(u) => { udp_close(u); }
            UdpOpened::Failed(e) => { }
        }
    }
    release(bound);
    release(io);
    return 1;
}
"#;

/// The bound is `host:port` and both halves are enforced, as `tcp_connect`'s are: a program
/// granted `127.0.0.1:P` can reach neither another host nor another port.
#[test]
fn connecting_outside_the_bound_traps() {
    for backend in BACKENDS {
        for (name, source) in [("host", OUTSIDE_HOST), ("port", OUTSIDE_PORT)] {
            let port = free_port();
            let dir = scratch(&format!("udp-outside-{name}-{backend}"));
            let exe = build(
                &dir,
                "outside",
                &dial_program(port, &format!("127.0.0.1:{port}"), source),
                backend,
            );
            let run = Command::new(&exe).output().expect("the program runs");
            assert_eq!(run.status.code(), None, "{backend}/{name}: killed by the trap");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

/// The report of a datagram client names the host and port it may reach, the two path-free
/// labels, and no `ffi`.
#[test]
fn a_datagram_client_reports_its_host_and_no_ffi() {
    let json = authority_json(&dial_program(9, "127.0.0.1:9", ECHO), "udp-client-authority");
    assert!(
        json.contains(
            "{ \"name\": \"net_out\", \"argument\": \"127.0.0.1:9\", \"bounded\": true }"
        ),
        "the bound survives the move to handles:\n{json}"
    );
    assert!(json.contains("\"udp_send\"") && json.contains("\"udp_recv\""), "{json}");
    assert!(!json.contains("\"ffi\"") && !json.contains("\"net_in\""), "{json}");
}

// ---- The bound half (`docs/udp.md` §4) ------------------------------------------------------

/// A client's view of a server: build `source` (a `run(bound, io)` program for `Net("PORT")`) on
/// each backend, wait until it says `ready` on standard error, run `client(port)`, and require the
/// server to exit `expect` within 20 seconds.
fn serve(tag: &str, source: &str, expect: i32, client: impl Fn(u16)) {
    for backend in BACKENDS {
        let port = free_udp_port();
        let dir = scratch(&format!("udp-serve-{tag}-{backend}"));
        let exe = build(&dir, tag, &io_program(port, source), backend);
        let mut child = Command::new(&exe).stderr(Stdio::piped()).spawn().expect("the server runs");
        let mut lines = std::io::BufReader::new(child.stderr.take().unwrap());
        wait_for(&mut lines, "ready");
        client(port);
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let code = loop {
            if let Some(status) = child.try_wait().expect("a waitable child") {
                break status.code();
            }
            if std::time::Instant::now() > deadline {
                child.kill().expect("a killable child");
                child.wait().expect("a reaped child");
                panic!("{backend}/{tag}: still running after 20 seconds");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(code, Some(expect), "{backend}/{tag}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn free_udp_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0").expect("a free loopback port").local_addr().unwrap().port()
}

/// Send `message` to `port` and read one datagram back.
fn ask(port: u16, message: &[u8]) -> Vec<u8> {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    socket.send_to(message, ("127.0.0.1", port)).unwrap();
    let mut buf = [0u8; 64];
    let (n, _) = socket.recv_from(&mut buf).unwrap();
    buf[..n].to_vec()
}

/// A bound socket reads one datagram and answers its sender through the ticket.
const SERVE_ECHO: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    var status = 1;
    borrow mut io as &!i in {
        borrow bound as &n in {
            match udp_bind(n, PORT, 0) {
                UdpOpened::Ok(u) => {
                    var sock = u;
                    borrow mut sock as &!uh in {
                        io.error_all(i, "ready\n");
                        region a {
                            var buf = alloc_slice[a](64, byte_of(0));
                            var who = alloc_slice[a](1, 0);
                            match udp_recv_from(uh, buf, who) {
                                Datagram::Got(k) => {
                                    match udp_send_to(uh, buf[0..k], who[0]) {
                                        Sent::Wrote(w) => { if w == k { status = 0; } else { status = 5; } }
                                        Sent::Again => { status = 6; }
                                        Sent::Failed(e) => { status = 7; }
                                    }
                                }
                                Datagram::Truncated(k) => { status = 3; }
                                Datagram::Again => { status = 4; }
                                Datagram::Failed(e) => { status = 8; }
                            }
                        }
                    }
                    udp_close(sock);
                }
                UdpOpened::Failed(e) => { status = 2; }
            }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

#[test]
fn a_bound_socket_answers_the_sender_it_heard_from() {
    serve("serve-echo", SERVE_ECHO, 0, |port| {
        assert_eq!(ask(port, b"hello-udp"), b"hello-udp");
    });
}

/// The ticket outlives the next receive: two senders are heard, then answered in the *opposite*
/// order, and each gets its own answer.
const SERVE_TWO: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    var status = 1;
    borrow mut io as &!i in {
        borrow bound as &n in {
            match udp_bind(n, PORT, 0) {
                UdpOpened::Ok(u) => {
                    var sock = u;
                    borrow mut sock as &!uh in {
                        io.error_all(i, "ready\n");
                        region a {
                            var buf = alloc_slice[a](64, byte_of(0));
                            var first = alloc_slice[a](1, 0);
                            var second = alloc_slice[a](1, 0);
                            match udp_recv_from(uh, buf, first) {
                                Datagram::Got(k) => {
                                    match udp_recv_from(uh, buf, second) {
                                        Datagram::Got(m) => {
                                            var both = 0;
                                            match udp_send_to(uh, "to-second", second[0]) {
                                                Sent::Wrote(w) => { both = both + 1; }
                                                Sent::Again => { }
                                                Sent::Failed(e) => { }
                                            }
                                            match udp_send_to(uh, "to-first", first[0]) {
                                                Sent::Wrote(w) => { both = both + 1; }
                                                Sent::Again => { }
                                                Sent::Failed(e) => { }
                                            }
                                            if both == 2 { status = 0; }
                                        }
                                        Datagram::Truncated(m) => { status = 3; }
                                        Datagram::Again => { status = 4; }
                                        Datagram::Failed(e) => { status = 8; }
                                    }
                                }
                                Datagram::Truncated(k) => { status = 3; }
                                Datagram::Again => { status = 4; }
                                Datagram::Failed(e) => { status = 8; }
                            }
                        }
                    }
                    udp_close(sock);
                }
                UdpOpened::Failed(e) => { status = 2; }
            }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

#[test]
fn a_reply_can_wait_for_a_later_receive_and_still_reach_its_sender() {
    serve("serve-two", SERVE_TWO, 0, |port| {
        let a = UdpSocket::bind("127.0.0.1:0").unwrap();
        let b = UdpSocket::bind("127.0.0.1:0").unwrap();
        for socket in [&a, &b] {
            socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        }
        a.send_to(b"one", ("127.0.0.1", port)).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        b.send_to(b"two", ("127.0.0.1", port)).unwrap();
        let mut buf = [0u8; 64];
        let (n, _) = a.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"to-first");
        let (n, _) = b.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"to-second");
    });
}

/// One refused `udp_send_to`: `Failed(EBADF)` for `TICKET`, and nothing else.
fn refused(ticket: &str) -> String {
    format!(
        "match udp_send_to(uh, \"bad\", {ticket}) {{\n\
             Sent::Failed(e) => {{ if e == 9 {{ refused = refused + 1; }} }}\n\
             Sent::Wrote(w) => {{ }}\n\
             Sent::Again => {{ }}\n\
         }}\n"
    )
}

/// Forged tickets get nothing: one never issued, zero, negative, and the same slot a ring's length
/// on. Only the real ticket sends, and the client sees exactly one datagram.
fn forged_program() -> String {
    let attempts: String = [
        refused("who[0] + 1000"),
        refused("0"),
        refused("0 - 5"),
        refused("who[0] + 65536"),
        refused("who[0] - 65536"),
    ]
    .concat();
    format!(
        r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {{
    var status = 1;
    borrow mut io as &!i in {{
        borrow bound as &n in {{
            match udp_bind(n, PORT, 0) {{
                UdpOpened::Ok(u) => {{
                    var sock = u;
                    borrow mut sock as &!uh in {{
                        io.error_all(i, "ready\n");
                        region a {{
                            var buf = alloc_slice[a](64, byte_of(0));
                            var who = alloc_slice[a](1, 0);
                            match udp_recv_from(uh, buf, who) {{
                                Datagram::Got(k) => {{
                                    var refused = 0;
                                    {attempts}
                                    match udp_send_to(uh, "ok", who[0]) {{
                                        Sent::Wrote(w) => {{ if refused == 5 && w == 2 {{ status = 0; }} else {{ status = 5; }} }}
                                        Sent::Again => {{ status = 6; }}
                                        Sent::Failed(e) => {{ status = 7; }}
                                    }}
                                }}
                                Datagram::Truncated(k) => {{ status = 3; }}
                                Datagram::Again => {{ status = 4; }}
                                Datagram::Failed(e) => {{ status = 8; }}
                            }}
                        }}
                    }}
                    udp_close(sock);
                }}
                UdpOpened::Failed(e) => {{ status = 2; }}
            }}
        }}
    }}
    release(bound);
    release(io);
    return status;
}}
"#
    )
}

#[test]
fn a_forged_ticket_sends_nothing() {
    serve("serve-forged", &forged_program(), 0, |port| {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        socket.send_to(b"hi", ("127.0.0.1", port)).unwrap();
        let mut buf = [0u8; 64];
        let (n, _) = socket.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"ok");
        // The five refused sends must not have sent anything: nothing else is waiting.
        socket.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
        assert!(socket.recv_from(&mut buf).is_err(), "a forged ticket sent a datagram");
    });
}

/// `main` for a program that holds the *whole* network (an unnarrowed `Net`), so it can bind two
/// ports and dial itself.
fn open_program(port: u16, other: u16, source: &str) -> String {
    let main = r#"
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi); release(fs); release(heap); release(args); release(clock);
    return run(net, io);
}
"#;
    format!("edition 5;\nimport std.io;\n{source}\n{main}")
        .replace("OTHER", &other.to_string())
        .replace("PORT", &port.to_string())
}

fn serve_open(tag: &str, source: &str, expect: i32, client: impl Fn(u16)) {
    for backend in BACKENDS {
        let (port, other) = (free_udp_port(), free_udp_port());
        let dir = scratch(&format!("udp-open-{tag}-{backend}"));
        let exe = build(&dir, tag, &open_program(port, other, source), backend);
        let mut child = Command::new(&exe).stderr(Stdio::piped()).spawn().expect("the server runs");
        let mut lines = std::io::BufReader::new(child.stderr.take().unwrap());
        wait_for(&mut lines, "ready");
        client(port);
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        let code = loop {
            if let Some(status) = child.try_wait().expect("a waitable child") {
                break status.code();
            }
            if std::time::Instant::now() > deadline {
                child.kill().expect("a killable child");
                child.wait().expect("a reaped child");
                panic!("{backend}/{tag}: still running after 60 seconds");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(code, Some(expect), "{backend}/{tag}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A ticket belongs to the socket that heard the sender: a second bound socket cannot use it.
const WRONG_SOCKET: &str = r#"
fn run(net: Net(""), io: Io) -> [] int {
    var status = 1;
    borrow mut io as &!i in {
        borrow net as &n in {
            match udp_bind(n, PORT, 0) {
                UdpOpened::Ok(u) => {
                    var sock = u;
                    match udp_bind(n, OTHER, 0) {
                        UdpOpened::Ok(v) => {
                            var stranger = v;
                            borrow mut sock as &!uh in {
                                borrow mut stranger as &!sh in {
                                    io.error_all(i, "ready\n");
                                    region a {
                                        var buf = alloc_slice[a](64, byte_of(0));
                                        var who = alloc_slice[a](1, 0);
                                        match udp_recv_from(uh, buf, who) {
                                            Datagram::Got(k) => {
                                                var wrong = 0;
                                                match udp_send_to(sh, "bad", who[0]) {
                                                    Sent::Failed(e) => { if e == 9 { wrong = 1; } }
                                                    Sent::Wrote(w) => { }
                                                    Sent::Again => { }
                                                }
                                                match udp_send_to(uh, "ok", who[0]) {
                                                    Sent::Wrote(w) => { if wrong == 1 { status = 0; } else { status = 5; } }
                                                    Sent::Again => { status = 6; }
                                                    Sent::Failed(e) => { status = 7; }
                                                }
                                            }
                                            Datagram::Truncated(k) => { status = 3; }
                                            Datagram::Again => { status = 4; }
                                            Datagram::Failed(e) => { status = 8; }
                                        }
                                    }
                                }
                            }
                            udp_close(stranger);
                        }
                        UdpOpened::Failed(e) => { status = 9; }
                    }
                    udp_close(sock);
                }
                UdpOpened::Failed(e) => { status = 2; }
            }
        }
    }
    release(net);
    release(io);
    return status;
}
"#;

#[test]
fn a_ticket_is_refused_by_a_socket_that_did_not_hear_the_sender() {
    serve_open("wrong-socket", WRONG_SOCKET, 0, |port| {
        assert_eq!(ask(port, b"hi"), b"ok");
    });
}

/// A ticket lasts the ring's length: after 65,536 more datagrams its entry belongs to a newer one,
/// and the late reply is `Failed(EBADF)` rather than a datagram to whoever is there now. The
/// program feeds itself the 65,536 through a connected socket, so no client can drop any.
const STALE: &str = r#"
fn run(net: Net(""), io: Io) -> [] int {
    var status = 1;
    borrow mut io as &!i in {
        borrow net as &n in {
            match udp_bind(n, PORT, 0) {
                UdpOpened::Ok(u) => {
                    var sock = u;
                    borrow mut sock as &!uh in {
                        io.error_all(i, "ready\n");
                        region a {
                            var buf = alloc_slice[a](64, byte_of(0));
                            var who = alloc_slice[a](1, 0);
                            match udp_recv_from(uh, buf, who) {
                                Datagram::Got(k) => {
                                    let first = who[0];
                                    match udp_connect(n, "127.0.0.1", PORT) {
                                        UdpOpened::Ok(c) => {
                                            var feeder = c;
                                            var count = 0;
                                            var healthy = true;
                                            borrow mut feeder as &!fh in {
                                                while count < 65536 && healthy {
                                                    match udp_send(fh, "x") {
                                                        Sent::Wrote(w) => { }
                                                        Sent::Again => { healthy = false; }
                                                        Sent::Failed(e) => { healthy = false; }
                                                    }
                                                    match udp_recv_from(uh, buf, who) {
                                                        Datagram::Got(m) => { }
                                                        Datagram::Truncated(m) => { healthy = false; }
                                                        Datagram::Again => { healthy = false; }
                                                        Datagram::Failed(e) => { healthy = false; }
                                                    }
                                                    count = count + 1;
                                                }
                                            }
                                            udp_close(feeder);
                                            match udp_send_to(uh, "late", first) {
                                                Sent::Failed(e) => { if e == 9 && healthy { status = 0; } else { status = 5; } }
                                                Sent::Wrote(w) => { status = 6; }
                                                Sent::Again => { status = 7; }
                                            }
                                        }
                                        UdpOpened::Failed(e) => { status = 9; }
                                    }
                                }
                                Datagram::Truncated(k) => { status = 3; }
                                Datagram::Again => { status = 4; }
                                Datagram::Failed(e) => { status = 8; }
                            }
                        }
                    }
                    udp_close(sock);
                }
                UdpOpened::Failed(e) => { status = 2; }
            }
        }
    }
    release(net);
    release(io);
    return status;
}
"#;

#[test]
fn a_ticket_older_than_the_ring_is_refused() {
    serve_open("stale", STALE, 0, |port| {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.send_to(b"first", ("127.0.0.1", port)).unwrap();
        // The late reply must not arrive.
        socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut buf = [0u8; 64];
        assert!(socket.recv_from(&mut buf).is_err(), "a stale ticket sent a datagram");
    });
}

/// Binding a port outside the bound traps, as `tcp_listen`'s does.
const BIND_OUTSIDE: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    borrow bound as &n in {
        match udp_bind(n, 1, 0) {
            UdpOpened::Ok(u) => { udp_close(u); }
            UdpOpened::Failed(e) => { }
        }
    }
    release(bound);
    release(io);
    return 1;
}
"#;

#[test]
fn binding_outside_the_bound_traps() {
    for backend in BACKENDS {
        let port = free_udp_port();
        let dir = scratch(&format!("udp-bind-outside-{backend}"));
        let exe = build(&dir, "outside", &io_program(port, BIND_OUTSIDE), backend);
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), None, "{backend}: killed by the trap");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The report of a datagram server names the port it may bind, the two path-free labels, and no
/// `net_out` (a bound socket reaches only those who wrote to it) and no `ffi`.
#[test]
fn a_datagram_server_reports_its_port_and_no_net_out() {
    let json = authority_json(&io_program(9, SERVE_ECHO), "udp-server-authority");
    assert!(
        json.contains("{ \"name\": \"net_in\", \"argument\": \"9\", \"bounded\": true }"),
        "the port survives the move to handles:\n{json}"
    );
    assert!(json.contains("\"udp_send\"") && json.contains("\"udp_recv\""), "{json}");
    assert!(
        !json.contains("\"net_out\"") && !json.contains("\"ffi\""),
        "a bound socket needs no outbound authority:\n{json}"
    );
}

/// An empty ticket slice is refused before the kernel (`Failed(EINVAL)`), and a receive that
/// delivered nothing leaves no ticket behind: the cell is still the 7 it was given.
const NO_CELL: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    var status = 1;
    borrow mut io as &!i in {
        borrow bound as &n in {
            match udp_bind(n, PORT, 0) {
                UdpOpened::Ok(u) => {
                    var sock = u;
                    borrow mut sock as &!uh in {
                        udp_nonblocking(uh);
                        io.error_all(i, "ready\n");
                        region a {
                            var buf = alloc_slice[a](64, byte_of(0));
                            var none = alloc_slice[a](0, 0);
                            var who = alloc_slice[a](1, 7);
                            var seen = 0;
                            match udp_recv_from(uh, buf, none) {
                                Datagram::Failed(e) => { if e == 22 { seen = seen + 1; } }
                                Datagram::Got(k) => { }
                                Datagram::Truncated(k) => { }
                                Datagram::Again => { }
                            }
                            match udp_recv_from(uh, buf, who) {
                                Datagram::Again => { if who[0] == 7 { seen = seen + 1; } }
                                Datagram::Got(k) => { }
                                Datagram::Truncated(k) => { }
                                Datagram::Failed(e) => { }
                            }
                            if seen == 2 { status = 0; }
                        }
                    }
                    udp_close(sock);
                }
                UdpOpened::Failed(e) => { status = 2; }
            }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

#[test]
fn an_empty_ticket_slice_is_einval_and_a_failed_receive_leaves_no_ticket() {
    serve("no-cell", NO_CELL, 0, |_port| {});
}

// ---------------------------------------------------------------------
// Tickets for datagram sockets (`docs/udp.md` §11)
// ---------------------------------------------------------------------

/// One attempt to redeem `TICKET` as a `Udp` that must be refused with `EBADF`.
fn udp_attach_refused(ticket: &str) -> String {
    format!(
        "match udp_attach({ticket}) {{\n\
             UdpOpened::Failed(e) => {{ if e == 9 {{ refused = refused + 1; }} }}\n\
             UdpOpened::Ok(x) => {{ udp_close(x); }}\n\
         }}\n"
    )
}

/// Forged tickets reach nothing, a ticket of the other kind is refused (and not spent), the real one
/// redeems once and the socket still works, and a second redemption is refused.
fn tickets_program() -> String {
    let forged: String = [
        udp_attach_refused("0"),
        udp_attach_refused("0 - 1"),
        udp_attach_refused("1"),
        udp_attach_refused("9223372036854775807"),
        udp_attach_refused("ticket + 1"),
        udp_attach_refused("ticket + 8589934592"),
        udp_attach_refused("ticket - 2147483648"),
        "match conn_attach(ticket) {\n\
             Attached::Failed(e) => { if e == 9 { refused = refused + 1; } }\n\
             Attached::Ok(x) => { conn_close(x); }\n\
         }\n"
        .to_owned(),
    ]
    .concat();
    format!(
        r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {{
    var status = 1;
    borrow bound as &n in {{
        match udp_connect(n, "127.0.0.1", PORT) {{
            UdpOpened::Ok(u) => {{
                let ticket = udp_detach(u);
                var refused = 0;
                if ticket > 0 {{
                    {forged}
                    match udp_attach(ticket) {{
                        UdpOpened::Ok(back) => {{
                            var sock = back;
                            borrow mut sock as &!uh in {{
                                match udp_send(uh, "ping") {{
                                    Sent::Wrote(w) => {{
                                        region a {{
                                            var buf = alloc_slice[a](16, byte_of(0));
                                            match udp_recv(uh, buf) {{
                                                Datagram::Got(k) => {{
                                                    if k == 4 && int_of(buf[0]) == 80 {{ status = 0; }} else {{ status = 5; }}
                                                }}
                                                Datagram::Truncated(k) => {{ status = 6; }}
                                                Datagram::Again => {{ status = 7; }}
                                                Datagram::Failed(e) => {{ status = 8; }}
                                            }}
                                        }}
                                    }}
                                    Sent::Again => {{ status = 9; }}
                                    Sent::Failed(e) => {{ status = 10; }}
                                }}
                            }}
                            udp_close(sock);
                        }}
                        UdpOpened::Failed(e) => {{ status = 3; }}
                    }}
                    {again}
                    if status == 0 && refused != 9 {{ status = 20 + refused; }}
                }} else {{
                    status = 4;
                }}
            }}
            UdpOpened::Failed(e) => {{ status = 2; }}
        }}
    }}
    release(bound);
    release(io);
    return status;
}}
"#,
        again = udp_attach_refused("ticket")
    )
}

#[test]
fn a_datagram_ticket_redeems_once_and_a_forged_one_reaches_nothing() {
    against_peer("tickets", &tickets_program(), 0, |socket| {
        let mut buf = [0u8; 16];
        let (n, from) = socket.recv_from(&mut buf).unwrap();
        socket.send_to(&buf[..n].to_ascii_uppercase(), from).unwrap();
    });
}

/// A `Conn`'s ticket is not a `Udp`'s: `udp_attach` refuses it, it is not spent by the refusal, and it
/// still redeems as the `Conn` it was.
const TCP_TICKET: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match tcp_connect(n, "127.0.0.1", PORT) {
            Dialed::Ok(c) => {
                let ticket = conn_detach(c);
                var score = 0;
                match udp_attach(ticket) {
                    UdpOpened::Failed(e) => { if e == 9 { score = score + 1; } }
                    UdpOpened::Ok(x) => { udp_close(x); }
                }
                match conn_attach(ticket) {
                    Attached::Ok(back) => { conn_close(back); score = score + 1; }
                    Attached::Failed(e) => { }
                }
                if score == 2 { status = 0; } else { status = 10 + score; }
            }
            Dialed::Failed(e) => { status = 2; }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

#[test]
fn a_connections_ticket_is_not_a_datagram_sockets() {
    for backend in BACKENDS {
        // The same port number for a TCP listener and a UDP socket, so one narrowing serves both.
        let (listener, port) = loop {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            if UdpSocket::bind(("127.0.0.1", port)).is_ok() {
                break (listener, port);
            }
        };
        let dir = scratch(&format!("udp-tcp-ticket-{backend}"));
        let exe = build(
            &dir,
            "tcpticket",
            &dial_program(port, &format!("127.0.0.1:{port}"), TCP_TICKET),
            backend,
        );
        let code = run_with_deadline(&exe, "tcpticket", backend);
        assert_eq!(code, Some(0), "{backend}");
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `std.udps`: three sockets by slot, each send and receive finding its own, a closed slot refusing
/// stale use and being reused, and `drop` closing what is left.
const UDPS: &str = r#"
edition 5;
import std.conns;
import std.udps;
import std.io;

fn first[&t, &b](table: &!t conns.Table, slot: int, buf: &!b [byte]) -> [udp_recv] int {
    match udps.recv(table, slot, buf) {
        Datagram::Got(n) => { return int_of(buf[0]); }
        Datagram::Truncated(n) => { return 0 - 1; }
        Datagram::Again => { return 0 - 2; }
        Datagram::Failed(e) => { return 0 - 3; }
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi); release(fs); release(args); release(clock);
    var h = heap;
    let bound = narrow(net, "BOUND");
    var score = 0;
    borrow mut h as &!hh in {
        borrow bound as &n in {
            var table = conns.empty(hh, 2);
            var k = 0;
            while k < 3 {
                match udp_connect(n, "127.0.0.1", PORT) {
                    UdpOpened::Ok(u) => {
                        let (t2, slot) = udps.put(hh, table, u);
                        table = t2;
                        if slot == k { score = score + 1; }
                    }
                    UdpOpened::Failed(e) => { }
                }
                k = k + 1;
            }
            region r {
                var buf = alloc_slice[r](8, byte_of(0));
                borrow mut table as &!tb in {
                    udps.send(tb, 0, "a");
                    udps.send(tb, 1, "b");
                    udps.send(tb, 2, "c");
                    if first(tb, 1, buf) == 66 { score = score + 1; }
                    if first(tb, 0, buf) == 65 { score = score + 1; }
                    if first(tb, 2, buf) == 67 { score = score + 1; }
                    if udps.close(tb, 0) == 0 && conns.live(tb) == 2 { score = score + 1; }
                    match udps.send(tb, 0, "x") {
                        Sent::Failed(e) => { if e == 9 { score = score + 1; } }
                        Sent::Wrote(w) => { }
                        Sent::Again => { }
                    }
                    if udps.close(tb, 0) == 9 { score = score + 1; }
                }
            }
            match udp_connect(n, "127.0.0.1", PORT) {
                UdpOpened::Ok(u) => {
                    let (t3, slot) = udps.put(hh, table, u);
                    table = t3;
                    if slot == 0 { score = score + 1; }
                }
                UdpOpened::Failed(e) => { }
            }
            if udps.drop(hh, table) == 3 { score = score + 1; }
        }
    }
    release(bound);
    release(io);
    release(h);
    if score == 11 {
        return 0;
    }
    return 100 + score;
}
"#;

#[test]
fn a_socket_table_finds_each_socket_by_slot() {
    for backend in BACKENDS {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let port = socket.local_addr().unwrap().port();
        let dir = scratch(&format!("udp-table-{backend}"));
        let source =
            UDPS.replace("BOUND", &format!("127.0.0.1:{port}")).replace("PORT", &port.to_string());
        let exe = build(&dir, "table", &source, backend);
        let peer = std::thread::spawn(move || {
            let mut buf = [0u8; 16];
            for _ in 0..3 {
                let (n, from) = socket.recv_from(&mut buf).unwrap();
                socket.send_to(&buf[..n].to_ascii_uppercase(), from).unwrap();
            }
        });
        let code = run_with_deadline(&exe, "table", backend);
        assert_eq!(code, Some(0), "{backend}");
        peer.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---------------------------------------------------------------------
// Who a ticket names (`docs/udp.md` §12)
// ---------------------------------------------------------------------

/// `udp_peer` writes the sender of a ticket in the 19 bytes `conn_peer` uses; a forged, stale, other-socket or too-short call
/// answers the errno it should and writes nothing.
const UDP_PEER: &str = r#"
fn run(net: Net(""), io: Io) -> [] int {
    var status = 1;
    borrow mut io as &!i in {
        borrow net as &n in {
            match udp_bind(n, PORT, 0) {
                UdpOpened::Ok(u) => {
                    var sock = u;
                    match udp_bind(n, OTHER, 0) {
                        UdpOpened::Ok(v) => {
                            var stranger = v;
                            borrow mut sock as &!uh in {
                                borrow stranger as &sh in {
                                    io.error_all(i, "ready\n");
                                    region a {
                                        var buf = alloc_slice[a](64, byte_of(0));
                                        var who = alloc_slice[a](1, 0);
                                        var out = alloc_slice[a](19, byte_of(7));
                                        var short = alloc_slice[a](18, byte_of(7));
                                        var spare = alloc_slice[a](19, byte_of(7));
                                        match udp_recv_from(uh, buf, who) {
                                            Datagram::Got(k) => {
                                                var bad = 0;
                                                if udp_peer(uh, 0, spare) != 9 { bad = bad + 1; }
                                                if udp_peer(uh, 0 - 5, spare) != 9 { bad = bad + 1; }
                                                if udp_peer(uh, who[0] + 1000, spare) != 9 { bad = bad + 1; }
                                                if udp_peer(uh, who[0] + 65536, spare) != 9 { bad = bad + 1; }
                                                if udp_peer(uh, who[0] - 65536, spare) != 9 { bad = bad + 1; }
                                                if udp_peer(sh, who[0], spare) != 9 { bad = bad + 1; }
                                                if udp_peer(uh, who[0], short) != 22 { bad = bad + 1; }
                                                if int_of(short[0]) != 7 || int_of(short[17]) != 7 || int_of(spare[0]) != 7 || int_of(spare[18]) != 7 { bad = bad + 1; }
                                                let ok = udp_peer(uh, who[0], out);
                                                if ok == 0 && bad == 0 {
                                                    match udp_send_to(uh, out, who[0]) {
                                                        Sent::Wrote(w) => { status = 0; }
                                                        Sent::Again => { status = 6; }
                                                        Sent::Failed(e) => { status = 7; }
                                                    }
                                                } else {
                                                    status = 10 + bad;
                                                }
                                            }
                                            Datagram::Truncated(k) => { status = 3; }
                                            Datagram::Again => { status = 4; }
                                            Datagram::Failed(e) => { status = 8; }
                                        }
                                    }
                                }
                            }
                            udp_close(stranger);
                        }
                        UdpOpened::Failed(e) => { status = 9; }
                    }
                    udp_close(sock);
                }
                UdpOpened::Failed(e) => { status = 2; }
            }
        }
    }
    release(net);
    release(io);
    return status;
}
"#;

#[test]
fn a_ticket_names_its_sender_and_only_on_the_socket_that_heard_it() {
    serve_open("udp-peer", UDP_PEER, 0, |port| {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        socket.send_to(b"hi", ("127.0.0.1", port)).unwrap();
        let mut buf = [0u8; 64];
        let (n, _) = socket.recv_from(&mut buf).unwrap();
        let mine = socket.local_addr().unwrap().port();
        let mut want = [0u8; 19];
        want[0] = 4;
        want[1..5].copy_from_slice(&[127, 0, 0, 1]);
        want[17] = (mine >> 8) as u8;
        want[18] = mine as u8;
        assert_eq!(&buf[..n], &want);
    });
}
