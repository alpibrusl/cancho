// `docs/json.md` §5: how fast `std.json` parses.
//
//     json_bench <rounds> [1] < document.json
//
// Reads the document once, parses it `<rounds>` times into one tape, and
// walks the tape once per round adding up every integer and string length --
// and every float too if a second argument of 1 says to, because converting a
// float is the expensive part of reading one -- so
// that the optimiser cannot decide the parse was unobserved. Prints the node
// count and the sum. Timing is done from outside, as the difference between
// one round and many, which leaves out the (byte-at-a-time) read of standard
// input.
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

fn number_of[&s](text: &s [byte]) -> [] int {
    var n = 0;
    var i = 0;
    while i < len(text) {
        n = n * 10 + (int_of(text[i]) - '0');
        i = i + 1;
    }
    return n;
}

fn walk[&s, &t](src: &s [byte], tape: &t [int], nodes: int, floats: int) -> [] int {
    var sum = 0;
    var j = 0;
    while j < nodes {
        let k = json.kind(tape, j);
        if k == 3 {
            sum = sum + json.to_int(src, tape, j) % 1000;
        } else if k == 4 && floats == 1 {
            sum = sum + truncate(json.to_float(src, tape, j)) % 1000;
        } else if k == 5 {
            sum = sum + len(json.string_view(src, tape, j));
        }
        j = j + 1;
    }
    return sum;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io, rounds: int, floats: int) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 1048576);
    text = read_stdin(heap, io, text);
    borrow text as &b in {
        let src = buffer.bytes(b);
        let tape = box_slice(heap, json.tape_len(src), 0);
        borrow mut tape as &!w in {
            var nodes = 0;
            var sum = 0;
            var r = 0;
            while r < rounds {
                nodes = json.parse(src, contents(w));
                sum = sum + walk(src, contents(w), nodes, floats);
                r = r + 1;
            }
            io.print_int(io, nodes);
            io.space(io);
            io.print_int(io, sum);
            io.newline(io);
        }
        unbox_slice(heap, tape);
    }
    buffer.drop(heap, text);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(fs);
    release(ffi);
    var rounds = 1;
    var floats = 0;
    borrow args as &g in {
        if arg_count(g) > 1 {
            rounds = number_of(arg(g, 1));
        }
        if arg_count(g) > 2 {
            floats = number_of(arg(g, 2));
        }
    }
    release(args);
    var status = 0;
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            status = run(h, i, rounds, floats);
        }
    }
    release(heap);
    release(io);
    return status;
}
