//~ EXIT 0

// `docs/parallelism.md` §3.1: a thread's payload is one pointer-width leaf, and a **unique reference to a
// struct** is one: the struct's fields cross with it, so a worker's whole job (its range, its answer, a boxed
// slice it owns) goes in one `&!` payload with no trampoline. The reference must not outlive the `borrow`
// block that made it, and the block ends at `join`: the existing escape check (`threads.md` §3), asked about
// one more kind of value.
//
// Two workers, each with a struct of its own. **Each spawn takes its own function value**: one `work` taken
// once would be instantiated at the first borrow's region, and `k1` does not outlive `k0` -- the reason the
// two `let`s below are not one.

edition 5;

res struct Worker {
    data: Box[[int]],
    out: int,
}

fn work[&r](w: &!r Worker) -> [] int {
    var total = 0;
    var i = 0;
    while i < len(contents(w.data)) {
        total = total + contents(w.data)[i];
        i = i + 1;
    }
    w.out = total;
    return 0;
}

fn make[&h](heap: &!h Heap, n: int, each: int) -> [heap] Worker {
    let held = box_slice(heap, n, each);
    return Worker { data: held, out: 0 };
}

// Take a worker apart: free its slice, answer what its thread computed.
fn finish[&h](heap: &!h Heap, w: Worker) -> [heap] int {
    let Worker { data, out } = w;
    unbox_slice(heap, data);
    return out;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(io);
    var h0 = heap;
    var code = 1;
    borrow mut h0 as &!h in {
        var a = make(h, 100, 3);
        var b = make(h, 50, 7);
        var status = 1;
        borrow mut a as &!ka in {
            borrow mut b as &!kb in {
                let fa = work;
                let fb = work;
                let ta = spawn(ka, fa);
                let tb = spawn(kb, fb);
                let sa = join(ta);
                let sb = join(tb);
                status = sa + sb;
            }
        }
        let oa = finish(h, a);
        let ob = finish(h, b);
        // 100 * 3 and 50 * 7, each summed by its own thread
        if status == 0 && oa == 300 && ob == 350 {
            code = 0;
        }
    }
    release(h0);
    return code;
}
