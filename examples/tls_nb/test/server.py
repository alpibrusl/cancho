#!/usr/bin/env python3
"""A TLS HTTP/1.1 test receiver for the tls_nb client: answers every POST with a fixed-size 200.

    server.py --port 9443 --cert certs/good_ec.pem --key certs/good_ec.key [--workers 2]
              [--tls-min 1.2] [--tls-max 1.3] [--resp-body 2] [--ciphers ...] [--delay-ms 0]

Prints `resp_len=<n>` (the exact size of every answer) and `listening` to stdout once it is up.
Handles many connections at once (asyncio), keep-alive, and a body of any Content-Length.
"""
import argparse, asyncio, os, signal, ssl, sys

VERS = {"1.0": ssl.TLSVersion.TLSv1, "1.1": ssl.TLSVersion.TLSv1_1, "1.2": ssl.TLSVersion.TLSv1_2, "1.3": ssl.TLSVersion.TLSv1_3}

def make_ctx(a):
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(a.cert if not a.chain else a.chain, a.key)
    ctx.minimum_version = VERS[a.tls_min]
    ctx.maximum_version = VERS[a.tls_max]
    if a.ciphers:
        ctx.set_ciphers(a.ciphers)
    if a.no_tickets:
        ctx.options |= ssl.OP_NO_TICKET
    return ctx

def response(body_len):
    body = b"o" * body_len
    return b"HTTP/1.1 200 OK\r\nContent-Length: %d\r\n\r\n" % body_len + body

async def handle(reader, writer, resp, delay):
    try:
        while True:
            head = await reader.readuntil(b"\r\n\r\n")
            n = 0
            close = False
            for line in head.split(b"\r\n"):
                low = line.lower()
                if low.startswith(b"content-length:"):
                    n = int(line.split(b":", 1)[1])
                if low.startswith(b"connection:") and b"close" in low:
                    close = True
            if n:
                await reader.readexactly(n)
            if delay:
                await asyncio.sleep(delay)
            writer.write(resp)
            await writer.drain()
            if close:
                break
    except (asyncio.IncompleteReadError, ConnectionError, ssl.SSLError, OSError):
        pass
    finally:
        try:
            writer.close()
        except Exception:
            pass

async def serve(a, ctx, resp):
    server = await asyncio.start_server(lambda r, w: handle(r, w, resp, a.delay_ms / 1000.0), a.host, a.port,
                                        ssl=ctx, reuse_port=True, backlog=2048, ssl_handshake_timeout=10)
    async with server:
        await server.serve_forever()

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, required=True)
    p.add_argument("--cert", required=True)
    p.add_argument("--chain", default="")
    p.add_argument("--key", required=True)
    p.add_argument("--workers", type=int, default=1)
    p.add_argument("--tls-min", default="1.2")
    p.add_argument("--tls-max", default="1.3")
    p.add_argument("--resp-body", type=int, default=2)
    p.add_argument("--ciphers", default="")
    p.add_argument("--delay-ms", type=int, default=0)
    p.add_argument("--no-tickets", action="store_true")
    a = p.parse_args()
    ctx = make_ctx(a)
    resp = response(a.resp_body)
    print("resp_len=%d" % len(resp), flush=True)
    kids = []
    for _ in range(a.workers - 1):
        pid = os.fork()
        if pid == 0:
            asyncio.run(serve(a, ctx, resp))
            sys.exit(0)
        kids.append(pid)
    def stop(*_):
        for k in kids:
            try: os.kill(k, signal.SIGTERM)
            except ProcessLookupError: pass
        os._exit(0)
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    print("listening", flush=True)
    asyncio.run(serve(a, ctx, resp))

main()
