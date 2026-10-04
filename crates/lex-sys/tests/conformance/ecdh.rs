//! `std.ecdh` (`docs/ecdh.md` §4): every case of Wycheproof's P-256 and
//! P-384 ECDH files with raw points, NIST CAVP's KAS validity cases for
//! those curves, the scalar's edges, and every refusal with its own tag.
//! All through `tests/programs/ecdh_driver.ls`.

use super::json::feed;
use super::*;

fn build_ecdh_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("ecdh-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/ecdh_driver.ls"))
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

/// `<code> <tag> <hex>`.
fn answer(line: &str) -> (i64, String, String) {
    let mut words = line.splitn(3, ' ');
    let code = words.next().unwrap().parse().unwrap();
    let tag = words.next().unwrap_or("").to_string();
    let hex = words.next().unwrap_or("").to_string();
    (code, tag, hex)
}

/// A Wycheproof private key (an ASN.1 integer's bytes: a leading zero
/// byte, or fewer bytes than the curve's size) as exactly `size` bytes.
fn scalar_hex(text: &str, size: usize) -> String {
    let digits = text.trim_start_matches('0');
    assert!(digits.len() <= 2 * size, "{text}");
    format!("{digits:0>width$}", width = 2 * size)
}

/// Wycheproof's `ecdh_secp{256,384}r1_ecpoint_test.json`, every case: a
/// valid one gives its shared secret; an invalid one (a point off the
/// curve, out of range, or badly encoded) is refused. The one
/// `acceptable` case a file is a compressed point, refused: TLS 1.3
/// sends points uncompressed (RFC 8446 §4.2.8.2).
#[test]
fn every_wycheproof_case_passes() {
    let (dir, exe) = build_ecdh_driver("wycheproof", "llvm");
    for (curve, total, valid, tags) in [
        (
            256,
            355,
            330,
            [("ecdh-point-encoding", 9), ("ecdh-point-not-on-curve", 9), ("ecdh-point-range", 7)],
        ),
        (
            384,
            790,
            771,
            [("ecdh-point-encoding", 3), ("ecdh-point-not-on-curve", 9), ("ecdh-point-range", 7)],
        ),
    ] {
        let path = repo_root()
            .join(format!("tests/vectors/wycheproof/ecdh_secp{curve}r1_ecpoint_test.json"));
        let file: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let size = curve / 8;
        let mut cases = Vec::new();
        let mut expect = Vec::new();
        for group in file["testGroups"].as_array().unwrap() {
            for t in group["tests"].as_array().unwrap() {
                let s = |k: &str| t[k].as_str().unwrap().to_string();
                let public = if s("public").is_empty() { "-".to_string() } else { s("public") };
                cases.push(format!("S {curve} {} {public}", scalar_hex(&s("private"), size)));
                expect.push((t["tcId"].as_u64().unwrap(), s("result"), s("shared")));
            }
        }
        assert_eq!(cases.len(), total);
        let mut counts = std::collections::BTreeMap::new();
        for ((id, result, shared), line) in expect.iter().zip(run_cases(&exe, &cases)) {
            let (code, tag, got) = answer(&line);
            if result == "valid" {
                assert_eq!((code, &got), (0, shared), "P-{curve} tcId {id}");
            } else {
                assert!(code < 0, "P-{curve} tcId {id} ({result}) must be refused: {line}");
                *counts.entry(tag).or_insert(0) += 1;
            }
        }
        assert_eq!(expect.iter().filter(|e| e.1 == "valid").count(), valid);
        let want: std::collections::BTreeMap<String, i32> =
            tags.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        assert_eq!(counts, want, "P-{curve}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// NIST CAVP's KAS validity test (static unified, Z only), its P-256 and
/// P-384 sections (`tests/vectors/cavp/README.md`), on both backends.
/// For a case marked `P`, the IUT's public key is its private key times
/// G, and Z is the private key times the CAVS's public key. Each `F`
/// case is caught by the check its reason names: the CAVS's key refused
/// as off the curve (1, 2); the IUT's public key not that of its private
/// key (5, 6, 7); Z not the shared secret (8).
#[test]
fn every_cavp_kas_case_passes_on_both_backends() {
    let text = std::fs::read_to_string(
        repo_root()
            .join("tests/vectors/cavp/KASValidityTest_ECCStaticUnified_NOKC_ZZOnly_resp.fax"),
    )
    .unwrap();
    let mut cases = Vec::new();
    let mut expect = Vec::new();
    let mut curve = 0;
    let mut f = std::collections::BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("[EC - ") {
            curve = 256;
        } else if line.starts_with("[ED - ") {
            curve = 384;
        } else if let Some((k, v)) = line.split_once(" = ") {
            f.insert(k.to_string(), v.to_string());
            if k == "Result" {
                let reason: u32 = v[3..].split(' ').next().unwrap().parse().unwrap();
                let pass = v.starts_with('P');
                cases.push(format!("K {curve} {}", f["dsIUT"]));
                cases.push(format!("S {curve} {} 04{}{}", f["dsIUT"], f["QsCAVSx"], f["QsCAVSy"]));
                expect.push((curve, f["COUNT"].clone(), pass, reason, f.clone()));
            }
        }
    }
    assert_eq!(expect.len(), 60);
    assert_eq!(expect.iter().filter(|e| e.2).count(), 36);
    for backend in ["llvm", "cranelift"] {
        let (dir, exe) = build_ecdh_driver("cavp", backend);
        let answers = run_cases(&exe, &cases);
        for (i, (curve, count, pass, reason, f)) in expect.iter().enumerate() {
            let what = format!("P-{curve} COUNT {count} on {backend}");
            let (kc, _, key) = answer(&answers[2 * i]);
            let (zc, ztag, z) = answer(&answers[2 * i + 1]);
            let iut = format!("04{}{}", f["QsIUTx"], f["QsIUTy"]);
            match (pass, reason) {
                (true, _) => {
                    assert_eq!((kc, &key), (0, &iut), "{what}");
                    assert_eq!((zc, &z), (0, &f["Z"]), "{what}");
                }
                (false, 1 | 2) => assert_eq!(ztag, "ecdh-point-not-on-curve", "{what}"),
                (false, 5..=7) => {
                    assert_eq!(kc, 0, "{what}");
                    assert_ne!(key, iut, "{what}");
                }
                (false, 8) => {
                    assert_eq!(zc, 0, "{what}");
                    assert_ne!(z, f["Z"], "{what}");
                }
                _ => panic!("{what}: an unexpected reason {reason}"),
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The scalar's edges: 1 gives G, n - 1 gives -G (G's x, the other y),
/// and 0 and n are refused, on both curves.
#[test]
fn the_scalar_edges() {
    let curves = [
        (
            256,
            "ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551",
            "6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296",
            "4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5",
            "ffffffff00000001000000000000000000000000ffffffffffffffffffffffff",
        ),
        (
            384,
            "ffffffffffffffffffffffffffffffffffffffffffffffffc7634d81f4372ddf581a0db248b0a77aecec196accc52973",
            "aa87ca22be8b05378eb1c71ef320ad746e1d3b628ba79b9859f741e082542a385502f25dbf55296c3a545e3872760ab7",
            "3617de4a96262c6f5d9e98bf9292dc29f8f41dbd289a147ce9da3113b5f0b8c00a60b1ce1d7e819d7a431d7c90ea0e5f",
            "fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffeffffffff0000000000000000ffffffff",
        ),
    ];
    let (dir, exe) = build_ecdh_driver("edges", "llvm");
    for (curve, n, gx, gy, p) in curves {
        let size = curve / 8;
        let one = format!("{:0>width$}", "1", width = 2 * size);
        let n_minus_1 = sub_one(n);
        let neg_gy = sub_hex(p, gy);
        let cases = vec![
            format!("K {curve} {one}"),
            format!("K {curve} {n_minus_1}"),
            format!("K {curve} {}", "0".repeat(2 * size)),
            format!("K {curve} {n}"),
            format!("S {curve} {one} 04{gx}{gy}"),
        ];
        let answers = run_cases(&exe, &cases);
        assert_eq!(answer(&answers[0]), (0, "ok".into(), format!("04{gx}{gy}")), "P-{curve}");
        assert_eq!(answer(&answers[1]), (0, "ok".into(), format!("04{gx}{neg_gy}")), "P-{curve}");
        assert_eq!(answer(&answers[2]).1, "ecdh-scalar-range", "P-{curve}");
        assert_eq!(answer(&answers[3]).1, "ecdh-scalar-range", "P-{curve}");
        assert_eq!(answer(&answers[4]), (0, "ok".into(), gx.to_string()), "P-{curve}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Big-endian hex minus one, for a value whose last digit is not 0.
fn sub_one(h: &str) -> String {
    let mut b: Vec<u8> =
        (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect();
    *b.last_mut().unwrap() -= 1;
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Big-endian hex a - b, for a >= b, the same length.
fn sub_hex(a: &str, b: &str) -> String {
    let bytes = |h: &str| -> Vec<u8> {
        (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect()
    };
    let (x, y) = (bytes(a), bytes(b));
    let mut out = vec![0u8; x.len()];
    let mut borrow = 0i32;
    for i in (0..x.len()).rev() {
        let d = x[i] as i32 - y[i] as i32 - borrow;
        out[i] = d.rem_euclid(256) as u8;
        borrow = (d < 0) as i32;
    }
    out.iter().map(|v| format!("{v:02x}")).collect()
}

/// Every refusal the driver can reach, with its own tag.
/// (`ecdh-work-length` and `ecdh-output-length` need a buffer of the wrong
/// size, which the driver never makes; `ecdh-result-infinity` needs an
/// input every check before it refuses.)
#[test]
fn every_refusal_is_reached_with_its_own_tag() {
    let d = format!("{:0>64}", "7");
    let g = "6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c2964fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5";
    let p = "ffffffff00000001000000000000000000000000ffffffffffffffffffffffff";
    let rows = [
        (format!("K 255 {d}"), "ecdh-curve"),
        (format!("K 256 {}", &d[2..]), "ecdh-scalar-length"),
        (format!("K 256 {}", "f".repeat(64)), "ecdh-scalar-range"),
        (format!("S 256 {d} 02{}", &g[..64]), "ecdh-point-encoding"),
        (format!("S 256 {d} 00"), "ecdh-point-encoding"),
        (format!("S 256 {d} 05{g}"), "ecdh-point-encoding"),
        (format!("S 256 {d} 04{p}{}", &g[64..]), "ecdh-point-range"),
        (format!("S 256 {d} 04{}{}", &g[..64], p), "ecdh-point-range"),
        (format!("S 256 {d} 04{}{}", &g[..64], &g[..64]), "ecdh-point-not-on-curve"),
    ];
    let cases: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let (dir, exe) = build_ecdh_driver("refusals", "llvm");
    for ((case, want), line) in rows.iter().zip(run_cases(&exe, &cases)) {
        let (code, tag, _) = answer(&line);
        assert!(code < 0, "{case}");
        assert_eq!(&tag, want, "{case}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `std.bigmod`'s constant-time reduction (`docs/ecdh.md` §2), through
/// `pow_mod` at moduli whose length is a multiple of 30 bits, where a sum
/// or a Montgomery product reaches the carry limb. P-256 and P-384 never
/// do, so the cases above cannot show it.
#[test]
fn bigmod_powers_at_every_limb_boundary_pass_on_both_backends() {
    let text =
        std::fs::read_to_string(repo_root().join("tests/vectors/bigmod_powers.txt")).unwrap();
    let rows: Vec<(String, String)> = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let (case, want) = l.split_once(" | ").unwrap();
            (case.to_string(), want.to_string())
        })
        .collect();
    assert_eq!(rows.len(), 220);
    let cases: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    for backend in ["llvm", "cranelift"] {
        let dir = scratch(&format!("bigmod-powers-{backend}"));
        let exe = dir.join("driver");
        let build = Command::new(BIN)
            .args(["build", "--std", "--backend", backend])
            .arg(repo_root().join("tests/programs/bigmod_driver.ls"))
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("the compiler runs");
        assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
        for ((case, want), line) in rows.iter().zip(run_cases(&exe, &cases)) {
            assert_eq!(line, format!("0 {want}"), "{case} on {backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
