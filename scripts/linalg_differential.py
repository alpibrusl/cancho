#!/usr/bin/env python3
"""`std.linalg` against numpy (docs/linalg.md §4).

    python3 scripts/linalg_differential.py <cancho binary>

Builds `tests/programs/linalg_driver.cho` and runs seeded random problems of every operation through it. numpy's results are not the reference to the last bit (its BLAS sums in another
order): what is checked is that the two agree to what the conditioning allows. For a solve, the relative difference of the solutions must be at most `C * cond(A) * eps`; for a product,
each entry's difference at most `C * k * eps * (|A| |B|)`; for least squares the solution to `C * eps * (cond + cond^2 * |r| / (|A| |x|))` (the standard first-order bound, with r the residual) and the residual norm to `C * eps * (|b| + cond * |x|)`. C is 20.
The worst ratio found of each is printed: a number well under 1 is a bound with room. Exit status 1 if any case is outside its bound, or a status the two disagree about.
"""
import json
import os
import subprocess
import sys
import tempfile

import numpy as np

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EPS = np.finfo(float).eps
C = 20.0


def run(exe, doc):
    r = subprocess.run([exe], input=json.dumps(doc), capture_output=True, text=True)
    if r.returncode:
        sys.exit("the driver failed: " + r.stderr[-300:])
    return json.loads(r.stdout)


def flat(a):
    return [float(v) for v in np.asarray(a).reshape(-1)]


def main():
    exe = os.path.join(tempfile.mkdtemp(), "linalg_driver")
    r = subprocess.run([sys.argv[1], "build", os.path.join(ROOT, "tests", "programs", "linalg_driver.cho"), "--std", "-o", exe], capture_output=True, text=True)
    if r.returncode:
        sys.exit("cannot build the driver:\n" + r.stderr[-800:])
    rng = np.random.default_rng(20261026)
    worst = {"gemm": 0.0, "cholesky": 0.0, "lu": 0.0, "lstsq x": 0.0, "lstsq residual": 0.0}
    bad = 0
    cases = 0
    for _ in range(150):
        m, k, n = (int(v) for v in rng.integers(1, 9, 3))
        a = rng.normal(size=(m, k)) * 10.0 ** rng.integers(-3, 4)
        b = rng.normal(size=(k, n))
        got = np.array(run(exe, {"op": "gemm", "m": m, "k": k, "n": n, "a": flat(a), "b": flat(b)})["out"]).reshape(m, n)
        bound = C * k * EPS * (np.abs(a) @ np.abs(b))
        ratio = float(np.max(np.abs(got - a @ b) / np.maximum(bound, 1e-300)))
        worst["gemm"] = max(worst["gemm"], ratio)
        bad += ratio > 1.0
        cases += 1
    for _ in range(150):
        n = int(rng.integers(1, 9))
        g = rng.normal(size=(n, n))
        a = g @ g.T + n * 10.0 ** (-float(rng.integers(0, 8))) * np.eye(n)   # positive definite, from well to badly conditioned
        a = (a + a.T) / 2
        xs = rng.normal(size=n)
        b = a @ xs
        out = run(exe, {"op": "cholesky", "n": n, "a": flat(a), "b": flat(b)})
        want = np.linalg.solve(a, b)
        cond = np.linalg.cond(a)
        if out["status"] != 0:
            bad += 1
            print("FAIL cholesky refused a positive definite matrix:", n, cond)
            continue
        ratio = float(np.linalg.norm(np.array(out["out"]) - want) / np.linalg.norm(want) / (C * cond * EPS))
        worst["cholesky"] = max(worst["cholesky"], ratio)
        bad += ratio > 1.0
        cases += 1
    for _ in range(150):
        n = int(rng.integers(1, 9))
        a = rng.normal(size=(n, n))
        a[0, :] *= 1e-3   # so that the first pivot is not on the diagonal
        b = rng.normal(size=n)
        out = run(exe, {"op": "lu", "n": n, "a": flat(a), "b": flat(b)})
        want = np.linalg.solve(a, b)
        cond = np.linalg.cond(a)
        ratio = float(np.linalg.norm(np.array(out["out"]) - want) / np.linalg.norm(want) / (C * cond * EPS))
        worst["lu"] = max(worst["lu"], ratio)
        bad += ratio > 1.0 or out["status"] != 0
        cases += 1
    for _ in range(150):
        n = int(rng.integers(1, 6))
        m = n + int(rng.integers(0, 8))
        a = rng.normal(size=(m, n)) * rng.choice([1.0, 1e3, 1e-3], size=n)
        b = rng.normal(size=m)
        out = run(exe, {"op": "lstsq", "m": m, "n": n, "a": flat(a), "b": flat(b)})
        want, res, _, _ = np.linalg.lstsq(a, b, rcond=None)
        cond = np.linalg.cond(a)
        rn = float(np.linalg.norm(a @ want - b))
        # the first bound tried, C cond eps, was too small for problems with a large residual (1.4 of it at the worst); the standard one for least squares
        # (Golub and Van Loan 5.3.8) has a cond^2 term that grows with the residual
        sol = float(np.linalg.norm(want))
        bound = C * EPS * (cond + cond * cond * rn / (np.linalg.norm(a, 2) * sol))
        ratio = float(np.linalg.norm(np.array(out["out"]) - want) / sol / bound)
        worst["lstsq x"] = max(worst["lstsq x"], ratio)
        rr = abs(out["residual"] - rn) / (C * EPS * (np.linalg.norm(b) + cond * np.linalg.norm(want)))
        worst["lstsq residual"] = max(worst["lstsq residual"], float(rr))
        bad += ratio > 1.0 or rr > 1.0 or out["status"] != 0
        cases += 1
    # the numerical refusals, against numpy's
    for _ in range(40):
        n = int(rng.integers(2, 7))
        a = rng.normal(size=(n, n))
        a[:, int(rng.integers(0, n))] = 0.0   # a zero column: singular, exactly
        out = run(exe, {"op": "lu", "n": n, "a": flat(a), "b": [1.0] * n})
        if out["status"] == 0:
            bad += 1
            print("FAIL lu did not refuse a matrix with a zero column")
        out = run(exe, {"op": "cholesky", "n": n, "a": flat(-(a @ a.T) - np.eye(n)), "b": [1.0] * n})
        if out["status"] != 1:
            bad += 1
            print("FAIL cholesky did not refuse a negative definite matrix at the first pivot:", out["status"])
        cases += 2
    for name, v in worst.items():
        print(f"  {name:15} worst error / bound = {v:.3f}")
    print(("ok  " if not bad else "FAIL") + f" {cases} random problems against numpy {np.__version__}: {bad} outside their bounds")
    sys.exit(1 if bad else 0)


main()
