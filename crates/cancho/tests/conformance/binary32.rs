//! `f32` (`docs/f32.md`): F1's gate, and the fixture that shows the type.
//!
//! The gate is `tests/programs/f32_gate.cho`, built by each backend and run:
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
        .arg(repo_root().join("tests/programs/f32_gate.cho"))
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

// `who` names the calling test: tests run in parallel, and two of them building into
// one scratch directory overwrote each other's executable.
fn gate(who: &str, backend: &str) -> String {
    let n = pairs();
    let (dir, exe) = build_gate(&format!("f32-gate-{who}-{backend}"), backend);
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
    gate("alone", "cranelift");
}

#[test]
fn f32_operations_match_binary64_on_llvm() {
    gate("alone", "llvm");
}

/// Both backends count the same, which they would not if one rounded a
/// single pair differently from the other even where each agreed with its
/// own oracle.
#[test]
fn the_gate_reports_the_same_on_both_backends() {
    assert_eq!(gate("both", "cranelift"), gate("both", "llvm"));
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

/// `tests/accept/f32.cho` on both backends, byte for byte the same and
/// equal to the header's own lines (the accept walker runs only the
/// default backend).
#[test]
fn the_f32_fixture_agrees_on_both_backends() {
    let source = std::fs::read_to_string(repo_root().join("tests/accept/f32.cho"))
        .expect("the fixture exists");
    let mut expected: String = source
        .lines()
        .filter_map(|line| line.strip_prefix("//~ STDOUT "))
        .collect::<Vec<_>>()
        .join("\n");
    expected.push('\n');
    for backend in ["cranelift", "llvm"] {
        let run = build_and_run(&format!("f32-fixture-{backend}"), "tests/accept/f32.cho", backend);
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
    let path = dir.join("layout.cho");
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

// ---------------------------------------------------------------------
// F2 (`docs/f32.md` §5.2): `sqrt32`, `f32_of_int`, `int_of_f32`.
// ---------------------------------------------------------------------

/// Random cases per operation, on top of the special values and the dense
/// ranges the program lists. `LEX_F32_CONVERT_CASES` overrides it.
const DEFAULT_CONVERT_CASES: u64 = 1_000_000;

struct Conversion {
    name: String,
    cases: u64,
    bad_integer: u64,
    bad_second: u64,
    first: [u64; 3],
    cover: [u64; 5],
}

fn conversions(text: &str) -> Vec<Conversion> {
    text.lines()
        .map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            let n: Vec<u64> = words[1..].iter().map(|w| w.parse().expect("a count")).collect();
            assert_eq!(n.len(), 13, "a conversion row has thirteen numbers: `{line}`");
            Conversion {
                name: words[0].to_owned(),
                cases: n[0],
                bad_integer: n[1],
                bad_second: n[2],
                first: [n[4], n[5], n[6]],
                cover: [n[8], n[9], n[10], n[11], n[12]],
            }
        })
        .collect()
}

fn convert_gate(who: &str, backend: &str) -> String {
    let n = std::env::var("LEX_F32_CONVERT_CASES")
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(DEFAULT_CONVERT_CASES);
    let dir = scratch(&format!("f32-convert-{who}-{backend}"));
    let exe = dir.join("f32_convert_gate");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/f32_convert_gate.cho"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let started = Instant::now();
    let out = Command::new(&exe).arg(n.to_string()).output().expect("the gate runs");
    println!(
        "f32 conversion gate on {backend}: {n} random cases each, ran in {:?}",
        started.elapsed()
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let rows = conversions(&stdout);
    assert_eq!(rows.len(), 3, "one row per builtin:\n{stdout}");
    for row in &rows {
        // `f32_of_int`'s second column is the route that rounds twice, which
        // is counted and not required (the program's header): it is the
        // reason the builtin exists.
        let second_matters = row.name != "f32_of_int";
        assert!(
            row.bad_integer == 0 && (!second_matters || row.bad_second == 0),
            "{} on {backend}: {} results differ from the integer-arithmetic oracle and {} from the \
             binary64 route; the first input was {:#x}, got {:#x}, expected {:#x}",
            row.name,
            row.bad_integer,
            row.bad_second,
            row.first[0],
            row.first[1],
            row.first[2],
        );
    }
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    if n >= 100_000 {
        // "No mismatch" is worth something only if the inputs made rounding
        // hard. A square root is never exactly halfway between two
        // binary32 values (its root would have 13 bits), so it has inexact
        // results, round-ups and odd exponents instead of ties; an integer
        // has all of them.
        let sqrt = &rows[0];
        assert_eq!(sqrt.name, "sqrt32");
        assert!(sqrt.cases > n, "{} sqrt cases", sqrt.cases);
        for (what, count) in [
            ("inexact", sqrt.cover[0]),
            ("odd exponent", sqrt.cover[1]),
            ("round up", sqrt.cover[3]),
        ] {
            assert!(count > 10_000, "sqrt32 coverage: only {count} {what}");
        }
        assert!(sqrt.cover[2] > 1_000, "sqrt32 coverage: only {} subnormal inputs", sqrt.cover[2]);
        let int = &rows[1];
        assert_eq!(int.name, "f32_of_int");
        for (what, count) in [("inexact", int.cover[0]), ("exact ties", int.cover[1])] {
            assert!(count > 10_000, "f32_of_int coverage: only {count} {what}");
        }
        assert!(int.cover[2] > 100, "f32_of_int coverage: only {} carries", int.cover[2]);
        assert!(int.cover[3] >= 1, "f32_of_int never saw -2^63");
        assert!(int.bad_second > 0, "binary64 would have been enough: the oracle's routes agree");
        assert_eq!(rows[2].name, "int_of_f32");
        assert!(rows[2].cases > n);
        let _ = rows[2].first;
    }
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

#[test]
fn f32_sqrt_and_int_conversions_match_integer_oracles_on_cranelift() {
    convert_gate("alone", "cranelift");
}

#[test]
fn f32_sqrt_and_int_conversions_match_integer_oracles_on_llvm() {
    convert_gate("alone", "llvm");
}

#[test]
fn the_conversion_gate_reports_the_same_on_both_backends() {
    assert_eq!(convert_gate("both", "cranelift"), convert_gate("both", "llvm"));
}

/// `int_of_f32` is `truncate` at the narrower width (`floating-point.md`
/// §4): it stops the process on NaN, either infinity, and any magnitude at
/// or beyond 2^63 -- which includes exactly -2^63, as `truncate`'s does --
/// and answers toward zero everywhere else, on both backends.
#[test]
fn int_of_f32_traps_where_truncate_does() {
    // (bits, expected value or None for a trap)
    let cases: [(u32, Option<i64>); 15] = [
        (0x7fc0_0000, None),                        // NaN
        (0xffc0_0001, None),                        // a negative NaN with a payload
        (0x7f80_0000, None),                        // +inf
        (0xff80_0000, None),                        // -inf
        (0x5f00_0000, None),                        // 2^63
        (0xdf00_0000, None),                        // -2^63
        (0x7f7f_ffff, None),                        // the largest f32
        (0x5eff_ffff, Some(0x7fff_ff80_0000_0000)), // 2^63 - 2^39, the largest below
        (0xdeff_ffff, Some(-0x7fff_ff80_0000_0000)),
        (0x0000_0000, Some(0)),
        (0x8000_0000, Some(0)), // -0
        (0x3f7f_ffff, Some(0)), // 1 - 2^-24
        (0xbf7f_ffff, Some(0)),
        (0xc02c_cccd, Some(-2)),         // -2.7
        (0x4b80_0001, Some(16_777_218)), // 2^24 + 2
    ];
    for backend in ["cranelift", "llvm"] {
        for (index, (bits, expected)) in cases.iter().enumerate() {
            // `0 - n` would trap at i64::MIN, so the comparison is made
            // with the value as given.
            let want = expected.unwrap_or(0);
            let source = format!(
                "edition 6;\n\
                 fn main(world: World) -> [] int {{\n\
                 \x20   let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);\n\
                 \x20   release(args); release(heap); release(fs); release(ffi); release(io);\n\
                 \x20   release(net); release(clock); release(signals);\n\
                 \x20   let seen = int_of_f32(f32_of_bits({bits}));\n\
                 \x20   if seen != {want} {{ return 1; }}\n\
                 \x20   return 0;\n\
                 }}\n"
            );
            let dir = scratch(&format!("f32-int-of-{backend}-{index}"));
            let path = dir.join("t.cho");
            std::fs::write(&path, &source).expect("a writable fixture");
            let exe = dir.join("t");
            let build = Command::new(BIN)
                .args(["build", "--backend", backend])
                .arg(&path)
                .arg("-o")
                .arg(&exe)
                .output()
                .expect("the compiler runs");
            assert!(
                build.status.success(),
                "`int_of_f32({bits:#x})` should compile on {backend}: {}",
                String::from_utf8_lossy(&build.stderr)
            );
            let run = Command::new(&exe).output().expect("the program runs");
            if expected.is_none() {
                assert_eq!(
                    run.status.code(),
                    None,
                    "`int_of_f32({bits:#x})` should be killed by a signal on {backend}"
                );
            } else {
                assert_eq!(
                    run.status.code(),
                    Some(0),
                    "`int_of_f32({bits:#x})` should answer {want} without trapping on {backend}"
                );
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
