//! Creating a directory (`docs/directory-handles.md` §3, slice 5): `dir_mkdir` is `mkdirat`, edition 9.
//!
//! One table, both backends, the same expectations. The probe makes each `(name, mode)` it is given and
//! prints `ok` or `err <errno>`; the test then looks at what is on disk. The probe runs under `umask 022`
//! so the modes are the ones a program sees on an ordinary machine.

use super::*;
use std::os::unix::fs::PermissionsExt;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// `argv[1]` is the directory; `argv[2..]` are pairs of a name and a mode in decimal (a leading `-` is
/// negative). Prints `ok` or `err <errno>` for each, a line apiece.
const PROBE: &str = r#"edition 9;

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

fn number[&t](text: &t [byte]) -> [] int {
    var at = 0;
    var sign = 1;
    if len(text) > 0 && int_of(text[0]) == 45 {
        sign = 0 - 1;
        at = 1;
    }
    var n = 0;
    while at < len(text) {
        n = n * 10 + (int_of(text[at]) - 48);
        at = at + 1;
    }
    return sign * n;
}

fn make_all[&i, &d, &a](io: &!i Io, dir: &d Dir, args: &a Args) -> [io_write, dir_write, args] int {
    var k = 2;
    while k + 1 < arg_count(args) {
        answer(io, dir_mkdir(dir, arg(args, k), number(arg(args, k + 1))));
        k = k + 2;
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec, tty } = split(world);
    release(ffi); release(heap); release(net); release(clock); release(signals); release(exec); release(tty);
    var status = 0;
    borrow args as &a in {
        borrow fs as &f in {
            borrow mut io as &!o in {
                match open_dir(f, arg(a, 1)) {
                    DirOpened::Ok(d0) => {
                        var d = d0;
                        borrow d as &r in {
                            status = make_all(o, r, a);
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
const EEXIST: i32 = 17;

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

/// The answers the probe printed, one per `(name, mode)`, run under `umask 022`.
fn mkdir(exe: &Path, root: &Path, cases: &[(&str, i64)]) -> Vec<String> {
    let mut command = Command::new("sh");
    command.args(["-c", "umask 022; exec \"$0\" \"$@\"", exe.to_str().expect("a path")]);
    command.arg(root);
    for (name, mode) in cases {
        command.arg(name).arg(mode.to_string());
    }
    let out = command.output().expect("the probe runs");
    assert!(out.status.code() == Some(0), "the probe failed: {:?}", out.status);
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned).collect()
}

fn bits(path: &Path) -> u32 {
    std::fs::symlink_metadata(path).expect("exists").permissions().mode() & 0o7777
}

#[test]
fn a_directory_is_made_with_its_mode_under_the_umask_on_both_backends() {
    for backend in BACKENDS {
        let dir = scratch(&format!("directory-mkdir-{backend}"));
        let root = dir.join("root");
        std::fs::create_dir_all(&root).expect("a writable scratch directory");
        let exe = build(&dir, backend);
        let err = |e: i32| format!("err {e}");

        // The mode is the program's, less the umask: 0o777 under 0o022 is 0o755.
        assert_eq!(
            mkdir(&exe, &root, &[("a755", 0o755), ("a700", 0o700), ("a777", 0o777), ("a000", 0)]),
            ["ok", "ok", "ok", "ok"],
            "{backend}"
        );
        assert_eq!(
            [
                bits(&root.join("a755")),
                bits(&root.join("a700")),
                bits(&root.join("a777")),
                bits(&root.join("a000"))
            ],
            [0o755, 0o700, 0o755, 0],
            "{backend}"
        );
        assert!(root.join("a700").is_dir(), "{backend}");

        // A name that is taken is EEXIST, whatever holds it, and what held it is untouched.
        std::fs::write(root.join("file"), "KEEP").expect("a file");
        std::fs::create_dir(root.join("dir")).expect("a directory");
        std::os::unix::fs::symlink("elsewhere", root.join("dangling")).expect("a symlink");
        std::os::unix::fs::symlink("dir", root.join("link")).expect("a symlink");
        assert_eq!(
            mkdir(
                &exe,
                &root,
                &[("file", 0o755), ("dir", 0o755), ("dangling", 0o755), ("link", 0o755)]
            ),
            [err(EEXIST), err(EEXIST), err(EEXIST), err(EEXIST)],
            "{backend}"
        );
        assert_eq!(read_or_absent(&root.join("file")), "KEEP", "{backend}");
        assert!(!root.join("elsewhere").exists(), "{backend}: a dangling link was followed");

        // Not one component, or a mode that is not permission bits: EINVAL, and no call was made.
        assert_eq!(
            mkdir(
                &exe,
                &root,
                &[
                    ("..", 0o755),
                    (".", 0o755),
                    ("", 0o755),
                    ("a/b", 0o755),
                    ("big", 0o1000),
                    ("suid", 0o4755),
                    ("sticky", 0o1777),
                    ("negative", -1)
                ]
            ),
            vec![err(EINVAL); 8],
            "{backend}"
        );
        for name in ["big", "suid", "sticky", "negative", "a"] {
            assert!(!root.join(name).exists(), "{backend}: `{name}` was made");
        }
    }
}

fn read_or_absent(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| "<absent>".to_owned())
}
