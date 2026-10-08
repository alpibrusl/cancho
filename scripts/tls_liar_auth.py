#!/usr/bin/env python3
"""A TLS server that lies about client certificates and ALPN, against `packages/tls` (docs/tls-parity.md §6).

    python3 scripts/tls_liar_auth.py <driver> <out.txt>

The same lying server as `scripts/tls_liar.py` (its classes are imported, unchanged, so the 84 connections recorded
in `tests/vectors/tls/liar.txt` stay as they were), with a CertificateRequest it can shape and a check of what the
client answers it with: the client's Certificate, its CertificateVerify under the P-256 key of
`tests/programs/tls_driver.cho`'s identity, and the Finished that covers both. One case is one connection, or for the
identity's own refusals one `I` line. `crates/cancho/tests/conformance/tls_auth.rs` replays every case on both
backends, and `scripts/tls_mutants.py` runs its mutants against them. Exit status 1 on any difference.
"""
import datetime
import hashlib
import hmac
import sys

sys.path.insert(0, __file__.rsplit("/", 1)[0])
import tls_liar as L  # noqa: E402
from tls_liar import (CA, HOST, NOW, OTHER_CA, OTHER_KEY, P256, START, END, Conversation, Failed, Keys, Server,  # noqa: E402
                      Server12, ext, expand_label, derive, message, plain_record, prf, seeded, sha256, u16, u24)

from cryptography import x509  # noqa: E402
from cryptography.exceptions import InvalidSignature  # noqa: E402
from cryptography.hazmat.primitives import hashes, serialization  # noqa: E402
from cryptography.hazmat.primitives.asymmetric import ec  # noqa: E402
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID  # noqa: E402

P256_N = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551
ECDSA_SHA256 = 0x0403
ED25519 = 0x0807
RSA_PSS = 0x0804


def client_key(name):
    return ec.derive_private_key(int.from_bytes(seeded(name), "big") % (P256_N - 1) + 1, ec.SECP256R1())


def client_certificate(key, ca, not_after=END, name="tls_liar client", serial=386):
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, name)])
    return (x509.CertificateBuilder().subject_name(subject).issuer_name(ca[1].subject).public_key(key.public_key())
            .serial_number(serial).not_valid_before(START).not_valid_after(not_after)
            .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.CLIENT_AUTH]), False)
            .sign(ca[0], None))


def pem_key(key):
    return key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption())


def pem_cert(cert):
    return cert.public_bytes(serialization.Encoding.PEM)


KEY = client_key("client key")
CERT = client_certificate(KEY, CA)
OTHER_CLIENT_KEY = client_key("other client key")
OTHER_CLIENT_CERT = client_certificate(OTHER_CLIENT_KEY, OTHER_CA)
EXPIRED = client_certificate(KEY, CA, not_after=START + datetime.timedelta(days=30))
CA_NAME = CA[1].subject.public_bytes()
OTHER_CA_NAME = OTHER_CA[1].subject.public_bytes()


def authorities(names):
    return ext(47, u16(sum(2 + len(n) for n in names)) + b"".join(u16(len(n)) + n for n in names))


def sigalgs(*schemes):
    return ext(13, u16(2 * len(schemes)) + b"".join(u16(s) for s in schemes))


def alpn_ext(*names):
    body = b"".join(bytes([len(n)]) + n for n in names)
    return ext(16, u16(len(body)) + body)


def client_extensions(msg):
    """The extensions of the ClientHello message `msg`, as (type, body) in order."""
    b = msg[4:]
    at = 2 + 32
    at += 1 + b[at]
    at += 2 + int.from_bytes(b[at:at + 2], "big")
    at += 1 + b[at]
    end = at + 2 + int.from_bytes(b[at:at + 2], "big")
    at += 2
    out = []
    while at < end:
        kind, n = int.from_bytes(b[at:at + 2], "big"), int.from_bytes(b[at + 2:at + 4], "big")
        out.append((kind, b[at + 4:at + 4 + n]))
        at += 4 + n
    assert at == end
    return out


def check_offer(msg, alpn):
    """The ClientHello carries the ALPN extension (16) once, with exactly the offer, between
    signature_algorithms_cert (50) and supported_versions (43); with no offer, none."""
    exts = client_extensions(msg)
    kinds = [k for k, _ in exts]
    if not alpn:
        assert 16 not in kinds, "no ALPN extension when nothing is offered"
        return
    assert kinds.count(16) == 1, "ALPN once"
    body = dict(exts)[16]
    names = alpn.split(" ")
    wire = b"".join(bytes([len(n)]) + n.encode() for n in names)
    assert body == u16(len(wire)) + wire, f"the offer: {body.hex()}"
    assert kinds.index(50) < kinds.index(16) < kinds.index(43), "its place among the extensions"


def prelude(conv, alpn, identity, hosts, extra=()):
    """The driver lines before `C`: the ALPN offer and the identity; `extra` certificates follow the leaf in its chain."""
    if alpn:
        f = conv.ask(f"L {alpn.encode().hex()}")
        assert f[0] == "0", f
    if identity:
        chain = pem_cert(CERT) + b"".join(pem_cert(c) for c in extra)
        f = conv.ask(f"I 0 {chain.hex()} {pem_key(KEY).hex()} {hosts.hex()} {NOW}")
        assert f[0] == "0", f


class AuthServer(Server):
    """The TLS 1.3 server of `tls_liar`, asking for a certificate when `cr_exts` is set."""

    def __init__(self, conv):
        super().__init__(conv)
        self.cr_exts = None  # the CertificateRequest's extensions, or None for no request
        self.cr_context = b""
        self.cr_raw = None  # a whole CertificateRequest body, for the malformed ones
        self.cr_count = 1
        self.client_pub = KEY.public_key()
        self.client_cert = CERT
        self.chain = None

    def start(self, alpn=None, identity=True, hosts=HOST, extra=()):
        prelude(self.c, alpn, identity, hosts, extra)
        self.chain = [self.client_cert] + list(extra)
        super().start()
        check_offer(self.transcript, alpn)

    def cr_message(self):
        if self.cr_raw is not None:
            return message(13, self.cr_raw)
        if self.cr_exts is None:
            return b""
        return message(13, bytes([len(self.cr_context)]) + self.cr_context + u16(len(self.cr_exts)) + self.cr_exts)

    def flight_messages(self):
        ee = self.encrypted_extensions()
        cr = self.cr_message() * self.cr_count
        cert = self.certificate()
        self.transcript += ee + cr + cert
        cv = self.certificate_verify(self.transcript)
        self.transcript += cv
        fin = self.finished(self.transcript)
        self.transcript += fin
        return ee, cr + cert, cv, fin

    def client_flight(self, expect):
        """The client's change_cipher_spec, then Certificate, CertificateVerify when it sent a chain, and Finished,
        each checked; "chain" or "empty"."""
        recs = self.c.take()
        assert recs[0] == plain_record(20, b"\1"), "a change_cipher_spec first"
        h = self.hash
        app_th = self.transcript
        kind, cert = self.read.open(recs[1])
        assert kind == 22 and cert[0] == 11, "the client's Certificate"
        body = cert[4:]
        assert body[0] == len(self.cr_context) and body[1:1 + body[0]] == self.cr_context, "the context echoed"
        lst = body[1 + body[0]:]
        assert int.from_bytes(lst[:3], "big") == len(lst) - 3
        rest = recs[2:]
        if expect == "empty":
            assert len(lst) == 3, "an empty Certificate"
        else:
            at = 3
            for cert_in_chain in (self.chain or [self.client_cert]):
                n = int.from_bytes(lst[at:at + 3], "big")
                der = lst[at + 3:at + 3 + n]
                assert der == cert_in_chain.public_bytes(serialization.Encoding.DER), "the identity's certificate"
                assert lst[at + 3 + n:at + 5 + n] == b"\0\0", "no certificate extensions"
                at += 5 + n
            assert at == len(lst), "the whole chain, and nothing after it"
        self.transcript += cert
        if expect == "chain":
            kind, cv = self.read.open(rest.pop(0))
            assert kind == 22 and cv[0] == 15, "CertificateVerify"
            assert int.from_bytes(cv[4:6], "big") == ECDSA_SHA256, "ecdsa_secp256r1_sha256"
            sig = cv[8:8 + int.from_bytes(cv[6:8], "big")]
            content = b" " * 64 + b"TLS 1.3, client CertificateVerify\0" + h(self.transcript).digest()
            try:
                self.client_pub.verify(sig, content, ec.ECDSA(hashes.SHA256()))
            except InvalidSignature:
                raise AssertionError("the client's CertificateVerify does not verify")
            self.transcript += cv
        kind, fin = self.read.open(rest.pop(0))
        key = expand_label(self.c_hs, b"finished", b"", len(self.c_hs), h)
        want = message(20, hmac.new(key, h(self.transcript).digest(), h).digest())
        assert (kind, fin) == (22, want), "the client's Finished"
        self.transcript += fin
        self.write = Keys(derive(self.master, b"s ap traffic", app_th, h), self.suite)
        self.read = Keys(derive(self.master, b"c ap traffic", app_th, h), self.suite)
        self.c.sent = b"".join(rest)

    def query(self):
        """`Y`: the client's account of the authentication and of ALPN, `(client_auth, protocol)`."""
        self.c.proc.stdin.write("Y\n")
        self.c.proc.stdin.flush()
        answer = self.c.proc.stdout.readline().strip()
        self.c.lines += ["Y", "= " + answer]
        f = answer.split(" ")
        return int(f[2]), (bytes.fromhex(f[3]) if f[3] != "-" else b"")

    def finish_honest(self):
        self.c.ask(f"W {L.REQUEST.hex()}")
        (req,) = self.c.take()
        assert self.read.open(req) == (23, L.REQUEST), "the request"
        f = self.c.feed(self.write.seal(23, b"mutual") + self.write.seal(21, b"\1\0"))
        assert f[2] == "4", f"closed: {f}"


CASES = []


def case(name, tag, alert=None, encrypted=False, server=AuthServer, config=False):
    def register(fn):
        CASES.append((name, tag, alert, encrypted, fn, server, config))
        return fn
    return register


def mutual(expect, auth, exts, suite=0x1303, retry=None, **kw):
    """A connection whose server asks with `exts`, and what the client does: `expect` ("chain" or "empty") on the wire,
    `auth` (1 or 2) in `client_auth`."""
    def run(s):
        s.start(**kw)
        s.suite = suite
        s.cr_exts = exts
        if retry:
            s.retry(retry)
        s.c.feed(s.hello_and_flight())
        s.client_flight(expect)
        assert s.query()[0] == auth, "client_auth"
        s.finish_honest()
    return run


# ---- TLS 1.3: a CertificateRequest the client can answer ----
SIG = sigalgs(ECDSA_SHA256, ED25519)
case("client auth: a chain and a CertificateVerify, ChaCha20-Poly1305", "ok")(mutual("chain", 1, SIG))
case("client auth: AES-256-GCM-SHA384, the transcript hash SHA-384 and the digest signed SHA-256", "ok")(
    mutual("chain", 1, SIG, suite=0x1302))
case("client auth: AES-128-GCM after a HelloRetryRequest to P-256", "ok")(mutual("chain", 1, SIG, suite=0x1301, retry=P256))
case("client auth: the authorities name the client's issuer", "ok")(
    mutual("chain", 1, SIG + authorities([OTHER_CA_NAME, CA_NAME])))
case("client auth: a context echoed in Certificate", "ok")(
    lambda s: (setattr(s, "cr_context", b"ctx"), mutual("chain", 1, SIG)(s)))
case("client auth: the identity named by a wildcard host", "ok")(
    mutual("chain", 1, SIG, hosts=b"*.lex-sys.test"))
case("client auth: the identity named for every host", "ok")(mutual("chain", 1, SIG, hosts=b"*"))
case("client auth: an unknown extension in the request is ignored", "ok")(
    mutual("chain", 1, SIG + ext(0x7a7a, b"opaque")))
# The last of 2,600 names is ours: the whole list is walked (a list of about 60 KB, nearly the slot's buffer).
many = [b"\x30\x0b\x31\x09\x30\x07\x06\x03\x55\x04\x03\x0c\x00" + i.to_bytes(4, "big") + b"x" * 4 for i in range(2600)]
case("client auth: 2,600 authorities, ours the last", "ok")(mutual("chain", 1, SIG + authorities(many + [CA_NAME])))
case("client auth: 2,600 authorities, none ours: the empty Certificate", "ok")(mutual("empty", 2, SIG + authorities(many)))

case("client auth: a chain of two certificates, each with an empty extension list", "ok")(
    mutual("chain", 1, SIG, extra=[CA[1]]))
# A chain whose second certificate was issued by another CA: naming that CA (and not the leaf's) in the request is answered
# with the chain, so every certificate's issuer is considered.
INTERMEDIATE = client_certificate(client_key("intermediate"), OTHER_CA, name="tls_liar intermediate", serial=388)
case("client auth: the authorities name only the second certificate's issuer", "ok")(
    mutual("chain", 1, SIG + authorities([OTHER_CA_NAME]), extra=[INTERMEDIATE]))
# The same length as CA_NAME, other bytes: only a byte-for-byte comparison tells them apart.
SAME_LENGTH = CA_NAME[:-1] + bytes([CA_NAME[-1] ^ 1])
case("client auth: an authorities name of the issuer's length and other bytes: empty Certificate", "ok")(
    mutual("empty", 2, SIG + authorities([SAME_LENGTH])))

# ---- TLS 1.3: a request it cannot answer is answered with the empty Certificate ----
case("client auth: no common signature scheme (Ed25519 only): empty Certificate", "ok")(
    mutual("empty", 2, sigalgs(ED25519)))
case("client auth: RSA-PSS only: empty Certificate", "ok")(mutual("empty", 2, sigalgs(RSA_PSS)))
case("client auth: no signature_algorithms at all: empty Certificate", "ok")(mutual("empty", 2, authorities([CA_NAME])))
case("client auth: the authorities name another CA only: empty Certificate", "ok")(
    mutual("empty", 2, SIG + authorities([OTHER_CA_NAME])))
case("client auth: no identity configured: empty Certificate", "ok")(mutual("empty", 2, SIG, identity=False))
case("client auth: the identity names another host: empty Certificate", "ok")(
    mutual("empty", 2, SIG, hosts=b"other.example"))


@case("no request: the client sends no Certificate and client_auth is 0", "ok")
def no_request(s):
    s.start()
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    assert s.query()[0] == 0, "client_auth"
    s.finish_honest()




@case("client auth: the server requires one, none configured: it ends with certificate_required", "tls-alert")
def required_without_identity(s):
    s.start(identity=False)
    s.cr_exts = SIG
    s.c.feed(s.hello_and_flight())
    s.client_flight("empty")
    s.c.feed(s.write.seal(21, b"\2\x74"))


# ---- TLS 1.3: a request that lies ----
case("client auth: a second CertificateRequest", "tls-unexpected-message", 10)(
    lambda s: (s.start(), setattr(s, "cr_exts", SIG), setattr(s, "cr_count", 2), s.c.feed(s.hello_and_flight())))
case("client auth: signature_algorithms twice", "tls-decode-error", 50)(
    lambda s: (s.start(), setattr(s, "cr_exts", SIG + sigalgs(ECDSA_SHA256)), s.c.feed(s.hello_and_flight())))
case("client auth: certificate_authorities twice", "tls-decode-error", 50)(
    lambda s: (s.start(), setattr(s, "cr_exts", SIG + authorities([CA_NAME]) + authorities([CA_NAME])),
               s.c.feed(s.hello_and_flight())))
case("client auth: an authorities list whose names do not tile it", "tls-decode-error", 50)(
    lambda s: (s.start(), setattr(s, "cr_exts", SIG + ext(47, u16(5) + u16(9) + b"abc")), s.c.feed(s.hello_and_flight())))
case("client auth: an authorities name of length 0", "tls-decode-error", 50)(
    lambda s: (s.start(), setattr(s, "cr_exts", SIG + ext(47, u16(2) + u16(0))), s.c.feed(s.hello_and_flight())))
case("client auth: a signature_algorithms list of odd length", "tls-decode-error", 50)(
    lambda s: (s.start(), setattr(s, "cr_exts", ext(13, u16(3) + b"\x04\x03\x08")), s.c.feed(s.hello_and_flight())))
case("client auth: a context longer than its length", "tls-decode-error", 50)(
    lambda s: (s.start(), setattr(s, "cr_raw", bytes([200]) + b"short"), s.c.feed(s.hello_and_flight())))
case("client auth: extensions longer than the message", "tls-decode-error", 50)(
    lambda s: (s.start(), setattr(s, "cr_raw", b"\0" + u16(400) + b"\0" * 8), s.c.feed(s.hello_and_flight())))


@case("client auth: a CertificateRequest after Finished", "tls-unexpected-message", 10, True)
def request_after_finished(s):
    s.start()
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    s.c.feed(s.write.seal(22, message(13, b"\0" + u16(len(SIG)) + SIG)))


@case("client auth: a CertificateRequest in a resumed handshake", "tls-unexpected-message", 10)
def request_when_resumed(s):
    psk = L.first_connection(s)
    r = L.resume(s, psk)
    r.c.feed(L.resumed_flight(r, after_ee=message(13, b"\0" + u16(len(SIG)) + SIG)))


# ---- TLS 1.3: ALPN ----
H2, H1 = b"h2", b"http/1.1"


def alpn_case(offer, answer, want, ee=True, **kw):
    def run(s):
        s.start(alpn=offer, identity=False, **kw)
        if ee:
            s.ee_extensions = answer
        s.c.feed(s.hello_and_flight())
        s.check_client_finished()
        assert s.query()[1] == want, "the protocol the client reports"
        s.finish_honest()
    return run


case("ALPN: offered h2 and http/1.1, the server picks http/1.1", "ok")(
    alpn_case("h2 http/1.1", alpn_ext(H1), H1))
case("ALPN: offered h2 and http/1.1, the server picks h2", "ok")(alpn_case("h2 http/1.1", alpn_ext(H2), H2))
case("ALPN: offered, the server answers none", "ok")(alpn_case("h2 http/1.1", b"", b""))
case("ALPN: nothing offered, nothing answered", "ok")(alpn_case(None, b"", b""))
case("ALPN: a 255-byte name offered and chosen", "ok")(alpn_case("p" * 255, alpn_ext(b"p" * 255), b"p" * 255))
case("ALPN: an unoffered selection (spdy/3)", "tls-alpn-selected", 47)(
    lambda s: (s.start(alpn="h2 http/1.1", identity=False), setattr(s, "ee_extensions", alpn_ext(b"spdy/3")),
               s.c.feed(s.hello_and_flight())))
case("ALPN: a selection that is a prefix of an offered name", "tls-alpn-selected", 47)(
    lambda s: (s.start(alpn="h2 http/1.1", identity=False), setattr(s, "ee_extensions", alpn_ext(b"http/1.")),
               s.c.feed(s.hello_and_flight())))
case("ALPN: a selection of an offered name's length and other bytes (h3 for h2)", "tls-alpn-selected", 47)(
    lambda s: (s.start(alpn="h2 http/1.1", identity=False), setattr(s, "ee_extensions", alpn_ext(b"h3")),
               s.c.feed(s.hello_and_flight())))
case("ALPN: a 255-byte selection never offered", "tls-alpn-selected", 47)(
    lambda s: (s.start(alpn="h2", identity=False), setattr(s, "ee_extensions", alpn_ext(b"q" * 255)),
               s.c.feed(s.hello_and_flight())))
case("ALPN: the answer names two protocols", "tls-decode-error", 50)(
    lambda s: (s.start(alpn="h2 http/1.1", identity=False), setattr(s, "ee_extensions", alpn_ext(H2, H1)),
               s.c.feed(s.hello_and_flight())))
case("ALPN: the answer's list length does not fit", "tls-decode-error", 50)(
    lambda s: (s.start(alpn="h2", identity=False), setattr(s, "ee_extensions", ext(16, u16(9) + b"\2h2")),
               s.c.feed(s.hello_and_flight())))
case("ALPN: the answer's name length does not fit", "tls-decode-error", 50)(
    lambda s: (s.start(alpn="h2", identity=False), setattr(s, "ee_extensions", ext(16, u16(3) + b"\5h2")),
               s.c.feed(s.hello_and_flight())))
case("ALPN: the answer names an empty protocol", "tls-decode-error", 50)(
    lambda s: (s.start(alpn="h2", identity=False), setattr(s, "ee_extensions", ext(16, u16(1) + b"\0")),
               s.c.feed(s.hello_and_flight())))
case("ALPN: the answer twice", "tls-decode-error", 50)(
    lambda s: (s.start(alpn="h2", identity=False), setattr(s, "ee_extensions", alpn_ext(H2) + alpn_ext(H2)),
               s.c.feed(s.hello_and_flight())))
case("ALPN: in the ServerHello of TLS 1.3, not EncryptedExtensions", "tls-unsupported-extension", 110)(
    lambda s: (s.start(alpn="h2", identity=False), setattr(s, "extra_extensions", alpn_ext(H2)),
               s.c.feed(plain_record(22, s.server_hello(s.sid)))))


@case("ALPN: after a HelloRetryRequest to P-384, the offer again and a selection", "ok")
def alpn_after_retry(s):
    s.start(alpn="h2 http/1.1", identity=False)
    s.suite = 0x1302
    s.retry(L.P384)
    s.ee_extensions = alpn_ext(H1)
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()
    assert s.query()[1] == H1
    s.finish_honest()


@case("ALPN and client auth together", "ok")
def alpn_and_auth(s):
    s.start(alpn="h2 http/1.1")
    s.cr_exts = SIG
    s.ee_extensions = alpn_ext(H2)
    s.c.feed(s.hello_and_flight())
    s.client_flight("chain")
    assert s.query() == (1, H2)
    s.finish_honest()


@case("ALPN: a 256-byte offer is refused when set", "tls-alpn-list", config=True)
def offer_too_long(s):
    s.c.ask(f"L {('x' * 255 + ' ' + 'y').encode().hex()}")


@case("ALPN: a name of 256 bytes is refused when set", "tls-alpn-list", config=True)
def name_too_long(s):
    s.c.ask(f"L {('x' * 256).encode().hex()}")


# ---- The identity's own refusals ----
def identity_case(name, tag, chain=None, key=None, hosts=HOST, now=NOW):
    @case(name, tag, config=True)
    def run(s):
        s.c.ask(f"I 0 {(chain or pem_cert(CERT)).hex()} {(key or pem_key(KEY)).hex()} {hosts.hex()} {now}")


identity_case("identity: a leaf that has expired", "tls-client-cert-expired", chain=pem_cert(EXPIRED))
identity_case("identity: the key is another leaf's", "tls-client-key-mismatch", key=pem_key(OTHER_CLIENT_KEY))
identity_case("identity: a key that is not PEM", "tls-client-key-format", key=b"not a key")
identity_case("identity: a P-384 key", "tls-client-key-type",
              key=pem_key(ec.derive_private_key(12345, ec.SECP384R1())))
identity_case("identity: no certificate in the chain", "tls-client-chain", chain=b"-----BEGIN NOPE-----\n")
identity_case("identity: no hosts", "tls-client-names", hosts=b"   ")
identity_case("identity: an Ed25519 leaf", "tls-client-key-type",
              chain=pem_cert(x509.load_der_x509_certificate(L.certificate("client ed", CA)[1])))


# ---- TLS 1.2 ----
class AuthServer12(Server12):
    """The TLS 1.2 server of `tls_liar`, asking for a certificate when `cr` is set."""

    def __init__(self, conv):
        super().__init__(conv)
        self.cr = None  # (certificate types, signature schemes, authorities)
        self.cr_raw = None
        self.client_pub = KEY.public_key()

    query = AuthServer.query
    finish_honest = AuthServer.finish_honest

    def start(self, alpn=None, identity=True, hosts=HOST, extra=()):
        prelude(self.c, alpn, identity, hosts, extra)
        self.chain = [CERT] + list(extra)
        super().start()
        check_offer(self.transcript, alpn)

    def cr_message(self):
        if self.cr_raw is not None:
            return message(13, self.cr_raw)
        types, schemes, cas = self.cr
        return message(13, bytes([len(types)]) + bytes(types) + u16(2 * len(schemes)) + b"".join(u16(x) for x in schemes)
                       + u16(sum(2 + len(n) for n in cas)) + b"".join(u16(len(n)) + n for n in cas))

    def flight12(self, middle=b""):
        """The base flight, with the request before ServerHelloDone; a message over a record's room is split."""
        msgs = [self.server_hello12(), self.certificate12(), self.key_exchange()]
        if self.cr is not None or self.cr_raw is not None:
            msgs.append(self.cr_message())
        msgs.append(middle + message(14, b""))
        self.transcript += b"".join(msgs)
        out = b""
        for m in msgs:
            for i in range(0, len(m), 16000):
                out += plain_record(22, m[i:i + 16000])
        return out

    def client_flight(self, expect=None):
        recs = self.c.take()
        h = self.hash
        asked = self.cr is not None or self.cr_raw is not None
        if asked:
            cert = recs.pop(0)
            assert cert[0] == 22 and cert[5] == 11, "the client's Certificate"
            if expect == "empty":
                assert cert == plain_record(22, message(11, u24(0))), "an empty Certificate"
            else:
                lst = cert[9:]
                assert int.from_bytes(lst[:3], "big") == len(lst) - 3
                at = 3
                for cert_in_chain in self.chain:
                    n = int.from_bytes(lst[at:at + 3], "big")
                    assert lst[at + 3:at + 3 + n] == cert_in_chain.public_bytes(serialization.Encoding.DER), "the identity's certificate"
                    at += 3 + n
                assert at == len(lst), "a list of the chain's certificates, with no extensions"
            self.transcript += cert[5:]
        cke = recs.pop(0)
        assert cke[0] == 22 and cke[5] == 16, cke[:6].hex()
        point = cke[10:]
        self.client_shares = {self.group: point}
        self.transcript += cke[5:]
        pms = self.shared_secret()
        master = prf(pms, b"extended master secret", h(self.transcript).digest(), 48, h)
        if expect == "chain":
            cv = recs.pop(0)
            assert cv[0] == 22 and cv[5] == 15, "CertificateVerify"
            assert int.from_bytes(cv[9:11], "big") == ECDSA_SHA256
            sig = cv[13:13 + int.from_bytes(cv[11:13], "big")]
            try:
                # RFC 5246 §7.4.8: over every handshake message so far, hashed with SHA-256 whatever the suite's.
                self.client_pub.verify(sig, self.transcript, ec.ECDSA(hashes.SHA256()))
            except InvalidSignature:
                raise AssertionError("the client's TLS 1.2 CertificateVerify does not verify")
            self.transcript += cv[5:]
        _, kl, il, _ = L.SUITES12[self.suite]
        block = prf(master, b"key expansion", self.server_random + self.client_random, 2 * kl + 2 * il, h)
        self.read = L.Keys12(self.suite, block[:kl], block[2 * kl:2 * kl + il])
        self.write = L.Keys12(self.suite, block[kl:2 * kl], block[2 * kl + il:])
        assert recs[0] == plain_record(20, b"\1"), "change_cipher_spec"
        kind, fin = self.read.open(recs[1])
        want = message(20, prf(master, b"client finished", h(self.transcript).digest(), 12, h))
        assert (kind, fin) == (22, want), "the client's Finished"
        self.transcript += fin
        self.master = master
        self.c.sent = b"".join(recs[2:])


def mutual12(expect, auth, cr, suite=0xc02b, **kw):
    def run(s):
        s.start(**kw)
        s.suite = suite
        s.cr = cr
        s.c.feed(s.flight12())
        s.client_flight(expect)
        assert s.query()[0] == auth, "client_auth"
        s.c.feed(s.server_finish())
        s.check_request()
        f = s.c.feed(s.write.seal(23, b"mutual 1.2") + s.write.seal(21, b"\1\0"))
        assert f[2] == "4", f"closed: {f}"
    return run


def case12(name, tag, alert=None, encrypted=False, config=False):
    return case(name, tag, alert, encrypted, AuthServer12, config)


ECDSA_SIGN, RSA_SIGN = 64, 1
case12("TLS 1.2 client auth: a chain, CertificateVerify over SHA-256 of the messages, AES-128-GCM", "ok")(
    mutual12("chain", 1, ([ECDSA_SIGN], [ECDSA_SHA256], [])))
case12("TLS 1.2 client auth: AES-256-GCM-SHA384, the signature still over SHA-256", "ok")(
    mutual12("chain", 1, ([ECDSA_SIGN], [ECDSA_SHA256], []), suite=0xc02c))
case12("TLS 1.2 client auth: ChaCha20-Poly1305 and the authorities name the issuer", "ok")(
    mutual12("chain", 1, ([RSA_SIGN, ECDSA_SIGN], [ED25519, ECDSA_SHA256], [CA_NAME]), suite=0xcca9))
case12("TLS 1.2 client auth: P-256 key exchange", "ok")(
    lambda s: (setattr(s, "group", P256), mutual12("chain", 1, ([ECDSA_SIGN], [ECDSA_SHA256], []))(s)))
case12("TLS 1.2 client auth: a chain of two certificates", "ok")(
    mutual12("chain", 1, ([ECDSA_SIGN], [ECDSA_SHA256], []), extra=[CA[1]]))
case12("TLS 1.2 client auth: the authorities name only the second certificate's issuer", "ok")(
    mutual12("chain", 1, ([ECDSA_SIGN], [ECDSA_SHA256], [OTHER_CA_NAME]), extra=[INTERMEDIATE]))
case12("TLS 1.2 client auth: ecdsa_sign is not a certificate type: empty Certificate", "ok")(
    mutual12("empty", 2, ([RSA_SIGN], [ECDSA_SHA256], [])))
case12("TLS 1.2 client auth: Ed25519 only: empty Certificate", "ok")(mutual12("empty", 2, ([ECDSA_SIGN], [ED25519], [])))
case12("TLS 1.2 client auth: the authorities name another CA: empty Certificate", "ok")(
    mutual12("empty", 2, ([ECDSA_SIGN], [ECDSA_SHA256], [OTHER_CA_NAME])))
case12("TLS 1.2 client auth: no identity configured: empty Certificate", "ok")(
    mutual12("empty", 2, ([ECDSA_SIGN], [ECDSA_SHA256], []), identity=False))
case12("TLS 1.2 client auth: 2,600 authorities, none ours, a message split over four records", "ok")(
    mutual12("empty", 2, ([ECDSA_SIGN], [ECDSA_SHA256], many)))
case12("TLS 1.2 client auth: a CertificateRequest with an authorities list that does not tile", "tls-decode-error", 50)(
    lambda s: (s.start(), setattr(s, "cr_raw", b"\1\x40" + u16(2) + u16(ECDSA_SHA256) + u16(5) + u16(9) + b"abc"),
               s.c.feed(s.flight12())))
case12("TLS 1.2 client auth: a signature_algorithms list of odd length", "tls-decode-error", 50)(
    lambda s: (s.start(), setattr(s, "cr_raw", b"\1\x40" + u16(3) + b"\4\3\0" + u16(0)), s.c.feed(s.flight12())))


@case12("TLS 1.2 client auth: a second CertificateRequest", "tls-unexpected-message", 10)
def second_request12(s):
    s.start()
    s.cr = ([ECDSA_SIGN], [ECDSA_SHA256], [])
    s.c.feed(s.flight12(middle=s.cr_message()))


@case12("TLS 1.2 client auth: a CertificateRequest after ServerHelloDone", "tls-unexpected-message", 10, True)
def request_after_done12(s):
    s.start()
    s.c.feed(s.flight12())
    s.client_flight()
    s.c.feed(plain_record(22, message(13, b"\1\x40" + u16(2) + u16(ECDSA_SHA256) + u16(0))))


@case12("TLS 1.2 ALPN: offered h2 and http/1.1, the server picks http/1.1", "ok")
def alpn12(s):
    s.start(alpn="h2 http/1.1", identity=False)
    s.sh_extra = alpn_ext(H1)
    s.c.feed(s.flight12())
    s.client_flight()
    assert s.query()[1] == H1
    s.c.feed(s.server_finish())
    s.check_request()
    f = s.c.feed(s.write.seal(23, b"alpn 1.2") + s.write.seal(21, b"\1\0"))
    assert f[2] == "4", f"closed: {f}"


@case12("TLS 1.2 ALPN: an unoffered selection", "tls-alpn-selected", 47)
def alpn12_unoffered(s):
    s.start(alpn="h2", identity=False)
    s.sh_extra = alpn_ext(b"spdy/3")
    s.c.feed(plain_record(22, s.server_hello12()))


@case12("TLS 1.2 ALPN: never offered", "tls-unsupported-extension", 110)
def alpn12_never(s):
    s.start(identity=False)
    s.sh_extra = alpn_ext(H2)
    s.c.feed(plain_record(22, s.server_hello12()))


@case12("TLS 1.2 ALPN: two protocols in the answer", "tls-decode-error", 50)
def alpn12_two(s):
    s.start(alpn="h2 http/1.1", identity=False)
    s.sh_extra = alpn_ext(H2, H1)
    s.c.feed(plain_record(22, s.server_hello12()))


@case12("TLS 1.2 ALPN: the answer twice", "tls-decode-error", 50)
def alpn12_twice(s):
    s.start(alpn="h2", identity=False)
    s.sh_extra = alpn_ext(H2) + alpn_ext(H2)
    s.c.feed(plain_record(22, s.server_hello12()))


def run_case(driver, name, tag, alert, encrypted, fn, server, config):
    conv = Conversation(driver)
    s = server(conv)
    try:
        fn(s)
        last = conv.answers[-1].split(" ")
        if last[1] != tag:
            raise Failed(f"ended {conv.answers[-1][:80]}, wanted {tag}")
        if tag != "ok" and not config:
            assert last[2] == "5", f"failed: {last[:3]}"
            if alert is not None:
                s.expect_alert(alert, encrypted)
    except (Failed, AssertionError) as e:
        conv.close()
        return False, f"{name}: {e}", conv.lines
    conv.close()
    return True, f"{name}: {tag}", conv.lines


def main():
    driver, out = sys.argv[1], sys.argv[2]
    lines = ["# scripts/tls_liar_auth.py: packages/tls against a server that lies about client certificates and ALPN.",
             "# `## <tag> <name>` starts a case; `=` lines are the client's answers."]
    bad = 0
    for name, tag, alert, encrypted, fn, server, config in CASES:
        ok, what, recorded = run_case(driver, name, tag, alert, encrypted, fn, server, config)
        print(("" if ok else "FAILED ") + what)
        bad += not ok
        lines += [f"## {tag} {name}"] + recorded
    open(out, "w").write("\n".join(lines) + "\n")
    print(f"{len(CASES)} cases, {bad} failed")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
