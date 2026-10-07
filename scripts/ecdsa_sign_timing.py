#!/usr/bin/env python3
"""A dudect-style timing test of `std.ecdsa_sign` (docs/ecdsa-sign.md §6).

    cancho build --std [--backend B] tests/programs/ecdsa_sign_timing.cho -l tick -L . -o timing
    python3 scripts/ecdsa_sign_timing.py ./timing [<samples per test>] [<batch>]

`tick` is the cycle counter `scripts/gcm_timing.py` describes. Each test
times `ecdsa_sign.sign` over one message, the private key from one of two
classes picked at random, as `scripts/ecdh_timing.py` does for a scalar:

- one random key, fixed, against random keys in [1, n);
- the key 1 against random keys: the case that found `std.ecdh`'s leak
  (docs/ecdh.md §3), here reaching `r·d` with a one-limb d.

The message and RFC 6979's added randomness are the same for every call,
so a fixed key gives the same nonce every time and a random key a random
nonce: the whole secret input is fixed against random. Samples are taken
in batches (default 50,000), each one run of the program, the classes
interleaved within a batch. Welch's t and dudect's crops are
`scripts/gcm_timing.py`'s; |t| below 4.5 is no evidence of a leak. The
default is 10^6 samples a test (docs/tls-server.md §3.4). Exit status 1 if
any test reaches 4.5.
"""
import os
import random
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gcm_timing import max_t  # noqa: E402

N = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551


def run(exe, digest, records):
    data = digest.hex().encode() + b"\n" + b"".join(records).hex().encode()
    out = subprocess.run([exe], input=data, capture_output=True, check=True).stdout
    return [int(x) for x in out.decode().split()]


def main():
    exe = sys.argv[1]
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 1000000
    batch = int(sys.argv[3]) if len(sys.argv) > 3 else 50000
    rng = random.Random(335)
    digest = rng.randbytes(32)
    extra = rng.randbytes(32)
    fixed = rng.randrange(1, N)
    leaked = False
    for name, first in (("fixed key", fixed), ("key 1", 1)):
        samples = []
        began = time.time()
        while len(samples) < n:
            m = min(batch, n - len(samples) + 100)
            classes = [rng.randrange(2) for _ in range(m)]
            keys = [first if c == 0 else rng.randrange(1, N) for c in classes]
            times = run(exe, digest, [k.to_bytes(32, "big") + extra for k in keys])
            assert len(times) == m
            # The first calls of each run warm the caches.
            samples += list(zip(classes, times))[100:]
            print(f"  {name}: {len(samples)} samples, {time.time() - began:.0f} s", file=sys.stderr, flush=True)
        t = max_t(samples)
        med = sorted(x for _, x in samples)[len(samples) // 2]
        leaked |= t >= 4.5
        print(f"P-256 sign, {name:9} {len(samples)} samples, median {med} cycles, max |t| = {t:.2f}", flush=True)
    sys.exit(1 if leaked else 0)


if __name__ == "__main__":
    main()
