//~ EXIT 0

// `docs/parallelism.md` §9: a thread needs a `Clock` of its own, because a `Clock` is `res` and `split` hands out one.
// `fork_clock(&Clock) -> Clock` makes another from a shared borrow of the first. Each of two threads owns a forked clock
// and reads it; the parent keeps its own and reads it after `join`. Monotonic time never goes backwards, so every
// reading is at least the one taken before the threads started.

edition 5;

fn read_clock(clock: Clock) -> [] int {
    var first = 0;
    var second = 0;
    borrow clock as &c in {
        first = clock_ms(c);
        second = clock_ms(c);
    }
    release(clock);
    if second >= first {
        return 0;
    }
    return 1;
}

fn pair[&k](clock: &k Clock) -> [clock, conc] int {
    let before = clock_ms(clock);
    let ca = fork_clock(clock);
    let cb = fork_clock(clock);
    let fa = read_clock;
    let fb = read_clock;
    let ta = spawn(ca, fa);
    let tb = spawn(cb, fb);
    let ra = join(ta);
    let rb = join(tb);
    let after = clock_ms(clock);
    if after < before {
        return 1;
    }
    return ra + rb;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(heap);
    release(io);
    var status = 1;
    borrow clock as &c in {
        status = pair(c);
    }
    release(clock);
    return status;
}
