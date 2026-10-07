//~ STDOUT 62

// `docs/function-values.md` §4.2: a call through a function value is an
// expression like any other call, so it can be an operand. The LLVM
// backend's `scalar_kind` had an arm for a named call and none for
// `CallIndirect`, so `f(x) + 1` was refused as an `internal` error while
// `let r = f(x); r + 1` compiled. Both spellings, on a `float` as well as
// an `int`, because the kind comes from the callee's declared return.

import std.io;

fn double(x: int) -> [] int {
    return x + x;
}

fn half(x: float) -> [] float {
    return x / 2.0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);

    let f = double;
    let g = half;
    let bound = f(20);
    // The operand forms: left, right, nested, a comparison, and a float.
    let n = f(10) + f(1) + bound - f(0) * 1;
    if f(1) == 2 && g(4.0) > 1.5 {
        borrow mut io as &!i in {
            io.print_int(i, n);
            io.newline(i);
        }
    }
    release(io);
    return 0;
}
