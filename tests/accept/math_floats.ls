// `docs/float-math.md` §8: `std.math`'s float helpers -- `fabs`, `fmin`,
// `fmax`, `floor`, `ceil`, `round` -- and the edges of `sin`/`cos` that do
// not depend on how many digits are right (their accuracy is measured
// against libm in `conformance/trig.rs`; their trap outside `|x| <= 1e6` is
// `conformance/traps.rs`, because a trapping program is not a fixture).
//
// Every line is `name 1` when the claim holds, so a regression prints which
// claim moved rather than a bare number.
//~ STDOUT fabs-negative 1
//~ STDOUT fabs-positive 1
//~ STDOUT fabs-minus-zero-is-plus-zero 1
//~ STDOUT fabs-nan-stays-nan 1
//~ STDOUT fmin 1
//~ STDOUT fmax 1
//~ STDOUT fmin-ignores-nan 1
//~ STDOUT fmax-ignores-nan 1
//~ STDOUT fmin-both-nan 1
//~ STDOUT floor-positive 1
//~ STDOUT floor-negative 1
//~ STDOUT floor-just-below-zero 1
//~ STDOUT floor-whole 1
//~ STDOUT floor-beyond-2-52 1
//~ STDOUT ceil-positive 1
//~ STDOUT ceil-negative 1
//~ STDOUT ceil-just-above-zero 1
//~ STDOUT ceil-whole 1
//~ STDOUT round-down 1
//~ STDOUT round-up 1
//~ STDOUT round-tie-away-positive 1
//~ STDOUT round-tie-away-negative 1
//~ STDOUT round-just-below-half 1
//~ STDOUT round-just-above-half-negative 1
//~ STDOUT rounding-nan-and-infinity 1
//~ STDOUT sin-zero 1
//~ STDOUT cos-zero 1
//~ STDOUT sin-is-odd 1
//~ STDOUT cos-is-even 1
//~ STDOUT sin-one 1
//~ STDOUT cos-one 1
//~ STDOUT pythagoras 1
//~ STDOUT sin-quarter-turn-is-one 1
//~ STDOUT cos-half-turn-is-minus-one 1
//~ STDOUT sin-nan-is-nan 1
//~ STDOUT cos-nan-is-nan 1
//~ EXIT 0

import std.io;
import std.math;

fn nan() -> [] float {
    return 0.0 / 0.0;
}

fn inf() -> [] float {
    return 1.0 / 0.0;
}

fn flag[&i, &s](io: &!i Io, name: &s [byte], holds: bool) -> [io_write] int {
    io.write_all(io, name);
    io.space(io);
    if holds {
        io.print_int(io, 1);
    } else {
        io.print_int(io, 0);
    }
    io.newline(io);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(heap);
    release(fs);
    release(ffi);
    borrow mut io as &!i in {
        flag(i, "fabs-negative", math.fabs(0.0 - 2.5) == 2.5);
        flag(i, "fabs-positive", math.fabs(2.5) == 2.5);
        flag(i, "fabs-minus-zero-is-plus-zero", bits_of(math.fabs(0.0 - 0.0)) == 0 && bits_of(math.fabs(0.0 * (0.0 - 1.0))) == 0);
        flag(i, "fabs-nan-stays-nan", is_nan(math.fabs(nan())));
        flag(i, "fmin", math.fmin(1.0, 2.0) == 1.0 && math.fmin(2.0, 1.0) == 1.0);
        flag(i, "fmax", math.fmax(1.0, 2.0) == 2.0 && math.fmax(2.0, 1.0) == 2.0);
        flag(i, "fmin-ignores-nan", math.fmin(nan(), 3.0) == 3.0 && math.fmin(3.0, nan()) == 3.0);
        flag(i, "fmax-ignores-nan", math.fmax(nan(), 3.0) == 3.0 && math.fmax(3.0, nan()) == 3.0);
        flag(i, "fmin-both-nan", is_nan(math.fmin(nan(), nan())));
        flag(i, "floor-positive", math.floor(2.5) == 2.0);
        flag(i, "floor-negative", math.floor(0.0 - 2.5) == 0.0 - 3.0);
        flag(i, "floor-just-below-zero", math.floor(0.0 - 0.5) == 0.0 - 1.0);
        flag(i, "floor-whole", math.floor(7.0) == 7.0 && math.floor(0.0 - 7.0) == 0.0 - 7.0);
        flag(i, "floor-beyond-2-52", math.floor(9007199254740993.0) == 9007199254740992.0 && math.floor(1.0e300) == 1.0e300);
        flag(i, "ceil-positive", math.ceil(2.5) == 3.0);
        flag(i, "ceil-negative", math.ceil(0.0 - 2.5) == 0.0 - 2.0);
        flag(i, "ceil-just-above-zero", math.ceil(0.5) == 1.0);
        flag(i, "ceil-whole", math.ceil(7.0) == 7.0);
        flag(i, "round-down", math.round(2.4) == 2.0);
        flag(i, "round-up", math.round(2.6) == 3.0);
        flag(i, "round-tie-away-positive", math.round(2.5) == 3.0 && math.round(0.5) == 1.0);
        flag(i, "round-tie-away-negative", math.round(0.0 - 2.5) == 0.0 - 3.0 && math.round(0.0 - 0.5) == 0.0 - 1.0);
        flag(i, "round-just-below-half", math.round(0.49999999999999994) == 0.0);
        flag(i, "round-just-above-half-negative", math.round(0.0 - 0.49999999999999994) == 0.0);
        flag(i, "rounding-nan-and-infinity", is_nan(math.floor(nan())) && math.ceil(inf()) == inf() && math.round(0.0 - inf()) == 0.0 - inf());
        flag(i, "sin-zero", math.sin(0.0) == 0.0);
        flag(i, "cos-zero", math.cos(0.0) == 1.0);
        flag(i, "sin-is-odd", math.sin(0.0 - 0.7) == 0.0 - math.sin(0.7));
        flag(i, "cos-is-even", math.cos(0.0 - 0.7) == math.cos(0.7));
        flag(i, "sin-one", bits_of(math.sin(1.0)) == 4605754516372524270);
        flag(i, "cos-one", bits_of(math.cos(1.0)) == 4603041830072026764);
        flag(i, "pythagoras", math.fabs(math.sin(2.0) * math.sin(2.0) + math.cos(2.0) * math.cos(2.0) - 1.0) < 1.0e-15);
        flag(i, "sin-quarter-turn-is-one", math.fabs(math.sin(1.5707963267948966) - 1.0) < 1.0e-15);
        flag(i, "cos-half-turn-is-minus-one", math.fabs(math.cos(3.141592653589793) + 1.0) < 1.0e-15);
        flag(i, "sin-nan-is-nan", is_nan(math.sin(nan())));
        flag(i, "cos-nan-is-nan", is_nan(math.cos(nan())));
    }
    release(io);
    return 0;
}
