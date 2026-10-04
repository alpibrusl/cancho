#!/usr/bin/env python3
"""Records a TLS 1.3 handshake between `packages/tls` and tlslite-ng (docs/tls-core.md §6.1).

    python3 scripts/tls_trace.py <driver> <rsa|ecdsa> <out.txt>

`driver` is `tests/programs/tls_driver.ls` built with `--std` and the package's
files. A tlslite-ng 0.8.2 server (pure Python, an implementation independent of
this one) runs in a thread on one end of a socket pair. It is restricted to
TLS 1.3, ChaCha20-Poly1305 and X25519, and has a certificate made here: RSA-2048
(the server signs `CertificateVerify` with RSA-PSS) or ECDSA P-256. The driver
is the client, with fixed "randomness" (the bytes 00 to 5f), so everything it
sends is a function of what it receives.

The client does the handshake, sends `GET / HTTP/1.0`, reads the answer, and
sees the server's close_notify. The file holds every line given to the driver
and, after each, the line it answered (`= ...`).
`crates/lex-sys/tests/conformance/tls.rs` replays it with no network: the same
input must give the same answers, byte for byte. The server's randomness is
not fixed, so a second recording differs; the replay does not need it to.
"""
import datetime
import socket
import subprocess
import sys
import threading

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, rsa
from cryptography.x509.oid import NameOID
from tlslite import HandshakeSettings, TLSConnection, X509CertChain
from tlslite.utils.keyfactory import parsePEMKey

BODY = b"HTTP/1.0 200 OK\r\nContent-Length: 13\r\n\r\nhello, lexsys"


def certificate(kind):
    key = rsa.generate_private_key(65537, 2048) if kind == "rsa" else ec.generate_private_key(ec.SECP256R1())
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "localhost")])
    start = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)
    cert = (x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(key.public_key())
            .serial_number(205).not_valid_before(start).not_valid_after(start + datetime.timedelta(days=3650))
            .add_extension(x509.SubjectAlternativeName([x509.DNSName("localhost")]), False)
            .sign(key, hashes.SHA256()))
    key_pem = key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8,
                                serialization.NoEncryption()).decode()
    return cert, key_pem


def main():
    driver, kind, out = sys.argv[1], sys.argv[2], sys.argv[3]
    cert, key_pem = certificate(kind)
    der = cert.public_bytes(serialization.Encoding.DER)
    pins = len(der).to_bytes(3, "big") + der
    server_end, client_end = socket.socketpair()
    seen = {}

    def serve():
        conn = TLSConnection(server_end)
        settings = HandshakeSettings()
        settings.minVersion = settings.maxVersion = (3, 4)
        settings.cipherNames = ["chacha20-poly1305"]
        settings.eccCurves = ["x25519"]
        settings.keyShares = ["x25519"]
        chain = X509CertChain()
        chain.parsePemList(cert.public_bytes(serialization.Encoding.PEM).decode())
        conn.handshakeServer(certChain=chain, privateKey=parsePEMKey(key_pem, private=True), settings=settings)
        seen["request"] = bytes(conn.read())
        conn.write(BODY)
        conn.close()

    thread = threading.Thread(target=serve, daemon=True)
    thread.start()
    proc = subprocess.Popen([driver], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
    lines = [f"# scripts/tls_trace.py {kind}: packages/tls against tlslite-ng 0.8.2. `=` lines are the client's answers.",
             f"# Pinned certificate: {der.hex()}"]

    def ask(line):
        proc.stdin.write(line + "\n")
        proc.stdin.flush()
        answer = proc.stdout.readline().strip()
        lines.extend([line, "= " + answer])
        fields = answer.split(" ")
        if fields[3] != "-":
            client_end.sendall(bytes.fromhex(fields[3]))
        return fields

    ask(f"C {b'localhost'.hex()} {bytes(range(96)).hex()} {pins.hex()}")
    client_end.settimeout(10)
    received = b""
    requested = False
    while True:
        data = client_end.recv(16384)
        if not data:
            break
        fields = ask(f"F {data.hex()}")
        if fields[4] != "-":
            received += bytes.fromhex(fields[4])
        if fields[0].startswith("-") or fields[2] == "4":
            break
        if fields[2] == "3" and not requested:
            ask(f"W {b'GET / HTTP/1.0'.hex()}0d0a0d0a")
            requested = True
    thread.join(5)
    proc.stdin.close()
    proc.wait()
    assert seen.get("request") == b"GET / HTTP/1.0\r\n\r\n", seen
    assert received == BODY, received
    assert lines[-1].split(" ")[3] == "4", "the client saw the server's close_notify"
    open(out, "w").write("\n".join(lines) + "\n")
    print(f"{out}: {sum(1 for l in lines if l[:1] in 'CFW')} driver lines; tlslite-ng got the request, the client the body")


if __name__ == "__main__":
    main()
