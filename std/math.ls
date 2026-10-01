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

// `exp`, `log`, `pow` and the functions built on them -- `docs/float-math.md`
// §6 and §9. Library code with a *measured* accuracy, not builtins: none is
// one instruction the way `sqrt` is (§4 there). The algorithms are fdlibm's
// (Sun Microsystems, 1993): argument reduction to a small interval, then a
// short polynomial or rational approximation whose coefficients are a minimax
// fit. Checked against the C library over wide sweeps in
// `crates/lex-sys/tests/conformance/` -- within a few units in the last
// place, not correctly rounded, and the table in §9 says how far.

// `ln2` split into a high part whose low 21 bits are zero and a residual, so
// `k * ln2_hi()` is exact for every `|k| < 2^11` that `exp` and `log`
// produce. fdlibm's constants, bit for bit.
fn ln2_hi() -> [] float {
    return 6.93147180369123816490e-01;
}

fn ln2_lo() -> [] float {
    return 1.90821492927058770002e-10;
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

// e^x: fdlibm's `__ieee754_exp`. Reduce to `x = k*ln2 + r` with
// `|r| <= ln2/2` (`ln2` in two pieces, so `r` is exact to the last bit), then
// `e^r = 1 + r + r*c/(2 - c)` where `c = r - r^2*P(r^2)` and `P` is a
// degree-4 minimax polynomial -- a rational form that is a bit more accurate
// than the Taylor series it replaced, in a quarter of the terms -- and scale
// by `2^k`.
//
// Within 1 ulp of the C library's, measured (`docs/float-math.md` §9); the
// Taylor series this replaces was off by up to 2.4e-14 relative, which is
// about a hundred ulp. `exp(x)` for `x > 709.78` is infinity and for
// `x < -745.13` is zero, by ordinary overflow and underflow in the scaling.
pub fn exp(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    // 750 sits past where `x/ln2`'s rounding could reach out of
    // `round_to_int`'s safe range for an extreme finite `x`, and past where an
    // infinite `x` itself lands -- one guard answers both. It is **not** the
    // overflow boundary: `e^x` overflows to infinity, correctly, anywhere
    // past ~709.78, through the scaling below.
    if x > 750.0 {
        return 1.0 / 0.0;
    }
    if x < -750.0 {
        return 0.0;
    }

    let k = round_to_int(x * 1.44269504088896338700e+00);
    let n = float_of(k);
    let hi = x - n * ln2_hi();
    let lo = n * ln2_lo();
    let r = hi - lo;
    let t = r * r;
    let c = r - t * (1.66666666666666019037e-01 + t * (-2.77777777770155933842e-03 + t * (6.61375632143793436117e-05 + t * (-1.65339022054652515390e-06 + t * 4.13813679705723846039e-08))));
    let y = 1.0 - (lo - r * c / (2.0 - c) - hi);

    // `pow2(k)` alone can overflow even where `y * pow2(k)` would not: `k` can
    // reach 1024, and `2^1024` is already past a `float`'s range though `y`
    // would have brought the product back under it. Splitting the exponent in
    // half keeps every intermediate in range up to the true overflow point
    // (found by this function's own differential test failing at
    // `exp(709.5)`, `float-math.md` §7.1).
    let half = k / 2;
    return y * pow2(k - half) * pow2(half);
}

// `x = 2^k * m` with `m` in `[sqrt(2)/2, sqrt(2)]`, so `log(x) = k*ln2 +
// log(m)` and `log(m)` is a small number the series below handles well. `k`
// is read out of `x`'s own bits (`bits_of`, the one direction this language
// can take a float apart); a subnormal is first scaled up by `2^54`, which is
// exact, because its raw exponent field is zero and carries no `k`. `x` is
// finite and positive here -- the callers have dealt with the rest.
fn split_exponent(x: float) -> [] (int, float) {
    var y = x;
    var adjust = 0;
    if y < 2.2250738585072014e-308 {
        y = y * 18014398509481984.0;
        adjust = 0 - 54;
    }
    let raw = bits_of(y) >> 52 & 0x7ff;
    var k = raw - 1023;
    var m = y / pow2(k);
    if m > 1.4142135623730951 {
        m = m * 0.5;
        k = k + 1;
    }
    return (k + adjust, m);
}

// `s * (f^2/2 + R)` for `f = m - 1`, `m` in `[sqrt(2)/2, sqrt(2)]`: the
// correction `log(1 + f) = f - (f^2/2 - this)` needs. `s = f/(2 + f)` and `R`
// is a degree-14 minimax polynomial in `s^2` (fdlibm's `__ieee754_log`,
// `Lg1..Lg7`). Written this way, not as the series for `log1p`, because the
// terms are added smallest first and nothing larger than `f` is ever
// subtracted from something near it.
fn log_tail(f: float) -> [] float {
    let s = f / (2.0 + f);
    let z = s * s;
    let w = z * z;
    let t1 = w * (3.999999999940941908e-01 + w * (2.222219843214978396e-01 + w * 1.531383769920937332e-01));
    let t2 = z * (6.666666666666735130e-01 + w * (2.857142874366239149e-01 + w * (1.818357216161805012e-01 + w * 1.479819860511658591e-01)));
    return s * (0.5 * f * f + (t2 + t1));
}

// log(m) alone, for `m` in `[sqrt(2)/2, sqrt(2)]`.
fn log_of_mantissa(m: float) -> [] float {
    let f = m - 1.0;
    return f - (0.5 * f * f - log_tail(f));
}

// The natural logarithm: fdlibm's `__ieee754_log`. Within 1 ulp of the C
// library's, measured (`docs/float-math.md` §9) -- including near `x == 1`,
// where `log(x)` is near zero and a relative error that was fine everywhere
// else (the series this replaces: 6e-14, with an absolute floor of 1e-12
// there) is not, and for a subnormal `x`, which it used to get wrong.
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
        // +infinity: answer it directly rather than falling into
        // `pow2` of its own raw exponent.
        return x;
    }

    let (k, m) = split_exponent(x);
    if k == 0 {
        return log_of_mantissa(m);
    }
    // With `k` the two pieces of `ln2` are added in at the right place, so the
    // rounding of `k * ln2` does not land on the result.
    let f = m - 1.0;
    let n = float_of(k);
    return n * ln2_hi() - (0.5 * f * f - (log_tail(f) + n * ln2_lo()) - f);
}

// log base 2. `k + log(m)/ln2`: exact for every power of two (`m == 1`, so
// `log(m)` is zero and what is left is the integer `k`), which `log(x)/ln2`
// is not.
pub fn log2(x: float) -> [] float {
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
        return x;
    }
    let (k, m) = split_exponent(x);
    return float_of(k) + log_of_mantissa(m) * 1.44269504088896338700e+00;
}

// log base 10, with `log10(2)` split in two as fdlibm does, so the `k` part is
// not rounded before the mantissa's is added to it.
pub fn log10(x: float) -> [] float {
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
        return x;
    }
    let (k, m) = split_exponent(x);
    let n = float_of(k);
    return n * 3.01029995663611771306e-01 + (n * 3.69423907715893078616e-13 + log_of_mantissa(m) * 4.34294481903251816668e-01);
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

// The sum of two floats as an exact pair: `s = fl(a + b)` and `a + b = s + e`
// (Knuth's `TwoSum`; no condition on the magnitudes).
fn two_sum(a: float, b: float) -> [] (float, float) {
    let s = a + b;
    let v = s - a;
    return (s, a - (s - v) + (b - v));
}

// `a` with its low 27 bits cleared (Veltkamp's split), so that `hi * hi`,
// `hi * lo` and `lo * lo` of two such halves are all exact. Needs
// `|a| < 2^996` or the scaling overflows.
fn high_half(a: float) -> [] float {
    let c = 134217729.0 * a;
    return c - (c - a);
}

// The product of two floats as an exact pair: `p = fl(a * b)` and `a * b = p
// + e` (Dekker's `TwoProduct`, which needs no fused multiply-add).
fn two_product(a: float, b: float) -> [] (float, float) {
    let p = a * b;
    let ah = high_half(a);
    let al = a - ah;
    let bh = high_half(b);
    let bl = b - bh;
    return (p, ah * bh - p + ah * bl + al * bh + al * bl);
}

// `log(x)` for finite positive `x` as an unevaluated sum `hi + lo`, with `lo`
// far below `hi`'s last bit. `k * ln2` is added to `log(m)` through `two_sum`
// so that its rounding error is kept, not lost; what remains is `log(m)`'s own
// error, about 4e-17 absolute, which is what `pow`'s accuracy rests on.
fn log_pair(x: float) -> [] (float, float) {
    let (k, m) = split_exponent(x);
    let n = float_of(k);
    let (s, e) = two_sum(n * ln2_hi(), log_of_mantissa(m));
    let lo = e + n * ln2_lo();
    let hi = s + lo;
    return (hi, lo - (hi - s));
}

// `x^n` for an integer `n`, by repeated squaring on `two_product`, and whether
// every multiplication was exact. When all were, the result is exactly `x^n`
// -- which is what `pow(7.0, 2.0) == 49.0` and `pow(10.0, 15.0) == 1e15`
// need, and what a logarithm and an exponential, with an error of a fraction
// of an ulp each, cannot promise. (If `x^n` is representable, so is every
// `x^j` on the way to it: fewer significant bits, not more.) A NaN from an
// overflowed split fails the `err != 0.0` test, so it reads as inexact.
fn pow_by_squaring(x: float, n: int) -> [] (float, bool) {
    var e = n;
    if e < 0 {
        e = 0 - e;
    }
    var result = 1.0;
    var base = x;
    var exact = true;
    while e > 0 {
        if e % 2 == 1 {
            let (p, err) = two_product(result, base);
            result = p;
            if err != 0.0 {
                exact = false;
            }
        }
        e = e / 2;
        if e > 0 {
            let (q, err) = two_product(base, base);
            base = q;
            if err != 0.0 {
                exact = false;
            }
        }
    }
    return (result, exact);
}

// `x^y` for `x > 0`: `exp(y * log(x))` with the product `y * log(x)` carried
// as a pair, so its rounding error does not become the answer's. A relative
// error of `d` in the exponent is a relative error of `d` in the result, and
// the exponent is as large as 700 -- computed as one float the result was off
// by 60 ulp at `x = 5e5`, `y = 3.7`, and by 275 for `1.7^-552`. What is left
// is `log`'s own error times `y`, about `y/3` ulp, which is small for the
// exponents programs use. Infinities and the overflow and underflow
// boundaries are settled before the pair is formed.
fn pow_positive(x: float, y: float) -> [] float {
    let ax = fabs(y);
    if ax > 1.7976931348623157e308 {
        // `y` is infinite: the result is 0, 1 or infinity by `x` against 1.
        if x == 1.0 {
            return 1.0;
        }
        if x > 1.0 == y > 0.0 {
            return 1.0 / 0.0;
        }
        return 0.0;
    }
    if x > 1.7976931348623157e308 {
        if y > 0.0 {
            return x;
        }
        return 0.0;
    }
    // The exponents that have a correctly rounded one-instruction answer.
    if y == 1.0 {
        return x;
    }
    if y == 2.0 {
        return x * x;
    }
    if y == 0.0 - 1.0 {
        return 1.0 / x;
    }
    if y == 0.5 {
        return sqrt(x);
    }
    let (lh, ll) = log_pair(x);
    let rough = y * lh;
    if rough > 710.0 {
        return 1.0 / 0.0;
    }
    if rough < 0.0 - 746.0 {
        return 0.0;
    }
    // Past this the split overflows; the product is then so sensitive to
    // `log`'s last bit that nothing finer would mean anything.
    if ax > 1.0e290 {
        return exp(rough);
    }
    // A whole exponent whose power is representable is answered exactly.
    if ax <= 1024.0 && fabs(rough) <= 600.0 && float_of(truncate(y)) == y {
        let n = truncate(y);
        let (q, exact) = pow_by_squaring(x, n);
        if exact {
            if n < 0 {
                return 1.0 / q;
            }
            return q;
        }
    }
    let (ph, pl) = two_product(y, lh);
    let rest = pl + y * ll;
    let zh = ph + rest;
    let zl = rest - (zh - ph);
    return exp(zh) * (1.0 + zl);
}

// `x` to the power `y`. `x > 0` is `pow_positive`; `x <= 0` needs its own
// cases: a negative base is only defined for an integer exponent, and
// `x == 0` and `y == 0` are the two conventions C's `pow` settled that this
// follows rather than reinvents.
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
        return pow_positive(x, y);
    }
    if !is_integer(y) {
        return 0.0 / 0.0;
    }
    let magnitude = pow_positive(0.0 - x, y);
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

// ---- e^x - 1, log(1 + x), and the hyperbolic functions -------------------
//
// `docs/float-math.md` §9. These are the cases where `exp(x) - 1` or
// `log(1 + x)` written out loses every digit: for small `x` the subtraction
// (or the addition) cancels the part of the answer that matters.

// e^x - 1, accurate for small `x`. Kahan's trick: with `u = exp(x)` the
// rounding error in `u` is the same error in `u - 1` and in `log(u)`, and it
// cancels in `(u - 1) * x / log(u)`. Where `u` rounds to exactly 1, `x` itself
// is the answer, and where `u - 1` is -1, so is the answer.
pub fn expm1(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    // Past ln(DBL_MAX) the answer is infinity, and `u` would be too: the
    // formula below would then be inf * x / inf.
    if x > 709.782712893384 {
        return 1.0 / 0.0;
    }
    let u = exp(x);
    if u == 1.0 {
        return x;
    }
    let um1 = u - 1.0;
    if um1 == 0.0 - 1.0 {
        return um1;
    }
    return um1 * x / log(u);
}

// log(1 + x), accurate for small `x`: the same trick as `expm1`, with the
// `1 + x` rounding error cancelling between `log(u)` and `u - 1`. `x < -1` is
// outside the domain (NaN, from `log` of a negative), `x == -1` is `-inf`.
pub fn log1p(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    let u = 1.0 + x;
    if u == 1.0 {
        return x;
    }
    if u > 1.7976931348623157e308 {
        return u;
    }
    return log(u) * x / (u - 1.0);
}

// fdlibm's `__ieee754_sinh`: `expm1` for `|x| < 22`, where `(e^x - e^-x)/2`
// would lose the small answer to cancellation, `exp` beyond, and the last
// sliver below the overflow threshold (710.4758...) done as two half-powers so
// `exp` itself does not overflow first.
pub fn sinh(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    let ax = fabs(x);
    // 2^-28: sinh(x) is x to the last bit below this.
    if ax < 3.725290298461914e-09 {
        return x;
    }
    var h = 0.5;
    if x < 0.0 {
        h = 0.0 - 0.5;
    }
    if ax < 22.0 {
        let t = expm1(ax);
        if ax < 1.0 {
            return h * (2.0 * t - t * t / (t + 1.0));
        }
        return h * (t + t / (t + 1.0));
    }
    if ax < 709.782712893384 {
        return h * exp(ax);
    }
    if ax <= 710.4758600739439 {
        let w = exp(0.5 * ax);
        let t = h * w;
        return t * w;
    }
    return x * (1.0 / 0.0);
}

// fdlibm's `__ieee754_cosh`, in the same three ranges as `sinh`; near zero
// `1 + t^2/(2(1+t))` with `t = expm1(|x|)` keeps the digits `1 + x^2/2` has.
pub fn cosh(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    let ax = fabs(x);
    if ax < 0.34657359027997264 {
        let t = expm1(ax);
        let w = 1.0 + t;
        // 2^-55: cosh(x) is 1 to the last bit below this.
        if ax < 2.7755575615628914e-17 {
            return w;
        }
        return 1.0 + t * t / (w + w);
    }
    if ax < 22.0 {
        let t = exp(ax);
        return 0.5 * t + 0.5 / t;
    }
    if ax < 709.782712893384 {
        return 0.5 * exp(ax);
    }
    if ax <= 710.4758600739439 {
        let w = exp(0.5 * ax);
        let t = 0.5 * w;
        return t * w;
    }
    return 1.0 / 0.0;
}

// fdlibm's `__ieee754_tanh`: `expm1(2|x|)` for the answer's magnitude, and
// exactly +-1 from 22 up, where `1 - 2/(e^{2x} + 1)` is 1 to the last bit.
pub fn tanh(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    let ax = fabs(x);
    if ax < 22.0 {
        if ax < 2.7755575615628914e-17 {
            return x;
        }
        var z = 0.0;
        if ax >= 1.0 {
            let t = expm1(2.0 * ax);
            z = 1.0 - 2.0 / (t + 2.0);
        } else {
            let t = expm1(0.0 - 2.0 * ax);
            z = (0.0 - t) / (t + 2.0);
        }
        if x < 0.0 {
            return 0.0 - z;
        }
        return z;
    }
    if x < 0.0 {
        return 0.0 - 1.0;
    }
    return 1.0;
}

// ln(2), for the large-argument forms of the inverse functions below.
fn ln2() -> [] float {
    return 6.93147180559945286227e-01;
}

// fdlibm's `__ieee754_asinh`: `log(2|x| + 1/(sqrt(x^2+1) + |x|))` past 2,
// `log1p` of a form that keeps the small answer's digits below it, and
// `log(|x|) + ln 2` past 2^28, where `x^2 + 1` would overflow for no benefit.
pub fn asinh(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    let ax = fabs(x);
    if ax < 3.725290298461914e-09 {
        return x;
    }
    var w = 0.0;
    if ax > 268435456.0 {
        w = log(ax) + ln2();
    } else if ax > 2.0 {
        w = log(2.0 * ax + 1.0 / (sqrt(x * x + 1.0) + ax));
    } else {
        let t = x * x;
        w = log1p(ax + t / (1.0 + sqrt(1.0 + t)));
    }
    if x < 0.0 {
        return 0.0 - w;
    }
    return w;
}

// fdlibm's `__ieee754_acosh`. `x < 1` is outside the domain: NaN.
pub fn acosh(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    if x < 1.0 {
        return 0.0 / 0.0;
    }
    if x >= 268435456.0 {
        return log(x) + ln2();
    }
    if x == 1.0 {
        return 0.0;
    }
    if x > 2.0 {
        let t = x * x;
        return log(2.0 * x - 1.0 / (x + sqrt(t - 1.0)));
    }
    let t = x - 1.0;
    return log1p(t + sqrt(2.0 * t + t * t));
}

// fdlibm's `__ieee754_atanh`: `0.5 * log1p(2x/(1-x))`, arranged so the small
// answer keeps its digits. `|x| > 1` is outside the domain (NaN); `|x| == 1`
// is infinity of the sign of `x`.
pub fn atanh(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    let ax = fabs(x);
    if ax > 1.0 {
        return 0.0 / 0.0;
    }
    if ax == 1.0 {
        return x / 0.0;
    }
    if ax < 3.725290298461914e-09 {
        return x;
    }
    var t = 0.0;
    if ax < 0.5 {
        let u = ax + ax;
        t = 0.5 * log1p(u + u * ax / (1.0 - ax));
    } else {
        t = 0.5 * log1p((ax + ax) / (1.0 - ax));
    }
    if x < 0.0 {
        return 0.0 - t;
    }
    return t;
}
