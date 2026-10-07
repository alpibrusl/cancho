//! `packages/x509`'s verifier (`docs/x509-verify.md` §6), with no network
//! and no OpenSSL: x509-limbo's cases that `scripts/x509_limbo.py` wrote,
//! limbo's 14 saved real chains against the system roots of
//! `scripts/x509_online.py`, and the OpenSSL matrix of
//! `scripts/x509_matrix.py`, each replayed through
//! `tests/programs/x509_verify_driver.cho` with every answer as recorded.

use super::json::feed;
use super::*;

fn build_verifier(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("x509-verify-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/x509_verify_driver.cho"))
        .args(
            ["verify.cho", "names.cho", "x509.cho"]
                .map(|f| repo_root().join("packages/x509").join(f)),
        )
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

fn text(name: &str) -> String {
    std::fs::read_to_string(repo_root().join("tests/vectors/x509/verify").join(name)).unwrap()
}

/// A file's driver lines and the answers recorded for them, with each
/// answer's case header (the last `## ` line before it).
fn recorded(name: &str) -> (Vec<String>, Vec<(String, String)>) {
    let (mut asked, mut answered) = (Vec::new(), Vec::new());
    let mut case = String::new();
    for line in text(name).lines() {
        if let Some(head) = line.strip_prefix("## ") {
            case = head.to_string();
        } else if let Some(a) = line.strip_prefix("= ") {
            answered.push((case.clone(), a.to_string()));
        } else if line == "S @roots.pem" {
            let roots =
                std::fs::read(repo_root().join("tests/vectors/x509/verify/roots.pem")).unwrap();
            asked.push(format!(
                "S {}",
                roots.iter().map(|b| format!("{b:02x}")).collect::<String>()
            ));
        } else if !line.starts_with('#') {
            asked.push(line.to_string());
        }
    }
    assert_eq!(asked.len(), answered.len(), "{name}");
    (asked, answered)
}

fn replay(exe: &Path, name: &str) -> Vec<(String, String)> {
    let (asked, answered) = recorded(name);
    let mut input = asked.join("\n");
    input.push('\n');
    let out = feed(exe, input.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let got: Vec<String> =
        String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    assert_eq!(got.len(), answered.len(), "{name}: one answer per line");
    for (g, (case, want)) in got.iter().zip(&answered) {
        assert_eq!(g, want, "{name}: {case}");
    }
    answered
}

fn tag(answer: &str) -> &str {
    answer.split(' ').nth(1).unwrap_or("")
}

/// Every case `scripts/x509_limbo.py` marked a pass ends as limbo expects:
/// SUCCESS accepted, FAILURE refused. Disagreements and the not
/// applicable are replayed too, as recorded.
#[test]
fn x509_limbo_cases_replay_and_pass_where_recorded() {
    let (dir, exe) = build_verifier("limbo", "llvm");
    let (asked, _) = recorded("limbo_subset.txt");
    let answered = replay(&exe, "limbo_subset.txt");
    let mut passes = 0;
    for (line, (case, answer)) in asked.iter().zip(&answered) {
        let f: Vec<&str> = case.split(' ').collect();
        if line.starts_with("V ") && f[2] == "pass" {
            assert_eq!(f[1] == "SUCCESS", tag(answer) == "ok", "{case}: {answer}");
            passes += 1;
        }
    }
    assert!(passes > 400, "{passes} passing cases replayed");
    let _ = std::fs::remove_dir_all(&dir);
}

/// limbo's 14 real chains against the 128 system roots: valid at their
/// saved time and at exactly the leaf's notAfter and notBefore, expired a
/// second after, not yet valid a second before, and not valid for another
/// name. On both backends.
#[test]
fn saved_real_chains_against_the_system_roots_on_both_backends() {
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_verifier("online", backend);
        let answered = replay(&exe, "online.txt");
        assert_eq!(answered[0].1, "128 0 138350", "every system root stored");
        let tags: Vec<&str> = answered[1..].iter().map(|(_, a)| tag(a)).collect();
        assert_eq!(tags.len(), 84);
        for (k, t) in tags.iter().enumerate() {
            let want =
                ["ok", "x509-expired", "x509-not-yet-valid", "x509-name-mismatch", "ok", "ok"]
                    [k % 6];
            assert_eq!(*t, want, "check {k} on {backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The OpenSSL matrix: each case gets its own tag, and none that OpenSSL
/// refused is accepted. On both backends.
#[test]
fn the_openssl_matrix_gets_each_tag_and_accepts_nothing_openssl_refused() {
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_verifier("matrix", backend);
        let answered = replay(&exe, "matrix.txt");
        for (case, answer) in &answered[1..] {
            let f: Vec<&str> = case.splitn(3, ' ').collect();
            assert_eq!(tag(answer), f[0], "{case}");
            if f[1] != "openssl=ok" {
                assert_ne!(tag(answer), "ok", "{case}: OpenSSL refused it");
            }
        }
        assert_eq!(answered.len(), 36);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A chain with no name (`docs/x509-verify.md` §10.4): `verify_chain` for
/// `clientAuth` and `serverAuth` beside `openssl verify -purpose sslclient`
/// and `sslserver`, each case with its tag and, when accepted, the subject
/// and SAN entries read back; `verify` refusing every host that is no name;
/// `verify_name` alone; purposes that are neither. None that OpenSSL refused
/// is accepted, but the one disagreement listed (a root's EKU, which `verify`
/// does not read). On both backends.
#[test]
fn chains_without_a_name_get_each_tag_and_accept_nothing_openssl_refused() {
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_verifier("chain", backend);
        let answered = replay(&exe, "chain_matrix.txt");
        let mut accepted = 0;
        for (case, answer) in &answered[1..] {
            let f: Vec<&str> = case.splitn(3, ' ').collect();
            assert_eq!(tag(answer), f[0], "{case}");
            if f[1] != "openssl=ok"
                && f[1] != "openssl=-"
                && !f[2].starts_with("known disagreement")
            {
                assert_ne!(tag(answer), "ok", "{case}: OpenSSL refused it");
            }
            if answer.starts_with("0 ok ") {
                accepted += 1;
            }
        }
        assert_eq!(answered.len(), 57);
        assert_eq!(accepted, 17, "every accepted chain carries its subject and SANs");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
