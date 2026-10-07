// A `Conn` cannot be the payload of a thread (the allowlist is int, bool, c_ptr, function values, one reference, and owned
// Io/File/Ffi/Fs/Args/Heap/Net/Clock), and one payload is all a thread gets: it cannot be given a capability AND a channel.
edition 5;
fn worker(c: Conn) -> [] int {
    return conn_close(c);
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(clock);
    var code = 1;
    borrow net as &n in {
        match tcp_connect(n, "127.0.0.1", 1) {
            Dialed::Ok(c) => {
                let w = worker;
                let t = spawn(c, w);
                code = join(t);
            }
            Dialed::Failed(e) => {
                code = 0;
            }
        }
    }
    release(net);
    return code;
}
