#!/usr/bin/env python3
"""Prints `packages/x509/x509.ls`'s `oid_table` from dotted OIDs (docs/x509.md §2.3).

    python3 scripts/x509_oids.py

So no byte of an OID is typed by hand. Each encoding was also checked
against `openssl asn1parse -genstr OID:<dotted>`; paste the output over
the static's body when the list changes.
"""
OIDS = [
    (1, "rsaEncryption", "1.2.840.113549.1.1.1"),
    (2, "sha1WithRSAEncryption", "1.2.840.113549.1.1.5"),
    (3, "sha256WithRSAEncryption", "1.2.840.113549.1.1.11"),
    (4, "sha384WithRSAEncryption", "1.2.840.113549.1.1.12"),
    (5, "sha512WithRSAEncryption", "1.2.840.113549.1.1.13"),
    (6, "rsassa-pss", "1.2.840.113549.1.1.10"),
    (7, "ecPublicKey", "1.2.840.10045.2.1"),
    (8, "prime256v1", "1.2.840.10045.3.1.7"),
    (9, "secp384r1", "1.3.132.0.34"),
    (10, "secp521r1", "1.3.132.0.35"),
    (11, "ecdsa-with-SHA256", "1.2.840.10045.4.3.2"),
    (12, "ecdsa-with-SHA384", "1.2.840.10045.4.3.3"),
    (13, "ecdsa-with-SHA512", "1.2.840.10045.4.3.4"),
    (14, "Ed25519", "1.3.101.112"),
    (20, "subjectKeyIdentifier", "2.5.29.14"),
    (21, "keyUsage", "2.5.29.15"),
    (22, "subjectAltName", "2.5.29.17"),
    (23, "basicConstraints", "2.5.29.19"),
    (24, "nameConstraints", "2.5.29.30"),
    (25, "cRLDistributionPoints", "2.5.29.31"),
    (26, "certificatePolicies", "2.5.29.32"),
    (27, "authorityKeyIdentifier", "2.5.29.35"),
    (28, "extKeyUsage", "2.5.29.37"),
    (29, "policyConstraints", "2.5.29.36"),
    (30, "inhibitAnyPolicy", "2.5.29.54"),
    (31, "policyMappings", "2.5.29.33"),
    (32, "authorityInfoAccess", "1.3.6.1.5.5.7.1.1"),
    (40, "serverAuth", "1.3.6.1.5.5.7.3.1"),
    (41, "clientAuth", "1.3.6.1.5.5.7.3.2"),
    (42, "anyExtendedKeyUsage", "2.5.29.37.0"),
]
def enc(dotted):
    a = [int(x) for x in dotted.split(".")]
    out = [a[0] * 40 + a[1]]
    for v in a[2:]:
        b = [v & 0x7f]
        v >>= 7
        while v:
            b.append(0x80 | (v & 0x7f)); v >>= 7
        out += reversed(b)
    return out
lines = []
i = 0
for code, name, dotted in OIDS:
    b = enc(dotted)
    lines.append(f"    // {code}: {name} {dotted}")
    for v in [code, len(b)] + b:
        lines.append(f"    t[{i}] = {v:#04x};")
        i += 1
print(f"    let t = alloc_slice[static]({i + 1}, 0);")
print("\n".join(lines))
print(f"    t[{i}] = 0;")
