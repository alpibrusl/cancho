//~ ERROR performs `clock`, which its row [] does not declare
//~ RULE effect-not-declared

// `docs/native-sockets.md` §5: reading the time is an effect. A function
// that borrows a `Clock` and reads it must say so, so the authority report
// of a program is true about whether it can observe the time at all.

edition 5;

fn now[&c](clock: &c Clock) -> [] int {
    return clock_ms(clock);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    var t = 0;
    borrow clock as &c in {
        t = now(c);
    }
    release(clock);
    return t - t;
}
