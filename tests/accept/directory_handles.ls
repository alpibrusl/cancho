//~ EXIT 0

// `docs/directory-handles.md`: the shape of a program that opens beneath a
// directory. `/` is opened as the anchor; a name that is not one component
// is refused with `EINVAL` before any call, and a missing one is `ENOENT`,
// a value like any other. Every `Dir` is closed.

edition 6;

import std.dirs;

fn check[&d](dir: &d Dir) -> [dir_read] int {
    var bad = 0;
    match dir_open_read(dir, "..") {
        Opened::Ok(f) => {
            file_close(f);
            bad = bad + 1;
        }
        Opened::Failed(e) => {
            if e != dirs.einval() {
                bad = bad + 2;
            }
        }
    }
    match dirs.open_file(dir, "lex-sys-no-such-dir/x") {
        Opened::Ok(f) => {
            file_close(f);
            bad = bad + 4;
        }
        Opened::Failed(e) => {
            if e != 2 {
                bad = bad + 8;
            }
        }
    }
    match dirs.enter(dir, "./a") {
        DirOpened::Ok(d) => {
            dir_close(d);
            bad = bad + 16;
        }
        DirOpened::Failed(e) => {
            if e != dirs.einval() {
                bad = bad + 32;
            }
        }
    }
    return bad;
}

// Owning an `Fs` discharges `dir_read` as it discharges `file_read`: the
// only way to hold a `Dir` is to have held the `Fs` that paid for it, so this
// function's row is `[]`.
fn owner(fs: Fs("/")) -> [] int {
    var bad = 0;
    borrow fs as &f in {
        match open_dir(f, "/") {
            DirOpened::Ok(d) => {
                var root = d;
                borrow root as &r in {
                    match dir_enter(r, "..") {
                        DirOpened::Ok(up) => {
                            dir_close(up);
                            bad = 128;
                        }
                        DirOpened::Failed(e) => {
                        }
                    }
                }
                dir_close(root);
            }
            DirOpened::Failed(e) => {
                bad = 128;
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
    release(heap);
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
                    bad = check(r);
                }
                dir_close(root);
            }
            DirOpened::Failed(e) => {
            }
        }
    }
    return bad + owner(root_fs);
}
