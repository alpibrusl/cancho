//~ ERROR `clock` has already been consumed; there is nothing left to borrow
//~ RULE linear-use-after-move

// `docs/parallelism.md` §9: forking needs the parent clock to still exist; releasing it first is the ordinary
// use-after-move every `res` value already has.

edition 5;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(heap);
    release(io);
    release(clock);
    borrow clock as &c in {
        let child = fork_clock(c);
        release(child);
    }
    return 0;
}
