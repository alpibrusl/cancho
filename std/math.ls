module std.math;

// `std.math` — the arithmetic every program writes for itself.

pub fn min(a: int, b: int) -> [] int {
    if a < b {
        return a;
    }
    return b;
}

pub fn max(a: int, b: int) -> [] int {
    if a > b {
        return a;
    }
    return b;
}

// **Traps on the most negative integer**, because negating it
// overflows.
//
// `docs/defined-behaviour.md` §2.1: an operation with no right answer
// stops rather than inventing one. Every other language's `abs` returns
// the negative number here, which is the silently-wrong answer this
// language exists to refuse — so this one does not return at all. The
// trap is `0 - n` doing what `-` already does, not a check added on
// top: the arithmetic is checked, so `abs` is too, for free.
pub fn abs(n: int) -> [] int {
    if n < 0 {
        return 0 - n;
    }
    return n;
}

pub fn sign(n: int) -> [] int {
    if n < 0 {
        return 0 - 1;
    }
    if n > 0 {
        return 1;
    }
    return 0;
}

// Euclid, on magnitudes. `gcd(0, 0)` is 0, which is the convention every
// library uses and the only value that makes `gcd` total.
pub fn gcd(a: int, b: int) -> [] int {
    var x = abs(a);
    var y = abs(b);
    while y != 0 {
        let r = x % y;
        x = y;
        y = r;
    }
    return x;
}

// `exp`, `log` and `pow` -- `docs/float-math.md` §6's open row, closed
// as library code with a *stated* accuracy rather than a builtin: none
// of the three is one instruction the way `sqrt` is, and §4 there says
// why that puts them here instead. Checked against Rust's own
// `f64::exp`/`f64::ln`/`f64::powf` over a wide sweep in
// `crates/lex-sys/tests/conformance/floats.rs` -- not correctly
// rounded, and not claimed to be.

// `ln2` split into a high part with its low twenty bits zeroed and a
// residual low part, so that `k * ln2_hi()` loses no precision for the
// range of `k` `exp` produces. The standard technique (fdlibm's
// `__ieee754_exp` does the same split) -- without it, `exp`'s worst
// relative error over ±700 was 8e-14; with it, 2.4e-14.
fn ln2_hi() -> [] float {
    return 0.6931471526622772;
}

fn ln2_lo() -> [] float {
    return 2.7897668064547076e-08;
}

// 2^k, exact wherever it does not overflow or underflow, by
// exponentiation by squaring on ordinary float multiplication.
//
// This is the whole reason `exp` and `log` can scale by an integer
// exponent at all: `bits_of` only reads a float's bits
// (`docs/float-printing.md` §2), and there is no builtin that goes the
// other way and builds one back up. Multiplying by 2.0 is exact as long
// as it does not overflow, so this needs no bit construction -- and an
// overflowing or underflowing result answers infinity or zero exactly
// the way the hardware would, since float arithmetic here does not trap
// (`docs/floating-point.md` §2.1).
fn pow2(k: int) -> [] float {
    var negative = false;
    var e = k;
    if e < 0 {
        negative = true;
        e = 0 - e;
    }
    var result = 1.0;
    var base = 2.0;
    while e > 0 {
        if e % 2 == 1 {
            result = result * base;
        }
        base = base * base;
        e = e / 2;
    }
    if negative {
        return 1.0 / result;
    }
    return result;
}

// Nearest integer, ties away from zero. `truncate` rounds toward zero,
// which is a different function (`docs/floating-point.md` §4); this is
// what `exp`'s range reduction needs to pick the closest multiple of
// `ln2`.
fn round_to_int(v: float) -> [] int {
    if v >= 0.0 {
        return truncate(v + 0.5);
    }
    return truncate(v - 0.5);
}

// e^x, by range reduction to `x = k*ln2 + r` with `|r| <= ln2/2`, a
// 14-term Taylor series for `e^r`, and `pow2(k)` to rescale.
//
// Accurate to within 2.4e-14 relative (measured over 300,000 values
// spanning the whole non-overflowing range) -- not correctly rounded,
// which `float-math.md` §2 already found is not achievable in library
// code for a hand-rolled `sqrt`, and this is the same shape of claim
// stated honestly rather than assumed.
pub fn exp(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    // 750 sits past where `x/ln2`'s rounding could reach out of
    // `round_to_int`'s safe range for an extreme finite `x`, and further
    // still past where an infinite `x` itself lands -- one guard answers
    // both. It is **not** the overflow boundary: `e^x` itself already
    // overflows to infinity, correctly, anywhere past ~709.78, through
    // the ordinary arithmetic below (see the `pow2` split next).
    if x > 750.0 {
        return 1.0 / 0.0;
    }
    if x < -750.0 {
        return 0.0;
    }

    let k = round_to_int(x / (ln2_hi() + ln2_lo()));
    let r = x - float_of(k) * ln2_hi() - float_of(k) * ln2_lo();

    var term = 1.0;
    var sum = 1.0;
    var n = 1;
    while n <= 14 {
        term = term * r / float_of(n);
        sum = sum + term;
        n = n + 1;
    }

    // `pow2(k)` alone can overflow even where `sum * pow2(k)` would not:
    // `k` can reach 1024, and `2^1024` is already past a `float`'s
    // range, though `sum` (always in `[0.5, 2)`) would have brought the
    // product back under it. Splitting the exponent in half before
    // scaling keeps every intermediate value in range up to the true
    // overflow point, and correctly *reaches* infinity exactly there --
    // found by this function's own differential test failing at
    // `exp(709.5)`, which is finite (~1.35e308) and this used to answer
    // infinity for.
    let half = k / 2;
    return sum * pow2(k - half) * pow2(half);
}

// The natural logarithm, by pulling the binary exponent `e` out of `x`'s
// own bits (`bits_of`, the one direction this language can read a float
// apart) so that `x / pow2(e)` is a mantissa `m` in `[1, 2)`, then a
// 14-term series in `y = (m-1)/(m+1)` -- `log(m) = 2*(y + y^3/3 + y^5/5
// + ...)`, which converges everywhere `y` can be over that range.
//
// Accurate to within 6e-14 relative away from `log(x) == 0`, where
// relative error stops meaning anything (`docs/float-math.md` §2 is the
// same caveat for `sqrt`'s own tails); the absolute error there is under
// 1e-12. Not accurate for subnormal `x` (below roughly 2.2e-308): a
// subnormal's mantissa has no implicit leading one, and this does not
// special-case that, which is a known and stated gap rather than a
// silent one.
pub fn log(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    if x < 0.0 {
        return 0.0 / 0.0;
    }
    if x == 0.0 {
        return 0.0 - 1.0 / 0.0;
    }
    if x > 1.7976931348623157e308 {
        // +infinity: `pow2` of its own raw exponent would overflow too,
        // turning `x / pow2(e)` into inf/inf -- answer it directly
        // rather than falling into that.
        return x;
    }

    let raw_exponent = bits_of(x) >> 52 & 0x7ff;
    let e = raw_exponent - 1023;
    let m = x / pow2(e);

    let y = (m - 1.0) / (m + 1.0);
    let y2 = y * y;
    var term = y;
    var sum = 0.0;
    var n = 1;
    while n <= 27 {
        sum = sum + term / float_of(n);
        term = term * y2;
        n = n + 2;
    }
    return 2.0 * sum + float_of(e) * (ln2_hi() + ln2_lo());
}

// Is `y` already a whole number? Beyond 2^53 every representable
// `float` is one, which this answers without going through `truncate`
// and its own trap boundary near 2^63 (`docs/floating-point.md` §4).
fn is_integer(y: float) -> [] bool {
    if y > 9007199254740992.0 || y < -9007199254740992.0 {
        return true;
    }
    return float_of(truncate(y)) == y;
}

// Is the integer-valued `y` odd? Only meaningful once `is_integer(y)`
// has said yes. Past 9e15 the sign `pow` would use it for is already
// lost in the noise of `exp(y * log(|x|))`, so this answers even rather
// than reach for `truncate` out near its trap boundary.
fn is_odd_integer(y: float) -> [] bool {
    if y > 9.0e15 || y < -9.0e15 {
        return false;
    }
    return truncate(y) % 2 != 0;
}

// `x` to the power `y`, as `exp(y * log(x))` for `x > 0` -- which is
// most of what a program wants `pow` for, and inherits `exp`/`log`'s own
// accuracy. `x <= 0` needs its own cases: a negative base is only
// defined for an integer exponent, and `x == 0` and `y == 0` are the two
// conventions C's `pow` settled that this follows rather than reinvents.
pub fn pow(x: float, y: float) -> [] float {
    if y == 0.0 {
        return 1.0;
    }
    if is_nan(x) || is_nan(y) {
        return 0.0 / 0.0;
    }
    if x == 0.0 {
        if y > 0.0 {
            return 0.0;
        }
        return 1.0 / 0.0;
    }
    if x > 0.0 {
        return exp(y * log(x));
    }
    if !is_integer(y) {
        return 0.0 / 0.0;
    }
    let magnitude = exp(y * log(0.0 - x));
    if is_odd_integer(y) {
        return 0.0 - magnitude;
    }
    return magnitude;
}

// ---- floats: sign, rounding, and the trigonometric pair ------------------
//
// `docs/float-math.md` §8. None of these is a builtin: each is a few lines
// of `float`/`truncate` arithmetic, so each is library code with a stated
// accuracy, the same shape as `exp`/`log`/`pow` above.

// |x|, as C's `fabs`: `fabs(-0.0)` is `+0.0` and a NaN stays a NaN. `x <= 0.0`
// rather than `x < 0.0` is what makes the zero case come out positive
// (`0.0 - -0.0` is `+0.0`), and a NaN fails the comparison and falls through.
pub fn fabs(x: float) -> [] float {
    if x <= 0.0 {
        return 0.0 - x;
    }
    return x;
}

// The smaller of two floats. As C's `fmin`, a NaN is treated as missing data:
// the other argument is the answer, and the answer is a NaN only if both are.
pub fn fmin(a: float, b: float) -> [] float {
    if is_nan(a) {
        return b;
    }
    if is_nan(b) {
        return a;
    }
    if a < b {
        return a;
    }
    return b;
}

// The larger of two floats; a NaN is missing data, as for `fmin`.
pub fn fmax(a: float, b: float) -> [] float {
    if is_nan(a) {
        return b;
    }
    if is_nan(b) {
        return a;
    }
    if a > b {
        return a;
    }
    return b;
}

// Every float at or past 2^52 is already a whole number, so these three
// answer it unchanged: that is the right answer, and it keeps `truncate`
// (which traps near 2^63, `docs/floating-point.md` §4) out of the range
// where it would matter.
fn whole_beyond(x: float) -> [] bool {
    return x >= 4503599627370496.0 || x <= 0.0 - 4503599627370496.0;
}

// The largest whole number not above `x`. Exact: `x - truncate(x)` is
// computed without rounding for any `|x| < 2^52`, so nothing here depends on
// an addition that could round across a boundary. The sign of a zero result
// is `+0.0`, where C's `floor(-0.0)` keeps the minus; nothing observable
// short of `bits_of` tells them apart.
pub fn floor(x: float) -> [] float {
    if is_nan(x) || whole_beyond(x) {
        return x;
    }
    let t = float_of(truncate(x));
    if t > x {
        return t - 1.0;
    }
    return t;
}

// The smallest whole number not below `x`; same exactness and zero as `floor`.
pub fn ceil(x: float) -> [] float {
    if is_nan(x) || whole_beyond(x) {
        return x;
    }
    let t = float_of(truncate(x));
    if t < x {
        return t + 1.0;
    }
    return t;
}

// The nearest whole number, ties away from zero (C's `round`; `floor(x + 0.5)`
// is not it -- `0.49999999999999994 + 0.5` rounds up to `1.0`). The distance
// to `truncate(x)` is exact, so it is compared against one half directly.
pub fn round(x: float) -> [] float {
    if is_nan(x) || whole_beyond(x) {
        return x;
    }
    let t = float_of(truncate(x));
    let d = x - t;
    if d >= 0.5 {
        return t + 1.0;
    }
    if d <= -0.5 {
        return t - 1.0;
    }
    return t;
}

// sin/cos on `[-pi/4, pi/4]`: fdlibm's `__kernel_sin` and `__kernel_cos`
// polynomials, whose coefficients are a minimax fit (chosen to minimise the
// worst error over the interval) and not a Taylor series's.
fn kernel_sin(x: float) -> [] float {
    let z = x * x;
    let r = 8.33333333332248946124e-03 + z * (-1.98412698298579493134e-04 + z * (2.75573137070700676789e-06 + z * (-2.50507602534068634195e-08 + z * 1.58969099521155010221e-10)));
    return x + z * x * (-1.66666666666666324348e-01 + z * r);
}

fn kernel_cos(x: float) -> [] float {
    let z = x * x;
    let r = z * (4.16666666666666019037e-02 + z * (-1.38888888888741095749e-03 + z * (2.48015872894767294178e-05 + z * (-2.75573143513906633035e-07 + z * (2.08757232129817482790e-09 + z * -1.13596475577881948265e-11)))));
    let half_z = 0.5 * z;
    let w = 1.0 - half_z;
    return w + (1.0 - w - half_z + z * r);
}

// The quarter-turn count nearest `x`, and what is left of `x` after taking
// it away, packed as `k` and the remainder `r` with `|r| <= pi/4`. `pi/2` is
// subtracted in three pieces of 33 bits each (fdlibm's `pio2_1..3`), so that
// `k * piece` is exact for `|k| < 2^20` and the cancellation in `x - k*pi/2`
// loses nothing: the three together carry `pi/2` to about 99 bits.
fn quarter_turns(x: float) -> [] int {
    return round_to_int(x * 6.36619772367581382433e-01);
}

fn remainder_of(x: float, k: int) -> [] float {
    let n = float_of(k);
    let r = x - n * 1.57079632673412561417e+00;
    let r = r - n * 6.07710050630396597660e-11;
    let r = r - n * 2.02226624871116645580e-21;
    return r - n * 8.47842766036889956997e-32;
}

// The reduction below is exact only while `k` fits in 20 bits, which is
// `|x| < 1.6e6`. Past that there is a right answer this does not compute --
// so, like every operation here with none to give (`docs/defined-behaviour.md`
// §2.1), it stops: the trap, not a NaN that looks like one of `sin`'s own.
// A NaN argument is not out of range; it is returned, as C does.
fn trig_domain(x: float) -> [] bool {
    return x >= -1.0e6 && x <= 1.0e6;
}

// sine, `|x| <= 1e6`; NaN in, NaN out; traps outside the range. Accuracy is
// measured in `docs/float-math.md` §8.
pub fn sin(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    if !trig_domain(x) {
        return float_of(trap());
    }
    let k = quarter_turns(x);
    let r = remainder_of(x, k);
    let q = (k % 4 + 4) % 4;
    if q == 0 {
        return kernel_sin(r);
    }
    if q == 1 {
        return kernel_cos(r);
    }
    if q == 2 {
        return 0.0 - kernel_sin(r);
    }
    return 0.0 - kernel_cos(r);
}

// cosine; same domain and accuracy as `sin`.
pub fn cos(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    if !trig_domain(x) {
        return float_of(trap());
    }
    let k = quarter_turns(x);
    let r = remainder_of(x, k);
    let q = (k % 4 + 4) % 4;
    if q == 0 {
        return kernel_cos(r);
    }
    if q == 1 {
        return 0.0 - kernel_sin(r);
    }
    if q == 2 {
        return 0.0 - kernel_cos(r);
    }
    return kernel_sin(r);
}
