#!/usr/bin/env python3
"""Differential tests of `std.ecdsa_sign` and `x509_key` against OpenSSL
(docs/ecdsa-sign.md §5).

    python3 scripts/ecdsa_sign_differential.py openssl <sign driver> <ecdsa driver> [<count>]
    python3 scripts/ecdsa_sign_differential.py keys <sign driver> [<count>]
    python3 scripts/ecdsa_sign_differential.py reference <sign driver> [<count>]

`sign driver` is `tests/programs/ecdsa_sign_driver.ls` built with
`lex-sys build --std` and `packages/x509/{x509,key}.ls`; `ecdsa driver` is
`tests/programs/ecdsa_driver.ls`.

`openssl` (default 10,000): each round draws a P-256 private key (random in
[1, n), with 1, 2, n - 2 and n - 1 drawn often), a random message, and 32
bytes of added randomness (empty one time in ten). `std.ecdsa_sign` signs
the message's SHA-256 as DER (`sign_checked`, its point derived by
`x509_key.public_point`). The signature is then verified by
`openssl dgst -sha256 -verify`, under a public key this script computes
itself (Python integers, not the code under test), and by `std.ecdsa`'s
`verify_der`; then one random bit of the DER is flipped, and both must
refuse it. The public point the driver derived must equal the script's.

`keys` (default 100): keys made by `openssl genpkey` (PKCS#8), the same
converted by `openssl ec` (SEC 1, `EC PRIVATE KEY`), and keys from
`openssl ecparam -genkey` (an `EC PARAMETERS` block, then the key). Each is
parsed by `x509_key.parse_pem`, which must give the private key and point
`openssl pkey -text` prints; a message is signed with it and verified by
`openssl dgst -verify` under `openssl pkey -pubout`'s key; and a
certificate `openssl req -x509` makes for the key must match it
(`matches_certificate`), and must not match the next key.

`reference` (default 10,000): the signature itself, byte for byte, against
RFC 6979 written here in Python (`hmac`, `hashlib` and the integers above):
random keys, digests and added randomness (empty one time in four), with the
edges drawn often (keys 1 and n - 1, digests 0, n - 1, n and 2^256 - 1, so
`bits2octets`' reduction is reached), as raw r || s and as DER. OpenSSL
verifying a signature does not show that the nonce is RFC 6979's or that the
randomness was used; this does.

Exit status 1 on any difference.
"""
import base64
import hashlib
import hmac
import os
import random
import subprocess
import sys
import tempfile

P = 0xFFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF
N = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551
B = 0x5AC635D8AA3A93E7B3EBBD55769886BC651D06B0CC53B0F63BCE3C3E27D2604B
G = (0x6B17D1F2E12C4247F8BCE6E563A440F277037D812DEB33A0F4A13945D898C296,
     0x4FE342E2FE1A7F9B8EE7EB4A7C0F9E162BCE33576B315ECECBB6406837BF51F5)
SPKI = bytes.fromhex("3059301306072a8648ce3d020106082a8648ce3d030107034200")


def jacobian_double(p):
    x, y, z = p
    if z == 0 or y == 0:
        return (1, 1, 0)
    d = z * z % P
    a = 3 * (x - d) * (x + d) % P
    s = 4 * x * y * y % P
    x3 = (a * a - 2 * s) % P
    return (x3, (a * (s - x3) - 8 * pow(y, 4, P)) % P, 2 * y * z % P)


def jacobian_add(p, q):
    if p[2] == 0:
        return q
    if q[2] == 0:
        return p
    z1z1, z2z2 = p[2] * p[2] % P, q[2] * q[2] % P
    u1, u2 = p[0] * z2z2 % P, q[0] * z1z1 % P
    s1, s2 = p[1] * q[2] * z2z2 % P, q[1] * p[2] * z1z1 % P
    if u1 == u2:
        return jacobian_double(p) if s1 == s2 else (1, 1, 0)
    h, r = (u2 - u1) % P, (s2 - s1) % P
    hh = h * h % P
    hhh = h * hh % P
    v = u1 * hh % P
    x3 = (r * r - hhh - 2 * v) % P
    return (x3, (r * (v - x3) - s1 * hhh) % P, h * p[2] * q[2] % P)


def public_point(d):
    """d·G as `04 || x || y`, in Python integers."""
    acc, add = (1, 1, 0), (G[0], G[1], 1)
    while d:
        if d & 1:
            acc = jacobian_add(acc, add)
        add = jacobian_double(add)
        d >>= 1
    zi = pow(acc[2], -1, P)
    x, y = acc[0] * zi * zi % P, acc[1] * zi * zi * zi % P
    assert (y * y - x * x * x + 3 * x - B) % P == 0
    return b"\x04" + x.to_bytes(32, "big") + y.to_bytes(32, "big")


def rfc6979_sign(d, digest, extra):
    """RFC 6979 §3.2 with §3.6's additional data, then SEC 1's s: r || s."""
    x = d.to_bytes(32, "big")
    h = (int.from_bytes(digest, "big") % N).to_bytes(32, "big")
    k_, v = b"\x00" * 32, b"\x01" * 32
    mac = lambda key, data: hmac.new(key, data, hashlib.sha256).digest()
    k_ = mac(k_, v + b"\x00" + x + h + extra)
    v = mac(k_, v)
    k_ = mac(k_, v + b"\x01" + x + h + extra)
    v = mac(k_, v)
    while True:
        v = mac(k_, v)
        k = int.from_bytes(v, "big")
        if 1 <= k < N:
            r = int.from_bytes(public_point(k)[1:33], "big") % N
            s = pow(k, -1, N) * (int.from_bytes(digest, "big") + r * d) % N
            if r and s:
                return r.to_bytes(32, "big") + s.to_bytes(32, "big")
        k_ = mac(k_, v + b"\x00")
        v = mac(k_, v)


def der(sig):
    out = b""
    for part in (sig[:32], sig[32:]):
        v = part.lstrip(b"\x00") or b"\x00"
        if v[0] >= 0x80:
            v = b"\x00" + v
        out += b"\x02" + bytes([len(v)]) + v
    return b"\x30" + bytes([len(out)]) + out


def reference_mode(sign_driver, count, rng):
    # The reference first reproduces RFC 6979 A.2.5's "sample" signature.
    assert rfc6979_sign(0xC9AFA9D845BA75166B5C215767B1D6934E50C3DB36E89B127B8A622B120F6721,
                        hashlib.sha256(b"sample").digest(), b"").hex() == (
        "efd48b2aacb6a8fd1140dd9cd45e81d69d2c877b56aaf991c34d0ea84eaf3716"
        "f7cb1c942d657c41d436c7a1b6e29f65f3e900dbb9aff4064dc4ab2f843acda8")
    cases, want = [], []
    for i in range(count):
        d = rng.choice([1, N - 1] + [rng.randrange(1, N)] * 18)
        digest = rng.choice([0, N - 1, N, 2 ** 256 - 1] + [rng.randrange(2 ** 256)] * 16).to_bytes(32, "big")
        extra = b"" if rng.randrange(4) == 0 else rng.randbytes(32)
        sig = rfc6979_sign(d, digest, extra)
        cases.append(f"S {digest.hex()} {d.to_bytes(32, 'big').hex()} {extra.hex() or '-'}")
        want.append(f"0 ok {sig.hex()}")
        cases.append(f"E {sig.hex()}")
        want.append(f"{len(der(sig))} ok {der(sig).hex()}")
    bad = 0
    for c, g, w in zip(cases, run(sign_driver, cases), want):
        if g != w:
            bad += 1
            if bad < 5:
                print(f"{c[:80]}\n  got  {g}\n  want {w}")
    print(f"reference: {count} signatures and their DER against RFC 6979 in Python; {bad} differences")
    return bad


def pem(point):
    body = base64.encodebytes(SPKI + point).decode()
    return f"-----BEGIN PUBLIC KEY-----\n{body}-----END PUBLIC KEY-----\n"


def run(driver, cases):
    out = subprocess.run([driver], input="\n".join(cases) + "\n", capture_output=True, text=True, check=True)
    lines = out.stdout.splitlines()
    assert len(lines) == len(cases), (len(lines), len(cases))
    return lines


def openssl_verifies(tmp, pub_pem, msg, sig):
    paths = [os.path.join(tmp, n) for n in ("pub.pem", "msg", "sig")]
    for path, data in zip(paths, (pub_pem.encode(), msg, sig)):
        open(path, "wb").write(data)
    r = subprocess.run(["openssl", "dgst", "-sha256", "-verify", paths[0], "-signature", paths[2], paths[1]],
                       capture_output=True)
    return r.returncode == 0


def openssl_mode(sign_driver, ecdsa_driver, count, rng):
    tmp = tempfile.mkdtemp()
    rounds = []
    for _ in range(count):
        d = rng.choice([1, 2, N - 2, N - 1] + [rng.randrange(1, N)] * 46)
        msg = rng.randbytes(rng.randint(0, 300))
        extra = b"" if rng.randrange(10) == 0 else rng.randbytes(32)
        rounds.append((d, msg, extra))
    signed = run(sign_driver, [f"M {m.hex() or '-'} {d.to_bytes(32, 'big').hex()} {e.hex() or '-'}" for d, m, e in rounds])
    points = run(sign_driver, [f"B 08 {pkcs8(d).hex()}" for d, _, _ in rounds])
    bad = 0
    cases, sigs = [], []
    for i, ((d, msg, extra), line, parsed) in enumerate(zip(rounds, signed, points)):
        code, _, der = line.split(" ")
        point = public_point(d)
        if not code.isdigit() or parsed.split(" ")[2] != (d.to_bytes(32, "big") + point).hex():
            bad += 1
            print(f"round {i}: signing or the derived point failed: {line} / {parsed}")
            sigs.append(None)
            continue
        sig = bytes.fromhex(der)
        flipped = bytearray(sig)
        bit = rng.randrange(len(flipped) * 8)
        flipped[bit // 8] ^= 1 << (bit % 8)
        sigs.append((point, msg, sig, bytes(flipped)))
        for s in (sig, bytes(flipped)):
            cases.append(f"V 256 32 {point.hex()} {msg.hex() or '-'} {s.hex()}")
    ours = iter(run(ecdsa_driver, cases))
    accepted = refused = 0
    for i, entry in enumerate(sigs):
        if entry is None:
            continue
        point, msg, sig, flipped = entry
        pub = pem(point)
        good_openssl = openssl_verifies(tmp, pub, msg, sig)
        bad_openssl = openssl_verifies(tmp, pub, msg, flipped)
        good_ours = next(ours).startswith("0 ok")
        bad_ours = next(ours).startswith("0 ok")
        accepted += good_openssl and good_ours
        refused += not bad_openssl and not bad_ours
        if not (good_openssl and good_ours) or bad_openssl or bad_ours:
            bad += 1
            print(f"round {i}: as made openssl {good_openssl} std.ecdsa {good_ours}; flipped openssl {bad_openssl} std.ecdsa {bad_ours}")
    print(f"openssl: {count} signatures; {accepted} accepted by both OpenSSL and std.ecdsa, "
          f"{refused} with a bit flipped refused by both; {bad} differences")
    return bad


def pkcs8(d):
    """A PKCS#8 PrivateKeyInfo for d with no public key, built here, so
    `x509_key.parse_der` derives the point from d alone."""
    inner = bytes.fromhex("020101") + b"\x04\x20" + d.to_bytes(32, "big")
    inner = b"\x30" + bytes([len(inner)]) + inner
    body = (bytes.fromhex("020100") + bytes.fromhex("301306072a8648ce3d020106082a8648ce3d030107")
            + b"\x04" + bytes([len(inner)]) + inner)
    return b"\x30" + bytes([len(body)]) + body


def openssl(*args, data=None):
    return subprocess.run(["openssl", *args], input=data, capture_output=True, check=True).stdout


def text_numbers(path):
    """`openssl pkey -text`'s priv and pub, as bytes."""
    out = openssl("pkey", "-in", path, "-text", "-noout").decode()
    fields, name = {}, None
    for line in out.splitlines():
        if line.startswith("priv:") or line.startswith("pub:"):
            name = line[:-1]
            fields[name] = ""
        elif name and line.startswith("    "):
            fields[name] += line.strip().replace(":", "")
        else:
            name = None
    return int(fields["priv"], 16).to_bytes(32, "big"), bytes.fromhex(fields["pub"])


def keys_mode(sign_driver, count, rng):
    tmp = tempfile.mkdtemp()
    files = []
    for i in range(count):
        k8 = os.path.join(tmp, f"k{i}.pem")
        if i % 3 == 2:
            open(k8, "wb").write(openssl("ecparam", "-name", "prime256v1", "-genkey"))
        else:
            openssl("genpkey", "-algorithm", "EC", "-pkeyopt", "ec_paramgen_curve:P-256", "-out", k8)
        k1 = os.path.join(tmp, f"k{i}.sec1.pem")
        openssl("ec", "-in", k8, "-out", k1)
        cert = os.path.join(tmp, f"k{i}.der")
        openssl("req", "-x509", "-new", "-key", k8, "-subj", f"/CN=k{i}", "-days", "1", "-outform", "DER", "-out", cert)
        files.append((k8, k1, cert))
    cases, expect = [], []
    numbers = [text_numbers(k8) for k8, _, _ in files]
    msgs = [rng.randbytes(rng.randint(0, 100)) for _ in files]
    for i, (k8, k1, cert) in enumerate(files):
        d, point = numbers[i]
        other = numbers[(i + 1) % len(files)][1]
        der = open(cert, "rb").read().hex()
        for path in (k8, k1):
            text = open(path, "rb").read()
            cases.append(f"K {text.hex()}")
            expect.append(("parse", path, f"0 ok {(d + point).hex()}"))
            cases.append(f"G {text.hex()} {msgs[i].hex() or '-'} {rng.randbytes(32).hex()}")
            expect.append(("sign", path, (point, msgs[i])))
        cases.append(f"X {der} {point.hex()}")
        expect.append(("cert", cert, "0 ok -"))
        cases.append(f"X {der} {other.hex()}")
        expect.append(("other", cert, "-92 key-certificate-mismatch -"))
    bad = 0
    for (kind, path, want), line in zip(expect, run(sign_driver, cases)):
        if kind == "sign":
            point, msg = want
            parts = line.split(" ")
            ok = parts[0].isdigit() and openssl_verifies(tmp, pem(point), msg, bytes.fromhex(parts[2]))
        else:
            ok = line == want
        if not ok:
            bad += 1
            print(f"{kind} {os.path.basename(path)}: got {line[:100]}")
    labels = sum(open(k8).read().count("EC PARAMETERS") > 0 for k8, _, _ in files)
    print(f"keys: {count} keys ({count - labels} PKCS#8, {labels} after an EC PARAMETERS block), each also as SEC 1: "
          f"{len(cases)} checks, {bad} differences")
    return bad


def main():
    mode = sys.argv[1]
    rng = random.Random(335)
    if mode == "openssl":
        count = int(sys.argv[4]) if len(sys.argv) > 4 else 10000
        bad = openssl_mode(sys.argv[2], sys.argv[3], count, rng)
    elif mode == "reference":
        bad = reference_mode(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 10000, rng)
    else:
        bad = keys_mode(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 100, rng)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
