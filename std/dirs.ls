edition 6;
module std.dirs;

// `std.dirs` -- a relative path opened beneath a directory handle, one
// component at a time, following no link.
//
// `docs/directory-handles.md` section 2. The builtins take one component
// each: `dir_enter` and `dir_open_read` refuse an empty name, `.`, `..`, a
// `/` and a NUL with `EINVAL` (22) and never follow a link. This file is the
// walk over a whole path, which is policy and so belongs where a reader can
// see it rather than in two code generators:
//
// * the path is split on `/`, and every component goes through the builtin's
//   own check, so `a//b`, `/a` (an empty first component), `a/` (an empty
//   last one), `./a` and `a/../b` are each `EINVAL`;
// * each directory on the way is entered with `O_NOFOLLOW`, so a link to a
//   directory is refused (`ENOTDIR` on Linux, `ELOOP` on Darwin), and the last
//   component is opened with it, so a link to a file is `ELOOP`;
// * every directory entered is closed before the answer is returned.
//
// The walk recurses once per component; a path deep enough for that to
// matter is not one a tool is handed.
//
// `list` is `docs/directory-listing.md` section 4: every name in a directory,
// sorted bytewise, at most `most` of them.

import std.buffer;
import std.bytes;
import std.vec;

// What a component that is not one answers. The builtins answer the same.
pub fn einval() -> [] int {
    return 22;
}

// Open `path` for reading beneath `dir`.
pub fn open_file[&d, &p](dir: &d Dir, path: &p [byte]) -> [dir_read] Opened {
    if len(path) == 0 {
        return Opened::Failed(einval());
    }
    let k = index_of_byte(path, byte_of('/'));
    if k < 0 {
        return dir_open_read(dir, path);
    }
    match dir_enter(dir, path[0..k]) {
        DirOpened::Ok(sub) => {
            var held = sub;
            // A `res` variable cannot be overwritten while it may hold a
            // file, so the placeholder is consumed before the answer lands.
            var out = Opened::Failed(einval());
            borrow held as &h in {
                match out {
                    Opened::Ok(unused) => {
                        file_close(unused);
                    }
                    Opened::Failed(unused) => {
                    }
                }
                out = open_file(h, path[k + 1..len(path)]);
            }
            dir_close(held);
            return out;
        }
        DirOpened::Failed(reason) => {
            return Opened::Failed(reason);
        }
    }
}

// Enter the directory `path` beneath `dir`: the same walk, ending in a
// directory. `.` alone is `EINVAL`, as every other component is; a caller
// that wants `dir` itself already has it.
pub fn enter[&d, &p](dir: &d Dir, path: &p [byte]) -> [dir_read] DirOpened {
    if len(path) == 0 {
        return DirOpened::Failed(einval());
    }
    let k = index_of_byte(path, byte_of('/'));
    if k < 0 {
        return dir_enter(dir, path);
    }
    match dir_enter(dir, path[0..k]) {
        DirOpened::Ok(sub) => {
            var held = sub;
            var out = DirOpened::Failed(einval());
            borrow held as &h in {
                match out {
                    DirOpened::Ok(unused) => {
                        dir_close(unused);
                    }
                    DirOpened::Failed(unused) => {
                    }
                }
                out = enter(h, path[k + 1..len(path)]);
            }
            dir_close(held);
            return out;
        }
        DirOpened::Failed(reason) => {
            return DirOpened::Failed(reason);
        }
    }
}

// What a kind is (`docs/directory-listing.md` section 3.1): the numbers
// `dir_next` answers.
pub fn kind_unknown() -> [] int {
    return 0;
}

pub fn kind_file() -> [] int {
    return 1;
}

pub fn kind_directory() -> [] int {
    return 2;
}

pub fn kind_link() -> [] int {
    return 3;
}

pub fn kind_other() -> [] int {
    return 4;
}

// Every name in a directory, in bytewise order. The names are one buffer;
// `order` is the sorted permutation of the entries, each entry a start, a
// length and a kind.
pub res struct Names {
    text: buffer.Buffer,
    starts: vec.Vec[int],
    lengths: vec.Vec[int],
    kinds: vec.Vec[int],
    order: vec.Vec[int],
    // More names were there than `most` allowed.
    truncated: bool,
    // The errno of a listing that could not start or stopped early; 0 for
    // none. The names read before it are kept.
    failed: int,
}

pub fn count[&n](names: &n Names) -> [] int {
    return vec.size(names.order);
}

// The `i`th name in bytewise order.
pub fn name[&n](names: &n Names, i: int) -> [] &n [byte] {
    let k = vec.get(names.order, i);
    let at = vec.get(names.starts, k);
    return buffer.bytes(names.text)[at..at + vec.get(names.lengths, k)];
}

// The `i`th name's kind, as `dir_next` answered it.
pub fn kind[&n](names: &n Names, i: int) -> [] int {
    return vec.get(names.kinds, vec.get(names.order, i));
}

pub fn truncated[&n](names: &n Names) -> [] bool {
    return names.truncated;
}

pub fn failed[&n](names: &n Names) -> [] int {
    return names.failed;
}

pub fn drop[&h](heap: &!h Heap, names: Names) -> [heap] int {
    let Names { text, starts, lengths, kinds, order, truncated, failed } = names;
    buffer.drop(heap, text);
    vec.drop(heap, starts);
    vec.drop(heap, lengths);
    vec.drop(heap, kinds);
    return vec.drop(heap, order);
}

// Every name beneath `dir`, at most `most`, sorted bytewise (a byte compares
// as unsigned, as `std.bytes.compare` does). Memory is the names and four
// integers an entry, plus one name buffer while reading.
pub fn list[&h, &d](heap: &!h Heap, dir: &d Dir, most: int) -> [heap, dir_read] Names {
    var text = buffer.empty(heap, 4096);
    var starts = vec.empty(heap, 64, 0);
    var lengths = vec.empty(heap, 64, 0);
    var kinds = vec.empty(heap, 64, 0);
    var truncated = false;
    var failed = 0;
    match dir_list(dir) {
        Listing::Ok(opened) => {
            var stream = opened;
            region a {
                let name = alloc_slice[a](255, byte_of(0));
                var going = true;
                while going {
                    var held = 0;
                    borrow starts as &s in {
                        held = vec.size(s);
                    }
                    borrow mut stream as &!s in {
                        match dir_next(s, name) {
                            Listed::Name(n, k) => {
                                if held >= most {
                                    truncated = true;
                                    going = false;
                                } else {
                                    var at = 0;
                                    borrow text as &t in {
                                        at = buffer.size(t);
                                    }
                                    text = buffer.append(heap, text, name[0..n]);
                                    starts = vec.push(heap, starts, at);
                                    lengths = vec.push(heap, lengths, n);
                                    kinds = vec.push(heap, kinds, k);
                                }
                            }
                            Listed::End => {
                                going = false;
                            }
                            Listed::Failed(reason) => {
                                failed = reason;
                                going = false;
                            }
                        }
                    }
                }
            }
            dir_list_close(stream);
        }
        Listing::Failed(reason) => {
            failed = reason;
        }
    }
    var order = vec.empty(heap, 1, 0);
    borrow text as &t in {
        borrow starts as &s in {
            borrow lengths as &l in {
                vec.drop(heap, order);
                order = sorted(heap, buffer.bytes(t), s, l);
            }
        }
    }
    return Names { text: text, starts: starts, lengths: lengths, kinds: kinds, order: order, truncated: truncated, failed: failed };
}

// Whether entry `i` sorts no later than entry `j`.
fn not_after[&t, &s, &l](text: &t [byte], starts: &s vec.Vec[int], lengths: &l vec.Vec[int], i: int, j: int) -> [] bool {
    let a = vec.get(starts, i);
    let b = vec.get(starts, j);
    return bytes.compare(text[a..a + vec.get(lengths, i)], text[b..b + vec.get(lengths, j)]) <= 0;
}

// The entries' sorted order: a bottom-up merge sort over indices, stable,
// `n log n` comparisons whatever the kernel's order was.
fn sorted[&h, &t, &s, &l](heap: &!h Heap, text: &t [byte], starts: &s vec.Vec[int], lengths: &l vec.Vec[int]) -> [heap] vec.Vec[int] {
    let n = vec.size(starts);
    var order = vec.empty(heap, n + 1, 0);
    var spare = vec.empty(heap, n + 1, 0);
    var i = 0;
    while i < n {
        order = vec.push(heap, order, i);
        spare = vec.push(heap, spare, 0);
        i = i + 1;
    }
    var width = 1;
    while width < n {
        borrow mut order as &!o in {
            borrow mut spare as &!w in {
                var lo = 0;
                while lo < n {
                    var mid = lo + width;
                    if mid > n {
                        mid = n;
                    }
                    var hi = lo + 2 * width;
                    if hi > n {
                        hi = n;
                    }
                    var x = lo;
                    var y = mid;
                    var k = lo;
                    while k < hi {
                        if x < mid && (y >= hi || not_after(text, starts, lengths, vec.get(o, x), vec.get(o, y))) {
                            vec.set(w, k, vec.get(o, x));
                            x = x + 1;
                        } else {
                            vec.set(w, k, vec.get(o, y));
                            y = y + 1;
                        }
                        k = k + 1;
                    }
                    lo = hi;
                }
                var c = 0;
                while c < n {
                    vec.set(o, c, vec.get(w, c));
                    c = c + 1;
                }
            }
        }
        width = width * 2;
    }
    vec.drop(heap, spare);
    return order;
}
