#!/usr/bin/env python3
"""`packages/tls`'s server against the TLS clients this machine can run (docs/tls-server.md §8, step 2).

    python3 scripts/tls_server_interop.py <tls_serve> [--corpus <dir>] [<client> ...]

`tls_serve` is `tests/programs/tls_serve.ls` built with `packages/tls` and `packages/x509`. It runs in the image
`scripts/interop/server.Dockerfile` describes (Ubuntu 24.04: OpenSSL 3.0.13, curl 8.5.0, Go 1.22, wolfSSL 5.6.6,
mosquitto 2.0); `/etc/hosts` must name `srv.example`, `other.example` and `a.wild.example` as 127.0.0.1, as
`docker run --add-host` does. The clients, each skipped with the reason when it cannot run:

    openssl    `openssl s_client`, an HTTP request, to the server in `http` mode
    curl       curl, to the same
    go         Go's crypto/tls (scripts/interop/go_client.go; `go` on PATH), to the server in `echo` mode
    wolfssl    wolfSSL (scripts/interop/wolfssl_client.c; libwolfssl-dev), to the same
    mosquitto  `mosquitto_sub` and `mosquitto_pub`, to the server in `mqtt` mode

Each row is one connection with one suite and one group, the client made to offer that suite only and a key share
of that group only, or, for a `retry` row, a key share of P-521 with the group after it in `supported_groups`, so
the server must ask with a HelloRetryRequest. curl and mosquitto have no option for TLS 1.3's groups or suites, so
they are set for them in an `OPENSSL_CONF` (`Groups`, `Ciphersuites`). Go cannot be told TLS 1.3's suites, so its
rows are its groups, with the suite the server prefers. Then rows for SNI choosing the identity (the second, by its
wildcard; an unknown name and none, the default) and for ALPN agreed and refused.

A row passes when the client completes (and checks the chain and the name against the row's CA), its data comes
back, and the server's line for the connection (`tls_serve`'s `conn ...`) shows the suite, group, name and protocol
the row asked for, and whether a HelloRetryRequest came first. A refused row passes when the client fails and the
server's line has the tag. One line a row, then a count; exit status 1 if any row failed.

With `--corpus <dir>`, each client's ClientHello as it sends it by default (and Chromium's, if `CHROMIUM_HELLO`
names a file holding one) is first caught by a listener that answers nothing, and written to `<dir>` in
`fuzz_server`'s input format (a 2-byte length, then the records), and its length printed: docs/tls-server.md §5.2
records the largest.
"""
import datetime
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INTEROP = os.path.join(ROOT, "scripts/interop")
SUITES = {"TLS_AES_128_GCM_SHA256": 0x1301, "TLS_AES_256_GCM_SHA384": 0x1302, "TLS_CHACHA20_POLY1305_SHA256": 0x1303}
WOLFSSL = {"TLS_AES_128_GCM_SHA256": "TLS13-AES128-GCM-SHA256", "TLS_AES_256_GCM_SHA384": "TLS13-AES256-GCM-SHA384",
           "TLS_CHACHA20_POLY1305_SHA256": "TLS13-CHACHA20-POLY1305-SHA256"}
GROUPS = {"X25519": 0x1D, "P-256": 0x17, "P-384": 0x18}
GO_CURVE = {"X25519": "X25519", "P-256": "P256", "P-384": "P384"}
MESSAGE = "hello over TLS 1.3"


def authority(work):
    """A P-256 CA and two P-256 identities: `srv.example` and `*.wild.example`, and `other.example`."""
    now = datetime.datetime.now(datetime.timezone.utc)
    ca_key = ec.generate_private_key(ec.SECP256R1())
    ca_name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "tls_server_interop CA")])
    ca = (x509.CertificateBuilder().subject_name(ca_name).issuer_name(ca_name).public_key(ca_key.public_key())
          .serial_number(x509.random_serial_number()).not_valid_before(now - datetime.timedelta(days=1))
          .not_valid_after(now + datetime.timedelta(days=30))
          .add_extension(x509.BasicConstraints(ca=True, path_length=None), True)
          .add_extension(x509.KeyUsage(False, False, False, False, False, True, True, False, False), True)
          .add_extension(x509.SubjectKeyIdentifier.from_public_key(ca_key.public_key()), False)
          .sign(ca_key, hashes.SHA256()))
    pem = lambda c: c.public_bytes(serialization.Encoding.PEM)
    open(os.path.join(work, "ca.pem"), "wb").write(pem(ca))
    for name, sans in [("main", ["srv.example", "*.wild.example"]), ("other", ["other.example"])]:
        key = ec.generate_private_key(ec.SECP256R1())
        cert = (x509.CertificateBuilder().subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, sans[0])]))
                .issuer_name(ca_name).public_key(key.public_key()).serial_number(x509.random_serial_number())
                .not_valid_before(now - datetime.timedelta(days=1)).not_valid_after(now + datetime.timedelta(days=30))
                .add_extension(x509.SubjectAlternativeName([x509.DNSName(s) for s in sans]), False)
                .add_extension(x509.KeyUsage(True, False, False, False, False, False, False, False, False), True)
                .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]), False)
                .add_extension(x509.AuthorityKeyIdentifier.from_issuer_public_key(ca_key.public_key()), False)
                .sign(ca_key, hashes.SHA256()))
        open(os.path.join(work, f"{name}.pem"), "wb").write(pem(cert) + pem(ca))
        open(os.path.join(work, f"{name}.key"), "wb").write(key.private_bytes(
            serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


class Server:
    """`tls_serve` in one mode, its `conn` lines collected as they come."""

    def __init__(self, exe, mode, work):
        self.port = free_port()
        self.proc = subprocess.Popen([exe, str(self.port), mode, "http/1.1,mqtt", "0", "-",
                                      "main.pem", "main.key", "srv.example,*.wild.example",
                                      "other.pem", "other.key", "other.example"],
                                     cwd=work, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
        self.lines, self.cond = [], threading.Condition()
        first = self.proc.stdout.readline()
        if first.strip() != "listening":
            raise RuntimeError(f"tls_serve: {first.strip()} {self.proc.stdout.read()[:300]}")
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        for line in self.proc.stdout:
            with self.cond:
                self.lines.append(line.strip())
                self.cond.notify_all()

    def next_conn(self, since, wait=5.0):
        """The first `conn` line after line `since`, waiting for it."""
        end = time.time() + wait
        with self.cond:
            while True:
                for line in self.lines[since:]:
                    if line.startswith("conn "):
                        return line
                left = end - time.time()
                if left <= 0:
                    return None
                self.cond.wait(left)

    def mark(self):
        with self.cond:
            return len(self.lines)

    def published(self, since):
        with self.cond:
            return [l for l in self.lines[since:] if l.startswith("publish ")]

    def stop(self):
        self.proc.kill()
        self.proc.wait()


def openssl_conf(work, suite, groups):
    path = os.path.join(work, "openssl.cnf")
    open(path, "w").write(f"""openssl_conf = c
[c]
ssl_conf = s
[s]
system_default = d
[d]
MinProtocol = TLSv1.3
Ciphersuites = {suite}
Groups = {groups}
""")
    return path


def groups_for(group, retry):
    """The client's groups: a key share of `group`, or of P-521 first so the server asks for `group`."""
    return f"P-521:{group}" if retry else group


def check(server, since, want_suite, want_group, retry, sni, alpn, tag="ok"):
    """None when the server's line for the connection says what the row asked for, else what it says."""
    line = server.next_conn(since)
    if line is None:
        return "no line from the server"
    f = line.split()
    # conn <code> <tag> <name> <alpn> <bytes> <suite> <group> <retried|direct>
    if f[2] != tag:
        return line
    if tag != "ok":
        return None
    want = [sni or "-", alpn or "-"]
    if want_suite is not None:
        want.append(str(want_suite))
    else:
        want.append(f[6])
    want += [str(want_group), "retried" if retry else "direct"]
    got = [f[3], f[4], f[6], f[7], f[8]]
    return None if got == want else line


def run(argv, env=None, input=None, timeout=20):
    try:
        r = subprocess.run(argv, input=input, capture_output=True, timeout=timeout, env=env)
        return r.returncode, r.stdout.decode(errors="replace") + r.stderr.decode(errors="replace")
    except subprocess.TimeoutExpired:
        return -1, "timed out"


def openssl_row(srv, work, suite, group, retry, sni="srv.example", alpn=None, refuse=None):
    args = ["openssl", "s_client", "-connect", f"127.0.0.1:{srv.port}", "-CAfile", "ca.pem", "-verify_return_error",
            "-ign_eof", "-quiet", "-tls1_3", "-ciphersuites", suite, "-groups", groups_for(group, retry)]
    if sni:
        args += ["-servername", sni, "-verify_hostname", sni]
    else:
        args += ["-noservername", "-verify_hostname", "srv.example"]
    if alpn:
        args += ["-alpn", alpn]
    since = srv.mark()
    code, out = run(args, input=b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
    if refuse:
        bad = check(srv, since, None, 0, False, None, None, refuse)
        return bad if bad else (None if code != 0 else "the client did not fail")
    if code != 0 or "hello from lex-sys" not in out:
        return f"s_client {code}: {out.strip()[-200:]}"
    name = sni if sni in ("srv.example", "other.example", "a.wild.example") else None
    return check(srv, since, SUITES[suite], GROUPS[group], retry, sni, "http/1.1" if alpn == "http/1.1" else None)


def curl_row(srv, work, suite, group, retry):
    env = dict(os.environ, OPENSSL_CONF=openssl_conf(work, suite, groups_for(group, retry)))
    since = srv.mark()
    code, out = run(["curl", "-sS", "--http1.1", "--cacert", "ca.pem", f"https://srv.example:{srv.port}/"], env=env)
    if code != 0 or "hello from lex-sys, name srv.example, alpn http/1.1" not in out:
        return f"curl {code}: {out.strip()[-200:]}"
    return check(srv, since, SUITES[suite], GROUPS[group], retry, "srv.example", "http/1.1")


def go_row(srv, work, exe, group, retry, sni="srv.example", alpn="-", refuse=None, chosen=None):
    curves = ("P521," if retry else "") + GO_CURVE[group]
    since = srv.mark()
    code, out = run([exe, f"127.0.0.1:{srv.port}", sni, "ca.pem", curves, alpn, MESSAGE])
    if refuse:
        bad = check(srv, since, None, 0, False, None, None, refuse)
        return bad if bad else (None if code != 0 else "the client did not fail")
    if code != 0 or not out.startswith("ok 304"):
        return f"go {code}: {out.strip()[-200:]}"
    return check(srv, since, None, GROUPS[group], retry, sni, chosen)


def wolfssl_row(srv, work, exe, suite, group, retry, alpn="-"):
    groups = ("P521," if retry else "") + GO_CURVE[group]
    since = srv.mark()
    code, out = run([exe, str(srv.port), "srv.example", "ca.pem", WOLFSSL[suite], groups, alpn, MESSAGE])
    if code != 0 or not out.startswith("ok "):
        return f"wolfssl {code}: {out.strip()[-200:]}"
    return check(srv, since, SUITES[suite], GROUPS[group], retry, "srv.example", None if alpn == "-" else alpn)


def mosquitto_row(srv, work, suite, group, retry, alpn=None):
    env = dict(os.environ, OPENSSL_CONF=openssl_conf(work, suite, groups_for(group, retry)))
    common = ["-h", "srv.example", "-p", str(srv.port), "--cafile", "ca.pem", "--tls-version", "tlsv1.3",
              "-t", "lexsys/interop", "-i", "lexsys-interop"]
    if alpn:
        common += ["--tls-alpn", alpn]
    since = srv.mark()
    code, out = run(["mosquitto_sub", *common, "-C", "1", "-W", "10"], env=env)
    if code != 0 or out.strip() != "hello from lex-sys":
        return f"mosquitto_sub {code}: {out.strip()[-200:]}"
    bad = check(srv, since, SUITES[suite], GROUPS[group], retry, "srv.example", alpn)
    if bad:
        return "sub: " + bad
    since = srv.mark()
    payload = f"from mosquitto_pub {suite} {group}"
    code, out = run(["mosquitto_pub", *common, "-m", payload], env=env)
    if code != 0:
        return f"mosquitto_pub {code}: {out.strip()[-200:]}"
    bad = check(srv, since, SUITES[suite], GROUPS[group], retry, "srv.example", alpn)
    if bad:
        return "pub: " + bad
    if f"publish lexsys/interop {payload}" not in srv.published(since):
        return "pub: the server did not print the message"
    return None


def capture(argvs, corpus, work):
    """Each client's ClientHello, caught by a listener that answers nothing."""
    sizes = []
    for name, (argv, env) in argvs.items():
        listener = socket.socket()
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)
        port = listener.getsockname()[1]
        got = {}

        def catch():
            conn, _ = listener.accept()
            conn.settimeout(5)
            data = b""
            try:
                while True:
                    chunk = conn.recv(65536)
                    if not chunk:
                        break
                    data += chunk
                    # The whole ClientHello: its handshake length, across records.
                    body, at = b"", 0
                    while at + 5 <= len(data) and at + 5 + int.from_bytes(data[at + 3:at + 5], "big") <= len(data):
                        n = int.from_bytes(data[at + 3:at + 5], "big")
                        body += data[at + 5:at + 5 + n]
                        at += 5 + n
                    if len(body) >= 4 and len(body) >= 4 + int.from_bytes(body[1:4], "big"):
                        got["records"], got["hello"] = data[:at], body[:4 + int.from_bytes(body[1:4], "big")]
                        break
            except OSError:
                pass
            conn.close()

        t = threading.Thread(target=catch, daemon=True)
        t.start()
        run([a.replace("PORT", str(port)) for a in argv], env=env, input=b"", timeout=8)
        t.join(6)
        listener.close()
        if "hello" not in got:
            print(f"capture {name}: nothing caught")
            continue
        hello = got["hello"]
        sizes.append((len(hello), name))
        rec = got["records"]
        open(os.path.join(corpus, f"hello_{name}"), "wb").write(len(rec).to_bytes(2, "big") + rec)
        print(f"capture {name}: a ClientHello of {len(hello)} bytes ({len(hello) - 4} after its header)")
    return sizes


def main():
    exe = os.path.abspath(sys.argv[1])
    args = sys.argv[2:]
    corpus = None
    if "--corpus" in args:
        i = args.index("--corpus")
        corpus = os.path.abspath(args[i + 1])
        args = args[:i] + args[i + 2:]
    wanted = args or ["openssl", "curl", "go", "wolfssl", "mosquitto"]
    work = tempfile.mkdtemp(prefix="tls-server-interop-")
    os.chdir(work)
    authority(work)
    tools = {}
    if shutil.which("go"):
        r = subprocess.run(["go", "build", "-o", f"{work}/go_client", f"{INTEROP}/go_client.go"], cwd=work,
                           capture_output=True, env=dict(os.environ, GOFLAGS="-mod=mod", HOME=work))
        tools["go"] = f"{work}/go_client" if r.returncode == 0 else (None, r.stderr.decode()[-200:])
    else:
        tools["go"] = (None, "`go` is not installed")
    r = subprocess.run(["cc", "-O2", f"{INTEROP}/wolfssl_client.c", "-lwolfssl", "-o", f"{work}/wolfssl_client"],
                       capture_output=True)
    tools["wolfssl"] = f"{work}/wolfssl_client" if r.returncode == 0 else (None, r.stderr.decode()[-200:])
    for name, tool in [("openssl", "openssl"), ("curl", "curl"), ("mosquitto", "mosquitto_pub")]:
        tools[name] = shutil.which(tool) or (None, f"`{tool}` is not installed")

    if corpus:
        os.makedirs(corpus, exist_ok=True)
        argvs = {
            "openssl": (["openssl", "s_client", "-connect", "127.0.0.1:PORT", "-servername", "srv.example"], None),
            "curl": (["curl", "-s", "https://srv.example:PORT/"], None),
            "mosquitto": (["mosquitto_pub", "-h", "srv.example", "-p", "PORT", "--cafile", "ca.pem", "-t", "t",
                           "-m", "m"], None),
        }
        if isinstance(tools["go"], str):
            argvs["go"] = ([tools["go"], "127.0.0.1:PORT", "srv.example", "ca.pem", "X25519,P256,P384", "h2,http/1.1",
                            "x"], None)
        if isinstance(tools["wolfssl"], str):
            argvs["wolfssl"] = ([tools["wolfssl"], "PORT", "srv.example", "ca.pem", "TLS13-AES128-GCM-SHA256",
                                 "X25519,P256,P384", "-", "x"], None)
        sizes = capture(argvs, corpus, work)
        chromium = os.environ.get("CHROMIUM_HELLO")
        if chromium:
            rec = open(chromium, "rb").read()
            open(os.path.join(corpus, "hello_chromium"), "wb").write(len(rec).to_bytes(2, "big") + rec)
            n = 0
            at = 0
            while at + 5 <= len(rec):
                n += int.from_bytes(rec[at + 3:at + 5], "big")
                at += 5 + int.from_bytes(rec[at + 3:at + 5], "big")
            sizes.append((n, "chromium"))
            print(f"capture chromium: a ClientHello of {n} bytes, from {chromium}")
        if sizes:
            print(f"the largest ClientHello: {max(sizes)[0]} bytes, {max(sizes)[1]}")

    servers = {mode: Server(exe, mode, work) for mode in ("http", "echo", "mqtt")}
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
        for retry in (False, True):
            for group in GROUPS:
                if client == "go":
                    row(f"go {group}{' retry' if retry else ''}",
                        lambda: go_row(servers["echo"], work, tool, group, retry))
                    continue
                for suite in SUITES:
                    name = f"{client} {suite} {group}{' retry' if retry else ''}"
                    if client == "openssl":
                        row(name, lambda: openssl_row(servers["http"], work, suite, group, retry))
                    elif client == "curl":
                        row(name, lambda: curl_row(servers["http"], work, suite, group, retry))
                    elif client == "wolfssl":
                        row(name, lambda: wolfssl_row(servers["echo"], work, tool, suite, group, retry))
                    elif client == "mosquitto":
                        row(name, lambda: mosquitto_row(servers["mqtt"], work, suite, group, retry))
        # SNI and ALPN.
        s13 = "TLS_CHACHA20_POLY1305_SHA256"
        if client == "openssl":
            row("openssl SNI chooses the second identity",
                lambda: openssl_row(servers["http"], work, s13, "X25519", False, sni="other.example"))
            row("openssl SNI by a wildcard name",
                lambda: openssl_row(servers["http"], work, s13, "X25519", False, sni="a.wild.example"))
            row("openssl no SNI: the default identity",
                lambda: openssl_row(servers["http"], work, s13, "X25519", False, sni=None))
            row("openssl ALPN http/1.1 agreed",
                lambda: openssl_row(servers["http"], work, s13, "X25519", False, alpn="http/1.1"))
            row("openssl ALPN h3 only: refused, no_application_protocol",
                lambda: openssl_row(servers["http"], work, s13, "X25519", False, alpn="h3", refuse="tls-server-alpn"))
        elif client == "go":
            row("go SNI chooses the second identity",
                lambda: go_row(servers["echo"], work, tool, "X25519", False, sni="other.example"))
            row("go ALPN h2 and mqtt offered: mqtt agreed",
                lambda: go_row(servers["echo"], work, tool, "X25519", False, alpn="h2,mqtt", chosen="mqtt"))
            row("go ALPN h3 only: refused", lambda: go_row(servers["echo"], work, tool, "X25519", False, alpn="h3",
                                                       refuse="tls-server-alpn"))
        elif client == "wolfssl":
            row("wolfssl ALPN mqtt agreed",
                lambda: wolfssl_row(servers["echo"], work, tool, s13, "X25519", False, alpn="mqtt"))
        elif client == "mosquitto":
            row("mosquitto ALPN mqtt agreed",
                lambda: mosquitto_row(servers["mqtt"], work, s13, "X25519", False, alpn="mqtt"))
    for s in servers.values():
        s.stop()
    failed = sum(1 for _, bad in rows if bad is not None)
    print(f"{len(rows)} rows, {len(rows) - failed} ok, {failed} failed")
    shutil.rmtree(work, ignore_errors=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
