//! `docs/conn-peer.md`: the other end of a connection, over real sockets on both backends. What is left
//! here is what a shell script cannot do: a connection that the client resets (`SO_LINGER` with a zero
//! timeout) and a dual-stack IPv6 socket made by the test and put under the program's listener. The rest
//! of the file (the slots, the builtin, the authority report, the dial refusal, the address type over a
//! corpus and its API) moved to `tests/commands/conn_peer.sh` (`docs/gate.md` G4, wave 4).

use super::*;
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::process::Child;
use std::time::Duration;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// Build `source` with `backend`, returning the executable's path.
fn build(dir: &Path, name: &str, source: &str, backend: &str) -> PathBuf {
    let file = dir.join(format!("{name}.cho"));
    std::fs::write(&file, source).expect("a writable fixture");
    let exe = dir.join(format!("{name}-{backend}"));
    let build = Command::new(BIN)
        .args(["build".as_ref(), file.as_os_str(), "--std".as_ref(), "--backend".as_ref()])
        .args([backend.as_ref(), "-o".as_ref(), exe.as_os_str()])
        .output()
        .expect("the compiler runs");
    assert!(
        build.status.success(),
        "`--backend {backend}` should build `{name}`, but said:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    exe
}

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
