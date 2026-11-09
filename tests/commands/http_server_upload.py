# http_server_upload.py -- `packages/http-server`'s streaming request bodies on the socket path (docs/http-server.md section 12), moved from
# crates/cancho/tests/conformance/http_server_upload.rs (docs/gate.md, G4 wave 3). The program tests/programs/server_upload.cho serves uploads
# over real sockets, so `step`, `settle`, `wait` and the timers are exercised as well as the byte-fed path tests/packages/http_server_body_test.cho
# covers without any. The server is built from the package's source (HTTP_SERVER_SOURCE names another file, as scripts/http_server_body_mutants.py
# does); the store is checked on its own, in http_server.
#     python3 tests/commands/http_server_upload.py CANCHO SCRATCH     (from the root of the repository)
import os
import sys
import time

sys.path.insert(0, "tests/support")
from sk import body_of, build, check, concurrently, run, same, tally, Server

cancho, scratch = sys.argv[1], sys.argv[2]
source = os.environ.get("HTTP_SERVER_SOURCE", "packages/http-server/server.cho")
exe = build(cancho, scratch, "server_upload", ["tests/programs/server_upload.cho", source])


def upload(size, max_body, head_ms, body_ms, lazy=0):
    return Server(exe, [size, max_body, head_ms, body_ms, lazy])


def post(path, length, extra=""):
    return ("POST %s HTTP/1.1\r\nHost: t\r\nContent-Length: %d\r\n%s\r\n" % (path, length, extra)).encode()


def chunked(body, piece, ext, trailers):
    out = b""
    for at in range(0, len(body), piece):
        c = body[at:at + piece]
        out += ("%x%s\r\n" % (len(c), ext)).encode() + c + b"\r\n"
    return out + ("0%s\r\n%s\r\n" % (ext, trailers)).encode()


WHOLE = b"GET /whole HTTP/1.1\r\nHost: t\r\n\r\n"
CHUNKED_POST = b"POST /upload HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n"


def arrive_byte_for_byte():
    with upload(4096, 64 << 20, 2000, 2000) as server:
        s = server.client()
        # A length body of 3 MiB, the client waiting for `100 Continue`: the server answers it only because the application proceeds.
        body = body_of(3 << 20, 1)
        s.send(post("/upload", len(body), "Expect: 100-continue\r\n"))
        check(s.head().startswith("HTTP/1.1 100 Continue\r\n\r\n"), "no 100 Continue")
        s.send(body)
        head, text = s.response()
        check(head.startswith("HTTP/1.1 200 "), head)
        same(text, tally(body), "3 MiB length body")
        # The same connection: chunked, with extensions and a trailer, in pieces the buffer (4096) does not line up with.
        body = body_of(1 << 20, 2)
        s.send(CHUNKED_POST)
        s.send(chunked(body, 777, ";k=v", "X-T: 1\r\n"))
        same(s.response()[1], tally(body), "chunked body")
        # Without waiting: head and body in one write, and a request pipelined behind it in the same write.
        body = body_of(100_000, 3)
        s.send(post("/upload", len(body)) + body + b"GET /whole HTTP/1.1\r\nHost: t\r\n\r\nPOST /whole HTTP/1.1\r\nHost: t\r\nContent-Length: 5\r\n\r\nhello")
        same(s.response()[1], tally(body), "pipelined body")
        same(s.response()[1], "n=0 c=0", "pipelined GET")
        same(s.response()[1], tally(b"hello"), "pipelined POST")
        # A body of nothing, and one that fits: not streamed at all.
        s.send(post("/upload", 0))
        same(s.response()[1], "n=0 c=0", "empty body")

        # Four uploads at the same time on four connections, each its own bytes: every one arrives whole, not mixed with its neighbours'.
        def one(i):
            def go():
                c = server.client(60)
                b = body_of((2 << 20) + i * 1001, 10 + i)
                c.send(post("/upload", len(b)))
                c.pieces(b, 7919)
                return c.response()[1], tally(b)
            return go

        for got, want in concurrently([one(i) for i in range(4)]):
            same(got, want, "concurrent upload")
        # Sizes around the buffer, on fresh connections.
        for n in [1, 4095, 4096, 4097, 8192, 65535, 65536, 65537]:
            c = server.client()
            b = body_of(n, n)
            c.send(post("/upload", n) + b)
            same(c.response()[1], tally(b), "%d bytes" % n)


def refusals_before_the_body():
    with upload(4096, 1 << 20, 2000, 2000) as server:
        s = server.client()
        # Refused on the head: no `100`, and the client never sent the body, so the connection goes on.
        s.send(post("/refuse", 500_000, "Expect: 100-continue\r\n"))
        head, text = s.response()
        check(head.startswith("HTTP/1.1 413 "), head)
        check("refused" in text, text)
        s.send(WHOLE)
        same(s.response()[1], "n=0 c=0", "the connection goes on")
        # The same without `Expect`, and a client that sends the body anyway: refused, and the connection closes.
        c = server.client()
        c.send(post("/refuse", 1 << 20))
        c.send(b"x" * 5000)
        check(c.head().startswith("HTTP/1.1 413 "), "no 413")
        c.ends()
        # A length over the server's maximum (1 MiB) is refused by the server, with its rule, before a `100`.
        d = server.client()
        d.send(post("/upload", (1 << 20) + 1, "Expect: 100-continue\r\n"))
        head, _ = d.response()
        check(head.startswith("HTTP/1.1 413 ") and "X-Rule: body.too-large" in head, head)
        check("100 Continue" not in head, head)
        check(d.ends(), "the connection did not end")
        # Another expectation: 417.
        e = server.client()
        e.send(post("/upload", 10, "Expect: gimme\r\n"))
        head, _ = e.response()
        check(head.startswith("HTTP/1.1 417 ") and "X-Rule: expect.unsupported" in head, head)
        # Bad chunking in the middle of an upload: 400 with the rule, and the server serves the next client.
        for wire, rule in [(b"zz\r\n", "body.chunk-size"), (b"5\r\nhelloXX", "body.chunk-framing"), (b"0\r\nT: v\nU\r\n\r\n", "body.trailer"), (b"fffffff\r\n", "body.too-large")]:
            f = server.client()
            f.send(CHUNKED_POST)
            f.send(wire)
            head, _ = f.response()
            check("X-Rule: " + rule in head, "%s: %s" % (rule, head))
            check(f.ends(), rule + ": the connection did not end")
        # Answered before the body is read, and the client still sending: the answer comes, and the connection closes.
        g = server.client()
        g.send(post("/early", 1 << 20))
        g.send(b"y" * 10_000)
        same(g.response()[1], "early", "early answer")
        check(g.ends(), "early: the connection did not end")
        h = server.client()
        h.send(WHOLE)
        same(h.response()[1], "n=0 c=0", "the next client")


def stalled_clients_are_timed_out():
    with upload(4096, 1 << 20, 600, 600) as server:
        # A head that never ends, and one that is trickled; a body that stops; all at once.
        head = server.client()
        head.send(b"POST /upload HTTP/1.1\r\nHo")
        trickle = server.client()
        stalled = server.client()
        stalled.send(post("/upload", 100_000))
        stalled.send(b"z" * 1000)
        started = time.time()
        for byte in b"GET /t HTTP/1.1\r\nHost: t\r\n"[:10]:
            try:
                trickle.send(bytes([byte]))
            except OSError:
                pass
            # The others are served throughout, whatever these three are doing.
            t = time.time()
            ok = server.client()
            ok.send(WHOLE)
            same(ok.response()[1], "n=0 c=0", "served while others stall")
            check(time.time() - t < 0.4, lambda: "a client waited %.2f s" % (time.time() - t))
            time.sleep(0.1)
        for name, c, rule in [("head", head, "timeout.head"), ("trickle", trickle, "timeout.head"), ("body", stalled, "timeout.body")]:
            c.sock.settimeout(5)
            h, _ = c.response()
            check(h.startswith("HTTP/1.1 408 ") and "X-Rule: " + rule in h, "%s: %s" % (name, h))
            check(c.ends(), name + ": the connection did not end")
        # Within a sane time of the deadline (600 ms) and not before it.
        spent = time.time() - started
        check(0.6 < spent < 5, lambda: "the deadline passed after %.2f s" % spent)


def a_slow_application_stops_the_reading():
    with upload(4096, 1 << 30, 5000, 5000) as server:
        # `/slow` takes 50 bytes a turn of 20 ms: the kernel's buffers fill and the client's writes block. What the server buffers is
        # 4096 bytes, so the client got no more than the sockets' own buffers in.
        s = server.client()
        s.send(post("/slow", 256 << 20))
        s.sock.setblocking(False)
        chunk = b"q" * 65536
        written = 0
        blocked_for = 0.0
        while blocked_for < 0.5 and written < (200 << 20):
            try:
                written += s.sock.send(chunk)
                blocked_for = 0.0
            except BlockingIOError:
                time.sleep(0.05)
                blocked_for += 0.05
        check(blocked_for >= 0.5, lambda: "never blocked: %d bytes" % written)
        check(written < (64 << 20), lambda: "%d bytes were taken by a server that takes 2,500 a second" % written)
        # The others are served meanwhile.
        ok = server.client()
        ok.send(WHOLE)
        same(ok.response()[1], "n=0 c=0", "served meanwhile")
        # The client leaves in the middle: nothing is left behind. Twenty more do the same, and the server serves after.
        s.close()
        for i in range(20):
            c = server.client()
            c.send(post("/upload", 1 << 20))
            c.send(body_of(10_000 + i, i))
            c.close()
        ok = server.client()
        ok.send(post("/upload", 5) + b"hello")
        same(ok.response()[1], tally(b"hello"), "after the leavers")


def a_refusal_found_while_reading_is_sent_even_when_next_is_slow():
    # The program visits connections every tenth turn, so a fault `wait` finds waits through turns in which the client sends more.
    with upload(4096, 1 << 20, 2000, 2000, 1) as server:
        for wire, rule in [(b"zz\r\n", "body.chunk-size"), (b"5\r\nhelloXX", "body.chunk-framing")]:
            c = server.client()
            c.send(CHUNKED_POST)
            time.sleep(0.1)
            c.send(wire)
            for _ in range(20):
                # More bytes, while the program is not looking.
                try:
                    c.send(b"0123456789")
                except OSError:
                    break
                time.sleep(0.003)
            head, _ = c.response()
            check(head.startswith("HTTP/1.1 400 ") and "X-Rule: " + rule in head, "%s: %s" % (rule, head))
            check(c.ends(), rule + ": the connection did not end")
        ok = server.client()
        ok.send(post("/upload", 5) + b"hello")
        same(ok.response()[1], tally(b"hello"), "the next client")


def a_connection_that_reads_never_writes_into_its_neighbours_buffer():
    # Connection F (slot 0) uploads in small writes; connection S (slot 1) is slow and has a full buffer of untaken bytes. Whatever F's reads do
    # to the room `body_take` gave back, S's body arrives whole.
    with upload(4096, 64 << 20, 5000, 5000) as server:
        f = server.client()
        f.send(post("/upload", 300_000))
        time.sleep(0.1)
        s = server.client()
        slow = body_of(6000, 99)
        s.send(post("/slow", len(slow)))
        s.send(slow)
        time.sleep(0.5)
        fast = body_of(300_000, 7)
        f.pieces(fast, 1000, 0.001)
        same(f.response()[1], tally(fast), "the fast upload")
        same(s.response()[1], tally(slow), "the slow upload")


run([
    ("uploads arrive byte for byte with 100-continue, pipelining and keep-alive", arrive_byte_for_byte),
    ("a refusal before the body sends no 100 and the faults are refused with their rules", refusals_before_the_body),
    ("a client that stalls in a head or a body is timed out and the others are served", stalled_clients_are_timed_out),
    ("a body the application takes slowly stops being read and a client that leaves costs nothing", a_slow_application_stops_the_reading),
    ("a refusal found while reading is sent even when the application is slow to call next", a_refusal_found_while_reading_is_sent_even_when_next_is_slow),
    ("a connection that reads never writes into its neighbours buffer", a_connection_that_reads_never_writes_into_its_neighbours_buffer),
])
