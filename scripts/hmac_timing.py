#!/usr/bin/env python3
"""A dudect-style timing test of SHA-256, HMAC-SHA256 and a prepared HMAC key (docs/sha256-builtin.md §6).

    cancho build --std tests/programs/hmac_timing.cho -l tick -L <dir with libtick.a> -o hmac_timing
    python3 scripts/hmac_timing.py ./hmac_timing [<samples per test>] [--pool <n>] [--only <name>] [--control]

`libtick.a` is `scripts/gcm_timing.py`'s cycle counter. Each test times one call `<samples>` times (default 100,000),
each sample from one of two classes picked at random (Reparaz, Balasch and Verbauwhede, "Dude, is my code constant
time?", 2017), a fixed input against a random one, its inputs from two pools of `--pool` (default 256) records a class,
interleaved in memory so that where a record lies says nothing about its class, every record decoded, and every key prepared,
before anything is timed. `--control` runs the same tests with both classes random: a harness that shows a difference there is
measuring where the records lie, not what they hold. Welch's t over all samples and over dudect's
percentile crops, the largest |t| reported, as `scripts/gcm_timing.py` computes it: below 4.5 is no evidence of a leak.
Exit status 1 if any test reaches 4.5.

The tests, under a 32-byte key (a SHA-256 traffic secret's length) and over a 64-byte message:
- hmac, key: `hmac.sha256`, an all-zero key against random ones;
- hmac, message: one key, an all-zero message against random ones;
- prepare, key: `hmac.prepare`, the two padded blocks alone, an all-zero key against random ones;
- prepared, key: `hmac.mac_prepared`, a prepared all-zero key against prepared random ones;
- prepared, message: one prepared key, an all-zero message against random ones;
- sha256, message: `crypto.sha256`, an all-zero message against random ones;
- expand-label, secret: `hkdf.expand_label(32, secret, "key", "", 16 bytes)`, an all-zero secret against random ones.
"""
import os
import random
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gcm_timing import max_t  # noqa: E402

KEY = 32
MSG = 64


def run_pooled(exe, op, fixed, rand, classes, pool):
    # Interleaved, fixed then random: both classes' records spread over the same memory (tests/programs/hmac_timing.cho).
    recs = [r for _ in range(pool) for r in (fixed(), rand())]
    data = (f"{op} {KEY} {MSG} {pool} {len(classes)}\n".encode()
            + b"".join(k + m for k, m in recs).hex().encode() + b"\n"
            + "".join(str(c) for c in classes).encode())
    out = subprocess.run([exe], input=data, capture_output=True, check=True).stdout
    return [int(x) for x in out.decode().split()]


def main():
    control = "--control" in sys.argv
    args = [a for a in sys.argv[1:] if a != "--control"]
    pool, only = 256, ""
    if "--pool" in args:
        at = args.index("--pool")
        pool = int(args[at + 1])
        del args[at:at + 2]
    if "--only" in args:
        at = args.index("--only")
        only = args[at + 1]
        del args[at:at + 2]
    exe = args[0]
    n = int(args[1]) if len(args) > 1 else 100000
    rng = random.Random(4231)
    key = rng.randbytes(KEY)
    msg = rng.randbytes(MSG)
    zk, zm = bytes(KEY), bytes(MSG)
    tests = {
        "hmac, key": ("M", lambda: (zk, msg), lambda: (rng.randbytes(KEY), msg)),
        "hmac, message": ("M", lambda: (key, zm), lambda: (key, rng.randbytes(MSG))),
        "prepare, key": ("P", lambda: (zk, msg), lambda: (rng.randbytes(KEY), msg)),
        "prepared, key": ("Q", lambda: (zk, msg), lambda: (rng.randbytes(KEY), msg)),
        "prepared, message": ("Q", lambda: (key, zm), lambda: (key, rng.randbytes(MSG))),
        "sha256, message": ("H", lambda: (key, zm), lambda: (key, rng.randbytes(MSG))),
        "expand-label, secret": ("L", lambda: (zk, msg), lambda: (rng.randbytes(KEY), msg)),
    }
    print(f"{KEY}-byte keys, {MSG}-byte messages, pools of {pool} records a class", flush=True)
    if control:
        tests = {f"{name} (control)": (op, rand, rand) for name, (op, _, rand) in tests.items()}
    leaked = False
    for name, (op, fixed, rand) in tests.items():
        if only not in name:
            continue
        classes = [rng.randrange(2) for _ in range(n)]
        times = run_pooled(exe, op, fixed, rand, classes, pool)
        assert len(times) == n
        # The first thousand warm the caches and the branch predictor.
        samples = list(zip(classes, times))[1000:]
        t = max_t(samples)
        med = sorted(times)[len(times) // 2]
        leaked |= t >= 4.5
        print(f"{name:21} {n} samples, median {med} cycles, max |t| = {t:.2f}", flush=True)
    sys.exit(1 if leaked else 0)


if __name__ == "__main__":
    main()
