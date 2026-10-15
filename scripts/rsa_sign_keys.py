#!/usr/bin/env python3
# Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
"""Throwaway RSA keys and the two committed key corpora of `std.rsa_sign` (docs/rsa-sign.md §11, gates G2 and G3).

    python3 scripts/rsa_sign_keys.py keys <dir>          the keys of the differential: <dir>/<name>.json (integers, hex) and
                                                         <dir>/<name>.pem (PKCS#8), 2,048/3,072/4,096 bits, e = 65537 and 3,
                                                         balanced and unbalanced primes in both orders. NOT committed.
    python3 scripts/rsa_sign_keys.py malformed [--check] tests/vectors/rsa_sign/malformed.txt
    python3 scripts/rsa_sign_keys.py valid [--check]     tests/vectors/rsa_sign/valid.txt

`malformed.txt` is the refusal corpus. Every line is `<parse> <sign> <name> <signature or -> <file hex>`:
    parse   the rule `x509_rsa_key.parse_pem` must answer (`ok` or a tag of docs/rsa-sign.md §6)
    sign    for a key that parses: `ok` (and the signature of SHA-256("rsa-sign corpus") is the fourth column, made by the
            Python reference) or the tag signing must answer; `-` when the key is refused
The expected rules were written HERE, from docs/rsa-sign.md §6, before the parser existed. OpenSSL is not the oracle: B2 found
that OpenSSL 4.0.3 accepts 16 of 28 malformed keys. Every key is a throwaway built from a fixed seed; none is used anywhere.
Generating takes a few minutes the first time (pure-Python primes); `RSA_SIGN_CACHE=<dir>` keeps them.
"""
import hashlib
import json
import math
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rsa_sign_lib as L  # noqa: E402

CACHE = os.environ.get("RSA_SIGN_CACHE")


def pool(bits, name, i, e=65537, top=3, lo=None, hi=None):
    """The i-th prime of a named stream (cached on disk when RSA_SIGN_CACHE is set)."""
    key = f"{name}-{bits}-{i}-{e}-{top}-{lo}-{hi}"
    path = os.path.join(CACHE, hashlib.sha256(key.encode()).hexdigest()[:24]) if CACHE else None
    if path and os.path.exists(path):
        return int(open(path).read(), 16)
    rng = random.Random("rsa-sign/" + key)
    p = L.prime(bits, rng, e, top=top, lo=lo, hi=hi)
    if path:
        os.makedirs(CACHE, exist_ok=True)
        open(path, "w").write(format(p, "x"))
    return p


def balanced(bits, name, i, e=65537):
    """A key of `bits` bits, primes from two streams, p != q."""
    p = pool(bits // 2, name + "-p", i, e)
    q = pool(bits // 2, name + "-q", i, e)
    return L.make_key(p, q, e)


def unbalanced(bits, name, i, order):
    h = bits // 2
    lo_p, hi_p = (1 << (h - 1)) * 110 // 100, (1 << (h - 1)) * 112 // 100
    lo_q, hi_q = (1 << (h - 1)) * 190 // 100, (1 << (h - 1)) * 199 // 100
    a = pool(h, name + "-a", i, 65537, lo=lo_p, hi=hi_p)
    b = pool(h, name + "-b", i, 65537, lo=lo_q, hi=hi_q)
    p, q = (a, b) if order == "pq" else (b, a)
    k = L.make_key(p, q, 65537)
    assert k["n"].bit_length() == bits and a.bit_length() == h and b.bit_length() == h
    return k


def all_keys():
    ks = {}
    for bits, count in ((2048, 3), (3072, 2), (4096, 2)):
        for i in range(count):
            ks[f"k{bits}_e65537_{i}"] = balanced(bits, "std", i)
    for i in range(2):
        ks[f"k2048_e3_{i}"] = balanced(2048, "e3", i, e=3)
    ks["k3072_e3_0"] = balanced(3072, "e3", 0, e=3)
    ks["k2048_e17_0"] = balanced(2048, "e17", 0, e=17)
    for bits in (2048, 3072, 4096):
        ks[f"k{bits}_e65537_upq"] = unbalanced(bits, "unb", 0, "pq")
        ks[f"k{bits}_e65537_uqp"] = unbalanced(bits, "unb", 0, "qp")
    for name in [n for n in ks if n.endswith("_0") and "e65537" in n]:
        ks[name.replace("_0", "_swapped")] = L.swapped(ks[name])
    # a modulus that is not a multiple of 1,024 bits: 2,176 = 2 x 1,088
    ks["k2176_e65537_0"] = balanced(2176, "odd", 0)
    return ks


def cmd_keys(out):
    os.makedirs(out, exist_ok=True)
    for name, k in sorted(all_keys().items()):
        json.dump(L.key_json(k), open(os.path.join(out, name + ".json"), "w"))
        open(os.path.join(out, name + ".pem"), "wb").write(L.pem("PRIVATE KEY", L.pkcs8(k)))
        print(name, k["n"].bit_length(), "bits", "e =", k["e"])


# ---- the refusal corpus ----

MSG = b"rsa-sign corpus"


def sign_col(k):
    return L.py_sign(k, 32, L.digest_of(32, MSG)).hex()


def corpus():
    B = balanced(2048, "corpus", 0)
    n, e, d, p, q, dp, dq, qinv = (B[f] for f in ("n", "e", "d", "p", "q", "dp", "dq", "qinv"))
    order = ("n", "e", "d", "p", "q", "dp", "dq", "qinv")
    ints = {f: L.der_int(B[f]) for f in order}

    def seq(*elts):
        return L.tlv(0x30, b"".join(elts))

    def pk1(version=0, over=None, drop=(), extra=()):
        parts = dict(ints)
        parts.update(over or {})
        elts = [L.der_int(version)] + [parts[f] for f in order if f not in drop] + list(extra)
        return seq(*elts)

    def with_key(k):
        return pk1(over={f: L.der_int(k[f]) for f in order})

    def pk8(inner, alg=L.ALG_RSA, version=0, tail=b""):
        return L.tlv(0x30, L.der_int(version) + alg + L.tlv(0x04, inner) + tail)

    def key_from(pp, qq, ee=65537, **over):
        k = L.make_key(pp, qq, ee)
        k.update(over)
        return k

    cases = []  # (parse, sign, name, file bytes, key for the signature column)

    def add(parse, name, data, sign=None, key=None):
        if sign is None:
            sign = "ok" if parse == "ok" else "-"
        cases.append((parse, sign, name, data, key if (parse == "ok" and sign == "ok") else None))

    P = lambda der, label="RSA PRIVATE KEY", **kw: L.pem(label, der, **kw)  # noqa: E731
    good1, good8 = pk1(), pk8(pk1())
    # ---- accepted, in every form a file takes ----
    add("ok", "valid-pkcs1", P(good1), key=B)
    add("ok", "valid-pkcs8", P(good8, "PRIVATE KEY"), key=B)
    add("ok", "valid-pkcs1-crlf", P(good1, eol=b"\r\n"), key=B)
    add("ok", "valid-pkcs1-width-76", P(good1, width=76), key=B)
    add("ok", "valid-pkcs1-single-line", P(good1, width=100000), key=B)
    add("ok", "valid-pkcs1-leading-text", b"Bag Attributes\n    friendlyName: test\n" + P(good1), key=B)
    add("ok", "valid-after-certificate-block", L.pem("CERTIFICATE", b"\x30\x03\x02\x01\x00") + P(good8, "PRIVATE KEY"), key=B)
    add("ok", "valid-pkcs1-spaces-in-base64", P(good1).replace(b"\n", b"\n  ", 3), key=B)
    add("ok", "valid-e3", P(with_key(balanced(2048, "e3", 0, e=3))), key=balanced(2048, "e3", 0, e=3))
    k3 = balanced(3072, "e3", 0, e=3)
    add("ok", "valid-3072-e3-pkcs8", P(L.pkcs8(k3), "PRIVATE KEY"), key=k3)
    add("ok", "valid-p-gt-q", P(with_key(L.swapped(B))), key=L.swapped(B))
    ub = unbalanced(2048, "unb", 0, "pq")
    add("ok", "valid-unbalanced-q-over-p", P(with_key(ub)), key=ub)
    add("ok", "valid-unbalanced-p-over-q", P(with_key(L.swapped(ub))), key=L.swapped(ub))
    k2176 = balanced(2176, "odd", 0)
    add("ok", "valid-2176-bits", P(with_key(k2176)), key=k2176)
    add("ok", "valid-4096", P(L.pkcs8(balanced(4096, "std", 0)), "PRIVATE KEY"), key=balanced(4096, "std", 0))
    # ---- PEM ----
    add("rsa-key-pem", "no-block", b"this is not a key\n")
    add("rsa-key-pem", "empty-file", b"")
    add("rsa-key-pem", "truncated-no-end", P(good1)[:-40])
    add("rsa-key-pem", "bad-base64-character", P(good1).replace(b"A", b"*", 1))
    add("rsa-key-pem", "bad-base64-padding", P(good1).replace(b"\n-----END", b"===\n-----END"))
    lines1 = P(good1).split(b"\n")
    lines1[1] += b"="
    add("rsa-key-pem", "padding-inside-body", b"\n".join(lines1))
    add("rsa-key-pem", "end-label-differs", P(good1).replace(b"END RSA PRIVATE KEY", b"END PRIVATE KEY"))
    add("rsa-key-pem", "end-label-same-length", P(good1, "RSA PRIVATE KEY").replace(b"-----END RSA PRIVATE KEY", b"-----END RSA PRIVATE KEX"))
    add("rsa-key-encrypted", "encrypted-pkcs8-label", P(good8, "ENCRYPTED PRIVATE KEY"))
    add("rsa-key-encrypted", "encrypted-pkcs1-proc-type", P(good1, extra_header=b"Proc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,00112233445566778899aabbccddeeff\n\n"))
    add("rsa-key-algorithm", "label-ec-private-key", P(good1, "EC PRIVATE KEY"))
    add("rsa-key-algorithm", "label-openssh", P(good1, "OPENSSH PRIVATE KEY"))
    add("rsa-key-algorithm", "label-dsa", P(good1, "DSA PRIVATE KEY"))
    add("rsa-key-block-size", "block-too-large", P(L.tlv(0x30, L.der_int(0) + L.tlv(0x04, b"\x00" * 9000))))
    # ---- PKCS#8 ----
    pss = L.tlv(0x30, L.tlv(0x06, bytes.fromhex("2a864886f70d01010a")) + L.tlv(0x05, b""))
    ec = L.tlv(0x30, L.tlv(0x06, bytes.fromhex("2a8648ce3d0201")) + L.tlv(0x06, bytes.fromhex("2a8648ce3d030107")))
    add("rsa-key-algorithm", "pkcs8-oid-rsassa-pss", P(pk8(good1, alg=pss), "PRIVATE KEY"))
    add("rsa-key-algorithm", "pkcs8-oid-ec-public-key", P(pk8(good1, alg=ec), "PRIVATE KEY"))
    add("rsa-key-parameters", "pkcs8-parameters-absent", P(pk8(good1, alg=L.tlv(0x30, L.tlv(0x06, L.OID_RSA))), "PRIVATE KEY"))
    add("rsa-key-parameters", "pkcs8-parameters-not-null", P(pk8(good1, alg=L.tlv(0x30, L.tlv(0x06, L.OID_RSA) + L.der_int(0))), "PRIVATE KEY"))
    add("rsa-key-parameters", "pkcs8-parameters-null-with-content", P(pk8(good1, alg=L.tlv(0x30, L.tlv(0x06, L.OID_RSA) + L.tlv(0x05, b"\x00"))), "PRIVATE KEY"))
    add("rsa-key-parameters", "pkcs8-parameters-octet-string", P(pk8(good1, alg=L.tlv(0x30, L.tlv(0x06, L.OID_RSA) + L.tlv(0x04, b""))), "PRIVATE KEY"))
    add("rsa-key-parameters", "pkcs8-parameters-null-with-bad-length-byte", P(pk8(good1, alg=L.tlv(0x30, L.tlv(0x06, L.OID_RSA) + b"\x05\x01")), "PRIVATE KEY"))
    add("rsa-key-parameters", "pkcs8-parameters-null-then-an-extra-null", P(pk8(good1, alg=L.tlv(0x30, L.tlv(0x06, L.OID_RSA) + b"\x05\x00\x05\x00")), "PRIVATE KEY"))
    add("rsa-key-version", "pkcs8-version-1", P(pk8(good1, version=1), "PRIVATE KEY"))
    add("rsa-key-der", "pkcs8-trailing-element", P(pk8(good1, tail=L.tlv(0xa0, b"")), "PRIVATE KEY"))
    add("rsa-key-der", "pkcs8-key-not-octet-string", P(L.tlv(0x30, L.der_int(0) + L.ALG_RSA + L.tlv(0x03, b"\x00" + good1)), "PRIVATE KEY"))
    add("rsa-key-der", "pkcs8-inner-has-trailing-byte", P(pk8(good1 + b"\x00"), "PRIVATE KEY"))
    # ---- PKCS#1 structure ----
    add("rsa-key-multiprime", "pkcs1-version-1", P(pk1(version=1)))
    add("rsa-key-version", "pkcs1-version-2", P(pk1(version=2)))
    add("rsa-key-version", "pkcs1-version-256", P(L.tlv(0x30, L.der_int(256) + b"".join(ints[f] for f in order))))
    add("rsa-key-der", "body-of-pkcs1-under-pkcs8-label", P(good1, "PRIVATE KEY"))
    add("rsa-key-der", "body-of-pkcs8-under-pkcs1-label", P(good8, "RSA PRIVATE KEY"))
    add("rsa-key-der", "der-trailing-byte", P(good1 + b"\x00"))
    assert good1[:2] == b"\x30\x82"
    add("rsa-key-der", "der-length-overrun", P(good1[:2] + (len(good1) - 4 + 1).to_bytes(2, "big") + good1[4:]))
    add("rsa-key-der", "der-length-underrun", P(good1[:2] + (len(good1) - 4 - 1).to_bytes(2, "big") + good1[4:]))
    add("rsa-key-der", "der-outer-tag-set", P(b"\x31" + good1[1:]))
    add("rsa-key-der", "der-indefinite-length", P(b"\x30\x80" + good1[4:] + b"\x00\x00"))
    add("rsa-key-der", "der-long-form-for-short-length", P(L.tlv(0x30, L.der_int(0))[:1] + b"\x81\x03" + L.der_int(0)))
    add("rsa-key-der", "integer-non-minimal-n", P(pk1(over={"n": L.tlv(0x02, b"\x00\x00" + n.to_bytes(256, "big"))})))
    add("rsa-key-der", "integer-non-minimal-negative-e", P(pk1(over={"e": L.tlv(0x02, b"\xff\xff")})))
    add("rsa-key-der", "integer-empty-e", P(pk1(over={"e": L.tlv(0x02, b"")})))
    add("rsa-key-der", "integer-wrong-tag-d", P(pk1(over={"d": L.tlv(0x04, d.to_bytes(256, "big"))})))
    add("rsa-key-der", "extra-element", P(pk1(extra=[L.der_int(5)])))
    add("rsa-key-der", "missing-qinv", P(pk1(drop=("qinv",))))
    add("rsa-key-der", "missing-everything-after-n", P(L.tlv(0x30, L.der_int(0) + ints["n"])))
    # ---- the integers ----
    add("rsa-key-modulus", "modulus-negative", P(pk1(over={"n": L.tlv(0x02, n.to_bytes(256, "big"))})))
    add("rsa-key-modulus", "modulus-even", P(with_key(dict(B, n=n + 1))))
    add("rsa-key-modulus", "modulus-zero", P(pk1(over={"n": L.der_int(0)})))
    k1024 = balanced(1024, "small", 0)
    add("rsa-key-size", "modulus-1024", P(with_key(k1024)))
    k4224 = balanced(4224, "big", 0)
    add("rsa-key-size", "modulus-4224", P(with_key(k4224)))
    p47, q47 = pool(1024, "n2047", 0, lo=1 << 1023, hi=(1 << 1023) * 125 // 100), pool(1024, "n2047", 1, lo=1 << 1023, hi=(1 << 1023) * 125 // 100)
    k2047 = L.make_key(p47, q47, 65537)
    assert k2047["n"].bit_length() == 2047
    add("rsa-key-size", "modulus-2047-bits-top-bit-clear", P(with_key(k2047)))
    k2112 = balanced(2112, "h132", 0)
    add("rsa-key-size", "modulus-2112-bits-half-not-a-multiple-of-8-bytes", P(with_key(k2112)))
    add("rsa-key-exponent", "e-even", P(pk1(over={"e": L.der_int(65536)})))
    add("rsa-key-exponent", "e-one", P(pk1(over={"e": L.der_int(1)})))
    add("rsa-key-exponent", "e-zero", P(pk1(over={"e": L.der_int(0)})))
    add("rsa-key-exponent", "e-65-bits", P(pk1(over={"e": L.der_int((1 << 64) + 13)})))
    add("rsa-key-exponent", "e-negative", P(pk1(over={"e": L.tlv(0x02, b"\xff\x01")})))
    add("rsa-key-prime", "p-even", P(with_key(dict(B, p=p + 1))))
    add("rsa-key-prime", "q-even", P(with_key(dict(B, q=q + 1))))
    add("rsa-key-prime", "p-equals-q", P(with_key(dict(B, q=p, n=p * p))))
    # primes of other lengths whose product is still exactly 2,048 bits
    ps = pool(1000, "short", 0, lo=(1 << 1000) * 90 // 100, hi=1 << 1000)
    qs = pool(1048, "short", 1, lo=(1 << 1048) * 90 // 100, hi=1 << 1048)
    ksh = L.make_key(ps, qs, 65537)
    assert ksh["n"].bit_length() == 2048
    add("rsa-key-prime", "p-1000-bits-q-1048-bits", P(with_key(ksh)))
    pt = pool(1023, "tbc", 0, lo=(1 << 1023) * 90 // 100, hi=1 << 1023)
    qt = pool(1025, "tbc", 1, lo=(1 << 1025) * 90 // 100, hi=1 << 1025)
    ktb = L.make_key(pt, qt, 65537)
    assert ktb["n"].bit_length() == 2048
    add("rsa-key-prime", "p-1023-bits-q-1025-bits", P(with_key(ktb)))
    add("rsa-key-prime", "p-is-one", P(with_key(dict(B, p=1, dp=0))))
    add("rsa-key-product", "p-times-q-differs-from-n", P(with_key(dict(B, p=p + 2))))
    add("rsa-key-product", "n-plus-two", P(with_key(dict(B, n=n + 2))))
    add("rsa-key-crt-exponent", "dp-wrong", P(with_key(dict(B, dp=dp ^ 2))))
    add("rsa-key-crt-exponent", "dq-wrong", P(with_key(dict(B, dq=dq ^ 2))))
    add("rsa-key-crt-exponent", "dp-zero", P(with_key(dict(B, dp=0))))
    add("rsa-key-crt-exponent", "dp-plus-p-minus-1", P(with_key(dict(B, dp=dp + p - 1))))
    add("rsa-key-crt-exponent", "dq-is-d", P(with_key(dict(B, dq=d))))
    # In range of the slot but not of the prime: congruent to the right value, so only the range rule can refuse them
    # (a key with p about 1.04 x 2^1023: dp + p - 1 and qinv + p fit in 128 bytes for about 90% of keys; the first that does).
    ubk = None
    for i in range(64):
        a = pool(1024, "fit-p", i, 65537, lo=(1 << 1023) * 103 // 100, hi=(1 << 1023) * 105 // 100)
        b = pool(1024, "fit-q", i, 65537, lo=(1 << 1023) * 197 // 100, hi=(1 << 1023) * 199 // 100)
        cand = L.make_key(a, b, 65537)
        if cand["n"].bit_length() == 2048 and (cand["dp"] + a - 1).bit_length() <= 1024 and (cand["qinv"] + a).bit_length() <= 1024:
            ubk = cand
            break
    assert ubk is not None
    add("rsa-key-crt-exponent", "dp-plus-p-minus-1-fits-the-slot", P(with_key(dict(ubk, dp=ubk["dp"] + ubk["p"] - 1))))
    add("rsa-key-crt-coefficient", "qinv-plus-p-fits-the-slot", P(with_key(dict(ubk, qinv=ubk["qinv"] + ubk["p"]))))
    # a negative INTEGER in each field: the top bit set and no 00 before it
    for field, tag in (("p", "rsa-key-prime"), ("q", "rsa-key-prime"), ("dp", "rsa-key-crt-exponent"), ("dq", "rsa-key-crt-exponent"),
                       ("qinv", "rsa-key-crt-coefficient"), ("d", "rsa-key-private-exponent")):
        raw = B[field].to_bytes(256 if field == "d" else 128, "big")
        add(tag, f"{field}-negative", P(pk1(over={field: L.tlv(0x02, bytes([raw[0] | 0x80]) + raw[1:])})))
    add("rsa-key-crt-coefficient", "qinv-wrong", P(with_key(dict(B, qinv=qinv ^ 2))))
    add("rsa-key-crt-coefficient", "qinv-plus-p", P(with_key(dict(B, qinv=qinv + p))))
    add("rsa-key-crt-coefficient", "qinv-zero", P(with_key(dict(B, qinv=0))))
    add("rsa-key-crt-coefficient", "qinv-is-pinv-of-wrong-prime", P(with_key(dict(B, qinv=pow(p, -1, q)))))
    lam = (p - 1) * (q - 1) // math.gcd(p - 1, q - 1)
    d_big = d + ((n - d + lam - 1) // lam) * lam   # congruent to d modulo lcm(p - 1, q - 1), so to dp and dq, and not below n
    assert n <= d_big < 1 << 2048
    add("rsa-key-private-exponent", "d-congruent-to-d-but-not-below-n", P(with_key(dict(B, d=d_big))))
    d_huge = d + ((1 << 2048) // lam + 1) * lam   # congruent too, and 257 bytes long
    assert 1 << 2048 <= d_huge < 1 << 2056
    add("rsa-key-private-exponent", "d-congruent-to-d-and-longer-than-n", P(with_key(dict(B, d=d_huge))))
    add("rsa-key-private-exponent", "d-matches-dp-only", P(with_key(dict(B, d=d + p - 1))))
    add("rsa-key-private-exponent", "d-matches-dq-only", P(with_key(dict(B, d=d + q - 1))))
    add("rsa-key-private-exponent", "d-wrong-but-crt-right", P(with_key(dict(B, d=d ^ 4))))
    add("rsa-key-private-exponent", "d-plus-n", P(with_key(dict(B, d=d + n))))
    add("rsa-key-private-exponent", "d-zero", P(with_key(dict(B, d=0))))
    add("rsa-key-private-exponent", "d-is-n-minus-one", P(with_key(dict(B, d=n - 1))))
    # ---- parses, signs wrongly: the gap §6 states (primality is not checked) ----
    r1, r2 = pool(512, "comp", 0), pool(512, "comp", 1)
    comp = r1 * r2
    assert comp.bit_length() == 1024 and comp >> 1023 == 1
    kc = L.make_key(comp, q, 65537)
    add("ok", "composite-p-with-consistent-exponents", P(with_key(kc)), sign="rsa-sign-check")
    return B, cases


def cmd_malformed(path, check):
    B, cases = corpus()
    lines = ["# scripts/rsa_sign_keys.py malformed: <parse> <sign> <name> <signature or -> <file hex>. Throwaway keys, fixed seed. docs/rsa-sign.md §11 G3."]
    for parse, sign, name, data, key in cases:
        lines.append(f"{parse} {sign} {name} {sign_col(key) if key else '-'} {data.hex() if data else '-'}")
    text = "\n".join(lines) + "\n"
    emit(path, text, check)


def cmd_valid(path, check):
    """Fixed known answers of our own keys (independent of NIST and Wycheproof): key block, message, signature from the Python reference."""
    ks = all_keys()
    lines = ["# scripts/rsa_sign_keys.py valid: K <name> <bits> <e> <key block hex> | M <name> <h> <msg hex> <signature hex>. Throwaway keys, fixed seed."]
    rng = random.Random("rsa-sign/valid-messages")
    for name in ("k2048_e65537_0", "k2048_e3_0", "k2048_e17_0", "k3072_e65537_0", "k3072_e65537_upq", "k4096_e65537_uqp", "k2176_e65537_0", "k2048_e65537_swapped"):
        k = ks[name]
        lines.append(f"K {name} {k['n'].bit_length()} {k['e']} {L.block(k).hex()}")
        for h in (32, 48, 64):
            msg = rng.randbytes(rng.choice([0, 1, 33, 200]))
            lines.append(f"M {name} {h} {L.h_(msg)} {L.py_sign(k, h, L.digest_of(h, msg)).hex()}")
    emit(path, "\n".join(lines) + "\n", check)


def emit(path, text, check):
    if check:
        have = open(path).read() if os.path.exists(path) else None
        if have != text:
            print(f"{path} is not what the generator makes", file=sys.stderr)
            sys.exit(1)
        print(f"{path}: identical to a fresh run")
    else:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        open(path, "w").write(text)
        print(f"wrote {path} ({len(text)} bytes)")


def main():
    a = sys.argv[1:]
    check = "--check" in a
    a = [x for x in a if x != "--check"]
    if not a:
        print(__doc__)
        sys.exit(2)
    if a[0] == "keys":
        cmd_keys(a[1])
    elif a[0] == "malformed":
        cmd_malformed(a[1] if len(a) > 1 else "tests/vectors/rsa_sign/malformed.txt", check)
    elif a[0] == "valid":
        cmd_valid(a[1] if len(a) > 1 else "tests/vectors/rsa_sign/valid.txt", check)
    else:
        print(__doc__)
        sys.exit(2)


main()
