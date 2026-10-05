//~ ERROR expected `f32`, found `float`
//~ RULE type-mismatch

// `docs/f32.md` §2: nor from `float` to `f32`. The annotation asks for
// binary32 and the literal is binary64; `f32_of(4.0)` rounds, and
// `4.0f32` is the literal.

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
    let x: f32 = 4.0;
    return 0;
}
