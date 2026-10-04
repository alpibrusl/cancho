// `docs/ecdh.md` §3: the timing of `std.ecdh.shared`, for the
// dudect-style test `scripts/ecdh_timing.py` runs.
//
// Standard input is one header line, `<curve>` (256 or 384, in
// decimal), then records in lowercase hex, each a scalar (curve / 8
// bytes) and an uncompressed peer point (1 + curve / 4 bytes), back to
// back with no separator.
//
// Everything is read and decoded before anything is timed. Each call's
// time is read with the `tick` library's `lexsys_tick` (a cycle
// counter), so this program is built with `-l tick -L <dir>`; the times
// are printed, one a line, once every call has run.
import std.buffer;
import std.ecdh;
import std.io;

extern fn lexsys_tick[&f](ffi: &f Ffi("tick")) -> [ffi("tick")] int;

fn read_stdin[&h, &i](heap: &!h Heap, io: &!i Io, text: buffer.Buffer) -> [heap, io_read] buffer.Buffer {
    var out = text;
    var c = getchar(io);
    while c >= 0 {
        out = buffer.push(heap, out, byte_of(c));
        c = getchar(io);
    }
    return out;
}

// The decimal number at `at`, and where it ends.
fn number_end[&s](s: &s [byte], at: int) -> [] int {
    var e = at;
    while e < len(s) && int_of(s[e]) >= 48 && int_of(s[e]) <= 57 {
        e = e + 1;
    }
    return e;
}

fn number[&s](s: &s [byte], at: int) -> [] int {
    var n = 0;
    var i = at;
    while i < number_end(s, at) {
        n = n * 10 + int_of(s[i]) - 48;
        i = i + 1;
    }
    return n;
}

fn nibble(c: int) -> [] int {
    if c >= 97 {
        return c - 87;
    }
    return c - 48;
}

// Decodes the records into `all`, then times every call into `times`.
fn time_into[&f, &s, &a, &t, &w](ffi: &f Ffi("tick"), s: &s [byte], start: int, curve: int, all: &!a [byte], times: &!t [int], work: &!w [int]) -> [ffi("tick")] int {
    let size = curve / 8;
    let rec = size + 1 + 2 * size;
    var j = 0;
    while j < len(all) {
        all[j] = byte_of(nibble(int_of(s[start + 2 * j])) * 16 + nibble(int_of(s[start + 2 * j + 1])));
        j = j + 1;
    }
    region r {
        let out = alloc_slice[r](size, byte_of(0));
        var i = 0;
        while i < len(times) {
            let at = i * rec;
            let scalar = all[at..at + size];
            let peer = all[at + size..at + rec];
            let t0 = lexsys_tick(ffi);
            ecdh.shared(curve, scalar, peer, out, work);
            times[i] = lexsys_tick(ffi) - t0;
            i = i + 1;
        }
    }
    return 0;
}

fn time_all[&h, &i, &f, &s](heap: &!h Heap, io: &!i Io, ffi: &f Ffi("tick"), s: &s [byte]) -> [heap, io_write, ffi("tick")] int {
    let curve = number(s, 0);
    let start = number_end(s, 0) + 1;
    let size = curve / 8;
    let rec = size + 1 + 2 * size;
    let count = (len(s) - start) / (2 * rec);
    // On the heap: a region holds at most one 64 KiB chunk.
    let all = box_slice(heap, count * rec, byte_of(0));
    let times = box_slice(heap, count, 0);
    let work = box_slice(heap, ecdh.work_len(), 0);
    borrow mut all as &!x in {
        borrow mut times as &!y in {
            borrow mut work as &!z in {
                time_into(ffi, s, start, curve, contents(x), contents(y), contents(z));
            }
        }
    }
    borrow times as &y in {
        let t = contents(y);
        var i = 0;
        while i < len(t) {
            io.print_int(io, t[i]);
            io.newline(io);
            i = i + 1;
        }
    }
    unbox_slice(heap, all);
    unbox_slice(heap, times);
    unbox_slice(heap, work);
    return 0;
}

fn run[&h, &i, &f](heap: &!h Heap, io: &!i Io, ffi: &f Ffi("tick")) -> [heap, io_read, io_write, ffi("tick")] int {
    var text = buffer.empty(heap, 1 << 20);
    text = read_stdin(heap, io, text);
    borrow text as &b in {
        time_all(heap, io, ffi, buffer.bytes(b));
    }
    buffer.drop(heap, text);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(fs);
    let tick = narrow(ffi, "tick");
    borrow tick as &f in {
        borrow mut heap as &!h in {
            borrow mut io as &!i in {
                run(h, i, f);
            }
        }
    }
    release(tick);
    release(heap);
    release(io);
    return 0;
}
