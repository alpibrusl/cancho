"""process_release.py -- the driver of tests/commands/process_release.sh, for `conformance/process_release.rs` (deleted): letting go of a child (docs/processes.md 4.11), `exec_spawn_detached`, `child_pid` and `child_release`,
over real processes. The program is tests/programs/process_release.cho; what is looked at is outside it: the state, process group and session of the children it left behind, read with `ps`.

    python3 tests/commands/process_release.py CANCHO SCRATCH      (run from the root of the repository)
"""
import os
import subprocess
import time

import drive
from drive import CANCHO, LINUX, WORK, check, equal, need, program

DRIVER = "tests/programs/process_release.cho"


def column(pid, name):
    """One `ps` column of a process, None once the process is gone."""
    done = subprocess.run(["ps", "-o", name + "=", "-p", str(pid)], capture_output=True, text=True)
    text = done.stdout.strip()
    return text or None


def leads_a_session(pid):
    """`s` in its `ps` state on both kernels (macOS's `ps` has no session id to compare; Linux's is compared as well)."""
    stat = column(pid, "stat")
    return stat is not None and "s" in stat


def alive(pid):
    """There, and not a zombie waiting for a parent to reap it."""
    stat = column(pid, "stat")
    return stat is not None and not stat.startswith("Z")


def ends(pid, seconds):
    """Not running within `seconds`: gone, or a zombie."""
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        if not alive(pid):
            return True
        time.sleep(0.05)
    return False


def kill_group(group):
    """Signal a whole process group, as a supervisor or a terminal does (the `kill` program: dash's builtin has no `--`)."""
    done = subprocess.run(["kill", "-KILL", "--", f"-{group}"])
    need(done.returncode == 0, f"group {group} should be there to signal")


def pids(lines):
    return [int(line[4:]) for line in lines if line.startswith("pid ")]


def started(exe, *args):
    """The driver in a process group of its own with its standard input open, read to its `ready` line: (process, lines)."""
    process = subprocess.Popen([exe, *args], stdin=subprocess.PIPE, stdout=subprocess.PIPE, preexec_fn=lambda: os.setpgid(0, 0), text=True)
    lines = []
    for line in process.stdout:
        line = line.rstrip("\n")
        lines.append(line)
        if line == "ready":
            break
    if not lines or lines[-1] != "ready":
        status = process.wait()
        need(False, f"the driver ended ({status}) without `ready`: {lines}")
    return process, lines


def finished(process):
    process.stdin.close()
    code = process.wait(timeout=20)
    process.stdout.close()
    return code


@check("a detached child outlives its parent in a session of its own")
def _():
    exe = program("release", DRIVER, std=False)
    begun = time.monotonic()
    run = subprocess.run([exe, "d"], capture_output=True, text=True)
    equal(run.returncode, 0, "the driver returns at once")
    need(time.monotonic() - begun < 20, "it must not wait for the child")
    lines = run.stdout.splitlines()
    need("released 0" in lines, run.stdout)
    pid = pids(lines)[0]
    try:
        need(alive(pid), "the child is still running after its parent ended")
        equal(column(pid, "pgid"), str(pid), "it leads its group")
        need(leads_a_session(pid), "it leads a session")
        if LINUX:
            equal(column(pid, "sid"), str(pid), "its session is its own pid")
    finally:
        if alive(pid):
            kill_group(pid)
    need(ends(pid, 10), "the group signal reaches the child")


@check("only the detached child survives its parent's group")
def _():
    exe = program("release", DRIVER, std=False)
    parent, lines = started(exe, "b")
    detached, plain = pids(lines)[0], pids(lines)[1]
    group = parent.pid
    try:
        need(leads_a_session(detached), "a session of its own")
        need(not leads_a_session(plain), "the plain child is in its parent's")
        if LINUX:
            need(column(detached, "sid") != column(group, "sid"), "the detached child has a session of its own")
            equal(column(plain, "sid"), column(group, "sid"), "the plain child's session")
        equal(column(detached, "pgid"), str(detached), "the detached child's group")
        equal(column(plain, "pgid"), str(group), "the plain child's group")
        kill_group(group)
        parent.wait()
        parent.stdin.close()
        need(ends(plain, 10), "the plain child went with its group")
        need(alive(detached), "the detached child outlived its parent's group")
    finally:
        if alive(detached):
            kill_group(detached)
    need(ends(detached, 10), "the detached child ends with its own group")


@check("released children do not become zombies")
def _():
    exe = program("release", DRIVER, std=False)
    parent, lines = started(exe, "c", "50")
    released = pids(lines)
    equal(len(released), 50, "the children released")
    for pid in released:
        equal(column(pid, "stat"), None, f"child {pid} ended and was released, so nothing is left of it")
    need("held 7" in lines, "a held child is still waited for")
    equal(finished(parent), 0, "the exit status")


@check("a child that has already ended is released cleanly")
def _():
    exe = program("release", DRIVER, std=False)
    parent, lines = started(exe, "e")
    pid = pids(lines)[0]
    need("released 0" in lines, str(lines))
    equal(column(pid, "stat"), None, "no zombie of the child")
    equal(finished(parent), 0, "the exit status")


@check("a detached start stays inside the prefix it was narrowed to")
def _():
    exe = program("bound", "tests/programs/process_release_bound.cho", std=False)
    inside = subprocess.run([exe, "inside"], capture_output=True)
    equal(inside.returncode, 0, "a start under the prefix")
    outside = subprocess.run([exe, "outside"], capture_output=True)
    need(outside.returncode in (-4, -5), f"a start outside the prefix traps (SIGILL or SIGTRAP), not {outside.returncode}")


@check("the authority report says a process may outlive the program")
def _():
    def report(path):
        return subprocess.run([CANCHO, "authority", path], capture_output=True, text=True).stdout

    released = report(DRIVER)
    need('exec("/bin")' in released, released)
    need("exec_detached" in released, released)
    waited = report("tests/accept/process_spawn.cho")
    need('exec("/bin")' in waited, waited)
    need("exec_detached" not in waited, waited)


@check("releasing a child gives back its pidfd")
def _():
    # The pidfd a Linux child carries is closed by the release as it is by the wait: after fifty releases the program holds as many descriptors as after none. macOS watches a pid and holds no
    # descriptor, so there is nothing to count there, and this prints what a pass prints.
    if not LINUX:
        return
    exe = program("release", DRIVER, std=False)
    counts = []
    for n in ("0", "50"):
        parent, _ = started(exe, "c", n)
        counts.append(len(os.listdir(f"/proc/{parent.pid}/fd")))
        finished(parent)
    equal(counts[0], counts[1], "descriptors held after none and after fifty releases")


drive.run_all()
