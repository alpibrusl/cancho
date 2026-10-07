//~ STDOUT 42
//~ STDOUT -1 0
//~ STDOUT 7 0

// `docs/value-barrier.md` §3: `value_barrier(x)` is `x`. What it adds is
// invisible here by design -- the optimiser may assume nothing about the
// answer -- so this checks only that it answers `x`, including a mask
// built from a comparison and the selection made under it.
edition 6;

import std.io;

fn mask_of(a: int, b: int) -> [] int {
    return value_barrier((a ^ b) - 1 >> 63);
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
        io.print_int(i, value_barrier(42));
        io.newline(i);
        io.print_int(i, mask_of(3, 3));
        io.space(i);
        io.print_int(i, mask_of(3, 4));
        io.newline(i);
        io.print_int(i, 7 & mask_of(5, 5));
        io.space(i);
        io.print_int(i, 7 & mask_of(5, 6));
        io.newline(i);
    }
    release(io);
    return 0;
}
