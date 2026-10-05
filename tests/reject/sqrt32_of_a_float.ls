//~ ERROR expected `f32`, found `float`
//~ RULE type-mismatch

// `docs/f32.md` §2: `sqrt32` takes an `f32`. `sqrt` is `float -> float` and a builtin has one signature, so the width is in the name; `f32_of` is the crossing.

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
    let r = sqrt32(4.0);
    return 0;
}
