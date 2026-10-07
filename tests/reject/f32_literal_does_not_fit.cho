//~ ERROR does not fit in `f32`
//~ RULE literal-out-of-range

// `docs/f32.md` §2: a literal too large for `f32` is refused, as one too
// large for `float` is, rather than quietly becoming infinity (which is
// `f32_of_bits(2139095040)`).

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
    let x = 1e39f32;
    return 0;
}
