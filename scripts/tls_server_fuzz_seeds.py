#!/usr/bin/env python3
"""ClientHellos that resume, as seeds for the server's fuzz harnesses (docs/tls-server.md §12.8).

    python3 scripts/tls_server_fuzz_seeds.py <out dir>

Writes `<out>/server/<name>` (a chunk: a 2-byte length and one record holding the ClientHello, as `fuzz_server`
reads it) and `<out>/hello/<name>` (the ClientHello's body after its 4-byte header, as `fuzz_hello` reads it), for
the identity, key, time and ticket key of `tests/programs/fuzz_server_fixture.cho`: tickets sealed here in the
format of §12.1, so the first ClientHello of a run reaches the binder, the key schedule and the NewSessionTicket
code that an input of random bytes would take a campaign to find. A ticket that is right, one that is a bit off in
each of its fields, a wrong binder, several identities, `psk_ke` only, and a ClientHello that is padded to 16 KiB
around a ticket. Written with the same fixed bytes each time.
"""
import hashlib
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_liar_client as liar  # noqa: E402
import tls_liar_tickets as t  # noqa: E402

KEY = b"fuzz_server's ticket key, 32 by."
NOW = 1780272000000
HOST = liar.HOST


def seed_client(**attrs):
    c = liar.Client(None)
    for k, v in attrs.items():
        setattr(c, k, v)
    return c


def ticket(psk, **kw):
    fp = hashlib.sha256(liar.der(liar.MAIN)).digest()
    plain = t.forged_plain(psk, kw.pop("fp", fp), issue=NOW - 5000, **kw)
    return t.seal_ticket(KEY, plain)


def main():
    out = sys.argv[1]
    for d in ("server", "hello"):
        os.makedirs(os.path.join(out, d), exist_ok=True)
    psk = liar.seeded("fuzz psk")
    good = ticket(psk)
    entry = lambda tk, p=psk, h=32, age=5000: (tk, p, h, (age + 1234) % 2**32)  # noqa: E731
    cases = {
        "resume": dict(psks=[entry(good)]),
        "resume_wrong_binder": dict(psks=[entry(good)], binder_flip=0, binder_flip_at=-1),
        "resume_two": dict(psks=[entry(good[:100]), entry(good)]),
        "resume_psk_ke": dict(psks=[entry(good)], modes=b"\1\0"),
        "resume_no_modes": dict(psks=[entry(good)], modes=None),
        "resume_other_name": dict(psks=[entry(good)], host=b"nobody.example"),
        "resume_old": dict(psks=[entry(good, age=900_000)]),
        "resume_tampered": dict(psks=[entry(good[:50] + bytes([good[50] ^ 1]) + good[51:])]),
        "resume_version": dict(psks=[entry(ticket(psk, version=2))]),
        "resume_auth": dict(psks=[entry(ticket(psk, auth=1))]),
        "resume_sha384": dict(psks=[entry(ticket(psk + bytes(16)), p=psk + bytes(16), h=48)], suites=[0x1302]),
        "resume_early": dict(psks=[entry(good)], early=True),
        "resume_retry": dict(psks=[entry(good)], groups=[liar.P521, liar.P256], share_groups=[liar.P521]),
    }
    for name, attrs in cases.items():
        c = seed_client(**attrs)
        hello = c.client_hello()
        record = liar.plain_record(22, hello, 0x0301)
        open(os.path.join(out, "server", f"ticket_{name}"), "wb").write(len(record).to_bytes(2, "big") + record)
        open(os.path.join(out, "hello", f"ticket_{name}"), "wb").write(hello[4:])
    print(f"{len(cases)} seeds in {out}")


if __name__ == "__main__":
    main()
