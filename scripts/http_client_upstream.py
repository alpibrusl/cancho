#!/usr/bin/env python3
"""An upstream for `scripts/http_client_test.py`: an HTTP/1.1 server written on raw sockets, so that every byte it sends is
decided here, not by a library. Plain, or TLS 1.3 with the test identity of `tests/vectors/tls/echo/`.

    python3 scripts/http_client_upstream.py [--tls] [--port N]      # runs until killed; prints `listening <port>`

It is what the client (`packages/http-client`, `docs/http-client.md`) is tested against, and it counts: `connections` accepted and
`requests` read, so a test can say that a hundred requests used one connection. A request path says what to do:

    GET  /len/N            200, `Content-Length: N`, N bytes of a..z repeating
    GET  /chunked/N?p=P    200, chunked in pieces of P bytes (1000), an extension on the first chunk and a trailer after the last
    GET  /close/N          200, no length: N bytes, then the connection ends
    GET  /closeabrupt/N    as /close/N, and over TLS the socket ends with no close_notify
    GET  /http10/N         `HTTP/1.0 200`, `Content-Length: N`, then the connection ends
    GET  /connclose/N      200, `Content-Length: N`, `Connection: close`, then the connection ends
    GET  /nobody           204
    HEAD /len/N            the head of `/len/N` and no body
    GET  /interim/N        `102`, `103`, then `/len/N`
    GET  /slow/N?ms=M&s=S  `/len/N` in pieces of S bytes (1) M milliseconds (10) apart
    GET  /flaky            200 `ok`, then the connection ends with no warning: the next request on it meets a dead connection
    GET  /dropsecond       200 `ok` to the first request on a connection; the second is read and the connection ended with no answer
    GET  /idle1            200 `ok`, and the connection is closed after 1 s idle (a server with a short keep-alive)
    GET  /hang             nothing, ever
    GET  /stall/N          the head and half of N bytes, then nothing
    GET  /cut/N            the head of N bytes and half of them, then the connection ends
    GET  /big/N            `/len/N`, streamed
    GET  /bad/<what>       a response that must be refused: two-lengths, dup-length, chunk, fold, status, version, upgrade, te, longhead
    POST /sink             reads the body (a length, or chunked), answers `bytes=<n> sha256=<hex>`
    POST /early413         answers 413 at once without reading the body, reads a little, ends
    POST /continue         `Expect: 100-continue` is answered `100`, the body read, `/sink`'s answer
    POST /nocontinue       the same, but no `100`: the client sends its body after its wait
    POST /reject-expect    417 at once, no `100`, the body not read
    POST /stalled-reader   reads nothing of the body and answers nothing
"""
import hashlib
import os
import socket
import ssl
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
VECTORS = os.path.join(os.path.dirname(HERE), "tests", "vectors", "tls", "echo")


def pattern(n, start=0):
    unit = bytes(97 + i % 26 for i in range(26))
    off = start % 26
    return (unit[off:] + unit * (n // 26 + 1))[:n]


class Upstream:
    def __init__(self, tls=False, port=0, host="127.0.0.1"):
        self.tls = tls
        self.sock = socket.socket()
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind((host, port))
        self.sock.listen(512)
        self.port = self.sock.getsockname()[1]
        self.connections = 0
        self.requests = 0
        self.lock = threading.Lock()
        self.log = []
        self.abrupt = set()
        self.ctx = None
        if tls:
            self.ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            self.ctx.minimum_version = ssl.TLSVersion.TLSv1_3
            self.ctx.load_cert_chain(os.path.join(VECTORS, "first", "chain.pem"), os.path.join(VECTORS, "first", "key.pem"))
        threading.Thread(target=self.accept_loop, daemon=True).start()

    def counts(self):
        with self.lock:
            return self.connections, self.requests

    def accept_loop(self):
        while True:
            try:
                conn, _ = self.sock.accept()
            except OSError:
                return
            with self.lock:
                self.connections += 1
            threading.Thread(target=self.serve, args=(conn,), daemon=True).start()

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass

    # ---- one connection ----

    def serve(self, conn):
        try:
            if self.ctx:
                conn = self.ctx.wrap_socket(conn, server_side=True)
            conn.settimeout(30)
            buf = b""
            index = 0
            while True:
                while b"\r\n\r\n" not in buf:
                    data = conn.recv(65536)
                    if not data:
                        return
                    buf += data
                head, _, buf = buf.partition(b"\r\n\r\n")
                lines = head.decode("latin-1").split("\r\n")
                method, target, _version = lines[0].split(" ", 2)
                headers = {}
                for l in lines[1:]:
                    k, _, v = l.partition(":")
                    headers[k.strip().lower()] = v.strip()
                with self.lock:
                    self.requests += 1
                index += 1
                buf, keep = self.handle(conn, method, target, headers, buf, index)
                if not keep:
                    return
        except (OSError, ssl.SSLError, ValueError):
            return
        finally:
            # A TLS connection that is ending says so (close_notify), unless the path asked for it to end abruptly: an end of
            # the socket without it may be a truncation, and the client must not take it for the end of a body.
            if isinstance(conn, ssl.SSLSocket) and id(conn) not in self.abrupt:
                try:
                    conn = conn.unwrap()
                except (OSError, ssl.SSLError, ValueError):
                    pass
            try:
                conn.close()
            except OSError:
                pass

    def read_body(self, conn, headers, buf):
        """The request body, a length or chunked: (the body, what was read past it)."""
        if "content-length" in headers:
            n = int(headers["content-length"])
            while len(buf) < n:
                data = conn.recv(1 << 20)
                if not data:
                    break
                buf += data
            return buf[:n], buf[n:]
        if headers.get("transfer-encoding", "").lower() == "chunked":
            body = b""
            while True:
                while b"\r\n" not in buf:
                    data = conn.recv(1 << 20)
                    if not data:
                        return body, b""
                    buf += data
                line, _, buf = buf.partition(b"\r\n")
                size = int(line.split(b";")[0], 16)
                while len(buf) < size + 2:
                    data = conn.recv(1 << 20)
                    if not data:
                        return body, b""
                    buf += data
                if size == 0:
                    # Trailers (none are sent by the client) up to the blank line.
                    while not buf.startswith(b"\r\n"):
                        data = conn.recv(1 << 20)
                        if not data:
                            break
                        buf += data
                    return body, buf[2:]
                body += buf[:size]
                buf = buf[size + 2:]
        return b"", buf

    def head(self, status, reason, headers):
        out = f"HTTP/1.1 {status} {reason}\r\n".encode()
        for k, v in headers:
            out += f"{k}: {v}\r\n".encode()
        return out + b"\r\n"

    def handle(self, conn, method, target, headers, buf, index):
        """Answer one request: (what is left in the buffer, whether the connection stays)."""
        path, _, query = target.partition("?")
        params = dict(p.split("=") for p in query.split("&") if "=" in p)
        parts = path.strip("/").split("/")
        what = parts[0]
        n = int(parts[1]) if len(parts) > 1 and parts[1].isdigit() else 0
        send = conn.sendall
        if method == "POST":
            return self.post(conn, what, headers, buf)
        if what == "len" or what == "big":
            send(self.head(200, "OK", [("Content-Type", "text/plain"), ("Content-Length", n)]))
            if method != "HEAD":
                sent = 0
                while sent < n:
                    k = min(65536, n - sent)
                    send(pattern(k, sent))
                    sent += k
            return buf, True
        if what == "chunked":
            piece = int(params.get("p", 1000))
            send(self.head(200, "OK", [("Content-Type", "text/plain"), ("Transfer-Encoding", "chunked")]))
            sent = 0
            first = True
            while sent < n:
                k = min(piece, n - sent)
                ext = b";name=value" if first else b""
                send(b"%x%s\r\n" % (k, ext) + pattern(k, sent) + b"\r\n")
                sent += k
                first = False
            send(b"0\r\nX-Trailer: done\r\n\r\n")
            return buf, True
        if what == "close":
            send(self.head(200, "OK", [("Content-Type", "text/plain")]))
            send(pattern(n))
            return buf, False
        if what == "closeabrupt":
            # Over TLS: the body is cut by an end of the socket with no close_notify.
            self.abrupt.add(id(conn))
            send(self.head(200, "OK", [("Content-Type", "text/plain")]))
            send(pattern(n))
            return buf, False
        if what == "http10":
            send(b"HTTP/1.0 200 OK\r\nContent-Length: %d\r\n\r\n" % n + pattern(n))
            return buf, False
        if what == "connclose":
            send(self.head(200, "OK", [("Content-Length", n), ("Connection", "close")]) + pattern(n))
            return buf, False
        if what == "nobody":
            send(self.head(204, "No Content", []))
            return buf, True
        if what == "interim":
            send(b"HTTP/1.1 102 Processing\r\n\r\nHTTP/1.1 103 Early Hints\r\nLink: </x>; rel=preload\r\n\r\n")
            send(self.head(200, "OK", [("Content-Length", n)]) + pattern(n))
            return buf, True
        if what == "slow":
            ms = int(params.get("ms", 10)) / 1000
            step = int(params.get("s", 1))
            send(self.head(200, "OK", [("Content-Length", n)]))
            sent = 0
            while sent < n:
                k = min(step, n - sent)
                send(pattern(k, sent))
                sent += k
                time.sleep(ms)
            return buf, True
        if what == "flaky":
            send(self.head(200, "OK", [("Content-Length", 2)]) + b"ok")
            return buf, False
        if what == "dropsecond":
            # The second request on a connection is read and the connection ended with no answer: a server that closed at the
            # very moment the request arrived.
            if index == 2:
                return buf, False
            send(self.head(200, "OK", [("Content-Length", 2)]) + b"ok")
            return buf, True
        if what == "idle1":
            send(self.head(200, "OK", [("Content-Length", 2)]) + b"ok")
            conn.settimeout(1)
            try:
                if conn.recv(1) == b"":
                    return buf, False
            except (socket.timeout, TimeoutError):
                pass
            return buf, False
        if what == "hang":
            time.sleep(60)
            return buf, False
        if what == "stall":
            send(self.head(200, "OK", [("Content-Length", n)]) + pattern(n // 2))
            time.sleep(60)
            return buf, False
        if what == "cut":
            send(self.head(200, "OK", [("Content-Length", n)]) + pattern(n // 2))
            return buf, False
        if what == "bad":
            kind = parts[1] if len(parts) > 1 else ""
            bad = {
                "two-lengths": b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\nok",
                "dup-length": b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\nok",
                "chunk": b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nok\r\n0\r\n\r\n",
                "fold": b"HTTP/1.1 200 OK\r\nX-A: b\r\n c\r\nContent-Length: 0\r\n\r\n",
                "status": b"HTTP/1.1 20 OK\r\nContent-Length: 0\r\n\r\n",
                "version": b"HTTP/2.0 200 OK\r\nContent-Length: 0\r\n\r\n",
                "upgrade": b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n",
                "te": b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n",
                "longhead": b"HTTP/1.1 200 OK\r\n" + b"X-Pad: " + b"a" * 70000 + b"\r\nContent-Length: 0\r\n\r\n",
                "garbage": b"\x16\x03\x03\x00\x02\x02\x28",
            }[kind]
            send(bad)
            return buf, False
        send(self.head(404, "Not Found", [("Content-Length", 0)]))
        return buf, True

    def post(self, conn, what, headers, buf):
        send = conn.sendall
        if what == "early413":
            send(self.head(413, "Payload Too Large", [("Content-Length", 4), ("Connection", "close")]) + b"big!")
            conn.settimeout(2)
            try:
                conn.recv(65536)
            except (socket.timeout, TimeoutError, OSError):
                pass
            return buf, False
        if what == "stalled-reader":
            # Reads nothing of the body, and answers nothing: a peer whose receive window fills.
            time.sleep(60)
            return buf, False
        if what == "reject-expect":
            send(self.head(417, "Expectation Failed", [("Content-Length", 0)]))
            return buf, True
        if what == "continue" and headers.get("expect", "").lower() == "100-continue":
            send(b"HTTP/1.1 100 Continue\r\n\r\n")
        body, rest = self.read_body(conn, headers, buf)
        answer = b"bytes=%d sha256=%s" % (len(body), hashlib.sha256(body).hexdigest().encode())
        send(self.head(200, "OK", [("Content-Length", len(answer))]) + answer)
        return rest, True


def main():
    tls = "--tls" in sys.argv
    port = 0
    if "--port" in sys.argv:
        port = int(sys.argv[sys.argv.index("--port") + 1])
    up = Upstream(tls=tls, port=port)
    print(f"listening {up.port}", flush=True)
    while True:
        time.sleep(3600)


if __name__ == "__main__":
    main()
