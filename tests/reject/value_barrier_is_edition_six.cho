//~ ERROR `value_barrier` is not a function in this program
//~ RULE not-a-function

// `docs/value-barrier.md` §3: `value_barrier` is edition 6. An older file may
// already declare its own `value_barrier`, so to it the name is not a builtin
// at all -- the rule every edition-gated builtin follows (`editions.md` §7).

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
    return value_barrier(0);
}
