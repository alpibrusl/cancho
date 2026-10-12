//! Float text at the edges (`docs/json.md` §4, `docs/float-printing.md` §3.4,
//! `docs/floating-point.md` §4.1): `std.json.to_float` on a million strings
//! including every halfway case among the subnormals, `std.fmt.float_into`'s
//! rule for exact ties, and `float_of_bits` against `bits_of`.
//!
//! The oracle for reading is Rust's `str::parse::<f64>` (correctly rounded,
//! ties to even, as Python's `float()` is: `scripts/float_differential.py`
//! runs the same drivers against Python and exact rationals).

use super::json::{Lcg, build_driver, feed};

/// `m * 2^-shift` written out in full, in decimal: `m * 5^shift`, with the
/// point `shift` places from the right. `shift == 0` is an integer.
fn dyadic(m: u64, shift: u32) -> String {
    let mut limbs: Vec<u32> = vec![m as u32, (m >> 32) as u32];
    let mut left = shift;
    while left > 0 {
        let step = left.min(13);
        left -= step;
        let factor = 5u64.pow(step);
        let mut carry = 0u64;
        for limb in limbs.iter_mut() {
            let v = u64::from(*limb) * factor + carry;
            *limb = v as u32;
            carry = v >> 32;
        }
        if carry > 0 {
            limbs.push(carry as u32);
        }
    }
    let mut chunks: Vec<u32> = Vec::new();
    while limbs.iter().any(|&l| l != 0) {
        let mut rem = 0u64;
        for limb in limbs.iter_mut().rev() {
            let v = (rem << 32) | u64::from(*limb);
            *limb = (v / 1_000_000_000) as u32;
            rem = v % 1_000_000_000;
        }
        chunks.push(rem as u32);
    }
    let mut digits = match chunks.pop() {
        None => String::from("0"),
        Some(top) => top.to_string(),
    };
    for chunk in chunks.iter().rev() {
        digits.push_str(&format!("{chunk:09}"));
    }
    if shift == 0 {
        return digits;
    }
    let padded = format!("{digits:0>width$}", width = shift as usize + 1);
    let split = padded.len() - shift as usize;
    format!("{}.{}", &padded[..split], &padded[split..])
}

/// The exact halfway point between the doubles `bits` and `bits + 1`, written
/// out, and a hair above and below it. `bits` is below infinity's pattern.
fn midpoint_cells(bits: u64, out: &mut Vec<String>) {
    // A double is `m * 2^q` with m < 2^53 (subnormals: q = -1074); the
    // midpoint is `(2m + 1) * 2^(q - 1)`, or `(2m + 1) / 2^(1 - q)`.
    let (m, q) = if bits >> 52 == 0 {
        (bits, -1074i32)
    } else {
        ((bits & ((1 << 52) - 1)) | (1 << 52), (bits >> 52) as i32 - 1075)
    };
    let odd = 2 * m + 1;
    let mid = if q >= 1 {
        // integer: shift left by q - 1 via the decimal of `odd * 2^(q-1)`
        let mut text = odd.to_string();
        for _ in 0..q - 1 {
            text = double_decimal(&text);
        }
        text
    } else {
        dyadic(odd, (1 - q) as u32)
    };
    out.push(mid.clone());
    if mid.contains('.') {
        out.push(format!("{mid}1"));
        let mut below = mid.clone();
        assert_eq!(below.pop(), Some('5'));
        out.push(format!("{below}4999999999"));
        out.push(format!("{mid}000000000000000000001"));
    } else {
        out.push(format!("{mid}.00000000000000000001"));
    }
}

fn double_decimal(text: &str) -> String {
    let mut carry = 0;
    let mut out: Vec<u8> = Vec::with_capacity(text.len() + 1);
    for byte in text.bytes().rev() {
        let v = (byte - b'0') * 2 + carry;
        out.push(b'0' + v % 10);
        carry = v / 10;
    }
    if carry > 0 {
        out.push(b'0' + carry);
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

fn read_corpus(total: usize) -> Vec<String> {
    let mut rng = Lcg(0xf10a7);
    let mut cells: Vec<String> = Vec::with_capacity(total + 64);
    // The boundary below the smallest subnormal, 2^-1075, exactly, and a
    // hair either side: the halfway point of 0 and 2^-1074 rounds to zero
    // (even), anything above it to 2^-1074.
    for bits in 0..64u64 {
        midpoint_cells(bits, &mut cells);
    }
    // The largest subnormals and the smallest normals, whose halfway points
    // are the longest numbers there are (768 significant digits).
    for bits in 0x000f_ffff_ffff_ff00u64..0x0010_0000_0000_0100 {
        midpoint_cells(bits, &mut cells);
    }
    // Halfway points anywhere: subnormals as often as the rest, and the
    // overflow edge.
    for _ in 0..30_000 {
        let bits = match rng.below(4) {
            0 => rng.next() & 0x000f_ffff_ffff_ffff,
            1 => rng.next() & 0x000f_ffff_ffff_ffff | (rng.below(40)) << 52,
            _ => rng.next() & 0x7fef_ffff_ffff_ffff,
        };
        midpoint_cells(bits, &mut cells);
    }
    // Up to DBL_MAX plus half an ulp, exactly: a tie that goes to the even
    // neighbour, which is infinity.
    for bits in 0x7fef_ffff_ffff_fff0u64..=0x7fef_ffff_ffff_ffff {
        midpoint_cells(bits, &mut cells);
    }
    cells.push("1.7976931348623158e308".to_owned());
    cells.push("1.7976931348623159e308".to_owned());
    // The rest: every shape a document holds.
    while cells.len() < total {
        let bits = rng.next() << 11 | rng.below(2048);
        let x = f64::from_bits(bits & 0x7fff_ffff_ffff_ffff);
        match rng.below(8) {
            0 | 1 if x.is_finite() => cells.push(format!("{x:e}")),
            2 if x.is_finite() => cells.push(format!("-{x:.16e}")),
            3 => cells.push(format!("{:.2}", rng.next() as f64 / (1u64 << 40) as f64 - 4e6)),
            4 => cells.push(((rng.next() as i64).wrapping_mul(rng.next() as i64 | 1)).to_string()),
            5 => cells.push(format!("{}e{}", rng.below(1_000_000), rng.below(640) as i64 - 330)),
            6 => cells.push(format!("0.{:017}", rng.next() % 100_000_000_000_000_000)),
            _ if x.is_finite() => cells.push(format!("{x:e}")),
            _ => {}
        }
    }
    cells
}

#[test]
fn a_million_numbers_are_read_as_rust_reads_them_halfway_cases_included() {
    let cells = read_corpus(1_000_000);
    assert!(cells.len() >= 1_000_000);
    let (dir, exe) = build_driver("float-text-read", "json_floats.cho");
    let mut wrong: Vec<String> = Vec::new();
    for chunk in cells.chunks(125_000) {
        let doc = format!("[{}]", chunk.join(","));
        let out = feed(&exe, doc.as_bytes());
        assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), chunk.len());
        for (cell, line) in chunk.iter().zip(lines) {
            let bits = line.split_once(' ').unwrap().0.parse::<i64>().unwrap() as u64;
            let want = cell.parse::<f64>().expect("Rust reads every cell written here");
            if bits != want.to_bits() {
                wrong.push(format!(
                    "{}: read {:#x}, Rust reads {:#x}",
                    abbreviate(cell),
                    bits,
                    want.to_bits()
                ));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        wrong.is_empty(),
        "{} of {} cells read wrongly, first few:\n{}",
        wrong.len(),
        cells.len(),
        wrong.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
    );
}

fn abbreviate(cell: &str) -> String {
    if cell.len() <= 60 {
        cell.to_owned()
    } else {
        format!("{}...({} bytes)", &cell[..40], cell.len())
    }
}

/// Doubles `j / 2^k` (j odd) whose exact decimal has 17 or 18 significant
/// digits ending in a 5: the value sits dead between two decimals one digit
/// shorter, and when both read back to it, which one is printed is the rule
/// under test.
fn tie_doubles(rng: &mut Lcg, count: usize) -> Vec<f64> {
    let mut out = Vec::new();
    while out.len() < count {
        let k = 4 + rng.below(47) as u32;
        let digits = 17 + rng.below(2) as u32;
        let pow5 = 5u128.pow(k);
        let lo = 10u128.pow(digits - 1).div_ceil(pow5);
        let hi = (10u128.pow(digits) - 1) / pow5;
        if lo > hi || lo >= 1 << 53 {
            continue;
        }
        let hi = hi.min((1 << 53) - 1);
        let j = (lo.max(1) + u128::from(rng.next()) % (hi - lo.max(1) + 1)) | 1;
        if j >= 1 << 53 {
            continue;
        }
        out.push(j as f64 / 2f64.powi(k as i32));
    }
    out
}

/// The digits of `{:e}` text, and its exponent.
fn digits_and_exponent(text: &str) -> (String, i32) {
    let (mantissa, exponent) = text.split_once('e').expect("an exponent form");
    (mantissa.chars().filter(char::is_ascii_digit).collect(), exponent.parse().unwrap())
}

/// Is `got` Rust's `{:e}` (or an exact tie rounded the other way)? A tie is
/// settled by the exact value: `x`'s own decimal expansion continues `got`'s
/// digits with a 5 and then only zeros, and `got`'s last digit is the even one.
pub(super) fn is_rust_or_even_tie(x: f64, got: &str, rust: &str) -> bool {
    if got == rust {
        return true;
    }
    let (g, ge) = digits_and_exponent(got);
    let (r, re) = digits_and_exponent(rust);
    let exact = format!("{:.1100e}", x.abs());
    let (exact_digits, _) = digits_and_exponent(&exact);
    let n = g.len();
    let tie = exact_digits.len() > n
        && exact_digits.as_bytes()[n] == b'5'
        && exact_digits[n + 1..].bytes().all(|b| b == b'0');
    ge == re
        && g.len() == r.len()
        && g[..n - 1] == r[..n - 1]
        && tie
        && (g.as_bytes()[n - 1] - b'0') % 2 == 0
}

#[test]
fn exact_ties_between_two_shortest_decimals_print_the_even_one_and_everything_reads_back() {
    let mut rng = Lcg(0x71e5);
    let mut values: Vec<f64> = tie_doubles(&mut rng, 20_000);
    values.push(2f64.powi(-25)); // 2.9802322387695312e-8: the documented example
    values.push(86.125 * 2f64.powi(40));
    for _ in 0..200_000 {
        let x = f64::from_bits(rng.next() << 11 | rng.below(2048));
        if x.is_finite() {
            values.push(x);
        }
    }
    let (dir, exe) = build_driver("float-text-print", "float_print.cho");
    let doc = format!(
        "[{}]",
        values.iter().map(|x| (x.to_bits() as i64).to_string()).collect::<Vec<_>>().join(",")
    );
    let out = feed(&exe, doc.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), values.len());
    let _ = std::fs::remove_dir_all(&dir);

    let mut wrong = Vec::new();
    let mut ties = 0;
    for (x, got) in values.iter().zip(lines) {
        let rust = format!("{x:e}");
        if got != rust {
            ties += 1;
        }
        let reads_back = got.parse::<f64>().map(|y| y.to_bits() == x.to_bits()).unwrap_or(false);
        if !reads_back {
            wrong.push(format!("{rust}: printed {got}, which does not read back"));
        } else if !is_rust_or_even_tie(*x, got, &rust) {
            wrong.push(format!("{rust}: printed {got}, neither Rust's nor the even tie"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} wrong, first few:\n{}",
        wrong.len(),
        wrong[..wrong.len().min(10)].join("\n")
    );
    // The corpus is built to contain ties, and Rust rounds them up: a run
    // with no difference would be a corpus that tests nothing.
    assert!(ties > 1000, "only {ties} values printed differently from Rust's `{{:e}}`");
    // The documented example, by name.
    let (dir, exe) = build_driver("float-text-print-doc", "float_print.cho");
    let out = feed(&exe, format!("[{}]", 2f64.powi(-25).to_bits() as i64).as_bytes());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2.9802322387695312e-8");
    let _ = std::fs::remove_dir_all(&dir);
}
