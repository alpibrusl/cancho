# Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
"""Shared helpers of the `std.rsa_sign` gates (docs/rsa-sign.md §11): a seeded generator of THROWAWAY RSA keys (never a real
key; nothing here is used anywhere but in tests), DER and PEM writers, the key block's layout, an independent RFC 8017
signer in pure Python (the second oracle, written from the RFC text), and the driver's line protocol.

Python's `random` is seeded from a string, so every key is the same on every run (the committed vectors are regenerated and
compared with `--check`). The generator is Miller-Rabin over `random.Random`: it is a test fixture, not a key generator.
"""
import base64
import hashlib
import json
import math
import os
import random
import subprocess

PREFIX = {  # RFC 8017 §9.2 note 1: DigestInfo prefixes for SHA-256/384/512 (checked against OpenSSL by the differential)
    32: bytes.fromhex("3031300d060960864801650304020105000420"),
    48: bytes.fromhex("3041300d060960864801650304020205000430"),
    64: bytes.fromhex("3051300d060960864801650304020305000440"),
}
HASH = {32: "sha256", 48: "sha384", 64: "sha512"}

_SMALL = [p for p in range(3, 2000) if all(p % q for q in range(2, int(p ** 0.5) + 1))]


def is_prime(n, rng, rounds=24):
    if n < 4:
        return n in (2, 3)
    if n % 2 == 0:
        return False
    for sp in _SMALL:
        if n % sp == 0:
            return n == sp
    d, r = n - 1, 0
    while d % 2 == 0:
        d //= 2
        r += 1
    for i in range(rounds):
        a = 2 if i == 0 else rng.randrange(2, n - 2)
        x = pow(a, d, n)
        if x in (1, n - 1):
            continue
        for _ in range(r - 1):
            x = x * x % n
            if x == n - 1:
                break
        else:
            return False
    return True


def prime(bits, rng, e, top=3, lo=None, hi=None):
    """A prime of exactly `bits` bits with the top `top` bits set (3: OpenSSL's shape), or in [lo, hi), with gcd(e, p-1) = 1."""
    while True:
        if lo is not None:
            c = rng.randrange(lo, hi) | 1
        else:
            c = rng.getrandbits(bits) | (((1 << top) - 1) << (bits - top)) | 1
        if (c - 1) % e != 0 and is_prime(c, rng):
            return c


def make_key(p, q, e):
    n = p * q
    lam = (p - 1) * (q - 1) // math.gcd(p - 1, q - 1)
    d = pow(e, -1, lam)
    return dict(n=n, e=e, d=d, p=p, q=q, dp=d % (p - 1), dq=d % (q - 1), qinv=pow(q, -1, p))


def swapped(k):
    s = dict(k)
    s.update(p=k["q"], q=k["p"], dp=k["dq"], dq=k["dp"], qinv=pow(k["p"], -1, k["q"]))
    return s


# ---- DER ----

def tlv(tag, content):
    n = len(content)
    if n < 128:
        ln = bytes([n])
    else:
        b = n.to_bytes((n.bit_length() + 7) // 8, "big")
        ln = bytes([0x80 | len(b)]) + b
    return bytes([tag]) + ln + content


def der_int(x):
    if x < 0:
        raise ValueError("negative")
    b = x.to_bytes(max(1, (x.bit_length() + 7) // 8), "big")
    if b[0] & 0x80:
        b = b"\x00" + b
    return tlv(0x02, b)


OID_RSA = bytes.fromhex("2a864886f70d010101")
ALG_RSA = tlv(0x30, tlv(0x06, OID_RSA) + tlv(0x05, b""))


def pkcs1(k, version=0, fields=None):
    f = fields or [k["n"], k["e"], k["d"], k["p"], k["q"], k["dp"], k["dq"], k["qinv"]]
    return tlv(0x30, der_int(version) + b"".join(der_int(x) for x in f))


def pkcs8(k, inner=None, alg=ALG_RSA, version=0):
    return tlv(0x30, der_int(version) + alg + tlv(0x04, inner if inner is not None else pkcs1(k)))


def pem(label, der, width=64, eol=b"\n", extra_header=b""):
    b = base64.b64encode(der)
    lines = [b[i:i + width] for i in range(0, len(b), width)]
    return b"-----BEGIN " + label.encode() + b"-----" + eol + extra_header + eol.join(lines) + eol + b"-----END " + label.encode() + b"-----" + eol


def block(k):
    """The key block of docs/rsa-sign.md §3.2: n || e (8) || p || q || dp || dq || qinv, h = len(p), fixed width."""
    h = (k["p"].bit_length() + 7) // 8
    L = 2 * h
    return (k["n"].to_bytes(L, "big") + k["e"].to_bytes(8, "big") + b"".join(k[f].to_bytes(h, "big") for f in ("p", "q", "dp", "dq", "qinv")))


# ---- the second oracle: RFC 8017 §8.2.1 and §9.2, from the text ----

def emsa(h, digest, k):
    t = PREFIX[h] + digest
    return b"\x00\x01" + b"\xff" * (k - len(t) - 3) + b"\x00" + t


def py_sign(key, h, digest):
    k = (key["n"].bit_length() + 7) // 8
    return pow(int.from_bytes(emsa(h, digest, k), "big"), key["d"], key["n"]).to_bytes(k, "big")


def digest_of(h, msg):
    return hashlib.new(HASH[h], msg).digest()


# ---- keys on disk for the differential ----

def key_json(k):
    return {f: format(k[f], "x") for f in ("n", "e", "d", "p", "q", "dp", "dq", "qinv")}


def load_json(path):
    j = json.load(open(path))
    k = {f: int(v, 16) for f, v in j.items()}
    k["bits"] = k["n"].bit_length()
    return k


def run_driver(driver, lines, timeout=None):
    out = subprocess.run([driver], input="\n".join(lines) + "\n", capture_output=True, text=True, timeout=timeout)
    res = out.stdout.splitlines()
    return res, out.returncode, out.stderr


def hx(n, k):
    return f"{n:0{2 * k}x}"


def h_(b):
    return b.hex() if b else "-"
