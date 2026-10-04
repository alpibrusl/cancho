//~ ERROR `index_of_byte` is not a function in this program
//~ RULE not-a-function

// `docs/byte-search.md`: `index_of_byte` is edition 5. An older file may
// already declare its own `index_of_byte`, so to it the name is not a
// builtin at all -- the rule every edition-gated builtin follows
// (`editions.md` §7).

edition 4;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    return index_of_byte("abc", byte_of(99));
}
