//! `docs/signals.md`: the signal capability, on both backends. Every program
//! here is `edition 6;`, declares no `extern fn` and holds no `Ffi` -- that is
//! the point -- and the signals are sent by the test, to the real process the
//! compiler built, through `kill(2)`.
//!
//! The programs speak on **standard error** (`ready`, `mask=8`, ...), which is
//! unbuffered; standard output is fully buffered behind a pipe and a line
//! written there would arrive at exit. They are driven by standard input, one
//! byte a command, so a signal sent before a `p` is queued before the poll
//! that must see it: `kill(2)` returns once the signal is pending.

use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, ExitStatus};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// The signal numbers of the machine the test runs on, by name.
fn host_number(name: &str) -> i32 {
    let darwin = cfg!(target_os = "macos");
    match name {
        "HUP" => 1,
        "INT" => 2,
        "QUIT" => 3,
        "ALRM" => 14,
        "TERM" => 15,
        "WINCH" => 28,
        "USR1" => {
            if darwin {
                30
            } else {
                10
            }
        }
        "USR2" => {
            if darwin {
                31
            } else {
                12
            }
        }
        other => panic!("no host number for `{other}`"),
    }
}

/// cancho's own bit for a claimable signal (`docs/signals.md` section 2.2).
fn bit(name: &str) -> i64 {
    match name {
        "HUP" => 1,
        "INT" => 2,
        "QUIT" => 4,
        "TERM" => 8,
        "USR1" => 16,
        "USR2" => 32,
        "ALRM" => 64,
        "WINCH" => 128,
        other => panic!("no bit for `{other}`"),
    }
}

const CLAIMABLE: [&str; 8] = ["ALRM", "HUP", "INT", "QUIT", "TERM", "USR1", "USR2", "WINCH"];

fn build(dir: &Path, name: &str, source: &str, backend: &str) -> PathBuf {
    let file = dir.join(format!("{name}.cho"));
    std::fs::write(&file, source).expect("a writable fixture");
    let exe = dir.join(format!("{name}-{backend}"));
    let build = Command::new(BIN)
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
        build.status.success(),
        "`--backend {backend}` should build `{name}`, but said:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    exe
}

/// A running program: its standard error as lines, its standard input open.
struct Run {
    child: Child,
    lines: Receiver<String>,
}

impl Run {
    fn start(exe: &Path) -> Run {
        Run::start_command(Command::new(exe))
    }

    /// The program with `signal` already ignored when it starts, as a
    /// process run by `nohup` or a background shell job has `HUP` and `INT`.
    fn start_ignoring(exe: &Path, signal: &str) -> Run {
        let mut command = Command::new("sh");
        command.arg("-c").arg(format!("trap '' {signal}; exec \"$0\"")).arg(exe);
        Run::start_command(command)
    }

    fn start_command(mut command: Command) -> Run {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the program runs");
        let stderr = child.stderr.take().expect("a piped standard error");
        let (send, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        Run { child, lines }
    }

    /// The next line the program wrote, or a panic after 20 seconds.
    fn line(&self) -> String {
        match self.lines.recv_timeout(Duration::from_secs(20)) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => panic!("the program wrote nothing for 20 seconds"),
            Err(RecvTimeoutError::Disconnected) => panic!("the program ended without a line"),
        }
    }

    /// The next line, which must be `expected`.
    fn expect(&self, expected: &str) {
        let got = self.line();
        assert_eq!(got, expected, "the program said {got:?}");
    }

    fn signal(&self, name: &str) {
        self.signal_number(host_number(name));
    }

    fn signal_number(&self, number: i32) {
        // SAFETY: `kill(2)` takes two integers and touches no memory of ours.
        let sent = unsafe { kill(self.child.id() as i32, number) };
        assert_eq!(sent, 0, "kill({}, {number})", self.child.id());
    }

    /// Standard input, one byte a command.
    fn command(&mut self, byte: u8) {
        let stdin = self.child.stdin.as_mut().expect("standard input is open");
        stdin.write_all(&[byte]).expect("the program reads its input");
        stdin.flush().expect("a flushed command");
    }

    /// Close standard input and wait for the end, however it comes.
    fn finish(&mut self) -> ExitStatus {
        drop(self.child.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.try_wait().expect("the child can be waited for") {
                return status;
            }
            assert!(Instant::now() < deadline, "the program did not end within 20 seconds");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Run {
    /// A test that panics must not leave the program behind.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Was the process ended by this signal?
fn killed_by(status: ExitStatus, number: i32) -> bool {
    use std::os::unix::process::ExitStatusExt;
    status.signal() == Some(number)
}

/// `say` and `say_number`: a line on standard error, which is unbuffered.
const SAY: &str = r#"
fn say[&i, &t](out: &!i Io, text: &t [byte]) -> [err_write] int {
    return io.error_all(out, text);
}

fn digits[&b](n: int, buf: &!b [byte]) -> [] int {
    var width = 1;
    var t = n;
    while t >= 10 {
        t = t / 10;
        width = width + 1;
    }
    var k = width;
    var m = n;
    while k > 0 {
        buf[k - 1] = byte_of('0' + m % 10);
        m = m / 10;
        k = k - 1;
    }
    return width;
}

fn say_number[&i](out: &!i Io, n: int) -> [err_write] int {
    region a {
        let nb = alloc_slice[a](24, byte_of(0));
        if n < 0 {
            say(out, "-");
            say(out, nb[0..digits(0 - n, nb)]);
        } else {
            say(out, nb[0..digits(n, nb)]);
        }
    }
    return 0;
}
"#;

/// `name=value\n` on standard error.
const SAY_PAIR: &str = r#"
fn say_pair[&i](out: &!i Io, name: &static [byte], value: int) -> [err_write] int {
    say(out, name);
    say_number(out, value);
    say(out, "\n");
    return 0;
}
"#;

/// Every claimable signal, claimed; standard input is the program's commands:
/// `p` polls once and says the mask, anything else ends.
fn every_signal_program() -> String {
    format!(
        r#"edition 6;
import std.io;
{SAY}{SAY_PAIR}
fn serve[&i, &w](out: &!i Io, watch: &!w SignalWatch) -> [io_read, err_write, signals_read] int {{
    var more = true;
    while more {{
        let c = getchar(out);
        if c == 112 {{
            say_pair(out, "mask=", signals_pending(watch));
        }} else {{
            more = false;
        }}
    }}
    return 0;
}}

fn main(world: World) -> [] int {{
    let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);
    release(ffi); release(fs); release(heap); release(args); release(net); release(clock);
    let claim = narrow(signals, "{}");
    var status = 1;
    borrow claim as &s in {{
        match signals_watch(s) {{
            Watching::Ok(w) => {{
                var watch = w;
                borrow mut io as &!i in {{
                    say(i, "ready\n");
                    borrow mut watch as &!wh in {{
                        serve(i, wh);
                    }}
                    say(i, "ended\n");
                }}
                status = signals_close(watch);
            }}
            Watching::Failed(e) => {{ status = 2; }}
        }}
    }}
    release(claim);
    release(io);
    return status;
}}
"#,
        CLAIMABLE.join(",")
    )
}

/// Each claimed signal is delivered **exactly once**: sent, then polled, it
/// is in the mask; polled again, it is not. The process is alive for all
/// eight, which a signal left to its default action would not allow (`HUP`,
/// `INT`, `QUIT`, `TERM`, `USR1`, `USR2` and `ALRM` all end a process).
#[test]
fn each_claimed_signal_is_delivered_exactly_once() {
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-once-{backend}"));
        let exe = build(&dir, "once", &every_signal_program(), backend);
        let mut run = Run::start(&exe);
        run.expect("ready");
        for name in CLAIMABLE {
            run.signal(name);
            run.command(b'p');
            run.expect(&format!("mask={}", bit(name)));
            run.command(b'p');
            run.expect("mask=0");
        }
        // And a poll when nothing was sent says nothing arrived.
        run.command(b'p');
        run.expect("mask=0");
        run.command(b'q');
        run.expect("ended");
        let status = run.finish();
        assert_eq!(status.code(), Some(0), "{backend}: {status:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Several signals that arrive **between two polls** are all reported: that
/// is what the mask is for. The same signal twice is one, because the kernel
/// keeps one pending instance of a standard signal -- the contract says
/// "at least once since", and this is the test that it does not say more.
#[test]
fn signals_between_polls_are_all_reported_and_a_repeat_is_one() {
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-mask-{backend}"));
        let exe = build(&dir, "mask", &every_signal_program(), backend);
        let mut run = Run::start(&exe);
        run.expect("ready");

        run.signal("TERM");
        run.signal("INT");
        run.signal("HUP");
        run.command(b'p');
        run.expect(&format!("mask={}", bit("TERM") | bit("INT") | bit("HUP")));

        // The same signal three times, with another between: two bits, not four.
        run.signal("USR1");
        run.signal("USR1");
        run.signal("WINCH");
        run.signal("USR1");
        run.command(b'p');
        run.expect(&format!("mask={}", bit("USR1") | bit("WINCH")));

        // All eight at once.
        for name in CLAIMABLE {
            run.signal(name);
        }
        run.command(b'p');
        run.expect("mask=255");
        run.command(b'p');
        run.expect("mask=0");

        run.command(b'q');
        run.expect("ended");
        assert_eq!(run.finish().code(), Some(0), "{backend}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A program claims **only** what it names: the same program, claiming `TERM`,
/// is ended by `USR1` as any program is. A claim that took more than it was
/// asked for would be a program that could not be stopped.
#[test]
fn a_signal_that_was_not_claimed_still_takes_its_default_action() {
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-unclaimed-{backend}"));
        let source = every_signal_program().replace(&CLAIMABLE.join(","), "TERM");
        let exe = build(&dir, "unclaimed", &source, backend);
        let mut run = Run::start(&exe);
        run.expect("ready");
        // The claimed one is held...
        run.signal("TERM");
        run.command(b'p');
        run.expect("mask=8");
        // ...the other is not.
        run.signal("USR1");
        let status = run.finish();
        assert!(killed_by(status, host_number("USR1")), "{backend}: {status:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn poller_program() -> String {
    format!(
        r#"edition 6;
import std.io;
{SAY}{SAY_PAIR}
fn main(world: World) -> [] int {{
    let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);
    release(ffi); release(fs); release(heap); release(args); release(net);
    let claim = narrow(signals, "TERM");
    var status = 1;
    borrow mut io as &!i in {{
        borrow claim as &s in {{
            match signals_watch(s) {{
                Watching::Ok(w) => {{
                    var watch = w;
                    match poller_new() {{
                        Polling::Ok(p) => {{
                            var poller = p;
                            borrow mut poller as &!ph in {{
                                borrow mut watch as &!wh in {{
                                    say_pair(i, "added=", poller_add_signals(ph, wh, 7));
                                    region a {{
                                        var ev = alloc_slice[a](4, 0);
                                        borrow clock as &c in {{
                                            say(i, "ready\n");
                                            let before = clock_ms(c);
                                            let n = poller_wait(ph, ev, 20000);
                                            let after = clock_ms(c);
                                            say_pair(i, "woke=", n);
                                            say_pair(i, "token=", ev[0]);
                                            say_pair(i, "events=", ev[1]);
                                            say_pair(i, "waited=", after - before);
                                            say_pair(i, "mask=", signals_pending(wh));
                                            // Taken, so the descriptor is no longer ready.
                                            say_pair(i, "again=", poller_wait(ph, ev, 150));
                                        }}
                                    }}
                                }}
                            }}
                            poller_close(poller);
                            status = 0;
                        }}
                        Polling::Failed(e) => {{ status = 3; }}
                    }}
                    signals_close(watch);
                }}
                Watching::Failed(e) => {{ status = 2; }}
            }}
        }}
    }}
    release(clock);
    release(claim);
    release(io);
    return status;
}}
"#
    )
}

/// A claimed signal is a handle the `Poller` can wait on: a `poller_wait`
/// with a 20-second timeout returns **when the signal arrives**, reporting the
/// token and *readable*, and keeps reporting it until `signals_pending` takes
/// the signal (level-triggered, like every handle on the `Poller`).
#[test]
fn a_signal_wakes_a_poller_wait_at_once() {
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-poller-{backend}"));
        let exe = build(&dir, "poller", &poller_program(), backend);
        let mut run = Run::start(&exe);
        run.expect("added=0");
        run.expect("ready");
        // The wait is established by now: a signal sent later cannot have been
        // picked up by a poll that ran before the wait began.
        std::thread::sleep(Duration::from_millis(300));
        let sent = Instant::now();
        run.signal("TERM");
        run.expect("woke=1");
        let woke = sent.elapsed();
        run.expect("token=7");
        run.expect("events=1");
        let waited: i64 = run.line().strip_prefix("waited=").expect("waited=").parse().unwrap();
        run.expect("mask=8");
        run.expect("again=0");
        eprintln!("{backend}: woke {woke:?} after the signal; the wait lasted {waited} ms");
        // 20 s was the timeout; a wake that took the timeout out would not be this.
        assert!(woke < Duration::from_secs(5), "{backend}: woke {woke:?} after the signal");
        assert!((250..5000).contains(&waited), "{backend}: the wait lasted {waited} ms");
        let status = run.finish();
        assert_eq!(status.code(), Some(0), "{backend}: {status:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn second_signal_program() -> String {
    format!(
        r#"edition 6;
import std.io;
{SAY}{SAY_PAIR}
// Wait on an empty poller for `ms`: a sleep.
fn sleep(ms: int) -> [poll] int {{
    match poller_new() {{
        Polling::Ok(p) => {{
            var poller = p;
            region a {{
                var ev = alloc_slice[a](2, 0);
                borrow mut poller as &!ph in {{ poller_wait(ph, ev, ms); }}
            }}
            poller_close(poller);
        }}
        Polling::Failed(e) => {{ }}
    }}
    return 0;
}}

// Commands on standard input: `r` reads the signals first, anything else
// closes the claim as it stands; then the claim is closed and the program
// says it survived.
fn main(world: World) -> [] int {{
    let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);
    release(ffi); release(fs); release(heap); release(args); release(net); release(clock);
    let claim = narrow(signals, "TERM,USR2");
    var status = 1;
    borrow mut io as &!i in {{
        borrow claim as &s in {{
            match signals_watch(s) {{
                Watching::Ok(w) => {{
                    var watch = w;
                    say(i, "ready\n");
                    let c = getchar(i);
                    if c == 114 {{
                        borrow mut watch as &!wh in {{
                            say_pair(i, "first=", signals_pending(wh));
                        }}
                    }}
                    say_pair(i, "closed=", signals_close(watch));
                    sleep(20000);
                    say(i, "survived\n");
                    status = 0;
                }}
                Watching::Failed(e) => {{ status = 2; }}
            }}
        }}
    }}
    release(claim);
    release(io);
    return status;
}}
"#
    )
}

/// "A second signal kills at once": after the first is read, closing the
/// claim puts the default action back, and the next `TERM` ends the process --
/// by the signal, not by an exit status.
#[test]
fn closing_the_claim_makes_the_next_signal_kill_at_once() {
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-second-{backend}"));
        let exe = build(&dir, "second", &second_signal_program(), backend);
        let mut run = Run::start(&exe);
        run.expect("ready");
        run.signal("TERM");
        run.command(b'r');
        run.expect("first=8");
        run.expect("closed=0");
        let sent = Instant::now();
        run.signal("TERM");
        let status = run.finish();
        assert!(killed_by(status, host_number("TERM")), "{backend}: {status:?}");
        assert!(sent.elapsed() < Duration::from_secs(5), "{backend}: not at once");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A signal that arrived **before the claim was closed and was never read**
/// is not swallowed: closing delivers it with the default action, so a
/// `TERM` ends the process before the next line. That is the second signal
/// arriving ahead of the first being acted on.
#[test]
fn closing_with_a_signal_unread_delivers_it_at_once() {
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-unread-{backend}"));
        let exe = build(&dir, "unread", &second_signal_program(), backend);
        let mut run = Run::start(&exe);
        run.expect("ready");
        run.signal("TERM");
        run.command(b'g');
        let status = run.finish();
        assert!(killed_by(status, host_number("TERM")), "{backend}: {status:?}");
        // It never got as far as saying it was closed.
        assert!(
            run.lines.try_iter().all(|line| !line.starts_with("closed=")),
            "{backend}: the program ran on past the close"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// And a signal read last is read: closing with nothing queued ends the claim
/// and the process lives on until it is signalled again.
#[test]
fn closing_after_the_read_leaves_nothing_to_deliver() {
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-read-{backend}"));
        let exe = build(&dir, "read", &second_signal_program(), backend);
        let mut run = Run::start(&exe);
        run.expect("ready");
        run.signal("USR2");
        run.command(b'r');
        run.expect("first=32");
        run.expect("closed=0");
        // Alive: it is sleeping on an empty poller, and the signal it read is gone.
        std::thread::sleep(Duration::from_millis(300));
        assert!(run.child.try_wait().unwrap().is_none(), "{backend}: it ended on its own");
        // Not claimed any more, so `USR2` is `USR2` again.
        run.signal("USR2");
        let status = run.finish();
        assert!(killed_by(status, host_number("USR2")), "{backend}: {status:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A signal the process inherited as *ignored* (a background job's `INT`, a
/// `nohup`ed `HUP`) can still be claimed and read: the claim does not depend
/// on the disposition. What it is afterwards is the one place the two kernels
/// differ (`docs/signals.md` section 3): Linux never touched it, so it is
/// ignored again; macOS ignored it to watch it and puts back the default.
#[test]
fn a_signal_ignored_on_entry_can_be_claimed() {
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-ignored-{backend}"));
        let exe = build(&dir, "ignored", &second_signal_program(), backend);
        let mut run = Run::start_ignoring(&exe, "USR2");
        run.expect("ready");
        run.signal("USR2");
        run.command(b'r');
        run.expect("first=32");
        run.expect("closed=0");
        if cfg!(target_os = "linux") {
            // Ignored as it was entered: the signal does nothing.
            run.signal("USR2");
            std::thread::sleep(Duration::from_millis(300));
            assert!(run.child.try_wait().unwrap().is_none(), "{backend}: ignored, so it lives");
            // And an unrelated one that was never ignored still ends it.
            run.signal("TERM");
            let status = run.finish();
            assert!(killed_by(status, host_number("TERM")), "{backend}: {status:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A thread spawned after the claim inherits it: a `TERM` that arrives while
/// the thread runs is held for the program, not delivered to the thread.
/// (A thread that did not have it blocked would take the signal and end the
/// process.) `join` then `signals_pending` sees it.
fn inherit_program(spawn_first: bool) -> String {
    format!(
        r#"edition 6;
import std.io;
{SAY}{SAY_PAIR}
fn nap(ms: int) -> [poll] int {{
    match poller_new() {{
        Polling::Ok(p) => {{
            var poller = p;
            region a {{
                var ev = alloc_slice[a](2, 0);
                borrow mut poller as &!ph in {{ poller_wait(ph, ev, ms); }}
            }}
            poller_close(poller);
        }}
        Polling::Failed(e) => {{ }}
    }}
    return 0;
}}

fn main(world: World) -> [conc] int {{
    let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);
    release(ffi); release(fs); release(heap); release(args); release(net); release(clock);
    let claim = narrow(signals, "TERM");
    var status = 1;
    borrow mut io as &!i in {{
        borrow claim as &s in {{
            let body = nap;
            {}
        }}
    }}
    release(claim);
    release(io);
    return status;
}}
"#,
        if spawn_first {
            // The thread is running when the claim is asked for: refused with
            // EBUSY, nothing changed; once it is joined the claim is granted.
            r#"let t = spawn(700, body);
            match signals_watch(s) {
                Watching::Ok(x) => { signals_close(x); say(i, "during=granted\n"); }
                Watching::Failed(e) => { say_pair(i, "during=", e); }
            }
            say(i, "ready\n");
            join(t);
            match signals_watch(s) {
                Watching::Ok(x) => { signals_close(x); say(i, "after=granted\n"); status = 0; }
                Watching::Failed(e) => { say_pair(i, "after=", e); }
            }"#
        } else {
            r#"match signals_watch(s) {
                Watching::Ok(w) => {
                    var watch = w;
                    let t = spawn(900, body);
                    say(i, "ready\n");
                    join(t);
                    borrow mut watch as &!wh in {
                        say_pair(i, "mask=", signals_pending(wh));
                    }
                    signals_close(watch);
                    status = 0;
                }
                Watching::Failed(e) => { say_pair(i, "failed=", e); }
            }"#
        }
    )
}

#[test]
fn a_thread_spawned_after_the_claim_does_not_take_the_signal() {
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-inherit-{backend}"));
        let exe = build(&dir, "inherit", &inherit_program(false), backend);
        let mut run = Run::start(&exe);
        run.expect("ready");
        // The thread is napping; the signal must wait for the program.
        std::thread::sleep(Duration::from_millis(200));
        run.signal("TERM");
        run.expect("mask=8");
        let status = run.finish();
        assert_eq!(status.code(), Some(0), "{backend}: {status:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// While a spawned thread has not been joined a claim is refused with
/// `EBUSY` (16): that thread would not have the signals blocked. Nothing is
/// left changed -- after the `join` the same claim is granted.
#[test]
fn a_claim_is_refused_while_a_thread_runs_and_granted_after_the_join() {
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-busy-{backend}"));
        let exe = build(&dir, "busy", &inherit_program(true), backend);
        let mut run = Run::start(&exe);
        run.expect("during=16");
        run.expect("ready");
        run.expect("after=granted");
        let status = run.finish();
        assert_eq!(status.code(), Some(0), "{backend}: {status:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Signals are process-wide, so two claims over one signal would race to read
/// it: the second is `Failed(EBUSY)`, and closing the first frees it.
#[test]
fn a_signal_another_claim_holds_is_refused_until_it_is_closed() {
    let source = format!(
        r#"edition 6;
import std.io;
{SAY}{SAY_PAIR}
fn main(world: World) -> [] int {{
    let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);
    release(ffi); release(fs); release(heap); release(args); release(net); release(clock);
    let claim = narrow(signals, "INT,TERM");
    var status = 1;
    borrow mut io as &!i in {{
        borrow claim as &s in {{
            match signals_watch(s) {{
                Watching::Ok(first) => {{
                    match signals_watch(s) {{
                        Watching::Ok(second) => {{ signals_close(second); say(i, "second=granted\n"); }}
                        Watching::Failed(e) => {{ say_pair(i, "second=", e); }}
                    }}
                    signals_close(first);
                    match signals_watch(s) {{
                        Watching::Ok(third) => {{ signals_close(third); say(i, "third=granted\n"); status = 0; }}
                        Watching::Failed(e) => {{ say_pair(i, "third=", e); }}
                    }}
                }}
                Watching::Failed(e) => {{ say_pair(i, "first=", e); }}
            }}
        }}
    }}
    release(claim);
    release(io);
    return status;
}}
"#
    );
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-overlap-{backend}"));
        let exe = build(&dir, "overlap", &source, backend);
        let mut run = Run::start(&exe);
        run.expect("second=16");
        run.expect("third=granted");
        let status = run.finish();
        assert_eq!(status.code(), Some(0), "{backend}: {status:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---- refusals, and the authority report --------------------------------

/// `cancho check --output json` on a source: the rule it was refused under,
/// and the message, or `None` if it was accepted.
fn refusal(source: &str, tag: &str) -> Option<(String, String)> {
    let dir = scratch(tag);
    let path = dir.join("program.cho");
    std::fs::write(&path, source).expect("a writable fixture");
    let out = Command::new(BIN)
        .args(["check".as_ref(), path.as_os_str(), "--std".as_ref(), "--output".as_ref()])
        .arg("json")
        .output()
        .expect("the compiler runs");
    let _ = std::fs::remove_dir_all(&dir);
    if out.status.success() {
        return None;
    }
    let json = String::from_utf8_lossy(&out.stdout).into_owned();
    let field = |key: &str| {
        json.lines()
            .find_map(|line| line.trim().strip_prefix(&format!("\"{key}\": \"")))
            .map(|rest| rest.trim_end_matches("\",").to_owned())
            .unwrap_or_default()
    };
    Some((field("rule"), field("message")))
}

/// `main` with the whole of `Split` taken apart, leaving `signals`.
fn with_signals(body: &str) -> String {
    format!(
        "edition 6;\n\
         fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args); release(net);\n\
             release(clock);\n\
             {body}\n\
             return 0;\n\
         }}\n"
    )
}

fn narrowing_to(set: &str) -> String {
    with_signals(&format!("let claimed = narrow(signals, \"{set}\"); release(claimed);"))
}

/// Every signal that is not claimable is refused at compile time, under one
/// tag, with the reason it is refused for.
#[test]
fn every_unclaimable_signal_is_refused_with_its_rule() {
    let reasons = [
        ("cannot be caught", ["KILL", "STOP"].as_slice()),
        ("a fault, not a request", ["SEGV", "ILL", "BUS", "FPE", "ABRT", "TRAP", "SYS"].as_slice()),
        (
            "not claimable (yet)",
            [
                "PIPE", "CHLD", "CONT", "TSTP", "TTIN", "TTOU", "URG", "IO", "XCPU", "XFSZ",
                "VTALRM", "PROF", "STKFLT", "PWR",
            ]
            .as_slice(),
        ),
    ];
    for (reason, names) in reasons {
        for name in names {
            let (rule, message) = refusal(&narrowing_to(name), &format!("signals-refuse-{name}"))
                .unwrap_or_else(|| panic!("`{name}` should be refused"));
            assert_eq!(rule, "signal-not-claimable", "{name}: {message}");
            assert!(message.contains(reason), "{name}: {message}");
            assert!(message.contains(&format!("`{name}`")), "{name}: {message}");
        }
    }
    // One bad name refuses the set, wherever it is.
    for set in ["KILL,TERM", "TERM,KILL", "INT,SEGV,TERM"] {
        let (rule, _) = refusal(&narrowing_to(set), "signals-refuse-mixed").expect("refused");
        assert_eq!(rule, "signal-not-claimable", "{set}");
    }
}

/// A malformed set is refused under the same rule; a well-formed one is
/// accepted in any order, and answers its canonical type.
#[test]
fn a_malformed_set_is_refused_and_any_order_is_accepted() {
    for set in
        ["", "TERM,", ",TERM", "TERM,TERM", "INT, TERM", "SIGTERM", "term", "FOO", "TERM;INT"]
    {
        let (rule, message) = refusal(&narrowing_to(set), "signals-malformed")
            .unwrap_or_else(|| panic!("`{set}` should be refused"));
        assert_eq!(rule, "signal-not-claimable", "{set:?}: {message}");
    }
    for name in CLAIMABLE {
        assert!(refusal(&narrowing_to(name), "signals-claimable").is_none(), "{name}");
    }
    assert!(refusal(&narrowing_to("TERM,INT"), "signals-order").is_none());
    assert!(refusal(&narrowing_to(&CLAIMABLE.join(",")), "signals-all").is_none());
    // The type is the canonical one whichever order the set was written in.
    let written = "fn f[&s](sig: &s Signals(\"INT,TERM\")) -> [] int { return 0; }";
    let program = format!(
        "{}\n{written}",
        with_signals(
            "let claimed = narrow(signals, \"TERM,INT\"); borrow claimed as &s in { f(s); } release(claimed);"
        )
    );
    assert!(refusal(&program, "signals-canonical").is_none());
}

/// `narrow` attenuates and never widens: a program cannot grant itself a
/// signal it was not given, and a narrowing that grants nothing is refused.
#[test]
fn narrowing_a_signal_set_never_widens_it() {
    let widen = with_signals(
        "let one = narrow(signals, \"INT\"); let two = narrow(one, \"INT,TERM\"); release(two);",
    );
    let (rule, message) = refusal(&widen, "signals-widen").expect("refused");
    assert_eq!(rule, "capability-not-narrowable", "{message}");

    let other = with_signals(
        "let one = narrow(signals, \"INT\"); let two = narrow(one, \"TERM\"); release(two);",
    );
    let (rule, message) = refusal(&other, "signals-sideways").expect("refused");
    assert_eq!(rule, "capability-not-narrowable", "{message}");

    let same = with_signals(
        "let one = narrow(signals, \"INT,TERM\"); let two = narrow(one, \"TERM,INT\"); release(two);",
    );
    let (rule, message) = refusal(&same, "signals-same").expect("refused");
    assert_eq!(rule, "capability-not-narrowable", "{message}");

    // Narrowing to a subset, and again to a smaller one, is the point.
    let ok = with_signals(
        "let a = narrow(signals, \"HUP,INT,TERM\"); let b = narrow(a, \"INT,TERM\"); \
         let c = narrow(b, \"TERM\"); release(c);",
    );
    assert!(refusal(&ok, "signals-subset").is_none());
}

/// A `main` that splits the world and releases what `released` names, then
/// runs `body`, which must dispose of the rest.
fn main_with(released: &[&str], body: &str) -> String {
    let releases: Vec<String> = released.iter().map(|name| format!("release({name});")).collect();
    format!(
        "edition 6;\n\
         fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);\n\
             {}\n\
             {body}\n\
             return 0;\n\
         }}\n",
        releases.join(" ")
    )
}

const ALL_BUT_SIGNALS: [&str; 7] = ["io", "ffi", "fs", "heap", "args", "net", "clock"];

const CLAIM_AND_CLOSE: &str = "match signals_watch(CAP) { Watching::Ok(w) => { signals_close(w); } \
                               Watching::Failed(e) => { } }";

/// The root names no signal, so claiming through it is refused: the report
/// could only say "any". A claim through something that is not a `Signals`
/// is refused too, and so is one that does not borrow it.
#[test]
fn only_a_narrowed_borrowed_signals_can_claim() {
    let claim = CLAIM_AND_CLOSE.replace("CAP", "s");
    let root = main_with(
        &ALL_BUT_SIGNALS,
        &format!("borrow signals as &s in {{ {claim} }} release(signals);"),
    );
    let (rule, message) = refusal(&root, "signals-root").expect("refused");
    assert_eq!(rule, "capability-misused", "{message}");
    assert!(message.contains("narrow"), "{message}");

    let clock_claim = CLAIM_AND_CLOSE.replace("CAP", "c");
    let not_signals = main_with(
        &["io", "ffi", "fs", "heap", "args", "net", "signals"],
        &format!("borrow clock as &c in {{ {clock_claim} }} release(clock);"),
    );
    let (rule, message) = refusal(&not_signals, "signals-not-signals").expect("refused");
    assert_eq!(rule, "capability-misused", "{message}");

    let owned = CLAIM_AND_CLOSE.replace("CAP", "claim");
    let by_value = main_with(
        &ALL_BUT_SIGNALS,
        &format!("let claim = narrow(signals, \"TERM\"); {owned} release(claim);"),
    );
    let (rule, message) = refusal(&by_value, "signals-by-value").expect("refused");
    assert_eq!(rule, "capability-misused", "{message}");

    let narrowed = CLAIM_AND_CLOSE.replace("CAP", "s");
    let good = main_with(
        &ALL_BUT_SIGNALS,
        &format!(
            "let claim = narrow(signals, \"TERM\"); borrow claim as &s in {{ {narrowed} }} release(claim);"
        ),
    );
    assert!(refusal(&good, "signals-good").is_none());
}

/// The new names are edition 6's: a file below it cannot name them, and an
/// edition-5 `Split` is still the seven fields it was.
#[test]
fn the_signal_names_are_edition_six() {
    let five = "edition 5;\nfn main(world: World) -> [] int {\n\
                let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n\
                release(io); release(ffi); release(fs); release(heap); release(args);\n\
                release(net); release(clock);\n return 0;\n}\n";
    assert!(refusal(five, "signals-ed5-split").is_none(), "edition 5 keeps its Split");

    let early = five.replace("return 0;", "return signals_close(3);");
    let (rule, _) = refusal(&early, "signals-ed5-name").expect("refused");
    assert_eq!(rule, "not-a-function");

    // A seven-field pattern at edition 6 names too few.
    let seven = five.replace("edition 5;", "edition 6;");
    let (rule, message) = refusal(&seven, "signals-ed6-seven").expect("refused");
    assert_eq!(rule, "arity-mismatch", "{message}");
    assert!(message.contains("8 fields"), "{message}");

    // Edition 6 is edition 5 and more: the socket names are still there.
    let sockets = "edition 6;\nfn f() -> [] int { match poller_new() { Polling::Ok(p) => { poller_close(p); } \
                   Polling::Failed(e) => { } } return 0; }\n\
                   fn main(world: World) -> [] int {\n\
                   let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);\n\
                   release(io); release(ffi); release(fs); release(heap); release(args);\n\
                   release(net); release(clock); release(signals);\n return f();\n}\n";
    assert!(refusal(sockets, "signals-ed6-sockets").is_none());
}

/// A claim is a resource: it is closed exactly once, and a pattern cannot
/// take it apart. The row is exact in both directions.
#[test]
fn the_claim_is_linear_and_its_rows_are_exact() {
    let claim = |body: &str| {
        main_with(
            &ALL_BUT_SIGNALS,
            &format!(
                "let c = narrow(signals, \"TERM\"); borrow c as &s in {{ \
                 match signals_watch(s) {{ Watching::Ok(w) => {{ {body} }} Watching::Failed(e) => {{ }} }} }} release(c);"
            ),
        )
    };
    let (rule, _) = refusal(&claim(""), "signals-unclosed").expect("refused");
    assert_eq!(rule, "linear-value-unconsumed");
    let (rule, _) =
        refusal(&claim("signals_close(w); signals_close(w);"), "signals-twice").expect("refused");
    assert_eq!(rule, "linear-use-after-move");
    let (rule, message) =
        refusal(&claim("let SignalWatch { } = w;"), "signals-apart").expect("refused");
    assert_eq!(rule, "linear-value-taken-apart", "{message}");
    assert!(message.contains("signals_close"), "{message}");
    assert!(refusal(&claim("signals_close(w);"), "signals-closed").is_none());

    // A function handed the claim declares what it does with it.
    let undeclared = "edition 6;\nfn look[&w](watch: &!w SignalWatch) -> [] int { return signals_pending(watch); }\n\
                      fn main(world: World) -> [] int { release(world); return 0; }\n";
    let (rule, message) = refusal(undeclared, "signals-undeclared").expect("refused");
    assert_eq!(rule, "effect-not-declared", "{message}");
    assert!(message.contains("signals_read"), "{message}");
    let unperformed = "edition 6;\nfn look[&w](watch: &!w SignalWatch) -> [signals_read] int { return 0; }\n\
                       fn main(world: World) -> [] int { release(world); return 0; }\n";
    let (rule, _) = refusal(unperformed, "signals-unperformed").expect("refused");
    assert_eq!(rule, "effect-declared-not-performed");
    let declared = "edition 6;\nfn look[&w](watch: &!w SignalWatch) -> [signals_read] int { return signals_pending(watch); }\n\
                    fn main(world: World) -> [] int { release(world); return 0; }\n";
    assert!(refusal(declared, "signals-declared").is_none());

    // Owning one is stronger than borrowing it, and says so by having an
    // empty row: an owned claim discharges `signals_read`, an owned
    // `Signals` discharges claiming its set and reading what it claimed.
    let owned_watch = "edition 6;\nfn drain(watch: SignalWatch) -> [] int {\n\
                       var held = watch;\n\
                       borrow mut held as &!w in { signals_pending(w); }\n\
                       return signals_close(held);\n}\n\
                       fn main(world: World) -> [] int { release(world); return 0; }\n";
    assert!(refusal(owned_watch, "signals-owned-watch").is_none());
    let owned_caps = "edition 6;\nfn claim(sig: Signals(\"TERM\")) -> [] int {\n\
                      var status = 1;\n\
                      borrow sig as &s in {\n\
                          match signals_watch(s) {\n\
                              Watching::Ok(w) => { var held = w; borrow mut held as &!h in { signals_pending(h); } \
                                                   status = signals_close(held); }\n\
                              Watching::Failed(e) => { status = e; }\n\
                          }\n\
                      }\n\
                      release(sig);\n\
                      return status;\n}\n\
                      fn main(world: World) -> [] int { release(world); return 0; }\n";
    assert!(refusal(owned_caps, "signals-owned-cap").is_none());
    // The root, owned, covers every set: it need not be narrowed to be released.
    let owned_root = owned_caps.replace("sig: Signals(\"TERM\")", "sig: Signals(\"\")");
    assert!(refusal(&owned_root, "signals-owned-root").is_some(), "the root cannot claim");
    // A set written in a type is the canonical one, or it is refused.
    let wider = owned_caps.replace("Signals(\"TERM\")", "Signals(\"INT,TERM\")");
    assert!(refusal(&wider, "signals-owned-wider").is_none());
    let unordered = owned_caps.replace("Signals(\"TERM\")", "Signals(\"TERM,INT\")");
    let (rule, _) = refusal(&unordered, "signals-owned-unordered").expect("refused");
    assert_eq!(rule, "signal-not-claimable");

    // The set in a row is the set in the capability's type, exactly.
    let row = |declared: &str| {
        format!(
            "edition 6;\nfn claim[&s](sig: &s Signals(\"INT,TERM\")) -> [signals(\"{declared}\")] int {{ \
             match signals_watch(sig) {{ Watching::Ok(w) => {{ return signals_close(w); }} Watching::Failed(e) => {{ return e; }} }} }}\n\
             fn main(world: World) -> [] int {{ release(world); return 0; }}\n"
        )
    };
    assert!(refusal(&row("INT,TERM"), "signals-row-exact").is_none());
    for wrong in ["INT", "TERM", "HUP,INT,TERM", "TERM,INT"] {
        let (rule, _) = refusal(&row(wrong), "signals-row-wrong").expect("refused");
        assert!(
            rule == "effect-not-declared" || rule == "effect-declared-not-performed",
            "{wrong}: {rule}"
        );
    }
}

// ---- the authority report ----------------------------------------------

fn report_program(set: &str) -> String {
    format!(
        "edition 6;\n\
         fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args); release(net);\n\
             release(clock);\n\
             let claim = narrow(signals, \"{set}\");\n\
             var status = 1;\n\
             borrow claim as &s in {{\n\
                 match signals_watch(s) {{\n\
                     Watching::Ok(w) => {{ var watch = w; borrow mut watch as &!wh in {{ signals_pending(wh); }} \
                                           status = signals_close(watch); }}\n\
                     Watching::Failed(e) => {{ status = 2; }}\n\
                 }}\n\
             }}\n\
             release(claim);\n\
             return status;\n\
         }}\n"
    )
}

/// `cancho authority` names the signals a program claims, exactly, and the
/// report is bounded: the thing the `Ffi("libc")` workaround could not say.
#[test]
fn the_authority_report_names_the_set_and_is_bounded() {
    let json = authority_json(&report_program("TERM,INT"), "signals-authority");
    assert!(json.contains("\"bounded\": true"), "{json}");
    assert!(
        json.contains("{ \"name\": \"signals\", \"argument\": \"INT,TERM\", \"bounded\": true }"),
        "{json}"
    );
    assert!(
        json.contains("{ \"name\": \"signals_read\", \"argument\": null, \"bounded\": true }"),
        "{json}"
    );
    assert!(!json.contains("\"ffi\""), "no foreign code: {json}");
    assert!(json.contains("\"foreign_symbols\": []"), "{json}");

    // The set is what was claimed: a different set is a different report.
    let all = authority_json(&report_program(&CLAIMABLE.join(",")), "signals-authority-all");
    assert!(all.contains(&format!("\"argument\": \"{}\"", CLAIMABLE.join(","))), "{all}");
    let one = authority_json(&report_program("HUP"), "signals-authority-one");
    assert!(one.contains("\"argument\": \"HUP\""), "{one}");
    assert!(!one.contains("TERM"), "{one}");
}

/// The text report says so too, and a program that claims nothing says it
/// never touches signals.
#[test]
fn the_text_report_says_what_is_claimed_and_what_is_not() {
    let dir = scratch("signals-authority-text");
    let claims = dir.join("claims.cho");
    std::fs::write(&claims, report_program("INT,TERM")).unwrap();
    let silent = dir.join("silent.cho");
    std::fs::write(
        &silent,
        main_with(&["io", "ffi", "fs", "heap", "args", "net", "clock", "signals"], ""),
    )
    .unwrap();
    let text = |path: &Path| {
        let out = Command::new(BIN)
            .args(["authority".as_ref(), path.as_os_str(), "--std".as_ref()])
            .output()
            .expect("the compiler runs");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let claims = text(&claims);
    assert!(claims.contains("signals(\"INT,TERM\")"), "{claims}");
    assert!(!claims.contains("UNBOUNDED"), "{claims}");
    assert!(
        !claims.contains("    signals\n"),
        "signals are claimed, so not 'never touches': {claims}"
    );
    let silent = text(&silent);
    assert!(silent.contains("never touches"), "{silent}");
    assert!(silent.contains("    signals\n"), "{silent}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The bits `std.signals` names are the ones `signals_pending` answers, and
/// its predicates mean what they say.
#[test]
fn std_signals_names_the_bits() {
    let source = r#"
import std.signals;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    var score = 0;
    if signals.sighup() == 1 && signals.sigint() == 2 && signals.sigquit() == 4 { score = score + 1; }
    if signals.sigterm() == 8 && signals.sigusr1() == 16 && signals.sigusr2() == 32 { score = score + 1; }
    if signals.sigalrm() == 64 && signals.sigwinch() == 128 { score = score + 1; }
    let mask = signals.sigterm() | signals.sigusr1();
    if signals.has(mask, signals.sigterm()) && !signals.has(mask, signals.sigint()) { score = score + 1; }
    if signals.has(mask, mask) && !signals.has(mask, mask | 1) { score = score + 1; }
    if signals.any(mask, signals.stop_signals()) && !signals.any(signals.sigusr1(), signals.stop_signals()) { score = score + 1; }
    if signals.any(signals.sigint(), signals.stop_signals()) && signals.any(signals.sigquit(), signals.stop_signals()) { score = score + 1; }
    if !signals.any(0, signals.stop_signals()) && signals.has(0, 0) { score = score + 1; }
    return score;
}
"#;
    for backend in BACKENDS {
        let dir = scratch(&format!("signals-std-{backend}"));
        let exe = build(&dir, "stdsignals", source, backend);
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), Some(8), "{backend}: eight checks pass");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
