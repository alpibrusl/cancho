//~ ERROR the only suffix is `f32`
//~ RULE unexpected-character

// `docs/f32.md` §2: the one suffix a literal may carry is `f32`.
// `f64` would be the type `float` already is, and a second spelling of
// it is not wanted.

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
    let x = 1.5f64;
    return 0;
}
