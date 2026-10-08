#!/usr/bin/env python3
"""The engine's rules for client identities and ALPN (docs/tls-parity.md §6.2, §6.5, §6.6), recorded.

    python3 scripts/tls_tickets_auth.py <tickets> <out.txt>

`tickets` is `tests/programs/tls_tickets.cho` built as for `scripts/tls_tickets.py` (an engine opened with
`open_mutual_with_tickets`: one slot, room for four tickets, entropy fixed). The server is `scripts/tls_liar_auth.py`'s,
which asks for a certificate and checks what comes back. Each case is one engine process. The rules:
- an identity added, replaced or removed after a ticket was saved: the ticket is not offered (rule 9);
- an identity configured before the ticket was saved: it is offered and the server resumes, with `client_auth` 0;
- a ticket from a connection that sent a client certificate is not offered after that certificate's notAfter, and is
  just before (rule 10), though the server's own certificate lasts ten years;
- the identity is chosen by the host: a wildcard names it, another host's is not sent, the first match wins;
- the identities' own refusals through the engine (full, never added, key mismatch, expired);
- the ALPN offer: the engine's default is sent, a connection's own replaces it (even by none), a name the offer cannot hold
  is refused, and a ticket is not bound to the offer: it resumes under another and the new choice is reported.
The file holds each case's lines and answers; `crates/cancho/tests/conformance/tls_auth.rs` replays them on both backends.
Exit status 1 if any case fails.
"""
import datetime
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_liar as L  # noqa: E402
import tls_liar_auth as A  # noqa: E402
import tls_tickets as T  # noqa: E402

from cryptography.hazmat.primitives import serialization  # noqa: E402

HOUR = datetime.timedelta(hours=1)


class Eng(A.AuthServer):
    def begin(self, now, handle=0, host=L.HOST, alpn=None, ms=0):
        """A start (`B`) at `now` seconds, offering `alpn` (text) or nothing; the ticket offered, if any."""
        f = self.c.ask(f"B {host.hex()} {now * 1000 + ms} {handle} {alpn.encode().hex() if alpn else '-'}")
        assert f[0] == "0", f
        (hello,) = self.c.take()
        msg, self.sid, self.client_shares, _ = L.parse_client_hello(hello)
        self.transcript = msg
        self.ccs_sent = False
        A.check_offer(msg, alpn)
        return L.psk_offer(msg)

    def engine_auth(self):
        """`Y`: the client_auth the engine reports."""
        return int(self.c.ask("Y")[0])

    def engine_alpn(self):
        """`E`: the protocol the engine reports the server chose."""
        self.c.proc.stdin.write("E\n")
        self.c.proc.stdin.flush()
        answer = self.c.proc.stdout.readline().strip()
        self.c.lines += ["E", "= " + answer]
        f = answer.split(" ")
        return bytes.fromhex(f[3]) if f[3] != "-" else b""


def identity(c, cert=A.CERT, key=A.KEY, hosts=L.HOST, now=L.NOW):
    """`tls.add_client_identity`: the code and its tag."""
    f = c.ask(f"I {A.pem_cert(cert).hex()} {A.pem_key(key).hex()} {hosts.hex()} {now * 1000}")
    return int(f[0]), f[1]


def finish(s, c):
    """close_notify both ways, then `tls.save`: the handle."""
    s.c.feed(s.write.seal(21, b"\1\0"))
    c.ask("Q")
    c.take()
    return int(c.ask("V")[0])


def plain(c, now=L.NOW):
    """A full handshake that asks for nothing, a ticket, a save: the handle and the PSK."""
    s = Eng(c)
    assert s.begin(now) is None
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    psk = L.issue(s)
    return finish(s, c), psk


def mutual(c, now=L.NOW, expect="chain", alpn=None, host=L.HOST, cert=A.CERT):
    """A full handshake that asks for a certificate, a ticket, a save: the handle and the PSK."""
    s = Eng(c)
    s.client_cert = cert
    assert s.begin(now, 0, host, alpn) is None
    s.cr_exts = A.SIG
    s.c.feed(s.hello_and_flight())
    s.client_flight(expect)
    assert s.engine_auth() == (1 if expect == "chain" else 2), "client_auth"
    psk = L.issue(s)
    return finish(s, c), psk


def offers(c, now, handle, host=L.HOST, alpn=None):
    """A start with `handle`: the server, and whether the ticket was on the wire."""
    r = Eng(c)
    return r, r.begin(now, handle, host, alpn) is not None


def resumes(c, r, psk, ee=b""):
    """The server resumes the ticket `r` was offered: no Certificate, client_auth 0, resumed."""
    L.check_binder(r.transcript, psk)
    r.psk = psk
    r.ee_extensions = ee
    r.c.feed(L.resumed_flight(r))
    L.resumed_to_the_end(r)
    assert c.ask("K")[0] == "1", "resumed"
    assert r.engine_auth() == 0, "client_auth 0 on a resumed connection"


CASES = []


def case(name):
    def register(fn):
        CASES.append((name, fn))
        return fn
    return register


@case("an identity added after a ticket was saved: not offered; a ticket saved after it: offered, and resumed")
def added_after(c):
    h, _ = plain(c)
    assert identity(c)[0] == 0
    assert not offers(c, L.NOW + 10, h)[1], "not offered after an identity was added"
    h2, psk = mutual(c, L.NOW + 20)
    r, on = offers(c, L.NOW + 30, h2)
    assert on, "a ticket saved with the identity configured is offered"
    resumes(c, r, psk)


@case("an identity replaced after a ticket was saved: not offered")
def replaced_after(c):
    assert identity(c)[0] == 0
    h, _ = mutual(c)
    f = c.ask(f"J 0 {A.pem_cert(A.CERT).hex()} {A.pem_key(A.KEY).hex()} {L.NOW * 1000}")
    assert f[:2] == ["0", "ok"], f
    assert not offers(c, L.NOW + 10, h)[1], "not offered after a replacement"


@case("an identity removed after a ticket was saved: not offered, and the next connection sends the empty Certificate")
def removed_after(c):
    assert identity(c)[0] == 0
    h, _ = mutual(c)
    assert c.ask("Z 0")[:2] == ["0", "ok"]
    assert not offers(c, L.NOW + 10, h)[1], "not offered after a removal"
    mutual(c, L.NOW + 20, expect="empty")


@case("the client certificate's notAfter bounds the ticket: offered an instant before, not after, with the server's certificate valid for ten years")
def client_not_after(c):
    c.ask("A 100000")
    cert = A.client_certificate(A.KEY, L.CA, not_after=datetime.datetime.fromtimestamp(L.NOW, datetime.timezone.utc) + HOUR)
    assert identity(c, cert=cert)[0] == 0
    h, _ = mutual(c, cert=cert)
    assert offers(c, L.NOW + 3599, h)[1], "offered before the client certificate expires"
    h, _ = mutual(c, L.NOW + 3600 - 100, cert=cert)
    assert not offers(c, L.NOW + 3601, h)[1], "not offered after it"


@case("the identity is chosen by the host: a wildcard names it, another host's identity is not sent")
def by_host(c):
    other = A.client_key("second client key")
    other_cert = A.client_certificate(other, L.CA, name="tls_liar second client", serial=387)
    assert identity(c, hosts=b"someone.else.example")[0] == 0
    assert identity(c, cert=other_cert, key=other, hosts=b"*.lex-sys.test")[0] == 1
    s = Eng(c)
    s.client_pub, s.client_cert = other.public_key(), other_cert
    assert s.begin(L.NOW) is None
    s.cr_exts = A.SIG
    s.c.feed(s.hello_and_flight())
    # The chain on the wire is the second identity's, and its key signed: the first's, for another host, was not sent.
    s.client_flight("chain")


@case("the identities' own refusals through the engine: full, never added, expired, mismatched")
def refusals(c):
    for n in range(4):
        assert identity(c, hosts=f"h{n}.example".encode())[0] == n
    assert identity(c, hosts=b"fifth.example")[1] == "tls-client-identities-full"
    assert c.ask("Z 7")[1] == "tls-client-no-identity"
    assert c.ask(f"J 5 {A.pem_cert(A.CERT).hex()} {A.pem_key(A.KEY).hex()} {L.NOW * 1000}")[1] == "tls-client-no-identity"
    assert c.ask("Z 3")[:2] == ["0", "ok"]
    assert identity(c, key=A.OTHER_CLIENT_KEY)[1] == "tls-client-key-mismatch"
    assert identity(c, cert=A.EXPIRED)[1] == "tls-client-cert-expired"
    assert identity(c, hosts=b" ")[1] == "tls-client-names"


@case("ALPN: the default offer is sent; a connection's own replaces it, even by none; a bad offer is refused")
def alpn_offers(c):
    assert c.ask(f"L {b'h2 http/1.1'.hex()}")[:2] == ["0", "ok"]
    r = Eng(c)
    assert r.begin(L.NOW, 0, L.HOST, "h2 http/1.1") is None, "the default offer"
    assert c.ask(f"L {(b'x' * 256).hex()}")[1] == "tls-alpn-list"
    f = c.ask(f"B {L.HOST.hex()} {L.NOW * 1000} 0 {b'spdy/3'.hex()}")
    assert f[:2] == ["0", "ok"]
    (hello,) = c.take()
    A.check_offer(L.parse_client_hello(hello)[0], "spdy/3")
    f = c.ask(f"B {L.HOST.hex()} {L.NOW * 1000} 0 -")
    (hello,) = c.take()
    A.check_offer(L.parse_client_hello(hello)[0], None)
    assert c.ask(f"B {L.HOST.hex()} {L.NOW * 1000} 0 {(b'y' * 256).hex()}")[1] == "tls-alpn-list"


@case("ALPN: tls.start and tls.start_with offer the engine's default")
def alpn_default_used(c):
    assert c.ask(f"L {b'h2 http/1.1'.hex()}")[:2] == ["0", "ok"]
    assert c.ask(f"C {L.HOST.hex()} {L.NOW * 1000} 0")[:2] == ["0", "ok"]
    (hello,) = c.take()
    A.check_offer(L.parse_client_hello(hello)[0], "h2 http/1.1")
    assert c.ask(f"L {b'spdy/3'.hex()}")[:2] == ["0", "ok"]
    assert c.ask(f"C {L.HOST.hex()} {L.NOW * 1000} 0")[:2] == ["0", "ok"]
    (hello,) = c.take()
    A.check_offer(L.parse_client_hello(hello)[0], "spdy/3")


@case("ALPN: the server's choice is reported by the engine")
def alpn_reported(c):
    s = Eng(c)
    assert s.begin(L.NOW, 0, L.HOST, "h2 http/1.1") is None
    s.ee_extensions = A.alpn_ext(A.H1)
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    assert s.engine_alpn() == A.H1


@case("ALPN: a ticket is not bound to the offer: it resumes under another, and the new choice is reported")
def alpn_resumed(c):
    s = Eng(c)
    assert s.begin(L.NOW, 0, L.HOST, "h2") is None
    s.ee_extensions = A.alpn_ext(A.H2)
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    assert s.engine_alpn() == A.H2
    psk = L.issue(s)
    h = finish(s, c)
    r, on = offers(c, L.NOW + 10, h, alpn="http/1.1")
    assert on, "offered although the offer changed"
    resumes(c, r, psk, ee=A.alpn_ext(A.H1))
    assert r.engine_alpn() == A.H1, "the choice of the resumed connection"


def main():
    exe, out = sys.argv[1], sys.argv[2]
    lines = ["# scripts/tls_tickets_auth.py: the engine's rules for client identities and ALPN, one engine process a case.",
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
