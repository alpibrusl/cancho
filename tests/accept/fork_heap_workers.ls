//~ EXIT 0

// `docs/parallelism.md` §8.4, the T1 gate: the parent forks two `Heap`s from its own and moves one into each worker's
// struct; the two threads allocate and free boxes in a loop (a million iterations each); and a box the *forked* heap
// allocates is freed by the *parent's* -- the interchangeability §8.3 states, and which holds only while `Heap` has no
// state. (A thread handing a box back to the parent is not expressible yet: `join` refuses a `Box` result, and a `res`
// field cannot be moved out through a `&!` reference. §8.4 records that.)

edition 5;

res struct Worker {
    heap: Heap,
    out: int,
}

fn churn[&r](w: &!r Worker, rounds: int) -> [heap] int {
    var total = 0;
    var i = 0;
    while i < rounds {
        let b = box_slice(w.heap, 16, i);
        borrow b as &d in {
            total = total + contents(d)[0] + contents(d)[15];
        }
        unbox_slice(w.heap, b);
        i = i + 1;
    }
    return total;
}

fn work[&r](w: &!r Worker) -> [heap] int {
    w.out = churn(w, 1000000);
    return 0;
}

fn work_b[&r](w: &!r Worker) -> [heap] int {
    w.out = churn(w, 1000000);
    return 0;
}

fn finish(w: Worker) -> [] int {
    let Worker { heap, out } = w;
    release(heap);
    return out;
}

fn pair[&p](parent: &!p Heap) -> [heap, conc] int {
    var ha = fork_heap(parent);
    let hb = fork_heap(parent);
    var kept = 0;
    borrow mut ha as &!x in {
        let k = box(x, 7);
        kept = unbox(parent, k);
    }
    var a = Worker { heap: ha, out: 0 };
    var b = Worker { heap: hb, out: 0 };
    var status = 0;
    borrow mut a as &!pa in {
        borrow mut b as &!pb in {
            let fa = work;
            let fb = work_b;
            let ta = spawn(pa, fa);
            let tb = spawn(pb, fb);
            let ra = join(ta);
            let rb = join(tb);
            status = ra + rb;
        }
    }
    let oa = finish(a);
    let ob = finish(b);
    if oa != ob {
        return 1;
    }
    return status + kept - 7;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(io);
    var status = 1;
    borrow mut heap as &!h in {
        status = pair(h);
    }
    release(heap);
    return status;
}
