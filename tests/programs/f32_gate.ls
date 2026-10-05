// `docs/f32.md` §5, F1's gate: every `f32` operation, over random operand
// pairs and every special value, agrees bit for bit with binary64.
//
// Usage: `f32_gate <pairs>`. Prints one line per operation and, last, a
// line `pairs <n>` with how many operand pairs each operation saw.
//
// Operands are made from bit patterns with `f32_of_bits`, so the
// operations under test are the backend's real binary32 instructions and
// not constants the front end folded (an `f32` literal is never folded,
// and nothing here is a literal). Each result is held against **two**
// oracles, and a mismatch against either is a failure:
//
//   1. `f32_of(float_of32(a) OP float_of32(b))`: the oracle `f32.md` §5
//      names, binary64 arithmetic rounded once to binary32. It is exact
//      by Figueroa (53 >= 2 * 24 + 2). It has a flaw as a *test*: an
//      optimiser may legally rewrite `fptrunc(fadd(fpext a, fpext b))`
//      into `fadd a, b` for that very reason, and then this oracle is
//      the instruction under test.
//   2. `round32(bits_of(float_of32(a) OP float_of32(b)))`: the same
//      binary64 result rounded to binary32 **by integer arithmetic**, in
//      this file, round to nearest even. Nothing an optimiser knows about
//      floating point reaches it. It also checks `f32_of` itself.
//
// NaN results are compared as "both NaN": a generated NaN's sign and
// payload belong to the hardware (`floating-point.md` §4.1). Comparisons
// are held against the same comparison on the promoted operands, which is
// exact, and against a comparison of the bit patterns made with integers
// only.
//
// Operation numbers: 0 add, 1 sub, 2 mul, 3 div, then the comparisons
// 4 ==, 5 !=, 6 <, 7 <=, 8 >, 9 >=. Each owns eight counters in `c`:
// pairs, bad against oracle 1, bad against oracle 2, then the first
// failing a, b, got and expected bits, then a flag that says there was one.
edition 6;

import std.io;

fn lsr(x: int, k: int) -> [] int {
    return x >> k & ~(-1 << 64 - k);
}

// splitmix64, in `st[0]`. The seed is fixed, so a failure reproduces.
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

fn make(sign: int, exponent: int, mantissa: int) -> [] int {
    return sign << 31 | exponent << 23 | mantissa;
}

fn clamp(e: int) -> [] int {
    if e < 0 {
        return 0;
    }
    if e > 254 {
        return 254;
    }
    return e;
}

// A mantissa, often a boundary one: the ones where rounding is decided.
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

fn exponent_of(bits: int) -> [] int {
    return bits >> 23 & 255;
}

// The first operand of a pair in operand class `m`.
fn first[&r](st: &!r [int], m: int) -> [] int {
    let sign = pick(st, 2);
    if m == 0 {
        return rnd(st) & 4294967295;
    }
    if m == 2 {
        return make(sign, 117 + pick(st, 21), mantissa(st));
    }
    if m == 3 {
        return make(sign, pick(st, 3), mantissa(st));
    }
    if m == 4 {
        return make(sign, 250 + pick(st, 5), mantissa(st));
    }
    return make(sign, 1 + pick(st, 254), mantissa(st));
}

// The second, which for most classes is chosen *against* the first.
fn second[&r](st: &!r [int], m: int, a: int) -> [] int {
    let sign = pick(st, 2);
    let ea = exponent_of(a);
    if m == 0 {
        return rnd(st) & 4294967295;
    }
    // Close in magnitude: cancellation, and sums whose rounding is
    // decided by the last bit or two.
    if m == 1 {
        return make(sign, clamp(ea + pick(st, 5) - 2), mantissa(st));
    }
    if m == 2 {
        return make(sign, 117 + pick(st, 21), mantissa(st));
    }
    if m == 3 {
        return make(sign, pick(st, 3), mantissa(st));
    }
    if m == 4 {
        return make(sign, 250 + pick(st, 5), mantissa(st));
    }
    // A product that lands at the top or the bottom of the exponent range.
    if m == 5 {
        var base = 128;
        if pick(st, 2) == 0 {
            base = 381;
        }
        return make(sign, clamp(base - ea + pick(st, 5) - 2), mantissa(st));
    }
    // A quotient that does.
    if m == 6 {
        var base = ea - 127;
        if pick(st, 2) == 0 {
            base = ea + 126;
        }
        return make(sign, clamp(base + pick(st, 5) - 2), mantissa(st));
    }
    // 23, 24 or 25 binades below: a sum that is a tie, or a hair either
    // side of one. Half the time a power of two, which is an exact tie.
    if m == 7 {
        var mant = 0;
        if pick(st, 2) == 0 {
            mant = mantissa(st);
        }
        return make(sign, clamp(ea - 24 + pick(st, 3) - 1), mant);
    }
    // The same binade, near the same mantissa: exact differences,
    // subnormal results.
    let near = (a & 8388607) + pick(st, 7) - 3;
    var mant = near;
    if mant < 0 {
        mant = 0;
    }
    if mant > 8388607 {
        mant = 8388607;
    }
    return make(sign, ea, mant);
}

fn bump[&r](c: &!r [int], at: int) -> [] int {
    c[at] = c[at] + 1;
    return 0;
}

fn record[&r](c: &!r [int], base: int, a: int, b: int, got: int, want: int) -> [] int {
    if c[base + 7] == 0 {
        c[base + 7] = 1;
        c[base + 3] = a;
        c[base + 4] = b;
        c[base + 5] = got;
        c[base + 6] = want;
    }
    return 0;
}

// A binary64 value, rounded to binary32 bits, nearest even, by integer
// arithmetic. `d` is not NaN.
fn round32(d: float) -> [] int {
    let bits = bits_of(d);
    let sign = bits >> 63 & 1;
    let e = (bits >> 52 & 2047) - 1023;
    let raw = bits >> 52 & 2047;
    let mant = bits & 4503599627370495;
    let top = sign << 31;
    if raw == 2047 {
        return top | 2139095040;
    }
    // Below every binary64 normal, so far below the smallest binary32
    // subnormal's half.
    if raw == 0 {
        return top;
    }
    let whole = 4503599627370496 + mant;
    if e >= -126 {
        var q = mant >> 29;
        let rest = mant & 536870911;
        if rest > 268435456 || rest == 268435456 && q & 1 == 1 {
            q = q + 1;
        }
        var exp = e;
        if q == 8388608 {
            q = 0;
            exp = exp + 1;
        }
        if exp > 127 {
            return top | 2139095040;
        }
        return top | exp + 127 << 23 | q;
    }
    // Subnormal in binary32: the unit is 2^-149, and `s` is how many low
    // bits of `whole` are below it.
    let s = -e - 97;
    if s >= 54 {
        return top;
    }
    var q = whole >> s;
    let rest = whole & (1 << s) - 1;
    let half = 1 << s - 1;
    if rest > half || rest == half && q & 1 == 1 {
        q = q + 1;
    }
    // `q` may now be 2^23, which is the smallest normal: the encoding
    // already says so.
    return top | q;
}

// What the random pairs actually reached, per arithmetic operation, so
// that "no mismatch" cannot be the product of operands that never made
// the rounding hard: results that were rounded at all, results that were
// exactly halfway between two binary32 values (normal range), results in
// the subnormal range that were rounded, finite operands whose result
// overflowed to infinity, and results that are exactly zero.
// Slots 80..: five per operation, in that order.
fn cover[&r](c: &!r [int], op: int, got: f32, want: float) -> [] int {
    let at = 80 + op * 5;
    let g = float_of32(got);
    let bits = bits_of32(got);
    if want - want == 0.0 {
        if g != want && g - g == 0.0 {
            bump(c, at);
            if bits & 2147483647 < 8388608 {
                bump(c, at + 2);
            }
        }
        if bits_of(want) & 536870911 == 268435456 {
            let e = bits_of(want) >> 52 & 2047;
            if e > 896 && e < 1151 {
                bump(c, at + 1);
            }
        }
        if g - g != 0.0 {
            bump(c, at + 3);
        }
        if want == 0.0 {
            bump(c, at + 4);
        }
    }
    return 0;
}

fn arith[&r](c: &!r [int], op: int, a: int, b: int, got: f32, want: float) -> [] int {
    let base = op * 8;
    bump(c, base);
    let g = float_of32(got);
    let bits = bits_of32(got);
    if is_nan(want) {
        if !is_nan(g) {
            bump(c, base + 1);
            bump(c, base + 2);
            record(c, base, a, b, bits, 2143289344);
        }
        return 0;
    }
    if is_nan(g) {
        bump(c, base + 1);
        bump(c, base + 2);
        record(c, base, a, b, bits, round32(want));
        return 0;
    }
    cover(c, op, got, want);
    if bits != bits_of32(f32_of(want)) {
        bump(c, base + 1);
    }
    if bits != round32(want) {
        bump(c, base + 2);
    }
    if bits != bits_of32(f32_of(want)) || bits != round32(want) {
        record(c, base, a, b, bits, round32(want));
    }
    return 0;
}

// A binary32 pattern as a number that orders like the value: sign and
// magnitude folded into one integer, so `-0` and `+0` meet at 0.
fn key(bits: int) -> [] int {
    let magnitude = bits & 2147483647;
    if bits >> 31 & 1 == 1 {
        return 0 - magnitude;
    }
    return magnitude;
}

fn nan_bits(bits: int) -> [] bool {
    return bits & 2147483647 > 2139095040;
}

// What a comparison must answer, from the bit patterns alone, with no
// floating-point instruction anywhere: NaN is unordered, so every
// comparison but `!=` is false and `!=` is true; otherwise it is the
// comparison of `key`s. This is the oracle that a bug shared by `float`
// and `f32` cannot hide behind -- both of them compare with the same
// code -- which the float oracle below would not see.
fn expected(op: int, a: int, b: int) -> [] bool {
    if nan_bits(a) || nan_bits(b) {
        return op == 5;
    }
    let x = key(a);
    let y = key(b);
    if op == 4 {
        return x == y;
    }
    if op == 5 {
        return x != y;
    }
    if op == 6 {
        return x < y;
    }
    if op == 7 {
        return x <= y;
    }
    if op == 8 {
        return x > y;
    }
    return x >= y;
}

fn compare[&r](c: &!r [int], op: int, a: int, b: int, got: bool, want: bool) -> [] int {
    let base = op * 8;
    bump(c, base);
    let exact = expected(op, a, b);
    var g = 0;
    var w = 0;
    if got {
        g = 1;
    }
    if exact {
        w = 1;
    }
    if got != want {
        bump(c, base + 1);
    }
    if got != exact {
        bump(c, base + 2);
    }
    if got != want || got != exact {
        record(c, base, a, b, g, w);
    }
    return 0;
}

fn pair[&r](c: &!r [int], a_bits: int, b_bits: int) -> [] int {
    let a = f32_of_bits(a_bits);
    let b = f32_of_bits(b_bits);
    let x = float_of32(a);
    let y = float_of32(b);
    arith(c, 0, a_bits, b_bits, a + b, x + y);
    arith(c, 1, a_bits, b_bits, a - b, x - y);
    arith(c, 2, a_bits, b_bits, a * b, x * y);
    arith(c, 3, a_bits, b_bits, a / b, x / y);
    compare(c, 4, a_bits, b_bits, a == b, x == y);
    compare(c, 5, a_bits, b_bits, a != b, x != y);
    compare(c, 6, a_bits, b_bits, a < b, x < y);
    compare(c, 7, a_bits, b_bits, a <= b, x <= y);
    compare(c, 8, a_bits, b_bits, a > b, x > y);
    compare(c, 9, a_bits, b_bits, a >= b, x >= y);
    return 0;
}

// Every special value: both signs of every power of two a binary32 has
// (subnormal ones included), the extremes, the neighbours of one and of
// the integers a binary32 stops representing, the infinities, NaNs of
// both signs and with payloads, and a few ordinary decimals.
fn special[&r](list: &!r [int]) -> [] int {
    var n = 0;
    var e = 0;
    while e < 255 {
        list[n] = make(0, e, 0);
        n = n + 1;
        list[n] = make(1, e, 0);
        n = n + 1;
        e = e + 1;
    }
    // The subnormal powers of two: one set bit in the mantissa.
    var k = 0;
    while k < 23 {
        list[n] = 1 << k;
        n = n + 1;
        list[n] = 1 << k | 2147483648;
        n = n + 1;
        k = k + 1;
    }
    var i = 0;
    while i < 2 {
        let s = i << 31;
        list[n] = s | 1; // smallest subnormal
        n = n + 1;
        list[n] = s | 2;
        n = n + 1;
        list[n] = s | 8388607; // largest subnormal
        n = n + 1;
        list[n] = s | 8388606;
        n = n + 1;
        list[n] = s | 8388609; // smallest normal, and the next
        n = n + 1;
        list[n] = s | 8388610;
        n = n + 1;
        list[n] = s | 8388608 | 8388607; // 2^-125 - ulp
        n = n + 1;
        list[n] = s | 2139095039; // largest finite
        n = n + 1;
        list[n] = s | 2139095038;
        n = n + 1;
        list[n] = s | 2139095040; // infinity
        n = n + 1;
        list[n] = s | 2143289344; // quiet NaN
        n = n + 1;
        list[n] = s | 2143289345; // a payload
        n = n + 1;
        list[n] = s | 2141192192; // signalling-pattern NaN
        n = n + 1;
        list[n] = s | 2147483647; // all ones
        n = n + 1;
        list[n] = s | 1065353217; // 1 + ulp
        n = n + 1;
        list[n] = s | 1065353215; // 1 - ulp
        n = n + 1;
        list[n] = s | 1065353218;
        n = n + 1;
        list[n] = s | 1077936128; // 3
        n = n + 1;
        list[n] = s | 1077936129;
        n = n + 1;
        list[n] = s | 1258291201; // 2^23 + 1
        n = n + 1;
        list[n] = s | 1266679807; // 2^24 - 1
        n = n + 1;
        list[n] = s | 1266679809; // 2^24 + 2
        n = n + 1;
        list[n] = s | 1266679808; // 2^24
        n = n + 1;
        list[n] = s | 1036831949; // 0.1
        n = n + 1;
        list[n] = s | 1045220557; // 0.2
        n = n + 1;
        list[n] = s | 1050253722; // 0.3
        n = n + 1;
        list[n] = s | 1078530011; // pi
        n = n + 1;
        list[n] = s | 1076754516; // e
        n = n + 1;
        list[n] = s | 1060439283; // 1 / sqrt 2
        n = n + 1;
        i = i + 1;
    }
    return n;
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

fn run[&g, &i](args: &g Args, io: &!i Io) -> [args, io_write] int {
    var count = 1000;
    if arg_count(args) > 1 {
        count = number_of(arg(args, 1));
    }
    var status = 0;
    region a {
        let c = alloc_slice[a](128, 0);
        let st = alloc_slice[a](1, 88172645463325252);
        let list = alloc_slice[a](1024, 0);
        let total = special(list);

        // Every special against every special, in both orders.
        var x = 0;
        while x < total {
            var y = 0;
            while y < total {
                pair(c, list[x], list[y]);
                y = y + 1;
            }
            x = x + 1;
        }

        var n = 0;
        while n < count {
            let m = pick(st, 9);
            let p = first(st, m);
            let q = second(st, m, p);
            pair(c, p, q);
            n = n + 1;
        }

        var cv = 0;
        while cv < 4 {
            let at = 80 + cv * 5;
            io.write_all(io, "cover ");
            io.print_int(io, cv);
            io.space(io);
            io.print_int(io, c[at]);
            io.space(io);
            io.print_int(io, c[at + 1]);
            io.space(io);
            io.print_int(io, c[at + 2]);
            io.space(io);
            io.print_int(io, c[at + 3]);
            io.space(io);
            io.print_int(io, c[at + 4]);
            io.newline(io);
            cv = cv + 1;
        }

        var op = 0;
        while op < 10 {
            let base = op * 8;
            io.print_int(io, op);
            io.space(io);
            io.print_int(io, c[base]);
            io.space(io);
            io.print_int(io, c[base + 1]);
            io.space(io);
            io.print_int(io, c[base + 2]);
            io.space(io);
            io.print_int(io, c[base + 3]);
            io.space(io);
            io.print_int(io, c[base + 4]);
            io.space(io);
            io.print_int(io, c[base + 5]);
            io.space(io);
            io.print_int(io, c[base + 6]);
            io.newline(io);
            status = status + c[base + 1] + c[base + 2];
            op = op + 1;
        }
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
