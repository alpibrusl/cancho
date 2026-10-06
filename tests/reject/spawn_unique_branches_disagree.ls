//~ ERROR the branches disagree about whether a thread still holds a `&!`
//~ RULE borrow-conflict

// `docs/aliasing.md` §6.1: one arm joins the handle where the checker sees it and the other hands it to `finish`,
// so the arms disagree about whether the reference is still lent.

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
    let flag = true;
    borrow mut c as &!r in {
        let f = work;
        let t = spawn(r, f);
        if flag {
            let x = join(t);
        } else {
            let x = finish(t);
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
