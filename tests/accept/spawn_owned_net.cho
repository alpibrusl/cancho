//~ EXIT 0

// `docs/parallelism.md` T2: an owned `Net`, moved into a spawned thread and used there for real. The thread dials a
// port nothing listens on and sees the refusal -- a real `connect` from a second OS thread holding the only
// reference to the capability that authorises it. `Net("")` is the unnarrowed capability; one narrowed to a host
// crosses the same way (the bound is a type argument, and the allowlist reads the definition, not the arguments).

edition 5;

fn worker(net: Net("")) -> [] int {
    var code = 1;
    borrow net as &n in {
        match tcp_connect(n, "127.0.0.1", 1) {
            Dialed::Ok(c) => {
                conn_close(c);
            }
            Dialed::Failed(e) => {
                code = 0;
            }
        }
    }
    release(net);
    return code;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(clock);
    release(heap);
    release(io);
    let w = worker;
    let h = spawn(net, w);
    return join(h);
}
