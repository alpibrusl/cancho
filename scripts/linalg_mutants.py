#!/usr/bin/env python3
"""Mutation check of `std/linalg.cho` (docs/linalg.md §5).

    python3 scripts/linalg_mutants.py <cancho binary>

Each mutant is `std/linalg.cho` with one deliberate bug, built beside `tests/accept/linalg_hand_checked.cho` and `std/math.cho` and run: a mutant is killed when the program's output
differs from the `//~ STDOUT` lines of the fixture (whose answers were worked out by hand) or it exits non-zero. The unmutated file is run first and must pass, or the check proves
nothing. Exit status 1 if a mutant survives or fails to build.
"""
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURE = os.path.join(ROOT, "tests", "accept", "linalg_hand_checked.cho")

# (name, the text replaced, its replacement). Each `old` must occur exactly once.
MUTANTS = [
    ("gemm reads b transposed", "s = s + a[i * k + p] * b[p * n + j];", "s = s + a[i * k + p] * b[j * n + p];"),
    ("gemv sums one term too few", "while j < n {\n            s = s + a[i * n + j] * x[j];", "while j + 1 < n {\n            s = s + a[i * n + j] * x[j];"),
    ("axpy subtracts", "y[k] = y[k] + alpha * x[k];", "y[k] = y[k] - alpha * x[k];"),
    ("transpose does not move", "b[j * m + i] = a[i * n + j];", "b[i * n + j] = a[i * n + j];"),
    ("cholesky accepts a zero pivot", "if !(s > 0.0) {", "if !(s >= 0.0) {"),
    ("cholesky multiplies by the pivot", "a[i * n + j] = t / d;", "a[i * n + j] = t * d;"),
    ("cholesky back substitution without the diagonal", "b[r] = s2 / l[r * n + r];", "b[r] = s2;"),
    ("lu moves the pivot on a tie", "if v > big {", "if v >= big {"),
    ("lu never sees a singular matrix", "if big == 0.0 {", "if big < 0.0 {"),
    ("lu forward substitution adds", "s = s - a[i * n + j] * b[j];", "s = s + a[i * n + j] * b[j];"),
    ("lu skips the row swaps of b", "        if q != k {\n            let t = b[k];", "        if q < 0 {\n            let t = b[k];"),
    ("lstsq reflects the wrong way", "if x0 > 0.0 {\n            alpha = 0.0 - norm;", "if x0 < 0.0 {\n            alpha = 0.0 - norm;"),
    ("lstsq reflector without the 2", "let f = 2.0 * d / vnorm2;", "let f = d / vnorm2;"),
    ("lstsq does not reflect b", "b[k] = b[k] - fb * head;", "b[k] = b[k];"),
]


def build_run(linalg_text, tmp):
    mod = os.path.join(tmp, "linalg.cho")
    exe = os.path.join(tmp, "t")
    with open(mod, "w") as f:
        f.write(linalg_text)
    r = subprocess.run([sys.argv[1], "build", FIXTURE, os.path.join(ROOT, "std", "math.cho"), mod, "-o", exe], capture_output=True, text=True)
    if r.returncode:
        return None, r.stderr[-300:]
    r = subprocess.run([exe], capture_output=True, text=True, timeout=60)
    return r, ""


def main():
    want = [line[len("//~ STDOUT "):].rstrip("\n") for line in open(FIXTURE) if line.startswith("//~ STDOUT")]
    src = open(os.path.join(ROOT, "std", "linalg.cho")).read()
    bad = 0
    tmp = tempfile.mkdtemp()
    try:
        r, err = build_run(src, tmp)
        if r is None or r.stdout.split("\n")[: len(want)] != want or r.returncode != 0:
            sys.exit("the unmutated std/linalg.cho does not pass its fixture: " + err)
        print("ok   unmutated passes")
        for name, old, new in MUTANTS:
            if src.count(old) != 1:
                print(f"FAIL {name}: the text it replaces occurs {src.count(old)} times")
                bad += 1
                continue
            r, err = build_run(src.replace(old, new), tmp)
            if r is None:
                print(f"FAIL {name}: does not build: {err}")
                bad += 1
            elif r.stdout.split("\n")[: len(want)] == want and r.returncode == 0:
                print(f"FAIL {name}: SURVIVED")
                bad += 1
            else:
                print(f"ok   killed: {name}")
    finally:
        shutil.rmtree(tmp)
    sys.exit(1 if bad else 0)


main()
