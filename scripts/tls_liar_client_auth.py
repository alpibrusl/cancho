#!/usr/bin/env python3
"""A TLS 1.3 client that presents certificates, and lies about them, against `packages/tls`'s server
(docs/tls-server.md §13, step 4).

    python3 scripts/tls_liar_client_auth.py <server driver> <out.txt>

The shape of `scripts/tls_liar_client.py`, whose client, conversation and fixtures it imports and extends: the same
server driver (`tests/programs/tls_server_driver.cho`, now with `G`, `H` and `P`), the same fixed seed, and a client
written on pyca/cryptography and RFC 8446 alone. Each case is one connection in which the client changes one thing:
one case per refusal rule of §13.2 to §13.4, and the honest ones (each key type, chains of one, two and three
certificates, the three RSA-PSS schemes, a HelloRetryRequest, the flight in one record and in several, `optional`
with and without a certificate, a store of two CAs, the store replaced after the connection). Every honest case asks
the server for the client's identity (`P`) and compares each field with what pyca reads from the same certificate:
the fingerprint, the subject's DER, the serial, `notAfter` and every subjectAltName entry.

Everything is fixed, so a second recording is identical byte for byte: the CAs are Ed25519 (deterministic), the client
keys come from fixed seeds (the RSA key from `tests/vectors/tls/client_auth/rsa2048.pem`), and the client's
signatures are deterministic too: ECDSA with a nonce hashed from the key and the message, RSA-PSS with a fixed salt,
both written here with Python integers, since pyca 41 cannot be asked for either.

For a refusal the server must end with the case's tag, failed (event 5), and have sent the alert the case names, under
its application key (it writes under it once its Finished is queued). The file holds each case's driver lines and
answers; `crates/cancho/tests/conformance/tls_client_auth.rs` replays every case on both backends and
`scripts/tls_server_mutants.py` runs each mutant against them. Exit status 1 on any difference.
"""
import hashlib
import hmac
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import tls_liar_client as base  # noqa: E402
from tls_liar_client import (CA, CA_KEY, CA_NAME, END, HOST, NOW_MS, P256, P384, SEED, START, X25519, Client,  # noqa: E402
                             Conversation, Failed, Keys, der, expand_label, derive, ext, key_pem,
                             message, pem, plain_record, run_case, seeded, sha256, u16, u24)
from cryptography import x509  # noqa: E402
from cryptography.hazmat.primitives import hashes, serialization  # noqa: E402
from cryptography.hazmat.primitives.asymmetric import ec, ed25519, padding, rsa, utils  # noqa: E402
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID  # noqa: E402
import datetime  # noqa: E402
import ipaddress  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CASES = []

# The CertificateRequest the server sends (docs/tls-server.md §13.2), written here from the RFC.
CV_SCHEMES = [0x0403, 0x0503, 0x0804, 0x0805, 0x0806, 0x0807]
CERT_SCHEMES = CV_SCHEMES + [0x0401, 0x0501, 0x0601]
EARLIER = START + datetime.timedelta(days=30)
# Not yet valid at the case's NOW_MS and at any real clock until 2031, so the live differential sees it too.
NOT_YET = datetime.datetime(2031, 1, 1, tzinfo=datetime.timezone.utc)


def case(name, tag, alert=None, where="ap"):
    def register(fn):
        CASES.append((name, tag, alert, where, fn))
        return fn
    return register


# ---- Fixtures: the clients' CAs and certificates ----

def ed_ca(name, constraints=None, path_length=None):
    key = ed25519.Ed25519PrivateKey.from_private_bytes(seeded("client ca " + name))
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "fleet " + name)])
    b = (x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key())
         .serial_number(1).not_valid_before(START).not_valid_after(END)
         .add_extension(x509.BasicConstraints(ca=True, path_length=path_length), True)
         .add_extension(x509.KeyUsage(False, False, False, False, False, True, True, False, False), True))
    if constraints is not None:
        b = b.add_extension(x509.NameConstraints(permitted_subtrees=constraints, excluded_subtrees=None), True)
    return key, subject, b.sign(key, None)


def ec_key(name, curve=ec.SECP256R1()):
    return ec.derive_private_key(int.from_bytes(seeded("client key " + name), "big"), curve)


def ed_key(name):
    return ed25519.Ed25519PrivateKey.from_private_bytes(seeded("client key " + name))


RSA_KEY = serialization.load_pem_private_key(
    open(os.path.join(ROOT, "tests/vectors/tls/client_auth/rsa2048.pem"), "rb").read(), None)


def client_cert(issuer, subject_cn, key, sans=None, not_before=START, not_after=END, eku=(ExtendedKeyUsageOID.CLIENT_AUTH,),
                key_usage=True, serial=17, ca=False, path_length=None, org=None):
    """A certificate signed by `issuer` = (key, name): deterministic, because every CA is Ed25519."""
    names = [x509.NameAttribute(NameOID.COMMON_NAME, subject_cn)]
    if org:
        names.insert(0, x509.NameAttribute(NameOID.ORGANIZATION_NAME, org))
    subject = x509.Name(names)
    b = (x509.CertificateBuilder().subject_name(subject).issuer_name(issuer[1]).public_key(key.public_key())
         .serial_number(serial).not_valid_before(not_before).not_valid_after(not_after))
    if ca:
        b = b.add_extension(x509.BasicConstraints(ca=True, path_length=path_length), True)
        b = b.add_extension(x509.KeyUsage(False, False, False, False, False, True, True, False, False), True)
    else:
        if key_usage is True:
            b = b.add_extension(x509.KeyUsage(True, False, False, False, False, False, False, False, False), True)
        elif key_usage is not False:
            b = b.add_extension(key_usage, True)
    if sans:
        b = b.add_extension(x509.SubjectAlternativeName(sans), False)
    if eku:
        b = b.add_extension(x509.ExtendedKeyUsage(list(eku)), False)
    return b.sign(issuer[0], None)


CLIENT_CA_KEY, CLIENT_CA_NAME, CLIENT_CA = ed_ca("one")
ROOT_ONE = (CLIENT_CA_KEY, CLIENT_CA_NAME)
SECOND_KEY, SECOND_NAME, SECOND_CA = ed_ca("two")
ROOT_TWO = (SECOND_KEY, SECOND_NAME)
UNKNOWN_KEY, UNKNOWN_NAME, UNKNOWN_CA = ed_ca("not in the store")
ROOT_UNKNOWN = (UNKNOWN_KEY, UNKNOWN_NAME)
CONSTRAINED_KEY, CONSTRAINED_NAME, CONSTRAINED_CA = ed_ca("constrained", constraints=[x509.DNSName("fleet.test")])
ROOT_CONSTRAINED = (CONSTRAINED_KEY, CONSTRAINED_NAME)

SANS = [x509.DNSName("device-17.fleet.test"), x509.UniformResourceIdentifier("spiffe://fleet.test/device/17"),
        x509.RFC822Name("ops@fleet.test"), x509.IPAddress(ipaddress.ip_address("10.17.0.17"))]
P256_KEY, P384_KEY, ED_KEY = ec_key("p256"), ec_key("p384", ec.SECP384R1()), ed_key("ed25519")
DEVICE = client_cert(ROOT_ONE, "device-17", P256_KEY, sans=SANS, serial=0x8099AABBCC, org="Fleet")
DEVICE_384 = client_cert(ROOT_ONE, "device-384", P384_KEY, sans=[x509.DNSName("d384.fleet.test")], serial=384)
DEVICE_RSA = client_cert(ROOT_ONE, "device-rsa", RSA_KEY, sans=[x509.DNSName("rsa.fleet.test")], serial=2048)
DEVICE_ED = client_cert(ROOT_ONE, "device-ed", ED_KEY, sans=[x509.DNSName("ed.fleet.test")], serial=25519)
DEVICE_NOSAN = client_cert(ROOT_ONE, "device-no-san", P256_KEY, serial=5)

INT_KEY = ed_key("intermediate")
INTERMEDIATE = client_cert(ROOT_ONE, "fleet intermediate", INT_KEY, ca=True, path_length=None, serial=3, eku=None)
ROOT_INT = (INT_KEY, INTERMEDIATE.subject)
VIA_INT = client_cert(ROOT_INT, "device-via", P256_KEY, sans=[x509.DNSName("via.fleet.test")], serial=18)
# Intermediates one under the other, for the path bound: the leaf under i4, i4 under i3, ... under the root.
DEEP = []
issuer = ROOT_ONE
for n in range(1, 5):
    k = ed_key(f"deep {n}")
    c = client_cert(issuer, f"deep {n}", k, ca=True, serial=100 + n, eku=None)
    DEEP.append(c)
    issuer = (k, c.subject)
DEEP_LEAF = client_cert(issuer, "device-deep", P256_KEY, sans=[x509.DNSName("deep.fleet.test")], serial=19)

OTHER_KEY = ec_key("other key")
EXPIRED = client_cert(ROOT_ONE, "device-expired", P256_KEY, not_after=EARLIER, serial=20)
NOT_YET_VALID = client_cert(ROOT_ONE, "device-future", P256_KEY, not_before=NOT_YET, serial=21)
SERVER_ONLY = client_cert(ROOT_ONE, "device-server-eku", P256_KEY, eku=(ExtendedKeyUsageOID.SERVER_AUTH,), serial=22)
NO_SIGNATURE_USAGE = client_cert(ROOT_ONE, "device-no-digital-signature", P256_KEY, serial=23,
                                 key_usage=x509.KeyUsage(False, True, False, False, False, False, False, False, False))
UNTRUSTED = client_cert(ROOT_UNKNOWN, "device-stranger", P256_KEY, serial=24)
CONSTRAINED_BAD = client_cert(ROOT_CONSTRAINED, "device-evil", P256_KEY, sans=[x509.DNSName("admin.other.test")], serial=26)
CONSTRAINED_OK = client_cert(ROOT_CONSTRAINED, "device-fine", P256_KEY, sans=[x509.DNSName("a.fleet.test")], serial=27)
SECOND_DEVICE = client_cert(ROOT_TWO, "device-of-the-second-ca", P256_KEY, sans=[x509.DNSName("two.fleet.test")], serial=28)
P521_KEY = ec.derive_private_key(int.from_bytes(seeded("client key p521"), "big") % (2 ** 520), ec.SECP521R1())
DEVICE_521 = client_cert(ROOT_ONE, "device-521", P521_KEY, serial=29)


def self_signed_leaf():
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "self-signed device")])
    key = ED_KEY
    return (x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(key.public_key()).serial_number(30)
            .not_valid_before(START).not_valid_after(END)
            .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.CLIENT_AUTH]), False).sign(key, None))


def self_signed_ca():
    """A device that is its own CA: self-signed, cA, keyCertSign beside digitalSignature, clientAuth."""
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "self-signed device that is a CA")])
    key = ED_KEY
    return (x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(key.public_key()).serial_number(31)
            .not_valid_before(START).not_valid_after(END)
            .add_extension(x509.BasicConstraints(ca=True, path_length=0), True)
            .add_extension(x509.KeyUsage(True, False, False, False, False, True, False, False, False), True)
            .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.CLIENT_AUTH]), False).sign(key, None))


SELF_SIGNED = self_signed_leaf()
SELF_CA = self_signed_ca()


def store(*cas):
    return b"".join(pem(c) for c in cas)


STORE_ONE = store(CLIENT_CA)


def long_ca(i, attributes):
    """A CA with a subject of `attributes` long attributes, so a store of 32 has names that do not all fit."""
    key = ed25519.Ed25519PrivateKey.from_private_bytes(seeded(f"long ca {i}"))
    types = [NameOID.COUNTRY_NAME, NameOID.ORGANIZATION_NAME, NameOID.ORGANIZATIONAL_UNIT_NAME, NameOID.LOCALITY_NAME,
             NameOID.STATE_OR_PROVINCE_NAME, NameOID.COMMON_NAME, NameOID.STREET_ADDRESS]
    names = []
    for t in types[:attributes]:
        names.append(x509.NameAttribute(t, ("ES" if t == NameOID.COUNTRY_NAME else f"{i:02d}" + "x" * 60)))
    subject = x509.Name(names)
    return (x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key())
            .serial_number(40 + i).not_valid_before(START).not_valid_after(END)
            .add_extension(x509.BasicConstraints(ca=True, path_length=None), True)
            .add_extension(x509.KeyUsage(False, False, False, False, False, True, True, False, False), True)
            .sign(key, None))


# ---- Signatures: deterministic, written with integers ----

ORDER = {256: 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551,
         384: 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFC7634D81F4372DDF581A0DB248B0A77AECEC196ACCC52973}


def det_ecdsa(key, hash_fn, data):
    n = ORDER[key.curve.key_size]
    e = int.from_bytes(hash_fn(data).digest(), "big")
    d = key.private_numbers().private_value
    size = (key.curve.key_size + 7) // 8
    ctr = 0
    while True:
        k = int.from_bytes(hashlib.sha512(b"k" + d.to_bytes(size, "big") + data + bytes([ctr])).digest(), "big") % (n - 1) + 1
        r = ec.derive_private_key(k, key.curve).public_key().public_numbers().x % n
        s = pow(k, -1, n) * (e + r * d) % n
        if r and s:
            return utils.encode_dss_signature(r, s)
        ctr += 1


def mgf1(seed, n, h):
    out = b""
    c = 0
    while len(out) < n:
        out += h(seed + c.to_bytes(4, "big")).digest()
        c += 1
    return out[:n]


def det_pss(key, hash_fn, data):
    """RSASSA-PSS (RFC 8017 §9.1.1), salt as long as the hash and fixed, MGF1 with the same hash."""
    h_len = hash_fn().digest_size
    bits = key.key_size - 1
    em_len = (bits + 7) // 8
    salt = hash_fn(b"salt" + data).digest()
    h = hash_fn(bytes(8) + hash_fn(data).digest() + salt).digest()
    db = bytes(em_len - 2 * h_len - 2) + b"\1" + salt
    masked = bytearray(a ^ b for a, b in zip(db, mgf1(h, em_len - h_len - 1, hash_fn)))
    masked[0] &= 0xFF >> (8 * em_len - bits)
    em = bytes(masked) + h + b"\xbc"
    nums = key.private_numbers()
    sig = pow(int.from_bytes(em, "big"), nums.d, nums.public_numbers.n)
    out = sig.to_bytes((key.key_size + 7) // 8, "big")
    key.public_key().verify(out, data, padding.PSS(padding.MGF1(hash_fn_class(hash_fn)), h_len), hash_fn_class(hash_fn))
    return out


def hash_fn_class(fn):
    return {hashlib.sha256: hashes.SHA256(), hashlib.sha384: hashes.SHA384(), hashlib.sha512: hashes.SHA512()}[fn]


def sign(key, scheme, data):
    if scheme == 0x0403 or scheme == 0x0503:
        return det_ecdsa(key, hashlib.sha256 if scheme == 0x0403 else hashlib.sha384, data)
    if scheme in (0x0804, 0x0805, 0x0806):
        return det_pss(key, {0x0804: hashlib.sha256, 0x0805: hashlib.sha384, 0x0806: hashlib.sha512}[scheme], data)
    if scheme == 0x0807:
        return key.sign(data)
    return bytes(64)  # a scheme with no signer here: any bytes, for a case that is refused before they are read


def default_scheme(key):
    if isinstance(key, ed25519.Ed25519PrivateKey):
        return 0x0807
    if isinstance(key, rsa.RSAPrivateKey):
        return 0x0804
    return 0x0403 if key.curve.key_size == 256 else 0x0503


# ---- The client ----

def expected_request(roots):
    """The CertificateRequest the server must send, from the RFC and the store's roots: the certificate_authorities
    only when their list is at most 8,192 bytes."""
    sigalgs = ext(13, u16(2 * len(CV_SCHEMES)) + b"".join(u16(s) for s in CV_SCHEMES))
    cert = ext(50, u16(2 * len(CERT_SCHEMES)) + b"".join(u16(s) for s in CERT_SCHEMES))
    names = b"".join(u16(len(c.subject.public_bytes())) + c.subject.public_bytes() for c in roots)
    authorities = ext(47, u16(len(names)) + names) if len(names) <= 8192 else b""
    exts = sigalgs + cert + authorities
    return message(13, b"\0" + u16(len(exts)) + exts)


def serial_bytes(n):
    b = n.to_bytes((n.bit_length() + 8) // 8, "big")
    return b


def san_text(cert):
    out = []
    try:
        san = cert.extensions.get_extension_for_class(x509.SubjectAlternativeName).value
    except x509.ExtensionNotFound:
        return "-"
    for name in san:
        if isinstance(name, x509.RFC822Name):
            out.append("129:" + name.value.encode().hex())
        elif isinstance(name, x509.DNSName):
            out.append("130:" + name.value.encode().hex())
        elif isinstance(name, x509.UniformResourceIdentifier):
            out.append("134:" + name.value.encode().hex())
        elif isinstance(name, x509.IPAddress):
            out.append("135:" + name.value.packed.hex())
    return ",".join(out) or "-"


class AuthClient(Client):
    """The honest client of `tls_liar_client`, asked for a certificate: `auth` says what it answers with."""

    def __init__(self, conv):
        super().__init__(conv)
        self.mode = 2
        self.store = STORE_ONE
        self.roots = [CLIENT_CA]
        self.asked = False
        # What the client sends; each key a case may change.
        self.auth = dict(chain=[DEVICE], key=P256_KEY)
        self.sigalgs = [0x0403, 0x0804, 0x0807]

    # ---- the server, configured ----
    def setup(self, alpn=None, identities=None, seed=SEED, store=None, mode=None, tickets=None, keys=None):
        f = self.c.ask(f"E {seed.hex()}")
        assert f[:2] == ["0", "ok"], f
        for chain, key, names in identities or [(base.CHAIN, key_pem(base.MAIN_KEY), HOST)]:
            f = self.c.ask(f"I {chain.hex()} {key.hex()} {names.hex()} {NOW_MS}")
            assert f[1] == "ok", f
        if alpn is not None:
            f = self.c.ask(f"A {alpn.hex()}")
            assert f[:2] == ["0", "ok"], f
        if tickets is not None:
            f = self.c.ask(f"T {tickets[0]} {tickets[1]}")
            assert f[:2] == ["0", "ok"], f
        if keys is not None:
            f = self.c.ask(f"K {b''.join(keys).hex()} {NOW_MS}")
            assert f[:2] == ["0", "ok"], f
        mode = self.mode if mode is None else mode
        if mode:
            f = self.c.ask(f"H {(self.store if store is None else store).hex()}")
            assert f[1] == "ok" and int(f[0]) == len(self.roots), f
            f = self.c.ask(f"G {mode}")
            assert f[:2] == ["0", "ok"], f
        f = self.c.ask(f"V {NOW_MS}")
        assert f[:3] == ["0", "ok", "1"], f

    # ---- the server's flight, with the request ----
    def flight(self):
        if self.expect_resume:
            # A resumed handshake asks for no certificate (RFC 8446 §4.3.2): the client of `tls_liar_client`.
            self.asked = False
            return Client.flight(self)
        recs = self.c.take()
        sh = recs[0][5:]
        assert recs[0][0] == 22
        _, self.suite, ks = self.parse_server_hello(sh)
        if self.expect_suite is not None:
            assert self.suite == self.expect_suite, f"suite {self.suite:#x}, wanted {self.expect_suite:#x}"
        group = int.from_bytes(ks[:2], "big")
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
        kinds = [m[0] for m in msgs]
        self.asked = self.mode != 0
        assert kinds == ([8, 13, 11, 15, 20] if self.asked else [8, 11, 15, 20]), kinds
        ee, *rest = msgs
        if self.asked:
            request, *rest = rest
            assert request == expected_request(self.roots), f"CertificateRequest {request.hex()}"
        else:
            request = b""
        cert, cv, fin = rest
        want = b"" if not self.expect_sni_ack else ext(0, b"")
        assert ee == message(8, u16(len(want)) + want), f"EncryptedExtensions {ee.hex()}"
        entries = b"".join(u24(len(c)) + c + u16(0) for c in self.expect_chain)
        assert cert == message(11, b"\0" + u24(len(entries)) + entries), "the chain as given"
        self.transcript += ee + request + cert
        sig = cv[8:]
        content = b" " * 64 + b"TLS 1.3, server CertificateVerify\0" + h(self.transcript).digest()
        self.expect_key.public_key().verify(sig, content, ec.ECDSA(hashes.SHA256()))
        self.transcript += cv
        key = expand_label(self.s_hs, b"finished", b"", len(self.s_hs), h)
        assert fin == message(20, hmac.new(key, h(self.transcript).digest(), h).digest()), "the server's Finished"
        self.transcript += fin
        self.app_th = self.transcript

    # ---- the client's flight ----
    def certificate_message(self, chain=None, context=b"", entry_ext=b""):
        chain = self.auth.get("chain") if chain is None else chain
        entries = b"".join(u24(len(der(c))) + der(c) + u16(len(entry_ext)) + entry_ext for c in chain)
        return message(11, bytes([len(context)]) + context + u24(len(entries)) + entries)

    def finish(self, ccs=None, extra=b""):
        if not self.asked:
            return super().finish(ccs, extra)
        return self.auth_flight(ccs=ccs, extra=extra)

    def auth_flight(self, ccs=None, extra=b"", empty=False, verify=True, certificate=None, scheme=None, key=None,
                    context=b"client", tamper=None, together=False, cert_first=True, finished_first=False,
                    transcript_for_signature=None, keys_after=True, raw=None, chunk=None):
        """The client's second flight: Certificate, CertificateVerify and Finished, each of which a case may change."""
        h = self.hash
        out = plain_record(20, b"\1") if (self.ccs if ccs is None else ccs) else b""
        msgs = []
        if raw is not None:
            msgs = raw
        else:
            if "chain" not in self.auth or self.auth["chain"] is None:
                empty = True
            cert = certificate if certificate is not None else (
                message(11, b"\0" + u24(0)) if empty else self.certificate_message())
            if finished_first:
                msgs.append(self.finished())
            else:
                self.transcript += cert
                msgs.append(cert)
                if verify and not empty:
                    k = key or self.auth["key"]
                    sc = scheme if scheme is not None else self.auth.get("scheme", default_scheme(k))
                    covered = transcript_for_signature if transcript_for_signature is not None else self.transcript
                    content = b" " * 64 + b"TLS 1.3, " + context + b" CertificateVerify\0" + h(covered).digest()
                    sig = sign(k, sc, content)
                    if tamper:
                        sig = tamper(sig)
                    cv = message(15, u16(sc) + u16(len(sig)) + sig)
                    self.transcript += cv
                    msgs.append(cv)
                self.client_finished = self.finished()
                msgs.append(self.client_finished)
        if chunk:
            data = b"".join(msgs)
            out += b"".join(self.write.seal(22, data[i:i + chunk]) for i in range(0, len(data), chunk))
        elif together:
            out += self.write.seal(22, b"".join(msgs))
        else:
            out += b"".join(self.write.seal(22, m) for m in msgs)
        out += extra
        f = self.c.feed(out)
        if keys_after:
            self.read = Keys(derive(self.master, b"s ap traffic", self.app_th, h), self.suite)
            self.write = Keys(derive(self.master, b"c ap traffic", self.app_th, h), self.suite)
            if f[2] == "3" and raw is None:
                self.collect_tickets()
        return f

    def collect_tickets(self):
        """The NewSessionTickets the server sent after the client's Finished (RFC 8446 §4.6.1), as `tls_liar_client` reads them."""
        h = self.hash
        res = derive(self.master, b"res master", self.transcript + self.client_finished, h)
        self.tickets = []
        for rec in self.c.take():
            kind, content = self.read.open(rec)
            assert kind == 22 and content[0] == 4, (kind, content[:4].hex())
            body = content[4:]
            nonce = body[9:9 + body[8]]
            at = 9 + body[8]
            size = int.from_bytes(body[at:at + 2], "big")
            self.tickets.append({"lifetime": int.from_bytes(body[0:4], "big"), "age_add": int.from_bytes(body[4:8], "big"),
                                 "nonce": nonce, "ticket": body[at + 2:at + 2 + size],
                                 "psk": expand_label(res, b"resumption", nonce, len(res), h), "hash": len(res),
                                 "suite": self.suite})

    # ---- what the program is given ----
    def identity(self, cert, generation=1, engine=1):
        f = self.c.ask("P")
        assert f[:2] == ["0", "ok"], f
        if cert is None:
            assert f[2:] == [str(0), str(engine), "-", "-", "-", "0", "-", "-"], f"no identity: {f}"
            return
        d = der(cert)
        want = [str(generation), str(engine), sha256(d).hex(), cert.subject.public_bytes().hex(),
                serial_bytes(cert.serial_number).hex(), str(int(cert.not_valid_after.replace(tzinfo=datetime.timezone.utc).timestamp())), d.hex(),
                san_text(cert)]
        assert f[2:] == want, f"identity\n got {f[2:]}\nwant {want}"

    def honest_auth(self, cert=True, **setup):
        """A whole honest connection, and the identity the program is given before it closes."""
        self.setup(**setup)
        self.send_hello()
        self.flight()
        f = self.finish()
        assert f[2] == "3", f"established: {f[:3]}"
        self.identity(self.auth["chain"][0] if cert is True and self.auth.get("chain") else cert or None,
                      engine=1 if self.mode else 0)
        f = self.c.feed(self.write.seal(23, b"GET / HTTP/1.0\r\n\r\n"))
        assert self.c.received.endswith(b"GET / HTTP/1.0\r\n\r\n"), "the request received"
        f = self.c.feed(self.write.seal(21, b"\1\0"))
        f = self.c.ask("Q")
        assert f[2] == "4", f"closed: {f[:3]}"
        return self.c.ask("N")

    def refused(self, **flight):
        """The flight up to the client's reply, which the case changes; the server must refuse it."""
        self.setup()
        self.send_hello()
        self.flight()
        self.auth_flight(**flight)


# ---- Honest: each key type, each chain ----

@case("honest: required, P-256 key, the leaf alone (the root is the store's)", "ok", None, None)
def honest_p256(c):
    c.auth = dict(chain=[DEVICE], key=P256_KEY)
    c.setup()
    c.send_hello()
    c.flight()
    n = c.c.ask("N")
    assert n[2:] == ["1", HOST.hex(), "-", "1"], f"the flight sent, waiting for the client's certificate, still in progress: {n}"
    f = c.finish()
    assert f[2] == "3", f
    c.identity(DEVICE)
    n = c.c.ask("N")
    assert n[2:] == ["3", HOST.hex(), "-", "0"], f"established, none in progress: {n}"


@case("honest: required, P-256 key, a chain of two (leaf and intermediate)", "ok", None, None)
def honest_two(c):
    c.auth = dict(chain=[VIA_INT, INTERMEDIATE], key=P256_KEY)
    c.honest_auth(VIA_INT)


@case("honest: required, a chain of three (leaf, intermediate and the root)", "ok", None, None)
def honest_three(c):
    c.auth = dict(chain=[VIA_INT, INTERMEDIATE, CLIENT_CA], key=P256_KEY)
    c.honest_auth(VIA_INT)


@case("honest: required, P-384 key, ecdsa_secp384r1_sha384, AES-256-GCM", "ok", None, None)
def honest_p384(c):
    c.suites, c.expect_suite = [0x1302], 0x1302
    c.auth = dict(chain=[DEVICE_384], key=P384_KEY)
    c.honest_auth(DEVICE_384)


for scheme, label in ((0x0804, "rsa_pss_rsae_sha256"), (0x0805, "rsa_pss_rsae_sha384"), (0x0806, "rsa_pss_rsae_sha512")):
    def make(scheme=scheme):
        def run(c):
            c.auth = dict(chain=[DEVICE_RSA], key=RSA_KEY, scheme=scheme)
            c.honest_auth(DEVICE_RSA)
        return run
    CASES.append((f"honest: required, RSA-2048 key, {label}", "ok", None, None, make()))


@case("honest: required, Ed25519 key", "ok", None, None)
def honest_ed(c):
    c.auth = dict(chain=[DEVICE_ED], key=ED_KEY)
    c.honest_auth(DEVICE_ED)


@case("honest: required, a leaf with four kinds of subjectAltName, a serial with its top bit set, an organization", "ok",
      None, None)
def honest_sans(c):
    c.auth = dict(chain=[DEVICE], key=P256_KEY)
    c.honest_auth(DEVICE)


@case("honest: required, a leaf with no subjectAltName: the subject only", "ok", None, None)
def honest_no_san(c):
    c.auth = dict(chain=[DEVICE_NOSAN], key=P256_KEY)
    c.honest_auth(DEVICE_NOSAN)


@case("honest: required, the Certificate, CertificateVerify and Finished in one record", "ok", None, None)
def honest_together(c):
    c.setup()
    c.send_hello()
    c.flight()
    f = c.auth_flight(together=True)
    assert f[2] == "3", f
    c.identity(DEVICE)


@case("honest: required, a HelloRetryRequest first", "ok", None, None)
def honest_retry(c):
    c.groups, c.share_groups = [base.P521, P256], [base.P521]
    c.expect_group = P256
    c.setup()
    c.send_hello()
    c.retry()
    c.flight()
    f = c.finish()
    assert f[2] == "3", f
    c.identity(DEVICE)


@case("honest: required, ChaCha20-Poly1305, no change_cipher_spec, an empty session id", "ok", None, None)
def honest_chacha(c):
    c.suites, c.expect_suite = [0x1303], 0x1303
    c.ccs, c.sid = False, b""
    c.honest_auth(DEVICE)


@case("honest: optional, a certificate sent: the identity is given", "ok", None, None)
def honest_optional(c):
    c.mode = 1
    c.honest_auth(DEVICE)


@case("honest: optional, an empty Certificate: accepted, no identity", "ok", None, None)
def honest_optional_none(c):
    c.mode = 1
    c.auth = dict(chain=None)
    c.honest_auth(False)


@case("honest: off, no request: every byte as before, no identity", "ok", None, None)
def honest_off(c):
    c.mode = 0
    c.honest_auth(False)


@case("honest: a store of two CAs, a certificate of the second", "ok", None, None)
def honest_two_cas(c):
    c.store, c.roots = store(CLIENT_CA, SECOND_CA), [CLIENT_CA, SECOND_CA]
    c.auth = dict(chain=[SECOND_DEVICE], key=P256_KEY)
    c.honest_auth(SECOND_DEVICE)


def pad_ca(length):
    """A CA whose subject is a common name of `length` characters."""
    key = ed25519.Ed25519PrivateKey.from_private_bytes(seeded(f"pad ca {length}"))
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "p" * length)])
    return (x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key())
            .serial_number(90 + length).not_valid_before(START).not_valid_after(END)
            .add_extension(x509.BasicConstraints(ca=True, path_length=None), True)
            .add_extension(x509.KeyUsage(False, False, False, False, False, True, True, False, False), True)
            .sign(key, None))


def names_size(cas):
    return sum(2 + len(x.subject.public_bytes()) for x in cas)


def names_at_the_limit(over=0):
    """32 CAs or fewer (the client CA among them) whose names, each after its 2-byte length, are `8192 + over` bytes:
    `n` long names, the client CA's, and two padding CAs whose common names are `a` and `b` characters long."""
    long_size, client_size = names_size([long_ca(0, 5)]), names_size([CLIENT_CA])
    for n in range(1, 30):
        for a in range(1, 65):
            for b in range(1, 65):
                if n * long_size + client_size + (15 + a) + (15 + b) == 8192 + over:
                    return [long_ca(i, 5) for i in range(n)] + [CLIENT_CA, pad_ca(a), pad_ca(b)]
    raise AssertionError("no store of that many bytes of names")


@case("honest: a store whose names are exactly 8,192 bytes: certificate_authorities at its limit", "ok", None, None)
def honest_names_at_the_limit(c):
    cas = names_at_the_limit()
    c.store, c.roots = b"".join(pem(x) for x in cas), cas
    c.honest_auth(DEVICE)


@case("honest: a store whose names are 8,193 bytes: certificate_authorities left out", "ok", None, None)
def honest_names_one_over(c):
    cas = names_at_the_limit(over=1)
    assert names_size(cas) == 8193 and len(cas) <= 32
    c.store, c.roots = b"".join(pem(x) for x in cas), cas
    c.honest_auth(DEVICE)


@case("honest: a name constraint the SAN satisfies", "ok", None, None)
def honest_constrained(c):
    c.store, c.roots = store(CONSTRAINED_CA), [CONSTRAINED_CA]
    c.auth = dict(chain=[CONSTRAINED_OK], key=P256_KEY)
    c.honest_auth(CONSTRAINED_OK)


@case("honest: the store replaced after the connection: the identity keeps the generation it was verified under", "ok",
      None, None)
def honest_replaced_after(c):
    c.setup()
    c.send_hello()
    c.flight()
    f = c.finish()
    assert f[2] == "3", f
    f = c.c.ask(f"H {store(CLIENT_CA, SECOND_CA).hex()}")
    assert f[:2] == ["2", "ok"], f
    c.identity(DEVICE, generation=1, engine=2)


@case("honest: a device that is its own CA (self-signed, cA) and is in the store", "ok", None, None)
def honest_own_ca(c):
    c.store, c.roots = store(SELF_CA), [SELF_CA]
    c.auth = dict(chain=[SELF_CA], key=ED_KEY)
    c.honest_auth(SELF_CA)


# ---- The client says it has no certificate, or sends the wrong message ----

@case("required, an empty Certificate", "tls-server-client-cert-required", 116)
def empty_required(c):
    c.refused(empty=True)


@case("required, Finished where the Certificate belongs", "tls-unexpected-message", 10)
def finished_for_certificate(c):
    c.refused(finished_first=True)


@case("required, a CertificateVerify where the Certificate belongs", "tls-unexpected-message", 10)
def verify_without_certificate(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.auth_flight(raw=[message(15, u16(0x0403) + u16(2) + b"\0\0")])


@case("required, a Certificate and no CertificateVerify: Finished next", "tls-unexpected-message", 10)
def certificate_without_verify(c):
    c.refused(verify=False)


@case("optional, an empty Certificate and then a CertificateVerify", "tls-unexpected-message", 10)
def empty_then_verify(c):
    c.mode = 1
    c.setup()
    c.send_hello()
    c.flight()
    cert = message(11, b"\0" + u24(0))
    c.transcript += cert
    c.auth_flight(raw=[cert, message(15, u16(0x0403) + u16(2) + b"\0\0")])


@case("a second Certificate where the CertificateVerify belongs", "tls-unexpected-message", 10)
def duplicate_certificate(c):
    c.setup()
    c.send_hello()
    c.flight()
    cert = c.certificate_message()
    c.transcript += cert
    c.auth_flight(raw=[cert, cert])


@case("a Certificate after the handshake (post-handshake authentication is not offered)", "tls-unexpected-message", 10)
def post_handshake(c):
    c.setup()
    c.send_hello()
    c.flight()
    f = c.finish()
    assert f[2] == "3", f
    c.c.feed(c.write.seal(22, c.certificate_message()))


@case("memory: a slot is what it was (10,241 words and 187,191 bytes); the engine's configuration grew by the clients' store (and the tickets' 640)",
      "ok", None, None)
def memory(c):
    # A slot's `ints` and `bytes` are the same as on main before client certificates (the client's identity is kept in
    # `b_leaf`, the server's own leaf a client slot keeps and a server slot did not use). `cfg` was 280,690 bytes on main; the
    # tickets (docs/tls-server.md §12.10) gave each identity a fingerprint and a notAfter, 16 x 40 = 640 bytes; this gained
    # the mode (1), the generation (4), the store's length (3) and the store (32,768) = 32,776.
    f = c.c.ask("L")
    assert f == ["0", "ok", "10241", "187191", str(280690 + 640 + 32776)], f


@case("peer_certificate into a buffer shorter than the certificate", "ok", None, None)
def short_buffer(c):
    c.setup()
    c.send_hello()
    c.flight()
    f = c.finish()
    assert f[2] == "3", f
    n = len(der(DEVICE))
    assert c.c.ask(f"B {n - 1}") == ["-71", "tls-server-peer-buffer"]
    assert c.c.ask(f"B {n}") == [str(n), "ok"]
    assert c.c.ask("B 0") == ["-71", "tls-server-peer-buffer"]


@case("a malformed record after establishing: the connection fails and the identity is no longer given",
      "tls-unexpected-message", 10, None)
def identity_after_failure(c):
    c.setup()
    c.send_hello()
    c.flight()
    f = c.finish()
    assert f[2] == "3", f
    c.identity(DEVICE)
    c.c.feed(c.write.seal(22, message(13, b"\0")))
    f = c.c.ask("P")
    assert f[2:] == ["0", "1", "-", "-", "-", "0", "-", "-"], f"no identity from a failed connection: {f}"


@case("off: an unsolicited Certificate", "tls-unexpected-message", 10)
def unsolicited(c):
    c.mode = 0
    c.setup()
    c.send_hello()
    c.flight()
    c.ccs = True
    c.auth_flight(raw=[c.certificate_message(), c.finished()], keys_after=True)


# ---- The Certificate message ----

@case("a Certificate with a request context of one byte", "tls-server-illegal-parameter", 47)
def context(c):
    c.refused(certificate=c.certificate_message(context=b"\1"))


@case("a CertificateEntry with an extension", "tls-unsupported-extension", 110)
def entry_extension(c):
    c.refused(certificate=c.certificate_message(entry_ext=ext(5, b"\1\0\0\0\0")))


@case("a Certificate whose list is cut short", "tls-decode-error", 50)
def cut(c):
    d = der(DEVICE)
    entries = u24(len(d)) + d[:-5]
    c.refused(certificate=message(11, b"\0" + u24(len(entries)) + entries))


@case("a Certificate whose length is wrong", "tls-decode-error", 50)
def wrong_length(c):
    m = bytearray(c.certificate_message())
    m[5] ^= 1
    c.refused(certificate=bytes(m))


@case("a certificate that is not DER", "x509-decode", 42)
def garbage(c):
    entries = u24(40) + seeded("not DER") + bytes(8) + u16(0)
    c.refused(certificate=message(11, b"\0" + u24(len(entries)) + entries))


@case("six certificates (over the bound of five)", "x509-chain-too-large", 42)
def six(c):
    c.auth = dict(chain=[DEEP_LEAF] + DEEP[::-1] + [CLIENT_CA], key=P256_KEY)
    c.refused()


@case("a Certificate message over 16 KiB", "x509-chain-too-large", 42)
def over_16k(c):
    entries = u24(16500) + bytes(16500) + u16(0)
    c.setup()
    c.send_hello()
    c.flight()
    c.auth_flight(raw=[message(11, b"\0" + u24(len(entries)) + entries)], chunk=10000)


@case("a Certificate of exactly 16 KiB + 1 in its header, nothing after it", "x509-chain-too-large", 42)
def header_only(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.auth_flight(raw=[bytes([11]) + u24(16385)])


@case("an intermediate more than the bound of three: leaf, four intermediates", "x509-path-too-long", 42)
def too_deep(c):
    c.auth = dict(chain=[DEEP_LEAF] + DEEP[::-1], key=P256_KEY)
    c.refused()


@case("a P-521 key (no verifier for it)", "x509-unsupported-algorithm", 43)
def p521_key(c):
    c.auth = dict(chain=[DEVICE_521], key=P521_KEY)
    c.refused(verify=False, certificate=c.certificate_message([DEVICE_521]))


# ---- The chain ----

@case("a leaf of a CA that is not in the store", "x509-unknown-issuer", 48)
def untrusted(c):
    c.auth = dict(chain=[UNTRUSTED], key=P256_KEY)
    c.refused()


@case("a self-signed leaf that is not in the store", "x509-unknown-issuer", 48)
def self_signed(c):
    c.auth = dict(chain=[SELF_SIGNED], key=ED_KEY)
    c.refused()


@case("a self-signed leaf that is not a CA, pinned in the store: a store holds CAs", "x509-not-ca", 42)
def pinned_leaf(c):
    c.store, c.roots = store(SELF_SIGNED), [SELF_SIGNED]
    c.auth = dict(chain=[SELF_SIGNED], key=ED_KEY)
    c.refused()


@case("optional, a certificate of a CA that is not in the store: refused, not ignored", "x509-unknown-issuer", 48)
def optional_untrusted(c):
    c.mode = 1
    c.auth = dict(chain=[UNTRUSTED], key=P256_KEY)
    c.refused()


@case("an expired leaf", "x509-expired", 45)
def expired(c):
    c.auth = dict(chain=[EXPIRED], key=P256_KEY)
    c.refused()


@case("a leaf that is not yet valid", "x509-not-yet-valid", 45)
def not_yet(c):
    c.auth = dict(chain=[NOT_YET_VALID], key=P256_KEY)
    c.refused()


@case("the engine's clock moved past the leaf's notAfter after serve (set_time): expired", "x509-expired", 45)
def clock_moved(c):
    c.setup()
    c.send_hello()
    c.flight()
    late = int((END + datetime.timedelta(days=200)).timestamp() * 1000)
    f = c.c.ask(f"M {late}")
    assert f[:2] == ["0", "ok"], f
    c.auth_flight()


@case("a leaf whose extendedKeyUsage is serverAuth only", "x509-key-usage", 43)
def server_eku(c):
    c.auth = dict(chain=[SERVER_ONLY], key=P256_KEY)
    c.refused()


@case("a leaf whose keyUsage lacks digitalSignature", "x509-key-usage", 43)
def key_usage(c):
    c.auth = dict(chain=[NO_SIGNATURE_USAGE], key=P256_KEY)
    c.refused()


@case("a SAN outside the CA's name constraint", "x509-name-constraint", 42)
def constraint(c):
    c.store, c.roots = store(CONSTRAINED_CA), [CONSTRAINED_CA]
    c.auth = dict(chain=[CONSTRAINED_BAD], key=P256_KEY)
    c.refused()


@case("the store replaced between the request and the Certificate: the old CA no longer vouches", "x509-unknown-issuer",
      48)
def replaced_between(c):
    c.setup()
    c.send_hello()
    c.flight()
    f = c.c.ask(f"H {store(SECOND_CA).hex()}")
    assert f[:2] == ["1", "ok"], f
    c.auth_flight()


# ---- The CertificateVerify ----

@case("a CertificateVerify signature with a bit flipped", "tls-bad-certificate-verify", 51)
def flipped(c):
    c.refused(tamper=lambda s: s[:-1] + bytes([s[-1] ^ 1]))


@case("a CertificateVerify under the server's context string", "tls-bad-certificate-verify", 51)
def server_context(c):
    c.refused(context=b"server")


@case("a CertificateVerify over the transcript before the client's Certificate", "tls-bad-certificate-verify", 51)
def old_transcript(c):
    c.setup()
    c.send_hello()
    c.flight()
    c.auth_flight(transcript_for_signature=c.transcript)


@case("a CertificateVerify signed by another key than the certificate's", "tls-bad-certificate-verify", 51)
def other_key(c):
    c.refused(key=OTHER_KEY)


@case("a scheme the request did not list (rsa_pkcs1_sha256, which is listed for certificates only)",
      "tls-server-client-sigalg", 47)
def unlisted(c):
    c.refused(scheme=0x0401)


@case("a scheme the request listed that the key does not fit (a P-256 key under ecdsa_secp384r1_sha384)",
      "tls-bad-certificate-verify", 51)
def mismatched(c):
    c.refused(scheme=0x0503)


@case("an RSA key under a PKCS#1 scheme (forbidden in TLS 1.3)", "tls-server-client-sigalg", 47)
def rsa_pkcs1(c):
    c.auth = dict(chain=[DEVICE_RSA], key=RSA_KEY)
    c.refused(scheme=0x0401, key=RSA_KEY)


@case("an Ed25519 key under an ECDSA scheme", "tls-bad-certificate-verify", 51)
def ed_under_ecdsa(c):
    c.auth = dict(chain=[DEVICE_ED], key=ED_KEY)
    c.refused(scheme=0x0403, key=P256_KEY)


@case("a client Finished with a wrong MAC after a good Certificate and CertificateVerify", "tls-server-finished", 51)
def bad_finished(c):
    c.setup()
    c.send_hello()
    c.flight()
    cert = c.certificate_message()
    c.transcript += cert
    k = c.auth["key"]
    content = b" " * 64 + b"TLS 1.3, client CertificateVerify\0" + c.hash(c.transcript).digest()
    sig = det_ecdsa(k, hashlib.sha256, content)
    cv = message(15, u16(0x0403) + u16(len(sig)) + sig)
    c.transcript += cv
    bad = bytearray(c.finished())
    bad[-1] ^= 1
    c.auth_flight(raw=[cert, cv, bytes(bad)])


@case("a Finished that leaves out the client's Certificate from its transcript", "tls-server-finished", 51)
def finished_without_certificate(c):
    c.setup()
    c.send_hello()
    c.flight()
    early = c.finished()
    cert = c.certificate_message()
    c.transcript += cert
    k = c.auth["key"]
    content = b" " * 64 + b"TLS 1.3, client CertificateVerify\0" + c.hash(c.transcript).digest()
    sig = det_ecdsa(k, hashlib.sha256, content)
    cv = message(15, u16(0x0403) + u16(len(sig)) + sig)
    c.auth_flight(raw=[cert, cv, early])


@case("a flight replayed from another connection (its Certificate and CertificateVerify, under this one's keys)",
      "tls-bad-certificate-verify", 51)
def replay(c):
    # The first connection, whose Certificate and CertificateVerify are kept.
    other = Conversation(c.c.proc.args[0])
    first = AuthClient(other)
    first.random, first.sid = seeded("random of the first"), seeded("session id of the first")
    first.setup()
    first.send_hello()
    first.flight()
    cert = first.certificate_message()
    first.transcript += cert
    content = b" " * 64 + b"TLS 1.3, client CertificateVerify\0" + first.hash(first.transcript).digest()
    sig = det_ecdsa(P256_KEY, hashlib.sha256, content)
    cv = message(15, u16(0x0403) + u16(len(sig)) + sig)
    other.close()
    # The second, with another client random, so another transcript.
    c.setup()
    c.send_hello()
    c.flight()
    c.transcript += cert
    c.auth_flight(raw=[cert, cv, c.finished()])


# ---- The configuration ----

def config_case(name, tag, lines):
    def run(c):
        c.c.ask(f"E {SEED.hex()}")
        answers = [c.c.ask(line) for line in lines]
        assert answers[-1][1] == tag, f"{answers[-1][:2]}, wanted {tag}"
    CASES.append((name, tag, None, None, run))


ID_LINE = f"I {base.CHAIN.hex()} {key_pem(base.MAIN_KEY).hex()} {HOST.hex()} {NOW_MS}"
config_case("a mode other than 0, 1 and 2", "tls-server-client-auth-config", [ID_LINE, "G 3"])
config_case("a mode of 1 asked for with no store", "tls-server-client-store", [ID_LINE, "G 1"])
config_case("a mode of 2 asked for with no store", "tls-server-client-store", [ID_LINE, "G 2"])
config_case("a bundle with no certificate", "tls-server-client-store", [ID_LINE, f"H {b'hello'.hex()}"])
config_case("a bundle with a private key among its blocks", "tls-server-client-store",
            [ID_LINE, f"H {(pem(CLIENT_CA) + key_pem(P256_KEY)).hex()}"])
config_case("a bundle of 33 roots", "tls-server-client-store",
            [ID_LINE, f"H {(b''.join(pem(CLIENT_CA) for _ in range(33))).hex()}"])
config_case("a bundle of 32 roots is taken", "ok", [ID_LINE, f"H {(b''.join(pem(CLIENT_CA) for _ in range(32))).hex()}"])


@case("the client-certificate calls on a client engine are all refused", "ok", None, None)
def roles(c):
    f = c.c.ask("Y")
    assert f == ["-57", "tls-role"] * 8, f


@case("a refused bundle leaves the store as it was: the generation does not move and a client is still verified", "ok",
      None, None)
def refused_keeps(c):
    c.setup()
    f = c.c.ask(f"H {pem(CLIENT_CA).hex()}")
    assert f[:2] == ["1", "ok"]  # the second load: the generation is 2
    f = c.c.ask(f"H {(pem(CLIENT_CA) + b'-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----').hex()}")
    assert f[1] == "tls-server-client-store", f
    c.send_hello()
    c.flight()
    f = c.finish()
    assert f[2] == "3", f
    c.identity(DEVICE, generation=2, engine=2)


# ---- Session tickets (docs/tls-server.md §12) and client certificates (§13.8) ----

TICKET_KEY = seeded("ticket key for client certificates")


def tickets_on(c, **kw):
    c.setup(tickets=(2, 3600), keys=[TICKET_KEY], **kw)


def second_connection(c, before=()):
    """The slot dropped, the engine reconfigured by `before`, a new connection served: a client for it."""
    f = c.c.ask("D")
    assert f[:2] == ["0", "ok"], f
    for line in before:
        f = c.c.ask(line)
        assert f[:2] == ["0", "ok"], (line, f)
    f = c.c.ask(f"V {NOW_MS + 5000}")
    assert f[:3] == ["0", "ok", "1"], f
    d = AuthClient(c.c)
    d.mode, d.store, d.roots, d.auth = c.mode, c.store, c.roots, c.auth
    return d


def offer(d, t):
    d.psks.append((t["ticket"], t["psk"], t["hash"], (5000 + t["age_add"]) % 2 ** 32))


@case("tickets, required: the client that presented a certificate is sent no ticket (a resumption would not say who it was)",
      "ok", None, None)
def no_ticket_for_an_authenticated_client(c):
    tickets_on(c)
    c.send_hello()
    c.flight()
    f = c.finish()
    assert f[2] == "3", f
    assert c.c.take() == [], "a client authenticated by a certificate was sent a NewSessionTicket"
    c.identity(DEVICE)


@case("tickets, optional: a client with no certificate is sent tickets, and one resumes: no CertificateRequest, no identity",
      "ok", None, None)
def anonymous_resumption(c):
    c.mode = 1
    c.auth = dict(chain=None)
    tickets_on(c)
    c.send_hello()
    c.flight()
    f = c.finish()
    assert f[2] == "3", f
    assert len(c.tickets) == 2, f"{len(c.tickets)} tickets for an anonymous client"
    d = second_connection(c)
    d.expect_resume = True
    offer(d, c.tickets[1])
    d.send_hello()
    d.flight()
    f = d.finish()
    assert f[2] == "3", f
    assert d.c.ask("U")[:3] == ["0", "ok", "1"], "resumed"
    d.identity(None)


@case("tickets, required: a ticket from an optional connection is refused, the client presents its certificate and is "
      "identified", "ok", None, None)
def required_refuses_a_ticket(c):
    c.mode = 1
    c.auth = dict(chain=None)
    tickets_on(c)
    c.send_hello()
    c.flight()
    f = c.finish()
    assert f[2] == "3", f
    assert len(c.tickets) == 2
    d = second_connection(c, before=["G 2"])
    d.mode, d.auth = 2, dict(chain=[DEVICE], key=P256_KEY)
    offer(d, c.tickets[0])
    d.send_hello()
    d.flight()
    u = d.c.ask("U")
    assert u[:3] == ["0", "ok", "0"] and u[4] == "tls-ticket-auth", f"not resumed, verdict auth: {u}"
    assert d.asked, "the full handshake asks for the certificate"
    f = d.finish()
    assert f[2] == "3", f
    d.identity(DEVICE, generation=1, engine=1)


def main():
    driver, out = sys.argv[1], sys.argv[2]
    lines = ["# scripts/tls_liar_client_auth.py: packages/tls's server and client certificates, one case a connection.",
             "# `## <tag> <name>` starts a case; `=` lines are the server driver's answers."]
    bad = 0
    for name, tag, alert, where, fn in CASES:
        conv = Conversation(driver)
        client = AuthClient(conv)
        try:
            fn(client)
            if where is not None:
                last = conv.answers[-1].split(" ")
                if last[1] != tag and tag != "ok":
                    raise Failed(f"ended {conv.answers[-1][:100]}, wanted {tag}")
                if tag != "ok":
                    assert last[2] == "5", f"failed: {last[:3]}"
                    if alert is not None:
                        client.expect_alert(alert, None if where == "plain" else client.read)
            ok, what = True, f"{name}: {tag}"
        except (Failed, AssertionError, Exception) as e:  # noqa: BLE001 -- reported
            ok, what = False, f"{name}: {type(e).__name__} {e}"
        conv.close()
        print(("" if ok else "FAILED ") + what)
        bad += not ok
        lines += [f"## {tag} {name}"] + conv.lines
    open(out, "w").write("\n".join(lines) + "\n")
    print(f"{len(CASES)} cases, {bad} failed")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
