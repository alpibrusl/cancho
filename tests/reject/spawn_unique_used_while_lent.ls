//~ ERROR is lent to a thread that has not been joined
//~ RULE borrow-conflict

// The spawning thread is a second writer too: it may not touch the object while the thread holds it.

edition 6;

struct Counter {
    n: int,
}

fn work[&r](c: &!r Counter) -> [] int {
    c.n = c.n + 1;
    return 0;
}

fn run[&i](io: &!i Io) -> [io_write, conc] int {
    var c = Counter { n: 0 };
    borrow mut c as &!r in {
        let f = work;
        let ta = spawn(r, f);
        r.n = 5;
        let x = join(ta);
    }
    putchar(io, '0' + c.n);
    putchar(io, 10);
    return c.n;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(signals);
    var i0 = io;
    borrow mut i0 as &!i in {
        let code = run(i);
    }
    release(i0);
    return 0;
}
