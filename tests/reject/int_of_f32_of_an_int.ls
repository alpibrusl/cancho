//~ ERROR expected `f32`, found `int`
//~ RULE type-mismatch

// `docs/f32.md` §2: no implicit `int` to `f32`, so `int_of_f32(3)` is refused; `f32_of_int` is the crossing.

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
    let r = int_of_f32(3);
    return 0;
}
