// `docs/map.md` §5: how fast `std.map` is.
//
//     map_bench <n>
//
// Puts `n` keys of the form `key-<i>`, looks every one up, removes every
// other one, looks every one up again, and prints how many were found
// and the sum of their values -- so the optimiser cannot drop the work.
// Timing is done from outside; `n` of a few million makes the build of
// the keys a small part of it.
import std.buffer;
import std.io;
import std.map;

fn key_of[&h](heap: &!h Heap, k: int) -> [heap] buffer.Buffer {
    let b = buffer.empty(heap, 24);
    let p = buffer.append(heap, b, "key-");
    return buffer.push_nat(heap, p, k);
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io, n: int) -> [heap, io_write] int {
    var m = map.empty(heap, 16, 0 - 1, 0);
    var k = 0;
    while k < n {
        let key = key_of(heap, k);
        borrow key as &b in {
            m = map.put(heap, m, buffer.bytes(b), k);
        }
        buffer.drop(heap, key);
        k = k + 1;
    }
    var found = 0;
    var sum = 0;
    k = 0;
    while k < n {
        let key = key_of(heap, k);
        borrow key as &b in {
            borrow m as &r in {
                let v = map.get(r, buffer.bytes(b), 0 - 1);
                if v >= 0 {
                    found = found + 1;
                    sum = sum + v;
                }
            }
        }
        buffer.drop(heap, key);
        k = k + 1;
    }
    k = 0;
    while k < n {
        let key = key_of(heap, k);
        borrow key as &b in {
            borrow mut m as &!w in {
                map.remove(w, buffer.bytes(b));
            }
        }
        buffer.drop(heap, key);
        k = k + 2;
    }
    k = 0;
    while k < n {
        let key = key_of(heap, k);
        borrow key as &b in {
            borrow m as &r in {
                let v = map.get(r, buffer.bytes(b), 0 - 1);
                if v >= 0 {
                    found = found + 1;
                    sum = sum + v;
                }
            }
        }
        buffer.drop(heap, key);
        k = k + 1;
    }
    io.print_int(io, found);
    io.space(io);
    io.print_int(io, sum);
    io.newline(io);
    map.drop(heap, m);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi);
    release(fs);
    var n = 1000000;
    borrow args as &g in {
        if arg_count(g) > 1 {
            n = 0;
            let a = arg(g, 1);
            var i = 0;
            while i < len(a) {
                n = n * 10 + (int_of(a[i]) - '0');
                i = i + 1;
            }
        }
    }
    var status = 0;
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            status = run(h, i, n);
        }
    }
    release(args);
    release(io);
    release(heap);
    return status;
}
