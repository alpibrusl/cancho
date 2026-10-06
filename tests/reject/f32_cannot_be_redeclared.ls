//~ ERROR `f32` is a built-in type and cannot be redeclared
//~ RULE builtin-redeclared

// `docs/f32.md` §6: from edition 6 `f32` is a type, so a file that declares
// one of its own is redeclaring it (`f32_names_are_edition_five.ls` is the
// other side: before edition 6 the name is the file's to use).
edition 6;

struct f32 {
    bits: int,
}

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
    return 0;
}
