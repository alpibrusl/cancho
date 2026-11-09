"""signals.py -- the driver of tests/commands/signals.sh, for `conformance/signals.rs` (deleted): the signal capability (docs/signals.md), over real processes.

    python3 tests/commands/signals.py CANCHO SCRATCH      (run from the root of the repository)

The programs under tests/programs/signals_*.cho speak on standard error (`ready`, `mask=8`, ...), which is unbuffered, and are driven by standard input, one byte a command, so that a signal sent
before a `p` is queued before the poll that must see it: kill(2) returns once the signal is pending. The signals are sent by this driver, to the real process the compiler built.
The transcript is drive.py's.
"""
import os
import queue
import signal
import subprocess
import threading
import time

import drive
from drive import CANCHO, LINUX, WORK, Failed, check, equal, need, program

CLAIMABLE = ["ALRM", "HUP", "INT", "QUIT", "TERM", "USR1", "USR2", "WINCH"]
# cancho's own bit for a claimable signal (docs/signals.md section 2.2).
BIT = {"HUP": 1, "INT": 2, "QUIT": 4, "TERM": 8, "USR1": 16, "USR2": 32, "ALRM": 64, "WINCH": 128}
drive.KNOWN_GAPS.update({
    "root": None,
    "not signals": None,
    "by value": "linear-use-after-move",
    "owned root": None,
    "unordered": None,
    "row exact": "effect-declared-not-performed",
})
judge, GAPS = drive.judge, drive.GAPS


def host(name):
    """The signal number of the machine this runs on, by name."""
    return int(getattr(signal, "SIG" + name))


class Run:
    """A running program: its standard error as lines, its standard input open."""

    def __init__(self, argv):
        self.process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        self.lines = queue.Queue()
        self.seen = []
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for raw in self.process.stderr:
            self.lines.put(raw.decode(errors="replace").rstrip("\n"))
        self.lines.put(None)

    def line(self):
        """The next line the program wrote, or a failure after 20 seconds."""
        try:
            line = self.lines.get(timeout=20)
        except queue.Empty:
            raise Failed("the program wrote nothing for 20 seconds")
        need(line is not None, "the program ended without a line")
        self.seen.append(line)
        return line

    def expect(self, wanted):
        equal(self.line(), wanted, "the program said")

    def signal(self, name):
        os.kill(self.process.pid, host(name))

    def command(self, byte):
        self.process.stdin.write(bytes([byte]))
        self.process.stdin.flush()

    def alive(self):
        return self.process.poll() is None

    def finish(self):
        """Close standard input and wait for the end, however it comes; the return code (negative: ended by that signal)."""
        try:
            self.process.stdin.close()
        except OSError:
            pass
        try:
            return self.process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            raise Failed("the program did not end within 20 seconds")

    def rest(self):
        """Every line written so far that was not read."""
        out = []
        while True:
            try:
                line = self.lines.get(timeout=0.5)
            except queue.Empty:
                return out
            if line is None:
                return out
            out.append(line)

    def kill(self):
        if self.alive():
            self.process.kill()
        self.process.wait()
        for pipe in (self.process.stdin, self.process.stderr):
            try:
                pipe.close()
            except OSError:
                pass


def started(exe, ignoring=None):
    """The program started; with `ignoring`, with that signal already ignored, as a `nohup`ed process has HUP and INT."""
    if ignoring:
        return Run(["sh", "-c", f"trap '' {ignoring}; exec \"$0\"", exe])
    return Run([exe])


EVERY = "tests/programs/signals_every.cho"
SECOND = "tests/programs/signals_second.cho"


# ---- the signals themselves ----


@check("each claimed signal is delivered exactly once")
def _():
    run = started(program("every", EVERY))
    try:
        run.expect("ready")
        for name in CLAIMABLE:
            run.signal(name)
            run.command(ord("p"))
            run.expect(f"mask={BIT[name]}")
            run.command(ord("p"))
            run.expect("mask=0")
        # And a poll when nothing was sent says nothing arrived.
        run.command(ord("p"))
        run.expect("mask=0")
        run.command(ord("q"))
        run.expect("ended")
        equal(run.finish(), 0, "the exit status")
    finally:
        run.kill()


@check("signals between polls are all reported and a repeat is one")
def _():
    run = started(program("every", EVERY))
    try:
        run.expect("ready")
        for name in ("TERM", "INT", "HUP"):
            run.signal(name)
        run.command(ord("p"))
        run.expect(f"mask={BIT['TERM'] | BIT['INT'] | BIT['HUP']}")
        # The same signal three times, with another between: two bits, not four.
        for name in ("USR1", "USR1", "WINCH", "USR1"):
            run.signal(name)
        run.command(ord("p"))
        run.expect(f"mask={BIT['USR1'] | BIT['WINCH']}")
        # All eight at once.
        for name in CLAIMABLE:
            run.signal(name)
        run.command(ord("p"))
        run.expect("mask=255")
        run.command(ord("p"))
        run.expect("mask=0")
        run.command(ord("q"))
        run.expect("ended")
        equal(run.finish(), 0, "the exit status")
    finally:
        run.kill()


@check("a signal that was not claimed still takes its default action")
def _():
    # The same program, claiming only TERM, is ended by USR1 as any program is.
    exe = program("unclaimed", EVERY, ("ALRM,HUP,INT,QUIT,TERM,USR1,USR2,WINCH", "TERM"))
    run = started(exe)
    try:
        run.expect("ready")
        run.signal("TERM")
        run.command(ord("p"))
        run.expect("mask=8")
        run.signal("USR1")
        equal(run.finish(), -host("USR1"), "ended by")
    finally:
        run.kill()


@check("a signal wakes a poller wait at once")
def _():
    run = started(program("poller", "tests/programs/signals_poller.cho"))
    try:
        run.expect("added=0")
        run.expect("ready")
        # The wait is established by now: a signal sent later cannot have been picked up by a poll that ran before the wait began.
        time.sleep(0.3)
        sent = time.monotonic()
        run.signal("TERM")
        run.expect("woke=1")
        woke = time.monotonic() - sent
        run.expect("token=7")
        run.expect("events=1")
        line = run.line()
        need(line.startswith("waited="), f"waited= expected, the program said {line!r}")
        waited = int(line[len("waited="):])
        run.expect("mask=8")
        run.expect("again=0")
        # 20 s was the timeout; a wake that took the timeout out would not be this.
        need(woke < 5, f"woke {woke:.2f} s after the signal")
        need(250 <= waited < 5000, f"the wait lasted {waited} ms")
        equal(run.finish(), 0, "the exit status")
    finally:
        run.kill()


@check("closing the claim makes the next signal kill at once")
def _():
    run = started(program("second", SECOND))
    try:
        run.expect("ready")
        run.signal("TERM")
        run.command(ord("r"))
        run.expect("first=8")
        run.expect("closed=0")
        sent = time.monotonic()
        run.signal("TERM")
        equal(run.finish(), -host("TERM"), "ended by")
        need(time.monotonic() - sent < 5, "not at once")
    finally:
        run.kill()


@check("closing with a signal unread delivers it at once")
def _():
    run = started(program("second", SECOND))
    try:
        run.expect("ready")
        run.signal("TERM")
        run.command(ord("g"))
        equal(run.finish(), -host("TERM"), "ended by")
        # It never got as far as saying it was closed.
        need(all(not line.startswith("closed=") for line in run.rest()), "the program ran on past the close")
    finally:
        run.kill()


@check("closing after the read leaves nothing to deliver")
def _():
    run = started(program("second", SECOND))
    try:
        run.expect("ready")
        run.signal("USR2")
        run.command(ord("r"))
        run.expect("first=32")
        run.expect("closed=0")
        # Alive: it is sleeping on an empty poller, and the signal it read is gone.
        time.sleep(0.3)
        need(run.alive(), "it ended on its own")
        # Not claimed any more, so USR2 is USR2 again.
        run.signal("USR2")
        equal(run.finish(), -host("USR2"), "ended by")
    finally:
        run.kill()


@check("a signal ignored on entry can be claimed")
def _():
    # What it is afterwards is the one place the two kernels differ (docs/signals.md section 3): Linux never touched it, so it is ignored again; macOS ignored it to watch it and puts back the default.
    run = started(program("second", SECOND), ignoring="USR2")
    try:
        run.expect("ready")
        run.signal("USR2")
        run.command(ord("r"))
        run.expect("first=32")
        run.expect("closed=0")
        if LINUX:
            # Ignored as it was entered: the signal does nothing.
            run.signal("USR2")
            time.sleep(0.3)
            need(run.alive(), "ignored, so it lives")
            # And an unrelated one that was never ignored still ends it.
            run.signal("TERM")
            equal(run.finish(), -host("TERM"), "ended by")
    finally:
        run.kill()


@check("a thread spawned after the claim does not take the signal")
def _():
    run = started(program("inherit", "tests/programs/signals_threads_inherit.cho"))
    try:
        run.expect("ready")
        # The thread is napping (2.5 s: a loaded host can be slow to send the signal); the signal must wait for the program.
        time.sleep(0.2)
        run.signal("TERM")
        run.expect("mask=8")
        equal(run.finish(), 0, "the exit status")
    finally:
        run.kill()


@check("a claim is refused while a thread runs and granted after the join")
def _():
    run = started(program("busy", "tests/programs/signals_threads_busy.cho"))
    try:
        run.expect("during=16")
        run.expect("ready")
        run.expect("after=granted")
        equal(run.finish(), 0, "the exit status")
    finally:
        run.kill()


@check("a signal another claim holds is refused until it is closed")
def _():
    run = started(program("overlap", "tests/programs/signals_overlap.cho"))
    try:
        run.expect("second=16")
        run.expect("third=granted")
        equal(run.finish(), 0, "the exit status")
    finally:
        run.kill()


@check("std.signals names the bits")
def _():
    exe = program("std", "tests/programs/signals_std.cho")
    equal(subprocess.run([exe]).returncode, 8, "the checks that pass")


# ---- refusals, and the authority report ----


def checked(source):
    """`cancho check --output json` on a source: None if it was accepted, else the rule it was refused under (the first, as the Rust test read it)."""
    path = os.path.join(WORK, "refusal.cho")
    with open(path, "w") as file:
        file.write(source)
    done = subprocess.run([CANCHO, "check", path, "--std", "--output", "json"], capture_output=True, text=True)
    if done.returncode == 0:
        return None
    for line in done.stdout.splitlines():
        line = line.strip()
        if line.startswith('"rule": "'):
            return line[len('"rule": "'):].rstrip('",')
    return "(refused with no rule: " + (done.stdout.strip() + done.stderr.strip())[:200] + ")"


def with_signals(body):
    """`main` with the whole of `Split` taken apart, leaving `signals`."""
    return (
        "edition 6;\nfn main(world: World) -> [] int {\n"
        "    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);\n"
        "    release(io); release(ffi); release(fs); release(heap); release(args); release(net);\n"
        "    release(clock);\n    " + body + "\n    return 0;\n}\n"
    )


def narrowing_to(names):
    return with_signals('let claimed = narrow(signals, "' + names + '"); release(claimed);')


def main_with(released, body):
    return (
        "edition 6;\nfn main(world: World) -> [] int {\n"
        "    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);\n"
        "    " + " ".join(f"release({name});" for name in released) + "\n    " + body + "\n    return 0;\n}\n"
    )


ALL_BUT_SIGNALS = ["io", "ffi", "fs", "heap", "args", "net", "clock"]
CLAIM_AND_CLOSE = "match signals_watch(CAP) { Watching::Ok(w) => { signals_close(w); } Watching::Failed(e) => { } }"


@check("every unclaimable signal is refused with its rule")
def _():
    # The Rust test also read the reason out of the message ("cannot be caught", "a fault, not a request", "not claimable (yet)"); the compiler in cancho says the sentence of the rule, so only the tag is compared.
    names = ["KILL", "STOP", "SEGV", "ILL", "BUS", "FPE", "ABRT", "TRAP", "SYS",
             "PIPE", "CHLD", "CONT", "TSTP", "TTIN", "TTOU", "URG", "IO", "XCPU", "XFSZ", "VTALRM", "PROF", "STKFLT", "PWR"]
    wrong = {name: checked(narrowing_to(name)) for name in names}
    wrong = {name: rule for name, rule in wrong.items() if rule != "signal-not-claimable"}
    need(not wrong, f"not refused under signal-not-claimable: {wrong}")
    # One bad name refuses the set, wherever it is.
    mixed = {s: checked(narrowing_to(s)) for s in ("KILL,TERM", "TERM,KILL", "INT,SEGV,TERM")}
    need(all(rule == "signal-not-claimable" for rule in mixed.values()), f"mixed sets: {mixed}")


@check("a malformed set is refused and any order is accepted")
def _():
    malformed = ["", "TERM,", ",TERM", "TERM,TERM", "INT, TERM", "SIGTERM", "term", "FOO", "TERM;INT"]
    wrong = {s: checked(narrowing_to(s)) for s in malformed}
    wrong = {s: rule for s, rule in wrong.items() if rule != "signal-not-claimable"}
    need(not wrong, f"not refused under signal-not-claimable: {wrong}")
    refused = {s: checked(narrowing_to(s)) for s in CLAIMABLE + ["TERM,INT", ",".join(CLAIMABLE)]}
    refused = {s: rule for s, rule in refused.items() if rule is not None}
    need(not refused, f"well-formed sets refused: {refused}")
    # The type is the canonical one whichever order the set was written in.
    written = 'fn f[&s](sig: &s Signals("INT,TERM")) -> [] int { return 0; }'
    program_text = with_signals('let claimed = narrow(signals, "TERM,INT"); borrow claimed as &s in { f(s); } release(claimed);') + "\n" + written
    equal(checked(program_text), None, "the canonical type")


@check("narrowing a signal set never widens it")
def _():
    cases = {
        "widen": ('let one = narrow(signals, "INT"); let two = narrow(one, "INT,TERM"); release(two);', "capability-not-narrowable"),
        "sideways": ('let one = narrow(signals, "INT"); let two = narrow(one, "TERM"); release(two);', "capability-not-narrowable"),
        "same": ('let one = narrow(signals, "INT,TERM"); let two = narrow(one, "TERM,INT"); release(two);', "capability-not-narrowable"),
        # Narrowing to a subset, and again to a smaller one, is the point.
        "subset": ('let a = narrow(signals, "HUP,INT,TERM"); let b = narrow(a, "INT,TERM"); let c = narrow(b, "TERM"); release(c);', None),
    }
    wrong = {name: checked(with_signals(body)) for name, (body, _) in cases.items()}
    wrong = {name: rule for name, rule in wrong.items() if rule != cases[name][1]}
    need(not wrong, f"wrong answers: {wrong}")


@check("only a narrowed borrowed Signals can claim")
def _():
    claim = CLAIM_AND_CLOSE.replace("CAP", "s")
    cases = {
        "root": (main_with(ALL_BUT_SIGNALS, "borrow signals as &s in { " + claim + " } release(signals);"), "capability-misused"),
        "not signals": (main_with(["io", "ffi", "fs", "heap", "args", "net", "signals"], "borrow clock as &c in { " + CLAIM_AND_CLOSE.replace("CAP", "c") + " } release(clock);"), "capability-misused"),
        "by value": (main_with(ALL_BUT_SIGNALS, 'let claim = narrow(signals, "TERM"); ' + CLAIM_AND_CLOSE.replace("CAP", "claim") + " release(claim);"), "capability-misused"),
        "good": (main_with(ALL_BUT_SIGNALS, 'let claim = narrow(signals, "TERM"); borrow claim as &s in { ' + claim + " } release(claim);"), None),
    }
    wrong = {}
    for name, (text, wanted) in cases.items():
        got = checked(text)
        if not judge(name, got, wanted):
            wrong[name] = got
    need(not wrong, f"wrong answers: {wrong}")


@check("the signal names are edition six")
def _():
    five = ("edition 5;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n"
            "    release(io); release(ffi); release(fs); release(heap); release(args);\n    release(net); release(clock);\n    return 0;\n}\n")
    equal(checked(five), None, "edition 5 keeps its Split")
    equal(checked(five.replace("return 0;", "return signals_close(3);")), "not-a-function", "a name below edition 6")
    # A seven-field pattern at edition 6 names too few.
    equal(checked(five.replace("edition 5;", "edition 6;")), "arity-mismatch", "seven fields at edition 6")
    # Edition 6 is edition 5 and more: the socket names are still there.
    sockets = ("edition 6;\nfn f() -> [] int { match poller_new() { Polling::Ok(p) => { poller_close(p); } Polling::Failed(e) => { } } return 0; }\n"
               "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);\n"
               "    release(io); release(ffi); release(fs); release(heap); release(args);\n    release(net); release(clock); release(signals);\n    return f();\n}\n")
    equal(checked(sockets), None, "the socket names at edition 6")


@check("the claim is linear and its rows are exact")
def _():
    def claim(body):
        return main_with(
            ALL_BUT_SIGNALS,
            'let c = narrow(signals, "TERM"); borrow c as &s in { match signals_watch(s) { Watching::Ok(w) => { ' + body
            + " } Watching::Failed(e) => { } } } release(c);",
        )

    main_only = "fn main(world: World) -> [] int { release(world); return 0; }\n"
    owned_caps = ('edition 6;\nfn claim(sig: Signals("TERM")) -> [] int {\n    var status = 1;\n    borrow sig as &s in {\n'
                  "        match signals_watch(s) {\n            Watching::Ok(w) => { var held = w; borrow mut held as &!h in { signals_pending(h); } status = signals_close(held); }\n"
                  "            Watching::Failed(e) => { status = e; }\n        }\n    }\n    release(sig);\n    return status;\n}\n" + main_only)

    def row(declared):
        return ('edition 6;\nfn claim[&s](sig: &s Signals("INT,TERM")) -> [signals("' + declared + '")] int { '
                "match signals_watch(sig) { Watching::Ok(w) => { return signals_close(w); } Watching::Failed(e) => { return e; } } }\n" + main_only)

    cases = {
        "unclosed": (claim(""), "linear-value-unconsumed"),
        "twice": (claim("signals_close(w); signals_close(w);"), "linear-use-after-move"),
        "apart": (claim("let SignalWatch { } = w;"), "linear-value-taken-apart"),
        "closed": (claim("signals_close(w);"), None),
        "undeclared": ("edition 6;\nfn look[&w](watch: &!w SignalWatch) -> [] int { return signals_pending(watch); }\n" + main_only, "effect-not-declared"),
        "unperformed": ("edition 6;\nfn look[&w](watch: &!w SignalWatch) -> [signals_read] int { return 0; }\n" + main_only, "effect-declared-not-performed"),
        "declared": ("edition 6;\nfn look[&w](watch: &!w SignalWatch) -> [signals_read] int { return signals_pending(watch); }\n" + main_only, None),
        # Owning one is stronger than borrowing it, and says so by having an empty row.
        "owned watch": ("edition 6;\nfn drain(watch: SignalWatch) -> [] int {\n    var held = watch;\n    borrow mut held as &!w in { signals_pending(w); }\n    return signals_close(held);\n}\n" + main_only, None),
        "owned cap": (owned_caps, None),
        # The root, owned, cannot claim; a set written in a type is the canonical one, or it is refused.
        "owned root": (owned_caps.replace('Signals("TERM")', 'Signals("")'), "(any refusal)"),
        "wider": (owned_caps.replace('Signals("TERM")', 'Signals("INT,TERM")'), None),
        "unordered": (owned_caps.replace('Signals("TERM")', 'Signals("TERM,INT")'), "signal-not-claimable"),
        # The set in a row is the set in the capability's type, exactly.
        "row exact": (row("INT,TERM"), None),
    }
    wrong = {}
    for name, (text, wanted) in cases.items():
        got = checked(text)
        if wanted == "(any refusal)":
            if got is None and not judge(name, got, got):
                wrong[name] = got
            elif got is None:
                GAPS.append(f"GAP: {name}: the Rust compiler refuses the owned root, the compiler in cancho accepts it")
        elif not judge(name, got, wanted):
            wrong[name] = got
    for declared in ("INT", "TERM", "HUP,INT,TERM", "TERM,INT"):
        got = checked(row(declared))
        if got not in ("effect-not-declared", "effect-declared-not-performed"):
            wrong["row " + declared] = got
    need(not wrong, f"wrong answers: {wrong}")


def authority(path, *flags):
    done = subprocess.run([CANCHO, "authority", path, "--std", *flags], capture_output=True, text=True)
    need(done.returncode == 0, f"authority said: {done.stderr.strip()[:300]}")
    return done.stdout


def report_program(names):
    text = open("tests/programs/signals_report.cho").read()
    path = os.path.join(WORK, "report.cho")
    with open(path, "w") as file:
        file.write(text.replace("@SET@", names))
    return path


@check("the authority report names the set and is bounded")
def _():
    out = authority(report_program("TERM,INT"), "--output", "json")
    need('"bounded": true' in out, out)
    need('{ "name": "signals", "argument": "INT,TERM", "bounded": true }' in out, out)
    need('{ "name": "signals_read", "argument": null, "bounded": true }' in out, out)
    need('"ffi"' not in out, "no foreign code: " + out)
    need('"foreign_symbols": []' in out, out)
    # The set is what was claimed: a different set is a different report.
    every = authority(report_program(",".join(CLAIMABLE)), "--output", "json")
    need('"argument": "' + ",".join(CLAIMABLE) + '"' in every, every)
    one = authority(report_program("HUP"), "--output", "json")
    need('"argument": "HUP"' in one and "TERM" not in one, one)


@check("the text report says what is claimed and what is not")
def _():
    claims = authority(report_program("INT,TERM"))
    need('signals("INT,TERM")' in claims, claims)
    need("UNBOUNDED" not in claims, claims)
    need("    signals\n" not in claims, "signals are claimed, so not 'never touches': " + claims)
    silent_path = os.path.join(WORK, "silent.cho")
    with open(silent_path, "w") as file:
        file.write(main_with(ALL_BUT_SIGNALS + ["signals"], ""))
    silent = authority(silent_path)
    need("never touches" in silent, silent)
    need("    signals\n" in silent, silent)


drive.run_all()
