//! Permission bits beneath a directory (`docs/directory-listing.md` §3.5, issue #243): `dir_mode` on one
//! component and `dir_own_mode` on the opened directory itself, both edition 7.
//!
//! The probe prints the directory's own bits, then each name's, and the test compares them with what Rust's
//! `symlink_metadata` sees (`mode() & 0o7777`). The names cover the nine read/write/execute bits, set-user-id
//! and the sticky bit, a directory, a symbolic link (whose own bits, never its target's), a missing name and
//! three that are not one component. Both backends, the same expectations.

use super::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// The names the probe asks for, in order; the last three are refused with `EINVAL` and no call.
const NAMES: [&str; 11] =
    ["f600", "f644", "f400", "f4755", "f000", "d0750", "d1777", "link", "missing", "..", "sub/x"];

/// `argv[1]` is the directory. Prints `own <bits>` or `own err <errno>`, then for each name `<name> <bits>`
/// or `<name> err <errno>`, bits in octal.
const PROBE: &str = r#"edition 7;

fn octal[&i](io: &!i Io, n: int) -> [io_write] int {
    if n >= 8 {
        octal(io, n / 8);
    }
    putchar(io, '0' + n % 8);
    return 0;
}

fn decimal[&i](io: &!i Io, n: int) -> [io_write] int {
    if n >= 10 {
        decimal(io, n / 10);
    }
    putchar(io, '0' + n % 10);
    return 0;
}

fn show[&i, &n](io: &!i Io, name: &n [byte], answer: Done) -> [io_write] int {
    write_bytes(io, name);
    putchar(io, ' ');
    match answer {
        Done::Ok(bits) => {
            octal(io, bits);
        }
        Done::Failed(e) => {
            write_bytes(io, "err ");
            decimal(io, e);
        }
    }
    putchar(io, 10);
    return 0;
}

fn modes[&i, &d](io: &!i Io, dir: &d Dir) -> [io_write, dir_read] int {
    show(io, "own", dir_own_mode(dir));
    show(io, "f600", dir_mode(dir, "f600"));
    show(io, "f644", dir_mode(dir, "f644"));
    show(io, "f400", dir_mode(dir, "f400"));
    show(io, "f4755", dir_mode(dir, "f4755"));
    show(io, "f000", dir_mode(dir, "f000"));
    show(io, "d0750", dir_mode(dir, "d0750"));
    show(io, "d1777", dir_mode(dir, "d1777"));
    show(io, "link", dir_mode(dir, "link"));
    show(io, "missing", dir_mode(dir, "missing"));
    show(io, "..", dir_mode(dir, ".."));
    show(io, "sub/x", dir_mode(dir, "sub/x"));
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi); release(net); release(clock); release(signals); release(exec); release(heap);
    var status = 0;
    borrow args as &a in {
        borrow fs as &f in {
            borrow mut io as &!o in {
                match open_dir(f, arg(a, 1)) {
                    DirOpened::Ok(d0) => {
                        var d = d0;
                        borrow d as &r in {
                            status = modes(o, r);
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

/// A directory of the names above, each given its bits after it is made, so the umask does not decide them.
fn tree(scratch: &Path) -> PathBuf {
    let root = scratch.join("modes");
    std::fs::create_dir_all(&root).expect("a writable scratch directory");
    let chmod = |name: &str, bits: u32| {
        std::fs::set_permissions(root.join(name), std::fs::Permissions::from_mode(bits))
            .expect("chmod in the scratch directory");
    };
    for (name, bits) in
        [("f600", 0o600), ("f644", 0o644), ("f400", 0o400), ("f4755", 0o4755), ("f000", 0)]
    {
        std::fs::write(root.join(name), name).expect("a writable scratch directory");
        chmod(name, bits);
    }
    for (name, bits) in [("d0750", 0o750), ("d1777", 0o1777)] {
        std::fs::create_dir(root.join(name)).expect("a writable scratch directory");
        chmod(name, bits);
    }
    std::os::unix::fs::symlink("f600", root.join("link")).expect("a symbolic link");
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o710))
        .expect("chmod the root");
    root
}

/// What the probe must print: `lstat`'s bits for each name that is there, `ENOENT` for the missing one and
/// `EINVAL` for the two that are not one component.
fn expected(root: &Path) -> Vec<String> {
    let bits = |path: &Path| std::fs::symlink_metadata(path).expect("an entry").mode() & 0o7777;
    let mut out = vec![format!("own {:o}", bits(root))];
    for name in NAMES {
        let line = match name {
            "missing" => format!("{name} err 2"),
            ".." | "sub/x" => format!("{name} err 22"),
            _ => format!("{name} {:o}", bits(&root.join(name))),
        };
        out.push(line);
    }
    out
}

#[test]
fn permission_bits_beneath_a_directory_and_of_it_on_both_backends() {
    let scratch = scratch("directory-modes");
    let root = tree(&scratch);
    let want = expected(&root);
    // The bits asked for are the bits there: the set-user-id and sticky bits survived `chmod`.
    assert!(
        want.contains(&"f4755 4755".to_owned()) && want.contains(&"d1777 1777".to_owned()),
        "{want:?}"
    );
    assert_eq!(want[0], "own 710");
    for backend in BACKENDS {
        let exe = build(&scratch, backend);
        let out = Command::new(&exe).arg(&root).output().expect("the probe runs");
        assert_eq!(out.status.code(), Some(0), "`--backend {backend}`: {out:?}");
        let got: Vec<String> =
            String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned).collect();
        assert_eq!(got, want, "`--backend {backend}`");
    }
    // The root is 0710: give it back to its owner before the next run removes it.
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
        .expect("chmod the root");
}
