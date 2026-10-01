// `docs/float-math.md` §8: `std.math`'s float helpers -- `fabs`, `fmin`,
// `fmax`, `floor`, `ceil`, `round` -- and the edges of `sin`/`cos` that do
// not depend on how many digits are right (their accuracy is measured
// against libm in `conformance/mathfn.rs`; their trap outside `|x| <= 1e6` is
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
//~ STDOUT exp-of-zero-is-one 1
//~ STDOUT exp-overflows-to-infinity 1
//~ STDOUT exp-underflows-to-zero 1
//~ STDOUT exp-of-minus-infinity 1
//~ STDOUT exp-of-nan 1
//~ STDOUT log-of-one-is-zero 1
//~ STDOUT log-of-zero-is-minus-infinity 1
//~ STDOUT log-of-negative-is-nan 1
//~ STDOUT log-of-infinity 1
//~ STDOUT log-of-the-smallest-subnormal 1
//~ STDOUT log2-of-a-power-of-two-is-exact 1
//~ STDOUT log10-of-a-power-of-ten 1
//~ STDOUT expm1-small-keeps-its-digits 1
//~ STDOUT expm1-of-minus-infinity 1
//~ STDOUT expm1-overflows 1
//~ STDOUT log1p-small-keeps-its-digits 1
//~ STDOUT log1p-of-minus-one 1
//~ STDOUT log1p-below-minus-one-is-nan 1
//~ STDOUT sinh-cosh-tanh-at-zero 1
//~ STDOUT sinh-is-odd-cosh-is-even 1
//~ STDOUT tanh-saturates 1
//~ STDOUT sinh-and-cosh-overflow 1
//~ STDOUT sinh-near-the-overflow-edge 1
//~ STDOUT hyperbolic-identity 1
//~ STDOUT asinh-inverts-sinh 1
//~ STDOUT acosh-inverts-cosh 1
//~ STDOUT atanh-inverts-tanh 1
//~ STDOUT acosh-of-one-is-zero 1
//~ STDOUT acosh-below-one-is-nan 1
//~ STDOUT atanh-at-the-ends 1
//~ STDOUT atanh-beyond-the-ends-is-nan 1
//~ STDOUT asinh-is-odd 1
//~ STDOUT pow-of-small-integers-is-exact 1
//~ STDOUT pow-of-two-is-exact 1
//~ STDOUT pow-negative-base 1
//~ STDOUT pow-zero-and-one 1
//~ STDOUT pow-infinite-exponent 1
//~ STDOUT pow-infinite-base 1
//~ STDOUT pow-overflows-and-underflows 1
//~ STDOUT pow-square-root 1
//~ STDOUT pow-nan 1
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
        flag(i, "exp-of-zero-is-one", math.exp(0.0) == 1.0);
        flag(i, "exp-overflows-to-infinity", math.exp(710.0) == inf());
        flag(i, "exp-underflows-to-zero", math.exp(0.0 - 746.0) == 0.0);
        flag(i, "exp-of-minus-infinity", math.exp(0.0 - inf()) == 0.0);
        flag(i, "exp-of-nan", is_nan(math.exp(nan())));
        flag(i, "log-of-one-is-zero", math.log(1.0) == 0.0);
        flag(i, "log-of-zero-is-minus-infinity", math.log(0.0) == 0.0 - inf());
        flag(i, "log-of-negative-is-nan", is_nan(math.log(0.0 - 1.0)));
        flag(i, "log-of-infinity", math.log(inf()) == inf());
        flag(i, "log-of-the-smallest-subnormal", math.log(5.0e-324) == 0.0 - 744.4400719213812);
        flag(i, "log2-of-a-power-of-two-is-exact", math.log2(1024.0) == 10.0 && math.log2(0.125) == 0.0 - 3.0 && math.log2(1.0) == 0.0);
        flag(i, "log10-of-a-power-of-ten", math.log10(1000.0) == 3.0 && math.log10(1.0e15) == 15.0 && math.log10(0.01) == 0.0 - 2.0);
        flag(i, "expm1-small-keeps-its-digits", math.expm1(1.0e-10) == 1.00000000005e-10);
        flag(i, "expm1-of-minus-infinity", math.expm1(0.0 - inf()) == 0.0 - 1.0);
        flag(i, "expm1-overflows", math.expm1(710.0) == inf());
        flag(i, "log1p-small-keeps-its-digits", math.fabs(math.log1p(1.0e-10) / 9.9999999995e-11 - 1.0) < 5.0e-16);
        flag(i, "log1p-of-minus-one", math.log1p(0.0 - 1.0) == 0.0 - inf());
        flag(i, "log1p-below-minus-one-is-nan", is_nan(math.log1p(0.0 - 2.0)));
        flag(i, "sinh-cosh-tanh-at-zero", math.sinh(0.0) == 0.0 && math.cosh(0.0) == 1.0 && math.tanh(0.0) == 0.0);
        flag(i, "sinh-is-odd-cosh-is-even", math.sinh(0.0 - 0.7) == 0.0 - math.sinh(0.7) && math.cosh(0.0 - 0.7) == math.cosh(0.7));
        flag(i, "tanh-saturates", math.tanh(30.0) == 1.0 && math.tanh(0.0 - 30.0) == 0.0 - 1.0 && math.tanh(inf()) == 1.0);
        flag(i, "sinh-and-cosh-overflow", math.sinh(711.0) == inf() && math.sinh(0.0 - 711.0) == 0.0 - inf() && math.cosh(711.0) == inf());
        flag(i, "sinh-near-the-overflow-edge", math.sinh(710.0) < inf() && math.cosh(710.0) < inf());
        flag(i, "hyperbolic-identity", math.fabs(math.cosh(1.5) * math.cosh(1.5) - math.sinh(1.5) * math.sinh(1.5) - 1.0) < 1.0e-14);
        flag(i, "asinh-inverts-sinh", math.fabs(math.asinh(math.sinh(1.25)) - 1.25) < 1.0e-15);
        flag(i, "acosh-inverts-cosh", math.fabs(math.acosh(math.cosh(1.25)) - 1.25) < 1.0e-15);
        flag(i, "atanh-inverts-tanh", math.fabs(math.atanh(math.tanh(0.75)) - 0.75) < 1.0e-15);
        flag(i, "acosh-of-one-is-zero", math.acosh(1.0) == 0.0);
        flag(i, "acosh-below-one-is-nan", is_nan(math.acosh(0.5)));
        flag(i, "atanh-at-the-ends", math.atanh(1.0) == inf() && math.atanh(0.0 - 1.0) == 0.0 - inf());
        flag(i, "atanh-beyond-the-ends-is-nan", is_nan(math.atanh(1.5)) && is_nan(math.atanh(0.0 - 1.5)));
        flag(i, "asinh-is-odd", math.asinh(0.0 - 0.3) == 0.0 - math.asinh(0.3));
        flag(i, "pow-of-small-integers-is-exact", math.pow(7.0, 2.0) == 49.0 && math.pow(3.0, 3.0) == 27.0 && math.pow(10.0, 15.0) == 1.0e15 && math.pow(5.0, 3.0) == 125.0);
        flag(i, "pow-of-two-is-exact", math.pow(2.0, 10.0) == 1024.0 && math.pow(2.0, 0.0 - 3.0) == 0.125 && math.pow(2.0, 100.0) == 1267650600228229401496703205376.0);
        flag(i, "pow-negative-base", math.pow(0.0 - 2.0, 3.0) == 0.0 - 8.0 && math.pow(0.0 - 2.0, 2.0) == 4.0 && is_nan(math.pow(0.0 - 2.0, 0.5)));
        flag(i, "pow-zero-and-one", math.pow(5.0, 0.0) == 1.0 && math.pow(1.0, 123456.0) == 1.0 && math.pow(0.0, 3.0) == 0.0 && math.pow(0.0, 0.0 - 1.0) == inf());
        flag(i, "pow-infinite-exponent", math.pow(2.0, inf()) == inf() && math.pow(0.5, inf()) == 0.0 && math.pow(2.0, 0.0 - inf()) == 0.0 && math.pow(1.0, inf()) == 1.0);
        flag(i, "pow-infinite-base", math.pow(inf(), 2.0) == inf() && math.pow(inf(), 0.0 - 2.0) == 0.0);
        flag(i, "pow-overflows-and-underflows", math.pow(10.0, 400.0) == inf() && math.pow(10.0, 0.0 - 400.0) == 0.0);
        flag(i, "pow-square-root", math.pow(2.0, 0.5) == sqrt(2.0));
        flag(i, "pow-nan", is_nan(math.pow(nan(), 2.0)) && is_nan(math.pow(2.0, nan())));
    }
    release(io);
    return 0;
}
