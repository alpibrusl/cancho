// `docs/f32.md` §2: the `f32` type, shown through the same `int` lines the
// float fixture uses -- an `f32` is printed as its **bits** (`bits_of32`),
// so `-0.0` and `0.0`, and a value one unit off, cannot compare equal by
// accident. The exhaustive evidence is the gate in `crates/lex-sys/tests/
// conformance/binary32.rs` (`f32.md` §5); this is what a reader looks at.
//~ STDOUT half 1056964608
//~ STDOUT thousand 1148846080
//~ STDOUT tenth 1036831949
//~ STDOUT literal-rounded-once 1065353217
//~ STDOUT sum 1050253722
//~ STDOUT ten-tenths 1065353217
//~ STDOUT neg -0.0 2147483648
//~ STDOUT neg-sum 3197737370
//~ STDOUT overflow-is-infinity 2139095040
//~ STDOUT one-over-zero 2139095040
//~ STDOUT nan-is-nan 1
//~ STDOUT nan-equals-itself 0
//~ STDOUT nan-not-equal-itself 1
//~ STDOUT nan-below-one 0
//~ STDOUT minus-zero-equals-zero 1
//~ STDOUT f32-of-tenth 1036831949
//~ STDOUT tenth-widened 4591870180174331904
//~ STDOUT tie-to-even-down 1065353216
//~ STDOUT tie-to-even-up 1065353218
//~ STDOUT f32-of-huge 2139095040
//~ STDOUT subnormal 1
//~ STDOUT from-bits 1065353216
//~ STDOUT low-32-bits-only 1065353216
//~ STDOUT nan-is-canonical 2143289344
//~ STDOUT sqrt-two 1068827891
//~ STDOUT sqrt-minus-one 2143289344
//~ STDOUT sqrt-minus-zero 2147483648
//~ STDOUT sqrt-nine 1077936128
//~ STDOUT int-tie-down 1266679808
//~ STDOUT int-tie-up 1266679810
//~ STDOUT int-rounded-once 1518338049
//~ STDOUT int-rounded-twice 1518338048
//~ STDOUT int-min 3741319168
//~ STDOUT toward-zero -2
//~ STDOUT ten-billion 10000000000
//~ STDOUT field 1080033280
//~ STDOUT slice 1089470464 1077936128
//~ STDOUT ordering 1 0 1
edition 6;

import std.io;

struct Pair {
    a: f32,
    b: f32,
}

fn label[&i, &s](io: &!i Io, name: &s [byte], value: int) -> [io_write] int {
    io.write_all(io, name);
    io.space(io);
    io.print_int(io, value);
    io.newline(io);
    return 0;
}

fn flag(b: bool) -> [] int {
    if b {
        return 1;
    }
    return 0;
}

fn scale(p: Pair, k: f32) -> [] Pair {
    return Pair { a: p.a * k, b: p.b * k };
}

fn sum_of[&r](xs: &r [f32]) -> [] f32 {
    var total = f32_of_bits(0);
    var i = 0;
    while i < len(xs) {
        total = total + xs[i];
        i = i + 1;
    }
    return total;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(signals);

    let nan = f32_of(0.0 / 0.0);
    let inf = 1.0f32 / 0.0f32;
    let tenth = 0.1f32;
    var running = 0.0f32;
    var k = 0;
    while k < 10 {
        running = running + tenth;
        k = k + 1;
    }

    borrow mut io as &!i in {
        label(i, "half", bits_of32(0.5f32));
        label(i, "thousand", bits_of32(1e3f32));
        label(i, "tenth", bits_of32(tenth));
        // One above 1.0 + 2^-24, which is a tie in `f32`. Read as a `float`
        // first it would *become* the tie (binary64 is finer than the
        // decimal) and then round to even, down: so this is 1065353217 only
        // if the literal is rounded once, from its digits.
        label(i, "literal-rounded-once", bits_of32(1.0000000596046447753906251f32));
        label(i, "sum", bits_of32(0.1f32 + 0.2f32));
        label(i, "ten-tenths", bits_of32(running));
        label(i, "neg -0.0", bits_of32(-0.0f32));
        label(i, "neg-sum", bits_of32(-(0.1f32 + 0.2f32)));
        label(i, "overflow-is-infinity", bits_of32(3.4028235e38f32 * 2.0f32));
        label(i, "one-over-zero", bits_of32(inf));
        label(i, "nan-is-nan", flag(is_nan(float_of32(nan))));
        label(i, "nan-equals-itself", flag(nan == nan));
        label(i, "nan-not-equal-itself", flag(nan != nan));
        label(i, "nan-below-one", flag(nan < 1.0f32));
        label(i, "minus-zero-equals-zero", flag(-0.0f32 == 0.0f32));
        label(i, "f32-of-tenth", bits_of32(f32_of(0.1)));
        label(i, "tenth-widened", bits_of(float_of32(tenth)));
        // 1 + 2^-24 is exactly halfway between 1.0 and the next `f32`:
        // ties go to the even mantissa, which is 1.0. 1 + 3 * 2^-24 is
        // halfway between 1 + 2^-23 (odd) and 1 + 2^-22, and goes up.
        label(i, "tie-to-even-down", bits_of32(f32_of(1.000000059604644775390625)));
        label(i, "tie-to-even-up", bits_of32(f32_of(1.000000178813934326171875)));
        label(i, "f32-of-huge", bits_of32(f32_of(1e39)));
        // The smallest subnormal is 2^-149, and `float_of32` is exact: it
        // widens to a positive `float` and not to zero.
        label(i, "subnormal", flag(float_of32(f32_of_bits(1)) > 0.0));
        label(i, "from-bits", bits_of32(f32_of_bits(1065353216)));
        label(i, "low-32-bits-only", bits_of32(f32_of_bits(4294967296 + 1065353216)));
        label(i, "nan-is-canonical", bits_of32(f32_of_bits(4290772992 + 1)));
        // `docs/f32.md` §2: `sqrt32` is one correctly rounded instruction;
        // `f32_of_int` rounds once, from the integer. 2^24 + 1 is a tie and
        // goes down to the even mantissa, 2^24 + 3 a tie that goes up; and
        // 2^54 + 2^30 + 1 is just above a tie, so it rounds up when rounded
        // once and, through binary64 (which drops the 1), to 2^54 when
        // rounded twice.
        label(i, "sqrt-two", bits_of32(sqrt32(2.0f32)));
        label(i, "sqrt-minus-one", bits_of32(sqrt32(-1.0f32)));
        label(i, "sqrt-minus-zero", bits_of32(sqrt32(-0.0f32)));
        label(i, "sqrt-nine", bits_of32(sqrt32(9.0f32)));
        label(i, "int-tie-down", bits_of32(f32_of_int(16777217)));
        label(i, "int-tie-up", bits_of32(f32_of_int(16777219)));
        label(i, "int-rounded-once", bits_of32(f32_of_int(18014398509481984 + 1073741824 + 1)));
        label(i, "int-rounded-twice", bits_of32(f32_of(float_of(18014398509481984 + 1073741824 + 1))));
        label(i, "int-min", bits_of32(f32_of_int(-9223372036854775807 - 1)));
        label(i, "toward-zero", int_of_f32(-2.7f32));
        label(i, "ten-billion", int_of_f32(1e10f32));
        let p = scale(Pair { a: 1.5f32, b: 2.0f32 }, 2.0f32);
        label(i, "field", bits_of32(p.a + 0.5f32));
        region r {
            let xs = alloc_slice[r](3, 1.5f32);
            xs[1] = 2.0f32;
            xs[2] = xs[1] + xs[1];
            io.write_all(i, "slice ");
            io.print_int(i, bits_of32(sum_of(xs)));
            io.space(i);
            io.print_int(i, bits_of32(xs[1] + 1.0f32));
            io.newline(i);
        }
        io.write_all(i, "ordering ");
        io.print_int(i, flag(1.0f32 < 2.0f32));
        io.space(i);
        io.print_int(i, flag(2.0f32 <= 1.0f32));
        io.space(i);
        io.print_int(i, flag(2.0f32 >= 2.0f32));
        io.newline(i);
    }
    release(io);
    return 0;
}
