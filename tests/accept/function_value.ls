//~ STDOUT 30
//~ STDOUT hello from a function value

// `docs/function-values.md` §4.2, built: a named, top-level function
// declaring no type parameters is a real, `val` value now -- copyable
// (`h` is called twice below with no move between the calls, §4.2's
// own "its mode: val"), region-polymorphic (`shout`'s own `&i`/`&r`
// are instantiated at the point the value is taken, solved by
// `call_with`'s declared parameter type, exactly as a call's region
// parameters already are), and its row travels with it (`call_with`'s
// own `[io_write]` is exactly `shout`'s, checked the same way a named
// call's row is).
//
// This is `write_with` from §4.3, no longer hypothetical: that section
// showed it recovering none of `defunctionalized_stream.ls`'s lost
// precision because nothing here needed a *row-polymorphic* higher
// order function. `call_with` below is the same shape it described,
// fixed-row and all, and it is exactly what it says: one function,
// one row, called through a value.

import std.io;

fn double(x: int) -> [] int {
    return x + x;
}

fn apply(f: fn(int) -> [] int, x: int) -> [] int {
    return f(x);
}

fn shout[&i, &r](out: &!i Io, s: &r [byte]) -> [io_write] int {
    return io.write_all(out, s);
}

fn call_with[&i, &r](
    out: &!i Io,
    s: &r [byte],
    f: fn(&!i Io, &r [byte]) -> [io_write] int,
) -> [io_write] int {
    return f(out, s);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);

    let doubler = double;
    let a = doubler(5);
    let b = doubler(10);

    borrow mut io as &!i in {
        io.print_int(i, a + b);
        io.newline(i);

        let h = shout;
        call_with(i, "hello from a function value\n", h);
    }
    release(io);
    return 0;
}
