#!/usr/bin/env python3
"""Writes `tests/vectors/ecdsa_sign/keys.txt`, the key files
`conformance/ecdsa_sign.rs` parses (docs/ecdsa-sign.md §5.3).

    python3 scripts/ecdsa_sign_keys.py

Each line is `<tag> <name> <file in hex> <key || point in hex, or ->`: the
tag `x509_key.parse_pem` must answer, and for a key it accepts, the private
key and public point it must give. The keys are made by `openssl` (its
version is written in the header) and then damaged here, one rule at a
time, so every refusal of `x509_key` that a file can reach has a file that
reaches it. They are test keys, made for this file and used nowhere else.
Also writes `certs.txt`: `<tag> <name> <certificate DER in hex> <point>`
for `matches_certificate`.
"""
import base64
import os
import subprocess
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "tests/vectors/ecdsa_sign")
N = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551
P256 = bytes.fromhex("06082a8648ce3d030107")
EC_ALG = bytes.fromhex("301306072a8648ce3d020106082a8648ce3d030107")


def openssl(*args, data=None):
    return subprocess.run(["openssl", *args], input=data, capture_output=True, check=True).stdout


def tlv(tag, body):
    n = len(body)
    if n < 0x80:
        head = bytes([n])
    elif n < 0x100:
        head = bytes([0x81, n])
    else:
        head = bytes([0x82, n >> 8, n & 0xFF])
    return bytes([tag]) + head + body


def armour(label, der, eol="\n"):
    body = base64.b64encode(der).decode()
    lines = [body[i:i + 64] for i in range(0, len(body), 64)]
    return (f"-----BEGIN {label}-----{eol}" + eol.join(lines) + f"{eol}-----END {label}-----{eol}").encode()


def unarmour(pem):
    lines = pem.decode().strip().splitlines()
    return base64.b64decode("".join(lines[1:-1]))


def sec1(d, point=None, params=True, version=1, dlen=32):
    body = tlv(0x02, bytes([version])) + tlv(0x04, d.to_bytes(dlen, "big") if dlen else b"")
    if params:
        body += tlv(0xA0, P256)
    if point is not None:
        body += tlv(0xA1, tlv(0x03, b"\x00" + point))
    return tlv(0x30, body)


def pkcs8(inner, version=0, alg=EC_ALG, outer_point=None):
    body = tlv(0x02, bytes([version])) + alg + tlv(0x04, inner)
    if outer_point is not None:
        body += tlv(0x81, b"\x00" + outer_point)
    return tlv(0x30, body)


def numbers(pem_path):
    out = openssl("pkey", "-in", pem_path, "-text", "-noout").decode()
    fields, name = {}, None
    for line in out.splitlines():
        if line.startswith("priv:") or line.startswith("pub:"):
            name = line[:-1]
            fields[name] = ""
        elif name and line.startswith("    "):
            fields[name] += line.strip().replace(":", "")
        else:
            name = None
    return int(fields["priv"], 16), bytes.fromhex(fields["pub"])


def main():
    os.makedirs(OUT, exist_ok=True)
    tmp = tempfile.mkdtemp()
    path = lambda n: os.path.join(tmp, n)
    rows, certs = [], []

    def add(tag, name, data, want=None):
        rows.append(f"{tag} {name} {data.hex() or '-'} {want.hex() if want else '-'}")

    # Two P-256 keys, one small enough for a 31-byte encoding.
    openssl("genpkey", "-algorithm", "EC", "-pkeyopt", "ec_paramgen_curve:P-256", "-out", path("a.pem"))
    openssl("genpkey", "-algorithm", "EC", "-pkeyopt", "ec_paramgen_curve:P-256", "-out", path("b.pem"))
    da, pa = numbers(path("a.pem"))
    db, pb = numbers(path("b.pem"))
    want_a = da.to_bytes(32, "big") + pa
    a8 = open(path("a.pem"), "rb").read()
    a1 = openssl("ec", "-in", path("a.pem"))
    add("ok", "pkcs8-genpkey", a8, want_a)
    add("ok", "sec1-openssl-ec", a1, want_a)
    add("ok", "sec1-crlf", a1.replace(b"\n", b"\r\n"), want_a)
    add("ok", "pkcs8-v2-public-key", armour("PRIVATE KEY", pkcs8(sec1(da, pa, params=False), 1, outer_point=pa)), want_a)
    attributes = tlv(0x30, tlv(0x02, b"\x00") + EC_ALG + tlv(0x04, sec1(da, pa, params=False)) + tlv(0xA0, b""))
    add("ok", "pkcs8-attributes", armour("PRIVATE KEY", attributes), want_a)
    ecparam = openssl("ecparam", "-name", "prime256v1", "-genkey")
    open(path("p.pem"), "wb").write(ecparam)
    dp, pp = numbers(path("p.pem"))
    add("ok", "ec-parameters-then-key", ecparam, dp.to_bytes(32, "big") + pp)
    # A key below 2^248, written in 31 bytes as OpenSSL before 1.1.0 did.
    open(path("s.pem"), "wb").write(armour("EC PRIVATE KEY", sec1(0x1234, None, dlen=31)))
    _, ps = numbers(path("s.pem"))
    add("ok", "sec1-31-byte-key", open(path("s.pem"), "rb").read(), (0x1234).to_bytes(32, "big") + ps)

    # Encrypted.
    add("key-encrypted", "pkcs8-encrypted", openssl("pkcs8", "-topk8", "-in", path("a.pem"), "-v2", "aes-256-cbc", "-passout", "pass:x"))
    add("key-encrypted", "sec1-encrypted", openssl("ec", "-in", path("a.pem"), "-aes256", "-passout", "pass:x"))
    # Not P-256, or not EC.
    openssl("genpkey", "-algorithm", "EC", "-pkeyopt", "ec_paramgen_curve:P-384", "-out", path("c.pem"))
    add("key-curve", "pkcs8-p384", open(path("c.pem"), "rb").read())
    add("key-curve", "sec1-p384", openssl("ec", "-in", path("c.pem")))
    add("key-curve", "sec1-explicit-parameters", openssl("ec", "-in", path("a.pem"), "-param_enc", "explicit"))
    # ECParameters as a SEQUENCE (explicit), not a named curve.
    explicit = tlv(0x30, bytes.fromhex("06072a8648ce3d0201") + tlv(0x30, b"\x02\x01\x01"))
    add("key-curve", "pkcs8-explicit-parameters", armour("PRIVATE KEY", pkcs8(sec1(da, pa, params=False), alg=explicit)))
    add("key-curve", "sec1-no-parameters", armour("EC PRIVATE KEY", sec1(da, pa, params=False)))
    openssl("genpkey", "-algorithm", "RSA", "-pkeyopt", "rsa_keygen_bits:2048", "-out", path("r.pem"))
    add("key-algorithm", "pkcs8-rsa", open(path("r.pem"), "rb").read())
    add("key-algorithm", "rsa-private-key", openssl("pkey", "-in", path("r.pem"), "-traditional"))
    openssl("genpkey", "-algorithm", "ED25519", "-out", path("e.pem"))
    add("key-algorithm", "pkcs8-ed25519", open(path("e.pem"), "rb").read())
    # PEM.
    add("key-pem", "empty", b"")
    cert = openssl("req", "-x509", "-new", "-key", path("a.pem"), "-subj", "/CN=a", "-days", "1")
    add("key-pem", "certificate-only", cert)
    add("key-pem", "bad-character", a8.replace(b"\n", b"\n*", 1)[:40] + a8[40:])
    add("key-pem", "no-end-line", a8[:a8.index(b"-----END")])
    add("key-pem", "end-label-differs", a8.replace(b"END PRIVATE KEY", b"END EC PRIVATE KEY"))
    add("key-pem", "end-label-same-length", a8.replace(b"END PRIVATE KEY", b"END PRIVATE KEX"))
    # SEC 1 of P-256 is 121 bytes, so its base64 ends `==`: moved inside.
    assert a1.count(b"==\n") == 1
    first = a1.index(b"\n") + 5
    add("key-pem", "padding-inside", a1[:first] + b"==" + a1[first:].replace(b"==\n", b"\n"))
    add("key-pem", "bad-padding", armour("PRIVATE KEY", unarmour(a8)).replace(b"-----END", b"=\n-----END"))
    add("key-size", "too-large", armour("PRIVATE KEY", b"\x00" * 9000))
    # DER.
    der = unarmour(a8)
    add("key-der", "truncated", armour("PRIVATE KEY", der[:-1]))
    add("key-der", "trailing-byte", armour("PRIVATE KEY", der + b"\x00"))
    add("key-der", "not-a-sequence", armour("PRIVATE KEY", b"\x31" + der[1:]))
    add("key-der", "sec1-wrong-key-tag", armour("EC PRIVATE KEY", sec1(da, pa).replace(b"\x04\x20", b"\x03\x20", 1)))
    add("key-der", "sec1-bad-public-key", armour("EC PRIVATE KEY", sec1(da, b"\x02" + pa[1:33] + pa[33:])))
    add("key-der", "pkcs8-extra-element", armour("PRIVATE KEY", tlv(0x30, tlv(0x02, b"\x00") + EC_ALG + tlv(0x04, sec1(da, pa, params=False)) + tlv(0x02, b"\x00"))))
    add("key-der", "sec1-extra-element", armour("EC PRIVATE KEY", tlv(0x30, sec1(da, pa)[2:] + tlv(0x02, b"\x00"))))
    add("key-version", "pkcs8-version-2", armour("PRIVATE KEY", pkcs8(sec1(da, pa, params=False), 2)))
    add("key-version", "sec1-version-0", armour("EC PRIVATE KEY", sec1(da, pa, version=0)))
    add("key-length", "sec1-33-byte-key", armour("EC PRIVATE KEY", sec1(da, None, dlen=33)))
    add("key-length", "sec1-empty-key", armour("EC PRIVATE KEY", sec1(0, None, dlen=0)))
    add("key-range", "sec1-key-zero", armour("EC PRIVATE KEY", sec1(0, None)))
    add("key-range", "sec1-key-n", armour("EC PRIVATE KEY", sec1(N, None)))
    add("key-range", "pkcs8-key-n-plus-1", armour("PRIVATE KEY", pkcs8(sec1(N + 1, None, params=False))))
    add("key-public-mismatch", "sec1-other-public-key", armour("EC PRIVATE KEY", sec1(da, pb)))
    add("key-public-mismatch", "pkcs8-v2-other-public-key", armour("PRIVATE KEY", pkcs8(sec1(da, None, params=False), 1, outer_point=pb)))
    add("key-public-mismatch", "pkcs8-v2-public-keys-differ", armour("PRIVATE KEY", pkcs8(sec1(da, pa, params=False), 1, outer_point=pb)))

    version = openssl("version").decode().strip()
    with open(os.path.join(OUT, "keys.txt"), "w") as f:
        f.write(f"# scripts/ecdsa_sign_keys.py, with {version}: <tag> <name> <file hex> <key || point hex>\n")
        f.write("\n".join(rows) + "\n")

    # Certificates for `matches_certificate`.
    der_cert = lambda key: openssl("req", "-x509", "-new", "-key", key, "-subj", "/CN=t", "-days", "1", "-outform", "DER")
    certs.append(f"ok cert-of-a {der_cert(path('a.pem')).hex()} {pa.hex()}")
    certs.append(f"key-certificate-mismatch cert-of-b {der_cert(path('b.pem')).hex()} {pa.hex()}")
    certs.append(f"key-certificate cert-p384 {der_cert(path('c.pem')).hex()} {pa.hex()}")
    certs.append(f"key-certificate cert-rsa {der_cert(path('r.pem')).hex()} {pa.hex()}")
    certs.append(f"key-certificate not-a-certificate 3000 {pa.hex()}")
    with open(os.path.join(OUT, "certs.txt"), "w") as f:
        f.write(f"# scripts/ecdsa_sign_keys.py, with {version}: <tag> <name> <certificate DER hex> <point hex>\n")
        f.write("\n".join(certs) + "\n")
    print(f"{len(rows)} keys, {len(certs)} certificates")


if __name__ == "__main__":
    main()
