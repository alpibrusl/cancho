//! `docs/native-sockets.md` section 12: `tcp_listen_on`, a listener bound to
//! one IPv4 address, over real sockets on both backends, and the checker's
//! side of it -- the authority row, the narrowing, and every refusal.

use super::sockets::{BACKENDS, build, wait_for};
use super::*;
use std::io::Read as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

/// A listener that says `ready`, accepts one connection, writes `ping` and
/// ends. `BOUND` is what the `Net` is narrowed to, `LISTEN` the call under
/// test.
const SERVER: &str = r#"
edition 9;
import std.io;

fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var status = 1;
    borrow mut io as &!i in {
        borrow bound as &n in {
            match LISTEN {
                Listening::Ok(l) => {
                    var listener = l;
                    borrow mut listener as &!lh in {
                        io.error_all(i, "ready\n");
                        match tcp_accept(lh) {
                            Accepted::Ok(c) => {
                                var conn = c;
                                borrow mut conn as &!ch in {
                                    match conn_write(ch, "ping") {
                                        Sent::Wrote(w) => { status = 0; }
                                        Sent::Again => { status = 3; }
                                        Sent::Failed(e) => { status = 4; }
                                    }
                                }
                                conn_close(conn);
                            }
                            Accepted::Again => { status = 2; }
                            Accepted::Failed(e) => { status = 5; }
                        }
                    }
                    listener_close(listener);
                }
                Listening::Failed(e) => { status = 6; }
            }
        }
    }
    release(bound);
    release(io);
    return status;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec, tty } = split(world);
    release(ffi); release(fs); release(heap); release(args); release(clock);
    release(signals); release(exec); release(tty);
    let bound = narrow(net, "BOUND");
    return run(bound, io);
}
"#;

fn server(bound: &str, listen: &str, port: u16) -> String {
    SERVER.replace("BOUND", bound).replace("LISTEN", listen).replace("PORT", &port.to_string())
}

/// This machine's non-loopback IPv4 address, if it has one: the source
/// address the OS would use towards a documentation address (RFC 5737). A UDP
/// `connect` only asks the routing table, so nothing is sent.
fn non_loopback_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

fn dial(address: Ipv4Addr, port: u16) -> std::io::Result<TcpStream> {
    TcpStream::connect_timeout(&SocketAddr::from((address, port)), Duration::from_secs(5))
}

fn read_ping(mut stream: TcpStream) -> String {
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut text = String::new();
    stream.read_to_string(&mut text).expect("the server writes and closes");
    text
}

/// Start the program, wait until it is listening, and hand back the child.
fn start(exe: &Path) -> std::process::Child {
    let mut child = Command::new(exe).stderr(Stdio::piped()).spawn().expect("the server runs");
    let mut lines = std::io::BufReader::new(child.stderr.take().unwrap());
    wait_for(&mut lines, "ready");
    // Keep draining, so a server that writes more is not blocked on a full pipe.
    std::thread::spawn(move || {
        let mut sink = String::new();
        while lines.read_line(&mut sink).map(|n| n > 0).unwrap_or(false) {
            sink.clear();
        }
    });
    child
}

use std::io::BufRead as _;

const LOOPBACK: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);

/// How a listener behaves towards each address, as data: which of the two
/// addresses reach it.
struct Reach {
    loopback: bool,
    other: Option<bool>,
}

/// Build `source`, start it, and ask both addresses. A listener takes one
/// connection, so the last accepted dial ends it; a refused dial costs it
/// nothing. `None` for `other` means the machine has no second address.
fn reach(source: &str, port: u16, backend: &str, tag: &str) -> Reach {
    let dir = scratch(&format!("listen-host-{tag}-{backend}"));
    let exe = build(&dir, "server", source, backend);
    let other_address = non_loopback_address();
    let mut child = start(&exe);

    // The non-loopback dial goes first: if it is accepted it is the one
    // connection the server takes, and then the loopback dial is asked of a
    // fresh run below.
    let mut other = None;
    if let Some(address) = other_address {
        match dial(address, port) {
            Ok(stream) => {
                assert_eq!(read_ping(stream), "ping", "{tag} {backend}");
                other = Some(true);
                assert_eq!(child.wait().unwrap().code(), Some(0));
                child = start(&exe);
            }
            Err(error) => {
                assert_eq!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionRefused,
                    "{tag} {backend}: the address must refuse, not hang or fail some other way"
                );
                other = Some(false);
            }
        }
    }
    let loopback = match dial(LOOPBACK, port) {
        Ok(stream) => {
            assert_eq!(read_ping(stream), "ping", "{tag} {backend}");
            true
        }
        Err(error) => {
            assert_eq!(error.kind(), std::io::ErrorKind::ConnectionRefused, "{tag} {backend}");
            let _ = child.kill();
            false
        }
    };
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
    Reach { loopback, other }
}

fn say_skipped(reach: &Reach, test: &str) {
    if reach.other.is_none() {
        eprintln!(
            "{test}: SKIPPED the non-loopback half -- this machine has no non-loopback IPv4 address"
        );
    }
}

/// The acceptance gate of #498: a listener on `127.0.0.1` is reachable at
/// `127.0.0.1` and **refuses a connection to the machine's other address**.
#[test]
fn a_loopback_listener_refuses_the_machines_other_address() {
    for backend in BACKENDS {
        let port = free_port();
        let source = server(
            &format!("127.0.0.1:{port}"),
            &format!("tcp_listen_on(n, \"127.0.0.1\", {port}, 8, 0)"),
            port,
        );
        let got = reach(&source, port, backend, "loopback");
        say_skipped(&got, "a_loopback_listener_refuses_the_machines_other_address");
        assert!(got.loopback, "{backend}: loopback must reach a loopback listener");
        assert_ne!(
            got.other,
            Some(true),
            "{backend}: a loopback listener answered a non-loopback address"
        );
    }
}

/// `0.0.0.0` listens everywhere: both addresses reach it.
#[test]
fn an_all_interfaces_listener_by_address_accepts_both() {
    for backend in BACKENDS {
        let port = free_port();
        let source = server(
            &format!("{port}"),
            &format!("tcp_listen_on(n, \"0.0.0.0\", {port}, 8, 0)"),
            port,
        );
        let got = reach(&source, port, backend, "everywhere");
        say_skipped(&got, "an_all_interfaces_listener_by_address_accepts_both");
        assert!(got.loopback, "{backend}");
        assert_ne!(
            got.other,
            Some(false),
            "{backend}: `0.0.0.0` must be reachable on the other address"
        );
    }
}

/// The old form is untouched: it still listens on every interface.
#[test]
fn the_old_form_still_listens_on_every_interface() {
    for backend in BACKENDS {
        let port = free_port();
        let source = server(&format!("{port}"), &format!("tcp_listen(n, {port}, 8, 0)"), port)
            .replace("edition 9;", "edition 5;")
            .replace(
                "let Split { io, ffi, fs, heap, args, net, clock, signals, exec, tty } = split(world);",
                "let Split { io, ffi, fs, heap, args, net, clock } = split(world);",
            )
            .replace("release(signals); release(exec); release(tty);", "");
        let got = reach(&source, port, backend, "old");
        say_skipped(&got, "the_old_form_still_listens_on_every_interface");
        assert!(got.loopback, "{backend}");
        assert_ne!(
            got.other,
            Some(false),
            "{backend}: `tcp_listen` must stay reachable on the other address"
        );
    }
}

/// A bare-port bound grants every interface, so loopback inside it is allowed
/// -- and is loopback only: the address is what the call says, not the bound.
#[test]
fn a_port_only_bound_allows_loopback_and_binds_only_loopback() {
    for backend in BACKENDS {
        let port = free_port();
        let source = server(
            &format!("{port}"),
            &format!("tcp_listen_on(n, \"127.0.0.1\", {port}, 8, 0)"),
            port,
        );
        let got = reach(&source, port, backend, "portonly");
        assert!(got.loopback, "{backend}");
        assert_ne!(got.other, Some(true), "{backend}");
    }
}

/// The port is still checked against the bound at run time.
#[test]
fn a_port_outside_the_bound_traps() {
    for backend in BACKENDS {
        let port = free_port();
        let source = server(
            &format!("127.0.0.1:{port}"),
            &format!("tcp_listen_on(n, \"127.0.0.1\", {}, 8, 0)", u32::from(port) + 1),
            port,
        );
        let dir = scratch(&format!("listen-host-trap-{backend}"));
        let exe = build(&dir, "server", &source, backend);
        // Without the check the program would listen and wait for a client
        // forever, so it is given a deadline rather than waited for.
        let mut child = Command::new(&exe).stderr(Stdio::null()).spawn().expect("the program runs");
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "{backend}: a port outside the bound did not trap; the program is listening"
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(status.code(), None, "{backend}: killed by the trap, not a normal exit");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `SO_REUSEPORT` (flag bit 1) works with an address: two listeners on one
/// loopback port, and a third that does not ask is refused.
#[test]
fn the_reuse_port_flag_works_with_an_address() {
    const SHARE: &str = r#"
edition 9;

fn run(bound: Net("127.0.0.1:PORT")) -> [] int {
    var shared = false;
    var refused_without = false;
    borrow bound as &n in {
        match tcp_listen_on(n, "127.0.0.1", PORT, 8, 1) {
            Listening::Ok(first) => {
                match tcp_listen_on(n, "127.0.0.1", PORT, 8, 1) {
                    Listening::Ok(second) => { shared = true; listener_close(second); }
                    Listening::Failed(e) => { }
                }
                match tcp_listen_on(n, "127.0.0.1", PORT, 8, 0) {
                    Listening::Ok(third) => { listener_close(third); }
                    Listening::Failed(e) => { refused_without = true; }
                }
                listener_close(first);
            }
            Listening::Failed(e) => { }
        }
    }
    release(bound);
    if shared && refused_without { return 0; }
    return 1;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec, tty } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(clock);
    release(signals); release(exec); release(tty);
    let bound = narrow(net, "127.0.0.1:PORT");
    return run(bound);
}
"#;
    for backend in BACKENDS {
        let port = free_port();
        let dir = scratch(&format!("listen-host-reuse-{backend}"));
        let exe = build(&dir, "reuse", &SHARE.replace("PORT", &port.to_string()), backend);
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), Some(0), "{backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `cancho check --output json`: the rule a source was refused under and the
/// message, or `None` if it was accepted.
fn refusal(source: &str, tag: &str) -> Option<(String, String)> {
    let dir = scratch(tag);
    let path = dir.join("program.cho");
    std::fs::write(&path, source).expect("a writable fixture");
    let out = Command::new(BIN)
        .args(["check".as_ref(), path.as_os_str(), "--std".as_ref(), "--output".as_ref()])
        .arg("json")
        .output()
        .expect("the compiler runs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.code().is_some_and(|c| c < 128), "the compiler crashed on {source}");
    if out.status.success() {
        return None;
    }
    let json = String::from_utf8_lossy(&out.stdout).into_owned();
    let field = |key: &str| {
        json.lines()
            .find_map(|line| line.trim().strip_prefix(&format!("\"{key}\": \"")))
            .map(|rest| rest.trim_end_matches("\",").to_owned())
            .unwrap_or_default()
    };
    Some((field("rule"), field("message")))
}

/// A program that narrows to `bound` and makes one listen call.
fn call_under(bound: &str, call: &str) -> String {
    server(bound, call, 8080)
}

/// The narrowing table of section 12.3: which bound allows which call.
#[test]
fn the_bound_decides_which_calls_are_allowed() {
    let rows: [(&str, &str, bool); 16] = [
        // (bound, call, allowed)
        ("127.0.0.1:8080", r#"tcp_listen_on(n, "127.0.0.1", 8080, 8, 0)"#, true),
        ("127.0.0.1:8080", r#"tcp_listen_on(n, "0.0.0.0", 8080, 8, 0)"#, false),
        ("127.0.0.1:8080", r#"tcp_listen_on(n, "127.0.0.2", 8080, 8, 0)"#, false),
        ("127.0.0.1:8080", r#"tcp_listen_on(n, "127.0.0.10", 8080, 8, 0)"#, false),
        ("127.0.0.1:8080", "tcp_listen(n, 8080, 8, 0)", false),
        ("127.0.0.1:", r#"tcp_listen_on(n, "127.0.0.1", 9, 8, 0)"#, true),
        ("127.0.0.1", r#"tcp_listen_on(n, "127.0.0.1", 9, 8, 0)"#, true),
        ("127.0.0.1", "tcp_listen(n, 9, 8, 0)", false),
        ("8080", r#"tcp_listen_on(n, "127.0.0.1", 8080, 8, 0)"#, true),
        ("8080", r#"tcp_listen_on(n, "0.0.0.0", 8080, 8, 0)"#, true),
        ("8080", "tcp_listen(n, 8080, 8, 0)", true),
        ("0.0.0.0:8080", r#"tcp_listen_on(n, "127.0.0.1", 8080, 8, 0)"#, true),
        ("0.0.0.0:8080", r#"tcp_listen_on(n, "0.0.0.0", 8080, 8, 0)"#, true),
        ("0.0.0.0:8080", "tcp_listen(n, 8080, 8, 0)", false),
        ("localhost:8080", r#"tcp_listen_on(n, "127.0.0.1", 8080, 8, 0)"#, false),
        ("::1", r#"tcp_listen_on(n, "127.0.0.1", 8080, 8, 0)"#, false),
    ];
    for (i, (bound, call, allowed)) in rows.iter().enumerate() {
        let got = refusal(&call_under(bound, call), &format!("listen-host-row-{i}"));
        match (allowed, got) {
            (true, None) | (false, Some(_)) => {}
            (true, Some((rule, message))) => {
                panic!("`{call}` under `{bound}` should be allowed, but: {rule}: {message}")
            }
            (false, None) => panic!("`{call}` under `{bound}` should be refused"),
        }
        if !allowed
            && let Some((rule, message)) = refusal(&call_under(bound, call), "listen-host-tag")
        {
            assert_eq!(rule, "listen-host", "{bound} {call}: {message}");
        }
    }
}

/// Every host text that is not a strict dotted quad is refused under the rule
/// -- names, IPv6, short forms, octal-looking octets, signs, whitespace,
/// unicode, the empty string, and one very long one.
#[test]
fn a_host_that_is_not_an_ipv4_literal_is_refused_with_its_rule() {
    let long = "1".repeat(5000);
    let hosts = [
        "",
        "localhost",
        "::1",
        "[::1]",
        "127.1",
        "127.0.1",
        "127.0.0.1.1",
        "127.0.0.1.",
        "256.0.0.1",
        "010.0.0.1",
        "+1.0.0.1",
        "1.2.3.4 ",
        " 1.2.3.4",
        "0x7f.0.0.1",
        "1.2.3",
        "１.2.3.4",
        "a.b.c.d",
        "1.2.3.4/8",
        "1.2.3.4:80",
        "0.0.0",
        "example.com",
        &long,
    ];
    for host in hosts {
        let call = format!("tcp_listen_on(n, \"{host}\", 8080, 8, 0)");
        let (rule, message) = refusal(&call_under("8080", &call), "listen-host-bad")
            .unwrap_or_else(|| panic!("`{host:.40}` should be refused"));
        assert_eq!(rule, "listen-host", "{host:.40}: {message}");
    }
    for host in ["127.0.0.1", "0.0.0.0", "10.20.30.40", "255.255.255.255", "127.0.0.2"] {
        let call = format!("tcp_listen_on(n, \"{host}\", 8080, 8, 0)");
        assert!(refusal(&call_under("8080", &call), "listen-host-good").is_none(), "{host}");
    }
}

/// A host that is not a literal -- a variable, a call result -- is refused.
#[test]
fn a_host_that_is_not_a_literal_is_refused() {
    let source = call_under("8080", "tcp_listen_on(n, host, 8080, 8, 0)")
        .replace("var status = 1;", "var status = 1;\n    let host = \"127.0.0.1\";");
    let (rule, message) = refusal(&source, "listen-host-variable").expect("refused");
    assert_eq!(rule, "listen-host", "{message}");
}

/// Wrong arity and wrong argument types are the ordinary refusals.
#[test]
fn the_other_arguments_are_checked_like_tcp_listens() {
    let (rule, _) = refusal(&call_under("8080", r#"tcp_listen_on(n, 8080, 8, 0)"#), "listen-arity")
        .expect("refused");
    assert_eq!(rule, "arity-mismatch");
    let (rule, _) = refusal(
        &call_under("8080", r#"tcp_listen_on(n, "127.0.0.1", "8080", 8, 0)"#),
        "listen-type",
    )
    .expect("refused");
    assert_eq!(rule, "type-mismatch");
}

/// The handle `tcp_listen_on` makes is a `Listener` like any other: a
/// `Poller` takes it and `listener_nonblocking` accepts it.
#[test]
fn the_listener_is_the_same_handle_the_poller_takes() {
    let source = r#"
edition 9;

fn run(bound: Net("127.0.0.1:8080")) -> [] int {
    var status = 1;
    borrow bound as &n in {
        match tcp_listen_on(n, "127.0.0.1", 8080, 8, 0) {
            Listening::Ok(l) => {
                var listener = l;
                borrow mut listener as &!lh in { listener_nonblocking(lh); }
                match poller_new() {
                    Polling::Ok(p) => {
                        var poller = p;
                        borrow mut poller as &!ph in {
                            borrow listener as &lr in { poller_add_listener(ph, lr, 1); }
                        }
                        poller_close(poller);
                        status = 0;
                    }
                    Polling::Failed(e) => { }
                }
                listener_close(listener);
            }
            Listening::Failed(e) => { }
        }
    }
    release(bound);
    return status;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec, tty } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(clock);
    release(signals); release(exec); release(tty);
    let bound = narrow(net, "127.0.0.1:8080");
    return run(bound);
}
"#;
    if let Some((rule, message)) = refusal(source, "listen-poller") {
        panic!("{rule}: {message}");
    }
}

/// The authority report, section 12.3: the bound is the row, so a loopback
/// program says loopback, an everywhere program says the bare port, and the
/// old form's row is what it was.
#[test]
fn the_authority_report_names_the_host() {
    let row = |bound: &str| {
        format!("{{ \"name\": \"net_in\", \"argument\": \"{bound}\", \"bounded\": true }}")
    };
    let loopback = authority_json(
        &call_under("127.0.0.1:8080", r#"tcp_listen_on(n, "127.0.0.1", 8080, 8, 0)"#),
        "listen-authority-loopback",
    );
    assert!(loopback.contains(&row("127.0.0.1:8080")), "{loopback}");
    assert!(!loopback.contains(&row("8080")), "loopback must not read as everywhere:\n{loopback}");

    let everywhere = authority_json(
        &call_under("8080", r#"tcp_listen_on(n, "0.0.0.0", 8080, 8, 0)"#),
        "listen-authority-everywhere",
    );
    assert!(everywhere.contains(&row("8080")), "{everywhere}");
    assert!(!everywhere.contains("127.0.0.1"), "{everywhere}");

    let old =
        authority_json(&call_under("8080", "tcp_listen(n, 8080, 8, 0)"), "listen-authority-old");
    assert!(old.contains(&row("8080")), "{old}");
    // The same row as the explicit everywhere form: they say the same thing.
    assert_eq!(
        old.lines().filter(|l| l.contains("net_in")).collect::<Vec<_>>(),
        everywhere.lines().filter(|l| l.contains("net_in")).collect::<Vec<_>>()
    );
}
