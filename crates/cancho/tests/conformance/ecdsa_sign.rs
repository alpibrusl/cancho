//! `std.ecdsa_sign` and `packages/x509`'s `x509_key` (`docs/ecdsa-sign.md`
//! §5): RFC 6979 Appendix A.2.5's P-256/SHA-256 signatures on both
//! backends, the DER they encode to, the nonce's added randomness, every
//! refusal of the signer with its own tag, and every key file of
//! `tests/vectors/ecdsa_sign/` answered with its tag or its key. All
//! through `tests/programs/ecdsa_sign_driver.cho`.

use super::json::feed;
use super::*;

fn build_sign_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("ecdsa-sign-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/ecdsa_sign_driver.cho"))
        .arg(repo_root().join("packages/x509/x509.cho"))
        .arg(repo_root().join("packages/x509/key.cho"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

fn run_cases(exe: &Path, cases: &[String]) -> Vec<String> {
    let mut input = cases.join("\n");
    input.push('\n');
    let out = feed(exe, input.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let lines: Vec<String> =
        String::from_utf8_lossy(&out.stdout).lines().map(|l| l.trim_end().to_string()).collect();
    assert_eq!(lines.len(), cases.len(), "one answer per case");
    lines
}

/// RFC 6979 A.2.5: the key, its public point, and SHA-256's two cases.
const KEY: &str = "c9afa9d845ba75166b5c215767b1d6934e50c3db36e89b127b8a622b120f6721";
const POINT: &str = "0460fed4ba255a9d31c961eb74c6356d68c049b8923b61fa6ce669622e60f29fb6\
                     7903fe1008b8bc99a41ae9e95628bc64f2f1b20c2d7e9f5177a3c294d4462299";
/// SHA-256 of "sample" and of "test".
const SAMPLE: &str = "af2bdbe1aa9b6ec1e2ade1d694f41fc71a831d0268e9891562113d8a62add1bf";
const TEST: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
const SAMPLE_SIG: &str = "efd48b2aacb6a8fd1140dd9cd45e81d69d2c877b56aaf991c34d0ea84eaf3716\
                          f7cb1c942d657c41d436c7a1b6e29f65f3e900dbb9aff4064dc4ab2f843acda8";
const TEST_SIG: &str = "f1abb023518351cd71d881567b1ea663ed3efcf6c5132b354f28d3b0b7d38367\
                        019f4113742a2b14bd25926b49c649155f267e60d3814b4c0cc84250e46f0083";
const N: &str = "ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551";
const N_MINUS_1: &str = "ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632550";

/// The deterministic nonce (no added randomness) gives RFC 6979's
/// signatures exactly, raw and checked, and their DER; on both backends.
#[test]
fn rfc6979_vectors_on_both_backends() {
    let cases: Vec<String> = vec![
        format!("S {SAMPLE} {KEY} -"),
        format!("S {TEST} {KEY} -"),
        format!("C {SAMPLE} {KEY} {POINT} -"),
        format!("C {TEST} {KEY} {POINT} -"),
        format!("E {SAMPLE_SIG}"),
        format!("E {TEST_SIG}"),
        // "sample" itself, hashed in the driver, signed and checked under
        // the point the key parser's `public_point` derives.
        format!("M {} {KEY} -", hex(b"sample")),
    ];
    let sample_der = format!("3046022100{}022100{}", &SAMPLE_SIG[..64], &SAMPLE_SIG[64..]);
    // s begins 01: no leading 00, and r's top bit is set.
    let test_der = format!("3045022100{}0220{}", &TEST_SIG[..64], &TEST_SIG[64..]);
    let want = [
        format!("0 ok {SAMPLE_SIG}"),
        format!("0 ok {TEST_SIG}"),
        format!("0 ok {SAMPLE_SIG}"),
        format!("0 ok {TEST_SIG}"),
        format!("72 ok {sample_der}"),
        format!("71 ok {test_der}"),
        format!("72 ok {sample_der}"),
    ];
    for backend in ["llvm", "cranelift"] {
        let (dir, exe) = build_sign_driver("rfc6979", backend);
        let got = run_cases(&exe, &cases);
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g, w, "case {i} on {backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// RFC 6979 §3.6's added randomness changes the nonce, so the signature,
/// and each one still verifies under the key's point; the same
/// randomness gives the same signature again.
#[test]
fn added_randomness_changes_the_signature_and_it_still_verifies() {
    let (dir, exe) = build_sign_driver("extra", "llvm");
    let extras = ["00".repeat(32), "01".repeat(32), "ff".repeat(32), "5a".repeat(32)];
    let mut cases: Vec<String> =
        extras.iter().map(|e| format!("C {SAMPLE} {KEY} {POINT} {e}")).collect();
    cases.push(format!("C {SAMPLE} {KEY} {POINT} {}", extras[1]));
    let got = run_cases(&exe, &cases);
    let mut seen = std::collections::BTreeSet::new();
    for g in &got[..4] {
        assert!(g.starts_with("0 ok "), "{g}");
        assert_ne!(g, &format!("0 ok {SAMPLE_SIG}"), "the randomness was used");
        seen.insert(g.clone());
    }
    assert_eq!(seen.len(), 4, "four randomnesses, four nonces");
    assert_eq!(got[4], got[1], "the same randomness, the same signature");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every refusal of `std.ecdsa_sign` the driver can reach, each with its
/// own tag, and the key range's edges: n - 1 signs, 0 and n do not.
#[test]
fn every_signing_refusal_has_its_own_tag() {
    let (dir, exe) = build_sign_driver("refusals", "llvm");
    let zero = "00".repeat(32);
    let other_point = format!(
        "04{}{}",
        "6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296",
        "4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5"
    );
    let cases = [
        (format!("S {} {KEY} -", &SAMPLE[..62]), "-70 ecdsa-sign-digest-length -"),
        (format!("S {SAMPLE} {} -", &KEY[..62]), "-71 ecdsa-sign-key-length -"),
        (format!("S {SAMPLE} {zero} -"), "-72 ecdsa-sign-key-range -"),
        (format!("S {SAMPLE} {N} -"), "-72 ecdsa-sign-key-range -"),
        (format!("S {SAMPLE} {} -", "ff".repeat(32)), "-72 ecdsa-sign-key-range -"),
        (format!("S {SAMPLE} {KEY} {}", "00".repeat(31)), "-73 ecdsa-sign-extra-length -"),
        (format!("E {}", &SAMPLE_SIG[..126]), "-74 ecdsa-sign-output-length -"),
        (format!("F {SAMPLE_SIG} 71"), "-74 ecdsa-sign-output-length -"),
        (format!("W {SAMPLE} {KEY} -"), "-75 ecdsa-sign-work-length -"),
        // The generator's point is not this key's: the check refuses.
        (format!("C {SAMPLE} {KEY} {other_point} -"), "-77 ecdsa-sign-check -"),
        (format!("C {SAMPLE} {KEY} 00 -"), "-77 ecdsa-sign-check -"),
    ];
    let input: Vec<String> = cases.iter().map(|(c, _)| c.clone()).collect();
    for ((case, want), got) in cases.iter().zip(run_cases(&exe, &input)) {
        assert_eq!(&got, want, "{case}");
    }
    // n - 1 is a key, and its point is -G.
    let edge = run_cases(&exe, &[format!("M 00 {N_MINUS_1} -")]);
    assert!(edge[0].starts_with("7"), "n - 1 signs: {}", edge[0]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `tests/vectors/ecdsa_sign/keys.txt` (`scripts/ecdsa_sign_keys.py`):
/// each key file answers the key and point OpenSSL printed for it, or is
/// refused with its own tag; and `certs.txt`'s certificates match the key
/// they were made for and no other. Every tag a file can reach is reached.
#[test]
fn every_key_file_answers_its_key_or_its_tag() {
    let (dir, exe) = build_sign_driver("keys", "llvm");
    let rows = |name: &str| -> Vec<Vec<String>> {
        std::fs::read_to_string(repo_root().join("tests/vectors/ecdsa_sign").join(name))
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| l.split(' ').map(str::to_string).collect())
            .collect()
    };
    let keys = rows("keys.txt");
    let certs = rows("certs.txt");
    assert_eq!(keys.len(), 43);
    assert_eq!(certs.len(), 5);
    let mut cases: Vec<String> = keys.iter().map(|r| format!("K {}", r[2])).collect();
    cases.extend(certs.iter().map(|r| format!("X {} {}", r[2], r[3])));
    let mut tags = std::collections::BTreeSet::new();
    for (row, got) in keys.iter().chain(&certs).zip(run_cases(&exe, &cases)) {
        let words: Vec<&str> = got.split(' ').collect();
        assert_eq!(words[1], row[0], "{}: {got}", row[1]);
        if row[0] == "ok" && row.len() > 3 && row[3] != "-" && keys.contains(row) {
            assert_eq!(words[2], row[3], "{}", row[1]);
        }
        tags.insert(row[0].clone());
    }
    let want: std::collections::BTreeSet<String> = [
        "ok",
        "key-pem",
        "key-encrypted",
        "key-algorithm",
        "key-curve",
        "key-der",
        "key-version",
        "key-length",
        "key-range",
        "key-public-mismatch",
        "key-size",
        "key-certificate",
        "key-certificate-mismatch",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(tags, want);
    let _ = std::fs::remove_dir_all(&dir);
}
