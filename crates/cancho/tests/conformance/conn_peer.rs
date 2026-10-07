//! `docs/conn-peer.md`: the other end of a connection, over real sockets on both backends, and the
//! address type against Rust's `std::net` as an oracle. Every program is `edition 5;` (or 6, for the one
//! that needs `Ffi` to put an IPv6 socket under a listener: see `ipv6_peers`), declares no `extern fn`
//! otherwise, and reads the peer through `std.conns.peer`.

use super::*;
use std::io::{BufRead, BufReader, Read as _};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpStream};
use std::process::Child;
use std::time::Duration;

use super::sockets::{BACKENDS, build};

/// A child that is killed when the test ends, however it ends: a server that never gets its client
/// must not outlive a failed assertion.
struct Server {
    child: Child,
    lines: BufReader<std::process::ChildStdout>,
}

impl Server {
    fn start(exe: &Path, extra: impl FnOnce(&mut Command)) -> Server {
        let mut command = Command::new(exe);
        command.stdout(Stdio::piped()).stderr(Stdio::inherit());
        extra(&mut command);
        let mut child = command.spawn().expect("the server runs");
        let lines = BufReader::new(child.stdout.take().unwrap());
        Server { child, lines }
    }

    fn line(&mut self) -> String {
        let mut line = String::new();
        let n = self.lines.read_line(&mut line).expect("the server's standard output");
        assert!(n > 0, "the server ended before its next line");
        line.trim_end().to_owned()
    }

    fn finish(mut self) -> Option<i32> {
        self.child.wait().expect("the server exits").code()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn connect(port: u16) -> TcpStream {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => return stream,
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(e) => panic!("could not connect within the deadline: {e}"),
        }
    }
}

/// The shared parts of the socket programs: `say` (a line, flushed, so the test can read it as it
/// happens), `take` (accept one and put it in the table), and `show` (a slot's peer as a line).
const PRELUDE: &str = r#"
import std.addr;
import std.conns;
import std.io;

fn say[&i](io: &!i Io, s: &static [byte]) -> [io_write] int {
    io.write_all(io, s);
    match flush_out(io) {
        Done::Ok(n) => { return 0; }
        Done::Failed(e) => { return e; }
    }
}

fn take[&h, &l](heap: &!h Heap, table: conns.Table, listener: &!l Listener) -> [heap, conn_accept] (conns.Table, int) {
    match tcp_accept(listener) {
        Accepted::Ok(c) => { return conns.put(heap, table, c); }
        Accepted::Again => { return (table, 0 - 2); }
        Accepted::Failed(e) => { return (table, 0 - 3); }
    }
}

// `slot <n> peer <address>:<port> key <family> <bits>`, or `slot <n> none <errno>`.
fn show[&i, &t](io: &!i Io, tb: &!t conns.Table, slot: int) -> [io_write] int {
    io.write_all(io, "slot ");
    io.print_int(io, slot);
    match PEER_CALL {
        conns.Peered::Known(p) => {
            region r {
                let buf = alloc_slice[r](64, byte_of(0));
                let k = addr.text_port(p, buf);
                io.write_all(io, " peer ");
                io.write_all(io, buf[0..k]);
                let key = addr.key(p);
                io.write_all(io, " key ");
                io.print_int(io, addr.key_family(key));
                io.write_all(io, " ");
                io.print_int(io, addr.key_bits(key));
            }
        }
        conns.Peered::Unavailable(e) => {
            io.write_all(io, " none ");
            io.print_int(io, e);
        }
    }
    io.newline(io);
    say(io, "");
    return 0;
}
"#;

/// A server whose `main` is `listen`, then `body` with `i` (the `Io`), `hh` (the heap) and `lh` (the
/// listener) in scope. `PORT` is substituted.
fn server(port: u16, body: &str, with_peer: bool) -> String {
    let peer_call = if with_peer { "conns.peer(tb, slot)" } else { "conns.Peered::Unavailable(0)" };
    format!(
        r#"edition 5;
{prelude}
fn main(world: World) -> [] int {{
    let Split {{ io, ffi, fs, heap, args, net, clock }} = split(world);
    release(ffi); release(fs); release(args); release(clock);
    var h = heap;
    var o = io;
    let bound = narrow(net, "{port}");
    borrow mut o as &!i in {{
    borrow mut h as &!hh in {{
        borrow bound as &n in {{
            match tcp_listen(n, {port}, 64, 0) {{
                Listening::Ok(l) => {{
                    var listener = l;
                    borrow mut listener as &!lh in {{
                        say(i, "ready\n");
{body}
                    }}
                    listener_close(listener);
                }}
                Listening::Failed(e) => {{ }}
            }}
        }}
    }}
    }}
    release(bound);
    release(o);
    release(h);
    return 0;
}}
"#,
        prelude = PRELUDE.replace("PEER_CALL", peer_call),
    )
}

/// Accept `COUNT` clients into slots, show each; ask about a slot out of range; close slot 0 and ask
/// again; accept one more (it takes slot 0) and show slots 0 and 1.
const PEERS: &str = r#"
                        var table = conns.empty(hh, 4);
                        var n = 0;
                        while n < COUNT {
                            let (t, slot) = take(hh, table, lh);
                            table = t;
                            borrow mut table as &!tb in { show(i, tb, slot); }
                            n = n + 1;
                        }
                        borrow mut table as &!tb in {
                            show(i, tb, 99);
                            show(i, tb, 0 - 1);
                            conns.close(tb, 0);
                            show(i, tb, 0);
                        }
                        let (t2, again) = take(hh, table, lh);
                        table = t2;
                        borrow mut table as &!tb in {
                            show(i, tb, again);
                            show(i, tb, 1);
                        }
                        conns.drop(hh, table);
"#;

fn peers_source(port: u16, count: usize, with_peer: bool) -> String {
    server(port, &PEERS.replace("COUNT", &count.to_string()), with_peer)
}

/// Eleven clients at once, each from its own source port: every slot's peer is its own client's address
/// and port, a slot out of range or closed has none (`EBADF`), and a slot freed and taken by a later
/// client now answers *that* client's port (docs/conn-peer.md section 7).
#[test]
fn every_slot_answers_its_own_clients_address_and_port() {
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("conn-peer-slots-{backend}"));
        let exe = build(&dir, "peers", &peers_source(port, 11, true), backend);
        let mut server = Server::start(&exe, |_| {});
        assert_eq!(server.line(), "ready");

        let mut clients = Vec::new();
        for slot in 0..11 {
            let stream = connect(port);
            let local = stream.local_addr().unwrap();
            let key = u32::from(Ipv4Addr::LOCALHOST);
            assert_eq!(
                server.line(),
                format!("slot {slot} peer 127.0.0.1:{} key 4 {key}", local.port()),
                "{backend}: slot {slot}"
            );
            clients.push(stream);
        }
        let mut ports: Vec<u16> = clients.iter().map(|c| c.local_addr().unwrap().port()).collect();
        ports.sort();
        ports.dedup();
        assert_eq!(ports.len(), 11, "{backend}: eleven distinct source ports");

        assert_eq!(server.line(), "slot 99 none 9", "{backend}: past the table");
        assert_eq!(server.line(), "slot -1 none 9", "{backend}: negative slot");
        assert_eq!(server.line(), "slot 0 none 9", "{backend}: a closed slot has no peer");

        let extra = connect(port);
        let extra_port = extra.local_addr().unwrap().port();
        let key = u32::from(Ipv4Addr::LOCALHOST);
        assert_eq!(
            server.line(),
            format!("slot 0 peer 127.0.0.1:{extra_port} key 4 {key}"),
            "{backend}: the reused slot answers the new client, not the old one"
        );
        assert_eq!(
            server.line(),
            format!(
                "slot 1 peer 127.0.0.1:{} key 4 {key}",
                clients[1].local_addr().unwrap().port()
            ),
            "{backend}: slot 1 is untouched"
        );
        assert_ne!(extra_port, clients[0].local_addr().unwrap().port());
        assert_eq!(server.finish(), Some(0), "{backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Read the connection in slot 0 until it ends, then ask for its peer.
const AFTER_END: &str = r#"
                        var table = conns.empty(hh, 2);
                        let (t, slot) = take(hh, table, lh);
                        table = t;
                        borrow mut table as &!tb in {
                            region r {
                                let buf = alloc_slice[r](16, byte_of(0));
                                match conns.read(tb, slot, buf) {
                                    Received::Data(k) => { say(i, "read data\n"); }
                                    Received::End => { say(i, "read end\n"); }
                                    Received::Again => { say(i, "read again\n"); }
                                    Received::Failed(e) => { say(i, "read failed\n"); }
                                }
                            }
                            show(i, tb, slot);
                        }
                        conns.drop(hh, table);
"#;

unsafe extern "C" {
    fn setsockopt(fd: i32, level: i32, name: i32, value: *const std::ffi::c_void, len: u32) -> i32;
}

/// Close `stream` so the kernel sends a reset instead of a FIN (`SO_LINGER` with a zero timeout).
fn reset(stream: TcpStream) {
    use std::os::fd::AsRawFd as _;
    let (level, name) = if cfg!(target_os = "macos") { (0xffff, 0x80) } else { (1, 13) };
    let linger: [i32; 2] = [1, 0];
    // SAFETY: `linger` outlives the call, `len` is its size, and the descriptor is open.
    let status = unsafe {
        setsockopt(
            stream.as_raw_fd(),
            level,
            name,
            linger.as_ptr().cast(),
            std::mem::size_of_val(&linger) as u32,
        )
    };
    assert_eq!(status, 0, "SO_LINGER");
    drop(stream);
}

/// The two ways a connection ends before the program asks. A peer that closed normally (FIN) still has
/// an address; one that reset does not, and the kernel says why: `ENOTCONN` (107) on Linux, `EINVAL`
/// (22) on macOS, both measured in docs/conn-peer.md section 2. The rule that follows -- ask right
/// after `put` and keep the answer -- is the document's section 7.
#[test]
fn a_connection_that_ended_has_a_peer_only_if_it_was_not_reset() {
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("conn-peer-ended-{backend}"));
        let exe = build(&dir, "ended", &server(port, AFTER_END, true), backend);
        for resets in [false, true] {
            let mut server = Server::start(&exe, |_| {});
            assert_eq!(server.line(), "ready");
            let client = connect(port);
            let local = client.local_addr().unwrap().port();
            if resets {
                reset(client);
                assert_eq!(server.line(), "read failed", "{backend}");
                let want = if cfg!(target_os = "macos") { 22 } else { 107 };
                assert_eq!(server.line(), format!("slot 0 none {want}"), "{backend}: reset");
            } else {
                drop(client);
                assert_eq!(server.line(), "read end", "{backend}");
                assert_eq!(
                    server.line(),
                    format!("slot 0 peer 127.0.0.1:{local} key 4 2130706433"),
                    "{backend}: a normal close keeps the address"
                );
            }
            assert_eq!(server.finish(), Some(0), "{backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The builtin by itself: a buffer of 18 bytes is refused with `EINVAL` and is not written; one of 19
/// is filled, with the family, the address and the port in the layout of section 3.1, and the twelve
/// bytes after an IPv4 address zero.
const RAW: &str = r#"
edition 5;
import std.io;

fn show[&i, &b](io: &!i Io, name: &static [byte], code: int, b: &b [byte]) -> [io_write] int {
    io.write_all(io, name);
    io.write_all(io, " ");
    io.print_int(io, code);
    var k = 0;
    while k < len(b) {
        io.write_all(io, " ");
        io.print_int(io, int_of(b[k]));
        k = k + 1;
    }
    io.newline(io);
    return 0;
}

fn run(bound: Net("PORT"), io: Io) -> [] int {
    var o = io;
    borrow mut o as &!i in {
        borrow bound as &n in {
            match tcp_listen(n, PORT, 8, 0) {
                Listening::Ok(l) => {
                    var listener = l;
                    borrow mut listener as &!lh in {
                        io.error_all(i, "ready\n");
                        match tcp_accept(lh) {
                            Accepted::Ok(c) => {
                                var conn = c;
                                borrow mut conn as &!ch in {
                                    region a {
                                        let small = alloc_slice[a](18, byte_of(170));
                                        let code = conn_peer(ch, small);
                                        show(i, "short", code, small);
                                        let exact = alloc_slice[a](19, byte_of(170));
                                        let code2 = conn_peer(ch, exact);
                                        show(i, "exact", code2, exact);
                                        let empty = alloc_slice[a](0, byte_of(170));
                                        let code3 = conn_peer(ch, empty);
                                        show(i, "empty", code3, empty);
                                        let big = alloc_slice[a](25, byte_of(170));
                                        let code4 = conn_peer(ch, big);
                                        show(i, "big", code4, big);
                                    }
                                }
                                conn_close(conn);
                            }
                            Accepted::Again => { }
                            Accepted::Failed(e) => { }
                        }
                    }
                    listener_close(listener);
                }
                Listening::Failed(e) => { }
            }
        }
    }
    release(bound);
    release(o);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi); release(fs); release(heap); release(args); release(clock);
    let bound = narrow(net, "PORT");
    return run(bound, io);
}
"#;

#[test]
fn the_builtin_fills_nineteen_bytes_and_refuses_fewer_without_writing() {
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("conn-peer-raw-{backend}"));
        let exe = build(&dir, "raw", &RAW.replace("PORT", &port.to_string()), backend);
        let mut command = Command::new(&exe);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().expect("the program runs");
        let mut err = BufReader::new(child.stderr.take().unwrap());
        super::sockets::wait_for(&mut err, "ready");
        let client = connect(port);
        let local = client.local_addr().unwrap().port();
        let mut out = String::new();
        child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
        assert_eq!(child.wait().unwrap().code(), Some(0), "{backend}");
        drop(client);

        let (hi, lo) = (local >> 8, local & 255);
        let mut lines = out.lines();
        let a = "170 ".repeat(18);
        assert_eq!(lines.next().unwrap(), format!("short 22 {}", a.trim_end()), "{backend}");
        assert_eq!(
            lines.next().unwrap(),
            format!("exact 0 4 127 0 0 1 0 0 0 0 0 0 0 0 0 0 0 0 {hi} {lo}"),
            "{backend}"
        );
        assert_eq!(lines.next().unwrap(), "empty 22", "{backend}");
        assert_eq!(
            lines.next().unwrap(),
            format!("big 0 4 127 0 0 1 0 0 0 0 0 0 0 0 0 0 0 0 {hi} {lo} 170 170 170 170 170 170"),
            "{backend}: only 19 bytes are written"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A program that calls `conns.peer` reports the same authority as one that does not: no new label,
/// no wider capability (docs/conn-peer.md section 9).
#[test]
fn asking_for_the_peer_adds_nothing_to_the_authority_report() {
    let with = authority_json(&peers_source(8080, 2, true), "conn-peer-authority-with");
    let without = authority_json(&peers_source(8080, 2, false), "conn-peer-authority-without");
    // Everything the report says about authority; the list of pure functions is about the program's
    // own functions (the call adds a few), not about what it can reach.
    let authority = |json: &str| {
        let from = json.find("\"effects\"").expect("effects");
        let to = json.find("\"pure\"").expect("pure");
        json[from..to].to_owned()
    };
    assert_eq!(authority(&with), authority(&without), "the call must not change the report");
    assert!(with.contains("\"net_in\""), "the report is the real one:\n{with}");
    assert!(!with.contains("\"ffi\""), "no foreign code:\n{with}");
    for label in ["conn_accept", "heap", "io_write", "net_in"] {
        assert!(with.contains(label), "`{label}` is expected:\n{with}");
    }
    // The labels it has, and no other: a new one would be a widening.
    let mut names: Vec<&str> =
        with.split("\"name\": \"").skip(1).filter_map(|rest| rest.split('"').next()).collect();
    names.sort();
    names.dedup();
    assert!(
        names.iter().all(|n| !n.contains("peer") && !n.contains("addr")),
        "no label for the peer: {names:?}"
    );
}

/// Dialling an address does not follow from knowing it: a program that holds `Net("127.0.0.1:PORT")`
/// and a peer address for another host is refused when it dials that host. The check is the bound's,
/// at the call site, as before (docs/conn-peer.md section 9).
#[test]
fn knowing_an_address_does_not_let_a_program_dial_it() {
    let source = r#"edition 5;
import std.addr;
import std.io;

fn run(bound: Net("127.0.0.1:9"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        // An address the program was "told" (a Peer is only four integers), spelled out.
        let somebody = addr.v4(203, 0, 113, 7, 80);
        match tcp_connect(n, "203.0.113.7", addr.port(somebody)) {
            Dialed::Ok(c) => { conn_close(c); status = 2; }
            Dialed::Failed(e) => { status = 3; }
        }
    }
    release(bound);
    release(io);
    return status;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi); release(fs); release(heap); release(args); release(clock);
    let bound = narrow(net, "127.0.0.1:9");
    return run(bound, io);
}
"#;
    for backend in BACKENDS {
        let dir = scratch(&format!("conn-peer-dial-{backend}"));
        let exe = build(&dir, "dial", source, backend);
        let run = Command::new(&exe).output().expect("the program runs");
        assert_ne!(
            run.status.code(),
            Some(2),
            "{backend}: it must not dial a host outside its bound"
        );
        assert_ne!(run.status.code(), Some(3), "{backend}: it traps, it does not answer Failed");
        assert!(run.status.code().is_none(), "{backend}: a trap is a signal, got {:?}", run.status);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---- IPv6: an AF_INET6 socket under a listener -------------------------------------------------

/// Every cancho socket is IPv4 today (docs/conn-peer.md section 2), so no cancho listener can be
/// reached from `::1`. The IPv6 half of the builtin is still the part a dual-stack listener will run,
/// so this test puts a dual-stack socket the harness made under the program's listener: the child
/// inherits it as descriptor 10 (`pre_exec`), and the program, which holds `Ffi("libc")` for this one
/// purpose, `dup2`s it over its listener's descriptor 3 -- the lowest free one in a fresh process --
/// before the first `accept`. Everything after that is the program an operator would write.
fn ipv6_source(port: u16, count: usize) -> String {
    let body = format!(
        r#"edition 6;
{prelude}
extern fn dup2[&f](ffi: &f Ffi("libc"), old: int, new: int) -> [ffi("libc")] c_int;

fn under_the_listener[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] int {{
    return dup2(ffi, 10, 3);
}}

fn main(world: World) -> [] int {{
    let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);
    release(fs); release(args); release(clock); release(signals);
    var h = heap;
    var o = io;
    let bound = narrow(net, "{port}");
    let libc = narrow(ffi, "libc");
    borrow mut o as &!i in {{
    borrow mut h as &!hh in {{
        borrow bound as &n in {{
            match tcp_listen(n, {port}, 64, 0) {{
                Listening::Ok(l) => {{
                    var listener = l;
                    borrow libc as &f in {{
                        if under_the_listener(f) != 3 {{ say(i, "dup2 failed\n"); }}
                    }}
                    borrow mut listener as &!lh in {{
                        say(i, "ready\n");
                        var table = conns.empty(hh, 4);
                        var n = 0;
                        while n < {count} {{
                            let (t, slot) = take(hh, table, lh);
                            table = t;
                            borrow mut table as &!tb in {{ show(i, tb, slot); }}
                            n = n + 1;
                        }}
                        conns.drop(hh, table);
                    }}
                    listener_close(listener);
                }}
                Listening::Failed(e) => {{ }}
            }}
        }}
    }}
    }}
    release(libc);
    release(bound);
    release(o);
    release(h);
    return 0;
}}
"#,
        prelude = PRELUDE.replace("PEER_CALL", "conns.peer(tb, slot)"),
    );
    body
}

#[cfg(unix)]
#[test]
fn an_ipv6_peer_is_read_and_an_ipv4_one_on_the_same_socket_is_normalised() {
    use std::os::fd::AsRawFd as _;
    use std::os::unix::process::CommandExt as _;
    unsafe extern "C" {
        fn dup2(old: i32, new: i32) -> i32;
    }
    let Ok(listener) = std::net::TcpListener::bind("[::]:0") else {
        eprintln!("note: this machine has no IPv6; the IPv6 peer test is skipped");
        return;
    };
    let v6_port = listener.local_addr().unwrap().port();
    // The harness can dial ::1 only if loopback has it; check before blaming the program.
    let Ok(probe) = TcpStream::connect(("::1", v6_port)) else {
        eprintln!("note: ::1 is not reachable here; the IPv6 peer test is skipped");
        return;
    };
    drop(probe);
    let _ = listener.accept();
    let fd = listener.as_raw_fd();

    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("conn-peer-v6-{backend}"));
        let exe = build(&dir, "v6", &ipv6_source(port, 3), backend);
        let mut server = Server::start(&exe, |c| {
            // SAFETY: runs in the child between fork and exec; `dup2` is async-signal-safe.
            unsafe {
                c.pre_exec(move || {
                    if dup2(fd, 10) < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
                });
            }
        });
        assert_eq!(server.line(), "ready", "{backend}");

        let six = TcpStream::connect(("::1", v6_port)).unwrap();
        let six_port = six.local_addr().unwrap().port();
        // The key of `::1` is its /64: the first eight bytes, all zero.
        assert_eq!(
            server.line(),
            format!("slot 0 peer [::1]:{six_port} key 6 0"),
            "{backend}: an IPv6 peer"
        );
        let four = TcpStream::connect(("127.0.0.1", v6_port)).unwrap();
        let four_port = four.local_addr().unwrap().port();
        // The kernel reports ::ffff:127.0.0.1; the program sees 127.0.0.1 (section 4).
        assert_eq!(
            server.line(),
            format!("slot 1 peer 127.0.0.1:{four_port} key 4 2130706433"),
            "{backend}: an IPv4-mapped peer is the IPv4 address"
        );
        // A second /64 address would be a different key; the machine has only ::1, so this
        // checks the port is what differs between two connections from one address.
        let again = TcpStream::connect(("::1", v6_port)).unwrap();
        let again_port = again.local_addr().unwrap().port();
        assert_eq!(server.line(), format!("slot 2 peer [::1]:{again_port} key 6 0"), "{backend}");
        assert_ne!(six_port, again_port);
        assert_eq!(server.finish(), Some(0), "{backend}");
        drop((six, four, again));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---- std.addr against Rust's std::net ----------------------------------------------------------

/// A deterministic generator (splitmix64); the corpus is the same on every run and every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// RFC 5952 text of an IPv6 address, written here so the oracle does not depend on which special
/// cases the toolchain's `Display` has (older ones printed `::a.b.c.d` for IPv4-compatible).
fn rfc5952(a: Ipv6Addr) -> String {
    let g = a.segments();
    let (mut best, mut best_len) = (0, 0);
    let mut i = 0;
    while i < 8 {
        if g[i] == 0 {
            let mut j = i;
            while j < 8 && g[j] == 0 {
                j += 1;
            }
            if j - i > best_len {
                best = i;
                best_len = j - i;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    let hex = |s: &[u16]| s.iter().map(|x| format!("{x:x}")).collect::<Vec<_>>().join(":");
    if best_len < 2 {
        return hex(&g);
    }
    format!("{}::{}", hex(&g[..best]), hex(&g[best + best_len..]))
}

/// What `std.addr` must answer for `a`, text and key, with IPv4-mapped IPv6 as IPv4.
fn expected(a: IpAddr) -> String {
    let a = match a {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        other => other,
    };
    match a {
        IpAddr::V4(v4) => format!("{v4} 4 {}", u32::from(v4)),
        IpAddr::V6(v6) => {
            let o = v6.octets();
            let bits = i64::from_be_bytes(o[..8].try_into().unwrap());
            format!("{} 6 {bits}", rfc5952(v6))
        }
    }
}

/// One address spelled in one of the ways the grammar allows.
fn spelling(rng: &mut Rng, a: Ipv6Addr) -> String {
    let g = a.segments();
    match rng.below(6) {
        0 => rfc5952(a),
        1 => rfc5952(a).to_uppercase(),
        2 => g.iter().map(|x| format!("{x:04x}")).collect::<Vec<_>>().join(":"),
        3 => g.iter().map(|x| format!("{x:X}")).collect::<Vec<_>>().join(":"),
        4 => {
            // The last two groups as a dotted quad.
            let head = g[..6].iter().map(|x| format!("{x:x}")).collect::<Vec<_>>().join(":");
            format!("{head}:{}.{}.{}.{}", g[6] >> 8, g[6] & 255, g[7] >> 8, g[7] & 255)
        }
        _ => {
            // `::` over a random run of zero groups (any length from one), when there is one.
            let start = rng.below(8) as usize;
            let mut end = start;
            while end < 8 && g[end] == 0 {
                end += 1;
            }
            if end == start {
                g.iter().map(|x| format!("{x:x}")).collect::<Vec<_>>().join(":")
            } else {
                let stop = start + 1 + rng.below((end - start) as u64) as usize;
                let hex =
                    |s: &[u16]| s.iter().map(|x| format!("{x:x}")).collect::<Vec<_>>().join(":");
                format!("{}::{}", hex(&g[..start]), hex(&g[stop..]))
            }
        }
    }
}

fn random_v6(rng: &mut Rng) -> Ipv6Addr {
    let mut g = [0u16; 8];
    for x in &mut g {
        *x = match rng.below(4) {
            0 | 1 => 0,
            2 => rng.below(0x10000) as u16,
            _ => [1, 0xffff, 0x2001, 0xdb8, 0xfe80, 0x10, 0x100, 0x1000][rng.below(8) as usize],
        };
    }
    // The mapped range and its neighbours.
    if rng.below(8) == 0 {
        g[..5].fill(0);
    }
    if rng.below(8) == 0 {
        g[..5].fill(0);
        g[5] = [0xffff, 0xfffe, 0, 0xffff][rng.below(4) as usize];
    }
    Ipv6Addr::from(g)
}

fn mutate(rng: &mut Rng, s: &str) -> String {
    let mut b: Vec<u8> = s.bytes().collect();
    let alphabet = b"0123456789abcdefABCDEFg:.:%[]/ x-+\t";
    for _ in 0..=rng.below(3) {
        let at = if b.is_empty() { 0 } else { rng.below(b.len() as u64) as usize };
        match rng.below(3) {
            0 if !b.is_empty() => {
                b.remove(at);
            }
            1 => b.insert(at, alphabet[rng.below(alphabet.len() as u64) as usize]),
            _ if !b.is_empty() => b[at] = alphabet[rng.below(alphabet.len() as u64) as usize],
            _ => {}
        }
    }
    String::from_utf8_lossy(&b).into_owned()
}

/// The lines the program reads, with what each must answer. `A text` asks `parse`; `R hex` asks `decode`.
fn corpus() -> Vec<(String, String)> {
    let mut rng = Rng(0x00c0_ffee);
    let mut cases: Vec<(String, String)> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    for _ in 0..1200 {
        texts.push(Ipv4Addr::from(rng.next() as u32).to_string());
    }
    for _ in 0..2500 {
        let a = random_v6(&mut rng);
        texts.push(spelling(&mut rng, a));
    }
    // By hand: the ends of the grammar.
    for s in [
        "::",
        "::1",
        "1::",
        "::ffff:1.2.3.4",
        "::ffff:102:304",
        "::FFFF:1.2.3.4",
        "::fffe:1.2.3.4",
        "0:0:0:0:0:ffff:1.2.3.4",
        "1:2:3:4:5:6:7:8",
        "1:2:3:4:5:6:7::",
        "::2:3:4:5:6:7:8",
        "1:2:3:4:5:6:1.2.3.4",
        "::1.2.3.4",
        "255.255.255.255",
        "0.0.0.0",
        "1.2.3.4",
        "2001:db8::",
        "fe80::1",
        "64:ff9b::1.2.3.4",
        "1::2:0:0:3",
    ] {
        texts.push(s.to_owned());
    }
    let valid = texts.len();
    for k in 0..valid {
        let m = mutate(&mut rng, &texts[k].clone());
        texts.push(m);
    }
    // Strings of the alphabet, mostly not addresses.
    let alphabet = b"0123456789abcdefABCDEF:::...%[]/ ";
    for _ in 0..2000 {
        let n = rng.below(48) as usize;
        let s: String =
            (0..n).map(|_| alphabet[rng.below(alphabet.len() as u64) as usize] as char).collect();
        texts.push(s);
    }
    texts.push(String::new());
    texts.push("1.2.3.4.5".into());
    texts.push("1:2:3:4:5:6:7:8:9".into());
    texts.push(":::".into());
    texts.push("1:::2".into());
    for t in texts {
        if t.len() > 60 || t.contains('\n') {
            continue;
        }
        let want = match t.parse::<IpAddr>() {
            Ok(a) => expected(a),
            Err(_) => "bad".to_owned(),
        };
        cases.push((format!("A {t}"), want));
    }
    // Raw buffers, as `conn_peer` writes them: both families, the mapped range, and families that are neither.
    for _ in 0..1500 {
        let mut raw = [0u8; 19];
        for b in &mut raw {
            *b = rng.next() as u8;
        }
        raw[0] = [4, 4, 6, 6, 6, 0, 5, 255][rng.below(8) as usize];
        if rng.below(3) == 0 {
            raw[1..11].fill(0);
            raw[11..13].fill(0xff);
        }
        let port = u16::from_be_bytes([raw[17], raw[18]]);
        let hex: String = raw.iter().map(|b| format!("{b:02x}")).collect();
        let want = match raw[0] {
            4 => format!("{}:{port}", Ipv4Addr::new(raw[1], raw[2], raw[3], raw[4])),
            6 => {
                let a = Ipv6Addr::from(<[u8; 16]>::try_from(&raw[1..17]).unwrap());
                match a.to_ipv4_mapped() {
                    Some(v4) => format!("{v4}:{port}"),
                    None => format!("[{}]:{port}", rfc5952(a)),
                }
            }
            _ => "bad".to_owned(),
        };
        cases.push((format!("R {hex}"), want));
    }
    cases
}

/// Reads a line at a time from standard input: `A <text>` or `R <38 hex digits>`; answers one line each.
const ADDR_DIFF: &str = r#"edition 5;
import std.addr;
import std.io;

// A line into `buf`: its length, -1 at the end of input, -2 if it did not fit (the rest is consumed).
fn read_line[&i, &b](io: &!i Io, buf: &!b [byte]) -> [io_read] int {
    var n = 0;
    var over = false;
    var going = true;
    while going {
        let c = getchar(io);
        if c < 0 {
            if n == 0 && !over {
                return 0 - 1;
            }
            going = false;
        } else if c == 10 {
            going = false;
        } else if n < len(buf) {
            buf[n] = byte_of(c);
            n = n + 1;
        } else {
            over = true;
        }
    }
    if over {
        return 0 - 2;
    }
    return n;
}

fn hex(c: int) -> [] int {
    if c >= '0' && c <= '9' { return c - '0'; }
    if c >= 'a' && c <= 'f' { return c - 'a' + 10; }
    return 0 - 1;
}

// The peer as the 19 bytes `conn_peer` writes, spelled as an IPv6 socket would spell it (an IPv4
// address mapped), so `decode` is asked to normalise it.
fn mapped_raw[&b](p: addr.Peer, raw: &!b [byte]) -> [] int {
    raw[0] = byte_of(6);
    var w = 0;
    while w < 4 {
        var word = addr.word(p, w);
        if addr.family(p) == 4 && w == 2 {
            word = 0xffff;
        }
        var k = 0;
        while k < 4 {
            raw[1 + w * 4 + k] = byte_of((word >> (24 - 8 * k)) & 255);
            k = k + 1;
        }
        w = w + 1;
    }
    raw[17] = byte_of((addr.port(p) >> 8) & 255);
    raw[18] = byte_of(addr.port(p) & 255);
    return 0;
}

fn answer_parse[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {
    match addr.parse(s) {
        addr.Parsed::Ok(p) => {
            region r {
                let buf = alloc_slice[r](64, byte_of(0));
                let k = addr.text(p, buf);
                io.write_all(io, buf[0..k]);
                let key = addr.key(p);
                io.write_all(io, " ");
                io.print_int(io, addr.key_family(key));
                io.write_all(io, " ");
                io.print_int(io, addr.key_bits(key));
                // The text parses back to the same address; so does the mapped raw form.
                match addr.parse(buf[0..k]) {
                    addr.Parsed::Ok(q) => {
                        if !addr.same(p, q) { io.write_all(io, " ROUNDTRIP-FAIL"); }
                    }
                    addr.Parsed::Bad => { io.write_all(io, " ROUNDTRIP-BAD"); }
                }
                let raw = alloc_slice[r](19, byte_of(0));
                mapped_raw(p, raw);
                match addr.decode(raw) {
                    addr.Parsed::Ok(q) => {
                        if !addr.same(p, q) { io.write_all(io, " DECODE-FAIL"); }
                    }
                    addr.Parsed::Bad => { io.write_all(io, " DECODE-BAD"); }
                }
                // A buffer under the maximum is refused and left alone.
                let small = alloc_slice[r](46, byte_of(7));
                if addr.text(p, small) != 0 - 1 || int_of(small[0]) != 7 { io.write_all(io, " SMALL-FAIL"); }
            }
        }
        addr.Parsed::Bad => { io.write_all(io, "bad"); }
    }
    io.newline(io);
    return 0;
}

fn answer_raw[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {
    if len(s) != 38 {
        io.write_all(io, "bad\n");
        return 0;
    }
    region r {
        let raw = alloc_slice[r](19, byte_of(0));
        var k = 0;
        while k < 19 {
            raw[k] = byte_of(hex(int_of(s[2 * k])) * 16 + hex(int_of(s[2 * k + 1])));
            k = k + 1;
        }
        match addr.decode(raw) {
            addr.Parsed::Ok(p) => {
                let buf = alloc_slice[r](64, byte_of(0));
                let n = addr.text_port(p, buf);
                io.write_all(io, buf[0..n]);
            }
            addr.Parsed::Bad => { io.write_all(io, "bad"); }
        }
    }
    io.newline(io);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi); release(fs); release(heap); release(args); release(net); release(clock);
    var o = io;
    borrow mut o as &!i in {
        region r {
            let line = alloc_slice[r](64, byte_of(0));
            var going = true;
            while going {
                let n = read_line(i, line);
                if n == 0 - 1 {
                    going = false;
                } else if n == 0 - 2 {
                    io.write_all(i, "bad\n");
                } else if n >= 2 && int_of(line[0]) == 'A' {
                    answer_parse(i, line[2..n]);
                } else if n >= 2 && int_of(line[0]) == 'R' {
                    answer_raw(i, line[2..n]);
                } else {
                    io.write_all(i, "bad\n");
                }
            }
        }
    }
    release(o);
    return 0;
}
"#;

/// `parse`, `text`, `text_port`, `decode` and `key` against Rust's `std::net` parser and an
/// independent RFC 5952 formatter, over a generated corpus: valid spellings (upper case, padded,
/// `::` anywhere, a dotted tail, the mapped range and its neighbours), single-character mutations of
/// them, strings of the alphabet, and raw buffers with families that are not 4 or 6. The corpus is also
/// the "no input reaches a panic" test: a program that trapped would exit by signal, and every line
/// must be answered.
#[test]
fn the_address_type_agrees_with_rust_on_a_generated_corpus() {
    let cases = corpus();
    assert!(cases.len() > 9000, "the corpus is {} lines", cases.len());
    let accepted = cases.iter().filter(|(_, want)| want != "bad").count();
    assert!(
        accepted > 3000 && cases.len() - accepted > 2000,
        "{accepted} accepted of {}",
        cases.len()
    );
    let input: String = cases.iter().map(|(line, _)| format!("{line}\n")).collect();
    for backend in BACKENDS {
        let dir = scratch(&format!("conn-peer-diff-{backend}"));
        let exe = build(&dir, "diff", ADDR_DIFF, backend);
        let mut child = Command::new(&exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the program runs");
        let mut stdin = child.stdin.take().unwrap();
        let bytes = input.clone().into_bytes();
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
        let output = child.wait_with_output().unwrap();
        writer.join().unwrap();
        assert_eq!(output.status.code(), Some(0), "{backend}: {:?}", output.status);
        let got: Vec<String> =
            String::from_utf8_lossy(&output.stdout).lines().map(str::to_owned).collect();
        assert_eq!(got.len(), cases.len(), "{backend}: one answer a line");
        let mut wrong = Vec::new();
        for ((line, want), got) in cases.iter().zip(&got) {
            if want != got {
                wrong.push(format!("{line:?}: wanted {want:?}, got {got:?}"));
            }
        }
        assert!(
            wrong.is_empty(),
            "{backend}: {} disagreements, the first ten:\n{}",
            wrong.len(),
            wrong.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The rest of the API, by hand: constructors normalise and mask, equality, keys, the sizes.
const ADDR_API: &str = r#"edition 5;
import std.addr;
import std.test;

fn text_of[&b](p: addr.Peer, buf: &!b [byte]) -> [] int {
    return addr.text(p, buf);
}

fn is_text[&b](p: addr.Peer, buf: &!b [byte], want: &static [byte]) -> [] bool {
    let n = addr.text(p, buf);
    if n != len(want) { return false; }
    var i = 0;
    while i < n {
        if buf[i] != want[i] { return false; }
        i = i + 1;
    }
    return true;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock);
    region r {
        let buf = alloc_slice[r](64, byte_of(0));
        // Constructors mask: an octet is 8 bits, a port 16.
        test.assert(is_text(addr.v4(256 + 10, 0, 0, 1, 70000), buf, "10.0.0.1"));
        test.assert_eq(addr.port(addr.v4(1, 2, 3, 4, 65536 + 80)), 80);
        // The mapped range is IPv4, whichever constructor made it, and nothing near it is.
        test.assert(addr.same(addr.v6(0, 0, 0xffff, 0x01020304, 9), addr.v4(1, 2, 3, 4, 9)));
        test.assert_eq(addr.family(addr.v6(0, 0, 0xffff, 0, 1)), 4);
        test.assert_eq(addr.family(addr.v6(0, 0, 0xfffe, 0x01020304, 1)), 6);
        test.assert_eq(addr.family(addr.v6(0, 1, 0xffff, 0x01020304, 1)), 6);
        test.assert_eq(addr.family(addr.v6(1, 0, 0xffff, 0x01020304, 1)), 6);
        test.assert_eq(addr.family(addr.v6(0, 0, 0, 0x01020304, 1)), 6);
        // Equality: the port is part of `same`, not of `same_address`.
        test.assert(!addr.same(addr.v4(1, 2, 3, 4, 1), addr.v4(1, 2, 3, 4, 2)));
        test.assert(addr.same_address(addr.v4(1, 2, 3, 4, 1), addr.v4(1, 2, 3, 4, 2)));
        test.assert(!addr.same_address(addr.v4(0, 0, 0, 1, 1), addr.v6(0, 0, 0, 1, 1)));
        test.assert_eq(addr.port(addr.with_port(addr.v4(1, 2, 3, 4, 1), 443)), 443);
        // Keys: an IPv4 address is its own key; an IPv6 address is its /64.
        test.assert(addr.same_key(addr.key(addr.v4(9, 9, 9, 9, 1)), addr.key(addr.v4(9, 9, 9, 9, 2))));
        test.assert(!addr.same_key(addr.key(addr.v4(9, 9, 9, 9, 1)), addr.key(addr.v4(9, 9, 9, 8, 1))));
        test.assert(addr.same_key(addr.key(addr.v6(0x20010db8, 1, 5, 6, 1)), addr.key(addr.v6(0x20010db8, 1, 0xffffffff, 0xffffffff, 2))));
        test.assert(!addr.same_key(addr.key(addr.v6(0x20010db8, 1, 5, 6, 1)), addr.key(addr.v6(0x20010db8, 2, 5, 6, 1))));
        test.assert(!addr.same_key(addr.key(addr.v6(0x20010db9, 1, 5, 6, 1)), addr.key(addr.v6(0x20010db8, 1, 5, 6, 1))));
        // The last bit of the /64 matters; the first bit of the rest does not.
        test.assert(!addr.same_key(addr.key(addr.v6(0, 0, 0, 1, 1)), addr.key(addr.v6(0, 1, 0, 1, 1))));
        test.assert(addr.same_key(addr.key(addr.v6(0, 0, 0, 1, 1)), addr.key(addr.v6(0, 0, 0x80000000, 1, 1))));
        // The two families never share a key, even for the same bits: `::` and `0.0.0.0`.
        test.assert(!addr.same_key(addr.key(addr.v4(0, 0, 0, 0, 1)), addr.key(addr.v6(0, 0, 0, 0, 1))));
        // A key rebuilt from its parts is the key.
        let k = addr.key(addr.v6(0xfe800000, 0x00000001, 7, 7, 1));
        test.assert(addr.same_key(k, addr.key_from(addr.key_family(k), addr.key_bits(k))));
        // A top bit set is a negative number, and still a key.
        test.assert(addr.key_bits(addr.key(addr.v6(0x80000000, 0, 0, 0, 1))) < 0);
        // Text and its limits.
        test.assert(is_text(addr.v6(0x20010db8, 0, 0, 1, 1), buf, "2001:db8::1"));
        test.assert(is_text(addr.v6(0, 0, 0, 0, 1), buf, "::"));
        test.assert(is_text(addr.v6(0x00010000, 0x00020000, 0x00030000, 0x00040000, 1), buf, "1:0:2:0:3:0:4:0"));
        test.assert_eq(addr.text_port(addr.v4(1, 2, 3, 4, 80), buf), 10);
        test.assert_eq(addr.text_port(addr.v6(0, 0, 0, 1, 65535), buf), 11);
        test.assert_eq(addr.max_text(), 47);
        test.assert_eq(addr.raw_len(), 19);
        let small = alloc_slice[r](46, byte_of(9));
        test.assert_eq(addr.text(addr.v4(1, 2, 3, 4, 1), small), 0 - 1);
        test.assert_eq(addr.text_port(addr.v4(1, 2, 3, 4, 1), small), 0 - 1);
        test.assert_eq(int_of(small[0]), 9);
        // The longest text there is.
        let long = addr.v6(0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 65535);
        test.assert_eq(addr.text_port(long, buf), 47);
        // `decode` refuses what is not 19 bytes of family 4 or 6.
        let short = alloc_slice[r](18, byte_of(0));
        short[0] = byte_of(4);
        match addr.decode(short) { addr.Parsed::Ok(p) => { test.assert(false); } addr.Parsed::Bad => { } }
        let raw = alloc_slice[r](19, byte_of(0));
        match addr.decode(raw) { addr.Parsed::Ok(p) => { test.assert(false); } addr.Parsed::Bad => { } }
        raw[0] = byte_of(6);
        raw[12] = byte_of(0xff);
        raw[11] = byte_of(0xff);
        raw[13] = byte_of(10); raw[14] = byte_of(1); raw[15] = byte_of(2); raw[16] = byte_of(3);
        raw[17] = byte_of(1); raw[18] = byte_of(187);
        match addr.decode(raw) {
            addr.Parsed::Ok(p) => { test.assert(addr.same(p, addr.v4(10, 1, 2, 3, 443))); }
            addr.Parsed::Bad => { test.assert(false); }
        }
    }
    return 0;
}
"#;

#[test]
fn the_address_api_does_what_the_document_says() {
    for backend in BACKENDS {
        let dir = scratch(&format!("conn-peer-api-{backend}"));
        let exe = build(&dir, "api", ADDR_API, backend);
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), Some(0), "{backend}: {:?}", run.status);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
