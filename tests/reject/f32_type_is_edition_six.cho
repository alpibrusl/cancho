//~ ERROR unknown type `f32`
//~ RULE unknown-name

// `docs/f32.md` §6: the type is edition 6's. In an earlier file `f32` is
// not a type, so it can still be a name the file declares itself.

edition 5;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let x: f32 = 1.5;
    return 0;
}
