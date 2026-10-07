#!/usr/bin/env python3
"""`packages/http-client` against real servers, over plain TCP and TLS 1.3 (docs/http-client.md §9).

    python3 scripts/http_client_test.py <http_fetch_nb> [--https-hello <https_hello>] [<case> ...]
    python3 scripts/http_client_test.py <http_fetch_nb> --cost <seconds> --server <plain upstream> [--https-hello <https_hello>]

`http_fetch_nb` is `examples/http_fetch_nb/` built with `packages/http-client`, `packages/tls` and `packages/x509`. Its upstreams are
`scripts/http_client_upstream.py` (raw sockets, so every byte it sends is decided there; it counts connections and requests), the
repository's own `examples/https_hello` (a TLS server on `packages/http-server`), and nginx when it is installed. Each case says `ok`
or what failed:

    framing     every response framing (a length, chunked with an extension and a trailer, until close, none, HTTP/1.0, 1xx first,
                HEAD) at sizes from 0 past the client's input buffer: status, byte count and SHA-256 of the body, on one connection
                each and all at once
    keepalive   1,000 requests over four URLs make four connections, counted at the server
    concurrent  40 URLs through 16 slots: all complete, at most 16 connections
    evict       3 upstreams through 2 slots, 60 requests: idle connections are closed for other upstreams, every dial counted at a server
    big         a 50 MB body, byte for byte, in a few MB of memory
    nospin      an upload to a peer that reads nothing, and a request that is never answered: the client waits for the poller,
                it does not spin (under 0.3 s of CPU in 3 s each)
    upload      POST bodies of 0 bytes to 5 MB, with a length and chunked, answered with the server's count and hash of what it got
    expect      `Expect: 100-continue`: answered with a 100 (prompt), not answered (after the client's wait), refused with a 417
    early       a 413 sent while 5 MB are still being uploaded: read, the connection not reused
    retry       a server that ends a connection as the next request arrives: the request is replayed once with --retry and fails
                with `client.closed-early` without it
    refused     ten kinds of response that must be refused, each with its tag
    timeouts    a server that never answers (head), one that stops mid-body (body), one that ends mid-body (truncated), a closed port
    tls         the same server over TLS 1.3: framing, 1,000 requests on 4 connections, 50 MB, an upload; a name the certificate does
                not carry, and roots that are not the server's CA, are refused (`client.tls`)
    hello       `examples/https_hello` (needs --https-hello): requests on one connection, counted by its log
    nginx       nginx, if installed: static files, `keepalive_requests 5` ending connections with `Connection: close`
    hostile     a server that answers with damaged responses (flipped bytes, cuts, junk) to 640 connections: the client never
                crashes and every lane ends

With `--cost <seconds>`: requests a second and the client's CPU a request against a plain upstream (`--server`, a fast one such as
`examples/api`) and, with --https-hello, over TLS 1.3 with keep-alive; `--pin` puts the server on core 0 and the client on core 1 (`taskset`). One line a case, then a count; exit status 1 if any failed.
"""
import hashlib
import os
import random
import re
import resource
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import http_client_upstream as up  # noqa: E402

ROOT = os.path.dirname(HERE)
CA = os.path.join(ROOT, "tests", "vectors", "tls", "echo", "ca.pem")
NAME = "echo.lex-sys.test"


# For the cost rows on Linux: the server on core `SERVER_CORE`, the client on `CLIENT_CORE` (`taskset`), as `https_hello_test.py` pins
# its server and its load. Elsewhere there is no pinning and the figures are the machine's, not the code's.
SERVER_CORE, CLIENT_CORE = 0, 1
PIN = False


def pinned(core, argv):
    if PIN and shutil.which("taskset"):
        return ["taskset", "-c", str(core)] + argv
    return argv


class Fetch:
    """One run of `http_fetch_nb`: its exit status, response lines, summary, wall time and peak memory."""

    def __init__(self, exe, args, stdin=None, timeout=120):
        self.args = [str(a) for a in args]
        t0 = time.monotonic()
        before = resource.getrusage(resource.RUSAGE_CHILDREN)
        p = subprocess.run(pinned(CLIENT_CORE, [exe] + self.args), input=stdin, capture_output=True, timeout=timeout)
        self.elapsed = time.monotonic() - t0
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
        self.cpu = after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime
        self.status = p.returncode
        self.stderr = p.stderr.decode(errors="replace")
        self.lines = p.stdout.decode(errors="replace").splitlines()
        self.maxrss = after.ru_maxrss
        self.responses = []
        self.failures = []
        self.summary = {}
        for line in self.lines:
            m = re.fullmatch(r"(\d+) (\d+) (\d+) ([0-9a-f]{64}) (new|reused)", line)
            if m:
                self.responses.append((int(m[1]), int(m[2]), int(m[3]), m[4], m[5]))
                continue
            m = re.fullmatch(r"(\d+) failed (\S+)", line)
            if m:
                self.failures.append((int(m[1]), m[2]))
                continue
            if line.startswith("done "):
                self.summary = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", line)}

    def __repr__(self):
        return f"status={self.status} {self.summary} failures={self.failures[:4]} stderr={self.stderr[:200]!r} lines={self.lines[:3]}"


def sha(b):
    return hashlib.sha256(b).hexdigest()


def plain(exe, up_, paths, *extra, stdin=None, timeout=120):
    return Fetch(exe, list(extra) + [f"http://127.0.0.1:{up_.port}{p}" for p in paths], stdin=stdin, timeout=timeout)


def secure(exe, up_, paths, *extra, host=NAME, ca=None, timeout=120):
    data = open(CA, "rb").read() if ca is None else ca
    return Fetch(exe, ["--resolve", f"{host}=127.0.0.1"] + list(extra) + [f"https://{host}:{up_.port}{p}" for p in paths],
                 stdin=data, timeout=timeout)


def expect_bodies(run, specs):
    """Each of `specs` (status, body) is the answer of the lane with its number, byte for byte."""
    if run.status != 0 or run.failures:
        return f"{run!r}"
    got = {r[0]: r for r in run.responses}
    for lane, (status, body) in enumerate(specs):
        r = got.get(lane)
        if r is None:
            return f"lane {lane} has no response: {run!r}"
        if r[1] != status or r[2] != len(body) or r[3] != sha(body):
            return f"lane {lane}: {r[1]} {r[2]} bytes {r[3][:12]}, wanted {status} {len(body)} bytes {sha(body)[:12]}"
    return None


# ---- the cases ----

FRAMINGS = [
    ("/len/0", 200, b""), ("/len/1", 200, up.pattern(1)), ("/len/1000", 200, up.pattern(1000)), ("/len/70000", 200, up.pattern(70000)),
    ("/chunked/0", 200, b""), ("/chunked/1?p=1", 200, up.pattern(1)), ("/chunked/5000?p=7", 200, up.pattern(5000)),
    ("/chunked/200000?p=1777", 200, up.pattern(200000)), ("/close/5000", 200, up.pattern(5000)), ("/close/0", 200, b""),
    ("/http10/100", 200, up.pattern(100)), ("/connclose/100", 200, up.pattern(100)), ("/nobody", 204, b""),
    ("/interim/50", 200, up.pattern(50)), ("/nothing-here", 404, b""),
]


def framing_checks(exe, srv, runner):
    for path, status, body in FRAMINGS:
        run = runner(exe, srv, [path])
        bad = expect_bodies(run, [(status, body)])
        if bad:
            return f"{path} alone: {bad}"
    run = runner(exe, srv, [p for p, _, _ in FRAMINGS])
    bad = expect_bodies(run, [(s, b) for _, s, b in FRAMINGS])
    if bad:
        return f"all at once: {bad}"
    return None


def case_framing(exe, _hello):
    srv = up.Upstream()
    bad = framing_checks(exe, srv, plain)
    if bad:
        return bad
    run = plain(exe, srv, ["/len/5000"], "--head")
    if run.status != 0 or [(r[1], r[2]) for r in run.responses] != [(200, 0)]:
        return f"HEAD: {run!r}"
    return "ok (15 framings alone and together, HEAD)"


def case_keepalive(exe, _hello):
    srv = up.Upstream()
    run = plain(exe, srv, ["/len/100", "/chunked/300?p=50", "/nobody", "/interim/10"], "--repeat", 250, "--quiet", "--slots", 8)
    conns, reqs = srv.counts()
    if run.status != 0 or run.summary.get("ok") != 1000 or run.summary.get("failed") != 0:
        return f"{run!r}"
    if run.summary["connects"] != 4 or run.summary["reuses"] != 996 or conns != 4 or reqs != 1000:
        return f"1000 requests: {run.summary}, the server saw {conns} connections and {reqs} requests"
    return "ok (1,000 requests on 4 connections, counted at the server)"


def case_concurrent(exe, _hello):
    srv = up.Upstream()
    run = plain(exe, srv, ["/slow/200?ms=2&s=20"] * 40, "--slots", 16)
    conns, _ = srv.counts()
    bad = expect_bodies(run, [(200, up.pattern(200))] * 40)
    if bad:
        return bad
    if conns > 16 or run.summary["connects"] > 16:
        return f"{conns} connections for 40 URLs through 16 slots: {run.summary}"
    return f"ok (40 URLs, 16 slots, {conns} connections, {run.elapsed:.1f} s)"


def case_evict(exe, _hello):
    # Three upstreams through two slots: each time a lane finds both busy it waits, and when one is idle for another upstream, that
    # connection is closed for it and the same slot dials again.
    servers = [up.Upstream() for _ in range(3)]
    urls = [f"http://127.0.0.1:{srv.port}/len/300" for srv in servers]
    run = Fetch(exe, ["--slots", 2, "--repeat", 20, "--quiet"] + urls)
    if run.status != 0 or run.summary.get("ok") != 60 or run.summary.get("failed") != 0:
        return f"{run!r}"
    seen = sum(srv.counts()[0] for srv in servers)
    requests = sum(srv.counts()[1] for srv in servers)
    if seen != run.summary["connects"] or requests != 60:
        return f"the client dialled {run.summary['connects']} times and the servers saw {seen} connections and {requests} requests"
    if run.summary["connects"] < 3:
        return f"three upstreams through two slots dialled only {run.summary['connects']} times"
    return f"ok (3 upstreams through 2 slots, 60 requests: {run.summary['connects']} connections, each counted at a server)"


def case_big(exe, _hello):
    srv = up.Upstream()
    n = 50_000_000
    run = plain(exe, srv, [f"/big/{n}"], "--timeout", 100)
    h = hashlib.sha256()
    for off in range(0, n, 1 << 20):
        h.update(up.pattern(min(1 << 20, n - off), off))
    if run.status != 0 or not run.responses or run.responses[0][2] != n or run.responses[0][3] != h.hexdigest():
        return f"{run!r}"
    mb = run.maxrss / (1024 * 1024) if sys.platform == "darwin" else run.maxrss / 1024
    if mb > 30:
        return f"50 MB passed through in {mb:.0f} MB of memory"
    return f"ok (50 MB byte for byte in {run.elapsed:.1f} s, {run.summary['connects']} connection, {mb:.0f} MB resident)"


def uploads(exe, srv, runner):
    for size in (0, 1, 1000, 70000, 1_000_000, 5_000_000):
        body = bytes(i % 251 for i in range(size))
        answer = b"bytes=%d sha256=%s" % (size, sha(body).encode())
        for flag in ("--post", "--chunked-post"):
            run = runner(exe, srv, ["/sink"], flag, size, "--timeout", 60)
            bad = expect_bodies(run, [(200, answer)])
            if bad:
                return f"{flag} {size}: {bad}"
    return None


def case_upload(exe, _hello):
    srv = up.Upstream()
    bad = uploads(exe, srv, plain)
    if bad:
        return bad
    return "ok (6 sizes from 0 to 5 MB, with a length and chunked, hash checked at the server)"


def case_expect(exe, _hello):
    srv = up.Upstream()
    body = bytes(i % 251 for i in range(100000))
    answer = b"bytes=100000 sha256=" + sha(body).encode()
    run = plain(exe, srv, ["/continue"], "--post", 100000, "--expect")
    bad = expect_bodies(run, [(200, answer)])
    if bad:
        return f"/continue: {bad}"
    if run.elapsed > 0.8:
        return f"a 100 Continue was sent and the client still took {run.elapsed:.2f} s"
    run = plain(exe, srv, ["/nocontinue"], "--post", 100000, "--expect")
    bad = expect_bodies(run, [(200, answer)])
    if bad:
        return f"/nocontinue: {bad}"
    if run.elapsed < 0.9 or run.elapsed > 4:
        return f"no 100 Continue: the client should send after its 1 s wait, took {run.elapsed:.2f} s"
    run = plain(exe, srv, ["/reject-expect"], "--post", 100000, "--expect", "--repeat", 2)
    if run.status != 0 or [(r[1], r[2], r[4]) for r in sorted(run.responses)] != [(417, 0, "new"), (417, 0, "new")]:
        return f"/reject-expect: {run!r}"
    return "ok (100 Continue answered promptly, waited out after 1 s, 417 refused and its connection not reused)"


def case_early(exe, _hello):
    srv = up.Upstream()
    run = plain(exe, srv, ["/early413"], "--post", 5_000_000, "--repeat", 2)
    if run.status != 0 or [(r[1], r[2], r[3], r[4]) for r in sorted(run.responses)] != [(413, 4, sha(b"big!"), "new")] * 2:
        return f"{run!r}"
    conns, _ = srv.counts()
    if conns != 2 or run.summary["reuses"] != 0:
        return f"the connection of an early response was reused: {conns} connections, {run.summary}"
    return f"ok (413 read while 5 MB were being sent, twice, 2 connections, {run.elapsed:.2f} s)"


def case_retry(exe, _hello):
    srv = up.Upstream()
    run = plain(exe, srv, ["/dropsecond"], "--repeat", 2, "--retry")
    conns, reqs = srv.counts()
    if run.status != 0 or run.summary.get("ok") != 2 or run.summary.get("retries") != 1 or run.summary.get("connects") != 2:
        return f"with --retry: {run!r}"
    if conns != 2 or reqs != 3:
        return f"with --retry the server saw {conns} connections and {reqs} requests (wanted 2 and 3)"
    srv2 = up.Upstream()
    run = plain(exe, srv2, ["/dropsecond"], "--repeat", 2)
    if run.status != 1 or run.failures != [(0, "client.closed-early")] or run.summary.get("retries") != 0:
        return f"without --retry: {run!r}"
    return "ok (replayed once on a new connection with --retry: 2 connections, 3 requests; client.closed-early without)"


def case_nospin(exe, _hello):
    srv = up.Upstream()
    # 200 MB offered to a peer that reads none of it: the socket fills and the client waits to be told it may write.
    run = plain(exe, srv, ["/stalled-reader"], "--post", 200_000_000, "--timeout", 3, timeout=60)
    if run.failures != [(0, "timeout")] or run.status != 1:
        return f"{run!r}"
    if run.cpu > 0.3:
        return f"a blocked upload used {run.cpu:.2f} s of CPU in {run.elapsed:.1f} s"
    # And a lane waiting for a response that does not come.
    run = plain(exe, srv, ["/hang"], "--timeout", 3, "--head-ms", 20000)
    if run.cpu > 0.3:
        return f"a request waiting for its answer used {run.cpu:.2f} s of CPU in {run.elapsed:.1f} s"
    return f"ok (a blocked upload and a waiting request: under 0.3 s of CPU in 3 s each; a loop that retried the blocked write took 0.55)"


REFUSED = [("two-lengths", "response.two-lengths"), ("dup-length", "response.length"), ("chunk", "response.chunk"),
           ("fold", "response.fold"), ("status", "response.status-line"), ("version", "response.version"),
           ("upgrade", "response.upgrade"), ("te", "response.transfer-encoding"), ("longhead", "response.head-too-large"),
           ("garbage", "response.status-line")]


def case_refused(exe, _hello):
    srv = up.Upstream()
    for kind, tag in REFUSED:
        run = plain(exe, srv, [f"/bad/{kind}"])
        if run.status != 1 or run.failures != [(0, tag)]:
            return f"/bad/{kind}: wanted {tag}: {run!r}"
    return f"ok ({len(REFUSED)} kinds, each with its tag)"


def case_timeouts(exe, _hello):
    srv = up.Upstream()
    t0 = time.monotonic()
    run = plain(exe, srv, ["/hang"], "--head-ms", 500)
    if run.failures != [(0, "client.head-timeout")] or not 0.4 < run.elapsed < 3:
        return f"/hang: {run!r} in {run.elapsed:.2f} s"
    run = plain(exe, srv, ["/stall/1000"], "--body-ms", 500)
    if run.failures != [(0, "client.body-timeout")] or not 0.4 < run.elapsed < 3:
        return f"/stall: {run!r} in {run.elapsed:.2f} s"
    run = plain(exe, srv, ["/cut/1000"])
    if run.failures != [(0, "client.truncated")]:
        return f"/cut: {run!r}"
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    closed = s.getsockname()[1]
    s.close()
    run = Fetch(exe, [f"http://127.0.0.1:{closed}/"])
    if run.failures != [(0, "client.connect")]:
        return f"a closed port: {run!r}"
    return f"ok (head 0.5 s, body 0.5 s, truncated, refused: {time.monotonic() - t0:.1f} s)"


def case_tls(exe, _hello):
    srv = up.Upstream(tls=True)
    bad = framing_checks(exe, srv, secure)
    if bad:
        return f"framing: {bad}"
    srv5 = up.Upstream(tls=True)
    run = secure(exe, srv5, ["/closeabrupt/1000"])
    if run.failures != [(0, "client.reset")]:
        return f"a body ended by the socket with no close_notify: {run!r}"
    srv2 = up.Upstream(tls=True)
    run = secure(exe, srv2, ["/len/100", "/chunked/300?p=50", "/nobody", "/interim/10"], "--repeat", 250, "--quiet", "--timeout", 100)
    conns, reqs = srv2.counts()
    if run.status != 0 or run.summary.get("ok") != 1000 or run.summary["connects"] != 4 or conns != 4 or reqs != 1000:
        return f"1000 requests over TLS: {run!r}, the server saw {conns} connections"
    srv3 = up.Upstream(tls=True)
    n = 50_000_000
    run = secure(exe, srv3, [f"/big/{n}"], "--timeout", 100)
    h = hashlib.sha256()
    for off in range(0, n, 1 << 20):
        h.update(up.pattern(min(1 << 20, n - off), off))
    if run.status != 0 or not run.responses or run.responses[0][2] != n or run.responses[0][3] != h.hexdigest():
        return f"50 MB over TLS: {run!r}"
    big_s = run.elapsed
    srv4 = up.Upstream(tls=True)
    bad = uploads(exe, srv4, secure)
    if bad:
        return f"upload over TLS: {bad}"
    # A name the certificate does not carry, and roots that are not its CA, are refused.
    run = secure(exe, srv4, ["/len/10"], host="other.example")
    if run.failures != [(0, "client.tls")]:
        return f"a certificate for another name: {run!r}"
    other = subprocess.run(["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes",
                            "-subj", "/CN=not-the-ca", "-days", "2", "-keyout", os.devnull, "-out", "-"],
                           capture_output=True).stdout
    run = secure(exe, srv4, ["/len/10"], ca=other)
    if run.failures != [(0, "client.tls")]:
        return f"another CA's roots: {run!r}"
    return f"ok (framing, 1,000 requests on 4 connections, 50 MB in {big_s:.1f} s, uploads, a wrong name and wrong roots refused)"


class Hello:
    """`examples/https_hello` on a free port with the test identity, its lines collected."""

    def __init__(self, exe):
        self.work = tempfile.mkdtemp(prefix="hello_")
        for f in ("chain.pem", "key.pem", "names"):
            shutil.copyfile(os.path.join(ROOT, "tests", "vectors", "tls", "echo", "first", f), os.path.join(self.work, f))
        s = socket.socket()
        s.bind(("127.0.0.1", 0))
        self.port = s.getsockname()[1]
        s.close()
        self.lines = []
        self.proc = subprocess.Popen(pinned(SERVER_CORE, [exe, "--port", str(self.port), "--dir", self.work, "--idle", "60000", "--connections", "64"]),
                                     stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        threading.Thread(target=self.read, daemon=True).start()
        for _ in range(100):
            if any(l.startswith("listening") for l in self.lines):
                break
            time.sleep(0.05)

    def read(self):
        for raw in self.proc.stdout:
            self.lines.append(raw.decode(errors="replace").rstrip("\n"))

    def stop(self):
        self.proc.send_signal(signal.SIGTERM)
        try:
            self.proc.wait(5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
        shutil.rmtree(self.work, ignore_errors=True)


def case_hello(exe, hello):
    if not hello:
        return "ok (skipped: no --https-hello)"
    srv = Hello(hello)
    try:
        run = secure(exe, srv, ["/hello/n1", "/big/200000", "/hello/n2"], "--repeat", 100, "--quiet")
        time.sleep(0.3)
        est = [l for l in srv.lines if " established " in l]
        if run.status != 0 or run.summary.get("ok") != 300 or run.summary["connects"] != 3 or len(est) != 3:
            return f"{run!r}, the server logged {len(est)} handshakes"
        closed = [l for l in srv.lines if " closed ok " in l]
        if len(closed) != 3:
            return f"the connections were not all closed with close_notify: {srv.lines[-6:]}"
        # A body longer than its 16 KiB request buffer is refused by it with a 413 before the body is sent: an early response, read,
        # and its connection not reused.
        run = secure(exe, srv, ["/echo"], "--post", 20000, "--repeat", 2)
        if run.status != 0 or sorted((r[1], r[4]) for r in run.responses) != [(413, "new"), (413, "new")]:
            return f"POST /echo: {run!r}"
        return "ok (300 requests on 3 TLS connections to examples/https_hello, 3 handshakes, closed with close_notify)"
    finally:
        srv.stop()


NGINX_CONF = """worker_processes 1; daemon off; pid {work}/nginx.pid; error_log {work}/error.log;
events {{ worker_connections 64; }}
http {{ access_log off; client_body_temp_path {work}/b; proxy_temp_path {work}/p; fastcgi_temp_path {work}/f;
  uwsgi_temp_path {work}/u; scgi_temp_path {work}/s; keepalive_requests 5; keepalive_timeout 30;
  server {{ listen 127.0.0.1:{port}; root {work}/www; }} }}
"""


def case_nginx(exe, _hello):
    nginx = shutil.which("nginx")
    if not nginx:
        return "ok (skipped: nginx is not installed)"
    work = tempfile.mkdtemp(prefix="nginx_")
    os.makedirs(os.path.join(work, "www"))
    data = up.pattern(300000)
    open(os.path.join(work, "www", "file"), "wb").write(data)
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    conf = os.path.join(work, "nginx.conf")
    open(conf, "w").write(NGINX_CONF.format(work=work, port=port))
    proc = subprocess.Popen([nginx, "-c", conf, "-p", work], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            try:
                socket.create_connection(("127.0.0.1", port), 0.2).close()
                break
            except OSError:
                time.sleep(0.05)
        run = Fetch(exe, ["--repeat", 23, f"http://127.0.0.1:{port}/file"])
        want = (200, len(data), sha(data))
        if run.status != 0 or len(run.responses) != 23 or any((r[1], r[2], r[3]) != want for r in run.responses):
            return f"{run!r}"
        # nginx ends a connection with `Connection: close` on its 5th request: 23 requests, 5 connections.
        if run.summary["connects"] != 5:
            return f"23 requests with keepalive_requests 5 made {run.summary['connects']} connections, not 5"
        run = Fetch(exe, ["--head", "--repeat", 3, f"http://127.0.0.1:{port}/file", f"http://127.0.0.1:{port}/missing"])
        if run.status != 0 or sorted((r[1], r[2]) for r in run.responses) != [(200, 0)] * 3 + [(404, 0)] * 3:
            return f"HEAD: {run!r}"
        return "ok (300 KB x 23 requests, 5 connections as nginx's keepalive_requests 5 ends them; HEAD; 404)"
    finally:
        proc.terminate()
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            proc.kill()
        shutil.rmtree(work, ignore_errors=True)


# ---- a hostile upstream ----

VALID = [b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello",
         b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
         b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 204 No Content\r\n\r\n",
         b"HTTP/1.0 200 OK\r\nConnection: close\r\n\r\nuntil the end",
         b"HTTP/1.1 301 Moved\r\nLocation: /x\r\nContent-Length: 0\r\nSet-Cookie: a=b\r\n\r\n"]


def mutate(rng):
    b = bytearray(rng.choice(VALID))
    for _ in range(rng.randint(0, 4)):
        kind = rng.randint(0, 5)
        if kind == 0 and b:
            b[rng.randrange(len(b))] = rng.randrange(256)
        elif kind == 1 and b:
            del b[rng.randrange(len(b)):]
        elif kind == 2:
            b[rng.randrange(len(b) + 1):rng.randrange(len(b) + 1)] = bytes(rng.randrange(256) for _ in range(rng.randint(1, 9)))
        elif kind == 3 and b:
            i = rng.randrange(len(b))
            b[i:i] = b[i:i + rng.randint(1, 20)] * rng.randint(2, 30)
        elif kind == 4:
            b = bytearray(b"\r\n" * rng.randint(1, 5)) + b
        else:
            b += bytes(rng.randrange(256) for _ in range(rng.randint(1, 3000)))
    return bytes(b)


def chaos_server(seed):
    rng = random.Random(seed)
    sock = socket.socket()
    sock.bind(("127.0.0.1", 0))
    sock.listen(256)

    def serve(conn, data, gap):
        try:
            conn.settimeout(1)
            try:
                conn.recv(65536)
            except OSError:
                pass
            for i in range(0, len(data), gap):
                conn.sendall(data[i:i + gap])
                time.sleep(0.001)
            time.sleep(rng.random() * 0.05)
        except OSError:
            pass
        finally:
            conn.close()

    def accept():
        while True:
            try:
                conn, _ = sock.accept()
            except OSError:
                return
            threading.Thread(target=serve, args=(conn, mutate(rng), rng.choice([1, 3, 64, 100000])), daemon=True).start()

    threading.Thread(target=accept, daemon=True).start()
    return sock


def case_hostile(exe, _hello):
    sock = chaos_server(7)
    port = sock.getsockname()[1]
    lanes = 0
    ended = 0
    try:
        for run_no in range(10):
            run = Fetch(exe, ["--head-ms", 300, "--body-ms", 300, "--repeat", 1, "--slots", 32, "--timeout", 20] +
                        [f"http://127.0.0.1:{port}/{i}" for i in range(64)])
            if run.status not in (0, 1):
                return f"run {run_no}: the client ended with status {run.status}: {run!r}"
            if "done " not in " ".join(run.lines):
                return f"run {run_no}: no summary: {run!r}"
            lanes += 64
            ended += run.summary["ok"] + run.summary["failed"]
        return f"ok ({lanes} damaged responses, split in pieces of 1 to 100,000 bytes: the client lived, every lane ended: {ended})"
    finally:
        sock.close()


CASES = {"framing": case_framing, "keepalive": case_keepalive, "concurrent": case_concurrent, "evict": case_evict, "big": case_big,
         "upload": case_upload, "nospin": case_nospin, "expect": case_expect, "early": case_early, "retry": case_retry, "refused": case_refused,
         "timeouts": case_timeouts, "tls": case_tls, "hello": case_hello, "nginx": case_nginx, "hostile": case_hostile}


# ---- cost ----

def cost(exe, label, urls, seconds, stdin=None, resolve=(), lanes=32):
    """Requests a second and the client's CPU a request, closed loop: `lanes` lanes on as many connections, one request out each.
    A short run finds the rate; three runs of about `seconds` each are reported, and the middle CPU figure."""
    def args(repeat):
        return list(resolve) + ["--repeat", repeat, "--slots", lanes, "--pool", lanes, "--quiet", "--timeout", 600] + urls

    calibrate = Fetch(exe, args(500), stdin=stdin)
    if calibrate.status != 0:
        return f"{label}: {calibrate!r}"
    rate = calibrate.summary["ok"] / max(calibrate.elapsed, 0.01)
    repeat = max(500, int(rate * seconds) // lanes)
    out = []
    for _ in range(3):
        run = Fetch(exe, args(repeat), stdin=stdin, timeout=900)
        if run.status != 0:
            return f"{label}: {run!r}"
        n = run.summary["ok"]
        out.append((n / run.elapsed, run.cpu / n * 1e6))
    rates = " ".join(f"{r:,.0f}" for r, _ in out)
    cpu = sorted(c for _, c in out)[1]
    return f"{label}: {rates} requests a second over three runs of {repeat * lanes:,}, the client's CPU {cpu:.2f} microseconds a request"


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    exe = os.path.abspath(args.pop(0))
    hello = None
    seconds = 0
    server = None
    if "--https-hello" in args:
        k = args.index("--https-hello")
        hello = os.path.abspath(args[k + 1])
        del args[k:k + 2]
    global PIN
    if "--pin" in args:
        PIN = True
        args.remove("--pin")
    if "--cost" in args:
        k = args.index("--cost")
        seconds = int(args[k + 1])
        del args[k:k + 2]
    if "--server" in args:
        k = args.index("--server")
        server = os.path.abspath(args[k + 1])
        del args[k:k + 2]
    if seconds:
        return cost_main(exe, hello, server, seconds)
    names = args or list(CASES)
    failed = 0
    for name in names:
        try:
            result = CASES[name](exe, hello)
        except Exception as e:  # noqa: BLE001 -- a case that raises has failed
            result = f"failed: {e!r}"
        ok = result.startswith("ok")
        failed += 0 if ok else 1
        print(f"{name:10} {result}", flush=True)
    print(f"{len(names) - failed} of {len(names)} cases ok")
    return 1 if failed else 0


def cost_main(exe, hello, server, seconds):
    """The cost rows. The plain upstream is a fast server (`--server`, `examples/api`: `GET /users/42`); over TLS it is `https_hello`."""
    rows = []
    if server:
        s = socket.socket()
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
        s.close()
        proc = subprocess.Popen(pinned(SERVER_CORE, [server, str(port)]), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(0.5)
        try:
            rows.append(cost(exe, "plain, keep-alive", [f"http://127.0.0.1:{port}/users/42"] * 32, seconds))
        finally:
            proc.terminate()
    if hello:
        srv = Hello(hello)
        try:
            rows.append(cost(exe, "TLS 1.3, keep-alive", [f"https://{NAME}:{srv.port}/hello/42"] * 32, seconds,
                             stdin=open(CA, "rb").read(), resolve=["--resolve", f"{NAME}=127.0.0.1"]))
        finally:
            srv.stop()
    for r in rows:
        print(r, flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
