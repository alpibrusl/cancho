//~ ERROR unexpected character in an integer literal
//~ RULE unexpected-character

// `docs/f32.md` §2: an integer-shaped literal takes no suffix, for the
// reason `float` needs a point or an exponent: `2f32` would be the one
// number in the language whose type is written after its digits and whose
// spelling has no mark of being a float.

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
    let x = 2f32;
    return 0;
}
