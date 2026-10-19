//! Signalling the process group of a detached child (`docs/processes.md`
//! §4.12): `child_kill_group`, and `std.process.capture` using it, over real
//! processes, on both backends.
//!
//! The driver is `tests/programs/process_group.cho`; it is cancho-forge's S7
//! (a worker that starts a subprocess, a deadline) as a program of this
//! repository. What these tests look at is outside it: the state of the
//! grandchild after the driver has ended, read with `ps`. A killed process
//! that nothing reaps stays a zombie (`Z`) in a container with no init to
//! adopt it, so "dead" here is "gone or `Z`", and "alive" is any other state.

use super::*;
use std::time::{Duration, Instant};

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// The driver, built once with each backend; the tests run in parallel.
fn driver() -> &'static [(&'static str, PathBuf)] {
    static BUILT: std::sync::OnceLock<Vec<(&'static str, PathBuf)>> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| {
        let dir = scratch("process-group");
        let source = repo_root().join("tests/programs/process_group.cho");
        let mut built = Vec::new();
        for backend in BACKENDS {
            let exe = dir.join(format!("group-{backend}"));
            let build = Command::new(BIN)
                .arg("build")
                .arg(&source)
                .args(["--std", "--backend", backend, "-o"])
                .arg(&exe)
                .output()
                .expect("the compiler runs");
            assert!(
                build.status.success(),
                "`--backend {backend}` should build the driver:\n{}",
                String::from_utf8_lossy(&build.stderr)
            );
            built.push((backend, exe));
        }
        built
    })
}

/// The `ps` state of a process, `None` once it is gone.
fn state(pid: u32) -> Option<String> {
    let out =
        Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).output().expect("ps runs");
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Running or sleeping: there, and not a zombie waiting for a reaper.
fn alive(pid: u32) -> bool {
    state(pid).is_some_and(|stat| !stat.starts_with('Z'))
}

/// Not running within `seconds`: gone, or a zombie.
fn ends(pid: u32, seconds: u64) -> bool {
    let until = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < until {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Run one mode of the driver; it must end by itself with status 0.
fn run(backend: &str, exe: &Path, mode: &str) -> Vec<String> {
    let out = Command::new(exe).arg(mode).output().expect("the driver runs");
    assert_eq!(out.status.code(), Some(0), "`{backend}` mode {mode}: {out:?}");
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned).collect()
}

/// The number on the line that starts with `prefix`.
fn number(lines: &[String], prefix: &str) -> u32 {
    lines
        .iter()
        .find_map(|l| l.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("no `{prefix}` line in {lines:?}"))
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("`{prefix}` is not followed by a number in {lines:?}"))
}

/// The S7 scenario (#533): a detached worker starts a subprocess, `capture`
/// times it out at 300 ms, and the subprocess is dead when the driver ends.
#[test]
fn a_deadline_ends_what_a_detached_worker_started() {
    for (backend, exe) in driver() {
        let lines = run(backend, exe, "t");
        assert!(lines.contains(&"ran timed out".to_owned()), "`{backend}`: {lines:?}");
        let (worker, grandchild) = (number(&lines, "worker "), number(&lines, "grandchild "));
        assert_ne!(worker, grandchild, "`{backend}`: two processes");
        assert!(
            ends(grandchild, 10),
            "`{backend}`: the grandchild survived: {:?}",
            state(grandchild)
        );
        assert!(!alive(worker), "`{backend}`: the worker is reaped");
    }
}

/// The control: the same scenario with a plain `exec_spawn` leaves the
/// grandchild running (the worker is in its parent's group, §4.12), so the
/// test above could have failed, and the driver is none the worse for asking.
#[test]
fn a_plain_workers_grandchild_is_left_running() {
    for (backend, exe) in driver() {
        let lines = run(backend, exe, "p");
        assert!(lines.contains(&"ran timed out".to_owned()), "`{backend}`: {lines:?}");
        let grandchild = number(&lines, "grandchild ");
        let running = alive(grandchild);
        // Whatever it was, do not leave it behind.
        let _ = Command::new("kill").args(["-KILL", &grandchild.to_string()]).status();
        assert!(running, "`{backend}`: a plain worker's grandchild is not in a group of its own");
        assert!(ends(grandchild, 10), "`{backend}`: the control's own cleanup");
    }
}

/// §4.12: what `child_kill_group` answers, by the child it is given.
#[test]
fn the_group_signal_answers() {
    for (backend, exe) in driver() {
        // A detached child: the signal is sent, and ends it.
        let lines = run(backend, exe, "k");
        assert!(lines.contains(&"group 0".to_owned()), "`{backend}` k: {lines:?}");
        assert!(lines.contains(&"exited signaled 256".to_owned()), "`{backend}` k: {lines:?}");

        // A plain child leads no group: ESRCH (3 on both kernels), nothing is
        // sent, the program that asked is still there to say so.
        let lines = run(backend, exe, "r");
        assert!(lines.contains(&"group 3".to_owned()), "`{backend}` r: {lines:?}");
        assert!(lines.contains(&"exited signaled 256".to_owned()), "`{backend}` r: {lines:?}");

        // A signal that is none: EINVAL and no call.
        let lines = run(backend, exe, "s");
        assert!(lines.contains(&"group 22".to_owned()), "`{backend}` s: {lines:?}");
    }
}

/// §4.12: the leader of the group may have ended and not been waited for (a
/// zombie that still holds its pid, §4.7); the group is still there and the
/// signal reaches the grandchild.
#[test]
fn a_leader_that_has_ended_does_not_hide_its_group() {
    for (backend, exe) in driver() {
        let lines = run(backend, exe, "z");
        let grandchild = number(&lines, "grandchild ");
        assert!(lines.contains(&"group 0".to_owned()), "`{backend}`: {lines:?}");
        assert!(lines.contains(&"exited code 0".to_owned()), "`{backend}`: {lines:?}");
        assert!(
            ends(grandchild, 10),
            "`{backend}`: the grandchild survived: {:?}",
            state(grandchild)
        );
    }
}

/// §4.12: the report of a program that stops a worker's group shows a signal
/// on children and the one program it may start; `/bin/kill` is not in it.
#[test]
fn the_authority_report_shows_a_signal_not_a_program() {
    let out = Command::new(BIN)
        .arg("authority")
        .arg(repo_root().join("tests/accept/process_group_kill.cho"))
        .output()
        .expect("the compiler runs");
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(report.contains("child_signal"), "{report}");
    assert!(report.contains("exec(\"/bin/sleep\")"), "{report}");
    assert!(!report.contains("exec(\"/bin\")"), "{report}");
    assert!(!report.contains("kill"), "{report}");
}
