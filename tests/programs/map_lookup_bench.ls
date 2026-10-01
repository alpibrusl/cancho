// `docs/map.md` §5: the cost of one lookup, with key construction taken
// out of the measurement.
//
//     map_lookup_bench <keys> <rounds>
//
// Builds `keys` keys of 12 bytes into one buffer, puts them all, then
// looks every one up `rounds` times and prints how many were found.
// Timing is done from outside as the difference between two round counts.
import std.buffer;
import std.io;
import std.map;

fn number_at[&g](args: &g Args, at: int, otherwise: int) -> [args] int {
    if arg_count(args) <= at {
        return otherwise;
    }
    let a = arg(args, at);
    var n = 0;
    var i = 0;
    while i < len(a) {
        n = n * 10 + (int_of(a[i]) - '0');
        i = i + 1;
    }
    return n;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io, n: int, rounds: int) -> [heap, io_write] int {
    // "key-" and eight digits, so every key is 12 bytes and sits at
    // `12 * k` in the buffer.
    var text = buffer.empty(heap, n * 12 + 16);
    var k = 0;
    while k < n {
        text = buffer.append(heap, text, "key-");
        var d = 10000000;
        while d > 0 {
            text = buffer.push(heap, text, byte_of('0' + k / d % 10));
            d = d / 10;
        }
        k = k + 1;
    }
    var m = map.empty(heap, 16, 0 - 1, 0);
    var found = 0;
    borrow text as &t in {
        let all = buffer.bytes(t);
        k = 0;
        while k < n {
            m = map.put(heap, m, all[k * 12..k * 12 + 12], k);
            k = k + 1;
        }
        borrow m as &r in {
            var round = 0;
            while round < rounds {
                k = 0;
                while k < n {
                    if map.get(r, all[k * 12..k * 12 + 12], 0 - 1) >= 0 {
                        found = found + 1;
                    }
                    k = k + 1;
                }
                round = round + 1;
            }
        }
    }
    io.print_int(io, found);
    io.newline(io);
    map.drop(heap, m);
    buffer.drop(heap, text);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi);
    release(fs);
    var n = 100000;
    var rounds = 10;
    borrow args as &g in {
        n = number_at(g, 1, n);
        rounds = number_at(g, 2, rounds);
    }
    var status = 0;
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            status = run(h, i, n, rounds);
        }
    }
    release(args);
    release(io);
    release(heap);
    return status;
}
