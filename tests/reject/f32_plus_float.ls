//~ ERROR expected `f32`, found `float`
//~ RULE type-mismatch

// `docs/f32.md` §2: there is no `f32 + float`, in either order. The
// conversion is where the decision lives, and it is written at the
// crossing: `f32_of(y)` rounds, `float_of32(x)` is exact.

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
    let y = 2.5;
    let z = x + y;
    return 0;
}
