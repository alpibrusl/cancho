"""drive.py -- what the Python drivers of tests/commands share (signals.py, process_release.py, capture.py): the compiler and a scratch directory from the command line, a registry of named checks,
a way to build a program of tests/programs/ with the compiler under test, and the transcript each driver prints: `ok NAME` for a check that holds, `GAP: ...` for a pinned difference from the Rust
compiler the check was written against, `FAILS NAME: why` for one that does not. A driver always exits 0: its transcript, compared with the file NAME.expected by tests/commands/script_tests.cho, is the verdict.
"""
import os
import subprocess
import sys

CANCHO, WORK = sys.argv[1], sys.argv[2]
DARWIN = sys.platform == "darwin"
LINUX = sys.platform.startswith("linux")


class Failed(Exception):
    pass


def need(holds, why):
    if not holds:
        raise Failed(why)


def equal(got, want, why=""):
    if got != want:
        raise Failed(f"{why}: got {got!r}, wanted {want!r}" if why else f"got {got!r}, wanted {want!r}")


CHECKS = []
# Where the compiler in cancho answers differently from the Rust compiler the test was written against: case name -> the answer pinned. The transcript says `GAP:` for each, so it changes when one closes.
KNOWN_GAPS = {}
GAPS = []


def judge(name, got, wanted):
    """True if `got` is what was wanted; a pinned gap is reported, not a failure; anything else is wrong."""
    if got == wanted:
        return True
    if name in KNOWN_GAPS and KNOWN_GAPS[name] == got:
        GAPS.append(f"GAP: {name}: the Rust compiler answers {wanted!r}, the compiler in cancho {got!r}")
        return True
    return False


def check(name):
    """Register a check: it prints `ok NAME`, or `FAILS NAME: why` if it raises."""

    def register(function):
        CHECKS.append((name, function))
        return function

    return register


def build(source_path, name, replace=None, std=True):
    """The program of `source_path` built by the compiler under test; with `replace`, a pair of strings swapped in the source first."""
    text = open(source_path).read()
    if replace:
        need(replace[0] in text, f"`{replace[0]}` is not in {source_path}")
        text = text.replace(*replace)
    src = os.path.join(WORK, name + ".cho")
    with open(src, "w") as file:
        file.write(text)
    exe = os.path.join(WORK, name)
    argv = [CANCHO, "build", src] + (["--std"] if std else []) + ["-o", exe]
    done = subprocess.run(argv, capture_output=True, text=True)
    need(done.returncode == 0, f"building {name} said: {done.stderr.strip()[:300]}")
    return exe


PROGRAMS = {}


def program(key, path, replace=None, std=True):
    """`build`, once for each key."""
    if key not in PROGRAMS:
        PROGRAMS[key] = build(path, key, replace, std)
    return PROGRAMS[key]


def without_pidfd_open():
    """For `preexec_fn`: a kernel without `pidfd_open` (before Linux 5.3), stood in for by a seccomp filter that answers it (434) with ENOSYS (38), installed in the child before it executes. Linux only."""
    import ctypes
    import struct

    libc = ctypes.CDLL(None, use_errno=True)
    BPF_LD_W_ABS, BPF_JEQ_K, BPF_RET_K = 0x20, 0x15, 0x06
    RET_ERRNO, RET_ALLOW = 0x00050000, 0x7FFF0000
    filters = [(BPF_LD_W_ABS, 0, 0, 0), (BPF_JEQ_K, 0, 1, 434), (BPF_RET_K, 0, 0, RET_ERRNO | 38), (BPF_RET_K, 0, 0, RET_ALLOW)]
    raw = b"".join(struct.pack("HBBI", *f) for f in filters)
    buf = ctypes.create_string_buffer(raw, len(raw))
    program = struct.pack("H6xQ", len(filters), ctypes.addressof(buf))
    holder = ctypes.create_string_buffer(program, len(program))
    PR_SET_NO_NEW_PRIVS, PR_SET_SECCOMP, SECCOMP_MODE_FILTER = 38, 22, 2
    if libc.prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 or libc.prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, ctypes.addressof(holder), 0, 0) != 0:
        raise OSError("could not install the seccomp filter")


def run_all():
    for name, function in CHECKS:
        del GAPS[:]
        try:
            function()
            for gap in GAPS:
                print(gap)
            print("ok " + name)
        except Failed as why:
            print(f"FAILS {name}: {why}")
        except Exception as why:  # a broken check is a failure, not a crash of the transcript
            print(f"FAILS {name}: {type(why).__name__}: {why}")
        sys.stdout.flush()
