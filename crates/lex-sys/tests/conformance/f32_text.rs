//! `f32` as text (`docs/f32.md` §5.3): `std.fmt32` held against Rust's own `{:?}`, `{:.N}`
//! and `str::parse::<f32>`, which are compiled from `tests/programs/f32_oracle.rs` with
//! `rustc -O` into each test's own scratch directory (tests run in parallel, and two of them
//! sharing a directory is a race that `binary32.rs` has already paid for once).
//!
//! * the shortest-round-trip gate over a sample of the 2^32 patterns (about 10^7 on LLVM and
//!   10^6 on Cranelift), as a digest of every printed line held against Rust's, plus the
//!   in-program checks that each line reads back to its bits and is shortest; the same over all
//!   2^32 patterns when `LEX_F32_EXHAUSTIVE` is set (opt in; resumable, see `exhaustive`);
//! * `{:?}` differential over 10^6 random patterns and the special ones;
//! * `{:.N}` differential, including exact ties;
//! * `parse` differential over 10^6 random decimal strings and a list of hard cases, among
//!   them exact halfway points between neighbouring `f32`s at up to 113 digits.

use std::io::Write;
use std::path::Path;
use std::time::Instant;

use super::*;

/// What the default suite spends on each gate.
const DEBUG_PATTERNS: u64 = 1_000_000;
const FIXED_PATTERNS: u64 = 1_000_000;
const PARSE_STRINGS: u64 = 1_000_000;

fn count(var: &str, default: u64) -> u64 {
    std::env::var(var).ok().and_then(|t| t.trim().parse().ok()).unwrap_or(default)
}

/// A splitmix64: the same stream for the same seed, with no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn build(dir: &Path, program: &str, backend: &str) -> PathBuf {
    let exe = dir.join(program);
    let out = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join(format!("tests/programs/{program}.ls")))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    exe
}

fn build_oracle(dir: &Path) -> PathBuf {
    let exe = dir.join("f32_oracle");
    let out = Command::new("rustc")
        .args(["-O", "--edition", "2021", "-o"])
        .arg(&exe)
        .arg(repo_root().join("tests/programs/f32_oracle.rs"))
        .output()
        .expect("rustc runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    exe
}

/// Feed `input` to `exe args` and answer what it printed, a line for a line. The input is
/// written from a thread: a filter that answers as it reads would otherwise fill its pipe
/// while this one is still writing.
fn filter(exe: &Path, args: &[&str], input: String) -> Vec<String> {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the filter starts");
    let mut stdin = child.stdin.take().expect("a stdin");
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });
    let out = child.wait_with_output().expect("the filter finishes");
    writer.join().expect("the writer finishes");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("utf-8").lines().map(str::to_owned).collect()
}

/// Both sides over the same input; the first disagreement, if any, as a message.
fn compare(
    what: &str,
    ours: &Path,
    oracle: &Path,
    mode: &[&str],
    inputs: &[String],
    show: impl Fn(&str) -> String,
) {
    let mut text = inputs.join("\n");
    text.push('\n');
    let got = filter(ours, mode, text.clone());
    let want = filter(oracle, mode, text);
    assert_eq!(got.len(), inputs.len(), "{what}: one answer per line");
    assert_eq!(want.len(), inputs.len(), "{what}: the oracle answers one per line");
    let mut bad = 0;
    let mut first = None;
    for i in 0..inputs.len() {
        if got[i] != want[i] {
            bad += 1;
            first.get_or_insert(i);
        }
    }
    if let Some(i) = first {
        panic!(
            "{what}: {bad} of {} differ from Rust; the first, for `{}`: got `{}`, Rust `{}`",
            inputs.len(),
            show(&inputs[i]),
            got[i],
            want[i]
        );
    }
}

// ---------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------

/// Random patterns, half of them uniform over all 32 bits and half with a random exponent
/// and the fraction biased toward short ones (so the exact and the nearly exact cases turn up
/// as often as the others do), then the special ones, NaNs excluded (a NaN's text is `NaN`
/// whatever its payload and `{:?}` agrees; they are in the sample gate's skip list too).
fn patterns(n: u64, seed: u64) -> Vec<u32> {
    let mut rng = Rng(seed);
    let mut out = Vec::new();
    for bits in [0u32, 1, 2, 0x007f_ffff, 0x0080_0000, 0x3f80_0000, 0x7f7f_ffff, 0x7f80_0000] {
        out.push(bits);
        out.push(bits | 0x8000_0000);
    }
    while (out.len() as u64) < n {
        let bits = match rng.below(4) {
            0 | 1 => rng.next() as u32,
            2 => {
                let e = rng.below(255) as u32;
                (rng.below(2) as u32) << 31 | e << 23 | (rng.next() as u32 & 0x7f_ffff)
            }
            _ => {
                // A short fraction: the value has few significant bits.
                let e = rng.below(255) as u32;
                let keep = rng.below(23) as u32;
                let fraction = (rng.next() as u32 & 0x7f_ffff) >> keep << keep;
                (rng.below(2) as u32) << 31 | e << 23 | fraction
            }
        };
        if bits & 0x7fff_ffff <= 0x7f80_0000 {
            out.push(bits);
        }
    }
    out
}

fn as_lines(bits: &[u32]) -> Vec<String> {
    bits.iter().map(u32::to_string).collect()
}

fn bits_of_line(line: &str) -> String {
    let bits: u32 = line.parse().expect("a pattern");
    format!("{bits:#010x} ({:?})", f32::from_bits(bits))
}

// ---------------------------------------------------------------------
// The sample digest gate
// ---------------------------------------------------------------------

/// `f32_exhaustive` over a set of patterns, held against the oracle's digest of Rust's `{:?}`
/// over the same set: equal digests mean every line, including which of several shortest
/// strings was picked, is the same as Rust's. The program also checks, on its own, that each
/// line reads back to its bits and that no decimal one digit shorter does.
fn digest_gate(tag: &str, backend: &str, stride: u64, per_exponent: u64) {
    let dir = scratch(tag);
    let ours = build(&dir, "f32_exhaustive", backend);
    let oracle = build_oracle(&dir);
    let started = Instant::now();
    let a = Command::new(&ours)
        .args(["sample", &stride.to_string(), &per_exponent.to_string()])
        .output()
        .expect("the program runs");
    let ran = started.elapsed();
    let b = Command::new(&oracle)
        .args(["sample", &stride.to_string(), &per_exponent.to_string()])
        .output()
        .expect("the oracle runs");
    let a = String::from_utf8_lossy(&a.stdout).into_owned();
    let b = String::from_utf8_lossy(&b.stdout).into_owned();
    let w: Vec<&str> = a.split_whitespace().collect();
    assert_eq!(w.len(), 13, "the program's report: `{a}`");
    println!("f32 shortest on {backend}: {} patterns in {ran:?}", w[1]);
    assert_eq!(
        (w[1], w[2]),
        (b.split_whitespace().next().unwrap_or(""), b.split_whitespace().nth(1).unwrap_or("")),
        "std.fmt32 and Rust's `{{:?}}` disagree somewhere over the sample (lines, digest): \
         `{a}` against `{b}`"
    );
    assert_eq!(w[4], "0", "{} of the lines did not read back to their bits: `{a}`", w[4]);
    assert_eq!(w[6], "0", "{} lines were not the shortest: `{a}`", w[6]);
    assert_eq!(w[12], "-1", "first failing pattern: `{a}`");
    assert!(w[1].parse::<u64>().unwrap() > per_exponent * 500, "the sample is the size asked");
    let _ = std::fs::remove_dir_all(&dir);
}

/// About 10^7 patterns: 2 signs x 256 exponents x (19,500 + 19).
#[test]
fn shortest_over_a_sample_matches_rust_on_llvm() {
    digest_gate("f32-text-sample-llvm", "llvm", 429, 19_500);
}

/// About 10^6: the same program through the other backend.
#[test]
fn shortest_over_a_sample_matches_rust_on_cranelift() {
    digest_gate("f32-text-sample-cranelift", "cranelift", 4093, 1_950);
}

/// All 2^32 patterns, **opt in**: `LEX_F32_EXHAUSTIVE=1 cargo test --release f32_text_exhaustive
/// -- --nocapture`. It is cut into 64 ranges of 2^26 patterns, run four at a time, each result
/// kept in `LEX_F32_EXHAUSTIVE_DIR` (default: the system temporary directory,
/// `lex-sys-f32-exhaustive`) as `done.<k>`, so a run that is killed starts again where it
/// stopped: a range is skipped only if its file says it agreed. Takes about twenty minutes
/// on four cores (`docs/f32.md` §5.3 has the measured number).
#[test]
fn f32_text_exhaustive() {
    if std::env::var_os("LEX_F32_EXHAUSTIVE").is_none() {
        return;
    }
    let state = std::env::var_os("LEX_F32_EXHAUSTIVE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("lex-sys-f32-exhaustive"));
    std::fs::create_dir_all(&state).expect("a state directory");
    let dir = scratch("f32-text-exhaustive");
    let ours = build(&dir, "f32_exhaustive", "llvm");
    let oracle = build_oracle(&dir);
    const RANGE: u64 = 1 << 26;
    let next = std::sync::atomic::AtomicU64::new(0);
    let started = Instant::now();
    let totals = std::sync::Mutex::new((0u64, 0u64));
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                loop {
                    let k = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if k >= 64 {
                        break;
                    }
                    let done = state.join(format!("done.{k}"));
                    if let Ok(line) = std::fs::read_to_string(&done) {
                        let n: u64 = line.split_whitespace().next().unwrap().parse().unwrap();
                        let mut t = totals.lock().unwrap();
                        t.0 += n;
                        t.1 += 1;
                        continue;
                    }
                    let (lo, hi) = ((k * RANGE).to_string(), ((k + 1) * RANGE).to_string());
                    let a = Command::new(&ours).args(["range", &lo, &hi]).output().unwrap();
                    let b = Command::new(&oracle).args(["range", &lo, &hi]).output().unwrap();
                    let a = String::from_utf8_lossy(&a.stdout).into_owned();
                    let b = String::from_utf8_lossy(&b.stdout).into_owned();
                    let w: Vec<&str> = a.split_whitespace().collect();
                    assert_eq!(w.len(), 13, "range {k}: `{a}`");
                    assert_eq!(
                        format!("{} {}", w[1], w[2]),
                        b.trim(),
                        "range {k}: std.fmt32 and Rust's `{{:?}}` disagree (`{a}` against `{b}`)"
                    );
                    assert_eq!((w[4], w[6], w[12]), ("0", "0", "-1"), "range {k}: `{a}`");
                    std::fs::write(&done, format!("{} {}\n", w[1], w[2])).unwrap();
                    let mut t = totals.lock().unwrap();
                    t.0 += w[1].parse::<u64>().unwrap();
                    t.1 += 1;
                }
            });
        }
    });
    let (lines, ranges) = *totals.lock().unwrap();
    println!("f32 exhaustive: {ranges} ranges, {lines} printed patterns, {:?}", started.elapsed());
    // 2^32 patterns less the NaNs: 2 * (2^23 - 1).
    assert_eq!(lines, (1u64 << 32) - 2 * ((1 << 23) - 1));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------
// `{:?}`
// ---------------------------------------------------------------------

fn debug_gate(backend: &str, n: u64) {
    let dir = scratch(&format!("f32-text-debug-{backend}"));
    let ours = build(&dir, "f32_text", backend);
    let oracle = build_oracle(&dir);
    let inputs = as_lines(&patterns(n, 0xdeb0_0001));
    let started = Instant::now();
    compare("`{:?}`", &ours, &oracle, &["debug"], &inputs, bits_of_line);
    println!("f32 `{{:?}}` on {backend}: {} patterns agree, {:?}", inputs.len(), started.elapsed());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn debug_matches_rust_on_llvm() {
    debug_gate("llvm", count("LEX_F32_DEBUG", DEBUG_PATTERNS));
}

#[test]
fn debug_matches_rust_on_cranelift() {
    debug_gate("cranelift", count("LEX_F32_DEBUG", DEBUG_PATTERNS) / 4);
}

// ---------------------------------------------------------------------
// `{:.N}`
// ---------------------------------------------------------------------

/// Values that are exact ties at `prec` places: `(2a + 1) / 2^(prec + 1)` is a decimal with
/// `prec + 1` places ending in 5, so it is halfway between two `prec`-place decimals, and it
/// is an `f32` while `2a + 1` is under 2^24. Also the same plus a whole part, and the ones
/// at the edge of rounding up through nines.
fn ties(rng: &mut Rng, prec: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let scale = 2f32.powi(prec as i32 + 1);
    for _ in 0..400 {
        let width = 1 + rng.below(24);
        let odd = (rng.next() & ((1 << width) - 1)) | 1;
        if odd >= 1 << 24 {
            continue;
        }
        let x = odd as f32 / scale;
        out.push(x.to_bits());
        out.push((-x).to_bits());
    }
    // 0.5, 1.5, 2.5, ... and k + 0.5 for large k that is still a tie; 0.125, 0.375, ...
    for k in 0..40u32 {
        out.push((k as f32 + 0.5).to_bits());
        out.push((k as f32 + 0.25).to_bits());
        out.push((k as f32 + 0.125).to_bits());
        out.push((k as f32 + 0.0625).to_bits());
        out.push((k as f32 + 0.03125).to_bits());
    }
    // Rounding up through nines: 0.9995, 9.5, 99.5 and the f32s next to them.
    for x in [0.5f32, 9.5, 99.5, 999.5, 0.95, 0.995, 0.9995, 0.99995, 9.99995, 0.05, 0.005] {
        let b = x.to_bits();
        for d in 0..4 {
            out.push(b + d);
            out.push(b - d);
        }
    }
    out
}

fn fixed_gate(backend: &str, n: u64) {
    let dir = scratch(&format!("f32-text-fixed-{backend}"));
    let ours = build(&dir, "f32_text", backend);
    let oracle = build_oracle(&dir);
    let started = Instant::now();
    let precisions = [0u32, 1, 2, 3, 4, 5, 6, 8, 10, 12, 20, 45, 60];
    let mut total = 0;
    let mut tied = 0;
    let per = n / precisions.len() as u64;
    for (i, prec) in precisions.iter().enumerate() {
        let mut rng = Rng(0xf1ed_0000 + i as u64);
        let mut set = patterns(per, 0xf1ed_1000 + i as u64);
        let t = ties(&mut rng, *prec);
        tied += t.len();
        set.extend(t);
        let inputs = as_lines(&set);
        let p = prec.to_string();
        compare(&format!("`{{:.{prec}}}`"), &ours, &oracle, &["fixed", &p], &inputs, |l| {
            bits_of_line(l)
        });
        total += inputs.len();
    }
    println!(
        "f32 `{{:.N}}` on {backend}: {total} (pattern, N) pairs agree, {tied} of them \
         constructed ties, {:?}",
        started.elapsed()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fixed_matches_rust_on_llvm() {
    fixed_gate("llvm", count("LEX_F32_FIXED", FIXED_PATTERNS));
}

#[test]
fn fixed_matches_rust_on_cranelift() {
    fixed_gate("cranelift", count("LEX_F32_FIXED", FIXED_PATTERNS) / 4);
}

// ---------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------

/// The exact decimal expansion of the value `m * 2^e`, as `(digits, point)`: the value is
/// `0.digits * 10^point`, with no leading or trailing zero in `digits`.
fn exact_decimal(m: u64, e: i32) -> (Vec<u8>, i32) {
    // A decimal digit vector, most significant first, scaled by repeated small multiplication.
    let mut digits: Vec<u8> = m.to_string().bytes().map(|b| b - b'0').collect();
    let mut point = digits.len() as i32;
    let mul = |digits: &mut Vec<u8>, by: u32, point: &mut i32| {
        let mut carry = 0u32;
        for d in digits.iter_mut().rev() {
            let v = *d as u32 * by + carry;
            *d = (v % 10) as u8;
            carry = v / 10;
        }
        while carry > 0 {
            digits.insert(0, (carry % 10) as u8);
            *point += 1;
            carry /= 10;
        }
    };
    if e >= 0 {
        for _ in 0..e {
            mul(&mut digits, 2, &mut point);
        }
    } else {
        // m / 2^k = m * 5^k / 10^k
        for _ in 0..(-e) {
            mul(&mut digits, 5, &mut point);
        }
        point -= -e;
    }
    while digits.last() == Some(&0) {
        digits.pop();
    }
    let lead = digits.iter().take_while(|d| **d == 0).count();
    digits.drain(..lead);
    point -= lead as i32;
    (digits, point)
}

fn text_of(digits: &[u8], point: i32) -> String {
    let body: String = digits.iter().map(|d| (b'0' + d) as char).collect();
    format!("0.{body}e{point}")
}

/// Strings that sit exactly on, one unit in the last place above, and one below the midpoint
/// of neighbouring `f32`s: a binary64 that read the string first would round it to the
/// midpoint itself in the last two cases, and then to even.
fn halfway_cases(rng: &mut Rng, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..n {
        // Spread over every exponent, including the subnormals, the largest finite
        // (whose midpoint above is the overflow boundary) and the smallest.
        let low = match i % 8 {
            0 => 0u32,
            1 => 1,
            2 => 0x7f_ffff,
            3 => 0x7f7f_fffe,
            _ => (rng.next() as u32) & 0x7f7f_ffff,
        };
        let bits = low & 0x7fff_ffff;
        let ieee_e = (bits >> 23) as i32;
        let frac = (bits & 0x7f_ffff) as u64;
        let (m, e) = if ieee_e == 0 { (frac, -149) } else { (frac | 1 << 23, ieee_e - 150) };
        // The midpoint above `bits` is (2m + 1) * 2^(e - 1).
        let (digits, point) = exact_decimal(2 * m + 1, e - 1);
        let exact = text_of(&digits, point);
        let mut above = digits.clone();
        above.push(1);
        let mut below = digits.clone();
        // One below: the last digit (a 5, for an odd multiple of a power of two with a
        // negative exponent) less one, then nines.
        if let Some(last) = below.last_mut() {
            if *last > 0 {
                *last -= 1;
                below.extend(std::iter::repeat_n(9, 1 + (rng.below(40) as usize)));
            }
        }
        out.push(exact);
        out.push(text_of(&above, point));
        out.push(text_of(&below, point));
        // The same, with the digits spread across a point and zero padded.
        let body: String = digits.iter().map(|d| (b'0' + d) as char).collect();
        out.push(format!("{body}0000e{}", point - digits.len() as i32 - 4));
        out.push(format!("-{body}e{}", point - digits.len() as i32));
    }
    out
}

/// `digits` plus one in the last place (carrying), as `(digits, point)`.
fn bump(digits: &[u8], point: i32) -> (Vec<u8>, i32) {
    let mut out = digits.to_vec();
    let mut i = out.len();
    while i > 0 {
        i -= 1;
        if out[i] == 9 {
            out[i] = 0;
        } else {
            out[i] += 1;
            return (out, point);
        }
    }
    out.insert(0, 1);
    (out, point + 1)
}

/// Short decimals next to a midpoint: its digits cut to 14..=17 and rounded down or up. They
/// have few enough digits for the reader's fast path (`w` under 2^53, `q` within 22), which
/// reads them as a `float` first, and are within a binary64 half unit of the midpoint, so
/// that path is where a double rounding would be, if there is one. The exponents are those
/// whose `q` is in range.
fn near_midpoint_cases(rng: &mut Rng, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    for _ in 0..n {
        let ieee_e = 100 + rng.below(90) as i32;
        let frac = rng.next() & 0x7f_ffff;
        let (digits, point) = exact_decimal(2 * (frac | 1 << 23) + 1, ieee_e - 150 - 1);
        for keep in 14..=17 {
            if digits.len() > keep {
                let down = &digits[..keep];
                out.push(text_of(down, point));
                let (up, up_point) = bump(down, point);
                out.push(text_of(&up, up_point));
            }
        }
    }
    out
}

/// A random decimal: 1..=40 digits, a point in a random place or none, an exponent that is
/// small, large, or absent, sometimes leading zeros or trailing ones.
fn random_text(rng: &mut Rng) -> String {
    let n = 1 + rng.below(40) as usize;
    let mut digits = String::new();
    for _ in 0..n {
        digits.push((b'0' + rng.below(10) as u8) as char);
    }
    let mut text = String::new();
    match rng.below(8) {
        0 => text.push('-'),
        1 => text.push('+'),
        _ => {}
    }
    match rng.below(4) {
        0 => text.push_str(&digits),
        1 => {
            let p = rng.below(n as u64 + 1) as usize;
            text.push_str(&digits[..p]);
            text.push('.');
            text.push_str(&digits[p..]);
        }
        2 => {
            text.push_str("0.");
            text.push_str(&"0".repeat(rng.below(50) as usize));
            text.push_str(&digits);
        }
        _ => {
            text.push_str(&digits[..1]);
            text.push('.');
            text.push_str(&digits[1..]);
        }
    }
    match rng.below(5) {
        0 => {}
        1 | 2 => text.push_str(&format!("e{}", rng.below(80) as i64 - 40)),
        3 => text.push_str(&format!("E{}", rng.below(20) as i64 - 10)),
        _ => text.push_str(&format!("e{}", rng.below(600) as i64 - 300)),
    }
    text
}

/// Fixed hard cases: what Rust's parser accepts and refuses, and the edges of the range.
fn hard_cases() -> Vec<String> {
    let mut cases: Vec<String> = [
        // Spellings.
        "",
        " ",
        "+",
        "-",
        ".",
        "+.",
        "-.",
        "e5",
        ".e5",
        "1e",
        "1e+",
        "1e-",
        "1e5.5",
        "1..2",
        "1.2.3",
        "1,5",
        "1_000",
        "0x10",
        "0b1",
        "1f32",
        "1f",
        "1d",
        "١",
        "1e5 ",
        " 1e5",
        "1\t",
        "\u{0}",
        "5.",
        ".5",
        "-.5",
        "+5.",
        "5.e3",
        "5.E3",
        "-0",
        "+0",
        "-0.0",
        "0e999",
        "0.0e-999",
        "00001",
        "1e0001",
        "1e+0005",
        "0.000000000000000000000000000000001e33",
        // Words.
        "nan",
        "NaN",
        "NAN",
        "nAn",
        "+nan",
        "-nan",
        "inf",
        "Inf",
        "INF",
        "+inf",
        "-inf",
        "infinity",
        "Infinity",
        "-INFINITY",
        "infinit",
        "infinityy",
        "nana",
        "in",
        "na",
        "n",
        "i",
        "-",
        "--1",
        "+-1",
        "-+1",
        "1-",
        "1+",
        "i nf",
        "inf ",
        " inf",
        "nan(0x1)",
        "infe5",
        // The edges.
        "3.4028235e38",
        "3.4028234663852886e38",
        "3.4028235677973362e38",
        "3.40282356779733661637539395458142568447e38",
        "3.40282356779733661637539395458142568448e38",
        "3.4028236e38",
        "3.5e38",
        "1e39",
        "1e38",
        "1e400",
        "1e-400",
        "1e99999999999999999999",
        "1e-99999999999999999999",
        "0.0e99999999999999999999",
        "1.17549435e-38",
        "1.1754942e-38",
        "1.1754943508222875e-38",
        "1.4e-45",
        "1.401298464324817e-45",
        "7.006492321624085e-46",
        "7.006492321624086e-46",
        "7.0064923216240853546186479164495806564013277134e-46",
        "7.0064923216240853546186479164495806564013277135e-46",
        "7e-46",
        "7.1e-46",
        "2.1019476964872256e-45",
        "16777216",
        "16777217",
        "16777218",
        "16777219",
        "16777217.0000000000000000000000000000000000000001",
        "16777216.9999999999999999999999999999999999999999",
        "1.00000005960464477539062",
        "1.0000000596046447753906250",
        "1.0000000596046447753906251",
        "1.00000005960464477539063",
        "0.99999997019767761230468750",
        "0.9999999701976776123046875000000000000000000000000000000000000001",
        "340282356779733661637539395458142568447.9999999999999999999999",
        "0.1",
        "0.2",
        "0.3",
        "123456789",
        "9999999999999999999999999999999999999999",
        "0.0000000000000000000000000000000000000000000001",
        "100000000000000000000000000000000000000000000000000000000000e-60",
        "1e-45",
        "1e-46",
        "0.7e-45",
        "0.70064923216240853546186479164495806564013277134e-45",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // Many digits: 200 of them, and one past the 160 the reader keeps.
    let long = "1".repeat(200);
    cases.push(long.clone());
    cases.push(format!("0.{long}e-100"));
    cases.push(format!("{long}e-200"));
    let tie = "1.000000059604644775390625";
    cases.push(format!("{tie}{}1", "0".repeat(300)));
    cases.push(format!("{tie}{}", "0".repeat(300)));
    cases.push(format!("{}{tie}", "0".repeat(300)));
    cases.push(format!("{}.{}", "0".repeat(300), "1".repeat(5)));
    // Every one of the first few hundred powers of ten, and their neighbours.
    for e in (-60..=50).step_by(1) {
        cases.push(format!("1e{e}"));
        cases.push(format!("9.99999999e{e}"));
    }
    cases
}

fn parse_gate(backend: &str, n: u64) {
    let dir = scratch(&format!("f32-text-parse-{backend}"));
    let ours = build(&dir, "f32_text", backend);
    let oracle = build_oracle(&dir);
    let mut rng = Rng(0x9a75_e000);
    let mut inputs = hard_cases();
    let hard = inputs.len();
    inputs.extend(halfway_cases(&mut rng, (n / 100).clamp(200, 5_000) as usize));
    inputs.extend(near_midpoint_cases(&mut rng, (n / 20).clamp(500, 50_000) as usize));
    let halfway = inputs.len() - hard;
    // Whole numbers of every exponent, as a reader of `{:?}` output sees them.
    for _ in 0..n / 20 {
        let bits = (rng.next() as u32) & 0x7f7f_ffff;
        inputs.push(format!("{:?}", f32::from_bits(bits)));
        inputs.push(format!("{:e}", f32::from_bits(bits)));
    }
    // `n` random decimals on top of those.
    let random = n;
    for _ in 0..random {
        inputs.push(random_text(&mut rng));
    }
    let started = Instant::now();
    compare("parse", &ours, &oracle, &["parse"], &inputs, |l| format!("{l:?}"));
    println!(
        "f32 parse on {backend}: {} strings agree ({random} random, {hard} hard, {halfway} from \
         midpoints, the rest `{{:?}}` and `{{:e}}` output), {:?}",
        inputs.len(),
        started.elapsed()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parse_matches_rust_on_llvm() {
    parse_gate("llvm", count("LEX_F32_PARSE", PARSE_STRINGS));
}

#[test]
fn parse_matches_rust_on_cranelift() {
    parse_gate("cranelift", count("LEX_F32_PARSE", PARSE_STRINGS) / 4);
}

/// The reason the reader exists: a decimal one digit above the tie `1 + 2^-24` is `0x3f800001`
/// read once, and `0x3f800000` through binary64. Rust agrees with the first, which is what the
/// oracle says, and which a reader that went through `float` would not.
#[test]
fn parse_does_not_round_twice() {
    let dir = scratch("f32-text-twice");
    let ours = build(&dir, "f32_text", "llvm");
    let oracle = build_oracle(&dir);
    let text = "1.0000000596046447753906251";
    let got = filter(&ours, &["parse"], format!("{text}\n"));
    let want = filter(&oracle, &["parse"], format!("{text}\n"));
    assert_eq!(got, want);
    assert_eq!(got, ["1065353217"]);
    // Through binary64 it is the tie itself, which rounds to even, down.
    assert_eq!((text.parse::<f64>().unwrap() as f32).to_bits(), 0x3f80_0000);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `tests/accept/f32_text.ls` on both backends, byte for byte what its header says (the accept
/// walker runs only the default one).
#[test]
fn the_f32_text_fixture_agrees_on_both_backends() {
    let source = std::fs::read_to_string(repo_root().join("tests/accept/f32_text.ls"))
        .expect("the fixture exists");
    let mut expected: String = source
        .lines()
        .filter_map(|line| line.strip_prefix("//~ STDOUT "))
        .collect::<Vec<_>>()
        .join("\n");
    expected.push('\n');
    for backend in ["cranelift", "llvm"] {
        let dir = scratch(&format!("f32-text-fixture-{backend}"));
        let exe = dir.join("out");
        let build = Command::new(BIN)
            .args(["build", "--std", "--backend", backend])
            .arg(repo_root().join("tests/accept/f32_text.ls"))
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("the compiler runs");
        assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "on {backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
