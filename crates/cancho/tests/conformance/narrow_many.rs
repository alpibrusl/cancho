//! `narrow(cap, "a", "b", ...)` (`docs/narrowing-into-several.md`): one capability consumed, a
//! tuple of capabilities answered, one narrowed to each literal. The refusals are fixtures under
//! `tests/reject/` (`narrow_many_*.cho`) and the accepted shapes under `tests/accept/`; this file
//! builds and runs programs on both backends, reads the authority report, and confines each child.

use super::*;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// Build `source` with `backend` into `dir` and answer the executable.
fn build_source(dir: &Path, name: &str, source: &str, backend: &str) -> PathBuf {
    let path = dir.join(format!("{name}.cho"));
    std::fs::write(&path, source).expect("a writable fixture");
    let exe = dir.join(format!("{name}-{backend}"));
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(&path)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(
        build.status.success(),
        "`{name}` should compile on {backend}:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    exe
}

/// The labels the report names, as `name=argument` (or `name`), in report order.
fn labels_of(dir: &Path, name: &str, source: &str) -> (String, Vec<String>) {
    let path = dir.join(format!("{name}.cho"));
    std::fs::write(&path, source).expect("a writable fixture");
    let (_, _, labels) = authority_of_paths(std::slice::from_ref(&path));
    // The printing of the numbers is `io_write`; what the test is about is the filesystem.
    let labels = labels.into_iter().filter(|l| l != "io_write").collect();
    let text = Command::new(BIN)
        .args(["authority", "--std"])
        .arg(&path)
        .output()
        .expect("the compiler runs");
    (String::from_utf8_lossy(&text.stdout).into_owned(), labels)
}

/// Two files with known bytes: `a.txt` four `A`, `b.txt` six `B`, `c.txt` three `C`.
fn files(dir: &Path) -> [String; 3] {
    let names = ["a.txt", "b.txt", "c.txt"];
    let bytes = ["AAAA", "BBBBBB", "CCC"];
    for (n, b) in names.iter().zip(bytes) {
        std::fs::write(dir.join(n), b).expect("a writable scratch directory");
    }
    names.map(|n| dir.join(n).to_string_lossy().into_owned())
}

const PRELUDE: &str = "fn digits[&i](io: &!i Io, n: int) -> [io_write] int {\n\
    if n >= 10 {\n        digits(io, n / 10);\n    }\n    return putchar(io, '0' + n % 10);\n}\n\n\
    fn sum[&b](bytes: &b [byte], n: int) -> [] int {\n\
    var total = 0;\n    var at = 0;\n    while at < n {\n        total = total + int_of(bytes[at]);\n        at = at + 1;\n    }\n    return total;\n}\n\n";

/// A program narrowing `Fs("")` to `paths` in one `narrow`, then reading `reads[i]` through child
/// `i` for each `(i, path)` in `reads`, and printing `<bytes read> <sum of the bytes>` for each.
fn reader(paths: &[&str], reads: &[(usize, &str)]) -> String {
    let names: Vec<String> = (0..paths.len()).map(|i| format!("c{i}")).collect();
    let literals: Vec<String> = paths.iter().map(|p| format!("\"{p}\"")).collect();
    let mut s = String::from(PRELUDE);
    s.push_str("fn main(world: World) -> [] int {\n");
    s.push_str("    let Split { io, ffi, fs, heap, args } = split(world);\n");
    s.push_str("    release(ffi); release(heap); release(args);\n");
    s.push_str(&format!("    let ({}) = narrow(fs, {});\n", names.join(", "), literals.join(", ")));
    s.push_str("    var counts = 0;\n    var sums = 0;\n    borrow mut io as &!o in {\n");
    for (child, path) in reads {
        s.push_str(&format!(
            "        region r{child} {{\n            let buffer = alloc_slice[r{child}](16, byte_of(0));\n\
             \x20           borrow c{child} as &x in {{\n                counts = fs_read(x, \"{path}\", buffer);\n            }}\n\
             \x20           sums = sum(buffer, counts);\n        }}\n\
             \x20       digits(o, counts); putchar(o, 32); digits(o, sums); putchar(o, 10);\n"
        ));
    }
    s.push_str("    }\n");
    for n in &names {
        s.push_str(&format!("    release({n});\n"));
    }
    s.push_str("    release(io);\n    return 0;\n}\n");
    s
}

fn run_ok(exe: &Path) -> String {
    let run = Command::new(exe).output().expect("the compiled program runs");
    assert!(run.status.success(), "{:?} {}", run.status, String::from_utf8_lossy(&run.stderr));
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// Killed by a signal, as the run-time prefix check does it (`docs/filesystem.md` section 4).
fn assert_traps(exe: &Path, what: &str) {
    let run = Command::new(exe).output().expect("the compiled program runs");
    assert!(!run.status.success(), "{what}: should not succeed");
    assert_eq!(run.status.code(), None, "{what}: should be killed by a signal, not exit");
}

#[test]
fn two_paths_are_read_through_two_children_on_both_backends() {
    let dir = scratch("narrow-many-two");
    let [a, b, _] = files(&dir);
    let source = reader(&[&a, &b], &[(0, &a), (1, &b)]);
    for backend in BACKENDS {
        let exe = build_source(&dir, "two", &source, backend);
        // 4 * 'A' (65) = 260 and 6 * 'B' (66) = 396.
        assert_eq!(run_ok(&exe), "4 260\n6 396\n", "{backend}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn three_paths_are_read_in_the_order_written_on_both_backends() {
    // The children come back in the order of the literals: a swapped pair would read the
    // wrong file and the run-time check would trap.
    let dir = scratch("narrow-many-three");
    let [a, b, c] = files(&dir);
    let source = reader(&[&a, &b, &c], &[(2, &c), (0, &a), (1, &b)]);
    for backend in BACKENDS {
        let exe = build_source(&dir, "three", &source, backend);
        assert_eq!(run_ok(&exe), "3 201\n4 260\n6 396\n", "{backend}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_report_names_exactly_the_paths_narrowed_to_and_read() {
    let dir = scratch("narrow-many-report");
    let [a, b, c] = files(&dir);

    let two = reader(&[&a, &b], &[(0, &a), (1, &b)]);
    let (text, labels) = labels_of(&dir, "two", &two);
    assert_eq!(labels, [format!("fs_read={a}"), format!("fs_read={b}")], "{text}");
    assert!(!text.contains("fs_read(\"\")"), "{text}");
    let json = Command::new(BIN)
        .args(["authority", "--std", "--output", "json"])
        .arg(dir.join("two.cho"))
        .output()
        .expect("the compiler runs");
    let json = String::from_utf8_lossy(&json.stdout);
    assert!(json.contains("\"bounded\": true"), "{json}");
    assert!(json.contains("\"effects\": [\"fs_read\"") || json.contains("\"fs_read\""), "{json}");

    // The report follows what is performed, not what is held: the same program with one read
    // removed lists one label.
    let one = reader(&[&a, &b], &[(1, &b)]);
    let (text, labels) = labels_of(&dir, "one", &one);
    assert_eq!(labels, [format!("fs_read={b}")], "{text}");

    let three = reader(&[&a, &b, &c], &[(0, &a), (1, &b), (2, &c)]);
    let (text, labels) = labels_of(&dir, "three", &three);
    assert_eq!(
        labels,
        [format!("fs_read={a}"), format!("fs_read={b}"), format!("fs_read={c}")],
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_read_outside_each_childs_path_traps_on_both_backends() {
    // The program reads a path its child was not narrowed to: the other file, a third file, and
    // each child's own sibling. The run-time check reads the prefix of the capability the
    // operation borrowed, so each child is checked against its own literal.
    let dir = scratch("narrow-many-traps");
    let [a, b, c] = files(&dir);
    let cases: [(&str, String); 4] = [
        ("the second child reads the first's file", reader(&[&a, &b], &[(1, &a)])),
        ("the first child reads the second's file", reader(&[&a, &b], &[(0, &b)])),
        ("a third file through the first child", reader(&[&a, &b], &[(0, &c)])),
        ("a third file through the second child", reader(&[&a, &b], &[(1, &c)])),
    ];
    for (what, source) in &cases {
        for backend in BACKENDS {
            let exe = build_source(&dir, "trap", source, backend);
            assert_traps(&exe, &format!("{what} ({backend})"));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_sibling_directory_is_not_inside_a_child_through_the_new_form() {
    // `docs/filesystem.md` section 1.1 through `narrow(fs, "/tmp/x", "/tmp/xevil")`: the two
    // are unrelated (neither extends the other at a `/`), so the pair is accepted, and a read of
    // `xevil` through the child for `x` traps, as does the reverse.
    let base = Path::new("/tmp").join(format!("cancho-narrow-many-{}", std::process::id()));
    let (x, evil) = (base.join("x"), base.join("xevil"));
    std::fs::create_dir_all(&x).expect("a writable /tmp");
    std::fs::create_dir_all(&evil).expect("a writable /tmp");
    std::fs::write(x.join("f"), "fine").unwrap();
    std::fs::write(evil.join("f"), "evil").unwrap();
    let (xs, es) = (x.to_string_lossy().into_owned(), evil.to_string_lossy().into_owned());
    let (xf, ef) = (format!("{xs}/f"), format!("{es}/f"));
    let dir = scratch("narrow-many-sibling");

    for backend in BACKENDS {
        let ok = build_source(&dir, "ok", &reader(&[&xs, &es], &[(0, &xf), (1, &ef)]), backend);
        assert_eq!(run_ok(&ok), "4 418\n4 432\n", "{backend}");
        let into_evil = build_source(&dir, "a", &reader(&[&xs, &es], &[(0, &ef)]), backend);
        assert_traps(&into_evil, &format!("the `x` child reads `xevil` ({backend})"));
        let into_x = build_source(&dir, "b", &reader(&[&xs, &es], &[(1, &xf)]), backend);
        assert_traps(&into_x, &format!("the `xevil` child reads `x` ({backend})"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&base);
}

/// The TLS server's setup: 32 bytes of entropy and the certificate files beneath a directory
/// opened once. One `narrow`, two paths: `/dev/urandom` and the directory.
fn tls_setup(certs: &str) -> String {
    format!(
        "edition 6;\n\n{PRELUDE}\
         fn main(world: World) -> [] int {{\n\
         \x20   let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);\n\
         \x20   release(ffi); release(heap); release(args); release(net); release(clock); release(signals);\n\
         \x20   let (rng, store) = narrow(fs, \"/dev/urandom\", \"{certs}\");\n\
         \x20   var seeded = 0;\n\
         \x20   var pem = 0;\n\
         \x20   region r {{\n\
         \x20       let seed = alloc_slice[r](32, byte_of(0));\n\
         \x20       let chain = alloc_slice[r](256, byte_of(0));\n\
         \x20       borrow rng as &x in {{\n\
         \x20           seeded = fs_read(x, \"/dev/urandom\", seed);\n\
         \x20       }}\n\
         \x20       borrow store as &y in {{\n\
         \x20           match open_dir(y, \"{certs}\") {{\n\
         \x20               DirOpened::Ok(d0) => {{\n\
         \x20                   var d = d0;\n\
         \x20                   borrow d as &h in {{\n\
         \x20                       match dir_open_read(h, \"chain.pem\") {{\n\
         \x20                           Opened::Ok(f0) => {{\n\
         \x20                               var f = f0;\n\
         \x20                               borrow mut f as &!fh in {{\n\
         \x20                                   match file_read(fh, chain) {{\n\
         \x20                                       Read::Got(n) => {{ pem = n; }}\n\
         \x20                                       Read::End => {{ pem = 0; }}\n\
         \x20                                       Read::Failed(e) => {{ pem = 1000 + e; }}\n\
         \x20                                   }}\n\
         \x20                               }}\n\
         \x20                               file_close(f);\n\
         \x20                           }}\n\
         \x20                           Opened::Failed(e) => {{ pem = 2000 + e; }}\n\
         \x20                       }}\n\
         \x20                   }}\n\
         \x20                   dir_close(d);\n\
         \x20               }}\n\
         \x20               DirOpened::Failed(e) => {{ pem = 3000 + e; }}\n\
         \x20           }}\n\
         \x20       }}\n\
         \x20   }}\n\
         \x20   release(rng);\n\
         \x20   release(store);\n\
         \x20   borrow mut io as &!o in {{\n\
         \x20       digits(o, seeded); putchar(o, 32); digits(o, pem); putchar(o, 10);\n\
         \x20   }}\n\
         \x20   release(io);\n\
         \x20   return 0;\n\
         }}\n"
    )
}

#[test]
fn entropy_and_a_certificate_directory_are_held_together_with_a_tight_report() {
    // `docs/narrowing-into-several.md` section 1: the program that held `Fs("")` and reported
    // `fs_read("")` now reports the two paths and the path-free directory-handle labels.
    let dir = scratch("narrow-many-tls");
    let certs = dir.join("certs");
    std::fs::create_dir_all(&certs).unwrap();
    std::fs::write(certs.join("chain.pem"), "-----BEGIN CERTIFICATE-----\n").unwrap();
    let certs = certs.to_string_lossy().into_owned();
    let source = tls_setup(&certs);

    for backend in BACKENDS {
        let exe = build_source(&dir, "setup", &source, backend);
        assert_eq!(run_ok(&exe), "32 28\n", "{backend}");
    }
    let (text, labels) = labels_of(&dir, "setup", &source);
    assert_eq!(
        labels,
        [
            "dir_read".to_string(),
            "file_read".to_string(),
            "fs_read=/dev/urandom".to_string(),
            format!("fs_read={certs}"),
        ],
        "{text}"
    );
    assert!(!text.contains("fs_read(\"\")"), "{text}");

    // The directory handle is confined to its own path: opening the entropy device's parent
    // through the directory child is a different capability's business, and traps here.
    let escape = source.replace(&format!("open_dir(y, \"{certs}\")"), "open_dir(y, \"/dev\")");
    for backend in BACKENDS {
        let exe = build_source(&dir, "escape", &escape, backend);
        assert_traps(&exe, &format!("opening `/dev` through the certificate child ({backend})"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exec_narrows_into_several_and_the_report_names_both_programs() {
    let (_, _, labels) = authority_of("tests/accept/narrow_many_exec.cho");
    assert_eq!(
        labels,
        [
            "exec=/usr/bin/false".to_string(),
            "exec=/usr/bin/true".to_string(),
            "io_write".to_string(),
        ],
        "{labels:?}"
    );
    // Starting the program that is the other child's is outside this child's prefix.
    let dir = scratch("narrow-many-exec");
    let source = std::fs::read_to_string(repo_root().join("tests/accept/narrow_many_exec.cho"))
        .unwrap()
        .replace("exec_spawn(exec, \"/usr/bin/false\"", "exec_spawn(exec, \"/usr/bin/true\"");
    for backend in BACKENDS {
        let exe = build_source(&dir, "exec", &source, backend);
        assert_traps(&exe, &format!("starting `true` through the `false` child ({backend})"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_single_literal_form_is_unchanged() {
    let dir = scratch("narrow-many-single");
    let [a, ..] = files(&dir);
    let source = reader(&[&a], &[(0, &a)]).replace("let (c0) =", "let c0 =");
    for backend in BACKENDS {
        let exe = build_source(&dir, "single", &source, backend);
        assert_eq!(run_ok(&exe), "4 260\n", "{backend}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
