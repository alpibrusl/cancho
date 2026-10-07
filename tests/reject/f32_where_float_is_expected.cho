//~ ERROR expected `float`, found `f32`
//~ RULE type-mismatch

// `docs/f32.md` §2: no implicit conversion from `f32` to `float`. `sqrt`
// takes a `float`; `float_of32(x)` is the exact crossing.

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
    let x: f32 = 4.0f32;
    let r = sqrt(x);
    return 0;
}
