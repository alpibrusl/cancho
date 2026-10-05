//! Processes (`docs/processes.md`, slice 1): `exec_spawn`, `child_wait`,
//! `child_kill` and the pipe verbs, over real processes, on both backends.
//!
//! `PROBE` holds `Exec("")` and takes one case from its command line: a
//! program, its arguments, its environment and its standard input, and a mode.
//! It copies the child's output to its own and ends with a line saying how the
//! child ended. `TRAPS` holds `Exec("/bin")` and makes one call per mode that
//! must trap, and one that must not.

use super::*;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

const PROBE: &str = r##"edition 7;

// argv: path, args, env, stdin, mode. A list is `|`-separated ("-" is
// empty); stdin "-" is `Stdio::Null`, anything else is written to a pipe.
// The child's standard output is copied to ours, then one line says how the
// child ended: `== code N`, `== signal B`, `== failed E`, `== spawn E`.

fn digits[&i](io: &!i Io, n: int) -> [io_write] int {
    if n < 0 {
        putchar(io, '-');
        return digits(io, 0 - n);
    }
    if n >= 10 {
        digits(io, n / 10);
    }
    putchar(io, '0' + n % 10);
    return 0;
}

fn say[&i, &t](io: &!i Io, text: &t [byte], n: int) -> [io_write] int {
    write_bytes(io, text);
    digits(io, n);
    putchar(io, 10);
    return 0;
}

// `a|b` as `a\0b\0`; `-` as nothing.
fn list[&s, &o](from: &s [byte], into: &!o [byte]) -> [] int {
    if len(from) == 1 && from[0] == byte_of('-') {
        return 0;
    }
    var i = 0;
    while i < len(from) {
        if from[i] == byte_of('|') {
            into[i] = byte_of(0);
        } else {
            into[i] = from[i];
        }
        i = i + 1;
    }
    into[i] = byte_of(0);
    return i + 1;
}

fn drain[&i](io: &!i Io, from: Pipe) -> [io_write] int {
    var reader = from;
    region a {
        let buf = alloc_slice[a](4096, byte_of(0));
        var going = true;
        while going {
            borrow mut reader as &!r in {
                match pipe_read(r, buf) {
                    Received::Data(n) => { write_bytes(io, buf[0..n]); }
                    Received::End => { going = false; }
                    Received::Again => { }
                    Received::Failed(e) => { going = false; }
                }
            }
        }
    }
    return pipe_close(reader);
}

// Write `text` to the child unless it was given `/dev/null`, and close our end.
fn feed[&i, &t](io: &!i Io, to: Pipe, text: &t [byte], wanted: bool) -> [io_write] int {
    var writer = to;
    if wanted {
        borrow mut writer as &!w in {
            match pipe_write(w, text) {
                Sent::Wrote(n) => { }
                Sent::Again => { }
                Sent::Failed(e) => { say(io, "== write ", e); }
            }
        }
    }
    return pipe_close(writer);
}

fn ended[&i](io: &!i Io, child: Child) -> [io_write] int {
    match child_wait(child) {
        Exited::Code(n) => { say(io, "== code ", n); }
        Exited::Signaled(s) => { say(io, "== signal ", s); }
        Exited::Failed(e) => { say(io, "== failed ", e); }
    }
    return 0;
}

fn run[&x, &i, &g, &s](exec: &x Exec(""), io: &!i Io, g: &g Args, signals: &s Signals("TERM"))
    -> [exec(""), io_write, args, child_signal, signals("TERM")] int {
    let mode = arg(g, 5);
    region a {
        let argv = alloc_slice[a](len(arg(g, 2)) + 1, byte_of(0));
        let envp = alloc_slice[a](len(arg(g, 3)) + 1, byte_of(0));
        let na = list(arg(g, 2), argv);
        let ne = list(arg(g, 3), envp);
        let text = arg(g, 4);
        match pipe_open() {
            Piped::Failed(e) => { say(io, "== pipe ", e); }
            Piped::Ok(out, out_end) => {
                match pipe_open() {
                    Piped::Failed(e) => { say(io, "== pipe ", e); pipe_close(out); child_end_close(out_end); }
                    Piped::Ok(input, in_end) => {
                        var stdin = Stdio::Pipe(in_end);
                        var feed_it = true;
                        if len(text) == 1 && text[0] == byte_of('-') {
                            match stdin {
                                Stdio::Pipe(unused) => { child_end_close(unused); }
                                Stdio::Null => { }
                                Stdio::File(unused) => { file_close(unused); }
                            }
                            stdin = Stdio::Null;
                            feed_it = false;
                        }
                        // `TERM` claimed: blocked here, and default in the child (§4.6).
                        var watch_held = false;
                        match signals_watch(signals) {
                            Watching::Ok(w) => {
                                match exec_spawn(exec, arg(g, 1), argv[0..na], envp[0..ne], stdin, Stdio::Pipe(out_end), Stdio::Null) {
                                    Spawned::Failed(e) => { say(io, "== spawn ", e); pipe_close(out); pipe_close(input); }
                                    Spawned::Ok(c) => {
                                        var child = c;
                                        var writer = input;
                                        if len(mode) == 4 && mode[0] == byte_of('k') {
                                            // kill: KILL by its bit.
                                            borrow child as &ch in { say(io, "== kill ", child_kill(ch, 256)); }
                                        }
                                        if len(mode) == 4 && mode[0] == byte_of('t') {
                                            // term: TERM, which this process holds blocked.
                                            borrow child as &ch in { say(io, "== kill ", child_kill(ch, 8)); }
                                        }
                                        if len(mode) == 3 && mode[0] == byte_of('b') {
                                            // bad: not a signal's bit.
                                            borrow child as &ch in { say(io, "== kill ", child_kill(ch, 3)); }
                                        }
                                        // late: the child is reaped before anything is written to
                                        // it; the write answers `EPIPE`. Reaped, not only drained:
                                        // the end of its output says its standard output is closed,
                                        // and Linux may close its standard input after that.
                                        if len(mode) == 4 && mode[0] == byte_of('l') {
                                            drain(io, out);
                                            ended(io, child);
                                            feed(io, writer, text, feed_it);
                                        } else {
                                            feed(io, writer, text, feed_it);
                                            drain(io, out);
                                            ended(io, child);
                                        }
                                    }
                                }
                                signals_close(w);
                            }
                            Watching::Failed(e) => {
                                say(io, "== watch ", e);
                                pipe_close(out);
                                pipe_close(input);
                                child_end_close(out_end);
                                match stdin {
                                    Stdio::Pipe(unused) => { child_end_close(unused); }
                                    Stdio::Null => { }
                                    Stdio::File(unused) => { file_close(unused); }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    return 0;
}

// Hold `n` more channels (two descriptors each) while the child is started,
// so a descriptor that crossed would show past the child's own few.
fn with_pipes[&x, &i, &g, &s](n: int, exec: &x Exec(""), io: &!i Io, g: &g Args, signals: &s Signals("TERM"))
    -> [exec(""), io_write, args, child_signal, signals("TERM")] int {
    if n == 0 {
        return run(exec, io, g, signals);
    }
    match pipe_open() {
        Piped::Ok(mine, theirs) => {
            let r = with_pipes(n - 1, exec, io, g, signals);
            pipe_close(mine);
            child_end_close(theirs);
            return r;
        }
        Piped::Failed(e) => { return run(exec, io, g, signals); }
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi); release(fs); release(heap); release(net); release(clock);
    let term = narrow(signals, "TERM");
    var console = io;
    borrow exec as &x in {
        borrow mut console as &!i in {
            borrow args as &g in {
                borrow term as &s in {
                    var extra = 0;
                    if int_of(arg(g, 5)[0]) == 'm' {
                        extra = 4;
                    }
                    with_pipes(extra, x, i, g, s);
                }
            }
        }
    }
    release(exec); release(console); release(args); release(term);
    return 0;
}
"##;

/// The waiter: one child under `/bin/sh -c`, watched through a `Poller`
/// (`docs/processes.md` §4.8). argv: a mode, and the shell's arguments
/// (`-c|script`).
///   `a`: the pipe and the child are both registered; says what `poller_wait`
///        reported while the child slept, then loops until the pipe ended and
///        the child's exit was reported.
///   `b`: the child has already ended when it is registered.
///   `c`: the child outlives a deadline, is killed, and its exit is reported.
///   `d`: sixty children are started and reaped first, then `b`.
///   `e`: only the registration of the child is attempted, and its answer said.
const WAITER: &str = r##"edition 7;

fn digits[&i](io: &!i Io, n: int) -> [io_write] int {
    if n < 0 {
        putchar(io, '-');
        return digits(io, 0 - n);
    }
    if n >= 10 {
        digits(io, n / 10);
    }
    putchar(io, '0' + n % 10);
    return 0;
}

fn say[&i, &t](io: &!i Io, text: &t [byte], n: int) -> [io_write] int {
    write_bytes(io, text);
    digits(io, n);
    putchar(io, 10);
    return 0;
}

fn list[&s, &o](from: &s [byte], into: &!o [byte]) -> [] int {
    var i = 0;
    while i < len(from) {
        if from[i] == byte_of('|') {
            into[i] = byte_of(0);
        } else {
            into[i] = from[i];
        }
        i = i + 1;
    }
    into[i] = byte_of(0);
    return i + 1;
}

fn reaped[&i](io: &!i Io, child: Child) -> [io_write] int {
    match child_wait(child) {
        Exited::Code(n) => { say(io, "== code ", n); }
        Exited::Signaled(s) => { say(io, "== signal ", s); }
        Exited::Failed(e) => { say(io, "== failed ", e); }
    }
    return 0;
}

// Start and reap `n` children that hold nothing: a program that leaked a
// descriptor for each would have none left.
fn churn[&x, &i](exec: &x Exec(""), io: &!i Io, n: int) -> [exec(""), io_write] int {
    var k = 0;
    var ok = 0;
    while k < n {
        match exec_spawn(exec, "/bin/sh", "-c\0exit 0\0", "", Stdio::Null, Stdio::Null, Stdio::Null) {
            Spawned::Ok(c) => {
                match child_wait(c) {
                    Exited::Code(z) => { if z == 0 { ok = ok + 1; } }
                    Exited::Signaled(s) => { }
                    Exited::Failed(e) => { }
                }
            }
            Spawned::Failed(e) => { }
        }
        k = k + 1;
    }
    say(io, "churned ", ok);
    return 0;
}

// The poller takes the last free descriptor; a child whose three streams are
// `Null` gives none back to the parent, so nothing is left for its `pidfd`.
fn full[&x, &i](exec: &x Exec(""), io: &!i Io) -> [exec(""), io_write, poll] int {
    match poller_new() {
        Polling::Failed(e) => { say(io, "== poller ", e); }
        Polling::Ok(p) => {
            var poller = p;
            match exec_spawn(exec, "/bin/sh", "-c\0exit 5\0", "", Stdio::Null, Stdio::Null, Stdio::Null) {
                Spawned::Failed(e) => { say(io, "== spawn ", e); }
                Spawned::Ok(c) => {
                    var child = c;
                    borrow mut poller as &!ph in {
                        borrow child as &ch in {
                            say(io, "add child ", poller_add_child(ph, ch, 2));
                        }
                    }
                    reaped(io, child);
                }
            }
            poller_close(poller);
        }
    }
    return 0;
}

fn watch[&x, &i, &g](exec: &x Exec(""), io: &!i Io, g: &g Args, mode: int)
    -> [exec(""), io_write, args, child_signal, poll, pipe_read] int {
    region a {
        let argv = alloc_slice[a](len(arg(g, 2)) + 1, byte_of(0));
        let na = list(arg(g, 2), argv);
        let buf = alloc_slice[a](64, byte_of(0));
        var ev = alloc_slice[a](8, 0);
        // The poller first, so a tight descriptor limit is the `pidfd`'s to hit.
        match poller_new() {
            Polling::Failed(e) => { say(io, "== poller ", e); }
            Polling::Ok(p) => {
                var poller = p;
                match pipe_open() {
                    Piped::Failed(e) => { say(io, "== pipe ", e); poller_close(poller); }
                    Piped::Ok(out, out_end) => {
                        match exec_spawn(exec, "/bin/sh", argv[0..na], "", Stdio::Null, Stdio::Pipe(out_end), Stdio::Null) {
                            Spawned::Failed(e) => { say(io, "== spawn ", e); pipe_close(out); poller_close(poller); }
                            Spawned::Ok(c) => {
                                var child = c;
                                var mine = out;
                                borrow mut poller as &!ph in {
                                    borrow mut mine as &!pp in {
                                        borrow child as &ch in {
                                            if mode == 'a' {
                                                pipe_nonblocking(pp);
                                                say(io, "add pipe ", poller_add_pipe(ph, pp, 1, 1));
                                                say(io, "add child ", poller_add_child(ph, ch, 2));
                                                say(io, "idle ", poller_wait(ph, ev, 150));
                                                var bytes = 0;
                                                var ended = false;
                                                var gone = false;
                                                var going = true;
                                                while going {
                                                    let n = poller_wait(ph, ev, 10000);
                                                    if n <= 0 {
                                                        say(io, "== timeout ", n);
                                                        going = false;
                                                    }
                                                    var k = 0;
                                                    while k < n {
                                                        let token = ev[2 * k];
                                                        if token == 1 {
                                                            var more = true;
                                                            while more {
                                                                match pipe_read(pp, buf) {
                                                                    Received::Data(m) => { bytes = bytes + m; }
                                                                    Received::End => { ended = true; more = false; }
                                                                    Received::Again => { more = false; }
                                                                    Received::Failed(e) => { ended = true; more = false; }
                                                                }
                                                            }
                                                        }
                                                        if token == 2 {
                                                            gone = true;
                                                        }
                                                        k = k + 1;
                                                    }
                                                    if ended && gone {
                                                        going = false;
                                                    }
                                                }
                                                say(io, "bytes ", bytes);
                                                if ended { say(io, "pipe ended ", 1); }
                                                if gone { say(io, "child reported ", 1); }
                                            }
                                            if mode == 'b' {
                                                say(io, "nap ", poller_wait(ph, ev, 300));
                                                say(io, "add child ", poller_add_child(ph, ch, 2));
                                                let n = poller_wait(ph, ev, 5000);
                                                say(io, "woke ", n);
                                                say(io, "token ", ev[0]);
                                                say(io, "events ", ev[1]);
                                            }
                                            if mode == 'e' {
                                                say(io, "add child ", poller_add_child(ph, ch, 2));
                                            }
                                            if mode == 'c' {
                                                say(io, "add child ", poller_add_child(ph, ch, 2));
                                                say(io, "early ", poller_wait(ph, ev, 150));
                                                say(io, "kill ", child_kill(ch, 256));
                                                let n = poller_wait(ph, ev, 5000);
                                                say(io, "woke ", n);
                                                say(io, "token ", ev[0]);
                                                say(io, "events ", ev[1]);
                                            }
                                        }
                                    }
                                }
                                reaped(io, child);
                                pipe_close(mine);
                                poller_close(poller);
                            }
                        }
                    }
                }
            }
        }
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi); release(fs); release(heap); release(net); release(clock); release(signals);
    var console = io;
    borrow exec as &x in {
        borrow mut console as &!i in {
            borrow args as &g in {
                var mode = int_of(arg(g, 1)[0]);
                if mode == 'd' {
                    churn(x, i, 60);
                    mode = 'b';
                }
                if mode == 'f' {
                    full(x, i);
                } else {
                    watch(x, i, g, mode);
                }
            }
        }
    }
    release(exec); release(console); release(args);
    return 0;
}
"##;

const TRAPS: &str = r##"edition 7;

// argv[1] picks one call; every refusal must trap (§4.1 to §4.3), and `ok`
// must not.
fn spawn_one[&x, &p, &a, &e](exec: &x Exec("/bin"), path: &p [byte], args: &a [byte], env: &e [byte])
    -> [exec("/bin")] int {
    match exec_spawn(exec, path, args, env, Stdio::Null, Stdio::Null, Stdio::Null) {
        Spawned::Ok(child) => {
            match child_wait(child) {
                Exited::Code(n) => { return n; }
                Exited::Signaled(s) => { return 100; }
                Exited::Failed(e) => { return 101; }
            }
        }
        Spawned::Failed(e) => { return 102; }
    }
}

fn pick[&x, &g](exec: &x Exec("/bin"), g: &g Args) -> [exec("/bin"), args] int {
    let mode = arg(g, 1);
    let c = int_of(mode[0]);
    if c == 'o' { return spawn_one(exec, "/bin/sh", "-c\0exit 0\0", "PATH=/bin\0"); }
    if c == 'u' { return spawn_one(exec, "/usr/bin/true", "", ""); }
    if c == 'd' { return spawn_one(exec, "/bin/../usr/bin/true", "", ""); }
    if c == 's' { return spawn_one(exec, "/binx/true", "", ""); }
    if c == 'l' { return spawn_one(exec, "/bin/sh", "-c\0exit 0\0", "A=1\0LD_PRELOAD=/tmp/x.so\0"); }
    if c == 'y' { return spawn_one(exec, "/bin/sh", "-c\0exit 0\0", "DYLD_INSERT_LIBRARIES=/tmp/x\0"); }
    if c == 'a' { return spawn_one(exec, "/bin/sh", "-c\0exit 0", ""); }
    if c == 'e' { return spawn_one(exec, "/bin/sh", "-c\0exit 0\0", "A=1"); }
    if c == 'n' {
        return spawn_one(exec, "/bin/sh", "-c\0/bin/echo out || exit 9; /bin/echo err >&2 || exit 8\0", "");
    }
    return 50;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(net); release(clock); release(signals);
    let bin = narrow(exec, "/bin");
    var status = 0;
    borrow bin as &x in {
        borrow args as &g in {
            status = pick(x, g);
        }
    }
    release(bin); release(args);
    return status;
}
"##;

/// Build `source` with each backend, answering `(backend, executable)`.
fn build_both(tag: &str, source: &str) -> (PathBuf, Vec<(&'static str, PathBuf)>) {
    let dir = scratch(tag);
    let file = dir.join(format!("{tag}.ls"));
    std::fs::write(&file, source).expect("the program is written");
    let mut built = Vec::new();
    for backend in BACKENDS {
        let exe = dir.join(format!("{tag}-{backend}"));
        let build = Command::new(BIN)
            .arg("build")
            .arg(&file)
            .args(["--backend", backend, "-o"])
            .arg(&exe)
            .output()
            .expect("the compiler runs");
        assert!(
            build.status.success(),
            "`--backend {backend}` should build `{tag}`:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
        built.push((backend, exe));
    }
    (dir, built)
}

/// The probe, built once for every test here: the tests run in parallel, and
/// each building its own into one scratch directory would remove another's.
fn probe_built() -> &'static [(&'static str, PathBuf)] {
    static BUILT: std::sync::OnceLock<Vec<(&'static str, PathBuf)>> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| build_both("process-probe", PROBE).1)
}

/// Run the probe on one case, on both backends, and require the two answers to
/// be the same; answer it.
fn probe(case: [&str; 5]) -> String {
    probe_holding(false, case)
}

/// `probe`, started -- when `inherited` -- holding descriptor 9 open without
/// close-on-exec, as a program started by a careless parent would: only
/// `closefrom` (Linux) or `POSIX_SPAWN_CLOEXEC_DEFAULT` (macOS) keeps it from
/// the child (§4.5).
fn probe_holding(inherited: bool, case: [&str; 5]) -> String {
    let mut answers = Vec::new();
    for (backend, exe) in probe_built() {
        let run = if inherited {
            Command::new("/bin/sh")
                .args(["-c", "exec 9</dev/null; exec \"$0\" \"$@\""])
                .arg(exe)
                .args(case)
                .output()
        } else {
            Command::new(exe).args(case).output()
        };
        let run = run.expect("the probe runs");
        assert_eq!(run.status.code(), Some(0), "`{backend}`: the probe itself should not fail");
        answers.push(String::from_utf8_lossy(&run.stdout).into_owned());
    }
    assert_eq!(answers[0], answers[1], "the two backends disagree on {case:?}");
    answers.remove(0)
}

/// §4.2, §4.3: the child's arguments and environment are exactly what was
/// passed -- an empty argument, a space inside one, and nothing inherited.
#[test]
fn a_child_gets_exactly_the_arguments_and_environment_passed() {
    let printed = probe(["/usr/bin/printf", "%s\\n|a|b c||end", "-", "-", "plain"]);
    assert_eq!(printed, "a\nb c\n\nend\n== code 0\n");
    let printed = probe(["/usr/bin/env", "-", "A=1|B=two", "-", "plain"]);
    assert_eq!(printed, "A=1\nB=two\n== code 0\n");
    // This test runs with an environment of its own; none of it reaches the child.
    let printed = probe(["/usr/bin/env", "-", "-", "-", "plain"]);
    assert_eq!(printed, "== code 0\n");
}

/// §4.4: a channel carries the parent's bytes to the child and back.
#[test]
fn a_child_reads_what_the_parent_writes() {
    assert_eq!(probe(["/bin/cat", "-", "-", "fed through", "plain"]), "fed through== code 0\n");
}

/// §4.5: holding eight more descriptors of its own, and one it inherited
/// without close-on-exec, the parent starts a child that holds its three
/// streams and nothing else. `ls` opens a descriptor of its own to read
/// `/dev/fd`, so `3` is listed, and on macOS (observed on CI) `4` too; a leaked one of the parent's would be 5 or higher.
#[test]
fn a_child_holds_exactly_its_three_streams() {
    let printed = probe_holding(true, ["/bin/sh", "-c|ls /dev/fd", "-", "-", "many"]);
    // Only `ls`'s own lines: the status line after them ends in a number too.
    let listing = printed.split("== ").next().unwrap_or("");
    let seen: Vec<u32> = listing.split_whitespace().filter_map(|w| w.parse().ok()).collect();
    assert!(seen.starts_with(&[0, 1, 2]), "the child should hold its streams: {printed}");
    assert!(
        seen.iter().all(|&fd| fd <= 4) && seen.len() <= 5,
        "the child holds descriptors it was not given: {printed}"
    );
    assert!(printed.ends_with("== code 0\n"), "{printed}");
}

/// §4.7: `KILL` by its bit ends a child as `Signaled(256)`; a bit that names no
/// signal is `EINVAL` with no call. §4.6: `TERM` ends a child although the
/// parent holds `TERM` claimed (blocked on Linux, ignored on macOS).
#[test]
fn a_child_is_killed_by_its_bit_and_starts_with_signals_at_their_defaults() {
    assert_eq!(probe(["/bin/sleep", "30", "-", "-", "kill"]), "== kill 0\n== signal 256\n");
    assert_eq!(probe(["/bin/sleep", "30", "-", "-", "term"]), "== kill 0\n== signal 8\n");
    assert_eq!(probe(["/bin/sleep", "0", "-", "-", "bad"]), "== kill 22\n== code 0\n");
}

/// §4.1, §4.7: a missing program is an outcome (`ENOENT`), and an exit code is
/// reported as itself.
#[test]
fn a_missing_program_is_failed_and_an_exit_code_is_reported() {
    assert_eq!(probe(["/nonexistent/program", "-", "-", "-", "plain"]), "== spawn 2\n");
    assert_eq!(probe(["/bin/sh", "-c|exit 3", "-", "-", "plain"]), "== code 3\n");
}

/// §4.4: writing to a child that has ended is `Sent::Failed(EPIPE)`, and the
/// parent lives to say so -- a socket pair, not a `pipe(2)`, so no `SIGPIPE`.
#[test]
fn writing_to_a_child_that_has_ended_is_epipe_not_a_signal() {
    assert_eq!(
        probe(["/usr/bin/true", "-", "-", "never read", "late"]),
        "== code 0\n== write 32\n"
    );
}

/// The waiter built once, as the probe is.
fn waiter_built() -> &'static [(&'static str, PathBuf)] {
    static BUILT: std::sync::OnceLock<Vec<(&'static str, PathBuf)>> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| build_both("process-waiter", WAITER).1)
}

/// Run the waiter on both backends; they must agree.
fn waiter(mode: &str, script: &str) -> String {
    let mut answers = Vec::new();
    for (backend, exe) in waiter_built() {
        let run = Command::new(exe).args([mode, &format!("-c|{script}")]).output();
        let run = run.expect("the waiter runs");
        assert_eq!(run.status.code(), Some(0), "`{backend}`: the waiter itself should not fail");
        answers.push(String::from_utf8_lossy(&run.stdout).into_owned());
    }
    assert_eq!(answers[0], answers[1], "the two backends disagree on {mode} {script}");
    answers.remove(0)
}

/// §4.8: a channel and a child watched together. While the child sleeps
/// nothing is reported; then its output arrives, the channel ends, and the
/// child's exit is reported, after which `child_wait` does not block.
#[test]
fn a_poller_reports_a_childs_output_and_its_exit() {
    let printed = waiter("a", "sleep 1; echo hi");
    assert_eq!(
        printed,
        "add pipe 0\nadd child 0\nidle 0\nbytes 3\npipe ended 1\nchild reported 1\n== code 0\n"
    );
}

/// §4.8: a child that ended before it was registered is reported at once --
/// a `pidfd` stays readable; macOS is asked for the exit of a process that is
/// already a zombie.
#[test]
fn a_child_that_has_already_ended_is_still_reported() {
    let printed = waiter("b", "exit 7");
    assert_eq!(printed, "nap 0\nadd child 0\nwoke 1\ntoken 2\nevents 1\n== code 7\n");
}

/// §4.8: `poller_wait`'s deadline passes with the child still running; a kill
/// then ends it, and the exit is reported.
#[test]
fn a_deadline_passes_and_a_kill_is_reported() {
    let printed = waiter("c", "sleep 30");
    assert_eq!(printed, "add child 0\nearly 0\nkill 0\nwoke 1\ntoken 2\nevents 1\n== signal 256\n");
}

/// §4.8: the `pidfd` a child carries is closed when it is reaped. Under a
/// limit of 24 descriptors, sixty children come and go and the next is still
/// watchable; one `pidfd` leaked per child would leave none to watch it with,
/// and `poller_add_child` would answer `EMFILE` (24).
#[test]
fn reaping_a_child_gives_back_its_pidfd() {
    for (backend, exe) in waiter_built() {
        let run = Command::new("/bin/sh")
            .args(["-c", "ulimit -n 24; exec \"$0\" \"$@\""])
            .arg(exe)
            .args(["d", "-c|exit 0"])
            .output()
            .expect("the waiter runs");
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "churned 60\nnap 0\nadd child 0\nwoke 1\ntoken 2\nevents 1\n== code 0\n",
            "`{backend}`"
        );
    }
}

/// §4.8: `EMFILE` reaches the `pidfd` when the spawn gave no descriptor back --
/// every stream `Null` -- and none was free. Under a limit of 8 the shell holds
/// 4 to 7, the poller takes 3, and Linux has nothing left for `pidfd_open`; the
/// child is still started and reaped. Darwin watches the pid and needs no
/// descriptor.
#[test]
fn a_child_with_no_descriptor_to_spare_says_emfile() {
    let expected = if cfg!(target_os = "linux") { "add child 24\n" } else { "add child 0\n" };
    for (backend, exe) in waiter_built() {
        let run = Command::new("/bin/sh")
            .args([
                "-c",
                "ulimit -n 8; exec 4</dev/null 5</dev/null 6</dev/null 7</dev/null; exec \"$0\" f",
            ])
            .arg(exe)
            .output()
            .expect("the waiter runs");
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            format!("{expected}== code 5\n"),
            "`{backend}`"
        );
    }
}

/// A kernel without `pidfd_open` (before Linux 5.3), stood in for: a
/// `seccomp` filter answering it with `ENOSYS`, installed in `command`'s
/// process before it starts. Also used by `capture.rs`.
#[cfg(target_os = "linux")]
pub(super) fn without_pidfd_open(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    #[repr(C)]
    struct Filter {
        code: u16,
        jump_true: u8,
        jump_false: u8,
        k: u32,
    }
    #[repr(C)]
    struct Program {
        length: u16,
        filter: *const Filter,
    }
    unsafe extern "C" {
        fn prctl(option: i32, a2: u64, a3: u64, a4: u64, a5: u64) -> i32;
    }
    const PR_SET_NO_NEW_PRIVS: i32 = 38;
    const PR_SET_SECCOMP: i32 = 22;
    const SECCOMP_MODE_FILTER: u64 = 2;
    // Load the system call number; if it is `pidfd_open`, fail with `ENOSYS`,
    // otherwise allow it.
    const BPF_LD_W_ABS: u16 = 0x20;
    const BPF_JEQ_K: u16 = 0x15;
    const BPF_RET_K: u16 = 0x06;
    const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
    const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
    static FILTER: [Filter; 4] = [
        Filter { code: BPF_LD_W_ABS, jump_true: 0, jump_false: 0, k: 0 },
        Filter { code: BPF_JEQ_K, jump_true: 0, jump_false: 1, k: 434 },
        Filter { code: BPF_RET_K, jump_true: 0, jump_false: 0, k: SECCOMP_RET_ERRNO | 38 },
        Filter { code: BPF_RET_K, jump_true: 0, jump_false: 0, k: SECCOMP_RET_ALLOW },
    ];
    struct Shared(Program);
    // SAFETY: the program points at a `static` that is never written.
    unsafe impl Sync for Shared {}
    static PROGRAM: Shared = Shared(Program { length: 4, filter: FILTER.as_ptr() });

    // SAFETY: `prctl` is async-signal-safe and allocates nothing, which is
    // what `pre_exec` requires of the closure it runs between `fork` and
    // `exec`; the filter is the `static` above.
    unsafe {
        command.pre_exec(|| {
            let program = std::ptr::addr_of!(PROGRAM.0) as u64;
            if prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == 0
                && prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, program, 0, 0) == 0
            {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
}

/// §4.8: a program whose kernel will not give the child a `pidfd` is told why
/// when it asks to watch the child -- the `errno`, here `ENOSYS` (38) as on a
/// Linux before 5.3 -- and the child is still started and reaped
/// (`without_pidfd_open` stands in for that kernel).
#[cfg(target_os = "linux")]
#[test]
fn a_child_without_a_pidfd_says_why_it_cannot_be_watched() {
    for (backend, exe) in waiter_built() {
        let mut command = Command::new(exe);
        command.args(["e", "-c|exit 5"]);
        without_pidfd_open(&mut command);
        let run = command.output().expect("the waiter runs");
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "add child 38\n== code 5\n",
            "`{backend}`"
        );
    }
}

/// §4.1 to §4.3: a path outside the bound, `..`, a sibling of the bound, a
/// loader variable and a list that does not end in `\\0` each trap -- and an
/// ordinary call under the bound does not. A trap is `SIGILL` or `SIGTRAP`
/// (`brk` on aarch64); a crash, such as reading past an unterminated list, is
/// not one. §4.4: a child writes to `Stdio::Null` as to any stream.
#[test]
fn every_refused_spawn_traps_on_both_backends() {
    use std::os::unix::process::ExitStatusExt;
    let (dir, built) = build_both("process-traps", TRAPS);
    for (backend, exe) in &built {
        let ok = Command::new(exe).arg("ok").output().expect("the program runs");
        assert_eq!(ok.status.code(), Some(0), "`{backend}`: an ordinary spawn under `/bin`");
        let null = Command::new(exe).arg("null").output().expect("the program runs");
        assert_eq!(null.status.code(), Some(0), "`{backend}`: a child writing to `Stdio::Null`");
        for mode in ["usr", "dotdot", "sibling", "ld", "yld", "args", "env"] {
            let run = Command::new(exe).arg(mode).output().expect("the program runs");
            assert!(
                matches!(run.status.signal(), Some(4 | 5)),
                "`{backend}`: `{mode}` should trap, not {:?}",
                run.status
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// §2: the report names the bound, and the program stays bounded.
#[test]
fn the_authority_report_names_the_programs_a_program_may_start() {
    let dir = scratch("process-authority");
    let file = dir.join("traps.ls");
    std::fs::write(&file, TRAPS).expect("the program is written");
    let out = Command::new(BIN).arg("authority").arg(&file).output().expect("the compiler runs");
    let report = String::from_utf8_lossy(&out.stdout);
    assert!(report.contains("exec(\"/bin\")"), "{report}");
    assert!(!report.contains("UNBOUNDED"), "{report}");
    let _ = std::fs::remove_dir_all(&dir);
}
