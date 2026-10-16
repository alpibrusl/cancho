//! Letting go of a child (`docs/processes.md` §4.11): `exec_spawn_detached`,
//! `child_pid` and `child_release`, over real processes, on both backends.
//!
//! The driver is `tests/programs/process_release.cho`. What these tests look
//! at is outside the program: the state, process group and session of the
//! children it left behind, read with `ps`.

use super::processes::build_both;
use super::*;
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::time::{Duration, Instant};

/// The driver, built once with each backend.
fn driver() -> &'static [(&'static str, PathBuf)] {
    static BUILT: std::sync::OnceLock<Vec<(&'static str, PathBuf)>> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| {
        let source =
            std::fs::read_to_string(repo_root().join("tests/programs/process_release.cho"))
                .expect("the driver is in the repository");
        build_both("process-release", &source).1
    })
}

/// One `ps` column of a process, `None` once the process is gone.
fn column(pid: u32, name: &str) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", &format!("{name}="), "-p", &pid.to_string()])
        .output()
        .expect("ps runs");
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Whether the process leads a session: `s` in its `ps` state on both kernels.
/// (macOS's `ps` has no session id to compare; Linux's is compared as well.)
fn leads_a_session(pid: u32) -> bool {
    column(pid, "stat").is_some_and(|stat| stat.contains('s'))
}

/// The session id, on Linux.
#[cfg(target_os = "linux")]
fn session(pid: u32) -> String {
    column(pid, "sid").unwrap_or_else(|| panic!("process {pid} is gone"))
}

/// Running: there, and not a zombie waiting for a parent to reap it.
fn alive(pid: u32) -> bool {
    column(pid, "stat").is_some_and(|stat| !stat.starts_with('Z'))
}

/// Not running within `seconds`: gone, or a zombie (which the caller then
/// tells apart with `column`).
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

/// Signal a whole process group, as a supervisor or a terminal does.
fn kill_group(group: u32) {
    // The `kill` program, not the shell's: dash's builtin has no `--`.
    let status = Command::new("kill")
        .args(["-KILL", "--", &format!("-{group}")])
        .status()
        .expect("kill runs");
    assert!(status.success(), "group {group} should be there to signal");
}

/// Every `pid N` line of the driver's output, in order.
fn pids(lines: &[String]) -> Vec<u32> {
    lines.iter().filter_map(|l| l.strip_prefix("pid ")).map(|n| n.parse().expect("a pid")).collect()
}

/// Run the driver in a process group of its own with its standard input
/// open, read to its `ready` line, and answer it, its lines, and the pipe to
/// close. A driver that ends first has no `ready`, and the test says so.
fn started(
    exe: &Path,
    args: &[&str],
) -> (std::process::Child, Vec<String>, std::process::ChildStdin) {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .process_group(0)
        .spawn()
        .expect("the driver runs");
    let stdin = child.stdin.take().expect("a pipe");
    let mut lines = Vec::new();
    for line in BufReader::new(child.stdout.as_mut().expect("a pipe")).lines() {
        let line = line.expect("a line");
        let done = line == "ready";
        lines.push(line);
        if done {
            break;
        }
    }
    // A driver that ended first is reaped here, and the test says so.
    if lines.last().map(String::as_str) != Some("ready") {
        let status = child.wait();
        panic!("the driver ended ({status:?}) without `ready`: {lines:?}");
    }
    (child, lines, stdin)
}

/// §4.11: a detached child outlives the program that started it, leads a
/// process group and a session of its own, and can be signalled as a group by
/// whoever holds its pid.
#[test]
fn a_detached_child_outlives_its_parent_in_a_session_of_its_own() {
    for (backend, exe) in driver() {
        let begun = Instant::now();
        let run = Command::new(exe).arg("d").output().expect("the driver runs");
        assert_eq!(run.status.code(), Some(0), "`{backend}`: the driver returns at once");
        assert!(
            begun.elapsed() < Duration::from_secs(20),
            "`{backend}`: it must not wait for the child"
        );
        let text = String::from_utf8_lossy(&run.stdout).into_owned();
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        assert!(lines.contains(&"released 0".to_owned()), "`{backend}`: {text}");
        let pid = pids(&lines)[0];

        assert!(alive(pid), "`{backend}`: the child is still running after its parent ended");
        assert_eq!(column(pid, "pgid"), Some(pid.to_string()), "`{backend}`: it leads its group");
        assert!(leads_a_session(pid), "`{backend}`: it leads a session");
        #[cfg(target_os = "linux")]
        assert_eq!(session(pid), pid.to_string(), "`{backend}`: its session is its own pid");
        kill_group(pid);
        assert!(ends(pid, 10), "`{backend}`: the group signal reaches the child");
    }
}

/// §4.11: of two children released by one program, the plain one is still in
/// its parent's session and group and dies with them; the detached one is in
/// neither and survives the group being killed.
#[test]
fn only_the_detached_child_survives_its_parents_group() {
    for (backend, exe) in driver() {
        let (mut parent, lines, stdin) = started(exe, &["b"]);
        let (detached, plain) = (pids(&lines)[0], pids(&lines)[1]);
        let group = parent.id();

        assert!(leads_a_session(detached), "`{backend}`: a session of its own");
        assert!(!leads_a_session(plain), "`{backend}`: the plain child is in its parent's");
        #[cfg(target_os = "linux")]
        {
            assert_ne!(session(detached), session(group), "`{backend}`");
            assert_eq!(session(plain), session(group), "`{backend}`");
        }
        assert_eq!(column(detached, "pgid"), Some(detached.to_string()), "`{backend}`");
        assert_eq!(column(plain, "pgid"), Some(group.to_string()), "`{backend}`");

        kill_group(group);
        let _ = parent.wait();
        drop(stdin);
        assert!(ends(plain, 10), "`{backend}`: the plain child went with its group");
        assert!(alive(detached), "`{backend}`: the detached child outlived its parent's group");
        kill_group(detached);
        assert!(ends(detached, 10), "`{backend}`");
    }
}

/// §4.11: a released child is reaped by the runtime, so a parent that lives on
/// and starts many does not collect zombies; and a child it did not release is
/// still its own to wait for.
#[test]
fn released_children_do_not_become_zombies() {
    let mut shapes = Vec::new();
    for (backend, exe) in driver() {
        let (mut parent, lines, stdin) = started(exe, &["c", "50"]);
        let released = pids(&lines);
        assert_eq!(released.len(), 50, "`{backend}`: {lines:?}");
        for pid in &released {
            assert_eq!(
                column(*pid, "stat"),
                None,
                "`{backend}`: child {pid} ended and was released, so nothing is left of it"
            );
        }
        assert!(
            lines.contains(&"held 7".to_owned()),
            "`{backend}`: a held child is still waited for"
        );
        drop(stdin);
        assert_eq!(parent.wait().expect("waits").code(), Some(0), "`{backend}`");
        shapes.push(lines.into_iter().filter(|l| !l.starts_with("pid ")).collect::<Vec<_>>());
    }
    assert_eq!(shapes[0], shapes[1], "the two backends disagree");
}

/// §4.11: releasing a child that has already ended reaps it at once, and
/// answers `0`.
#[test]
fn a_child_that_has_already_ended_is_released_cleanly() {
    for (backend, exe) in driver() {
        let (mut parent, lines, stdin) = started(exe, &["e"]);
        let pid = pids(&lines)[0];
        assert!(lines.contains(&"released 0".to_owned()), "`{backend}`: {lines:?}");
        assert_eq!(column(pid, "stat"), None, "`{backend}`: no zombie of the child");
        drop(stdin);
        assert_eq!(parent.wait().expect("waits").code(), Some(0), "`{backend}`");
    }
}

/// §4.1, §4.11: a detached start is bound by the capability's prefix as any
/// start is.
const BOUND: &str = r##"edition 9;
fn start[&x, &p](exec: &x Exec("/bin"), path: &p [byte]) -> [exec("/bin"), exec_detached] int {
    match exec_spawn_detached(exec, path, "-c\0exit 0\0", "", Stdio::Null, Stdio::Null, Stdio::Null) {
        Spawned::Ok(child) => { return child_release(child); }
        Spawned::Failed(e) => { return 100 + e; }
    }
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec, tty } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(net); release(clock);
    release(signals); release(tty);
    let bin = narrow(exec, "/bin");
    var status = 0;
    borrow bin as &x in {
        borrow args as &g in {
            if int_of(arg(g, 1)[0]) == 'i' {
                status = start(x, "/bin/sh");
            } else {
                status = start(x, "/usr/bin/true");
            }
        }
    }
    release(bin);
    release(args);
    return status;
}
"##;

#[test]
fn a_detached_start_stays_inside_the_prefix_it_was_narrowed_to() {
    use std::os::unix::process::ExitStatusExt;
    let (dir, built) = build_both("process-detached-bound", BOUND);
    for (backend, exe) in &built {
        let inside = Command::new(exe).arg("inside").output().expect("the program runs");
        assert_eq!(inside.status.code(), Some(0), "`{backend}`: a start under the prefix");
        let outside = Command::new(exe).arg("outside").output().expect("the program runs");
        assert!(
            matches!(outside.status.signal(), Some(4 | 5)),
            "`{backend}`: a start outside the prefix traps, not {:?}",
            outside.status
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// §4.11: the report says that a program can leave a process running, and
/// only a program that can does.
#[test]
fn the_authority_report_says_a_process_may_outlive_the_program() {
    let report = |relative: &str| {
        let out = Command::new(BIN)
            .arg("authority")
            .arg(repo_root().join(relative))
            .output()
            .expect("the compiler runs");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let released = report("tests/programs/process_release.cho");
    assert!(released.contains("exec(\"/bin\")"), "{released}");
    assert!(released.contains("exec_detached"), "{released}");
    let waited = report("tests/accept/process_spawn.cho");
    assert!(waited.contains("exec(\"/bin\")"), "{waited}");
    assert!(!waited.contains("exec_detached"), "{waited}");
}

/// §4.11: the `pidfd` a Linux child carries is closed by the release as it is
/// by the wait: after fifty releases the program holds as many descriptors as
/// after none. (macOS watches a pid and holds no descriptor, so there is
/// nothing to count there.)
#[test]
fn releasing_a_child_gives_back_its_pidfd() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let open = |pid: u32| {
        std::fs::read_dir(format!("/proc/{pid}/fd")).expect("the driver's descriptors").count()
    };
    for (backend, exe) in driver() {
        let mut counts = Vec::new();
        for n in ["0", "50"] {
            let (mut parent, _lines, stdin) = started(exe, &["c", n]);
            counts.push(open(parent.id()));
            drop(stdin);
            let _ = parent.wait();
        }
        assert_eq!(counts[0], counts[1], "`{backend}`: a descriptor is left behind per release");
    }
}
