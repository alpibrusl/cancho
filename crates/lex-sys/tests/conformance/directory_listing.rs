//! Directory listing (`docs/directory-listing.md`, issue #222): the names beneath a `Dir`, one at a time
//! with `dir_next` or all of them sorted with `std.dirs.list`.
//!
//! Each case runs one probe against a directory this test builds and compares what it printed with what
//! Rust's own `read_dir` and `symlink_metadata` see. A name is printed as hex, so a newline or a byte that
//! is not UTF-8 survives the trip. Both backends, the same expectations.

use super::*;
use std::os::unix::ffi::OsStrExt;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// `argv[1]` is the directory, `argv[2]` the mode: `sorted` prints `std.dirs.list`'s answer, `raw` every
/// `dir_next` answer in the kernel's order, `short` the first `dir_next` answer into a three-byte buffer,
/// `cap` `std.dirs.list` with at most two names, and `twice` two listings of one `Dir` read in turns.
/// Each name is a line `<hex> <kind>`; a failure is `err <errno>`.
const PROBE: &str = r#"edition 6;

import std.dirs;

fn digits[&i](io: &!i Io, n: int) -> [io_write] int {
    if n >= 10 {
        digits(io, n / 10);
    }
    putchar(io, '0' + n % 10);
    return 0;
}

fn hex[&i](io: &!i Io, d: int) -> [io_write] int {
    if d < 10 {
        putchar(io, '0' + d);
    } else {
        putchar(io, 'a' + d - 10);
    }
    return 0;
}

fn entry[&i, &n](io: &!i Io, name: &n [byte], kind: int) -> [io_write] int {
    var k = 0;
    while k < len(name) {
        hex(io, int_of(name[k]) / 16);
        hex(io, int_of(name[k]) % 16);
        k = k + 1;
    }
    putchar(io, ' ');
    digits(io, kind);
    putchar(io, 10);
    return 0;
}

fn failed[&i](io: &!i Io, e: int) -> [io_write] int {
    write_bytes(io, "err ");
    digits(io, e);
    putchar(io, 10);
    return 0;
}

// Every `dir_next` answer into a buffer of `room` bytes, at most `most` of them.
fn raw[&i, &d](io: &!i Io, dir: &d Dir, room: int, most: int) -> [io_write, dir_read] int {
    match dir_list(dir) {
        Listing::Ok(l) => {
            var list = l;
            var seen = 0;
            region a {
                let name = alloc_slice[a](room, byte_of(0));
                var going = seen < most;
                while going {
                    borrow mut list as &!s in {
                        match dir_next(s, name) {
                            Listed::Name(n, kind) => {
                                entry(io, name[0..n], kind);
                            }
                            Listed::End => {
                                going = false;
                            }
                            Listed::Failed(e) => {
                                failed(io, e);
                                going = false;
                            }
                        }
                    }
                    seen = seen + 1;
                    if seen >= most {
                        going = false;
                    }
                }
            }
            dir_list_close(list);
            return 0;
        }
        Listing::Failed(e) => {
            failed(io, e);
            return 1;
        }
    }
}

// Two listings of one directory, read in turns: each must see every name once.
fn twice[&i, &d](io: &!i Io, dir: &d Dir) -> [io_write, dir_read] int {
    match dir_list(dir) {
        Listing::Ok(l1) => {
            var one = l1;
            match dir_list(dir) {
                Listing::Ok(l2) => {
                    var two = l2;
                    region a {
                        let name = alloc_slice[a](255, byte_of(0));
                        var going = true;
                        while going {
                            going = false;
                            borrow mut one as &!s in {
                                match dir_next(s, name) {
                                    Listed::Name(n, kind) => {
                                        entry(io, name[0..n], kind);
                                        going = true;
                                    }
                                    Listed::End => {
                                    }
                                    Listed::Failed(e) => {
                                        failed(io, e);
                                    }
                                }
                            }
                            borrow mut two as &!s in {
                                match dir_next(s, name) {
                                    Listed::Name(n, kind) => {
                                        entry(io, name[0..n], kind);
                                        going = true;
                                    }
                                    Listed::End => {
                                    }
                                    Listed::Failed(e) => {
                                        failed(io, e);
                                    }
                                }
                            }
                        }
                    }
                    dir_list_close(two);
                }
                Listing::Failed(e) => {
                    failed(io, e);
                }
            }
            dir_list_close(one);
            return 0;
        }
        Listing::Failed(e) => {
            failed(io, e);
            return 1;
        }
    }
}

fn sorted[&h, &i, &d](heap: &!h Heap, io: &!i Io, dir: &d Dir, most: int) -> [heap, io_write, dir_read] int {
    let names = dirs.list(heap, dir, most);
    borrow names as &n in {
        var k = 0;
        while k < dirs.count(n) {
            entry(io, dirs.name(n, k), dirs.kind(n, k));
            k = k + 1;
        }
        if dirs.truncated(n) {
            write_bytes(io, "truncated\n");
        }
        if dirs.failed(n) != 0 {
            failed(io, dirs.failed(n));
        }
    }
    dirs.drop(heap, names);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(ffi); release(net); release(clock); release(signals);
    var status = 0;
    borrow args as &a in {
        borrow fs as &f in {
            borrow mut io as &!o in {
                borrow mut heap as &!p in {
                    match open_dir(f, arg(a, 1)) {
                        DirOpened::Ok(d0) => {
                            var d = d0;
                            borrow d as &r in {
                                let mode = arg(a, 2);
                                if len(mode) == 6 {
                                    status = sorted(p, o, r, 1000000);
                                } else if len(mode) == 3 && int_of(mode[0]) == 'r' {
                                    status = raw(o, r, 255, 1000000);
                                } else if len(mode) == 3 {
                                    status = sorted(p, o, r, 2);
                                } else if len(mode) == 5 && int_of(mode[0]) == 's' {
                                    status = raw(o, r, 3, 1);
                                } else {
                                    status = twice(o, r);
                                }
                            }
                            dir_close(d);
                        }
                        DirOpened::Failed(e) => {
                            failed(o, e);
                            status = 2;
                        }
                    }
                }
            }
        }
    }
    release(args); release(fs); release(io); release(heap);
    return status;
}
"#;

fn build(dir: &Path, backend: &str) -> PathBuf {
    let file = dir.join("probe.ls");
    std::fs::write(&file, PROBE).expect("a writable fixture");
    let exe = dir.join(format!("probe-{backend}"));
    let out = Command::new(BIN)
        .args([
            "build".as_ref(),
            file.as_os_str(),
            "--std".as_ref(),
            "--backend".as_ref(),
            backend.as_ref(),
            "-o".as_ref(),
            exe.as_os_str(),
        ])
        .output()
        .expect("the compiler runs");
    assert!(
        out.status.success(),
        "`--backend {backend}`: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}

fn probe(exe: &Path, dir: &Path, mode: &str) -> Vec<String> {
    let out = Command::new(exe).arg(dir).arg(mode).output().expect("the probe runs");
    assert!(out.status.code().is_some(), "`{mode}` was killed by a signal");
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned).collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What the language calls an entry's kind (§3.1), from the file type `lstat` reports.
fn kind(path: &Path) -> u8 {
    let t = std::fs::symlink_metadata(path).expect("an entry").file_type();
    if t.is_symlink() {
        3
    } else if t.is_dir() {
        2
    } else if t.is_file() {
        1
    } else {
        4
    }
}

/// What a listing must print: every entry Rust sees, sorted as bytes, `<hex> <kind>`.
fn expected(dir: &Path) -> Vec<String> {
    let mut names: Vec<Vec<u8>> = std::fs::read_dir(dir)
        .expect("a readable directory")
        .map(|e| e.expect("an entry").file_name().as_bytes().to_vec())
        .collect();
    names.sort();
    names
        .iter()
        .map(|n| {
            let path = dir.join(std::ffi::OsStr::from_bytes(n));
            format!("{} {}", hex(n), kind(&path))
        })
        .collect()
}

/// The hostile directory of §6: a newline, a byte that is not UTF-8, a 255-byte name, a dangling link, a
/// link to `..`, a FIFO and a subdirectory, among `count` plain files.
fn hostile(dir: &Path, count: usize) -> PathBuf {
    let root = dir.join("hostile");
    std::fs::create_dir_all(root.join("sub")).expect("a writable scratch directory");
    for i in 0..count {
        std::fs::write(root.join(format!("f{i:06}")), "").expect("a writable scratch directory");
    }
    for name in [&b"new\nline"[..], b"caf\xe9", b"cafe", b"Z", b"-dash"] {
        std::fs::write(root.join(std::ffi::OsStr::from_bytes(name)), "")
            .expect("a writable scratch directory");
    }
    std::fs::write(root.join("x".repeat(255)), "").expect("a 255-byte name");
    std::os::unix::fs::symlink("nowhere", root.join("dangling")).expect("a symlink");
    std::os::unix::fs::symlink("..", root.join("up")).expect("a symlink");
    let fifo = root.join("fifo");
    let made = Command::new("mkfifo").arg(&fifo).status().expect("mkfifo runs");
    assert!(made.success(), "mkfifo");
    root
}

#[test]
fn a_hostile_directory_lists_sorted_and_identically_on_both_backends() {
    let scratch = scratch("directory-listing");
    let root = hostile(&scratch, 100_000);
    let want = expected(&root);
    assert_eq!(want.len(), 100_000 + 10, "the tree this test built");
    let mut seen = Vec::new();
    for backend in BACKENDS {
        let exe = build(&scratch, backend);
        let sorted = probe(&exe, &root, "sorted");
        assert_eq!(sorted, want, "`--backend {backend}`: std.dirs.list");
        // `dir_next` alone: the same entries, in the kernel's order, with no `.` and no `..`.
        let mut raw = probe(&exe, &root, "raw");
        raw.sort_by_key(|line| {
            let hex = line.split(' ').next().unwrap_or("");
            (0..hex.len() / 2)
                .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap_or(0))
                .collect::<Vec<u8>>()
        });
        assert_eq!(raw, want, "`--backend {backend}`: dir_next");
        seen.push(sorted);
    }
    assert_eq!(seen[0], seen[1], "the two backends list differently");
}

#[test]
fn a_short_buffer_a_cap_and_two_listings_on_both_backends() {
    let scratch = scratch("directory-listing-edges");
    let root = scratch.join("small");
    std::fs::create_dir_all(&root).expect("a writable scratch directory");
    for name in ["longname", "b", "a"] {
        std::fs::write(root.join(name), "").expect("a writable scratch directory");
    }
    let only = scratch.join("only");
    std::fs::create_dir_all(&only).expect("a writable scratch directory");
    std::fs::write(only.join("longname"), "").expect("a writable scratch directory");
    let too_long = if cfg!(target_os = "macos") { 63 } else { 36 };
    for backend in BACKENDS {
        let exe = build(&scratch, backend);
        // One entry, longer than the buffer: refused whole, nothing copied.
        assert_eq!(probe(&exe, &only, "short"), vec![format!("err {too_long}")], "{backend}");
        // At most two: the first two in bytewise order of what was read, and said to be cut.
        let cap = probe(&exe, &root, "cap");
        assert_eq!(cap.len(), 3, "`--backend {backend}`: {cap:?}");
        assert_eq!(cap[2], "truncated", "`--backend {backend}`");
        // Two listings of one `Dir` do not share a position: each sees all three names.
        let mut both = probe(&exe, &root, "twice");
        both.sort();
        let mut want: Vec<String> =
            expected(&root).into_iter().flat_map(|l| [l.clone(), l]).collect();
        want.sort();
        assert_eq!(both, want, "`--backend {backend}`: two listings");
    }
}
