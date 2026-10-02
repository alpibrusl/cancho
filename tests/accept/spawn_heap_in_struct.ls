//~ EXIT 0

// `docs/parallelism.md` §8.1: a struct that **holds a `Heap`** crosses to a thread by `&!` reference, and the thread
// allocates and frees through that field. This is the carrying half of giving a worker its own heap (T1); what it
// does not show is a *second* heap, because there is only one to move in -- the `fork_heap` builtin of §8 is what
// would make one.
//
// `work`'s row is `[heap]`: only *owning* a capability discharges its label (`spawn_owned_io.ls`), and here the
// thread borrows it through the struct. `main`'s row is `[conc]`, because `main` owns the heap it moved into `Worker`.

edition 5;

res struct Worker {
    heap: Heap,
    out: int,
}

fn work[&r](w: &!r Worker) -> [heap] int {
    var total = 0;
    let b = box_slice(w.heap, 8, 3);
    borrow b as &d in {
        total = contents(d)[0] + contents(d)[7];
    }
    unbox_slice(w.heap, b);
    w.out = total;
    return 0;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(io);
    var wk = Worker { heap: heap, out: 0 };
    var status = 1;
    borrow mut wk as &!k in {
        let f = work;
        let t = spawn(k, f);
        status = join(t);
    }
    let Worker { heap, out } = wk;
    release(heap);
    return status + out - 6;
}
