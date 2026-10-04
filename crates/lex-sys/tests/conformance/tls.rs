//! `packages/tls` with no network (`docs/tls-core.md` §6.1): two recorded
//! handshakes against tlslite-ng replayed byte for byte on both backends,
//! the same server bytes fed one byte at a time and all at once, a wrong
//! pin, and a crafted ServerHello for each rule of RFC 8446 §4.1.3 the
//! client enforces. All through `tests/programs/tls_driver.ls`.

use super::json::feed;
use super::*;

fn build_tls_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("tls-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/tls_driver.ls"))
        .args(
            ["record.ls", "message.ls", "client.ls"]
                .map(|f| repo_root().join("packages/tls").join(f)),
        )
        .arg(repo_root().join("packages/x509/x509.ls"))
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

fn field(line: &str, n: usize) -> &str {
    line.split(' ').nth(n).unwrap_or("")
}

fn hex(s: &str) -> String {
    if s == "-" { String::new() } else { s.to_string() }
}

#[test]
fn both_recorded_handshakes_replay_byte_for_byte_on_both_backends() {
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_tls_driver("replay", backend);
        for name in ["tlslite_rsa.txt", "tlslite_ecdsa.txt"] {
            let (asked, answered) = trace(name);
            let got = run(&exe, &asked);
            for (n, (g, w)) in got.iter().zip(&answered).enumerate() {
                assert_eq!(g, w, "{name} line {n} on {backend}");
            }
            assert_eq!(
                field(answered.last().unwrap(), 2),
                "4",
                "{name}: the server's close_notify was seen"
            );
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
    for name in ["tlslite_rsa.txt", "tlslite_ecdsa.txt"] {
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

/// A server whose certificate is not the pinned one is refused, with the
/// unknown_ca alert, before anything else of its flight is read.
#[test]
fn an_unpinned_certificate_is_refused() {
    let (asked, _) = trace("tlslite_rsa.txt");
    let (other, _) = trace("tlslite_ecdsa.txt");
    let other_pins = field(&other[0], 3);
    let mut lines = vec![format!("C {} {} {other_pins}", field(&asked[0], 1), field(&asked[0], 2))];
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
        ("HelloRetryRequest", hello("0303", hrr, &sid, "1303", &good), "tls-hello-retry"),
        (
            "the downgrade sentinel",
            hello("0303", &downgrade, &sid, "1303", &good),
            "tls-protocol-version",
        ),
        (
            "TLS 1.2: no supported_versions",
            hello("0303", &random, &sid, "1303", &key_share),
            "tls-protocol-version",
        ),
        (
            "supported_versions 1.2",
            hello("0303", &random, &sid, "1303", &format!("002b00020303{key_share}")),
            "tls-protocol-version",
        ),
        ("AES-128-GCM chosen", hello("0303", &random, &sid, "1301", &good), "tls-no-shared-cipher"),
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
            "a P-256 share",
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
