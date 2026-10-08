#!/usr/bin/env python3
"""The fixture and the seeds of the `clientauth` fuzzing harness (docs/tls-server.md §13.11).

    python3 scripts/tls_clientauth_fuzz_seeds.py            # rewrites tests/programs/fuzz_clientauth_fixture.cho
                                                            # and tests/vectors/fuzz/clientauth/seed_*

`tests/programs/fuzz_clientauth.cho` hands the client-certificate code (`tls_clientauth.on_certificate` and
`on_certificate_verify`) one input: a first byte (bit 0 chooses the message, a Certificate or a CertificateVerify
after a good Certificate; bits 1 to 3 the client's chain among the fixture's five; bit 4 `required`) and the message's
body. The slot is as `serve` leaves it, with a transcript of the Certificate alone, so a CertificateVerify can be
valid. The fixture holds the clients' store and five chains from `tls_liar_client_auth`'s certificates; the seeds are
a good Certificate and a good CertificateVerify for each chain, and the Certificates of the lying client's cases.
Nothing is committed that a second run would not produce again byte for byte.
"""
import hashlib
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_liar_client_auth as a  # noqa: E402
from tls_liar_client import der, message, pem, u16, u24  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CHAINS = [
    ([a.DEVICE], a.P256_KEY, 0x0403),
    ([a.VIA_INT, a.INTERMEDIATE], a.P256_KEY, 0x0403),
    ([a.DEVICE_RSA], a.RSA_KEY, 0x0804),
    ([a.DEVICE_ED], a.ED_KEY, 0x0807),
    ([a.DEVICE_384], a.P384_KEY, 0x0503),
]


def literal(data):
    text = data.decode()
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'


def certificate_body(chain):
    entries = b"".join(u24(len(der(c))) + der(c) + u16(0) for c in chain)
    return b"\0" + u24(len(entries)) + entries


def verify_body(chain, key, scheme):
    cert = message(11, certificate_body(chain))
    content = b" " * 64 + b"TLS 1.3, client CertificateVerify\0" + hashlib.sha256(cert).digest()
    sig = a.sign(key, scheme, content)
    return u16(scheme) + u16(len(sig)) + sig


def main():
    lines = ["edition 5;", "module fuzz_clientauth_fixture;", "",
             "// The fixture of `fuzz_clientauth` (docs/tls-server.md §13.11): the clients' store and five chains of",
             "// `scripts/tls_liar_client_auth.py`, written by `scripts/tls_clientauth_fuzz_seeds.py`. The time is the",
             "// liar's NOW_MS in seconds.", "",
             "pub fn store() -> [] &static [byte] {", f"    return {literal(pem(a.CLIENT_CA))};", "}", ""]
    lines += ["pub fn chain(i: int) -> [] &static [byte] {"]
    for i, (chain, _, _) in enumerate(CHAINS):
        lines += [f"    if i == {i} {{", f"        return {literal(b''.join(pem(c) for c in chain))};", "    }"]
    lines += ['    return "";', "}", "", "pub fn now_s() -> [] int {", f"    return {a.NOW_MS // 1000};", "}", ""]
    open(os.path.join(ROOT, "tests/programs/fuzz_clientauth_fixture.cho"), "w").write("\n".join(lines))
    out = os.path.join(ROOT, "tests/vectors/fuzz/clientauth")
    os.makedirs(out, exist_ok=True)
    seeds = {}
    for i, (chain, key, scheme) in enumerate(CHAINS):
        seeds[f"seed_certificate_{i}"] = bytes([i << 1 | 16]) + certificate_body(chain)
        seeds[f"seed_certificate_{i}_optional"] = bytes([i << 1]) + certificate_body(chain)
        seeds[f"seed_verify_{i}"] = bytes([i << 1 | 1 | 16]) + verify_body(chain, key, scheme)
    body = certificate_body([a.DEVICE])
    seeds["seed_empty_required"] = bytes([16]) + b"\0\0\0\0"
    seeds["seed_empty_optional"] = bytes([0]) + b"\0\0\0\0"
    seeds["seed_context"] = bytes([16]) + b"\1\0" + body[1:]
    seeds["seed_entry_extension"] = bytes([16]) + body[:-2] + u16(2)[:2] + b"\0\0"
    seeds["seed_untrusted"] = bytes([16]) + certificate_body([a.UNTRUSTED])
    seeds["seed_expired"] = bytes([16]) + certificate_body([a.EXPIRED])
    seeds["seed_server_only"] = bytes([16]) + certificate_body([a.SERVER_ONLY])
    seeds["seed_six"] = bytes([16]) + certificate_body([a.DEEP_LEAF] + a.DEEP[::-1] + [a.CLIENT_CA])
    for name, data in seeds.items():
        open(os.path.join(out, name), "wb").write(data)
    print(f"{len(seeds)} seeds in {out}")


if __name__ == "__main__":
    main()
