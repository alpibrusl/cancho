#!/usr/bin/env python3
"""Writes `tests/vectors/x509/`: the certificates `packages/x509` is tested on (docs/x509.md §5.2).

    python3 scripts/x509_corpus.py

Run once; its output is committed, and the test reads the files, not this
script (keys are random, so a second run writes different bytes).

- `corpus.pem`: certificates made here with pyca/cryptography, one of each
  key type and signature algorithm `docs/x509.md` §2 names, with every
  extension the parser reads, and the two leniencies.
- `corpus.txt`: the line `tests/programs/x509_driver.cho` must print for each,
  as `scripts/x509_check.py` computes it from pyca's reading.
- `negative.txt`: `name | tag | hex`, one damaged certificate per refusal,
  each made by editing one field of a good certificate's DER tree and
  re-encoding (so every other length stays right, and the refusal is the
  one named, not a truncation it caused).
"""
import base64
import datetime
import ipaddress
import os
import sys

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519, padding, rsa
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from x509_check import expected  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "vectors", "x509")


# ---- A DER tree: (tag, [children]) for constructed, (tag, bytes) for primitive ----


def decode(d, at=0, end=None):
    end = len(d) if end is None else end
    out = []
    while at < end:
        tag, n = d[at], d[at + 1]
        at += 2
        if n & 0x80:
            k = n & 0x7F
            n = int.from_bytes(d[at : at + k], "big")
            at += k
        body = d[at : at + n]
        out.append((tag, decode(d, at, at + n) if tag & 0x20 else body))
        at += n
    return out


def length(n, long_form=False):
    if n < 0x80 and not long_form:
        return bytes([n])
    b = n.to_bytes(max(1, (n.bit_length() + 7) // 8), "big")
    return bytes([0x80 | len(b)]) + b


def encode(nodes, long_form=()):
    out = b""
    for node in nodes:
        tag, body = node[0], node[1]
        content = encode(body, long_form) if isinstance(body, list) else body
        out += bytes([tag]) + length(len(content), id(node) in long_form) + content
    return out


def tbs_fields(cert):
    return cert[0][1][0][1]


def extension_list(cert):
    for tag, body in tbs_fields(cert):
        if tag == 0xA3:
            return body[0][1]
    raise ValueError("no extensions")


def oid(dotted):
    a = [int(x) for x in dotted.split(".")]
    out = [a[0] * 40 + a[1]]
    for v in a[2:]:
        b = [v & 0x7F]
        v >>= 7
        while v:
            b.append(0x80 | (v & 0x7F))
            v >>= 7
        out += reversed(b)
    return bytes(out)


def find_extension(cert, dotted):
    for ext in extension_list(cert):
        if ext[1][0][1] == oid(dotted):
            return ext
    raise ValueError(dotted)


# ---- Good certificates ----

NOW = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)


def name(cn):
    return x509.Name([x509.NameAttribute(NameOID.ORGANIZATION_NAME, "cancho tests"), x509.NameAttribute(NameOID.COMMON_NAME, cn)])


def build(subject, issuer, key, signer, serial, days, exts, alg=hashes.SHA256(), pss=False, start=NOW):
    b = x509.CertificateBuilder().subject_name(subject).issuer_name(issuer).public_key(key.public_key())
    b = b.serial_number(serial).not_valid_before(start).not_valid_after(start + datetime.timedelta(days=days))
    for ext, critical in exts:
        b = b.add_extension(ext, critical)
    if isinstance(signer, ed25519.Ed25519PrivateKey):
        return b.sign(signer, None)
    if pss:
        return b.sign(signer, alg, rsa_padding=padding.PSS(padding.MGF1(alg), padding.PSS.DIGEST_LENGTH))
    return b.sign(signer, alg)


def ski(key):
    return x509.SubjectKeyIdentifier.from_public_key(key.public_key())


def aki(key):
    return x509.AuthorityKeyIdentifier.from_issuer_public_key(key.public_key())


def ca_usage():
    return x509.KeyUsage(False, False, False, False, False, True, True, False, False)


def leaf_usage():
    return x509.KeyUsage(True, False, True, False, False, False, False, False, False)


def good():
    rsa_root = rsa.generate_private_key(65537, 3072)
    p384_ca = ec.generate_private_key(ec.SECP384R1())
    rsa_leaf = rsa.generate_private_key(65537, 2048)
    p256_leaf = ec.generate_private_key(ec.SECP256R1())
    p521 = ec.generate_private_key(ec.SECP521R1())
    edkey = ed25519.Ed25519PrivateKey.generate()
    rsa4096 = rsa.generate_private_key(3, 4096)
    root_name, ca_name = name("Test Root R1"), name("Test CA E1")
    certs = []
    # An RSA-3072 root, SHA-384.
    certs.append(
        build(root_name, root_name, rsa_root, rsa_root, 1, 3650,
              [(x509.BasicConstraints(True, None), True), (ca_usage(), True), (ski(rsa_root), False)],
              alg=hashes.SHA384())
    )
    # A P-384 intermediate under it: path length 0, name constraints, EKU serverAuth.
    nc = x509.NameConstraints([x509.DNSName("example.com"), x509.IPAddress(ipaddress.ip_network("10.0.0.0/8"))], [x509.DNSName("bad.example.com")])
    certs.append(
        build(ca_name, root_name, p384_ca, rsa_root, 0x00FF00FF00FF00FF00FF, 1825,
              [(x509.BasicConstraints(True, 0), True), (ca_usage(), True), (nc, True), (ski(p384_ca), False), (aki(rsa_root), False),
               (x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]), False)],
              alg=hashes.SHA256())
    )
    # A P-256 leaf from the P-384 CA, ECDSA-SHA384: every kind of SAN entry the parser checks.
    san = x509.SubjectAlternativeName([
        x509.DNSName("www.example.com"),
        x509.DNSName("*.api.example.com"),
        x509.IPAddress(ipaddress.ip_address("10.1.2.3")),
        x509.IPAddress(ipaddress.ip_address("2001:db8::1")),
        x509.RFC822Name("ops@example.com"),
        x509.UniformResourceIdentifier("https://example.com/"),
        x509.DirectoryName(name("dir")),
        x509.RegisteredID(x509.ObjectIdentifier("1.2.3.4")),
    ])
    certs.append(
        build(name("www.example.com"), ca_name, p256_leaf, p384_ca, 0x7F, 397,
              [(x509.BasicConstraints(False, None), True), (leaf_usage(), True), (san, False), (aki(p384_ca), False),
               (x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH, ExtendedKeyUsageOID.CLIENT_AUTH]), False)],
              alg=hashes.SHA384())
    )
    # An RSA-2048 leaf from the root, RSASSA-PSS; EKU any and an unknown one; a 20-byte serial with its top bit set.
    certs.append(
        build(name("pss.example.com"), root_name, rsa_leaf, rsa_root, 0x80 << 144, 90,
              [(x509.SubjectAlternativeName([x509.DNSName("pss.example.com")]), True),
               (x509.ExtendedKeyUsage([ExtendedKeyUsageOID.ANY_EXTENDED_KEY_USAGE, ExtendedKeyUsageOID.CODE_SIGNING]), False)],
              pss=True)
    )
    # A self-signed P-521 certificate, ECDSA-SHA512, keyUsage with decipherOnly (bit 8: a two-byte BIT STRING).
    certs.append(
        build(name("P-521"), name("P-521"), p521, p521, 2, 30,
              [(x509.KeyUsage(False, False, False, False, True, False, False, False, True), False)], alg=hashes.SHA512())
    )
    # A self-signed Ed25519 certificate valid past 2050 (its notAfter a GeneralizedTime, as RFC 5280 requires), no extensions.
    certs.append(build(name("Ed25519"), name("Ed25519"), edkey, edkey, 3, 365 * 30, []))
    # A self-signed RSA-4096 key with exponent 3, SHA-512 signed, starting in 1999 (a 19xx UTCTime).
    certs.append(
        build(name("RSA-4096"), name("RSA-4096"), rsa4096, rsa4096, 4, 365 * 40,
              [(x509.BasicConstraints(True, 5), True)], alg=hashes.SHA512(), start=datetime.datetime(1999, 12, 31, 23, 59, 59, tzinfo=datetime.timezone.utc))
    )
    der = [c.public_bytes(serialization.Encoding.DER) for c in certs]
    # A v1 certificate: the Ed25519 one without its version (the signature no longer matches; the parser does not check it).
    t = decode(der[5])
    fields = tbs_fields(t)
    del fields[0]
    der.append(encode(t))
    # The two leniencies: a GeneralizedTime notBefore in 2026, and a critical flag written out as FALSE.
    t = decode(der[2])
    validity = tbs_fields(t)[4][1]
    validity[0] = (0x18, b"20260101000000Z")
    der.append(encode(t))
    t = decode(der[2])
    ext = find_extension(t, "2.5.29.35")
    ext[1].insert(1, (0x01, b"\x00"))
    der.append(encode(t))
    return der


# ---- Damaged certificates: one refusal each ----


def damage(base):
    """`base` is a leaf with SAN, basicConstraints, keyUsage and AKI (corpus.pem's third)."""
    cases = []

    def edit(label, tag, fn, long_form=None):
        t = decode(base)
        nodes = fn(t)
        keep = set()
        if long_form:
            keep = {id(long_form(t))}
        out = nodes if isinstance(nodes, bytes) else encode(t, keep)
        cases.append((label, tag, out.hex()))

    edit("cut short by one byte", "der-truncated", lambda t: base[:-1])
    edit("a byte after the certificate", "der-trailing-bytes", lambda t: base + b"\x00")
    edit("indefinite length", "der-indefinite-length", lambda t: base[:1] + b"\x80" + base[4:] + b"\x00\x00")
    edit("serial's length in two bytes", "der-non-minimal-length", lambda t: None, long_form=lambda t: tbs_fields(t)[1])
    edit("length with a leading zero byte", "der-non-minimal-length", lambda t: base[:1] + b"\x83\x00" + base[2:])
    edit("high-tag-number form", "der-tag", lambda t: (tbs_fields(t).__setitem__(1, (0x1F, b"\x02\x01")), None)[1])
    edit("TBSCertificate as a SET", "der-tag", lambda t: (t[0][1].__setitem__(0, (0x31, t[0][1][0][1])), None)[1])

    def deep(t):
        nest = (0x04, b"x")
        for _ in range(30):
            nest = (0x30, [nest])
        tbs_fields(t)[5][1][0][1][0][1][1] = nest
    edit("30 nested SEQUENCEs in a name", "der-too-deep", deep)
    edit("serial with a leading zero", "der-integer", lambda t: tbs_fields(t).__setitem__(1, (0x02, b"\x00\x7f")))
    edit("serial of 22 bytes", "der-integer", lambda t: tbs_fields(t).__setitem__(1, (0x02, b"\x01" * 22)))
    edit("pathLenConstraint negative", "der-integer",
         lambda t: find_extension(t, "2.5.29.19")[1].__setitem__(-1, (0x04, encode([(0x30, [(0x01, b"\xff"), (0x02, b"\xff")])]))))
    edit("critical flag 0x01", "der-boolean", lambda t: find_extension(t, "2.5.29.19")[1].__setitem__(1, (0x01, b"\x01")))
    edit("signature with unused bits", "der-bit-string", lambda t: t[0][1].__setitem__(2, (0x03, b"\x01" + t[0][1][2][1][1:])))
    edit("keyUsage padding bits set", "der-bit-string",
         lambda t: find_extension(t, "2.5.29.15")[1].__setitem__(-1, (0x04, encode([(0x03, b"\x07\x81")]))))
    edit("a NULL after the extensions", "x509-structure", lambda t: tbs_fields(t).append((0x05, b"")))
    edit("empty extensions", "x509-structure", lambda t: tbs_fields(t).__setitem__(-1, (0xA3, [(0x30, [])])))
    edit("empty subjectAltName", "x509-structure", lambda t: find_extension(t, "2.5.29.17")[1].__setitem__(-1, (0x04, encode([(0x30, [])]))))
    edit("version 4", "x509-version", lambda t: tbs_fields(t).__setitem__(0, (0xA0, [(0x02, b"\x03")])))
    edit("extensions in a v2 certificate", "x509-version", lambda t: tbs_fields(t).__setitem__(0, (0xA0, [(0x02, b"\x01")])))
    edit("month 13", "x509-time", lambda t: tbs_fields(t)[4][1].__setitem__(0, (0x17, b"261301000000Z")))
    edit("February 29 in a common year", "x509-time", lambda t: tbs_fields(t)[4][1].__setitem__(0, (0x17, b"270229000000Z")))
    edit("UTCTime without seconds", "x509-time", lambda t: tbs_fields(t)[4][1].__setitem__(0, (0x17, b"2601010000Z")))
    edit("GeneralizedTime with a fraction", "x509-time", lambda t: tbs_fields(t)[4][1].__setitem__(1, (0x18, b"20500101000000.5Z")))
    edit("outer algorithm differs from the inner one", "x509-signature-algorithm-mismatch",
         lambda t: t[0][1].__setitem__(1, (0x30, [(0x06, oid("1.2.840.10045.4.3.2"))])))

    def dup(t):
        exts = extension_list(t)
        exts.append(exts[-1])
    edit("an extension twice", "x509-duplicate-extension", dup)
    edit("unknown critical extension", "x509-critical-extension",
         lambda t: extension_list(t).append((0x30, [(0x06, oid("1.2.3.4.5")), (0x01, b"\xff"), (0x04, b"\x05\x00")])))
    edit("critical certificatePolicies", "x509-critical-extension",
         lambda t: extension_list(t).append((0x30, [(0x06, oid("2.5.29.32")), (0x01, b"\xff"), (0x04, encode([(0x30, [(0x30, [(0x06, oid("2.5.29.32.0"))])])]))])))
    edit("1,025 SAN entries", "x509-too-large",
         lambda t: find_extension(t, "2.5.29.17")[1].__setitem__(-1, (0x04, encode([(0x30, [(0x82, b"a.example")] * 1025)]))))

    def many(t):
        exts = extension_list(t)
        for k in range(40):
            exts.append((0x30, [(0x06, oid(f"1.2.3.{k}")), (0x04, b"\x05\x00")]))
    edit("40 extensions", "x509-too-large", many)
    edit("over 16,384 bytes", "x509-too-large", lambda t: tbs_fields(t)[5][1][0][1][0][1].__setitem__(1, (0x0C, b"x" * 16400)))
    edit("an empty RDN", "x509-name", lambda t: tbs_fields(t)[5][1].append((0x31, [])))
    edit("an attribute without a value", "x509-name", lambda t: tbs_fields(t)[5][1][0][1][0][1].pop())
    edit("non-ASCII dNSName", "x509-name", lambda t: find_extension(t, "2.5.29.17")[1].__setitem__(-1, (0x04, encode([(0x30, [(0x82, "é.example".encode())])]))))
    edit("8-byte iPAddress", "x509-name", lambda t: find_extension(t, "2.5.29.17")[1].__setitem__(-1, (0x04, encode([(0x30, [(0x87, bytes(8))])]))))

    def short_point(t):
        spki = tbs_fields(t)[6][1]
        spki[1] = (0x03, spki[1][1][:-1])
    edit("P-256 point one byte short", "x509-key", short_point)

    def compressed(t):
        spki = tbs_fields(t)[6][1]
        spki[1] = (0x03, b"\x00\x02" + spki[1][1][2:34])
    edit("compressed P-256 point", "x509-key", compressed)
    return cases


def main():
    os.makedirs(OUT, exist_ok=True)
    der = good()
    with open(os.path.join(OUT, "corpus.pem"), "w") as f:
        for d in der:
            body = base64.b64encode(d).decode()
            lines = [body[i : i + 64] for i in range(0, len(body), 64)]
            f.write("-----BEGIN CERTIFICATE-----\n" + "\n".join(lines) + "\n-----END CERTIFICATE-----\n")
    with open(os.path.join(OUT, "corpus.txt"), "w") as f:
        for n, d in enumerate(der):
            if n < len(der) - 2:
                e = expected(x509.load_der_x509_certificate(d))
            else:
                # pyca refuses an explicit DEFAULT (`EncodedDefault`), so the two lenient ones are the third
                # certificate's line with only the flag changed: the GeneralizedTime names the same instant.
                e = dict(expected(x509.load_der_x509_certificate(der[2])), lenient=str(n - len(der) + 3))
            f.write("0 ok " + " ".join(f"{k}={v}" for k, v in e.items()) + "\n")
    with open(os.path.join(OUT, "negative.txt"), "w") as f:
        f.write("# scripts/x509_corpus.py: name | tag | DER in hex. Each damages one field of corpus.pem's third certificate.\n")
        cases = damage(der[2])
        for label, tag, hexed in cases:
            f.write(f"{label} | {tag} | {hexed}\n")
    print(f"{len(der)} certificates, {len(cases)} damaged ones")


if __name__ == "__main__":
    main()
