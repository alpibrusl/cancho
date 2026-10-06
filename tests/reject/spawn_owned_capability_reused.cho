//~ ERROR `io` has already been consumed; a `res` value is used exactly once
//~ RULE linear-use-after-move

// `docs/threads.md` §5 step 3: "the spawning function checked to no
// longer hold that capability" -- `spawn` consumes `payload` exactly
// the way any function call consumes an argument whose type is not
// `val` (`linearity-and-effects.md`'s existing rule). `io` moves into
// `spawn` here, and `main`'s own `release(io)` afterward is an
// ordinary use-after-move, caught by the same `Rule::LinearUseAfterMove`
// every other `res` value already is -- no rule specific to `spawn`
// was needed or added.

edition 4;

fn worker(io: Io) -> [] int {
    release(io);
    return 0;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);

    let w = worker;
    let h = spawn(io, w);
    let n = join(h);
    release(io);
    return n;
}
