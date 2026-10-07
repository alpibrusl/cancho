// `docs/f32.md` §5.3: `std.fmt32` -- an `f32` written the way Rust's `{:?}` and
// `{:.N}` write it, and read from a decimal to the nearest `f32` in one
// rounding. Each expected line below is what Rust prints for the same value
// (`f32_oracle.rs` is the machine that checks it for millions of them; this is
// what a reader looks at).
//~ STDOUT debug 0.1
//~ STDOUT debug 1.0
//~ STDOUT debug -0.0
//~ STDOUT debug 16777216.0
//~ STDOUT debug 1e16
//~ STDOUT debug 9999999000000000.0
//~ STDOUT debug 0.0001
//~ STDOUT debug 1e-5
//~ STDOUT debug 3.4028235e38
//~ STDOUT debug 1e-45
//~ STDOUT debug 1.1754944e-38
//~ STDOUT debug 0.3
//~ STDOUT debug NaN
//~ STDOUT debug inf
//~ STDOUT debug -inf
//~ STDOUT debug 123456.79
//~ STDOUT fixed.0 2
//~ STDOUT fixed.2 0.12
//~ STDOUT fixed.4 0.0312
//~ STDOUT fixed.4 0.0938
//~ STDOUT fixed.4 0.0000
//~ STDOUT fixed.4 1.0000
//~ STDOUT fixed.2 -0.00
//~ STDOUT fixed.0 0
//~ STDOUT fixed.0 2
//~ STDOUT fixed.3 340282346638528859811704183484516925440.000
//~ STDOUT fixed.8 0.00000000
//~ STDOUT parse 1065353217
//~ STDOUT parse 1266679808
//~ STDOUT parse 1266679810
//~ STDOUT parse 2139095040
//~ STDOUT parse 0
//~ STDOUT parse 1
//~ STDOUT parse 2147483648
//~ STDOUT parse 1036831949
//~ STDOUT parse 2143289344
//~ STDOUT parse err
//~ STDOUT parse err
//~ STDOUT parse err
//~ STDOUT -1 -1 -1
//~ STDOUT -1 -1 2139095040 0 0
//~ EXIT 0

edition 6;

import std.fmt32;
import std.io;

fn show[&i, &b](io: &!i Io, tag: &static [byte], buf: &b [byte], n: int) -> [io_write] int {
    io.write_all(io, tag);
    io.write_all(io, " ");
    io.write_all(io, buf[0..n]);
    io.newline(io);
    return 0;
}

fn debug[&i, &b](io: &!i Io, buf: &!b [byte], x: f32) -> [io_write] int {
    return show(io, "debug", buf, fmt32.f32_into(buf, x));
}

fn fixed[&i, &b](io: &!i Io, tag: &static [byte], buf: &!b [byte], x: f32, prec: int) -> [io_write] int {
    return show(io, tag, buf, fmt32.f32_fixed_into(buf, x, prec));
}

// The bits of a parse, or "err", or the NaN's.
fn parse[&i, &t](io: &!i Io, text: &t [byte]) -> [io_write] int {
    let (ok, value) = fmt32.f32_of_text(text);
    io.write_all(io, "parse ");
    if !ok {
        io.write_all(io, "err");
    } else {
        io.print_int(io, bits_of32(value));
    }
    io.newline(io);
    return 0;
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
    borrow mut io as &!i in {
        region a {
            let buf = alloc_slice[a](64, byte_of(0));
            debug(i, buf, 0.1f32);
            debug(i, buf, 1.0f32);
            debug(i, buf, -0.0f32);
            debug(i, buf, 16777216.0f32);
            debug(i, buf, 1e16f32);
            debug(i, buf, 9999999e9f32);
            debug(i, buf, 0.0001f32);
            debug(i, buf, 0.00001f32);
            debug(i, buf, 3.4028235e38f32);
            debug(i, buf, f32_of_bits(1));
            debug(i, buf, f32_of_bits(8388608));
            debug(i, buf, 0.1f32 + 0.2f32);
            debug(i, buf, f32_of_bits(2143289344));
            debug(i, buf, f32_of_bits(2139095040));
            debug(i, buf, f32_of_bits(4286578688));
            debug(i, buf, 123456.789f32);
            // Exact ties go to the even digit, as Rust's do: 2.5 is 2,
            // 0.125 is 0.12, 1/32 is 0.0312 and 3/32 is 0.0938.
            fixed(i, "fixed.0", buf, 2.5f32, 0);
            fixed(i, "fixed.2", buf, 0.125f32, 2);
            fixed(i, "fixed.4", buf, 0.03125f32, 4);
            fixed(i, "fixed.4", buf, 0.09375f32, 4);
            // Neither 0.00005 nor 1.00005 is a tie in binary: both `f32`s are a hair
            // below the decimal, so both round down.
            fixed(i, "fixed.4", buf, 0.00005f32, 4);
            fixed(i, "fixed.4", buf, 1.00005f32, 4);
            fixed(i, "fixed.2", buf, -0.001f32, 2);
            fixed(i, "fixed.0", buf, 0.5f32, 0);
            fixed(i, "fixed.0", buf, 1.5f32, 0);
            fixed(i, "fixed.3", buf, 3.4028235e38f32, 3);
            fixed(i, "fixed.8", buf, f32_of_bits(1), 8);
            // One digit above the tie 1 + 2^-24 reads as the number above it:
            // through binary64 it would *be* the tie and round down.
            parse(i, "1.0000000596046447753906251");
            parse(i, "16777217");
            parse(i, "16777219");
            parse(i, "1e39");
            parse(i, "7e-46");
            parse(i, "7.1e-46");
            parse(i, "-0");
            parse(i, "0.1");
            parse(i, "NaN");
            parse(i, "1e");
            parse(i, " 1");
            parse(i, ".");
            // A buffer too short is -1, and so is a negative precision: neither
            // is a trap.
            let small = alloc_slice[a](2, byte_of(0));
            io.print_int(i, fmt32.f32_into(small, 1.5f32));
            io.space(i);
            io.print_int(i, fmt32.f32_fixed_into(small, 1.5f32, 4));
            io.space(i);
            io.print_int(i, fmt32.f32_fixed_into(small, 1.5f32, 0 - 1));
            io.newline(i);
            // Nor is an exponent or a precision at the end of the `int` range.
            io.print_int(i, fmt32.f32_fixed_into(buf, 1.5f32, 9223372036854775807));
            io.space(i);
            io.print_int(i, fmt32.f32_fixed_into(buf, 1.5f32, 9223372036854775806));
            io.space(i);
            io.print_int(i, fmt32.decimal_bits(9223372036854775807, 9223372036854775807));
            io.space(i);
            io.print_int(i, fmt32.decimal_bits(9223372036854775807, 0 - 9223372036854775807));
            io.space(i);
            io.print_int(i, fmt32.decimal_bits(0 - 5, 3));
            io.newline(i);
        }
    }
    release(io);
    return 0;
}
