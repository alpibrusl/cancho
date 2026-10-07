// `docs/f32.md` §5.3's driver: `std.fmt32` as a filter, for
// `conformance/f32_text.rs` to hold against Rust's own formatting and
// parsing (`f32_oracle.rs` is the other side, line for line).
//
//     f32_text debug            one `f32_into` per line, from decimal bit patterns
//     f32_text fixed <prec>     one `f32_fixed_into` per line, from the same
//     f32_text parse            one `f32_of_text` per line, from lines of text:
//                               the bits as decimal, or `nan`, or `err`
//
// Reads stdin to the end, writes one line out for every line in. A line over
// 16,000 bytes is answered `toolong` (the oracle never sees one).
edition 6;

import std.io;
import std.fmt32;

// One line into `buf`: how many bytes, -1 at the end of the input, -2 for a
// line that did not fit (the rest of it is read and dropped).
fn read_line[&b, &i](io: &!i Io, buf: &!b [byte]) -> [io_read] int {
    var n = 0;
    var c = getchar(io);
    if c < 0 {
        return 0 - 1;
    }
    var too_long = false;
    while c >= 0 && c != 10 {
        if n < len(buf) {
            buf[n] = byte_of(c);
            n = n + 1;
        } else {
            too_long = true;
        }
        c = getchar(io);
    }
    if too_long {
        return 0 - 2;
    }
    return n;
}

fn number_in[&b](buf: &b [byte], n: int) -> [] int {
    var value = 0;
    var i = 0;
    while i < n {
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

fn run[&g, &i](args: &g Args, io: &!i Io) -> [args, io_read, io_write] int {
    var mode = 0;
    var prec = 0;
    if arg_count(args) > 1 {
        let word = arg(args, 1);
        if is_word(word, "debug") {
            mode = 1;
        }
        if is_word(word, "fixed") {
            mode = 2;
        }
        if is_word(word, "parse") {
            mode = 3;
        }
    }
    if arg_count(args) > 2 {
        let word = arg(args, 2);
        prec = number_in(word, len(word));
    }
    if mode == 0 {
        return 2;
    }
    region a {
        let line = alloc_slice[a](16384, byte_of(0));
        let out = alloc_slice[a](16384, byte_of(0));
        var n = read_line(io, line);
        while n != 0 - 1 {
            var wrote = 0 - 1;
            if n == 0 - 2 {
                wrote = put_text(out, "toolong");
            } else if mode == 1 {
                wrote = fmt32.f32_into(out, f32_of_bits(number_in(line, n)));
            } else if mode == 2 {
                wrote = fmt32.f32_fixed_into(out, f32_of_bits(number_in(line, n)), prec);
            } else {
                wrote = parsed(out, fmt32.f32_of_text(line[0..n]));
            }
            if wrote < 0 {
                wrote = put_text(out, "short");
            }
            out[wrote] = byte_of(10);
            io.write_all(io, out[0..wrote + 1]);
            n = read_line(io, line);
        }
    }
    return 0;
}

// `nan`, or the bits as decimal, or `err`.
fn parsed[&o](out: &!o [byte], result: (bool, f32)) -> [] int {
    let (ok, value) = result;
    if !ok {
        return put_text(out, "err");
    }
    let bits = bits_of32(value);
    if bits & 2147483647 > 2139095040 {
        return put_text(out, "nan");
    }
    var digits = 1;
    var scale = 1;
    while bits / scale >= 10 {
        scale = scale * 10;
        digits = digits + 1;
    }
    var i = 0;
    var rest = bits;
    while i < digits {
        out[digits - 1 - i] = byte_of(48 + rest % 10);
        rest = rest / 10;
        i = i + 1;
    }
    return digits;
}

fn put_text[&o](out: &!o [byte], text: &static [byte]) -> [] int {
    var i = 0;
    while i < len(text) {
        out[i] = text[i];
        i = i + 1;
    }
    return len(text);
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
