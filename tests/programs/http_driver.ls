// `docs/http.md` §6: `std.http`'s parser, checked against `httparse`.
//
// Reads frames from standard input -- a decimal length, a newline, then
// that many bytes -- and prints one line per frame:
//
//     E <code> <position>
//     OK <body_start> <method_end> <target_start> <target_end> <path_end>
//        <version> <headers> <body_start> <content_length> <flags>
//        {<ns> <ne> <vs> <ve>}*
//
// `conformance/http.rs` writes the frames (valid requests, and every
// mutation of them) and compares every line with what the reference parser
// says about the same bytes.
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

fn report[&i, &t](io: &!i Io, r: int, table: &t [int]) -> [io_write] int {
    if r < 0 {
        io.write_all(io, "E");
        put(io, http.error_code(r));
        put(io, http.error_position(r));
        io.newline(io);
        return 0;
    }
    io.write_all(io, "OK");
    put(io, r);
    var k = 0;
    while k < 9 {
        put(io, table[k]);
        k = k + 1;
    }
    var h = 0;
    while h < table[5] * 4 {
        put(io, table[16 + h]);
        h = h + 1;
    }
    io.newline(io);
    return 0;
}

fn frames[&i, &s, &t](io: &!i Io, all: &s [byte], table: &!t [int]) -> [io_write] int {
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
        let r = http.parse(frame, table);
        report(io, r, table);
        cases = cases + 1;
    }
    return cases;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    let table = box_slice(heap, http.slots(64), 0);
    borrow text as &b in {
        borrow mut table as &!w in {
            frames(io, buffer.bytes(b), contents(w));
        }
    }
    unbox_slice(heap, table);
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
