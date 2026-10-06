edition 7;

// `docs/processes.md` §4.10: a child that starts in a `Dir`. argv: the
// directory to open, the program, one argument for it (`-` for none), and a
// mode:
//   `d`  `exec_spawn_in` the directory, then the child's output and how it ended
//   `n`  `exec_spawn`, the parent's own directory
//   `b`  `d`, then `n`, then `dir_close` of the same `Dir`: it was only lent
// A line `== code N`, `== signal B`, `== failed E` says how a child ended,
// `== spawn E` that it did not start, `== opendir E` that the directory did
// not open, and `== dirclose N` what closing the `Dir` afterwards answered.

fn digits[&i](io: &!i Io, n: int) -> [io_write] int {
    if n >= 10 {
        digits(io, n / 10);
    }
    putchar(io, '0' + n % 10);
    return 0;
}

fn say[&i, &t](io: &!i Io, text: &t [byte], n: int) -> [io_write] int {
    write_bytes(io, text);
    digits(io, n);
    putchar(io, 10);
    return 0;
}

fn show[&i](io: &!i Io, from: Pipe, child: Child) -> [io_write] int {
    var reader = from;
    region a {
        let buf = alloc_slice[a](4096, byte_of(0));
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
    pipe_close(reader);
    match child_wait(child) {
        Exited::Code(n) => {
            say(io, "== code ", n);
        }
        Exited::Signaled(s) => {
            say(io, "== signal ", s);
        }
        Exited::Failed(e) => {
            say(io, "== failed ", e);
        }
    }
    return 0;
}

fn start[&x, &i, &d, &p, &a](exec: &x Exec(""), io: &!i Io, dir: &d Dir, in_dir: bool, program: &p [byte], args: &a [byte]) -> [exec(""), io_write] int {
    match pipe_open() {
        Piped::Failed(e) => {
            say(io, "== pipe ", e);
        }
        Piped::Ok(from, out_end) => {
            if in_dir {
                match exec_spawn_in(exec, dir, program, args, "", Stdio::Null, Stdio::Pipe(out_end), Stdio::Null) {
                    Spawned::Failed(e) => {
                        say(io, "== spawn ", e);
                        pipe_close(from);
                    }
                    Spawned::Ok(c) => {
                        show(io, from, c);
                    }
                }
            } else {
                match exec_spawn(exec, program, args, "", Stdio::Null, Stdio::Pipe(out_end), Stdio::Null) {
                    Spawned::Failed(e) => {
                        say(io, "== spawn ", e);
                        pipe_close(from);
                    }
                    Spawned::Ok(c) => {
                        show(io, from, c);
                    }
                }
            }
        }
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi);
    release(heap);
    release(net);
    release(clock);
    release(signals);
    var console = io;
    borrow exec as &x in {
        borrow fs as &f in {
            borrow args as &g in {
                borrow mut console as &!i in {
                    match open_dir(f, arg(g, 1)) {
                        DirOpened::Failed(e) => {
                            say(i, "== opendir ", e);
                        }
                        DirOpened::Ok(d0) => {
                            let dir = d0;
                            let mode = int_of(arg(g, 4)[0]);
                            region a {
                                // The one argument, `\0`-terminated: `-` is none.
                                let want = arg(g, 3);
                                var n = 0;
                                if !(len(want) == 1 && want[0] == byte_of('-')) {
                                    n = len(want) + 1;
                                }
                                let argv = alloc_slice[a](len(want) + 1, byte_of(0));
                                var k = 0;
                                while k < n - 1 {
                                    argv[k] = want[k];
                                    k = k + 1;
                                }
                                borrow dir as &r in {
                                    if mode == 'd' || mode == 'b' {
                                        start(x, i, r, true, arg(g, 2), argv[0..n]);
                                    }
                                    if mode == 'n' || mode == 'b' {
                                        start(x, i, r, false, arg(g, 2), argv[0..n]);
                                    }
                                }
                            }
                            say(i, "== dirclose ", dir_close(dir));
                        }
                    }
                }
            }
        }
    }
    release(exec);
    release(fs);
    release(console);
    release(args);
    return 0;
}
