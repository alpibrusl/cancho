//! `f32` (`docs/f32.md`): F1's gate, and the fixture that shows the type.
//!
//! The gate is `tests/programs/f32_gate.ls`, built by each backend and run:
//! every `f32` operation (`+ - * /` and the six comparisons) over random
//! operand pairs and over every special value, each result held against
//! binary64 arithmetic rounded once -- by `f32_of`, and independently by
//! integer arithmetic in the program itself, because an optimiser may
//! legally turn the first into the instruction under test (the program's
//! own header). The operands are made with `f32_of_bits`, so what runs is
//! each backend's real binary32 instructions.

use std::time::Instant;

use super::*;

/// Random pairs per operation, on top of every special value against every
/// special value. `docs/f32.md` §5 asks for 10^6; it takes a fraction of a
/// second per backend, so that is the default and the whole gate runs in
/// every `cargo test`. `LEX_F32_PAIRS` overrides it, for a longer run.
const DEFAULT_PAIRS: u64 = 1_000_000;

fn pairs() -> u64 {
    std::env::var("LEX_F32_PAIRS")
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(DEFAULT_PAIRS)
}

/// How many special values the program lists, which the program does not
/// print: 255 exponents and both signs, 46 subnormal powers of two, and
/// 29 more for each sign. Squared, because every one meets every one.
const SPECIALS: u64 = 255 * 2 + 23 * 2 + 2 * 29;

fn build_gate(tag: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(tag);
    let exe = dir.join("f32_gate");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/f32_gate.ls"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

/// One row of the gate's report: `op pairs bad-vs-f32_of bad-vs-integer-rounding
/// first-a first-b first-got first-want`.
struct Row {
    op: u64,
    pairs: u64,
    bad_f32_of: u64,
    bad_integer: u64,
    first: [u64; 4],
}

fn rows(text: &str) -> (Vec<Row>, Vec<[u64; 5]>) {
    let mut rows = Vec::new();
    let mut cover = Vec::new();
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.first() == Some(&"cover") {
            let numbers: Vec<u64> =
                words[2..].iter().map(|w| w.parse().expect("a count")).collect();
            cover.push(numbers.try_into().expect("five coverage counts"));
            continue;
        }
        let n: Vec<u64> = words.iter().map(|w| w.parse().expect("a number")).collect();
        assert_eq!(n.len(), 8, "a gate row has eight numbers: `{line}`");
        rows.push(Row {
            op: n[0],
            pairs: n[1],
            bad_f32_of: n[2],
            bad_integer: n[3],
            first: [n[4], n[5], n[6], n[7]],
        });
    }
    (rows, cover)
}

const OPS: [&str; 10] = ["+", "-", "*", "/", "==", "!=", "<", "<=", ">", ">="];

fn gate(backend: &str) -> String {
    let n = pairs();
    let (dir, exe) = build_gate(&format!("f32-gate-{backend}"), backend);
    let started = Instant::now();
    let out = Command::new(&exe).arg(n.to_string()).output().expect("the gate runs");
    let elapsed = started.elapsed();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let (rows, cover) = rows(&stdout);

    // Printed so `cargo test -- --nocapture` shows the run time the
    // document quotes.
    println!("f32 gate on {backend}: {n} random pairs + {SPECIALS}^2 special, ran in {elapsed:?}");

    assert_eq!(rows.len(), 10, "one row per operation:\n{stdout}");
    let expected = n + SPECIALS * SPECIALS;
    for row in &rows {
        let op = OPS[row.op as usize];
        assert_eq!(
            row.pairs, expected,
            "`{op}` saw the wrong number of pairs; the list of special values changed?"
        );
        assert!(
            row.bad_f32_of == 0 && row.bad_integer == 0,
            "f32 `{op}` on {backend}: {} results differ from the binary64 oracle (`f32_of(\
             float_of32(a) {op} float_of32(b))`, or for a comparison the same comparison on \
             the promoted operands) and {} from the integer oracle (that result rounded by \
             integer arithmetic, or the comparison made on bit patterns); the first was a = \
             {:#010x}, b = {:#010x}, got {:#010x}, expected {:#010x} (a comparison's 1 is \
             true)",
            row.bad_f32_of,
            row.bad_integer,
            row.first[0],
            row.first[1],
            row.first[2],
            row.first[3],
        );
    }
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));

    // "No mismatch" means nothing if the operands never made rounding hard.
    // Over the default count every arithmetic operation must have been
    // rounded, and +, - and * must have met exact ties; every one must
    // have overflowed and produced zero; and * and / must have reached the
    // subnormal range with a rounding.
    assert_eq!(cover.len(), 4, "one coverage row per arithmetic operation");
    if n >= 100_000 {
        for (op, [inexact, ties, subnormal, overflow, zero]) in cover.iter().copied().enumerate() {
            let name = OPS[op];
            assert!(inexact > 10_000, "`{name}`: only {inexact} rounded results");
            assert!(overflow > 100, "`{name}`: only {overflow} overflows");
            assert!(zero > 100, "`{name}`: only {zero} zero results");
            if op != 3 {
                // A quotient is never exactly halfway; a sum or product can be.
                assert!(ties > 1_000, "`{name}`: only {ties} exact ties");
            }
            if op >= 2 {
                // A sum or difference of binary32 values is exact in the
                // subnormal range; a product or quotient is not.
                assert!(subnormal > 1_000, "`{name}`: only {subnormal} subnormal roundings");
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

#[test]
fn f32_operations_match_binary64_on_cranelift() {
    gate("cranelift");
}

#[test]
fn f32_operations_match_binary64_on_llvm() {
    gate("llvm");
}

/// Both backends count the same, which they would not if one rounded a
/// single pair differently from the other even where each agreed with its
/// own oracle.
#[test]
fn the_gate_reports_the_same_on_both_backends() {
    assert_eq!(gate("cranelift"), gate("llvm"));
}

fn build_and_run(tag: &str, relative: &str, backend: &str) -> std::process::Output {
    let dir = scratch(tag);
    let exe = dir.join("out");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join(relative))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(
        build.status.success(),
        "`{relative}` should build on {backend}:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&exe).output().expect("the program runs");
    let _ = std::fs::remove_dir_all(&dir);
    run
}

/// `tests/accept/f32.ls` on both backends, byte for byte the same and
/// equal to the header's own lines (the accept walker runs only the
/// default backend).
#[test]
fn the_f32_fixture_agrees_on_both_backends() {
    let source = std::fs::read_to_string(repo_root().join("tests/accept/f32.ls"))
        .expect("the fixture exists");
    let mut expected: String = source
        .lines()
        .filter_map(|line| line.strip_prefix("//~ STDOUT "))
        .collect::<Vec<_>>()
        .join("\n");
    expected.push('\n');
    for backend in ["cranelift", "llvm"] {
        let run = build_and_run(&format!("f32-fixture-{backend}"), "tests/accept/f32.ls", backend);
        assert_eq!(run.status.code(), Some(0), "{}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "on {backend}");
    }
}

/// The type's size, as the layout report says it (`docs/f32.md` §2): a
/// slice of `f32` is four bytes an element, and a struct is still eight
/// per leaf, which is `layout.md` §2's packing and not built here.
#[test]
fn the_layout_report_says_what_an_f32_costs() {
    let dir = scratch("f32-layout");
    let path = dir.join("layout.ls");
    std::fs::write(
        &path,
        "edition 6;\nstruct Sample { value: f32 }\nstruct Pair { a: f32, b: f32 }\n\
         fn main(world: World) -> [] int { release(world); return 0; }\n",
    )
    .expect("a writable fixture");
    let out = Command::new(BIN).arg("layout").arg(&path).output().expect("the compiler runs");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{text}{}", String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_dir_all(&dir);
    // `Sample` is one leaf: eight bytes in a struct, four in a slice, four
    // if its only leaf were packed.
    let sample = text.lines().find(|l| l.contains("Sample")).expect("a row for Sample");
    let columns: Vec<&str> = sample.split_whitespace().collect();
    assert_eq!(&columns[1..], ["1", "8", "4", "8"], "{text}");
    let pair = text.lines().find(|l| l.contains("Pair")).expect("a row for Pair");
    let columns: Vec<&str> = pair.split_whitespace().collect();
    assert_eq!(&columns[1..], ["2", "16", "8", "16"], "{text}");
}
