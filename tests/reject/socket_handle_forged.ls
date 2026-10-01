//~ ERROR `Conn` is a capability and has no literal form
//~ RULE capability-misused

// `docs/native-sockets.md` §2: a descriptor is never a number a program
// can write. `Conn { }` would be someone else's open socket -- or standard
// output, if the number were 1 -- so a socket handle has no literal form,
// the rule `File` already has (`file-handles.md` §2).

edition 5;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(clock);
    release(net);
    let c = Conn { };
    conn_close(c);
    return 0;
}
