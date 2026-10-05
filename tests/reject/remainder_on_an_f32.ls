//~ ERROR expected `int`, found `f32`
//~ RULE type-mismatch

// `docs/f32.md` §5: `%` on `f32` is F2, as `float`'s never was
// (`remainder_on_a_float.ls`).

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
    let x = 5.5f32 % 2.0f32;
    return 0;
}
