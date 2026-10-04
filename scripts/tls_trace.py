#!/usr/bin/env python3
"""Records a TLS 1.3 handshake between `packages/tls` and tlslite-ng (docs/tls-core.md §6.1).

    python3 scripts/tls_trace.py <driver> <rsa|ecdsa> <out.txt> [<suite> <group>]
    python3 scripts/tls_trace.py <driver> <rsa|ecdsa> <out.txt> --openssl12 <cipher>

`driver` is `tests/programs/tls_driver.ls` built with `--std` and the package's
files. A tlslite-ng 0.8.2 server (pure Python, an implementation independent of
this one) runs in a thread on one end of a socket pair. It is restricted to
TLS 1.3, ChaCha20-Poly1305 and X25519 (or the tlslite-ng suite and group
named, `aes128gcm`, `aes256gcm` or `chacha20-poly1305` and `x25519`,
`secp256r1` or `secp384r1`: a group other than X25519 makes the server answer
the client's X25519 share with a HelloRetryRequest, docs/tls-parity.md §3.3),
and has a certificate made here, issued
by a CA made here: an RSA-2048 leaf under an RSA-2048 CA (the server signs
`CertificateVerify` with RSA-PSS), or a P-256 leaf under a P-256 CA. The CA is
the client's whole trust store, and the clock is fixed at 2026-06-01, so the
chain is verified (`docs/x509-verify.md`). The driver is the client, with
fixed "randomness" (the bytes 00 to 5f), so everything it sends is a
function of what it receives.

With `--openssl12`, the server is `openssl s_server -tls1_2 -cipher <cipher> -www`
instead (OpenSSL's own TLS 1.2, docs/tls-parity.md §3.4), over a loopback
socket: the same certificate and root, and it answers the request with its
status page and closes.

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


NOW = 1780272000  # 2026-06-01, inside both certificates' validity


def certificate(kind):
    """A CA and a leaf for `localhost` it issued, both of `kind`."""
    def new_key():
        return rsa.generate_private_key(65537, 2048) if kind == "rsa" else ec.generate_private_key(ec.SECP256R1())
    start = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)
    end = start + datetime.timedelta(days=3650)
    ca_key, key = new_key(), new_key()
    ca_name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, f"tls_trace {kind} CA")])
    ca = (x509.CertificateBuilder().subject_name(ca_name).issuer_name(ca_name).public_key(ca_key.public_key())
          .serial_number(1).not_valid_before(start).not_valid_after(end)
          .add_extension(x509.BasicConstraints(ca=True, path_length=None), True)
          .sign(ca_key, hashes.SHA256()))
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "localhost")])
    cert = (x509.CertificateBuilder().subject_name(name).issuer_name(ca_name).public_key(key.public_key())
            .serial_number(205).not_valid_before(start).not_valid_after(end)
            .add_extension(x509.SubjectAlternativeName([x509.DNSName("localhost")]), False)
            .sign(ca_key, hashes.SHA256()))
    key_pem = key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8,
                                serialization.NoEncryption()).decode()
    return ca, cert, key_pem


def openssl12(cert, key_pem, cipher):
    """`openssl s_server -tls1_2` on a loopback port: (process, connected socket, temporary directory)."""
    import tempfile
    import time
    d = tempfile.TemporaryDirectory()
    open(f"{d.name}/c.pem", "wb").write(cert.public_bytes(serialization.Encoding.PEM))
    open(f"{d.name}/k.pem", "w").write(key_pem)
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    proc = subprocess.Popen(["openssl", "s_server", "-accept", f"127.0.0.1:{port}", "-cert", "c.pem", "-key", "k.pem",
                             "-tls1_2", "-cipher", cipher, "-www", "-naccept", "1", "-quiet"],
                            cwd=d.name, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        try:
            return proc, socket.create_connection(("127.0.0.1", port), 1), d
        except OSError:
            time.sleep(0.05)
    raise RuntimeError("openssl s_server did not start")


def main():
    driver, kind, out = sys.argv[1], sys.argv[2], sys.argv[3]
    if len(sys.argv) > 5 and sys.argv[4] == "--openssl12":
        return main12(driver, kind, out, sys.argv[5])
    suite, group = (sys.argv[4], sys.argv[5]) if len(sys.argv) > 5 else ("chacha20-poly1305", "x25519")
    ca, cert, key_pem = certificate(kind)
    roots = ca.public_bytes(serialization.Encoding.PEM)
    server_end, client_end = socket.socketpair()
    seen = {}

    def serve():
        conn = TLSConnection(server_end)
        settings = HandshakeSettings()
        settings.minVersion = settings.maxVersion = (3, 4)
        settings.cipherNames = [suite]
        settings.eccCurves = [group]
        settings.keyShares = [group]
        chain = X509CertChain()
        chain.parsePemList(cert.public_bytes(serialization.Encoding.PEM).decode())
        conn.handshakeServer(certChain=chain, privateKey=parsePEMKey(key_pem, private=True), settings=settings)
        seen["request"] = bytes(conn.read())
        conn.write(BODY)
        conn.close()

    thread = threading.Thread(target=serve, daemon=True)
    thread.start()
    proc = subprocess.Popen([driver], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
    lines = [f"# scripts/tls_trace.py {kind} {suite} {group}: packages/tls against tlslite-ng 0.8.2. `=` lines are the client's answers.",
             f"# Root: {ca.public_bytes(serialization.Encoding.DER).hex()}"]

    def ask(line):
        proc.stdin.write(line + "\n")
        proc.stdin.flush()
        answer = proc.stdout.readline().strip()
        lines.extend([line, "= " + answer])
        fields = answer.split(" ")
        if fields[3] != "-":
            client_end.sendall(bytes.fromhex(fields[3]))
        return fields

    ask(f"C {b'localhost'.hex()} {bytes(range(96)).hex()} {roots.hex()} {NOW}")
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


def main12(driver, kind, out, cipher):
    ca, cert, key_pem = certificate(kind)
    roots = ca.public_bytes(serialization.Encoding.PEM)
    proc, sock, tmp = openssl12(cert, key_pem, cipher)
    client = subprocess.Popen([driver], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
    lines = [f"# scripts/tls_trace.py {kind} --openssl12 {cipher}: packages/tls against openssl s_server -tls1_2 -www.",
             "# `=` lines are the client's answers.",
             f"# Root: {ca.public_bytes(serialization.Encoding.DER).hex()}"]

    def ask(line):
        client.stdin.write(line + "\n")
        client.stdin.flush()
        answer = client.stdout.readline().strip()
        lines.extend([line, "= " + answer])
        fields = answer.split(" ")
        if fields[3] != "-":
            sock.sendall(bytes.fromhex(fields[3]))
        return fields

    ask(f"C {b'localhost'.hex()} {bytes(range(96)).hex()} {roots.hex()} {NOW}")
    sock.settimeout(10)
    received = b""
    requested = False
    while True:
        try:
            data = sock.recv(16384)
        except TimeoutError:
            break
        if not data:
            break
        fields = ask(f"F {data.hex()}")
        if fields[4] != "-":
            received += bytes.fromhex(fields[4])
        if fields[0].startswith("-") or fields[2] in ("4", "5"):
            break
        if fields[2] == "3" and not requested:
            ask(f"W {b'GET / HTTP/1.0'.hex()}0d0a0d0a")
            requested = True
        if b"</HTML>" in received:
            # The status page is whole; the server waits for the client
            # to close, so the client sends close_notify.
            ask("Q")
            sock.settimeout(2)
    if lines[-1].split(" ")[3] not in ("4", "5"):
        # The server closed the socket first: the client still closes.
        ask("Q")
    client.stdin.close()
    client.wait()
    proc.kill()
    proc.wait()
    assert received.startswith(b"HTTP/1.0 200 ok"), received[:40]
    assert f"Cipher is {cipher}".encode() in received or cipher.encode() in received, "the cipher asked for"
    assert lines[-1].split(" ")[3] == "4", "the connection closed"
    open(out, "w").write("\n".join(lines) + "\n")
    print(f"{out}: {sum(1 for l in lines if l[:1] in 'CFW')} driver lines; {len(received)} bytes of OpenSSL's status page")


if __name__ == "__main__":
    main()
