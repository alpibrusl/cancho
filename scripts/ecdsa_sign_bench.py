#!/usr/bin/env python3
"""The cost of one P-256 signature (docs/ecdsa-sign.md §7).

    python3 scripts/ecdsa_sign_bench.py <sign driver> [<count>]

`sign driver` is `tests/programs/ecdsa_sign_driver.cho` built as
`conformance/ecdsa_sign.rs` builds it, with the backend to be measured.
Times `count` (default 2,000) signatures with random keys and digests and
32 bytes of added randomness, as `sign` (`S`) and as `sign_checked` (`C`,
the signature verified before it is given out, docs/tls-server.md §3.3),
each as one run of the driver, and subtracts a run of as many DER encodings
(`E`), which costs the driver's reading and printing and next to nothing
else. Prints the best of three runs of each, per signature.
"""
import os
import random
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ecdsa_sign_differential import N, public_point  # noqa: E402


def timed(driver, lines):
    data = "\n".join(lines) + "\n"
    best = None
    for _ in range(3):
        began = time.perf_counter()
        out = subprocess.run([driver], input=data, capture_output=True, text=True, check=True).stdout
        took = time.perf_counter() - began
        assert len(out.splitlines()) == len(lines)
        best = took if best is None else min(best, took)
    return best


def main():
    driver = sys.argv[1]
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 2000
    rng = random.Random(6)
    keys = [rng.randrange(1, N) for _ in range(20)]
    points = [public_point(d).hex() for d in keys]
    rows = []
    for _ in range(count):
        i = rng.randrange(len(keys))
        rows.append((rng.randbytes(32).hex(), keys[i].to_bytes(32, "big").hex(), points[i], rng.randbytes(32).hex()))
    base = timed(driver, [f"E {'7f' * 64}" for _ in rows])
    sign = timed(driver, [f"S {g} {d} {e}" for g, d, _, e in rows])
    checked = timed(driver, [f"C {g} {d} {q} {e}" for g, d, q, e in rows])
    per = lambda t: (t - base) / count * 1000
    print(f"{count} signatures: sign {per(sign):.3f} ms, sign_checked {per(checked):.3f} ms "
          f"(the check {per(checked) - per(sign):.3f} ms); {count / (sign - base):.0f} signatures/s")


if __name__ == "__main__":
    main()
