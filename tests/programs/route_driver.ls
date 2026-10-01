// `docs/http.md` §6: `std.route`, checked against a reference matcher.
//
// Reads a script on standard input, one command a line:
//
//     R <method> <pattern> <id>      add a route
//     Q <method> <path>              look one up
//
// and prints, for each `Q`, the answer (an id, -1 or -2) followed by the
// parameter table's first `2 * most_params` integers. `conformance/http.rs`
// generates the script and compares every line with a matcher written from
// the rules in `docs/http.md` §5 rather than from this library.
import std.buffer;
import std.bytes;
import std.io;
import std.route;

fn read_stdin[&h, &i](heap: &!h Heap, io: &!i Io, text: buffer.Buffer) -> [heap, io_read] buffer.Buffer {
    var out = text;
    var c = getchar(io);
    while c >= 0 {
        out = buffer.push(heap, out, byte_of(c));
        c = getchar(io);
    }
    return out;
}

fn number[&s](text: &s [byte]) -> [] int {
    var n = 0;
    var i = 0;
    while i < len(text) {
        n = n * 10 + (int_of(text[i]) - 48);
        i = i + 1;
    }
    return n;
}

fn answer[&i, &r, &t](io: &!i Io, router: &r route.Router, method: &r [byte], path: &r [byte], table: &!t [int]) -> [io_write] int {
    let id = route.find(router, method, path, table);
    io.print_int(io, id);
    var k = 0;
    while k < len(table) {
        io.space(io);
        io.print_int(io, table[k]);
        k = k + 1;
    }
    io.newline(io);
    return id;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    var router = route.empty(heap);
    var table = box_slice(heap, 2, 0);
    var queries = 0;
    borrow text as &b in {
        let all = buffer.bytes(b);
        var at = 0;
        while at < len(all) {
            var end = at;
            while end < len(all) && int_of(all[end]) != 10 {
                end = end + 1;
            }
            let line = all[at..end];
            at = end + 1;
            let command = bytes.field(line, 32, 1);
            let method = bytes.field(line, 32, 2);
            if int_of(command[0]) == 'R' {
                router = route.add(heap, router, method, bytes.field(line, 32, 3), number(bytes.field(line, 32, 4)));
            } else {
                // The table is sized for the widest route known so far.
                var widest = 0;
                borrow router as &rr in {
                    widest = route.most_params(rr);
                }
                unbox_slice(heap, table);
                table = box_slice(heap, 2 * widest, 0);
                borrow router as &rr in {
                    borrow mut table as &!w in {
                        answer(io, rr, method, bytes.field(line, 32, 3), contents(w));
                    }
                }
                queries = queries + 1;
            }
        }
    }
    unbox_slice(heap, table);
    route.drop(heap, router);
    buffer.drop(heap, text);
    return queries;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(ffi);
    release(fs);
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            run(h, i);
        }
    }
    release(io);
    release(heap);
    return 0;
}
