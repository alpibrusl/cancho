//! `packages/tls`'s server with client certificates and no network (`docs/tls-server.md` §13): the
//! connections of `scripts/tls_liar_client_auth.py` replayed byte for byte on both backends, each ending with
//! its tag through `tests/programs/tls_server_driver.cho`. Every rule of §13.2 to §13.4 and every tag of
//! §13.7 is reached; every honest case asks the server for the client's identity (`P`) and the recording
//! holds the answer, which the script compared with what pyca reads from the same certificate. The honest
//! connections again with the client's bytes fed one byte a line.

use super::tls_server::{Case, build_server_driver, cases_in, field, hex, run};

const RECORDING: &str = "tests/vectors/tls/liar_client_auth.txt";

/// Each case replays byte for byte and ends with its tag: a refused connection failed (event 5); an honest
/// one, or one of the configuration, answers `ok` on every line.
#[test]
fn every_client_certificate_case_replays_with_its_own_tag_on_both_backends() {
    let cases = cases_in(RECORDING);
    assert_eq!(cases.len(), 77);
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_server_driver("client-auth", backend);
        for (tag, name, asked, answered) in &cases {
            assert_eq!(asked.len(), answered.len(), "{name}");
            let got = run(&exe, asked);
            for (n, (g, w)) in got.iter().zip(answered).enumerate() {
                assert_eq!(g, w, "{name} line {n} on {backend}");
            }
            let last_op = asked.last().unwrap().split(' ').next().unwrap();
            let last = got.last().unwrap();
            if ["F", "Z"].contains(&last_op) && tag != "ok" {
                assert_eq!(field(last, 1), tag, "{name}");
                assert_eq!(field(last, 2), "5", "{name}: {last}");
            } else if tag != "ok" {
                assert!(got.iter().any(|a| field(a, 1) == tag), "{name}: {tag} on some line");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Every tag of §13.7 and every refusal of the client's flight is in the recording, so a rule cannot be
/// removed without a case failing.
#[test]
fn the_recording_reaches_every_refusal_of_the_clients_flight() {
    let cases = cases_in(RECORDING);
    let tags: std::collections::BTreeSet<&str> =
        cases.iter().map(|(tag, ..)| tag.as_str()).collect();
    for want in [
        "ok",
        "tls-server-client-cert-required",
        "tls-server-client-sigalg",
        "tls-server-client-store",
        "tls-server-client-auth-config",
        "tls-unexpected-message",
        "tls-server-illegal-parameter",
        "tls-unsupported-extension",
        "tls-decode-error",
        "tls-bad-certificate-verify",
        "tls-server-finished",
        "x509-decode",
        "x509-chain-too-large",
        "x509-path-too-long",
        "x509-unsupported-algorithm",
        "x509-unknown-issuer",
        "x509-not-ca",
        "x509-expired",
        "x509-not-yet-valid",
        "x509-key-usage",
        "x509-name-constraint",
    ] {
        assert!(tags.contains(want), "no case ends with {want}");
    }
}

/// The honest connections with the client's bytes fed one byte a line: the Certificate, the
/// CertificateVerify and the Finished reassembled from one-byte records of input. The server must send
/// exactly what it sent before, receive the same data and give the same identity.
#[test]
fn the_clients_flight_in_any_split_gives_the_same_connection() {
    let (dir, exe) = build_server_driver("client-auth-splits", "llvm");
    let honest: Vec<Case> = cases_in(RECORDING)
        .into_iter()
        .filter(|(tag, name, asked, _)| {
            tag == "ok" && name.starts_with("honest: required") && asked.iter().any(|l| l == "P")
        })
        .collect();
    assert!(honest.len() >= 10, "{} honest connections", honest.len());
    for (_, name, asked, answered) in &honest {
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
        let total = |answers: &[String], asked: &[String], n: usize| {
            answers
                .iter()
                .zip(asked)
                .filter(|(_, q)| ["V", "F", "W", "Q", "Z"].iter().any(|op| q.starts_with(op)))
                .map(|(a, _)| hex(field(a, n)))
                .collect::<String>()
        };
        assert_eq!(total(&got, &bytewise, 3), total(answered, asked, 3), "{name}: sent");
        assert_eq!(total(&got, &bytewise, 4), total(answered, asked, 4), "{name}: received");
        let identity = |answers: &[String], asked: &[String]| {
            answers.iter().zip(asked).find(|(_, q)| *q == "P").map(|(a, _)| a.clone())
        };
        assert_eq!(identity(&got, &bytewise), identity(answered, asked), "{name}: the identity");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
