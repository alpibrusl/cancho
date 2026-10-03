#!/usr/bin/env python3
"""A load generator for OCPP-J over WebSocket, the same for every server (docs/websocket-spike.md, section 3).

    python3 scripts/ws_load.py <scenario> --port P --pid SERVER_PID [--conns N] [--procs K] ...

Scenarios:
  scale        N connections, each handshake + BootNotification checked (id echoed, Accepted, interval, ISO currentTime, and the
               Sec-WebSocket-Accept recomputed here); then --hold seconds idle; then a burst of one Heartbeat on every connection at
               once; then --hb-duration seconds of a Heartbeat every --hb-interval seconds on every connection (random phase)
  throughput   --conns connections in a closed loop of 1 KiB MeterValues for --seconds
The server's resident memory and CPU time are read from /proc/<pid>, not from the server. One JSON object is printed at the end.
The generator runs in --procs processes pinned to the CPUs in --cpus; pin the server elsewhere with `taskset`.
"""
import argparse
import asyncio
import base64
import hashlib
import json
import multiprocessing as mp
import os
import random
import sys
import time

GUID = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


def mask_payload(data, mask):
    n = len(data)
    if n == 0:
        return b""
    m = (mask * ((n + 3) // 4))[:n]
    return (int.from_bytes(data, "big") ^ int.from_bytes(m, "big")).to_bytes(n, "big")


def frame(payload, opcode=1):
    n = len(payload)
    if n < 126:
        head = bytes([0x80 | opcode, 0x80 | n])
    elif n < 65536:
        head = bytes([0x80 | opcode, 0x80 | 126]) + n.to_bytes(2, "big")
    else:
        head = bytes([0x80 | opcode, 0x80 | 127]) + n.to_bytes(8, "big")
    mask = os.urandom(4)
    return head + mask + mask_payload(payload, mask)


async def read_frame(r):
    b = await r.readexactly(2)
    opcode, ln = b[0] & 15, b[1] & 127
    if ln == 126:
        ln = int.from_bytes(await r.readexactly(2), "big")
    elif ln == 127:
        ln = int.from_bytes(await r.readexactly(8), "big")
    return opcode, await r.readexactly(ln)


async def connect(host, port, cp):
    r, w = await asyncio.open_connection(host, port)
    key = base64.b64encode(os.urandom(16))
    w.write(b"GET /ocpp/" + cp.encode() + b" HTTP/1.1\r\nHost: " + host.encode() + b"\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: "
            + key + b"\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: ocpp1.6\r\n\r\n")
    head = await r.readuntil(b"\r\n\r\n")
    want = base64.b64encode(hashlib.sha1(key + GUID).digest())
    accept = [l.split(b":", 1)[1].strip() for l in head.split(b"\r\n") if l.lower().startswith(b"sec-websocket-accept:")]
    if not head.startswith(b"HTTP/1.1 101") or accept != [want]:
        raise RuntimeError(f"bad handshake: {head[:80]!r}")
    return r, w


async def call(r, w, mid, action, payload):
    w.write(frame(json.dumps([2, mid, action, payload]).encode()))
    op, data = await read_frame(r)
    msg = json.loads(data)
    if op != 1 or msg[0] != 3 or msg[1] != mid:
        raise RuntimeError(f"bad answer {data[:80]!r}")
    return msg


OPEN_CONCURRENCY = int(os.environ.get("WS_OPEN_CONCURRENCY", "100"))


class Worker:
    def __init__(self, idx, host, port, cpus):
        self.idx, self.host, self.port = idx, host, port
        os.sched_setaffinity(0, cpus)
        self.conns = []

    async def open(self, n):
        sem = asyncio.Semaphore(OPEN_CONCURRENCY)
        t0 = time.time()

        async def one(i):
            cp = f"CP-{self.idx}-{i}"
            async with sem:
                r, w = await connect(self.host, self.port, cp)
                m = await call(r, w, "boot-" + cp, "BootNotification", {"chargePointVendor": "V", "chargePointModel": "M", "chargePointSerialNumber": cp})
                assert m[2]["status"] == "Accepted" and isinstance(m[2]["interval"], int) and m[2]["currentTime"].endswith("Z"), m
            self.conns.append((r, w, cp))

        await asyncio.gather(*[one(i) for i in range(n)])
        return {"opened": len(self.conns), "seconds": time.time() - t0}

    async def burst(self):
        t0 = time.time()
        rtts = []

        async def one(r, w, cp):
            s = time.perf_counter()
            await call(r, w, "b-" + cp, "Heartbeat", {})
            rtts.append(time.perf_counter() - s)

        await asyncio.gather(*[one(*c) for c in self.conns])
        return {"seconds": time.time() - t0, "rtts": rtts}

    async def heartbeats(self, interval, duration):
        rtts = []
        end = time.time() + duration

        async def one(r, w, cp):
            await asyncio.sleep(random.random() * interval)
            k = 0
            while time.time() < end:
                s = time.perf_counter()
                await call(r, w, f"h-{cp}-{k}", "Heartbeat", {})
                rtts.append(time.perf_counter() - s)
                k += 1
                await asyncio.sleep(interval)

        await asyncio.gather(*[one(*c) for c in self.conns])
        return {"rtts": rtts}

    async def throughput(self, seconds):
        count = 0
        end = time.time() + seconds
        pad = "x" * 900

        async def one(r, w, cp):
            nonlocal count
            k = 0
            while time.time() < end:
                await call(r, w, f"m-{k}", "MeterValues", {"connectorId": 1, "pad": pad})
                count += 1
                k += 1

        await asyncio.gather(*[one(*c) for c in self.conns])
        return {"messages": count, "seconds": seconds}

    async def close(self):
        for r, w, cp in self.conns:
            w.close()
        self.conns = []
        return {}


def worker_main(idx, host, port, cpus, cmds, results):
    w = Worker(idx, host, port, cpus)

    async def loop():
        while True:
            cmd = await asyncio.get_running_loop().run_in_executor(None, cmds.get)
            name, args = cmd[0], cmd[1:]
            try:
                res = await getattr(w, name)(*args)
            except Exception as e:  # reported, not hidden
                res = {"error": repr(e)}
            results.put((idx, name, res))
            if name == "close":
                return

    asyncio.run(loop())


def proc_stat(pid):
    with open(f"/proc/{pid}/stat") as f:
        parts = f.read().rsplit(")", 1)[1].split()
    ticks = int(parts[11]) + int(parts[12])
    with open(f"/proc/{pid}/status") as f:
        rss = next(int(l.split()[1]) for l in f if l.startswith("VmRSS"))
    return {"cpu_s": ticks / os.sysconf("SC_CLK_TCK"), "rss_kb": rss}


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(len(xs) * p))] if xs else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("scenario", choices=["scale", "throughput"])
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, required=True)
    ap.add_argument("--pid", type=int, required=True)
    ap.add_argument("--conns", type=int, default=1000)
    ap.add_argument("--procs", type=int, default=3)
    ap.add_argument("--cpus", default="1,2,3")
    ap.add_argument("--hold", type=float, default=30)
    ap.add_argument("--hb-interval", type=float, default=30)
    ap.add_argument("--hb-duration", type=float, default=120)
    ap.add_argument("--seconds", type=float, default=10)
    a = ap.parse_args()
    cpus = {int(c) for c in a.cpus.split(",")}
    os.sched_setaffinity(0, cpus)
    ctx = mp.get_context("fork")
    results = ctx.Queue()
    queues, procs = [], []
    for i in range(a.procs):
        q = ctx.Queue()
        p = ctx.Process(target=worker_main, args=(i, a.host, a.port, cpus, q, results))
        p.start()
        queues.append(q)
        procs.append(p)

    def order(*cmd):
        for q in queues:
            q.put(cmd)
        got = [results.get() for _ in queues]
        errs = [g[2]["error"] for g in got if "error" in g[2]]
        if errs:
            raise RuntimeError(errs[0])
        return [g[2] for g in got]

    out = {"scenario": a.scenario, "conns": a.conns}
    per = a.conns // a.procs
    base = proc_stat(a.pid)
    out["server_at_start"] = base
    try:
        order("open", per)
        opened = per * a.procs
        up = proc_stat(a.pid)
        out["opened"] = opened
        out["server_after_open"] = up
        if a.scenario == "scale":
            t0 = time.time()
            time.sleep(a.hold)
            idle = proc_stat(a.pid)
            out["idle_seconds"] = a.hold
            out["idle_cpu_percent_of_a_core"] = 100 * (idle["cpu_s"] - up["cpu_s"]) / (time.time() - t0)
            out["server_after_idle"] = idle
            t0 = time.time()
            b = order("burst")
            rt = [x for r in b for x in r["rtts"]]
            out["burst"] = {"all_answered_seconds": time.time() - t0, "connections": len(rt), "p50_ms": 1000 * pct(rt, .5), "p99_ms": 1000 * pct(rt, .99), "max_ms": 1000 * max(rt)}
            before = proc_stat(a.pid)
            t0 = time.time()
            h = order("heartbeats", a.hb_interval, a.hb_duration)
            took = time.time() - t0
            rt = [x for r in h for x in r["rtts"]]
            after = proc_stat(a.pid)
            out["heartbeats"] = {"interval_s": a.hb_interval, "duration_s": took, "messages": len(rt), "per_second": len(rt) / took,
                                 "p50_ms": 1000 * pct(rt, .5), "p99_ms": 1000 * pct(rt, .99), "max_ms": 1000 * max(rt),
                                 "server_cpu_percent_of_a_core": 100 * (after["cpu_s"] - before["cpu_s"]) / took}
            out["server_end"] = after
        else:
            before = proc_stat(a.pid)
            r = order("throughput", a.seconds)
            after = proc_stat(a.pid)
            total = sum(x["messages"] for x in r)
            out["throughput"] = {"messages": total, "per_second": total / a.seconds, "server_cpu_percent_of_a_core": 100 * (after["cpu_s"] - before["cpu_s"]) / a.seconds}
    finally:
        try:
            order("close")
        except Exception:
            pass
        for p in procs:
            p.join(timeout=10)
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
