#!/usr/bin/env python3
"""X25519 before and after #381 (docs/wide-multiply.md §10.4), on one machine, interleaved.

    python3 scripts/x25519_before_after.py <cancho binary> [--backend llvm|cranelift] [--runs 7] [--rounds 2000] [--taskset 4-5]

"Before" is `std/x25519.cho` as it was before it moved onto `std.field25519_51`: taken from git (the parent of the commit that
removed `import std.field25519;` from it), built as a local module `x25519old` over the unchanged `std.field25519`, with the same
compiler. "After" is the tree's `std.x25519`. Both are `tests/programs/ed25519_bench.cho`'s X25519 loop, timed by
`scripts/curve25519_bench.py` (median of five runs of `rounds` operations, less one). The two are run alternately `runs` times so a
change in the machine's load shows in both; it prints every figure, the median and minimum of each, and the ratio of the medians.
"""
import argparse
import os
import re
import statistics
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True).stdout


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cancho")
    ap.add_argument("--backend", default="llvm")
    ap.add_argument("--runs", type=int, default=7)
    ap.add_argument("--rounds", type=int, default=2000)
    ap.add_argument("--taskset")
    a = ap.parse_args()
    rev = git("log", "-1", "--format=%H", "-S", "import std.field25519;", "--", "std/x25519.cho").strip()
    old = git("show", f"{rev}^:std/x25519.cho").replace("module std.x25519;", "module x25519old;", 1)
    bench = open(os.path.join(ROOT, "tests/programs/ed25519_bench.cho")).read()
    with tempfile.TemporaryDirectory() as d:
        open(os.path.join(d, "x25519old.cho"), "w").write(old)
        open(os.path.join(d, "bench_old.cho"), "w").write(
            bench.replace("import std.x25519;", "import x25519old;").replace("x25519.scalarmult", "x25519old.scalarmult"))
        exes = {"before": os.path.join(d, "before"), "after": os.path.join(d, "after")}
        b = ["--backend", a.backend]
        subprocess.run([a.cancho, "build", "--std", *b, os.path.join(d, "bench_old.cho"), os.path.join(d, "x25519old.cho"), "-o", exes["before"]],
                       check=True, capture_output=True)
        subprocess.run([a.cancho, "build", "--std", *b, os.path.join(ROOT, "tests/programs/ed25519_bench.cho"), "-o", exes["after"]],
                       check=True, capture_output=True)
        print(f"before: std/x25519.cho at {rev[:10]}^; backend {a.backend}")
        got = {"before": [], "after": []}
        pin = ["taskset", "-c", a.taskset] if a.taskset else []
        for _ in range(a.runs):
            for name, exe in exes.items():
                out = subprocess.run([*pin, sys.executable, os.path.join(ROOT, "scripts/curve25519_bench.py"), exe, "3", str(a.rounds)],
                                     check=True, capture_output=True, text=True).stdout
                ms = float(re.search(r"x25519: ([\d.]+) ms", out).group(1))
                got[name].append(ms)
                print(f"{name:6} {ms:.3f} ms", flush=True)
    for name, v in got.items():
        print(f"{name:6} median {statistics.median(v):.3f} ms, min {min(v):.3f} ms")
    print(f"speed-up (medians): {statistics.median(got['before']) / statistics.median(got['after']):.1f}x")


if __name__ == "__main__":
    main()
