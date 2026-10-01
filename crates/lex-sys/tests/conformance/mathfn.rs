//! `std.math`'s transcendental functions against the C library
//! (`docs/float-math.md` §9).

use super::*;

// The C library's own functions, called directly rather than through Rust's
// `f64` methods: `asinh`, `acosh` and `atanh` are written in Rust there, so
// they would be a second implementation to compare against, not the library.
unsafe extern "C" {
    safe fn sin(x: f64) -> f64;
    safe fn cos(x: f64) -> f64;
    safe fn exp(x: f64) -> f64;
    safe fn log(x: f64) -> f64;
    safe fn expm1(x: f64) -> f64;
    safe fn log1p(x: f64) -> f64;
    safe fn log2(x: f64) -> f64;
    safe fn log10(x: f64) -> f64;
    safe fn sinh(x: f64) -> f64;
    safe fn cosh(x: f64) -> f64;
    safe fn tanh(x: f64) -> f64;
    safe fn asinh(x: f64) -> f64;
    safe fn acosh(x: f64) -> f64;
    safe fn atanh(x: f64) -> f64;
    safe fn pow(x: f64, y: f64) -> f64;
}

/// The distance between two floats in units in the last place: how many
/// representable floats lie between them. Sign-magnitude bits are mapped
/// onto a line where adjacent floats are adjacent integers, so `-0.0` and
/// `+0.0` are one point, and two NaNs, or two equal infinities, are no
/// distance apart. A NaN against a number is as far as it gets.
fn ulps(a: f64, b: f64) -> u64 {
    fn key(x: f64) -> i64 {
        let bits = x.to_bits() as i64;
        if bits < 0 { -(bits & i64::MAX) } else { bits }
    }
    if a.is_nan() && b.is_nan() {
        return 0;
    }
    if a.is_nan() || b.is_nan() {
        return u64::MAX;
    }
    (key(a) - key(b)).unsigned_abs()
}

/// `(centre, width)` of each range `tests/programs/math_samples.ls` knows, by
/// id. Repeated from there, on purpose: a copy that drifted would show as an
/// enormous error, not a quiet one, because the arguments would differ.
const RANGES: [(f64, f64); 20] = [
    (0.0, 1.0),
    (0.0, 10.0),
    (0.0, 1000.0),
    (0.0, 1000000.0),
    (157.07963267948966, 0.001),
    (157079.63267948966, 0.001),
    (1.0, 0.5),
    (50.5, 49.5),
    (500000.0, 500000.0),
    (0.0, 700.0),
    (0.0, 30.0),
    (0.0, 0.001),
    (1.0, 0.001),
    (2.0, 1.0),
    (0.0, 0.999),
    (0.0, 0.000000001),
    (0.0005, 0.0005),
    (-720.0, 25.0),
    (355.0, 355.0),
    (5.0e299, 5.0e299),
];

const SAMPLES: usize = 10000;

/// The arguments the program replays: a linear-congruential state scaled onto
/// `[centre - width, centre + width]`, by the same operations in the same
/// order. Every step is exact or correctly rounded, so this and the program
/// compute the same float bit for bit.
fn arguments(range: usize) -> Vec<f64> {
    let (centre, width) = RANGES[range];
    let mut state: i64 = 12345;
    (0..SAMPLES)
        .map(|_| {
            state = (state * 1103515245 + 12345) % 2147483648;
            let unit = state as f64 / 2147483648.0;
            centre + (2.0 * unit - 1.0) * width
        })
        .collect()
}

/// One function: its name, its id in `math_samples.ls`, the C library's
/// answer, and the ranges to sweep with the most the answer may differ, in
/// ulps. The bounds are the **measured** worst case against glibc plus one --
/// the reference is itself a library, and another platform's rounds
/// differently in the last place -- and §9 has the measurements.
struct Function {
    name: &'static str,
    id: u32,
    reference: fn(f64) -> f64,
    ranges: &'static [(usize, u64)],
}

/// `fn name_(x) -> f64 { name(x) }` for each C function: a Rust-ABI pointer
/// the table can hold, and the same call.
macro_rules! wrap {
    ($($name:ident => $wrapper:ident),* $(,)?) => {
        $(fn $wrapper(x: f64) -> f64 { $name(x) })*
    };
}

wrap! {
    sin => w_sin, cos => w_cos, exp => w_exp, log => w_log, expm1 => w_expm1,
    log1p => w_log1p, log2 => w_log2, log10 => w_log10, sinh => w_sinh,
    cosh => w_cosh, tanh => w_tanh, asinh => w_asinh, acosh => w_acosh,
    atanh => w_atanh,
}

fn w_pow_37(x: f64) -> f64 {
    pow(x, 3.7)
}
fn w_pow_03(x: f64) -> f64 {
    pow(x, 0.3)
}
fn w_pow_base(x: f64) -> f64 {
    pow(1.7, x)
}
fn w_pow_2(x: f64) -> f64 {
    pow(x, 2.0)
}
fn w_pow_7(x: f64) -> f64 {
    pow(x, 7.0)
}
fn w_pow_m3(x: f64) -> f64 {
    pow(x, -3.0)
}

const FUNCTIONS: &[Function] = &[
    Function {
        name: "sin",
        id: 0,
        reference: w_sin,
        ranges: &[(0, 2), (1, 2), (2, 2), (3, 3), (4, 2), (5, 2)],
    },
    Function {
        name: "cos",
        id: 1,
        reference: w_cos,
        ranges: &[(0, 2), (1, 2), (2, 2), (3, 3), (4, 2), (5, 2)],
    },
    Function {
        name: "exp",
        id: 2,
        reference: w_exp,
        ranges: &[(0, 2), (1, 2), (9, 2), (11, 2), (15, 2), (17, 2)],
    },
    Function {
        name: "log",
        id: 3,
        reference: w_log,
        ranges: &[(6, 2), (7, 2), (8, 2), (12, 2), (16, 2), (19, 2)],
    },
    Function {
        name: "expm1",
        id: 4,
        reference: w_expm1,
        ranges: &[(0, 3), (1, 3), (10, 3), (11, 3), (15, 3)],
    },
    Function {
        name: "log1p",
        id: 5,
        reference: w_log1p,
        ranges: &[(14, 3), (16, 3), (15, 3), (7, 3), (8, 3)],
    },
    Function { name: "log2", id: 6, reference: w_log2, ranges: &[(6, 2), (7, 2), (8, 2), (19, 2)] },
    Function {
        name: "log10",
        id: 7,
        reference: w_log10,
        ranges: &[(6, 3), (7, 3), (8, 3), (19, 3)],
    },
    Function {
        name: "sinh",
        id: 8,
        reference: w_sinh,
        ranges: &[(0, 4), (1, 4), (10, 4), (11, 4), (15, 4), (18, 4)],
    },
    Function {
        name: "cosh",
        id: 9,
        reference: w_cosh,
        ranges: &[(0, 3), (1, 3), (10, 3), (11, 3), (18, 3)],
    },
    Function {
        name: "tanh",
        id: 10,
        reference: w_tanh,
        ranges: &[(0, 4), (1, 4), (10, 4), (11, 4), (15, 4)],
    },
    Function {
        name: "asinh",
        id: 11,
        reference: w_asinh,
        ranges: &[(0, 3), (1, 3), (3, 3), (11, 3), (15, 3), (19, 3)],
    },
    Function {
        name: "acosh",
        id: 12,
        reference: w_acosh,
        ranges: &[(13, 3), (7, 3), (8, 3), (19, 3)],
    },
    Function {
        name: "atanh",
        id: 13,
        reference: w_atanh,
        ranges: &[(14, 3), (11, 3), (15, 3), (0, 3)],
    },
    Function {
        name: "pow(x, 3.7)",
        id: 14,
        reference: w_pow_37,
        ranges: &[(6, 3), (7, 3), (8, 3)],
    },
    Function {
        name: "pow(x, 0.3)",
        id: 15,
        reference: w_pow_03,
        ranges: &[(6, 3), (7, 3), (8, 3)],
    },
    Function {
        name: "pow(1.7, x)",
        id: 16,
        reference: w_pow_base,
        ranges: &[(0, 2), (9, 5), (11, 2)],
    },
    Function { name: "pow(x, 2)", id: 17, reference: w_pow_2, ranges: &[(6, 3), (7, 3), (8, 3)] },
    Function { name: "pow(x, 7)", id: 18, reference: w_pow_7, ranges: &[(6, 4), (7, 4)] },
    Function { name: "pow(x, -3)", id: 19, reference: w_pow_m3, ranges: &[(6, 3), (7, 3), (8, 3)] },
];

#[test]
fn the_transcendental_functions_agree_with_libm_within_stated_accuracy() {
    let dir = scratch("mathfn");
    let exe = dir.join("math_samples");
    let build = Command::new(BIN)
        .args(["build", "--std"])
        .arg(repo_root().join("tests/programs/math_samples.ls"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));

    let mut failures = Vec::new();
    for function in FUNCTIONS {
        for (range, bound) in function.ranges {
            let out = Command::new(&exe)
                .args([function.id.to_string(), range.to_string()])
                .output()
                .expect("the program runs");
            assert!(out.status.success(), "{} range {range} failed", function.name);
            let text = String::from_utf8_lossy(&out.stdout);
            let answers: Vec<f64> =
                text.lines().map(|l| f64::from_bits(l.parse::<i64>().unwrap() as u64)).collect();
            assert_eq!(answers.len(), SAMPLES);

            let (mut worst, mut differ, mut at) = (0u64, 0usize, 0.0);
            for (got, x) in answers.iter().zip(arguments(*range)) {
                let d = ulps(*got, (function.reference)(x));
                if d > 0 {
                    differ += 1;
                }
                if d > worst {
                    (worst, at) = (d, x);
                }
            }
            eprintln!(
                "{:>6} range {range:>2} {:?}: worst {worst} ulp (at {at:e}), {differ} of {SAMPLES} differ",
                function.name, RANGES[*range]
            );
            if worst > *bound {
                failures.push(format!(
                    "{}({at:e}) is {worst} ulp from libm's, over the bound of {bound}",
                    function.name
                ));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
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
