//! `docs/unsafe-and-global-state.md` §5 (#593, #594): the process-wide tables the socket builtins keep, held against real
//! threads. A generated program that runs two threads at the same moment, on every code generator.
//!
//! A race is only visible if the threads arrive together, so each round waits for the same millisecond on the clock
//! in both and then acts; the number of rounds is what turns a window of a few instructions into a measurement.

use super::*;
use std::net::TcpListener;

const CONFIGS: [(&str, &[&str]); 3] = [
    ("cranelift", &["--backend", "cranelift"]),
    ("llvm", &["--backend", "llvm", "--compiler", "rust"]),
    ("compiler in cancho", &["--backend", "llvm", "--compiler", "auto"]),
];

/// `program` of `tests/programs`, built for each configuration.
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

/// A server that accepts every connection and keeps it, until the test is over.
fn holder() -> (u16, std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("an address").port();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done = stop.clone();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept() {
            if done.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            held.push(stream);
        }
    });
    (port, stop)
}

/// The two numbers of the line `rounds N doubled D`.
fn counts(stdout: &str) -> (u32, u32) {
    let words: Vec<&str> = stdout.split_whitespace().collect();
    assert_eq!(words.len(), 4, "a `rounds N doubled D` line: {stdout:?}");
    (words[1].parse().expect("a number"), words[3].parse().expect("a number"))
}

/// #593: a ticket is redeemed once even when two threads redeem copies of it at the same moment. Measured before the fix
/// on x86-64 (4 cores): 4 to 35 of 300 rounds on Cranelift, on LLVM through the Rust compiler and on LLVM through the compiler in
/// cancho (a load, a compare and a store let both threads through); after it, none.
#[test]
fn a_ticket_redeemed_by_two_threads_at_once_is_given_out_once() {
    const ROUNDS: u32 = 400;
    for (config, exe) in drivers("thread-state-tickets", "ticket_race.cho") {
        // Twice, so one unlucky schedule cannot hide a window the first run missed.
        for run in 0..2 {
            let (port, stop) = holder();
            let out = Command::new(&exe)
                .arg(port.to_string())
                .arg(ROUNDS.to_string())
                .output()
                .expect("the driver runs");
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = std::net::TcpStream::connect(("127.0.0.1", port));
            assert_eq!(out.status.code(), Some(0), "`{config}`: {out:?}");
            let (rounds, doubled) = counts(&String::from_utf8_lossy(&out.stdout));
            assert_eq!(rounds, ROUNDS, "`{config}` run {run}: every round was dialled");
            assert_eq!(
                doubled, 0,
                "`{config}` run {run}: {doubled} of {rounds} rounds gave one ticket to two threads"
            );
        }
    }
}
