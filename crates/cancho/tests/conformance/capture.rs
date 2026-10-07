//! `std.process` (`docs/processes.md` §7.1): an argument list, and a child's
//! output captured with a bound and a deadline. One driver,
//! `tests/programs/process_capture.cho`, built with each backend; every case
//! runs on both and the two must say the same, apart from the time taken.

use super::*;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// The driver built once for every test here, as `processes.rs` builds its
/// probe: the tests run in parallel.
fn driver_built() -> &'static [(&'static str, PathBuf)] {
    static BUILT: std::sync::OnceLock<Vec<(&'static str, PathBuf)>> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| {
        let dir = scratch("process-capture");
        let source = repo_root().join("tests/programs/process_capture.cho");
        let mut built = Vec::new();
        for backend in BACKENDS {
            let exe = dir.join(format!("capture-{backend}"));
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

/// Run one case on both backends: what each printed, less its last line, the
/// milliseconds it took, which comes back as the second value (the slower).
fn case(mode: &str) -> (String, i64) {
    let mut answers = Vec::new();
    let mut slowest = 0;
    for (backend, exe) in driver_built() {
        let run = Command::new(exe).arg(mode).output().expect("the driver runs");
        assert_eq!(run.status.code(), Some(0), "`{backend}`: the driver itself should not fail");
        let text = String::from_utf8_lossy(&run.stdout).into_owned();
        let mut kept = String::new();
        for line in text.lines() {
            match line.strip_prefix("ms ") {
                Some(ms) => slowest = slowest.max(ms.parse::<i64>().expect("a time")),
                None => {
                    kept.push_str(line);
                    kept.push('\n');
                }
            }
        }
        answers.push(kept);
    }
    assert_eq!(answers[0], answers[1], "the two backends disagree on `{mode}`");
    (answers.remove(0), slowest)
}

/// §7.1: 1 MiB through `cat`, written while it is read. A channel holds 8,192
/// bytes on macOS and 180,224 on Linux, and writing it all before reading
/// any deadlocks from 32 KiB and 512 KiB; every byte comes back, in order.
#[test]
fn input_and_output_are_interleaved_past_both_kernels_buffers() {
    let (printed, _) = case("cat");
    let mut lines = printed.lines();
    let want = lines.next().and_then(|l| l.strip_prefix("want sum ")).expect("the expected sum");
    assert_eq!(
        lines.collect::<Vec<_>>(),
        ["code 0", "kept 1048576", &format!("sum {want}")],
        "{printed}"
    );
}

/// §7.1: `most` bytes are kept and the capture is a normal one; the byte
/// past `most` kills the child, and exactly `most` are kept. The child
/// lingers after its output (`sleep 30`), so only the kill ends it: closing
/// the channel alone would end the writer and not the shell.
#[test]
fn output_past_the_bound_ends_the_child() {
    assert_eq!(case("exact").0, "code 0\nkept 1000\nsum 0\n");
    let (printed, ms) = case("most");
    assert_eq!(printed, "too much\nkept 1000\nsum 0\n");
    assert!(ms < 5000, "the child outlived the bound: {ms} ms");
}

/// §7.1: a child the poller cannot watch is killed and reaped, and the answer
/// is the `errno`: with `pidfd_open` refused (`ENOSYS`, 38), `sleep 30` is not
/// waited on blind.
#[cfg(target_os = "linux")]
#[test]
fn a_child_that_cannot_be_watched_is_killed_not_waited_on() {
    for (backend, exe) in driver_built() {
        let mut command = Command::new(exe);
        command.arg("failed");
        super::processes::without_pidfd_open(&mut command);
        let run = command.output().expect("the driver runs");
        let text = String::from_utf8_lossy(&run.stdout);
        assert!(text.starts_with("failed 38\nkept 0\nsum 0\nms "), "`{backend}`: {text}");
        let ms: i64 =
            text.trim_end().rsplit(' ').next().and_then(|n| n.parse().ok()).expect("a time");
        assert!(ms < 5000, "`{backend}`: the child was waited on: {ms} ms");
    }
}

/// §7.1: a capture with nothing to do waits; it does not spin. The child
/// closes both its streams and sleeps for a second: a channel that has ended,
/// or one whose reader has gone, stays ready for ever unless it is closed, and
/// a capture that kept either on its poller would spend that second on the
/// processor. Measured by `wait4`'s account of the driver's own time.
#[test]
fn a_quiet_child_is_waited_for_not_spun_on() {
    quiet_child_costs_little("quiet", "code 0\nkept 0\nsum 0\n");
}

/// §7.2: the same with the errors channel. The child closes all three of its
/// streams and sleeps, and ends of all three channels are reported readable
/// for ever unless each is closed: `capture_both` that forgot the errors
/// would spin on that one.
#[test]
fn a_quiet_childs_errors_are_waited_for_not_spun_on() {
    quiet_child_costs_little("r", "code 0\nout kept 0\nout sum 0\nerr kept 0\nerr sum 0\n");
}

// The driver is reaped by `wait4`, which clippy cannot see: `Child::wait` would
// reap it without the account this test is for.
#[allow(clippy::zombie_processes)]
fn quiet_child_costs_little(mode: &str, answer: &str) {
    #[repr(C)]
    struct Usage {
        // Two `struct timeval`s, user then system, then fields not read.
        times: [i64; 4],
        rest: [i64; 14],
    }
    unsafe extern "C" {
        fn wait4(pid: i32, status: *mut i32, options: i32, usage: *mut Usage) -> i32;
    }
    for (backend, exe) in driver_built() {
        let mut child = Command::new(exe)
            .arg(mode)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the driver runs");
        let mut printed = String::new();
        std::io::Read::read_to_string(child.stdout.as_mut().expect("its output"), &mut printed)
            .expect("its output is read");
        let mut usage = Usage { times: [0; 4], rest: [0; 14] };
        let mut status = 0;
        // SAFETY: the driver is this test's own child, not yet waited for; `Usage`
        // is at least as large as `struct rusage` on both targets.
        let pid = unsafe { wait4(child.id() as i32, &mut status, 0, &mut usage) };
        assert_eq!(pid, child.id() as i32, "`{backend}`: wait4");
        // A `timeval`'s microseconds are its second word's low 32 bits on both
        // targets (an `i64` on Linux, an `i32` and padding on macOS).
        let micros = |sec: i64, usec: i64| sec * 1_000_000 + (usec as i32) as i64;
        let cpu_ms = (micros(usage.times[0], usage.times[1])
            + micros(usage.times[2], usage.times[3]))
            / 1000;
        assert!(printed.starts_with(answer), "`{backend}`: {printed}");
        assert!(cpu_ms < 400, "`{backend}`: a one-second wait took {cpu_ms} ms of processor time");
    }
}

/// §7.1: the deadline is the `Clock`'s, not a timeout per wait. `sleep 30`
/// under 200 ms is killed, reaped and reported `TimedOut`, well before 30 s.
#[test]
fn a_deadline_ends_a_child_that_outlives_it() {
    let (printed, ms) = case("time");
    assert_eq!(printed, "timed out\nkept 0\nsum 0\n");
    assert!((200..5000).contains(&ms), "200 ms asked for, {ms} taken");
}

/// §7.1: the child's exit ends the capture, not the end of the stream. A
/// background `sleep 2` holds the output end for two seconds after the shell
/// has exited; the shell's `hi` is captured long before that.
#[test]
fn a_process_left_behind_does_not_hold_the_capture() {
    let (printed, ms) = case("behind");
    assert_eq!(printed, "code 0\nkept 3\nsum 103209\n");
    assert!(ms < 1500, "the capture waited for the grandchild: {ms} ms");
}

/// §7.1: a child that never reads its input is not an error -- the write
/// fails, the input end is closed, and the child's own exit status is the
/// answer. No input at all is the same.
#[test]
fn a_child_that_does_not_read_its_input_still_exits() {
    assert_eq!(case("deaf").0, "code 0\nkept 0\nsum 0\ncode 4\nkept 0\nsum 0\n");
}

/// §7.1: an argument containing a `\0` would arrive as two; `add` refuses it
/// with `EINVAL` (22) and leaves it out, and the list without it runs.
#[test]
fn an_argument_with_a_nul_is_refused_and_left_out() {
    assert_eq!(case("nul").0, "0 22 0 2\ncode 6\nkept 0\nsum 0\n");
}

/// §7.1: `capture`'s row is `[heap, clock, poll]`, and a caller that lends it
/// the `Clock` performs `clock` through it: a row that leaves it out is
/// refused. A test rather than a reject fixture, because fixtures are checked
/// without `--std`.
#[test]
fn a_caller_of_capture_declares_the_clock() {
    const CALLER: &str = r##"edition 7;
import std.buffer;
import std.process;

fn wait[&h, &c](heap: &!h Heap, clock: &c Clock, child: Child, to: Pipe, from: Pipe)
    -> [heap, poll] int {
    let (out, ran) = process.capture(heap, clock, child, to, from, "", 10, 10);
    return buffer.drop(heap, out);
}

fn main(world: World) -> [] int {
    release(world);
    return 0;
}
"##;
    let dir = scratch("capture-row");
    let path = dir.join("caller.cho");
    std::fs::write(&path, CALLER).expect("the program is written");
    let out = Command::new(BIN)
        .arg("check")
        .arg(&path)
        .args(["--std", "--output", "json"])
        .output()
        .expect("the compiler runs");
    let body = String::from_utf8_lossy(&out.stdout);
    assert!(body.contains("\"rule\": \"effect-not-declared\""), "{body}");
    assert!(body.contains("performs `clock`"), "{body}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// §7.2: `capture_both` gives every channel back when the deadline ends it.
/// Under a limit of 40 descriptors sixty captures come and go, each with three
/// channels; the errors end that was not closed would leak one per capture, and
/// `channels_with_errors` would answer `EMFILE` (24) well before the sixtieth.
#[test]
fn a_capture_that_times_out_gives_back_all_three_channels() {
    for (backend, exe) in driver_built() {
        let run = Command::new("/bin/sh")
            .args(["-c", "ulimit -n 40; exec \"$0\" \"$@\""])
            .arg(exe)
            .arg("l")
            .output()
            .expect("the driver runs");
        let text = String::from_utf8_lossy(&run.stdout);
        assert!(text.starts_with("timed out 60\n"), "`{backend}`: {text}");
    }
}

/// The driver's checksum (`sum` in `process_capture.cho`) of `bytes`.
fn checksum(bytes: &[u8]) -> i64 {
    bytes.iter().fold(0, |s, &b| (s * 31 + i64::from(b)) % 1_000_000_007)
}

/// §7.2: the two channels are kept apart, each with its own bytes, and the
/// child's exit status is the answer.
#[test]
fn standard_error_is_captured_beside_standard_output() {
    let (printed, _) = case("x");
    assert_eq!(
        printed,
        format!(
            "code 3\nout kept 4\nout sum {}\nerr kept 4\nerr sum {}\n",
            checksum(b"out\n"),
            checksum(b"err\n")
        )
    );
}

/// §7.2: a megabyte of errors before any output. A channel holds 8,192 bytes on
/// macOS and 180,224 on Linux, so a capture that read the output to its end
/// before the errors would leave the child blocked on the errors for ever;
/// both are on the poller, and every byte of both comes back.
#[test]
fn a_flood_of_errors_does_not_stall_the_output() {
    let (printed, ms) = case("y");
    assert_eq!(
        printed,
        format!(
            "code 0\nout kept 4\nout sum {}\nerr kept 1048576\nerr sum 0\n",
            checksum(b"out\n")
        )
    );
    assert!(ms < 5000, "the capture waited on the child: {ms} ms");
}

/// §7.2: `most_errors` is the errors' own bound. Errors past it end the child
/// (`sleep 30` follows them, so only the kill ends it) and exactly the bound is
/// kept; and the bounds are separate: four bytes of output under a bound of
/// four is no overrun, and four of errors under three is.
#[test]
fn errors_past_their_bound_end_the_child() {
    let (printed, ms) = case("z");
    assert_eq!(printed, "too much\nout kept 0\nout sum 0\nerr kept 1000\nerr sum 0\n");
    assert!(ms < 5000, "the child outlived the bound: {ms} ms");
    assert_eq!(
        case("w").0,
        format!(
            "too much\nout kept 4\nout sum {}\nerr kept 3\nerr sum {}\n",
            checksum(b"out\n"),
            checksum(b"err")
        )
    );
}

/// §7.2: a deadline that passes returns what both channels held by then.
#[test]
fn a_deadline_returns_both_channels_so_far() {
    let (printed, ms) = case("v");
    assert_eq!(
        printed,
        format!(
            "timed out\nout kept 2\nout sum {}\nerr kept 2\nerr sum {}\n",
            checksum(b"a\n"),
            checksum(b"b\n")
        )
    );
    assert!((300..5000).contains(&ms), "300 ms asked for, {ms} taken");
}
