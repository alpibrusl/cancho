//~ EXIT 0

// `docs/directory-listing.md`: the shape of a program that lists a
// directory. `/` is listed twice -- once a name at a time with `dir_next`,
// once sorted with `std.dirs.list` -- and the two must agree on how many
// names there are, with no `.` or `..` among them and the sorted ones in
// bytewise order; `dir_stat` agrees with the listing on every kind. Every
// `DirList` and every `Dir` is closed.

edition 6;

import std.bytes;
import std.dirs;

// A name at a time, into a buffer `NAME_MAX` long.
fn counted[&d](dir: &d Dir) -> [dir_read] int {
    var count = 0;
    match dir_list(dir) {
        Listing::Ok(l) => {
            var list = l;
            region a {
                let name = alloc_slice[a](255, byte_of(0));
                var going = true;
                while going {
                    borrow mut list as &!s in {
                        match dir_next(s, name) {
                            Listed::Name(n, kind) => {
                                if bytes.equal(name[0..n], ".") || bytes.equal(name[0..n], "..") {
                                    count = 0 - 1000000;
                                }
                                count = count + 1;
                            }
                            Listed::End => {
                                going = false;
                            }
                            Listed::Failed(e) => {
                                count = 0 - 1000000;
                                going = false;
                            }
                        }
                    }
                }
            }
            dir_list_close(list);
        }
        Listing::Failed(e) => {
            count = 0 - 1000000;
        }
    }
    return count;
}

// Owning a listing outright discharges `dir_read`, as owning a `File`
// discharges `file_read`: this row is `[]`. Answers how many names it read.
fn drain(list: DirList) -> [] int {
    var stream = list;
    var count = 0;
    region a {
        let name = alloc_slice[a](255, byte_of(0));
        var going = true;
        while going {
            borrow mut stream as &!s in {
                match dir_next(s, name) {
                    Listed::Name(n, kind) => {
                        count = count + 1;
                    }
                    Listed::End => {
                        going = false;
                    }
                    Listed::Failed(e) => {
                        going = false;
                    }
                }
            }
        }
    }
    dir_list_close(stream);
    return count;
}

// All of them, sorted: answers 0 when every check holds.
fn check[&h, &d](heap: &!h Heap, dir: &d Dir) -> [heap, dir_read] int {
    var bad = 0;
    let names = dirs.list(heap, dir, 100000);
    borrow names as &n in {
        if dirs.count(n) != counted(dir) || dirs.count(n) == 0 {
            bad = bad + 1;
        }
        match dir_list(dir) {
            Listing::Ok(l) => {
                if drain(l) != dirs.count(n) {
                    bad = bad + 16;
                }
            }
            Listing::Failed(e) => {
                bad = bad + 16;
            }
        }
        var k = 1;
        while k < dirs.count(n) {
            if bytes.compare(dirs.name(n, k - 1), dirs.name(n, k)) >= 0 {
                bad = bad + 2;
            }
            k = k + 1;
        }
        if dirs.failed(n) != 0 || dirs.truncated(n) {
            bad = bad + 4;
        }
        // `dir_stat` agrees with the listing on every kind the listing knew,
        // and never answers unknown.
        var j = 0;
        while j < dirs.count(n) {
            match dir_stat(dir, dirs.name(n, j)) {
                DirStat::Ok(found, size, mtime) => {
                    let listed = dirs.kind(n, j);
                    if found == dirs.kind_unknown() || listed != dirs.kind_unknown() && found != listed {
                        bad = bad + 32;
                    }
                }
                DirStat::Failed(e) => {
                }
            }
            j = j + 1;
        }
    }
    dirs.drop(heap, names);
    return bad;
}

// Owning the `Fs` that paid for the directory discharges `dir_read`, as it
// does for every step beneath one: this row is `[]`.
fn owner(fs: Fs("/")) -> [] int {
    var bad = 8;
    borrow fs as &f in {
        match open_dir(f, "/") {
            DirOpened::Ok(d) => {
                var dir = d;
                borrow dir as &r in {
                    if counted(r) > 0 {
                        bad = 0;
                    }
                }
                dir_close(dir);
            }
            DirOpened::Failed(e) => {
            }
        }
    }
    release(fs);
    return bad;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io);
    release(ffi);
    release(args);
    release(net);
    release(clock);
    release(signals);
    let root_fs = narrow(fs, "/");
    var bad = 64;
    borrow root_fs as &f in {
        match open_dir(f, "/") {
            DirOpened::Ok(d) => {
                var root = d;
                borrow root as &r in {
                    borrow mut heap as &!h in {
                        bad = check(h, r);
                    }
                }
                dir_close(root);
            }
            DirOpened::Failed(e) => {
            }
        }
    }
    release(heap);
    return bad + owner(root_fs);
}
