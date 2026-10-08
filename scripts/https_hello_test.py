#!/usr/bin/env python3
"""`examples/https_hello` against real clients (docs/http-server.md §11.6).

    python3 scripts/https_hello_test.py <https_hello> [<case> ...]

`https_hello` is `examples/https_hello/` built with `examples/tls_echo/front.cho`, `identity.cho`, `packages/http-server`,
`packages/tls` and `packages/x509`. The certificates are `scripts/tls_echo_test.py`'s: the committed test identities in
`tests/vectors/tls/echo/` (a CA, and two leaves for `echo.lex-sys.test`). Needs python3 (`ssl`, `http.client`) and
`openssl`; `curl` is used if it was built with OpenSSL (its `--cacert` is not honoured by a SecureTransport build, and
the case says so rather than pass). Each case starts its own server on a free port and says `ok` or what failed:

    curl       curl, HTTP/1.1 over TLS: one request, then 100 requests on ONE connection (`%{num_connects}` is 1), a JSON
               POST echoed, a 404 and a 405, and a 100,000-byte streamed body
    openssl    `openssl s_client`: a request, a keep-alive one after it, then three pipelined in one write, then
               `Connection: close`, which is answered and then closed with close_notify
    http       Python `http.client`: 2,000 requests on one connection (paths echoed, each checked), the JSON echo with
               every escape, an HTTP/1.0 request closed after its answer, a chunked request body
    pipelined  300 requests in one write, then 300 with bodies (length and chunked) in one write, answers in order
    many       200 connections at once (Python `ssl`, one thread each), 10 requests each
    big        `/big/<n>` for n from 0 to 64 MiB: every byte checked (the body is a to z repeating), keep-alive after
    stalled    a client asks for 1 GiB and stops reading: the server's memory does not grow, the other clients are
               served meanwhile, and the stalled one is closed after `--idle`
    slow       a client that sends half a request and waits is closed after `--idle`, and does not hold up the rest
    ended      a connection ended by `Connection: close` and then written to (from its own thread: OpenSSL's per-thread
               error queue would report its refused writes on the next write of another socket in the same thread) does
               not disturb a keep-alive client beside it: 40 requests answered, no other connection closed
    reload     as `tls_echo`'s: a connection open before SIGHUP keeps its certificate and keeps being served; one after
               gets the renewed one; a refused reload leaves the renewed one serving
    bound      `--handshakes 2`: two peers that connect and send nothing hold both places, an honest client waits
               behind them (not refused) until their `--handshake-timeout` frees one
    full       `--connections 4`: a fifth connection is closed at once
    idle       `--idle 1000`: an established keep-alive connection with no traffic gets close_notify
    shutdown   SIGTERM with 10 keep-alive connections open: each gets close_notify, the server exits 0
    hostile    1,500 connections each sending a mangled request (flipped bytes, cuts, bare LFs, 100 KB headers, a thousand
               headers, nested JSON 5,000 deep, huge or negative lengths, chunk sizes past the end): the server is alive
               after every one, and still serves
    upload     `POST /upload` of 0 bytes to 1 GiB (`UPLOAD_TOP_MIB`), with a length and chunked (pieces of 1 byte to 1 MiB, with
               extensions and trailers): the answer's byte count and SHA-256 are those of what was sent, the server's memory does
               not grow with the size, and the connection serves after; a request pipelined behind an upload is answered in order
    expect     `Expect: 100-continue`: the `100` is sent when the application accepts the body; `/upload/refuse` is answered 413 with no
               `100` (and the connection goes on); another expectation is 417; curl's `-T`, `--expect100-timeout` and a refused
               upload that sends no byte of its body; `openssl s_client` uploading; `--max-body` refuses by the head, and mid-chunked
    halfbody   a client that sends half a head, half a length body, half a chunked body, or a byte at a time, and stalls: answered 408
               (`timeout.head`, `timeout.body`) after `--read-timeout`, and the clients beside it are served throughout
    mangled    1,500 connections each sending a mangled upload (flipped bytes, cuts, bad chunk sizes, trailers and extensions, lengths
               that lie, `Expect` heads): the server is alive after every one, its memory is where it was, and a good upload still hashes

    tickets    `--tickets 2 --ticket-keys keys`: a session kept from one request resumes, twice, and a request on the resumed
               connection is answered; a process with the same key file resumes it too, one with another file does not; a
               rotation by SIGHUP keeps the session and a dropped key refuses it (docs/tls-server.md §12)

With `--cost <seconds>`: requests a second over TLS against `kload` (benches/server/kload.c, plain, `examples/api`) and
`tload` (benches/server/tload.c, the same closed loop over OpenSSL) on one core each side; see docs/http-server.md §11.7.
One line a case, then a count; exit status 1 if any failed.
"""
import hashlib
import http.client
import json
import os
import re
import select
import shutil
import signal
import socket
import ssl
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_echo_test as echo  # noqa: E402 -- the certificates, `Server` and the helpers are the echo's

HOST = echo.HOST
CA = echo.CA


class Server(echo.Server):
    """`https_hello` on a free port: `tls_echo`'s harness, its lines collected as they come."""


def https(port, timeout=30):
    return echo.connect(port, timeout=timeout)


def client(port, timeout=30):
    """An `http.client` connection over a verified TLS 1.3 socket to the server."""
    h = http.client.HTTPSConnection("127.0.0.1", port, timeout=timeout, context=echo.context())
    h.sock = https(port, timeout)
    return h


def get(h, path):
    h.request("GET", path)
    r = h.getresponse()
    return r.status, r.read()


def rss_kb(pid):
    if os.path.exists(f"/proc/{pid}/status"):
        for line in open(f"/proc/{pid}/status"):
            if line.startswith("VmRSS:"):
                return int(line.split()[1])
    out = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True).stdout.decode().strip()
    return int(out)


def pattern(n, start=0):
    unit = bytes(97 + i % 26 for i in range(26))
    off = start % 26
    return (unit[off:] + unit * (n // 26 + 1))[:n]


# ---- the cases ----

def case_curl(exe):
    version = subprocess.run(["curl", "--version"], capture_output=True).stdout.decode()
    if "OpenSSL" not in version.split("\n")[0]:
        return "ok (skipped: this curl is not built with OpenSSL, so it cannot be given the test CA)"
    server = Server(exe)
    try:
        base = f"https://{HOST}:{server.port}"
        common = ["curl", "-sS", "--cacert", CA, "--resolve", f"{HOST}:{server.port}:127.0.0.1", "--tlsv1.3"]
        out = subprocess.run(common + [base + "/"], capture_output=True)
        if out.stdout != b"hello over TLS\n":
            return f"GET /: {out.stdout!r} {out.stderr!r}"
        urls = [f"{base}/hello/n{i}" for i in range(100)]
        out = subprocess.run(common + urls + ["-w", "\n%{num_connects}\n"], capture_output=True)
        # `-w` is written after each transfer: the body, a blank line, and how many connections it had to open.
        found = re.findall(rb"hello, n(\d+)\n\n(\d+)\n", out.stdout)
        if [int(n) for n, _ in found] != list(range(100)):
            return f"100 requests: {out.stdout[:200]!r} {out.stderr!r}"
        opened = sum(int(c) for _, c in found)
        if opened != 1:
            return f"curl opened {opened} connections for 100 requests"
        out = subprocess.run(common + ["-d", '{"k":[1,"two",null]}', base + "/echo"], capture_output=True)
        got = json.loads(out.stdout)
        if got != {"method": "POST", "path": "/echo", "bytes": 20, "json": {"k": [1, "two", None]}}:
            return f"POST /echo: {got}"
        codes = [subprocess.run(common + ["-o", "/dev/null", "-w", "%{http_code}"] + extra + [url], capture_output=True).stdout.decode()
                 for extra, url in (([], base + "/nothing"), (["-X", "DELETE"], base + "/echo"))]
        if codes != ["404", "405"]:
            return f"404 and 405: {codes}"
        out = subprocess.run(common + [base + "/big/100000"], capture_output=True)
        if out.stdout != pattern(100000):
            return f"/big/100000: {len(out.stdout)} bytes, wrong content"
        return "ok (100 requests on 1 connection, POST echoed, 404, 405, 100,000-byte body)"
    finally:
        server.stop()


def s_client(server, payload, expect, timeout=10, keep=False):
    p = subprocess.Popen(["openssl", "s_client", "-connect", f"127.0.0.1:{server.port}", "-servername", HOST,
                          "-CAfile", CA, "-verify_return_error", "-tls1_3", "-quiet"],
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    p.stdin.write(payload)
    p.stdin.flush()
    got = b""
    end = time.monotonic() + timeout
    while time.monotonic() < end and not expect(got):
        r, _, _ = select.select([p.stdout], [], [], 0.2)
        if r:
            part = os.read(p.stdout.fileno(), 65536)
            if not part:
                break
            got += part
    return p, got


def case_openssl(exe):
    server = Server(exe)
    try:
        one = b"GET /hello/a HTTP/1.1\r\nHost: x\r\n\r\n"
        p, got = s_client(server, one, lambda g: g.endswith(b"hello, a\n"))
        if not got.startswith(b"HTTP/1.1 200 OK\r\n") or not got.endswith(b"hello, a\n"):
            p.kill()
            return f"first request: {got!r}"
        p.stdin.write(b"GET /hello/b HTTP/1.1\r\nHost: x\r\n\r\n")
        p.stdin.flush()
        more = b""
        end = time.monotonic() + 5
        while not more.endswith(b"hello, b\n") and time.monotonic() < end:
            r, _, _ = select.select([p.stdout], [], [], 0.2)
            if r:
                more += os.read(p.stdout.fileno(), 65536)
        if not more.endswith(b"hello, b\n"):
            p.kill()
            return f"second request on the same connection: {more!r}"
        three = b"".join(f"GET /hello/p{i} HTTP/1.1\r\nHost: x\r\n\r\n".encode() for i in range(3))
        p.stdin.write(three)
        p.stdin.flush()
        more = b""
        end = time.monotonic() + 5
        while more.count(b"HTTP/1.1 200 OK") < 3 or not more.endswith(b"hello, p2\n"):
            if time.monotonic() > end:
                p.kill()
                return f"three pipelined: {more!r}"
            r, _, _ = select.select([p.stdout], [], [], 0.2)
            if r:
                more += os.read(p.stdout.fileno(), 65536)
        order = [more.index(f"hello, p{i}\n".encode()) for i in range(3)]
        if order != sorted(order):
            p.kill()
            return f"pipelined answers out of order: {more!r}"
        p.stdin.write(b"GET /hello/z HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        p.stdin.flush()
        last = b""
        end = time.monotonic() + 5
        while time.monotonic() < end:
            r, _, _ = select.select([p.stdout], [], [], 0.2)
            if r:
                part = os.read(p.stdout.fileno(), 65536)
                if not part:
                    break
                last += part
        p.stdin.close()
        p.wait(5)
        if b"Connection: close" not in last or not last.endswith(b"hello, z\n"):
            return f"Connection: close: {last!r}"
        server.wait_for(lambda l: " closed ok " in l, 5)
        return "ok (keep-alive, 3 pipelined, Connection: close ended with close_notify)"
    except subprocess.TimeoutExpired:
        return "s_client did not exit after the connection was closed"
    finally:
        server.stop()


def case_http(exe):
    server = Server(exe)
    try:
        h = client(server.port)
        t0 = time.monotonic()
        for i in range(2000):
            status, body = get(h, f"/hello/{i}")
            if status != 200 or body != f"hello, {i}\n".encode():
                return f"request {i}: {status} {body!r}"
        took = time.monotonic() - t0
        # The JSON echo: every escape, non-ASCII, nesting.
        doc = {"s": 'quote " back \\ nl \n tab \t', "u": "café 日 \U0001f600", "n": [1, -2, 3.5, None, True], "o": {"a": {"b": []}}}
        raw = json.dumps(doc).encode()
        h.request("POST", "/echo", body=raw, headers={"Content-Type": "application/json"})
        r = h.getresponse()
        got = json.loads(r.read())
        if r.status != 200 or got["json"] != doc or got["bytes"] != len(raw) or got["path"] != "/echo":
            return f"JSON echo: {r.status} {got}"
        h.request("POST", "/echo", body=b"not json")
        r = h.getresponse()
        r.read()
        if r.status != 400:
            return f"a body that is not JSON: {r.status}"
        # A request body in chunks.
        h.putrequest("POST", "/echo")
        h.putheader("Transfer-Encoding", "chunked")
        h.endheaders()
        for piece in (b'{"a":', b"[1,2,", b"3]}"):
            h.send(f"{len(piece):x}\r\n".encode() + piece + b"\r\n")
        h.send(b"0\r\n\r\n")
        r = h.getresponse()
        got = json.loads(r.read())
        if r.status != 200 or got["json"] != {"a": [1, 2, 3]}:
            return f"chunked request: {r.status} {got}"
        status, body = get(h, "/hello/after-the-body")
        if (status, body) != (200, b"hello, after-the-body\n"):
            return f"the connection after a chunked body: {status} {body!r}"
        # HTTP/1.0: answered, then closed.
        raw = https(server.port)
        raw.sendall(b"GET /hello/old HTTP/1.0\r\n\r\n")
        data = b""
        while True:
            part = raw.recv(4096)
            if not part:
                break
            data += part
        if b"Connection: close" not in data or not data.endswith(b"hello, old\n"):
            return f"HTTP/1.0: {data!r}"
        return f"ok (2,000 requests on one connection in {took:.2f} s, JSON echo, chunked body, HTTP/1.0)"
    finally:
        server.stop()


def read_answers(conn, count, timeout=30):
    """`count` whole Content-Length answers from `conn`: [(status, body)]."""
    conn.settimeout(timeout)
    buf = b""
    out = []
    while len(out) < count:
        while b"\r\n\r\n" not in buf:
            part = conn.recv(65536)
            if not part:
                raise AssertionError(f"closed after {len(out)} of {count} answers")
            buf += part
        head, rest = buf.split(b"\r\n\r\n", 1)
        m = re.search(rb"Content-Length: (\d+)", head)
        n = int(m.group(1)) if m else 0
        while len(rest) < n:
            part = conn.recv(65536)
            if not part:
                raise AssertionError("closed in a body")
            rest += part
        out.append((int(head.split(b" ")[1]), rest[:n]))
        buf = rest[n:]
    return out


def case_pipelined(exe):
    server = Server(exe)
    try:
        c = https(server.port)
        c.sendall(b"".join(f"GET /hello/q{i} HTTP/1.1\r\nHost: x\r\n\r\n".encode() for i in range(300)))
        got = read_answers(c, 300)
        want = [(200, f"hello, q{i}\n".encode()) for i in range(300)]
        if got != want:
            bad = next(i for i, (a, b) in enumerate(zip(got, want)) if a != b)
            return f"answer {bad}: {got[bad]!r}"
        # With bodies, a length and chunked, mixed, and a 404 in the middle.
        reqs = []
        want = []
        for i in range(300):
            doc = json.dumps({"i": i}).encode()
            if i % 3 == 0:
                reqs.append(b"POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: %d\r\n\r\n" % len(doc) + doc)
            elif i % 3 == 1:
                reqs.append(b"POST /echo HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n%x\r\n" % len(doc) + doc + b"\r\n0\r\n\r\n")
            else:
                reqs.append(b"GET /missing%d HTTP/1.1\r\nHost: x\r\n\r\n" % i)
            want.append(i)
        c.sendall(b"".join(reqs))
        got = read_answers(c, 300)
        for i, (status, body) in enumerate(got):
            if i % 3 == 2:
                if status != 404:
                    return f"answer {i}: {status} for a missing path"
            else:
                j = json.loads(body)
                if status != 200 or j["json"] != {"i": i}:
                    return f"answer {i}: {status} {body!r}"
        # And after all that the connection still serves.
        c.sendall(b"GET /hello/last HTTP/1.1\r\nHost: x\r\n\r\n")
        if read_answers(c, 1) != [(200, b"hello, last\n")]:
            return "the connection did not serve after 600 pipelined requests"
        return "ok (300 pipelined, then 300 with length and chunked bodies and 404s, in order)"
    finally:
        server.stop()


def case_many(exe):
    server = Server(exe, extra=["--connections", "256", "--handshakes", "64", "--rate", "100000"])
    try:
        errors = []
        start = threading.Barrier(200)

        def run(k):
            try:
                start.wait(30)
                h = client(server.port, 60)
                for i in range(10):
                    status, body = get(h, f"/hello/c{k}-{i}")
                    if (status, body) != (200, f"hello, c{k}-{i}\n".encode()):
                        raise AssertionError(f"{status} {body!r}")
                h.sock.unwrap()
            except Exception as e:  # noqa: BLE001 -- every failure is reported
                errors.append(f"{k}: {e!r}")

        threads = [threading.Thread(target=run, args=(k,)) for k in range(200)]
        t0 = time.monotonic()
        for t in threads:
            t.start()
        for t in threads:
            t.join(120)
        took = time.monotonic() - t0
        if errors:
            return f"{len(errors)} of 200 failed, first: {errors[0]}"
        server.wait_for(lambda l: " closed ok " in l, 10, 200)
        return f"ok (200 connections, 2,000 requests, {took:.1f} s)"
    finally:
        server.stop()


def case_big(exe):
    server = Server(exe)
    try:
        h = client(server.port, 120)
        sizes = [0, 1, 2, 25, 26, 27, 4095, 16384, 16385, 65535, 65536, 65537, 100000, 1048576, 8 * 1048576, 64 * 1048576]
        total = 0
        t0 = time.monotonic()
        for n in sizes:
            h.request("GET", f"/big/{n}")
            r = h.getresponse()
            if r.status != 200 or int(r.getheader("Content-Length")) != n:
                return f"/big/{n}: {r.status} {r.getheader('Content-Length')}"
            at = 0
            while True:
                part = r.read(1 << 20)
                if not part:
                    break
                if part != pattern(len(part), at):
                    return f"/big/{n}: wrong bytes at offset {at}"
                at += len(part)
            if at != n:
                return f"/big/{n}: {at} bytes"
            total += n
            status, body = get(h, "/hello/between")
            if (status, body) != (200, b"hello, between\n"):
                return f"after /big/{n}: {status} {body!r}"
        took = time.monotonic() - t0
        # A request pipelined behind a big one waits for it and is answered in order.
        c = https(server.port, 120)
        c.sendall(b"GET /big/300000 HTTP/1.1\r\nHost: x\r\n\r\nGET /hello/behind HTTP/1.1\r\nHost: x\r\n\r\n")
        got = read_answers(c, 2, 60)
        if got[0] != (200, pattern(300000)) or got[1] != (200, b"hello, behind\n"):
            return "a request pipelined behind /big was not answered in order"
        # Past the limit and not a number: refused, and the connection goes on.
        for bad in ("/big/1073741825", "/big/x", "/big/-1", "/big/"):
            h.request("GET", bad)
            r = h.getresponse()
            r.read()
            if r.status not in (400, 404):
                return f"{bad}: {r.status}"
        return f"ok ({len(sizes)} sizes to 64 MiB, {total / 1e6:.0f} MB checked byte for byte, {total / 1e6 / took:.0f} MB/s)"
    finally:
        server.stop()


def case_stalled(exe):
    server = Server(exe, extra=["--idle", "2000", "--handshake-timeout", "3000"])
    try:
        # Other clients are served while one stalls; the server's memory does not grow with what the
        # stalled client asked for (1 GiB).
        h = client(server.port)
        get(h, "/")
        before = rss_kb(server.proc.pid)
        stall = https(server.port, 60)
        stall.sendall(b"GET /big/1073741824 HTTP/1.1\r\nHost: x\r\n\r\n")
        got = b""
        while len(got) < 100:
            got += stall.recv(100 - len(got))
        t0 = time.monotonic()
        slowest = 0
        served = 0
        while time.monotonic() - t0 < 1.5:
            a = time.monotonic()
            status, body = get(h, f"/hello/during{served}")
            slowest = max(slowest, time.monotonic() - a)
            if (status, body) != (200, f"hello, during{served}\n".encode()):
                return f"a client beside the stalled one: {status} {body!r}"
            served += 1
        during = rss_kb(server.proc.pid)
        # 16 MiB, not 4: the server serves 18,000+ requests beside the stalled client in this window, and on a shared CI runner the
        # allocator alone grew it 4.1 and 4.2 MB (the same commit passed one run and failed the other at 4,244 KB). A server that
        # buffered the stalled body would grow by the part of the 1 GiB that was sent, orders of magnitude over this.
        if during - before > 16384:
            return f"server RSS grew {during - before} KB while a client stalled on a 1 GiB body"
        # The stalled client does not read; once idle passes the server ends it.
        line = server.wait_for(lambda l: l.startswith("conn 2 ") and " closed idle" in l, 8)[-1]
        stall.settimeout(10)
        ended = False
        try:
            end = time.monotonic() + 10
            while time.monotonic() < end:
                part = stall.recv(1 << 20)
                if not part:
                    ended = True
                    break
        except (ssl.SSLError, ConnectionError, socket.timeout):
            ended = True
        if not ended:
            return "the stalled connection was not ended"
        status, body = get(client(server.port), "/hello/after")
        if (status, body) != (200, b"hello, after\n"):
            return f"after the stalled client was ended: {status} {body!r}"
        after = rss_kb(server.proc.pid)
        return (f"ok ({served} requests served beside it, slowest {slowest * 1000:.0f} ms; RSS {before} -> {during} -> "
                f"{after} KB; server: {line})")
    finally:
        server.stop()


def case_slow(exe):
    server = Server(exe, extra=["--idle", "1000"])
    try:
        slow = https(server.port, 20)
        slow.sendall(b"GET /hello/half HTTP/1.1\r\nHost: x\r\nX-Slow: ")
        h = client(server.port)
        t0 = time.monotonic()
        for i in range(20):
            if get(h, f"/hello/{i}")[0] != 200:
                return "a client beside the slow one was not served"
        beside = time.monotonic() - t0
        slow.settimeout(8)
        ended = False
        t1 = time.monotonic()
        try:
            while time.monotonic() - t1 < 8:
                part = slow.recv(100)
                if not part:
                    ended = True
                    break
        except (ssl.SSLError, ConnectionError, socket.timeout):
            ended = True
        took = time.monotonic() - t1
        if not ended or took > 5:
            return f"the half request was not ended in time ({ended}, {took:.1f} s)"
        return f"ok (20 requests beside it in {beside:.2f} s; half a request ended after {took:.1f} s idle)"
    finally:
        server.stop()


def case_ended(exe):
    server = Server(exe)
    try:
        ended = https(server.port, 10)
        h = client(server.port, 10)
        if get(h, "/hello/b") != (200, b"hello, b\n"):
            return "the keep-alive client was not served before"
        ended.sendall(b"GET /hello/x HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        server.wait_for(lambda l: l.startswith("conn 1 ") and " closed" in l, 5)
        # The ended client keeps writing, from a thread of its own: OpenSSL's error queue is per thread and CPython's
        # `_ssl` does not clear it, so a write refused on `ended` (EPIPE) would otherwise be reported again by the next
        # write on `h` in the same thread, as `h`'s own BrokenPipeError, though `h` is untouched (http-server.md §11.6).
        refused = []
        done = threading.Event()

        def write_on():
            while not done.is_set():
                try:
                    ended.sendall(b"y")
                except (ssl.SSLError, OSError):
                    refused.append(1)
                time.sleep(0.05)

        writer = threading.Thread(target=write_on, daemon=True)
        writer.start()
        try:
            for i in range(40):
                if get(h, f"/hello/k{i}") != (200, f"hello, k{i}\n".encode()):
                    return f"the keep-alive client's request {i} after the other connection ended was not served"
                time.sleep(0.05)
        finally:
            done.set()
            writer.join(5)
        if not refused:
            return "the ended connection's writes were never refused: it was not closed"
        others = [l for l in server.text() if l.startswith("conn ") and not l.startswith("conn 1 ")]
        if any(" closed" in l for l in others):
            return f"another connection was closed: {others}"
        return f"ok (40 requests beside a connection written to after it ended; {len(refused)} of its writes refused)"
    finally:
        server.stop()


def case_reload(exe):
    server = Server(exe, identity="first")
    try:
        old = client(server.port)
        if get(old, "/hello/before") != (200, b"hello, before\n"):
            return "no answer before the reload"
        if echo.serial(old.sock) != echo.file_serial("first"):
            return f"first connection's certificate is {echo.serial(old.sock)}"
        server.install("renewed", files=("chain.pem", "key.pem"))
        server.signal(signal.SIGHUP)
        server.wait_for(lambda l: l == "reload 0 ok", 5)
        new = client(server.port)
        if get(new, "/hello/after") != (200, b"hello, after\n"):
            return "no answer after the reload"
        if echo.serial(new.sock) != echo.file_serial("renewed"):
            return f"a connection after the reload got {echo.serial(new.sock)}"
        if get(old, "/hello/still") != (200, b"hello, still\n"):
            return "the old connection stopped being served"
        server.install("first", files=("chain.pem",))
        server.signal(signal.SIGHUP)
        server.wait_for(lambda l: l == "reload 0 refused tls-server-key-mismatch", 5)
        third = client(server.port)
        if get(third, "/")[0] != 200 or echo.serial(third.sock) != echo.file_serial("renewed"):
            return "after a refused reload the renewed identity did not serve"
        for h in (old, new, third):
            h.sock.unwrap()
        return "ok (old connection kept its certificate and kept being served, new renewed, refused reload left renewed serving)"
    finally:
        server.stop()


def case_bound(exe):
    server = Server(exe, extra=["--handshakes", "2", "--handshake-timeout", "1500"])
    try:
        stalls = [socket.create_connection(("127.0.0.1", server.port)) for _ in range(2)]
        time.sleep(0.3)
        t0 = time.monotonic()
        h = client(server.port, 10)
        took = time.monotonic() - t0
        status, body = get(h, "/hello/behind-two-stalled-handshakes")
        h.sock.unwrap()
        timeouts = server.wait_for(lambda l: "closed handshake-timeout" in l, 5, 2)
        line = server.wait_for(lambda l: " established " in l, 5)[0]
        waited = int(echo.field(line, "waited"))
        for s in stalls:
            s.close()
        if status != 200:
            return f"the honest client: {status}"
        if took < 1.0 or waited < 1000:
            return f"not delayed ({took:.2f} s, waited={waited})"
        return f"ok (delayed {took:.2f} s, waited={waited} ms; {len(timeouts)} stalled handshakes timed out)"
    finally:
        server.stop()


def case_full(exe):
    server = Server(exe, extra=["--connections", "4"])
    try:
        held = [socket.create_connection(("127.0.0.1", server.port)) for _ in range(4)]
        time.sleep(0.3)
        extra = socket.create_connection(("127.0.0.1", server.port), timeout=5)
        t0 = time.monotonic()
        try:
            got = extra.recv(1)
        except ConnectionResetError:
            got = b""
        took = time.monotonic() - t0
        server.wait_for(lambda l: l.endswith(" full"), 5)
        for s in held + [extra]:
            s.close()
        if got != b"" or took > 1.0:
            return f"the fifth connection was not closed at once ({got!r}, {took:.2f} s)"
        return f"ok (closed in {took * 1000:.0f} ms)"
    finally:
        server.stop()


def case_idle(exe):
    server = Server(exe, extra=["--idle", "1000"])
    try:
        h = client(server.port)
        get(h, "/")
        t0 = time.monotonic()
        h.sock.settimeout(5)
        got = h.sock.recv(10)
        took = time.monotonic() - t0
        server.wait_for(lambda l: "closed idle" in l, 5)
        if got != b"" or took < 0.8 or took > 2.5:
            return f"not closed at the idle timeout ({got!r} after {took:.2f} s)"
        return f"ok (close_notify after {took:.2f} s)"
    finally:
        server.stop()


def case_shutdown(exe):
    server = Server(exe)
    try:
        hs = [client(server.port) for _ in range(10)]
        for k, h in enumerate(hs):
            if get(h, f"/hello/s{k}")[0] != 200:
                return "no answer"
        t0 = time.monotonic()
        server.signal(signal.SIGTERM)
        notified = 0
        for h in hs:
            h.sock.settimeout(5)
            if h.sock.recv(10) == b"":
                notified += 1
        status = server.proc.wait(5)
        took = time.monotonic() - t0
        server.wait_for(lambda l: "closed shutdown" in l, 5, 10)
        if notified != 10 or status != 0:
            return f"{notified} of 10 got close_notify; exit status {status}"
        return f"ok (10 close_notify, exit 0 in {took:.2f} s)"
    finally:
        server.stop()


def case_hostile(exe):
    import random
    server = Server(exe, extra=["--connections", "64", "--handshakes", "16", "--rate", "100000"])
    try:
        rnd = random.Random(20261007)
        bases = [b"GET /hello/x HTTP/1.1\r\nHost: h\r\n\r\n",
                 b"POST /echo HTTP/1.1\r\nHost: h\r\nContent-Length: 11\r\n\r\n{\"a\":[1,2]}",
                 b"POST /echo HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{\"a\":\r\n3\r\n1}x\r\n0\r\n\r\n",
                 b"GET /big/70000 HTTP/1.1\r\nHost: h\r\nConnection: close\r\n\r\n",
                 b"DELETE /echo HTTP/1.1\r\nHost: h\r\n\r\n"]
        specials = [b"POST /echo HTTP/1.1\r\nContent-Length: 99999999999999999999\r\n\r\n",
                    b"POST /echo HTTP/1.1\r\nContent-Length: -1\r\n\r\n",
                    b"POST /echo HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\n",
                    b"POST /echo HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nab\r\n0\r\n\r\n",
                    b"GET /" + b"a" * 100000 + b" HTTP/1.1\r\n\r\n",
                    b"GET / HTTP/1.1\r\n" + b"X: y\r\n" * 1000 + b"\r\n",
                    b"POST /echo HTTP/1.1\r\nContent-Length: 10001\r\n\r\n" + b"[" * 5000 + b"]" * 5000,
                    b"GET /big/99999999999999999999 HTTP/1.1\r\nHost: h\r\n\r\n",
                    b"GET /big/%31 HTTP/1.1\r\nHost: h\r\n\r\n",
                    b"\r\n" * 200, b"\x00" * 1000, b"GET / HTTP/1.1\nHost: h\n\n", b"GET / HTTP/9.9\r\n\r\n"]
        sent = 0
        for i in range(1500):
            if i % 5 == 0:
                data = rnd.choice(specials)
            else:
                data = bytearray(rnd.choice(bases))
                for _ in range(rnd.randint(0, 4)):
                    kind = rnd.randint(0, 3)
                    at = rnd.randrange(len(data) + 1)
                    if kind == 0 and data:
                        data[min(at, len(data) - 1)] = rnd.randrange(256)
                    elif kind == 1:
                        del data[at:]
                    elif kind == 2:
                        data[at:at] = rnd.choice([b"\r\n", b"\n", b"\x00", b" ", b":"])
                    else:
                        data[at:at] = rnd.randbytes(rnd.randint(1, 20))
                data = bytes(data)
            try:
                c = https(server.port, 5)
                c.sendall(data)
                c.settimeout(0.05)
                try:
                    while c.recv(65536):
                        pass
                except (socket.timeout, ssl.SSLError, ConnectionError, OSError):
                    pass
                c.close()
            except (ssl.SSLError, ConnectionError, OSError):
                pass
            sent += 1
            if server.proc.poll() is not None:
                return f"the server died after {sent} requests (exit {server.proc.returncode}), last: {data[:200]!r}"
        h = client(server.port)
        status, body = get(h, "/hello/alive")
        if (status, body) != (200, b"hello, alive\n"):
            return f"after the hostile requests: {status} {body!r}"
        return f"ok ({sent} mangled requests; the server is alive and serves)"
    finally:
        server.stop()


# ---- uploads ----

MIB = 1 << 20
UPLOAD_TOP = int(os.environ.get("UPLOAD_TOP_MIB", "1024")) * MIB
BLOCK = os.urandom(MIB + 13)
BLOCK2 = BLOCK + BLOCK  # so a piece starting anywhere in the first MiB and up to 1 MiB long is never cut short


class Wire:
    """A TLS socket with a read buffer, so answers can be read one at a time however they arrive."""

    def __init__(self, port, timeout=600):
        self.sock = https(port, timeout)
        self.buf = b""

    def sendall(self, data):
        self.sock.sendall(data)

    def response(self, timeout=600):
        """One answer: (status, {header: value}, body). A `100 Continue` is an answer like any other."""
        self.sock.settimeout(timeout)
        while b"\r\n\r\n" not in self.buf:
            part = self.sock.recv(65536)
            if not part:
                raise AssertionError(f"closed before an answer: {self.buf[:200]!r}")
            self.buf += part
        head, rest = self.buf.split(b"\r\n\r\n", 1)
        lines = head.decode("latin-1").split("\r\n")
        headers = {}
        for line in lines[1:]:
            name, _, value = line.partition(":")
            headers[name.strip().lower()] = value.strip()
        n = int(headers.get("content-length", 0))
        while len(rest) < n:
            part = self.sock.recv(65536)
            if not part:
                raise AssertionError("closed in a body")
            rest += part
        self.buf = rest[n:]
        return int(lines[0].split(" ")[1]), headers, rest[:n]

    def ended(self, timeout=8):
        """Does the connection end (close_notify, a reset, end of file) within `timeout` seconds, with nothing but the buffer left?"""
        end = time.monotonic() + timeout
        self.sock.settimeout(1)
        while time.monotonic() < end:
            try:
                if not self.sock.recv(65536):
                    return True
            except socket.timeout:
                continue
            except (ssl.SSLError, ConnectionError, OSError):
                return True
        return False

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass


def body_pieces(size, piece):
    """`size` bytes of the repeating block, `piece` at a time, starting somewhere different each time."""
    view = memoryview(BLOCK2)
    sent = 0
    while sent < size:
        n = min(piece, size - sent)
        start = (sent * 7) % MIB
        yield view[start:start + n]
        sent += n


def upload(wire, path, size, chunk=0, headers=b"", ext=b"", trailers=b"", wait100=False):
    """POST `size` bytes to `path`: a length body if `chunk` is 0, else chunked in pieces of `chunk` bytes (an extension `ext` after every
    seventh size, `trailers` after the last chunk). The answer, and the SHA-256 of what was sent."""
    digest = hashlib.sha256()
    if chunk:
        head = b"POST %s HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n%s\r\n" % (path.encode(), headers)
    else:
        head = b"POST %s HTTP/1.1\r\nHost: x\r\nContent-Length: %d\r\n%s\r\n" % (path.encode(), size, headers)
    wire.sendall(head)
    if wait100:
        status, _, _ = wire.response(30)
        if status != 100:
            raise AssertionError(f"waiting for 100 Continue: {status}")
    k = 0
    for piece in body_pieces(size, chunk or MIB):
        digest.update(piece)
        if chunk:
            wire.sendall(b"%x%s\r\n" % (len(piece), ext if k % 7 == 0 else b"") + bytes(piece) + b"\r\n")
            k += 1
        else:
            wire.sendall(piece)
    if chunk:
        wire.sendall(b"0\r\n" + trailers + b"\r\n")
    return wire.response(), digest.hexdigest()


def check_upload(wire, size, **kw):
    (status, _, body), want = upload(wire, "/upload", size, **kw)
    got = json.loads(body) if status == 200 else None
    if status != 200 or got != {"bytes": size, "sha256": want}:
        return f"{size} bytes {kw}: {status} {body[:200]!r} (wanted {want})"
    return None


def case_upload(exe):
    server = Server(exe)
    try:
        sizes = [s for s in (0, 1, 2, 16383, 16384, 16385, 65535, 65536, 65537, MIB, 16 * MIB, 128 * MIB) if s < UPLOAD_TOP] + [UPLOAD_TOP]
        c = Wire(server.port)
        before = rss_kb(server.proc.pid)
        peak = [before]
        stop = threading.Event()

        def sample():
            while not stop.is_set():
                peak[0] = max(peak[0], rss_kb(server.proc.pid))
                time.sleep(0.2)

        sampler = threading.Thread(target=sample, daemon=True)
        sampler.start()
        t0 = time.monotonic()
        total = 0
        try:
            for size in sizes:
                for kw in ({}, {"chunk": 65536 if size else 1}):
                    failed = check_upload(c, size, **kw)
                    if failed:
                        return failed
                    total += size
            took = time.monotonic() - t0
        finally:
            stop.set()
            sampler.join()
        # The connection that carried all of that still serves.
        c.sendall(b"GET /hello/after HTTP/1.1\r\nHost: x\r\n\r\n")
        if c.response()[2] != b"hello, after\n":
            return "the connection did not serve after the uploads"
        if peak[0] - before > 16384:
            return f"server RSS went {before} -> {peak[0]} KB over {total / MIB:.0f} MiB of uploads"
        # Pieces of every size, with extensions and trailers, on fresh connections.
        for chunk in (1, 2, 7, 100, 4095, 16384, 16385, 1000003):
            n = min(chunk * 300, 4 * MIB)
            failed = check_upload(Wire(server.port, 120), n, chunk=chunk, ext=b";name=\"v v\";k", trailers=b"X-Sum: 1\r\nX-Other: two words\r\n")
            if failed:
                return failed
        # Pipelined: an upload, a request behind it, an upload, all in one write; answers in order.
        body = os.urandom(300000)
        e = Wire(server.port, 120)
        e.sendall(b"POST /upload HTTP/1.1\r\nHost: x\r\nContent-Length: 300000\r\n\r\n" + body
                  + b"GET /hello/mid HTTP/1.1\r\nHost: x\r\n\r\n"
                  + b"POST /upload HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n%x\r\n" % len(body) + body + b"\r\n0\r\n\r\n")
        want = hashlib.sha256(body).hexdigest()
        answer = json.dumps({"bytes": len(body), "sha256": want}, separators=(",", ":")).encode()
        for expected in (answer, b"hello, mid\n", answer):
            status, _, got = e.response(60)
            if (status, got) != (200, expected):
                return f"pipelined: {status} {got[:100]!r}"
        return (f"ok ({len(sizes)} sizes to {UPLOAD_TOP // MIB} MiB, each with a length and chunked, {total / MIB:.0f} MiB hashed correctly at "
                f"{total / MIB / took:.0f} MiB/s; server RSS {before} -> {peak[0]} KB; chunks of 1 to 1,000,003 bytes; pipelined)")
    finally:
        server.stop()


def case_expect(exe):
    server = Server(exe, extra=["--max-body", "1"])
    try:
        # The `100` comes when the application accepts the body, and the upload completes.
        w = Wire(server.port)
        t0 = time.monotonic()
        (status, _, body), want = upload(w, "/upload", 300000, headers=b"Expect: 100-continue\r\n", wait100=True)
        if status != 200 or json.loads(body)["sha256"] != want:
            return f"Expect upload: {status} {body[:100]!r}"
        waited = time.monotonic() - t0
        # Refused by its head: no `100`, the first thing back is the 413, and the connection serves after.
        w.sendall(b"POST /upload/refuse HTTP/1.1\r\nHost: x\r\nContent-Length: 900000\r\nExpect: 100-continue\r\n\r\n")
        status, headers, body = w.response(10)
        if status != 413 or b"takes no body" not in body:
            return f"refused: {status} {body!r}"
        w.sendall(b"GET /hello/after HTTP/1.1\r\nHost: x\r\n\r\n")
        if w.response(10)[2] != b"hello, after\n":
            return "the connection did not serve after a refusal that sent no 100"
        # The same for a client that does not wait and sends some of its body: refused, and the connection ends.
        w2 = Wire(server.port)
        w2.sendall(b"POST /upload/refuse HTTP/1.1\r\nHost: x\r\nContent-Length: 1000000\r\n\r\n" + b"x" * 5000)
        if w2.response(10)[0] != 413 or not w2.ended():
            return "a refused upload that was already sending was not refused and ended"
        # Another expectation: 417, with its rule. A length over --max-body (1 MiB): 413 by the head, before any 100.
        w3 = Wire(server.port)
        w3.sendall(b"POST /upload HTTP/1.1\r\nHost: x\r\nContent-Length: 10\r\nExpect: gimme\r\n\r\n")
        status, headers, _ = w3.response(10)
        if status != 417 or headers.get("x-rule") != "expect.unsupported":
            return f"Expect: gimme: {status} {headers}"
        w4 = Wire(server.port)
        w4.sendall(b"POST /upload HTTP/1.1\r\nHost: x\r\nContent-Length: %d\r\nExpect: 100-continue\r\n\r\n" % (MIB + 1))
        status, headers, _ = w4.response(10)
        if status != 413 or headers.get("x-rule") != "body.too-large":
            return f"a length over --max-body: {status} {headers}"
        # Chunked past --max-body: refused while it is still being sent.
        w5 = Wire(server.port)
        w5.sendall(b"POST /upload HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n")
        for _ in range(17):
            # Sixteen chunks of 64 KiB are exactly the 1 MiB; the size line of the seventeenth is refused.
            w5.sendall(b"10000\r\n" + b"z" * 65536 + b"\r\n")
        status, headers, _ = w5.response(10)
        if status != 413 or headers.get("x-rule") != "body.too-large":
            return f"chunked past --max-body: {status} {headers}"
        notes = [f"waited {waited * 1000:.0f} ms for the whole upload, 100 included"]
        # curl: -T with Expect, and a refused upload that sends no byte of its body.
        version = subprocess.run(["curl", "--version"], capture_output=True).stdout.decode()
        if "OpenSSL" in version.split("\n")[0]:
            path = os.path.join(server.work, "payload")
            with open(path, "wb") as f:
                f.write(BLOCK[:900000])
            common = ["curl", "-sS", "--cacert", CA, "--resolve", f"{HOST}:{server.port}:127.0.0.1", "--tlsv1.3", "-X", "POST",
                      "-H", "Expect: 100-continue", "--expect100-timeout", "10", "-T", path, "-w", "\n%{http_code} %{size_upload}"]
            t0 = time.monotonic()
            out = subprocess.run(common + [f"https://{HOST}:{server.port}/upload"], capture_output=True)
            took = time.monotonic() - t0
            body, _, tail = out.stdout.rpartition(b"\n")
            if tail != b"200 900000" or json.loads(body)["sha256"] != hashlib.sha256(BLOCK[:900000]).hexdigest():
                return f"curl -T: {out.stdout[:200]!r} {out.stderr[:200]!r}"
            if took > 5:
                return f"curl -T waited {took:.1f} s: the 100 did not come"
            out = subprocess.run(common + [f"https://{HOST}:{server.port}/upload/refuse"], capture_output=True)
            tail = out.stdout.rpartition(b"\n")[2]
            if tail != b"413 0":
                return f"curl refused upload: {out.stdout[:200]!r} {out.stderr[:200]!r} (wanted 413 and 0 bytes uploaded)"
            notes.append("curl -T with Expect uploaded 900,000 bytes in %.2f s; a refused one uploaded 0 bytes" % took)
        else:
            notes.append("curl skipped: not built with OpenSSL")
        # openssl s_client uploading 300 KB, answered.
        body = os.urandom(300000)
        p, got = s_client(server, b"POST /upload HTTP/1.1\r\nHost: x\r\nContent-Length: 300000\r\n\r\n" + body, lambda g: g.endswith(b'"}'), timeout=30)
        p.kill()
        p.wait()
        if hashlib.sha256(body).hexdigest().encode() not in got:
            return f"openssl s_client upload: {got[-200:]!r}"
        notes.append("openssl s_client uploaded 300,000 bytes")
        return "ok (" + "; ".join(notes) + ")"
    finally:
        server.stop()


def case_halfbody(exe):
    server = Server(exe, extra=["--read-timeout", "1000"])
    try:
        stalled = []
        # Half a head; half a length body; half a chunked body; a head a byte at a time.
        a = Wire(server.port, 30)
        a.sendall(b"POST /upload HTTP/1.1\r\nHost: x\r\nX-Slow: ")
        b = Wire(server.port, 30)
        b.sendall(b"POST /upload HTTP/1.1\r\nHost: x\r\nContent-Length: 100000\r\n\r\n" + b"b" * 50000)
        c = Wire(server.port, 30)
        c.sendall(b"POST /upload HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n186a0\r\n" + b"c" * 50000)
        d = Wire(server.port, 30)
        stalled = [("head", a, "timeout.head"), ("length body", b, "timeout.body"), ("chunked body", c, "timeout.body"), ("trickled head", d, "timeout.head")]
        t0 = time.monotonic()
        answers = {}

        def wait_for(name, wire):
            try:
                answers[name] = (wire.response(10), time.monotonic() - t0)
            except Exception as e:  # noqa: BLE001 -- reported below
                answers[name] = (e, time.monotonic() - t0)

        waiters = [threading.Thread(target=wait_for, args=(name, wire), daemon=True) for name, wire, _ in stalled]
        for w in waiters:
            w.start()
        beside = 0
        slowest = 0
        h = client(server.port)
        trickle = b"POST /upload HTTP/1.1\r\nHost: x\r\nX-Slow: yyyyyyyyyyyyyyyyyyyyyyyy"
        i = 0
        while time.monotonic() - t0 < 1.4:
            if i < len(trickle) and time.monotonic() - t0 < 0.9:
                d.sendall(trickle[i:i + 1])
                i += 1
            started = time.monotonic()
            status, body = get(h, f"/hello/beside{beside}")
            slowest = max(slowest, time.monotonic() - started)
            if (status, body) != (200, f"hello, beside{beside}\n".encode()):
                return f"a client beside the stalled ones: {status} {body!r}"
            beside += 1
            time.sleep(0.05)
        for w in waiters:
            w.join(15)
        waits = []
        for name, wire, rule in stalled:
            got, when = answers[name]
            if isinstance(got, Exception):
                return f"{name}: {got!r}"
            status, headers, _ = got
            waits.append(when)
            if status != 408 or headers.get("x-rule") != rule:
                return f"{name}: {status} {headers}"
            if not wire.ended():
                return f"{name}: the connection was not ended after the 408"
        if min(waits) < 0.9:
            return f"a 408 came after {min(waits):.2f} s, before the 1 s deadline"
        # The server is fine, and an upload that sends its body in time is not timed out however long it takes in all.
        w = Wire(server.port, 60)
        w.sendall(b"POST /upload HTTP/1.1\r\nHost: x\r\nContent-Length: 6000\r\n\r\n")
        digest = hashlib.sha256()
        for k in range(6):
            piece = os.urandom(1000)
            digest.update(piece)
            w.sendall(piece)
            time.sleep(0.5)
        status, _, body = w.response(10)
        if status != 200 or json.loads(body)["sha256"] != digest.hexdigest():
            return f"a slow but steady upload (3 s in all against a 1 s timeout): {status} {body[:100]!r}"
        return (f"ok (4 stalled clients answered 408 after {min(waits):.2f} to {max(waits):.2f} s; {beside} requests beside them, slowest "
                f"{slowest * 1000:.0f} ms; a steady upload of 3 s was not timed out)")
    finally:
        server.stop()


def case_mangled(exe):
    import random
    server = Server(exe, extra=["--connections", "64", "--handshakes", "16", "--rate", "100000", "--read-timeout", "300", "--max-body", "4"])
    try:
        rnd = random.Random(20261008)
        body = os.urandom(3000)
        chunked = b"%x;a=b\r\n" % len(body) + body + b"\r\n0\r\nT: v\r\n\r\n"
        bases = [b"POST /upload HTTP/1.1\r\nHost: h\r\nContent-Length: 3000\r\n\r\n" + body,
                 b"POST /upload HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n" + chunked,
                 b"POST /upload HTTP/1.1\r\nHost: h\r\nExpect: 100-continue\r\nContent-Length: 3000\r\n\r\n" + body,
                 b"POST /echo HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n" + chunked,
                 b"POST /upload/refuse HTTP/1.1\r\nHost: h\r\nExpect: 100-continue\r\nContent-Length: 3000\r\n\r\n" + body]
        specials = [b"POST /upload HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\nffffffff\r\n",
                    b"POST /upload HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n" + b"X: " + b"a" * 5000 + b"\r\n\r\n",
                    b"POST /upload HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n5;" + b"e" * 300 + b"\r\nhello\r\n0\r\n\r\n",
                    b"POST /upload HTTP/1.1\r\nHost: h\r\nContent-Length: 99999999999999999999\r\n\r\n",
                    b"POST /upload HTTP/1.1\r\nHost: h\r\nContent-Length: 5\r\nContent-Length: 6\r\n\r\nhello",
                    b"POST /upload HTTP/1.1\r\nHost: h\r\nContent-Length: 5\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
                    b"POST /upload HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: gzip, chunked\r\n\r\n",
                    b"POST /upload HTTP/1.1\r\nHost: h\r\nExpect: 100-continue\r\nExpect: nope\r\nContent-Length: 5\r\n\r\n",
                    b"POST /upload HTTP/1.1\r\nHost: h\r\nContent-Length: 3000\r\n\r\n" + b"z" * 10,
                    b"POST /upload HTTP/1.1\r\nHost: h\r\nTransfer-Encoding: chunked\r\n\r\n" + b"1\r\na\r\n" * 3000]
        before = rss_kb(server.proc.pid)
        sent = 0
        for i in range(1500):
            if i % 5 == 0:
                data = rnd.choice(specials)
            else:
                data = bytearray(rnd.choice(bases))
                for _ in range(rnd.randint(0, 4)):
                    kind = rnd.randint(0, 4)
                    at = rnd.randrange(len(data) + 1)
                    if kind == 0 and data:
                        data[min(at, len(data) - 1)] = rnd.randrange(256)
                    elif kind == 1:
                        del data[at:]
                    elif kind == 2:
                        data[at:at] = rnd.choice([b"\r\n", b"\n", b"\x00", b" ", b":", b";", b"0\r\n\r\n", b"ffff\r\n"])
                    elif kind == 3:
                        data[at:at] = rnd.randbytes(rnd.randint(1, 20))
                    else:
                        data[at:at + rnd.randint(1, 30)] = b""
                data = bytes(data)
            try:
                c = https(server.port, 5)
                c.sendall(data)
                c.settimeout(0.05)
                try:
                    while c.recv(65536):
                        pass
                except (socket.timeout, ssl.SSLError, ConnectionError, OSError):
                    pass
                c.close()
            except (ssl.SSLError, ConnectionError, OSError):
                pass
            sent += 1
            if server.proc.poll() is not None:
                return f"the server died after {sent} requests (exit {server.proc.returncode}), last: {data[:200]!r}"
        time.sleep(1.5)
        failed = check_upload(Wire(server.port, 60), 1000000)
        if failed:
            return f"after the mangled uploads: {failed}"
        failed = check_upload(Wire(server.port, 60), 100000, chunk=777)
        if failed:
            return f"after the mangled uploads: {failed}"
        after = rss_kb(server.proc.pid)
        if after - before > 16384:
            return f"server RSS went {before} -> {after} KB over {sent} mangled uploads"
        return f"ok ({sent} mangled uploads; the server is alive, RSS {before} -> {after} KB, and a good upload still hashes)"
    finally:
        server.stop()


def case_tickets(exe):
    k1, k2, k3 = (os.urandom(32).hex().encode() for _ in range(3))
    flags = ["--tickets", "2", "--ticket-keys", "keys"]
    a = Server(exe, extra=flags, files={"keys": k1 + b"\n"})
    b = Server(exe, extra=flags, files={"keys": k1 + b"\n"})
    c = Server(exe, extra=flags, files={"keys": k3 + b"\n"})
    try:
        def fetch(port, sess=None):
            raw = socket.create_connection(("127.0.0.1", port), timeout=10)
            conn = echo.SESSION_CONTEXT.wrap_socket(raw, server_hostname=HOST, session=sess)
            conn.sendall(b"GET /hello/ticket HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
            got = b""
            while True:
                part = conn.recv(65536)
                if not part:
                    break
                got += part
            reused, new = conn.session_reused, conn.session
            conn.close()
            if not got.startswith(b"HTTP/1.1 200"):
                raise AssertionError(f"an answer of {got[:80]!r}")
            return reused, new

        reused, sess = fetch(a.port)
        if reused:
            return "the first request resumed"
        for n in range(2):
            reused, _ = fetch(a.port, sess)
            if not reused:
                return f"resumption {n} did not resume"
        if not fetch(b.port, sess)[0]:
            return "a process with the same key file did not resume the session"
        if fetch(c.port, sess)[0]:
            return "a process with another key file resumed it"
        open(os.path.join(a.work, "keys"), "wb").write(k2 + b"\n" + k1 + b"\n")
        a.signal(signal.SIGHUP)
        a.wait_for(lambda l: l == "reload tickets ok", 5)
        if not fetch(a.port, sess)[0]:
            return "after a rotation the previous key no longer opened the session"
        open(os.path.join(a.work, "keys"), "wb").write(k2 + b"\n")
        a.signal(signal.SIGHUP)
        a.wait_for(lambda l: l == "reload tickets ok", 5, 2)
        if fetch(a.port, sess)[0]:
            return "a session from a dropped key resumed"
        return "ok (resumed twice, at a second process with the same keys, across a rotation; the dropped key refused)"
    finally:
        for srv in (a, b, c):
            srv.stop()


CASES = {"curl": case_curl, "openssl": case_openssl, "http": case_http, "pipelined": case_pipelined, "many": case_many,
         "big": case_big, "stalled": case_stalled, "slow": case_slow, "ended": case_ended, "reload": case_reload, "bound": case_bound,
         "full": case_full, "idle": case_idle, "shutdown": case_shutdown, "hostile": case_hostile, "upload": case_upload,
         "expect": case_expect, "halfbody": case_halfbody, "mangled": case_mangled, "tickets": case_tickets}


# ---- the cost ----

def cost(exe, seconds, kload, tload, plain):
    """Requests a second, one core for the server (`taskset -c 0` where there is one), the load on two others, three rounds;
    and the server's own CPU a request, which a loaded or virtual machine disturbs less than a rate."""
    rows = []
    pin = ["taskset", "-c", "0"] if shutil.which("taskset") else []
    load_pin = ["taskset", "-c", "2,3"] if shutil.which("taskset") else []
    if plain:
        port = echo.free_port()
        proc = subprocess.Popen(pin + [plain, str(port)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            time.sleep(1)
            before = echo.cpu_seconds(proc.pid)
            rounds = [int(subprocess.run(load_pin + [kload, str(port), "2", "16", str(seconds), "/users/42"], capture_output=True).stdout.split()[0])
                      for _ in range(3)]
            cpu = echo.cpu_seconds(proc.pid) - before
            rows.append(("plain `examples/api` over kload, GET /users/42", rounds, cpu / (sum(rounds) * seconds) * 1e6))
        finally:
            proc.kill()
            proc.wait()
    server = Server(exe, extra=["--connections", "256", "--handshakes", "256", "--rate", "100000", "--idle", "120000"])
    try:
        if pin:
            os.system(f"taskset -cp 0 {server.proc.pid} >/dev/null")
        rounds = []
        before = echo.cpu_seconds(server.proc.pid)
        for _ in range(3):
            out = subprocess.run(load_pin + [tload, str(server.port), "2", "16", str(seconds), "/hello/42", CA, HOST], capture_output=True)
            rounds.append(int(out.stdout.split()[0]))
        cpu = echo.cpu_seconds(server.proc.pid) - before
        rows.append(("`https_hello` over tload (TLS 1.3, keep-alive), GET /hello/42", rounds, cpu / (sum(rounds) * seconds) * 1e6))
    finally:
        server.stop()
    return rows


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    exe = os.path.abspath(args.pop(0))
    seconds = 0
    tools = {}
    if "--cost" in args:
        k = args.index("--cost")
        seconds = int(args[k + 1])
        del args[k:k + 2]
        for name in ("kload", "tload", "plain"):
            if f"--{name}" in args:
                k = args.index(f"--{name}")
                tools[name] = os.path.abspath(args[k + 1])
                del args[k:k + 2]
    names = args or list(CASES)
    failed = 0
    for name in names if not seconds else []:
        try:
            result = CASES[name](exe)
        except Exception as e:  # noqa: BLE001 -- a case that raises has failed
            result = f"failed: {e!r}"
        ok = result.startswith("ok")
        failed += 0 if ok else 1
        print(f"{name:10} {result}", flush=True)
    if seconds:
        for what, rounds, per in cost(exe, seconds, tools["kload"], tools["tload"], tools.get("plain")):
            print(f"cost       {what}: {' '.join(str(r) for r in rounds)} requests a second ({seconds} s rounds), "
                  f"{per:.2f} microseconds of server CPU a request")
        return 0
    print(f"{len(names) - failed} of {len(names)} cases ok")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
