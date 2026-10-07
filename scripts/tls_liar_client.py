#!/usr/bin/env python3
"""A TLS 1.3 client that lies, against `packages/tls`'s server (docs/tls-server.md §8, step 2).

    python3 scripts/tls_liar_client.py <server driver> <out.txt>

`server driver` is `tests/programs/tls_server_driver.ls` built with `--std` and the package's files. The
client is written here on pyca/cryptography's primitives (X25519, P-256 and P-384 ECDH, ECDSA verification,
ChaCha20-Poly1305 and AES-GCM, HMAC-SHA-256 and -SHA-384), independently of the server; it knows only RFC 8446.
It is the shape of `scripts/tls_liar.py`, the lying server, turned round. Each case is one connection in which
the client changes one thing: one case per refusal tag of docs/tls-server.md §5.4, and one per rule of §5.2,
besides the honest ones (each suite and group, HelloRetryRequest to each group, SNI choosing an identity, ALPN,
early data skipped, a ClientHello in one-byte records and one of exactly 16 KiB, KeyUpdate, close_notify).

Everything is fixed: the server's DRBG seed, its two identities (P-256 keys from fixed seeds, certificates from
an Ed25519 CA, whose signatures are deterministic), the client's random and key shares. The server's signature
is RFC 6979's nonce hedged with its DRBG's bytes, so it too is the same each time, and a second recording is
identical byte for byte. The honest client checks everything the server sends: the ServerHello's echo and
choices, the chain, the CertificateVerify signature (with pyca, under the leaf's key), the Finished MAC, and the
application data under the keys the RFC says.

For a refusal, the server must end with the case's tag, failed (event 5), and must have sent the alert RFC 8446
§6.2 names: in plaintext before its ServerHello, under its handshake key or application key after (the server
writes under its application key once its Finished is queued). The configuration refusals (`add_identity`,
`set_alpn`, the role) answer their tag on the line itself. The file holds each case's driver lines and answers;
`crates/lex-sys/tests/conformance/tls_server.rs` replays every case and `scripts/tls_server_mutants.py` runs each
mutant against them. Exit status 1 on any difference.
"""
import datetime
import hashlib
import hmac
import subprocess
import sys

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519, x25519
from cryptography.hazmat.primitives.ciphers.aead import AESGCM, ChaCha20Poly1305
from cryptography.x509.oid import NameOID

HOST = b"liar.lex-sys.test"
OTHER = b"www.other.lex-sys.test"
SEED = bytes(range(32, 64))
HRR = bytes.fromhex("cf21ad74e59a6111be1d8c021e65b891c2a211167abb8c5e079e09e2c8a8339c")
SUITES = {0x1301: (AESGCM, 16, hashlib.sha256), 0x1302: (AESGCM, 32, hashlib.sha384),
          0x1303: (ChaCha20Poly1305, 32, hashlib.sha256)}
X25519, P256, P384 = 0x1D, 0x17, 0x18
P521, FFDHE2048, MLKEM = 0x19, 0x100, 0x11EC
CURVES = {P256: ec.SECP256R1(), P384: ec.SECP384R1()}
SHARE_LEN = {X25519: 32, P256: 65, P384: 97}
START = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)
END = START + datetime.timedelta(days=3650)
NOW_MS = 1780272000 * 1000  # 2026-06-01, inside every certificate's validity


def sha256(b):
    return hashlib.sha256(b).digest()


def seeded(name):
    return sha256(b"tls_liar_client " + name.encode())


def u16(n):
    return n.to_bytes(2, "big")


def u24(n):
    return n.to_bytes(3, "big")


def message(kind, body):
    return bytes([kind]) + u24(len(body)) + body


def ext(kind, body):
    return u16(kind) + u16(len(body)) + body


def plain_record(kind, content, version=0x0303):
    return bytes([kind]) + u16(version) + u16(len(content)) + content


def p256_key(name):
    n = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551
    return ec.derive_private_key(int.from_bytes(seeded(name), "big") % (n - 1) + 1, ec.SECP256R1())


CA_KEY = ed25519.Ed25519PrivateKey.from_private_bytes(seeded("ca"))
CA_NAME = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "tls_liar_client ca")])
CA = (x509.CertificateBuilder().subject_name(CA_NAME).issuer_name(CA_NAME).public_key(CA_KEY.public_key())
      .serial_number(1).not_valid_before(START).not_valid_after(END)
      .add_extension(x509.BasicConstraints(ca=True, path_length=None), True).sign(CA_KEY, None))


def leaf(key, names, not_after=END, serial=7):
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, names[0])])
    return (x509.CertificateBuilder().subject_name(subject).issuer_name(CA_NAME).public_key(key.public_key())
            .serial_number(serial).not_valid_before(START).not_valid_after(not_after)
            .add_extension(x509.SubjectAlternativeName([x509.DNSName(n) for n in names]), False)
            .sign(CA_KEY, None))


def pem(cert):
    return cert.public_bytes(serialization.Encoding.PEM)


def der(cert):
    return cert.public_bytes(serialization.Encoding.DER)


def key_pem(key, sec1=False):
    fmt = serialization.PrivateFormat.TraditionalOpenSSL if sec1 else serialization.PrivateFormat.PKCS8
    return key.private_bytes(serialization.Encoding.PEM, fmt, serialization.NoEncryption())


MAIN_KEY = p256_key("main")
MAIN = leaf(MAIN_KEY, [HOST.decode()])
OTHER_KEY = p256_key("other")
OTHER_CERT = leaf(OTHER_KEY, ["*.other.lex-sys.test"], serial=8)
CHAIN = pem(MAIN) + pem(CA)
OTHER_CHAIN = pem(OTHER_CERT) + pem(CA)


def expand_label(secret, label, context, n, hash=hashlib.sha256):
    full = b"tls13 " + label
    info = n.to_bytes(2, "big") + bytes([len(full)]) + full + bytes([len(context)]) + context
    out, t, i = b"", b"", 1
    while len(out) < n:
        t = hmac.new(secret, t + info + bytes([i]), hash).digest()
        out, i = out + t, i + 1
    return out[:n]


def derive(secret, label, transcript, hash=hashlib.sha256):
    return expand_label(secret, label, hash(transcript).digest(), hash().digest_size, hash)


class Keys:
    def __init__(self, secret, suite):
        self.secret, self.suite = secret, suite
        self.aead, size, self.hash = SUITES[suite]
        self.key = expand_label(secret, b"key", b"", size, self.hash)
        self.iv = expand_label(secret, b"iv", b"", 12, self.hash)
        self.seq = 0

    def nonce(self):
        return bytes(a ^ b for a, b in zip(self.iv, self.seq.to_bytes(12, "big")))

    def seal(self, kind, content, pad=0):
        inner = content + bytes([kind]) + bytes(pad)
        header = bytes([23, 3, 3]) + u16(len(inner) + 16)
        record = header + self.aead(self.key).encrypt(self.nonce(), inner, header)
        self.seq += 1
        return record

    def open(self, record):
        inner = self.aead(self.key).decrypt(self.nonce(), record[5:], record[:5])
        self.seq += 1
        inner = inner.rstrip(b"\0")
        return inner[-1], inner[:-1]

    def next(self):
        return Keys(expand_label(self.secret, b"traffic upd", b"", len(self.secret), self.hash), self.suite)


def records(data):
    out = []
    while data:
        n = 5 + int.from_bytes(data[3:5], "big")
        out.append(data[:n])
        data = data[n:]
    return out


class Failed(Exception):
    pass


class Conversation:
    """One connection: the driver's lines and answers, and what the server sent."""

    def __init__(self, driver):
        self.proc = subprocess.Popen([driver], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        self.lines, self.answers = [], []
        self.sent = b""  # what the server sent, not yet read
        self.received = b""  # application data the server received

    def ask(self, line):
        self.proc.stdin.write(line + "\n")
        self.proc.stdin.flush()
        answer = self.proc.stdout.readline().strip()
        if not answer:
            raise Failed(f"the driver ended after {line[:40]}")
        self.lines += [line, "= " + answer]
        self.answers.append(answer)
        f = answer.split(" ")
        if line[0] in "VFWQZ":
            if f[3] != "-":
                self.sent += bytes.fromhex(f[3])
            if f[4] != "-":
                self.received += bytes.fromhex(f[4])
        return f

    def feed(self, data):
        """Every byte, over as many lines as the server needs; the last answer."""
        while True:
            f = self.ask(f"F {data.hex() if data else '-'}")
            n = int(f[0])
            if n < 0 or n >= len(data):
                return f
            data = data[n:]

    def take(self):
        out, self.sent = self.sent, b""
        return records(out)

    def close(self):
        self.proc.stdin.close()
        self.proc.wait()


class Client:
    """The honest client, whose every step a case may replace."""

    def __init__(self, conv):
        self.c = conv
        self.random = seeded("random")
        self.sid = seeded("session id")
        # Not AES-128-GCM by default: whether the server prefers it depends on the CPU (docs/tls-server.md
        # §2.1), and a recording must replay on any. Case `suite_order` covers that choice.
        self.suites = [0x1303, 0x1302]
        self.groups = [X25519, P256, P384]
        self.share_groups = [X25519]
        self.host = HOST
        self.alpn = None
        self.early = False
        self.extra = b""  # extensions added after the usual ones
        self.versions = [0x0304, 0x0303]
        self.sigalgs = [0x0403, 0x0804, 0x0807]
        self.ccs = True  # compatibility mode: a change_cipher_spec before the second flight
        self.retried = False
        self.expect_suite = None
        self.expect_group = None
        self.expect_sni_ack = True
        self.expect_alpn = None
        self.expect_chain = [der(MAIN), der(CA)]
        self.expect_key = MAIN_KEY

    # ---- setting up the server ----
    def setup(self, alpn=None, identities=None, seed=SEED):
        f = self.c.ask(f"E {seed.hex()}")
        assert f[:2] == ["0", "ok"], f
        for chain, key, names in identities or [(CHAIN, key_pem(MAIN_KEY), HOST),
                                                (OTHER_CHAIN, key_pem(OTHER_KEY, sec1=True), b"*.other.lex-sys.test")]:
            f = self.c.ask(f"I {chain.hex()} {key.hex()} {names.hex()} {NOW_MS}")
            assert f[1] == "ok", f
        if alpn is not None:
            f = self.c.ask(f"A {alpn.hex()}")
            assert f[:2] == ["0", "ok"], f
        f = self.c.ask(f"V {NOW_MS}")
        assert f[:3] == ["0", "ok", "1"], f

    # ---- the ClientHello ----
    def ecdh_key(self, group):
        return ec.derive_private_key(int.from_bytes(seeded(f"ecdh {group}"), "big"), CURVES[group])

    def x25519_key(self):
        return x25519.X25519PrivateKey.from_private_bytes(seeded("x25519"))

    def public_share(self, group):
        if group == X25519:
            return self.x25519_key().public_key().public_bytes_raw()
        if group in CURVES:
            return self.ecdh_key(group).public_key().public_bytes(
                serialization.Encoding.X962, serialization.PublicFormat.UncompressedPoint)
        return seeded(f"share {group}") * 40  # a group this server does not have: any bytes

    def extensions(self):
        e = b""
        if self.host:
            name = b"\0" + u16(len(self.host)) + self.host
            e += ext(0, u16(len(name)) + name)
        e += ext(10, u16(2 * len(self.groups)) + b"".join(u16(g) for g in self.groups))
        e += ext(13, u16(2 * len(self.sigalgs)) + b"".join(u16(s) for s in self.sigalgs))
        if self.versions is not None:
            e += ext(43, bytes([2 * len(self.versions)]) + b"".join(u16(v) for v in self.versions))
        shares = b""
        for g in self.share_groups:
            share = self.public_share(g)[:SHARE_LEN.get(g, 1216)]
            shares += u16(g) + u16(len(share)) + share
        e += ext(51, u16(len(shares)) + shares)
        if self.alpn is not None:
            names = b"".join(bytes([len(n)]) + n for n in self.alpn)
            e += ext(16, u16(len(names)) + names)
        e += ext(45, b"\1\1")
        if self.early:
            e += ext(42, b"")
        return e + self.extra

    def client_hello(self, legacy=0x0303, compression=b"\0", body=None):
        if body is None:
            exts = self.extensions()
            suites = b"".join(u16(s) for s in self.suites)
            body = (u16(legacy) + self.random + bytes([len(self.sid)]) + self.sid + u16(len(suites)) + suites
                    + bytes([len(compression)]) + compression + u16(len(exts)) + exts)
        return message(1, body)

    def send_hello(self, hello=None, split=None):
        """The ClientHello in one record (or in records of `split` bytes); the transcript starts."""
        hello = hello or self.client_hello()
        self.transcript = (self.transcript if self.retried else b"") + hello
        if split:
            data = b"".join(plain_record(22, hello[i:i + split], 0x0301) for i in range(0, len(hello), split))
        else:
            data = plain_record(22, hello, 0x0301)
        return self.c.feed(data)

    # ---- the server's flight ----
    @property
    def hash(self):
        return SUITES[self.suite][2]

    def shared_secret(self, group, peer):
        if group == X25519:
            return self.x25519_key().exchange(x25519.X25519PublicKey.from_public_bytes(peer))
        point = ec.EllipticCurvePublicKey.from_encoded_point(CURVES[group], peer)
        return self.ecdh_key(group).exchange(ec.ECDH(), point)

    def parse_server_hello(self, msg):
        assert msg[0] == 2, f"a ServerHello: {msg[:4].hex()}"
        b = msg[4:]
        assert b[:2] == u16(0x0303)
        random = b[2:34]
        at = 34
        sid = b[at + 1:at + 1 + b[at]]
        assert sid == self.sid, "the session id echoed"
        at += 1 + b[at]
        suite = int.from_bytes(b[at:at + 2], "big")
        assert b[at + 2] == 0
        at += 3
        end = at + 2 + int.from_bytes(b[at:at + 2], "big")
        assert end == len(b)
        at += 2
        found = {}
        while at < end:
            kind, n = int.from_bytes(b[at:at + 2], "big"), int.from_bytes(b[at + 2:at + 4], "big")
            assert kind not in found, f"extension {kind} twice"
            found[kind] = b[at + 4:at + 4 + n]
            at += 4 + n
        assert set(found) == {43, 51}, f"extensions {sorted(found)}"
        assert found[43] == u16(0x0304)
        return random, suite, found[51]

    def retry(self, hello2=None):
        """Reads a HelloRetryRequest and answers with the second ClientHello."""
        (rec, *rest) = self.c.take()
        assert rec[:5] == bytes([22, 3, 3]) + rec[3:5]
        hrr = rec[5:]
        random, suite, ks = self.parse_server_hello(hrr)
        assert random == HRR, "a HelloRetryRequest"
        assert len(ks) == 2
        group = int.from_bytes(ks, "big")
        assert rest == [plain_record(20, b"\1")], "the server's change_cipher_spec after it"
        if self.expect_group is not None:
            assert group == self.expect_group, f"retry to {group:#x}"
        self.suite = suite
        if self.expect_suite is not None:
            assert suite == self.expect_suite, f"suite {suite:#x}"
        self.transcript = message(254, self.hash(self.transcript).digest()) + hrr
        self.retried = True
        self.share_groups = [group]
        self.early = False
        out = plain_record(20, b"\1") if self.ccs else b""
        self.ccs = False
        hello2 = hello2 or self.client_hello()
        self.transcript += hello2
        return self.c.feed(out + plain_record(22, hello2))

    def flight(self):
        """The server's flight, checked; the client's keys."""
        recs = self.c.take()
        sh = recs[0][5:]
        assert recs[0][0] == 22
        _, self.suite, ks = self.parse_server_hello(sh)
        if self.expect_suite is not None:
            assert self.suite == self.expect_suite, f"suite {self.suite:#x}, wanted {self.expect_suite:#x}"
        group = int.from_bytes(ks[:2], "big")
        if self.expect_group is not None:
            assert group == self.expect_group, f"group {group:#x}"
        assert int.from_bytes(ks[2:4], "big") == len(ks) - 4 == SHARE_LEN[group]
        self.transcript += sh
        h = self.hash
        n = h().digest_size
        shared = self.shared_secret(group, ks[4:])
        early = hmac.new(bytes(n), bytes(n), h).digest()
        hs = hmac.new(derive(early, b"derived", b"", h), shared, h).digest()
        self.c_hs = derive(hs, b"c hs traffic", self.transcript, h)
        self.s_hs = derive(hs, b"s hs traffic", self.transcript, h)
        self.master = hmac.new(derive(hs, b"derived", b"", h), bytes(n), h).digest()
        self.read = Keys(self.s_hs, self.suite)
        self.write = Keys(self.c_hs, self.suite)
        rest = recs[1:]
        if not self.retried:
            assert rest[0] == plain_record(20, b"\1"), "the server's change_cipher_spec"
            rest = rest[1:]
        data = b""
        for r in rest:
            kind, content = self.read.open(r)
            assert kind == 22
            data += content
        msgs = []
        while data:
            n = 4 + int.from_bytes(data[1:4], "big")
            msgs.append(data[:n])
            data = data[n:]
        assert [m[0] for m in msgs] == [8, 11, 15, 20], [m[0] for m in msgs]
        ee, cert, cv, fin = msgs
        want = b""
        if self.expect_sni_ack:
            want += ext(0, b"")
        if self.expect_alpn:
            want += ext(16, u16(1 + len(self.expect_alpn)) + bytes([len(self.expect_alpn)]) + self.expect_alpn)
        assert ee == message(8, u16(len(want)) + want), f"EncryptedExtensions {ee.hex()}"
        entries = b"".join(u24(len(c)) + c + u16(0) for c in self.expect_chain)
        assert cert == message(11, b"\0" + u24(len(entries)) + entries), "the chain as given"
        self.transcript += ee + cert
        assert cv[4:6] == u16(0x0403)
        sig = cv[8:]
        assert int.from_bytes(cv[6:8], "big") == len(sig) == len(cv) - 8
        content = b" " * 64 + b"TLS 1.3, server CertificateVerify\0" + h(self.transcript).digest()
        self.expect_key.public_key().verify(sig, content, ec.ECDSA(hashes.SHA256()))
        self.transcript += cv
        key = expand_label(self.s_hs, b"finished", b"", len(self.s_hs), h)
        assert fin == message(20, hmac.new(key, h(self.transcript).digest(), h).digest()), "the server's Finished"
        self.transcript += fin
        self.app_th = self.transcript

    def finished(self, mac=None):
        h = self.hash
        key = expand_label(self.c_hs, b"finished", b"", len(self.c_hs), h)
        return message(20, mac if mac is not None else hmac.new(key, h(self.transcript).digest(), h).digest())

    def finish(self, ccs=None, extra=b""):
        """The client's second flight: change_cipher_spec (in compatibility mode) and Finished."""
        out = plain_record(20, b"\1") if (self.ccs if ccs is None else ccs) else b""
        out += self.write.seal(22, self.finished()) + extra
        h = self.hash
        f = self.c.feed(out)
        self.read = Keys(derive(self.master, b"s ap traffic", self.app_th, h), self.suite)
        self.write = Keys(derive(self.master, b"c ap traffic", self.app_th, h), self.suite)
        return f

    def established(self):
        """The rest of an honest connection: data both ways, a KeyUpdate, close_notify both ways."""
        f = self.finish()
        assert f[2] == "3", f"established: {f[:3]}"
        f = self.c.feed(self.write.seal(23, b"GET / HTTP/1.0\r\n\r\n"))
        assert self.c.received.endswith(b"GET / HTTP/1.0\r\n\r\n"), "the request received"
        f = self.c.ask(f"W {b'HTTP/1.0 200 OK'.hex()}")
        (rec,) = self.c.take()
        assert self.read.open(rec) == (23, b"HTTP/1.0 200 OK"), "the response"
        # A KeyUpdate that asks for one back: the answer under the old key, then data under the new.
        f = self.c.feed(self.write.seal(22, message(24, b"\1")))
        self.write = self.write.next()
        (rec,) = self.c.take()
        assert self.read.open(rec) == (22, message(24, b"\0")), "the KeyUpdate answered"
        self.read = self.read.next()
        f = self.c.feed(self.write.seal(23, b"after the update", pad=10))
        assert self.c.received.endswith(b"after the update")
        f = self.c.ask(f"W {b'and back'.hex()}")
        (rec,) = self.c.take()
        assert self.read.open(rec) == (23, b"and back")
        f = self.c.feed(self.write.seal(21, b"\1\0"))
        f = self.c.ask("Q")
        assert f[2] == "4", f"closed: {f[:3]}"
        (rec,) = self.c.take()
        assert self.read.open(rec) == (21, b"\1\0"), "close_notify"
        n = self.c.ask("N")
        return n

    def honest(self, **setup):
        self.setup(**setup)
        self.send_hello()
        self.flight()
        return self.established()

    def expect_alert(self, description, keys=None):
        recs = self.c.take()
        assert len(recs) >= 1, "an alert was sent"
        last = recs[-1]
        if keys is not None:
            kind, content = keys.open(last)
            assert kind == 21, kind
        else:
            assert last[:5] == bytes([21, 3, 3, 0, 2]), last.hex()
            content = last[5:]
        assert content == bytes([2, description]), f"alert {content.hex()}, wanted 02{description:02x}"


# Each case: (name, the server's tag, the alert it must send, where: "plain", "hs" (the server's handshake
# key), "ap" (its application key), or None for a refusal on its line; the script).
CASES = []


def case(name, tag, alert=None, where="plain"):
    def register(fn):
        CASES.append((name, tag, alert, where, fn))
        return fn
    return register


def refused_hello(name, tag, alert, **kw):
    """A ClientHello the server refuses, built from the honest one with `kw` changed."""
    hello = kw.pop("hello", None)
    alpn = kw.pop("setup_alpn", None)

    def run(c):
        c.setup(alpn=alpn)
        for k, v in kw.items():
            setattr(c, k, v)
        c.send_hello(hello(c) if hello else None)
    CASES.append((name, tag, alert, "plain", run))


# ---- Honest ----

@case("honest: X25519, ChaCha20-Poly1305 offered first and AES-256-GCM, SNI and ALPN", "ok")
def honest(c):
    c.suites = [0x1303, 0x1302]
    c.expect_suite, c.expect_group = 0x1303, X25519
    c.alpn, c.expect_alpn = [b"h2", b"http/1.1"], b"http/1.1"
    n = c.honest(alpn=b"http/1.1 mqtt")
    assert n[3:5] == [HOST.hex(), b"http/1.1".hex()], f"server_name and alpn: {n}"


@case("the server's order of suites, with and without AES instructions", "ok", None, None)
def suite_order(c):
    f = c.c.ask("O")
    # Each mask of AES-128-GCM (1), AES-256-GCM (2), ChaCha20 (4): the suite chosen with, then without.
    want = []
    for mask in range(8):
        hw = 0x1301 if mask & 1 else 0x1303 if mask & 4 else 0x1302 if mask & 2 else 0
        sw = 0x1303 if mask & 4 else 0x1301 if mask & 1 else 0x1302 if mask & 2 else 0
        want += [str(hw), str(sw)]
    assert f == ["0", "ok"] + want, f


@case("honest: P-256 share only, AES-128-GCM", "ok")
def honest_p256(c):
    c.suites, c.share_groups = [0x1301], [P256]
    c.expect_suite, c.expect_group = 0x1301, P256
    c.honest()


@case("honest: P-384 share only, AES-256-GCM", "ok")
def honest_p384(c):
    c.suites, c.share_groups = [0x1302], [P384]
    c.expect_suite, c.expect_group = 0x1302, P384
    c.honest()


@case("honest: AES-256-GCM and AES-128-GCM offered, AES-128-GCM chosen", "ok")
def honest_aes_order(c):
    c.suites = [0x1302, 0x1301]
    c.expect_suite = 0x1301
    c.honest()


@case("honest: X25519 chosen over a P-256 share sent first", "ok")
def honest_group_order(c):
    c.share_groups = [P256, X25519]
    c.expect_group = X25519
    c.honest()


@case("honest: an X25519MLKEM768 share beside the X25519 one, the hybrid ignored", "ok")
def honest_hybrid(c):
    c.groups = [MLKEM, X25519, P256]
    c.share_groups = [MLKEM, X25519]
    c.expect_group = X25519
    c.honest()


def retry_case(group, groups, shares, suite):
    def run(c):
        c.suites = [suite]
        c.groups, c.share_groups = groups, shares
        c.expect_group, c.expect_suite = group, suite
        c.setup()
        c.send_hello()
        c.retry()
        c.flight()
        c.established()
    return run


case("honest: HelloRetryRequest to P-256 (a P-521 share), ChaCha20", "ok")(
    retry_case(P256, [P521, P256], [P521], 0x1303))
case("honest: HelloRetryRequest to P-384 (an ffdhe2048 share), AES-256-GCM", "ok")(
    retry_case(P384, [FFDHE2048, P384], [FFDHE2048], 0x1302))
case("honest: HelloRetryRequest to X25519 (only an X25519MLKEM768 share), AES-128-GCM", "ok")(
    retry_case(X25519, [MLKEM, X25519, P256], [MLKEM], 0x1301))


@case("honest: no share at all, HelloRetryRequest to X25519", "ok")
def honest_no_share(c):
    c.share_groups = []
    c.expect_group = X25519
    c.setup()
    c.send_hello()
    c.retry()
    c.flight()
    c.established()


@case("honest: SNI chooses the second identity by its wildcard, and a SEC 1 key", "ok")
def honest_sni(c):
    c.host = OTHER
    c.expect_chain, c.expect_key = [der(OTHER_CERT), der(CA)], OTHER_KEY
    n = c.honest()
    assert n[3] == OTHER.hex(), n


@case("honest: a name no identity has gets the default, and no server_name answered", "ok")
def honest_unknown_name(c):
    c.host = b"nobody.example"
    c.expect_sni_ack = False
    c.honest()


@case("honest: no server_name, the default identity", "ok")
def honest_no_sni(c):
    c.host = None
    c.expect_sni_ack = False
    n = c.honest()
    assert n[3] == "-", n


@case("honest: a name in capitals matches, and is kept lowercased", "ok")
def honest_upper(c):
    c.host = HOST.upper()
    n = c.honest()
    assert n[3] == HOST.hex(), n


@case("honest: ALPN offered, the server has no list: ignored", "ok")
def honest_alpn_ignored(c):
    c.alpn = [b"h2"]
    n = c.honest()
    assert n[4] == "-", n


@case("honest: ALPN, the server's order of preference wins", "ok")
def honest_alpn_order(c):
    c.alpn, c.expect_alpn = [b"http/1.1", b"mqtt", b"h2"], b"h2"
    n = c.honest(alpn=b"h2 http/1.1")
    assert n[4] == b"h2".hex(), n


@case("honest: early data offered and 2 records of it skipped, then the handshake", "ok")
def honest_early(c):
    c.early = True
    c.setup()
    c.send_hello()
    c.flight()
    junk = b"".join(plain_record(23, seeded(f"early {i}") * 20) for i in range(2))
    c.c.feed(plain_record(20, b"\1") + junk)
    c.ccs = False
    c.established()


@case("honest: early data offered, HelloRetryRequest, early data before the second ClientHello skipped", "ok")
def honest_early_retry(c):
    c.early = True
    c.share_groups = [P521]
    c.groups = [P521, P256]
    c.expect_group = P256
    c.setup()
    c.send_hello()
    c.c.feed(plain_record(23, seeded("early") * 10))
    c.retry()
    c.flight()
    c.established()


@case("honest: early data of exactly 16 KiB skipped", "ok")
def honest_early_limit(c):
    c.early = True
    c.setup()
    c.send_hello()
    c.flight()
    c.c.feed(plain_record(20, b"\1") + plain_record(23, bytes(8192)) + plain_record(23, bytes(8192)))
    c.ccs = False
    c.established()


@case("honest: the ClientHello in one-byte records", "ok")
def honest_split(c):
    c.setup()
    c.send_hello(split=1)
    c.flight()
    c.established()


@case("honest: a ClientHello of exactly 16 KiB (padding), in two records", "ok")
def honest_largest(c):
    c.setup()
    base = len(c.client_hello()) - 4
    c.extra = ext(21, bytes(16384 - base - 4))
    hello = c.client_hello()
    assert len(hello) == 4 + 16384
    c.send_hello(hello, split=16384)
    c.flight()
    c.established()


@case("honest: no change_cipher_spec from the client (not in compatibility mode), empty session id", "ok")
def honest_no_ccs(c):
    c.ccs = False
    c.sid = b""
    c.honest()


@case("honest: legacy_version 0x0303 in a TLS 1.0 record, unknown extensions and GREASE skipped", "ok")
def honest_grease(c):
    c.extra = ext(0x0A0A, b"") + ext(0xFF01, b"\0") + ext(23, b"") + ext(35, b"") + ext(5, b"\1\0\0\0\0")
    c.suites = [0x0A0A, 0x1303, 0xC02B]
    c.groups = [0x1A1A, X25519]
    c.honest()


@case("honest: pre_shared_key last, ignored: a full handshake", "ok")
def honest_psk_ignored(c):
    ident = u16(5) + b"ticks" + bytes(4)
    binders = u16(33) + bytes([32]) + bytes(32)
    c.extra = ext(41, u16(len(ident)) + ident + binders)
    c.honest()


# ---- The server's configuration (docs/tls-server.md §4) ----

def config_case(name, tag, line):
    def run(c):
        c.c.ask(f"E {SEED.hex()}")
        f = c.c.ask(line(c))
        assert f[1] == tag, f"{f[:2]}, wanted {tag}"
    CASES.append((name, tag, None, None, run))


# An encrypted PKCS#8 block: its content is never decoded, so fixed bytes stand for one (pyca's encryption
# draws a fresh salt, and the recording must be the same every time).
ENCRYPTED = (b"-----BEGIN ENCRYPTED PRIVATE KEY-----\n" + __import__("base64").encodebytes(seeded("encrypted") * 4)
             + b"-----END ENCRYPTED PRIVATE KEY-----\n")
P384_KEY = ec.derive_private_key(int.from_bytes(seeded("p384"), "big"), ec.SECP384R1())
P384_CERT = leaf(P384_KEY, [HOST.decode()], serial=9)
config_case("add_identity: a P-384 key and certificate", "tls-server-key-type",
            lambda c: f"I {(pem(P384_CERT) + pem(CA)).hex()} {key_pem(P384_KEY).hex()} {HOST.hex()} {NOW_MS}")
config_case("add_identity: a P-256 key with a P-384 leaf", "tls-server-key-type",
            lambda c: f"I {(pem(P384_CERT) + pem(CA)).hex()} {key_pem(MAIN_KEY).hex()} {HOST.hex()} {NOW_MS}")
config_case("add_identity: an Ed25519 key", "tls-server-key-type",
            lambda c: f"I {CHAIN.hex()} {CA_KEY.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()).hex()} {HOST.hex()} {NOW_MS}")
config_case("add_identity: a key that is not PEM", "tls-server-key-format",
            lambda c: f"I {CHAIN.hex()} {b'not a key'.hex()} {HOST.hex()} {NOW_MS}")
config_case("add_identity: an encrypted key", "tls-server-key-format",
            lambda c: f"I {CHAIN.hex()} {ENCRYPTED.hex()} {HOST.hex()} {NOW_MS}")
config_case("add_identity: the other identity's key", "tls-server-key-mismatch",
            lambda c: f"I {CHAIN.hex()} {key_pem(OTHER_KEY).hex()} {HOST.hex()} {NOW_MS}")
config_case("add_identity: the leaf expired at the time given", "tls-server-cert-expired",
            lambda c: f"I {(pem(leaf(MAIN_KEY, [HOST.decode()], START + datetime.timedelta(days=30), 10)) + pem(CA)).hex()} {key_pem(MAIN_KEY).hex()} {HOST.hex()} {NOW_MS}")
config_case("add_identity: no certificate in the chain", "tls-server-chain",
            lambda c: f"I {key_pem(MAIN_KEY).hex()} {key_pem(MAIN_KEY).hex()} {HOST.hex()} {NOW_MS}")
config_case("add_identity: a certificate block that is not base64", "tls-server-chain",
            lambda c: f"I {b'-----BEGIN CERTIFICATE-----\n!!!!\n-----END CERTIFICATE-----\n'.hex()} {key_pem(MAIN_KEY).hex()} {HOST.hex()} {NOW_MS}")
config_case("add_identity: a chain over 16 KiB", "tls-server-chain",
            lambda c: f"I {(pem(MAIN) + pem(CA) * 80).hex()} {key_pem(MAIN_KEY).hex()} {HOST.hex()} {NOW_MS}")
config_case("add_identity: names over 1 KiB", "tls-server-names",
            lambda c: f"I {CHAIN.hex()} {key_pem(MAIN_KEY).hex()} {(b'a.example ' * 103).hex()} {NOW_MS}")
config_case("serve with no identity", "tls-server-no-identity", lambda c: f"V {NOW_MS}")
config_case("replace_identity of one never added", "tls-server-no-identity",
            lambda c: f"R 3 {CHAIN.hex()} {key_pem(MAIN_KEY).hex()} {NOW_MS}")
config_case("set_alpn: a protocol name over 255 bytes", "tls-server-alpn-list", lambda c: f"A {(b'x' * 256).hex()}")
config_case("set_alpn: a list over 512 bytes", "tls-server-alpn-list", lambda c: f"A {(b'abcdefghi ' * 60).hex()}")


@case("add_identity: a 17th identity", "tls-server-identities-full", None, None)
def identities_full(c):
    c.c.ask(f"E {SEED.hex()}")
    for i in range(16):
        f = c.c.ask(f"I {CHAIN.hex()} {key_pem(MAIN_KEY).hex()} {f'h{i}.example'.encode().hex()} {NOW_MS}")
        assert f[:2] == [str(i), "ok"], f
    f = c.c.ask(f"I {CHAIN.hex()} {key_pem(MAIN_KEY).hex()} {HOST.hex()} {NOW_MS}")
    assert f[1] == "tls-server-identities-full", f


@case("the other role's calls on each engine", "tls-role", None, None)
def roles(c):
    f = c.c.ask("C")
    assert f == ["-57", "tls-role"] * 6, f


@case("replace_identity: a renewed chain and key are served, the names kept", "ok")
def replaced(c):
    c.setup()
    renewed = leaf(OTHER_KEY, [HOST.decode()], serial=11)
    f = c.c.ask(f"R 0 {(pem(renewed) + pem(CA)).hex()} {key_pem(OTHER_KEY).hex()} {NOW_MS}")
    assert f[:2] == ["0", "ok"], f
    c.expect_chain, c.expect_key = [der(renewed), der(CA)], OTHER_KEY
    c.send_hello()
    c.flight()
    c.established()


@case("replace_identity refused (the key is not the leaf's): the old identity is served", "ok")
def replace_refused(c):
    c.setup()
    f = c.c.ask(f"R 0 {CHAIN.hex()} {key_pem(OTHER_KEY).hex()} {NOW_MS}")
    assert f[1] == "tls-server-key-mismatch", f
    c.send_hello()
    c.flight()
    c.established()


# ---- The ClientHello's rules (docs/tls-server.md §5.2) ----

refused_hello("no supported_versions: a TLS 1.2 client", "tls-server-version", 70, versions=None)
refused_hello("an empty supported_versions", "tls-server-client-hello-format", 50, versions=[])
refused_hello("supported_versions without TLS 1.3", "tls-server-version", 70, versions=[0x0303, 0x0302])
refused_hello("legacy_version 0x0301", "tls-server-version", 70,
              hello=lambda c: c.client_hello(legacy=0x0301))
refused_hello("no extensions at all", "tls-server-version", 70,
              hello=lambda c: message(1, u16(0x0303) + c.random + b"\0" + u16(2) + u16(0x1301) + b"\1\0"))
refused_hello("compression methods deflate and null", "tls-server-illegal-parameter", 47,
              hello=lambda c: c.client_hello(compression=b"\1\0"))
refused_hello("no TLS 1.3 suite", "tls-server-suite", 40, suites=[0xC02B, 0xC02F, 0x00FF])
refused_hello("signature_algorithms without ecdsa_secp256r1_sha256", "tls-server-sigalg", 40,
              sigalgs=[0x0804, 0x0503, 0x0807])
refused_hello("no group of the server's, and no share of one", "tls-server-group", 40,
              groups=[P521, FFDHE2048], share_groups=[P521])
refused_hello("no signature_algorithms", "tls-server-missing-extension", 109,
              hello=lambda c: c.client_hello(body=strip_ext(c, 13)))
refused_hello("no supported_groups", "tls-server-missing-extension", 109,
              hello=lambda c: c.client_hello(body=strip_ext(c, 10)))
refused_hello("no key_share", "tls-server-missing-extension", 109,
              hello=lambda c: c.client_hello(body=strip_ext(c, 51)))
refused_hello("supported_groups twice", "tls-server-extension-repeat", 47,
              extra=ext(10, u16(2) + u16(X25519)))
refused_hello("an unknown extension twice", "tls-server-extension-repeat", 47,
              extra=ext(0x7777, b"") + ext(0x7777, b""))
refused_hello("pre_shared_key not last", "tls-server-illegal-parameter", 47,
              extra=ext(41, u16(9) + u16(3) + b"abc" + bytes(4) + u16(0)) + ext(0x7777, b""))
refused_hello("an X25519 share of 31 bytes", "tls-server-illegal-parameter", 47,
              hello=lambda c: c.client_hello(body=replace_ext(c, 51, u16(35) + u16(X25519) + u16(31) + bytes(31))))
refused_hello("two X25519 shares", "tls-server-illegal-parameter", 47,
              hello=lambda c: c.client_hello(body=replace_ext(
                  c, 51, u16(72) + (u16(X25519) + u16(32) + c.public_share(X25519)) * 2)))
refused_hello("a P-256 share, P-256 not in supported_groups", "tls-server-illegal-parameter", 47,
              groups=[X25519], share_groups=[P256])
refused_hello("two host names", "tls-server-illegal-parameter", 47,
              hello=lambda c: c.client_hello(body=replace_ext(
                  c, 0, u16(2 * (3 + len(HOST))) + (b"\0" + u16(len(HOST)) + HOST) * 2)))
refused_hello("ALPN offered, none the server's", "tls-server-alpn", 120, alpn=[b"spdy/3", b"h3"],
              setup_alpn=b"h2 http/1.1")
refused_hello("a session id of 33 bytes", "tls-server-client-hello-format", 50, sid=bytes(33))
refused_hello("an odd cipher_suites length", "tls-server-client-hello-format", 50,
              hello=lambda c: message(1, u16(0x0303) + c.random + b"\0" + u16(3) + bytes(3) + b"\1\0" + u16(0)))
refused_hello("the extensions' length one over", "tls-server-client-hello-format", 50,
              hello=lambda c: bump_ext_len(c))
refused_hello("a truncated ClientHello", "tls-server-client-hello-format", 50,
              hello=lambda c: message(1, c.client_hello()[4:40]))
refused_hello("an empty ALPN name", "tls-server-client-hello-format", 50,
              hello=lambda c: c.client_hello(body=with_ext(c, ext(16, u16(3) + b"\0\1x"))))
refused_hello("supported_groups of odd length", "tls-server-client-hello-format", 50,
              hello=lambda c: c.client_hello(body=replace_ext(c, 10, u16(3) + bytes(3))))
refused_hello("a key share running past its extension", "tls-server-client-hello-format", 50,
              hello=lambda c: c.client_hello(body=replace_ext(c, 51, u16(6) + u16(X25519) + u16(32) + bytes(2))))
refused_hello("early_data with content", "tls-server-client-hello-format", 50, extra=ext(42, b"\0\0\0\1"))


def strip_ext(c, kind):
    return rebuild(c, [e for e in split_exts(c.extensions()) if e[0] != kind])


def replace_ext(c, kind, body):
    return rebuild(c, [(k, body if k == kind else b) for k, b in split_exts(c.extensions())])


def with_ext(c, extra):
    return rebuild(c, split_exts(c.extensions() + extra))


def split_exts(e):
    out = []
    while e:
        k, n = int.from_bytes(e[:2], "big"), int.from_bytes(e[2:4], "big")
        out.append((k, e[4:4 + n]))
        e = e[4 + n:]
    return out


def rebuild(c, exts):
    e = b"".join(ext(k, b) for k, b in exts)
    suites = b"".join(u16(s) for s in c.suites)
    return (u16(0x0303) + c.random + bytes([len(c.sid)]) + c.sid + u16(len(suites)) + suites + b"\1\0"
            + u16(len(e)) + e)


def bump_ext_len(c):
    body = rebuild(c, split_exts(c.extensions()))
    e = len(c.extensions())
    at = len(body) - e - 2
    return message(1, body[:at] + u16(e + 1) + body[at + 2:])


@case("a ClientHello header saying 16 KiB and one byte", "tls-server-client-hello-length", 50)
def hello_too_long(c):
    c.setup()
    c.c.feed(plain_record(22, b"\1" + u24(16385) + bytes(100)))


@case("a ClientHello of 16 KiB and one byte, in two records", "tls-server-client-hello-length", 50)
def hello_one_over(c):
    c.setup()
    base = len(c.client_hello()) - 4
    c.extra = ext(21, bytes(16385 - base - 4))
    hello = c.client_hello()
    c.c.feed(plain_record(22, hello[:16000]) + plain_record(22, hello[16000:]))


# ---- The second ClientHello (RFC 8446 §4.1.2) ----

@case("after a retry, still no share of the group asked for", "tls-server-retry-share", 47)
def retry_without_share(c):
    c.share_groups = [P521]
    c.groups = [P521, P256]
    c.setup()
    c.send_hello()
    c.c.take()
    c.c.feed(plain_record(22, c.client_hello()))


def retry_changed(name, **kw):
    def run(c):
        c.share_groups = [P521]
        c.groups = [P521, P256]
        c.setup()
        c.send_hello()
        c.c.take()
        c.share_groups = [P256]
        for k, v in kw.items():
            setattr(c, k, v)
        c.c.feed(plain_record(22, c.client_hello()))
    CASES.append((name, "tls-server-retry-share", 47, "plain", run))


retry_changed("after a retry, another session id", sid=seeded("another"))
@case("after a retry, the suite it named no longer offered", "tls-server-retry-share", 47)
def retry_other_suite(c):
    c.share_groups = [P521]
    c.groups = [P521, P256]
    c.suites = [0x1303]
    c.setup()
    c.send_hello()
    c.c.take()
    c.share_groups, c.suites = [P256], [0x1301, 0x1302]
    c.c.feed(plain_record(22, c.client_hello()))
retry_changed("after a retry, early_data offered", early=True)


@case("a second HelloRetryRequest is never sent: a ClientHello after one with a share of another group",
      "tls-server-retry-share", 47)
def retry_other_group(c):
    c.share_groups = [P521]
    c.groups = [P521, P256, P384]
    c.setup()
    c.send_hello()
    c.c.take()
    c.share_groups = [P384]
    c.c.feed(plain_record(22, c.client_hello()))


# ---- Records and messages around the ClientHello ----

@case("a Finished first, instead of a ClientHello", "tls-unexpected-message", 10)
def finished_first(c):
    c.setup()
    c.c.feed(plain_record(22, message(20, bytes(32))))


@case("application data before the ClientHello", "tls-unexpected-message", 10)
def data_first(c):
    c.setup()
    c.c.feed(plain_record(23, b"hello"))


@case("change_cipher_spec before the ClientHello", "tls-unexpected-message", 10)
def ccs_first(c):
    c.setup()
    c.c.feed(plain_record(20, b"\1"))


@case("the ClientHello and more handshake bytes in one record", "tls-unexpected-message", 10)
def hello_and_more(c):
    c.setup()
    c.c.feed(plain_record(22, c.client_hello() + message(20, bytes(32))))


@case("two change_cipher_spec records", "tls-unexpected-message", 10, "ap")
def two_ccs(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.c.feed(plain_record(20, b"\1") + plain_record(20, b"\1"))
    c.read = Keys(derive(c.master, b"s ap traffic", c.app_th, c.hash), c.suite)


@case("a change_cipher_spec of 02", "tls-unexpected-message", 10, "ap")
def ccs_two(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.c.feed(plain_record(20, b"\2"))
    c.read = Keys(derive(c.master, b"s ap traffic", c.app_th, c.hash), c.suite)


@case("a record of version 2.0", "tls-protocol-version", 70)
def sslv2(c):
    c.setup()
    c.c.feed(plain_record(22, c.client_hello(), 0x0200))


@case("a plaintext record over 2^14 bytes", "tls-record-overflow", 22)
def overflow(c):
    c.setup()
    c.c.feed(plain_record(22, bytes(16385)))


@case("a fatal alert instead of a ClientHello", "tls-alert")
def alert_first(c):
    c.setup()
    c.c.feed(plain_record(21, b"\2\x28"))


@case("close_notify instead of a ClientHello", "tls-peer-closed")
def close_first(c):
    c.setup()
    c.c.feed(plain_record(21, b"\1\0"))


@case("the socket ends during the handshake", "tls-peer-closed")
def eof_mid(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.c.ask("Z")


@case("an X25519 share of a low-order point: the all-zero secret", "tls-key-share", 47)
def zero_share(c):
    c.setup()
    c.send_hello(c.client_hello(body=replace_ext(c, 51, u16(36) + u16(X25519) + u16(32) + bytes(32))))


@case("a P-256 share that is not on the curve", "tls-key-share", 47)
def off_curve(c):
    c.share_groups = [P256]
    c.setup()
    bad = b"\4" + bytes(31) + b"\1" + bytes(31) + b"\1"
    c.send_hello(c.client_hello(body=replace_ext(c, 51, u16(69) + u16(P256) + u16(65) + bad)))


# ---- The client's second flight ----

@case("a wrong Finished", "tls-server-finished", 51, "ap")
def bad_finished(c):
    c.setup()
    c.send_hello()
    c.flight()
    mac = bytearray(c.finished()[4:])
    mac[-1] ^= 1
    c.c.feed(plain_record(20, b"\1") + c.write.seal(22, c.finished(bytes(mac))))
    c.read = Keys(derive(c.master, b"s ap traffic", c.app_th, c.hash), c.suite)


@case("a Finished of the wrong length", "tls-decode-error", 50, "ap")
def short_finished(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.c.feed(c.write.seal(22, message(20, bytes(31))))
    c.read = Keys(derive(c.master, b"s ap traffic", c.app_th, c.hash), c.suite)


@case("a Finished record that does not authenticate", "tls-bad-record-mac", 20, "ap")
def bad_mac(c):
    c.setup()
    c.send_hello()
    c.flight()
    rec = bytearray(c.write.seal(22, c.finished()))
    rec[-1] ^= 1
    c.c.feed(bytes(rec))
    c.read = Keys(derive(c.master, b"s ap traffic", c.app_th, c.hash), c.suite)


@case("application data before the Finished", "tls-unexpected-message", 10, "ap")
def data_before_finished(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.c.feed(c.write.seal(23, b"early"))
    c.read = Keys(derive(c.master, b"s ap traffic", c.app_th, c.hash), c.suite)


@case("a Certificate the server never asked for", "tls-unexpected-message", 10, "ap")
def unasked_certificate(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.c.feed(c.write.seal(22, message(11, b"\0" + u24(0))))
    c.read = Keys(derive(c.master, b"s ap traffic", c.app_th, c.hash), c.suite)


@case("the Finished and a KeyUpdate in one record", "tls-unexpected-message", 10, "ap")
def finished_and_more(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.c.feed(c.write.seal(22, c.finished() + message(24, b"\0")))
    c.read = Keys(derive(c.master, b"s ap traffic", c.app_th, c.hash), c.suite)


@case("17 KiB of early data", "tls-server-early-data-size", 10, "ap")
def early_too_much(c):
    c.early = True
    c.setup()
    c.send_hello()
    c.flight()
    c.c.feed(plain_record(23, bytes(9000)) + plain_record(23, bytes(9000)))
    c.read = Keys(derive(c.master, b"s ap traffic", c.app_th, c.hash), c.suite)


@case("early data when none was offered", "tls-bad-record-mac", 20, "ap")
def early_not_offered(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.c.feed(plain_record(23, bytes(100)))
    c.read = Keys(derive(c.master, b"s ap traffic", c.app_th, c.hash), c.suite)


@case("the client's alert, in plaintext, refusing the server's certificate", "tls-alert")
def client_refuses(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.c.feed(plain_record(21, b"\2\x30"))


# ---- After the handshake ----

def connected(c):
    c.setup()
    c.send_hello()
    c.flight()
    f = c.finish()
    assert f[2] == "3", f


@case("33 KeyUpdates", "tls-too-many-messages", 80, "ap")
def key_updates(c):
    connected(c)
    for i in range(33):
        c.c.feed(c.write.seal(22, message(24, b"\0")))
        c.write = c.write.next()


@case("17 user_canceled warnings", "tls-too-many-messages", 80, "ap")
def warnings(c):
    connected(c)
    for i in range(17):
        c.c.feed(c.write.seal(21, b"\1\x5a"))


@case("a KeyUpdate of 2", "tls-decode-error", 50, "ap")
def bad_key_update(c):
    connected(c)
    c.c.feed(c.write.seal(22, message(24, b"\2")))


@case("a NewSessionTicket from the client", "tls-unexpected-message", 10, "ap")
def client_ticket(c):
    connected(c)
    c.c.feed(c.write.seal(22, message(4, bytes(13))))


@case("a ciphertext record over 2^14 + 256 bytes", "tls-record-overflow", 22, "ap")
def big_record(c):
    connected(c)
    c.c.feed(bytes([23, 3, 3]) + u16(16641) + bytes(16641))


@case("the socket ends without close_notify", "tls-peer-closed")
def truncated(c):
    connected(c)
    c.c.feed(c.write.seal(23, b"cut short"))
    c.c.ask("Z")


def run_case(driver, name, tag, alert, where, fn):
    conv = Conversation(driver)
    c = Client(conv)
    try:
        fn(c)
        if where is not None:
            last = conv.answers[-1].split(" ")
            if tag != "ok" or last[1] != "ok":
                if last[1] != tag:
                    raise Failed(f"ended {conv.answers[-1][:100]}, wanted {tag}")
            if tag != "ok":
                assert last[2] == "5", f"failed: {last[:3]}"
                if alert is not None:
                    keys = None if where == "plain" else c.read
                    c.expect_alert(alert, keys)
    except (Failed, AssertionError, Exception) as e:  # noqa: BLE001 -- reported
        conv.close()
        return False, f"{name}: {type(e).__name__} {e}", conv.lines
    conv.close()
    return True, f"{name}: {tag}", conv.lines


def main():
    driver, out = sys.argv[1], sys.argv[2]
    lines = ["# scripts/tls_liar_client.py: packages/tls's server against a client that lies, one case a connection.",
             "# `## <tag> <name>` starts a case; `=` lines are the server driver's answers."]
    bad = 0
    for name, tag, alert, where, fn in CASES:
        ok, what, recorded = run_case(driver, name, tag, alert, where, fn)
        print(("" if ok else "FAILED ") + what)
        bad += not ok
        lines += [f"## {tag} {name}"] + recorded
    open(out, "w").write("\n".join(lines) + "\n")
    print(f"{len(CASES)} cases, {bad} failed")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
