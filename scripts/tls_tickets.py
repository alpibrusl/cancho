#!/usr/bin/env python3
"""The engine's rules for offering a saved ticket (docs/tls-resumption.md §3), recorded.

    python3 scripts/tls_tickets.py <tickets> <out.txt>

`tickets` is `tests/programs/tls_tickets.ls` built with `--std`, `packages/tls/tls.ls` and the package's
files: the engine, one slot and room for four tickets, resumption on, entropy fixed. Every full handshake's
ClientHello must advertise psk_dhe_ke: a server may withhold tickets from one that does not (RFC 8446 §4.2.9). Each case first loads the liar's one root. The server is `scripts/tls_liar.py`'s
honest TLS 1.3 server, written on pyca/cryptography and RFC 8446 alone; it issues a ticket after a full
handshake, and checks the binder of any ticket offered back. Each case is one engine process, and decides
from the ClientHello alone whether the engine offered the ticket: a rule that should keep a ticket back is
broken exactly when `pre_shared_key` is on the wire.

The cases:
- offered for the same host in time, with the obfuscated age RFC 8446 §4.2.11.1 asks for (the milliseconds
  since the ticket was received plus its ticket_age_add), and the server resumes (no Certificate), checked
  through `tls.resumed`;
- the age counted in milliseconds, not from a whole second (wolfSSL refuses an age more than 1 s high);
- not offered for another host name, and the ticket is spent by that refusal;
- not offered after `tls.trust` is called again;
- not offered after the leaf's notAfter (a leaf valid for an hour, the maximum age raised past it), and
  offered just before;
- not offered after the maximum age (60 s), and offered just before;
- not offered after the ticket's own lifetime (100 s);
- not offered when the clock is before the ticket was received;
- offered once: a second start with the same handle offers nothing;
- not offered after `tls.forget`;
- a full table (four tickets) replaces the oldest: its handle offers nothing, the newest's does;
- pools (docs/tls-resumption.md §12), each ticket with an identity of its own so the ClientHello says which
  was offered: a pool of one by default; three in a pool, offered newest first, then none; a pool over its
  size losing its oldest; `forget` emptying a pool; a refused ticket overwritten and an older good one
  offered; a pool refilled after it was emptied; a handle never issued.

The file holds each case's lines and answers; `crates/lex-sys/tests/conformance/tls.rs` replays them on both
backends. Exit status 1 if any case fails.
"""
import datetime
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_liar as L  # noqa: E402

from cryptography import x509  # noqa: E402
from cryptography.hazmat.primitives import serialization  # noqa: E402
from cryptography.x509.oid import NameOID  # noqa: E402

# The same length as HOST, so that only comparing its bytes tells them apart.
OTHER_HOST = b"lier.lex-sys.test"
assert len(OTHER_HOST) == len(L.HOST)


def short_lived():
    """The server's certificate, valid for one hour from NOW."""
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, L.HOST.decode())])
    now = datetime.datetime.fromtimestamp(L.NOW, datetime.timezone.utc)
    cert = (x509.CertificateBuilder().subject_name(name).issuer_name(L.CA[1].subject).public_key(L.KEY.public_key())
            .serial_number(207).not_valid_before(L.START).not_valid_after(now + datetime.timedelta(hours=1))
            .add_extension(x509.SubjectAlternativeName([x509.DNSName(L.HOST.decode())]), False)
            .sign(L.CA[0], None))
    return cert.public_bytes(serialization.Encoding.DER)


def modes_of(msg):
    """The psk_key_exchange_modes of the ClientHello `msg`, or None."""
    b = msg[4:]
    at = 2 + 32
    at += 1 + b[at]
    at += 2 + int.from_bytes(b[at:at + 2], "big")
    at += 1 + b[at]
    end = at + 2 + int.from_bytes(b[at:at + 2], "big")
    at += 2
    while at < end:
        kind, n = int.from_bytes(b[at:at + 2], "big"), int.from_bytes(b[at + 2:at + 4], "big")
        if kind == 45:
            body = b[at + 4:at + 4 + n]
            return list(body[1:1 + body[0]])
        at += 4 + n
    return None


class Engine(L.Server):
    """The liar's honest server, for a connection the engine starts with `C`."""

    def begin(self, now, handle=0, host=L.HOST, ms=0):
        """A start at `now` seconds and `ms` milliseconds."""
        f = self.c.ask(f"C {host.hex()} {now * 1000 + ms} {handle}")
        assert f[0] == "0", f
        (hello,) = self.c.take()
        msg, self.sid, self.client_shares, _ = L.parse_client_hello(hello)
        self.transcript = msg
        self.ccs_sent = False
        self.modes = modes_of(msg)
        return L.psk_offer(msg)


def full(c, now=L.NOW, cert_der=None, lifetime=7200, ms=0, ticket=L.TICKET, pool=None):
    """A full handshake at `now`, a ticket, close_notify both ways, then `tls.save` (or `tls.save_to(pool)`):
    the handle and the PSK."""
    s = Engine(c)
    if cert_der is not None:
        s.cert_der = cert_der
    assert s.begin(now, ms=ms) is None, "nothing offered with handle 0"
    assert s.modes == [1], f"psk_dhe_ke advertised, so a server may send tickets (RFC 8446 §4.2.9): {s.modes}"
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    psk = L.issue(s, ticket=ticket, lifetime=lifetime)
    s.c.feed(s.write.seal(21, b"\1\0"))
    s.c.ask("Q")
    (alert,) = s.c.take()
    assert s.read.open(alert) == (21, b"\1\0"), "the client's close_notify"
    f = c.ask("V" if pool is None else f"S {pool}")
    handle = int(f[0])
    assert handle > 0, f"a handle: {f}"
    return handle, psk


def offered(c, now, handle, host=L.HOST, ms=0):
    """A start with `handle`: the server, and whether the ticket was on the wire."""
    r = Engine(c)
    r.offer = r.begin(now, handle, host, ms)
    return r, r.offer is not None


CASES = []


def case(name):
    def register(fn):
        CASES.append((name, fn))
        return fn
    return register


@case("the same host, in time: offered, and the server resumes")
def same_host(c):
    h, psk = full(c)
    r, on = offered(c, L.NOW + 60, h)
    assert on, "offered"
    assert r.offer[1] == (60000 + 0x01020304) % 2**32, f"the obfuscated age: {r.offer[1]}"
    L.check_binder(r.transcript, psk)
    r.psk = psk
    r.c.feed(L.resumed_flight(r))
    L.resumed_to_the_end(r)
    assert c.ask("K")[0] == "1", "resumed"


@case("the age in milliseconds: a ticket saved 999 ms into a second, offered 60 s later, is 60,000 ms old")
def age_in_milliseconds(c):
    h, _ = full(c, ms=999)
    r, on = offered(c, L.NOW + 60, h, ms=999)
    assert on, "offered"
    assert r.offer[1] == (60000 + 0x01020304) % 2**32, f"the obfuscated age: {r.offer[1]}"


@case("another host name: not offered, and the ticket is spent")
def other_host(c):
    h, _ = full(c)
    assert not offered(c, L.NOW + 60, h, OTHER_HOST)[1], "not offered to another name"
    assert not offered(c, L.NOW + 61, h)[1], "spent"


@case("the trust store changed: not offered")
def trust_changed(c):
    h, _ = full(c)
    c.ask(f"T {L.ROOTS.hex()}")
    assert not offered(c, L.NOW + 60, h)[1], "not offered after trust"


@case("after the leaf's notAfter: not offered")
def leaf_expired(c):
    c.ask("A 100000")
    h, _ = full(c, cert_der=short_lived())
    assert not offered(c, L.NOW + 3601, h)[1], "not offered after notAfter"


@case("just before the leaf's notAfter: offered")
def leaf_valid(c):
    c.ask("A 100000")
    h, _ = full(c, cert_der=short_lived())
    assert offered(c, L.NOW + 3599, h)[1], "offered"


@case("after the maximum age of a verification (60 s): not offered")
def too_old(c):
    c.ask("A 60")
    h, _ = full(c)
    assert not offered(c, L.NOW + 61, h)[1], "not offered"


@case("just inside the maximum age (60 s): offered")
def not_too_old(c):
    c.ask("A 60")
    h, _ = full(c)
    assert offered(c, L.NOW + 59, h)[1], "offered"


@case("after the ticket's lifetime (100 s): not offered")
def lifetime_over(c):
    h, _ = full(c, lifetime=100)
    assert not offered(c, L.NOW + 101, h)[1], "not offered"


@case("a clock before the ticket was received: not offered")
def clock_back(c):
    h, _ = full(c)
    assert not offered(c, L.NOW - 1, h)[1], "not offered"


@case("used once: the second start with a handle offers nothing")
def used_once(c):
    h, _ = full(c)
    assert offered(c, L.NOW + 10, h)[1], "offered the first time"
    assert not offered(c, L.NOW + 11, h)[1], "not the second"


@case("forgotten: not offered")
def forgotten(c):
    h, _ = full(c)
    c.ask(f"X {h}")
    assert not offered(c, L.NOW + 10, h)[1], "not offered"


@case("a full table (four tickets) replaces the oldest")
def table_full(c):
    first, _ = full(c)
    for _ in range(3):
        full(c)
    fifth, _ = full(c)
    assert not offered(c, L.NOW + 10, first)[1], "the oldest is gone"
    assert offered(c, L.NOW + 11, fifth)[1], "the newest is offered"


def which(c, now, pool):
    """The identity of the ticket a start with `pool` offers, or None."""
    r, on = offered(c, now, pool)
    return r.offer[0] if on else None


@case("a pool holds one ticket unless set: the newest replaces the one before")
def pool_of_one(c):
    p, _ = full(c, ticket=b"ticket one")
    assert full(c, ticket=b"ticket two", pool=p)[0] == p, "the same pool"
    assert which(c, L.NOW + 10, p) == b"ticket two", "the newest"
    assert which(c, L.NOW + 11, p) is None, "and no other"


@case("a pool of three: three connections resume, newest first, then none")
def pool_of_three(c):
    c.ask("P 3")
    p, _ = full(c, ticket=b"ticket one")
    full(c, ticket=b"ticket two", pool=p)
    full(c, ticket=b"ticket three", pool=p)
    got = [which(c, L.NOW + 10 + k, p) for k in range(4)]
    assert got == [b"ticket three", b"ticket two", b"ticket one", None], got


@case("a pool over its size loses its oldest")
def pool_over_size(c):
    c.ask("P 2")
    p, _ = full(c, ticket=b"ticket one")
    full(c, ticket=b"ticket two", pool=p)
    full(c, ticket=b"ticket three", pool=p)
    got = [which(c, L.NOW + 10 + k, p) for k in range(3)]
    assert got == [b"ticket three", b"ticket two", None], got


@case("forget empties a pool")
def pool_forgotten(c):
    c.ask("P 3")
    p, _ = full(c, ticket=b"ticket one")
    full(c, ticket=b"ticket two", pool=p)
    c.ask(f"X {p}")
    assert which(c, L.NOW + 10, p) is None, "nothing"


@case("a pool's refused ticket is overwritten, and an older good one is offered")
def pool_refused_newest(c):
    c.ask("P 2")
    p, _ = full(c, ticket=b"ticket one")
    full(c, ticket=b"ticket two", pool=p, lifetime=100)
    assert which(c, L.NOW + 150, p) == b"ticket one", "the newest is past its lifetime"
    assert which(c, L.NOW + 151, p) is None, "and was overwritten"


@case("a pool refilled after it was emptied")
def pool_refilled(c):
    p, _ = full(c, ticket=b"ticket one")
    assert which(c, L.NOW + 10, p) == b"ticket one"
    assert which(c, L.NOW + 11, p) is None, "empty"
    assert full(c, ticket=b"ticket two", pool=p)[0] == p, "the same pool"
    assert which(c, L.NOW + 12, p) == b"ticket two"


@case("a handle never issued is a new pool, and names nothing")
def pool_never_issued(c):
    p, _ = full(c, pool=999)
    assert p != 999, f"a new pool: {p}"
    assert which(c, L.NOW + 10, 999) is None, "999 names nothing"
    assert which(c, L.NOW + 11, p) == L.TICKET, "the new pool holds the ticket"


def main():
    exe, out = sys.argv[1], sys.argv[2]
    lines = ["# scripts/tls_tickets.py: the engine's rules for offering a ticket, one engine process a case.",
             "# `## <name>` starts a case; `=` lines are the engine's answers."]
    bad = 0
    for name, fn in CASES:
        c = L.Conversation(exe)
        try:
            assert c.ask(f"T {L.ROOTS.hex()}")[0] == "1", "one root"
            fn(c)
            print(f"{name}: ok")
        except (L.Failed, AssertionError) as e:
            print(f"FAILED {name}: {e}")
            bad += 1
        c.close()
        lines += [f"## {name}"] + c.lines
    open(out, "w").write("\n".join(lines) + "\n")
    print(f"{len(CASES)} cases, {bad} failed")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
