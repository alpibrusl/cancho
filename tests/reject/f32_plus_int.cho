//~ ERROR expected `f32`, found `int`
//~ RULE type-mismatch

// `docs/f32.md` §2: nor is there `f32 + int`; `int` to `f32` is F2's
// conversion, and until it exists the answer is `f32_of(float_of(n))`.

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
    let x: f32 = 1.5f32;
    let z = x + 2;
    return 0;
}
