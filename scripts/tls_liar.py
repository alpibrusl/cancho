#!/usr/bin/env python3
"""A TLS 1.3 server that lies, against `packages/tls` (docs/tls-core.md §6.3).

    python3 scripts/tls_liar.py <driver> <out.txt>
    python3 scripts/tls_liar.py --streams <tls_many> <out.txt>

`driver` is `tests/programs/tls_driver.ls` built with `--std` and the package's
files. The server is written here on pyca/cryptography's primitives (X25519,
ChaCha20-Poly1305, Ed25519, HMAC-SHA-256), independently of the client; it
knows only RFC 8446. Each case is one connection in which the server changes
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
from cryptography.hazmat.primitives.asymmetric import ed25519, x25519
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.x509.oid import NameOID

HOST = b"liar.lex-sys.test"
RANDOM = bytes(range(96))
HRR = bytes.fromhex("cf21ad74e59a6111be1d8c021e65b891c2a211167abb8c5e079e09e2c8a8339c")
REQUEST = b"GET / HTTP/1.0\r\n\r\n"


def sha256(b):
    return hashlib.sha256(b).digest()


def seeded(name):
    return sha256(b"tls_liar " + name.encode())


def certificate(seed_name):
    key = ed25519.Ed25519PrivateKey.from_private_bytes(seeded(seed_name))
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, HOST.decode())])
    start = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)
    cert = (x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(key.public_key())
            .serial_number(206).not_valid_before(start).not_valid_after(start + datetime.timedelta(days=3650))
            .add_extension(x509.SubjectAlternativeName([x509.DNSName(HOST.decode())]), False)
            .sign(key, None))
    return key, cert.public_bytes(serialization.Encoding.DER)


KEY, DER = certificate("server")
OTHER_KEY, OTHER_DER = certificate("other")
PINS = len(DER).to_bytes(3, "big") + DER


def expand_label(secret, label, context, n):
    full = b"tls13 " + label
    info = n.to_bytes(2, "big") + bytes([len(full)]) + full + bytes([len(context)]) + context
    out, t, i = b"", b"", 1
    while len(out) < n:
        t = hmac.new(secret, t + info + bytes([i]), hashlib.sha256).digest()
        out, i = out + t, i + 1
    return out[:n]


def derive(secret, label, transcript):
    return expand_label(secret, label, sha256(transcript), 32)


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
    def __init__(self, secret):
        self.secret = secret
        self.key = expand_label(secret, b"key", b"", 32)
        self.iv = expand_label(secret, b"iv", b"", 12)
        self.seq = 0

    def nonce(self):
        return bytes(a ^ b for a, b in zip(self.iv, self.seq.to_bytes(12, "big")))

    def seal(self, kind, content, pad=0):
        inner = content + bytes([kind]) + bytes(pad)
        header = bytes([23, 3, 3]) + u16(len(inner) + 16)
        record = header + ChaCha20Poly1305(self.key).encrypt(self.nonce(), inner, header)
        self.seq += 1
        return record

    def open(self, record):
        inner = ChaCha20Poly1305(self.key).decrypt(self.nonce(), record[5:], record[:5])
        self.seq += 1
        inner = inner.rstrip(b"\0")
        return inner[-1], inner[:-1]

    def next(self):
        return Keys(expand_label(self.secret, b"traffic upd", b"", 32))


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
    share = None
    while at < end:
        kind, n = int.from_bytes(b[at:at + 2], "big"), int.from_bytes(b[at + 2:at + 4], "big")
        body = b[at + 4:at + 4 + n]
        if kind == 51:
            s = 2
            while s < len(body):
                group, m = int.from_bytes(body[s:s + 2], "big"), int.from_bytes(body[s + 2:s + 4], "big")
                if group == 0x1D:
                    share = body[s + 4:s + 4 + m]
                s += 4 + m
        at += 4 + n
    return msg, sid, share


class Server:
    """The honest server, whose every step a case may replace."""

    def __init__(self, conv):
        self.c = conv
        self.transcript = b""
        self.x25519 = x25519.X25519PrivateKey.from_private_bytes(seeded("x25519"))
        self.random = seeded("random")
        self.suite = 0x1303
        self.version_ext = ext(43, u16(0x0304))
        self.share = None  # the server's public share, unless a case sets it
        self.extra_extensions = b""
        self.ee_extensions = b""
        self.cert_der = DER
        self.cv_key = KEY
        self.bad_finished = False
        self.filler = []  # more chain entries after the leaf, never parsed before #206

    # ---- the flight ----
    def server_hello(self, sid):
        share = self.share if self.share is not None else self.x25519.public_key().public_bytes_raw()
        exts = self.version_ext + ext(51, u16(0x1D) + u16(len(share)) + share) + self.extra_extensions
        body = u16(0x0303) + self.random + bytes([len(sid)]) + sid + u16(self.suite) + b"\0" + u16(len(exts)) + exts
        return message(2, body)

    def encrypted_extensions(self):
        return message(8, u16(len(self.ee_extensions)) + self.ee_extensions)

    def certificate(self):
        entries = b"".join(u24(len(c)) + c + u16(0) for c in [self.cert_der] + self.filler)
        return message(11, b"\0" + u24(len(entries)) + entries)

    def certificate_verify(self, transcript):
        content = b" " * 64 + b"TLS 1.3, server CertificateVerify\0" + sha256(transcript)
        sig = self.cv_key.sign(content)
        return message(15, u16(0x0807) + u16(len(sig)) + sig)

    def finished(self, transcript):
        key = expand_label(self.s_hs, b"finished", b"", 32)
        mac = hmac.new(key, sha256(transcript), hashlib.sha256).digest()
        if self.bad_finished:
            mac = bytes([mac[0] ^ 1]) + mac[1:]
        return message(20, mac)

    def start(self):
        f = self.c.ask(f"C {HOST.hex()} {RANDOM.hex()} {PINS.hex()}")
        assert f[0] == "0", f
        (hello,) = self.c.take()
        msg, self.sid, self.client_share = parse_client_hello(hello)
        self.transcript = msg

    def keys(self, sh):
        self.transcript += sh
        shared = self.x25519.exchange(x25519.X25519PublicKey.from_public_bytes(self.client_share))
        early = hmac.new(bytes(32), bytes(32), hashlib.sha256).digest()
        hs = hmac.new(derive(early, b"derived", b""), shared, hashlib.sha256).digest()
        self.c_hs = derive(hs, b"c hs traffic", self.transcript)
        self.s_hs = derive(hs, b"s hs traffic", self.transcript)
        self.master = hmac.new(derive(hs, b"derived", b""), bytes(32), hashlib.sha256).digest()
        self.write = Keys(self.s_hs)
        self.read = Keys(self.c_hs)

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
        out = plain_record(22, sh) + plain_record(20, b"\1")
        for i in range(k):
            piece = cert[cuts[i]:cuts[i + 1]]
            out += self.write.seal(22, ee + piece if i == 0 else piece, pad=7 if i == 1 else 0)
        out += self.write.seal(22, cv + fin, pad=300)
        return out

    def check_client_finished(self):
        recs = self.c.take()
        assert recs[0] == plain_record(20, b"\1"), "a change_cipher_spec first"
        kind, fin = self.read.open(recs[1])
        key = expand_label(self.c_hs, b"finished", b"", 32)
        want = message(20, hmac.new(key, sha256(self.transcript), hashlib.sha256).digest())
        assert (kind, fin) == (22, want), "the client's Finished"
        app_th = self.transcript
        self.transcript += fin
        self.write = Keys(derive(self.master, b"s ap traffic", app_th))
        self.read = Keys(derive(self.master, b"c ap traffic", app_th))
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
        CASES.append((name, tag, alert, encrypted, fn))
        return fn
    return register


@case("honest, in the hardest legal shape", "ok")
def honest(s):
    s.start()
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
case("TLS 1.2: no supported_versions", "tls-protocol-version", 70)(lambda s: hello(s, version_ext=b""))
case("supported_versions TLS 1.2", "tls-protocol-version", 70)(lambda s: hello(s, version_ext=ext(43, u16(0x0303))))
case("the TLS 1.2 downgrade sentinel", "tls-protocol-version", 70)(
    lambda s: hello(s, random=bytes(24) + b"DOWNGRD\1"))
case("the TLS 1.1 downgrade sentinel", "tls-protocol-version", 70)(
    lambda s: hello(s, random=bytes(24) + b"DOWNGRD\0"))
case("AES-128-GCM, not offered", "tls-no-shared-cipher", 40)(lambda s: hello(s, suite=0x1301))
case("HelloRetryRequest", "tls-hello-retry", 40)(lambda s: hello(s, random=HRR))
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
case("an unpinned certificate", "x509-unknown-issuer", 48)(
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


def run_case(driver, name, tag, alert, encrypted, fn):
    conv = Conversation(driver)
    s = Server(conv)
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
    msg, s.sid, s.client_share = parse_client_hello(hello)
    s.transcript = msg
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
    pem = ("-----BEGIN CERTIFICATE-----\n" + __import__("base64").encodebytes(DER).decode() + "-----END CERTIFICATE-----\n").encode()
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
            f"# Pinned certificate: {DER.hex()}"]
    open(out, "w").write("\n".join(head + sorted(rows)) + "\n")
    open(out.rsplit(".", 1)[0] + ".pem", "wb").write(pem)
    print(f"{out}: {conc} streams; the certificate to pin beside it, .pem")


def main():
    if sys.argv[1] == "--streams":
        record_streams(sys.argv[2], sys.argv[3])
        return
    driver, out = sys.argv[1], sys.argv[2]
    lines = ["# scripts/tls_liar.py: packages/tls against a server that lies, one case a connection.",
             "# `## <tag> <name>` starts a case; `=` lines are the client's answers."]
    bad = 0
    for name, tag, alert, encrypted, fn in CASES:
        ok, what, recorded = run_case(driver, name, tag, alert, encrypted, fn)
        print(("" if ok else "FAILED ") + what)
        bad += not ok
        lines += [f"## {tag} {name}"] + recorded
    open(out, "w").write("\n".join(lines) + "\n")
    print(f"{len(CASES)} cases, {bad} failed")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
