// `docs/float-math.md` §9: what one `std.math` function answers over one
// range of arguments, as the raw bit pattern of each answer, one per line.
//
//     math_samples <function> <range>
//
// A foreign signature cannot carry a `float` (`opaque-pointers.md`), so the C
// library's own answers cannot be called from here. The comparison is made
// from outside: `conformance/mathfn.rs` replays the same arguments -- a fixed
// linear-congruential sequence scaled onto `[centre - width, centre + width]`,
// every step of which is exact or correctly rounded, so Rust and this program
// compute the same argument bit for bit -- and measures the distance from the
// library's answer in units in the last place.
//
// The tables below are repeated in `mathfn.rs`; a mismatch shows up as an
// enormous error rather than a quiet one, because the arguments differ.
import std.io;
import std.math;

fn apply(function: int, x: float) -> [] float {
    if function == 0 {
        return math.sin(x);
    }
    if function == 1 {
        return math.cos(x);
    }
    if function == 2 {
        return math.exp(x);
    }
    if function == 3 {
        return math.log(x);
    }
    if function == 4 {
        return math.expm1(x);
    }
    if function == 5 {
        return math.log1p(x);
    }
    if function == 6 {
        return math.log2(x);
    }
    if function == 7 {
        return math.log10(x);
    }
    if function == 8 {
        return math.sinh(x);
    }
    if function == 9 {
        return math.cosh(x);
    }
    if function == 10 {
        return math.tanh(x);
    }
    if function == 11 {
        return math.asinh(x);
    }
    if function == 12 {
        return math.acosh(x);
    }
    if function == 13 {
        return math.atanh(x);
    }
    if function == 14 {
        return math.pow(x, 3.7);
    }
    if function == 15 {
        return math.pow(x, 0.3);
    }
    if function == 16 {
        return math.pow(1.7, x);
    }
    if function == 17 {
        return math.pow(x, 2.0);
    }
    if function == 18 {
        return math.pow(x, 7.0);
    }
    if function == 19 {
        return math.pow(x, 0.0 - 3.0);
    }
    if function == 20 {
        return math.tan(x);
    }
    if function == 21 {
        return math.asin(x);
    }
    if function == 22 {
        return math.acos(x);
    }
    if function == 23 {
        return math.atan(x);
    }
    if function == 24 {
        return math.atan2(x, 1.7);
    }
    if function == 25 {
        return math.atan2(1.7, x);
    }
    if function == 26 {
        return math.atan2(x, 0.0 - 0.9);
    }
    return 0.0 / 0.0;
}

// `(centre, width)` of each range.
fn range(id: int) -> [] (float, float) {
    if id == 0 {
        return (0.0, 1.0);
    }
    if id == 1 {
        return (0.0, 10.0);
    }
    if id == 2 {
        return (0.0, 1000.0);
    }
    if id == 3 {
        return (0.0, 1000000.0);
    }
    if id == 4 {
        return (157.07963267948966, 0.001);
    }
    if id == 5 {
        return (157079.63267948966, 0.001);
    }
    if id == 6 {
        return (1.0, 0.5);
    }
    if id == 7 {
        return (50.5, 49.5);
    }
    if id == 8 {
        return (500000.0, 500000.0);
    }
    if id == 9 {
        return (0.0, 700.0);
    }
    if id == 10 {
        return (0.0, 30.0);
    }
    if id == 11 {
        return (0.0, 0.001);
    }
    if id == 12 {
        return (1.0, 0.001);
    }
    if id == 13 {
        return (2.0, 1.0);
    }
    if id == 14 {
        return (0.0, 0.999);
    }
    if id == 15 {
        return (0.0, 0.000000001);
    }
    if id == 16 {
        return (0.0005, 0.0005);
    }
    if id == 17 {
        return (0.0 - 720.0, 25.0);
    }
    if id == 18 {
        return (355.0, 355.0);
    }
    if id == 19 {
        return (5.0e299, 5.0e299);
    }
    if id == 20 {
        return (1.5707963267948966, 0.001);
    }
    if id == 21 {
        return (0.99, 0.0099);
    }
    if id == 22 {
        return (500000000.0, 500000000.0);
    }
    return (0.0, 1.0e15);
}

fn next(state: int) -> [] int {
    return (state * 1103515245 + 12345) % 2147483648;
}

fn unit(state: int) -> [] float {
    return float_of(state) / 2147483648.0;
}

fn number_of[&s](text: &s [byte]) -> [] int {
    var n = 0;
    var i = 0;
    while i < len(text) {
        n = n * 10 + (int_of(text[i]) - '0');
        i = i + 1;
    }
    return n;
}

fn sweep[&i](io: &!i Io, function: int, id: int, count: int) -> [io_write] int {
    let (centre, width) = range(id);
    var state = 12345;
    var n = 0;
    while n < count {
        state = next(state);
        let x = centre + (2.0 * unit(state) - 1.0) * width;
        io.print_int(io, bits_of(apply(function, x)));
        io.newline(io);
        n = n + 1;
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(heap);
    release(fs);
    release(ffi);
    var function = 0;
    var id = 0;
    borrow args as &g in {
        function = number_of(arg(g, 1));
        id = number_of(arg(g, 2));
    }
    release(args);
    borrow mut io as &!i in {
        sweep(i, function, id, 10000);
    }
    release(io);
    return 0;
}
