//~ ERROR `heap` has already been consumed; there is nothing left to borrow
//~ RULE linear-use-after-move

// `docs/parallelism.md` §8.4: the parent heap is a `res` value like any other; releasing it and then forking from it is an
// ordinary use-after-move, with no rule specific to `fork_heap`.

edition 5;

fn main(world: World) -> [heap] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(io);
    release(heap);
    borrow mut heap as &!h in {
        let child = fork_heap(h);
        release(child);
    }
    return 0;
}
