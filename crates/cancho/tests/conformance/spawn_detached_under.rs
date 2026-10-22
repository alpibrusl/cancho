//! `exec_spawn_detached_under` (`docs/processes.md` §4.15, #562): a worker started by a name beneath a directory chosen
//! at deployment that leads a process group of its own, so that the deadline of `capture` ends what the worker started.
//! Over real processes and directories, on every code generator.
//!
//! The driver is `tests/programs/spawn_detached_under.cho`, cancho-forge's S7 (a worker that starts a subprocess, a
//! deadline) with the worker found beneath a `Dir`. What these tests look at is outside it: the state of the grandchild
//! after the driver has ended, read with `ps` (`process_group.rs`). A killed process nothing reaps stays a zombie in a
//! container with no init, so "dead" is "gone or `Z`".

use super::process_group::{alive, ends, number, state};
use super::*;
use std::os::unix::process::ExitStatusExt;

const CONFIGS: [(&str, &[&str]); 3] = [
    ("cranelift", &["--backend", "cranelift"]),
    ("llvm", &["--backend", "llvm", "--compiler", "rust"]),
    ("compiler in cancho", &["--backend", "llvm", "--compiler", "auto"]),
];

/// The driver built for each configuration.
fn drivers() -> &'static [(&'static str, PathBuf)] {
    static BUILT: std::sync::OnceLock<Vec<(&'static str, PathBuf)>> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| {
        let dir = scratch("spawn-detached-under");
        CONFIGS
            .iter()
            .map(|(name, flags)| {
                let exe = dir.join(name.replace(' ', "-"));
                let build = Command::new(BIN)
                    .arg("build")
                    .arg(repo_root().join("tests/programs/spawn_detached_under.cho"))
                    .arg("--std")
                    .args(*flags)
                    .arg("-o")
                    .arg(&exe)
                    .output()
                    .expect("the compiler runs");
                assert!(
                    build.status.success(),
                    "`{name}` should build the driver:\n{}",
                    String::from_utf8_lossy(&build.stderr)
                );
                (*name, exe)
            })
            .collect()
    })
}

/// A worker directory holding `sh`, a link to the shell, which the driver gives a script that starts `sleep`, prints its
/// pid and waits for it. The worker is not a file written here: a script written and executed at once is `ETXTBSY` (26)
/// whenever another thread of the test process forks between the write and the close (measured: 11 of 600 spawns with four
/// threads forking), and the tests of this crate run in parallel. The pid is printed, so the grandchild is known
/// without a duration of its own.
fn workers(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    let _ = std::fs::remove_file(dir.join("sh"));
    std::os::unix::fs::symlink("/bin/sh", dir.join("sh")).expect("a link");
    dir
}

fn run(exe: &Path, dir: &Path, mode: &str, name: &str) -> std::process::Output {
    let mut driver = Command::new(exe);
    driver.arg(dir).args([mode, name]);
    // A name the rule refuses is a trap, which would cost a core dump on a runner that pipes them.
    differential::without_a_core_dump(&mut driver).output().expect("the driver runs")
}

fn lines(out: &std::process::Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned).collect()
}

/// The S7 scenario of #562: a worker found beneath a `Dir` starts a subprocess, `capture` times the worker out at 300
/// ms, and the subprocess is dead when the driver ends. No `/bin/kill` and no path in the driver.
#[test]
fn a_deadline_ends_what_a_worker_beneath_a_directory_started() {
    for (config, exe) in drivers() {
        let dir = workers(&format!("sdu-deadline-{}", config.replace(' ', "-")));
        let out = run(exe, &dir, "d", "sh");
        assert_eq!(out.status.code(), Some(0), "`{config}`: {out:?}");
        let lines = lines(&out);
        assert!(lines.contains(&"ran timed out".to_owned()), "`{config}`: {lines:?}");
        let (worker, grandchild) = (number(&lines, "worker "), number(&lines, "grandchild "));
        assert_ne!(worker, grandchild, "`{config}`: two processes");
        assert!(
            ends(grandchild, 10),
            "`{config}`: the grandchild survived: {:?}",
            state(grandchild)
        );
        assert!(!alive(worker), "`{config}`: the worker is reaped");
    }
}

/// The control: the same worker started by `exec_spawn_under` is not a group leader, so `capture` kills the worker and
/// the grandchild runs on (#562's report, measured by S7). The test above could have failed.
#[test]
fn a_worker_started_without_a_group_of_its_own_leaks_what_it_started() {
    for (config, exe) in drivers() {
        let dir = workers(&format!("sdu-control-{}", config.replace(' ', "-")));
        let out = run(exe, &dir, "u", "sh");
        assert_eq!(out.status.code(), Some(0), "`{config}`: {out:?}");
        let lines = lines(&out);
        assert!(lines.contains(&"ran timed out".to_owned()), "`{config}`: {lines:?}");
        let grandchild = number(&lines, "grandchild ");
        let running = alive(grandchild);
        // Whatever it was, do not leave it behind.
        let _ = Command::new("kill").args(["-KILL", &grandchild.to_string()]).status();
        assert!(running, "`{config}`: a plain worker's grandchild is not in a group of its own");
        assert!(ends(grandchild, 10), "`{config}`: the control's own cleanup");
    }
}

/// A name the directory does not hold is a value, as for `exec_spawn_under`; the name rule is the same one, a trap.
#[test]
fn the_name_rule_is_exec_spawn_unders() {
    for (config, exe) in drivers() {
        let dir = workers(&format!("sdu-names-{}", config.replace(' ', "-")));
        let out = run(exe, &dir, "d", "no-such-worker");
        assert_eq!(out.status.code(), Some(0), "`{config}`: {out:?}");
        assert!(lines(&out).contains(&"spawn 2".to_owned()), "`{config}`: {out:?}");
        for name in ["/bin/sh", "../sh", "sub/../sh"] {
            let out = run(exe, &dir, "d", name);
            assert!(
                out.status.signal().is_some() && out.stdout.is_empty(),
                "`{config}` {name:?} neither trapped nor was silent: {out:?}"
            );
        }
    }
}

/// The row: both labels, no path. A supervisor that holds `Exec("")` and starts its workers this way reports
/// `exec_under` and `exec_detached` and no `exec(...)`.
#[test]
fn the_authority_report_names_two_labels_and_no_path() {
    let out = Command::new(BIN)
        .arg("authority")
        .arg(repo_root().join("tests/programs/spawn_detached_under.cho"))
        .arg("--std")
        .output()
        .expect("the compiler runs");
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(report.contains("exec_under"), "{report}");
    assert!(report.contains("exec_detached"), "{report}");
    assert!(!report.contains("exec(\""), "{report}");
    assert!(!report.contains("kill"), "{report}");
}
