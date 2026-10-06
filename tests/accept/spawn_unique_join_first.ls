//~ STDOUT 8
//~ EXIT 0

// `docs/aliasing.md` §6.1: the control to `tests/reject/spawn_two_copies_of_unique.ls`. A `&!` lent to a
// thread comes back at `join`, so the second spawn is the only writer once the first has finished: sequential,
// exact, and accepted. Each shape the checker follows is here: a copy joined before the next spawn, a loop
// that joins in every iteration, a `spawn` joined where it is made, and a handle joined in one branch arm and
// the other alike.

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
        let a = r;
        let f = work;
        let ta = spawn(a, f);
        let x = join(ta);
        let b = r;
        let tb = spawn(b, f);
        let y = join(tb);
        var i = 0;
        while i < 3 {
            let t = spawn(r, f);
            let z = join(t);
            i = i + 1;
        }
        let w = join(spawn(r, f));
        let tc = spawn(r, f);
        if i == 3 {
            let u = join(tc);
        } else {
            let u = join(tc);
        }
        r.n = r.n + 1;
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
