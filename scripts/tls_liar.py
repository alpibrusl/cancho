#!/usr/bin/env python3
"""A TLS 1.3 server that lies, against `packages/tls` (docs/tls-core.md §6.3).

    python3 scripts/tls_liar.py <driver> <out.txt>
    python3 scripts/tls_liar.py --streams <tls_many> <out.txt>

`driver` is `tests/programs/tls_driver.ls` built with `--std` and the package's
files. The server is written here on pyca/cryptography's primitives (X25519,
P-256 and P-384 ECDH, ChaCha20-Poly1305 and AES-GCM, Ed25519, HMAC-SHA-256 and
-SHA-384), independently of the client; it knows only RFC 8446. It answers
with ChaCha20-Poly1305 and X25519 unless a case says otherwise; a case can pick
another suite, and a HelloRetryRequest to P-256 or P-384 first
(docs/tls-parity.md §3.3). Each case is one connection in which the server changes
one thing. Everything the server uses is fixed (its random, its X25519 key,
an Ed25519 certificate from a fixed seed, deterministic signatures), and the
driver's "randomness" is the bytes 00 to 5f, so a second recording is
identical byte for byte.

The honest case is not plain either: its flight is the RFC's hardest legal
shape. A change_cipher_spec; EncryptedExtensions and the start of
Certificate in one record; Certificate in three; CertificateVerify and
Finished in one, padded. Then a NewSessionTicket, a record of exactly
2^14 + 256 bytes, a KeyUpdate that asks for an answer, data under the new
key, a KeyUpdate that does not, and close_notify. The server checks the
client's Finished, decrypts what it sends under the keys the RFC says it
must use (so a KeyUpdate answered wrongly fails here), and checks the request.

For the others, the client must end with the case's tag, failed (event 5),
and must have sent the alert RFC 8446 §6.2 names: in plaintext during the
server's flight (the client's write key changes only after the server's
Finished, RFC 8446 Appendix A.1), under its application key after. The file holds each case's driver lines
and answers; `crates/lex-sys/tests/conformance/tls.rs` replays every case and
`scripts/tls_mutants.py` runs each mutant against them. Exit status 1 on any
difference.

With `--streams`, the honest server listens on a socket instead, and
`tests/programs/tls_many.ls` (built with `packages/tls/tls.ls`) makes 64
connections to it with the fixed seed 00 to 1f. Each connection's
ClientHello depends only on the seed and on how many connections started
before it, and everything the honest server sends depends only on the
ClientHello, so the file holds, per connection, its ClientHello, the
server's flight (as above; for the one ClientHello in eight whose SHA-256
starts with three zero bits, a chain of about 60 KB, nearly filling the
slot's handshake buffer), its reply once the client's Finished and
request have come (a padded response and close_notify), and the length
and SHA-256 of the response. `conformance/tls.rs` serves
the streams back, with no Python, to 64 concurrent connections of
`tls_many`.
"""
import datetime
import hashlib
import hmac
import subprocess
import sys

from cryptography import x509
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519, x25519
from cryptography.hazmat.primitives.ciphers.aead import AESGCM, ChaCha20Poly1305
from cryptography.x509.oid import NameOID

HOST = b"liar.lex-sys.test"
RANDOM = bytes(range(96))
HRR = bytes.fromhex("cf21ad74e59a6111be1d8c021e65b891c2a211167abb8c5e079e09e2c8a8339c")
REQUEST = b"GET / HTTP/1.0\r\n\r\n"


def sha256(b):
    return hashlib.sha256(b).digest()


# Each suite: its AEAD, key length and hash (RFC 8446 §B.4).
SUITES = {0x1301: (AESGCM, 16, hashlib.sha256), 0x1302: (AESGCM, 32, hashlib.sha384),
          0x1303: (ChaCha20Poly1305, 32, hashlib.sha256)}
X25519, P256, P384 = 0x1D, 0x17, 0x18
CURVES = {P256: ec.SECP256R1(), P384: ec.SECP384R1()}


def seeded(name):
    return sha256(b"tls_liar " + name.encode())


START = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)
END = START + datetime.timedelta(days=3650)
NOW = 1780272000  # 2026-06-01: the time the driver is given, inside every certificate's validity


def authority(seed_name):
    """A CA with an Ed25519 key from a fixed seed: its key and certificate."""
    key = ed25519.Ed25519PrivateKey.from_private_bytes(seeded(seed_name))
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, f"tls_liar {seed_name}")])
    cert = (x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(key.public_key())
            .serial_number(1).not_valid_before(START).not_valid_after(END)
            .add_extension(x509.BasicConstraints(ca=True, path_length=None), True)
            .sign(key, None))
    return key, cert


def certificate(seed_name, ca):
    """The server's Ed25519 key from a fixed seed, and its DER certificate from `ca`."""
    key = ed25519.Ed25519PrivateKey.from_private_bytes(seeded(seed_name))
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, HOST.decode())])
    cert = (x509.CertificateBuilder().subject_name(name).issuer_name(ca[1].subject).public_key(key.public_key())
            .serial_number(206).not_valid_before(START).not_valid_after(END)
            .add_extension(x509.SubjectAlternativeName([x509.DNSName(HOST.decode())]), False)
            .sign(ca[0], None))
    return key, cert.public_bytes(serialization.Encoding.DER)


CA = authority("ca")
OTHER_CA = authority("other ca")
KEY, DER = certificate("server", CA)
OTHER_KEY, OTHER_DER = certificate("other", OTHER_CA)
# The client's trust store: the one CA, as PEM.
ROOTS = CA[1].public_bytes(serialization.Encoding.PEM)


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


def u16(n):
    return n.to_bytes(2, "big")


def u24(n):
    return n.to_bytes(3, "big")


def message(kind, body):
    return bytes([kind]) + u24(len(body)) + body


def ext(kind, body):
    return u16(kind) + u16(len(body)) + body


def plain_record(kind, content):
    return bytes([kind, 3, 3]) + u16(len(content)) + content


class Keys:
    def __init__(self, secret, suite=0x1303):
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


def records_whole(data):
    """The whole records at the front of `data`."""
    out = []
    while len(data) >= 5 and len(data) >= 5 + int.from_bytes(data[3:5], "big"):
        n = 5 + int.from_bytes(data[3:5], "big")
        out.append(data[:n])
        data = data[n:]
    return out


class Failed(Exception):
    pass


class Conversation:
    """One connection: the driver's lines and answers, and the server's view."""

    def __init__(self, driver):
        self.proc = subprocess.Popen([driver], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        self.lines = []
        self.answers = []
        self.sent = b""  # what the client sent, all of it
        self.received = b""  # what the client received

    def ask(self, line):
        self.proc.stdin.write(line + "\n")
        self.proc.stdin.flush()
        answer = self.proc.stdout.readline().strip()
        if not answer:
            raise Failed(f"the driver ended after {line[:40]}")
        self.lines += [line, "= " + answer]
        self.answers.append(answer)
        f = answer.split(" ")
        if f[3] != "-":
            self.sent += bytes.fromhex(f[3])
        if f[4] != "-":
            self.received += bytes.fromhex(f[4])
        return f

    def feed(self, data):
        """Every byte, over as many lines as the client needs; the last answer."""
        while True:
            f = self.ask(f"F {data.hex()}")
            n = int(f[0])
            if n < 0 or n >= len(data):
                return f
            data = data[n:]

    def take(self):
        """What the client sent since the last call, as records."""
        out, self.sent = self.sent, b""
        return records(out)

    def close(self):
        self.proc.stdin.close()
        self.proc.wait()


def parse_client_hello(record):
    assert record[0] == 22, record[:5].hex()
    msg = record[5:]
    assert msg[0] == 1
    b = msg[4:]
    at = 2 + 32
    sid = b[at + 1:at + 1 + b[at]]
    at += 1 + b[at]
    at += 2 + int.from_bytes(b[at:at + 2], "big")
    at += 1 + b[at]
    end = at + 2 + int.from_bytes(b[at:at + 2], "big")
    at += 2
    shares, cookie = {}, None
    while at < end:
        kind, n = int.from_bytes(b[at:at + 2], "big"), int.from_bytes(b[at + 2:at + 4], "big")
        body = b[at + 4:at + 4 + n]
        if kind == 51:
            s = 2
            while s < len(body):
                group, m = int.from_bytes(body[s:s + 2], "big"), int.from_bytes(body[s + 2:s + 4], "big")
                shares[group] = body[s + 4:s + 4 + m]
                s += 4 + m
        if kind == 44:
            cookie = body[2:]
        at += 4 + n
    return msg, sid, shares, cookie


class Server:
    """The honest server, whose every step a case may replace."""

    def __init__(self, conv):
        self.c = conv
        self.transcript = b""
        self.x25519 = x25519.X25519PrivateKey.from_private_bytes(seeded("x25519"))
        self.random = seeded("random")
        self.suite = 0x1303
        self.group = X25519  # the group of the share answered, after any retry
        self.share_group = None  # the group the ServerHello's share claims, if a case lies about it
        self.version_ext = ext(43, u16(0x0304))
        self.share = None  # the server's public share, unless a case sets it
        self.extra_extensions = b""
        self.ee_extensions = b""
        self.cert_der = DER
        self.cv_key = KEY
        self.bad_finished = False
        self.filler = []  # more chain entries after the leaf, never parsed before #206
        self.resumed = False  # a PSK handshake (docs/tls-resumption.md): its early secret is from `psk`
        self.psk = None

    # ---- the flight ----
    @property
    def hash(self):
        return SUITES[self.suite][2]

    def ecdh_key(self, group):
        return ec.derive_private_key(int.from_bytes(seeded(f"ecdh {group}"), "big"), CURVES[group])

    def public_share(self):
        if self.group == X25519:
            return self.x25519.public_key().public_bytes_raw()
        return self.ecdh_key(self.group).public_key().public_bytes(
            serialization.Encoding.X962, serialization.PublicFormat.UncompressedPoint)

    def shared_secret(self):
        peer = self.client_shares[self.group]
        if self.group == X25519:
            return self.x25519.exchange(x25519.X25519PublicKey.from_public_bytes(peer))
        point = ec.EllipticCurvePublicKey.from_encoded_point(CURVES[self.group], peer)
        return self.ecdh_key(self.group).exchange(ec.ECDH(), point)

    def server_hello(self, sid):
        share = self.share if self.share is not None else self.public_share()
        group = self.share_group or self.group
        exts = self.version_ext + ext(51, u16(group) + u16(len(share)) + share) + self.extra_extensions
        body = u16(0x0303) + self.random + bytes([len(sid)]) + sid + u16(self.suite) + b"\0" + u16(len(exts)) + exts
        return message(2, body)

    def encrypted_extensions(self):
        return message(8, u16(len(self.ee_extensions)) + self.ee_extensions)

    def certificate(self):
        entries = b"".join(u24(len(c)) + c + u16(0) for c in [self.cert_der] + self.filler)
        return message(11, b"\0" + u24(len(entries)) + entries)

    def hello_retry(self, sid, group=None, cookie=None, suite=None, extra=b""):
        """A HelloRetryRequest naming `group` and carrying `cookie`, either or both."""
        exts = self.version_ext
        if group is not None:
            exts += ext(51, u16(group))
        if cookie is not None:
            exts += ext(44, u16(len(cookie)) + cookie)
        exts += extra
        body = u16(0x0303) + HRR + bytes([len(sid)]) + sid + u16(suite or self.suite) + b"\0" + u16(len(exts)) + exts
        return message(2, body)

    def retry(self, group, cookie=None, ccs=False):
        """A HelloRetryRequest for `group`, then the client's second ClientHello: the
        transcript restarts from message_hash (RFC 8446 §4.4.1), and the cookie must
        come back."""
        hrr = self.hello_retry(self.sid, group, cookie)
        self.c.feed(plain_record(22, hrr) + (plain_record(20, b"\1") if ccs else b""))
        (hello,) = self.c.take()
        msg, sid, self.client_shares, echoed = parse_client_hello(hello)
        assert sid == self.sid, "the same session id"
        assert echoed == cookie, f"the cookie echoed: {echoed!r}"
        assert set(self.client_shares) == {group}, f"one share, of the group asked for: {sorted(self.client_shares)}"
        first = self.hash(self.transcript).digest()
        self.transcript = message(254, first) + hrr + msg
        self.group = group
        self.ccs_sent = ccs

    def certificate_verify(self, transcript):
        content = b" " * 64 + b"TLS 1.3, server CertificateVerify\0" + self.hash(transcript).digest()
        sig = self.cv_key.sign(content)
        return message(15, u16(0x0807) + u16(len(sig)) + sig)

    def finished(self, transcript):
        key = expand_label(self.s_hs, b"finished", b"", len(self.s_hs), self.hash)
        mac = hmac.new(key, self.hash(transcript).digest(), self.hash).digest()
        if self.bad_finished:
            mac = bytes([mac[0] ^ 1]) + mac[1:]
        return message(20, mac)

    def start(self):
        f = self.c.ask(f"C {HOST.hex()} {RANDOM.hex()} {ROOTS.hex()} {NOW}")
        assert f[0] == "0", f
        (hello,) = self.c.take()
        msg, self.sid, self.client_shares, _ = parse_client_hello(hello)
        self.transcript = msg
        self.ccs_sent = False

    def keys(self, sh):
        self.transcript += sh
        h = self.hash
        n = h().digest_size
        shared = self.shared_secret()
        early = hmac.new(bytes(n), self.psk if self.resumed else bytes(n), h).digest()
        hs = hmac.new(derive(early, b"derived", b"", h), shared, h).digest()
        self.c_hs = derive(hs, b"c hs traffic", self.transcript, h)
        self.s_hs = derive(hs, b"s hs traffic", self.transcript, h)
        self.master = hmac.new(derive(hs, b"derived", b"", h), bytes(n), h).digest()
        self.write = Keys(self.s_hs, self.suite)
        self.read = Keys(self.c_hs, self.suite)

    def flight_messages(self):
        """EncryptedExtensions to Finished, each added to the transcript."""
        ee = self.encrypted_extensions()
        cert = self.certificate()
        self.transcript += ee + cert
        cv = self.certificate_verify(self.transcript)
        self.transcript += cv
        fin = self.finished(self.transcript)
        self.transcript += fin
        return ee, cert, cv, fin

    def hello_and_flight(self):
        """The honest flight, in the layout the module text describes."""
        sh = self.server_hello(self.sid)
        self.keys(sh)
        ee, cert, cv, fin = self.flight_messages()
        # Certificate in three records, or in as many as a long chain needs.
        k = max(3, -(-len(cert) // 16000))
        cuts = [len(cert) * i // k for i in range(k + 1)]
        out = plain_record(22, sh) + (b"" if self.ccs_sent else plain_record(20, b"\1"))
        for i in range(k):
            piece = cert[cuts[i]:cuts[i + 1]]
            out += self.write.seal(22, ee + piece if i == 0 else piece, pad=7 if i == 1 else 0)
        out += self.write.seal(22, cv + fin, pad=300)
        return out

    def check_client_finished(self):
        recs = self.c.take()
        assert recs[0] == plain_record(20, b"\1"), "a change_cipher_spec first"
        kind, fin = self.read.open(recs[1])
        h = self.hash
        key = expand_label(self.c_hs, b"finished", b"", len(self.c_hs), h)
        want = message(20, hmac.new(key, h(self.transcript).digest(), h).digest())
        assert (kind, fin) == (22, want), "the client's Finished"
        app_th = self.transcript
        self.transcript += fin
        self.write = Keys(derive(self.master, b"s ap traffic", app_th, h), self.suite)
        self.read = Keys(derive(self.master, b"c ap traffic", app_th, h), self.suite)
        # Anything after is for `expect_alert`.
        self.c.sent = b"".join(recs[2:])

    def expect_alert(self, description, encrypted):
        recs = self.c.take()
        assert len(recs) >= 1, "an alert was sent"
        last = recs[-1]
        if encrypted:
            kind, content = self.read.open(last)
            assert kind == 21, kind
        else:
            assert last[:5] == bytes([21, 3, 3, 0, 2]), last.hex()
            content = last[5:]
        assert content == bytes([2, description]), f"alert {content.hex()}, wanted 02{description:02x}"


# Each case: (name, the client's tag, the alert it must send or None, the server's script).
CASES = []


def case(name, tag, alert=None, encrypted=False):
    def register(fn):
        CASES.append((name, tag, alert, encrypted, fn, None))
        return fn
    return register


@case("honest, in the hardest legal shape", "ok")
def honest(s):
    s.start()
    honest_after_hello(s)


def honest_after_hello(s):
    """The honest connection from the server's flight on, under whatever suite and
    group the case set up."""
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    ticket = message(4, (7200).to_bytes(4, "big") + bytes(4) + b"\1\0" + u16(3) + b"abc" + u16(0))
    s.c.feed(s.write.seal(22, ticket))
    f = s.c.ask(f"W {REQUEST.hex()}")
    (req,) = s.c.take()
    assert s.read.open(req) == (23, REQUEST), "the request"
    # A record of exactly 2^14 + 256 bytes: 2^14 of data, its type, 239 of padding.
    big = bytes(i * 7 % 251 for i in range(16384))
    record = s.write.seal(23, big, pad=239)
    assert len(record) == 5 + 16384 + 256
    s.c.feed(record)
    # KeyUpdate, update_requested: the client answers under its old key,
    # then writes under the next.
    s.c.feed(s.write.seal(22, message(24, b"\1")))
    s.write = s.write.next()
    (answer,) = s.c.take()
    assert s.read.open(answer) == (22, message(24, b"\0")), "the client's KeyUpdate answer"
    s.read = s.read.next()
    s.c.feed(s.write.seal(23, b"after one update", pad=1))
    s.c.ask(f"W {b'again'.hex()}")
    (again,) = s.c.take()
    assert s.read.open(again) == (23, b"again"), "data under the client's next key"
    # KeyUpdate, update_not_requested: no answer.
    s.c.feed(s.write.seal(22, message(24, b"\0")))
    s.write = s.write.next()
    assert s.c.take() == [], "no answer to update_not_requested"
    f = s.c.feed(s.write.seal(23, b"after two") + s.write.seal(21, b"\1\0"))
    assert f[2] == "4", f"closed: {f}"
    assert s.c.received == big + b"after one update" + b"after two", "what the client received"


@case("user_canceled, then close_notify", "ok")
def user_canceled(s):
    s.start()
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    f = s.c.feed(s.write.seal(21, b"\1\x5a") + s.write.seal(21, b"\1\0"))
    assert f[2] == "4", f


def flight_case(change):
    """The honest flight, with `change` applied to the server first."""
    def run(s):
        s.start()
        change(s)
        s.c.feed(s.hello_and_flight())
    return run


def hello(s, **kw):
    s.start()
    for k, v in kw.items():
        setattr(s, k, v)
    s.c.feed(plain_record(22, s.server_hello(s.sid)))


case("EncryptedExtensions before ServerHello", "tls-unexpected-message", 10)(
    lambda s: (s.start(), s.c.feed(plain_record(22, s.encrypted_extensions()))))


@case("Certificate before EncryptedExtensions", "tls-unexpected-message", 10)
def cert_first(s):
    s.start()
    sh = s.server_hello(s.sid)
    s.keys(sh)
    s.c.feed(plain_record(22, sh) + s.write.seal(22, s.certificate()))


@case("Finished before CertificateVerify", "tls-unexpected-message", 10)
def finished_first(s):
    s.start()
    sh = s.server_hello(s.sid)
    s.keys(sh)
    ee, cert = s.encrypted_extensions(), s.certificate()
    s.transcript += ee + cert
    s.c.feed(plain_record(22, sh) + s.write.seal(22, ee + cert + s.finished(s.transcript)))


@case("application data during the handshake", "tls-unexpected-message", 10)
def early_data(s):
    s.start()
    sh = s.server_hello(s.sid)
    s.keys(sh)
    s.c.feed(plain_record(22, sh) + s.write.seal(22, s.encrypted_extensions()) + s.write.seal(23, b"too soon"))


# The client has sent its Finished, under its new key, when it finds the
# ticket: the alert follows under its application key.
@case("a NewSessionTicket in Finished's record", "tls-unexpected-message", 10, True)
def ticket_with_finished(s):
    s.start()
    sh = s.server_hello(s.sid)
    s.keys(sh)
    ee, cert, cv, fin = s.flight_messages()
    ticket = message(4, bytes(8) + b"\1\0" + u16(3) + b"abc" + u16(0))
    s.c.feed(plain_record(22, sh) + s.write.seal(22, ee + cert + cv + fin + ticket))
    s.check_client_finished()


case("ALPN in EncryptedExtensions, never offered", "tls-unsupported-extension", 110)(
    flight_case(lambda s: setattr(s, "ee_extensions", ext(16, u16(3) + b"\2h2"))))
case("early_data in EncryptedExtensions, never offered", "tls-unsupported-extension", 110)(
    flight_case(lambda s: setattr(s, "ee_extensions", ext(42, b""))))
case("no supported_versions, with a TLS 1.3 suite", "tls-no-shared-cipher", 40)(lambda s: hello(s, version_ext=b""))
case("supported_versions TLS 1.2", "tls-protocol-version", 70)(lambda s: hello(s, version_ext=ext(43, u16(0x0303))))


def sentinel_in_tls13(tail):
    """A TLS 1.3 ServerHello whose random ends in a downgrade sentinel: RFC
    8446 §4.1.3 has the client check for one only in a ServerHello for TLS
    1.2 or below, so here it is random bytes, and the connection goes on
    (docs/tls-assurance.md §4: OpenSSL takes it too)."""
    def run(s):
        s.start()
        s.random = bytes(24) + tail
        honest_after_hello(s)
    return run


case("the TLS 1.2 downgrade sentinel in a TLS 1.3 ServerHello", "ok")(sentinel_in_tls13(b"DOWNGRD\1"))
case("the TLS 1.1 downgrade sentinel in a TLS 1.3 ServerHello", "ok")(sentinel_in_tls13(b"DOWNGRD\0"))
case("TLS_AES_128_CCM_SHA256, not offered", "tls-no-shared-cipher", 40)(lambda s: hello(s, suite=0x1304))
case("server_name in a TLS 1.3 ServerHello, not EncryptedExtensions", "tls-unsupported-extension", 110)(
    lambda s: hello(s, extra_extensions=ext(0, b"")))
case("a P-256 share, never sent", "tls-key-share", 47)(lambda s: hello(s, group=P256))


# ---- Other suites, and HelloRetryRequest (docs/tls-parity.md §3.3) ----

@case("honest, AES-256-GCM-SHA384", "ok")
def honest_aes256(s):
    s.start()
    s.suite = 0x1302
    honest_after_hello(s)


@case("honest, a HelloRetryRequest to P-256 with a cookie and a change_cipher_spec, AES-128-GCM", "ok")
def honest_retry_p256(s):
    s.start()
    s.suite = 0x1301
    s.retry(P256, cookie=b"a stateless server's cookie " * 4, ccs=True)
    honest_after_hello(s)


@case("honest, a HelloRetryRequest to P-384, AES-256-GCM-SHA384", "ok")
def honest_retry_p384(s):
    s.start()
    s.suite = 0x1302
    s.retry(P384)
    honest_after_hello(s)


def retry_hello(group=None, cookie=None, suite=None):
    def run(s):
        s.start()
        s.c.feed(plain_record(22, s.hello_retry(s.sid, group, cookie, suite)))
    return run


case("a HelloRetryRequest for X25519, whose share was sent", "tls-key-share", 47)(retry_hello(X25519))
case("a HelloRetryRequest for X448, never offered", "tls-key-share", 47)(retry_hello(0x1E))
case("a HelloRetryRequest that changes nothing", "tls-hello-retry", 47)(retry_hello())
case("a HelloRetryRequest with a suite not offered", "tls-no-shared-cipher", 40)(retry_hello(P256, suite=0x1304))
case("a HelloRetryRequest with a cookie over 2,048 bytes", "tls-hello-retry", 47)(retry_hello(P256, b"c" * 2049))


@case("a second HelloRetryRequest", "tls-unexpected-message", 10)
def second_retry(s):
    s.start()
    s.retry(P256)
    s.c.feed(plain_record(22, s.hello_retry(s.sid, P384)))


@case("after a retry, a ServerHello with another suite", "tls-hello-retry", 47)
def retry_then_suite(s):
    s.start()
    s.suite = 0x1301
    s.retry(P256)
    s.suite = 0x1303
    s.c.feed(plain_record(22, s.server_hello(s.sid)))


@case("after a retry to P-256, a ServerHello with an X25519 share", "tls-key-share", 47)
def retry_then_x25519(s):
    s.start()
    s.retry(P256)
    s.group = X25519
    s.c.feed(plain_record(22, s.server_hello(s.sid)))


@case("after a retry to P-256, a point not on the curve", "tls-key-share", 47)
def retry_then_off_curve(s):
    s.start()
    s.retry(P256)
    s.share = b"\4" + bytes(64)
    s.c.feed(plain_record(22, s.server_hello(s.sid)))
case("an all-zero X25519 share", "tls-key-share", 47)(lambda s: hello(s, share=bytes(32)))
case("a low-order X25519 share", "tls-key-share", 47)(lambda s: hello(s, share=b"\1" + bytes(31)))


@case("an encrypted record over 2^14 + 256", "tls-record-overflow", 22)
def overflow(s):
    s.start()
    sh = s.server_hello(s.sid)
    s.keys(sh)
    s.c.feed(plain_record(22, sh) + bytes([23, 3, 3]) + u16(16384 + 257) + bytes(16))


@case("an inner plaintext over 2^14", "tls-record-overflow", 22, True)
def inner_overflow(s):
    s.start()
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    s.c.feed(s.write.seal(23, bytes(16385)))


@case("a Certificate over 64 KiB", "tls-record-overflow", 22)
def long_certificate(s):
    s.start()
    sh = s.server_hello(s.sid)
    s.keys(sh)
    s.c.feed(plain_record(22, sh) + s.write.seal(22, s.encrypted_extensions() + b"\x0b" + u24(65533)))


@case("one bit flipped in an encrypted record", "tls-bad-record-mac", 20)
def flipped(s):
    s.start()
    data = bytearray(s.hello_and_flight())
    data[-20] ^= 0x10
    s.c.feed(bytes(data))


@case("records out of order", "tls-bad-record-mac", 20)
def reordered(s):
    s.start()
    sh = s.server_hello(s.sid)
    s.keys(sh)
    ee, cert, cv, fin = s.flight_messages()
    first, second = s.write.seal(22, ee), s.write.seal(22, cert)
    s.c.feed(plain_record(22, sh) + second + first)


case("CertificateVerify signed with another key", "tls-bad-certificate-verify", 51)(
    flight_case(lambda s: setattr(s, "cv_key", OTHER_KEY)))
case("a wrong Finished", "tls-bad-finished", 51)(flight_case(lambda s: setattr(s, "bad_finished", True)))
case("a certificate from an untrusted CA", "x509-unknown-issuer", 48)(
    flight_case(lambda s: (setattr(s, "cert_der", OTHER_DER), setattr(s, "cv_key", OTHER_KEY))))
case("a fatal alert instead of ServerHello", "tls-alert")(
    lambda s: (s.start(), s.c.feed(plain_record(21, b"\2\x28"))))


@case("a warning-level alert in the flight", "tls-alert")
def warning(s):
    s.start()
    sh = s.server_hello(s.sid)
    s.keys(sh)
    s.c.feed(plain_record(22, sh) + s.write.seal(21, b"\1\x28"))


@case("a fatal alert after the handshake", "tls-alert")
def fatal_after(s):
    s.start()
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    s.c.feed(s.write.seal(21, b"\2\x50"))


@case("data under the key a KeyUpdate replaced", "tls-bad-record-mac", 20, True)
def stale_update(s):
    s.start()
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    s.c.feed(s.write.seal(22, message(24, b"\0")))
    # The server's write key did not move on; the client's read key did.
    s.c.feed(s.write.seal(23, b"the old key"))


# ---- TLS 1.2 (docs/tls-parity.md §3.4) ----

# Each TLS 1.2 suite: its AEAD, key length, fixed IV length and hash.
SUITES12 = {0xc02b: (AESGCM, 16, 4, hashlib.sha256), 0xc02c: (AESGCM, 32, 4, hashlib.sha384),
            0xc02f: (AESGCM, 16, 4, hashlib.sha256), 0xc030: (AESGCM, 32, 4, hashlib.sha384),
            0xcca8: (ChaCha20Poly1305, 32, 12, hashlib.sha256), 0xcca9: (ChaCha20Poly1305, 32, 12, hashlib.sha256)}


def prf(secret, label, seed, n, h):
    """RFC 5246 §5: P_hash(secret, label || seed)."""
    seed = label + seed
    out, a = b"", hmac.new(secret, seed, h).digest()
    while len(out) < n:
        out += hmac.new(secret, a + seed, h).digest()
        a = hmac.new(secret, a, h).digest()
    return out[:n]


class Keys12:
    """One direction's TLS 1.2 record protection (RFC 5288 §3, RFC 7905 §2)."""

    def __init__(self, suite, key, iv):
        self.aead, _, self.fixed, _ = SUITES12[suite]
        self.key, self.iv, self.seq = key, iv, 0

    def nonce(self, explicit):
        if self.fixed == 12:
            return bytes(a ^ b for a, b in zip(self.iv, self.seq.to_bytes(12, "big")))
        return self.iv + explicit

    def seal(self, kind, content, explicit=None):
        ad = self.seq.to_bytes(8, "big") + bytes([kind, 3, 3]) + u16(len(content))
        exp = b"" if self.fixed == 12 else (explicit if explicit is not None else self.seq.to_bytes(8, "big"))
        body = exp + self.aead(self.key).encrypt(self.nonce(exp), content, ad)
        self.seq += 1
        return bytes([kind, 3, 3]) + u16(len(body)) + body

    def open(self, record):
        exp = b"" if self.fixed == 12 else record[5:13]
        body = record[5 + len(exp):]
        ad = self.seq.to_bytes(8, "big") + record[:3] + u16(len(body) - 16)
        content = self.aead(self.key).decrypt(self.nonce(exp), body, ad)
        self.seq += 1
        return record[0], content


class Server12(Server):
    """A TLS 1.2 server: ServerHello, Certificate, ServerKeyExchange, ServerHelloDone,
    then the client's key exchange, change_cipher_spec and Finished, and its own.
    ECDHE-ECDSA with ChaCha20-Poly1305 and X25519 unless a case says otherwise; the
    Ed25519 certificate signs the key exchange (RFC 8422 §5.4)."""

    def __init__(self, conv):
        super().__init__(conv)
        self.suite = 0xcca9
        self.ems = True
        self.sh_random = None  # a case's random, else a fixed one
        self.sh_sid = seeded("tls12 session")
        self.sh_extra = b""
        self.reneg = ext(0xFF01, b"\0")
        self.ske_group = None
        self.signed_randoms = None
        self.request_cert = False

    @property
    def hash(self):
        return SUITES12[self.suite][3] if self.suite in SUITES12 else hashlib.sha256

    def server_hello12(self):
        exts = self.reneg + (ext(23, b"") if self.ems else b"") + ext(11, b"\1\0") + self.sh_extra
        self.server_random = self.sh_random or seeded("tls12 random")
        body = (u16(0x0303) + self.server_random + bytes([len(self.sh_sid)]) + self.sh_sid + u16(self.suite) + b"\0"
                + u16(len(exts)) + exts)
        return message(2, body)

    def certificate12(self):
        entry = u24(len(self.cert_der)) + self.cert_der
        return message(11, u24(len(entry)) + entry)

    def key_exchange(self):
        group = self.ske_group or self.group
        point = self.public_share()
        params = bytes([3]) + u16(group) + bytes([len(point)]) + point
        randoms = self.signed_randoms or (self.client_random + self.server_random)
        sig = self.cv_key.sign(randoms + params)
        return message(12, params + u16(0x0807) + u16(len(sig)) + sig)

    def start(self):
        super().start()
        self.client_random = self.transcript[6:38]

    def server_random_for_test(self):
        """Both randoms, in the wrong order: what a key exchange replayed from another
        connection would be signed over."""
        return (self.sh_random or seeded("tls12 random")) + self.client_random

    def flight12(self, middle=b""):
        """ServerHello to ServerHelloDone, in plaintext records; `middle` goes before
        ServerHelloDone."""
        msgs = [self.server_hello12(), self.certificate12(), self.key_exchange()]
        if self.request_cert:
            msgs.append(message(13, b"\1\x40" + u16(2) + u16(0x0807) + u16(0)))
        msgs.append(middle + message(14, b""))
        self.transcript += b"".join(msgs)
        return b"".join(plain_record(22, m) for m in msgs)

    def client_flight(self):
        """The client's Certificate (if asked), ClientKeyExchange, change_cipher_spec and
        Finished: the keys from them, and the Finished checked."""
        recs = self.c.take()
        h = self.hash
        if self.request_cert:
            cert = recs.pop(0)
            assert cert == plain_record(22, message(11, u24(0))), "an empty Certificate"
            self.transcript += cert[5:]
        cke = recs[0]
        assert cke[0] == 22 and cke[5] == 16, cke[:6].hex()
        point = cke[10:]
        assert cke[9] == len(point)
        self.client_shares = {self.group: point}
        self.transcript += cke[5:]
        pms = self.shared_secret()
        master = prf(pms, b"extended master secret", h(self.transcript).digest(), 48, h)
        _, kl, il, _ = SUITES12[self.suite]
        block = prf(master, b"key expansion", self.server_random + self.client_random, 2 * kl + 2 * il, h)
        self.read = Keys12(self.suite, block[:kl], block[2 * kl:2 * kl + il])
        self.write = Keys12(self.suite, block[kl:2 * kl], block[2 * kl + il:])
        assert recs[1] == plain_record(20, b"\1"), "change_cipher_spec"
        kind, fin = self.read.open(recs[2])
        want = message(20, prf(master, b"client finished", h(self.transcript).digest(), 12, h))
        assert (kind, fin) == (22, want), "the client's Finished"
        self.transcript += fin
        self.master = master
        self.c.sent = b"".join(recs[3:])

    def server_finish(self, bad=False):
        h = self.hash
        vd = prf(self.master, b"server finished", h(self.transcript).digest(), 12, h)
        if bad:
            vd = bytes([vd[0] ^ 1]) + vd[1:]
        return plain_record(20, b"\1") + self.write.seal(22, message(20, vd))


def honest12(setup=None):
    def run(s):
        s.start()
        if setup:
            setup(s)
        s.c.feed(s.flight12())
        s.client_flight()
        s.c.feed(s.server_finish())
        s.c.ask(f"W {REQUEST.hex()}")
        (req,) = s.c.take()
        assert s.read.open(req) == (23, REQUEST), "the request"
        f = s.c.feed(s.write.seal(23, b"HTTP/1.0 200 OK\r\n\r\ntls 1.2") + s.write.seal(21, b"\1\0"))
        assert f[2] == "4", f"closed: {f}"
        assert s.c.received == b"HTTP/1.0 200 OK\r\n\r\ntls 1.2"
    return run


def case12(name, tag, alert=None, encrypted=False):
    def register(fn):
        CASES.append((name, tag, alert, encrypted, fn, Server12))
        return fn
    return register


def hello12(**kw):
    def run(s):
        s.start()
        for k, v in kw.items():
            setattr(s, k, v)
        s.c.feed(plain_record(22, s.server_hello12()))
    return run


def flight12_case(change, middle=b""):
    def run(s):
        s.start()
        change(s)
        s.c.feed(s.flight12(middle))
    return run


case12("TLS 1.2, honest: ECDHE-ECDSA-CHACHA20-POLY1305", "ok")(honest12())
case12("TLS 1.2, honest: ECDHE-ECDSA-AES256-GCM-SHA384", "ok")(honest12(lambda s: setattr(s, "suite", 0xc02c)))
case12("TLS 1.2, honest: a CertificateRequest, AES-128-GCM", "ok")(
    honest12(lambda s: (setattr(s, "suite", 0xc02b), setattr(s, "request_cert", True))))
case12("TLS 1.2, honest: P-256", "ok")(honest12(lambda s: setattr(s, "group", P256)))
# RFC 6066 §3: a server that used the name sends server_name back, empty;
# nginx does (docs/tls-assurance.md §5).
case12("TLS 1.2, honest: server_name acknowledged", "ok")(honest12(lambda s: setattr(s, "sh_extra", ext(0, b""))))
case12("TLS 1.2, server_name twice", "tls-decode-error", 50)(hello12(sh_extra=ext(0, b"") + ext(0, b"")))
case12("TLS 1.2, the 1.2 downgrade sentinel", "tls-protocol-version", 70)(hello12(sh_random=bytes(24) + b"DOWNGRD\1"))
case12("TLS 1.2, the 1.1 downgrade sentinel", "tls-protocol-version", 70)(hello12(sh_random=bytes(24) + b"DOWNGRD\0"))
case12("TLS 1.2, no extended master secret", "tls-extended-master-secret", 40)(hello12(ems=False))
case12("TLS 1.2, a CBC suite", "tls-no-shared-cipher", 40)(hello12(suite=0xC013))
case12("TLS 1.2, a static-RSA suite", "tls-no-shared-cipher", 40)(hello12(suite=0x009C))
case12("TLS 1.2, a ServerHello echoing the client's session id", "tls-decode-error", 50)(
    lambda s: (s.start(), setattr(s, "sh_sid", s.sid), s.c.feed(plain_record(22, s.server_hello12()))))
case12("TLS 1.2, a key_share in the ServerHello", "tls-unsupported-extension", 110)(
    hello12(sh_extra=ext(51, u16(0x1D) + u16(32) + bytes(range(32)))))
case12("TLS 1.2, a pre_shared_key in the ServerHello (a TLS 1.3 resumption's)", "tls-unsupported-extension", 110)(
    hello12(sh_extra=ext(41, u16(0))))
case12("TLS 1.2, a renegotiated connection in renegotiation_info", "tls-decode-error", 50)(
    hello12(reneg=ext(0xFF01, b"\x0c" + bytes(12))))
case12("TLS 1.2, the key exchange signed by another key", "tls-bad-certificate-verify", 51)(
    flight12_case(lambda s: setattr(s, "cv_key", OTHER_KEY)))
case12("TLS 1.2, the key exchange signed over the wrong randoms", "tls-bad-certificate-verify", 51)(
    flight12_case(lambda s: setattr(s, "signed_randoms", s.server_random_for_test())))
case12("TLS 1.2, an RSA suite with an Ed25519 key", "tls-bad-certificate-verify", 51)(
    flight12_case(lambda s: setattr(s, "suite", 0xCCA8)))
case12("TLS 1.2, a key exchange on X448, never offered", "tls-key-share", 47)(
    flight12_case(lambda s: setattr(s, "ske_group", 0x1E)))
case12("TLS 1.2, ServerHelloDone before ServerKeyExchange", "tls-unexpected-message", 10)(
    lambda s: (s.start(), s.c.feed(plain_record(22, s.server_hello12()) + plain_record(22, s.certificate12())
                                   + plain_record(22, message(14, b"")))))
case12("TLS 1.2, a NewSessionTicket, never asked for", "tls-unexpected-message", 10)(
    flight12_case(lambda s: None, middle=message(4, bytes(4) + u16(3) + b"abc")))


@case12("TLS 1.2, a wrong Finished", "tls-bad-finished", 51, True)
def wrong_finished12(s):
    s.start()
    s.c.feed(s.flight12())
    s.client_flight()
    s.c.feed(s.server_finish(bad=True))


@case12("TLS 1.2, a plaintext Finished after change_cipher_spec", "tls-bad-record-mac", 20, True)
def plain_after_ccs(s):
    s.start()
    s.c.feed(s.flight12())
    s.client_flight()
    s.c.feed(plain_record(20, b"\1") + plain_record(22, message(20, bytes(12))))


@case12("TLS 1.2, a HelloRequest", "tls-renegotiation", 100, True)
def hello_request(s):
    s.start()
    s.c.feed(s.flight12())
    s.client_flight()
    s.c.feed(s.server_finish())
    s.c.feed(s.write.seal(22, message(0, b"")))


@case12("TLS 1.2 after a HelloRetryRequest", "tls-protocol-version", 70)
def tls12_after_retry(s):
    s.start()
    s.suite = 0x1303
    Server.retry(s, P256)
    s.suite = 0xCCA9
    s.c.feed(plain_record(22, s.server_hello12()))


# ---- Resumption (docs/tls-resumption.md) ----

TICKET = b"liar ticket, opaque to the client"


def psk_offer(msg):
    """The PSK the ClientHello `msg` offers: (identity, obfuscated age, binder, where in `msg` its
    binders start, the psk_key_exchange_modes), or None. pre_shared_key must be the last extension."""
    b = msg[4:]
    at = 2 + 32
    at += 1 + b[at]
    at += 2 + int.from_bytes(b[at:at + 2], "big")
    at += 1 + b[at]
    end = at + 2 + int.from_bytes(b[at:at + 2], "big")
    at += 2
    modes, found, last = None, None, None
    while at < end:
        kind, n = int.from_bytes(b[at:at + 2], "big"), int.from_bytes(b[at + 2:at + 4], "big")
        body = b[at + 4:at + 4 + n]
        last = kind
        if kind == 45:
            modes = list(body[1:1 + body[0]])
        if kind == 41:
            ids = int.from_bytes(body[0:2], "big")
            idl = int.from_bytes(body[2:4], "big")
            assert ids == 2 + idl + 4, "one identity"
            identity = body[4:4 + idl]
            age = int.from_bytes(body[4 + idl:8 + idl], "big")
            binders = 2 + ids
            assert int.from_bytes(body[binders:binders + 2], "big") == 1 + body[binders + 2], "one binder"
            binder = body[binders + 3:binders + 3 + body[binders + 2]]
            found = (identity, age, binder, 4 + at + 4 + binders)
        at += 4 + n
    if found is None:
        return None
    assert last == 41, "pre_shared_key is the last extension"
    return found + (modes,)


def psk_hash(psk):
    return hashlib.sha384 if len(psk) == 48 else hashlib.sha256


def check_binder(msg, psk, prefix=b""):
    """The binder of the ClientHello `msg`, which follows `prefix` in the transcript, as RFC 8446
    §4.2.11.2 computes it: the client's is checked against this server's."""
    offer = psk_offer(msg)
    assert offer is not None, "a PSK is offered"
    identity, age, binder, binders_at, modes = offer
    assert identity == TICKET, f"the ticket offered: {identity!r}"
    assert modes == [1], f"psk_dhe_ke only: {modes}"
    h = psk_hash(psk)
    n = len(psk)
    early = hmac.new(bytes(n), psk, h).digest()
    finished_key = expand_label(derive(early, b"res binder", b"", h), b"finished", b"", n, h)
    want = hmac.new(finished_key, h(prefix + msg[:binders_at]).digest(), h).digest()
    assert binder == want, "the binder"


def issue(s, ticket=TICKET, nonce=b"\7", lifetime=7200, age_add=0x01020304):
    """A NewSessionTicket on the connection `s`, whose client Finished has been checked: its PSK, as
    this server derives it (RFC 8446 §4.6.1, §7.1)."""
    res = derive(s.master, b"res master", s.transcript, s.hash)
    psk = expand_label(res, b"resumption", nonce, len(res), s.hash)
    body = lifetime.to_bytes(4, "big") + age_add.to_bytes(4, "big") + bytes([len(nonce)]) + nonce + u16(len(ticket)) + ticket + u16(0)
    s.c.feed(s.write.seal(22, message(4, body)))
    return psk


def first_connection(s, suite=0x1303, **ticket):
    """A full handshake under `suite`, a ticket, and the client's account of it: the PSK it derived
    must be this server's."""
    s.start()
    s.suite = suite
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    psk = issue(s, **ticket)
    f = s.c.ask("K")
    assert f[5] == "0", "the first connection is not a resumption"
    assert (f[6], f[7]) == (TICKET.hex(), psk.hex()), f"the ticket and PSK the client kept: {f[6:8]}"
    return psk


def resume(s, psk, age=1500):
    """A second connection, on the same driver, offering the ticket: a new Server for it."""
    r = Server(s.c)
    f = r.c.ask(f"R {HOST.hex()} {RANDOM.hex()} {ROOTS.hex()} {NOW} {TICKET.hex()} {psk.hex()} {age} {NOW} {NOW + 86400}")
    assert f[0] == "0", f
    (hello,) = r.c.take()
    msg, r.sid, r.client_shares, _ = parse_client_hello(hello)
    r.transcript = msg
    r.ccs_sent = False
    r.psk = psk
    return r


def resumed_flight(r, after_ee=b""):
    """ServerHello accepting identity 0, then EncryptedExtensions and Finished: no Certificate."""
    r.resumed = True
    r.extra_extensions += ext(41, u16(0))
    sh = r.server_hello(r.sid)
    r.keys(sh)
    ee = r.encrypted_extensions()
    r.transcript += ee + after_ee
    fin = r.finished(r.transcript)
    r.transcript += fin
    return plain_record(22, sh) + plain_record(20, b"\1") + r.write.seal(22, ee + after_ee + fin)


def resumed_to_the_end(r):
    r.check_client_finished()
    r.c.ask(f"W {REQUEST.hex()}")
    (req,) = r.c.take()
    assert r.read.open(req) == (23, REQUEST), "the request, under the resumed keys"
    f = r.c.feed(r.write.seal(23, b"resumed") + r.write.seal(21, b"\1\0"))
    assert f[2] == "4", f"closed: {f}"


@case("resumption: a ticket, then a second connection offering it, the binder checked; no Certificate", "ok")
def resumption(s):
    psk = first_connection(s)
    r = resume(s, psk)
    check_binder(r.transcript, psk)
    r.c.feed(resumed_flight(r))
    resumed_to_the_end(r)
    f = r.c.ask("K")
    assert f[5] == "1", f"resumed: {f}"


@case("resumption under AES-256-GCM-SHA384: a 48-byte PSK, the binder under SHA-384", "ok")
def resumption_sha384(s):
    psk = first_connection(s, suite=0x1302)
    assert len(psk) == 48, "a SHA-384 PSK"
    r = resume(s, psk)
    check_binder(r.transcript, psk)
    r.suite = 0x1302
    r.c.feed(resumed_flight(r))
    resumed_to_the_end(r)
    assert r.c.ask("K")[5] == "1", "resumed"


@case("resumption after a HelloRetryRequest to P-256: the second ClientHello's binder over message_hash and the retry", "ok")
def resumption_after_retry(s):
    psk = first_connection(s)
    r = resume(s, psk)
    first = r.transcript
    hrr = r.hello_retry(r.sid, P256)
    r.c.feed(plain_record(22, hrr))
    (hello,) = r.c.take()
    msg, sid, r.client_shares, _ = parse_client_hello(hello)
    prefix = message(254, r.hash(first).digest()) + hrr
    check_binder(msg, psk, prefix)
    r.transcript = prefix + msg
    r.group = P256
    r.c.feed(resumed_flight(r))
    resumed_to_the_end(r)
    assert r.c.ask("K")[5] == "1", "resumed"


@case("resumption declined: the server answers a ticket with a full handshake, verified", "ok")
def resumption_declined(s):
    psk = first_connection(s)
    r = resume(s, psk)
    check_binder(r.transcript, psk)
    r.c.feed(r.hello_and_flight())
    resumed_to_the_end(r)
    assert r.c.ask("K")[5] == "0", "not resumed"


@case("a HelloRetryRequest to a suite with another hash: the second ClientHello offers no ticket", "ok")
def resumption_retry_other_hash(s):
    psk = first_connection(s)
    r = resume(s, psk)
    first = r.transcript
    hrr = r.hello_retry(r.sid, P256, suite=0x1302)
    r.c.feed(plain_record(22, hrr))
    (hello,) = r.c.take()
    msg, sid, r.client_shares, _ = parse_client_hello(hello)
    assert psk_offer(msg) is None, "no ticket under SHA-384"
    r.suite = 0x1302
    r.transcript = message(254, hashlib.sha384(first).digest()) + hrr + msg
    r.group = P256
    r.c.feed(r.hello_and_flight())
    resumed_to_the_end(r)
    assert r.c.ask("K")[5] == "0", "not resumed"


@case("resumption: the ServerHello selects identity 1, of the one offered", "tls-illegal-psk", alert=47)
def resumption_identity_one(s):
    r = resume(s, first_connection(s))
    r.extra_extensions = ext(41, u16(1))
    r.c.feed(plain_record(22, r.server_hello(r.sid)))


@case("resumption: pre_shared_key and no key_share (psk_ke, which was not offered)", "tls-key-share", alert=47)
def resumption_no_share(s):
    r = resume(s, first_connection(s))
    exts = r.version_ext + ext(41, u16(0))
    body = u16(0x0303) + r.random + bytes([len(r.sid)]) + r.sid + u16(r.suite) + b"\0" + u16(len(exts)) + exts
    r.c.feed(plain_record(22, message(2, body)))


@case("resumption: a suite whose hash is not the ticket's", "tls-illegal-psk", alert=47)
def resumption_other_hash(s):
    r = resume(s, first_connection(s))
    r.suite = 0x1302
    r.extra_extensions = ext(41, u16(0))
    r.c.feed(plain_record(22, r.server_hello(r.sid)))


@case("pre_shared_key in a ServerHello when no ticket was offered", "tls-unsupported-extension", alert=110)
def psk_not_offered(s):
    s.start()
    s.extra_extensions = ext(41, u16(0))
    s.c.feed(plain_record(22, s.server_hello(s.sid)))


@case("resumption: a Certificate after a resumed ServerHello", "tls-unexpected-message", alert=10)
def resumption_with_certificate(s):
    r = resume(s, first_connection(s))
    r.c.feed(resumed_flight(r, after_ee=r.certificate()))


@case("tickets not kept: 3,000 bytes, a lifetime of 0; one over 7 days kept for 7", "ok")
def tickets_not_kept(s):
    s.start()
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    issue(s, ticket=bytes(3000))
    assert s.c.ask("K")[6] == "-", "a ticket over 2,048 bytes is not kept"
    issue(s, lifetime=0)
    assert s.c.ask("K")[6] == "-", "a lifetime of 0 is not kept"
    issue(s, lifetime=700000)
    f = s.c.ask("K")
    assert f[6] == TICKET.hex() and f[8] == "604800", f"kept, for 7 days: {f[6:9]}"
    f = s.c.feed(s.write.seal(21, b"\1\0"))
    assert f[2] == "4", f"closed: {f}"
    f = s.c.ask("K")


def run_case(driver, name, tag, alert, encrypted, fn, server=None):
    conv = Conversation(driver)
    s = (server or Server)(conv)
    try:
        fn(s)
        last = conv.answers[-1].split(" ")
        got = last[1]
        if got != tag:
            raise Failed(f"ended {conv.answers[-1][:80]}, wanted {tag}")
        if tag != "ok":
            assert last[2] == "5", f"failed: {last[:3]}"
            if alert is not None:
                s.expect_alert(alert, encrypted)
    except (Failed, AssertionError) as e:
        conv.close()
        return False, f"{name}: {e}", conv.lines
    conv.close()
    return True, f"{name}: {tag}", conv.lines


SEED = bytes(range(32))


def response(hello):
    body = hashlib.sha512(hello).digest() * 16
    return b"HTTP/1.0 200 OK\r\nContent-Length: 1024\r\n\r\n" + body


def stream_for(hello):
    """The honest server's stream for one ClientHello record, and its Server."""
    s = Server(None)
    if sha256(hello)[0] % 8 == 0:
        # A chain of about 60 KB: the slot's reassembly buffer nearly full.
        s.filler = [hashlib.shake_256(hello + bytes([i])).digest(15000) for i in range(4)]
    msg, s.sid, s.client_shares, _ = parse_client_hello(hello)
    s.transcript = msg
    s.ccs_sent = False
    flight = s.hello_and_flight()
    # The application keys do not depend on the client's Finished.
    write = Keys(derive(s.master, b"s ap traffic", s.transcript))
    reply = write.seal(23, response(hello), pad=33) + write.seal(21, b"\1\0")
    return flight, reply, s


def record_streams(tls_many, out, conc=64):
    import socket
    import threading
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(conc)
    rows, problems = [], []

    def serve(conn):
        try:
            data = b""
            while len(data) < 5 or len(data) < 5 + int.from_bytes(data[3:5], "big"):
                data += conn.recv(4096)
            hello = data[:5 + int.from_bytes(data[3:5], "big")]
            flight, reply, s = stream_for(hello)
            conn.sendall(flight)
            # The reply after the client's Finished and request: two
            # application_data records after its change_cipher_spec.
            got = data[len(hello):]
            while sum(1 for r in records_whole(got) if r[0] == 23) < 2:
                got += conn.recv(65536)
            conn.sendall(reply)
            while chunk := conn.recv(65536):
                got += chunk
            recs = records(got)
            assert recs[0] == plain_record(20, b"\1")
            key = expand_label(s.c_hs, b"finished", b"", 32)
            assert s.read.open(recs[1]) == (22, message(20, hmac.new(key, sha256(s.transcript), hashlib.sha256).digest()))
            app = Keys(derive(s.master, b"c ap traffic", s.transcript))
            assert app.open(recs[2])[1].startswith(b"GET / HTTP/1.0\r\nHost: " + HOST), "the request"
            assert app.open(recs[3]) == (21, b"\1\0"), "close_notify"
            r = response(hello)
            rows.append(f"{hello.hex()} {flight.hex()} {reply.hex()} {len(r)} {hashlib.sha256(r).hexdigest()}")
        except Exception as e:  # noqa: BLE001 -- reported below
            problems.append(repr(e))
        finally:
            conn.close()

    def accept():
        for _ in range(conc):
            conn, _ = listener.accept()
            threading.Thread(target=serve, args=(conn,), daemon=True).start()

    threading.Thread(target=accept, daemon=True).start()
    pem = ROOTS
    port = listener.getsockname()[1]
    run = subprocess.run([tls_many, "127.0.0.1", str(port), HOST.decode(), str(conc), "65536", SEED.hex()],
                         input=pem, capture_output=True, timeout=120)
    lines = run.stdout.decode().splitlines()
    import time
    for _ in range(50):
        if len(rows) + len(problems) == conc:
            break
        time.sleep(0.1)
    print(lines[-1] if lines else run.stderr.decode())
    if problems or run.returncode != 0 or len(rows) != conc:
        print("\n".join(problems[:5]))
        sys.exit(1)
    head = ["# scripts/tls_liar.py --streams: the honest server's stream for each of tls_many's 64 ClientHellos",
            f"# with the seed {SEED.hex()}: ClientHello, the server's flight, its reply after the client's",
            "# Finished and request, and the response's length and SHA-256.",
            f"# Root: {CA[1].public_bytes(serialization.Encoding.DER).hex()}"]
    open(out, "w").write("\n".join(head + sorted(rows)) + "\n")
    open(out.rsplit(".", 1)[0] + ".pem", "wb").write(pem)
    print(f"{out}: {conc} streams; the root to trust beside it, .pem")


def main():
    if sys.argv[1] == "--streams":
        record_streams(sys.argv[2], sys.argv[3])
        return
    driver, out = sys.argv[1], sys.argv[2]
    lines = ["# scripts/tls_liar.py: packages/tls against a server that lies, one case a connection.",
             "# `## <tag> <name>` starts a case; `=` lines are the client's answers."]
    bad = 0
    for name, tag, alert, encrypted, fn, server in CASES:
        ok, what, recorded = run_case(driver, name, tag, alert, encrypted, fn, server)
        print(("" if ok else "FAILED ") + what)
        bad += not ok
        lines += [f"## {tag} {name}"] + recorded
    open(out, "w").write("\n".join(lines) + "\n")
    print(f"{len(CASES)} cases, {bad} failed")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
