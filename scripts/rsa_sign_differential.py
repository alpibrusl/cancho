#!/usr/bin/env python3
# Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
"""Differential tests of `std.rsa_sign` and `std.bigmod`'s secret-modulus functions (docs/rsa-sign.md §11, gate G2).

    python3 scripts/rsa_sign_differential.py <driver> sign <keydir> [messages-per-key]
    python3 scripts/rsa_sign_differential.py <driver> parse <keydir>
    python3 scripts/rsa_sign_differential.py <driver> blinding <keydir>
    python3 scripts/rsa_sign_differential.py <driver> bigmod [count]

`driver` is `tests/programs/rsa_sign_driver.cho` built with `cancho build --std` (with packages/x509/{x509,key,rsa_key}.cho).
`keydir` is made by `python3 scripts/rsa_sign_keys.py keys <keydir>` (throwaway keys: 2,048/3,072/4,096 bits, e = 65537, 3 and
17, unbalanced primes in both orders, p and q swapped, a 2,176-bit modulus).

`sign`: for every key, hash (SHA-256 always, SHA-384/512 on the e = 65537 keys) and message, the driver's signature is judged by
five independent things -- (1) byte-identical to `openssl dgst -<hash> -sign`; (2) byte-identical to the pure-Python RFC 8017
reference (`pow(, d, n)`); (3) accepted by `std.rsa.pkcs1_verify`, which this slice did not change; (4) accepted by `openssl dgst
-verify`; (5) the same signature with one bit flipped REFUSED by 3 and 4, else 3 and 4 prove nothing -- and the entropy is fresh
random per call. The signature must not depend on it.
`parse`: the same keys as PKCS#8 and PKCS#1 PEM through `x509_rsa_key.parse_pem` give the same block as Python's, and signing from
the PEM equals signing from the block.
`blinding`: the p-half exponent the signer uses (test mode 64) is dp + kappa (p - 1), kappa the first 8 bytes of the entropy with
the top bit set; the q-half the last 8; with no blinding it is dp.
`bigmod`: `setup_secret` + `pow_secret` against Python's `pow` over random odd (not necessarily prime) moduli of 8 to 512 bytes
whose top bit is set, `reduce_once` and `blind_exponent` against Python integers.
Exit status 1 on any difference.
"""
import os
import random
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rsa_sign_lib as L  # noqa: E402


def keys_of(keydir):
    out = []
    for f in sorted(os.listdir(keydir)):
        if f.endswith(".json"):
            k = L.load_json(os.path.join(keydir, f))
            k["name"] = f[:-5]
            k["pem"] = os.path.join(keydir, f[:-5] + ".pem")
            out.append(k)
    return out


def ossl_sign(path, h, msg, tmp):
    m = os.path.join(tmp, "msg")
    open(m, "wb").write(msg)
    return subprocess.run(["openssl", "dgst", f"-{L.HASH[h]}", "-sign", path, m], capture_output=True, check=True).stdout


def ossl_pub(path, tmp):
    out = os.path.join(tmp, os.path.basename(path) + ".pub")
    if not os.path.exists(out):
        subprocess.run(["openssl", "pkey", "-in", path, "-pubout", "-out", out], check=True, capture_output=True)
    return out


def ossl_verify(pub, h, msg, sig, tmp):
    m, s = os.path.join(tmp, "msg"), os.path.join(tmp, "sig")
    open(m, "wb").write(msg)
    open(s, "wb").write(sig)
    return subprocess.run(["openssl", "dgst", f"-{L.HASH[h]}", "-verify", pub, "-signature", s, m], capture_output=True).returncode == 0


def ehex(e):
    h = f"{e:x}"
    return h if len(h) % 2 == 0 else "0" + h


def flip(sig, rng):
    b = bytearray(sig)
    b[rng.randrange(len(b))] ^= 1 << rng.randrange(8)
    return bytes(b)


def check_sign(driver, keydir, per_key):
    rng = random.Random(20261014)
    tmp = tempfile.mkdtemp(prefix="rsasign_")
    fails, total = [], 0
    for key in keys_of(keydir):
        hashes = [32, 48, 64] if key["e"] == 65537 else [32]
        pub = ossl_pub(key["pem"], tmp)
        blk = L.block(key).hex()
        k = (key["n"].bit_length() + 7) // 8
        cases = []
        for h in hashes:
            nmsg = per_key if h == 32 else max(2, per_key // 3)
            for msg in [b"", b"a"] + [rng.randbytes(rng.choice([1, 17, 64, 200, 1000])) for _ in range(nmsg - 2)]:
                cases.append((h, msg, rng.randbytes(16)))
        res, rc, err = L.run_driver(driver, [f"M {h} {blk} {L.h_(m)} {e.hex()}" for h, m, e in cases])
        if len(res) != len(cases):
            fails.append(f"{key['name']}: driver answered {len(res)} lines for {len(cases)} cases (rc {rc}) {err[:200]}")
            continue
        # verifications by std.rsa in one batch
        for (h, msg, ent), line in zip(cases, res):
            total += 1
            digest = L.digest_of(h, msg)
            parts = line.split()
            if parts[0] != "0" or len(parts) < 3:
                fails.append(f"{key['name']} {L.HASH[h]}: signer answered {line[:60]}")
                continue
            sig = bytes.fromhex(parts[2])
            if sig != L.py_sign(key, h, digest):
                fails.append(f"{key['name']} {L.HASH[h]}: differs from the Python reference")
            elif sig != ossl_sign(key["pem"], h, msg, tmp):
                fails.append(f"{key['name']} {L.HASH[h]}: differs from openssl dgst -sign")
            else:
                v = L.run_driver(driver, [f"D {h} {key['n']:0{2 * k}x} {ehex(key['e'])} {digest.hex()} {sig.hex()}"])[0][0]
                bad = flip(sig, rng)
                v2 = L.run_driver(driver, [f"D {h} {key['n']:0{2 * k}x} {ehex(key['e'])} {digest.hex()} {bad.hex()}"])[0][0]
                if not v.startswith("0 ok"):
                    fails.append(f"{key['name']} {L.HASH[h]}: std.rsa refused the signature: {v[:50]}")
                elif not ossl_verify(pub, h, msg, sig, tmp):
                    fails.append(f"{key['name']} {L.HASH[h]}: openssl refused the signature")
                elif v2.startswith("0 ok") or ossl_verify(pub, h, msg, bad, tmp):
                    fails.append(f"{key['name']} {L.HASH[h]}: a flipped signature was ACCEPTED (the verifier proves nothing)")
        if os.environ.get("RSA_SIGN_VERBOSE"):
            print(f"{key['name']:24s} {k * 8:5d} bits e={key['e']:<6d} {len(cases)} cases")
    return fails, total


def check_parse(driver, keydir):
    fails, total = [], 0
    msg = b"parse"
    rng = random.Random(77)
    for key in keys_of(keydir):
        blk = L.block(key)
        for label, der in (("PRIVATE KEY", L.pkcs8(key)), ("RSA PRIVATE KEY", L.pkcs1(key))):
            pem = L.pem(label, der)
            res = L.run_driver(driver, [f"P {pem.hex()}", f"B {8 if label == 'PRIVATE KEY' else 2} {der.hex()}",
                                         f"G {pem.hex()} 32 {msg.hex()} {rng.randbytes(16).hex()}"])[0]
            total += 3
            if len(res) != 3:
                fails.append(f"{key['name']} {label}: no answer")
                continue
            want = f"{len(blk)} ok {blk.hex()}"
            if res[0] != want or res[1] != want:
                fails.append(f"{key['name']} {label}: block differs: {res[0][:40]} / {res[1][:40]}")
            if res[2] != f"0 ok {L.py_sign(key, 32, L.digest_of(32, msg)).hex()}":
                fails.append(f"{key['name']} {label}: parse-then-sign differs: {res[2][:40]}")
    return fails, total


def check_blinding(driver, keydir):
    fails, total = [], 0
    rng = random.Random(5)
    for key in keys_of(keydir):
        blk = L.block(key).hex()
        d32 = L.digest_of(32, b"b")
        h = (key["p"].bit_length() + 7) // 8  # the exponent is h + 16 bytes (k + 2 limbs), at the start of the signature's L
        for _ in range(4):
            ent = rng.randbytes(16)
            kp = int.from_bytes(ent[:8], "big") | 1 << 63
            kq = int.from_bytes(ent[8:], "big") | 1 << 63
            line = L.run_driver(driver, [f"U 32 {blk} {d32.hex()} {ent.hex()} 64", f"U 32 {blk} {d32.hex()} {ent.hex()} 128"])[0]
            total += 2
            want_p = key["dp"] + kp * (key["p"] - 1)
            want_q = key["dq"] + kq * (key["q"] - 1)
            for got, want, name in ((line[0], want_p, "p"), (line[1], want_q, "q")):
                parts = got.split()
                if parts[0] != "0":
                    fails.append(f"{key['name']}: exponent of the {name}-half: {got[:50]}")
                elif int(parts[2][:2 * (h + 16)], 16) != want:
                    fails.append(f"{key['name']}: the {name}-half exponent is not d + kappa (r - 1)")
        un = L.run_driver(driver, [f"U 32 {blk} {d32.hex()} - {64 | 16}"])[0][0].split()
        total += 1
        if un[0] != "0" or int(un[2][:2 * (h + 16)], 16) != key["dp"]:
            fails.append(f"{key['name']}: unblinded exponent is not dp")
    return fails, total


def rand_modulus(rng, nbytes):
    bits = 8 * nbytes
    shape = rng.randrange(6)
    if shape == 0:
        n = (1 << bits) - 1                       # all ones
    elif shape == 1:
        n = (1 << (bits - 1)) | 1                 # 2^(bits-1) + 1
    elif shape == 2:
        n = (1 << bits) - rng.randrange(1, 1000) * 2 - 1
    else:
        n = rng.getrandbits(bits) | (1 << (bits - 1)) | 1
    return n


def check_bigmod(driver, count):
    rng = random.Random(31337)
    fails, total = [], 0
    lines, wants = [], []
    for i in range(count):
        nb = rng.choice([8, 8, 16, 24, 32, 64, 96, 128, 129 // 8 * 8, 192, 256, 384, 512, rng.randrange(1, 65) * 8])
        n = rand_modulus(rng, nb)
        pick = rng.randrange(10)
        a = 0 if pick == 0 else 1 if pick == 1 else n - 1 if pick == 2 else rng.randrange(n)
        el = rng.choice([1, 2, 8, nb // 2, nb, nb])
        e = rng.getrandbits(8 * el) if rng.randrange(8) else 0
        op = rng.randrange(3)
        if op == 0:
            lines.append(f"Z {n:0{2 * nb}x} {e:0{2 * el}x} {a:0{2 * nb}x}")
            wants.append(f"0 ok {pow(a, e, n):0{2 * nb}x}")
        elif op == 1:
            x = rng.choice([0, n - 1, n, n + 1, 2 * n - 1, rng.randrange(2 * n)])
            if x.bit_length() > 8 * nb:
                x = n  # a register holds 64k bits
            lines.append(f"R {n:0{2 * nb}x} {x:0{2 * nb}x}")
            wants.append(f"0 ok {x % n:0{2 * nb}x}")
        else:
            d = rng.randrange(n)
            kappa = rng.getrandbits(64) | (1 << 63) if rng.randrange(4) else rng.getrandbits(64)
            lines.append(f"Y {n:0{2 * nb}x} {d:0{2 * nb}x} {kappa:016x}")
            wants.append(f"0 ok {d + kappa * (n - 1):0{2 * (nb + 16)}x}")
    for lo in range(0, len(lines), 2000):
        res, rc, err = L.run_driver(driver, lines[lo:lo + 2000])
        if len(res) != len(lines[lo:lo + 2000]):
            fails.append(f"driver answered {len(res)} for {len(lines[lo:lo + 2000])} (rc {rc}) {err[:100]}")
            continue
        for i, (g, w) in enumerate(zip(res, wants[lo:lo + 2000])):
            total += 1
            if g != w:
                fails.append(f"case {lo + i}: {lines[lo + i][:30]}...: got {g[:40]} want {w[:40]}")
    return fails, total


def main():
    driver, mode = sys.argv[1], sys.argv[2]
    if mode == "sign":
        fails, total = check_sign(driver, sys.argv[3], int(sys.argv[4]) if len(sys.argv) > 4 else 12)
    elif mode == "parse":
        fails, total = check_parse(driver, sys.argv[3])
    elif mode == "blinding":
        fails, total = check_blinding(driver, sys.argv[3])
    elif mode == "bigmod":
        fails, total = check_bigmod(driver, int(sys.argv[3]) if len(sys.argv) > 3 else 20000)
    else:
        print(__doc__)
        sys.exit(2)
    print(f"{mode}: {total} cases, {len(fails)} failures")
    for f in fails[:25]:
        print("FAIL", f)
    sys.exit(1 if fails or total == 0 else 0)


if __name__ == "__main__":
    main()
