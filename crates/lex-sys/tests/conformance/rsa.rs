//! `std.bigmod` and `std.rsa` (`docs/rsa.md` §4): every Wycheproof RSA
//! PKCS#1 v1.5 and PSS case that uses SHA-256, SHA-384 or SHA-512, the
//! NIST CAVP SigVer files, and every refusal with its own tag. All
//! through `tests/programs/rsa_driver.ls`.

use super::json::feed;
use super::*;

fn build_rsa_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("rsa-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/rsa_driver.ls"))
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
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let lines: Vec<String> = text.lines().map(|l| l.trim_end().to_string()).collect();
    assert_eq!(lines.len(), cases.len(), "one answer per case");
    lines
}

fn tag(line: &str) -> &str {
    line.split(' ').nth(1).unwrap_or("")
}

fn hash_len(name: &str) -> Option<u32> {
    match name {
        "SHA-256" | "SHA256" => Some(32),
        "SHA-384" | "SHA384" => Some(48),
        "SHA-512" | "SHA512" => Some(64),
        _ => None,
    }
}

fn or_dash(hex: &str) -> &str {
    if hex.is_empty() { "-" } else { hex }
}

const WYCHEPROOF: [&str; 17] = [
    "rsa_signature_2048_sha256_test.json",
    "rsa_signature_2048_sha384_test.json",
    "rsa_signature_2048_sha512_test.json",
    "rsa_signature_3072_sha256_test.json",
    "rsa_signature_3072_sha384_test.json",
    "rsa_signature_3072_sha512_test.json",
    "rsa_signature_4096_sha256_test.json",
    "rsa_signature_4096_sha384_test.json",
    "rsa_signature_4096_sha512_test.json",
    "rsa_pss_2048_sha256_mgf1_0_test.json",
    "rsa_pss_2048_sha256_mgf1_32_test.json",
    "rsa_pss_2048_sha384_mgf1_48_test.json",
    "rsa_pss_3072_sha256_mgf1_32_test.json",
    "rsa_pss_4096_sha256_mgf1_32_test.json",
    "rsa_pss_4096_sha384_mgf1_48_test.json",
    "rsa_pss_4096_sha512_mgf1_64_test.json",
    "rsa_pss_misc_test.json",
];

/// Every valid case verifies, every invalid one is refused, and the
/// "acceptable" ones (a DigestInfo without its NULL, RFC 8017 §9.2 note
/// 2) are refused as `docs/rsa.md` §3.2 decides. Cases over SHA-1 or
/// SHA-224 are counted, not run.
#[test]
fn every_wycheproof_rsa_case_passes() {
    let mut cases = Vec::new();
    let mut want = Vec::new();
    let mut unsupported = 0;
    for file in WYCHEPROOF {
        let path = repo_root().join("tests/vectors/wycheproof").join(file);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for group in v["testGroups"].as_array().unwrap() {
            let n = group["publicKey"]["modulus"].as_str().unwrap();
            let e = group["publicKey"]["publicExponent"].as_str().unwrap();
            let h = hash_len(group["sha"].as_str().unwrap());
            let pss = group["type"] == "RsassaPssVerify";
            let mgf = if pss { hash_len(group["mgfSha"].as_str().unwrap()) } else { h };
            for t in group["tests"].as_array().unwrap() {
                let (Some(h), Some(mgf)) = (h, mgf) else {
                    unsupported += 1;
                    continue;
                };
                let msg = or_dash(t["msg"].as_str().unwrap());
                let sig = or_dash(t["sig"].as_str().unwrap());
                cases.push(if pss {
                    format!("S {h} {mgf} {} {n} {e} {msg} {sig}", group["sLen"])
                } else {
                    format!("P {h} {n} {e} {msg} {sig}")
                });
                want.push((
                    file,
                    t["tcId"].as_u64().unwrap(),
                    t["result"].as_str().unwrap().to_string(),
                ));
            }
        }
    }
    assert_eq!(unsupported, 96, "rsa_pss_misc's SHA-1 and SHA-224 groups");
    let (dir, exe) = build_rsa_driver("wycheproof", "llvm");
    let mut counts = std::collections::BTreeMap::new();
    for ((file, id, result), line) in want.iter().zip(run_cases(&exe, &cases)) {
        match result.as_str() {
            "valid" => assert_eq!(tag(&line), "ok", "{file} tcId {id}: {line}"),
            "invalid" => assert!(line.starts_with('-'), "{file} tcId {id}: {line}"),
            _ => assert_eq!(tag(&line), "rsa-pkcs1-mismatch", "{file} tcId {id}: {line}"),
        }
        *counts.entry(format!("{result} {}", tag(&line))).or_insert(0) += 1;
    }
    let counts: Vec<String> = counts.iter().map(|(k, v)| format!("{k} {v}")).collect();
    assert_eq!(
        counts,
        [
            "acceptable rsa-pkcs1-mismatch 9",
            "invalid rsa-pkcs1-mismatch 2201",
            "invalid rsa-pss-mismatch 112",
            "invalid rsa-pss-padding 106",
            "invalid rsa-pss-top-bits 2",
            "invalid rsa-pss-trailer 54",
            "invalid rsa-signature-length 53",
            "invalid rsa-signature-range 41",
            "valid ok 694",
        ]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// FIPS 186-4's SigVer files for SHA-256/384/512: every case of 2,048 bits
/// and up gives the file's result, and every 1,024- and 1,536-bit one is
/// refused for its size.
#[test]
fn every_nist_sigver_case_passes() {
    let (dir, exe) = build_rsa_driver("nist", "cranelift");
    for (file, pss) in [("SigVer15_186-3.rsp", false), ("SigVerPSS_186-3.rsp", true)] {
        let text =
            std::fs::read_to_string(repo_root().join("tests/vectors/cavp").join(file)).unwrap();
        let (mut cases, mut want) = (Vec::new(), Vec::new());
        let (mut modulus, mut n, mut h, mut e, mut msg, mut sig, mut salt) =
            (0, String::new(), 0, String::new(), String::new(), String::new(), String::new());
        for line in text.lines() {
            if let Some(m) = line.strip_prefix("[mod = ") {
                modulus = m.trim_end_matches(']').parse().unwrap();
                continue;
            }
            let Some((k, v)) = line.split_once(" = ") else { continue };
            let v = v.trim().to_string();
            match k {
                "n" => n = v,
                "SHAAlg" => h = hash_len(&v).expect("only SHA-256/384/512 are kept"),
                "e" => e = if v.len() % 2 == 1 { format!("0{v}") } else { v },
                "Msg" => msg = v,
                "S" => sig = v,
                "SaltVal" => salt = v,
                "Result" => {
                    cases.push(if pss {
                        let salt_len = if salt == "00" { 0 } else { salt.len() / 2 };
                        format!("S {h} {h} {salt_len} {n} {e} {msg} {sig}")
                    } else {
                        format!("P {h} {n} {e} {msg} {sig}")
                    });
                    want.push((modulus, v.starts_with('P')));
                }
                _ => {}
            }
        }
        assert_eq!(cases.len(), 270, "{file}");
        let (mut pass, mut fail, mut small) = (0, 0, 0);
        for ((modulus, ok), line) in want.iter().zip(run_cases(&exe, &cases)) {
            if *modulus < 2048 {
                assert_eq!(tag(&line), "rsa-modulus-size", "{file} {modulus}: {line}");
                small += 1;
            } else if *ok {
                assert_eq!(tag(&line), "ok", "{file} {modulus}: {line}");
                pass += 1;
            } else {
                assert!(line.starts_with('-'), "{file} {modulus}: {line}");
                fail += 1;
            }
        }
        assert_eq!((pass, fail, small), (27, 135, 108), "{file}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every refusal `std.bigmod` and `std.rsa` have, reached with its own
/// tag. The four PSS refusals that need a crafted signature
/// (`rsa-pss-trailer`, `-top-bits`, `-padding`, `-mismatch`) are reached
/// by Wycheproof's cases, counted in the first test.
#[test]
fn every_rsa_refusal_is_reached_with_its_own_tag() {
    let path = repo_root().join("tests/vectors/wycheproof/rsa_signature_2048_sha256_test.json");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let group = &v["testGroups"][0];
    let n = group["publicKey"]["modulus"].as_str().unwrap().to_string();
    let e = group["publicKey"]["publicExponent"].as_str().unwrap().to_string();
    let good = group["tests"].as_array().unwrap().iter().find(|t| t["result"] == "valid").unwrap();
    let msg = good["msg"].as_str().unwrap();
    let sig = good["sig"].as_str().unwrap();
    // The modulus without Wycheproof's leading 00, an even one, a short one,
    // and one past 4,096 bits.
    let bare = n.trim_start_matches("00");
    let even = format!("{}{}", &bare[..bare.len() - 1], "0");
    let short = &bare[..256];
    let long = format!("01{}", "00".repeat(512)) + "01";
    let cases: Vec<(String, &str)> = vec![
        ("M 10 03 01".into(), "bigmod-even-modulus"),
        ("M 01 03 00".into(), "bigmod-modulus-size"),
        (format!("M {long} 03 01"), "bigmod-modulus-size"),
        ("M 0b 03 0c".into(), "bigmod-not-reduced"),
        ("M 0b - 02".into(), "bigmod-exponent"),
        ("X 0b 03 02".into(), "bigmod-output-length"),
        ("Y 0b 03 02".into(), "bigmod-work-length"),
        (format!("P 32 {n} {e} {msg} {sig}"), "ok"),
        (format!("P 32 {short} {e} {msg} {sig}"), "rsa-modulus-size"),
        (format!("P 32 {even} {e} {msg} {sig}"), "rsa-even-modulus"),
        (format!("P 32 {n} 01 {msg} {sig}"), "rsa-exponent"),
        (format!("P 32 {n} 010000 {msg} {sig}"), "rsa-exponent"),
        (format!("P 32 {n} {n} {msg} {sig}"), "rsa-exponent"),
        // Over 64 bits (#317): odd, 65 bits, and shorter than the modulus.
        (format!("P 32 {n} 010000000000000001 {msg} {sig}"), "rsa-exponent"),
        (format!("P 20 {n} {e} {msg} {sig}"), "rsa-hash"),
        (format!("D 32 {n} {e} {} {sig}", "00".repeat(31)), "rsa-digest-length"),
        (format!("P 32 {n} {e} {msg} {}", &sig[2..]), "rsa-signature-length"),
        (format!("P 32 {n} {e} {msg} {bare}"), "rsa-signature-range"),
        (format!("P 32 {n} {e} 00 {sig}"), "rsa-pkcs1-mismatch"),
        (format!("S 32 32 300 {n} {e} {msg} {sig}"), "rsa-pss-length"),
        (format!("S 32 20 32 {n} {e} {msg} {sig}"), "rsa-hash"),
    ];
    let lines: Vec<String> = cases.iter().map(|(c, _)| c.clone()).collect();
    let (dir, exe) = build_rsa_driver("refusals", "cranelift");
    for ((case, want), line) in cases.iter().zip(run_cases(&exe, &lines)) {
        assert_eq!(tag(&line), *want, "{}: {line}", &case[..case.len().min(40)]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
