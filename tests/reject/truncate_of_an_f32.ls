//~ ERROR expected `float`, found `f32`
//~ RULE type-mismatch

// `docs/f32.md` §2: `truncate` is `float -> int` and is not overloaded; `int_of_f32` is the `f32` one.

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
    let r = truncate(4.5f32);
    return 0;
}
