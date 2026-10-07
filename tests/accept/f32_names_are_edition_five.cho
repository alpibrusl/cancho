// `docs/f32.md` §6: the edition decision, as a program. Before edition 6
// `f32` is not a type and `f32_of`, `float_of32`, `bits_of32` and
// `f32_of_bits` are not builtins, so a file at an earlier edition may use
// every one of those names for itself. Nothing in this repository did
// (counted in `f32.md` §6); this is what makes "nothing broke" a thing the
// suite checks and not a thing the count asserts.
//~ STDOUT 7 8 9 10
//~ EXIT 0
edition 5;

import std.io;

struct f32 {
    bits: int,
}

fn f32_of(x: int) -> [] f32 {
    return f32 { bits: x };
}

fn float_of32(x: f32) -> [] int {
    return x.bits + 1;
}

fn bits_of32(x: f32) -> [] int {
    return x.bits + 2;
}

fn f32_of_bits(x: int) -> [] int {
    return x + 3;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let seven = f32_of(6);
    borrow mut io as &!i in {
        io.print_int(i, float_of32(seven));
        io.space(i);
        io.print_int(i, bits_of32(seven));
        io.space(i);
        io.print_int(i, f32_of_bits(6));
        io.space(i);
        io.print_int(i, bits_of32(f32_of(8)));
        io.newline(i);
    }
    release(io);
    return 0;
}
