//! `std.aes` and `std.gcm` (`docs/tls-parity.md` §3.1): FIPS 197's
//! examples, NIST CAVP's 96-bit-IV GCM cases, every Wycheproof case,
//! every reachable refusal with its own tag, and a one-bit change
//! anywhere in a sealed message refused. All through
//! `tests/programs/gcm_driver.ls`.

use super::json::feed;
use super::*;

fn build_gcm_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("gcm-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/gcm_driver.ls"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

/// Every case through the driver, one line out per line in.
fn run_cases(exe: &Path, cases: &[String]) -> Vec<String> {
    let mut input = cases.join("\n");
    input.push('\n');
    let out = feed(exe, input.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
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

fn hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".to_string();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len() / 2).map(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap()).collect()
}

fn or_dash(text: &str) -> String {
    if text.is_empty() { "-".to_string() } else { text.to_string() }
}

/// One CAVP case: its fields, and whether it is marked `FAIL`.
struct Cavp {
    fields: std::collections::BTreeMap<String, String>,
    fail: bool,
}

/// The cases of one of the CAVP `.rsp` files in `tests/vectors/cavp`.
fn cavp(file: &str) -> Vec<Cavp> {
    let text = std::fs::read_to_string(repo_root().join("tests/vectors/cavp").join(file)).unwrap();
    let mut cases: Vec<Cavp> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("Count = ") {
            cases.push(Cavp { fields: Default::default(), fail: false });
        } else if line == "FAIL" {
            cases.last_mut().unwrap().fail = true;
        } else if let (Some(case), Some((k, v))) = (cases.last_mut(), line.split_once(" = ")) {
            case.fields.insert(k.to_string(), v.to_string());
        } else if let (Some(case), Some(k)) = (cases.last_mut(), line.strip_suffix(" =")) {
            case.fields.insert(k.to_string(), String::new());
        }
    }
    cases
}

/// FIPS 197's appendix C.1 and C.3 examples, one block each.
#[test]
fn the_fips197_examples_pass_on_both_backends() {
    let block = "00112233445566778899aabbccddeeff";
    let rows = [
        ("000102030405060708090a0b0c0d0e0f", "69c4e0d86a7b0430d8cdb78070b4c55a"),
        (
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
            "8ea2b7ca516745bfeafc49904b496089",
        ),
    ];
    let cases: Vec<String> = rows.iter().map(|(k, _)| format!("E {k} {block}")).collect();
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_gcm_driver("fips197", backend);
        for ((_, want), line) in rows.iter().zip(run_cases(&exe, &cases)) {
            assert_eq!(answer(&line), (0, "ok".to_string(), want.to_string()), "{backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// NIST CAVP's `gcmEncryptExtIV` and `gcmDecrypt` for 128- and 256-bit
/// keys, cut to 96-bit IVs and 128-bit tags (`tests/vectors/cavp/README.md`):
/// every encryption gives its ciphertext and tag, every decryption its
/// plaintext, and every case marked `FAIL` is refused without writing.
#[test]
fn every_cavp_gcm_case_passes_on_both_backends() {
    let mut cases = Vec::new();
    let mut expect = Vec::new();
    for file in ["gcmEncryptExtIV128.rsp", "gcmEncryptExtIV256.rsp"] {
        for c in cavp(file) {
            let f = |k: &str| c.fields[k].clone();
            cases.push(format!(
                "S {} {} {} {}",
                f("Key"),
                f("IV"),
                or_dash(&f("AAD")),
                or_dash(&f("PT"))
            ));
            expect.push((file, Some(format!("{}{}", f("CT"), f("Tag"))), 0));
        }
    }
    for file in ["gcmDecrypt128.rsp", "gcmDecrypt256.rsp"] {
        for c in cavp(file) {
            let f = |k: &str| c.fields[k].clone();
            let ct = f("CT");
            cases.push(format!(
                "O {} {} {} {}{}",
                f("Key"),
                f("IV"),
                or_dash(&f("AAD")),
                ct,
                f("Tag")
            ));
            let want = if c.fail { None } else { Some(f("PT")) };
            expect.push((file, want, ct.len() / 2));
        }
    }
    assert_eq!(cases.len(), 4 * 375);
    assert_eq!(expect.iter().filter(|e| e.1.is_none()).count(), 196 + 191);
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_gcm_driver("cavp", backend);
        for ((file, want, room), (case, line)) in
            expect.iter().zip(cases.iter().zip(run_cases(&exe, &cases)))
        {
            let (code, tag, got) = answer(&line);
            match want {
                Some(want) => {
                    assert_eq!((code, &got), (0, want), "{file} on {backend}: {case}")
                }
                None => {
                    assert_eq!(tag, "aead-tag-mismatch", "{file} on {backend}: {case}");
                    assert_eq!(got, "aa".repeat(*room), "{file}: a refused open wrote its output");
                }
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Wycheproof's `aes_gcm_test.json`, every case. A 128- or 256-bit key
/// with a 96-bit nonce is what `std.gcm` supports: a valid case seals to
/// its ciphertext and tag and opens to its message, and an invalid one
/// (a modified tag) is refused without writing. A 192-bit key, or a
/// nonce of any other length, is refused by both operations with its own
/// tag, valid case or not: TLS uses neither (`docs/tls-parity.md` §3.1).
#[test]
fn every_wycheproof_case_passes() {
    let path = repo_root().join("tests/vectors/wycheproof/aes_gcm_test.json");
    let file: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut cases = Vec::new();
    let mut expect = Vec::new();
    for group in file["testGroups"].as_array().unwrap() {
        for t in group["tests"].as_array().unwrap() {
            let s = |k: &str| t[k].as_str().unwrap().to_string();
            let (key, iv, aad, msg, ct, tag) =
                (s("key"), s("iv"), s("aad"), s("msg"), s("ct"), s("tag"));
            let valid = s("result") == "valid";
            assert!(valid || s("result") == "invalid", "no `acceptable` cases in this file");
            assert_eq!(tag.len(), 32, "every tag is 128 bits");
            let supported = (key.len() == 32 || key.len() == 64) && iv.len() == 24;
            let id = t["tcId"].as_u64().unwrap();
            cases.push(format!(
                "O {} {} {} {}",
                or_dash(&key),
                or_dash(&iv),
                or_dash(&aad),
                or_dash(&format!("{ct}{tag}"))
            ));
            expect.push((id, 'O', valid && supported, msg.clone(), ct.len() / 2));
            cases.push(format!(
                "S {} {} {} {}",
                or_dash(&key),
                or_dash(&iv),
                or_dash(&aad),
                or_dash(&msg)
            ));
            // A modified tag is a fault in `open`'s input only; sealing
            // its message is fine and simply gives the real tag, which
            // is not the case's.
            expect.push((id, 'S', valid && supported, format!("{ct}{tag}"), 0));
        }
    }
    assert_eq!(cases.len(), 2 * 316);

    let (dir, exe) = build_gcm_driver("wycheproof", "llvm");
    let mut counts = std::collections::BTreeMap::new();
    for ((id, op, works, want, room), line) in expect.into_iter().zip(run_cases(&exe, &cases)) {
        let (code, tag, got) = answer(&line);
        if works {
            assert_eq!((code, got.as_str()), (0, want.as_str()), "tcId {id} {op}: {line}");
        } else if op == 'O' {
            assert!(code < 0, "tcId {id} {op} must be refused: {line}");
            assert_eq!(got, "aa".repeat(room), "tcId {id}: a refused open wrote its output");
        }
        *counts.entry(format!("{op} {tag}")).or_insert(0) += 1;
    }
    // 79 supported valid cases, 54 modified tags (whose messages still
    // seal), 103 cases with a 192-bit key (the key is checked first) and 80
    // with a 128- or 256-bit key and another nonce length.
    let want: std::collections::BTreeMap<String, i32> = [
        ("O ok", 79),
        ("O aead-tag-mismatch", 54),
        ("O gcm-key-length", 103),
        ("O gcm-nonce-length", 80),
        ("S ok", 79 + 54),
        ("S gcm-key-length", 103),
        ("S gcm-nonce-length", 80),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    assert_eq!(counts, want);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every refusal `std.aes` and `std.gcm` can be driven to from the
/// driver, with its own tag. (`gcm-output-length` needs an output of the
/// wrong size, which the driver never makes, and `gcm-too-long` a
/// 64 GiB message; `conformance/refusals.rs` checks every tag is
/// distinct.)
#[test]
fn every_refusal_is_reached_with_its_own_tag() {
    let key = hex(&[7u8; 16]);
    let nonce = hex(&[9u8; 12]);
    let rows = [
        (format!("E {} {}", hex(&[7u8; 24]), hex(&[0u8; 16])), "aes-key-length"),
        (format!("S {} {nonce} - 00", hex(&[7u8; 31])), "gcm-key-length"),
        (format!("O {} {nonce} - {}", hex(&[7u8; 0]), hex(&[0u8; 20])), "gcm-key-length"),
        // A context `gcm.prepare` never filled is refused as a key.
        (format!("U {key} {nonce} - 00"), "gcm-key-length"),
        (format!("S {key} {} - 00", hex(&[9u8; 16])), "gcm-nonce-length"),
        (format!("O {key} {} - {}", hex(&[9u8; 8]), hex(&[0u8; 20])), "gcm-nonce-length"),
        (format!("O {key} {} - {}", hex(&[9u8; 8]), hex(&[0u8; 3])), "gcm-nonce-length"),
        (format!("O {key} {nonce} - {}", hex(&[0u8; 15])), "gcm-too-short"),
        (format!("O {key} {nonce} - {}", hex(&[0u8; 40])), "aead-tag-mismatch"),
        (format!("C {key} {nonce} 4294967296 {}", hex(&[0u8; 1])), "aes-counter-exhausted"),
        (format!("C {key} {nonce} 4294967295 {}", hex(&[0u8; 17])), "aes-counter-exhausted"),
        (format!("C {key} {nonce} -1 {}", hex(&[0u8; 1])), "aes-counter-exhausted"),
    ];
    let cases: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let (dir, exe) = build_gcm_driver("refusals", "llvm");
    for ((case, want), line) in rows.iter().zip(run_cases(&exe, &cases)) {
        let (code, tag, _) = answer(&line);
        assert!(code < 0, "{case}");
        assert_eq!(&tag, want, "{case}");
    }
    // The last block before the counter wraps is still allowed, and is
    // the encryption of the IV with the counter 0xffffffff.
    let edge = vec![
        format!("C {key} {nonce} 4294967295 {}", hex(&[0u8; 16])),
        format!("E {key} {nonce}ffffffff"),
    ];
    let answers = run_cases(&exe, &edge);
    assert_eq!(answer(&answers[0]).0, 0);
    assert_eq!(answer(&answers[0]).2, answer(&answers[1]).2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A CAVP message with each bit of the ciphertext, the tag and the
/// associated data flipped in turn: every one is refused, and none
/// writes its output.
#[test]
fn one_flipped_bit_anywhere_is_refused() {
    let c = cavp("gcmEncryptExtIV256.rsp")
        .into_iter()
        .find(|c| c.fields["PT"].len() == 2 * 51 && c.fields["AAD"].len() == 2 * 20)
        .expect("a case with a 51-byte message and 20 bytes of associated data");
    let f = |k: &str| c.fields[k].clone();
    let (key, nonce, aad) = (f("Key"), f("IV"), unhex(&f("AAD")));
    let sealed = unhex(&format!("{}{}", f("CT"), f("Tag")));
    let mut cases = vec![format!("O {key} {nonce} {} {}", hex(&aad), hex(&sealed))];
    for bit in 0..sealed.len() * 8 {
        let mut s = sealed.clone();
        s[bit / 8] ^= 1 << (bit % 8);
        cases.push(format!("O {key} {nonce} {} {}", hex(&aad), hex(&s)));
    }
    for bit in 0..aad.len() * 8 {
        let mut a = aad.clone();
        a[bit / 8] ^= 1 << (bit % 8);
        cases.push(format!("O {key} {nonce} {} {}", hex(&a), hex(&sealed)));
    }
    let (dir, exe) = build_gcm_driver("bitflip", "llvm");
    let answers = run_cases(&exe, &cases);
    assert_eq!(answer(&answers[0]), (0, "ok".to_string(), f("PT")), "the unmodified message opens");
    let room = sealed.len() - 16;
    for (case, line) in cases.iter().zip(&answers).skip(1) {
        let (_, tag, got) = answer(line);
        assert_eq!(tag, "aead-tag-mismatch", "{case}");
        assert_eq!(got, "aa".repeat(room), "{case}");
    }
    assert_eq!(answers.len(), 1 + 67 * 8 + 20 * 8);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `docs/crypto-builtins.md` §7: the hardware path (`S`, `O`, where the
/// CPU has the instructions) and the software path (`s`, `o`, whatever
/// it has) give the same bytes, over random keys, nonces, associated data
/// and lengths across block boundaries, in one LLVM program. On the CI
/// hosts the first is the hardware path (`crypto_builtins.rs` asserts
/// `hw_aes_gcm()` there).
#[test]
fn the_hardware_and_software_paths_agree() {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut cases = Vec::new();
    for k in 0..400u64 {
        let key: Vec<u8> = (0..if k % 2 == 0 { 16 } else { 32 }).map(|_| next() as u8).collect();
        let nonce: Vec<u8> = (0..12).map(|_| next() as u8).collect();
        let aad: Vec<u8> = (0..next() % 41).map(|_| next() as u8).collect();
        let text: Vec<u8> = (0..next() % 200).map(|_| next() as u8).collect();
        let (key, nonce, aad, text) =
            (hex(&key), hex(&nonce), or_dash(&hex(&aad)), or_dash(&hex(&text)));
        cases.push(format!("S {key} {nonce} {aad} {text}"));
        cases.push(format!("s {key} {nonce} {aad} {text}"));
    }
    let (dir, exe) = build_gcm_driver("paths", "llvm");
    let sealed = run_cases(&exe, &cases);
    let mut opens = Vec::new();
    for (pair, case) in sealed.chunks(2).zip(cases.chunks(2)) {
        assert_eq!(pair[0], pair[1], "{}", case[0]);
        let (code, _, out) = answer(&pair[0]);
        assert_eq!(code, 0, "{}", case[0]);
        let f: Vec<&str> = case[0].split(' ').collect();
        // Opened by each path, and once with the last byte of the tag
        // changed, which both refuse.
        for op in ["O", "o"] {
            opens.push(format!("{op} {} {} {} {out}", f[1], f[2], f[3]));
            let mut bad = unhex(&out);
            let last = bad.len() - 1;
            bad[last] ^= 1;
            opens.push(format!("{op} {} {} {} {}", f[1], f[2], f[3], hex(&bad)));
        }
    }
    let opened = run_cases(&exe, &opens);
    for (quad, case) in opened.chunks(4).zip(cases.chunks(2)) {
        assert_eq!(quad[0], quad[2], "{}", case[0]);
        assert_eq!(quad[1], quad[3], "{}", case[0]);
        assert_eq!(answer(&quad[0]).0, 0, "{}", case[0]);
        assert_eq!(answer(&quad[1]).1, "aead-tag-mismatch", "{}", case[0]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
