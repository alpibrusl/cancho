//! `std.math.sin`/`cos` against the C library (`docs/float-math.md` §8).

use super::*;

/// The distance between two floats in units in the last place: how many
/// representable floats lie between them. Sign-magnitude bits are mapped
/// onto a line where adjacent floats are adjacent integers, and `-0.0` and
/// `+0.0` are the same point.
fn ulps(a: f64, b: f64) -> u64 {
    fn key(x: f64) -> i64 {
        let bits = x.to_bits() as i64;
        if bits < 0 { -(bits & i64::MAX) } else { bits }
    }
    (key(a) - key(b)).unsigned_abs()
}

/// The arguments `tests/programs/trig_samples.ls` replays, in the same order and
/// by the same operations: a linear-congruential state, scaled onto
/// `[centre - width, centre + width]`. Every step is exact or correctly
/// rounded, so this and the program compute the same float bit for bit.
fn arguments(centre: f64, width: f64, count: usize) -> Vec<f64> {
    let mut state: i64 = 12345;
    (0..count)
        .map(|_| {
            state = (state * 1103515245 + 12345) % 2147483648;
            let unit = state as f64 / 2147483648.0;
            centre + (2.0 * unit - 1.0) * width
        })
        .collect()
}

/// `(label, centre, width, bound)` of each range the program sweeps. The
/// bound is the most the answer may differ from libm's, in ulps: the worst
/// case *measured* against glibc's `sin`/`cos` (1, 1, 1, 2, 1, 1 -- §8)
/// plus one, because the reference is itself a library, and another
/// platform's rounds differently from glibc's in the last place.
const RANGES: &[(&str, f64, f64, u64)] = &[
    ("[-1, 1]", 0.0, 1.0, 2),
    ("[-10, 10]", 0.0, 10.0, 2),
    ("[-1e3, 1e3]", 0.0, 1000.0, 2),
    ("[-1e6, 1e6]", 0.0, 1_000_000.0, 3),
    ("near 100 * pi/2", 157.07963267948966, 0.001, 2),
    ("near 100000 * pi/2", 157079.63267948966, 0.001, 2),
];

const SAMPLES: usize = 20000;

#[test]
fn sin_and_cos_agree_with_libm_within_stated_accuracy() {
    let out = Command::new(BIN)
        .args(["run", "--std"])
        .arg(repo_root().join("tests/programs/trig_samples.ls"))
        .output()
        .expect("the compiler runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), RANGES.len() * SAMPLES, "one line per argument");

    for (r, (label, centre, width, bound)) in RANGES.iter().enumerate() {
        let xs = arguments(*centre, *width, SAMPLES);
        let (mut worst, mut differ) = (0u64, 0usize);
        for (k, x) in xs.iter().enumerate() {
            let mut it = lines[r * SAMPLES + k].split(' ');
            let mut bits = || f64::from_bits(it.next().unwrap().parse::<i64>().unwrap() as u64);
            let (s, c) = (bits(), bits());
            for (got, want, what) in [(s, x.sin(), "sin"), (c, x.cos(), "cos")] {
                let d = ulps(got, want);
                if d > 0 {
                    differ += 1;
                }
                worst = worst.max(d);
                assert!(
                    d <= *bound,
                    "{label}: {what}({x:e}) = {got:e}, libm says {want:e} ({d} ulp)"
                );
            }
        }
        eprintln!("{label}: worst {worst} ulp, {differ} of {} differ", 2 * SAMPLES);
    }
}

#[test]
fn sin_and_cos_trap_outside_their_domain_rather_than_answering_wrongly() {
    // `|x| <= 1e6` is where argument reduction is exact (`std/math.ls`); past
    // it there is a right answer this does not compute, so it stops
    // (`docs/defined-behaviour.md` §2.1), the same way every other check
    // here does. A NaN is not out of range and is answered, not trapped
    // (`tests/accept/math_floats.ls`).
    for (tag, expr) in [
        ("sin-large", "math.sin(2000000.0)"),
        ("cos-large", "math.cos(0.0 - 1.0e7)"),
        ("sin-inf", "math.sin(1.0 / 0.0)"),
    ] {
        let dir = scratch(&format!("trig-trap-{tag}"));
        let source = dir.join("trap.ls");
        std::fs::write(
            &source,
            format!(
                "import std.math;\n\n\
                 fn main(world: World) -> [] int {{\n\
                     let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
                     release(args);\n    release(heap);\n    release(fs);\n    release(ffi);\n    release(io);\n\
                     return truncate({expr});\n\
                 }}\n"
            ),
        )
        .expect("a writable fixture");
        let exe = dir.join("trap");
        let build = Command::new(BIN)
            .args(["build", "--std"])
            .arg(&source)
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("the compiler runs");
        assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), None, "{expr} should be killed by a signal, not exit");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
