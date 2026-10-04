//! Writing beneath a directory handle (`docs/directory-handles.md` §3, issue #227 slice 2): create,
//! append, rename, remove and sync, each on one path component, following no link.
//!
//! Each case runs one probe against a tree this test builds and reads back what it printed (`ok` and a
//! number, or `err` and the `errno`) and what is on disk afterwards. Both backends, the same
//! expectations; nothing outside the tree may be created, changed or removed.

use super::*;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// `argv[1]` is the directory to open, `argv[2]` the operation, `argv[3]` (and `argv[4]` for a rename)
/// the names. `new` and `append` write `hello` to what they open; `rename`, `remove` and `sync` answer
/// `Done`. Prints `ok <n>` or `err <errno>`.
const PROBE: &str = r#"edition 6;

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

fn write_hello[&i](io: &!i Io, opened: Opened) -> [io_write, file_write] int {
    match opened {
        Opened::Ok(f) => {
            var file = f;
            var wrote = 0;
            borrow mut file as &!h in {
                match file_write(h, "hello") {
                    Done::Ok(n) => { wrote = n; }
                    Done::Failed(e) => { wrote = 1000 + e; }
                }
            }
            file_close(file);
            report(io, true, wrote);
            return 0;
        }
        Opened::Failed(e) => {
            report(io, false, e);
            return 1;
        }
    }
}

fn done[&i](io: &!i Io, answer: Done) -> [io_write] int {
    match answer {
        Done::Ok(n) => { report(io, true, n); return 0; }
        Done::Failed(e) => { report(io, false, e); return 1; }
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
                            let op = arg(a, 2);
                            if op[0] == byte_of('n') {
                                status = write_hello(o, dir_open_new(r, arg(a, 3)));
                            } else if op[0] == byte_of('a') {
                                status = write_hello(o, dir_open_append(r, arg(a, 3)));
                            } else if op[0] == byte_of('m') {
                                status = done(o, dir_rename(r, arg(a, 3), arg(a, 4)));
                            } else if op[0] == byte_of('r') {
                                status = done(o, dir_remove(r, arg(a, 3)));
                            } else {
                                status = done(o, dir_sync(r));
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
const EEXIST: i32 = 17;
const ELOOP: i32 = if cfg!(target_os = "macos") { 62 } else { 40 };

fn build(dir: &Path, backend: &str) -> PathBuf {
    let file = dir.join("probe.ls");
    std::fs::write(&file, PROBE).expect("a writable fixture");
    let exe = dir.join(format!("probe-{backend}"));
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
    assert!(
        out.status.success(),
        "`--backend {backend}`: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}

fn probe(exe: &Path, root: &Path, args: &[&str]) -> String {
    let out = Command::new(exe).arg(root).args(args).output().expect("the probe runs");
    assert!(out.status.code().is_some(), "{args:?} was killed by a signal");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[test]
fn writing_beneath_a_directory_follows_no_link_on_both_backends() {
    for backend in BACKENDS {
        let dir = scratch(&format!("directory-writes-{backend}"));
        let root = dir.join("root");
        let outside = dir.join("outside");
        std::fs::create_dir_all(&root).expect("a writable scratch directory");
        std::fs::create_dir_all(&outside).expect("a writable scratch directory");
        std::fs::write(outside.join("kept.txt"), "KEPT").expect("a writable scratch directory");
        // A link to a file outside, and a dangling one whose target is outside.
        std::os::unix::fs::symlink("../outside/kept.txt", root.join("link.txt"))
            .expect("a symlink");
        std::os::unix::fs::symlink("../outside/new.txt", root.join("dangling.txt"))
            .expect("a symlink");
        let exe = build(&dir, backend);
        let err = |e: i32| format!("err {e}");
        let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_else(|_| "<absent>".to_owned());

        // Create: once, then refused; through either link, refused, and nothing appears outside.
        assert_eq!(probe(&exe, &root, &["new", "f"]), "ok 5", "{backend}");
        assert_eq!(read(&root.join("f")), "hello", "{backend}");
        assert_eq!(probe(&exe, &root, &["new", "f"]), err(EEXIST), "{backend}");
        assert_eq!(probe(&exe, &root, &["new", "link.txt"]), err(EEXIST), "{backend}");
        assert_eq!(probe(&exe, &root, &["new", "dangling.txt"]), err(EEXIST), "{backend}");
        assert_eq!(read(&outside.join("new.txt")), "<absent>", "{backend}: created through a link");
        // Append: created if missing, then added to; through a link, refused.
        assert_eq!(probe(&exe, &root, &["append", "log"]), "ok 5", "{backend}");
        assert_eq!(probe(&exe, &root, &["append", "log"]), "ok 5", "{backend}");
        assert_eq!(read(&root.join("log")), "hellohello", "{backend}");
        assert_eq!(probe(&exe, &root, &["append", "link.txt"]), err(ELOOP), "{backend}");
        assert_eq!(probe(&exe, &root, &["append", "dangling.txt"]), err(ELOOP), "{backend}");
        assert_eq!(read(&outside.join("kept.txt")), "KEPT", "{backend}: written through a link");
        assert_eq!(read(&outside.join("new.txt")), "<absent>", "{backend}: created through a link");
        // Rename within the directory; every name is one component.
        assert_eq!(probe(&exe, &root, &["move", "f", "g"]), "ok 0", "{backend}");
        assert_eq!(
            (read(&root.join("f")), read(&root.join("g"))),
            ("<absent>".to_owned(), "hello".to_owned())
        );
        assert_eq!(probe(&exe, &root, &["move", "g", "../escaped"]), err(EINVAL), "{backend}");
        assert_eq!(probe(&exe, &root, &["move", "..", "x"]), err(EINVAL), "{backend}");
        assert_eq!(probe(&exe, &root, &["move", "g", ""]), err(EINVAL), "{backend}");
        assert_eq!(read(&dir.join("escaped")), "<absent>", "{backend}");
        assert_eq!(probe(&exe, &root, &["move", "missing", "x"]), err(ENOENT), "{backend}");
        // Remove: a file, a link (the link goes, its target stays), nothing, and not a component.
        assert_eq!(probe(&exe, &root, &["remove", "g"]), "ok 0", "{backend}");
        assert_eq!(read(&root.join("g")), "<absent>", "{backend}");
        assert_eq!(probe(&exe, &root, &["remove", "link.txt"]), "ok 0", "{backend}");
        assert!(
            std::fs::symlink_metadata(root.join("link.txt")).is_err(),
            "{backend}: the link stayed"
        );
        assert_eq!(read(&outside.join("kept.txt")), "KEPT", "{backend}: removed through a link");
        assert_eq!(probe(&exe, &root, &["remove", "missing"]), err(ENOENT), "{backend}");
        assert_eq!(
            probe(&exe, &root, &["remove", "../outside/kept.txt"]),
            err(EINVAL),
            "{backend}"
        );
        assert_eq!(read(&outside.join("kept.txt")), "KEPT", "{backend}");
        // Sync the directory itself.
        assert_eq!(probe(&exe, &root, &["sync"]), "ok 0", "{backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A file made by `dir_open_new` gets mode `0644`, as one made by `open_new` does: on Apple AArch64
/// the mode is a variadic argument on the stack, so a call shaped wrongly there gives junk bits.
#[test]
fn a_created_file_is_0644_on_both_backends() {
    use std::os::unix::fs::PermissionsExt;
    for backend in BACKENDS {
        let dir = scratch(&format!("directory-writes-mode-{backend}"));
        let root = dir.join("root");
        std::fs::create_dir_all(&root).expect("a writable scratch directory");
        let exe = build(&dir, backend);
        assert_eq!(probe(&exe, &root, &["new", "made"]), "ok 5", "{backend}");
        assert_eq!(probe(&exe, &root, &["append", "added"]), "ok 5", "{backend}");
        for name in ["made", "added"] {
            let mode =
                std::fs::metadata(root.join(name)).expect("the file exists").permissions().mode();
            // The umask can only clear bits; `0o644` under the usual `022` stays `0o644`.
            assert_eq!(mode & 0o7133, 0, "{backend}: `{name}` has mode {mode:o}");
            assert_eq!(mode & 0o600, 0o600, "{backend}: `{name}` has mode {mode:o}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
