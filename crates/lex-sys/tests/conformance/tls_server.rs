//! `packages/tls`'s server with no network (`docs/tls-server.md` §8, step 2):
//! the connections of `scripts/tls_liar_client.py`'s lying client replayed
//! byte for byte on both backends, each ending with its tag, through
//! `tests/programs/tls_server_driver.ls`. Every refusal tag of §5.4 but
//! `tls-server-sign-check` (which needs a fault in the signer) is reached,
//! and every rule of §5.2; and the honest connections again with the
//! client's bytes fed one byte a line.

use super::json::feed;
use super::*;

/// The package's files: the engine, the client it is built on, the server,
/// and `packages/x509` with the key parser.
pub(super) fn package_files() -> Vec<PathBuf> {
    let tls = [
        "tls.ls",
        "record.ls",
        "message.ls",
        "slot.ls",
        "client12.ls",
        "client.ls",
        "hello.ls",
        "identity.ls",
        "server.ls",
    ];
    let x509 = ["verify.ls", "names.ls", "x509.ls", "key.ls"];
    let mut files: Vec<PathBuf> =
        tls.iter().map(|f| repo_root().join("packages/tls").join(f)).collect();
    files.extend(x509.iter().map(|f| repo_root().join("packages/x509").join(f)));
    files
}

fn build_server_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("tls-server-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/tls_server_driver.ls"))
        .args(package_files())
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

/// `liar_client.txt`'s cases: the tag each must end with, its name, its
/// driver lines and the answers recorded for them.
type Case = (String, String, Vec<String>, Vec<String>);

fn cases() -> Vec<Case> {
    let text =
        std::fs::read_to_string(repo_root().join("tests/vectors/tls/liar_client.txt")).unwrap();
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

/// Each lying client's connection replays byte for byte and ends with its
/// tag: a refused connection failed (event 5), an honest one closed
/// (event 4) or, for the configuration's cases, answered on its line.
#[test]
fn every_lying_client_is_refused_with_its_own_tag_on_both_backends() {
    let cases = cases();
    assert_eq!(cases.len(), 107);
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_server_driver("liar", backend);
        for (tag, name, asked, answered) in &cases {
            assert_eq!(asked.len(), answered.len(), "{name}");
            let got = run(&exe, asked);
            for (n, (g, w)) in got.iter().zip(answered).enumerate() {
                assert_eq!(g, w, "{name} line {n} on {backend}");
            }
            let last_op = asked.last().unwrap().split(' ').next().unwrap();
            let last = got.last().unwrap();
            if ["F", "Z"].contains(&last_op) {
                assert_eq!(field(last, 1), tag, "{name}");
                assert_eq!(field(last, 2), "5", "{name}: {last}");
            } else if tag != "ok" {
                assert!(got.iter().any(|a| field(a, 1) == tag), "{name}: {tag} on some line");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The honest connections with the client's bytes fed one byte a line:
/// every record and handshake message reassembled, the ClientHello's
/// included. The server must send exactly what it sent before and receive
/// exactly the same data.
#[test]
fn the_client_bytes_in_any_split_give_the_same_connection() {
    let (dir, exe) = build_server_driver("splits", "llvm");
    let honest: Vec<Case> = cases()
        .into_iter()
        .filter(|(tag, name, asked, _)| {
            tag == "ok" && name.starts_with("honest") && asked.iter().any(|l| l.starts_with("Q"))
        })
        .collect();
    assert!(honest.len() >= 20, "{} honest connections", honest.len());
    for (_, name, asked, answered) in &honest {
        let total = |answers: &[String], asked: &[String], n: usize| {
            answers
                .iter()
                .zip(asked)
                .filter(|(_, q)| ["V", "F", "W", "Q", "Z"].iter().any(|op| q.starts_with(op)))
                .map(|(a, _)| hex(field(a, n)))
                .collect::<String>()
        };
        let mut bytewise = Vec::new();
        for line in asked {
            if let Some(data) = line.strip_prefix("F ") {
                for k in (0..data.len()).step_by(2) {
                    bytewise.push(format!("F {}", &data[k..k + 2]));
                }
            } else {
                bytewise.push(line.clone());
            }
        }
        let got = run(&exe, &bytewise);
        assert_eq!(
            total(&got, &bytewise, 3),
            total(answered, asked, 3),
            "{name}: what the server sent"
        );
        assert_eq!(
            total(&got, &bytewise, 4),
            total(answered, asked, 4),
            "{name}: what it received"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
