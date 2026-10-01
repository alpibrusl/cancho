// `docs/json.md` §5: how fast `std.json` writes.
//
//     json_write_bench <objects>
//
// Writes an array of that many small objects --
// `{"id":7,"name":"user_7","score":3.5,"active":true,"tags":["a","b"]}` --
// and prints the length of the document, so the work cannot be dropped.
import std.buffer;
import std.io;
import std.json;

fn number_of[&s](text: &s [byte]) -> [] int {
    var n = 0;
    var i = 0;
    while i < len(text) {
        n = n * 10 + (int_of(text[i]) - '0');
        i = i + 1;
    }
    return n;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io, objects: int) -> [heap, io_write] int {
    var w = json.writer(heap, 1048576);
    w = json.begin_array(heap, w);
    var k = 0;
    while k < objects {
        w = json.begin_object(heap, w);
        w = json.put_key(heap, w, "id");
        w = json.put_int(heap, w, k);
        w = json.put_key(heap, w, "name");
        w = json.put_string(heap, w, "user_name");
        w = json.put_key(heap, w, "score");
        w = json.put_float(heap, w, float_of(k % 1000) / 8.0);
        w = json.put_key(heap, w, "active");
        w = json.put_bool(heap, w, k % 2 == 0);
        w = json.put_key(heap, w, "tags");
        w = json.begin_array(heap, w);
        w = json.put_string(heap, w, "a");
        w = json.put_string(heap, w, "b");
        w = json.end_array(heap, w);
        w = json.end_object(heap, w);
        k = k + 1;
    }
    w = json.end_array(heap, w);
    var length = 0;
    borrow w as &r in {
        length = len(json.bytes(r));
    }
    json.drop(heap, w);
    io.print_int(io, length);
    io.newline(io);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(fs);
    release(ffi);
    var objects = 1000;
    borrow args as &g in {
        if arg_count(g) > 1 {
            objects = number_of(arg(g, 1));
        }
    }
    release(args);
    var status = 0;
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            status = run(h, i, objects);
        }
    }
    release(heap);
    release(io);
    return status;
}
