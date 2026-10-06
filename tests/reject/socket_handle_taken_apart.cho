//~ ERROR is ended by `conn_close`, not by being taken apart
//~ RULE linear-value-taken-apart

// `docs/native-sockets.md` §3: a pattern that named the descriptor would
// be a way to drop one without `conn_close`; the kernel keeps that leak.

edition 5;
fn take(c: Conn) -> [] int {
    let Conn {  } = c;
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(clock);
    release(net);
    return 0;
}
