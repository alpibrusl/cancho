//! `mul_wide`, `add_carry` and `sub_borrow` (`docs/wide-multiply.md` §6),
//! through `tests/programs/wide_multiply_driver.cho`, on both backends, against
//! Rust's `u128` as the oracle:
//!
//! - every pair of operand edges (0, 1, 2, around 2^32, around 2^63, 2^64 - 1,
//!   alternating bits) with each kind of carry-in, line by line;
//! - a ripple of carries through eight words;
//! - millions of random cases, edge-biased, whose answers the driver folds into
//!   one checksum that this file recomputes from the same generator.

use super::json::feed;
use super::*;

fn build_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("wide-multiply-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/wide_multiply_driver.cho"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

fn mul(a: u64, b: u64) -> (u64, u64) {
    let p = u128::from(a) * u128::from(b);
    ((p >> 64) as u64, p as u64)
}

fn add(a: u64, b: u64, carry: u64) -> (u64, u64) {
    let s = u128::from(a) + u128::from(b) + u128::from(carry != 0);
    (s as u64, (s >> 64) as u64)
}

fn sub(a: u64, b: u64, borrow: u64) -> (u64, u64) {
    let need = u128::from(b) + u128::from(borrow != 0);
    (a.wrapping_sub(b).wrapping_sub(u64::from(borrow != 0)), u64::from(u128::from(a) < need))
}

fn pair((x, y): (u64, u64)) -> String {
    format!("{x:016x} {y:016x}")
}

const EDGES: [u64; 15] = [
    0,
    1,
    2,
    0xffff_ffff,
    1 << 32,
    (1 << 32) + 1,
    0xffff_ffff_0000_0000,
    (1 << 63) - 1,
    1 << 63,
    (1 << 63) + 1,
    u64::MAX - 1,
    u64::MAX,
    0x5555_5555_5555_5555,
    0xaaaa_aaaa_aaaa_aaaa,
    0x0000_0001_ffff_ffff,
];

/// The driver's generator (`random_cases`), word for word, with `u64` for its
/// two's-complement `int`.
fn random_checksum(seed: u64, count: u64) -> u64 {
    let step =
        |x: u64| x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    let edge = |sel: u64, word: u64| match sel {
        0 => 0,
        1 => u64::MAX,
        2 => 1 << 63,
        3 => (1 << 63) - 1,
        4 => 1 << 32,
        5 => 0xffff_ffff,
        6 => 1,
        7 => 0xffff_ffff_0000_0000,
        _ => word,
    };
    let mix = |acc: u64, v: u64| (acc ^ v).wrapping_mul(1_099_511_628_211);
    let mut x = seed;
    let mut acc = (-3_750_763_034_362_895_579_i64) as u64;
    for _ in 0..count {
        let mut w = [0u64; 3];
        for slot in &mut w {
            x = step(x);
            let (sel, hi) = ((x >> 40) & 15, (x >> 32) & 0xffff_ffff);
            x = step(x);
            *slot = edge(sel, (hi << 32) | ((x >> 32) & 0xffff_ffff));
        }
        let (a, b, c) = (w[0], w[1], w[2]);
        let (mh, ml) = mul(a, b);
        let (s, co) = add(a, b, c);
        let (d, bo) = sub(a, b, c);
        acc = mix(mix(mix(acc, mh), ml), mix(mix(s, co), mix(d, bo)));
    }
    acc
}

fn check_backend(backend: &str) {
    let (dir, exe) = build_driver("agree", backend);
    let mut lines = Vec::new();
    let mut want = Vec::new();
    for &a in &EDGES {
        for &b in &EDGES {
            lines.push(format!("M {a:016x} {b:016x}"));
            want.push(pair(mul(a, b)));
            for c in [0u64, 1, 2, u64::MAX] {
                lines.push(format!("A {a:016x} {b:016x} {c:016x}"));
                want.push(pair(add(a, b, c)));
                lines.push(format!("S {a:016x} {b:016x} {c:016x}"));
                want.push(pair(sub(a, b, c)));
            }
        }
    }
    // A ripple: 2^512 - 1 plus 1, one `add_carry` a word, each carry-in the
    // previous carry-out; then the same number minus 1 back down.
    let mut carry = 1u64;
    for _ in 0..8 {
        lines.push(format!("A {:016x} {:016x} {carry:016x}", u64::MAX, 0));
        let (s, c) = add(u64::MAX, 0, carry);
        want.push(pair((s, c)));
        carry = c;
    }
    assert_eq!(carry, 1, "the oracle's own ripple leaves the top carry set");
    let mut borrow = 1u64;
    for _ in 0..8 {
        lines.push(format!("S {:016x} {:016x} {borrow:016x}", 0, 0));
        let (d, bo) = sub(0, 0, borrow);
        want.push(pair((d, bo)));
        borrow = bo;
    }
    assert_eq!(borrow, 1);
    // Known answers, from the definition and not from the oracle's code.
    lines.push("M ffffffffffffffff ffffffffffffffff".into());
    want.push("fffffffffffffffe 0000000000000001".into());
    lines.push("M 8000000000000000 0000000000000002".into());
    want.push("0000000000000001 0000000000000000".into());
    lines.push("A ffffffffffffffff ffffffffffffffff 0000000000000001".into());
    want.push("ffffffffffffffff 0000000000000001".into());
    lines.push("S 0000000000000000 0000000000000000 0000000000000001".into());
    want.push("ffffffffffffffff 0000000000000001".into());

    let seeds: [u64; 3] = [0x9e37_79b9_7f4a_7c15, 1, 0xdead_beef_cafe_f00d];
    let count = 2_000_000u64;
    for seed in seeds {
        lines.push(format!("R {seed:016x} {count}"));
        want.push(format!("{:016x}", random_checksum(seed, count)));
    }

    let mut input = lines.join("\n");
    input.push('\n');
    let out = feed(&exe, input.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let got: Vec<String> =
        String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    assert_eq!(got.len(), want.len(), "one answer per case");
    for (k, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "case {k}: {}", lines[k]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_wide_builtins_agree_with_u128_on_llvm() {
    check_backend("llvm");
}

#[test]
fn the_wide_builtins_agree_with_u128_on_cranelift() {
    check_backend("cranelift");
}

/// The two backends and the oracle agree about the one rule a caller could
/// get wrong: a carry-in is "nonzero", not "one".
#[test]
fn a_carry_in_other_than_zero_or_one_counts_as_one() {
    assert_eq!(add(5, 6, 2), (12, 0));
    assert_eq!(add(u64::MAX, 0, u64::MAX), (0, 1));
    assert_eq!(sub(5, 6, 7), (u64::MAX - 1, 1));
}

/// `wasm32-wasip1` (`docs/wide-multiply.md` §4): the product there is four 32-bit
/// multiplies, not a call to `__multi3`, which the sysroot does not ship. Needs
/// the wasm toolchain `docs/wasm.md` names (`CLANG` with the wasm32 target,
/// `WASM_LD`, `WASI_SYSROOT`, `WASMTIME`), which CI is not assumed to have, so it
/// says it did not run when they are not set.
#[test]
fn on_wasm32_the_wide_builtins_link_and_give_the_native_checksum() {
    let tools = ["CLANG", "WASM_LD", "WASI_SYSROOT", "WASMTIME"];
    if tools.iter().any(|t| std::env::var_os(t).is_none()) {
        eprintln!("skipped: {tools:?} are not all set");
        return;
    }
    let dir = scratch("wide-multiply-wasm");
    let wasm = dir.join("driver.wasm");
    let build = Command::new(BIN)
        .args(["build", "--std", "--target", "wasm32-wasip1"])
        .arg(repo_root().join("tests/programs/wide_multiply_driver.cho"))
        .arg("-o")
        .arg(&wasm)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let seed = 0x9e37_79b9_7f4a_7c15u64;
    let input = format!("R {seed:016x} 300000\nM ffffffffffffffff ffffffffffffffff\n");
    let mut child = Command::new(std::env::var_os("WASMTIME").expect("set"))
        .arg(&wasm)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("wasmtime runs");
    child.stdin.take().expect("piped").write_all(input.as_bytes()).expect("written");
    let out = child.wait_with_output().expect("wasmtime finishes");
    let got = String::from_utf8_lossy(&out.stdout).into_owned();
    let want =
        format!("{:016x}\nfffffffffffffffe 0000000000000001\n", random_checksum(seed, 300_000));
    assert_eq!(got, want);
    let _ = std::fs::remove_dir_all(&dir);
}
