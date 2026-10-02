//~ ERROR is not a uniquely borrowed `Heap`
//~ RULE capability-misused

// `docs/parallelism.md` §8.4: forking a heap needs the same unique borrow allocating does. A shared borrow of the
// parent would let two holders each fork from it, which is the sharing `Heap`'s uniqueness exists to refuse.

edition 5;

fn main(world: World) -> [heap] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(io);
    borrow heap as &h in {
        let child = fork_heap(h);
        release(child);
    }
    release(heap);
    return 0;
}
