edition 7;

import std.buffer;
import std.io;
import std.process;

// argv[1] picks a case; each prints how the capture ended, how many bytes it
// kept, and a checksum of them, then how long it took (§7.1).
fn ran_name[&i](io: &!i Io, ran: process.Ran) -> [io_write] int {
    match ran {
        process.Ran::Code(n) => {
            io.write_all(io, "code ");
            io.print_int(io, n);
        }
        process.Ran::Signaled(s) => {
            io.write_all(io, "signaled ");
            io.print_int(io, s);
        }
        process.Ran::TimedOut => {
            io.write_all(io, "timed out");
        }
        process.Ran::TooMuch => {
            io.write_all(io, "too much");
        }
        process.Ran::Failed(e) => {
            io.write_all(io, "failed ");
            io.print_int(io, e);
        }
    }
    io.newline(io);
    return 0;
}

fn sum[&b](b: &b [byte]) -> [] int {
    var s = 0;
    var i = 0;
    while i < len(b) {
        s = (s * 31 + int_of(b[i])) % 1000000007;
        i = i + 1;
    }
    return s;
}

fn one[&h, &x, &c, &i, &p, &a, &n](heap: &!h Heap, exec: &x Exec(""), clock: &c Clock, io: &!i Io, path: &p [byte], args: &a [byte], input: &n [byte], most: int, timeout: int) -> [heap, exec(""), clock, poll, io_write] int {
    let start = clock_ms(clock);
    match process.channels() {
        process.Channels::Failed(e) => {
            io.write_all(io, "channels ");
            io.print_int(io, e);
            io.newline(io);
        }
        process.Channels::Ok(to_child, child_in, from_child, child_out) => {
            match exec_spawn(exec, path, args, "", Stdio::Pipe(child_in), Stdio::Pipe(child_out), Stdio::Null) {
                Spawned::Failed(e) => {
                    process.close(to_child, from_child);
                    io.write_all(io, "spawn ");
                    io.print_int(io, e);
                    io.newline(io);
                }
                Spawned::Ok(child) => {
                    let (out, ran) = process.capture(heap, clock, child, to_child, from_child, input, most, timeout);
                    ran_name(io, ran);
                    borrow out as &o in {
                        io.write_all(io, "kept ");
                        io.print_int(io, buffer.size(o));
                        io.newline(io);
                        io.write_all(io, "sum ");
                        io.print_int(io, sum(buffer.bytes(o)));
                        io.newline(io);
                    }
                    buffer.drop(heap, out);
                }
            }
        }
    }
    io.write_all(io, "ms ");
    io.print_int(io, clock_ms(clock) - start);
    io.newline(io);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi);
    release(fs);
    release(net);
    release(signals);
    var console = io;
    var h = heap;
    var mode = 0;
    borrow args as &g in {
        mode = int_of(arg(g, 1)[0]);
    }
    borrow exec as &x in {
        borrow clock as &c in {
            borrow mut console as &!i in {
                borrow mut h as &!hh in {
                    if mode == 'c' {
                        // 1 MiB through `cat`, past both kernels' deadlock sizes.
                        let big = box_slice(hh, 1048576, byte_of(0));
                        borrow mut big as &!bw in {
                            let s = contents(bw);
                            var k = 0;
                            while k < len(s) {
                                s[k] = byte_of(k * 7 % 251);
                                k = k + 1;
                            }
                        }
                        borrow big as &b in {
                            io.write_all(i, "want sum ");
                            io.print_int(i, sum(contents(b)));
                            io.newline(i);
                            one(hh, x, c, i, "/bin/cat", "", contents(b), 2097152, 20000);
                        }
                        unbox_slice(hh, big);
                    }
                    if mode == 'm' {
                        // It then lingers: only a kill ends it before the deadline.
                        one(hh, x, c, i, "/bin/sh", "-c\0head -c 100000 /dev/zero; sleep 30\0", "", 1000, 20000);
                    }
                    if mode == 'e' {
                        one(hh, x, c, i, "/bin/sh", "-c\0head -c 1000 /dev/zero\0", "", 1000, 20000);
                    }
                    if mode == 't' {
                        one(hh, x, c, i, "/bin/sleep", "30\0", "", 1000, 200);
                    }
                    if mode == 'b' {
                        one(hh, x, c, i, "/bin/sh", "-c\0sleep 2 & echo hi\0", "", 1000, 20000);
                    }
                    if mode == 'd' {
                        let big = box_slice(hh, 1048576, byte_of(1));
                        borrow big as &b in {
                            one(hh, x, c, i, "/bin/sh", "-c\0exit 0\0", "", 1000, 20000);
                            one(hh, x, c, i, "/bin/sh", "-c\0exit 4\0", contents(b), 1000, 20000);
                        }
                        unbox_slice(hh, big);
                    }
                    if mode == 'f' {
                        // Run with `pidfd_open` refused: the child cannot be watched.
                        one(hh, x, c, i, "/bin/sleep", "30\0", "", 1000, 20000);
                    }
                    if mode == 'q' {
                        // Closes both its streams, then sleeps: nothing to read or
                        // write for a second, and a capture that spun would show it.
                        let big = box_slice(hh, 1048576, byte_of(1));
                        borrow big as &b in {
                            one(hh, x, c, i, "/bin/sh", "-c\0exec <&- >&-; sleep 1\0", contents(b), 1000, 20000);
                        }
                        unbox_slice(hh, big);
                    }
                    if mode == 'n' {
                        let l0 = process.argv(hh, 16);
                        let (l1, a1) = process.add(hh, l0, "-c");
                        let (l2, a2) = process.add(hh, l1, "exit 0\0--root /");
                        let (l3, a3) = process.add(hh, l2, "exit 6");
                        io.print_int(i, a1);
                        io.space(i);
                        io.print_int(i, a2);
                        io.space(i);
                        io.print_int(i, a3);
                        io.space(i);
                        borrow l3 as &lr in {
                            io.print_int(i, process.count(lr));
                            io.newline(i);
                            one(hh, x, c, i, "/bin/sh", process.list(lr), "", 1000, 20000);
                        }
                        process.drop(hh, l3);
                    }
                }
            }
        }
    }
    release(exec);
    release(clock);
    release(console);
    release(h);
    release(args);
    return 0;
}
