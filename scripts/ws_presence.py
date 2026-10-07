#!/usr/bin/env python3
"""The Redis workload a charge-point registry needs, run against any RESP server (Redis or cancho-cache).

    python3 scripts/ws_presence.py --port 6380 --pid PID [--keys 10000] [--rate 333] [--seconds 30]

Phases: register N keys with an expiry (`SET cp:<id> <pod> EX 90`), refresh them at `--rate` per second (the 30 s heartbeat of
10,000 charge points), look them up (`GET`), then the two commands `lex-csms` actually uses Redis for: `PUBLISH` and
`PSUBSCRIBE`. Prints one JSON object. Latency is one request, one reply, on one connection (no pipelining): what a handler pays.
"""
import argparse, json, os, time
import redis


def rss_kb(pid):
    for line in open(f"/proc/{pid}/status"):
        if line.startswith("VmRSS:"):
            return int(line.split()[1])


def pct(v, p):
    v = sorted(v)
    return v[min(len(v) - 1, int(len(v) * p))] * 1000


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, required=True)
    ap.add_argument("--pid", type=int, required=True)
    ap.add_argument("--keys", type=int, default=10000)
    ap.add_argument("--rate", type=float, default=333)
    ap.add_argument("--seconds", type=float, default=30)
    a = ap.parse_args()
    r = redis.Redis(port=a.port, socket_timeout=5)
    out = {"keys": a.keys, "rss_start_kb": rss_kb(a.pid)}
    t0 = time.time()
    for i in range(a.keys):
        r.set(f"cp:CP-{i:05d}", "pod-a", ex=90)
    out["register_seconds"] = round(time.time() - t0, 3)
    out["rss_after_register_kb"] = rss_kb(a.pid)
    assert r.dbsize() == a.keys, r.dbsize()
    lat, n, t0 = [], 0, time.time()
    cpu0 = sum(map(float, open(f"/proc/{a.pid}/stat").read().split()[13:15])) / os.sysconf("SC_CLK_TCK")
    while time.time() - t0 < a.seconds:
        due = t0 + n / a.rate
        now = time.time()
        if due > now:
            time.sleep(due - now)
        k = f"cp:CP-{n % a.keys:05d}"
        s = time.perf_counter()
        r.set(k, "pod-a", ex=90)
        lat.append(time.perf_counter() - s)
        if n % 10 == 0:
            s = time.perf_counter()
            assert r.get(k) == b"pod-a"
            lat.append(time.perf_counter() - s)
        n += 1
    cpu1 = sum(map(float, open(f"/proc/{a.pid}/stat").read().split()[13:15])) / os.sysconf("SC_CLK_TCK")
    out["refreshes"] = n
    out["rate_achieved"] = round(n / (time.time() - t0), 1)
    out["p50_ms"], out["p99_ms"], out["max_ms"] = round(pct(lat, .5), 3), round(pct(lat, .99), 3), round(max(lat) * 1000, 3)
    out["server_cpu_percent_of_a_core"] = round(100 * (cpu1 - cpu0) / (time.time() - t0), 2)
    out["ttl_set"] = 0 < r.ttl("cp:CP-00000") <= 90
    out["rss_end_kb"] = rss_kb(a.pid)
    for name, cmd in (("PUBLISH", ["PUBLISH", "csms:cmd:CP-00001", "x"]), ("PSUBSCRIBE", ["PSUBSCRIBE", "csms:cmd:*"])):
        try:
            out[name] = repr(r.execute_command(*cmd))
        except Exception as e:
            out[name] = "error: " + str(e)[:100]
        if name == "PSUBSCRIBE":
            r = redis.Redis(port=a.port, socket_timeout=5)
    print(json.dumps(out, indent=1))


main()
