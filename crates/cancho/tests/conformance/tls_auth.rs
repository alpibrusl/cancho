//! `packages/tls`'s client certificates and ALPN with no network (`docs/tls-parity.md` §6): the connections of
//! `scripts/tls_liar_auth.py`'s lying server replayed byte for byte on both backends, each ending with its tag. The
//! server there checks, with pyca/cryptography, the client's Certificate, its CertificateVerify under the identity's key
//! (TLS 1.3's context string, TLS 1.2's SHA-256 over every message) and the Finished that covers both, so a replay that
//! matches is a signature that verified when it was recorded.

use super::*;

fn cases() -> Vec<(String, String, Vec<String>, Vec<String>)> {
    let text =
        std::fs::read_to_string(repo_root().join("tests/vectors/tls/liar_auth.txt")).unwrap();
    let mut cases: Vec<(String, String, Vec<String>, Vec<String>)> = Vec::new();
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

#[test]
fn every_client_certificate_and_alpn_case_replays_and_ends_with_its_tag_on_both_backends() {
    let cases = cases();
    assert_eq!(cases.len(), 77);
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = super::tls::build_tls_driver("auth", backend);
        for (tag, name, asked, answered) in &cases {
            assert_eq!(asked.len(), answered.len(), "{name}");
            let got = super::tls::run(&exe, asked);
            for (n, (g, w)) in got.iter().zip(answered).enumerate() {
                assert_eq!(g, w, "{name} line {n} on {backend}");
            }
            let last = got.last().unwrap();
            assert_eq!(field(last, 1), tag, "{name}");
            // A connection ends closed (4) or failed (5); the identity's and the offer's own refusals
            // are single lines, answered at event 0.
            let configuration = name.starts_with("identity:") || name.ends_with("refused when set");
            let want = if tag == "ok" {
                "4"
            } else if configuration {
                "0"
            } else {
                "5"
            };
            assert_eq!(field(last, 2), want, "{name}: {last}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The engine's rules for client identities and ALPN (`docs/tls-parity.md` §6.2, §6.5, §6.6), recorded by
/// `scripts/tls_tickets_auth.py` against the same server: an identity added, replaced or removed keeps a saved ticket
/// back, the client certificate's notAfter bounds a ticket, the identity is chosen by the host, and a ticket is not bound
/// to the ALPN offer. Each case replays byte for byte.
#[test]
fn the_engine_keeps_a_ticket_back_when_the_identity_changes_and_chooses_it_by_host_on_both_backends()
 {
    let text =
        std::fs::read_to_string(repo_root().join("tests/vectors/tls/tickets_auth.txt")).unwrap();
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
    assert_eq!(cases.len(), 10);
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = super::tls::build_tls_tickets_in("tls-auth-tickets", backend);
        for (name, asked, answered) in &cases {
            let got = super::tls::run(&exe, asked);
            for (n, (g, w)) in got.iter().zip(answered).enumerate() {
                assert_eq!(g, w, "{name} line {n} on {backend}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
