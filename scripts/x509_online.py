#!/usr/bin/env python3
"""Real chains against the system's roots (docs/x509-verify.md §6.3).

    python3 scripts/x509_online.py <verify driver> limbo.json

x509-limbo's 14 `online::*` cases are chains saved from public sites, each
with the time it was saved. Each is verified here against this machine's
whole root bundle (/etc/ssl/certs/ca-certificates.crt, from Debian's
ca-certificates 20260601~24.04.1: Mozilla's roots), not against the one
root the case names:
- at its saved time, with its own name: must be `ok`;
- one second after the leaf's notAfter: `x509-expired`;
- one second before the leaf's notBefore: `x509-not-yet-valid`;
- under a name it does not hold (`<name>.invalid`): `x509-name-mismatch`;
- at exactly notAfter, and at exactly notBefore: `ok` (both are inclusive,
  RFC 5280 §4.1.2.5).

pyca/cryptography reads the dates and checks, independently, that each
case's root is in the bundle byte for byte. Writes
tests/vectors/x509/verify/online.txt (the driver's lines and answers) and
copies the bundle to tests/vectors/x509/verify/roots.pem, so the replay in
`conformance/x509_verify.rs` needs no system file. Exit status 1 on any
answer other than the one wanted.
"""
import datetime
import json
import shutil
import subprocess
import sys

from cryptography import x509
from cryptography.hazmat.primitives import serialization

BUNDLE = "/etc/ssl/certs/ca-certificates.crt"
OUT = "tests/vectors/x509/verify"


def der_hex(pem):
    return x509.load_pem_x509_certificate(pem.encode()).public_bytes(serialization.Encoding.DER).hex()


def main():
    driver, path = sys.argv[1], sys.argv[2]
    cases = [c for c in json.load(open(path))["testcases"] if c["id"].startswith("online::")]
    bundle = open(BUNDLE, "rb").read()
    roots = {r.public_bytes(serialization.Encoding.DER) for r in x509.load_pem_x509_certificates(bundle)}
    lines, want = [f"S {bundle.hex()}"], [None]
    for c in sorted(cases, key=lambda c: c["id"]):
        root = x509.load_pem_x509_certificate(c["trusted_certs"][0].encode())
        assert root.public_bytes(serialization.Encoding.DER) in roots, c["id"]
        leaf = x509.load_pem_x509_certificate(c["peer_certificate"].encode())
        chain = " ".join(der_hex(p) for p in [c["peer_certificate"]] + c["untrusted_intermediates"])
        name = c["expected_peer_name"]["value"]
        saved = int(datetime.datetime.fromisoformat(c["validation_time"]).timestamp())
        after = int(leaf.not_valid_after_utc.timestamp()) + 1
        before = int(leaf.not_valid_before_utc.timestamp()) - 1
        checks = [(saved, name, "ok"), (after, name, "x509-expired"), (before, name, "x509-not-yet-valid"),
                  (saved, name + ".invalid", "x509-name-mismatch"), (after - 1, name, "ok"), (before + 1, name, "ok")]
        for now, host, tag in checks:
            lines.append(f"V {now} 6 {host.encode().hex()} {chain}")
            want.append((c["id"], tag))
    out = subprocess.run([driver], input=("\n".join(lines) + "\n").encode(), capture_output=True, check=True).stdout.decode().splitlines()
    assert len(out) == len(lines)
    roots_line = out[0].split(" ")
    print(f"store: {roots_line[0]} roots, {roots_line[1]} skipped, {roots_line[2]} bytes")
    bad = 0
    for got, w in zip(out[1:], want[1:]):
        if got.split(" ")[1] != w[1]:
            print(f"  {w[0]}: wanted {w[1]}, got {got}")
            bad += 1
    print(f"{len(cases)} chains, {len(want) - 1} checks, {bad} wrong")
    with open(f"{OUT}/online.txt", "w") as f:
        f.write("# scripts/x509_online.py: x509-limbo's online:: chains against roots.pem (Debian ca-certificates\n")
        f.write("# 20260601~24.04.1); each line, then the driver's answer after `= `.\n")
        f.write("# `S @roots.pem` is that file's bytes in hex.\n")
        for line, got in zip(["S @roots.pem"] + lines[1:], out):
            f.write(f"{line}\n= {got}\n")
    shutil.copy(BUNDLE, f"{OUT}/roots.pem")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
