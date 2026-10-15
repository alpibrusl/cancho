#!/usr/bin/env python3
# Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
"""Cost of a signature (docs/rsa-sign.md §13), against OpenSSL's in the same session. NOT publishable: one shared machine, no pinned core.

    python3 scripts/rsa_sign_bench.py <driver> <keydir> [rounds]

For each size, the time of one `pkcs1_sign` as it is meant to be used, then with the check off (mode 1: what verify-after-sign costs),
then with blinding off (mode 16: what the blinding costs), then `bigmod.pow_mod` with the secret d and no CRT (the leaky control, `L`):
`T <rounds> <case>` runs the case `rounds` times in one process; the difference between that and `T 1` is divided by `rounds - 1`; the
best of 5. Then `openssl speed -seconds 2 rsaNNNN`'s sign time.
"""
import os
import re
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rsa_sign_lib as L  # noqa: E402
from rsa_sign_differential import keys_of  # noqa: E402

driver, keydir = sys.argv[1], sys.argv[2]
rounds = int(sys.argv[3]) if len(sys.argv) > 3 else 200


def per_sign(line, n):
    best = None
    for _ in range(5):
        t = []
        for r in (1, n):
            s = time.perf_counter_ns()
            out = subprocess.run([driver], input=f"T {r} {line}\n", capture_output=True, text=True, check=True).stdout
            t.append(time.perf_counter_ns() - s)
            assert out.startswith("0 "), out[:80]
        d = (t[1] - t[0]) / (n - 1)
        best = d if best is None else min(best, d)
    return best / 1e6


rows = []
for bits in (2048, 3072, 4096):
    key = next(k for k in keys_of(keydir) if k["bits"] == bits and k["name"].endswith("_e65537_0"))
    blk = L.block(key).hex()
    d = L.digest_of(32, b"bench").hex()
    ent = bytes(range(16)).hex()
    n = (bits + 7) // 8
    n_rounds = max(20, rounds * 2048 // bits)
    full = per_sign(f"S 32 {blk} {d} {ent}", n_rounds)
    nochk = per_sign(f"U 32 {blk} {d} {ent} 1", n_rounds)
    noblind = per_sign(f"U 32 {blk} {d} - 16", n_rounds)
    leaky = per_sign(f"L {key['n']:0{2 * n}x} {key['d']:0{2 * n}x} {L.emsa(32, bytes.fromhex(d), n).hex()}", max(10, n_rounds // 3))
    rows.append((bits, full, nochk, noblind, leaky))
ossl = subprocess.run(["openssl", "speed", "-seconds", "2", "rsa2048", "rsa3072", "rsa4096"], capture_output=True, text=True).stdout
speeds = {int(m.group(1)): float(m.group(2)) * 1000 for m in re.finditer(r"rsa\s+(\d+) bits\s+([0-9.]+)s", ossl)}
print(f"{'bits':>5} {'OpenSSL sign':>13} {'rsa_sign':>10} {'x OpenSSL':>10} {'no check':>10} {'check costs':>12} {'no blinding':>12} {'blinding costs':>15} {'no CRT (pow_mod)':>17}  (ms per signature)")
for bits, full, nochk, noblind, leaky in rows:
    o = speeds.get(bits, float("nan"))
    print(f"{bits:5d} {o:13.3f} {full:10.3f} {full / o:10.2f} {nochk:10.3f} {(full - nochk) / full * 100:11.1f}% {noblind:12.3f} {(full - noblind) / full * 100:14.1f}% {leaky:17.3f}")
