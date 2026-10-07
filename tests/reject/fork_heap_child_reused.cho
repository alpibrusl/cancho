//~ ERROR `child` has already been consumed; a `res` value is used exactly once
//~ RULE linear-use-after-move

// `docs/parallelism.md` §8.4: a forked heap is an ordinary owned capability. Moving it into a thread's payload and then
// releasing it here is a use-after-move, caught by the rule every `res` value already has.

edition 5;

fn worker(heap: Heap) -> [] int {
    release(heap);
    return 0;
}

fn run[&p](parent: &!p Heap) -> [heap, conc] int {
    let child = fork_heap(parent);
    let w = worker;
    let t = spawn(child, w);
    let n = join(t);
    release(child);
    return n;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(io);
    var n = 1;
    borrow mut heap as &!h in {
        n = run(h);
    }
    release(heap);
    return n;
}
