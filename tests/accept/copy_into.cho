//~ EXIT 0

// `docs/bulk-copy.md`: `copy_into(dst, src)` is all of `src` onto the front of `dst`, as one `memmove`, and answers
// `len(src)`. The two may be views of one buffer, in either order.

edition 5;

import std.buffer;

fn fill[&b](xs: &!b [byte]) -> [] int {
    var i = 0;
    while i < len(xs) {
        xs[i] = byte_of(i + 1);
        i = i + 1;
    }
    return 0;
}

// `std.buffer` grows through `copy_into`: append past several doublings, then read every byte back.
fn grow[&h](heap: &!h Heap) -> [heap] int {
    var bad = 0;
    var b = buffer.empty(heap, 1);
    var round = 0;
    while round < 40 {
        region t {
            let chunk = alloc_slice[t](7, byte_of(0));
            var i = 0;
            while i < 7 {
                chunk[i] = byte_of((round * 7 + i) % 256);
                i = i + 1;
            }
            b = buffer.append(heap, b, chunk);
        }
        round = round + 1;
    }
    borrow b as &r in {
        let got = buffer.bytes(r);
        if len(got) != 280 {
            bad = bad + 1;
        }
        var j = 0;
        while j < len(got) {
            if int_of(got[j]) != j % 256 {
                bad = bad + 2;
                j = len(got);
            }
            j = j + 1;
        }
    }
    buffer.drop(heap, b);
    return bad * 64;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(io);
    var bad = 0;
    region a {
        let xs = alloc_slice[a](8, byte_of(0));
        let ys = alloc_slice[a](5, byte_of(9));
        // Between two slices: 1 2 3 onto the front of 9 9 9 9 9, and the rest untouched.
        fill(xs);
        if copy_into(ys, xs[0..3]) != 3 {
            bad = bad + 1;
        }
        if int_of(ys[0]) != 1 || int_of(ys[1]) != 2 || int_of(ys[2]) != 3 || int_of(ys[3]) != 9 || int_of(ys[4]) != 9 {
            bad = bad + 2;
        }
        // Exactly full, and nothing at all (into nothing, too).
        if copy_into(xs[3..8], ys) != 5 || int_of(xs[3]) != 1 || int_of(xs[7]) != 9 || int_of(xs[2]) != 3 {
            bad = bad + 4;
        }
        if copy_into(ys, xs[0..0]) != 0 || copy_into(ys[5..5], xs[8..8]) != 0 || int_of(ys[0]) != 1 {
            bad = bad + 8;
        }
        // One buffer, overlapping both ways: a front-to-back loop smears the forward case.
        fill(xs);
        copy_into(xs[2..8], xs[0..5]);
        if int_of(xs[0]) != 1 || int_of(xs[1]) != 2 || int_of(xs[2]) != 1 || int_of(xs[3]) != 2 || int_of(xs[6]) != 5 || int_of(xs[7]) != 8 {
            bad = bad + 16;
        }
        fill(xs);
        copy_into(xs[0..8], xs[3..8]);
        if int_of(xs[0]) != 4 || int_of(xs[4]) != 8 || int_of(xs[5]) != 6 || int_of(xs[7]) != 8 {
            bad = bad + 32;
        }
    }
    // `std.buffer` grows through `copy_into`: append past several doublings, then read every byte back.
    borrow mut heap as &!h in {
        bad = bad + grow(h);
    }
    release(heap);
    return bad;
}
