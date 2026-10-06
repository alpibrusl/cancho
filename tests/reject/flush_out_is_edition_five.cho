//~ ERROR `flush_out` is not a function in this program
//~ RULE not-a-function

// `docs/checked-output.md` §2: `flush_out` is edition 5. An older file may
// already declare its own `flush_out`, so to it the name is not a builtin at
// all -- the rule every edition-gated builtin follows (`editions.md` §7).

edition 4;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    borrow mut io as &!i in {
        flush_out(i);
    }
    release(io);
    return 0;
}
