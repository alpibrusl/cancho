//! `packages/tls` with no network (`docs/tls-core.md` §6.1): eleven recorded
//! handshakes replayed byte for byte on both backends -- five TLS 1.3
//! against tlslite-ng (ChaCha20 with X25519, AES-256-GCM with X25519, and a
//! HelloRetryRequest to P-256 and to P-384 under AES-GCM,
//! `docs/tls-parity.md` §3.3) and six TLS 1.2 against `openssl s_server`,
//! one a suite (§3.4) -- the TLS 1.2 PRF against its definition,
//! the same server bytes fed one byte at a time and all at once, a wrong
//! root, a crafted ServerHello for each rule of RFC 8446 §4.1.3 the
//! client enforces, and the 84 connections of `scripts/tls_liar.py`'s
//! lying server (§6.3), resumption's among them. All through
//! `tests/programs/tls_driver.ls`; and the engine's rules for offering a
//! saved ticket (`docs/tls-resumption.md` §3), through
//! `tests/programs/tls_tickets.ls`.

use super::json::feed;
use super::*;

fn build_tls_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("tls-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/tls_driver.ls"))
        .args(
            ["record.ls", "message.ls", "slot.ls", "client12.ls", "client.ls"]
                .map(|f| repo_root().join("packages/tls").join(f)),
        )
        .args(
            ["verify.ls", "names.ls", "x509.ls"].map(|f| repo_root().join("packages/x509").join(f)),
        )
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

fn run(exe: &Path, lines: &[String]) -> Vec<String> {
    let mut input = lines.join("\n");
    input.push('\n');
    let out = feed(exe, input.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let answers: Vec<String> =
        String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    assert_eq!(answers.len(), lines.len(), "one answer per line");
    answers
}

/// A trace's driver lines and the answers recorded for them.
fn trace(name: &str) -> (Vec<String>, Vec<String>) {
    let text = std::fs::read_to_string(repo_root().join("tests/vectors/tls").join(name)).unwrap();
    let (mut asked, mut answered) = (Vec::new(), Vec::new());
    for line in text.lines() {
        if let Some(a) = line.strip_prefix("= ") {
            answered.push(a.to_string());
        } else if !line.starts_with('#') {
            asked.push(line.to_string());
        }
    }
    assert_eq!(asked.len(), answered.len());
    (asked, answered)
}

/// `tests/programs/tls_tickets.ls`: the engine, built with the whole package.
fn build_tls_tickets(backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("tls-tickets-{backend}"));
    let exe = dir.join("tickets");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/tls_tickets.ls"))
        .args(super::tls_server::package_files())
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

/// Each of `scripts/tls_tickets.py`'s cases replays byte for byte: whether
/// the engine puts a saved ticket on the wire is decided by the rules of
/// `docs/tls-resumption.md` §3, and the ClientHello it sends is in the answers.
#[test]
fn the_engine_offers_a_ticket_only_where_the_rules_allow_on_both_backends() {
    let text = std::fs::read_to_string(repo_root().join("tests/vectors/tls/tickets.txt")).unwrap();
    let mut cases: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("## ") {
            cases.push((name.to_string(), Vec::new(), Vec::new()));
        } else if line.starts_with('#') {
        } else if let Some(a) = line.strip_prefix("= ") {
            cases.last_mut().unwrap().2.push(a.to_string());
        } else {
            cases.last_mut().unwrap().1.push(line.to_string());
        }
    }
    assert_eq!(cases.len(), 20);
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_tls_tickets(backend);
        for (name, asked, answered) in &cases {
            let got = run(&exe, asked);
            for (n, (g, w)) in got.iter().zip(answered).enumerate() {
                assert_eq!(g, w, "{name} line {n} on {backend}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `liar.txt`'s cases: the tag each must end with, its name, its driver
/// lines and the answers recorded for them.
type Case = (String, String, Vec<String>, Vec<String>);

fn liar_cases() -> Vec<Case> {
    let text = std::fs::read_to_string(repo_root().join("tests/vectors/tls/liar.txt")).unwrap();
    let mut cases: Vec<Case> = Vec::new();
    for line in text.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            let (tag, name) = head.split_once(' ').unwrap();
            cases.push((tag.to_string(), name.to_string(), Vec::new(), Vec::new()));
        } else if line.starts_with('#') {
        } else if let Some(a) = line.strip_prefix("= ") {
            cases.last_mut().unwrap().3.push(a.to_string());
        } else {
            cases.last_mut().unwrap().2.push(line.to_string());
        }
    }
    cases
}

fn field(line: &str, n: usize) -> &str {
    line.split(' ').nth(n).unwrap_or("")
}

fn hex(s: &str) -> String {
    if s == "-" { String::new() } else { s.to_string() }
}

/// The recorded handshakes: `scripts/tls_trace.py`'s certificate, suite and
/// group for each, against tlslite-ng (TLS 1.3) and `openssl s_server
/// -tls1_2` (`docs/tls-parity.md` §3.4).
const TRACES: [&str; 11] = [
    "tlslite_rsa.txt",
    "tlslite_ecdsa.txt",
    "tlslite_aes256_x25519.txt",
    "tlslite_aes128_p256.txt",
    "tlslite_aes256_p384.txt",
    "openssl12_ecdhe_ecdsa_aes128_gcm_sha256.txt",
    "openssl12_ecdhe_ecdsa_aes256_gcm_sha384.txt",
    "openssl12_ecdhe_ecdsa_chacha20_poly1305.txt",
    "openssl12_ecdhe_rsa_aes128_gcm_sha256.txt",
    "openssl12_ecdhe_rsa_aes256_gcm_sha384.txt",
    "openssl12_ecdhe_rsa_chacha20_poly1305.txt",
];

#[test]
fn every_recorded_handshake_replays_byte_for_byte_on_both_backends() {
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_tls_driver("replay", backend);
        for name in TRACES {
            let (asked, answered) = trace(name);
            let got = run(&exe, &asked);
            for (n, (g, w)) in got.iter().zip(&answered).enumerate() {
                assert_eq!(g, w, "{name} line {n} on {backend}");
            }
            assert_eq!(field(answered.last().unwrap(), 2), "4", "{name}: closed");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Each lying server's connection replays byte for byte and ends with its
/// tag; the honest ones end closed.
#[test]
fn every_lying_server_is_refused_with_its_own_tag_on_both_backends() {
    let cases = liar_cases();
    assert_eq!(cases.len(), 84);
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_tls_driver("liar", backend);
        for (tag, name, asked, answered) in &cases {
            assert_eq!(asked.len(), answered.len(), "{name}");
            let got = run(&exe, asked);
            for (n, (g, w)) in got.iter().zip(answered).enumerate() {
                assert_eq!(g, w, "{name} line {n} on {backend}");
            }
            let last = got.last().unwrap();
            assert_eq!(field(last, 1), tag, "{name}");
            assert_eq!(field(last, 2), if tag == "ok" { "4" } else { "5" }, "{name}: {last}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The same server bytes, one byte a line (fragmentation: every record and
/// handshake message reassembled) and the handshake flight in one line
/// (coalescing): the client must send exactly what it sent before and
/// receive exactly the same data.
#[test]
fn the_same_bytes_in_any_split_give_the_same_connection() {
    let (dir, exe) = build_tls_driver("splits", "llvm");
    for name in TRACES {
        let (asked, answered) = trace(name);
        let total = |answers: &[String], n: usize| {
            answers.iter().map(|a| hex(field(a, n))).collect::<String>()
        };
        let want_out = total(&answered, 3);
        let want_in = total(&answered, 4);
        // One byte a line.
        let mut bytewise = Vec::new();
        for line in &asked {
            if let Some(data) = line.strip_prefix("F ") {
                for k in (0..data.len()).step_by(2) {
                    bytewise.push(format!("F {}", &data[k..k + 2]));
                }
            } else {
                bytewise.push(line.clone());
            }
        }
        let got = run(&exe, &bytewise);
        assert_eq!(total(&got, 3), want_out, "{name}: one byte a line, what the client sent");
        assert_eq!(total(&got, 4), want_in, "{name}: one byte a line, what the client received");
        assert_eq!(field(got.last().unwrap(), 2), "4", "{name}: closed");
        // Every F before the request in one line.
        let first_w = asked.iter().position(|l| l.starts_with("W ")).unwrap();
        let flight: String = asked[1..first_w].iter().map(|l| &l[2..]).collect();
        let mut coalesced = vec![asked[0].clone(), format!("F {flight}")];
        coalesced.extend(asked[first_w..].iter().cloned());
        let got = run(&exe, &coalesced);
        assert_eq!(total(&got, 3), want_out, "{name}: coalesced, what the client sent");
        assert_eq!(total(&got, 4), want_in, "{name}: coalesced, what the client received");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A server whose certificate another CA issued is refused, with the
/// unknown_ca alert, before anything else of its flight is read: the RSA
/// trace's server against the ECDSA trace's root.
#[test]
fn a_certificate_from_another_ca_is_refused() {
    let (asked, _) = trace("tlslite_rsa.txt");
    let (other, _) = trace("tlslite_ecdsa.txt");
    let other_root = field(&other[0], 3);
    let mut lines = vec![format!(
        "C {} {} {other_root} {}",
        field(&asked[0], 1),
        field(&asked[0], 2),
        field(&asked[0], 4)
    )];
    lines.extend(asked[1..3].iter().cloned());
    let (dir, exe) = build_tls_driver("pin", "cranelift");
    let got = run(&exe, &lines);
    let last = got.last().unwrap();
    assert_eq!(field(last, 1), "x509-unknown-issuer", "{last}");
    assert_eq!(field(last, 2), "5", "failed");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A ServerHello is plaintext, so each rule can be broken offline: build a
/// ServerHello answering the recorded ClientHello and change one thing.
#[test]
fn every_server_hello_rule_is_refused_with_its_own_tag() {
    let (asked, _) = trace("tlslite_ecdsa.txt");
    let start = asked[0].clone();
    // The ClientHello's session id is bytes 32..64 of the fixed randomness.
    let sid: String = (32u8..64).map(|b| format!("{b:02x}")).collect();
    let share = format!("09{}", "00".repeat(31));
    // A ServerHello record; `trailer` is bytes after the extensions, inside
    // the message.
    let hello_with =
        |version: &str, random: &str, sid: &str, suite: &str, exts: &str, trailer: &str| {
            let body = format!(
                "{version}{random}{:02x}{sid}{suite}00{:04x}{exts}{trailer}",
                sid.len() / 2,
                exts.len() / 2
            );
            let msg = format!("02{:06x}{body}", body.len() / 2);
            format!("F 16030300{:02x}{msg}", msg.len() / 2)
        };
    let hello = |version: &str, random: &str, sid: &str, suite: &str, exts: &str| {
        hello_with(version, random, sid, suite, exts, "")
    };
    let random = "11".repeat(32);
    let versions = "002b00020304";
    let key_share = format!("00330024001d0020{share}");
    let good = format!("{versions}{key_share}");
    let hrr = "cf21ad74e59a6111be1d8c021e65b891c2a211167abb8c5e079e09e2c8a8339c";
    let downgrade = format!("{}444f574e47524401", "11".repeat(24));
    let cases: Vec<(&str, String, &str)> = vec![
        ("a valid one", hello("0303", &random, &sid, "1303", &good), "ok"),
        (
            "a HelloRetryRequest for P-256",
            hello("0303", hrr, &sid, "1301", &format!("{versions}003300020017")),
            "ok",
        ),
        (
            "a HelloRetryRequest with a share in it",
            hello("0303", hrr, &sid, "1303", &good),
            "tls-decode-error",
        ),
        (
            "a HelloRetryRequest for X25519, already sent",
            hello("0303", hrr, &sid, "1303", &format!("{versions}00330002001d")),
            "tls-key-share",
        ),
        (
            "a HelloRetryRequest that changes nothing",
            hello("0303", hrr, &sid, "1303", versions),
            "tls-hello-retry",
        ),
        (
            "the downgrade sentinel in a TLS 1.3 ServerHello: random bytes (RFC 8446 §4.1.3)",
            hello("0303", &downgrade, &sid, "1303", &good),
            "ok",
        ),
        (
            "server_name acknowledged, empty, in a TLS 1.2 ServerHello (RFC 6066 §3; nginx sends it)",
            hello("0303", &random, &"aa".repeat(32), "c02f", "ff010001000017000000000000"),
            "ok",
        ),
        (
            "server_name in a TLS 1.3 ServerHello: its place is EncryptedExtensions",
            hello("0303", &random, &sid, "1303", &format!("{good}00000000")),
            "tls-unsupported-extension",
        ),
        (
            "server_name twice in a TLS 1.2 ServerHello",
            hello(
                "0303",
                &random,
                &"aa".repeat(32),
                "c02f",
                "ff0100010000170000000000000000000000",
            ),
            "tls-decode-error",
        ),
        (
            "the downgrade sentinel in a TLS 1.2 ServerHello",
            hello("0303", &downgrade, &"aa".repeat(32), "c02f", "ff0100010000170000"),
            "tls-protocol-version",
        ),
        (
            "no supported_versions (TLS 1.2), with a TLS 1.3 suite",
            hello("0303", &random, &sid, "1303", &key_share),
            "tls-no-shared-cipher",
        ),
        (
            "TLS 1.2 with no extended master secret",
            hello("0303", &random, &"aa".repeat(32), "c02f", "ff01000100"),
            "tls-extended-master-secret",
        ),
        (
            "TLS 1.2, valid",
            hello("0303", &random, &"aa".repeat(32), "c02f", "ff0100010000170000"),
            "ok",
        ),
        (
            "supported_versions 1.2",
            hello("0303", &random, &sid, "1303", &format!("002b00020303{key_share}")),
            "tls-protocol-version",
        ),
        ("AES-128-GCM chosen", hello("0303", &random, &sid, "1301", &good), "ok"),
        ("AES-256-GCM chosen", hello("0303", &random, &sid, "1302", &good), "ok"),
        (
            "TLS_AES_128_CCM_SHA256",
            hello("0303", &random, &sid, "1304", &good),
            "tls-no-shared-cipher",
        ),
        (
            "ALPN, never offered",
            hello("0303", &random, &sid, "1303", &format!("{good}001000050003026832")),
            "tls-unsupported-extension",
        ),
        (
            "a duplicate supported_versions",
            hello("0303", &random, &sid, "1303", &format!("{versions}{good}")),
            "tls-decode-error",
        ),
        (
            "another session id",
            hello("0303", &random, &"00".repeat(32), "1303", &good),
            "tls-decode-error",
        ),
        (
            "a P-256 share, never sent",
            hello(
                "0303",
                &random,
                &sid,
                "1303",
                &format!("{versions}003300450017004104{}", "11".repeat(64)),
            ),
            "tls-key-share",
        ),
        (
            "an all-zero share",
            hello(
                "0303",
                &random,
                &sid,
                "1303",
                &format!("{versions}00330024001d0020{}", "00".repeat(32)),
            ),
            "tls-key-share",
        ),
        (
            "a byte after the extensions",
            hello_with("0303", &random, &sid, "1303", &good, "00"),
            "tls-decode-error",
        ),
        ("a record over 2^14 + 256", "F 1603034201".to_string(), "tls-record-overflow"),
        ("an unknown content type", "F 1803030001".to_string(), "tls-unexpected-message"),
        (
            "application data before the handshake",
            format!("F 1703030010{}", "00".repeat(16)),
            "tls-unexpected-message",
        ),
    ];
    let (dir, exe) = build_tls_driver("server-hello", "cranelift");
    for (what, line, tag) in &cases {
        let got = run(&exe, &[start.clone(), line.clone()]);
        assert_eq!(field(&got[1], 1), *tag, "{what}: {}", got[1]);
        if *tag != "ok" {
            assert_eq!(field(&got[1], 2), "5", "{what}: failed");
            assert!(
                field(&got[1], 3).starts_with("15030300020"),
                "{what}: a fatal alert is sent: {}",
                got[1]
            );
        }
    }
    // The record layer's other half: a bit flipped in the server's encrypted
    // flight is a bad record MAC.
    let (rsa, _) = trace("tlslite_rsa.txt");
    let flight = &rsa[2][2..];
    let mut bytes: Vec<u8> = (0..flight.len())
        .step_by(2)
        .map(|k| u8::from_str_radix(&flight[k..k + 2], 16).unwrap())
        .collect();
    bytes[40] ^= 1;
    let flipped: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let got = run(&exe, &[rsa[0].clone(), rsa[1].clone(), format!("F {flipped}")]);
    assert_eq!(field(&got[2], 1), "tls-bad-record-mac", "{}", got[2]);
    let _ = std::fs::remove_dir_all(&dir);
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|k| u8::from_str_radix(&s[k..k + 2], 16).unwrap()).collect()
}

/// One recorded stream: the ClientHello it answers, the server's flight,
/// its reply, and the response's length and SHA-256 as `tls_many` prints
/// them.
struct Stream {
    flight: Vec<u8>,
    reply: Vec<u8>,
    seen: String,
}

/// Reads one record's worth, or what is left at the end, into `got`.
fn read_record(conn: &mut std::net::TcpStream, got: &mut Vec<u8>) -> Option<Vec<u8>> {
    use std::io::Read;
    loop {
        if got.len() >= 5 {
            let n = 5 + usize::from(got[3]) * 256 + usize::from(got[4]);
            if got.len() >= n {
                return Some(got.drain(..n).collect());
            }
        }
        let mut buf = [0u8; 65536];
        match conn.read(&mut buf) {
            Ok(0) | Err(_) => return None,
            Ok(k) => got.extend_from_slice(&buf[..k]),
        }
    }
}

/// The engine and the poller (`tls_many`), 64 connections at once on one
/// thread, against `scripts/tls_liar.py`'s honest server replayed from
/// `streams.txt`: the server's bytes for each ClientHello the fixed seed
/// gives. Each socket read is one byte, then 65,536: every connection must
/// end with close_notify and the response recorded for it. Then the same
/// server cut short, its close_notify left out and the socket closed: the
/// data may have been truncated, and every connection fails
/// `tls-peer-closed` (RFC 8446 §6.1).
#[test]
fn sixty_four_connections_on_one_thread_fed_one_byte_and_in_bulk() {
    use std::collections::HashMap;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    let text = std::fs::read_to_string(repo_root().join("tests/vectors/tls/streams.txt")).unwrap();
    let seed = text.lines().find_map(|l| l.split("seed ").nth(1)).unwrap()[..64].to_string();
    let mut streams = HashMap::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split(' ').collect();
        let seen = format!("{} {}", f[3], f[4]);
        streams.insert(unhex(f[0]), Stream { flight: unhex(f[1]), reply: unhex(f[2]), seen });
    }
    assert_eq!(streams.len(), 64);
    let mut want: Vec<String> = streams.values().map(|s| s.seen.clone()).collect();
    want.sort();
    let streams = Arc::new(streams);
    let dir = scratch("tls-many");
    let exe = dir.join("tls_many");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", "llvm"])
        .arg(repo_root().join("tests/programs/tls_many.ls"))
        .args(super::tls_server::package_files())
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let pem = std::fs::read(repo_root().join("tests/vectors/tls/streams.pem")).unwrap();
    // close_notify's record: a 5-byte header, 2 bytes, the type, the tag.
    let notify = 5 + 2 + 1 + 16;
    for (chunk, cut) in [("1", false), ("65536", false), ("65536", true)] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port().to_string();
        let served = Arc::new(Mutex::new(0usize));
        let (all, counter) = (Arc::clone(&streams), Arc::clone(&served));
        let server = std::thread::spawn(move || {
            let mut handlers = Vec::new();
            for _ in 0..64 {
                let (mut conn, _) = listener.accept().unwrap();
                let (streams, counter) = (Arc::clone(&all), Arc::clone(&counter));
                handlers.push(std::thread::spawn(move || {
                    let mut got = Vec::new();
                    let Some(hello) = read_record(&mut conn, &mut got) else { return };
                    let Some(s) = streams.get(&hello) else { return };
                    conn.write_all(&s.flight).unwrap();
                    // The reply after the client's Finished and request.
                    let mut encrypted = 0;
                    while encrypted < 2 {
                        let Some(r) = read_record(&mut conn, &mut got) else { return };
                        encrypted += usize::from(r[0] == 23);
                    }
                    if cut {
                        conn.write_all(&s.reply[..s.reply.len() - notify]).unwrap();
                        let _ = conn.shutdown(std::net::Shutdown::Write);
                    } else {
                        conn.write_all(&s.reply).unwrap();
                    }
                    while read_record(&mut conn, &mut got).is_some() {}
                    *counter.lock().unwrap() += 1;
                }));
            }
            for h in handlers {
                let _ = h.join();
            }
        });
        let mut child = Command::new(&exe)
            .args(["127.0.0.1", &port, "liar.lex-sys.test", "64", chunk, &seed])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&pem).unwrap();
        let out = child.wait_with_output().unwrap();
        let lines: Vec<String> =
            String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
        server.join().unwrap();
        assert_eq!(
            *served.lock().unwrap(),
            64,
            "chunk {chunk}: the server saw every connection end"
        );
        if cut {
            assert_eq!(lines.last().map(String::as_str), Some("done ok=0 failed=64"), "{lines:?}");
            assert!(lines[..64].iter().all(|l| field(l, 2) == "tls-peer-closed"), "{lines:?}");
            continue;
        }
        assert_eq!(lines.last().map(String::as_str), Some("done ok=64 failed=0"), "{lines:?}");
        let mut got: Vec<String> =
            lines[..64].iter().map(|l| field(l, 3).to_string() + " " + field(l, 4)).collect();
        got.sort();
        assert_eq!(got, want, "chunk {chunk}: every response, once");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// RFC 8448 §3's encrypted server flight (`tests/vectors/tls/rfc8448_record.txt`)
/// opened by `tls_record.open` under TLS_AES_128_GCM_SHA256, on both backends:
/// the content type is the inner plaintext's last byte, and the content
/// everything before it. With one bit flipped, the record is refused. RFC
/// 8448's whole handshake cannot be replayed (`docs/tls-core.md` §6.2), but
/// its record protection can.
#[test]
fn rfc8448s_server_flight_opens_under_aes_128_gcm() {
    let text =
        std::fs::read_to_string(repo_root().join("tests/vectors/tls/rfc8448_record.txt")).unwrap();
    let value = |name: &str| {
        text.lines().find_map(|l| l.strip_prefix(&format!("{name} "))).unwrap().to_string()
    };
    let (key, iv, seq, record, inner) =
        (value("key"), value("iv"), value("seq"), value("record"), value("inner"));
    assert_eq!(record.len() / 2, 5 + 0x2a2);
    assert!(inner.ends_with("16"), "a handshake record");
    let content = &inner[..inner.len() - 2];
    let mut flipped = unhex(&record);
    flipped[100] ^= 4;
    let flipped: String = flipped.iter().map(|b| format!("{b:02x}")).collect();
    let lines = vec![
        format!("O 1301 {key} {iv} {seq} {record}"),
        format!("O 1301 {key} {iv} {seq} {flipped}"),
        format!("O 1303 {key}{key} {iv} {seq} {record}"),
    ];
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_tls_driver("rfc8448", backend);
        let got = run(&exe, &lines);
        assert_eq!(got[0], format!("0 ok 22 {content}"), "{backend}");
        assert_eq!(field(&got[1], 1), "tls-bad-record-mac", "{backend}");
        assert_eq!(field(&got[2], 1), "tls-bad-record-mac", "the other AEAD, {backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The TLS 1.2 PRF (`tls_record.prf`, RFC 5246 §5) and the extended master
/// secret (RFC 7627 §4) against `tests/vectors/tls/prf12.txt`, whose rows
/// `scripts/tls12_prf_differential.py` printed from OpenSSL's TLS1-PRF and
/// checked against Python's `hmac`, on both backends.
#[test]
fn the_tls12_prf_matches_openssl_on_both_backends() {
    let text = std::fs::read_to_string(repo_root().join("tests/vectors/tls/prf12.txt")).unwrap();
    let rows: Vec<(String, String)> = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let (case, want) = l.split_once(" | ").unwrap();
            (case.to_string(), want.to_string())
        })
        .collect();
    assert_eq!(rows.len(), 40);
    let cases: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_tls_driver("prf", backend);
        for ((case, want), got) in rows.iter().zip(run(&exe, &cases)) {
            assert_eq!(got, format!("0 ok {want}"), "{case} on {backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
