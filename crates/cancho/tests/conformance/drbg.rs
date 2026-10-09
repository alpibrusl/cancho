//! `std.drbg` (#289): the fast-key-erasure generator that
//! `packages/tls` carried privately (`docs/tls-pure.md` §6), moved to
//! `std`. The output bytes of one draw and a draw past one block, the
//! reseed's key erasure, and every refusal with its tag, through
//! `tests/programs/drbg_driver.cho`. The Cranelift backend here; CI,
//! which has `clang`, runs the same cases on LLVM through the shared
//! driver. The expected
//! bytes were computed from RFC 8439's ChaCha20 block directly, not
//! from `std.chacha20`.

use super::json::feed;
use super::*;

fn build_drbg_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("drbg-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/drbg_driver.cho"))
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

const SEED: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const SEED2: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

/// One block's second half (32 bytes), a short draw (16), and draws
/// that span two and three keys (48, 64), 
#[test]
fn fast_key_erasure_draws() {
    let cases = vec![
        format!("D {SEED} 32"),
        format!("D {SEED} 16"),
        format!("D {SEED} 48"),
        format!("D {SEED} 64"),
    ];
    let want = [
        format!("0 ok 2b23cce7a26023ab3f0eef693ac87f64258235eab1f7a32dc22762a0485b410c"),
        format!("0 ok 2b23cce7a26023ab3f0eef693ac87f64"),
        format!("0 ok 2b23cce7a26023ab3f0eef693ac87f64258235eab1f7a32dc22762a0485b410c2d41a59c90e41a8e7a4dccaa1c460699"),
        format!("0 ok 2b23cce7a26023ab3f0eef693ac87f64258235eab1f7a32dc22762a0485b410c2d41a59c90e41a8e7a4dccaa1c46069983b1a333ce25719ec3437768ab57fa42"),
    ];
    for backend in ["cranelift"] {
        let (dir, exe) = build_drbg_driver("draws", backend);
        let got = run_cases(&exe, &cases);
        for (g, w) in got.iter().zip(want.iter()) {
            assert_eq!(g, w, "{backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A reseed erases the old key (the reseeded draw equals a fresh one
/// under the second seed), a draw before any seed refuses
/// `drbg-unseeded`, and a seed of the wrong length refuses
/// `drbg-seed-length`.
#[test]
fn reseed_erases_and_refusals() {
    let cases = vec![
        format!("R {SEED} {SEED2}"),
        format!("D {SEED2} 32"),
        format!("U 32"),
        format!("S ff"),
        format!("S -"),
    ];
    let want = [
        format!("0 ok 66dd107034b4582a2ef42c5e1ea475f2fea477a10a9f1d75b3635243b2506b32"),
        format!("0 ok 66dd107034b4582a2ef42c5e1ea475f2fea477a10a9f1d75b3635243b2506b32"),
        "-2 drbg-unseeded -".to_string(),
        "-1 drbg-seed-length -".to_string(),
        "-1 drbg-seed-length -".to_string(),
    ];
    for backend in ["cranelift"] {
        let (dir, exe) = build_drbg_driver("reseed", backend);
        let got = run_cases(&exe, &cases);
        for (g, w) in got.iter().zip(want.iter()) {
            assert_eq!(g, w, "{backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
