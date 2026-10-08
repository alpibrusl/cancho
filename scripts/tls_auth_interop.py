#!/usr/bin/env python3
"""`packages/tls`'s client certificates and ALPN against real servers (docs/tls-parity.md §6.10).

    python3 scripts/tls_auth_interop.py <fetch> [openssl nginx go wolfssl ...]

`fetch` is `examples/http_fetch_nb/fetch.cho` built with `packages/tls`, `packages/x509` and `packages/http-client`
(`--std`). Each row starts one server, made to ask for a client certificate (`require`, or `optional`), or to speak ALPN,
in TLS 1.3 or TLS 1.2, and runs `fetch` against it once with the options the row names. The servers:

    openssl   `openssl s_server -Verify 1` (require) or `-verify 1` (optional), `-alpn`; `-attime` moves its clock
    nginx     `ssl_verify_client on|optional` with `ssl_client_certificate`; `listen ... ssl http2` for ALPN
    go        scripts/interop/go_mtls_server.go: ClientAuth RequireAndVerifyClientCert or VerifyClientCertIfGiven
    wolfssl   scripts/interop/wolfssl_mtls_server.c: VERIFY_PEER with and without VERIFY_FAIL_IF_NO_PEER_CERT

The rows (some do not apply to every server):
- a valid client certificate is accepted, in TLS 1.3 and TLS 1.2 (`auth=1`), and where the server says what it saw
  (nginx, Go, wolfSSL put it in the body) the body names the certificate's common name;
- a certificate from another CA is refused by the server; none configured, against `require`, ends `tls-alert`; with
  `optional` it completes (`auth=2`, the empty Certificate), and with a certificate too (`auth=1`);
- a certificate that has expired is refused by the client before any connection (`tls-client-cert-expired`), and, with
  OpenSSL's clock moved past it (`-attime`), by the server; an RSA key is refused by the client (`tls-client-key-type`);
- ALPN: the offer that the server also speaks is chosen and reported; a server decides between two (its order, or the client's); an offer
  with nothing in common completes without a protocol or ends in the server's `no_application_protocol`, whichever the
  server does; no offer gets no protocol from a server that has a list.
One line a row, `ok` or what differed, then a count. Exit status 1 if any row differed.
"""
import datetime
import hashlib
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, rsa
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INTEROP = os.path.join(ROOT, "scripts/interop")
HOST = "mtls.lex-sys.test"
NOW = datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0)


def key_pem(key):
    return key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption())


def make_ca(name):
    key = ec.generate_private_key(ec.SECP256R1())
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, name)])
    cert = (x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key())
            .serial_number(x509.random_serial_number()).not_valid_before(NOW - datetime.timedelta(days=1))
            .not_valid_after(NOW + datetime.timedelta(days=30))
            .add_extension(x509.BasicConstraints(ca=True, path_length=None), True)
            .add_extension(x509.KeyUsage(True, False, False, False, False, True, True, False, False), True)
            .sign(key, hashes.SHA256()))
    return key, cert


def make_leaf(ca, name, key=None, not_before=None, not_after=None, server=False, bad_key=None):
    key = key or ec.generate_private_key(ec.SECP256R1())
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, name)])
    builder = (x509.CertificateBuilder().subject_name(subject).issuer_name(ca[1].subject).public_key(key.public_key())
               .serial_number(x509.random_serial_number())
               .not_valid_before(not_before or NOW - datetime.timedelta(hours=1))
               .not_valid_after(not_after or NOW + datetime.timedelta(days=30)))
    if server:
        builder = builder.add_extension(x509.SubjectAlternativeName([x509.DNSName(HOST)]), False)
        builder = builder.add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]), False)
    else:
        builder = builder.add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.CLIENT_AUTH]), False)
    return key, builder.sign(ca[0], hashes.SHA256())


class Material:
    """The certificates of a run, as files in `work`."""

    def __init__(self, work):
        self.work = work
        ca, other = make_ca("tls_auth_interop ca"), make_ca("tls_auth_interop other ca")
        self.files = {}
        self.write("ca.pem", ca[1].public_bytes(serialization.Encoding.PEM))
        k, c = make_leaf(ca, HOST, server=True)
        self.write("server.pem", c.public_bytes(serialization.Encoding.PEM))
        self.write("server.key", key_pem(k))
        k, c = make_leaf(ca, "client-good")
        self.write("good.pem", c.public_bytes(serialization.Encoding.PEM))
        self.write("good.key", key_pem(k))
        k, c = make_leaf(other, "client-other-ca")
        self.write("wrongca.pem", c.public_bytes(serialization.Encoding.PEM))
        self.write("wrongca.key", key_pem(k))
        k, c = make_leaf(ca, "client-expired", not_before=NOW - datetime.timedelta(days=10),
                         not_after=NOW - datetime.timedelta(days=5))
        self.write("expired.pem", c.public_bytes(serialization.Encoding.PEM))
        self.write("expired.key", key_pem(k))
        # Valid today, expired a day from now: OpenSSL's `-attime` puts the server's clock past it.
        k, c = make_leaf(ca, "client-soon", not_after=NOW + datetime.timedelta(days=1))
        self.write("soon.pem", c.public_bytes(serialization.Encoding.PEM))
        self.write("soon.key", key_pem(k))
        k = rsa.generate_private_key(65537, 2048)
        subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "client-rsa")])
        c = (x509.CertificateBuilder().subject_name(subject).issuer_name(ca[1].subject).public_key(k.public_key())
             .serial_number(x509.random_serial_number()).not_valid_before(NOW - datetime.timedelta(hours=1))
             .not_valid_after(NOW + datetime.timedelta(days=30)).sign(ca[0], hashes.SHA256()))
        self.write("rsa.pem", c.public_bytes(serialization.Encoding.PEM))
        self.write("rsa.key", key_pem(k))
        self.attime = int((NOW + datetime.timedelta(days=3)).timestamp())

    def write(self, name, data):
        path = os.path.join(self.work, name)
        open(path, "wb").write(data)
        self.files[name] = path

    def path(self, name):
        return self.files[name]


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


class Server:
    def __init__(self, argv, work, port, env=None):
        self.port = port
        self.proc = subprocess.Popen(argv, cwd=work, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                     stderr=subprocess.DEVNULL, env=env)
        for _ in range(200):
            if self.proc.poll() is not None:
                raise RuntimeError(f"exited with {self.proc.returncode}")
            try:
                socket.create_connection(("127.0.0.1", port), 0.1).close()
                return
            except OSError:
                threading.Event().wait(0.05)
        raise RuntimeError("never listened")

    def stop(self):
        self.proc.kill()
        self.proc.wait()


NGINX = """daemon off; master_process off; worker_processes 1; error_log stderr; pid nginx.pid;
events {{ worker_connections 64; }}
http {{
  access_log off; client_body_temp_path .; proxy_temp_path .; fastcgi_temp_path .; uwsgi_temp_path .; scgi_temp_path .;
  server {{
    listen 127.0.0.1:{port} ssl{http2};
    ssl_certificate {cert}; ssl_certificate_key {key};
    ssl_protocols {protocol};
    {directive} {ca};
    ssl_verify_client {mode};
    location / {{ return 200 "client=$ssl_client_s_dn alpn=$ssl_alpn_protocol"; }}
  }}
}}
"""


def launch(kind, m, work, mode, version, alpn, attime=False, nolist=False):
    """A server of `kind`; mode is none, optional or require; version "1.2" or "1.3"; alpn a comma list or "". `nolist`:
    the CertificateRequest names no authorities (OpenSSL's `-verifyCAfile`, nginx's `ssl_trusted_certificate`), so a client
    sends whatever certificate it has and the server's own verification decides."""
    port = free_port()
    cert, key, ca = m.path("server.pem"), m.path("server.key"), m.path("ca.pem")
    if kind == "openssl":
        argv = ["openssl", "s_server", "-accept", f"127.0.0.1:{port}", "-cert", cert, "-key", key, "-www",
                "-verifyCAfile" if nolist else "-CAfile", ca, "-verify_return_error",
                "-tls1_2" if version == "1.2" else "-tls1_3"]
        if mode == "require":
            argv += ["-Verify", "1"]
        elif mode == "optional":
            argv += ["-verify", "1"]
        if alpn:
            argv += ["-alpn", alpn]
        if attime:
            argv += ["-attime", str(m.attime)]
        return Server(argv, work, port)
    if kind == "nginx":
        http2 = " http2" if "h2" in alpn.split(",") else ""
        conf = os.path.join(work, f"nginx-{port}.conf")
        open(conf, "w").write(NGINX.format(port=port, http2=http2, cert=cert, key=key, ca=ca,
                                           directive="ssl_trusted_certificate" if nolist else "ssl_client_certificate",
                                           protocol="TLSv1.2" if version == "1.2" else "TLSv1.3",
                                           mode={"none": "off", "optional": "optional", "require": "on"}[mode]))
        return Server([shutil.which("nginx"), "-c", conf, "-p", work], work, port)
    if kind == "go":
        return Server([os.path.join(work, "go_mtls"), str(port), cert, key, version, ca, mode, alpn], work, port)
    if kind == "wolfssl":
        return Server([os.path.join(work, "wolf_mtls"), str(port), cert, key, version, ca, mode, alpn], work, port)
    raise ValueError(kind)


def build_servers(work):
    """The Go and wolfSSL servers, built once; the kinds that cannot be run, with why."""
    out = {}
    for kind, argv, exe in [
        ("go", ["go", "build", "-o", os.path.join(work, "go_mtls"), os.path.join(INTEROP, "go_mtls_server.go")], "go_mtls"),
        ("wolfssl", ["cc", "-O2", os.path.join(INTEROP, "wolfssl_mtls_server.c"), "-lwolfssl", "-o",
                     os.path.join(work, "wolf_mtls")], "wolf_mtls"),
    ]:
        try:
            subprocess.run(argv, cwd=work, check=True, capture_output=True, timeout=600)
            out[kind] = True
        except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
            out[kind] = f"cannot build: {str(getattr(e, 'stderr', b'') or e)[-120:]}"
    for kind, tool in [("openssl", "openssl"), ("nginx", "nginx")]:
        out[kind] = True if shutil.which(tool) else f"`{tool}` is not installed"
    return out


def run_fetch(fetch, m, port, client=None, alpn=None, extra=()):
    argv = [fetch, "--resolve", f"{HOST}=127.0.0.1", "--timeout", "10"]
    if client:
        argv += ["--client-cert", m.path(f"{client}.pem"), "--client-key", m.path(f"{client}.key")]
    if alpn is not None:
        argv += ["--alpn", alpn]
    argv += list(extra) + [f"https://{HOST}:{port}/"]
    r = subprocess.run(argv, input=open(m.path("ca.pem"), "rb").read(), capture_output=True, timeout=60)
    return r.returncode, r.stdout.decode(), r.stderr.decode()


def body_sha(text):
    return hashlib.sha256(text.encode()).hexdigest()


def parse(out):
    """The first response line: (status, length, sha, alpn, auth), or ("failed", tag)."""
    for line in out.splitlines():
        f = line.split()
        if len(f) >= 3 and f[1] == "failed":
            return ("failed", f[2])
        if len(f) >= 5 and f[1].isdigit():
            alpn = next((x[5:] for x in f if x.startswith("alpn=")), None)
            auth = next((int(x[5:]) for x in f if x.startswith("auth=")), None)
            return (int(f[1]), int(f[2]), f[3], alpn, auth)
    return None


def rows(kind):
    """(name, mode, version, client, server alpn, client alpn, expectation, attime, nolist), the expectation one of
    ("ok", auth, alpn) | ("failed",) | ("refused", tag) | ("either",)."""
    out = []
    for v in ("1.3", "1.2"):
        out += [
            (f"{v} require: a valid certificate", "require", v, "good", "", None, ("ok", 1, None), False, False),
            (f"{v} require: none configured", "require", v, None, "", "", ("failed",), False, False),
            (f"{v} require: a certificate from another CA", "require", v, "wrongca", "", None, ("failed",), False, False),
            (f"{v} optional: none configured", "optional", v, None, "", "", ("ok", 2, None), False, False),
            (f"{v} optional: a valid certificate", "optional", v, "good", "", None, ("ok", 1, None), False, False),
            (f"{v} none: a certificate configured and never asked for", "none", v, "good", "", None, ("ok", 0, None), False, False),
        ]
    out += [
        ("require: the client's own refusal of an expired certificate", "require", "1.3", "expired", "", None,
         ("refused", "tls-client-cert-expired"), False, False),
        ("require: the client's own refusal of an RSA key", "require", "1.3", "rsa", "", None,
         ("refused", "tls-client-key-type"), False, False),
    ]
    if kind == "openssl":
        out += [
            ("1.3 require: the server's clock past the certificate", "require", "1.3", "soon", "", None, ("failed",), True, True),
            ("1.2 require: the server's clock past the certificate", "require", "1.2", "soon", "", None, ("failed",), True, True),
        ]
    if kind == "openssl":
        # No authorities in the request: the client sends the certificate it has, and the server's verification refuses it.
        for v in ("1.3", "1.2"):
            out += [
                (f"{v} require, no authorities named: a certificate from another CA is sent and refused", "require", v,
                 "wrongca", "", None, ("failed",), False, True),
                (f"{v} require, no authorities named: a valid certificate", "require", v, "good", "", None,
                 ("ok", 1, None), False, True),
            ]
    # ALPN, with no client certificate asked for.
    for v in ("1.3", "1.2"):
        if kind == "nginx":
            speaks = "http/1.1,h2"
            out += [
                (f"{v} ALPN: the offer http/1.1", "none", v, None, speaks, "http/1.1", ("ok", 0, "http/1.1"), False, False),
                (f"{v} ALPN: no offer", "none", v, None, speaks, "", ("ok", 0, "-"), False, False),
                (f"{v} ALPN: nothing in common (spdy/3)", "none", v, None, speaks, "spdy/3", ("either",), False, False),
            ]
            continue
        out += [
            (f"{v} ALPN: the offer http/1.1", "none", v, None, "http/1.1", "http/1.1", ("ok", 0, "http/1.1"), False, False),
            (f"{v} ALPN: two offered, the server's order decides", "none", v, None, "http/1.1,h2", "h2,http/1.1",
             ("ok", 0, ("http/1.1", "h2")), False, False),
            (f"{v} ALPN: no offer to a server with a list", "none", v, None, "http/1.1", "", ("ok", 0, "-"), False, False),
            (f"{v} ALPN: nothing in common (spdy/3)", "none", v, None, "http/1.1", "spdy/3", ("either",), False, False),
        ]
    return out


def check(kind, expect, got, rc, stdout, stderr, client):
    """None when `got` is what `expect` says, else why not."""
    if expect[0] == "refused":
        if rc != 2 or expect[1] not in stderr:
            return f"wanted the client's {expect[1]}, got exit {rc}: {stderr.strip()[-80:]}"
        return None
    if expect[0] == "failed":
        if kind == "nginx" and got is not None and got[0] == 400:
            # nginx completes the handshake and answers 400 "No required SSL certificate was sent".
            return None
        if got is None or got[0] != "failed" or rc != 1:
            return f"wanted a refused connection, got {got} (exit {rc})"
        return None
    if expect[0] == "either":
        if got is None:
            return f"nothing: exit {rc}"
        if got[0] == "failed":
            return None
        expect = ("ok", 0, "-")
    if got is None or got[0] == "failed":
        return f"wanted a response, got {got} (exit {rc}) {stderr.strip()[-80:]}"
    status, length, sha, alpn, auth = got
    if status != 200:
        return f"status {status}"
    _, want_auth, want_alpn = expect
    if want_alpn is not None and alpn not in (want_alpn if isinstance(want_alpn, tuple) else (want_alpn,)):
        return f"ALPN {alpn}, wanted {want_alpn}"
    if want_auth is not None and auth is not None and auth != want_auth:
        return f"auth {auth}, wanted {want_auth}"
    if kind in ("go", "wolfssl", "nginx") and want_auth in (0, 1, 2):
        # The server's own account of the connection: who it saw and what it negotiated.
        who = {1: "client-good"}.get(want_auth, "none")
        proto = "none" if alpn in (None, "-") else alpn
        if kind == "nginx":
            text = f"client={'CN=' + who if who != 'none' else ''} alpn={'' if proto == 'none' else proto}"
        else:
            text = f"client={who} alpn={proto}"
        if sha != body_sha(text):
            return f"the server's account differs: wanted {text!r}"
    return None


def main():
    fetch = sys.argv[1]
    wanted = sys.argv[2:]
    work = tempfile.mkdtemp(prefix="tls-auth-interop-")
    m = Material(work)
    available = build_servers(work)
    bad = total = 0
    for kind in ("openssl", "nginx", "go", "wolfssl"):
        if wanted and kind not in wanted:
            continue
        if available[kind] is not True:
            print(f"{kind}: skipped: {available[kind]}")
            continue
        for name, mode, version, client, s_alpn, c_alpn, expect, attime, nolist in rows(kind):
            total += 1
            try:
                server = launch(kind, m, work, mode, version, s_alpn, attime, nolist)
            except RuntimeError as e:
                print(f"{kind}: {name}: server did not start: {e}")
                bad += 1
                continue
            try:
                rc, out, err = run_fetch(fetch, m, server.port, client, c_alpn)
            finally:
                server.stop()
            why = check(kind, expect, parse(out), rc, out, err, client)
            tail = parse(out)
            print(f"{kind}: {name}: " + ("ok" if why is None else f"DIFFERS: {why}") + (f"   [{tail}]" if why or expect[0] == "either" else ""))
            bad += why is not None
    print(f"{total - bad} of {total} rows ok")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
