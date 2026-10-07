edition 5;
import std.io;

// 50 million int64 values (400 MB), the same ones tests/duck.py loads into DuckDB: x[i] = ((i * 2654435761) >> 7) & 1023.
// Times, best of 5, a checked sum, a wrapping sum, and a filtered count (x > 500), checked and wrapping.

fn fill[&h](heap: &!h Heap, n: int) -> [heap] Box[[int]] {
    let held = box_slice(heap, n, 0);
    borrow mut held as &!b in {
        let v = contents(b);
        var i = 0;
        while i < n {
            v[i] = i * 2654435761 >> 7 & 1023;
            i = i + 1;
        }
    }
    return held;
}

fn sum_checked[&b](v: &b [int]) -> [] int {
    var total = 0;
    var i = 0;
    while i < len(v) {
        total = total + v[i];
        i = i + 1;
    }
    return total;
}

fn sum_wrapping[&b](v: &b [int]) -> [] int {
    var total = 0;
    var i = 0;
    while i < len(v) {
        total = wrapping_add(total, v[i]);
        i = wrapping_add(i, 1);
    }
    return total;
}

fn count_checked[&b](v: &b [int], above: int) -> [] int {
    var c = 0;
    var i = 0;
    while i < len(v) {
        if v[i] > above {
            c = c + 1;
        }
        i = i + 1;
    }
    return c;
}

fn count_wrapping[&b](v: &b [int], above: int) -> [] int {
    var c = 0;
    var i = 0;
    while i < len(v) {
        if v[i] > above {
            c = wrapping_add(c, 1);
        }
        i = wrapping_add(i, 1);
    }
    return c;
}

fn show[&i](io: &!i Io, what: &static [byte], ms: int, answer: int) -> [io_write] int {
    io.write_all(io, what);
    io.write_all(io, " ");
    io.print_nat(io, ms);
    io.write_all(io, " ms  answer ");
    io.print_nat(io, answer);
    io.write_all(io, "\n");
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    var h0 = heap;
    borrow mut h0 as &!h in {
        let held = fill(h, 50000000);
        borrow held as &b in {
            borrow mut io as &!i in {
                borrow clock as &c in {
                    var kind = 0;
                    while kind < 4 {
                        var best = 999999;
                        var answer = 0;
                        var rep = 0;
                        while rep < 5 {
                            let t0 = clock_ms(c);
                            if kind == 0 {
                                answer = sum_checked(contents(b));
                            } else if kind == 1 {
                                answer = sum_wrapping(contents(b));
                            } else if kind == 2 {
                                answer = count_checked(contents(b), 500);
                            } else {
                                answer = count_wrapping(contents(b), 500);
                            }
                            let t1 = clock_ms(c);
                            if t1 - t0 < best {
                                best = t1 - t0;
                            }
                            rep = rep + 1;
                        }
                        if kind == 0 {
                            show(i, "sum, checked      ", best, answer);
                        } else if kind == 1 {
                            show(i, "sum, wrapping     ", best, answer);
                        } else if kind == 2 {
                            show(i, "count x>500, checked ", best, answer);
                        } else {
                            show(i, "count x>500, wrapping", best, answer);
                        }
                        kind = kind + 1;
                    }
                }
            }
        }
        unbox_slice(h, held);
    }
    release(h0);
    release(clock);
    release(io);
    return 0;
}
