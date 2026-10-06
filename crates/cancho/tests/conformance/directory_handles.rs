//! Directory handles (`docs/directory-handles.md`, issue #227): a path opened beneath a directory, one
//! component at a time, following no link.
//!
//! Each case runs one probe against a tree this test builds -- a file two directories down, a link to a
//! file outside the tree and a link to a directory outside it -- and reads what the probe printed: `ok`
//! and the bytes it read, or `err` and the `errno`. Both backends, the same expectations; the outside
//! file's bytes must never be what a probe read.

use super::*;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// `argv[1]` is the directory to open, `argv[2]` the path beneath it, `argv[3]` the mode: `walk` opens
/// the path with `dirs.open_file`, `one` hands it to `dir_open_read` as a single name, `enter` walks to a
/// directory with `dirs.enter`, and `n` hands `dir_open_read` a name holding a NUL. Prints `ok <bytes
/// read>` or `err <errno>`, and exits 2 when `open_dir` itself failed.
const PROBE: &str = r#"edition 6;

import std.dirs;

fn digits[&i](io: &!i Io, n: int) -> [io_write] int {
    if n >= 10 {
        digits(io, n / 10);
    }
    putchar(io, '0' + n % 10);
    return 0;
}

fn report[&i](io: &!i Io, ok: bool, n: int) -> [io_write] int {
    if ok {
        write_bytes(io, "ok ");
    } else {
        write_bytes(io, "err ");
    }
    digits(io, n);
    putchar(io, 10);
    return 0;
}

fn read_all[&i](io: &!i Io, opened: Opened) -> [io_write, file_read] int {
    match opened {
        Opened::Ok(f) => {
            var file = f;
            var got = 0;
            region t {
                let buf = alloc_slice[t](64, byte_of(0));
                borrow mut file as &!h in {
                    match file_read(h, buf) {
                        Read::Got(n) => { got = n; }
                        Read::End => { got = 0; }
                        Read::Failed(e) => { got = 1000 + e; }
                    }
                }
            }
            file_close(file);
            report(io, true, got);
            return 0;
        }
        Opened::Failed(e) => {
            report(io, false, e);
            return 1;
        }
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(ffi); release(heap); release(net); release(clock); release(signals);
    var status = 0;
    borrow args as &a in {
        borrow fs as &f in {
            borrow mut io as &!o in {
                match open_dir(f, arg(a, 1)) {
                    DirOpened::Ok(d0) => {
                        var d = d0;
                        borrow d as &r in {
                            let mode = arg(a, 3);
                            if len(mode) == 4 {
                                status = read_all(o, dirs.open_file(r, arg(a, 2)));
                            } else if len(mode) == 3 {
                                status = read_all(o, dir_open_read(r, arg(a, 2)));
                            } else if len(mode) == 1 {
                                // A NUL cannot come through `argv`: `a` exists, so a check that
                                // missed it would open `a` and read nothing.
                                status = read_all(o, dir_open_read(r, "a\0b"));
                            } else {
                                match dirs.enter(r, arg(a, 2)) {
                                    DirOpened::Ok(sub) => {
                                        dir_close(sub);
                                        report(o, true, 0);
                                    }
                                    DirOpened::Failed(e) => {
                                        report(o, false, e);
                                        status = 1;
                                    }
                                }
                            }
                        }
                        dir_close(d);
                    }
                    DirOpened::Failed(e) => {
                        report(o, false, e);
                        status = 2;
                    }
                }
            }
        }
    }
    release(args); release(fs); release(io);
    return status;
}
"#;

const EINVAL: i32 = 22;
const ENOENT: i32 = 2;
const ENOTDIR: i32 = 20;
const ELOOP: i32 = if cfg!(target_os = "macos") { 62 } else { 40 };

/// The tree: `root/a/b/f.txt` (seven bytes), `root/link.txt` to a file outside, `root/dirlink` to the
/// directory outside, and `outside/s.txt`, which no probe may read.
fn tree(dir: &Path) -> PathBuf {
    let root = dir.join("root");
    std::fs::create_dir_all(root.join("a/b")).expect("a writable scratch directory");
    std::fs::create_dir_all(dir.join("outside")).expect("a writable scratch directory");
    std::fs::write(root.join("a/b/f.txt"), "inside\n").expect("a writable scratch directory");
    std::fs::write(dir.join("outside/s.txt"), "SECRET-SECRET\n")
        .expect("a writable scratch directory");
    std::os::unix::fs::symlink("../outside/s.txt", root.join("link.txt")).expect("a symlink");
    std::os::unix::fs::symlink("../outside", root.join("dirlink")).expect("a symlink");
    root
}

fn build(dir: &Path, backend: &str) -> PathBuf {
    let file = dir.join("probe.cho");
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

fn probe(exe: &Path, root: &Path, path: &str, mode: &str) -> String {
    probe_status(exe, root, path, mode).0
}

fn probe_status(exe: &Path, root: &Path, path: &str, mode: &str) -> (String, i32) {
    let out = Command::new(exe).arg(root).arg(path).arg(mode).output().expect("the probe runs");
    let code = out.status.code();
    assert!(code.is_some(), "`{path}` ({mode}) was killed by a signal");
    (String::from_utf8_lossy(&out.stdout).trim().to_owned(), code.unwrap_or(-1))
}

#[test]
fn a_path_beneath_a_directory_follows_no_link_on_both_backends() {
    for backend in BACKENDS {
        let dir = scratch(&format!("directory-handles-{backend}"));
        let root = tree(&dir);
        let exe = build(&dir, backend);
        let err = |e: i32| format!("err {e}");
        let cases: Vec<(&str, &str, Vec<String>)> = vec![
            // The file two directories down, through the walk, and the directory above it.
            ("a/b/f.txt", "walk", vec!["ok 7".to_owned()]),
            ("a/b", "enter", vec!["ok 0".to_owned()]),
            // A link to a file outside, last component: refused, not read.
            ("link.txt", "walk", vec![err(ELOOP)]),
            ("link.txt", "one", vec![err(ELOOP)]),
            // A link to a directory outside, entered on the way: refused.
            ("dirlink/s.txt", "walk", vec![err(ENOTDIR), err(ELOOP)]),
            ("dirlink", "enter", vec![err(ENOTDIR), err(ELOOP)]),
            // What `O_NOFOLLOW` does not cover, refused by the component check.
            ("../outside/s.txt", "walk", vec![err(EINVAL)]),
            ("a/../a/b/f.txt", "walk", vec![err(EINVAL)]),
            ("./a/b/f.txt", "walk", vec![err(EINVAL)]),
            ("a//b/f.txt", "walk", vec![err(EINVAL)]),
            ("/a/b/f.txt", "walk", vec![err(EINVAL)]),
            ("a/b/", "walk", vec![err(EINVAL)]),
            ("", "walk", vec![err(EINVAL)]),
            ("..", "enter", vec![err(EINVAL)]),
            // The builtin alone: one name, so a `/`, `.` and `..` are refused without a call.
            ("a/b/f.txt", "one", vec![err(EINVAL)]),
            ("..", "one", vec![err(EINVAL)]),
            (".", "one", vec![err(EINVAL)]),
            ("", "one", vec![err(EINVAL)]),
            // Ordinary failures are values.
            ("a/b/missing", "walk", vec![err(ENOENT)]),
            ("a/b/f.txt/x", "walk", vec![err(ENOTDIR)]),
        ];
        for (path, mode, expected) in &cases {
            let got = probe(&exe, &root, path, mode);
            assert!(
                expected.contains(&got),
                "`{path}` ({mode}) on {backend}: expected one of {expected:?}, got {got:?}"
            );
            assert!(!got.contains("14"), "`{path}` ({mode}) on {backend} read the outside file");
        }
        // A name longer than `NAME_MAX` is refused before the copy; 255 bytes reach the kernel.
        let long = "x".repeat(256);
        assert_eq!(probe(&exe, &root, &long, "one"), err(EINVAL), "{backend}");
        assert_eq!(probe(&exe, &root, &"x".repeat(255), "one"), err(ENOENT), "{backend}");
        // A NUL inside a name, which `argv` cannot carry.
        assert_eq!(probe(&exe, &root, "x", "n"), err(EINVAL), "{backend}");
        // `open_dir` of a file, and of nothing: `open_dir` itself answers (exit 2), not a step after it.
        let file = root.join("a/b/f.txt");
        assert_eq!(probe_status(&exe, &file, "x", "walk"), (err(ENOTDIR), 2), "{backend}");
        assert_eq!(
            probe_status(&exe, &dir.join("absent"), "x", "walk"),
            (err(ENOENT), 2),
            "{backend}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `open_dir` checks its path against the capability's prefix exactly as `open_read` does: outside
/// it, the program is killed, on both backends.
#[test]
fn open_dir_outside_the_prefix_traps_on_both_backends() {
    let source = "edition 6;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io); release(ffi); release(heap); release(args); release(net); release(clock); release(signals);
    let tmp = narrow(fs, \"/tmp/cancho-dirs-prefix\");
    var status = 0;
    borrow tmp as &t in {
        match open_dir(t, \"/etc\") {
            DirOpened::Ok(d) => { dir_close(d); status = 1; }
            DirOpened::Failed(e) => { status = 2; }
        }
    }
    release(tmp);
    return status;
}
";
    for backend in BACKENDS {
        let dir = scratch(&format!("directory-handles-prefix-{backend}"));
        let file = dir.join("outside.cho");
        std::fs::write(&file, source).expect("a writable fixture");
        let exe = dir.join("outside");
        let out = Command::new(BIN)
            .args([
                "build".as_ref(),
                file.as_os_str(),
                "--backend".as_ref(),
                backend.as_ref(),
                "-o".as_ref(),
                exe.as_os_str(),
            ])
            .output()
            .expect("the compiler runs");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(
            run.status.code(),
            None,
            "`open_dir` outside its prefix on {backend} should trap"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
