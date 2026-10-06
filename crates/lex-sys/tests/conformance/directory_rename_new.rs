//! The rename that never replaces (`docs/directory-handles.md` §3, slice 4): `dir_rename_new` is
//! `renameat2(RENAME_NOREPLACE)` on Linux and `renameatx_np(RENAME_EXCL)` on Darwin, edition 7.
//!
//! Two checks, both backends, the same expectations. The first is a table: a free name, a name that
//! holds a file, a link (dangling too), a directory, a missing source and names that are not one
//! component, each followed by what is on disk. The second is a race: a thread of this test creates the
//! destination, exclusively, at the moment the program renames onto it, and one of the two must lose.

use super::*;
use std::os::unix::fs::MetadataExt;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// `argv[1]` is the directory and `argv[2..]` are pairs of names, each renamed with `dir_rename_new` in
/// order. Prints `ok` or `err <errno>` for each, a line apiece.
const PROBE: &str = r#"edition 7;

fn digits[&i](io: &!i Io, n: int) -> [io_write] int {
    if n >= 10 {
        digits(io, n / 10);
    }
    putchar(io, '0' + n % 10);
    return 0;
}

fn answer[&i](io: &!i Io, done: Done) -> [io_write] int {
    match done {
        Done::Ok(n) => {
            write_bytes(io, "ok\n");
        }
        Done::Failed(e) => {
            write_bytes(io, "err ");
            digits(io, e);
            putchar(io, 10);
        }
    }
    return 0;
}

fn rename_all[&i, &d, &a](io: &!i Io, dir: &d Dir, args: &a Args) -> [io_write, dir_write, args] int {
    var k = 2;
    while k + 1 < arg_count(args) {
        answer(io, dir_rename_new(dir, arg(args, k), arg(args, k + 1)));
        k = k + 2;
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi); release(heap); release(net); release(clock); release(signals); release(exec);
    var status = 0;
    borrow args as &a in {
        borrow fs as &f in {
            borrow mut io as &!o in {
                match open_dir(f, arg(a, 1)) {
                    DirOpened::Ok(d0) => {
                        var d = d0;
                        borrow d as &r in {
                            status = rename_all(o, r, a);
                        }
                        dir_close(d);
                    }
                    DirOpened::Failed(e) => {
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

/// The answers the probe printed, one per pair of names.
fn rename_new(exe: &Path, root: &Path, pairs: &[(&str, &str)]) -> Vec<String> {
    let mut command = Command::new(exe);
    command.arg(root);
    for (from, to) in pairs {
        command.arg(from).arg(to);
    }
    let out = command.output().expect("the probe runs");
    assert!(out.status.code() == Some(0), "the probe failed: {:?}", out.status);
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned).collect()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| "<absent>".to_owned())
}

#[test]
fn a_rename_never_replaces_a_name_on_both_backends() {
    for backend in BACKENDS {
        let dir = scratch(&format!("directory-rename-new-{backend}"));
        let root = dir.join("root");
        std::fs::create_dir_all(root.join("subdir")).expect("a writable scratch directory");
        let outside = dir.join("outside");
        std::fs::create_dir_all(&outside).expect("a writable scratch directory");
        std::fs::write(outside.join("target.txt"), "OUTSIDE")
            .expect("a writable scratch directory");
        let exe = build(&dir, backend);
        let err = |e: i32| format!("err {e}");
        let make =
            |name: &str, bytes: &str| std::fs::write(root.join(name), bytes).expect("a file");

        // A free destination: the source moves, byte for byte, and the inode is the same file's.
        make("a", "AAAA");
        let inode = std::fs::metadata(root.join("a")).expect("exists").ino();
        assert_eq!(rename_new(&exe, &root, &[("a", "free")]), ["ok"], "{backend}");
        assert_eq!(
            (read(&root.join("a")), read(&root.join("free"))),
            ("<absent>".into(), "AAAA".into())
        );
        assert_eq!(std::fs::metadata(root.join("free")).expect("exists").ino(), inode, "{backend}");

        // An existing destination: refused with EEXIST, its bytes and the source's untouched.
        make("src", "NEW!");
        make("held", "KEEP");
        let held = std::fs::metadata(root.join("held")).expect("exists").ino();
        assert_eq!(rename_new(&exe, &root, &[("src", "held")]), [err(EEXIST)], "{backend}");
        assert_eq!(
            (read(&root.join("src")), read(&root.join("held"))),
            ("NEW!".into(), "KEEP".into())
        );
        assert_eq!(std::fs::metadata(root.join("held")).expect("exists").ino(), held, "{backend}");

        // A destination that is a link, dangling or not, is a name that is taken: the link stays and
        // nothing is written through it.
        std::os::unix::fs::symlink("../outside/target.txt", root.join("link")).expect("a symlink");
        std::os::unix::fs::symlink("../outside/absent.txt", root.join("dangling"))
            .expect("a symlink");
        assert_eq!(
            rename_new(&exe, &root, &[("src", "link"), ("src", "dangling")]),
            [err(EEXIST), err(EEXIST)],
            "{backend}"
        );
        assert_eq!(read(&root.join("src")), "NEW!", "{backend}");
        assert_eq!(read(&outside.join("target.txt")), "OUTSIDE", "{backend}");
        assert!(
            std::fs::symlink_metadata(root.join("link")).expect("the link stayed").is_symlink()
        );
        assert!(std::fs::symlink_metadata(root.join("dangling")).expect("stayed").is_symlink());
        assert_eq!(read(&outside.join("absent.txt")), "<absent>", "{backend}");

        // A destination that is a directory (empty, which a plain rename of a directory would replace).
        std::fs::create_dir(root.join("emptydir")).expect("a directory");
        std::fs::create_dir(root.join("srcdir")).expect("a directory");
        assert_eq!(
            rename_new(
                &exe,
                &root,
                &[("src", "subdir"), ("srcdir", "emptydir"), ("srcdir", "held")]
            ),
            [err(EEXIST), err(EEXIST), err(EEXIST)],
            "{backend}"
        );
        assert!(root.join("srcdir").is_dir() && root.join("emptydir").is_dir(), "{backend}");

        // A directory onto a free name moves; a missing source is ENOENT, even onto a taken name.
        assert_eq!(rename_new(&exe, &root, &[("srcdir", "movedir")]), ["ok"], "{backend}");
        assert!(root.join("movedir").is_dir() && !root.join("srcdir").exists(), "{backend}");
        assert_eq!(
            rename_new(&exe, &root, &[("missing", "held"), ("missing", "nowhere")]),
            [err(ENOENT), err(ENOENT)],
            "{backend}"
        );
        assert_eq!(read(&root.join("held")), "KEEP", "{backend}");

        // Names that are not one component are refused before any call, as `dir_rename` refuses them:
        // `EINVAL`, which is therefore never what a filesystem without the flag answers.
        let refused = rename_new(
            &exe,
            &root,
            &[("src", "../escaped"), ("..", "x"), ("src", ""), ("src", "a/b"), ("src", ".")],
        );
        assert_eq!(refused, vec![err(EINVAL); 5], "{backend}");
        assert_eq!(read(&dir.join("escaped")), "<absent>", "{backend}");
        assert_eq!(read(&root.join("src")), "NEW!", "{backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Trials of the race; each is a fresh pair of names.
const TRIALS: usize = 300;

/// The program renames `s<i>` to `d<i>` for each `i`, while this test, in a thread, creates `d<i>`
/// exclusively with `RIVAL`, with a head start of a few trials on two trials in three. Exactly one of the two may
/// win a name: if the rename answered `ok`, the rival's create must have been refused and the file
/// holds the source's bytes; if the rename answered `EEXIST`, the rival's create succeeded, the file
/// holds the rival's bytes and the source is still there. A rename that replaced would answer `ok`
/// *and* leave the rival believing it had created the name, which is the loss this slice exists to
/// end (measured at 0.28% for a stat followed by a rename, and in every trial when the rename is
/// delayed).
#[test]
fn a_rival_creating_the_destination_loses_or_wins_but_is_never_overwritten() {
    for backend in BACKENDS {
        let dir = scratch(&format!("directory-rename-new-race-{backend}"));
        let root = dir.join("root");
        std::fs::create_dir_all(&root).expect("a writable scratch directory");
        for i in 0..TRIALS {
            std::fs::write(root.join(format!("s{i}")), "SOURCE").expect("a source");
        }
        let exe = build(&dir, backend);

        let rival_root = root.clone();
        let rival = std::thread::spawn(move || {
            use std::io::Write;
            let mut created = Vec::with_capacity(TRIALS);
            let give_up = std::time::Instant::now() + std::time::Duration::from_secs(10);
            for i in 0..TRIALS {
                // Every third trial is a control the rival leaves alone, so `s<i>` always goes and the
                // program's progress can be read from the directory. The others are created after
                // waiting for the control four or eight trials back: a head start of a few trials.
                if i % 3 == 0 {
                    created.push(false);
                    continue;
                }
                let behind = i.saturating_sub(if i % 3 == 1 { 4 } else { 8 });
                let behind = behind - behind % 3;
                let marker = rival_root.join(format!("s{behind}"));
                // A program that stopped renaming would leave the control for ever; the test then
                // fails on the answers rather than hanging (one deadline for the whole run).
                while marker.exists() && std::time::Instant::now() < give_up {
                    std::hint::spin_loop();
                }
                let made = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(rival_root.join(format!("d{i}")))
                    .and_then(|mut f| f.write_all(b"RIVAL"));
                created.push(made.is_ok());
            }
            created
        });

        let names: Vec<(String, String)> =
            (0..TRIALS).map(|i| (format!("s{i}"), format!("d{i}"))).collect();
        let pairs: Vec<(&str, &str)> =
            names.iter().map(|(s, d)| (s.as_str(), d.as_str())).collect();
        let answers = rename_new(&exe, &root, &pairs);
        let created = rival.join().expect("the rival ran");
        assert_eq!(answers.len(), TRIALS, "{backend}");

        let (mut program_won, mut rival_won) = (0, 0);
        for i in 0..TRIALS {
            let held = read(&root.join(format!("d{i}")));
            let source = read(&root.join(format!("s{i}")));
            match (answers[i].as_str(), created[i]) {
                ("ok", false) => {
                    program_won += 1;
                    assert_eq!(
                        (held.as_str(), source.as_str()),
                        ("SOURCE", "<absent>"),
                        "{backend} {i}"
                    );
                }
                ("err 17", true) => {
                    rival_won += 1;
                    assert_eq!(
                        (held.as_str(), source.as_str()),
                        ("RIVAL", "SOURCE"),
                        "{backend} {i}"
                    );
                }
                (said, made) => panic!(
                    "{backend} trial {i}: the rename said `{said}` and the rival's create {}; \
                     `d{i}` holds {held:?}, `s{i}` holds {source:?}",
                    if made {
                        "succeeded: the rival's file was replaced or the program was refused wrongly"
                    } else {
                        "failed"
                    }
                ),
            }
        }
        eprintln!("{backend}: {program_won} trials won by the program, {rival_won} by the rival");
        // Both sides must have won some, or the rival never contested the name and the test proves
        // nothing: the head start gives the rival most of the contested trials.
        assert!(rival_won > 0, "{backend}: the rival never created a name first");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// What a filesystem that cannot refuse to replace answers for a free destination: `EOPNOTSUPP` on
/// Linux, `ENOTSUP` on Darwin. Never `EINVAL`, which is what the kernel says and what a bad name gets.
const UNSUPPORTED: i32 = if cfg!(target_os = "macos") { 45 } else { 95 };

/// A directory on a filesystem without the flag (NFS v3 and ntfs-3g answer `EINVAL` for it on Linux; see
/// the table in `docs/directory-handles.md` §3, slice 4) is named by `LEX_SYS_RENAME_UNSUPPORTED_DIR`.
/// CI's filesystems all have it, so without the variable this test has nothing to run on and says so.
/// The program must answer `UNSUPPORTED`, leave both names as they were, and, for a taken name, still
/// answer `EEXIST` (the kernel checks that before it looks at the flag).
#[test]
fn a_filesystem_without_the_flag_is_refused_and_never_replaced_on_both_backends() {
    let Some(base) = std::env::var("LEX_SYS_RENAME_UNSUPPORTED_DIR").ok().filter(|v| !v.is_empty())
    else {
        eprintln!(
            "LEX_SYS_RENAME_UNSUPPORTED_DIR is not set: no filesystem without RENAME_NOREPLACE here"
        );
        return;
    };
    for backend in BACKENDS {
        let dir = scratch(&format!("directory-rename-new-unsupported-{backend}"));
        let root = Path::new(&base).join(format!("rename-new-{backend}-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("a writable directory on the filesystem under test");
        std::fs::write(root.join("src"), "NEW!").expect("a file");
        std::fs::write(root.join("held"), "KEEP").expect("a file");
        let exe = build(&dir, backend);
        let answers = rename_new(&exe, &root, &[("src", "free"), ("src", "held")]);
        assert_eq!(answers, [format!("err {UNSUPPORTED}"), format!("err {EEXIST}")], "{backend}");
        assert_eq!(
            (read(&root.join("src")), read(&root.join("free"))),
            ("NEW!".into(), "<absent>".into())
        );
        assert_eq!(read(&root.join("held")), "KEEP", "{backend}");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
