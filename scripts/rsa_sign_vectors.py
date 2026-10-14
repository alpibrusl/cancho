#!/usr/bin/env python3
# Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
"""Derives `tests/vectors/rsa_sign/nist_siggen15.txt` from NIST's `tests/vectors/cavp/SigGen15_186-3.txt` (docs/rsa-sign.md §11 G1).

    python3 scripts/rsa_sign_vectors.py [--check]

NIST's SigGen file gives each key as (n, e, d) -- "with the additional value d added to the file for testing purposes" -- and
messages with their signatures S (RSASSA-PKCS1-v1_5). `std.rsa_sign` signs with the Chinese remainder theorem, so p and q are
recovered from (n, e, d) (the standard probabilistic factoring of RFC 8017 / NIST SP 800-56B Appendix C, with fixed bases, so the
output is the same on every run), and each key is written as a PKCS#1 `RSA PRIVATE KEY` PEM, the way a key reaches the parser.
SHA-224 cases are counted, not written (`std.rsa_sign` has no SHA-224). Output lines:
    # ... header
    K <bits> <PEM hex>
    M <h> <msg hex> <signature hex>          h = 32, 48 or 64; belongs to the last K
"""
import math
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rsa_sign_lib as L  # noqa: E402

SRC = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "vectors", "cavp", "SigGen15_186-3.txt")
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "vectors", "rsa_sign", "nist_siggen15.txt")
H = {"SHA256": 32, "SHA384": 48, "SHA512": 64}


def factor(n, e, d):
    k = e * d - 1
    t = 0
    r = k
    while r % 2 == 0:
        r //= 2
        t += 1
    for g in range(2, 100):
        y = pow(g, r, n)
        if y in (1, n - 1):
            continue
        for _ in range(t):
            z = y * y % n
            if z == 1:
                p = math.gcd(y - 1, n)
                if 1 < p < n:
                    return min(p, n // p), max(p, n // p)
                break
            if z == n - 1:
                break
            y = z
    raise ValueError("no factor found")


def main():
    text = open(SRC).read()
    out = ["# Derived by scripts/rsa_sign_vectors.py from NIST CAVP SigGen15_186-3.txt (FIPS 186-4, RSASSA-PKCS1-v1_5 signature generation, test keys)."]
    skipped = 0
    for g in re.split(r"\n\[mod = ", text)[1:]:
        bits = int(g.split("]")[0])
        n, e, d = (int(re.search(rf"\n{f} = ([0-9a-f]+)", g).group(1), 16) for f in "ned")
        p, q = factor(n, e, d)
        assert p * q == n and n.bit_length() == bits
        k = dict(n=n, e=e, d=d, p=p, q=q, dp=d % (p - 1), dq=d % (q - 1), qinv=pow(q, -1, p))
        out.append(f"K {bits} {L.pem('RSA PRIVATE KEY', L.pkcs1(k)).hex()}")
        for alg, msg, sig in re.findall(r"SHAAlg = (\w+)\nMsg = ([0-9a-f]+)\nS = ([0-9a-f]+)", g):
            if alg not in H:
                skipped += 1
                continue
            h = H[alg]
            assert L.py_sign(k, h, L.digest_of(h, bytes.fromhex(msg))) == bytes.fromhex(sig), "NIST's S is not RFC 8017's signature (script bug)"
            out.append(f"M {h} {msg} {sig}")
    out.append(f"# {skipped} SHA-224 cases not written")
    new = "\n".join(out) + "\n"
    if "--check" in sys.argv:
        if not os.path.exists(OUT) or open(OUT).read() != new:
            print("tests/vectors/rsa_sign/nist_siggen15.txt is not what the generator makes", file=sys.stderr)
            sys.exit(1)
        print("nist_siggen15.txt: identical to a fresh run")
    else:
        os.makedirs(os.path.dirname(OUT), exist_ok=True)
        open(OUT, "w").write(new)
        print(f"wrote {OUT}: {sum(1 for l in out if l.startswith('M '))} cases, {skipped} SHA-224 skipped")


main()
