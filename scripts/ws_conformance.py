#!/usr/bin/env python3
"""The OCPP WebSocket example against an independent client (docs/websocket-spike.md, gates G1 and G4).

    python3 scripts/ws_conformance.py <server binary> [<port>]

Starts the server, speaks to it with the `websockets` library (which is not ours) and with raw sockets for what the library will not
send, and checks every answer. Exit status 1 and a line for each check that failed.
"""
import asyncio
import json
import os
import re
import socket
import subprocess
import sys
import time

import websockets

BIN = sys.argv[1]
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 18700
FAILS = []


def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + ("" if ok else f"  {detail}"))
    if not ok:
        FAILS.append(name)


ISO = re.compile(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$")


async def call(ws, mid, action, payload):
    await ws.send(json.dumps([2, mid, action, payload]))
    return json.loads(await asyncio.wait_for(ws.recv(), 5))


async def good_path():
    async with websockets.connect(f"ws://127.0.0.1:{PORT}/ocpp/CP001", subprotocols=["ocpp1.6"]) as ws:
        check("G1 the subprotocol ocpp1.6 is agreed", ws.subprotocol == "ocpp1.6", str(ws.subprotocol))
        r = await call(ws, "19223201", "BootNotification", {"chargePointVendor": "V", "chargePointModel": "M"})
        check("G1 BootNotification: [3, id, {currentTime, interval, status Accepted}]",
              r[0] == 3 and r[1] == "19223201" and r[2]["status"] == "Accepted" and r[2]["interval"] == 300 and ISO.match(r[2]["currentTime"]), str(r))
        now = time.time()
        r = await call(ws, "hb-1", "Heartbeat", {})
        import datetime
        t = datetime.datetime.strptime(r[2]["currentTime"], "%Y-%m-%dT%H:%M:%S.%fZ").replace(tzinfo=datetime.timezone.utc).timestamp()
        check("G1 Heartbeat: the id is echoed and currentTime is the clock's (within 2 s)", r[0] == 3 and r[1] == "hb-1" and abs(t - now) < 2, str(r))
        r = await call(ws, "sn-1", "StatusNotification", {"connectorId": 1, "errorCode": "NoError", "status": "Available"})
        check("G1 StatusNotification: [3, id, {}]", r == [3, "sn-1", {}], str(r))
        r = await call(ws, "mv-1", "MeterValues", {"connectorId": 1, "meterValue": [{"timestamp": "2026-01-01T00:00:00Z", "sampledValue": [{"value": "10"}]}]})
        check("G1 MeterValues: [3, id, {}]", r == [3, "mv-1", {}], str(r))
        r = await call(ws, "au-1", "Authorize", {"idTag": "X"})
        check("G1 an action it does not know: CallError NotImplemented", r[0] == 4 and r[1] == "au-1" and r[2] == "NotImplemented", str(r))
        pong = await asyncio.wait_for(await ws.ping(b"hello"), 5)
        check("G1 a ping is answered with a pong", pong is not None)
        await ws.send("this is not json")
        r = json.loads(await asyncio.wait_for(ws.recv(), 5))
        check("G1 text that is not JSON: CallError FormationViolation", r[0] == 4 and r[2] == "FormationViolation", str(r))
        await ws.send('{"not":"a call"}')
        r = json.loads(await asyncio.wait_for(ws.recv(), 5))
        check("G1 JSON that is not an OCPP message: CallError ProtocolError", r[0] == 4 and r[2] == "ProtocolError", str(r))
        await ws.send(json.dumps([3, "x", {}]))
        r = await call(ws, "after", "Heartbeat", {})
        check("G1 a CallResult from the charge point is accepted silently (the next call is answered)", r[1] == "after", str(r))
        big = await call(ws, "big", "MeterValues", {"pad": "x" * 1500})
        check("G1 a message of about 1.5 KiB (a 16-bit length) is answered", big == [3, "big", {}], str(big)[:80])
        await ws.close()
        check("G1 a close is acknowledged", ws.close_code == 1000, str(ws.close_code))


async def refusal(name, send, code, subprotocol="ocpp1.6"):
    try:
        async with websockets.connect(f"ws://127.0.0.1:{PORT}/ocpp/CP002", subprotocols=[subprotocol]) as ws:
            await send(ws)
            try:
                await asyncio.wait_for(ws.recv(), 5)
            except websockets.ConnectionClosed:
                pass
            check(f"G4 {name}: closed with {code}", ws.close_code == code, str(ws.close_code))
    except Exception as e:
        check(f"G4 {name}", False, repr(e))


def raw_handshake(extra=""):
    s = socket.create_connection(("127.0.0.1", PORT), timeout=5)
    s.sendall((f"GET /ocpp/CP9 HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
               f"Sec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: ocpp1.6\r\n{extra}\r\n").encode())
    data = b""
    while b"\r\n\r\n" not in data:
        data += s.recv(4096)
    return s, data


async def refusals():
    async def too_big(ws):
        await ws.send("x" * 3000)

    async def fragmented(ws):
        async def parts():
            yield "[2,\"1\","
            yield "\"Heartbeat\",{}]"
        await ws.send(parts())

    async def binary(ws):
        await ws.send(b"\x00\x01")

    await refusal("a message larger than the buffer", too_big, 1009)
    await refusal("a fragmented message", fragmented, 1003)
    await refusal("a binary frame", binary, 1003)
    # the RFC's own handshake, with the RFC's own key, seen raw
    s, data = raw_handshake()
    check("G1 the RFC 6455 example key gets the RFC's accept value", b"Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=" in data and data.startswith(b"HTTP/1.1 101"), str(data))
    s.close()
    # a ping with a 200-byte payload (not allowed: control frames carry at most 125), masked, sent raw
    s, _ = raw_handshake()
    payload = b"p" * 200
    s.sendall(bytes([0x89, 0xfe, 0, 200]) + b"\0\0\0\0" + payload)
    got = s.recv(4096)
    check("G4 a ping with 200 bytes: close 1002", len(got) >= 4 and got[0] == 0x88 and int.from_bytes(got[2:4], "big") == 1002, str(got[:6]))
    s.close()
    # no subprotocol
    s = socket.create_connection(("127.0.0.1", PORT), timeout=5)
    s.sendall(b"GET /ocpp/CP9 HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n")
    got = s.recv(4096)
    check("G4 a client that does not offer ocpp1.6: HTTP 400", got.startswith(b"HTTP/1.1 400"), str(got[:30]))
    s.close()
    # a request that is not a WebSocket at all
    s = socket.create_connection(("127.0.0.1", PORT), timeout=5)
    s.sendall(b"GET /healthz HTTP/1.1\r\nHost: x\r\n\r\n")
    got = s.recv(4096)
    check("G4 a plain HTTP request: HTTP 400", got.startswith(b"HTTP/1.1 400"), str(got[:30]))
    s.close()
    # a head that never ends, larger than the buffer
    s = socket.create_connection(("127.0.0.1", PORT), timeout=5)
    s.sendall(b"GET /ocpp/CP9 HTTP/1.1\r\nX-Pad: " + b"a" * 4000)
    got = s.recv(4096)
    check("G4 a head larger than the buffer: HTTP 431", got.startswith(b"HTTP/1.1 431"), str(got[:30]))
    s.close()
    # 1,000 connections that send nothing, and then a good one
    idle = []
    for _ in range(1000):
        idle.append(socket.create_connection(("127.0.0.1", PORT), timeout=5))
    await good_path()
    for c in idle:
        c.close()
    check("G4 1,000 connections that never speak do not stop a good one (G1 repeated on top of them)", True)


def main():
    proc = subprocess.Popen([BIN, str(PORT), "4000", "30"], stderr=subprocess.PIPE, stdout=subprocess.DEVNULL)
    proc.stderr.readline()
    try:
        asyncio.run(good_path())
        asyncio.run(refusals())
        # after all of that, the server is still serving
        check("G4 the server is still running", proc.poll() is None)
        asyncio.run(good_path())
    finally:
        proc.terminate()
        proc.wait()
    print("FAILED: " + ", ".join(FAILS) if FAILS else "all conformance checks passed")
    sys.exit(1 if FAILS else 0)


main()
