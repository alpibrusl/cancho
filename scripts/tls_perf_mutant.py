#!/usr/bin/env python3
"""Shows that the guard of docs/tls-performance.md §6 can fail (CONTRIBUTING.md: a test that could not have failed proves nothing).

    python3 scripts/tls_perf_mutant.py <cancho> [--baseline benches/tls_baseline.json]

Writes a copy of `tests/programs/tls_prims_bench.cho` in which the X25519 key pair and SHA-256 are each computed twice a call,
builds it with `<cancho>`, and runs `scripts/tls_perf.py check` on it. It passes when exactly the four rows those touch
(`x25519_public/0` and `sha256` at 64, 1,024 and 16,384 bytes) fail, at about 2.0, and every other row passes; anything
else is a failure of the guard, and the exit status is 1. Needs valgrind, as `check` does.
"""
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
EXPECTED = {"x25519_public/0", "sha256/64", "sha256/1024", "sha256/16384"}
MUTATIONS = [
    ("x25519.public_key(scalar, out32);\n", "x25519.public_key(scalar, out32);\n                x25519.public_key(scalar, out32);\n"),
    ("crypto.sha256(msg, out32);\n", "crypto.sha256(msg, out32);\n                crypto.sha256(msg, out32);\n"),
]


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    cancho = os.path.abspath(args[0])
    extra = args[1:]
    source = open(os.path.join(ROOT, "tests/programs/tls_prims_bench.cho")).read()
    for old, new in MUTATIONS:
        if source.count(old) != 1:
            print(f"the benchmark no longer has exactly one `{old.strip()}`: update this script", file=sys.stderr)
            return 1
        source = source.replace(old, new)
    work = tempfile.mkdtemp(prefix="tls-perf-mutant-")
    path, exe = os.path.join(work, "mutant.cho"), os.path.join(work, "mutant")
    open(path, "w").write(source)
    subprocess.run([cancho, "build", "--std", path, "-o", exe], check=True)
    r = subprocess.run([sys.executable, os.path.join(HERE, "tls_perf.py"), "check", exe, "--mode", "instructions"] + extra,
                       capture_output=True, text=True)
    print(r.stdout, end="")
    failed = set()
    for line in r.stdout.splitlines():
        m = re.match(r"(\S+)\s+(\d+)\s.*(SLOWER|CHEAPER|NOT MEASURED|NEW ROW)", line)
        if m:
            failed.add(f"{m.group(1)}/{m.group(2)}")
    if r.returncode == 1 and failed == EXPECTED:
        print(f"ok: the guard failed exactly the {len(EXPECTED)} mutated rows")
        return 0
    print(f"the guard did not fail as it should: exit {r.returncode}, failed rows {sorted(failed)}, expected {sorted(EXPECTED)}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
