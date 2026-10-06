//~ ERROR a thread spawned in this loop is still holding a `&!`
//~ RULE borrow-conflict

// `docs/aliasing.md` §6.1: the lease ends only where the checker can see the `join`. `finish` joins the handle, but
// the caller cannot tell, so the lease outlives the iteration and the next spawn would reuse a held reference. This
// refuses a program that is in fact fine, which is the side the rule errs on.

edition 6;

struct Counter {
    n: int,
}

fn work[&r](c: &!r Counter) -> [] int {
    c.n = c.n + 1;
    return 0;
}

fn finish[&r](t: Thread[&!r Counter, int]) -> [] int {
    return join(t);
}

fn run[&i](io: &!i Io) -> [io_write, conc] int {
    var c = Counter { n: 0 };
    borrow mut c as &!r in {
        let f = work;
        var i = 0;
        while i < 2 {
            let t = spawn(r, f);
            let x = finish(t);
            i = i + 1;
        }
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
