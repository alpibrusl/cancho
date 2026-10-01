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

// 2/pi, in 24-bit limbs: `2/pi = sum_j c_j * 2^(-24 (j+1))`, 52 of them, 1,248
// bits -- enough to reduce any finite float, whose largest binary exponent is
// 971 (`reduce_large` needs `q + 10` limbs for exponent `24q + r`). The same
// table fdlibm carries as `ipio2`, generated here with integer arithmetic
// (Machin's formula for pi) rather than copied; the first nine limbs agree
// with fdlibm's, and the whole is checked against libm up to `1e300` by
// `conformance/mathfn.rs`.
static two_over_pi: [int] {
    let t = alloc_slice[static](52, 0);
    t[0] = 0xa2f983;
    t[1] = 0x6e4e44;
    t[2] = 0x1529fc;
    t[3] = 0x2757d1;
    t[4] = 0xf534dd;
    t[5] = 0xc0db62;
    t[6] = 0x95993c;
    t[7] = 0x439041;
    t[8] = 0xfe5163;
    t[9] = 0xabdebb;
    t[10] = 0xc561b7;
    t[11] = 0x246e3a;
    t[12] = 0x424dd2;
    t[13] = 0xe00649;
    t[14] = 0x2eea09;
    t[15] = 0xd1921c;
    t[16] = 0xfe1deb;
    t[17] = 0x1cb129;
    t[18] = 0xa73ee8;
    t[19] = 0x8235f5;
    t[20] = 0x2ebb44;
    t[21] = 0x84e99c;
    t[22] = 0x7026b4;
    t[23] = 0x5f7e41;
    t[24] = 0x3991d6;
    t[25] = 0x398353;
    t[26] = 0x39f49c;
    t[27] = 0x845f8b;
    t[28] = 0xbdf928;
    t[29] = 0x3b1ff8;
    t[30] = 0x97ffde;
    t[31] = 0x05980f;
    t[32] = 0xef2f11;
    t[33] = 0x8b5a0a;
    t[34] = 0x6d1f6d;
    t[35] = 0x367ecf;
    t[36] = 0x27cb09;
    t[37] = 0xb74f46;
    t[38] = 0x3f669e;
    t[39] = 0x5fea2d;
    t[40] = 0x7527ba;
    t[41] = 0xc7ebe5;
    t[42] = 0xf17b3d;
    t[43] = 0x0739f7;
    t[44] = 0x8a5292;
    t[45] = 0xea6bfb;
    t[46] = 0x5fb11f;
    t[47] = 0x8d5d08;
    t[48] = 0x560330;
    t[49] = 0x46fc7b;
    t[50] = 0x6babf0;
    t[51] = 0xcfbc20;
    return t;
}

// `two_over_pi[j]`, or zero off either end of the table: before it, `2/pi` has
// no integer part, and the high limbs of a product past it are never asked for.
fn pio2_limb(j: int) -> [] int {
    if j < 0 || j >= 52 {
        return 0;
    }
    return two_over_pi[j];
}

// Payne and Hanek's reduction, for an `ax` too large for `remainder_of`: the
// quadrant `k` (0..3) and remainder `r`, `|r| <= pi/4`, of `ax` against `pi/2`.
//
// `ax * 2/pi` is a product of a 53-bit integer and a very long fraction, and
// only its fractional part and its integer part mod 4 matter. So it is computed
// exactly where it matters and nowhere else, in base 2^24 so every partial
// product fits an `int`:
//
// * `ax = mant * 2^e`, `e = 24q + r`; `mant * 2^r` is four 24-bit limbs `n_i`;
// * the limb of `ax * 2/pi` at weight `2^(24 s)` is `sum_i n_i * c_(i + q - 1 - s)`
//   plus the carry from the limb below. Limbs of weight `2^24` and up are
//   multiples of 4 and are never computed; limbs below `s = -8` are
//   truncated, an error under 2^-168 against the 2^-115 the worst argument
//   (one whose fraction has 60 leading zero bits) needs;
// * limb 0 gives the quadrant (its low two bits), limbs -1..-8 the fraction.
//
// The fraction is then rounded to the *nearest* quadrant: if it is a half or
// more, the quadrant goes up by one and the remainder is the *negative*
// complement, formed by subtracting the limbs from zero with a borrow (`owed`) -- not
// as `1 - f` in floating point, which would throw away the very bits the
// reduction was run to keep. Converted to a float by Horner's rule from the
// low limb up (so a fraction with leading zero limbs still gets all 53 bits of
// the limbs below them), and times pi/2 in two pieces.
fn reduce_large(ax: float) -> [] (int, float) {
    let bits = bits_of(ax);
    let mant = (bits & 4503599627370495) + 4503599627370496;
    let e = (bits >> 52 & 2047) - 1075;
    var q = e / 24;
    var shift = e - q * 24;
    if shift < 0 {
        shift = shift + 24;
        q = q - 1;
    }
    let mask = 16777215;
    let t0 = (mant & mask) << shift;
    let n0 = t0 & mask;
    let t1 = ((mant >> 24 & mask) << shift) + (t0 >> 24);
    let n1 = t1 & mask;
    let t2 = (mant >> 48 << shift) + (t1 >> 24);
    let n2 = t2 & mask;
    let n3 = t2 >> 24;

    var k = 0;
    var r = 0.0;
    region a {
        let limbs = alloc_slice[a](9, 0);
        var carry = 0;
        var s = 0 - 8;
        while s <= 0 {
            let t = carry + n0 * pio2_limb(q - 1 - s) + n1 * pio2_limb(q - s) + n2 * pio2_limb(q + 1 - s) + n3 * pio2_limb(q + 2 - s);
            limbs[s + 8] = t & mask;
            carry = t >> 24;
            s = s + 1;
        }
        let up = limbs[7] >> 23;
        k = (limbs[8] & 3) + up & 3;
        var owed = 0;
        var f = 0.0;
        var i = 0;
        while i < 8 {
            var d = limbs[i];
            if up == 1 {
                d = 0 - d - owed;
                if d < 0 {
                    d = d + 16777216;
                    owed = 1;
                } else {
                    owed = 0;
                }
            }
            f = (f + float_of(d)) * 5.9604644775390625e-08;
            i = i + 1;
        }
        r = f * 1.57079632679489655800e+00 + f * 6.12323399573676603587e-17;
        if up == 1 {
            r = 0.0 - r;
        }
    }
    return (k, r);
}

// `x = k * pi/2 + r` up to a multiple of `2 pi`: `k` in 0..3 and `|r| <= pi/4`,
// for any finite `x`. `remainder_of`'s three-piece subtraction while `|x|` is
// small enough for it to be exact (`|k| < 2^20`), Payne--Hanek past that.
fn reduce(x: float) -> [] (int, float) {
    if x >= -1.0e6 && x <= 1.0e6 {
        let k = quarter_turns(x);
        return ((k % 4 + 4) % 4, remainder_of(x, k));
    }
    if x < 0.0 {
        let (k, r) = reduce_large(0.0 - x);
        return ((4 - k) % 4, 0.0 - r);
    }
    return reduce_large(x);
}

// sine of any finite `x`; NaN in, NaN out; an infinity has no sine, and is
// NaN as it is for `sqrt(-1)`. Accuracy is measured in `docs/float-math.md`
// §8 and §10.
pub fn sin(x: float) -> [] float {
    if is_nan(x) || fabs(x) > 1.7976931348623157e308 {
        return 0.0 / 0.0;
    }
    let (q, r) = reduce(x);
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
    if is_nan(x) || fabs(x) > 1.7976931348623157e308 {
        return 0.0 / 0.0;
    }
    let (q, r) = reduce(x);
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

// ---- the rest of the trigonometric family --------------------------------
//
// `docs/float-math.md` §10. `tan` shares `sin`/`cos`'s reduction and its domain;
// `atan`, `atan2`, `asin` and `acos` are fdlibm's, and take any finite argument
// (the inverses have no argument-reduction problem: their input is a ratio, not
// an angle).

// The sign bit of a float, so a zero is told apart from its negative -- which
// `x < 0.0` cannot do, and `atan2` must.
fn is_negative(x: float) -> [] bool {
    return bits_of(x) < 0;
}

// tangent of any finite `x`; NaN in, NaN out, an infinity is NaN as for `sin`.
// `sin(r)/cos(r)` for an even quarter turn and
// `-cos(r)/sin(r)` for an odd one, on the same reduced `r` and the same two
// kernels: a quotient of two answers each good to a fraction of an ulp, so it
// is good to about two -- not fdlibm's own `__kernel_tan`, which gets one
// by a longer path and is a separate piece of work. Near an odd multiple of
// pi/2 `r` is tiny but never zero, so the quotient is large and finite, as
// it should be.
pub fn tan(x: float) -> [] float {
    if is_nan(x) || fabs(x) > 1.7976931348623157e308 {
        return 0.0 / 0.0;
    }
    let (q, r) = reduce(x);
    if q % 2 == 0 {
        return kernel_sin(r) / kernel_cos(r);
    }
    return (0.0 - kernel_cos(r)) / kernel_sin(r);
}

// The arctangent. fdlibm's `atan`: past 2^66 the answer is pi/2 to the last
// bit; below 7/16 a single odd polynomial in `x`; between, the argument is
// first mapped to `[-1/2 + , 1/2 -]` around one of four points (`atan(0.5)`,
// `atan(1)`, `atan(1.5)`, `atan(inf)`) by the identity
// `atan(x) = atan(c) + atan((x - c)/(1 + x*c))`, whose constants are kept in
// two pieces each so the sum does not lose their low bits.
pub fn atan(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    let ax = fabs(x);
    if ax >= 7.378697629483821e19 {
        // 2^66: atan is pi/2 here, to the last bit.
        if x < 0.0 {
            return 0.0 - 1.57079632679489655800e+00 - 6.12323399573676603587e-17;
        }
        return 1.57079632679489655800e+00 + 6.12323399573676603587e-17;
    }
    var id = 0 - 1;
    var t = ax;
    if ax < 0.4375 {
        // 2^-27: atan(x) is x to the last bit below this.
        if ax < 7.450580596923828e-09 {
            return x;
        }
    } else if ax < 1.1875 {
        if ax < 0.6875 {
            id = 0;
            t = (2.0 * ax - 1.0) / (2.0 + ax);
        } else {
            id = 1;
            t = (ax - 1.0) / (ax + 1.0);
        }
    } else if ax < 2.4375 {
        id = 2;
        t = (ax - 1.5) / (1.0 + 1.5 * ax);
    } else {
        id = 3;
        t = 0.0 - 1.0 / ax;
    }
    let z = t * t;
    let w = z * z;
    let s1 = z * (3.33333333333329318027e-01 + w * (1.42857142725034663711e-01 + w * (9.09088713343650656196e-02 + w * (6.66107313738753120669e-02 + w * (4.97687799461593236017e-02 + w * 1.62858201153657823623e-02)))));
    let s2 = w * (-1.99999999998764832476e-01 + w * (-1.11111104054623557880e-01 + w * (-7.69187620504482999495e-02 + w * (-5.83357013379057348645e-02 + w * -3.65315727442169155270e-02))));
    if id < 0 {
        let r = t - t * (s1 + s2);
        if x < 0.0 {
            return 0.0 - r;
        }
        return r;
    }
    var hi = 0.0;
    var lo = 0.0;
    if id == 0 {
        hi = 4.63647609000806093515e-01;
        lo = 2.26987774529616870924e-17;
    } else if id == 1 {
        hi = 7.85398163397448278999e-01;
        lo = 3.06161699786838301793e-17;
    } else if id == 2 {
        hi = 9.82793723247329054082e-01;
        lo = 1.39033110312309984516e-17;
    } else {
        hi = 1.57079632679489655800e+00;
        lo = 6.12323399573676603587e-17;
    }
    let r = hi - (t * (s1 + s2) - lo - t);
    if x < 0.0 {
        return 0.0 - r;
    }
    return r;
}

// pi, and the low part of it: `pi() + pi_lo()` is pi to about 107 bits, which
// `atan2` needs because its quadrant adjustments subtract an `atan` from pi.
fn pi() -> [] float {
    return 3.141592653589793116e+00;
}

fn pi_lo() -> [] float {
    return 1.2246467991473531772e-16;
}

// The angle of the point `(x, y)` from the positive x axis, in `(-pi, pi]`;
// `atan2(y, x)`, the argument order C and every other language use.
// fdlibm's `__ieee754_atan2`: every case C's `atan2` settles is settled the
// same way -- the sign of a zero argument decides the half-plane
// (`atan2(+0, -1)` is `pi`, `atan2(-0, -1)` is `-pi`), an infinity argument
// gives one of the eight multiples of pi/4, and otherwise `atan(|y/x|)` is
// moved into the quadrant the two signs name. `y/x` overflowing or
// underflowing is harmless: `atan` of infinity is pi/2 and of zero is zero.
pub fn atan2(y: float, x: float) -> [] float {
    if is_nan(x) || is_nan(y) {
        return 0.0 / 0.0;
    }
    let y_neg = is_negative(y);
    let x_neg = is_negative(x);
    var sign_y = 1.0;
    if y_neg {
        sign_y = 0.0 - 1.0;
    }
    if y == 0.0 {
        if x_neg {
            return sign_y * (pi() + pi_lo());
        }
        return y;
    }
    if x == 0.0 {
        return sign_y * 1.57079632679489655800e+00;
    }
    let inf = 1.7976931348623157e308;
    if fabs(x) > inf {
        if fabs(y) > inf {
            // Both infinite: an odd multiple of pi/4.
            if x_neg {
                return sign_y * 2.35619449019234483700e+00;
            }
            return sign_y * 7.85398163397448278999e-01;
        }
        if x_neg {
            return sign_y * (pi() + pi_lo());
        }
        return sign_y * 0.0;
    }
    if fabs(y) > inf {
        return sign_y * 1.57079632679489655800e+00;
    }
    let ratio = fabs(y / x);
    if ratio > 1.152921504606847e18 {
        // |y/x| > 2^60: the angle is pi/2 whichever side of the y axis `x` is
        // on, to the last bit. Handled here, not left to the quadrant formula
        // below, whose `pi - (pi/2 - ...)` would round the answer a float away
        // from `pi/2` for a ratio this large.
        return sign_y * (1.57079632679489655800e+00 + 0.5 * pi_lo());
    }
    let z = atan(ratio);
    if !x_neg {
        return sign_y * z;
    }
    // Second and third quadrants: `pi - z`, with `pi`'s low part kept.
    return sign_y * (pi() - (z - pi_lo()));
}

// The coefficients `asin` and `acos` share: `R(t) = p(t)/q(t)`, a rational
// minimax approximation of `(asin(x)/x - 1)/x^2` in `t = x^2`.
fn asin_p(t: float) -> [] float {
    return t * (1.66666666666666657415e-01 + t * (-3.25565818622400915405e-01 + t * (2.01212532134862925881e-01 + t * (-4.00555345006794114027e-02 + t * (7.91534994289814532176e-04 + t * 3.47933107596021167570e-05)))));
}

fn asin_q(t: float) -> [] float {
    return 1.0 + t * (-2.40339491173441421878e+00 + t * (2.02094576023350569471e+00 + t * (-6.88283971605453293030e-01 + t * 7.70381505559019352791e-02)));
}

// The arcsine, `|x| <= 1` (NaN beyond). fdlibm's `__ieee754_asin`: a rational
// approximation straight in `x` below one half, and the half-angle identity
// `asin(x) = pi/2 - 2*asin(sqrt((1 - x)/2))` above it, which avoids the
// cancellation `1 - x` would otherwise bring near 1. The square root `s` is
// split into a high half `w` and a remainder, so that `s*s` is recovered to
// more bits than a float holds -- fdlibm clears the low 32 bits of the word;
// this has no way to write one, so it clears the low 27 by Veltkamp's split,
// which does the same job.
pub fn asin(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    let ax = fabs(x);
    if ax > 1.0 {
        return 0.0 / 0.0;
    }
    if ax == 1.0 {
        return x * 1.57079632679489655800e+00 + x * 6.12323399573676603587e-17;
    }
    if ax < 0.5 {
        // 2^-27: asin(x) is x to the last bit below this.
        if ax < 7.450580596923828e-09 {
            return x;
        }
        let t = x * x;
        return x + x * (asin_p(t) / asin_q(t));
    }
    let w = 1.0 - ax;
    let t = w * 0.5;
    let s = sqrt(t);
    var r = 0.0;
    if ax >= 0.975 {
        r = 1.57079632679489655800e+00 - (2.0 * (s + s * (asin_p(t) / asin_q(t))) - 6.12323399573676603587e-17);
    } else {
        let h = high_half(s);
        let c = (t - h * h) / (s + h);
        let p = 2.0 * s * (asin_p(t) / asin_q(t)) - (6.12323399573676603587e-17 - 2.0 * c);
        let q = 7.85398163397448278999e-01 - 2.0 * h;
        r = 7.85398163397448278999e-01 - (p - q);
    }
    if x < 0.0 {
        return 0.0 - r;
    }
    return r;
}

// The arccosine, `|x| <= 1` (NaN beyond). fdlibm's `__ieee754_acos`, in the
// same three pieces as `asin`: straight in `x` below one half, and a
// half-angle form in each direction above it (`x > 0.5` gives a small angle
// from `sqrt((1 - x)/2)`, `x < -0.5` gives `pi` minus one).
pub fn acos(x: float) -> [] float {
    if is_nan(x) {
        return x;
    }
    let ax = fabs(x);
    if ax > 1.0 {
        return 0.0 / 0.0;
    }
    if ax == 1.0 {
        if x > 0.0 {
            return 0.0;
        }
        return pi() + pi_lo();
    }
    if ax < 0.5 {
        // 2^-57: acos(x) is pi/2 to the last bit below this.
        if ax <= 6.938893903907228e-18 {
            return 1.57079632679489655800e+00 + 6.12323399573676603587e-17;
        }
        let z = x * x;
        let r = asin_p(z) / asin_q(z);
        return 1.57079632679489655800e+00 - (x - (6.12323399573676603587e-17 - x * r));
    }
    if x < 0.0 {
        let z = (1.0 + x) * 0.5;
        let s = sqrt(z);
        let w = asin_p(z) / asin_q(z) * s - 6.12323399573676603587e-17;
        return pi() - 2.0 * (s + w);
    }
    let z = (1.0 - x) * 0.5;
    let s = sqrt(z);
    let h = high_half(s);
    let c = (z - h * h) / (s + h);
    let w = asin_p(z) / asin_q(z) * s + c;
    return 2.0 * (h + w);
}
