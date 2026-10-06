#!/usr/bin/env python3
"""How fast `std.ed25519` signs and verifies and `std.x25519` computes (docs/x25519.md §6).

    cancho build --std tests/programs/ed25519_bench.cho -o ed25519_bench
    python3 scripts/curve25519_bench.py ./ed25519_bench <ed25519 rounds> <x25519 rounds>

Each figure is the median of five runs of `rounds` operations, less the median of five runs of one, divided by `rounds - 1`.
"""
import statistics
import subprocess
import sys
import time

exe = sys.argv[1]
for op, name, rounds in ((0, "sign", int(sys.argv[2])), (1, "verify", int(sys.argv[2])), (2, "x25519", int(sys.argv[3]))):
    r = {}
    for n in (1, rounds):
        ts = []
        for _ in range(5):
            t = time.perf_counter(); subprocess.run([exe, str(op), str(n)], check=True, capture_output=True); ts.append(time.perf_counter() - t)
        r[n] = statistics.median(ts)
    dt = (r[rounds] - r[1]) / (rounds - 1)
    print(f"{name}: {dt*1e3:.3f} ms each, {1/dt:.1f} per second")
