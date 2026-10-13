//! `std.chacha20` (`docs/chacha20.md` §4): the RFC 8439 vectors, every
//! Wycheproof case, every refusal reached with its own tag, and a
//! one-bit change anywhere in a sealed message refused. All through
//! `tests/programs/aead_driver.cho`, built by both backends.

use super::json::feed;
use super::*;

fn build_aead_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("aead-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/aead_driver.cho"))
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

fn rfc8439_rows() -> Vec<(String, String, String)> {
    let text = std::fs::read_to_string(repo_root().join("tests/vectors/rfc8439.txt")).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let parts: Vec<&str> = l.split(" | ").collect();
            (parts[0].to_string(), parts[1].to_string(), parts[2].to_string())
        })
        .collect()
}

#[test]
fn every_rfc8439_vector_passes_on_both_backends() {
    let rows = rfc8439_rows();
    assert_eq!(rows.len(), 28, "§2.3.2 to §2.8.2 and A.1 to A.5");
    let cases: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_aead_driver("rfc8439", backend);
        for ((name, _, want), line) in rows.iter().zip(run_cases(&exe, &cases)) {
            let (code, tag, got) = answer(&line);
            assert_eq!((code, tag.as_str()), (0, "ok"), "{name} on {backend}");
            assert_eq!(&got, want, "{name} on {backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Wycheproof's `chacha20_poly1305_test.json`, every case: a valid one
/// seals to its ciphertext and tag and opens to its message; an invalid
/// one is refused by both, and `open` writes nothing.
#[test]
fn every_wycheproof_case_passes() {
    let path = repo_root().join("tests/vectors/wycheproof/chacha20_poly1305_test.json");
    let file: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut cases = Vec::new();
    let mut expect = Vec::new();
    for group in file["testGroups"].as_array().unwrap() {
        for t in group["tests"].as_array().unwrap() {
            let s = |k: &str| t[k].as_str().unwrap().to_string();
            let or_dash = |v: String| if v.is_empty() { "-".to_string() } else { v };
            let (key, iv, aad, msg, ct, tag) =
                (s("key"), s("iv"), s("aad"), s("msg"), s("ct"), s("tag"));
            let valid = s("result") == "valid";
            assert!(valid || s("result") == "invalid", "no `acceptable` cases in this file");
            let id = t["tcId"].as_u64().unwrap();
            cases.push(format!(
                "O {} {} {} {}",
                or_dash(key.clone()),
                or_dash(iv.clone()),
                or_dash(aad.clone()),
                or_dash(format!("{ct}{tag}"))
            ));
            expect.push((id, 'O', valid, msg.clone(), ct.len() / 2));
            // A modified tag is a fault in `open`'s input only; sealing
            // its message is fine and simply gives the real tag.
            let flags = t["flags"].to_string();
            if valid || flags.contains("InvalidNonceSize") {
                cases.push(format!(
                    "S {} {} {} {}",
                    or_dash(key),
                    or_dash(iv),
                    or_dash(aad),
                    or_dash(msg)
                ));
                expect.push((id, 'S', valid, format!("{ct}{tag}"), 0));
            }
        }
    }
    assert_eq!(cases.len(), 325 + 256 + 9);

    let (dir, exe) = build_aead_driver("wycheproof", "llvm");
    let mut counts = std::collections::BTreeMap::new();
    for ((id, op, valid, want, room), line) in expect.into_iter().zip(run_cases(&exe, &cases)) {
        let (code, tag, got) = answer(&line);
        if valid {
            assert_eq!((code, got.as_str()), (0, want.as_str()), "tcId {id} {op}: {line}");
        } else {
            assert!(code < 0, "tcId {id} {op} must be refused: {line}");
            if op == 'O' {
                assert_eq!(got, "aa".repeat(room), "tcId {id}: a refused open wrote its output");
            }
        }
        *counts.entry(tag).or_insert(0) += 1;
    }
    // 256 valid cases opened and sealed, 60 modified tags, 9 nonce sizes
    // refused by both operations: the counts `docs/chacha20.md` §4 states.
    let want: std::collections::BTreeMap<String, i32> =
        [("ok", 2 * 256), ("aead-tag-mismatch", 60), ("chacha20-nonce-length", 2 * 9)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
    assert_eq!(counts, want);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every refusal `std.chacha20` has, reached, with its own tag.
#[test]
fn every_refusal_is_reached_with_its_own_tag() {
    let key = hex(&[7u8; 32]);
    let nonce = hex(&[9u8; 12]);
    let rows = [
        (format!("S {} {nonce} - 00", hex(&[7u8; 31])), "chacha20-key-length"),
        (format!("O {key} {} - {}", hex(&[9u8; 8]), hex(&[0u8; 20])), "chacha20-nonce-length"),
        (format!("O {key} {nonce} - {}", hex(&[0u8; 15])), "aead-too-short"),
        (format!("O {key} {nonce} - {}", hex(&[0u8; 40])), "aead-tag-mismatch"),
        (format!("B {key} 4294967296 {nonce}"), "chacha20-counter-exhausted"),
        (format!("X {key} 4294967295 {nonce} {}", hex(&[0u8; 65])), "chacha20-counter-exhausted"),
        (format!("P {} 00", hex(&[1u8; 16])), "chacha20-key-length"),
    ];
    let cases: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let (dir, exe) = build_aead_driver("refusals", "llvm");
    for ((case, want), line) in rows.iter().zip(run_cases(&exe, &cases)) {
        let (code, tag, _) = answer(&line);
        assert!(code < 0, "{case}");
        assert_eq!(&tag, want, "{case}");
    }
    // The last block before the counter wraps is still allowed.
    let edge = vec![format!("X {key} 4294967295 {nonce} {}", hex(&[0u8; 64]))];
    assert_eq!(answer(&run_cases(&exe, &edge)[0]).0, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// RFC 8439 §2.8.2's message with each bit of the ciphertext, the tag
/// and the associated data flipped in turn: every one is refused, and
/// none writes its output. The tag check compares all sixteen bytes and
/// covers every byte it should.
#[test]
fn one_flipped_bit_anywhere_is_refused() {
    let rows = rfc8439_rows();
    let (_, line, sealed) = rows.iter().find(|r| r.0 == "rfc8439 2.8.2 aead seal").unwrap();
    let f: Vec<&str> = line.split(' ').collect();
    let (key, nonce, aad) = (f[1], f[2], unhex(f[3]));
    let sealed = unhex(sealed);
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
    let (dir, exe) = build_aead_driver("bitflip", "llvm");
    let answers = run_cases(&exe, &cases);
    assert_eq!(answer(&answers[0]).0, 0, "the unmodified message opens");
    let room = sealed.len() - 16;
    for (case, line) in cases.iter().zip(&answers).skip(1) {
        let (_, tag, got) = answer(line);
        assert_eq!(tag, "aead-tag-mismatch", "{case}");
        assert_eq!(got, "aa".repeat(room), "{case}");
    }
    assert_eq!(answers.len(), 1 + 130 * 8 + 12 * 8);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `docs/record-in-place.md` §3: `seal_in_place` writes what `seal` does and `open_in_place` opens what `open` does, on
/// both backends; a tag changed in its last byte is refused and leaves the buffer as it was; a buffer with no room for a
/// tag is refused with `seal`'s and `open`'s tags.
#[test]
fn in_place_seals_and_opens_what_seal_and_open_do_on_both_backends() {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut cases = Vec::new();
    for _ in 0..120 {
        let key: Vec<u8> = (0..32).map(|_| next() as u8).collect();
        let nonce: Vec<u8> = (0..12).map(|_| next() as u8).collect();
        let aad: Vec<u8> = (0..next() % 41).map(|_| next() as u8).collect();
        let text: Vec<u8> = (0..next() % 300).map(|_| next() as u8).collect();
        let (key, nonce, aad, text) = (hex(&key), hex(&nonce), hex(&aad), hex(&text));
        cases.push(format!("S {key} {nonce} {aad} {text}"));
        cases.push(format!("T {key} {nonce} {aad} {text}"));
    }
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_aead_driver("in-place", backend);
        let sealed = run_cases(&exe, &cases);
        let mut opens = Vec::new();
        for (two, case) in sealed.chunks(2).zip(cases.chunks(2)) {
            assert_eq!(two[0], two[1], "{} on {backend}", case[1]);
            let (code, _, out) = answer(&two[0]);
            assert_eq!(code, 0, "{}", case[0]);
            let f: Vec<&str> = case[0].split(' ').collect();
            let mut bad = unhex(&out);
            let last = bad.len() - 1;
            bad[last] ^= 1;
            for op in ["O", "U"] {
                opens.push(format!("{op} {} {} {} {out}", f[1], f[2], f[3]));
                opens.push(format!("{op} {} {} {} {}", f[1], f[2], f[3], hex(&bad)));
            }
        }
        let opened = run_cases(&exe, &opens);
        for (four, asked) in opened.chunks(4).zip(opens.chunks(4)) {
            assert_eq!(answer(&four[0]).0, 0, "{}", asked[0]);
            assert_eq!(four[2], four[0], "{} on {backend}", asked[2]);
            let (code, tag, got) = answer(&four[3]);
            assert_eq!((code, tag.as_str()), (-6, "aead-tag-mismatch"), "{}", asked[3]);
            // Nothing written: the buffer is the sealed message it was given, the last byte changed.
            assert_eq!(got, asked[3].rsplit(' ').next().unwrap(), "{} on {backend}", asked[3]);
        }
        let key = hex(&[7u8; 32]);
        let nonce = hex(&[9u8; 12]);
        let mut short = Vec::new();
        for n in 0..16u8 {
            let b = hex(&(0..n).collect::<Vec<u8>>());
            short.push(format!("V {key} {nonce} - {b}"));
            short.push(format!("U {key} {nonce} - {b}"));
        }
        for (case, line) in short.iter().zip(run_cases(&exe, &short)) {
            let (code, tag, got) = answer(&line);
            let want = if case.starts_with('V') {
                (-3, "chacha20-output-length")
            } else {
                (-5, "aead-too-short")
            };
            assert_eq!((code, tag.as_str()), want, "{case} on {backend}");
            let given = case.rsplit(' ').next().unwrap();
            assert_eq!(
                if got.is_empty() { "-" } else { got.as_str() },
                given,
                "{case}: the buffer as it was"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
