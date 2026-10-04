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
