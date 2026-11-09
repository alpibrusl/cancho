# api.py -- examples/api (docs/server.md): the server loop over real sockets, moved from crates/cancho/tests/conformance/api.rs (docs/gate.md,
# G4 wave 3). Every case starts the compiled server on a port of its own, talks to it from this process and kills it. The things worth testing
# about a server are not the routes (those are std.route's tests) but what a loop around them can get wrong: a request split across segments,
# several in one, a client that goes silent, a client that vanishes, and a refusal that must close the connection.
#     python3 tests/commands/api.py CANCHO SCRATCH     (from the root of the repository)
import socket
import subprocess
import sys
import time

sys.path.insert(0, "tests/support")
from sk import Client, Server, build, check, concurrently, fetch_dependencies, free_port, run, same

cancho, scratch = sys.argv[1], sys.argv[2]
# examples/api/api.cho and the one file `http.server` fetches to (packages/http-server/), fetched fresh: `vcs fetch` re-verifies the pin.
paths = ["examples/api/api.cho"] + fetch_dependencies(cancho, scratch, "api", [("examples/api/server.lock", "packages/http-server/.cancho-vcs")])
exe = build(cancho, scratch, "api", paths)


def start(extra=()):
    # The server writes `listening on <port>` to standard error after its `listen` succeeded; a server that found the port busy exits without it.
    return Server(exe, extra, announce="listening on %d")


def get(path):
    return "GET %s HTTP/1.1\r\nHost: t\r\n\r\n" % path


def post(path, body):
    return "POST %s HTTP/1.1\r\nHost: t\r\nContent-Length: %d\r\n\r\n%s" % (path, len(body), body)


def chunked(path, pieces):
    out = "POST %s HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n" % path
    for piece in pieces:
        out += "%x\r\n%s\r\n" % (len(piece), piece)
    return out + "0\r\n\r\n"


def is_(r, status, body, why=""):
    same((r[0], r[2]), (status, body), why)


def the_routes_answer():
    with start() as server:
        s = server.client()
        r = s.ask(get("/health"))
        is_(r, 200, '{"ok":true}')
        check("Content-Type: application/json" in r[1], r[1])
        check("Connection: keep-alive" in r[1], r[1])
        is_(s.ask(get("/users/42")), 200, '{"id":42,"name":"user-42"}')
        is_(s.ask(get("/users/abc")), 400, '{"error":"id must be a number"}')
        same(s.ask(get("/users/"))[0], 404, "/users/")
        # (Bodies are read as latin-1, so the two bytes of the UTF-8 "e acute" are the two characters below.)
        is_(s.ask(get("/search?q=a%20b+c%C3%A9")), 200, '{"q":"a b cÃ©","length":7}')
        is_(s.ask(get("/search")), 400, '{"error":"q is required"}')
        same(s.ask(get("/search?q=%zz"))[0], 400, "bad escape")
        is_(s.ask(post("/add", '{"a": 40, "b": 2}')), 200, '{"sum":42}')
        is_(s.ask(post("/add", '{"a": 1.5, "b": 2}')), 422, '{"error":"a and b must be integers"}')
        same(s.ask(post("/add", '{"a": 1'))[0], 422, "short body")
        r = s.ask(post("/health", ""))
        is_(r, 405, '{"error":"method not allowed"}')
        # RFC 9110 section 15.5.6: a 405 says what the path does allow, and a path with two methods says both.
        check("\r\nAllow: GET\r\n" in r[1] or r[1].endswith("Allow: GET"), r[1])
        r = s.ask(get("/add"))
        same(r[0], 405, "GET /add")
        check("Allow: POST" in r[1], r[1])
        # A 404 has none: no route has the path.
        r = s.ask(get("/nothing"))
        check("Allow:" not in r[1], r[1])
        is_(s.ask(get("/nothing")), 404, '{"error":"not found"}')


def one_connection_serves_many_requests_and_pipelined_ones_in_order():
    with start() as server:
        s = server.client()
        for i in range(60):
            same(s.ask(get("/users/%d" % i))[2], '{"id":%d,"name":"user-%d"}' % (i, i), "request %d" % i)
        # Thirty requests in one write: thirty answers, in order.
        s.send("".join(get("/users/%d" % (1000 + i)) for i in range(30)).encode())
        for i in range(30):
            r = s.reply()
            check(r is not None, "an answer")
            same(r[2], '{"id":%d,"name":"user-%d"}' % (1000 + i, 1000 + i), "pipelined %d" % i)


def a_request_split_across_segments_is_answered_once_whole():
    with start() as server:
        s = server.client()
        s.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        body = '{"a": 5, "b": 6}'
        request = post("/add", body).encode()
        # Cut it inside the request line, inside a header, at the blank line, and inside the body: each piece is its own segment.
        cuts = [5, 22, len(request) - len(body) - 2, len(request) - 6]
        at = 0
        for cut in cuts:
            s.send(request[at:cut])
            time.sleep(0.04)
            at = cut
        s.send(request[at:])
        r = s.reply()
        check(r is not None, "an answer")
        is_(r, 200, '{"sum":11}')
        # And the connection is still good.
        same(s.ask(get("/health"))[0], 200, "after the split")


def the_start_of_the_next_request_after_a_whole_one_is_kept_and_completed():
    # One read holding a whole request and the front of the next: the first is answered, the rest is moved to the start of the buffer, and the
    # other half, arriving later, completes it. Without that move the second request is parsed as its own tail.
    with start() as server:
        s = server.client()
        s.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        for n in range(20):
            first = get("/users/%d" % n)
            second = post("/add", '{"a": %d, "b": 100}' % n).encode()
            cut = 11 + n % 7
            s.send(first.encode() + second[:cut])
            r = s.reply()
            check(r is not None, "the first answer")
            same(r[2], '{"id":%d,"name":"user-%d"}' % (n, n), "round %d first" % n)
            time.sleep(0.03)
            s.send(second[cut:])
            r = s.reply()
            check(r is not None, "the second answer")
            same(r[2], '{"sum":%d}' % (n + 100), "round %d second" % n)


def closing_a_connection_moves_the_last_one_with_its_buffered_bytes():
    # Connections are kept in a dense array and a closed one is replaced by the last. The last may be holding the front of a request; it must
    # still be holding it afterwards, and still be the same client's.
    with start() as server:
        for n in range(30):
            early = server.client()
            late = server.client()
            late.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            request = get("/users/%d" % n).encode()
            late.send(request[:12])
            time.sleep(0.02)
            # `early` goes: the server moves `late` into its slot.
            early.close()
            time.sleep(0.03)
            late.send(request[12:])
            r = late.reply()
            check(r is not None, "an answer")
            same(r[2], '{"id":%d,"name":"user-%d"}' % (n, n), "round %d" % n)


def what_cannot_be_trusted_is_refused_and_the_connection_closes():
    cases = [
        ("both lengths", "POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 4\r\nTransfer-Encoding: chunked\r\n\r\n{}{}", 400),
        ("line folding", "GET /health HTTP/1.1\r\nHost: t\r\nX: a\r\n b\r\n\r\n", 400),
        ("space before colon", "GET /health HTTP/1.1\r\nHost : t\r\n\r\n", 400),
        ("no host", "GET /health HTTP/1.1\r\n\r\n", 400),
        ("garbage method", "G@T /health HTTP/1.1\r\nHost: t\r\n\r\n", 400),
        ("version", "GET /health HTTP/2.0\r\nHost: t\r\n\r\n", 400),
        ("two lengths", "POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 2\r\nContent-Length: 3\r\n\r\n{}", 400),
        ("chunk size not hex", "POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n{}\r\n0\r\n\r\n", 400),
        ("chunk extension", "POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n2;x=1\r\n{}\r\n0\r\n\r\n", 400),
        ("chunked trailer", "POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\nX: 1\r\n\r\n", 400),
        ("chunk framing", "POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n2\n{}\r\n0\r\n\r\n", 400),
        ("other encoding", "POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: gzip, chunked\r\n\r\n0\r\n\r\n", 400),
        ("chunked body too large", "POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n%x\r\n" % 100_000, 413),
        ("body too large", "POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 100000\r\n\r\n", 413),
        ("head too large", "GET /health HTTP/1.1\r\nHost: t\r\nX: %s\r\n\r\n" % ("a" * 20_000), 431),
    ]
    with start() as server:
        for name, request, status in cases:
            s = server.client()
            # The big ones may be refused before the whole request is written.
            try:
                s.send(request.encode())
            except OSError:
                pass
            r = s.reply()
            check(r is not None, name + ": no answer")
            same(r[0], status, "%s: %s" % (name, r[2]))
            check("Connection: close" in r[1], name)
            check(r[2].startswith('{"error":'), name)
            # Closed: the next read is the end of the stream (or a reset, if the server closed with the rest of a large request unread).
            check(s.reply() is None, name + ": still open")
        # None of that hurt the server.
        same(server.client().ask(get("/health"))[0], 200, "after the refusals")


def connection_close_is_honoured():
    with start() as server:
        s = server.client()
        r = s.ask("GET /health HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n")
        same(r[0], 200, "close")
        check("Connection: close" in r[1], r[1])
        check(s.reply() is None, "still open")
        # HTTP/1.0 closes unless it asks to stay.
        s = server.client()
        s.send(b"GET /health HTTP/1.0\r\n\r\n")
        r = s.reply()
        check(r is not None, "an answer")
        check("Connection: close" in r[1], r[1])
        check(s.reply() is None, "HTTP/1.0 stayed open")


def a_silent_client_does_not_hold_up_the_others():
    with start() as server:
        # Fifty connections that open and say nothing, one that sends half a head.
        idle = [server.client() for _ in range(50)]
        half = server.client()
        half.send(b"GET /health HTT")
        # Everyone else is served promptly all the while.
        started = time.time()
        for i in range(40):
            same(server.client().ask(get("/users/%d" % i))[0], 200, "request %d" % i)
        check(time.time() - started < 3, "the loop waited on a silent client")
        for c in idle:
            c.close()
        half.close()


def many_clients_at_once_each_get_their_own_answers():
    with start() as server:
        def client(t):
            def go():
                s = server.client(20)
                for i in range(40):
                    n = t * 1000 + i
                    if i % 4 == 3:
                        r = s.ask(post("/add", '{"a": %d, "b": 1}' % n))
                        want = '{"sum":%d}' % (n + 1)
                    else:
                        r = s.ask(get("/users/%d" % n))
                        want = '{"id":%d,"name":"user-%d"}' % (n, n)
                    same(r[2], want, "client %d request %d" % (t, i))
            return go
        concurrently([client(t) for t in range(64)])


def clients_that_vanish_do_not_hurt_the_server():
    with start() as server:
        for n in range(150):
            s = server.client()
            try:
                if n % 3 == 0:
                    # Ask and leave without reading: the answer is written to a closed socket, which would be SIGPIPE.
                    s.send(get("/health").encode())
                elif n % 3 == 1:
                    # Leave in the middle of a head.
                    s.send(b"GET /hea")
                else:
                    # Leave in the middle of a body.
                    s.send(b'POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 50\r\n\r\n{"a"')
            except OSError:
                pass
            s.close()
        time.sleep(0.1)
        same(server.client().ask(get("/health"))[0], 200, "after the vanishing")


def an_idle_connection_is_closed_after_the_timeout():
    # `api <port> reuseport 1`: one second of silence.
    with start(["0", "1"]) as server:
        quiet = server.client()
        busy = server.client()
        started = time.time()
        # The busy one keeps talking and is never closed.
        for _ in range(5):
            same(busy.ask(get("/health"))[0], 200, "busy")
            time.sleep(0.6)
        # The quiet one has been closed in the meantime.
        check(quiet.reply() is None, "an idle connection stayed open")
        check(time.time() - started < 15, "slow")
        same(busy.ask(get("/health"))[0], 200, "busy after")


# ---- writes that do not block ----

def loopback_capacity():
    """How many bytes the kernel will queue on a loopback connection whose far end never reads: a writer that is never refused until the buffers are
    full, and then refused. Linux holds a few megabytes; macOS may hold far more, and the cases that need a client to be *stalled* must send more than
    that, so they ask rather than assume."""
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    writer = socket.create_connection(listener.getsockname())
    reader, _ = listener.accept()
    writer.setblocking(False)
    chunk = bytes(65536)
    total = 0
    refused = 0
    # Refused twice in a row, 150 ms apart, means full: the first refusal can be the buffer waiting to grow.
    while refused < 2 and total < 1 << 30:
        try:
            total += writer.send(chunk)
            refused = 0
        except BlockingIOError:
            refused += 1
            time.sleep(0.15)
    writer.close()
    reader.close()
    listener.close()
    return total


def requests_to_stall(capacity):
    """How many 32 KiB answers it takes to be sure of filling `capacity` twice over, and at least a thousand."""
    return max(2 * capacity // 32768 + 200, 1000)


def blob_requests(sizes):
    return "".join(get("/blob/%d" % n) for n in sizes)


def a_client_that_stops_reading_stalls_only_itself():
    # Enough requests for 32 KiB each that the answers are more than twice what the kernel will hold for a client that reads none of it. A server
    # that *blocks* in `write` stops dead there; this one parks the rest of that connection's answers and serves everyone else.
    capacity = loopback_capacity()
    count = requests_to_stall(capacity)
    with start() as server:
        slow = server.client()
        slow.send(blob_requests([32768] * count).encode())
        time.sleep(0.5)
        started = time.time()
        for i in range(30):
            other = server.client()
            other.send(get("/users/%d" % i).encode())
            r = other.reply()
            check(r is not None, "request %d on another connection got no answer (%d requests)" % (i, count))
            same(r[0], 200, "other %d" % i)
        check(time.time() - started < 2, "the loop waited on a client that was not reading")
        # And the stalled client loses nothing: it now reads every answer, whole and in order.
        for i in range(count):
            r = slow.reply()
            check(r is not None, "answer %d of %d missing" % (i, count))
            same((r[0], len(r[2])), (200, 32768), "answer %d of %d" % (i, count))


def a_slow_reader_gets_every_byte_in_the_right_order():
    # Answers of a thousand different sizes, read a little at a time. Each time the kernel takes only part of one, the rest waits in the connection's
    # output buffer and is sent when `poll` says there is room: a byte dropped, repeated or reordered there changes a length. `1500` is the fourth
    # argument: no `send` is handed more than that, so every answer is sent in pieces and the rest of a partial one has to be resumed.
    with start(["0", "9", "1500"]) as server:
        sizes = [1 + (i * 7919) % 32768 for i in range(1000)]
        slow = server.client()
        slow.send(blob_requests(sizes).encode())
        for i, size in enumerate(sizes):
            r = slow.reply()
            check(r is not None, "answer %d missing" % i)
            same((r[0], len(r[2])), (200, size), "answer %d" % i)
            want = "".join(chr(97 + at % 26) for at in range(min(size, 52)))
            check(r[2][:52] == want and r[2] == (("abcdefghijklmnopqrstuvwxyz" * (size // 26 + 1))[:size]), "answer %d has the wrong bytes" % i)
            if i % 50 == 0:
                time.sleep(0.02)
                # Others are served in the middle of it.
                same(server.client().ask(get("/health"))[0], 200, "meanwhile")


def a_client_that_never_reads_is_closed_after_the_idle_timeout():
    # Two seconds without progress. The connection has output waiting and no POLLIN interest, so nothing it sends can keep it alive.
    capacity = loopback_capacity()
    count = requests_to_stall(capacity)
    with start(["0", "2"]) as server:
        s = server.client(20)
        s.send(blob_requests([32768] * count).encode())
        time.sleep(5)
        # Whatever the kernel was holding comes out, and then the connection ends -- by end of stream, or by a reset, which is what closing a
        # socket with unread input sends. It must not still be open.
        total = 0
        while True:
            try:
                more = s.sock.recv(65536)
            except (ConnectionResetError, BrokenPipeError):
                break
            except OSError as why:
                raise AssertionError("the connection was still open after the timeout (%d bytes read): %s" % (total, why))
            if not more:
                break
            total += len(more)
        check(total < count * 32768, "every answer (%d of them, %d bytes) was delivered to a client that was not reading" % (count, total))
        same(server.client().ask(get("/health"))[0], 200, "after")


def a_connection_that_is_to_close_closes_only_once_its_answer_has_gone():
    # `Connection: close` on a 20 KB answer, with `send` limited to 1500 bytes at a time: the answer goes out in pieces over several `poll` rounds,
    # and the connection must stay open until the last of them -- closing at the first partial send would cut the answer short.
    with start(["0", "30", "1500"]) as server:
        s = server.client()
        s.send(b"GET /blob/20000 HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n")
        r = s.reply()
        check(r is not None, "an answer")
        same((r[0], len(r[2])), (200, 20000), "the blob")
        check("Connection: close" in r[1], r[1])
        check(s.reply() is None, "the connection stayed open")
        # And so does a refusal that is bigger than one piece: 20,000 bytes of header.
        s = server.client()
        try:
            s.send(("GET /health HTTP/1.1\r\nHost: t\r\nX: %s\r\n\r\n" % ("a" * 20_000)).encode())
        except OSError:
            pass
        r = s.reply()
        check(r is not None, "a refusal")
        same(r[0], 431, "the refusal")


def the_server_announces_its_port_on_standard_error():
    # On the unbuffered stream: a piped standard output is held until the process ends, and a server that announces itself only then has not
    # announced itself. No send flag is announced any more -- there is none: `conn_write` picks the system's way of not waiting.
    port = free_port()
    child = subprocess.Popen([exe, str(port)], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    line = child.stderr.readline().decode().strip()
    child.kill()
    child.wait()
    child.stderr.close()
    same(line, "listening on %d" % port, "the first line")


def the_server_holds_no_foreign_authority():
    # The point of docs/native-sockets.md: the same server that reported `ffi("libc")` -- "may call anything in libc" -- now reports exactly what
    # it does. The port is an argument, so `net_in` names none; that is said rather than hidden.
    out = subprocess.run([cancho, "authority"] + paths + ["--std", "--output", "json"], capture_output=True, text=True)
    same(out.returncode, 0, out.stderr)
    check('"ffi"' not in out.stdout, "no foreign code anywhere:\n" + out.stdout)
    for label in ["net_in", "conn_accept", "conn_read", "conn_write", "poll", "clock", "heap"]:
        check('"name": "%s"' % label in out.stdout, "`%s` is reported:\n%s" % (label, out.stdout))


def a_chunked_body_is_decoded_and_the_handler_cannot_tell():
    with start() as server:
        s = server.client()
        # The same answer as a body with a length.
        is_(s.ask(chunked("/add", ['{"a": 40,', ' "b": 2}'])), 200, '{"sum":42}')
        # One chunk, and a body that splits mid-token across chunks.
        same(s.ask(chunked("/add", ['{"a": 1, "b": 2}']))[2], '{"sum":3}', "one chunk")
        same(s.ask(chunked("/add", ['{"a', '": 7, "b"', ': 8}']))[2], '{"sum":15}', "split mid-token")
        # An empty chunked body is an empty body, which `/add` refuses as it does any.
        same(s.ask(chunked("/add", []))[0], 422, "empty chunked body")
        # And the connection is still good for the next request, chunked or not.
        same(s.ask(get("/health"))[0], 200, "after")


def a_chunked_body_arriving_a_byte_at_a_time_is_answered_once_and_whole():
    with start() as server:
        s = server.client()
        s.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        for byte in chunked("/add", ['{"a": 20,', ' "b": 22}']).encode():
            s.send(bytes([byte]))
            time.sleep(0.001)
        r = s.reply()
        check(r is not None, "an answer")
        is_(r, 200, '{"sum":42}')
        # A pipelined pair: a chunked request and a plain one in one write.
        s.send((chunked("/add", ['{"a": 1, "b": 1}']) + get("/health")).encode())
        same(s.reply()[2], '{"sum":2}', "pipelined chunked")
        same(s.reply()[2], '{"ok":true}', "pipelined plain")


def padded():
    """A body of whitespace around a small object: valid JSON, 40,000 bytes."""
    return '{"a": 1, "b": 2}' + " " * 40_000


def a_body_larger_than_the_default_buffer_needs_the_buffer_argument():
    # By default a request must fit 16 KiB, and is refused at once if it cannot.
    with start() as small:
        r = small.client().ask(post("/add", padded()))
        same(r[0], 413, r[2])
    # With the sixth argument -- a 64 KiB buffer -- the same request is answered. (`no` is not `1` or `r`, so the port is not shared; 9 seconds
    # idle; 0, no write quantum.)
    with start(["no", "9", "0", "65536"]) as big:
        is_(big.client().ask(post("/add", padded())), 200, '{"sum":3}')
        # A chunked one too, across several chunks.
        body = padded()
        is_(big.client().ask(chunked("/add", [body[:20_000], body[20_000:]])), 200, '{"sum":3}')
        # Still refused past the bigger buffer.
        same(big.client().ask(post("/add", '{"a": 1, "b": 2}' + " " * 70_000))[0], 413, "past the bigger buffer")


def a_buffer_the_server_cannot_honour_is_refused_at_start():
    # Below a page, or above a megabyte: the server says nothing and exits 2.
    for size in ["100", "2097152"]:
        done = subprocess.run([exe, str(free_port()), "no", "9", "0", size], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        same(done.returncode, 2, "buffer " + size)


def a_port_taken_by_another_listener_is_not_mistaken_for_the_server():
    # Issue #675. Another test's listener takes the port a test found free, between the test finding it and the server listening on it. The server
    # then exits, finding the port busy; a connect to that port succeeds all the same, to the other listener. A start that trusted the connect
    # would talk to the wrong one. Here the other listener is held open, so the collision is certain rather than a matter of timing.
    other = socket.socket()
    other.bind(("0.0.0.0", 0))
    other.listen(8)
    taken = other.getsockname()[1]
    offered = [free_port(), taken]
    with Server(exe, announce="listening on %d", pick=offered.pop) as server:
        # The connect a start once trusted does succeed: the other listener takes it.
        socket.create_connection(("127.0.0.1", taken)).close()
        check(server.port != taken, "the server was taken to be where it is not")
        is_(server.client().ask(get("/health")), 200, '{"ok":true}')
    other.close()


run([
    ("the routes answer", the_routes_answer),
    ("one connection serves many requests and pipelined ones in order", one_connection_serves_many_requests_and_pipelined_ones_in_order),
    ("a request split across segments is answered once whole", a_request_split_across_segments_is_answered_once_whole),
    ("the start of the next request after a whole one is kept and completed", the_start_of_the_next_request_after_a_whole_one_is_kept_and_completed),
    ("closing a connection moves the last one with its buffered bytes", closing_a_connection_moves_the_last_one_with_its_buffered_bytes),
    ("what cannot be trusted is refused and the connection closes", what_cannot_be_trusted_is_refused_and_the_connection_closes),
    ("connection close is honoured", connection_close_is_honoured),
    ("a silent client does not hold up the others", a_silent_client_does_not_hold_up_the_others),
    ("many clients at once each get their own answers", many_clients_at_once_each_get_their_own_answers),
    ("clients that vanish do not hurt the server", clients_that_vanish_do_not_hurt_the_server),
    ("an idle connection is closed after the timeout", an_idle_connection_is_closed_after_the_timeout),
    ("a client that stops reading stalls only itself", a_client_that_stops_reading_stalls_only_itself),
    ("a slow reader gets every byte in the right order", a_slow_reader_gets_every_byte_in_the_right_order),
    ("a client that never reads is closed after the idle timeout", a_client_that_never_reads_is_closed_after_the_idle_timeout),
    ("a connection that is to close closes only once its answer has gone", a_connection_that_is_to_close_closes_only_once_its_answer_has_gone),
    ("the server announces its port on standard error", the_server_announces_its_port_on_standard_error),
    ("the server holds no foreign authority", the_server_holds_no_foreign_authority),
    ("a chunked body is decoded and the handler cannot tell", a_chunked_body_is_decoded_and_the_handler_cannot_tell),
    ("a chunked body arriving a byte at a time is answered once and whole", a_chunked_body_arriving_a_byte_at_a_time_is_answered_once_and_whole),
    ("a body larger than the default buffer needs the buffer argument", a_body_larger_than_the_default_buffer_needs_the_buffer_argument),
    ("a buffer the server cannot honour is refused at start", a_buffer_the_server_cannot_honour_is_refused_at_start),
    ("a port taken by another listener is not mistaken for the server", a_port_taken_by_another_listener_is_not_mistaken_for_the_server),
])
