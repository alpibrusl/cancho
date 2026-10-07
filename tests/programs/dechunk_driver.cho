// `docs/http.md` §5.1: `std.http.dechunk`, checked against a second decoder
// written independently in `conformance/http.rs`.
//
// Reads frames from standard input -- a decimal length, a newline, then that
// many bytes -- and prints one line per frame, decoding into 256 bytes of room:
//
//     <consumed> <decoded> <the decoded bytes in hex>
//
// where `consumed` is negative for the refusals and "not all here yet".
import std.buffer;
import std.http;
import std.io;

fn read_stdin[&h, &i](heap: &!h Heap, io: &!i Io, text: buffer.Buffer) -> [heap, io_read] buffer.Buffer {
    var out = text;
    var c = getchar(io);
    while c >= 0 {
        out = buffer.push(heap, out, byte_of(c));
        c = getchar(io);
    }
    return out;
}

fn put[&i](io: &!i Io, n: int) -> [io_write] int {
    io.space(io);
    io.print_int(io, n);
    return n;
}

fn hex[&i](io: &!i Io, n: int) -> [io_write] int {
    let digits = "0123456789abcdef";
    io.write_all(io, digits[n / 16..n / 16 + 1]);
    io.write_all(io, digits[n % 16..n % 16 + 1]);
    return n;
}

fn frames[&i, &s, &o](io: &!i Io, all: &s [byte], out: &!o [byte]) -> [io_write] int {
    var at = 0;
    var cases = 0;
    while at < len(all) {
        var size = 0;
        while at < len(all) && int_of(all[at]) != 10 {
            size = size * 10 + (int_of(all[at]) - 48);
            at = at + 1;
        }
        at = at + 1;
        let frame = all[at..at + size];
        at = at + size;
        let (used, n) = http.dechunk(frame, out);
        io.print_int(io, used);
        put(io, n);
        io.space(io);
        var k = 0;
        while k < n {
            hex(io, int_of(out[k]));
            k = k + 1;
        }
        io.newline(io);
        cases = cases + 1;
    }
    return cases;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    let out = box_slice(heap, 256, byte_of(0));
    borrow text as &b in {
        borrow mut out as &!w in {
            frames(io, buffer.bytes(b), contents(w));
        }
    }
    unbox_slice(heap, out);
    buffer.drop(heap, text);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(ffi);
    release(fs);
    var status = 0;
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            status = run(h, i);
        }
    }
    release(io);
    release(heap);
    return status;
}
