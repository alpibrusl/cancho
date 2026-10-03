//! The write side of a file handle (`docs/file-writes.md`): the opens that append, create and
//! update, and the verbs that write, sync, cut and measure.
//!
//! Every test judges the result from *outside* the program, for the reason `filesystem.md` section 2.2
//! gave: a read that happens to succeed is not evidence about a mode, and a sync that returned `Ok` is
//! not evidence that it called anything. The bytes are read by the test; the flags and the mode are
//! read by the kernel and by `strace`.

use super::*;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// A `main` that narrows `Fs` to `dir`, borrows it as `f` and runs `body` in the borrow. `code` is the
/// exit status.
fn program(dir: &Path, body: &str) -> String {
    format!(
        "edition 5;\n\
         fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args, net, clock }} = split(world);\n\
             release(args); release(heap); release(ffi); release(io); release(net); release(clock);\n\
             let d = narrow(fs, \"{dir}\");\n\
             var code = 0;\n\
             borrow d as &f in {{\n{body}\n}}\n\
             release(d);\n\
             return code;\n\
         }}\n",
        dir = dir.display()
    )
}

/// Open `path` with `opener`, run `inner` with the handle as `h`, close it. A failed open sets `code` to
/// `100 + errno`.
fn with_file(opener: &str, path: &Path, inner: &str) -> String {
    format!(
        "match {opener}(f, \"{path}\") {{\n\
             Opened::Failed(e) => {{ code = 100 + e; }}\n\
             Opened::Ok(opened) => {{\n\
                 var file = opened;\n\
                 borrow mut file as &!h in {{\n{inner}\n}}\n\
                 file_close(file);\n\
             }}\n\
         }}\n",
        path = path.display()
    )
}

fn build_program(dir: &Path, name: &str, source: &str, backend: &str) -> PathBuf {
    let file = dir.join(format!("{name}-{backend}.ls"));
    std::fs::write(&file, source).expect("a writable fixture");
    let exe = dir.join(format!("{name}-{backend}"));
    let build = Command::new(BIN)
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
        build.status.success(),
        "`--backend {backend}` should build `{name}`, but said:\n{}\n{source}",
        String::from_utf8_lossy(&build.stderr)
    );
    exe
}

/// Build and run `body` on both backends, each over a fresh copy of `before` (the files the program
/// starts with), and hand `check` the directory and the exit status.
fn on_both(
    tag: &str,
    before: &[(&str, &[u8])],
    body: impl Fn(&Path) -> String,
    check: impl Fn(&str, &Path, Option<i32>),
) {
    for backend in BACKENDS {
        let dir = scratch(&format!("file-writes-{tag}-{backend}"));
        let data = dir.join("data");
        std::fs::create_dir_all(&data).unwrap();
        for (name, bytes) in before {
            std::fs::write(data.join(name), bytes).unwrap();
        }
        let exe = build_program(&dir, tag, &program(&data, &body(&data)), backend);
        let run = Command::new(&exe).output().expect("the compiled program runs");
        check(backend, &data, run.status.code());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn open_append_keeps_what_is_there_and_writes_at_the_end() {
    // `"ab"`: an existing file is not truncated, and every write goes to the end. The program also
    // answers the sizes it was told, so a count that lied would show.
    on_both(
        "append",
        &[("log.bin", b"AB")],
        |data| {
            with_file(
                "open_append",
                &data.join("log.bin"),
                "match file_write(h, \"cd\") { Done::Ok(n) => { code = n; } Done::Failed(e) => { code = 90 + e; } }\n\
                 match file_write(h, \"ef\") { Done::Ok(n) => { code = code * 10 + n; } Done::Failed(e) => { code = 90 + e; } }\n\
                 match file_size(h) { Done::Ok(n) => { code = code * 10 + n; } Done::Failed(e) => { code = 80 + e; } }",
            )
        },
        |backend, data, status| {
            assert_eq!(
                std::fs::read(data.join("log.bin")).unwrap(),
                b"ABcdef",
                "{backend}: the bytes on disk"
            );
            // 2 written, 2 written, size 6.
            assert_eq!(status, Some(226), "{backend}: counts and size as the program saw them");
        },
    );
}

#[test]
fn open_append_creates_a_missing_file() {
    on_both(
        "append-create",
        &[],
        |data| {
            with_file(
                "open_append",
                &data.join("new.bin"),
                "match file_write(h, \"xyz\") { Done::Ok(n) => { code = n; } Done::Failed(e) => { code = 90 + e; } }",
            )
        },
        |backend, data, status| {
            assert_eq!(status, Some(3), "{backend}");
            assert_eq!(std::fs::read(data.join("new.bin")).unwrap(), b"xyz", "{backend}");
        },
    );
}

#[test]
fn open_write_truncates_and_open_new_refuses_a_file_that_is_there() {
    // `"wb"` empties an existing file; `"wbx"` is `O_EXCL` and answers `Failed(EEXIST)` (17), which is
    // the reason a lock file or a fresh segment is created with it.
    on_both(
        "write-and-new",
        &[("old.bin", b"previous contents"), ("held.bin", b"keep")],
        |data| {
            let mut s = with_file(
                "open_write",
                &data.join("old.bin"),
                "match file_write(h, \"fresh\") { Done::Ok(n) => { code = n; } Done::Failed(e) => { code = 90 + e; } }",
            );
            // `open_new` on a file that exists: the open fails, `code` becomes 100 + 17 and the file is
            // untouched.
            s.push_str(&with_file("open_new", &data.join("held.bin"), ""));
            s
        },
        |backend, data, status| {
            assert_eq!(
                std::fs::read(data.join("old.bin")).unwrap(),
                b"fresh",
                "{backend}: truncated"
            );
            assert_eq!(
                std::fs::read(data.join("held.bin")).unwrap(),
                b"keep",
                "{backend}: `open_new` left it alone"
            );
            assert_eq!(
                status,
                Some(117),
                "{backend}: `open_new` of an existing file is `Failed(EEXIST)`"
            );
        },
    );
}

#[test]
fn open_new_creates_a_file_that_is_not_there() {
    on_both(
        "new-missing",
        &[],
        |data| {
            with_file(
                "open_new",
                &data.join("fresh.bin"),
                "match file_write(h, \"abc\") { Done::Ok(n) => { code = n; } Done::Failed(e) => { code = 90 + e; } }",
            )
        },
        |backend, data, status| {
            assert_eq!(status, Some(3), "{backend}");
            assert_eq!(std::fs::read(data.join("fresh.bin")).unwrap(), b"abc", "{backend}");
        },
    );
}

#[test]
fn open_rw_refuses_a_file_that_is_missing() {
    // `"r+b"` does not create; a missing file is `Failed(ENOENT)` (2).
    on_both(
        "rw-missing",
        &[],
        |data| with_file("open_rw", &data.join("absent.bin"), ""),
        |backend, data, status| {
            assert_eq!(status, Some(102), "{backend}: `Failed(ENOENT)`");
            assert!(!data.join("absent.bin").exists(), "{backend}: nothing was created");
        },
    );
}

#[test]
fn pwrite_and_pread_take_an_offset_and_leave_the_cursor_alone() {
    // `AAAA`, `pwrite("BB", 1)` makes `ABBA`. `pread` at 1 sees `BB`. And neither moved the cursor: a
    // `file_read` afterwards starts at 0, so it sees `AB`.
    on_both(
        "positional",
        &[("rw.bin", b"AAAA")],
        |data| {
            with_file(
                "open_rw",
                &data.join("rw.bin"),
                "match file_pwrite(h, 1, \"BB\") { Done::Ok(n) => { code = n; } Done::Failed(e) => { code = 90 + e; } }\n\
                 region a {\n\
                     var buf = alloc_slice[a](2, byte_of(0));\n\
                     match file_pread(h, 1, buf) {\n\
                         Read::Got(n) => { code = code * 10 + n; if int_of(buf[0]) == 66 { code = code + 100; } }\n\
                         Read::End => { code = 71; }\n\
                         Read::Failed(e) => { code = 70 + e; }\n\
                     }\n\
                     match file_read(h, buf) {\n\
                         Read::Got(n) => { if int_of(buf[1]) == 66 { code = code + 1000; } }\n\
                         Read::End => { code = 72; }\n\
                         Read::Failed(e) => { code = 70 + e; }\n\
                     }\n\
                 }",
            )
        },
        |backend, data, status| {
            assert_eq!(
                std::fs::read(data.join("rw.bin")).unwrap(),
                b"ABBA",
                "{backend}: the bytes on disk"
            );
            // 2 written, 2 read (22), first read byte was 'B' (+100), and `file_read` from the cursor
            // saw `AB`, whose second byte is 'B' (+1000).
            assert_eq!(status, Some((22 + 100 + 1000) % 256), "{backend}");
        },
    );
}

#[test]
fn pread_past_the_end_is_end() {
    on_both(
        "pread-end",
        &[("short.bin", b"abc")],
        |data| {
            with_file(
                "open_rw",
                &data.join("short.bin"),
                "region a {\n\
                     var buf = alloc_slice[a](4, byte_of(0));\n\
                     match file_pread(h, 3, buf) {\n\
                         Read::Got(n) => { code = 50 + n; }\n\
                         Read::End => { code = 7; }\n\
                         Read::Failed(e) => { code = 70 + e; }\n\
                     }\n\
                 }",
            )
        },
        |backend, _data, status| assert_eq!(status, Some(7), "{backend}"),
    );
}

#[test]
fn pwrite_on_an_append_handle_still_appends() {
    // Documented Linux behaviour, measured and written down in `docs/file-writes.md` section 4.2, so it
    // is a known fact and not a surprise: on a handle opened for append, the offset is ignored. It is why
    // `open_rw` exists as a separate open.
    on_both(
        "pwrite-append",
        &[("a.bin", b"AAAA")],
        |data| {
            with_file(
                "open_append",
                &data.join("a.bin"),
                "match file_pwrite(h, 0, \"BB\") { Done::Ok(n) => { code = n; } Done::Failed(e) => { code = 90 + e; } }",
            )
        },
        |backend, data, status| {
            if cfg!(target_os = "linux") {
                assert_eq!(std::fs::read(data.join("a.bin")).unwrap(), b"AAAABB", "{backend}");
            }
            assert_eq!(status, Some(2), "{backend}");
        },
    );
}

#[test]
fn truncate_cuts_and_size_leaves_the_cursor_where_it_was() {
    // `hello world` cut to 5. `file_size` reads the end and puts the cursor back, so a `file_read` after
    // it still starts at 0 and sees `h`.
    on_both(
        "truncate",
        &[("t.bin", b"hello world")],
        |data| {
            with_file(
                "open_rw",
                &data.join("t.bin"),
                "match file_truncate(h, 5) { Done::Ok(n) => { code = n; } Done::Failed(e) => { code = 90 + e; } }\n\
                 match file_size(h) { Done::Ok(n) => { code = code * 10 + n; } Done::Failed(e) => { code = 80 + e; } }\n\
                 region a {\n\
                     var buf = alloc_slice[a](1, byte_of(0));\n\
                     match file_read(h, buf) {\n\
                         Read::Got(n) => { if int_of(buf[0]) == 104 { code = code + 100; } }\n\
                         Read::End => { code = 72; }\n\
                         Read::Failed(e) => { code = 70 + e; }\n\
                     }\n\
                 }",
            )
        },
        |backend, data, status| {
            assert_eq!(std::fs::read(data.join("t.bin")).unwrap(), b"hello", "{backend}");
            // truncate answers 0, size answers 5, the read saw 'h'.
            assert_eq!(status, Some(5 + 100), "{backend}");
        },
    );
}

#[test]
fn a_verb_the_kernel_refuses_answers_failed_with_the_errno() {
    // Section 5.1: `Done` and `Read` carry the reason. A write on a handle opened for reading is `EBADF`
    // (9), a `pread` on one opened only for append is `EBADF`, and a negative length to `ftruncate` is
    // `EINVAL` (22). Each sets one bit when it saw exactly its errno, so a `Failed` that was reported as
    // `Ok`, or with the wrong reason, leaves a bit out.
    on_both(
        "refusals",
        &[("r.bin", b"abc"), ("a.bin", b"abc"), ("w.bin", b"abc")],
        |data| {
            let mut s = with_file(
                "open_read",
                &data.join("r.bin"),
                "match file_write(h, \"x\") { Done::Ok(n) => { code = code + 64; } Done::Failed(e) => { if e == 9 { code = code + 1; } } }",
            );
            s.push_str(&with_file(
                "open_append",
                &data.join("a.bin"),
                "region a {\n\
                     var buf = alloc_slice[a](2, byte_of(0));\n\
                     match file_pread(h, 0, buf) {\n\
                         Read::Got(n) => { code = code + 64; }\n\
                         Read::End => { code = code + 64; }\n\
                         Read::Failed(e) => { if e == 9 { code = code + 2; } }\n\
                     }\n\
                 }",
            ));
            s.push_str(&with_file(
                "open_rw",
                &data.join("w.bin"),
                "match file_truncate(h, 0 - 1) { Done::Ok(n) => { code = code + 64; } Done::Failed(e) => { if e == 22 { code = code + 4; } } }",
            ));
            s
        },
        |backend, data, status| {
            assert_eq!(status, Some(7), "{backend}: each refusal reported with its own errno");
            assert_eq!(
                std::fs::read(data.join("r.bin")).unwrap(),
                b"abc",
                "{backend}: nothing was written"
            );
            assert_eq!(
                std::fs::read(data.join("w.bin")).unwrap(),
                b"abc",
                "{backend}: nothing was cut"
            );
        },
    );
}

#[test]
fn a_file_created_by_an_open_has_the_default_mode_masked_by_the_umask() {
    // `docs/file-writes.md` section 3: `0666 & ~umask`, which is more permissive than `fs_write`'s `0644`
    // and the reason a `mode` argument is on the list. Read from outside, by `stat`.
    use std::os::unix::fs::PermissionsExt;
    for backend in BACKENDS {
        let dir = scratch(&format!("file-writes-mode-{backend}"));
        let data = dir.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let source = program(
            &data,
            &with_file(
                "open_append",
                &data.join("m.bin"),
                "match file_write(h, \"x\") { Done::Ok(n) => { code = n; } Done::Failed(e) => { code = 90 + e; } }",
            ),
        );
        let exe = build_program(&dir, "mode", &source, backend);
        let run = Command::new("sh")
            .arg("-c")
            .arg(format!("umask 027; exec {}", exe.display()))
            .output()
            .expect("a shell");
        assert_eq!(run.status.code(), Some(1), "{backend}");
        let mode = std::fs::metadata(data.join("m.bin")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640, "{backend}: 0666 with umask 027");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn sync_calls_fsync_on_the_descriptor_it_was_given() {
    // "Sync returned `Ok`" proves nothing about whether it called anything, so where `strace` is installed
    // this reads the trace: one `fsync`, on the descriptor `open_append` returned, answering 0. A mutant
    // that returned `Ok` without calling it fails here.
    if Command::new("strace").arg("-V").output().is_err() {
        eprintln!("strace is not installed; the fsync trace is not checked");
        return;
    }
    for backend in BACKENDS {
        let dir = scratch(&format!("file-writes-sync-{backend}"));
        let data = dir.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let source = program(
            &data,
            &with_file(
                "open_append",
                &data.join("s.bin"),
                "match file_write(h, \"x\") { Done::Ok(n) => { code = n; } Done::Failed(e) => { code = 90 + e; } }\n\
                 match file_sync(h) { Done::Ok(n) => { code = code + n; } Done::Failed(e) => { code = 90 + e; } }",
            ),
        );
        let exe = build_program(&dir, "sync", &source, backend);
        let trace = dir.join("trace.txt");
        let run = Command::new("strace")
            .args(["-f", "-e", "trace=openat,fsync,write", "-o"])
            .arg(&trace)
            .arg(&exe)
            .output()
            .expect("strace runs");
        assert_eq!(run.status.code(), Some(1), "{backend}: one byte written, sync answered 0");
        let trace = std::fs::read_to_string(&trace).expect("a trace");
        let fsyncs: Vec<&str> = trace.lines().filter(|l| l.contains("fsync(")).collect();
        assert_eq!(fsyncs.len(), 1, "{backend}: exactly one fsync, in:\n{trace}");
        assert!(fsyncs[0].ends_with("= 0"), "{backend}: and it succeeded: {}", fsyncs[0]);
        // The fsync is on the descriptor the file was opened on: the one the `write` of `x` used.
        let written_fd = trace
            .lines()
            .find(|l| l.contains("write(") && l.contains("\"x\""))
            .and_then(|l| l.split('(').nth(1))
            .and_then(|l| l.split(',').next())
            .unwrap_or_else(|| panic!("{backend}: no write of x in:\n{trace}"));
        assert!(
            fsyncs[0].contains(&format!("fsync({written_fd})")),
            "{backend}: fsync should name fd {written_fd}: {}",
            fsyncs[0]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn an_open_outside_the_granted_prefix_traps() {
    // The same refusal `open_read` makes (`filesystem.md` section 4): the prefix is in the type, the path
    // is a runtime slice, and a path outside what was granted kills the program rather than failing softly.
    for backend in BACKENDS {
        let dir = scratch(&format!("file-writes-outside-{backend}"));
        let data = dir.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let outside = dir.join("elsewhere.bin");
        let source = program(&data, &with_file("open_append", &outside, ""));
        let exe = build_program(&dir, "outside", &source, backend);
        let run = Command::new(&exe).output().expect("the compiled program runs");
        assert_eq!(run.status.code(), None, "{backend}: killed by a signal, not an exit");
        assert!(!outside.exists(), "{backend}: nothing was created outside the prefix");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn the_authority_report_names_the_directory_and_adds_one_path_free_label() {
    // `file-handles.md` section 4's rule on the write side: a program must not look more powerful for
    // having been written better. An append-and-sync program names the directory under `fs_write` and
    // nothing under `fs_read`, and the handle's own verb adds `file_write` with no argument to widen.
    let source = program(
        Path::new("/var/lib/log"),
        &with_file(
            "open_append",
            Path::new("/var/lib/log/seg.bin"),
            "file_write(h, \"x\");\nfile_sync(h);",
        ),
    );
    let json = authority_json(&source, "file-writes-authority");
    assert!(json.contains("\"fs_write\""), "the directory is named under fs_write:\n{json}");
    assert!(json.contains("/var/lib/log"), "and it is the granted directory:\n{json}");
    assert!(
        json.contains("{ \"name\": \"file_write\", \"argument\": null, \"bounded\": true }"),
        "the verb performs a path-free `file_write`:\n{json}"
    );
    assert!(!json.contains("\"fs_read\""), "an append performs no read of the directory:\n{json}");
}

#[test]
fn an_earlier_edition_does_not_see_the_new_names() {
    // `open_new` and `file_write` are names an edition-4 file may already declare against libc, so to
    // that file they are not builtins at all.
    let dir = scratch("file-writes-edition");
    let file = dir.join("old.ls");
    std::fs::write(
        &file,
        "edition 4;\n\
         fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args, net } = split(world);\n\
             release(args); release(heap); release(ffi); release(io); release(net);\n\
             let d = narrow(fs, \"/tmp\");\n\
             borrow d as &f in { open_new(f, \"/tmp/x\"); }\n\
             release(d);\n\
             return 0;\n\
         }\n",
    )
    .unwrap();
    let out = Command::new(BIN).args(["check".as_ref(), file.as_os_str()]).output().unwrap();
    assert!(!out.status.success(), "an edition-4 file should not resolve `open_new`");
    let _ = std::fs::remove_dir_all(&dir);
}
