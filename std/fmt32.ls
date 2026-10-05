edition 6;
module std.fmt32;

// `std.fmt32` -- `f32` as text: the shortest decimal that reads back to the
// same `f32`, fixed point with exact ties, and a decimal read directly to
// the nearest `f32` (`docs/f32.md` §5.3).
//
// A module of its own, and not more of `std.fmt`, for one reason: `f32` is
// visible only from edition 6 (`docs/f32.md` §6) and `std/fmt.ls` is not an
// edition 6 file. Raising its edition to add three functions would move a
// module that prints every `float`; a second module moves nothing.
//
// What it writes is **Rust's `{:?}` for an `f32`**, because the program that
// asked (`lexsys-gpu`, #251) must match `lex-gpu`'s Rust byte for byte, and
// that is not `std.fmt.float_into`'s form (`d[.ddd]e[-]k`, which
// `float-printing.md` §1.1 chose to leave positional notation out of):
// positional from `1e-4` up to (not including) `1e16` with at least one digit
// after the point (`0.5`, `16384.0`), exponential outside it (`1e-5`,
// `1.5e16`), `NaN`, `inf`, `-inf`, `0.0` and `-0.0`.
//
// **The digits** are Ryu's (Adams, 2018), at binary32: the shortest decimal
// inside the interval of values that round to this `f32`, the closest to the
// true value when several have that length, found with 64-bit integer
// arithmetic against two small tables of powers of five (no bignum, and no
// loop over digits except to strip them). It differs from the published
// algorithm in exactly one place: when the true value is *exactly* halfway
// between two shortest candidates, Ryu picks the even one and Rust picks
// the larger (Steele and White's rule, `float-printing.md` §3.4), so the tie
// is rounded up here. `docs/f32.md` §5.3 says how this was checked: every
// one of the 2^32 patterns against Rust's own output.
//
// **Fixed point** (`f32_fixed_into`) is `{:.N}`: the exact binary value is
// expanded digit by digit in integer arithmetic and rounded half to even at
// the N-th place, as Rust does, so `0.125` with two places is `0.12` and
// `2.5` with none is `2`.
//
// **Parsing** (`f32_of_text`, `f32_of_decimal`) is correctly rounded
// *directly to binary32*, from the decimal digits, and never through
// binary64. Reading a decimal as a `float` and narrowing rounds twice, and is
// wrong when the first rounding lands exactly on a binary32 midpoint that the
// decimal was not on. The digits are held exactly (up to 160 of them, then a
// sticky bit: no binary32 midpoint has more than 113 significant digits, so
// nothing past 160 can matter except as "more than"), scaled by integer
// arithmetic until the quotient has 26 or 27 bits, and rounded once with
// the remainder as the sticky bit. A fast path takes the common case:
// at most 15 digits and a power of ten up to 22, where binary64 holds both
// operands exactly, so one correctly rounded division or product is within
// half a unit of binary64 of the answer, and rounding that to binary32 is
// the same as rounding the exact value *unless* it landed exactly on a
// binary32 midpoint, which is detected and sent to the exact path.

import std.bignum;
import std.fmt;

static pow5_inv_split: [int] {
    let t = alloc_slice[static](32, 0);
    t[0] = 576460752303423489;
    t[1] = 461168601842738791;
    t[2] = 368934881474191033;
    t[3] = 295147905179352826;
    t[4] = 472236648286964522;
    t[5] = 377789318629571618;
    t[6] = 302231454903657294;
    t[7] = 483570327845851670;
    t[8] = 386856262276681336;
    t[9] = 309485009821345069;
    t[10] = 495176015714152110;
    t[11] = 396140812571321688;
    t[12] = 316912650057057351;
    t[13] = 507060240091291761;
    t[14] = 405648192073033409;
    t[15] = 324518553658426727;
    t[16] = 519229685853482763;
    t[17] = 415383748682786211;
    t[18] = 332306998946228969;
    t[19] = 531691198313966350;
    t[20] = 425352958651173080;
    t[21] = 340282366920938464;
    t[22] = 544451787073501542;
    t[23] = 435561429658801234;
    t[24] = 348449143727040987;
    t[25] = 557518629963265579;
    t[26] = 446014903970612463;
    t[27] = 356811923176489971;
    t[28] = 570899077082383953;
    t[29] = 456719261665907162;
    t[30] = 365375409332725730;
    t[31] = 292300327466180584;
    return t;
}

static pow5_split: [int] {
    let t = alloc_slice[static](50, 0);
    t[0] = 1152921504606846976;
    t[1] = 1441151880758558720;
    t[2] = 1801439850948198400;
    t[3] = 2251799813685248000;
    t[4] = 1407374883553280000;
    t[5] = 1759218604441600000;
    t[6] = 2199023255552000000;
    t[7] = 1374389534720000000;
    t[8] = 1717986918400000000;
    t[9] = 2147483648000000000;
    t[10] = 1342177280000000000;
    t[11] = 1677721600000000000;
    t[12] = 2097152000000000000;
    t[13] = 1310720000000000000;
    t[14] = 1638400000000000000;
    t[15] = 2048000000000000000;
    t[16] = 1280000000000000000;
    t[17] = 1600000000000000000;
    t[18] = 2000000000000000000;
    t[19] = 1250000000000000000;
    t[20] = 1562500000000000000;
    t[21] = 1953125000000000000;
    t[22] = 1220703125000000000;
    t[23] = 1525878906250000000;
    t[24] = 1907348632812500000;
    t[25] = 1192092895507812500;
    t[26] = 1490116119384765625;
    t[27] = 1862645149230957031;
    t[28] = 1164153218269348144;
    t[29] = 1455191522836685180;
    t[30] = 1818989403545856475;
    t[31] = 2273736754432320594;
    t[32] = 1421085471520200371;
    t[33] = 1776356839400250464;
    t[34] = 2220446049250313080;
    t[35] = 1387778780781445675;
    t[36] = 1734723475976807094;
    t[37] = 2168404344971008868;
    t[38] = 1355252715606880542;
    t[39] = 1694065894508600678;
    t[40] = 2117582368135750847;
    t[41] = 1323488980084844279;
    t[42] = 1654361225106055349;
    t[43] = 2067951531382569187;
    t[44] = 1292469707114105741;
    t[45] = 1615587133892632177;
    t[46] = 2019483917365790221;
    t[47] = 1262177448353618888;
    t[48] = 1577721810442023610;
    t[49] = 1972152263052529513;
    return t;
}

// ---------------------------------------------------------------------
// The digits
// ---------------------------------------------------------------------

// ceil(log2(5^e)) for 0 <= e <= 3528, and 1 for e = 0.
fn pow5bits(e: int) -> [] int {
    return (e * 1217359 >> 19) + 1;
}

// floor(log10(2^e)) and floor(log10(5^e)).
fn log10_pow2(e: int) -> [] int {
    return e * 78913 >> 18;
}

fn log10_pow5(e: int) -> [] int {
    return e * 732923 >> 20;
}

// `m * factor >> shift` for `m` under 2^28 and `factor` under 2^62, `shift`
// over 32: the 90-bit product is made from two 32-bit halves of `factor`, so
// no intermediate passes 2^60.
fn mul_shift(m: int, factor: int, shift: int) -> [] int {
    let lo = factor & 0xffffffff;
    let hi = factor >> 32;
    let sum = (m * lo >> 32) + m * hi;
    return sum >> shift - 32;
}

// How many times 5 divides `value` (positive).
fn pow5_factor(value: int) -> [] int {
    var rest = value;
    var count = 0;
    var going = true;
    while going {
        let quotient = rest / 5;
        if rest - quotient * 5 != 0 {
            going = false;
        } else {
            rest = quotient;
            count = count + 1;
        }
    }
    return count;
}

fn multiple_of_pow5(value: int, p: int) -> [] bool {
    return pow5_factor(value) >= p;
}

fn multiple_of_pow2(value: int, p: int) -> [] bool {
    return value & (1 << p) - 1 == 0;
}

// The shortest digits of a finite, nonzero, positive `f32` given as its bit
// pattern: answers `(d, e)` with the value read back from `d * 10^e`.
//
// Ryu (Adams 2018) at binary32. With `m2 * 2^e2` the value, scaled by four so
// the midpoints to both neighbours are integers, `mv`, `mp` and `mm` are the
// value, the midpoint above and the midpoint below, and the three are carried
// through a multiplication by a power of five and a shift to the decimal
// scale `10^e10` where the digit stripping happens. `vr` is the value there,
// `vp` the upper bound, `vm` the lower, and the answer is the shortest `vr`
// that stays between them; `last` is the digit most recently stripped from
// `vr`, which is what rounds it.
//
// The one change from the published algorithm is in the rounding at the
// end (see the module's header): the exact tie rounds up.
pub fn shortest(bits: int) -> [] (int, int) {
    // Not a finite, nonzero value: no digits.
    if bits <= 0 || bits >= 2139095040 {
        return (0, 0);
    }
    let ieee_m = bits & 8388607;
    let ieee_e = bits >> 23 & 255;
    var e2 = 0 - 151;
    var m2 = ieee_m;
    if ieee_e != 0 {
        e2 = ieee_e - 152;
        m2 = 8388608 | ieee_m;
    }
    // An even significand includes its interval's ends: a decimal exactly on
    // the midpoint reads back to the even neighbour, which is this one.
    let accept = m2 & 1 == 0;
    let mv = 4 * m2;
    let mp = mv + 2;
    var mm_shift = 1;
    if ieee_m == 0 && ieee_e > 1 {
        mm_shift = 0;
    }
    let mm = mv - 1 - mm_shift;

    var vr = 0;
    var vp = 0;
    var vm = 0;
    var e10 = 0;
    var vm_zeros = false;
    var vr_zeros = false;
    var last = 0;
    if e2 >= 0 {
        let q = log10_pow2(e2);
        e10 = q;
        let k = 58 + pow5bits(q);
        let i = q + k - e2;
        vr = mul_shift(mv, pow5_inv_split[q], i);
        vp = mul_shift(mp, pow5_inv_split[q], i);
        vm = mul_shift(mm, pow5_inv_split[q], i);
        if q != 0 && (vp - 1) / 10 <= vm / 10 {
            // One removed digit has to be known even if the loop below
            // removes none.
            let l = 58 + pow5bits(q - 1);
            last = mul_shift(mv, pow5_inv_split[q - 1], q - 1 - e2 + l) % 10;
        }
        if q <= 9 {
            // `5^q` can divide at most one of the three.
            if mv % 5 == 0 {
                vr_zeros = multiple_of_pow5(mv, q);
            } else if accept {
                vm_zeros = multiple_of_pow5(mm, q);
            } else if multiple_of_pow5(mp, q) {
                vp = vp - 1;
            }
        }
    } else {
        let q = log10_pow5(0 - e2);
        e10 = q + e2;
        let i = 0 - e2 - q;
        let k = pow5bits(i) - 61;
        let j = q - k;
        vr = mul_shift(mv, pow5_split[i], j);
        vp = mul_shift(mp, pow5_split[i], j);
        vm = mul_shift(mm, pow5_split[i], j);
        if q != 0 && (vp - 1) / 10 <= vm / 10 {
            let j2 = q - 1 - (pow5bits(i + 1) - 61);
            last = mul_shift(mv, pow5_split[i + 1], j2) % 10;
        }
        if q <= 1 {
            // `mv` has at least `q` trailing zero decimal digits: `4 * m2`
            // always has two.
            vr_zeros = true;
            if accept {
                vm_zeros = mm_shift == 1;
            } else {
                vp = vp - 1;
            }
        } else if q < 31 {
            vr_zeros = multiple_of_pow2(mv, q - 1);
        }
    }

    var removed = 0;
    var up = 0;
    if vm_zeros || vr_zeros {
        // The general case, about 4% of values: the exact value or the
        // lower bound has trailing zeros, so the digits removed have to be
        // tracked and not merely discarded.
        while vp / 10 > vm / 10 {
            vm_zeros = vm_zeros && vm % 10 == 0;
            vr_zeros = vr_zeros && last == 0;
            last = vr % 10;
            vr = vr / 10;
            vp = vp / 10;
            vm = vm / 10;
            removed = removed + 1;
        }
        if vm_zeros {
            while vm % 10 == 0 {
                vr_zeros = vr_zeros && last == 0;
                last = vr % 10;
                vr = vr / 10;
                vp = vp / 10;
                vm = vm / 10;
                removed = removed + 1;
            }
        }
        // Here Ryu would round an exact `...50...0` to the even digit; Rust
        // rounds it up, so a `last` of five is simply five.
        if vr == vm && (!accept || !vm_zeros) || last >= 5 {
            up = 1;
        }
    } else {
        // The common case, about 96%: nothing is exact, so the removed
        // digit alone decides.
        while vp / 10 > vm / 10 {
            last = vr % 10;
            vr = vr / 10;
            vp = vp / 10;
            vm = vm / 10;
            removed = removed + 1;
        }
        if vr == vm || last >= 5 {
            up = 1;
        }
    }
    return (vr + up, e10 + removed);
}

fn digit_count(value: int) -> [] int {
    var n = 1;
    var rest = value;
    while rest >= 10 {
        rest = rest / 10;
        n = n + 1;
    }
    return n;
}

// ---------------------------------------------------------------------
// Writing bytes
// ---------------------------------------------------------------------

fn put_byte[&o](out: &!o [byte], at: int, value: int) -> [] int {
    if at < 0 || at >= len(out) {
        return 0 - 1;
    }
    out[at] = byte_of(value);
    return at + 1;
}

// `n` copies of `value`.
fn put_run[&o](out: &!o [byte], at: int, value: int, n: int) -> [] int {
    if at < 0 || at + n > len(out) {
        return 0 - 1;
    }
    var i = 0;
    while i < n {
        out[at + i] = byte_of(value);
        i = i + 1;
    }
    return at + n;
}

// The `n` low decimal digits of `value`, zero padded.
fn put_digits[&o](out: &!o [byte], at: int, value: int, n: int) -> [] int {
    if at < 0 || at + n > len(out) {
        return 0 - 1;
    }
    var rest = value;
    var i = n - 1;
    while i >= 0 {
        out[at + i] = byte_of(48 + rest % 10);
        rest = rest / 10;
        i = i - 1;
    }
    return at + n;
}

fn put_int[&o](out: &!o [byte], at: int, value: int) -> [] int {
    var here = at;
    var rest = value;
    if rest < 0 {
        here = put_byte(out, here, 45);
        rest = 0 - rest;
    }
    return put_digits(out, here, rest, digit_count(rest));
}

// ---------------------------------------------------------------------
// `f32_into`: Rust's `{:?}`
// ---------------------------------------------------------------------

// The bit patterns of `1e-4f32` and `1e16f32`, below and from which `{:?}`
// is exponential. The test pins both against the literals.
fn debug_low() -> [] int {
    return 953267991;
}

fn debug_high() -> [] int {
    return 1510874058;
}

// Write `x` into `out` as Rust's `{:?}` writes an `f32`: the shortest digits
// that read back to the same `f32`, positional from `1e-4` to below `1e16`
// with at least one digit after the point (`0.5`, `16384.0`, `0.0001`),
// exponential outside that (`1e-5`, `1.5e16`, `3.4028235e38`), and `NaN`,
// `inf`, `-inf`, `0.0` and `-0.0`. Answers how many bytes it wrote, or -1
// if `out` is too short; 19 bytes are always enough (the longest, measured
// over every `f32`, is in `docs/f32.md` §5.3).
pub fn f32_into[&o](out: &!o [byte], x: f32) -> [] int {
    let bits = bits_of32(x);
    let magnitude = bits & 2147483647;
    if magnitude > 2139095040 {
        return fmt.put(out, 0, "NaN");
    }
    var at = 0;
    if bits >> 31 == 1 {
        at = put_byte(out, 0, 45);
    }
    if magnitude == 2139095040 {
        return fmt.put(out, at, "inf");
    }
    if magnitude == 0 {
        return fmt.put(out, at, "0.0");
    }
    let (d, e) = shortest(magnitude);
    let n = digit_count(d);
    // `d * 10^e` is `0.d1d2... * 10^k`.
    let k = e + n;
    if magnitude < debug_low() || magnitude >= debug_high() {
        // `d1.d2...e(k-1)`, without a point for one digit.
        var scale = 1;
        var i = 1;
        while i < n {
            scale = scale * 10;
            i = i + 1;
        }
        at = put_byte(out, at, 48 + d / scale);
        if n > 1 {
            at = put_byte(out, at, 46);
            at = put_digits(out, at, d % scale, n - 1);
        }
        at = put_byte(out, at, 101);
        return put_int(out, at, k - 1);
    }
    if k <= 0 {
        // `0.000ddd`
        at = fmt.put(out, at, "0.");
        at = put_run(out, at, 48, 0 - k);
        return put_digits(out, at, d, n);
    }
    if k < n {
        // `dd.ddd`
        var scale = 1;
        var i = k;
        while i < n {
            scale = scale * 10;
            i = i + 1;
        }
        at = put_digits(out, at, d / scale, k);
        at = put_byte(out, at, 46);
        return put_digits(out, at, d % scale, n - k);
    }
    // `ddd000.0`
    at = put_digits(out, at, d, n);
    at = put_run(out, at, 48, k - n);
    return fmt.put(out, at, ".0");
}

// ---------------------------------------------------------------------
// `f32_fixed_into`: Rust's `{:.N}`
// ---------------------------------------------------------------------

// `f = f * 10` over `l` base-2^32 limbs, least significant first, answering
// what carried out of the top limb: when `f` is a fraction scaled by
// 2^(32 l), that is the next decimal digit of it.
fn mul10_digit[&f](f: &!f [int], l: int) -> [] int {
    var carry = 0;
    var i = 0;
    while i < l {
        let product = f[i] * 10 + carry;
        f[i] = product & 0xffffffff;
        carry = product >> 32;
        i = i + 1;
    }
    return carry;
}

// `w = w / 10` over four limbs, answering the remainder.
fn div10[&w](w: &!w [int]) -> [] int {
    var rest = 0;
    var i = 3;
    while i >= 0 {
        let current = rest * 4294967296 + w[i];
        w[i] = current / 10;
        rest = current - w[i] * 10;
        i = i - 1;
    }
    return rest;
}

fn is_zero4[&w](w: &w [int]) -> [] bool {
    return w[0] == 0 && w[1] == 0 && w[2] == 0 && w[3] == 0;
}

// Is the fraction `f` (`l` limbs, scaled by 2^(32 l)) zero, below, exactly
// at, or above one half: 0, 1, 2 or 3 for zero / below / half / above.
fn against_half[&f](f: &f [int], l: int) -> [] int {
    if l == 0 {
        return 0;
    }
    var rest_zero = true;
    var i = 0;
    while i < l - 1 {
        if f[i] != 0 {
            rest_zero = false;
        }
        i = i + 1;
    }
    let top = f[l - 1];
    if top == 0 && rest_zero {
        return 0;
    }
    if top < 2147483648 {
        return 1;
    }
    if top == 2147483648 && rest_zero {
        return 2;
    }
    return 3;
}

// Write `x` into `out` as Rust's `{:.prec$}` writes an `f32`: the exact
// binary value expanded to `prec` places and rounded half to even there
// (`0.125` with two places is `0.12`; `2.5` with none is `2`), the sign kept
// on a negative value that rounds to zero (`-0.00`), and `NaN`, `inf` and
// `-inf` written as they are whatever `prec` is. Answers how many bytes it
// wrote, or -1 if `out` is too short or `prec` is negative.
//
// The value is `m * 2^e` with `m` under 2^24. A non-negative `e` is a whole
// number of at most 128 bits, held in four limbs and turned into digits by
// dividing by ten. A negative `e` leaves a whole part under 2^24 and a
// fraction `f / 2^k` with `k` up to 149, held in at most five limbs scaled so
// that multiplying by ten carries the next digit out of the top limb: no
// division, and no number wider than 160 bits.
pub fn f32_fixed_into[&o](out: &!o [byte], x: f32, prec: int) -> [] int {
    // `prec` places and a point and a digit: nothing shorter is an answer, so
    // a `prec` the buffer cannot hold is refused before any work is done.
    if prec < 0 || prec > 0 && prec > len(out) - 2 {
        return 0 - 1;
    }
    let bits = bits_of32(x);
    let magnitude = bits & 2147483647;
    if magnitude > 2139095040 {
        return fmt.put(out, 0, "NaN");
    }
    var at = 0;
    if bits >> 31 == 1 {
        at = put_byte(out, 0, 45);
    }
    if magnitude == 2139095040 {
        return fmt.put(out, at, "inf");
    }

    let ieee_e = magnitude >> 23;
    var m = magnitude & 8388607;
    var e = 0 - 149;
    if ieee_e != 0 {
        m = m | 8388608;
        e = ieee_e - 150;
    }

    var written = 0 - 1;
    region a {
        // The whole part: four limbs.
        let whole = alloc_slice[a](4, 0);
        // The fraction, as it was and as it is being consumed, and the
        // decimal digits of the whole part, least significant first.
        let first = alloc_slice[a](6, 0);
        let rest = alloc_slice[a](6, 0);
        let digits = alloc_slice[a](48, 0);
        var l = 0;
        if e >= 0 {
            let at_limb = e / 32;
            let shifted = m << e % 32;
            whole[at_limb] = shifted & 0xffffffff;
            if at_limb + 1 < 4 {
                whole[at_limb + 1] = shifted >> 32;
            }
        } else {
            let k = 0 - e;
            var f = m;
            if k < 24 {
                f = m & (1 << k) - 1;
                whole[0] = m >> k;
            }
            l = (k + 31) / 32;
            // `f / 2^k` as `first / 2^(32 l)`.
            let place = 32 * l - k;
            let at_limb = place / 32;
            let shifted = f << place % 32;
            first[at_limb] = shifted & 0xffffffff;
            if at_limb + 1 < l {
                first[at_limb + 1] = shifted >> 32;
            }
        }

        // The whole part's digits.
        var n_whole = 0;
        var going = true;
        while going {
            digits[n_whole] = div10(whole);
            n_whole = n_whole + 1;
            if is_zero4(whole) {
                going = false;
            }
        }

        // First pass over the fraction: which way the last place rounds, and
        // where a carry out of it would stop.
        var p = 0 - 1;
        var last = digits[0];
        var live = true;
        var i = 0;
        bignum.copy(rest, first);
        while i < prec && live {
            let digit = mul10_digit(rest, l);
            last = digit;
            if digit != 9 {
                p = i;
            }
            i = i + 1;
            if against_half(rest, l) == 0 {
                // Nothing left: every further digit is zero, and none is
                // a nine, so a carry stops at the last one.
                live = false;
                if i < prec {
                    last = 0;
                    p = prec - 1;
                }
            }
        }
        let side = against_half(rest, l);
        var round_up = false;
        if side == 3 || side == 2 && last % 2 == 1 {
            round_up = true;
        }
        // A carry that runs out of the fraction goes into the whole part.
        var carry_whole = round_up && p < 0;
        if carry_whole {
            var j = 0;
            var carrying = true;
            while carrying {
                if digits[j] == 9 {
                    digits[j] = 0;
                    j = j + 1;
                    if j == n_whole {
                        digits[j] = 0;
                        n_whole = n_whole + 1;
                    }
                } else {
                    carrying = false;
                }
            }
            digits[j] = digits[j] + 1;
        }

        // Second pass: write it.
        var pos = at;
        var j = n_whole - 1;
        while j >= 0 {
            pos = put_byte(out, pos, 48 + digits[j]);
            j = j - 1;
        }
        if prec > 0 {
            pos = put_byte(out, pos, 46);
            bignum.copy(rest, first);
            var d = 0;
            while d < prec {
                var digit = 0;
                if l > 0 {
                    digit = mul10_digit(rest, l);
                }
                if round_up {
                    if d == p {
                        digit = digit + 1;
                    } else if d > p {
                        digit = 0;
                    }
                }
                pos = put_byte(out, pos, 48 + digit);
                d = d + 1;
            }
        }
        written = pos;
    }
    return written;
}

// ---------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------

// 1e0 ... 1e22: the powers of ten a `float` holds exactly (`5^22 < 2^53`).
static ten_powers: [float] {
    let t = alloc_slice[static](23, 0.0);
    t[0] = 1e0;
    t[1] = 1e1;
    t[2] = 1e2;
    t[3] = 1e3;
    t[4] = 1e4;
    t[5] = 1e5;
    t[6] = 1e6;
    t[7] = 1e7;
    t[8] = 1e8;
    t[9] = 1e9;
    t[10] = 1e10;
    t[11] = 1e11;
    t[12] = 1e12;
    t[13] = 1e13;
    t[14] = 1e14;
    t[15] = 1e15;
    t[16] = 1e16;
    t[17] = 1e17;
    t[18] = 1e18;
    t[19] = 1e19;
    t[20] = 1e20;
    t[21] = 1e21;
    t[22] = 1e22;
    return t;
}

fn bit_length_of(value: int) -> [] int {
    var n = 0;
    var rest = value;
    while rest > 0 {
        n = n + 1;
        rest = rest >> 1;
    }
    return n;
}

// Limbs: the position of the highest set bit plus one, or 0.
fn big_bits[&a](a: &a [int]) -> [] int {
    var i = len(a) - 1;
    while i >= 0 {
        if a[i] != 0 {
            return 32 * i + bit_length_of(a[i]);
        }
        i = i - 1;
    }
    return 0;
}

// `a = a >> 1`.
fn big_shift_right_one[&a](a: &!a [int]) -> [] int {
    var i = 0;
    while i < len(a) - 1 {
        a[i] = a[i] >> 1 | (a[i + 1] & 1) << 31;
        i = i + 1;
    }
    a[len(a) - 1] = a[len(a) - 1] >> 1;
    return 0;
}

// The binary32 bits of `(w + sticky) * 10^q`, rounded to nearest even once, where
// `w` is a positive integer given as limbs, and `sticky` says there was more
// below it (so the value is strictly between `w * 10^q` and `(w + 1) * 10^q`).
// `decimals` is how many decimal digits `w` has, so that the size of the
// answer is known before any arithmetic: a value of `10^39` or more is
// infinity and one under `10^-45` rounds to zero.
//
// The value is `N / D`, with `N = w * 10^q` and `D = 1` for a non-negative `q`,
// else `N = w` and `D = 10^-q`. Both are scaled by a power of two until
// `N / D` has 26 or 27 bits before the point, which is two or three more than
// a binary32 keeps, and the quotient is found by 27 trial subtractions (it is
// under 2^27, so nothing like a division routine is needed). What remains
// after the last is the sticky bit that decides a tie.
fn exact_bits[&w](wl: &w [int], decimals: int, q: int, sticky: bool) -> [] int {
    let x = decimals + q;
    if x > 39 {
        return 2139095040;
    }
    if x < 0 - 45 {
        return 0;
    }
    var p = 0;
    var raised = 0;
    if q >= 0 {
        raised = q;
    } else {
        p = 0 - q;
    }
    // Bits enough for the widest of `N`, `D` and either scaled by two to the
    // `27 + difference of widths`: the wider of the two, plus 28.
    let bits_n = 32 * len(wl) + raised * 3322 / 1000 + 2;
    let bits_d = p * 3322 / 1000 + 2;
    var most = bits_n;
    if bits_d > most {
        most = bits_d;
    }
    let limbs = (most + 28) / 32 + 2;
    var answer = 0;
    region a {
        let n = alloc_slice[a](limbs, 0);
        let d = alloc_slice[a](limbs, 0);
        let t = alloc_slice[a](limbs, 0);
        var i = 0;
        while i < len(wl) {
            n[i] = wl[i];
            i = i + 1;
        }
        d[0] = 1;
        bignum.mul_pow10(n, raised);
        bignum.mul_pow10(d, p);
        let bn = big_bits(n);
        let bd = big_bits(d);
        // Scale by 2^s so that the quotient has 26 or 27 bits.
        let s = 26 + bd - bn;
        if s >= 0 {
            bignum.shift_left(n, s);
        } else {
            bignum.shift_left(d, 0 - s);
        }
        bignum.copy(t, d);
        bignum.shift_left(t, 26);
        var quotient = 0;
        var bit = 26;
        while bit >= 0 {
            if bignum.compare(n, t) >= 0 {
                bignum.subtract(n, t);
                quotient = quotient | 1 << bit;
            }
            big_shift_right_one(t);
            bit = bit - 1;
        }
        // The value is `(quotient + remainder / d) * 2^-s`.
        let more = sticky || !bignum.is_zero(n);
        let top = bit_length_of(quotient) - 1;
        // Bits to drop to reach 24 bits (a normal) or the unit 2^-149 (a
        // subnormal), whichever drops more.
        var drop = top - 23;
        let sub_drop = s - 149;
        let subnormal = sub_drop > drop;
        if subnormal {
            drop = sub_drop;
        }
        if drop >= top + 2 {
            // Under half the unit, whatever is below.
            answer = 0;
        } else {
            var kept = quotient;
            var tail = 0;
            var half = 0;
            if drop > 0 {
                kept = quotient >> drop;
                tail = quotient - (kept << drop);
                half = 1 << drop - 1;
            }
            if drop > 0 && (tail > half || tail == half && (more || kept & 1 == 1)) {
                kept = kept + 1;
            }
            if subnormal {
                // `kept` is a count of 2^-149, and 2^23 of them is the
                // smallest normal, which is how it is written too.
                answer = kept;
            } else {
                var exponent = top - s;
                if kept == 16777216 {
                    kept = 8388608;
                    exponent = exponent + 1;
                }
                if exponent > 127 {
                    answer = 2139095040;
                } else {
                    answer = exponent + 127 << 23 | kept - 8388608;
                }
            }
        }
    }
    return answer;
}

// `float_of(w) * 10^q` or `/ 10^-q` where both operands are exact, and the
// result is already rounded to binary64. The binary32 answer is that rounded
// again, which is the correct one except when the binary64 value is *exactly*
// halfway between two binary32 values: then the true value may have been
// anywhere within half a binary64 unit of it, and the decision is not this
// function's. Answers -1 for that case; the exact path takes it. (Every
// other case is safe because a midpoint is a binary64 too: a midpoint
// strictly between the true value and its binary64 rounding would be nearer
// the true value than the rounding is.)
fn fast_bits(w: int, q: int) -> [] int {
    var r = float_of(w);
    if q >= 0 {
        r = r * ten_powers[q];
    } else {
        r = r / ten_powers[0 - q];
    }
    let raw = bits_of(r);
    if raw & 536870911 == 268435456 {
        return 0 - 1;
    }
    return bits_of32(f32_of(r));
}

// The binary32 bits of the positive decimal `w * 10^q` (`w >= 1`), correctly
// rounded: the fast path where binary64 holds both operands, else the exact one.
pub fn decimal_bits(w: int, q: int) -> [] int {
    if w <= 0 {
        return 0;
    }
    // An exponent this far out is infinity or zero whatever `w` is; saying
    // so first keeps the sum below from overflowing.
    if q > 1000 {
        return 2139095040;
    }
    if q < 0 - 1000 {
        return 0;
    }
    let decimals = digit_count(w);
    let x = decimals + q;
    if x > 39 {
        return 2139095040;
    }
    if x < 0 - 45 {
        return 0;
    }
    if w <= 9007199254740992 {
        if q >= 0 - 22 && q <= 22 {
            let bits = fast_bits(w, q);
            if bits >= 0 {
                return bits;
            }
        } else if q > 22 && q <= 37 {
            // `w * 10^(q - 22)` is still an exact integer in binary64 when
            // it stays under 2^53.
            var scaled = w;
            var left = q - 22;
            while left > 0 && scaled <= 900719925474099 {
                scaled = scaled * 10;
                left = left - 1;
            }
            if left == 0 {
                let bits = fast_bits(scaled, 22);
                if bits >= 0 {
                    return bits;
                }
            }
        }
    }
    var answer = 0;
    region a {
        let wl = alloc_slice[a](2, 0);
        bignum.set(wl, w);
        answer = exact_bits(wl, decimals, q, false);
    }
    return answer;
}

// `w * 10^exp10` as an `f32`, correctly rounded, negated if `negative`.
// `w` is a non-negative `int`; zero gives a signed zero.
pub fn f32_of_decimal(negative: bool, w: int, exp10: int) -> [] f32 {
    var bits = 0;
    if w > 0 {
        bits = decimal_bits(w, exp10);
    }
    if negative {
        bits = bits | 2147483648;
    }
    return f32_of_bits(bits);
}

// Is `text[at]` the letter `lower` in either case?
fn is_letter[&t](text: &t [byte], at: int, lower: int) -> [] bool {
    return int_of(text[at]) | 32 == lower;
}

// Does `text[at..]` spell `word` (lower case, letters only), in either case,
// and nothing more?
fn spells[&t](text: &t [byte], at: int, word: &static [byte]) -> [] bool {
    if len(text) - at != len(word) {
        return false;
    }
    var i = 0;
    while i < len(word) {
        if !is_letter(text, at + i, int_of(word[i])) {
            return false;
        }
        i = i + 1;
    }
    return true;
}

fn is_digit_at[&t](text: &t [byte], at: int) -> [] bool {
    return at < len(text) && int_of(text[at]) >= 48 && int_of(text[at]) <= 57;
}

// Digit `k` of the mantissa, counting those before the point and then those
// after it: `int_from` and `frac_from` are where the two runs start.
fn mantissa_digit[&t](text: &t [byte], int_from: int, int_count: int, frac_from: int, k: int) -> [] int {
    if k < int_count {
        return int_of(text[int_from + k]) - 48;
    }
    return int_of(text[frac_from + k - int_count]) - 48;
}

// Read `text` as Rust's `str::parse::<f32>` does, correctly rounded to the
// nearest `f32` in one step. Answers `(true, value)`, or `(false, 0.0f32)` if
// `text` is not a number.
//
// The grammar is Rust's: an optional sign, then `inf`, `infinity` or `nan`
// in any case, or digits with an optional point (`5.`, `.5` and `5.5`, but not
// `.` alone) and an optional exponent `e` or `E` with an optional sign and at
// least one digit; nothing before, between or after. A magnitude past the
// largest `f32` is infinity and one under half the smallest is zero, both
// signed; an exponent too big to hold is clamped, which cannot change the
// answer.
pub fn f32_of_text[&t](text: &t [byte]) -> [] (bool, f32) {
    let none = f32_of_bits(0);
    var at = 0;
    var negative = false;
    if len(text) > 0 && (text[0] == byte_of(43) || text[0] == byte_of(45)) {
        negative = text[0] == byte_of(45);
        at = 1;
    }
    if spells(text, at, "inf") || spells(text, at, "infinity") {
        var bits = 2139095040;
        if negative {
            bits = bits | 2147483648;
        }
        return (true, f32_of_bits(bits));
    }
    if spells(text, at, "nan") {
        return (true, f32_of_bits(2143289344));
    }

    let int_from = at;
    while is_digit_at(text, at) {
        at = at + 1;
    }
    let int_count = at - int_from;
    var frac_from = at;
    var frac_count = 0;
    if at < len(text) && text[at] == byte_of(46) {
        at = at + 1;
        frac_from = at;
        while is_digit_at(text, at) {
            at = at + 1;
        }
        frac_count = at - frac_from;
    }
    if int_count + frac_count == 0 {
        return (false, none);
    }
    var exponent = 0;
    if at < len(text) && (text[at] == byte_of(101) || text[at] == byte_of(69)) {
        at = at + 1;
        var exponent_negative = false;
        if at < len(text) && (text[at] == byte_of(43) || text[at] == byte_of(45)) {
            exponent_negative = text[at] == byte_of(45);
            at = at + 1;
        }
        if !is_digit_at(text, at) {
            return (false, none);
        }
        while is_digit_at(text, at) {
            // Clamped: past this the answer is zero or infinity whatever
            // the digits say (a mantissa has at most `len(text)` of them).
            if exponent < 100000000 {
                exponent = exponent * 10 + int_of(text[at]) - 48;
            }
            at = at + 1;
        }
        if exponent_negative {
            exponent = 0 - exponent;
        }
    }
    if at != len(text) {
        return (false, none);
    }

    // The significant digits: from the first nonzero to the last nonzero.
    let total = int_count + frac_count;
    var first = 0;
    while first < total && mantissa_digit(text, int_from, int_count, frac_from, first) == 0 {
        first = first + 1;
    }
    if first == total {
        var bits = 0;
        if negative {
            bits = 2147483648;
        }
        return (true, f32_of_bits(bits));
    }
    var last = total - 1;
    while mantissa_digit(text, int_from, int_count, frac_from, last) == 0 {
        last = last - 1;
    }
    let decimals = last - first + 1;
    // `digits * 10^(exponent - frac_count + (total - 1 - last))`.
    let q = exponent - frac_count + total - 1 - last;
    var bits = 0;
    if decimals <= 18 {
        var w = 0;
        var k = first;
        while k <= last {
            w = w * 10 + mantissa_digit(text, int_from, int_count, frac_from, k);
            k = k + 1;
        }
        bits = decimal_bits(w, q);
    } else {
        // At most 160 digits are kept, and a sticky bit says the rest were
        // not all zero (the last one is not).
        var kept = decimals;
        if kept > 160 {
            kept = 160;
        }
        region a {
            let wl = alloc_slice[a](18, 0);
            var k = 0;
            while k < kept {
                bignum.mul_small(wl, 10);
                var carry = mantissa_digit(text, int_from, int_count, frac_from, first + k);
                var j = 0;
                while carry > 0 && j < len(wl) {
                    let sum = wl[j] + carry;
                    wl[j] = sum & 0xffffffff;
                    carry = sum >> 32;
                    j = j + 1;
                }
                k = k + 1;
            }
            bits = exact_bits(wl, kept, q + decimals - kept, decimals > kept);
        }
    }
    if negative {
        bits = bits | 2147483648;
    }
    return (true, f32_of_bits(bits));
}
