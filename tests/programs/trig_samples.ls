// `docs/float-math.md` §8: what `std.math.sin`/`cos` answer at a fixed sample
// of arguments, as the raw bit patterns, one line per argument.
//
// A foreign signature cannot carry a `float` (`opaque-pointers.md`'s list of
// layouts the boundary agrees on is `int`, `bool`, `c_ptr`), so the C
// library's own answers cannot be called from here. The comparison is made
// from outside instead: `conformance/floats.rs::sin_and_cos_agree_with_libm`
// replays the same sample -- the arguments are a fixed linear-congruential
// sequence, and every operation that makes one is exact or correctly rounded,
// so Rust and this program compute the same argument bit for bit -- and
// measures the distance from libm in units in the last place.
//
// Six ranges, `SAMPLES` arguments each: one turn, a few turns, a thousand,
// the whole supported domain, and two neighbourhoods of a multiple of pi/2,
// where argument reduction cancels the most.
import std.io;
import std.math;

fn next(state: int) -> [] int {
    return (state * 1103515245 + 12345) % 2147483648;
}

fn unit(state: int) -> [] float {
    return float_of(state) / 2147483648.0;
}

fn sweep[&i](io: &!i Io, centre: float, width: float, count: int) -> [io_write] int {
    var state = 12345;
    var n = 0;
    while n < count {
        state = next(state);
        let x = centre + (2.0 * unit(state) - 1.0) * width;
        io.print_int(io, bits_of(math.sin(x)));
        io.space(io);
        io.print_int(io, bits_of(math.cos(x)));
        io.newline(io);
        n = n + 1;
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(heap);
    release(fs);
    release(ffi);
    borrow mut io as &!i in {
        sweep(i, 0.0, 1.0, 20000);
        sweep(i, 0.0, 10.0, 20000);
        sweep(i, 0.0, 1000.0, 20000);
        sweep(i, 0.0, 1000000.0, 20000);
        sweep(i, 157.07963267948966, 0.001, 20000);
        sweep(i, 157079.63267948966, 0.001, 20000);
    }
    release(io);
    return 0;
}
