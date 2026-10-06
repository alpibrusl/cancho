//! `std.x25519` (`docs/x25519.md` §4): the RFC 7748 vectors on both backends,
//! every Wycheproof X25519 case, and each refusal with its own tag. Through
//! `tests/programs/curve25519_driver.cho`, which also drives `std.ed25519` for
//! `scripts/curve25519_differential.py`.

use super::json::feed;
use super::*;

fn build_curve_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("curve25519-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/curve25519_driver.cho"))
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

#[test]
fn every_rfc7748_vector_passes_on_both_backends() {
    let text = std::fs::read_to_string(repo_root().join("tests/vectors/rfc7748.txt")).unwrap();
    let rows: Vec<Vec<&str>> = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split(" | ").collect())
        .collect();
    assert_eq!(rows.len(), 8, "§5.2's two vectors and two iterations, §6.1's four values");
    let cases: Vec<String> = rows.iter().map(|r| r[1].to_string()).collect();
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_curve_driver("rfc7748", backend);
        for (row, line) in rows.iter().zip(run_cases(&exe, &cases)) {
            assert_eq!(line, format!("0 ok {}", row[2]), "{} on {backend}", row[0]);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Wycheproof's `x25519_test.json`, every case. A shared secret of all
/// zeros (a low-order public value) must be refused with
/// `x25519-zero-secret`, as RFC 8446 §7.4.2 requires of a TLS client;
/// every other case, valid or "acceptable" (twists, non-canonical values,
/// the top bit set), must give exactly the expected secret.
#[test]
fn every_wycheproof_x25519_case_passes() {
    let path = repo_root().join("tests/vectors/wycheproof/x25519_test.json");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut cases = Vec::new();
    let mut want = Vec::new();
    for group in v["testGroups"].as_array().unwrap() {
        assert_eq!(group["curve"], "curve25519");
        for t in group["tests"].as_array().unwrap() {
            let s = |k: &str| t[k].as_str().unwrap().to_string();
            cases.push(format!("S {} {}", s("private"), s("public")));
            let shared = s("shared");
            want.push((t["tcId"].as_u64().unwrap(), shared));
        }
    }
    assert_eq!(cases.len(), 518);
    let (dir, exe) = build_curve_driver("wycheproof", "llvm");
    let mut zero = 0;
    for ((id, shared), line) in want.iter().zip(run_cases(&exe, &cases)) {
        if shared.chars().all(|c| c == '0') {
            assert_eq!(line, format!("-2 x25519-zero-secret {shared}"), "tcId {id}");
            zero += 1;
        } else {
            assert_eq!(line, format!("0 ok {shared}"), "tcId {id}");
        }
    }
    assert_eq!(zero, 31, "the file's ZeroSharedSecret cases");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every refusal `std.x25519` has, reached with its own tag.
#[test]
fn every_x25519_refusal_is_reached_with_its_own_tag() {
    let k = "07".repeat(32);
    let rows = [
        (format!("S {} {}", "07".repeat(31), "09".repeat(32)), "x25519-length"),
        (format!("S {k} {}", "09".repeat(31)), "x25519-length"),
        // u = 0 and u = 1 are low-order points: the result is zero.
        (format!("S {k} {}", "00".repeat(32)), "x25519-zero-secret"),
        (format!("S {k} 01{}", "00".repeat(31)), "x25519-zero-secret"),
    ];
    let cases: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let (dir, exe) = build_curve_driver("refusals", "llvm");
    for ((case, tag), line) in rows.iter().zip(run_cases(&exe, &cases)) {
        assert_eq!(line.split(' ').nth(1), Some(*tag), "{case}: {line}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Wycheproof's `ed25519_test.json`, every case, through `std.ed25519.verify`
/// on the field module it now shares with X25519 (`docs/x25519.md` §4.2).
/// Two of these found bugs that predate the port: a truncated signature
/// trapped (no length check), and case 151, an `R` with x = 0 and the sign
/// bit set, was accepted (RFC 8032 §5.1.3 step 4 was not applied).
#[test]
fn every_wycheproof_ed25519_case_passes() {
    let path = repo_root().join("tests/vectors/wycheproof/ed25519_test.json");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut cases = Vec::new();
    let mut want = Vec::new();
    for group in v["testGroups"].as_array().unwrap() {
        let pk = group["publicKey"]["pk"].as_str().unwrap();
        for t in group["tests"].as_array().unwrap() {
            let s = |k: &str| t[k].as_str().unwrap().to_string();
            let dash = |x: String| if x.is_empty() { "-".to_string() } else { x };
            cases.push(format!("V {pk} {} {}", dash(s("msg")), dash(s("sig"))));
            want.push((t["tcId"].as_u64().unwrap(), s("result") == "valid"));
        }
    }
    assert_eq!(cases.len(), 151);
    let (dir, exe) = build_curve_driver("wycheproof-ed25519", "llvm");
    let mut valid = 0;
    for ((id, ok), line) in want.iter().zip(run_cases(&exe, &cases)) {
        let got = line.split(' ').next() == Some("1");
        assert_eq!(got, *ok, "tcId {id}: {line}");
        valid += usize::from(got);
    }
    assert_eq!(valid, 88, "the file's valid cases");
    let _ = std::fs::remove_dir_all(&dir);
}
