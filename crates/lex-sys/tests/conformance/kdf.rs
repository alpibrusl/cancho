//! `std.crypto`'s SHA-2 family, `std.hmac` and `std.hkdf` (`docs/hkdf.md`
//! §4): the NIST CAVP files, RFC 4231, RFC 5869, three TLS 1.3 key
//! schedules, every Wycheproof HMAC and HKDF case, messages past the 64 KiB
//! an arena holds, and every refusal with its own tag. All through
//! `tests/programs/kdf_driver.ls`.

use super::json::feed;
use super::*;

fn build_kdf_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("kdf-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/kdf_driver.ls"))
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

fn or_dash(hex: &str) -> &str {
    if hex.is_empty() { "-" } else { hex }
}

#[test]
fn every_hmac_hkdf_and_key_schedule_vector_passes_on_both_backends() {
    let text = std::fs::read_to_string(repo_root().join("tests/vectors/kdf.txt")).unwrap();
    let rows: Vec<Vec<&str>> = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split(" | ").collect())
        .collect();
    assert_eq!(rows.len(), 62, "RFC 4231, RFC 5869, pyca, RFC 8448, ACVP, OpenSSL");
    let cases: Vec<String> = rows.iter().map(|r| r[1].to_string()).collect();
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_kdf_driver("vectors", backend);
        for (row, line) in rows.iter().zip(run_cases(&exe, &cases)) {
            assert_eq!(line, format!("0 ok {}", row[2]), "{} on {backend}", row[0]);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Every case of the byte-oriented CAVP response files: short and long
/// messages, and the Monte Carlo test's 100 checkpoints.
#[test]
fn every_cavp_sha2_vector_passes() {
    let mut cases = Vec::new();
    let mut want = Vec::new();
    let mut counts = Vec::new();
    for (alg, file) in [
        (256, "SHA256ShortMsg"),
        (256, "SHA256LongMsg"),
        (384, "SHA384ShortMsg"),
        (384, "SHA384LongMsg"),
        (512, "SHA512ShortMsg"),
    ] {
        let text =
            std::fs::read_to_string(repo_root().join(format!("tests/vectors/cavp/{file}.rsp")))
                .unwrap();
        let (mut len, mut msg, mut n) = (0usize, String::new(), 0);
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("Len = ") {
                len = v.trim().parse().unwrap();
            } else if let Some(v) = line.strip_prefix("Msg = ") {
                msg = if len == 0 { String::new() } else { v.trim().to_string() };
            } else if let Some(v) = line.strip_prefix("MD = ") {
                cases.push(format!("H {alg} {}", or_dash(&msg)));
                want.push(format!("0 ok {}", v.trim()));
                n += 1;
            }
        }
        counts.push((file, n));
    }
    for (alg, file) in [(256, "SHA256Monte"), (384, "SHA384Monte"), (512, "SHA512Monte")] {
        let text =
            std::fs::read_to_string(repo_root().join(format!("tests/vectors/cavp/{file}.rsp")))
                .unwrap();
        let seed = text.lines().find_map(|l| l.strip_prefix("Seed = ")).unwrap().trim().to_string();
        let mds: Vec<&str> =
            text.lines().filter_map(|l| l.strip_prefix("MD = ")).map(str::trim).collect();
        assert_eq!(mds.len(), 100);
        cases.push(format!("C {alg} {seed}"));
        want.push(format!("0 ok {}", mds.join(",")));
        counts.push((file, 100));
    }
    // 65 short and 64 long messages for SHA-256, 129 and 128 for SHA-384
    // and SHA-512's short file: the counts `docs/hkdf.md` §4.1 states.
    assert_eq!(
        counts,
        [
            ("SHA256ShortMsg", 65),
            ("SHA256LongMsg", 64),
            ("SHA384ShortMsg", 129),
            ("SHA384LongMsg", 128),
            ("SHA512ShortMsg", 129),
            ("SHA256Monte", 100),
            ("SHA384Monte", 100),
            ("SHA512Monte", 100),
        ]
    );
    let (dir, exe) = build_kdf_driver("cavp", "llvm");
    for ((case, want), line) in cases.iter().zip(&want).zip(run_cases(&exe, &cases)) {
        assert_eq!(&line, want, "{}", &case[..case.len().min(60)]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `docs/hkdf.md` §2: the one-shot hashes copied the whole padded message
/// into one 64 KiB arena and trapped at 65,536 bytes. Now they stream, and
/// so does a message fed in pieces that straddle every block boundary.
#[test]
fn a_message_past_64_kib_hashes_and_pieces_do_not_matter() {
    let digests = [
        (256, 65536, "bf718b6f653bebc184e1479f1935b8da974d701b893afcf49e701f3e2f9f9c5a"),
        (256, 200000, "2287d207f24a941ff3b56c04c8a25ad56b63e3023207b3bb5b4ac0c9869d74be"),
        (
            384,
            65536,
            "c0675b21a7e5017efac4c32d72a372c41cca817204a5cb6f055cbdd3de3a66b8136bf521a6f7ea9cbe86927f45de6d25",
        ),
        (
            384,
            200000,
            "753b0cd072c3da5c8618e3e07359b74f767611cec28dc5d9bc09fe9f30f98d50c85e93c374c555dc7650aa3395d54463",
        ),
        (
            512,
            65536,
            "3d7f4d370531902d128619b9b9fb424b598b5fb11e92f44654e8bb754deed22c6cd5587482bd5d3bf506b05818848931a1e5627a542ba6aa4b7a27e003355291",
        ),
        (
            512,
            200000,
            "2ed06b8d48b029c60ebcab0e319f9a2080f219b2da482d1376f24413f18f6ae5b938b4f39ea523852c4f51cdd4d3809f82ee960ec277de954e0736ae550c12ed",
        ),
    ];
    // FIPS 180-4's own examples: a million `a`.
    let million = [
        (256, "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"),
        (
            384,
            "9d0e1809716474cb086e834e310a4a1ced149e9c00f248527972cec5704c2a5b07b8b3dc38ecc4ebae97ddd87f3d8985",
        ),
        (
            512,
            "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973ebde0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b",
        ),
    ];
    let mut cases = Vec::new();
    let mut want = Vec::new();
    for (alg, n, d) in digests {
        cases.push(format!("B {alg} {n}"));
        want.push(format!("0 ok {d}"));
    }
    for (alg, d) in million {
        cases.push(format!("A {alg} 1000000 61"));
        want.push(format!("0 ok {d}"));
    }
    // Every length from 0 to 300 bytes, one-shot and in pieces of 1, 7,
    // 63, 64, 65, 127, 128 and 129: the streamed digest must be the
    // one-shot one, whatever the boundaries.
    let mut pairs = Vec::new();
    for alg in [256, 384, 512] {
        for n in 0..=300usize {
            let msg: String = (0..n).map(|i| format!("{:02x}", (i * 131 + n) % 256)).collect();
            cases.push(format!("H {alg} {}", or_dash(&msg)));
            want.push(String::new());
            let base = cases.len() - 1;
            for piece in [1, 7, 63, 64, 65, 127, 128, 129] {
                cases.push(format!("U {alg} {piece} {}", or_dash(&msg)));
                want.push(String::new());
                pairs.push((base, cases.len() - 1));
            }
        }
    }
    let (dir, exe) = build_kdf_driver("long", "llvm");
    let answers = run_cases(&exe, &cases);
    for (i, w) in want.iter().enumerate() {
        if !w.is_empty() {
            assert_eq!(&answers[i], w, "{}", cases[i]);
        }
    }
    for (one, streamed) in pairs {
        assert_eq!(
            answers[one],
            answers[streamed],
            "{} against {}",
            cases[one],
            &cases[streamed][..12]
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Wycheproof's HMAC-SHA256/384 and HKDF-SHA256/384 files, every case. A
/// truncated tag is compared with the front of the full one; a modified
/// tag must differ from it; an output size over `255 * hash_len` must be
/// refused with `hkdf-length-too-large`.
#[test]
fn every_wycheproof_hmac_and_hkdf_case_passes() {
    let mut cases = Vec::new();
    let mut checks: Vec<(String, bool, String)> = Vec::new();
    let mut seen = std::collections::BTreeMap::new();
    for (hl, file) in [(32, "hmac_sha256_test"), (48, "hmac_sha384_test")] {
        let path = repo_root().join(format!("tests/vectors/wycheproof/{file}.json"));
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for group in v["testGroups"].as_array().unwrap() {
            for t in group["tests"].as_array().unwrap() {
                let s = |k: &str| t[k].as_str().unwrap().to_string();
                cases.push(format!("M {hl} {hl} {} {}", or_dash(&s("key")), or_dash(&s("msg"))));
                let valid = s("result") == "valid";
                checks.push((format!("{file} {}", t["tcId"]), valid, s("tag")));
                *seen.entry((file, valid)).or_insert(0) += 1;
            }
        }
    }
    for (hl, file) in [(32, "hkdf_sha256_test"), (48, "hkdf_sha384_test")] {
        let path = repo_root().join(format!("tests/vectors/wycheproof/{file}.json"));
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for group in v["testGroups"].as_array().unwrap() {
            for t in group["tests"].as_array().unwrap() {
                let s = |k: &str| t[k].as_str().unwrap().to_string();
                let size = t["size"].as_u64().unwrap();
                cases.push(format!(
                    "K {hl} {} {} {} {size}",
                    or_dash(&s("salt")),
                    or_dash(&s("ikm")),
                    or_dash(&s("info"))
                ));
                let valid = s("result") == "valid";
                checks.push((format!("{file} {}", t["tcId"]), valid, s("okm")));
                *seen.entry((file, valid)).or_insert(0) += 1;
            }
        }
    }
    let (dir, exe) = build_kdf_driver("wycheproof", "llvm");
    for ((name, valid, want), line) in checks.iter().zip(run_cases(&exe, &cases)) {
        let (code, tag, got) = answer(&line);
        if name.starts_with("hmac") {
            assert_eq!(code, 0, "{name}");
            assert_eq!(got.starts_with(want.as_str()), *valid, "{name}: {line}");
        } else if *valid {
            assert_eq!((code, got.as_str()), (0, want.as_str()), "{name}");
        } else {
            assert_eq!(tag, "hkdf-length-too-large", "{name}");
        }
    }
    let want: std::collections::BTreeMap<(&str, bool), i32> = [
        (("hkdf_sha256_test", false), 3),
        (("hkdf_sha256_test", true), 83),
        (("hkdf_sha384_test", false), 3),
        (("hkdf_sha384_test", true), 80),
        (("hmac_sha256_test", false), 108),
        (("hmac_sha256_test", true), 66),
        (("hmac_sha384_test", false), 108),
        (("hmac_sha384_test", true), 66),
    ]
    .into_iter()
    .collect();
    assert_eq!(seen, want);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every refusal `std.hmac` and `std.hkdf` have, reached, with its own tag,
/// and the edges that are still allowed.
#[test]
fn every_refusal_is_reached_with_its_own_tag() {
    let k32 = "07".repeat(32);
    let label = |n: usize| "61".repeat(n);
    let rows = [
        (format!("M 20 20 {k32} 00"), "hash-unsupported"),
        (format!("M 64 64 {k32} 00"), "hash-unsupported"),
        (format!("M 32 31 {k32} 00"), "hash-output-length"),
        (format!("E 32 31 - {k32}"), "hash-output-length"),
        (format!("E 48 32 - {k32}"), "hash-output-length"),
        (format!("X 32 {k32} - {}", 255 * 32 + 1), "hkdf-length-too-large"),
        (format!("X 48 {} - {}", "07".repeat(48), 255 * 48 + 1), "hkdf-length-too-large"),
        (format!("X 32 {} - 32", "07".repeat(31)), "hkdf-prk-length"),
        (format!("L 32 {k32} - - 32"), "hkdf-label-length"),
        (format!("L 32 {k32} {} - 32", label(250)), "hkdf-label-length"),
        (format!("L 32 {k32} {} {} 32", label(5), "00".repeat(256)), "hkdf-context-length"),
        (format!("D 32 32 {k32} {} {}", label(7), "00".repeat(31)), "hkdf-transcript-hash-length"),
        (format!("D 32 31 {k32} {} {k32}", label(7)), "hash-output-length"),
        (format!("D 48 48 {} {} {k32}", "07".repeat(48), label(7)), "hkdf-transcript-hash-length"),
    ];
    let edges = [
        format!("X 32 {k32} - {}", 255 * 32),
        format!("X 48 {} - {}", "07".repeat(48), 255 * 48),
        format!("L 32 {k32} {} {} 16", label(249), "00".repeat(255)),
        format!("L 32 {k32} {} - 0", label(1)),
        format!("X 32 {k32} - 0"),
    ];
    let mut cases: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    cases.extend(edges.iter().cloned());
    let (dir, exe) = build_kdf_driver("refusals", "llvm");
    let answers = run_cases(&exe, &cases);
    for ((case, want), line) in rows.iter().zip(&answers) {
        let (code, tag, got) = answer(line);
        assert!(code < 0, "{case}");
        assert_eq!(&tag, want, "{case}");
        assert!(got.chars().all(|c| c == '0'), "a refusal wrote its output: {case}");
    }
    for (case, line) in edges.iter().zip(&answers[rows.len()..]) {
        assert_eq!(answer(line).0, 0, "{case}: {line}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
