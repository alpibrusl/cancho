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

With `--cost <seconds>`: requests a second over TLS against `kload` (benches/server/kload.c, plain, `examples/api`) and
`tload` (benches/server/tload.c, the same closed loop over OpenSSL) on one core each side; see docs/http-server.md §11.7.
One line a case, then a count; exit status 1 if any failed.
"""
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
        if during - before > 4096:
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
        server.wait_for(lambda l: l.startswith("conn 1 closed"), 5)
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


CASES = {"curl": case_curl, "openssl": case_openssl, "http": case_http, "pipelined": case_pipelined, "many": case_many,
         "big": case_big, "stalled": case_stalled, "slow": case_slow, "ended": case_ended, "reload": case_reload, "bound": case_bound,
         "full": case_full, "idle": case_idle, "shutdown": case_shutdown, "hostile": case_hostile}


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
