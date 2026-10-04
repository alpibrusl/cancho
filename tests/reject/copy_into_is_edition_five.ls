//~ ERROR `copy_into` is not a function in this program
//~ RULE not-a-function

// `docs/bulk-copy.md`: `copy_into` is edition 5. An older file may already
// declare its own `copy_into`, so to it the name is not a builtin at all --
// the rule every edition-gated builtin follows (`editions.md` §7).

edition 4;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    var n = 0;
    region a {
        let xs = alloc_slice[a](4, byte_of(1));
        let ys = alloc_slice[a](4, byte_of(0));
        n = copy_into(ys, xs);
    }
    return n;
}
