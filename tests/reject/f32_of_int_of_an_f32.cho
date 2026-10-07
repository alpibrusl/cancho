//~ ERROR expected `int`, found `f32`
//~ RULE type-mismatch

// `docs/f32.md` §2: `f32_of_int` takes an `int`; an `f32` is already one.

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
    let r = f32_of_int(4.0f32);
    return 0;
}
