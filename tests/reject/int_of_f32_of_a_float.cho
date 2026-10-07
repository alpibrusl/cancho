//~ ERROR expected `f32`, found `float`
//~ RULE type-mismatch

// `docs/f32.md` §2: `int_of_f32` takes an `f32`; a `float` is `truncate`'s.

edition 6;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(signals);
    let r = int_of_f32(4.5);
    return 0;
}
