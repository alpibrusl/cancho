// `docs/json.md` §4: `std.json`'s number parsing, checked against a
// reference from outside.
//
// Reads one JSON array of numbers on standard input and prints, for each
// element, the bit pattern of `json.to_float` and then `json.to_int`, one
// element per line. `conformance/json.rs` writes the array (random doubles
// in their shortest form, halfway cases between two floats, subnormals, the
// overflow edge, integers at the ends of `int`) and compares every line with
// what Rust's own `str::parse::<f64>` -- correctly rounded -- says.
import std.buffer;
import std.io;
import std.json;

fn read_stdin[&h, &i](heap: &!h Heap, io: &!i Io, text: buffer.Buffer) -> [heap, io_read] buffer.Buffer {
    var out = text;
    var c = getchar(io);
    while c >= 0 {
        out = buffer.push(heap, out, byte_of(c));
        c = getchar(io);
    }
    return out;
}

fn report[&i, &s, &t](io: &!i Io, src: &s [byte], tape: &t [int], nodes: int) -> [io_write] int {
    var j = 1;
    while j < nodes {
        io.print_int(io, bits_of(json.to_float(src, tape, j)));
        io.space(io);
        io.print_int(io, json.to_int(src, tape, j));
        io.newline(io);
        j = json.skip(tape, j);
    }
    return 0;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    var status = 0;
    borrow text as &b in {
        let src = buffer.bytes(b);
        let tape = box_slice(heap, json.tape_len(src), 0);
        borrow mut tape as &!w in {
            let nodes = json.parse(src, contents(w));
            if nodes < 0 {
                status = 1;
            } else {
                report(io, src, contents(w), nodes);
            }
        }
        unbox_slice(heap, tape);
    }
    buffer.drop(heap, text);
    return status;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(fs);
    release(ffi);
    var status = 0;
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            status = run(h, i);
        }
    }
    release(heap);
    release(io);
    return status;
}
