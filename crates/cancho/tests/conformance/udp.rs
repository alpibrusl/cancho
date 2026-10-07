//! `docs/udp.md`: datagram sockets, over real sockets, on both backends. Every program is
//! `edition 5;` and declares no `extern fn` and holds no `Ffi`.

use super::sockets::{BACKENDS, build, dial_program};
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
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(
            run.status.code(),
            Some(expect),
            "{backend}/{tag}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        thread.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
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

/// Two sockets get two kernel-chosen ports, both real: the way to a fresh source port is a
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
                                if p > 0 && q > 0 && p != q { status = 0; } else { status = 5; }
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
