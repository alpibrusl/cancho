//~ EXIT 0

// `docs/byte-search.md`: `index_of_byte(text, b)` is one `memchr` -- the first `b` in `text`, or -1 -- and `std.bytes`'
// `find` and `count_byte` are built on it. Every answer here is checked against a byte loop that cannot be wrong the same way.

edition 5;

import std.bytes;

fn slow_index[&t](text: &t [byte], b: byte) -> [] int {
    var i = 0;
    while i < len(text) {
        if text[i] == b {
            return i;
        }
        i = i + 1;
    }
    return 0 - 1;
}

fn slow_find[&t, &n](text: &t [byte], needle: &n [byte]) -> [] int {
    var at = 0;
    while at + len(needle) <= len(text) {
        if bytes.equal(text[at..at + len(needle)], needle) {
            return at;
        }
        at = at + 1;
    }
    return 0 - 1;
}

fn slow_count[&t](text: &t [byte], b: int) -> [] int {
    var n = 0;
    var i = 0;
    while i < len(text) {
        if int_of(text[i]) == b {
            n = n + 1;
        }
        i = i + 1;
    }
    return n;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(io);
    release(heap);
    var bad = 0;
    // The builtin: first, last, middle, absent, empty, and the two extreme byte values.
    let text = "abcabc\nxyz";
    if index_of_byte(text, byte_of('a')) != 0 || index_of_byte(text, byte_of('z')) != 9 || index_of_byte(text, byte_of(10)) != 6 {
        bad = bad + 1;
    }
    if index_of_byte(text, byte_of('q')) != 0 - 1 || index_of_byte(text[3..3], byte_of('a')) != 0 - 1 {
        bad = bad + 2;
    }
    // A sub-slice answers relative to its own start, and nothing past its end is seen.
    if index_of_byte(text[1..10], byte_of('a')) != 2 || index_of_byte(text[0..6], byte_of('x')) != 0 - 1 {
        bad = bad + 4;
    }
    region a {
        // Every position of a long slice, with 0 and 255 as the wanted byte -- each must be found at exactly
        // that position, and not where the slice ends.
        let xs = alloc_slice[a](300, byte_of(7));
        var at = 0;
        while at < 300 {
            xs[at] = byte_of(0);
            if index_of_byte(xs, byte_of(0)) != at || index_of_byte(xs[at..300], byte_of(0)) != 0 {
                bad = bad + 8;
            }
            xs[at] = byte_of(255);
            if index_of_byte(xs, byte_of(255)) != slow_index(xs, byte_of(255)) || index_of_byte(xs, byte_of(0)) != 0 - 1 {
                bad = bad + 16;
            }
            xs[at] = byte_of(7);
            at = at + 1;
        }
    }
    // `bytes.find`, against the byte loop: false starts, the needle at the very end, a needle longer than the text,
    // an empty needle, an empty text.
    let hay = "aab aaab aaaab";
    let needles = "aaab;aaaab;b;ab;aaaaab;x; ;";
    var from = 0;
    var i = 0;
    while i <= len(needles) {
        if i == len(needles) || needles[i] == byte_of(';') {
            let needle = needles[from..i];
            if bytes.find(hay, needle) != slow_find(hay, needle) {
                bad = bad + 32;
            }
            if bytes.find(hay[0..0], needle) != slow_find(hay[0..0], needle) {
                bad = bad + 64;
            }
            from = i + 1;
        }
        i = i + 1;
    }
    if bytes.find("abc", "abcd") != 0 - 1 || bytes.find("abc", "") != 0 || bytes.find("xabc", "abc") != 1 {
        bad = bad + 128;
    }
    // `bytes.count_byte`, including a byte that is not one.
    if bytes.count_byte(hay, 'a') != slow_count(hay, 'a') || bytes.count_byte(hay, ' ') != 2 || bytes.count_byte(hay, 'z') != 0 {
        bad = bad + 256;
    }
    if bytes.count_byte(hay, 0 - 1) != 0 || bytes.count_byte(hay, 353) != 0 || bytes.count_byte("", 'a') != 0 {
        bad = bad + 512;
    }
    // An exit status keeps eight bits, and 256 and 512 must not read as success.
    if bad > 255 {
        return bad / 256;
    }
    return bad;
}
