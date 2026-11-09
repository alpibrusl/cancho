# http_server_stream.py -- `packages/http-server`'s `stream` on a socket server (docs/http-server.md section 14), moved from
# crates/cancho/tests/conformance/http_server_stream.rs (docs/gate.md, G4 wave 3). The program tests/programs/server_stream.cho holds
# `GET /stream` and sends it one event every 100 ms, chunked, then ends it; `GET /flood` is offered 4,007-byte chunks until the connection's
# answer room will not take one whole. The server is built from the package's source (HTTP_SERVER_SOURCE names another file, as
# scripts/http_server_stream_mutants.py does); the store is checked in `stream_through_the_store`.
#     python3 tests/commands/http_server_stream.py CANCHO SCRATCH     (from the root of the repository)
import os
import sys
import time

sys.path.insert(0, "tests/support")
from sk import build, check, fetch_dependencies, run, same, Server

cancho, scratch = sys.argv[1], sys.argv[2]
source = os.environ.get("HTTP_SERVER_SOURCE", "packages/http-server/server.cho")
exe = build(cancho, scratch, "server_stream", ["tests/programs/server_stream.cho", source])

HEAD = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n"
STREAM = b"GET /stream HTTP/1.1\r\nHost: t\r\n\r\n"


def stream_of_five(text):
    check(text.startswith(HEAD), lambda: "head: %r" % text[:200])
    want = HEAD + "".join("9\r\ndata: %d\n\n\r\n" % n for n in range(1, 6)) + "0\r\n\r\n"
    same(text, want, "the stream")


def events_arrive_while_the_stream_is_open():
    with Server(exe) as server:
        a = server.client()
        a.send(STREAM)
        began = time.time()
        # Another connection is answered while the stream is open.
        time.sleep(0.15)
        same(server.get("/plain"), "/plain", "another connection")
        got, marks = a.read_until(b"0\r\n\r\n", 10)
        stream_of_five(got.decode("latin-1"))
        # The events were not all written at the end: the reads that carried them are spread over time (five events 100 ms apart).
        first, last = marks[0][0], marks[-1][0]
        check(last - first >= 0.25 and len(marks) >= 3, lambda: "the stream arrived in one piece: %d reads over %.2f s" % (len(marks), last - first))
        check(last - began < 5, "the stream was slow")
        # The ticket of the stream that is over takes nothing (the request is not held any more), and nothing was written to its connection.
        same(server.get("/stale"), "-1", "stale ticket")
        same(server.get("/spare"), "0", "spare ticket")
        # The response is over and the connection is still good: the next request on it is answered.
        a.send(b"GET /again HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n")
        rest = a.read_to_end().decode("latin-1")
        check(rest.endswith("/again"), repr(rest))


def a_client_that_leaves_ends_its_stream():
    with Server(exe) as server:
        a = server.client()
        a.send(STREAM)
        got, _ = a.read_until(b"data: 1", 5)
        check(b"data: 1" in got, "no first event")
        a.close()
        time.sleep(0.9)
        # `stream` answered -1 for the ticket of the connection that left (once), and the loop answers others.
        same(server.get("/gone"), "1", "gone")
        same(server.get("/plain"), "/plain", "plain")
        # A second stream still works afterwards.
        b = server.client()
        b.send(STREAM)
        got, _ = b.read_until(b"0\r\n\r\n", 10)
        stream_of_five(got.decode("latin-1"))


def a_ticket_of_a_connection_that_left_does_not_reach_the_next():
    with Server(exe) as server:
        a = server.client()
        a.send(STREAM)
        got, _ = a.read_until(b"data: 1", 5)
        check(b"data: 1" in got, "no first event")
        a.close()
        time.sleep(0.5)
        same(server.get("/gone"), "1", "gone")
        # Another stream, in the slot the first one left; while it runs, the old ticket is offered a byte.
        b = server.client()
        b.send(STREAM)
        time.sleep(0.15)
        same(server.get("/stale"), "-1", "stale")
        got, _ = b.read_until(b"0\r\n\r\n", 10)
        stream_of_five(got.decode("latin-1"))


def a_request_refused_while_it_streams_is_given_no_more_bytes():
    # The program streams to a request whose declared body never arrives, with a stall timer of 250 ms: the request is refused at 250 ms, and an event
    # offered after that must not be added to the answer that is about to be replaced by the refusal and the end of the connection.
    with Server(exe, ["250"]) as server:
        a = server.client()
        a.send(b"GET /stream HTTP/1.1\r\nHost: t\r\nContent-Length: 10\r\n\r\n")
        began = time.time()
        got, _ = a.read_until(b"0\r\n\r\n", 3)
        text = got.decode("latin-1")
        check(time.time() - began < 3, text)
        events = text.count("data: ")
        check(1 <= events <= 3, "%d events before the refusal: %r" % (events, text))
        check("0\r\n\r\n" not in text, "the stream was ended properly: %r" % text)
        time.sleep(0.3)
        same(server.get("/gone"), "1", "gone")


def a_reader_that_never_reads_is_never_given_half_a_message():
    with Server(exe) as server:
        a = server.client()
        a.send(b"GET /flood HTTP/1.1\r\nHost: t\r\n\r\n")
        # `a` reads nothing. The program offers a 4,007-byte chunk every turn while the room takes it whole; the first turn it does not, the stream is ended.
        began = time.time()
        while True:
            stats = server.get("/stats")
            if "refused=0" not in stats:
                break
            check(time.time() - began < 30, "the stream was never ended: " + stats)
            time.sleep(0.1)
        taken = int(stats[len("taken="):].split(" ")[0])
        # The program offered the chunk anyway, as an application that ignores `space` would: it took what the room held, less than a chunk.
        blind = int(stats.rsplit("blind=", 1)[1])
        check(blind < 4007, "a refused offer took %d bytes" % blind)
        same(stats, "taken=%d refused=1 blind=%d" % (taken, blind), "the stats")
        check(taken % 4007 == 0, "a message was cut: %d bytes taken" % taken)
        check(taken > 0, "nothing was taken")
        # What the reader gets when it finally reads is the head and whole chunks, nothing cut.
        a.sock.settimeout(0.2)
        got = b""
        end = time.time() + 20
        closed = False
        while not closed and time.time() < end:
            try:
                more = a.sock.recv(65536)
            except TimeoutError:
                continue
            except OSError:
                break
            if not more:
                closed = True
            got += more
        # The server ended the connection of the reader it could not keep up with (`answer` of the end of the body did not fit the full room).
        check(closed, "the connection of a reader that fell behind is still open")
        text = got[len(HEAD):]
        # What arrived is the stream from its start, whole chunks, and at most the last one cut by the end of the connection: nothing else.
        chunk = b"fa0\r\n" + b"x" * 4000 + b"\r\n"
        whole = len(text) // 4007
        for i in range(whole):
            check(text[i * 4007:(i + 1) * 4007] == chunk, "chunk %d is not whole" % i)
        tail = text[whole * 4007:]
        check(tail == chunk[:len(tail)], "the tail is not the start of a chunk")
        # What was waiting in the answer room when the connection was ended (at most its size, 65,536 bytes) never reached the reader.
        lost = taken + blind - len(text)
        check(0 <= lost <= 65536, "%d bytes were lost" % lost)
        same(server.get("/plain"), "/plain", "plain")


def stream_through_the_store():
    paths = ["tests/programs/server_stream.cho"] + fetch_dependencies(cancho, scratch, "hs-stream-store", [("tests/programs/server_stream.lock", "packages/http-server/.cancho-vcs")])
    stored = build(cancho, scratch, "server_stream_store", paths)
    with Server(stored) as server:
        a = server.client()
        a.send(STREAM)
        got, _ = a.read_until(b"0\r\n\r\n", 10)
        stream_of_five(got.decode("latin-1"))


run([
    ("events arrive while the stream is open and others are served", events_arrive_while_the_stream_is_open),
    ("a client that leaves ends its stream and the loop goes on", a_client_that_leaves_ends_its_stream),
    ("a ticket of a connection that left does not reach the one that took its slot", a_ticket_of_a_connection_that_left_does_not_reach_the_next),
    ("a request refused while it streams is given no more bytes", a_request_refused_while_it_streams_is_given_no_more_bytes),
    ("a reader that never reads is never given half a message", a_reader_that_never_reads_is_never_given_half_a_message),
    ("stream through the store", stream_through_the_store),
])
