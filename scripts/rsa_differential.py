#!/usr/bin/env python3
"""Differential tests of `std.bigmod` and `std.rsa` (docs/rsa.md §4).

    python3 scripts/rsa_differential.py <driver> pow [<count>]
    python3 scripts/rsa_differential.py <driver> openssl [<count>]

`driver` is `tests/programs/rsa_driver.ls` built with `lex-sys build --std`.

`pow` (default 100,000): `bigmod.pow_mod` against Python's `pow(a, e, n)`.
Moduli are odd, of a random size from 2 to 4,096 bits, with the sizes at
limb boundaries (29 to 31, 59 to 61 bits, ...) and RSA's sizes drawn more
often. Leading zero bytes on n and a are sometimes added. The exponents are
mostly small (0 to 64 bits); one case in 100 has an exponent as long as the
modulus. 1,000 cases have a = 0, a = 1 or a = n - 1, and 1,000 have
n = p^j and a = p^i, where a^e is often 0 mod n.

`openssl` (default 1,000): for each round, a message, a key (three of each
size: 2,048, 3,072 and 4,096 bits, made once by `openssl genpkey`), a hash
(SHA-256, SHA-384 or SHA-512) and a padding (PKCS#1 v1.5, or PSS with the
salt as long as the digest, as TLS 1.3 uses it). `openssl dgst -sign` signs.
The signature is then verified by `openssl dgst -verify` and by `std.rsa`,
once as made and once with one random bit flipped. The two must agree every
time; for the first, both must accept.

Exit status 1 on any difference.
"""
import os
import random
import subprocess
import sys
import tempfile


def run(driver, cases):
    out = subprocess.run([driver], input="\n".join(cases) + "\n", capture_output=True, text=True, check=True)
    lines = out.stdout.splitlines()
    assert len(lines) == len(cases), (len(lines), len(cases))
    return lines


def hexed(v, n):
    return v.to_bytes(n, "big").hex() if n else "-"


def pow_cases(count, rng):
    edges = [2, 3, 5, 8, 29, 30, 31, 32, 59, 60, 61, 64, 89, 90, 91, 2047, 2048, 2049, 3071, 3072, 4095, 4096]
    cases, want = [], []
    for i in range(count):
        r = rng.random()
        bits = rng.choice(edges) if r < 0.4 else rng.choice([2048, 3072, 4096]) if r < 0.6 else rng.randint(2, 4096)
        n = rng.getrandbits(bits) | 1 | 1 << (bits - 1)
        n = max(n, 3)
        nl = (n.bit_length() + 7) // 8 + (1 if rng.random() < 0.1 else 0)
        if i % 100 == 99:
            e = rng.getrandbits(n.bit_length())
        else:
            e = rng.getrandbits(rng.choice([1, 2, 3, 8, 16, 17, 32, 64]))
        el = max(1, (e.bit_length() + 7) // 8)
        if i < 1000:
            a = [0, 1, n - 1][i % 3]
        elif i < 2000:
            # n = p^j and a = p^i, so a^e is 0 mod n once i*e >= j: the one
            # result Montgomery's lazy reduction could leave as n, not 0.
            p = rng.choice([3, 5, 7, 11, 13, 101, 65537, (1 << 31) - 1, (1 << 61) - 1])
            j = rng.randint(2, max(2, 4000 // p.bit_length()))
            n = p ** j
            nl = (n.bit_length() + 7) // 8
            a = p ** rng.randint(1, j - 1)
            e = rng.randint(1, j + 2)
            el = (e.bit_length() + 7) // 8
        else:
            a = rng.randrange(n)
        al = nl if rng.random() < 0.5 else max(1, (a.bit_length() + 7) // 8)
        cases.append(f"M {hexed(n, nl)} {hexed(e, el)} {hexed(a, al)}")
        want.append(f"0 ok {hexed(pow(a, e, n), nl)}")
    return cases, want


def openssl(args, data=None):
    return subprocess.run(["openssl"] + args, input=data, capture_output=True)


def key_numbers(path):
    from cryptography.hazmat.primitives.serialization import load_pem_private_key

    pub = load_pem_private_key(open(path, "rb").read(), None).public_key().public_numbers()
    return pub.n, pub.e


def openssl_rounds(driver, count, rng):
    tmp = tempfile.mkdtemp()
    keys = []
    for bits in (2048, 3072, 4096):
        for j in range(3):
            path = os.path.join(tmp, f"k{bits}_{j}.pem")
            subprocess.run(["openssl", "genpkey", "-algorithm", "RSA", "-pkeyopt", f"rsa_keygen_bits:{bits}", "-out", path],
                           check=True, capture_output=True)
            pubpath = path + ".pub"
            subprocess.run(["openssl", "pkey", "-in", path, "-pubout", "-out", pubpath], check=True, capture_output=True)
            keys.append((path, pubpath) + key_numbers(path))
    cases, theirs = [], []
    msg_path, sig_path = os.path.join(tmp, "msg"), os.path.join(tmp, "sig")
    for i in range(count):
        path, pubpath, n, e = rng.choice(keys)
        h = rng.choice([32, 48, 64])
        dgst = {32: "-sha256", 48: "-sha384", 64: "-sha512"}[h]
        pss = rng.random() < 0.5
        opts = ["-sigopt", "rsa_padding_mode:pss", "-sigopt", "rsa_pss_saltlen:digest"] if pss else []
        msg = rng.randbytes(rng.randint(0, 300))
        open(msg_path, "wb").write(msg)
        sig = openssl(["dgst", dgst, "-sign", path] + opts + [msg_path]).stdout
        nl = (n.bit_length() + 7) // 8
        bad = bytearray(sig)
        bit = rng.randrange(len(bad) * 8)
        bad[bit // 8] ^= 1 << (bit % 8)
        for s in (sig, bytes(bad)):
            open(sig_path, "wb").write(s)
            vopts = ["-sigopt", "rsa_padding_mode:pss", "-sigopt", "rsa_pss_saltlen:digest"] if pss else []
            r = openssl(["dgst", dgst, "-verify", pubpath] + vopts + ["-signature", sig_path, msg_path])
            theirs.append(r.returncode == 0)
            m = msg.hex() or "-"
            el = (e.bit_length() + 7) // 8
            if pss:
                cases.append(f"S {h} {h} {h} {hexed(n, nl)} {hexed(e, el)} {m} {s.hex()}")
            else:
                cases.append(f"P {h} {hexed(n, nl)} {hexed(e, el)} {m} {s.hex()}")
    return cases, theirs


def main():
    driver, mode = sys.argv[1], sys.argv[2]
    rng = random.Random(203)
    bad = 0
    if mode == "pow":
        count = int(sys.argv[3]) if len(sys.argv) > 3 else 100000
        done = 0
        while done < count:
            step = min(10000, count - done)
            cases, want = pow_cases(step, rng)
            for c, g, w in zip(cases, run(driver, cases), want):
                if g != w:
                    bad += 1
                    if bad < 5:
                        print(f"{c[:80]}...\n  got  {g[:80]}\n  want {w[:80]}")
            done += step
        print(f"pow_mod: {count} cases, {bad} differences")
    else:
        count = int(sys.argv[3]) if len(sys.argv) > 3 else 1000
        cases, theirs = openssl_rounds(driver, count, rng)
        agree = {True: 0, False: 0}
        for i, (c, line, t) in enumerate(zip(cases, run(driver, cases), theirs)):
            ours = line.startswith("0 ok")
            if ours != t or (i % 2 == 0 and not t):
                bad += 1
                print(f"round {i // 2} ({'as made' if i % 2 == 0 else 'one bit flipped'}): openssl {t}, std.rsa {line}")
            agree[t] += 1
        print(f"openssl: {count} signatures, {count} flipped; openssl accepted {agree[True]}, refused {agree[False]}; {bad} differences")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
