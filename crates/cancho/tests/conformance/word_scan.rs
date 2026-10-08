//! `docs/word-scan.md`: `load_le64`, `byte_mask64`, `trailing_zeros`, `leading_zeros` and `popcount`, on both backends.
//!
//! * a differential test against Rust as the oracle: a program fills two buffers from a seeded generator, folds every result
//!   the primitives give at every start offset, length and `at`, and prints one line per (buffer, start, length); the test
//!   computes the same lines with `u64::from_le_bytes`, `trailing_zeros`, `leading_zeros` and `count_ones`;
//! * the traps: each bound, both sides, and the calls at the edge that must not trap;

use super::*;

/// The program. `SEED` is the generator's seed. Prints `buffer start length hash_words hash_masks hash_counts` per line.
const PROGRAM: &str = r#"edition 8;

import std.io;

fn fill[&b](xs: &!b [byte], seed: int, few: bool) -> [] int {
    var x = seed;
    var i = 0;
    while i < len(xs) {
        x = wrapping_add(wrapping_mul(x, 6364136223846793005), 1442695040888963407);
        var r = (x >> 33) & 255;
        if few {
            r = 255;
            let pick = ((x >> 33) & 255) % 5;
            if pick == 0 { r = 44; }
            if pick == 1 { r = 34; }
            if pick == 2 { r = 10; }
            if pick == 3 { r = 0; }
        }
        xs[i] = byte_of(r);
        i = i + 1;
    }
    return 0;
}

fn mix(h: int, v: int) -> [] int {
    return wrapping_add(wrapping_mul(h, 1099511628211), v);
}

fn needle(which: int) -> [] int {
    var b = 0;
    if which == 1 { b = 44; }
    if which == 2 { b = 34; }
    if which == 3 { b = 10; }
    if which == 4 { b = 255; }
    if which == 5 { b = 128; }
    if which == 6 { b = 127; }
    if which == 7 { b = 1; }
    return b;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(signals);
    release(exec);
    release(heap);
    region a {
        var which = 0;
        while which < 2 {
            let buf = alloc_slice[a](400, byte_of(0));
            fill(buf, SEED + which, which == 1);
            var start = 0;
            while start < 10 {
                var length = 0;
                while length <= 140 {
                    let text = buf[start..start + length];
                    var hw = 0;
                    var hm = 0;
                    var hc = 0;
                    var at = 0;
                    while at + 8 <= length {
                        let w = load_le64(text, at);
                        hw = mix(hw, w);
                        hc = mix(mix(mix(hc, trailing_zeros(w)), leading_zeros(w)), popcount(w));
                        at = at + 1;
                    }
                    at = 0;
                    while at + 64 <= length {
                        var n = 0;
                        while n < 8 {
                            hm = mix(hm, byte_mask64(text, at, byte_of(needle(n))));
                            n = n + 1;
                        }
                        at = at + 1;
                    }
                    borrow mut io as &!i in {
                        io.print_int(i, which);
                        io.space(i);
                        io.print_int(i, start);
                        io.space(i);
                        io.print_int(i, length);
                        io.space(i);
                        io.print_int(i, hw);
                        io.space(i);
                        io.print_int(i, hm);
                        io.space(i);
                        io.print_int(i, hc);
                        io.newline(i);
                    }
                    length = length + 1;
                }
                start = start + 1;
            }
            which = which + 1;
        }
    }
    release(io);
    return 0;
}
"#;

fn mix(h: i64, v: i64) -> i64 {
    h.wrapping_mul(1099511628211).wrapping_add(v)
}

fn needle(which: usize) -> u8 {
    [0, 44, 34, 10, 255, 128, 127, 1][which]
}

/// The generator of the program above, byte for byte.
fn buffer(seed: i64, few: bool) -> Vec<u8> {
    let mut x = seed;
    (0..400)
        .map(|_| {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let r = ((x >> 33) & 255) as u8;
            if few { [44, 34, 10, 0, 255][(r % 5) as usize] } else { r }
        })
        .collect()
}

/// The program's whole output, computed by Rust.
fn expected(seed: i64) -> String {
    let mut out = String::new();
    for which in 0..2 {
        let buf = buffer(seed + which, which == 1);
        for start in 0..10usize {
            for length in 0..=140usize {
                let text = &buf[start..start + length];
                let (mut hw, mut hm, mut hc) = (0i64, 0i64, 0i64);
                for at in 0..(length + 1).saturating_sub(8) {
                    let w = u64::from_le_bytes(text[at..at + 8].try_into().unwrap());
                    hw = mix(hw, w as i64);
                    hc = mix(hc, i64::from(w.trailing_zeros()));
                    hc = mix(hc, i64::from(w.leading_zeros()));
                    hc = mix(hc, i64::from(w.count_ones()));
                }
                for at in 0..(length + 1).saturating_sub(64) {
                    for n in 0..8 {
                        let mask = text[at..at + 64]
                            .iter()
                            .enumerate()
                            .fold(0u64, |m, (k, b)| m | (u64::from(*b == needle(n)) << k));
                        hm = mix(hm, mask as i64);
                    }
                }
                out.push_str(&format!("{which} {start} {length} {hw} {hm} {hc}\n"));
            }
        }
    }
    out
}

fn build(dir: &Path, source: &str, backend: &str) -> PathBuf {
    let path = dir.join("program.cho");
    std::fs::write(&path, source).expect("a writable fixture");
    let exe = dir.join(format!("program-{backend}"));
    let build = Command::new(BIN)
        .args([
            "build".as_ref(),
            "--std".as_ref(),
            path.as_os_str(),
            "--backend".as_ref(),
            backend.as_ref(),
            "-o".as_ref(),
            exe.as_os_str(),
        ])
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    exe
}

#[test]
fn the_primitives_agree_with_rust_over_random_buffers_and_every_offset_and_length() {
    for seed in [1i64, 0x5eed_cafe_f00d, -987654321987654321] {
        let want = expected(seed);
        for backend in ["cranelift", "llvm"] {
            let dir = scratch(&format!("word-scan-diff-{backend}-{}", seed.unsigned_abs() % 1000));
            let exe = build(&dir, &PROGRAM.replace("SEED", &format!("({seed})")), backend);
            let run = Command::new(&exe).output().expect("the compiled program runs");
            assert_eq!(run.status.code(), Some(0), "seed {seed} on {backend}");
            let got = String::from_utf8_lossy(&run.stdout);
            if got != want {
                let line = got.lines().zip(want.lines()).position(|(g, w)| g != w);
                panic!(
                    "seed {seed} on {backend} differs from Rust at line {line:?}:\n got {:?}\nwant {:?}",
                    line.map(|n| got.lines().nth(n)),
                    line.map(|n| want.lines().nth(n)),
                );
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

/// Every call below is on an 8- or 64-byte-or-longer sub-slice of a 100-byte buffer, so what is under test is the check
/// against the *slice's* end, not the allocation's.
#[test]
fn the_loads_and_masks_trap_outside_the_slice_and_only_there_on_both_backends() {
    // (name, call, must trap)
    let calls: &[(&str, &str, bool)] = &[
        ("load-first", "load_le64(t, 0)", false),
        ("load-last", "load_le64(t, len(t) - 8)", false),
        ("load-one-past-last", "load_le64(t, len(t) - 7)", true),
        ("load-at-the-end", "load_le64(t, len(t))", true),
        ("load-negative", "load_le64(t, 0 - 1)", true),
        ("load-most-negative", "load_le64(t, 0 - 9223372036854775807 - 1)", true),
        ("load-largest", "load_le64(t, 9223372036854775807)", true),
        ("load-from-a-short-slice", "load_le64(t[0..7], 0)", true),
        ("load-from-an-empty-slice", "load_le64(t[3..3], 0)", true),
        ("load-from-exactly-eight", "load_le64(t[3..11], 0)", false),
        ("mask-first", "byte_mask64(t, 0, byte_of(1))", false),
        ("mask-last", "byte_mask64(t, len(t) - 64, byte_of(1))", false),
        ("mask-one-past-last", "byte_mask64(t, len(t) - 63, byte_of(1))", true),
        ("mask-at-the-end", "byte_mask64(t, len(t), byte_of(1))", true),
        ("mask-negative", "byte_mask64(t, 0 - 1, byte_of(1))", true),
        ("mask-most-negative", "byte_mask64(t, 0 - 9223372036854775807 - 1, byte_of(1))", true),
        ("mask-largest", "byte_mask64(t, 9223372036854775807, byte_of(1))", true),
        ("mask-from-a-short-slice", "byte_mask64(t[0..63], 0, byte_of(1))", true),
        ("mask-from-an-empty-slice", "byte_mask64(t[3..3], 0, byte_of(1))", true),
        ("mask-from-exactly-sixty-four", "byte_mask64(t[3..67], 0, byte_of(1))", false),
    ];
    for backend in ["cranelift", "llvm"] {
        for (name, call, should_trap) in calls {
            let dir = scratch(&format!("word-scan-trap-{name}-{backend}"));
            let source = format!(
                "edition 8;\n\
                 fn main(world: World) -> [] int {{\n\
                     let Split {{ io, ffi, fs, heap, args, net, clock, signals, exec }} = split(world);\n\
                     release(args); release(heap); release(fs); release(ffi); release(io); release(net); release(clock); release(signals); release(exec);\n\
                     var n = 0;\n\
                     region a {{ let t = alloc_slice[a](100, byte_of(7)); n = {call}; }}\n\
                     return n - n;\n\
                 }}\n"
            );
            let exe = build(&dir, &source, backend);
            let run = Command::new(&exe).output().expect("the compiled program runs");
            if *should_trap {
                assert_eq!(
                    run.status.code(),
                    None,
                    "`{call}` on {backend} should be killed by a signal"
                );
            } else {
                assert_eq!(run.status.code(), Some(0), "`{call}` on {backend} should succeed");
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

/// The counts are total: no input reaches a trap, the extremes included.
#[test]
fn the_counts_answer_the_width_for_zero_and_never_trap_on_both_backends() {
    let source = "edition 8;\n\
        fn main(world: World) -> [] int {\n\
            let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n\
            release(args); release(heap); release(fs); release(ffi); release(io); release(net); release(clock); release(signals); release(exec);\n\
            let least = 0 - 9223372036854775807 - 1;\n\
            if trailing_zeros(0) != 64 || leading_zeros(0) != 64 || popcount(0) != 0 { return 1; }\n\
            if trailing_zeros(least) != 63 || leading_zeros(least) != 0 || popcount(least) != 1 { return 2; }\n\
            if trailing_zeros(9223372036854775807) != 0 || leading_zeros(9223372036854775807) != 1 || popcount(9223372036854775807) != 63 { return 3; }\n\
            if trailing_zeros(0 - 1) != 0 || leading_zeros(0 - 1) != 0 || popcount(0 - 1) != 64 { return 4; }\n\
            return 0;\n\
        }\n";
    for backend in ["cranelift", "llvm"] {
        let dir = scratch(&format!("word-scan-counts-{backend}"));
        let exe = build(&dir, source, backend);
        let run = Command::new(&exe).output().expect("the compiled program runs");
        assert_eq!(run.status.code(), Some(0), "{backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
