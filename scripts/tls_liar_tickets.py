"""The session-ticket cases of `scripts/tls_liar_client.py` (docs/tls-server.md §12.8).

Imported by it at its end, and registering its cases in its CASES: one case per rule of §12.3 and per
verdict and refusal of §12.5 and §12.7, besides the honest resumptions. A case is a first connection
that is sent tickets, then a second, on the same engine, that offers one of them, changed in the one
way the case is about. The lying client holds the ticket key (it supplies it, `K`), so it can also
seal tickets of its own in the format of §12.1, written here apart from the server's code: a ticket it
forges with a version of 2, or an `auth` of 1, reaches the rules a stolen ticket cannot.
"""
import datetime
import hashlib
import hmac
import sys

from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305

m = sys.modules.get("tls_liar_client") or sys.modules["__main__"]
Client, case, CASES = m.Client, m.case, m.CASES
HOST, OTHER, NOW_MS = m.HOST, m.OTHER, m.NOW_MS
X25519, P256, P384, P521 = m.X25519, m.P256, m.P384, m.P521
seeded, message, ext, u16, u24 = m.seeded, m.message, m.ext, m.u16, m.u24
plain_record, expand_label = m.plain_record, m.expand_label

LIFE = 3600
KEY_A, KEY_B, KEY_C, KEY_D, KEY_E = (seeded(f"ticket key {n}") for n in "abcde")


def name_of(key):
    return hmac.new(key, b"cancho tls ticket key name v1", hashlib.sha256).digest()[:16]


def hkdf_extract(salt, ikm):
    return hmac.new(salt, ikm, hashlib.sha256).digest()


def seal_ticket(key, plain, salt=bytes(range(32))):
    """A ticket in the format of §12.1, as the server would seal it: key name, salt, AEAD, tag."""
    prk = hkdf_extract(salt, key)
    subkey = hmac.new(prk, b"cancho tls ticket v1" + b"\1", hashlib.sha256).digest()
    aad = b"\1" + name_of(key) + salt
    return name_of(key) + salt + ChaCha20Poly1305(subkey).encrypt(bytes(12), plain, aad)


def forged_plain(psk, fp, sni=HOST, alpn=b"", suite=0x1303, issue=NOW_MS, life=LIFE, age_add=1234, version=1, auth=0):
    return (bytes([version, auth]) + issue.to_bytes(8, "big") + (issue + life * 1000).to_bytes(8, "big")
            + age_add.to_bytes(4, "big") + u16(suite) + bytes([len(psk)]) + psk + fp + bytes([len(sni)]) + sni
            + bytes([len(alpn)]) + alpn)


FP_MAIN = hashlib.sha256(m.der(m.MAIN)).digest()


def first(c, tickets=(2, LIFE), keys=None, **kw):
    """A full connection that is sent tickets; the tickets."""
    c.setup(tickets=tickets, keys=keys, **kw)
    c.send_hello()
    c.flight()
    c.established()
    assert len(c.tickets) == tickets[0], f"{len(c.tickets)} tickets"
    return c.tickets


def again(c, now=NOW_MS, before=()):
    """The slot dropped, the engine configured by `before`, a new connection served at `now`: a client for it."""
    f = c.c.ask("D")
    assert f[:2] == ["0", "ok"], f
    for line in before:
        f = c.c.ask(line)
        assert f[:2] == ["0", "ok"], (line, f)
    f = c.c.ask(f"V {now}")
    assert f[:3] == ["0", "ok", "1"], f
    return Client(c.c)


def offer(c, t, now=NOW_MS + 5000, age_ms=None, issued=NOW_MS, **over):
    """`c` offers ticket `t` at `now`, claiming the age `age_ms` (the true one since `issued` by default)."""
    age = (now - issued) if age_ms is None else age_ms
    ticket = over.pop("ticket", t["ticket"])
    psk = over.pop("psk", t["psk"])
    h = over.pop("hash", t["hash"])
    c.psks.append((ticket, psk, h, (age + t["age_add"]) % 2**32))
    for k, v in over.items():
        setattr(c, k, v)


def verdict(c, resumed, tag):
    f = c.c.ask("U")
    assert f[:3] == ["0", "ok", "1" if resumed else "0"], f"resumed {f[2]}, wanted {int(resumed)}: {f}"
    assert f[4] == tag, f"verdict {f[4]}, wanted {tag}"
    return f


def resume(c, t, tag="tls-ticket-resumed", now=NOW_MS + 5000, before=(), hrr=False, **over):
    """The whole second connection: serve, offer, handshake, established; checks the engine's verdict."""
    c2 = again(c, now, before)
    resumed = tag == "tls-ticket-resumed"
    c2.expect_resume = resumed
    offer(c2, t, now, **over)
    c2.send_hello()
    if hrr:
        c2.retry()
    c2.flight()
    verdict(c2, resumed, tag)
    c2.established()
    return c2


# ---- Honest resumptions ----

@case("tickets: a full handshake is sent 2 tickets; one resumes with a fresh key exchange, no Certificate", "ok")
def resumes(c):
    ts = first(c)
    assert [t["lifetime"] for t in ts] == [LIFE, LIFE] and len({t["ticket"] for t in ts}) == 2
    assert len({t["psk"] for t in ts}) == 2, "a PSK of its own for each ticket"
    c2 = resume(c, ts[1])
    assert len(c2.tickets) == 2, "a resumed connection is sent tickets too"
    # The new tickets resume in turn: a chain of resumptions.
    resume(c2, c2.tickets[0], now=NOW_MS + 9000, issued=NOW_MS + 5000)


@case("tickets: AES-256-GCM, SHA-384: a 48-byte PSK", "ok")
def resumes_sha384(c):
    c.suites = [0x1302]
    c.setup(tickets=(2, LIFE))
    c.send_hello()
    c.flight()
    c.established()
    assert c.tickets[0]["hash"] == 48
    c2 = again(c)
    c2.suites = [0x1302]
    c2.expect_resume = True
    offer(c2, c.tickets[0])
    c2.send_hello()
    c2.flight()
    verdict(c2, True, "tls-ticket-resumed")
    c2.established()


@case("tickets: ChaCha20-Poly1305 and a P-256 key exchange", "ok")
def resumes_p256(c):
    c.suites, c.share_groups = [0x1303], [P256]
    c.setup(tickets=(1, LIFE))
    c.send_hello()
    c.flight()
    c.established()
    c2 = again(c)
    c2.suites, c2.share_groups, c2.expect_group, c2.expect_resume = [0x1303], [P256], P256, True
    offer(c2, c.tickets[0])
    c2.send_hello()
    c2.flight()
    verdict(c2, True, "tls-ticket-resumed")
    c2.established()


@case("tickets: a HelloRetryRequest first: the binder is over the new transcript", "ok")
def resumes_after_retry(c):
    ts = first(c)
    c2 = again(c)
    c2.groups, c2.share_groups, c2.expect_group, c2.expect_resume = [P521, P384], [P521], P384, True
    offer(c2, ts[0])
    c2.send_hello()
    c2.retry()
    c2.flight()
    verdict(c2, True, "tls-ticket-resumed")
    c2.established()


@case("tickets: the second identity (SNI), ALPN agreed, resumes", "ok")
def resumes_other_identity(c):
    c.host, c.alpn = OTHER, [b"mqtt"]
    c.expect_chain, c.expect_key, c.expect_alpn = [m.der(m.OTHER_CERT), m.der(m.CA)], m.OTHER_KEY, b"mqtt"
    c.setup(alpn=b"http/1.1 mqtt", tickets=(1, LIFE))
    c.send_hello()
    c.flight()
    c.established()
    c2 = again(c)
    c2.host, c2.alpn, c2.expect_alpn, c2.expect_resume = OTHER, [b"mqtt"], b"mqtt", True
    offer(c2, c.tickets[0])
    c2.send_hello()
    c2.flight()
    verdict(c2, True, "tls-ticket-resumed")
    c2.established()


@case("tickets: the good ticket is the second of three identities: selected_identity 1", "ok")
def second_of_three(c):
    ts = first(c)
    c2 = again(c)
    c2.expect_resume = True
    offer(c2, ts[0], ticket=seeded("junk") * 4, psk=None)
    offer(c2, ts[1])
    offer(c2, ts[0], ticket=seeded("junk 2") * 4, psk=None)
    c2.send_hello()
    c2.flight()
    assert c2.sh_psk == 1, c2.sh_psk
    verdict(c2, True, "tls-ticket-resumed")
    c2.established()


@case("tickets: the good ticket is the fifth identity: only four are tried", "ok")
def fifth_of_five(c):
    ts = first(c)
    c2 = again(c)
    for i in range(4):
        offer(c2, ts[0], ticket=seeded(f"junk {i}") * 4, psk=None)
    offer(c2, ts[1])
    c2.send_hello()
    c2.flight()
    verdict(c2, False, "tls-ticket-unknown")
    c2.established()


@case("tickets: one ticket used three times, each resumes: a ticket is not single use on the server (§12.4)", "ok")
def reused(c):
    ts = first(c)
    for i in range(3):
        resume(c, ts[0], now=NOW_MS + 5000 + 1000 * i)


@case("tickets: early data offered with a good ticket: skipped, never accepted; the handshake completes", "ok")
def resumes_early(c):
    ts = first(c)
    c2 = again(c)
    c2.early, c2.expect_resume = True, True
    offer(c2, ts[0])
    c2.send_hello()
    c2.flight()
    junk = b"".join(plain_record(23, seeded(f"early {i}") * 20) for i in range(2))
    c2.c.feed(plain_record(20, b"\1") + junk)
    c2.ccs = False
    verdict(c2, True, "tls-ticket-resumed")
    c2.established()


@case("tickets: the ticket's lifetime is cut to the certificate's remaining life (900 s of 3600)", "ok")
def lifetime_capped(c):
    soon = datetime.datetime.fromtimestamp(NOW_MS // 1000 + 900, datetime.timezone.utc)
    cert = m.leaf(m.MAIN_KEY, [HOST.decode()], not_after=soon, serial=9)
    c.expect_chain = [m.der(cert), m.der(m.CA)]
    ts = first(c, tickets=(3, LIFE), identities=[(m.pem(cert) + m.pem(m.CA), m.key_pem(m.MAIN_KEY), HOST)])
    assert [t["lifetime"] for t in ts] == [900, 900, 900], [t["lifetime"] for t in ts]
    c2 = again(c, NOW_MS + 899_000)
    c2.expect_chain, c2.expect_resume = c.expect_chain, True
    offer(c2, ts[0], NOW_MS + 899_000)
    c2.send_hello()
    c2.flight()
    verdict(c2, True, "tls-ticket-resumed")
    c2.established()


@case("tickets: a ticket sealed by this script in §12.1's format, a key holder's, resumes", "ok")
def forged_resumes(c):
    c.setup(tickets=(1, LIFE), keys=[KEY_A])
    psk = seeded("forged psk")
    t = {"ticket": seal_ticket(KEY_A, forged_plain(psk, FP_MAIN)), "psk": psk, "hash": 32, "age_add": 1234}
    c2 = again(c, NOW_MS)
    c2.expect_resume = True
    offer(c2, t, NOW_MS, age_ms=0)
    c2.send_hello()
    c2.flight()
    verdict(c2, True, "tls-ticket-resumed")
    c2.established()


# ---- Rotation ----

@case("tickets: the previous key still opens: keys [B, A] after [A]", "ok")
def previous_key(c):
    ts = first(c, keys=[KEY_A])
    resume(c, ts[0], before=[f"K {(KEY_B + KEY_A).hex()} {NOW_MS + 1000}"])
    f = c.c.ask("U")
    assert f[5] == "2", f"two keys open tickets: {f}"


@case("tickets: a ticket from a key the program dropped: unknown, a full handshake", "ok")
def dropped_key(c):
    ts = first(c, keys=[KEY_A])
    resume(c, ts[0], "tls-ticket-unknown", before=[f"K {KEY_B.hex()} {NOW_MS + 1000}"])


@case("tickets: giving the engine the keys it holds changes nothing", "ok")
def keys_idempotent(c):
    ts = first(c, keys=[KEY_B, KEY_A])
    resume(c, ts[0], before=[f"K {(KEY_B + KEY_A).hex()} {NOW_MS + 2000}"])


@case("tickets: a key rotated out of the ring of four: unknown", "ok")
def ring_overflow(c):
    ts = first(c)
    resume(c, ts[0], "tls-ticket-unknown", now=NOW_MS + 5000,
           before=[f"X {NOW_MS + 100 * (i + 1)}" for i in range(4)])


@case("tickets: three rotations later the first key still opens", "ok")
def ring_holds(c):
    ts = first(c)
    resume(c, ts[0], now=NOW_MS + 5000, before=[f"X {NOW_MS + 100 * (i + 1)}" for i in range(3)])
    assert c.c.ask("U")[5] == "4"


@case("tickets: the engine's own rotation after exactly a lifetime: two keys open", "ok")
def auto_rotation(c):
    ts = first(c, tickets=(1, 600))
    c2 = again(c, NOW_MS + 600_000)
    f = c.c.ask("U")
    assert f[5] == "2", f"the old key and the new: {f}"
    c2.send_hello()
    c2.flight()
    c2.established()
    assert len(c2.tickets) == 1


@case("tickets: a previous key stops opening one lifetime after it was retired, though the ticket has not expired", "ok")
def previous_key_ends(c):
    ts = first(c, keys=[KEY_A])
    # One millisecond before the end it still opens; at the end it does not.
    c3 = again(c, NOW_MS + 61_000 - 1, before=["T 1 60", f"K {(KEY_B + KEY_A).hex()} {NOW_MS + 1000}"])
    offer(c3, ts[0], NOW_MS + 61_000 - 1)
    c3.expect_resume = True
    c3.send_hello()
    c3.flight()
    verdict(c3, True, "tls-ticket-resumed")
    c3.established()
    resume(c, ts[0], "tls-ticket-unknown", now=NOW_MS + 61_000)


@case("tickets: the key a rotation retired stops opening one lifetime later", "ok")
def rotated_key_ends(c):
    ts = first(c)
    resume(c, ts[0], "tls-ticket-unknown", now=NOW_MS + 61_000, before=["T 1 60", f"X {NOW_MS + 1000}"])


@case("tickets: keys the program supplied are never replaced by the engine's own, after a lifetime or two", "ok")
def supplied_keys_stay(c):
    first(c, keys=[KEY_A])
    again(c, NOW_MS + 2 * LIFE * 1000)
    f = c.c.ask("U")
    assert f[5] == "1", f"the one key the program gave: {f}"


@case("tickets: the count raised during a handshake: this connection is sent the tickets it was drawn randomness for", "ok")
def count_raised(c):
    c.setup(tickets=(1, LIFE))
    c.send_hello()
    c.flight()
    f = c.c.ask("T 4 3600")
    assert f[:2] == ["0", "ok"], f
    c.established()
    assert len(c.tickets) == 1, f"{len(c.tickets)} tickets"


@case("tickets: the clock moved during a long connection: a ticket is dated when it is made", "ok")
def clock_moves(c):
    c.setup(tickets=(1, LIFE))
    c.send_hello()
    c.flight()
    later = NOW_MS + 7200_000
    f = c.c.ask(f"M {later}")
    assert f[:2] == ["0", "ok"], f
    c.established()
    c2 = again(c, later + 5000)
    c2.expect_resume = True
    offer(c2, c.tickets[0], later + 5000, issued=later)
    c2.send_hello()
    c2.flight()
    verdict(c2, True, "tls-ticket-resumed")
    c2.established()


# ---- Fallbacks: a ticket the rules refuse is a full handshake, and the engine says why ----

@case("tickets: expired (served after its lifetime): a full handshake", "ok")
def expired(c):
    ts = first(c)
    resume(c, ts[0], "tls-ticket-expired", now=NOW_MS + LIFE * 1000 + 1)


@case("tickets: served at exactly its expiry: a full handshake", "ok")
def expired_exactly(c):
    ts = first(c)
    resume(c, ts[0], "tls-ticket-expired", now=NOW_MS + LIFE * 1000)


@case("tickets: served the last millisecond of its lifetime: resumes", "ok")
def not_yet_expired(c):
    ts = first(c)
    resume(c, ts[0], now=NOW_MS + LIFE * 1000 - 1)


@case("tickets: the age claimed 60 s too high: a full handshake", "ok")
def age_high(c):
    ts = first(c)
    resume(c, ts[0], "tls-ticket-age", age_ms=5000 + 60_000)


@case("tickets: the age claimed 31 s too low (the window is 30 s): a full handshake", "ok")
def age_low(c):
    ts = first(c)
    resume(c, ts[0], "tls-ticket-age", now=NOW_MS + 40_000, age_ms=9_000)


@case("tickets: the age claimed 29 s off: resumes", "ok")
def age_inside(c):
    ts = first(c)
    resume(c, ts[0], age_ms=5000 + 29_000)


@case("tickets: a ClientHello replayed within the window resumes; replayed after it, a full handshake", "ok")
def replay(c):
    ts = first(c)
    c2 = again(c, NOW_MS + 5000)
    c2.expect_resume = True
    offer(c2, ts[0], NOW_MS + 5000)
    hello = c2.client_hello()
    c2.send_hello(hello)
    c2.c.take()
    c3 = again(c, NOW_MS + 6000)
    c3.expect_resume = True
    offer(c3, ts[0], NOW_MS + 5000)
    c3.send_hello(hello)
    c3.flight()
    verdict(c3, True, "tls-ticket-resumed")
    c3.established()
    c4 = again(c, NOW_MS + 5000 + 60_000)
    c4.send_hello(hello)
    c4.flight()
    verdict(c4, False, "tls-ticket-age")
    c4.established()


@case("tickets: a ticket from a clock 60 s ahead: a full handshake", "ok")
def from_the_future(c):
    c.setup(tickets=(1, LIFE), keys=[KEY_A])
    psk = seeded("future psk")
    ahead = NOW_MS + 60_000
    t = {"ticket": seal_ticket(KEY_A, forged_plain(psk, FP_MAIN, issue=ahead)), "psk": psk, "hash": 32, "age_add": 1234}
    c2 = again(c, NOW_MS)
    offer(c2, t, NOW_MS, age_ms=0)
    c2.send_hello()
    c2.flight()
    verdict(c2, False, "tls-ticket-age")
    c2.established()


@case("tickets: another host name than the ticket's: a full handshake", "ok")
def other_name(c):
    ts = first(c)
    c2 = again(c)
    offer(c2, ts[0], NOW_MS + 5000)
    c2.host = OTHER
    c2.expect_chain, c2.expect_key = [m.der(m.OTHER_CERT), m.der(m.CA)], m.OTHER_KEY
    c2.send_hello()
    c2.flight()
    verdict(c2, False, "tls-ticket-name")
    c2.established()


@case("tickets: no host name where the ticket has one: a full handshake", "ok")
def no_name(c):
    ts = first(c)
    c2 = again(c)
    offer(c2, ts[0], NOW_MS + 5000)
    c2.host, c2.expect_sni_ack = None, False
    c2.send_hello()
    c2.flight()
    verdict(c2, False, "tls-ticket-name")
    c2.established()


@case("tickets: a ticket of one identity after its certificate was replaced: a full handshake", "ok")
def replaced_identity(c):
    ts = first(c)
    renewed = m.leaf(m.MAIN_KEY, [HOST.decode()], serial=70)
    c2 = again(c, NOW_MS + 5000)
    f = c.c.ask(f"R 0 {(m.pem(renewed) + m.pem(m.CA)).hex()} {m.key_pem(m.MAIN_KEY).hex()} {NOW_MS}")
    assert f[:2] == ["0", "ok"], f
    c2.expect_chain = [m.der(renewed), m.der(m.CA)]
    offer(c2, ts[0], NOW_MS + 5000)
    c2.send_hello()
    c2.flight()
    verdict(c2, False, "tls-ticket-identity")
    c2.established()


@case("tickets: the same certificate loaded again (a reload that found nothing new): still resumes", "ok")
def reloaded_same(c):
    ts = first(c)
    c2 = again(c, NOW_MS + 5000)
    f = c.c.ask(f"R 0 {m.CHAIN.hex()} {m.key_pem(m.MAIN_KEY).hex()} {NOW_MS}")
    assert f[:2] == ["0", "ok"], f
    c2.expect_resume = True
    offer(c2, ts[0], NOW_MS + 5000)
    c2.send_hello()
    c2.flight()
    verdict(c2, True, "tls-ticket-resumed")
    c2.established()


@case("tickets: a SHA-256 ticket offered where AES-256-GCM (SHA-384) is chosen: a full handshake", "ok")
def wrong_suite_hash(c):
    ts = first(c)
    c2 = again(c)
    c2.suites = [0x1302]
    offer(c2, ts[0], NOW_MS + 5000)
    c2.send_hello()
    c2.flight()
    verdict(c2, False, "tls-ticket-suite")
    c2.established()


@case("tickets: the ALPN protocol chosen is not the ticket's: a full handshake", "ok")
def other_alpn(c):
    c.alpn, c.expect_alpn = [b"http/1.1"], b"http/1.1"
    c.setup(alpn=b"http/1.1 mqtt", tickets=(1, LIFE))
    c.send_hello()
    c.flight()
    c.established()
    c2 = again(c)
    c2.alpn, c2.expect_alpn = [b"mqtt"], b"mqtt"
    offer(c2, c.tickets[0], NOW_MS + 5000)
    c2.send_hello()
    c2.flight()
    verdict(c2, False, "tls-ticket-alpn")
    c2.established()


def tampered(name, tag, edit):
    def run(c):
        ts = first(c)
        resume(c, ts[0], tag, ticket=edit(ts[0]["ticket"]))
    case(name, "ok")(run)


tampered("tickets: one bit of the sealed text changed: tampered, a full handshake", "tls-ticket-tampered",
         lambda t: t[:60] + bytes([t[60] ^ 1]) + t[61:])
tampered("tickets: one bit of the tag changed: tampered", "tls-ticket-tampered",
         lambda t: t[:-1] + bytes([t[-1] ^ 1]))
tampered("tickets: one bit of the salt changed: tampered", "tls-ticket-tampered",
         lambda t: t[:20] + bytes([t[20] ^ 1]) + t[21:])
tampered("tickets: truncated by 10 bytes: tampered", "tls-ticket-tampered", lambda t: t[:-10])
tampered("tickets: one bit of the key name changed: unknown", "tls-ticket-unknown",
         lambda t: bytes([t[0] ^ 1]) + t[1:])
tampered("tickets: cut to 40 bytes: unknown", "tls-ticket-unknown", lambda t: t[:40])
tampered("tickets: 15,000 bytes, larger than any ticket: unknown, not opened", "tls-ticket-unknown",
         lambda t: t + bytes(15000 - len(t)))
tampered("tickets: appended to by one byte: tampered", "tls-ticket-tampered", lambda t: t + b"\0")


def forged(name, tag, **kw):
    def run(c):
        c.setup(tickets=(1, LIFE), keys=[KEY_A])
        psk = seeded("forged psk")
        age_add = kw.get("age_add", 1234)
        t = {"ticket": seal_ticket(KEY_A, kw.pop("plain", None) or forged_plain(psk, kw.pop("fp", FP_MAIN), **kw)),
             "psk": psk, "hash": 32, "age_add": age_add}
        c2 = again(c, NOW_MS)
        offer(c2, t, NOW_MS, age_ms=0)
        c2.send_hello()
        c2.flight()
        verdict(c2, False, tag)
        c2.established()
    case(name, "ok")(run)


forged("tickets: a key holder's ticket of version 2: format", "tls-ticket-format", version=2)
forged("tickets: a ticket whose client authenticated (auth 1): refused until #384 defines it", "tls-ticket-auth", auth=1)
forged("tickets: a ticket whose plaintext has trailing bytes: format", "tls-ticket-format",
       plain=forged_plain(seeded("forged psk"), FP_MAIN) + b"\0")
forged("tickets: a ticket of a certificate no identity has (a made-up fingerprint): identity", "tls-ticket-identity",
       fp=bytes(32))
forged("tickets: a ticket with a 20-byte PSK: format", "tls-ticket-format",
       plain=forged_plain(bytes(20), FP_MAIN))


@case("tickets: only psk_ke offered: a full handshake, and no tickets are sent", "ok")
def psk_ke_only(c):
    ts = first(c)
    c2 = again(c)
    c2.modes = b"\1\0"
    offer(c2, ts[0], NOW_MS + 5000)
    c2.send_hello()
    c2.flight()
    verdict(c2, False, "tls-ticket-no-psk-dhe-ke")
    c2.established()
    assert c2.tickets == [], "no tickets to a client that cannot use psk_dhe_ke"


@case("tickets: a client that lists no psk_dhe_ke is sent no tickets on a full handshake", "ok")
def no_tickets_without_dhe(c):
    c.modes = b"\1\0"
    c.setup(tickets=(2, LIFE))
    c.send_hello()
    c.flight()
    c.established()
    assert c.tickets == []


@case("tickets: a client without psk_key_exchange_modes at all is sent none", "ok")
def no_modes_no_tickets(c):
    c.modes = None
    c.setup(tickets=(2, LIFE))
    c.send_hello()
    c.flight()
    c.established()
    assert c.tickets == []


@case("tickets: off by default: a ticket offered is ignored, verdict off, and none is sent", "ok")
def tickets_off(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.established()
    assert c.tickets == []
    c2 = again(c)
    offer(c2, {"ticket": seeded("t") * 4, "psk": None, "hash": 32, "age_add": 0}, NOW_MS)
    c2.send_hello()
    c2.flight()
    f = verdict(c2, False, "tls-ticket-off")
    assert f[5] == "0", f"the engine made no ticket key while tickets are off: {f}"
    c2.established()


@case("tickets: turned off after they were sent: the ticket is ignored", "ok")
def turned_off(c):
    ts = first(c)
    resume(c, ts[0], "tls-ticket-off", before=["T 0 3600"])


@case("tickets: a count of 9 and a lifetime of 0 and of 8 days are refused", "tls-server-ticket-config", None, None)
def config(c):
    c.c.ask(f"E {m.SEED.hex()}")
    for line in ("T 9 3600", "T 2 0", "T 2 691200"):
        f = c.c.ask(line)
        assert f[1] == "tls-server-ticket-config", f
    assert c.c.ask("T 8 604800")[:2] == ["0", "ok"]
    assert c.c.ask("T 0 1")[:2] == ["0", "ok"]


@case("tickets: keys of the wrong size or number are refused", "tls-server-ticket-key", None, None)
def key_config(c):
    c.c.ask(f"E {m.SEED.hex()}")
    for keys in ("-", "00" * 31, "00" * 33, "00" * 160):
        f = c.c.ask(f"K {keys} {NOW_MS}")
        assert f[1] == "tls-server-ticket-key", (keys[:8], f)
    assert c.c.ask(f"K {b''.join(seeded(f'k{i}') for i in range(4)).hex()} {NOW_MS}")[:2] == ["0", "ok"]


@case("tickets: rotating before the engine is seeded is refused", "tls-no-entropy", None, None)
def rotate_unseeded(c):
    f = c.c.ask(f"X {NOW_MS}")
    assert f[1] == "tls-no-entropy", f


# ---- Refusals: the connection ends ----

@case("tickets: a good ticket and a wrong binder: decrypt_error, not a full handshake", "tls-server-binder", 51)
def wrong_binder(c):
    ts = first(c)
    c2 = again(c)
    c2.binder_flip = 0
    offer(c2, ts[0], NOW_MS + 5000)
    c2.send_hello()


@case("tickets: a binder wrong in its last byte only: decrypt_error", "tls-server-binder", 51)
def binder_last_byte(c):
    ts = first(c)
    c2 = again(c)
    c2.binder_flip, c2.binder_flip_at = 0, -1
    offer(c2, ts[0], NOW_MS + 5000)
    c2.send_hello()


@case("tickets: a binder one byte too long: decrypt_error", "tls-server-binder", 51)
def long_binder(c):
    ts = first(c)
    c2 = again(c)
    c2.binder_flip, c2.binder_pad, c2.binder_flip_at = 0, 1, None
    offer(c2, ts[0], NOW_MS + 5000)
    c2.send_hello()


@case("tickets: after a HelloRetryRequest a binder that leaves it out of the transcript: decrypt_error",
      "tls-server-binder", 51, "plain")
def binder_without_retry(c):
    ts = first(c)
    c2 = again(c)
    c2.groups, c2.share_groups, c2.expect_group = [P521, P384], [P521], P384
    c2.binder_no_hrr = True
    offer(c2, ts[0], NOW_MS + 5000)
    c2.send_hello()
    c2.retry()


@case("tickets: the right ticket and a PSK from another connection: binder wrong", "tls-server-binder", 51)
def wrong_psk(c):
    ts = first(c)
    c2 = again(c)
    offer(c2, ts[0], NOW_MS + 5000, psk=ts[1]["psk"])
    c2.send_hello()


@case("tickets: a ticket sealed by a key holder with a wrong binder: decrypt_error too", "tls-server-binder", 51)
def forged_wrong_binder(c):
    c.setup(tickets=(1, LIFE), keys=[KEY_A])
    psk = seeded("forged psk")
    t = {"ticket": seal_ticket(KEY_A, forged_plain(psk, FP_MAIN)), "psk": psk, "hash": 32, "age_add": 1234}
    c2 = again(c, NOW_MS)
    c2.binder_flip = 0
    offer(c2, t, NOW_MS, age_ms=0)
    c2.send_hello()


@case("tickets: a wrong binder on a ticket that fails a rule is no abort: the ticket is ignored first", "ok")
def wrong_binder_expired(c):
    ts = first(c)
    resume(c, ts[0], "tls-ticket-expired", now=NOW_MS + LIFE * 1000 + 1, binder_flip=0)


@case("tickets: pre_shared_key without psk_key_exchange_modes: missing_extension", "tls-server-missing-extension", 109)
def psk_without_modes(c):
    ts = first(c)
    c2 = again(c)
    c2.modes = None
    offer(c2, ts[0], NOW_MS + 5000)
    c2.send_hello()


@case("tickets: psk_key_exchange_modes with no mode: decode_error", "tls-server-client-hello-format", 50)
def empty_modes(c):
    ts = first(c)
    c2 = again(c)
    c2.modes = b"\0"
    offer(c2, ts[0], NOW_MS + 5000)
    c2.send_hello()


@case("tickets: two identities and one binder: illegal_parameter", "tls-server-illegal-parameter", 47)
def binder_count(c):
    ts = first(c)
    c2 = again(c)
    offer(c2, ts[0], NOW_MS + 5000)
    offer(c2, ts[1], NOW_MS + 5000)
    c2.binders_keep = 1
    c2.send_hello()


@case("tickets: an identity longer than the list that holds it: decode_error", "tls-server-client-hello-format", 50)
def psk_overrun(c):
    ts = first(c)
    c2 = again(c)
    ident = u16(500) + bytes(10) + bytes(4)
    c2.extra = ext(41, u16(len(ident)) + ident + u16(33) + bytes([32]) + bytes(32))
    c2.send_hello()


@case("tickets: an empty identity: decode_error", "tls-server-client-hello-format", 50)
def psk_empty_identity(c):
    ts = first(c)
    c2 = again(c)
    ident = u16(0) + bytes(4)
    c2.extra = ext(41, u16(len(ident)) + ident + u16(33) + bytes([32]) + bytes(32))
    c2.send_hello()


@case("tickets: two identities, the first empty: decode_error", "tls-server-client-hello-format", 50)
def psk_empty_first(c):
    ts = first(c)
    c2 = again(c)
    ids = u16(0) + bytes(4) + u16(5) + b"ticks" + bytes(4)
    c2.extra = ext(41, u16(len(ids)) + ids + u16(66) + bytes([32]) + bytes(32) + bytes([32]) + bytes(32))
    c2.send_hello()


@case("tickets: two binders of 31 bytes: decode_error", "tls-server-client-hello-format", 50)
def psk_two_short_binders(c):
    ts = first(c)
    c2 = again(c)
    ids = u16(5) + b"ticks" + bytes(4) + u16(5) + b"tickt" + bytes(4)
    c2.extra = ext(41, u16(len(ids)) + ids + u16(64) + bytes([31]) + bytes(31) + bytes([31]) + bytes(31))
    c2.send_hello()


@case("tickets: a binder of 31 bytes: decode_error", "tls-server-client-hello-format", 50)
def psk_short_binder(c):
    ts = first(c)
    c2 = again(c)
    ident = u16(5) + b"ticks" + bytes(4)
    c2.extra = ext(41, u16(len(ident)) + ident + u16(32) + bytes([31]) + bytes(31))
    c2.send_hello()


@case("tickets: a resumed handshake and a wrong client Finished: decrypt_error", "tls-server-finished", 51, "ap")
def resumed_bad_finished(c):
    ts = first(c)
    c2 = again(c)
    c2.expect_resume = True
    offer(c2, ts[0], NOW_MS + 5000)
    c2.send_hello()
    c2.flight()
    mac = bytearray(c2.finished()[4:])
    mac[0] ^= 1
    c2.c.feed(c2.write.seal(22, c2.finished(bytes(mac))))
    c.read = m.Keys(m.derive(c2.master, b"s ap traffic", c2.app_th, c2.hash), c2.suite)
