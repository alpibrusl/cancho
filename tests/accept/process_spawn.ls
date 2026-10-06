//~ STDOUT hello world
//~ EXIT 0

// `docs/processes.md` §3.3: start a program narrowed to `/bin`, read what it
// writes through a channel, and reap it. The report says `exec("/bin")`.

edition 7;

fn run[&x, &i](exec: &x Exec("/bin"), io: &!i Io) -> [exec("/bin"), pipe_read, io_write] int {
    var status = 100;
    match pipe_open() {
        Piped::Ok(from, out_end) => {
            var reader = from;
            match exec_spawn(exec, "/bin/echo", "hello\0world\0", "", Stdio::Null, Stdio::Pipe(out_end), Stdio::Null) {
                Spawned::Ok(c) => {
                    region a {
                        let buf = alloc_slice[a](64, byte_of(0));
                        var going = true;
                        while going {
                            borrow mut reader as &!r in {
                                match pipe_read(r, buf) {
                                    Received::Data(n) => {
                                        write_bytes(io, buf[0..n]);
                                    }
                                    Received::End => {
                                        going = false;
                                    }
                                    Received::Again => {
                                    }
                                    Received::Failed(e) => {
                                        going = false;
                                    }
                                }
                            }
                        }
                    }
                    match child_wait(c) {
                        Exited::Code(n) => {
                            status = n;
                        }
                        Exited::Signaled(s) => {
                            status = 200 + s;
                        }
                        Exited::Failed(e) => {
                            status = 150;
                        }
                    }
                }
                Spawned::Failed(e) => {
                    status = e;
                }
            }
            pipe_close(reader);
        }
        Piped::Failed(e) => {
            status = 99;
        }
    }
    return status;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(signals);
    let bin = narrow(exec, "/bin");
    var status = 0;
    var console = io;
    borrow bin as &b in {
        borrow mut console as &!i in {
            status = run(b, i);
        }
    }
    release(bin);
    release(console);
    return status;
}
