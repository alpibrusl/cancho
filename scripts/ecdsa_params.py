#!/usr/bin/env python3
"""Prints `std/ecdsa.ls`'s curve constants from OpenSSL (docs/ecdsa.md §2.1).

    python3 scripts/ecdsa_params.py

For P-256 and P-384, `openssl ecparam -param_enc explicit` gives p, a, b, G
and n; this checks that a = p - 3, that G is on the curve, that n is prime
(Miller-Rabin, 64 rounds) and that n·G is the point at infinity, then
prints the constants as hex strings. No value is typed by hand.
"""
import random
import re
import subprocess

CURVES = (("p256", "prime256v1"), ("p384", "secp384r1"))


def params(name):
    der = subprocess.run(["openssl", "ecparam", "-name", name, "-param_enc", "explicit", "-outform", "DER"],
                         capture_output=True, check=True).stdout
    text = subprocess.run(["openssl", "asn1parse", "-inform", "DER"], input=der, capture_output=True, check=True).stdout.decode()
    ints = re.findall(r"prim: INTEGER\s+:([0-9A-F]+)", text)
    octets = re.findall(r"prim: OCTET STRING\s+\[HEX DUMP\]:([0-9A-F]+)", text)
    p, n = int(ints[1], 16), int(ints[2], 16)
    a, b = int(octets[0], 16), int(octets[1], 16)
    g = octets[2]
    size = (p.bit_length() + 7) // 8
    assert g[:2] == "04"
    gx, gy = int(g[2 : 2 + 2 * size], 16), int(g[2 + 2 * size :], 16)
    return p, a, b, gx, gy, n, size


def is_prime(n, rng):
    d, r = n - 1, 0
    while d % 2 == 0:
        d, r = d // 2, r + 1
    for _ in range(64):
        x = pow(rng.randrange(2, n - 1), d, n)
        if x in (1, n - 1):
            continue
        for _ in range(r - 1):
            x = x * x % n
            if x == n - 1:
                break
        else:
            return False
    return True


def mul(k, pt, p, a):
    def add(P, Q):
        if P is None:
            return Q
        if Q is None:
            return P
        if P[0] == Q[0] and (P[1] + Q[1]) % p == 0:
            return None
        if P == Q:
            m = (3 * P[0] * P[0] + a) * pow(2 * P[1], -1, p) % p
        else:
            m = (Q[1] - P[1]) * pow(Q[0] - P[0], -1, p) % p
        x = (m * m - P[0] - Q[0]) % p
        return x, (m * (P[0] - x) - P[1]) % p

    r = None
    while k:
        if k & 1:
            r = add(r, pt)
        pt = add(pt, pt)
        k >>= 1
    return r


def main():
    rng = random.Random(204)
    for short, name in CURVES:
        p, a, b, gx, gy, n, size = params(name)
        assert a == p - 3, name
        assert (gy * gy - gx ** 3 - a * gx - b) % p == 0, f"{name}: G is not on the curve"
        assert is_prime(p, rng) and is_prime(n, rng), name
        assert mul(n, (gx, gy), p, a) is None, f"{name}: n·G is not infinity"
        for label, v in (("p", p), ("b", b), ("gx", gx), ("gy", gy), ("n", n)):
            print(f"fn {short}_{label}() -> [] &static [byte] {{")
            print(f'    return "{v:0{2 * size}x}";')
            print("}\n")


if __name__ == "__main__":
    main()
