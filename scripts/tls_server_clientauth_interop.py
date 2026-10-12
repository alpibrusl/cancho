#!/usr/bin/env python3
"""`packages/tls`'s server asking for client certificates, against the clients that can present one
(docs/tls-server.md §13.11).

    python3 scripts/tls_server_clientauth_interop.py <tls_serve> [<client> ...]

`tls_serve` is `tests/programs/tls_serve.cho` built as for `scripts/tls_server_interop.py`, whose image
(`scripts/interop/server.Dockerfile`), clients and `authority` this uses; `/etc/hosts` must name `srv.example` as
127.0.0.1. The clients: `openssl` (`s_client -cert/-key`), `curl` (`--cert/--key`), `go` (`tls.Config.Certificates`),
`wolfssl` (`wolfSSL_CTX_use_certificate_chain_file`) and `mosquitto` (`--cert/--key`). `packages/tls`'s own client
cannot present a certificate yet (#386); its row is added when it can.

A CA of the clients' own (P-256) signs five client certificates, and an intermediate CA signs a sixth, so one chain has
two certificates: a P-256 key, a P-256 key under the intermediate, a P-384 key, an RSA-2048 key (the client signs
RSA-PSS), an Ed25519 key. Each has a subject with a country, an organization and a common name, and four kinds of
subjectAltName. A second CA that the server does not trust signs a stranger. Three servers run, `tls_serve` in
`--client-ca <file> required`, `--client-ca <file> optional` and with no client CA, each in the mode a client needs.

Each row is one connection. A row passes when the client completes and its data comes back, and the identity the server
printed for the connection (`tls_serve`'s `conn` line, the program's own view through `tls.peer_*`) is what **OpenSSL
reports for the same certificate**: the fingerprint is `openssl x509 -fingerprint -sha256`'s, and the SANs are
`openssl x509 -ext subjectAltName`'s, in order; the subject's DER is the certificate's (read with pyca/cryptography,
which parses the same bytes OpenSSL does). A refused row passes when the client fails and the server's line has the
tag the row expects. The off server accepts a client that has a certificate and gives it no identity. One line a
row, then a count; exit status 1 if any row failed.
"""
import datetime
import ipaddress
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, ed25519, rsa
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_server_interop as base  # noqa: E402

MESSAGE = "hello over mutual TLS"
KINDS = ["p256", "p256-via-intermediate", "p384", "rsa2048", "ed25519"]


def pem(cert):
    return cert.public_bytes(serialization.Encoding.PEM)


def key_pem(key):
    return key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption())


def ca(name, parent=None, key=None):
    now = datetime.datetime.now(datetime.timezone.utc)
    key = key or ec.generate_private_key(ec.SECP256R1())
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, name)])
    issuer_name, issuer_key = (parent[1].subject, parent[0]) if parent else (subject, key)
    cert = (x509.CertificateBuilder().subject_name(subject).issuer_name(issuer_name).public_key(key.public_key())
            .serial_number(x509.random_serial_number()).not_valid_before(now - datetime.timedelta(days=1))
            .not_valid_after(now + datetime.timedelta(days=30))
            .add_extension(x509.BasicConstraints(ca=True, path_length=None), True)
            .add_extension(x509.KeyUsage(False, False, False, False, False, True, True, False, False), True)
            .add_extension(x509.SubjectKeyIdentifier.from_public_key(key.public_key()), False)
            .sign(issuer_key, hashes.SHA256()))
    return key, cert


def client_cert(issuer, kind):
    now = datetime.datetime.now(datetime.timezone.utc)
    key = {"p384": lambda: ec.generate_private_key(ec.SECP384R1()),
           "rsa2048": lambda: rsa.generate_private_key(65537, 2048),
           "ed25519": ed25519.Ed25519PrivateKey.generate}.get(kind, lambda: ec.generate_private_key(ec.SECP256R1()))()
    subject = x509.Name([x509.NameAttribute(NameOID.COUNTRY_NAME, "ES"),
                         x509.NameAttribute(NameOID.ORGANIZATION_NAME, "Fleet S.L."),
                         x509.NameAttribute(NameOID.COMMON_NAME, f"device-{kind}")])
    sans = [x509.DNSName(f"{kind}.fleet.test"), x509.UniformResourceIdentifier(f"spiffe://fleet.test/{kind}"),
            x509.RFC822Name("ops@fleet.test"), x509.IPAddress(ipaddress.ip_address("10.17.0.17"))]
    cert = (x509.CertificateBuilder().subject_name(subject).issuer_name(issuer[1].subject).public_key(key.public_key())
            .serial_number(x509.random_serial_number()).not_valid_before(now - datetime.timedelta(days=1))
            .not_valid_after(now + datetime.timedelta(days=30))
            .add_extension(x509.SubjectAlternativeName(sans), False)
            .add_extension(x509.KeyUsage(True, False, False, False, False, False, False, False, False), True)
            .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.CLIENT_AUTH]), False)
            .sign(issuer[0], hashes.SHA256()))
    return key, cert


def clients_authority(work):
    """The client CA, an intermediate under it, a certificate of each kind, and a stranger's."""
    root = ca("fleet root")
    middle = ca("fleet intermediate", parent=root)
    other = ca("not trusted")
    files = {}
    for kind in KINDS:
        issuer = middle if kind == "p256-via-intermediate" else root
        key, cert = client_cert(issuer, kind)
        chain = pem(cert) + (pem(middle[1]) if issuer is middle else b"")
        open(os.path.join(work, f"{kind}.pem"), "wb").write(chain)
        open(os.path.join(work, f"{kind}.leaf.pem"), "wb").write(pem(cert))
        open(os.path.join(work, f"{kind}.key"), "wb").write(key_pem(key))
        files[kind] = cert
    open(os.path.join(work, "int.pem"), "wb").write(pem(middle[1]))
    key, cert = client_cert(other, "stranger")
    open(os.path.join(work, "stranger.pem"), "wb").write(pem(cert))
    open(os.path.join(work, "stranger.leaf.pem"), "wb").write(pem(cert))
    open(os.path.join(work, "stranger.key"), "wb").write(key_pem(key))
    open(os.path.join(work, "clients_ca.pem"), "wb").write(pem(root[1]))
    return files


class Server(base.Server):
    """`tls_serve` in one mode with the clients' CA given as `--client-ca <file> <how>` (or none)."""

    def __init__(self, exe, mode, work, how=None):
        self.port = base.free_port()
        argv = [exe, str(self.port), mode, "http/1.1,mqtt", "0", "-", "-", "main.pem", "main.key",
                "srv.example,*.wild.example", "other.pem", "other.key", "other.example"]
        if how:
            argv += ["--client-ca", "clients_ca.pem", how]
        self.proc = subprocess.Popen(argv, cwd=work, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                                     bufsize=1)
        self.lines, self.cond = [], threading.Condition()
        first = self.proc.stdout.readline()
        if first.strip() != "listening":
            raise RuntimeError(f"tls_serve: {first.strip()} {self.proc.stdout.read()[:300]}")
        threading.Thread(target=self.read, daemon=True).start()


def openssl_fingerprint(work, name):
    code, out = base.run(["openssl", "x509", "-in", os.path.join(work, f"{name}.leaf.pem"), "-noout", "-fingerprint",
                          "-sha256"])
    return re.search(r"=([0-9A-F:]+)", out).group(1).replace(":", "").lower()


def openssl_sans(work, name):
    """The subjectAltName entries as OpenSSL prints them, as `tag:hex` in order (tags of docs/tls-server.md §13.6)."""
    code, out = base.run(["openssl", "x509", "-in", os.path.join(work, f"{name}.leaf.pem"), "-noout", "-ext",
                          "subjectAltName"])
    entries = []
    for part in out.split("\n")[1].split(","):
        kind, _, value = part.strip().partition(":")
        if kind == "DNS":
            entries.append("130:" + value.encode().hex())
        elif kind == "URI":
            entries.append("134:" + value.encode().hex())
        elif kind == "email":
            entries.append("129:" + value.encode().hex())
        elif kind == "IP Address":
            entries.append("135:" + ipaddress.ip_address(value).packed.hex())
    return ",".join(entries)


def identity_of(line, work, kind, certs):
    """None when the `conn` line's identity is the certificate's, as OpenSSL reports it; else what it says."""
    f = line.split()
    # conn <code> <tag> <name> <alpn> <bytes> <suite> <group> <retried|direct> <resumed|full> <verdict> <verified>
    #   <fingerprint> <subject> <sans>
    if kind is None:
        return None if f[11:] == ["0", "-", "-", "-"] else f"no identity wanted: {line}"
    want_subject = certs[kind].subject.public_bytes().hex()
    got = f[11:]
    want = [got[0], openssl_fingerprint(work, kind), want_subject, openssl_sans(work, kind)]
    if got[0] == "0" or got != want:
        return f"identity\n  got  {got}\n  want {want}"
    return None


def connection(srv, since, tag="ok", kind=None, work=None, certs=None):
    line = srv.next_conn(since)
    if line is None:
        return "no line from the server"
    f = line.split()
    if f[2] != tag:
        return line
    if tag != "ok":
        return None
    return identity_of(line, work, kind, certs)


def openssl_row(srv, work, certs, kind, tag="ok", cert_of=None):
    name = cert_of or kind
    args = ["openssl", "s_client", "-connect", f"127.0.0.1:{srv.port}", "-CAfile", "ca.pem", "-verify_return_error",
            "-ign_eof", "-quiet", "-tls1_3", "-servername", "srv.example", "-verify_hostname", "srv.example"]
    if name is not None:
        args += ["-cert", f"{name}.leaf.pem", "-key", f"{name}.key"]
        if name == "p256-via-intermediate":
            args += ["-cert_chain", "int.pem"]
    since = srv.mark()
    code, out = base.run(args, input=b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
    if tag != "ok":
        bad = connection(srv, since, tag)
        return bad if bad else None if code != 0 or "hello from cancho" not in out else "the client was not refused"
    if "hello from cancho" not in out:
        return f"s_client {code}: {out.strip()[-200:]}"
    return connection(srv, since, "ok", kind, work, certs)


def curl_row(srv, work, certs, kind, tag="ok", cert_of=None):
    name = cert_of or kind
    args = ["curl", "-sS", "--http1.1", "--cacert", "ca.pem", f"https://srv.example:{srv.port}/"]
    if name is not None:
        args += ["--cert", f"{name}.pem", "--key", f"{name}.key"]
    since = srv.mark()
    code, out = base.run(args)
    if tag != "ok":
        bad = connection(srv, since, tag)
        return bad if bad else None if code != 0 or "hello from cancho" not in out else "the client was not refused"
    if code != 0 or "hello from cancho" not in out:
        return f"curl {code}: {out.strip()[-200:]}"
    return connection(srv, since, "ok", kind, work, certs)


def go_row(srv, work, certs, kind, exe, tag="ok", cert_of=None):
    name = cert_of or kind
    args = [exe, f"127.0.0.1:{srv.port}", "srv.example", "ca.pem", "X25519", "-", MESSAGE]
    if name is not None:
        args += [f"{name}.pem", f"{name}.key"]
    since = srv.mark()
    code, out = base.run(args)
    if tag != "ok":
        bad = connection(srv, since, tag)
        return bad if bad else None if code != 0 else "the client was not refused"
    if code != 0 or not out.startswith("ok 304"):
        return f"go {code}: {out.strip()[-200:]}"
    return connection(srv, since, "ok", kind, work, certs)


def wolfssl_row(srv, work, certs, kind, exe, tag="ok", cert_of=None):
    name = cert_of or kind
    args = [exe, str(srv.port), "srv.example", "ca.pem", "TLS13-AES128-GCM-SHA256", "X25519", "-", MESSAGE]
    if name is not None:
        args += [f"{name}.pem", f"{name}.key"]
    since = srv.mark()
    code, out = base.run(args)
    if tag != "ok":
        bad = connection(srv, since, tag)
        return bad if bad else None if code != 0 else "the client was not refused"
    if code != 0 or not out.startswith("ok "):
        return f"wolfssl {code}: {out.strip()[-200:]}"
    return connection(srv, since, "ok", kind, work, certs)


def mosquitto_row(srv, work, certs, kind, tag="ok", cert_of=None):
    name = cert_of or kind
    common = ["-h", "srv.example", "-p", str(srv.port), "--cafile", "ca.pem", "--tls-version", "tlsv1.3",
              "-t", "cancho/mutual", "-i", "cancho-mutual"]
    if name is not None:
        common += ["--cert", f"{name}.pem", "--key", f"{name}.key"]
    since = srv.mark()
    code, out = base.run(["mosquitto_sub", *common, "-C", "1", "-W", "10"])
    if tag != "ok":
        bad = connection(srv, since, tag)
        return bad if bad else None if code != 0 else "the client was not refused"
    if code != 0 or out.strip() != "hello from cancho":
        return f"mosquitto_sub {code}: {out.strip()[-200:]}"
    bad = connection(srv, since, "ok", kind, work, certs)
    if bad:
        return "sub: " + bad
    since = srv.mark()
    payload = f"from mosquitto_pub with a certificate {kind}"
    code, out = base.run(["mosquitto_pub", *common, "-m", payload])
    if code != 0:
        return f"mosquitto_pub {code}: {out.strip()[-200:]}"
    bad = connection(srv, since, "ok", kind, work, certs)
    if bad:
        return "pub: " + bad
    if f"publish cancho/mutual {payload}" not in srv.published(since):
        return "pub: the server did not print the message"
    return None


def main():
    exe = os.path.abspath(sys.argv[1])
    wanted = sys.argv[2:] or ["openssl", "curl", "go", "wolfssl", "mosquitto"]
    work = tempfile.mkdtemp(prefix="tls-clientauth-interop-")
    os.chdir(work)
    base.authority(work)
    certs = clients_authority(work)
    tools = {}
    if shutil.which("go"):
        r = subprocess.run(["go", "build", "-o", f"{work}/go_client", f"{base.INTEROP}/go_client.go"], cwd=work,
                           capture_output=True, env=dict(os.environ, GOFLAGS="-mod=mod", HOME=work))
        tools["go"] = f"{work}/go_client" if r.returncode == 0 else (None, r.stderr.decode()[-200:])
    else:
        tools["go"] = (None, "`go` is not installed")
    r = subprocess.run(["cc", "-O2", f"{base.INTEROP}/wolfssl_client.c", "-lwolfssl", "-o", f"{work}/wolfssl_client"],
                       capture_output=True)
    tools["wolfssl"] = f"{work}/wolfssl_client" if r.returncode == 0 else (None, r.stderr.decode()[-200:])
    for name, tool in [("openssl", "openssl"), ("curl", "curl"), ("mosquitto", "mosquitto_pub")]:
        tools[name] = shutil.which(tool) or (None, f"`{tool}` is not installed")

    # One server a (mode of the data, how the clients are asked) the clients need.
    servers = {}
    for how in ("required", "optional", None):
        for data in ("http", "echo", "mqtt"):
            servers[(data, how)] = Server(exe, data, work, how)
    rows = []

    def row(name, fn):
        try:
            bad = fn()
        except Exception as e:  # noqa: BLE001 -- a row that throws is a failed row
            bad = f"{type(e).__name__}: {e}"
        rows.append((name, bad))
        print(f"{name}: {'ok' if bad is None else 'FAILED ' + bad}", flush=True)

    for client in wanted:
        tool = tools.get(client)
        if not isinstance(tool, str):
            print(f"{client}: not run, {tool[1] if tool else 'unknown client'}")
            continue
        data = {"openssl": "http", "curl": "http", "go": "echo", "wolfssl": "echo", "mosquitto": "mqtt"}[client]
        call = {"openssl": lambda s, k, **kw: openssl_row(s, work, certs, k, **kw),
                "curl": lambda s, k, **kw: curl_row(s, work, certs, k, **kw),
                "go": lambda s, k, **kw: go_row(s, work, certs, k, tool, **kw),
                "wolfssl": lambda s, k, **kw: wolfssl_row(s, work, certs, k, tool, **kw),
                "mosquitto": lambda s, k, **kw: mosquitto_row(s, work, certs, k, **kw)}[client]
        for how in ("required", "optional"):
            for kind in KINDS:
                row(f"{client} {how}: {kind}", lambda: call(servers[(data, how)], kind))
        row(f"{client} required: no certificate refused, certificate_required",
            lambda: call(servers[(data, "required")], None, tag="tls-server-client-cert-required"))
        row(f"{client} optional: no certificate accepted, no identity", lambda: call(servers[(data, "optional")], None))
        if client == "go":
            # Go's client honors certificate_authorities (docs/tls-server.md §13.2): a certificate no listed CA
            # issued is not sent, so the server sees a client with none.
            row("go required: a stranger's certificate is not sent (not issued by a listed CA): certificate_required",
                lambda: call(servers[(data, "required")], None, tag="tls-server-client-cert-required",
                             cert_of="stranger"))
            row("go optional: a stranger's certificate is not sent: accepted, no identity",
                lambda: call(servers[(data, "optional")], None, cert_of="stranger"))
        else:
            row(f"{client} required: a stranger's certificate refused, unknown_ca",
                lambda: call(servers[(data, "required")], None, tag="x509-unknown-issuer", cert_of="stranger"))
            row(f"{client} optional: a stranger's certificate refused too",
                lambda: call(servers[(data, "optional")], None, tag="x509-unknown-issuer", cert_of="stranger"))
        row(f"{client} off: a client that has a certificate is accepted and given no identity",
            lambda: call(servers[(data, None)], None, cert_of="p256"))
    for s in servers.values():
        s.stop()
    failed = sum(1 for _, bad in rows if bad is not None)
    print(f"{len(rows)} rows, {len(rows) - failed} ok, {failed} failed")
    print(f"the files of the run are in {work}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
