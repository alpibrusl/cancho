#!/usr/bin/env python3
"""`packages/tls` against every other TLS server this machine can run
(docs/tls-assurance.md §5). `scripts/tls_live.py` covers OpenSSL's
`s_server`, Python `ssl` and tlslite-ng; this covers the rest.

    python3 scripts/tls_interop.py <tls_many> [<server> ...]

`tls_many` is `tests/programs/tls_many.cho` built with `packages/tls` and
`packages/x509`. The servers, each skipped with the reason when it cannot run:

    go        Go's crypto/tls (scripts/interop/go_server.go; `go` on PATH)
    rustls    rustls 0.23 on ring (scripts/interop/rustls_server; `cargo`)
    wolfssl   wolfSSL (scripts/interop/wolfssl_server.c; libwolfssl-dev)
    mbedtls   mbedTLS 2.28, TLS 1.2 only (scripts/interop/mbedtls_server.c; libmbedtls-dev)
    botan     Botan 2.19, TLS 1.2 only, Botan 2 having no TLS 1.3
              (scripts/interop/botan_server.cpp; libbotan-2-dev)
    boringssl Android's build of BoringSSL (scripts/interop/boringssl_server.c;
              android-libboringssl-dev)
    nginx     nginx, on OpenSSL
    gnutls    GnuTLS's `gnutls-serv`

For each server, each row is one certificate key type, one version and one
suite, the server made to offer that suite only (or its default, `*`):
every suite the server can be told to use with a P-256 certificate (an RSA
one for TLS 1.2's RSA suites), and every certificate type with the server's
default suite. `tls_many` makes 8 connections at once, reading one byte at a
time and then 65,536 at a time; every connection must end `ok`, having read
the whole response up to the server's close_notify. Each CA is made for the
row and is all `tls_many` trusts. One line a row, `ok` or what failed, then
a count. Exit status 1 if any row failed.

Each server with TLS 1.3 has one more row, `resume` (docs/tls-resumption.md): `tls_many resume` makes its 8
connections, keeps each one's ticket, and makes 8 more offering them. Every connection of both rounds must
end `ok`, and every second-round connection must have resumed, unless `NO_RESUME` names the server with the
reason it does not.
"""
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_live  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INTEROP = os.path.join(ROOT, "scripts/interop")
CERTS = ["p256", "p384", "rsa2048", "rsa4096", "ed25519"]
ECDSA12 = ["ECDHE-ECDSA-AES128-GCM-SHA256", "ECDHE-ECDSA-AES256-GCM-SHA384", "ECDHE-ECDSA-CHACHA20-POLY1305"]
RSA12 = ["ECDHE-RSA-AES128-GCM-SHA256", "ECDHE-RSA-AES256-GCM-SHA384", "ECDHE-RSA-CHACHA20-POLY1305"]
SUITES13 = ["TLS_AES_128_GCM_SHA256", "TLS_AES_256_GCM_SHA384", "TLS_CHACHA20_POLY1305_SHA256"]


def iana12(name):
    """An OpenSSL TLS 1.2 suite name as IANA (and Go, and rustls) spell it."""
    kx, auth, *rest = name.split("-")
    cipher = {"AES128-GCM": "AES_128_GCM", "AES256-GCM": "AES_256_GCM", "CHACHA20-POLY1305": "CHACHA20_POLY1305"}
    body = "-".join(rest)
    for k, v in cipher.items():
        if body.startswith(k):
            tail = body[len(k):].lstrip("-")
            return f"TLS_ECDHE_{auth}_WITH_{v}" + (f"_{tail}" if tail else "_SHA256")
    raise ValueError(name)


class Server:
    """A server process on a free port, serving one certificate."""

    def __init__(self, argv, work, port, env=None):
        self.port = port
        self.work = work
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


def build(scratch):
    """Each server's binary, built once into `scratch`; a name to None (and
    why) when it cannot be."""
    out = {}

    def attempt(name, argv, exe, cwd=None):
        try:
            subprocess.run(argv, cwd=cwd, check=True, capture_output=True, timeout=1200)
            out[name] = exe
        except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
            detail = getattr(e, "stderr", b"") or str(e).encode()
            out[name] = None, f"cannot build: {detail.decode(errors='replace').strip().splitlines()[-1:]}"

    attempt("go", ["go", "build", "-o", f"{scratch}/go_server", f"{INTEROP}/go_server.go"], f"{scratch}/go_server", cwd=scratch)
    attempt("rustls", ["cargo", "build", "--release", "--quiet", "--manifest-path", f"{INTEROP}/rustls_server/Cargo.toml",
                       "--target-dir", f"{scratch}/rustls"], f"{scratch}/rustls/release/rustls_server")
    attempt("wolfssl", ["cc", "-O2", f"{INTEROP}/wolfssl_server.c", "-lwolfssl", "-o", f"{scratch}/wolfssl_server"],
            f"{scratch}/wolfssl_server")
    attempt("mbedtls", ["cc", "-O2", f"{INTEROP}/mbedtls_server.c", "-lmbedtls", "-lmbedx509", "-lmbedcrypto", "-o",
                        f"{scratch}/mbedtls_server"], f"{scratch}/mbedtls_server")
    attempt("botan", ["c++", "-O2", "-std=c++17", "-I/usr/include/botan-2", f"{INTEROP}/botan_server.cpp", "-lbotan-2",
                      "-o", f"{scratch}/botan_server"], f"{scratch}/botan_server")
    android = "/usr/lib/x86_64-linux-gnu/android"
    attempt("boringssl", ["cc", "-O2", "-I/usr/include/android", f"{INTEROP}/boringssl_server.c", f"-L{android}",
                          f"-Wl,-rpath,{android}", "-lssl", "-lcrypto", "-o", f"{scratch}/boringssl_server"],
            f"{scratch}/boringssl_server")
    for name, tool in [("nginx", "nginx"), ("gnutls", "gnutls-serv")]:
        out[name] = shutil.which(tool) or (None, f"`{tool}` is not installed")
    return out


NGINX = """daemon off; master_process off; worker_processes 1; error_log stderr; pid nginx.pid;
events {{ worker_connections 64; }}
http {{
  access_log off; client_body_temp_path .; proxy_temp_path .; fastcgi_temp_path .; uwsgi_temp_path .; scgi_temp_path .;
  server {{
    listen 127.0.0.1:{port} ssl;
    ssl_certificate c.pem; ssl_certificate_key k.pem;
    ssl_protocols {protocol};
    {suites}
    location / {{ return 200 "hello, cancho"; }}
  }}
}}
"""

# Botan's names for TLS 1.2's AEADs.
BOTAN_CIPHER = {"AES128-GCM": "AES-128/GCM", "AES256-GCM": "AES-256/GCM", "CHACHA20-POLY1305": "ChaCha20Poly1305"}

# wolfSSL's names for TLS 1.3's suites.
WOLFSSL13 = {"TLS_AES_128_GCM_SHA256": "TLS13-AES128-GCM-SHA256", "TLS_AES_256_GCM_SHA384": "TLS13-AES256-GCM-SHA384",
             "TLS_CHACHA20_POLY1305_SHA256": "TLS13-CHACHA20-POLY1305-SHA256"}

# GnuTLS priority names.
GNUTLS_CIPHER = {"TLS_AES_128_GCM_SHA256": "AES-128-GCM", "TLS_AES_256_GCM_SHA384": "AES-256-GCM",
                 "TLS_CHACHA20_POLY1305_SHA256": "CHACHA20-POLY1305"}


def fragment(suite):
    """The AEAD part of an OpenSSL TLS 1.2 suite name."""
    return next(c for c in ("AES128-GCM", "AES256-GCM", "CHACHA20-POLY1305") if c in suite)


def cipher12(suite):
    return BOTAN_CIPHER[fragment(suite)]


def gnutls12(suite):
    return {"AES128-GCM": "AES-128-GCM", "AES256-GCM": "AES-256-GCM", "CHACHA20-POLY1305": "CHACHA20-POLY1305"}[
        fragment(suite)]


def argv_for(name, exe, port, version, suite):
    """The command line for one row; `suite` None is the server's default.
    Returns (argv, env)."""
    v = "1.2" if version == "1.2" else "1.3"
    if name == "go":
        return [exe, str(port), "c.pem", "k.pem", v] + ([iana12(suite)] if suite and v == "1.2" else []), None
    if name == "rustls":
        if suite is None:
            return [exe, str(port), "c.pem", "k.pem", v], None
        return [exe, str(port), "c.pem", "k.pem", v, iana12(suite) if v == "1.2" else "TLS13_" + suite[4:]], None
    if name == "wolfssl":
        if suite is None:
            return [exe, str(port), "c.pem", "k.pem", v], None
        return [exe, str(port), "c.pem", "k.pem", v, WOLFSSL13[suite] if v == "1.3" else suite], None
    if name == "mbedtls":
        return [exe, str(port), "c.pem", "k.pem"] + (["TLS-" + iana12(suite)[4:].replace("_", "-")] if suite else []), None
    if name == "botan":
        return [exe, str(port), "c.pem", "k.pem"] + ([cipher12(suite)] if suite else []), None
    if name == "boringssl":
        return [exe, str(port), "c.pem", "k.pem", v] + ([suite] if suite and v == "1.2" else []), None
    if name == "nginx":
        if v == "1.2":
            suites = f"ssl_ciphers {suite or 'ECDHE+AESGCM:ECDHE+CHACHA20'};"
        else:
            suites = f"ssl_conf_command Ciphersuites {suite};" if suite else ""
        open("nginx.conf", "w").write(NGINX.format(port=port, protocol="TLSv1.2" if v == "1.2" else "TLSv1.3",
                                                    suites=suites))
        return [exe, "-p", ".", "-c", "nginx.conf", "-e", "stderr"], None
    if name == "gnutls":
        version_p = "+VERS-TLS1.2" if v == "1.2" else "+VERS-TLS1.3"
        cipher_p = ""
        if suite:
            c = GNUTLS_CIPHER[suite] if v == "1.3" else gnutls12(suite)
            cipher_p = f":-CIPHER-ALL:+{c}"
        return [exe, "--http", "-q", "-p", str(port), "--x509certfile", "c.pem", "--x509keyfile", "k.pem",
                "--priority", f"NORMAL:-VERS-ALL:{version_p}{cipher_p}"], None
    raise ValueError(name)


# What each server cannot do, from its documentation or build, each row it
# would have been skipped with the reason.
LIMITS = {
    "mbedtls": {"versions": ["1.2"], "certs": ["p256", "p384", "rsa2048", "rsa4096"],
                "why": "mbedTLS 2.28 has no TLS 1.3 server and no Ed25519"},
    "botan": {"versions": ["1.2"], "certs": ["p256", "p384", "rsa2048", "rsa4096"],
              "why": "Botan 2 has no TLS 1.3, and its TLS 1.2 serves no Ed25519 certificate (`openssl s_client` "
                     "gets handshake_failure from it too)"},
    "wolfssl": {"certs": ["p256", "p384", "rsa2048", "rsa4096"], "why": "this libwolfssl is built without Ed25519"},
}


def rows(name):
    limit = LIMITS.get(name, {})
    versions = limit.get("versions", ["1.3", "1.2"])
    certs = limit.get("certs", CERTS)
    out = []
    for v in versions:
        for cert in certs:
            out.append((cert, v, None))
        if v == "1.3" and name not in ("go", "botan", "boringssl"):
            out += [("p256", v, s) for s in SUITES13]
        if v == "1.3":
            out.append(("p256", v, "resume"))
        if v == "1.2":
            out += [("p256", v, s) for s in ECDSA12] + [("rsa2048", v, s) for s in RSA12]
    return out


# Servers that complete both rounds but do not resume, and why.
NO_RESUME = {}


def resumption(exe, server, roots, name):
    """`tls_many resume` against `server`: "ok", or what failed."""
    out = subprocess.run([exe, "127.0.0.1", str(server.port), tls_live.HOST, "8", "65536", "resume"],
                         input=roots, capture_output=True, timeout=120)
    lines = out.stdout.decode().splitlines()
    if lines.count("done ok=8 failed=0") != 2:
        return f"exit {out.returncode}: {[l for l in lines if l.startswith('done')]}"
    second = lines[lines.index("round 2") + 1:-1]
    resumed = sum(1 for l in second if l.endswith(" resumed"))
    if name in NO_RESUME:
        return "ok" if resumed == 0 else f"resumed {resumed} of 8, where NO_RESUME says it does not"
    return "ok" if resumed == 8 else f"resumed {resumed} of 8"


def main():
    exe = os.path.abspath(sys.argv[1])
    wanted = sys.argv[2:] or ["go", "rustls", "wolfssl", "mbedtls", "botan", "boringssl", "nginx", "gnutls"]
    scratch = tempfile.mkdtemp(prefix="tls_interop_")
    built = build(scratch)
    keys = {kind: tls_live.certificate(kind) for kind in CERTS}
    failed = passed = 0
    for name in wanted:
        binary = built[name]
        if isinstance(binary, tuple):
            print(f"{name:10} skipped: {binary[1]}")
            continue
        if name in LIMITS:
            print(f"{name:10} note: {LIMITS[name]['why']}")
        for cert, version, suite in rows(name):
            cert_pem, key_pem, ca = keys[cert]
            work = tempfile.mkdtemp(dir=scratch)
            open(f"{work}/c.pem", "wb").write(cert_pem)
            open(f"{work}/k.pem", "wb").write(key_pem)
            port = tls_live.free_port()
            here = os.getcwd()
            os.chdir(work)
            try:
                argv, env = argv_for(name, binary, port, version, None if suite == "resume" else suite)
            finally:
                os.chdir(here)
            label = f"{name:10} TLS {version} {cert:8} {suite or '*'}"
            try:
                server = Server(argv, work, port, env)
            except RuntimeError as e:
                print(f"{label}: the server would not start ({e})")
                failed += 1
                continue
            if suite == "resume":
                try:
                    verdict = resumption(exe, server, ca, name)
                finally:
                    server.stop()
                if verdict == "ok":
                    passed += 1
                else:
                    failed += 1
                print(f"{label}: {verdict}")
                continue
            results = []
            try:
                for chunk in (1, 65536):
                    code, lines = tls_live.run(exe, server, ca, 8, chunk)
                    results.append(lines[-1] if lines else f"exit {code}, no output")
            finally:
                server.stop()
            if all(r == "done ok=8 failed=0" for r in results):
                passed += 1
                print(f"{label}: ok")
            else:
                failed += 1
                print(f"{label}: {results}")
    print(f"interop: {passed} rows ok, {failed} failed")
    shutil.rmtree(scratch, ignore_errors=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
