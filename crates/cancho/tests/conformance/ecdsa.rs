//! `std.ecdsa` (`docs/ecdsa.md` §4): every case of seven Wycheproof ECDSA
//! files on P-256 and P-384 (DER and raw signatures, and digests longer and
//! shorter than n), the NIST CAVP SigVer cases for those curves, and every
//! refusal with its own tag. All through `tests/programs/ecdsa_driver.cho`.

use super::json::feed;
use super::*;

fn build_ecdsa_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("ecdsa-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/ecdsa_driver.cho"))
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

fn hash_len(name: &str) -> u32 {
    match name {
        "SHA-256" => 32,
        "SHA-384" => 48,
        "SHA-512" => 64,
        other => panic!("no {other} in std"),
    }
}

fn or_dash(hex: &str) -> &str {
    if hex.is_empty() { "-" } else { hex }
}

const WYCHEPROOF: [&str; 7] = [
    "ecdsa_secp256r1_sha256_test.json",
    "ecdsa_secp256r1_sha256_p1363_test.json",
    "ecdsa_secp256r1_sha512_test.json",
    "ecdsa_secp384r1_sha384_test.json",
    "ecdsa_secp384r1_sha384_p1363_test.json",
    "ecdsa_secp384r1_sha256_test.json",
    "ecdsa_secp384r1_sha512_test.json",
];

/// Every valid case verifies and every invalid one is refused; the files
/// have no "acceptable" cases. The counts of each refusal are pinned, so a
/// change in which rule refuses a case is seen.
#[test]
fn every_wycheproof_ecdsa_case_passes() {
    let mut cases = Vec::new();
    let mut want = Vec::new();
    for file in WYCHEPROOF {
        let path = repo_root().join("tests/vectors/wycheproof").join(file);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for group in v["testGroups"].as_array().unwrap() {
            let curve = match group["publicKey"]["curve"].as_str().unwrap() {
                "secp256r1" => 256,
                "secp384r1" => 384,
                other => panic!("{other}"),
            };
            let h = hash_len(group["sha"].as_str().unwrap());
            let op = if group["type"] == "EcdsaP1363Verify" { "R" } else { "V" };
            let point = group["publicKey"]["uncompressed"].as_str().unwrap();
            for t in group["tests"].as_array().unwrap() {
                let msg = or_dash(t["msg"].as_str().unwrap());
                let sig = or_dash(t["sig"].as_str().unwrap());
                cases.push(format!("{op} {curve} {h} {point} {msg} {sig}"));
                want.push((
                    file,
                    t["tcId"].as_u64().unwrap(),
                    t["result"].as_str().unwrap().to_string(),
                ));
            }
        }
    }
    assert_eq!(cases.len(), 3098);
    let (dir, exe) = build_ecdsa_driver("wycheproof", "llvm");
    let mut counts = std::collections::BTreeMap::new();
    for ((file, id, result), line) in want.iter().zip(run_cases(&exe, &cases)) {
        match result.as_str() {
            "valid" => assert_eq!(tag(&line), "ok", "{file} tcId {id}: {line}"),
            "invalid" => assert!(line.starts_with('-'), "{file} tcId {id}: {line}"),
            other => panic!("{file} tcId {id}: an unexpected result {other}"),
        }
        *counts.entry(format!("{result} {}", tag(&line))).or_insert(0) += 1;
    }
    let counts: Vec<String> = counts.iter().map(|(k, v)| format!("{k} {v}")).collect();
    assert_eq!(
        counts,
        [
            "invalid ecdsa-der-non-minimal 45",
            "invalid ecdsa-der-structure 875",
            "invalid ecdsa-mismatch 119",
            "invalid ecdsa-r-range 462",
            "invalid ecdsa-raw-length 40",
            "invalid ecdsa-result-infinity 35",
            "invalid ecdsa-s-range 152",
            "valid ok 1370",
        ]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// FIPS 186-3's SigVer for P-256 and P-384 with SHA-256/384/512: 90 cases,
/// each given the file's result.
#[test]
fn every_nist_ecdsa_sigver_case_passes() {
    let text =
        std::fs::read_to_string(repo_root().join("tests/vectors/cavp/ECDSA_SigVer_186-3.rsp"))
            .unwrap();
    let (mut cases, mut want) = (Vec::new(), Vec::new());
    let (mut curve, mut h, mut size) = (0, 0, 0);
    let (mut msg, mut qx, mut qy, mut r) =
        (String::new(), String::new(), String::new(), String::new());
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("[P-") {
            let (c, sha) = rest.trim_end_matches(']').split_once(",SHA-").unwrap();
            curve = c.parse().unwrap();
            h = sha.parse::<usize>().unwrap() / 8;
            size = curve / 8;
            continue;
        }
        let Some((k, v)) = line.split_once(" = ") else { continue };
        let pad = |s: &str| format!("{s:0>width$}", width = 2 * size);
        match k {
            "Msg" => msg = v.to_string(),
            "Qx" => qx = pad(v),
            "Qy" => qy = pad(v),
            "R" => r = pad(v),
            "S" => cases.push(format!("R {curve} {h} 04{qx}{qy} {msg} {r}{}", pad(v))),
            "Result" => want.push(v.starts_with('P')),
            _ => {}
        }
    }
    assert_eq!(cases.len(), 90);
    let (dir, exe) = build_ecdsa_driver("nist", "cranelift");
    let mut pass = 0;
    for (ok, line) in want.iter().zip(run_cases(&exe, &cases)) {
        if *ok {
            assert_eq!(tag(&line), "ok", "{line}");
            pass += 1;
        } else {
            assert_eq!(tag(&line), "ecdsa-mismatch", "{line}");
        }
    }
    assert_eq!(pass, 18);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every refusal `std.ecdsa` has, reached with its own tag
/// (`ecdsa-result-infinity` by Wycheproof's cases, in the first test).
#[test]
fn every_ecdsa_refusal_is_reached_with_its_own_tag() {
    let path = repo_root().join("tests/vectors/wycheproof/ecdsa_secp256r1_sha256_p1363_test.json");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let group = &v["testGroups"][0];
    let point = group["publicKey"]["uncompressed"].as_str().unwrap().to_string();
    let good = group["tests"].as_array().unwrap().iter().find(|t| t["result"] == "valid").unwrap();
    let msg = good["msg"].as_str().unwrap();
    let raw = good["sig"].as_str().unwrap();
    let (r, s) = raw.split_at(64);
    // The same signature in DER: both values are positive once a leading
    // 00 is added where the top bit is set.
    let int = |x: &str| {
        let x = x.trim_start_matches("00");
        let x = if u8::from_str_radix(&x[..2], 16).unwrap() >= 0x80 {
            format!("00{x}")
        } else {
            x.to_string()
        };
        format!("02{:02x}{x}", x.len() / 2)
    };
    let body = format!("{}{}", int(r), int(s));
    let der = format!("30{:02x}{body}", body.len() / 2);
    let p = "ffffffff00000001000000000000000000000000ffffffffffffffffffffffff";
    let n = "ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551";
    let x = &point[2..66];
    let y = &point[66..];
    // y + 1 (no carry: the test key's y does not end in ff).
    let last = u8::from_str_radix(&y[62..], 16).unwrap();
    assert_ne!(last, 0xff);
    let y1 = format!("{}{:02x}", &y[..62], last + 1);
    let cases: Vec<(String, &str)> = vec![
        (format!("R 256 32 {point} {msg} {raw}"), "ok"),
        (format!("V 256 32 {point} {msg} {der}"), "ok"),
        (format!("R 521 32 {point} {msg} {raw}"), "ecdsa-curve"),
        (format!("W 256 32 {point} {msg} {raw}"), "ecdsa-work-length"),
        (format!("R 256 32 02{x} {msg} {raw}"), "ecdsa-point-encoding"),
        (format!("R 256 32 {point}00 {msg} {raw}"), "ecdsa-point-encoding"),
        (format!("R 256 32 00 {msg} {raw}"), "ecdsa-point-infinity"),
        (format!("R 256 32 04{p}{y} {msg} {raw}"), "ecdsa-point-range"),
        (format!("R 256 32 04{x}{y1} {msg} {raw}"), "ecdsa-point-not-on-curve"),
        (format!("R 256 32 {point} {msg} {}", &raw[2..]), "ecdsa-raw-length"),
        (format!("V 256 32 {point} {msg} 31{}", &der[2..]), "ecdsa-der-structure"),
        (format!("V 256 32 {point} {msg} {der}00"), "ecdsa-der-structure"),
        (format!("V 256 32 {point} {msg} 3080{body}0000"), "ecdsa-der-structure"),
        (
            format!("V 256 32 {point} {msg} 3081{:02x}{body}", body.len() / 2),
            "ecdsa-der-non-minimal",
        ),
        (format!("V 256 32 {point} {msg} 3008020200010202{}", "0001"), "ecdsa-der-non-minimal"),
        (format!("R 256 32 {point} {msg} {}{s}", "00".repeat(32)), "ecdsa-r-range"),
        (format!("R 256 32 {point} {msg} {n}{s}"), "ecdsa-r-range"),
        (format!("R 256 32 {point} {msg} {r}{}", "00".repeat(32)), "ecdsa-s-range"),
        (format!("R 256 32 {point} {msg} {r}{n}"), "ecdsa-s-range"),
        (format!("V 256 32 {point} {msg} 3006020101020181"), "ecdsa-s-range"),
        (format!("R 256 32 {point} 00 {raw}"), "ecdsa-mismatch"),
        // A digest equal to n is e = 0: a signature made for e = 0 (with a
        // key and nonce from `random.Random(2041)`; pyca/cryptography
        // accepts it over this digest and refuses it over 00..01).
        (
            format!(
                "D 256 04e43536a661c7360d0f51bc8776c785682289e6594a550492b2f5af557ce73fd40d3d5b06fae511a128f0c7e4ef7ba418912e1d603e19bb2b72e1044877d76100 {n} 30460221009b27932cd0f703d408788b6f1538eae6db5bda4c9d1d0cfe8cd794e9b1c354ad022100b528c8277a637ad2c7d5b26e04394e10d6dccb9e99904ac089426db47637d012"
            ),
            "ok",
        ),
        (
            format!(
                "D 256 04e43536a661c7360d0f51bc8776c785682289e6594a550492b2f5af557ce73fd40d3d5b06fae511a128f0c7e4ef7ba418912e1d603e19bb2b72e1044877d76100 {}01 30460221009b27932cd0f703d408788b6f1538eae6db5bda4c9d1d0cfe8cd794e9b1c354ad022100b528c8277a637ad2c7d5b26e04394e10d6dccb9e99904ac089426db47637d012",
                "00".repeat(31)
            ),
            "ecdsa-mismatch",
        ),
    ];
    let lines: Vec<String> = cases.iter().map(|(c, _)| c.clone()).collect();
    let (dir, exe) = build_ecdsa_driver("refusals", "cranelift");
    for ((case, want), line) in cases.iter().zip(run_cases(&exe, &lines)) {
        assert_eq!(tag(&line), *want, "{}: {line}", &case[..case.len().min(40)]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
