//! `std.p256` (`docs/p256-fast.md` §4 and §8): the field and the group-order arithmetic, the
//! public key of edge scalars through the generator's table, and the wNAF of edge scalars,
//! against Python's integers (`tests/vectors/p256.txt`, written by
//! `scripts/p256_field_differential.py --vectors`), on both backends; and the generated files
//! are what their generators print. All through `tests/programs/p256_driver.cho`.

use super::json::feed;
use super::*;

fn build_driver(backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("p256-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/p256_driver.cho"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

/// Every case of the vector file: `<driver case> | <answer>`, run through the driver in one go.
fn check_vectors(backend: &str) {
    let (_dir, exe) = build_driver(backend);
    let text =
        std::fs::read_to_string(repo_root().join("tests/vectors/p256.txt")).expect("the vectors");
    let mut cases = Vec::new();
    let mut wants = Vec::new();
    for line in text.lines() {
        let (case, want) = line.split_once(" | ").expect("`case | answer`");
        cases.push(case.to_string());
        wants.push(want.to_string());
    }
    let mut input = cases.join("\n");
    input.push('\n');
    let out = feed(&exe, input.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let got: Vec<String> =
        String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    assert_eq!(got.len(), cases.len(), "one answer per case");
    let mut bad = Vec::new();
    for (i, ((case, want), got)) in cases.iter().zip(&wants).zip(&got).enumerate() {
        // The driver prints a trailing space after the last digit of a `W`; the file keeps it.
        if got.trim_end() != want.trim_end() {
            bad.push(format!("case {i}: {case}\n  want {want}\n  got  {got}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {} differ on {backend}:\n{}",
        bad.len(),
        cases.len(),
        bad[..bad.len().min(3)].join("\n")
    );
}

/// Multiplication, addition, subtraction, squaring, inversion and a lazy chain modulo p; the same
/// modulo the group order; load and store; `below_p` and `below_n`; the public key of 100-odd edge
/// and random scalars (the signed digits' carries, 8 and 9 in every nibble, n - 1, the refusals of
/// 0, n and 2^256 - 1); the 5- and 7-wide wNAF of edge and random scalars, digit for digit.
#[test]
fn the_field_the_table_and_the_wnaf_equal_pythons_on_both_backends() {
    check_vectors("llvm");
    check_vectors("cranelift");
}

/// `std/p256_kernels.cho` and `std/p256_comb.cho` are generated; a hand edit, or a change to a
/// generator that was not re-run, is a failure here. Skipped, loudly, where there is no `python3`.
#[test]
fn the_generated_files_are_what_their_generators_print() {
    for script in ["scripts/p256_gen.py", "scripts/p256_tables.py"] {
        let run = Command::new("python3").arg(repo_root().join(script)).arg("--check").output();
        match run {
            Ok(out) => assert!(
                out.status.success(),
                "{script} --check: {}",
                String::from_utf8_lossy(&out.stdout)
            ),
            Err(e) => eprintln!("skipped {script}: no python3 ({e})"),
        }
    }
}

/// The value bounds the lazy reduction relies on (`scripts/p256_bounds.py`): every formula's
/// multiplication inputs, subtrahends and values stay inside the kernels' limits.
#[test]
fn the_formulas_stay_inside_the_kernels_bounds() {
    match Command::new("python3").arg(repo_root().join("scripts/p256_bounds.py")).output() {
        Ok(out) => assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout)),
        Err(e) => eprintln!("skipped p256_bounds.py: no python3 ({e})"),
    }
}
