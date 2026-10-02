//~ EXIT 0

// `docs/memory-moves.md`: `copy_within` is `memmove` inside one slice. The ranges may overlap, in either direction, and
// the result is as if the bytes had been copied out first -- which a byte-by-byte loop in the wrong direction gets wrong.

edition 5;

fn fill[&b](xs: &!b [byte]) -> [] int {
    var i = 0;
    while i < len(xs) {
        xs[i] = byte_of(i + 1);
        i = i + 1;
    }
    return 0;
}

// The bytes of `xs`, as one number: 3-digit base-256 would not fit for 8 bytes, so a simple checksum that is sensitive
// to position: sum of (i + 1) * byte.
fn weigh[&b](xs: &b [byte]) -> [] int {
    var total = 0;
    var i = 0;
    while i < len(xs) {
        total = total + (i + 1) * int_of(xs[i]);
        i = i + 1;
    }
    return total;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(heap);
    release(io);
    var bad = 0;
    region a {
        // 1 2 3 4 5 6 7 8
        let xs = alloc_slice[a](8, byte_of(0));
        fill(xs);
        // Forward and overlapping (dst < src): 3 4 5 6 5 6 7 8 would be a byte loop's right answer too.
        copy_within(xs, 0, 2, 4);
        if int_of(xs[0]) != 3 || int_of(xs[1]) != 4 || int_of(xs[2]) != 5 || int_of(xs[3]) != 6 || int_of(xs[4]) != 5 {
            bad = bad + 1;
        }
        // Backward and overlapping (dst > src): a front-to-back loop would smear xs[0]; memmove does not.
        fill(xs);
        copy_within(xs, 2, 0, 5);
        if int_of(xs[0]) != 1 || int_of(xs[1]) != 2 || int_of(xs[2]) != 1 || int_of(xs[3]) != 2 || int_of(xs[4]) != 3 || int_of(xs[5]) != 4 || int_of(xs[6]) != 5 || int_of(xs[7]) != 8 {
            bad = bad + 2;
        }
        // The whole slice onto itself, and nothing at the very end, and nothing from the very end.
        fill(xs);
        let before = weigh(xs);
        copy_within(xs, 0, 0, 8);
        copy_within(xs, 8, 0, 0);
        copy_within(xs, 0, 8, 0);
        copy_within(xs, 3, 3, 0);
        if weigh(xs) != before {
            bad = bad + 4;
        }
        // The last bytes onto the first, and a one-byte move.
        copy_within(xs, 0, 5, 3);
        if int_of(xs[0]) != 6 || int_of(xs[1]) != 7 || int_of(xs[2]) != 8 || int_of(xs[3]) != 4 {
            bad = bad + 8;
        }
        copy_within(xs, 7, 0, 1);
        if int_of(xs[7]) != 6 {
            bad = bad + 16;
        }
        // A sub-slice moves inside itself, and nothing outside it is touched.
        fill(xs);
        copy_within(xs[2..6], 0, 1, 3);
        if int_of(xs[2]) != 4 || int_of(xs[3]) != 5 || int_of(xs[4]) != 6 || int_of(xs[5]) != 6 || int_of(xs[6]) != 7 || int_of(xs[1]) != 2 {
            bad = bad + 32;
        }
    }
    return bad;
}
