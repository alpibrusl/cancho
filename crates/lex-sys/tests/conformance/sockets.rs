//! `docs/native-sockets.md` §3: the socket handles, over real sockets, on
//! both backends. Every program here is `edition 5;` and declares no
//! `extern fn` and holds no `Ffi` -- that is the point.

use super::*;
use std::io::Read as _;
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant};

/// Build `source` with `backend`, returning the executable's path.
fn build(dir: &Path, name: &str, source: &str, backend: &str) -> PathBuf {
    let file = dir.join(format!("{name}.ls"));
    std::fs::write(&file, source).expect("a writable fixture");
    let exe = dir.join(format!("{name}-{backend}"));
    let build = Command::new(BIN)
        .args([
            "build".as_ref(),
            file.as_os_str(),
            "--std".as_ref(),
            "--backend".as_ref(),
            backend.as_ref(),
            "-o".as_ref(),
            exe.as_os_str(),
        ])
        .output()
        .expect("the compiler runs");
    assert!(
        build.status.success(),
        "`--backend {backend}` should build `{name}`, but said:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    exe
}

fn connect(port: u16) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => {
                stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
                return stream;
            }
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => panic!("could not connect within the deadline: {e}"),
        }
    }
}

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// The prologue every program here shares: split the world, keep the
/// network, release the rest.
fn program(port: u16, body: &str) -> String {
    format!(
        "edition 5;\n\
         {body}\n\
         fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args, net, clock }} = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args); release(clock);\n\
             let bound = narrow(net, \"{port}\");\n\
             let status = run(bound);\n\
             return status;\n\
         }}\n"
    )
}

/// A listener that accepts one connection and echoes it until the peer
/// is done: a `Listening`, an `Accepted`, then `Received` and `Sent` in a
/// loop, and both handles closed.
fn echo_program(port: u16) -> String {
    program(
        port,
        &format!(
            "fn echo[&c, &b](conn: &!c Conn, buf: &!b [byte]) -> [conn_read, conn_write] int {{\n\
                 var total = 0;\n\
                 var open = true;\n\
                 while open {{\n\
                     match conn_read(conn, buf) {{\n\
                         Received::Data(n) => {{\n\
                             match conn_write(conn, buf[0..n]) {{\n\
                                 Sent::Wrote(k) => {{ total = total + k; }}\n\
                                 Sent::Again => {{ open = false; total = 0 - 3; }}\n\
                                 Sent::Failed(e) => {{ open = false; total = 0 - 1; }}\n\
                             }}\n\
                         }}\n\
                         Received::End => {{ open = false; }}\n\
                         Received::Again => {{ open = false; total = 0 - 4; }}\n\
                         Received::Failed(e) => {{ open = false; total = 0 - 2; }}\n\
                     }}\n\
                 }}\n\
                 return total;\n\
             }}\n\
             fn run(bound: Net(\"{port}\")) -> [] int {{\n\
                 var status = 1;\n\
                 borrow bound as &n in {{\n\
                     match tcp_listen(n, {port}, 8, 0) {{\n\
                         Listening::Ok(l) => {{\n\
                             var listener = l;\n\
                             borrow mut listener as &!lh in {{\n\
                                 match tcp_accept(lh) {{\n\
                                     Accepted::Ok(c) => {{\n\
                                         var conn = c;\n\
                                         region a {{\n\
                                             var buf = alloc_slice[a](64, byte_of(0));\n\
                                             borrow mut conn as &!ch in {{\n\
                                                 let total = echo(ch, buf);\n\
                                                 if total >= 0 {{ status = 0; }}\n\
                                             }}\n\
                                         }}\n\
                                         conn_close(conn);\n\
                                     }}\n\
                                     Accepted::Again => {{ status = 2; }}\n\
                                     Accepted::Failed(e) => {{ status = 3; }}\n\
                                 }}\n\
                             }}\n\
                             listener_close(listener);\n\
                         }}\n\
                         Listening::Failed(e) => {{ status = 4; }}\n\
                     }}\n\
                 }}\n\
                 release(bound);\n\
                 return status;\n\
             }}\n"
        ),
    )
}

/// Accept, read, write, end-of-stream and close, with no `Ffi` anywhere,
/// answering a real client -- on both backends.
#[test]
fn a_listener_echoes_a_real_client_on_both_backends() {
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("sockets-echo-{backend}"));
        let exe = build(&dir, "echo", &echo_program(port), backend);

        let mut child = Command::new(&exe).spawn().expect("the server runs");
        let mut stream = connect(port);
        stream.write_all(b"ping").unwrap();
        let mut echoed = [0u8; 4];
        stream.read_exact(&mut echoed).unwrap();
        assert_eq!(&echoed, b"ping", "{backend}: the connection should echo what was sent");
        stream.write_all(b" and pong").unwrap();
        let mut more = [0u8; 9];
        stream.read_exact(&mut more).unwrap();
        assert_eq!(&more, b" and pong", "{backend}");
        // Half-close: the server must see `End`, not an error.
        stream.shutdown(Shutdown::Write).unwrap();
        let mut rest = Vec::new();
        stream.read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty(), "{backend}: nothing more to echo");

        let status = child.wait().expect("the server exits");
        assert_eq!(status.code(), Some(0), "{backend}: the server should see End and finish");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A program with its own `run(bound, io)`, the port substituted for
/// `PORT`. The `io` is there so it can announce itself on standard error,
/// which is how a test knows the server has reached a given point (a piped
/// standard output is fully buffered and would say nothing until exit).
fn io_program(port: u16, source: &str) -> String {
    let main = r#"
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi); release(fs); release(heap); release(args); release(clock);
    let bound = narrow(net, "PORT");
    return run(bound, io);
}
"#;
    format!("edition 5;\nimport std.io;\n{source}\n{main}").replace("PORT", &port.to_string())
}

/// Wait for a line on the child's standard error.
fn wait_for(lines: &mut impl std::io::BufRead, wanted: &str) {
    let mut line = String::new();
    loop {
        line.clear();
        let n = lines.read_line(&mut line).expect("the server's standard error");
        assert!(n > 0, "the server ended before saying `{wanted}`");
        if line.trim() == wanted {
            return;
        }
    }
}

const NONBLOCKING: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    var nothing_waiting = false;
    var nothing_to_read = false;
    var echoed = false;
    var spins = 0;
    var got = false;
    borrow mut io as &!i in {
        borrow bound as &n in {
            match tcp_listen(n, PORT, 8, 0) {
                Listening::Ok(l) => {
                    var listener = l;
                    borrow mut listener as &!lh in {
                        listener_nonblocking(lh);
                        // Nobody has connected: a non-blocking accept answers
                        // `Again`, not a wait and not a failure.
                        match tcp_accept(lh) {
                            Accepted::Again => { nothing_waiting = true; }
                            Accepted::Ok(c) => { conn_close(c); }
                            Accepted::Failed(e) => { }
                        }
                        io.error_all(i, "ready\n");
                        while !got && spins < 100000000 {
                            spins = spins + 1;
                            match tcp_accept(lh) {
                                Accepted::Ok(c) => {
                                    got = true;
                                    var conn = c;
                                    borrow mut conn as &!ch in {
                                        conn_nonblocking(ch);
                                        region a {
                                            var buf = alloc_slice[a](64, byte_of(0));
                                            // Nothing sent yet: `Again`.
                                            match conn_read(ch, buf) {
                                                Received::Again => { nothing_to_read = true; }
                                                Received::Data(k) => { }
                                                Received::End => { }
                                                Received::Failed(e) => { }
                                            }
                                            io.error_all(i, "idle\n");
                                            var done = false;
                                            var tries = 0;
                                            while !done && tries < 100000000 {
                                                tries = tries + 1;
                                                match conn_read(ch, buf) {
                                                    Received::Data(k) => {
                                                        done = true;
                                                        match conn_write(ch, buf[0..k]) {
                                                            Sent::Wrote(w) => { echoed = w == k; }
                                                            Sent::Again => { }
                                                            Sent::Failed(e) => { }
                                                        }
                                                    }
                                                    Received::Again => { }
                                                    Received::End => { done = true; }
                                                    Received::Failed(e) => { done = true; }
                                                }
                                            }
                                        }
                                    }
                                    conn_close(conn);
                                }
                                Accepted::Again => { }
                                Accepted::Failed(e) => { got = true; }
                            }
                        }
                    }
                    listener_close(listener);
                }
                Listening::Failed(e) => { }
            }
        }
    }
    release(bound);
    release(io);
    if nothing_waiting && nothing_to_read && echoed {
        return 0;
    }
    return 1;
}
"#;

/// `Again` is a constructor, not a wait and not a failure: a non-blocking
/// accept with nobody there, and a non-blocking read with nothing sent,
/// each answer it, and the connection still works afterwards.
#[test]
fn non_blocking_accept_and_read_answer_again() {
    use std::io::Write as _;
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("sockets-nonblocking-{backend}"));
        let exe = build(&dir, "nb", &io_program(port, NONBLOCKING), backend);

        let mut child = Command::new(&exe).stderr(Stdio::piped()).spawn().expect("the server runs");
        let mut lines = std::io::BufReader::new(child.stderr.take().unwrap());
        wait_for(&mut lines, "ready");
        let mut stream = connect(port);
        wait_for(&mut lines, "idle");
        stream.write_all(b"hi").unwrap();
        let mut echoed = [0u8; 2];
        stream.read_exact(&mut echoed).unwrap();
        assert_eq!(&echoed, b"hi", "{backend}");
        drop(stream);

        let status = child.wait().expect("the server exits");
        assert_eq!(status.code(), Some(0), "{backend}: both `Again`s and the echo should happen");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const FILL: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    var written = 0;
    var filled = false;
    var broke = false;
    borrow mut io as &!i in {
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
                                    conn_nonblocking(ch);
                                    region a {
                                        let chunk = alloc_slice[a](4096, byte_of(120));
                                        var tries = 0;
                                        while !filled && !broke && tries < 100000000 {
                                            tries = tries + 1;
                                            match conn_write(ch, chunk) {
                                                Sent::Wrote(w) => { written = written + w; }
                                                Sent::Again => { filled = true; }
                                                Sent::Failed(e) => { broke = true; }
                                            }
                                        }
                                    }
                                }
                                // Tell the test the buffer is full, then end:
                                // closing lets it read what was queued.
                                io.error_all(i, "full\n");
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
        io.print_nat(i, written);
        io.newline(i);
    }
    release(bound);
    release(io);
    if filled && !broke {
        return 0;
    }
    return 1;
}
"#;

/// A write the kernel has no room for answers `Again` -- it does not wait
/// and it does not fail -- and what it did take arrives intact. This is
/// the contract `examples/api` needed `MSG_DONTWAIT`, a send timeout and a
/// per-OS flag to get; here it is one builtin.
#[test]
fn a_write_the_kernel_has_no_room_for_answers_again() {
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("sockets-fill-{backend}"));
        let exe = build(&dir, "fill", &io_program(port, FILL), backend);

        let mut child = Command::new(&exe)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server runs");
        let mut lines = std::io::BufReader::new(child.stderr.take().unwrap());
        wait_for(&mut lines, "ready");
        // Connect and do not read: the server's sends pile up until the
        // kernel says no.
        let mut stream = connect(port);
        wait_for(&mut lines, "full");
        let mut received = 0usize;
        let mut buf = vec![0u8; 65536];
        loop {
            match stream.read(&mut buf).unwrap() {
                0 => break,
                n => {
                    assert!(buf[..n].iter().all(|b| *b == b'x'), "{backend}: bytes arrive intact");
                    received += n;
                }
            }
        }
        let out = child.wait_with_output().expect("the server exits");
        assert_eq!(out.status.code(), Some(0), "{backend}: `Again`, not `Failed`");
        let wrote: usize = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
        assert!(wrote > 0, "{backend}: something was queued before the buffer filled");
        assert_eq!(received, wrote, "{backend}: everything queued is delivered, nothing else");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const GONE: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    var failed = false;
    borrow mut io as &!i in {
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
                                        let byte = alloc_slice[a](1, byte_of(33));
                                        var tries = 0;
                                        // The peer has gone: sooner or later a send
                                        // fails. On Linux that is `SIGPIPE` and the
                                        // process dies, unless the send says not to.
                                        while !failed && tries < 10000000 {
                                            tries = tries + 1;
                                            match conn_write(ch, byte) {
                                                Sent::Wrote(w) => { }
                                                Sent::Again => { }
                                                Sent::Failed(e) => { failed = true; }
                                            }
                                        }
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
    release(io);
    if failed {
        return 0;
    }
    return 1;
}
"#;

/// Writing to a peer that has gone answers `Failed`; it does not kill the
/// process. `SIGPIPE` used to be the program's problem -- the api server
/// installed `signal(SIGPIPE, SIG_IGN)` through libc -- and is now the
/// builtin's, per socket, on both kernels.
#[test]
fn a_write_to_a_peer_that_has_gone_fails_instead_of_killing_the_process() {
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("sockets-gone-{backend}"));
        let exe = build(&dir, "gone", &io_program(port, GONE), backend);

        let mut child = Command::new(&exe).stderr(Stdio::piped()).spawn().expect("the server runs");
        let mut lines = std::io::BufReader::new(child.stderr.take().unwrap());
        wait_for(&mut lines, "ready");
        drop(connect(port));

        let status = child.wait().expect("the server exits");
        assert_eq!(
            status.code(),
            Some(0),
            "{backend}: the process must exit, not be killed by a signal ({status:?})"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const PORT_TAKEN: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        // A second listener on a port that is already held fails, and says
        // so with a constructor rather than a sentinel.
        match tcp_listen(n, PORT, 8, 0) {
            Listening::Ok(l) => { listener_close(l); }
            Listening::Failed(e) => { if e > 0 { status = 0; } }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

/// `tcp_listen` on a port something else holds answers `Failed(errno)`
/// with a real, positive `errno`, and leaks no descriptor.
#[test]
fn listening_on_a_port_that_is_taken_answers_failed() {
    for backend in BACKENDS {
        // `SO_REUSEADDR` lets a bind succeed over a closed socket's
        // TIME_WAIT, not over a live listener -- so hold one. On the
        // *wildcard* address, because that is what `tcp_listen` binds: BSD
        // lets a wildcard bind sit beside a listener on one specific
        // address (`127.0.0.1`) when `SO_REUSEADDR` is set, and Linux does
        // not, which is how this passed on Linux and failed on macOS.
        let holder = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        let port = holder.local_addr().unwrap().port();
        let dir = scratch(&format!("sockets-taken-{backend}"));
        let exe = build(&dir, "taken", &io_program(port, PORT_TAKEN), backend);
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(
            run.status.code(),
            Some(0),
            "{backend}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let _ = std::fs::remove_dir_all(&dir);
        drop(holder);
    }
}

const WRONG_PORT: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        // The capability names one port, and this asks for another.
        match tcp_listen(n, 1, 8, 0) {
            Listening::Ok(l) => { listener_close(l); }
            Listening::Failed(e) => { }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

/// `tcp_listen`'s port is checked against the capability's bound, as
/// `bind`'s is: a program granted one port cannot listen on another.
#[test]
fn listening_on_a_port_outside_the_bound_traps() {
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("sockets-wrong-port-{backend}"));
        let exe = build(&dir, "wrong", &io_program(port, WRONG_PORT), backend);
        let run = Command::new(&exe).output().expect("the program runs");
        assert!(!run.status.success(), "{backend}: a port outside the bound must not succeed");
        assert_eq!(run.status.code(), None, "{backend}: killed by the trap, not a normal exit");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const EMPTY_BUFFER: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    var status = 1;
    borrow mut io as &!i in {
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
                                        let none = alloc_slice[a](0, byte_of(0));
                                        var empty = none;
                                        // A zero-length buffer is a refusal, not an
                                        // end of stream: a zero from the kernel
                                        // would read as the peer closing.
                                        match conn_read(ch, empty) {
                                            Received::Failed(e) => { if e == 22 { status = 0; } }
                                            Received::End => { status = 2; }
                                            Received::Data(k) => { status = 3; }
                                            Received::Again => { status = 4; }
                                        }
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
    release(io);
    return status;
}
"#;

/// A read into an empty buffer is `Failed(EINVAL)`, never `End`.
#[test]
fn reading_into_an_empty_buffer_is_refused_not_an_end_of_stream() {
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("sockets-empty-{backend}"));
        let exe = build(&dir, "empty", &io_program(port, EMPTY_BUFFER), backend);
        let mut child = Command::new(&exe).stderr(Stdio::piped()).spawn().expect("the server runs");
        let mut lines = std::io::BufReader::new(child.stderr.take().unwrap());
        wait_for(&mut lines, "ready");
        let _stream = connect(port);
        let status = child.wait().expect("the server exits");
        assert_eq!(status.code(), Some(0), "{backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const REUSE_PORT: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    var shared = false;
    var refused_without = false;
    borrow bound as &n in {
        match tcp_listen(n, PORT, 8, 1) {
            Listening::Ok(first) => {
                // Both ask to share the port: both get it.
                match tcp_listen(n, PORT, 8, 1) {
                    Listening::Ok(second) => { shared = true; listener_close(second); }
                    Listening::Failed(e) => { }
                }
                // One that does not ask is refused.
                match tcp_listen(n, PORT, 8, 0) {
                    Listening::Ok(third) => { listener_close(third); }
                    Listening::Failed(e) => { refused_without = true; }
                }
                listener_close(first);
            }
            Listening::Failed(e) => { }
        }
    }
    release(bound);
    release(io);
    if shared && refused_without {
        return 0;
    }
    return 1;
}
"#;

/// Bit 1 of `tcp_listen`'s flags is `SO_REUSEPORT`: two listeners that ask
/// for it share a port (one process per core), and one that does not is
/// refused -- so the flag is the only way in, and it is honoured.
#[test]
fn the_reuse_port_flag_lets_two_listeners_share_a_port() {
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("sockets-reuseport-{backend}"));
        let exe = build(&dir, "reuse", &io_program(port, REUSE_PORT), backend);
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), Some(0), "{backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The point of the whole stage, as a test: a server over socket handles
/// reports **the port it listens on and the two path-free labels of what
/// it does with a connection** -- and no `ffi`. The same server written
/// against `Ffi("libc")` reported `ffi("libc")`, which means "may call
/// anything in libc" and says nothing (`native-sockets.md` §1).
#[test]
fn a_socket_program_reports_its_port_and_no_ffi() {
    let json = authority_json(&echo_program(8080), "sockets-authority");
    assert!(
        json.contains("{ \"name\": \"net_in\", \"argument\": \"8080\", \"bounded\": true }"),
        "the port survives the move to handles (§3):\n{json}"
    );
    for label in ["conn_accept", "conn_read", "conn_write"] {
        assert!(
            json.contains(&format!(
                "{{ \"name\": \"{label}\", \"argument\": null, \"bounded\": true }}"
            )),
            "`{label}` is path-free and bounded (§3):\n{json}"
        );
    }
    assert!(!json.contains("\"ffi\""), "no foreign code anywhere:\n{json}");
}

/// Like [`io_program`], but the capability is narrowed to `bound` (a
/// `host:port`) rather than to the port: `BOUND` and `PORT` are
/// substituted, and `run` takes a `Net("BOUND")`.
fn dial_program(port: u16, bound: &str, source: &str) -> String {
    let main = r#"
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi); release(fs); release(heap); release(args); release(clock);
    let bound = narrow(net, "BOUND");
    return run(bound, io);
}
"#;
    format!("edition 5;\nimport std.io;\n{source}\n{main}")
        .replace("BOUND", bound)
        .replace("PORT", &port.to_string())
}

const DIAL: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match tcp_connect(n, "127.0.0.1", PORT) {
            Dialed::Ok(c) => {
                var conn = c;
                borrow mut conn as &!ch in {
                    match conn_write(ch, "ping") {
                        Sent::Wrote(w) => {
                            region a {
                                var buf = alloc_slice[a](16, byte_of(0));
                                match conn_read(ch, buf) {
                                    Received::Data(k) => {
                                        // The peer upper-cases what it was sent.
                                        if k == 4 && int_of(buf[0]) == 80 { status = 0; }
                                    }
                                    Received::End => { status = 5; }
                                    Received::Again => { status = 6; }
                                    Received::Failed(e) => { status = 7; }
                                }
                            }
                        }
                        Sent::Again => { status = 3; }
                        Sent::Failed(e) => { status = 4; }
                    }
                }
                conn_close(conn);
            }
            Dialed::Failed(e) => { status = 2; }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

/// `tcp_connect` dials a real server, writes, reads the answer and closes
/// -- the outbound half of the stage, with no `Ffi`, on both backends.
#[test]
fn a_dialled_connection_exchanges_bytes_with_a_real_server() {
    for backend in BACKENDS {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let dir = scratch(&format!("sockets-dial-{backend}"));
        let exe =
            build(&dir, "dial", &dial_program(port, &format!("127.0.0.1:{port}"), DIAL), backend);

        let peer = std::thread::spawn(move || {
            use std::io::Write as _;
            let (mut stream, _) = server.accept().unwrap();
            let mut request = [0u8; 4];
            stream.read_exact(&mut request).unwrap();
            stream.write_all(&request.to_ascii_uppercase()).unwrap();
            request
        });
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(
            run.status.code(),
            Some(0),
            "{backend}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            &peer.join().unwrap(),
            b"ping",
            "{backend}: the server received what was written"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const REFUSED: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match tcp_connect(n, "127.0.0.1", PORT) {
            Dialed::Ok(c) => { conn_close(c); status = 2; }
            // The kernel's own reason: a positive `errno`.
            Dialed::Failed(e) => { if e > 0 { status = 0; } }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

/// Nothing listening: `Failed` with a positive `errno`, and no descriptor
/// leaked (the program would fail to close it -- it has none to close).
#[test]
fn dialling_a_closed_port_answers_failed_with_an_errno() {
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("sockets-refused-{backend}"));
        let exe = build(
            &dir,
            "refused",
            &dial_program(port, &format!("127.0.0.1:{port}"), REFUSED),
            backend,
        );
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), Some(0), "{backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const UNRESOLVED: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match tcp_connect(n, "no-such-host.invalid", 80) {
            Dialed::Ok(c) => { conn_close(c); status = 2; }
            // No `errno` is negative: -1 says the *name* did not resolve.
            Dialed::Failed(e) => { if e == 0 - 1 { status = 0; } }
        }
    }
    release(bound);
    release(io);
    return status;
}
"#;

/// A name that does not resolve is `Failed(-1)`, which no kernel `errno`
/// ever is -- so a program can tell the resolver's refusal from the
/// network's.
#[test]
fn an_unresolvable_name_is_failed_minus_one() {
    for backend in BACKENDS {
        let dir = scratch(&format!("sockets-unresolved-{backend}"));
        let exe = build(
            &dir,
            "unresolved",
            &dial_program(0, "no-such-host.invalid:80", UNRESOLVED),
            backend,
        );
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), Some(0), "{backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const OUTSIDE_HOST: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    borrow bound as &n in {
        match tcp_connect(n, "localhost", PORT) {
            Dialed::Ok(c) => { conn_close(c); }
            Dialed::Failed(e) => { }
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
        match tcp_connect(n, "127.0.0.1", 1) {
            Dialed::Ok(c) => { conn_close(c); }
            Dialed::Failed(e) => { }
        }
    }
    release(bound);
    release(io);
    return 1;
}
"#;

/// The bound is `host:port` and both halves are enforced, as `connect`'s
/// are: a program granted `127.0.0.1:P` can reach neither another host nor
/// another port.
#[test]
fn dialling_outside_the_bound_traps() {
    for backend in BACKENDS {
        for (name, source) in [("host", OUTSIDE_HOST), ("port", OUTSIDE_PORT)] {
            let port = free_port();
            let dir = scratch(&format!("sockets-outside-{name}-{backend}"));
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

/// The report of a client names the host and port it may dial -- and no
/// `ffi` -- the outbound half of `a_socket_program_reports_its_port_and_no_ffi`.
#[test]
fn a_client_reports_its_host_and_no_ffi() {
    let json = authority_json(&dial_program(9, "127.0.0.1:9", DIAL), "sockets-client-authority");
    assert!(
        json.contains(
            "{ \"name\": \"net_out\", \"argument\": \"127.0.0.1:9\", \"bounded\": true }"
        ),
        "the bound survives the move to handles:\n{json}"
    );
    assert!(json.contains("\"conn_write\"") && json.contains("\"conn_read\""), "{json}");
    assert!(!json.contains("\"ffi\""), "no foreign code anywhere:\n{json}");
}

const POLLER: &str = r#"
fn run(bound: Net("PORT"), io: Io) -> [] int {
    var score = 0;
    borrow mut io as &!i in {
        borrow bound as &n in {
            match tcp_listen(n, PORT, 8, 0) {
                Listening::Ok(l) => {
                    var listener = l;
                    borrow mut listener as &!lh in {
                        listener_nonblocking(lh);
                        match poller_new() {
                            Polling::Ok(p) => {
                                var poller = p;
                                borrow mut poller as &!ph in {
                                    poller_add_listener(ph, lh, 100);
                                    region a {
                                        var ev = alloc_slice[a](16, 0);
                                        var buf = alloc_slice[a](16, byte_of(0));
                                        // 1. Nothing is ready: a timed wait answers 0.
                                        if poller_wait(ph, ev, 50) == 0 { score = score + 1; }
                                        io.error_all(i, "ready\n");
                                        // 2. A client arrives: the listener, token 100, readable.
                                        let ready = poller_wait(ph, ev, 5000);
                                        if ready == 1 && ev[0] == 100 && ev[1] == 1 { score = score + 1; }
                                        match tcp_accept(lh) {
                                            Accepted::Ok(c) => {
                                                var conn = c;
                                                borrow mut conn as &!ch in {
                                                    conn_nonblocking(ch);
                                                    poller_add_conn(ph, ch, 7, 1);
                                                    // 3. The client sent "hello": token 7, readable.
                                                    let got = poller_wait(ph, ev, 5000);
                                                    if got == 1 && ev[0] == 7 && ev[1] == 1 { score = score + 1; }
                                                    match conn_read(ch, buf) {
                                                        Received::Data(k) => { }
                                                        Received::End => { }
                                                        Received::Again => { }
                                                        Received::Failed(e) => { }
                                                    }
                                                    // 4. Removed from the set, a connection is silent
                                                    //    however much it is sent.
                                                    poller_remove(ph, ch);
                                                    io.error_all(i, "removed\n");
                                                    if poller_wait(ph, ev, 200) == 0 { score = score + 1; }
                                                    // 5. Added again, what was sent while it was
                                                    //    away is still there to be read.
                                                    poller_add_conn(ph, ch, 7, 1);
                                                    let back = poller_wait(ph, ev, 5000);
                                                    var more = 0;
                                                    match conn_read(ch, buf) {
                                                        Received::Data(k) => { more = k; }
                                                        Received::End => { }
                                                        Received::Again => { }
                                                        Received::Failed(e) => { }
                                                    }
                                                    if back == 1 && ev[0] == 7 && more == 4 { score = score + 1; }
                                                    // 6. Asking for writability alone: ready at
                                                    //    once, and only writable.
                                                    poller_modify(ph, ch, 7, 2);
                                                    let room = poller_wait(ph, ev, 5000);
                                                    if room == 1 && ev[0] == 7 && ev[1] == 2 { score = score + 1; }
                                                    io.error_all(i, "writable\n");
                                                    // 7. The client has gone: readable, and the
                                                    //    read says End.
                                                    poller_modify(ph, ch, 7, 1);
                                                    let gone = poller_wait(ph, ev, 5000);
                                                    var ended = false;
                                                    match conn_read(ch, buf) {
                                                        Received::End => { ended = true; }
                                                        Received::Data(k) => { }
                                                        Received::Again => { }
                                                        Received::Failed(e) => { }
                                                    }
                                                    if gone == 1 && ended { score = score + 1; }
                                                }
                                                conn_close(conn);
                                            }
                                            Accepted::Again => { }
                                            Accepted::Failed(e) => { }
                                        }
                                    }
                                }
                                poller_close(poller);
                            }
                            Polling::Failed(e) => { }
                        }
                    }
                    listener_close(listener);
                }
                Listening::Failed(e) => { }
            }
        }
    }
    release(bound);
    release(io);
    if score == 7 {
        return 0;
    }
    return 100 + score;
}
"#;

/// One `Poller` over a listener and a connection, on both backends: a timed
/// wait that answers 0, readiness by token, removal that really silences,
/// a re-added handle's pending data, writability alone, and a hang-up read
/// as `End` -- epoll on Linux and kqueue on macOS behind one surface.
#[test]
fn a_poller_reports_readiness_by_token() {
    use std::io::Write as _;
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("sockets-poller-{backend}"));
        let exe = build(&dir, "poller", &io_program(port, POLLER), backend);

        let mut child = Command::new(&exe).stderr(Stdio::piped()).spawn().expect("the server runs");
        let mut lines = std::io::BufReader::new(child.stderr.take().unwrap());
        wait_for(&mut lines, "ready");
        let mut stream = connect(port);
        stream.write_all(b"hello").unwrap();
        wait_for(&mut lines, "removed");
        stream.write_all(b"more").unwrap();
        wait_for(&mut lines, "writable");
        drop(stream);

        let status = child.wait().expect("the server exits");
        // 0 is all seven checks; otherwise 100 plus how many passed.
        assert_eq!(status.code(), Some(0), "{backend}: score {:?}", status.code());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const CLOCK: &str = r#"
edition 5;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net);
    var status = 1;
    match poller_new() {
        Polling::Ok(p) => {
            var poller = p;
            borrow mut poller as &!ph in {
                borrow clock as &c in {
                    region a {
                        var ev = alloc_slice[a](4, 0);
                        let before = clock_ms(c);
                        // An empty set: the wait is a sleep, and the clock says how long.
                        poller_wait(ph, ev, 1100);
                        let after = clock_ms(c);
                        let slept = after - before;
                        // Milliseconds, monotonic: a wait longer than a second always
                        // crosses a second boundary, which is where seconds and
                        // milliseconds mixed up would show as a negative span.
                        if slept >= 1000 && slept < 3000 && before > 0 { status = 0; }
                    }
                }
            }
            poller_close(poller);
        }
        Polling::Failed(e) => { }
    }
    release(clock);
    return status;
}
"#;

/// `clock_ms` reads a monotonic clock in **milliseconds**: a 1.1 s wait on
/// an empty `Poller` is seen as 1000-3000 -- so the unit is right, the clock
/// moves, and the seconds and nanoseconds are combined across a second
/// boundary -- on both backends.
#[test]
fn the_clock_measures_a_wait_in_milliseconds() {
    for backend in BACKENDS {
        let dir = scratch(&format!("sockets-clock-{backend}"));
        let exe = build(&dir, "clock", CLOCK, backend);
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), Some(0), "{backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Reading the time is an effect, reported: `clock` is a path-free label
/// and a program that reads it says so.
#[test]
fn a_program_that_reads_the_clock_reports_it() {
    let json = authority_json(CLOCK, "sockets-clock-authority");
    assert!(
        json.contains("{ \"name\": \"clock\", \"argument\": null, \"bounded\": true }"),
        "{json}"
    );
    assert!(json.contains("\"poll\""), "the poller is reported too:\n{json}");
}
