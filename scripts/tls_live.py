#!/usr/bin/env python3
"""`packages/tls` against live servers on this machine (docs/tls-core.md §10).

    python3 scripts/tls_live.py <tls_many> [<conc>]

`tls_many` is `tests/programs/tls_many.ls` built with `packages/tls` and
`packages/x509`. For each certificate key (P-256, P-384, RSA-2048, RSA-4096,
Ed25519) a threaded Python `ssl` server (OpenSSL underneath) is started with a
fresh certificate from a fresh P-256 CA, which is all tls_many trusts, TLS 1.3 only. It answers each request with a
body of its own (64 KiB of seeded bytes plus the connection's number) and
closes with close_notify. `tls_many` makes `conc` connections (default 64) at
once, first reading one byte a socket read, then 65,536; every connection
must end `ok`, with the SHA-256 of what the server sent. Then a server that
closes WITHOUT close_notify: every connection must fail `tls-peer-closed`
(a truncation the client must not take for the end, RFC 8446 §6.1). Then
the same server under a host its certificate does not name, and with
another CA's root as the store: every connection must fail
`x509-name-mismatch`, then `x509-unknown-issuer`.

Then every TLS 1.3 suite against every group `openssl s_server -HTTP` can be
told to accept (docs/tls-parity.md §3.3): AES-128-GCM, AES-256-GCM and
ChaCha20-Poly1305, each with X25519 (the share the ClientHello sends) and with
P-256 and P-384 (which the server asks for with a HelloRetryRequest), 16
connections each. Every connection must end `ok`.

Then TLS 1.2 (docs/tls-parity.md §3.4), 16 connections each, every one `ok`:
`openssl s_server -tls1_2` with each of the six suites (ECDSA ones with a P-256
certificate, RSA ones with RSA-2048), and with an RSA key made to sign the key
exchange with RSASSA-PKCS1-v1_5 (`-sigalgs RSA+SHA256`); Python `ssl` with
`maximum_version` TLS 1.2, each certificate type; tlslite-ng with TLS 1.2.

Then two other implementations, P-256 and RSA-2048 certificates, both read
sizes: `openssl s_server -HTTP` (OpenSSL's own TLS, not Python's use of it;
it serves one connection at a time, so the others wait in its backlog), and
a threaded tlslite-ng server (pure Python). Every connection must end `ok`,
and all of one server's bodies must be the same. Exit status 1 on any
difference.
"""
import datetime
import hashlib
import random
import socket
import ssl
import subprocess
import sys
import tempfile
import threading

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519, rsa
from cryptography.x509.oid import NameOID

HOST = "live.lex-sys.test"


def certificate(kind):
    key = {
        "p256": lambda: ec.generate_private_key(ec.SECP256R1()),
        "p384": lambda: ec.generate_private_key(ec.SECP384R1()),
        "rsa2048": lambda: rsa.generate_private_key(65537, 2048),
        "rsa4096": lambda: rsa.generate_private_key(65537, 4096),
        "ed25519": ed25519.Ed25519PrivateKey.generate,
    }[kind]()
    # A P-256 CA, made for the run, issues the server's certificate; the
    # CA is what tls_many trusts.
    ca_key = ec.generate_private_key(ec.SECP256R1())
    ca_name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, f"tls_live {kind} CA")])
    now = datetime.datetime.now(datetime.timezone.utc)
    ca = (
        x509.CertificateBuilder()
        .subject_name(ca_name)
        .issuer_name(ca_name)
        .public_key(ca_key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - datetime.timedelta(days=1))
        .not_valid_after(now + datetime.timedelta(days=30))
        .add_extension(x509.BasicConstraints(ca=True, path_length=None), True)
        .sign(ca_key, hashes.SHA256())
    )
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, HOST)])
    cert = (
        x509.CertificateBuilder()
        .subject_name(name)
        .issuer_name(ca_name)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - datetime.timedelta(days=1))
        .not_valid_after(now + datetime.timedelta(days=30))
        .add_extension(x509.SubjectAlternativeName([x509.DNSName(HOST)]), False)
        .sign(ca_key, hashes.SHA256())
    )
    key_pem = key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption())
    return cert.public_bytes(serialization.Encoding.PEM), key_pem, ca.public_bytes(serialization.Encoding.PEM)


def body(n):
    return random.Random(205).randbytes(65536) + f" connection {n}\n".encode()


class Server:
    def __init__(self, cert_pem, key_pem, notify=True, tls12=False):
        self.notify = notify
        self.sent = []
        self.lock = threading.Lock()
        self.dir = tempfile.TemporaryDirectory()
        cert, key = f"{self.dir.name}/c.pem", f"{self.dir.name}/k.pem"
        open(cert, "wb").write(cert_pem)
        open(key, "wb").write(key_pem)
        self.ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        self.ctx.minimum_version = ssl.TLSVersion.TLSv1_2 if tls12 else ssl.TLSVersion.TLSv1_3
        if tls12:
            self.ctx.maximum_version = ssl.TLSVersion.TLSv1_2
        self.ctx.load_cert_chain(cert, key)
        self.sock = socket.socket()
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(256)
        self.port = self.sock.getsockname()[1]
        threading.Thread(target=self.accept, daemon=True).start()

    def accept(self):
        n = 0
        while True:
            raw, _ = self.sock.accept()
            threading.Thread(target=self.serve, args=(raw, n), daemon=True).start()
            n += 1

    def serve(self, raw, n):
        try:
            conn = self.ctx.wrap_socket(raw, server_side=True)
            request = b""
            while b"\r\n\r\n" not in request:
                chunk = conn.recv(4096)
                if not chunk:
                    return
                request += chunk
            data = body(n)
            reply = b"HTTP/1.0 200 OK\r\nContent-Length: %d\r\n\r\n" % len(data) + data
            conn.sendall(reply)
            with self.lock:
                self.sent.append(hashlib.sha256(reply).hexdigest())
            if self.notify:
                conn.unwrap()
            raw.close()
        except (OSError, ssl.SSLError):
            raw.close()


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class OpenSSL:
    """`openssl s_server -HTTP`, TLS 1.3 with one suite and one group: ChaCha20-Poly1305 and X25519
    unless told otherwise. A group other than X25519 makes it answer the ClientHello's X25519 share
    with a HelloRetryRequest."""

    def __init__(self, cert_pem, key_pem, conc, suite="TLS_CHACHA20_POLY1305_SHA256", group="X25519", tls12=None):
        self.dir = tempfile.TemporaryDirectory()
        open(f"{self.dir.name}/c.pem", "wb").write(cert_pem)
        open(f"{self.dir.name}/k.pem", "wb").write(key_pem)
        self.port = free_port()
        self.proc = subprocess.Popen(
            ["openssl", "s_server", "-accept", f"127.0.0.1:{self.port}", "-cert", "c.pem", "-key", "k.pem",
             *(["-tls1_3", "-ciphersuites", suite, "-groups", group] if tls12 is None else ["-tls1_2", "-cipher", tls12[0], *tls12[1:]]), "-HTTP",
             "-naccept", str(conc), "-quiet"],
            cwd=self.dir.name, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            try:
                socket.create_connection(("127.0.0.1", self.port), 0.1).close()
                break
            except OSError:
                threading.Event().wait(0.05)
        # The probe used one of its `-naccept`; one more connection is
        # not needed, so the server is left to time out with the test.

    def stop(self):
        self.proc.kill()
        self.proc.wait()


class TlsLite:
    """A threaded tlslite-ng server, TLS 1.3 with ChaCha20-Poly1305 and X25519 only."""

    def __init__(self, cert_pem, key_pem, tls12=False):
        from tlslite import HandshakeSettings, TLSConnection, X509CertChain
        from tlslite.utils.keyfactory import parsePEMKey
        self.chain = X509CertChain()
        self.chain.parsePemList(cert_pem.decode())
        self.key = parsePEMKey(key_pem.decode(), private=True)
        self.settings = HandshakeSettings()
        self.settings.minVersion = self.settings.maxVersion = (3, 3) if tls12 else (3, 4)
        self.settings.cipherNames = ["chacha20-poly1305"]
        self.settings.eccCurves = ["x25519"]
        self.settings.keyShares = ["x25519"]
        self.connection = TLSConnection
        self.sock = socket.socket()
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(256)
        self.port = self.sock.getsockname()[1]
        threading.Thread(target=self.accept, daemon=True).start()

    def accept(self):
        while True:
            raw, _ = self.sock.accept()
            threading.Thread(target=self.serve, args=(raw,), daemon=True).start()

    def serve(self, raw):
        try:
            conn = self.connection(raw)
            conn.handshakeServer(certChain=self.chain, privateKey=self.key, settings=self.settings)
            request = b""
            while b"\r\n\r\n" not in request:
                request += bytes(conn.read())
            conn.write(b"HTTP/1.0 200 OK\r\nContent-Length: 13\r\n\r\nhello, lexsys")
            conn.close()
        except Exception:  # noqa: BLE001 -- the client's line reports it
            raw.close()


def same_bodies(lines, conc):
    rows = [l.split() for l in lines[:-1]]
    return len(rows) == conc and lines[-1] == f"done ok={conc} failed=0" and len({r[4] for r in rows}) == 1


def run(exe, server, roots, conc, chunk, host=HOST):
    out = subprocess.run(
        [exe, "127.0.0.1", str(server.port), host, str(conc), str(chunk)],
        input=roots, capture_output=True, timeout=120,
    )
    return out.returncode, out.stdout.decode().splitlines()


def main():
    exe = sys.argv[1]
    conc = int(sys.argv[2]) if len(sys.argv) > 2 else 64
    bad = 0
    for kind in ["p256", "p384", "rsa2048", "rsa4096", "ed25519"]:
        cert, key, ca = certificate(kind)
        for chunk in [1, 65536]:
            server = Server(cert, key)
            code, lines = run(exe, server, ca, conc, chunk)
            rows = [l.split() for l in lines[:-1]]
            ok = [r for r in rows if r[2] == "ok"]
            hashes_seen = sorted(r[4] for r in ok)
            fine = code == 0 and len(ok) == conc and hashes_seen == sorted(server.sent) and lines[-1] == f"done ok={conc} failed=0"
            print(f"{kind} chunk {chunk}: {lines[-1] if lines else 'no output'}, bodies {'match' if fine else 'DIFFER'}")
            if not fine:
                bad += 1
                print("\n".join(lines[:5]), file=sys.stderr)
    cert, key, ca = certificate("p256")
    for suite in ["TLS_AES_128_GCM_SHA256", "TLS_AES_256_GCM_SHA384", "TLS_CHACHA20_POLY1305_SHA256"]:
        for group in ["X25519", "P-256", "P-384"]:
            server = OpenSSL(cert, key, 17, suite, group)
            code, lines = run(exe, server, ca, 16, 65536)
            server.stop()
            fine = code == 0 and same_bodies(lines, 16)
            print(f"openssl s_server {suite} {group}: {lines[-1] if lines else 'no output'}")
            if not fine:
                bad += 1
                print("\n".join(lines[:5]), file=sys.stderr)
    certs = {kind: certificate(kind) for kind in ["p256", "rsa2048"]}
    for cipher in ["ECDHE-ECDSA-AES128-GCM-SHA256", "ECDHE-ECDSA-AES256-GCM-SHA384", "ECDHE-ECDSA-CHACHA20-POLY1305",
                   "ECDHE-RSA-AES128-GCM-SHA256", "ECDHE-RSA-AES256-GCM-SHA384", "ECDHE-RSA-CHACHA20-POLY1305", "PKCS1"]:
        kind = "p256" if "ECDSA" in cipher else "rsa2048"
        cert, key, ca = certs[kind]
        tls12 = ["ECDHE-RSA-AES128-GCM-SHA256", "-sigalgs", "RSA+SHA256"] if cipher == "PKCS1" else [cipher]
        server = OpenSSL(cert, key, 17, tls12=tls12)
        code, lines = run(exe, server, ca, 16, 65536)
        server.stop()
        fine = code == 0 and same_bodies(lines, 16)
        print(f"openssl s_server -tls1_2 {' '.join(tls12)}: {lines[-1] if lines else 'no output'}")
        if not fine:
            bad += 1
            print("\n".join(lines[:5]), file=sys.stderr)
    for kind in ["p256", "p384", "rsa2048", "ed25519"]:
        cert, key, ca = certificate(kind)
        server = Server(cert, key, tls12=True)
        code, lines = run(exe, server, ca, 16, 65536)
        rows = [l.split() for l in lines[:-1]]
        fine = code == 0 and sorted(r[4] for r in rows if r[2] == "ok") == sorted(server.sent) and len(rows) == 16
        print(f"Python ssl, TLS 1.2, {kind}: {lines[-1] if lines else 'no output'}")
        bad += 0 if fine else 1
    for kind in ["p256", "rsa2048"]:
        cert, key, ca = certs[kind]
        server = TlsLite(cert, key, tls12=True)
        code, lines = run(exe, server, ca, 16, 65536)
        fine = code == 0 and same_bodies(lines, 16)
        print(f"tlslite-ng, TLS 1.2, {kind}: {lines[-1] if lines else 'no output'}")
        bad += 0 if fine else 1
    for kind in ["p256", "rsa2048"]:
        cert, key, ca = certificate(kind)
        for chunk in [1, 65536]:
            for name in ["openssl s_server", "tlslite-ng"]:
                server = OpenSSL(cert, key, conc + 1) if name == "openssl s_server" else TlsLite(cert, key)
                code, lines = run(exe, server, ca, conc, chunk)
                if name == "openssl s_server":
                    server.stop()
                fine = code == 0 and same_bodies(lines, conc)
                print(f"{name} {kind} chunk {chunk}: {lines[-1] if lines else 'no output'}, bodies {'the same' if fine else 'DIFFER'}")
                if not fine:
                    bad += 1
                    print("\n".join(lines[:5]), file=sys.stderr)
    cert, key, ca = certificate("p256")
    server = Server(cert, key, notify=False)
    code, lines = run(exe, server, ca, conc, 65536)
    tags = {l.split()[2] for l in lines[:-1]}
    fine = tags == {"tls-peer-closed"} and lines[-1] == f"done ok=0 failed={conc}"
    print(f"no close_notify: {lines[-1] if lines else 'no output'}, tags {sorted(tags)}")
    bad += 0 if fine else 1
    # The chain itself refused: a host the certificate does not name, and
    # roots from another CA.
    _, _, other_ca = certificate("p384")  # another name too: "tls_live p384 CA"
    for what, roots, host, want in [("another host", ca, "other.lex-sys.test", "x509-name-mismatch"),
                                    ("another CA's root", other_ca, HOST, "x509-unknown-issuer")]:
        server = Server(cert, key)
        code, lines = run(exe, server, roots, conc, 65536, host)
        tags = {l.split()[2] for l in lines[:-1]}
        fine = tags == {want} and lines[-1] == f"done ok=0 failed={conc}"
        print(f"{what}: {lines[-1] if lines else 'no output'}, tags {sorted(tags)}")
        bad += 0 if fine else 1
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
