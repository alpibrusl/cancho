// `docs/f32.md` §5.3's gate (1): `std.fmt32` over every `f32` bit pattern, or over
// the sample set of the default suite.
//
//     f32_exhaustive range <lo> <hi>             the patterns lo..hi
//     f32_exhaustive sample <stride> <count>     for each sign, each exponent
//         field 0..=255: the fractions `j * stride & 0x7fffff` for `j < count`,
//         then nineteen boundary ones (`f32_oracle.rs` builds the same set)
//
// NaNs are skipped. For every other pattern `f32_into` is written into a
// buffer and
//
//   * hashed, FNV-1a 64 over the bytes and a newline, so that the one line
//     printed at the end can be held against Rust's own `{:?}` over the same
//     patterns (`f32_oracle range` or `sample`): equal digests mean every
//     line was byte for byte the same, including *which* of several shortest
//     strings was chosen;
//   * read back with `f32_of_text`, which must give the same bits (a zero's
//     sign included), and
//   * checked to be shortest, independently of how it was found: if it has
//     more than one digit, then neither of the two decimals one digit shorter
//     that neighbour it (the digits without the last, and that plus one, at the
//     next power of ten) may read back to the same bits. The set of reals that
//     read back to one `f32` is an interval, so no shorter decimal at all can
//     read back if these two do not.
//
// Output: `lines <n> <hash> unread <n> longer <n> longest <bytes> widest
// <digits> first <pattern>` (`first` is the first failing pattern, or -1).
edition 6;

import std.io;
import std.fmt32;

fn number_in[&b](buf: &b [byte]) -> [] int {
    var value = 0;
    var i = 0;
    while i < len(buf) {
        value = value * 10 + int_of(buf[i]) - 48;
        i = i + 1;
    }
    return value;
}

fn is_word[&a](arg: &a [byte], word: &static [byte]) -> [] bool {
    if len(arg) != len(word) {
        return false;
    }
    var i = 0;
    while i < len(arg) {
        if arg[i] != word[i] {
            return false;
        }
        i = i + 1;
    }
    return true;
}

// The counters, in `c`: 0 lines, 1 hash, 2 unread, 3 longer, 4 longest, 5
// widest, 6 first failing pattern.
fn check[&r](c: &!r [int], buf: &!r [byte], pattern: int) -> [] int {
    let x = f32_of_bits(pattern);
    let n = fmt32.f32_into(buf, x);
    var h = c[1];
    var i = 0;
    while i < n {
        h = wrapping_mul(h ^ int_of(buf[i]), 1099511628211);
        i = i + 1;
    }
    c[1] = wrapping_mul(h ^ 10, 1099511628211);
    c[0] = c[0] + 1;
    if n > c[4] {
        c[4] = n;
    }
    var bad = false;
    let (ok, y) = fmt32.f32_of_text(buf[0..n]);
    if !ok || bits_of32(y) != pattern {
        c[2] = c[2] + 1;
        bad = true;
    }
    let magnitude = pattern & 2147483647;
    if magnitude != 0 && magnitude != 2139095040 {
        let (d, e) = fmt32.shortest(magnitude);
        var digits = 1;
        var rest = d;
        while rest >= 10 {
            rest = rest / 10;
            digits = digits + 1;
        }
        if digits > c[5] {
            c[5] = digits;
        }
        if digits > 1 {
            let below = d / 10;
            if fmt32.decimal_bits(below, e + 1) == magnitude || fmt32.decimal_bits(below + 1, e + 1) == magnitude {
                c[3] = c[3] + 1;
                bad = true;
            }
        }
    }
    if bad && c[6] < 0 {
        c[6] = pattern;
    }
    return 0;
}

fn is_nan_bits(pattern: int) -> [] bool {
    return pattern & 2147483647 > 2139095040;
}

fn run[&g, &i](args: &g Args, io: &!i Io) -> [args, io_write] int {
    if arg_count(args) < 4 {
        return 2;
    }
    let mode = arg(args, 1);
    let first = number_in(arg(args, 2));
    let second = number_in(arg(args, 3));
    var status = 0;
    region a {
        let c = alloc_slice[a](8, 0);
        let buf = alloc_slice[a](64, byte_of(0));
        c[1] = -3750763034362895579;
        c[6] = 0 - 1;
        if is_word(mode, "range") {
            var p = first;
            while p < second {
                if !is_nan_bits(p) {
                    check(c, buf, p);
                }
                p = p + 1;
            }
        } else if is_word(mode, "sample") {
            var sign = 0;
            while sign < 2 {
                var e = 0;
                while e < 256 {
                    var j = 0;
                    while j < second + 19 {
                        var fraction = 0;
                        if j < second {
                            fraction = j * first & 8388607;
                        } else {
                            fraction = boundary(j - second);
                        }
                        let pattern = sign << 31 | e << 23 | fraction;
                        if !is_nan_bits(pattern) {
                            check(c, buf, pattern);
                        }
                        j = j + 1;
                    }
                    e = e + 1;
                }
                sign = sign + 1;
            }
        } else {
            status = 2;
        }
        io.write_all(io, "lines ");
        io.print_int(io, c[0]);
        io.write_all(io, " ");
        io.print_int(io, c[1]);
        io.write_all(io, " unread ");
        io.print_int(io, c[2]);
        io.write_all(io, " longer ");
        io.print_int(io, c[3]);
        io.write_all(io, " longest ");
        io.print_int(io, c[4]);
        io.write_all(io, " widest ");
        io.print_int(io, c[5]);
        io.write_all(io, " first ");
        io.print_int(io, c[6]);
        io.newline(io);
        if c[2] + c[3] > 0 {
            status = 1;
        }
    }
    return status;
}

// The nineteen boundary fractions: the first eight, the two around the middle
// and the middle itself, and the last eight.
fn boundary(k: int) -> [] int {
    if k < 8 {
        return k;
    }
    if k == 8 {
        return 4194303;
    }
    if k == 9 {
        return 4194304;
    }
    if k == 10 {
        return 4194305;
    }
    return 8388607 - (18 - k);
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
    return status;
}
