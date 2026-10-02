import std.buffer;
import std.bytes;
import std.test;

// `append` makes room once and stores in one pass; what it must not change is
// what the buffer holds, at every boundary of its capacity.

fn test_append_to_an_empty_buffer_of_every_small_capacity[&h](heap: &!h Heap) -> [heap] int {
    var cap = 0;
    while cap < 6 {
        var b = buffer.empty(heap, cap);
        b = buffer.append(heap, b, "");
        borrow b as &e in {
            test.assert_eq(buffer.size(e), 0);
        }
        b = buffer.append(heap, b, "abc");
        b = buffer.append(heap, b, "");
        b = buffer.append(heap, b, "de");
        borrow b as &r in {
            test.assert_eq(buffer.size(r), 5);
            test.assert(bytes.equal(buffer.bytes(r), "abcde"));
        }
        buffer.drop(heap, b);
        cap = cap + 1;
    }
    return 0;
}

// Filling the allocation exactly must not grow it; one byte more must, and
// nothing already stored may be lost either way.
fn test_append_across_the_capacity_boundary[&h](heap: &!h Heap) -> [heap] int {
    var b = buffer.empty(heap, 4);
    b = buffer.append(heap, b, "abcd");
    borrow b as &r in {
        test.assert_eq(buffer.size(r), 4);
        test.assert(bytes.equal(buffer.bytes(r), "abcd"));
    }
    b = buffer.append(heap, b, "e");
    b = buffer.push(heap, b, byte_of(102));
    b = buffer.append(heap, b, "ghijklmnop");
    borrow b as &r in {
        test.assert_eq(buffer.size(r), 16);
        test.assert(bytes.equal(buffer.bytes(r), "abcdefghijklmnop"));
    }
    buffer.drop(heap, b);
    return 0;
}

// A long text, appended after a push and before one, byte for byte.
fn test_append_a_long_text_in_place[&h](heap: &!h Heap) -> [heap] int {
    let src = box_slice(heap, 1000, byte_of(0));
    borrow mut src as &!w in {
        let s = contents(w);
        var i = 0;
        while i < 1000 {
            s[i] = byte_of(33 + (i * 7) % 90);
            i = i + 1;
        }
    }
    var b = buffer.push(heap, buffer.empty(heap, 1), byte_of(91));
    borrow src as &sr in {
        b = buffer.append(heap, b, contents(sr));
        b = buffer.append(heap, b, contents(sr)[10..20]);
    }
    b = buffer.push(heap, b, byte_of(93));
    borrow b as &r in {
        let got = buffer.bytes(r);
        test.assert_eq(buffer.size(r), 1 + 1000 + 10 + 1);
        test.assert_eq(int_of(got[0]), 91);
        var i = 0;
        while i < 1000 {
            test.assert_eq(int_of(got[1 + i]), 33 + (i * 7) % 90);
            i = i + 1;
        }
        var j = 0;
        while j < 10 {
            test.assert_eq(int_of(got[1001 + j]), 33 + ((10 + j) * 7) % 90);
            j = j + 1;
        }
        test.assert_eq(int_of(got[1011]), 93);
    }
    buffer.drop(heap, b);
    unbox_slice(heap, src);
    return 0;
}
