#!/usr/bin/env python3
"""`packages/tls`'s server resuming the clients that resume (docs/tls-server.md §12.8).

    python3 scripts/tls_server_tickets_interop.py <tls_serve> [--many <tls_many>] [<client> ...]

`tls_serve` is `tests/programs/tls_serve.cho` built with `packages/tls` and `packages/x509`; it runs with session
tickets on (`2:3600`). The clients, each skipped with the reason when it cannot run, are those of
`scripts/tls_server_interop.py` that keep a session (and its image, `scripts/interop/server.Dockerfile`):

    openssl    `openssl s_client -sess_out` then `-sess_in`, with each suite, group, a HelloRetryRequest
    curl       curl, two URLs in one run (its session cache)
    go         Go's crypto/tls with a ClientSessionCache (scripts/interop/go_resume_client.go)
    wolfssl    wolfSSL, a session kept between connections (scripts/interop/wolfssl_resume_client.c)
    gnutls     `gnutls-cli --resume`
    rustls     rustls 0.23 (scripts/interop/rustls_client; `cargo`)
    mosquitto  `mosquitto_sub` twice: libmosquitto never sets a session, so a row is expected NOT to resume, and
               the row says so rather than hide it
    many       `packages/tls`'s own client (`tls_many resume`, needs --many), 8 connections, then 8 resumed

A row passes when the client completes and its data comes back, and the server's `conn` lines say the first
connection was `full` and every later one `resumed` with the verdict `tls-ticket-resumed` (the engine's own
account, `tls.ticket_verdict`). Then rows for what only the server can show: a ticket key shared by two
processes (a ticket made by one resumed by the other, and not by a third with another key), and a ticket
that has expired. Exit status 1 if any row failed.
"""
import os
import shutil
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_server_interop as interop  # noqa: E402

INTEROP = interop.INTEROP
GROUPS, SUITES = interop.GROUPS, interop.SUITES
MESSAGE = "hello over TLS 1.3"
HTTP = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n"


def conn_lines(server, since, n, wait=8.0):
    """The next `n` `conn` lines after line `since`, as field lists."""
    out, at = [], since
    while len(out) < n:
        line = server.next_conn(at, wait)
        if line is None:
            break
        out.append(line.split())
        with server.cond:
            at = server.lines.index(line, at) + 1
    return out


def expect(server, since, pattern):
    """None when the server's lines for the next connections are `pattern` (`full` or `resumed` each)."""
    lines = conn_lines(server, since, len(pattern))
    if len(lines) != len(pattern):
        return f"{len(lines)} connections, wanted {len(pattern)}"
    for f, want in zip(lines, pattern):
        # conn <code> <tag> <name> <alpn> <bytes> <suite> <group> <retried|direct> <resumed|full> <verdict>
        if f[2] != "ok" and not (f[2] in ("socket", "tls-peer-closed") and want):
            return " ".join(f)
        if f[9] != want:
            return f"{' '.join(f)}: wanted {want}"
        if want == "resumed" and f[10] != "tls-ticket-resumed":
            return " ".join(f)
    return None


def openssl_row(srv, work, suite, group, retry):
    base = ["openssl", "s_client", "-connect", f"127.0.0.1:{srv.port}", "-CAfile", "ca.pem", "-verify_return_error",
            "-ign_eof", "-tls1_3", "-ciphersuites", suite, "-groups", interop.groups_for(group, retry),
            "-servername", "srv.example", "-verify_hostname", "srv.example"]
    since = srv.mark()
    sess = os.path.join(work, "sess.pem")
    if os.path.exists(sess):
        os.unlink(sess)
    code, out = interop.run(base + ["-sess_out", sess], input=HTTP)
    if code != 0 or "hello from cancho" not in out or "New, TLSv1.3" not in out:
        return f"first: s_client {code}: {out.strip()[-200:]}"
    code, out = interop.run(base + ["-sess_in", sess], input=HTTP)
    if code != 0 or "hello from cancho" not in out or "Reused, TLSv1.3" not in out:
        return f"second: s_client {code}, not reused: {out.strip()[-200:]}"
    return expect(srv, since, ["full", "resumed"])


def curl_row(srv, work):
    env = dict(os.environ)
    since = srv.mark()
    url = f"https://srv.example:{srv.port}/"
    code, out = interop.run(["curl", "-sS", "--http1.1", "--cacert", "ca.pem", "-H", "Connection: close", url, url],
                            env=env)
    if code != 0 or out.count("hello from cancho") != 2:
        return f"curl {code}: {out.strip()[-200:]}"
    return expect(srv, since, ["full", "resumed"])


def resume_client_row(srv, argv, rounds):
    since = srv.mark()
    code, out = interop.run(argv)
    words = out.split()
    if code != 0 or not words or words[0] != "ok" or len(words) != rounds + 1:
        return f"{argv[0]} {code}: {out.strip()[-200:]}"
    want = ["full"] + ["resumed"] * (rounds - 1)
    if words[1:] != want:
        return f"the client says {words[1:]}"
    return expect(srv, since, want)


def gnutls_row(srv, work):
    since = srv.mark()
    code, out = interop.run(["gnutls-cli", "--resume", "--x509cafile", "ca.pem", "-p", str(srv.port),
                             "--priority", "NORMAL:-VERS-ALL:+VERS-TLS1.3", "--sni-hostname", "srv.example",
                             "srv.example"], input=HTTP, timeout=20)
    if "resum" not in out.lower() or "hello from cancho" not in out:
        return f"gnutls-cli {code}: {out.strip()[-300:]}"
    return expect(srv, since, ["full", "resumed"])


def mosquitto_row(srv, work):
    common = ["-h", "srv.example", "-p", str(srv.port), "--cafile", "ca.pem", "--tls-version", "tlsv1.3",
              "-t", "cancho/interop", "-i", "cancho-interop"]
    since = srv.mark()
    for _ in range(2):
        code, out = interop.run(["mosquitto_sub", *common, "-C", "1", "-W", "10"])
        if code != 0 or out.strip() != "hello from cancho":
            return f"mosquitto_sub {code}: {out.strip()[-200:]}"
    # libmosquitto sets no session on its SSL object, so neither connection resumes. The row passes because
    # the server says `full` twice; if a mosquitto ever resumes, this row fails and says so.
    return expect(srv, since, ["full", "full"])


def many_row(srv, exe, work):
    since = srv.mark()
    r = subprocess.run([exe, "127.0.0.1", str(srv.port), "srv.example", "8", "65536", "resume"],
                       stdin=open("ca.pem", "rb"), capture_output=True, timeout=60)
    lines = r.stdout.decode().splitlines()
    second = [l for l in lines if l.endswith(" resumed")]
    if len(second) != 8 or "failed=0" not in lines[-1]:
        return f"tls_many: {len(second)} of 8 resumed: {lines[-3:]}"
    got = conn_lines(srv, since, 16)
    if [f[9] for f in got].count("resumed") != 8 or any(f[2] != "ok" for f in got):
        return f"server lines: {[' '.join(f) for f in got][-3:]}"
    return None


def shared_key_rows(exe, work):
    """Two processes holding one key file resume each other's tickets; a third with another does not."""
    keys = os.path.join(work, "keys_a")
    open(keys, "wb").write(os.urandom(64))
    other = os.path.join(work, "keys_b")
    open(other, "wb").write(os.urandom(32))
    a = interop.Server(exe, "http", work, tickets="2:3600:keys_a")
    b = interop.Server(exe, "http", work, tickets="2:3600:keys_a")
    c = interop.Server(exe, "http", work, tickets="2:3600:keys_b")
    rows = []
    sess = os.path.join(work, "shared.pem")
    suite, group = "TLS_AES_128_GCM_SHA256", "X25519"

    def client(srv, flag):
        base = ["openssl", "s_client", "-connect", f"127.0.0.1:{srv.port}", "-CAfile", "ca.pem", "-ign_eof",
                "-tls1_3", "-ciphersuites", suite, "-groups", group, "-servername", "srv.example", flag, sess]
        return interop.run(base, input=HTTP)

    try:
        since_a = a.mark()
        code, out = client(a, "-sess_out")
        bad = None if code == 0 and "New, TLSv1.3" in out else f"a: {out[-200:]}"
        rows.append(("a ticket from process A, resumed by B (the same key file)", bad or None))
        since_b = b.mark()
        code, out = client(b, "-sess_in")
        bad = None
        if "Reused, TLSv1.3" not in out:
            bad = f"B did not resume: {out[-200:]}"
        else:
            bad = expect(b, since_b, ["resumed"])
        rows[-1] = (rows[-1][0], rows[-1][1] or bad)
        since_c = c.mark()
        code, out = client(c, "-sess_in")
        bad = None
        if "Reused, TLSv1.3" in out:
            bad = "C resumed with another key"
        else:
            line = conn_lines(c, since_c, 1)
            bad = None if line and line[0][9] == "full" and line[0][10] == "tls-ticket-unknown" else f"{line}"
        rows.append(("the same ticket at process C (another key): a full handshake, `unknown`", bad))
    finally:
        for s in (a, b, c):
            s.stop()
    return rows


def expiry_row(exe, work):
    srv = interop.Server(exe, "http", work, tickets="1:2")
    try:
        sess = os.path.join(work, "expiry.pem")
        base = ["openssl", "s_client", "-connect", f"127.0.0.1:{srv.port}", "-CAfile", "ca.pem", "-ign_eof",
                "-tls1_3", "-servername", "srv.example"]
        since = srv.mark()
        interop.run(base + ["-sess_out", sess], input=HTTP)
        time.sleep(3.5)
        code, out = interop.run(base + ["-sess_in", sess], input=HTTP)
        lines = conn_lines(srv, since, 2)
        # OpenSSL's client keeps the lifetime the NewSessionTicket named and does not offer an expired session
        # (`tls-ticket-none`); a client that did would be refused here, `expired` (the lying client's case).
        if len(lines) != 2 or lines[1][9] != "full" or lines[1][10] not in (
                "tls-ticket-none", "tls-ticket-expired", "tls-ticket-age"):
            return f"{[' '.join(f) for f in lines]}"
        return None
    finally:
        srv.stop()


def main():
    args = sys.argv[1:]
    exe = os.path.abspath(args[0])
    many = None
    if "--many" in args:
        i = args.index("--many")
        many = os.path.abspath(args[i + 1])
        args = args[:i] + args[i + 2:]
    wanted = args[1:] or ["openssl", "curl", "go", "wolfssl", "gnutls", "rustls", "mosquitto", "many"]
    work = tempfile.mkdtemp(prefix="tls-server-tickets-")
    os.chdir(work)
    interop.authority(work)
    tools = {}
    if shutil.which("go"):
        r = subprocess.run(["go", "build", "-o", f"{work}/go_resume", f"{INTEROP}/go_resume_client.go"], cwd=work,
                           capture_output=True, env=dict(os.environ, GOFLAGS="-mod=mod", HOME=work))
        tools["go"] = f"{work}/go_resume" if r.returncode == 0 else (None, r.stderr.decode()[-200:])
    else:
        tools["go"] = (None, "`go` is not installed")
    r = subprocess.run(["cc", "-O2", f"{INTEROP}/wolfssl_resume_client.c", "-lwolfssl", "-o", f"{work}/wolfssl_resume"],
                       capture_output=True)
    tools["wolfssl"] = f"{work}/wolfssl_resume" if r.returncode == 0 else (None, r.stderr.decode()[-200:])
    if shutil.which("cargo"):
        r = subprocess.run(["cargo", "build", "--release", "--quiet", "--manifest-path",
                            f"{INTEROP}/rustls_client/Cargo.toml", "--target-dir", f"{work}/rustls"],
                           capture_output=True)
        tools["rustls"] = f"{work}/rustls/release/rustls_client" if r.returncode == 0 else (
            None, r.stderr.decode()[-300:])
    else:
        tools["rustls"] = (None, "`cargo` is not installed")
    for name, tool in [("openssl", "openssl"), ("curl", "curl"), ("gnutls", "gnutls-cli"),
                       ("mosquitto", "mosquitto_sub")]:
        tools[name] = shutil.which(tool) or (None, f"`{tool}` is not installed")
    tools["many"] = many or (None, "no --many given")

    servers = {mode: interop.Server(exe, mode, work, tickets="2:3600") for mode in ("http", "echo", "mqtt")}
    rows = []

    def row(name, fn):
        try:
            bad = fn()
        except Exception as e:  # noqa: BLE001 -- a row that throws is a failed row
            bad = f"{type(e).__name__}: {e}"
        rows.append((name, bad))
        print(f"{name}: {'ok' if bad is None else 'FAILED ' + bad}", flush=True)

    port = lambda s: str(s.port)  # noqa: E731
    for client in wanted:
        tool = tools.get(client)
        if not isinstance(tool, str):
            print(f"{client}: not run, {tool[1] if tool else 'unknown client'}")
            continue
        if client == "openssl":
            for suite in SUITES:
                row(f"openssl resumes {suite} X25519", lambda: openssl_row(servers["http"], work, suite, "X25519", False))
            for group in ("P-256", "P-384"):
                row(f"openssl resumes AES-128-GCM {group}",
                    lambda: openssl_row(servers["http"], work, "TLS_AES_128_GCM_SHA256", group, False))
            row("openssl resumes after a HelloRetryRequest (P-521, then P-256)",
                lambda: openssl_row(servers["http"], work, "TLS_AES_128_GCM_SHA256", "P-256", True))
        elif client == "curl":
            row("curl resumes (two URLs, one run)", lambda: curl_row(servers["http"], work))
        elif client == "go":
            row("go resumes twice", lambda: resume_client_row(
                servers["echo"], [tool, f"127.0.0.1:{servers['echo'].port}", "srv.example", "ca.pem", "3", MESSAGE], 3))
        elif client == "wolfssl":
            row("wolfssl resumes twice", lambda: resume_client_row(
                servers["echo"], [tool, port(servers["echo"]), "srv.example", "ca.pem", "3", MESSAGE], 3))
        elif client == "gnutls":
            row("gnutls-cli --resume", lambda: gnutls_row(servers["http"], work))
        elif client == "rustls":
            row("rustls resumes twice", lambda: resume_client_row(
                servers["http"], [tool, port(servers["http"]), "srv.example", "ca.pem", "3"], 3))
        elif client == "mosquitto":
            row("mosquitto_sub twice: does not resume (libmosquitto sets no session)",
                lambda: mosquitto_row(servers["mqtt"], work))
        elif client == "many":
            row("packages/tls's own client: 8 connections, then 8 resumed", lambda: many_row(servers["http"], tool, work))
    for name, bad in shared_key_rows(exe, work):
        rows.append((name, bad))
        print(f"{name}: {'ok' if bad is None else 'FAILED ' + str(bad)}", flush=True)
    row("a ticket of 2 s lifetime, 3.5 s later: a full handshake (the client does not offer it)", lambda: expiry_row(exe, work))
    for s in servers.values():
        s.stop()
    failed = sum(1 for _, bad in rows if bad is not None)
    print(f"{len(rows)} rows, {len(rows) - failed} ok, {failed} failed")
    shutil.rmtree(work, ignore_errors=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
