//! `docs/unsafe-and-global-state.md` (#587): the two classes of the PHP security release of 2026-10-21 in this
//! project's own terms, held against real processes. A failed allocation must leave nothing half-made that a later
//! step trusts (§6), and state that outlives the thing it described must not be usable by the next one (§5).
//!
//! A generated program is one process, and `cancho test` runs one process a test (`docs/testing.md`), so "a later
//! run" is a later *unit of work* in a long-lived program (a server) or a later compilation in one compiler process.

use super::sockets::{BACKENDS, build, dial_program};
use super::*;
use std::net::TcpListener;
use std::os::unix::process::ExitStatusExt;

const CONFIGS: [(&str, &[&str]); 3] = [
    ("cranelift", &["--backend", "cranelift"]),
    ("llvm", &["--backend", "llvm", "--compiler", "rust"]),
    ("compiler in cancho", &["--backend", "llvm", "--compiler", "auto"]),
];

/// The memory driver built for each configuration.
fn drivers() -> &'static [(&'static str, PathBuf)] {
    static BUILT: std::sync::OnceLock<Vec<(&'static str, PathBuf)>> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| {
        let dir = scratch("global-state");
        CONFIGS
            .iter()
            .map(|(name, flags)| {
                let exe = dir.join(name.replace(' ', "-"));
                let build = Command::new(BIN)
                    .arg("build")
                    .arg(repo_root().join("tests/programs/memory_exhaustion.cho"))
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

/// How a run under an address-space limit ended.
#[derive(Debug, PartialEq)]
enum Ended {
    /// It finished and printed `done` and this number.
    Done(String),
    /// The trap every checked operation ends with (`SIGILL`), and it had printed no `done`.
    Trapped,
    /// The loader could not map the program under so small a limit: the program never ran.
    NeverStarted,
}

/// `exe mode` with `ulimit -v kib`. Anything but the three ends of [`Ended`] is a failure of the program: a signal that
/// is not the trap (a null pointer used, a bad free) or a status that is not 0.
fn under_limit(exe: &Path, mode: &str, kib: u32) -> Ended {
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!("ulimit -v {kib} && exec \"$0\" \"$1\""))
        .arg(exe)
        .arg(mode)
        .output()
        .expect("sh runs");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    match (out.status.code(), out.status.signal()) {
        (Some(0), _) => Ended::Done(
            stdout
                .trim()
                .strip_prefix("done ")
                .unwrap_or_else(|| panic!("no `done`: {stdout:?}"))
                .to_owned(),
        ),
        (_, Some(4)) => {
            assert!(
                !stdout.contains("done"),
                "{mode} {kib}: trapped after saying done: {stdout:?}"
            );
            Ended::Trapped
        }
        (Some(127), _) if stderr.contains("error while loading") => Ended::NeverStarted,
        other => panic!(
            "{mode} under {kib} KiB ended {other:?}, not by finishing or by the trap:\n{stdout}{stderr}"
        ),
    }
}

/// §6: allocation failure partway through a run. The three drivers ask for more than the limit lets them have, at
/// every step of a sweep from plenty to nothing; each run finishes with the right answer or traps. A failed `malloc`
/// that was used (a null `Box`, a half-grown buffer read after the failed copy) would be a `SIGSEGV`, not a trap, and
/// a wrong answer would be a `done` with another number.
// macOS cannot set the limit the sweep needs: `ulimit -v` answers "cannot modify limit: Invalid argument" there (seen on
// CI), and the kernel has no other per-process address-space limit a shell can set. The claim is measured on Linux only
// (`docs/unsafe-and-global-state.md` §6).
#[cfg_attr(
    target_os = "macos",
    ignore = "macOS has no settable address-space limit (`ulimit -v` is refused); measured on Linux only"
)]
#[test]
fn a_failed_allocation_traps_and_never_leaves_a_half_made_value_in_use() {
    // KiB. The first rows are enough for every driver; the last are not enough for any.
    let limits = [
        1_000_000, 400_000, 200_000, 120_000, 100_000, 80_000, 60_000, 50_000, 40_000, 30_000,
        25_000, 20_000, 15_000, 12_000, 10_000, 9_000, 8_000, 6_000, 4_000,
    ];
    for (config, exe) in drivers() {
        // `b` allocates what it only reads back once, so `-O2` removes the allocation (measured: it finishes under every
        // limit on LLVM) and it is held to the outcomes but not to a trap.
        for (mode, expected, must_trap) in
            [("g", "268435456", true), ("r", "0", true), ("b", "496", false)]
        {
            let mut finished = 0;
            let mut trapped = 0;
            for kib in limits {
                match under_limit(exe, mode, kib) {
                    Ended::Done(n) => {
                        assert_eq!(n, expected, "`{config}` {mode} under {kib} KiB");
                        finished += 1;
                    }
                    Ended::Trapped => trapped += 1,
                    Ended::NeverStarted => {}
                }
            }
            assert!(
                finished > 0 && (trapped > 0 || !must_trap),
                "`{config}` {mode}: the sweep must see both ends ({finished}, {trapped})"
            );
        }
    }
}

/// §6: what a trap leaves. The process ends by `SIGILL` and does not flush: a line written to standard output before the
/// trap is lost (a pipe and a file alike; `docs/agent-toolbox.md` L1, `docs/checked-output.md`), so a log kept on
/// standard output is what a trap erases and one on standard error is not. The test pins the first half; the driver says
/// nothing on standard error here, so the second half is `docs/differential.md`'s measurement.
#[test]
fn a_trap_ends_the_process_and_loses_what_standard_output_still_buffered() {
    for (config, exe) in drivers() {
        let piped = Command::new(exe).arg("x").output().expect("the driver runs");
        assert_eq!(piped.status.signal(), Some(4), "`{config}`: a trap is SIGILL: {piped:?}");
        assert!(
            piped.stdout.is_empty(),
            "`{config}`: the line written before the trap reached the pipe: {piped:?}"
        );
    }
}

/// A server that keeps every connection open, so the descriptors a program dials stay in use at the other end only.
fn keep_open() -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("an address").port();
    let thread = std::thread::spawn(move || {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept() {
            held.push(stream);
            if held.len() == 2 {
                break;
            }
        }
        // Hold both until the program has gone.
        std::thread::sleep(std::time::Duration::from_secs(5));
        drop(held);
    });
    (port, thread)
}

const REUSED_DESCRIPTOR: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var score = 0;
    borrow bound as &n in {
        match tcp_connect(n, "127.0.0.1", PORT) {
            Dialed::Ok(a) => {
                let first = conn_detach(a);
                match conn_attach(first) {
                    Attached::Ok(back) => {
                        // The connection ends and the kernel may give its number to the next one.
                        conn_close(back);
                        match tcp_connect(n, "127.0.0.1", PORT) {
                            Dialed::Ok(b) => {
                                let second = conn_detach(b);
                                // The same descriptor number, held by another connection: the case the epoch is for.
                                if first & 4294967295 == second & 4294967295 { score = score + 1; }
                                // The old ticket must not reach the new connection...
                                match conn_attach(first) {
                                    Attached::Ok(stolen) => { conn_close(stolen); }
                                    Attached::Failed(e) => { if e == 9 { score = score + 1; } }
                                }
                                // ...which is still there for the ticket that is its own.
                                match conn_attach(second) {
                                    Attached::Ok(own) => { score = score + 1; conn_close(own); }
                                    Attached::Failed(e) => { }
                                }
                            }
                            Dialed::Failed(e) => { }
                        }
                    }
                    Attached::Failed(e) => { }
                }
            }
            Dialed::Failed(e) => { }
        }
    }
    release(bound);
    release(io);
    if score == 3 {
        return 0;
    }
    return 100 + score;
}
"#;

/// §5: `lexs_fd_epoch`. A ticket from `conn_detach` names a descriptor and the epoch it had; the table is the one piece
/// of state a long-lived server carries from one connection to the next. When a connection ends and another takes its
/// descriptor number, the first one's ticket must not reach the second (the use-after-free of a descriptor). The program
/// arranges the reuse and *checks that it happened* (equal numbers) before it asks.
#[test]
fn a_ticket_does_not_reach_the_connection_that_took_its_descriptor_number() {
    for backend in BACKENDS {
        let (port, server) = keep_open();
        let dir = scratch(&format!("global-state-fd-{backend}"));
        let exe = build(
            &dir,
            "reuse",
            &dial_program(port, &format!("127.0.0.1:{port}"), REUSED_DESCRIPTOR),
            backend,
        );
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(
            run.status.code(),
            Some(0),
            "{backend}: score {:?} (101 means the number was not reused, so nothing was shown)",
            run.status.code().map(|c| c - 100)
        );
        drop(server);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
