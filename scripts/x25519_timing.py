#!/usr/bin/env python3
"""A dudect-style timing test of `std.x25519` (docs/tls-assurance.md §6).

    lex-sys build --std [--backend B] tests/programs/x25519_timing.ls -l tick -L . -o timing
    python3 scripts/x25519_timing.py ./timing [<samples per test>]

`tick` is the cycle counter `scripts/gcm_timing.py` describes. Each test
times `x25519.scalarmult` on one peer u-coordinate many times, the scalar
from one of two classes picked at random. Welch's t and dudect's crops are
`scripts/gcm_timing.py`'s; |t| below 4.5 is no evidence of a leak. Exit
status 1 if any test reaches 4.5.

The tests:
- a fixed random scalar against random scalars;
- a scalar with few bits set (only those RFC 7748 §5's clamping sets, and
  bit 3) against random scalars: the ladder's swaps then almost never swap.
"""
import os
import random
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gcm_timing import max_t  # noqa: E402

from cryptography.hazmat.primitives.asymmetric import x25519  # noqa: E402


def run(exe, records):
    out = subprocess.run([exe], input=b"".join(records).hex().encode(), capture_output=True, check=True).stdout
    return [int(x) for x in out.decode().split()]


def main():
    exe = sys.argv[1]
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 20000
    rng = random.Random(208)
    peer = x25519.X25519PrivateKey.from_private_bytes(rng.randbytes(32)).public_key().public_bytes_raw()
    sparse = bytes([8] + [0] * 30 + [64])
    leaked = False
    for name, first in (("fixed scalar", rng.randbytes(32)), ("sparse scalar", sparse)):
        classes = [rng.randrange(2) for _ in range(n)]
        times = run(exe, [(first if c == 0 else rng.randbytes(32)) + peer for c in classes])
        assert len(times) == n
        # The first hundred warm the caches and the branch predictor.
        samples = list(zip(classes, times))[100:]
        t = max_t(samples)
        med = sorted(times)[len(times) // 2]
        leaked |= t >= 4.5
        print(f"{name:13} {n} samples, median {med} cycles, max |t| = {t:.2f}", flush=True)
    sys.exit(1 if leaked else 0)


if __name__ == "__main__":
    main()
