"""capture.py -- the driver of tests/commands/capture.sh, for `conformance/capture.rs` (deleted): `std.process` (docs/processes.md 7.1), an argument list, and a child's output captured with a bound and a deadline.
One program, tests/programs/process_capture.cho; each case is a mode on its command line, and what it prints ends in the milliseconds it took (`ms N`), which is checked here and left out of the comparison.

    python3 tests/commands/capture.py CANCHO SCRATCH      (run from the root of the repository)
"""
import os
import subprocess

import drive
from drive import CANCHO, LINUX, WORK, check, equal, need, program

DRIVER = "tests/programs/process_capture.cho"
NL = "\n"


def case(mode):
    """Run one case: what it printed less its `ms` line, and the milliseconds it took."""
    exe = program("capture", DRIVER)
    done = subprocess.run([exe, mode], capture_output=True, text=True)
    equal(done.returncode, 0, "the driver itself should not fail")
    kept, ms = "", 0
    for line in done.stdout.splitlines():
        if line.startswith("ms "):
            ms = max(ms, int(line[3:]))
        else:
            kept += line + NL
    return kept, ms


def checksum(text):
    """The driver's checksum (`sum` in process_capture.cho) of a string."""
    s = 0
    for byte in text.encode():
        s = (s * 31 + byte) % 1_000_000_007
    return s


@check("input and output are interleaved past both kernels' buffers")
def _():
    # 7.1: 1 MiB through `cat`, written while it is read. A channel holds 8,192 bytes on macOS and 180,224 on Linux, and writing it all before reading any deadlocks from 32 KiB and 512 KiB; every byte comes back.
    printed, _ms = case("cat")
    lines = printed.splitlines()
    need(lines and lines[0].startswith("want sum "), printed)
    want = lines[0][len("want sum "):]
    equal(lines[1:], ["code 0", "kept 1048576", f"sum {want}"], "what came back")


@check("output past the bound ends the child")
def _():
    # 7.1: `most` bytes are kept and the capture is a normal one; the byte past `most` kills the child, and exactly `most` are kept. The child lingers after its output (`sleep 30`), so only the kill ends it.
    equal(case("exact")[0], "code 0\nkept 1000\nsum 0\n", "exactly the bound")
    printed, ms = case("most")
    equal(printed, "too much\nkept 1000\nsum 0\n", "past the bound")
    need(ms < 5000, f"the child outlived the bound: {ms} ms")


@check("a child that cannot be watched is killed, not waited on")
def _():
    # 7.1: with `pidfd_open` refused (ENOSYS, 38), `sleep 30` is not waited on blind. Linux only; elsewhere this prints what a pass prints.
    if not LINUX:
        return
    exe = program("capture", DRIVER)
    done = subprocess.run([exe, "failed"], capture_output=True, text=True, preexec_fn=drive.without_pidfd_open)
    need(done.stdout.startswith("failed 38\nkept 0\nsum 0\nms "), done.stdout)
    ms = int(done.stdout.split()[-1])
    need(ms < 5000, f"the child was waited on: {ms} ms")


def cpu_ms_of(mode):
    """The driver on `mode`: (what it printed, the processor time it used in ms by `wait4`'s account)."""
    exe = program("capture", DRIVER)
    child = subprocess.Popen([exe, mode], stdout=subprocess.PIPE)
    printed = child.stdout.read().decode()
    _pid, _status, usage = os.wait4(child.pid, 0)
    child.stdout.close()
    return printed, int((usage.ru_utime + usage.ru_stime) * 1000)


@check("a quiet child is waited for, not spun on")
def _():
    # 7.1: a capture with nothing to do waits; it does not spin. The child closes both its streams and sleeps for a second: a channel that has ended stays ready for ever unless it is closed.
    printed, cpu = cpu_ms_of("quiet")
    need(printed.startswith("code 0\nkept 0\nsum 0\n"), printed)
    need(cpu < 400, f"a one-second wait took {cpu} ms of processor time")


@check("a quiet child's errors are waited for, not spun on")
def _():
    # 7.2: the same with the errors channel: `capture_both` that forgot the errors would spin on that one.
    printed, cpu = cpu_ms_of("r")
    need(printed.startswith("code 0\nout kept 0\nout sum 0\nerr kept 0\nerr sum 0\n"), printed)
    need(cpu < 400, f"a one-second wait took {cpu} ms of processor time")


@check("a deadline ends a child that outlives it")
def _():
    # 7.1: the deadline is the `Clock`'s, not a timeout per wait. `sleep 30` under 200 ms is killed, reaped and reported `TimedOut`, well before 30 s.
    printed, ms = case("time")
    equal(printed, "timed out\nkept 0\nsum 0\n", "the answer")
    need(200 <= ms < 5000, f"200 ms asked for, {ms} taken")


@check("a process left behind does not hold the capture")
def _():
    # 7.1: the child's exit ends the capture, not the end of the stream. A background `sleep 2` holds the output end for two seconds after the shell has exited.
    printed, ms = case("behind")
    equal(printed, "code 0\nkept 3\nsum 103209\n", "the answer")
    need(ms < 1500, f"the capture waited for the grandchild: {ms} ms")


@check("a child that does not read its input still exits")
def _():
    # 7.1: the write fails, the input end is closed, and the child's own exit status is the answer. No input at all is the same.
    equal(case("deaf")[0], "code 0\nkept 0\nsum 0\ncode 4\nkept 0\nsum 0\n", "the answers")


@check("an argument with a NUL is refused and left out")
def _():
    # 7.1: an argument containing a `\0` would arrive as two; `add` refuses it with EINVAL (22) and leaves it out, and the list without it runs.
    equal(case("nul")[0], "0 22 0 2\ncode 6\nkept 0\nsum 0\n", "the answers")


@check("a caller of capture declares the clock")
def _():
    # 7.1: `capture`'s row is [heap, clock, poll], and a caller that lends it the `Clock` performs `clock` through it: a row that leaves it out is refused. A check here rather than a reject fixture, because fixtures are checked without --std.
    caller = (
        "edition 7;\nimport std.buffer;\nimport std.process;\n\n"
        "fn wait[&h, &c](heap: &!h Heap, clock: &c Clock, child: Child, to: Pipe, from: Pipe)\n    -> [heap, poll] int {\n"
        '    let (out, ran) = process.capture(heap, clock, child, to, from, "", 10, 10);\n    return buffer.drop(heap, out);\n}\n\n'
        "fn main(world: World) -> [] int {\n    release(world);\n    return 0;\n}\n"
    )
    path = os.path.join(WORK, "caller.cho")
    with open(path, "w") as file:
        file.write(caller)
    done = subprocess.run([CANCHO, "check", path, "--std", "--output", "json"], capture_output=True, text=True)
    need('"rule": "effect-not-declared"' in done.stdout, done.stdout)
    # The Rust compiler's message also said "performs `clock`"; the compiler in cancho says the sentence of the rule, so only the tag is compared.


@check("a capture that times out gives back all three channels")
def _():
    # 7.2: under a limit of 40 descriptors sixty captures come and go, each with three channels; the errors end that was not closed would leak one per capture, and `channels_with_errors` would answer EMFILE (24) well before the sixtieth.
    exe = program("capture", DRIVER)
    done = subprocess.run(["/bin/sh", "-c", 'ulimit -n 40; exec "$0" "$@"', exe, "l"], capture_output=True, text=True)
    need(done.stdout.startswith("timed out 60\n"), done.stdout)


@check("standard error is captured beside standard output")
def _():
    printed, _ms = case("x")
    equal(printed, f"code 3\nout kept 4\nout sum {checksum('out' + NL)}\nerr kept 4\nerr sum {checksum('err' + NL)}\n", "the answer")


@check("a flood of errors does not stall the output")
def _():
    # 7.2: a megabyte of errors before any output: a capture that read the output to its end before the errors would leave the child blocked on the errors for ever; both are on the poller.
    printed, ms = case("y")
    equal(printed, f"code 0\nout kept 4\nout sum {checksum('out' + NL)}\nerr kept 1048576\nerr sum 0\n", "the answer")
    need(ms < 5000, f"the capture waited on the child: {ms} ms")


@check("errors past their bound end the child")
def _():
    # 7.2: `most_errors` is the errors' own bound. Errors past it end the child (`sleep 30` follows them, so only the kill ends it) and exactly the bound is kept; and the bounds are separate.
    printed, ms = case("z")
    equal(printed, "too much\nout kept 0\nout sum 0\nerr kept 1000\nerr sum 0\n", "the answer")
    need(ms < 5000, f"the child outlived the bound: {ms} ms")
    equal(case("w")[0], f"too much\nout kept 4\nout sum {checksum('out' + NL)}\nerr kept 3\nerr sum {checksum('err')}\n", "separate bounds")


@check("a deadline returns both channels so far")
def _():
    printed, ms = case("v")
    equal(printed, f"timed out\nout kept 2\nout sum {checksum('a' + NL)}\nerr kept 2\nerr sum {checksum('b' + NL)}\n", "the answer")
    need(300 <= ms < 5000, f"300 ms asked for, {ms} taken")


drive.run_all()
