// `docs/f32.md` §5, F2's gate: `sqrt32`, `f32_of_int` and `int_of_f32`
// against oracles made of integer arithmetic alone.
//
// Usage: `f32_convert_gate <random cases per operation>`. Prints one line
// per operation: `<name> <cases> <bad vs integer oracle> <bad vs the
// binary64 route> <flag> <first input> <first got> <first expected>
// <coverage x5>`. The test reads those lines and requires zero.
//
// Operands are made from bit patterns with `f32_of_bits` (and, for
// `f32_of_int`, from integers at run time), so what runs is each
// backend's real instruction and not a folded constant.
//
//   sqrt32      the correctly rounded square root, from the significand
//               alone: x = m * 2^e with m an integer, e made even, M = m *
//               2^s with 51 or 52 bits, R = floor(sqrt(M)) by the digit
//               recurrence (26 bits: 24 for the answer, one rounding bit,
//               one more), the remainder (R*R != M) the sticky bit. Round
//               to nearest even on those. No floating point anywhere.
//               The second oracle is `f32_of(sqrt(float_of32(x)))`, which
//               Figueroa makes exact (53 >= 2 * 24 + 2) and which LLVM may
//               fold into the instruction under test: so it is a
//               cross-check, and the integer one is the gate.
//   f32_of_int  the integer rounded to 24 significant bits, ties to even,
//               by shifts. The second route is `f32_of(float_of(n))`,
//               which rounds twice (to 53 bits, then to 24) and is
//               counted, not required: it is the reason `f32_of_int` is a
//               builtin, and the count says how often it would be wrong.
//   int_of_f32  toward zero, from the bit fields; second route
//               `truncate(float_of32(x))`. Only inputs below 2^63 in
//               magnitude, since the rest trap and a trap ends the
//               process (the trap cases are `conformance/binary32.rs`'s).
//
// Counters, per operation at `op * 16`: cases, bad against the integer
// oracle, bad against the second route, flag, first input, first got,
// first expected, then five coverage counts.
edition 6;

import std.io;

fn lsr(x: int, k: int) -> [] int {
    if k == 0 {
        return x;
    }
    return x >> k & ~(-1 << 64 - k);
}

fn rnd[&r](st: &!r [int]) -> [] int {
    let s = wrapping_add(st[0], -7046029254386353131);
    st[0] = s;
    var z = s;
    z = wrapping_mul(z ^ lsr(z, 30), -4658895280553007687);
    z = wrapping_mul(z ^ lsr(z, 27), -7723592293110705685);
    return z ^ lsr(z, 31);
}

fn pick[&r](st: &!r [int], n: int) -> [] int {
    return lsr(rnd(st), 1) % n;
}

fn bump[&r](c: &!r [int], at: int) -> [] int {
    c[at] = c[at] + 1;
    return 0;
}

fn record[&r](c: &!r [int], base: int, a: int, got: int, want: int) -> [] int {
    if c[base + 3] == 0 {
        c[base + 3] = 1;
        c[base + 4] = a;
        c[base + 5] = got;
        c[base + 6] = want;
    }
    return 0;
}

fn bit_length(x: int) -> [] int {
    var n = 0;
    var t = x;
    while t != 0 {
        n = n + 1;
        t = t >> 1;
    }
    return n;
}

// floor(sqrt(n)) for 0 <= n < 2^62, the digit-by-digit recurrence.
fn isqrt(n: int) -> [] int {
    var rest = n;
    var root = 0;
    var bit = 1 << 62;
    while bit > rest {
        bit = bit >> 2;
    }
    while bit != 0 {
        if rest >= root + bit {
            rest = rest - (root + bit);
            root = (root >> 1) + bit;
        } else {
            root = root >> 1;
        }
        bit = bit >> 2;
    }
    return root;
}

fn nan_bits(bits: int) -> [] bool {
    return bits & 2147483647 > 2139095040;
}

// The binary32 bits of sqrt(x), by integers alone. A NaN answers 0x7fc00000,
// which is what `bits_of32` reports for every NaN. Coverage slots:
// 8 inexact, 9 an odd exponent made even, 10 subnormal input, 11 rounded
// up, 12 the round-up carried into the next binade.
fn sqrt_bits[&r](c: &!r [int], x: int, count: bool) -> [] int {
    let nan = 2143289344;
    let sign = x >> 31 & 1;
    let ex = x >> 23 & 255;
    let fr = x & 8388607;
    if ex == 255 {
        if fr != 0 || sign == 1 {
            return nan;
        }
        return 2139095040;
    }
    if ex == 0 && fr == 0 {
        return x;
    }
    if sign == 1 {
        return nan;
    }
    var m = fr | 8388608;
    var e = ex - 150;
    if ex == 0 {
        m = fr;
        e = -149;
        while m < 8388608 {
            m = m << 1;
            e = e - 1;
        }
        if count {
            bump(c, 16 + 10);
        }
    }
    if e & 1 == 1 {
        m = m << 1;
        e = e - 1;
        if count {
            bump(c, 16 + 9);
        }
    }
    var s = 28;
    if m >= 16777216 {
        s = 26;
    }
    let big = m << s;
    let root = isqrt(big);
    let inexact = root * root != big;
    var q = root >> 2;
    let low = root & 3;
    if inexact && count {
        bump(c, 16 + 8);
    }
    var up = false;
    if low == 3 || low == 2 && (inexact || q & 1 == 1) {
        up = true;
    }
    var exp = (e - s >> 1) + 2;
    if up {
        q = q + 1;
        if count {
            bump(c, 16 + 11);
        }
    }
    if q == 16777216 {
        q = 8388608;
        exp = exp + 1;
        if count {
            bump(c, 16 + 12);
        }
    }
    return exp + 150 << 23 | q - 8388608;
}

// The binary32 bits of an integer, nearest even, by shifts. Coverage:
// 40 inexact, 41 exact tie, 42 round-up carry, 43 the 2^63 corner,
// 44 negative.
fn int_bits[&r](c: &!r [int], n: int, count: bool) -> [] int {
    if n == 0 {
        return 0;
    }
    if n == -9223372036854775807 - 1 {
        if count {
            bump(c, 32 + 11);
        }
        return 3741319168;
    }
    var sign = 0;
    var mag = n;
    if n < 0 {
        sign = 2147483648;
        mag = 0 - n;
        if count {
            bump(c, 32 + 12);
        }
    }
    var len = bit_length(mag);
    if len <= 24 {
        let whole = mag << 24 - len;
        return sign | len + 126 << 23 | whole - 8388608;
    }
    let shift = len - 24;
    var q = mag >> shift;
    let rest = mag & (1 << shift) - 1;
    let half = 1 << shift - 1;
    if rest != 0 && count {
        bump(c, 32 + 8);
    }
    if rest == half && count {
        bump(c, 32 + 9);
    }
    if rest > half || rest == half && q & 1 == 1 {
        q = q + 1;
    }
    if q == 16777216 {
        q = 8388608;
        len = len + 1;
        if count {
            bump(c, 32 + 10);
        }
    }
    return sign | len + 126 << 23 | q - 8388608;
}

// truncate toward zero from the bit fields; `x` is finite and below 2^63.
fn int_of_bits(x: int) -> [] int {
    let e = x >> 23 & 255;
    if e < 127 {
        return 0;
    }
    let m = x & 8388607 | 8388608;
    let sh = e - 150;
    var mag = 0;
    if sh >= 0 {
        mag = m << sh;
    } else {
        mag = m >> 0 - sh;
    }
    if x >> 31 & 1 == 1 {
        return 0 - mag;
    }
    return mag;
}

fn check_sqrt[&r](c: &!r [int], x: int) -> [] int {
    let base = 16;
    bump(c, base);
    let got = bits_of32(sqrt32(f32_of_bits(x)));
    let want = sqrt_bits(c, x, true);
    let wide = bits_of32(f32_of(sqrt(float_of32(f32_of_bits(x)))));
    if got != want {
        bump(c, base + 1);
        record(c, base, x, got, want);
    }
    if wide != want {
        bump(c, base + 2);
        record(c, base, x, wide, want);
    }
    return 0;
}

fn check_int[&r](c: &!r [int], n: int) -> [] int {
    let base = 32;
    bump(c, base);
    let got = bits_of32(f32_of_int(n));
    let want = int_bits(c, n, true);
    let twice = bits_of32(f32_of(float_of(n)));
    if got != want {
        bump(c, base + 1);
        record(c, base, n, got, want);
    }
    // Counted, not required: the route that rounds twice.
    if twice != want {
        bump(c, base + 2);
    }
    return 0;
}

fn check_trunc[&r](c: &!r [int], x: int) -> [] int {
    let base = 48;
    bump(c, base);
    let f = f32_of_bits(x);
    let got = int_of_f32(f);
    let want = int_of_bits(x);
    let wide = truncate(float_of32(f));
    if got != want {
        bump(c, base + 1);
        record(c, base, x, got, want);
    }
    if wide != want {
        bump(c, base + 2);
    }
    return 0;
}

fn mantissa[&r](st: &!r [int]) -> [] int {
    let k = pick(st, 8);
    if k == 0 {
        return 0;
    }
    if k == 1 {
        return 8388607;
    }
    if k == 2 {
        return 1;
    }
    if k == 3 {
        return 4194304;
    }
    return pick(st, 8388608);
}

fn sqrt_input[&r](st: &!r [int]) -> [] int {
    let m = pick(st, 6);
    if m == 0 {
        return rnd(st) & 4294967295;
    }
    if m == 1 {
        return 1 + pick(st, 254) << 23 | mantissa(st);
    }
    // A perfect square, or one ulp either side.
    if m == 2 {
        let k = 1 + pick(st, 4096);
        let base = int_bits(st, k * k, false);
        return base + pick(st, 3) - 1 + (pick(st, 20) * 2 - 20) * 8388608;
    }
    // The neighbours of a point exactly halfway between two binary32
    // results: (t / 2)^2 with t odd and 25 bits. The top 24 bits of t * t,
    // and the values next to them, are as close to a tie as a binary32 gets.
    if m == 3 {
        let t = 16777216 + pick(st, 16777216) | 1;
        let u = t * t;
        let top = u >> bit_length(u) - 24;
        let e = 60 + pick(st, 134);
        return (e << 23 | top & 8388607) + pick(st, 3) - 1;
    }
    if m == 4 {
        return pick(st, 16777216);
    }
    return pick(st, 255) << 23 | rnd(st) & 8388607;
}

fn int_input[&r](st: &!r [int]) -> [] int {
    let m = pick(st, 6);
    if m == 0 {
        return rnd(st);
    }
    if m == 1 {
        return rnd(st) >> pick(st, 64);
    }
    if m == 2 {
        let k = 24 + pick(st, 39);
        var v = (1 << k) + pick(st, 1025) - 512;
        if pick(st, 2) == 0 {
            v = 0 - v;
        }
        return v;
    }
    if m == 3 {
        let len = 25 + pick(st, 38);
        let shift = len - 24;
        let q = 8388608 + pick(st, 8388608);
        var v = (q << shift) + (1 << shift - 1) + pick(st, 3) - 1;
        if pick(st, 2) == 0 {
            v = 0 - v;
        }
        return v;
    }
    if m == 4 {
        return pick(st, 33554432) - 16777216;
    }
    return lsr(rnd(st), pick(st, 64));
}

// Finite, below 2^63 in magnitude: exponent field at most 189.
fn trunc_input[&r](st: &!r [int]) -> [] int {
    let m = pick(st, 4);
    var e = 100 + pick(st, 90);
    if m == 1 {
        e = 120 + pick(st, 12);
    }
    if m == 2 {
        e = 140 + pick(st, 12);
    }
    return pick(st, 2) << 31 | e << 23 | mantissa(st);
}

fn specials[&r](c: &!r [int]) -> [] int {
    // Every exponent, both signs, seven mantissas: the zeros, subnormals,
    // normals, infinities and NaNs, and the boundaries of each binade.
    var e = 0;
    while e < 256 {
        var k = 0;
        while k < 7 {
            var mant = 0;
            if k == 1 {
                mant = 1;
            }
            if k == 2 {
                mant = 2;
            }
            if k == 3 {
                mant = 4194304;
            }
            if k == 4 {
                mant = 8388607;
            }
            if k == 5 {
                mant = 2097152;
            }
            if k == 6 {
                mant = 5592405;
            }
            check_sqrt(c, e << 23 | mant);
            check_sqrt(c, 2147483648 | e << 23 | mant);
            if e < 190 {
                check_trunc(c, e << 23 | mant);
                check_trunc(c, 2147483648 | e << 23 | mant);
            }
            k = k + 1;
        }
        e = e + 1;
    }
    // Every integer within 8192 of 2^24 and of 2^53, both signs; within
    // 300 of every power of two, and of the ties between neighbours.
    var d = -8192;
    while d <= 8192 {
        check_int(c, (1 << 24) + d);
        check_int(c, 0 - (1 << 24) - d);
        check_int(c, (1 << 53) + d);
        check_int(c, 0 - (1 << 53) - d);
        d = d + 1;
    }
    var k = 0;
    while k < 63 {
        var j = -300;
        while j <= 300 {
            check_int(c, (1 << k) + j);
            check_int(c, 0 - (1 << k) - j);
            j = j + 1;
        }
        k = k + 1;
    }
    check_int(c, 9223372036854775807);
    check_int(c, -9223372036854775807);
    check_int(c, -9223372036854775807 - 1);
    // Every integer up to 2^20 exactly: all of them are exact.
    var n = -1048576;
    while n <= 1048576 {
        check_int(c, n);
        n = n + 1;
    }
    return 0;
}

fn number_of[&r](text: &r [byte]) -> [] int {
    var n = 0;
    var i = 0;
    while i < len(text) {
        n = n * 10 + int_of(text[i]) - 48;
        i = i + 1;
    }
    return n;
}

fn line[&i, &r](io: &!i Io, c: &r [int], name: &r [byte], op: int) -> [io_write] int {
    let base = 16 + op * 16;
    io.write_all(io, name);
    var k = 0;
    while k < 13 {
        io.space(io);
        io.print_int(io, c[base + k]);
        k = k + 1;
    }
    io.newline(io);
    return 0;
}

fn run[&g, &i](args: &g Args, io: &!i Io) -> [args, io_write] int {
    var count = 1000;
    if arg_count(args) > 1 {
        count = number_of(arg(args, 1));
    }
    var status = 0;
    region a {
        let c = alloc_slice[a](128, 0);
        let st = alloc_slice[a](1, 88172645463325252);
        specials(c);
        var n = 0;
        while n < count {
            check_sqrt(c, sqrt_input(st));
            check_int(c, int_input(st));
            check_trunc(c, trunc_input(st));
            n = n + 1;
        }
        // The counters live at `16 + op * 16` here; the `check_*`
        // functions use 16, 32 and 48 for sqrt, int and trunc, which is
        // the same thing.
        line(io, c, "sqrt32", 0);
        line(io, c, "f32_of_int", 1);
        line(io, c, "int_of_f32", 2);
        status = c[17] + c[18] + c[33] + c[49] + c[50];
    }
    return status;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(net);
    release(clock);
    release(signals);
    var status = 0;
    borrow args as &g in {
        borrow mut io as &!i in {
            status = run(g, i);
        }
    }
    release(args);
    release(io);
    if status > 0 {
        return 1;
    }
    return 0;
}
