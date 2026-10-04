#!/usr/bin/env python3
"""`packages/x509` against pyca/cryptography over a PEM bundle (docs/x509.md §5.1).

    python3 scripts/x509_check.py <driver> [<bundle.pem>]

`driver` is `tests/programs/x509_driver.ls` built with `lex-sys build --std`
together with `packages/x509/x509.ls`; the bundle defaults to the system's,
`/etc/ssl/certs/ca-certificates.crt`. Every certificate must be accepted
(`0 ok`), and every field the driver prints must equal what pyca/cryptography
(OpenSSL's and rust-asn1's parsers underneath) says it is. The two leniency
flags are computed here with a separate DER walk, not by pyca, which accepts
both silently. Exit status 1 on any difference.
"""
import subprocess
import sys

from cryptography import x509
from cryptography.hazmat.primitives.asymmetric import ec, ed25519, rsa
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat
from cryptography.x509.oid import ExtendedKeyUsageOID, ExtensionOID

SIGNATURES = {
    "1.2.840.113549.1.1.5": 2,
    "1.2.840.113549.1.1.11": 3,
    "1.2.840.113549.1.1.12": 4,
    "1.2.840.113549.1.1.13": 5,
    "1.2.840.113549.1.1.10": 6,
    "1.2.840.10045.4.3.2": 11,
    "1.2.840.10045.4.3.3": 12,
    "1.2.840.10045.4.3.4": 13,
    "1.3.101.112": 14,
}
CURVES = {"secp256r1": 8, "secp384r1": 9, "secp521r1": 10}
KEY_USAGE = [
    "digital_signature",
    "content_commitment",
    "key_encipherment",
    "data_encipherment",
    "key_agreement",
    "key_cert_sign",
    "crl_sign",
]


def tlv(d, at):
    """One DER TLV at `at`: (tag, content start, content end)."""
    tag, n = d[at], d[at + 1]
    at += 2
    if n & 0x80:
        k = n & 0x7F
        n = int.from_bytes(d[at : at + k], "big")
        at += k
    return tag, at, at + n


def children(d, s, e):
    out = []
    while s < e:
        t = tlv(d, s)
        out.append(t)
        s = t[2]
    return out


def leniency(der):
    """GeneralizedTime before 2050 is 1, an explicit DEFAULT FALSE is 2."""
    flags = 0
    _, s, e = tlv(der, 0)
    tbs = children(der, *tlv(der, s)[1:])
    if tbs[0][0] == 0xA0:
        tbs = tbs[1:]
    for tag, ts, te in children(der, tbs[3][1], tbs[3][2]):
        if tag == 0x18 and int(der[ts : ts + 4]) < 2050:
            flags |= 1
    for tag, ts, te in tbs:
        if tag != 0xA3:
            continue
        exts = tlv(der, ts)
        for _, xs, xe in children(der, exts[1], exts[2]):
            parts = children(der, xs, xe)
            if parts[1][0] == 0x01 and der[parts[1][1]] == 0:
                flags |= 2
            oid = der[parts[0][1] : parts[0][2]]
            if oid == bytes([0x55, 0x1D, 0x13]):
                value = parts[-1]
                inner = tlv(der, value[1])
                fields = children(der, inner[1], inner[2])
                if fields and fields[0][0] == 0x01 and der[fields[0][1]] == 0:
                    flags |= 2
    return flags


def serial_bytes(n):
    k = (n.bit_length() + 8) // 8 if n >= 0 else ((-n - 1).bit_length() + 8) // 8
    return n.to_bytes(max(k, 1), "big", signed=True)


def expected(cert):
    der = cert.public_bytes(Encoding.DER)
    f = {"v": str(cert.version.value + 1)}
    f["serial"] = serial_bytes(cert.serial_number).hex()
    f["issuer"] = cert.issuer.public_bytes().hex()
    f["subject"] = cert.subject.public_bytes().hex()
    f["nb"] = str(int(cert.not_valid_before_utc.timestamp()))
    f["na"] = str(int(cert.not_valid_after_utc.timestamp()))
    key = cert.public_key()
    if isinstance(key, rsa.RSAPublicKey):
        bits = key.public_bytes(Encoding.DER, PublicFormat.PKCS1)
        f["key"] = f"1/0/{len(bits)}"
    elif isinstance(key, ec.EllipticCurvePublicKey):
        point = key.public_bytes(Encoding.X962, PublicFormat.UncompressedPoint)
        f["key"] = f"7/{CURVES[key.curve.name]}/{len(point)}"
    elif isinstance(key, ed25519.Ed25519PublicKey):
        f["key"] = "14/0/32"
    else:
        f["key"] = "unknown"
    ext = cert.extensions
    try:
        bc = ext.get_extension_for_oid(ExtensionOID.BASIC_CONSTRAINTS).value
        f["ca"] = "1" if bc.ca else "0"
        f["pathlen"] = str(-1 if bc.path_length is None else bc.path_length)
    except x509.ExtensionNotFound:
        f["ca"], f["pathlen"] = "-1", "-1"
    try:
        ku = ext.get_extension_for_oid(ExtensionOID.KEY_USAGE).value
        mask = sum(1 << i for i, name in enumerate(KEY_USAGE) if getattr(ku, name))
        if ku.key_agreement:
            mask |= (1 << 7) * ku.encipher_only | (1 << 8) * ku.decipher_only
        f["ku"] = str(mask)
    except x509.ExtensionNotFound:
        f["ku"] = "-1"
    try:
        eku = ext.get_extension_for_oid(ExtensionOID.EXTENDED_KEY_USAGE).value
        mask = 0
        for oid in eku:
            mask |= {
                ExtendedKeyUsageOID.SERVER_AUTH: 1,
                ExtendedKeyUsageOID.CLIENT_AUTH: 2,
                ExtendedKeyUsageOID.ANY_EXTENDED_KEY_USAGE: 4,
            }.get(oid, 8)
        f["eku"] = str(mask)
    except x509.ExtensionNotFound:
        f["eku"] = "-1"
    try:
        san = ext.get_extension_for_oid(ExtensionOID.SUBJECT_ALTERNATIVE_NAME).value
        whole = san.public_bytes()
        f["san"] = whole[tlv(whole, 0)[1] :].hex()
    except x509.ExtensionNotFound:
        f["san"] = ""
    f["sig"] = str(SIGNATURES.get(cert.signature_algorithm_oid.dotted_string, 0))
    f["lenient"] = str(leniency(der))
    f["ext"] = str(len(ext))
    return f


def main():
    driver = sys.argv[1]
    path = sys.argv[2] if len(sys.argv) > 2 else "/etc/ssl/certs/ca-certificates.crt"
    text = open(path, "rb").read()
    certs = x509.load_pem_x509_certificates(text)
    out = subprocess.run([driver], input=text, capture_output=True, check=True).stdout
    lines = out.decode().splitlines()
    bad = 0
    if len(lines) != len(certs):
        print(f"{len(certs)} certificates, {len(lines)} answers")
        bad += 1
    for n, (cert, line) in enumerate(zip(certs, lines)):
        words = line.split(" ")
        if words[:2] != ["0", "ok"]:
            print(f"#{n} {cert.subject.rfc4514_string()}: {' '.join(words[:2])}")
            bad += 1
            continue
        got = dict(w.split("=", 1) for w in words[2:])
        for name, want in expected(cert).items():
            if got.get(name) != want:
                print(f"#{n} {cert.subject.rfc4514_string()}: {name} {got.get(name)} != {want}")
                bad += 1
    print(f"{len(certs)} certificates, {bad} differences")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
