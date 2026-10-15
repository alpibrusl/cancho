// Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
//! `std.rsa_sign`, `x509_rsa_key` and `std.bigmod`'s secret-modulus functions
//! (`docs/rsa-sign.md` §11): the known-answer vectors of NIST and Wycheproof and
//! of our own throwaway keys, the malformed-key corpus with the rule each file
//! must be refused with (written before the parser: `scripts/rsa_sign_keys.py`),
//! every refusal of the signer, the fault gate, blinding, EMSA-PKCS1-v1_5 and
//! the authority report. All through `tests/programs/rsa_sign_driver.cho`.
//!
//! These are the quick gates. The differential against OpenSSL and Python, the
//! mutants, the branch audit and the timing test are scripts
//! (`scripts/rsa_sign_*.py`) whose recorded runs are in `docs/rsa-sign.md`.

use super::json::feed;
use super::*;

fn build_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("rsa-sign-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/rsa_sign_driver.cho"))
        .arg(repo_root().join("packages/x509/x509.cho"))
        .arg(repo_root().join("packages/x509/key.cho"))
        .arg(repo_root().join("packages/x509/rsa_key.cho"))
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

fn hex(b: &[u8]) -> String {
    if b.is_empty() {
        return "-".to_string();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    if s == "-" {
        return Vec::new();
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn root_file(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap()
}

/// Sixteen bytes of "entropy": the signer's blinding input. The signature does
/// not depend on it (RSASSA-PKCS1-v1_5 is deterministic), which is what lets a
/// known answer judge a blinded signer.
const ENTROPY: &str = "000102030405060708090a0b0c0d0e0f";
const ENTROPY_B: &str = "f0e0d0c0b0a090807060504030201000";

fn hash_len(name: &str) -> Option<u32> {
    match name {
        "SHA-256" => Some(32),
        "SHA-384" => Some(48),
        "SHA-512" => Some(64),
        _ => None,
    }
}

/// NIST CAVP `SigGen15_186-3`: two test keys, 60 signatures over SHA-256, -384
/// and -512 (SHA-224's 20 are not run), each from a PEM through the parser,
/// the blinded CRT signer and the check, byte for byte NIST's S; on both
/// backends.
#[test]
fn nist_siggen15_known_answers() {
    let text = root_file("tests/vectors/rsa_sign/nist_siggen15.txt");
    let (mut cases, mut want) = (Vec::new(), Vec::new());
    let mut pem = String::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "K" => pem = f[2].to_string(),
            "M" => {
                cases.push(format!("G {pem} {} {} {ENTROPY}", f[1], f[2]));
                want.push(format!("0 ok {}", f[3]));
            }
            _ => unreachable!("{line}"),
        }
    }
    assert_eq!(cases.len(), 60);
    for backend in ["llvm", "cranelift"] {
        let (dir, exe) = build_driver("nist", backend);
        for (i, (got, w)) in run_cases(&exe, &cases).iter().zip(&want).enumerate() {
            assert_eq!(got, w, "NIST case {i} on {backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Wycheproof's `RsassaPkcs1Generate` files: every valid case of 2,048, 3,072
/// and 4,096 bits with a supported hash is reproduced byte for byte from the
/// file's PKCS#1 PEM (72 cases; the 5 `SmallPublicKey` ones use unbalanced primes
/// and are refused as `rsa-key-prime`); the PKCS#8 form of the same key parses to the same block;
/// the 1,024- and 1,536-bit keys are refused for their size. Cases over SHA-1
/// and SHA-224 are counted.
#[test]
fn wycheproof_signature_generation() {
    let (dir, exe) = build_driver("wycheproof", "llvm");
    let (mut cases, mut want) = (Vec::new(), Vec::new());
    let (mut unsupported, mut acceptable, mut groups) = (0, 0, 0);
    let mut twins = Vec::new();
    for bits in [2048, 3072, 4096] {
        let file = format!("tests/vectors/wycheproof/rsa_pkcs1_{bits}_sig_gen_test.json");
        let v: serde_json::Value = serde_json::from_str(&root_file(&file)).unwrap();
        for g in v["testGroups"].as_array().unwrap() {
            groups += 1;
            let pem = hex(g["privateKeyPem"].as_str().unwrap().as_bytes());
            twins.push(format!("P {pem}"));
            twins.push(format!("B 8 {}", g["privateKeyPkcs8"].as_str().unwrap()));
            let Some(h) = hash_len(g["sha"].as_str().unwrap()) else {
                unsupported += g["tests"].as_array().unwrap().len();
                continue;
            };
            for t in g["tests"].as_array().unwrap() {
                let result = t["result"].as_str().unwrap();
                let msg = hex(&unhex(t["msg"].as_str().unwrap_or("")));
                cases.push(format!(
                    "G {pem} {h} {} {ENTROPY}",
                    if msg.is_empty() { "-" } else { &msg }
                ));
                if result == "acceptable" {
                    // Wycheproof's `SmallPublicKey` groups: e = 3 with primes of 1,364
                    // and 684 bits (or 2,047 and 1,025): outside the balanced-prime
                    // policy of `docs/rsa-sign.md` §3.2, so refused, not signed.
                    acceptable += 1;
                    want.push("-153 rsa-key-prime -".to_string());
                } else {
                    want.push(format!("0 ok {}", t["sig"].as_str().unwrap()));
                }
            }
        }
    }
    let got = run_cases(&exe, &cases);
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "Wycheproof case {i}");
    }
    // The PKCS#8 and PKCS#1 forms of one key are the same block (or the same
    // refusal: 5 of the 16 groups have unbalanced primes).
    let blocks = run_cases(&exe, &twins);
    let mut refused = 0;
    for pair in blocks.chunks(2) {
        assert_eq!(pair[0].split_once(' ').unwrap().1, pair[1].split_once(' ').unwrap().1);
        if pair[0] == "-153 rsa-key-prime -" {
            refused += 1;
        } else {
            assert!(pair[0].contains(" ok ") && !pair[0].starts_with('-'), "{}", pair[0]);
        }
    }
    assert_eq!(refused, 5);
    assert_eq!(
        (groups, cases.len(), unsupported, acceptable),
        (16, 77, 16, 5),
        "the files changed"
    );
    for bits in [1024, 1536] {
        let file = format!("tests/vectors/wycheproof/rsa_pkcs1_{bits}_sig_gen_test.json");
        let v: serde_json::Value = serde_json::from_str(&root_file(&file)).unwrap();
        for g in v["testGroups"].as_array().unwrap() {
            let pem = hex(g["privateKeyPem"].as_str().unwrap().as_bytes());
            let got = run_cases(&exe, &[format!("P {pem}")]);
            assert!(got[0].ends_with(" rsa-key-size -"), "{bits}: {}", got[0]);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Our own throwaway keys (`tests/vectors/rsa_sign/valid.txt`), signatures made by
/// the Python reference: 2,048 to 4,096 bits, e = 3, 17 and 65537, a modulus of
/// 2,176 bits, p above and below q, primes 1.1 and 1.9 times 2^(h-1) apart;
/// on both backends. The key goes in as the block, not through the parser.
#[test]
fn our_own_known_answers() {
    let text = root_file("tests/vectors/rsa_sign/valid.txt");
    let mut blocks = std::collections::BTreeMap::new();
    let mut want = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "K" => {
                blocks.insert(f[1].to_string(), f[4].to_string());
            }
            "M" => want.push(format!("0 ok {}", f[4])),
            _ => unreachable!("{line}"),
        }
    }
    // The driver hashes the message (`M`) and signs the digest under the block.
    let cases: Vec<String> = text
        .lines()
        .filter(|l| l.starts_with("M "))
        .map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            format!("M {} {} {} {ENTROPY}", f[2], blocks[f[1]], f[3])
        })
        .collect();
    assert_eq!(cases.len(), 24);
    for backend in ["llvm", "cranelift"] {
        let (dir, exe) = build_driver("valid", backend);
        for (i, (got, w)) in run_cases(&exe, &cases).iter().zip(&want).enumerate() {
            assert_eq!(got, w, "case {i} on {backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const CORPUS_MSG: &[u8] = b"rsa-sign corpus";

/// The malformed-key corpus: every file answers the rule that
/// `scripts/rsa_sign_keys.py` wrote down for it before the parser existed (no
/// oracle answers for these: OpenSSL accepts most of them), and every file that
/// parses signs, or is refused at signing, as the corpus says; on both backends.
#[test]
fn the_malformed_key_corpus_is_refused_with_its_own_rules() {
    let text = root_file("tests/vectors/rsa_sign/malformed.txt");
    let rows: Vec<Vec<&str>> =
        text.lines().filter(|l| !l.starts_with('#')).map(|l| l.split(' ').collect()).collect();
    assert_eq!(rows.len(), 105, "the corpus changed");
    for backend in ["llvm", "cranelift"] {
        let (dir, exe) = build_driver("malformed", backend);
        let parse: Vec<String> = rows.iter().map(|r| format!("P {}", r[4])).collect();
        let got = run_cases(&exe, &parse);
        let mut refused = std::collections::BTreeMap::new();
        for (r, g) in rows.iter().zip(&got) {
            let f: Vec<&str> = g.split(' ').collect();
            if r[0] == "ok" {
                assert!(f[0].parse::<i64>().unwrap() > 0 && f[1] == "ok", "{}: {g}", r[2]);
            } else {
                assert!(
                    f[0].parse::<i64>().unwrap() < 0 && f[1] == r[0],
                    "{}: wanted {} got {}",
                    r[2],
                    r[0],
                    g
                );
                *refused.entry(r[0].to_string()).or_insert(0) += 1;
            }
        }
        // Every rule of docs/rsa-sign.md §6 that a file can reach has a fixture.
        let tags: Vec<&String> = refused.keys().collect();
        assert_eq!(
            tags,
            [
                "rsa-key-algorithm",
                "rsa-key-block-size",
                "rsa-key-crt-coefficient",
                "rsa-key-crt-exponent",
                "rsa-key-der",
                "rsa-key-encrypted",
                "rsa-key-exponent",
                "rsa-key-modulus",
                "rsa-key-multiprime",
                "rsa-key-parameters",
                "rsa-key-pem",
                "rsa-key-prime",
                "rsa-key-private-exponent",
                "rsa-key-product",
                "rsa-key-size",
                "rsa-key-version",
            ]
        );
        // Signing what parses.
        let msg = hex(CORPUS_MSG);
        let signed: Vec<(&Vec<&str>, String)> = rows
            .iter()
            .filter(|r| r[0] == "ok")
            .map(|r| (r, format!("G {} 32 {msg} {ENTROPY}", r[4])))
            .collect();
        let lines: Vec<String> = signed.iter().map(|(_, c)| c.clone()).collect();
        for ((r, _), g) in signed.iter().zip(run_cases(&exe, &lines)) {
            if r[1] == "ok" {
                assert_eq!(g, format!("0 ok {}", r[3]), "{} on {backend}", r[2]);
            } else {
                assert!(g.starts_with('-') && g.contains(&format!(" {} ", r[1])), "{}: {g}", r[2]);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---- the signer's own refusals, the fault gate, blinding and the encoding ----

/// The two refusals of `x509_rsa_key` that are about the caller's buffers.
#[test]
fn the_key_reader_refuses_buffers_that_are_too_short() {
    let (dir, exe) = build_driver("buffers", "llvm");
    let text = root_file("tests/vectors/rsa_sign/malformed.txt");
    let pem =
        text.lines().find(|l| l.contains(" valid-pkcs1 ")).unwrap().rsplit(' ').next().unwrap();
    let got = run_cases(
        &exe,
        &[
            format!("N {pem} 1800"),
            format!("N {pem} 1799"),
            format!("N {pem} 0"),
            format!("Q {pem}"),
        ],
    );
    assert!(got[0].starts_with("904 ok "), "{}", got[0]);
    assert_eq!(got[1], "-177 rsa-key-buffer-length -");
    assert_eq!(got[2], "-177 rsa-key-buffer-length -");
    assert_eq!(got[3], "-177 rsa-key-buffer-length -");
    // A format that is neither PKCS#1 nor PKCS#8.
    assert_eq!(run_cases(&exe, &["B 7 3000".to_string()])[0], "-172 rsa-key-algorithm -");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A valid 2,048-bit block (h = 128): n 0..256, e 256..264, p 264..392, q 392..520,
/// dp 520..648, dq 648..776, qinv 776..904.
fn first_block() -> Vec<u8> {
    let text = root_file("tests/vectors/rsa_sign/valid.txt");
    let line = text.lines().find(|l| l.starts_with("K k2048_e65537_0 ")).unwrap();
    unhex(line.split(' ').nth(4).unwrap())
}

fn digest32() -> String {
    hex(&[0x5a; 32])
}

fn tag_of(line: &str) -> (i64, &str) {
    let f: Vec<&str> = line.split(' ').collect();
    (f[0].parse().unwrap(), f[1])
}

/// Every refusal of `std.rsa_sign` the driver can reach, each with its own tag
/// and code, and the order they are given in.
#[test]
fn every_signing_refusal_has_its_own_tag() {
    let (dir, exe) = build_driver("refusals", "llvm");
    let block = first_block();
    assert_eq!(block.len(), 904);
    let key = hex(&block);
    let d = digest32();
    let mut cases = vec![
        format!("S 32 {key} {d} {ENTROPY}"),
        format!("S 33 {key} {d} {ENTROPY}"),
        format!("S 32 {key} {} {ENTROPY}", hex(&[1u8; 31])),
        format!("S 32 {} {d} {ENTROPY}", hex(&block[..903])),
        format!("S 32 {} {d} {ENTROPY}", hex(&[block.clone(), vec![0]].concat())),
        // h = 120 (not a multiple of 8), h = 64 (a 1,024-bit key), h = 264 (a 4,224-bit one).
        format!("S 32 {} {d} {ENTROPY}", hex(&vec![0x80u8; 7 * 120 + 8])),
        format!("S 32 {} {d} {ENTROPY}", hex(&vec![0x80u8; 7 * 64 + 8])),
        format!("S 32 {} {d} {ENTROPY}", hex(&vec![0x80u8; 7 * 264 + 8])),
        // h = 132: in range, but not a multiple of 8 (a 2,112-bit modulus).
        format!("S 32 {} {d} {ENTROPY}", hex(&vec![0x80u8; 7 * 132 + 8])),
        format!("S 32 - {d} {ENTROPY}"),
        format!("S 32 {key} {d} {}", hex(&[7u8; 15])),
        format!("S 32 {key} {d} {}", hex(&[7u8; 17])),
        format!("S 32 {key} {d} -"),
        format!("O 32 {key} {d} {ENTROPY} 255"),
        format!("O 32 {key} {d} {ENTROPY} 257"),
        format!("W 32 {key} {d} {ENTROPY}"),
    ];
    let mut want: Vec<(i64, &str)> = vec![
        (0, "ok"),
        (-140, "rsa-sign-hash"),
        (-141, "rsa-sign-digest-length"),
        (-142, "rsa-sign-key-layout"),
        (-142, "rsa-sign-key-layout"),
        (-142, "rsa-sign-key-layout"),
        (-142, "rsa-sign-key-layout"),
        (-142, "rsa-sign-key-layout"),
        (-142, "rsa-sign-key-layout"),
        (-142, "rsa-sign-key-layout"),
        (-143, "rsa-sign-entropy"),
        (-143, "rsa-sign-entropy"),
        (-143, "rsa-sign-entropy"),
        (-144, "rsa-sign-output-length"),
        (-144, "rsa-sign-output-length"),
        (-145, "rsa-sign-work-length"),
    ];
    // A key outside what the signer reads: a prime without its top bit, an even
    // prime, an even or short modulus.
    let edits: [(usize, u8, i64, &str); 7] = [
        (264, 0x7f, -146, "rsa-sign-key-shape"),
        (392, 0x7f, -146, "rsa-sign-key-shape"),
        (263 + 128, 0xfe, -146, "rsa-sign-key-shape"),
        (391 + 128, 0xfe, -146, "rsa-sign-key-shape"),
        (0, 0x7f, -146, "rsa-sign-key-shape"),
        (255, 0xfe, -147, "rsa-sign-check"),
        (900, 0x00, -147, "rsa-sign-check"),
    ];
    for (at, mask, code, tag) in edits {
        let mut b = block.clone();
        if at == 900 {
            b[at] ^= 1; // qinv wrong: the check refuses
        } else if at == 255 {
            b[at] &= mask; // n even: `std.rsa`'s verification refuses it, so the check does
        } else {
            b[at] &= mask;
        }
        cases.push(format!("S 32 {} {d} {ENTROPY}", hex(&b)));
        want.push((code, tag));
    }
    // A wrong key, each field, caught by the check at the end.
    for (at, _name) in [(100, "n"), (300, "p"), (420, "q"), (560, "dp"), (700, "dq")] {
        let mut b = block.clone();
        b[at] ^= 1;
        cases.push(format!("S 32 {} {d} {ENTROPY}", hex(&b)));
        want.push((-147, "rsa-sign-check"));
    }
    let got = run_cases(&exe, &cases);
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(tag_of(g), *w, "case {i}: {g}");
        assert_eq!(g.ends_with(" -"), w.0 < 0, "a refusal gives no signature: case {i}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The fault gate (`docs/rsa-sign.md` §9, G4): a bit flipped in either CRT half,
/// in the recombined signature or in `qinv` is refused by the check and no
/// signature comes out; the same fault with the check switched off gives a
/// signature that `std.rsa` refuses (the Python gate shows the prime factor in
/// it). Unblinded and unchecked, nothing faulted, the signature is the same.
#[test]
fn a_fault_is_caught_and_its_signature_is_not_given_out() {
    let (dir, exe) = build_driver("fault", "llvm");
    let key = hex(&first_block());
    let d = digest32();
    let b = first_block();
    let n = hex(&b[..256]);
    let e = hex(&b[256..264]);
    let (unchecked, faults) = (1, [2, 4, 8, 32]);
    let mut cases = vec![format!("S 32 {key} {d} {ENTROPY}")];
    for f in faults {
        cases.push(format!("U 32 {key} {d} {ENTROPY} {f}"));
        cases.push(format!("U 32 {key} {d} {ENTROPY} {}", f | unchecked));
    }
    cases.push(format!("U 32 {key} {d} {ENTROPY} {unchecked}"));
    cases.push(format!("U 32 {key} {d} - 16"));
    cases.push(format!("U 32 {key} {d} - {}", 16 | unchecked));
    let got = run_cases(&exe, &cases);
    let good = got[0].strip_prefix("0 ok ").expect("the unfaulted key signs").to_string();
    let mut faulty = Vec::new();
    for (i, f) in faults.iter().enumerate() {
        assert_eq!(got[1 + 2 * i], "-147 rsa-sign-check -", "fault {f} with the check");
        let line = &got[2 + 2 * i];
        let sig =
            line.strip_prefix("0 ok ").unwrap_or_else(|| panic!("fault {f} unchecked: {line}"));
        assert_ne!(sig, good, "fault {f} changed the signature");
        faulty.push(format!("D 32 {n} {e} {d} {sig}"));
    }
    assert_eq!(got[9], format!("0 ok {good}"), "unchecked, no fault");
    assert_eq!(got[10], format!("0 ok {good}"), "unblinded");
    assert_eq!(got[11], format!("0 ok {good}"), "unblinded and unchecked");
    for g in run_cases(&exe, &faulty) {
        assert!(
            g.starts_with("-1") && !g.starts_with("0"),
            "std.rsa accepted a faulty signature: {g}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Blinding (`docs/rsa-sign.md` §8): the signature does not depend on the
/// entropy; the exponent the p-half is raised to does, and with no blinding it
/// is dp itself.
#[test]
fn blinding_changes_the_exponent_and_not_the_signature() {
    let (dir, exe) = build_driver("blinding", "llvm");
    let b = first_block();
    let key = hex(&b);
    let d = digest32();
    let dp = hex(&b[520..648]);
    let cases = vec![
        format!("S 32 {key} {d} {ENTROPY}"),
        format!("S 32 {key} {d} {ENTROPY_B}"),
        format!("S 32 {key} {d} {}", "00".repeat(16)),
        format!("U 32 {key} {d} {ENTROPY} 64"),
        format!("U 32 {key} {d} {ENTROPY_B} 64"),
        format!("U 32 {key} {d} {ENTROPY} 64"),
        format!("U 32 {key} {d} - {}", 64 | 16),
    ];
    let got = run_cases(&exe, &cases);
    assert_eq!(got[0], got[1]);
    assert_eq!(got[0], got[2]);
    assert!(got[0].starts_with("0 ok "));
    let ex: Vec<&str> = got[3..].iter().map(|g| g.strip_prefix("0 ok ").expect(g)).collect();
    assert_ne!(ex[0], ex[1], "two entropies, two exponents");
    assert_eq!(ex[0], ex[2], "the same entropy, the same exponent");
    assert_ne!(ex[0], dp);
    // h + 16 bytes of exponent (k + 2 limbs), then zeros to the signature's length.
    let unblinded = ex[3];
    assert_eq!(&unblinded[..2 * 16], &"00".repeat(16), "no blinding: the high limbs are 0");
    assert_eq!(&unblinded[2 * 16..2 * 16 + 256], dp, "no blinding: the exponent is dp");
    let _ = std::fs::remove_dir_all(&dir);
}

/// EMSA-PKCS1-v1_5 (RFC 8017 §9.2): 00 01 FF..FF 00 || DigestInfo || digest.
#[test]
fn emsa_pkcs1_encoding() {
    let (dir, exe) = build_driver("emsa", "llvm");
    let prefixes: [(u32, &str); 3] = [
        (32, "3031300d060960864801650304020105000420"),
        (48, "3041300d060960864801650304020205000430"),
        (64, "3051300d060960864801650304020305000440"),
    ];
    let mut cases = Vec::new();
    let mut want = Vec::new();
    for (h, prefix) in prefixes {
        for len in [256usize, 384, 512] {
            let digest: Vec<u8> = (0..h as u8).map(|i| i.wrapping_mul(7).wrapping_add(3)).collect();
            let t = unhex(prefix);
            let mut em = vec![0u8, 1];
            em.extend(std::iter::repeat_n(0xffu8, len - t.len() - digest.len() - 3));
            em.push(0);
            em.extend(&t);
            em.extend(&digest);
            assert_eq!(em.len(), len);
            cases.push(format!("E {h} {} {len}", hex(&digest)));
            want.push(format!("0 ok {}", hex(&em)));
        }
    }
    cases.push(format!("E 20 {} 256", hex(&[0u8; 20])));
    want.push("-140 rsa-sign-hash -".to_string());
    cases.push(format!("E 32 {} 256", hex(&[0u8; 31])));
    want.push("-141 rsa-sign-digest-length -".to_string());
    cases.push(format!("E 32 {} 61", hex(&[0u8; 32])));
    want.push("-144 rsa-sign-output-length -".to_string());
    for (g, w) in run_cases(&exe, &cases).iter().zip(&want) {
        assert_eq!(g, w);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `std.bigmod`'s secret-modulus functions on moduli of 64 bits, where `u128`
/// is the reference: `setup_secret` and `pow_secret` against a^e mod n, the
/// refusals of `setup_secret`, `reduce_once` and `blind_exponent`. (The Python
/// differential does 20,000 more, up to 4,096 bits.)
#[test]
fn bigmod_secret_functions_on_64_bit_moduli() {
    fn pow(a: u128, e: u128, n: u128) -> u128 {
        let (mut r, mut b, mut e) = (1u128, a % n, e);
        while e > 0 {
            if e & 1 == 1 {
                r = r * b % n;
            }
            b = b * b % n;
            e >>= 1;
        }
        r
    }
    let (dir, exe) = build_driver("bigmod", "llvm");
    let mut cases = Vec::new();
    let mut want = Vec::new();
    let mut x: u64 = 0x9e3779b97f4a7c15;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..200 {
        let n = (next() | 1 << 63 | 1) as u128;
        let (a, e) = ((next() as u128) % n, next() as u128);
        cases.push(format!("Z {n:016x} {e:016x} {a:016x}"));
        want.push(format!("0 ok {:016x}", pow(a, e, n)));
        let a2 = (next() as u128 + next() as u128) % (2 * n);
        cases.push(format!("R {n:016x} {:016x}", a2 % (1u128 << 64)));
        want.push(format!("0 ok {:016x}", (a2 % (1u128 << 64)) % n));
    }
    // Refusals: an even modulus, a top bit clear, a length that is not a multiple of 8.
    cases.push("Z fffffffffffffffe 03 02".to_string());
    want.push("-1 bigmod-even-modulus -".to_string());
    cases.push("Z 7fffffffffffffff 03 02".to_string());
    want.push("-2 bigmod-modulus-size -".to_string());
    cases.push("Z ffffffffffffffffff 03 02".to_string());
    want.push("-2 bigmod-modulus-size -".to_string());
    for (g, w) in run_cases(&exe, &cases).iter().zip(&want) {
        assert_eq!(g, w);
    }
    // d + kappa (n - 1), for n = 2^64 - 59: 2^64 - 60 = 0xffffffffffffffc4.
    let got =
        run_cases(&exe, &["Y ffffffffffffffc5 0000000000000007 8000000000000001".to_string()]);
    let n1 = 0xffffffffffffffc4u128;
    let v = 7u128 + 0x8000000000000001u128 * n1;
    assert_eq!(
        got[0],
        format!(
            "0 ok {}",
            hex(&{
                let mut b = vec![0u8; 24];
                for (i, byte) in b.iter_mut().rev().enumerate() {
                    *byte = if i < 16 { (v >> (8 * i)) as u8 } else { 0 };
                }
                b
            })
        )
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `cancho authority` on the driver (`docs/rsa-sign.md` §5, G8): the signer, the
/// key reader and the arithmetic reach the console and the heap the driver itself
/// uses, and nothing else: no filesystem, no network, no foreign code.
#[test]
fn the_signer_reaches_no_authority() {
    let out = Command::new(BIN)
        .args(["authority", "--std", "--output", "json"])
        .arg(repo_root().join("tests/programs/rsa_sign_driver.cho"))
        .arg(repo_root().join("packages/x509/x509.cho"))
        .arg(repo_root().join("packages/x509/key.cho"))
        .arg(repo_root().join("packages/x509/rsa_key.cho"))
        .output()
        .expect("the compiler runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["bounded"], true, "{v}");
    assert_eq!(v["effects"], serde_json::json!(["heap", "io_read", "io_write"]), "{v}");
    assert_eq!(v["foreign_symbols"], serde_json::json!([]), "{v}");
    assert_eq!(v["unbounded_by"], serde_json::json!([]), "{v}");
    for what in [
        "the filesystem",
        "the network",
        "the command line",
        "signals",
        "other programs",
        "foreign code",
    ] {
        assert!(v["never_touches"].as_array().unwrap().iter().any(|w| w == what), "{what}: {v}");
    }
}
