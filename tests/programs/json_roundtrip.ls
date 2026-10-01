// `docs/json.md` §4: parse standard input and write it back.
//
// Prints `E <code> <position>` if `std.json` refuses the document, or the
// document re-serialised by `std.json`'s writer from the tape otherwise.
// `conformance/json.rs` feeds it valid documents, and the same documents
// with bytes damaged, and checks both halves against `serde_json`: that the
// two parsers accept and refuse the same inputs, and that what comes back
// out means what went in.
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

// Write node `i` and everything inside it; answers the writer and the index
// of the node after.
fn emit[&h, &s, &t](heap: &!h Heap, w: json.Writer, src: &s [byte], tape: &t [int], i: int) -> [heap] (json.Writer, int) {
    var out = w;
    let k = json.kind(tape, i);
    if k == 0 {
        out = json.put_null(heap, out);
        return (out, i + 1);
    }
    if k == 1 || k == 2 {
        out = json.put_bool(heap, out, json.to_bool(tape, i));
        return (out, i + 1);
    }
    if k == 3 {
        if json.fits_int(src, tape, i) {
            out = json.put_int(heap, out, json.to_int(src, tape, i));
        } else {
            out = json.put_float(heap, out, json.to_float(src, tape, i));
        }
        return (out, i + 1);
    }
    if k == 4 {
        out = json.put_float(heap, out, json.to_float(src, tape, i));
        return (out, i + 1);
    }
    if k == 5 {
        region a {
            let text = alloc_slice[a](json.string_length(src, tape, i) + 1, byte_of(0));
            let n = json.string_into(src, tape, i, text);
            out = json.put_string(heap, out, text[0..n]);
        }
        return (out, i + 1);
    }
    var j = i + 1;
    var left = json.count(tape, i);
    if k == 6 {
        out = json.begin_array(heap, out);
        while left > 0 {
            let (next_writer, next) = emit(heap, out, src, tape, j);
            out = next_writer;
            j = next;
            left = left - 1;
        }
        out = json.end_array(heap, out);
    } else {
        out = json.begin_object(heap, out);
        while left > 0 {
            region a {
                let text = alloc_slice[a](json.string_length(src, tape, j) + 1, byte_of(0));
                let n = json.string_into(src, tape, j, text);
                out = json.put_key(heap, out, text[0..n]);
            }
            let (next_writer, next) = emit(heap, out, src, tape, j + 1);
            out = next_writer;
            j = next;
            left = left - 1;
        }
        out = json.end_object(heap, out);
    }
    return (out, j);
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    borrow text as &b in {
        let src = buffer.bytes(b);
        let tape = box_slice(heap, json.tape_len(src), 0);
        borrow mut tape as &!w in {
            let nodes = json.parse(src, contents(w));
            if nodes < 0 {
                io.write_all(io, "E ");
                io.print_int(io, json.error_code(nodes));
                io.space(io);
                io.print_int(io, json.error_position(nodes));
                io.newline(io);
            } else {
                let (done, end) = emit(heap, json.writer(heap, 64), src, contents(w), 0);
                borrow done as &d in {
                    io.write_all(io, json.bytes(d));
                }
                json.drop(heap, done);
                io.newline(io);
            }
        }
        unbox_slice(heap, tape);
    }
    buffer.drop(heap, text);
    return 0;
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
