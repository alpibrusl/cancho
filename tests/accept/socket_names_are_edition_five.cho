// `docs/native-sockets.md` §3: `Conn`, `Sent` and `conn_read` are edition 5.
// An edition-1 file that declares its own is not redeclaring anything it
// can name -- `Net` and `Thread` are the precedent, and until now a file
// declaring either was refused whatever its edition.
//~ EXIT 0

struct Conn {
    fd: int,
}

enum Sent {
    Wrote(int),
    Lost,
}

fn conn_read(c: Conn) -> [] int {
    return c.fd;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    let c = Conn { fd: 7 };
    return conn_read(c) - 7;
}
