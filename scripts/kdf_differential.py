#!/usr/bin/env python3
"""Differential test of `std.crypto`'s SHA-2, `std.hmac` and `std.hkdf` (docs/hkdf.md §4.3).

    python3 scripts/kdf_differential.py <driver> [<count>]

`driver` is `tests/programs/kdf_driver.cho` built with `cancho build --std`.
The references: Python's `hashlib` and `hmac` (OpenSSL's digests underneath),
pyca/cryptography's `HKDF` (OpenSSL's `EVP_KDF`), and, for HKDF-Expand-Label,
the RFC 8446 §7.1 structure built in Python over `hmac`.

For each of `count` (default 10,000) rounds, with random sizes: a SHA-256,
SHA-384 and SHA-512 digest (one-shot, and streamed in random pieces; one round
in fifty is 64 to 200 KiB, past the arena the old code copied into); an
HMAC-SHA256 and HMAC-SHA384 with keys of 0 to 300 bytes; a whole HKDF under
each hash; and an Expand-Label. Exit status 1 on any difference.
"""
import hashlib
import hmac
import random
import subprocess
import sys

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.hkdf import HKDF


def h(b):
    return b.hex() if b else "-"


def expand_label(hl, secret, label, ctx, n):
    full = b"tls13 " + label
    info = n.to_bytes(2, "big") + bytes([len(full)]) + full + bytes([len(ctx)]) + ctx
    out, t, i = b"", b"", 1
    digest = hashlib.sha256 if hl == 32 else hashlib.sha384
    while len(out) < n:
        t = hmac.new(secret, t + info + bytes([i]), digest).digest()
        out += t
        i += 1
    return out[:n]


def main():
    driver = sys.argv[1]
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 10000
    rng = random.Random(5869)
    algs = {256: hashlib.sha256, 384: hashlib.sha384, 512: hashlib.sha512}
    cases, want = [], []
    for n in range(count):
        size = rng.randrange(65536, 204800) if n % 50 == 0 else rng.randrange(0, 600)
        msg = rng.randbytes(size)
        alg = rng.choice([256, 384, 512])
        d = algs[alg](msg).hexdigest()
        cases.append(f"H {alg} {h(msg)}")
        want.append(d)
        cases.append(f"U {alg} {rng.randrange(1, 300)} {h(msg)}")
        want.append(d)
        for hl, digest in ((32, hashlib.sha256), (48, hashlib.sha384)):
            key = rng.randbytes(rng.randrange(0, 301))
            m = rng.randbytes(rng.randrange(0, 600))
            cases.append(f"M {hl} {hl} {h(key)} {h(m)}")
            want.append(hmac.new(key, m, digest).hexdigest())
            salt = rng.randbytes(rng.choice([0, rng.randrange(1, 200)]))
            ikm = rng.randbytes(rng.randrange(0, 200))
            info = rng.randbytes(rng.randrange(0, 300))
            length = rng.randrange(1, 255 * hl + 1) if n % 20 == 0 else rng.randrange(1, 200)
            algo = hashes.SHA256() if hl == 32 else hashes.SHA384()
            okm = HKDF(algorithm=algo, length=length, salt=salt or None, info=info).derive(ikm)
            cases.append(f"K {hl} {h(salt)} {h(ikm)} {h(info)} {length}")
            want.append(okm.hex())
            secret = rng.randbytes(hl)
            label = rng.randbytes(rng.randrange(1, 250))
            ctx = rng.randbytes(rng.randrange(0, 256))
            out = rng.randrange(0, 300)
            cases.append(f"L {hl} {h(secret)} {h(label)} {h(ctx)} {out}")
            want.append(expand_label(hl, secret, label, ctx, out).hex())

    run = subprocess.run([driver], input=("\n".join(cases) + "\n").encode(), capture_output=True, check=True)
    lines = run.stdout.decode().splitlines()
    assert len(lines) == len(cases), f"{len(lines)} answers to {len(cases)} cases"
    bad = 0
    for case, w, line in zip(cases, want, lines):
        if line.rstrip() != f"0 ok {w}".rstrip():
            bad += 1
            if bad <= 10:
                print(f"DIFFERENT {case[:50]}: {line[:80]}")
    print(f"{count} rounds, {len(cases)} checks, {bad} differences")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
