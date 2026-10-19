//! `exec_spawn_under` (`docs/processes.md` §4.14, #534): starting a program by a name beneath a directory that was
//! chosen when the program was deployed, over real processes and directories, on every code generator.
//!
//! The driver is `tests/programs/spawn_under.cho`, the shape cancho-forge asked for: a supervisor given its worker
//! directory on the command line (`argv[1]`) that opens it once and starts workers under it. Every behaviour is held
//! against the real thing: a worker is a shell script that leaves a marker in its working directory and exits with a
//! code of its own, so "it ran", "it ran in the directory it was found in" and "it was *that* program" are all read
//! off the file system and the exit status, and a name the rule refuses must kill the driver (a signal, no output).

use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::ExitStatusExt;

const CONFIGS: [(&str, &[&str]); 3] = [
    ("cranelift", &["--backend", "cranelift"]),
    ("llvm", &["--backend", "llvm", "--compiler", "rust"]),
    ("compiler in cancho", &["--backend", "llvm", "--compiler", "auto"]),
];

/// `program` built for each configuration into the scratch directory `tag`.
fn drivers(tag: &str, program: &str) -> Vec<(&'static str, PathBuf)> {
    let dir = scratch(tag);
    CONFIGS
        .iter()
        .map(|(name, flags)| {
            let exe = dir.join(name.replace(' ', "-"));
            let build = Command::new(BIN)
                .arg("build")
                .arg(repo_root().join("tests/programs").join(program))
                .arg("--std")
                .args(*flags)
                .arg("-o")
                .arg(&exe)
                .output()
                .expect("the compiler runs");
            assert!(
                build.status.success(),
                "`{name}` should build `{program}`:\n{}",
                String::from_utf8_lossy(&build.stderr)
            );
            (*name, exe)
        })
        .collect()
}

/// A worker directory: `worker.sh` and `sub/worker.sh` that write `ran.txt` beside themselves and exit with `code`.
fn workers(tag: &str, code: i32) -> PathBuf {
    let dir = scratch(tag);
    std::fs::create_dir_all(dir.join("sub")).expect("a writable scratch directory");
    for script in ["worker.sh", "sub/worker.sh"] {
        let path = dir.join(script);
        std::fs::write(&path, format!("#!/bin/sh\npwd > ran.txt\nexit {code}\n"))
            .expect("a script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    dir
}

/// What the driver did: `Some(stdout)` when it returned, `None` when the trap killed it (a signal and nothing printed).
fn run(exe: &Path, dir: &Path, mode: &str, name: &str) -> Option<String> {
    let out = Command::new(exe).arg(dir).args([mode, name]).output().expect("the driver runs");
    if out.status.code() == Some(0) {
        return Some(String::from_utf8_lossy(&out.stdout).trim().to_owned());
    }
    assert!(
        out.status.signal().is_some() && out.stdout.is_empty(),
        "{exe:?} {mode} {name:?} neither returned nor trapped: {out:?}"
    );
    None
}

/// Where the worker ran: what it wrote in `ran.txt`, which is removed first.
fn ran(dir: &Path, sub: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(sub).join("ran.txt")).ok().map(|s| s.trim().to_owned())
}

/// A name under the directory starts that program, with that directory as its working directory.
#[test]
fn a_name_beneath_the_directory_starts_that_program_there() {
    for (config, exe) in drivers("spawn-under-runs", "spawn_under.cho") {
        let dir = workers(&format!("spawn-under-runs-{}", config.replace(' ', "-")), 7);
        // The working directory is the directory itself, whatever the name reaches: a worker found in `sub` still
        // starts in `dir`, so each leaves its marker there and not beside itself or in the driver's directory.
        for name in ["worker.sh", "sub/worker.sh", "./worker.sh"] {
            let _ = std::fs::remove_file(dir.join("ran.txt"));
            assert_eq!(
                run(&exe, &dir, "u", name).as_deref(),
                Some("spawned code 7"),
                "`{config}` {name:?}"
            );
            let real = std::fs::canonicalize(&dir).expect("the directory exists");
            assert_eq!(
                ran(&dir, "").map(PathBuf::from),
                Some(real),
                "`{config}` {name:?} ran elsewhere"
            );
        }
        // A name the directory does not hold is a value, as a missing program is for `exec_spawn`.
        assert_eq!(
            run(&exe, &dir, "u", "no-such-worker").as_deref(),
            Some("failed errno 2"),
            "`{config}`"
        );
        // A directory is not a program: the kernel's own refusal, `EACCES`, a value.
        assert_eq!(run(&exe, &dir, "u", "sub").as_deref(), Some("failed errno 13"), "`{config}`");
    }
}

/// The bound is the directory the program was given: the same binary and the same name, two directories, two
/// programs. This is the whole of the issue, a bound that is not in the program text.
#[test]
fn the_same_binary_with_another_directory_starts_another_program() {
    for (config, exe) in drivers("spawn-under-two", "spawn_under.cho") {
        let tag = config.replace(' ', "-");
        let (a, b) = (
            workers(&format!("spawn-under-a-{tag}"), 7),
            workers(&format!("spawn-under-b-{tag}"), 9),
        );
        assert_eq!(
            run(&exe, &a, "u", "worker.sh").as_deref(),
            Some("spawned code 7"),
            "`{config}`"
        );
        assert_eq!(
            run(&exe, &b, "u", "worker.sh").as_deref(),
            Some("spawned code 9"),
            "`{config}`"
        );
        assert!(
            ran(&a, "").is_some() && ran(&b, "").is_some(),
            "`{config}`: each ran in its own directory"
        );
    }
}

/// What the rule refuses is a trap (§4.1's convention: a path outside the bound is a program doing what its type said
/// it would not): an empty name, a rooted one, `..` anywhere (the byte pair, as `exec_covers` has it), a name of 4096
/// bytes. Each rooted or empty one is held against the **control**, `exec_spawn_in` under the same `Exec("")` and the
/// same directory, which takes it: the trap is the new builtin's and not the capability's.
#[test]
fn a_name_that_is_not_beneath_the_directory_traps() {
    let scripts = scratch("spawn-under-outside");
    let outside = scripts.join("outside.sh");
    std::fs::write(&outside, "#!/bin/sh\nexit 5\n").expect("a script");
    std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let outside = outside.to_string_lossy().into_owned();
    let long = "a".repeat(4096);
    let control = drivers("spawn-under-control", "spawn_in_control.cho");
    for (config, exe) in drivers("spawn-under-traps", "spawn_under.cho") {
        let dir = workers(&format!("spawn-under-traps-{}", config.replace(' ', "-")), 7);
        let (_, control) =
            control.iter().find(|(c, _)| *c == config).expect("a control per configuration");
        for (name, control_says) in [
            ("/bin/true", "spawned code 0"),
            (outside.as_str(), "spawned code 5"),
            ("/", "failed errno 13"),
            ("", "failed errno 2"),
        ] {
            assert_eq!(run(&exe, &dir, "u", name), None, "`{config}` {name:?} should trap");
            let out = Command::new(control).arg(&dir).arg(name).output().expect("the control runs");
            assert_eq!(
                String::from_utf8_lossy(&out.stdout).trim(),
                control_says,
                "`{config}` {name:?}: the control, `exec_spawn_in` under `Exec(\"\")`, takes it: {out:?}"
            );
        }
        for name in
            ["..", "../worker.sh", "sub/../worker.sh", "worker.sh/..", "a..b", long.as_str()]
        {
            assert_eq!(run(&exe, &dir, "u", name), None, "`{config}` {name:?} should trap");
        }
        // The longest name the spawn copies is 4095 bytes: at that length it reaches the kernel, which answers a value.
        let longest = "a".repeat(4095);
        assert_eq!(
            run(&exe, &dir, "u", &longest).as_deref(),
            Some("failed errno 36"),
            "`{config}`"
        );
    }
}

/// The supervisor of the issue: it asks first (`exec_covers`, and a name that is not empty or rooted), so a refused name
/// is a line in its journal and not the end of the supervisor. Held against the trap: every name the supervisor
/// refuses is a name `u` traps on, and every name it takes is one `u` returns from.
#[test]
fn a_supervisor_that_asks_first_survives() {
    for (config, exe) in drivers("spawn-under-supervisor", "spawn_under.cho") {
        let dir = workers(&format!("spawn-under-sup-{}", config.replace(' ', "-")), 7);
        for name in [
            "/bin/true",
            "",
            "../worker.sh",
            "a..b",
            "sub/../worker.sh",
            "worker.sh",
            "sub/worker.sh",
            "no-such-worker",
        ] {
            let asked = run(&exe, &dir, "s", name)
                .unwrap_or_else(|| panic!("`{config}` {name:?}: the supervisor died"));
            let trapped = run(&exe, &dir, "u", name).is_none();
            if trapped {
                assert_eq!(asked, "refused\njournaled", "`{config}` {name:?}");
            } else {
                assert!(
                    asked.ends_with("\njournaled") && !asked.starts_with("refused"),
                    "`{config}` {name:?}: {asked}"
                );
            }
        }
    }
}

/// `Dir` resolves the directory once, as a descriptor, so a link the kernel follows is followed: a script that is a
/// link to a program outside the directory runs. The textual rule is the whole of the compiler's check (§4.14, as
/// §4.13 says of `Exec` and `Fs`), and this pins the limit rather than leaving it unsaid.
#[test]
fn a_link_in_the_directory_is_the_kernels_to_follow() {
    let scripts = scratch("spawn-under-link-target");
    let target = scripts.join("target.sh");
    std::fs::write(&target, "#!/bin/sh\nexit 5\n").expect("a script");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    for (config, exe) in drivers("spawn-under-link", "spawn_under.cho") {
        let dir = workers(&format!("spawn-under-link-{}", config.replace(' ', "-")), 7);
        let _ = std::fs::remove_file(dir.join("link.sh"));
        symlink(&target, dir.join("link.sh")).expect("a link");
        assert_eq!(
            run(&exe, &dir, "u", "link.sh").as_deref(),
            Some("spawned code 5"),
            "`{config}`"
        );
    }
}

/// A directory that is not there is not a program's to start things under: the driver says so before any spawn.
#[test]
fn a_directory_that_is_not_there_starts_nothing() {
    for (config, exe) in drivers("spawn-under-missing", "spawn_under.cho") {
        let out = Command::new(&exe)
            .arg("/no/such/worker/directory")
            .args(["u", "worker.sh"])
            .output()
            .expect("runs");
        assert_eq!(out.status.code(), Some(3), "`{config}`: {out:?}");
        assert!(out.stdout.is_empty(), "`{config}`");
    }
}

/// §4.14: the report. A supervisor that spawns only beneath a directory reports `exec_under` and `fs_read("")` (the
/// open) and **no `exec`**; the control that spawns by path under the same `Exec("")` reports `exec("")`. The row is
/// narrower than the capability it spends, and says what it is: programs a directory chosen at run time holds.
#[test]
fn the_report_names_exec_under_and_not_exec() {
    let report = |program: &str| {
        let out = Command::new(BIN)
            .arg("authority")
            .arg("--std")
            .arg("--output")
            .arg("json")
            .arg(repo_root().join("tests/programs").join(program))
            .output()
            .expect("the compiler runs");
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let under = report("spawn_under.cho");
    assert!(
        under.contains(r#""effects": ["args", "exec_under", "fs_read", "io_write"]"#),
        "{under}"
    );
    assert!(under.contains(r#"{ "name": "fs_read", "argument": "", "bounded": true }"#), "{under}");
    assert!(!under.contains(r#""name": "exec","#), "{under}");
    let control = report("spawn_in_control.cho");
    assert!(
        control.contains(r#"{ "name": "exec", "argument": "", "bounded": true }"#),
        "{control}"
    );
    assert!(!control.contains("exec_under"), "{control}");
}
