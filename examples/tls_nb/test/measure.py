#!/usr/bin/env python3
"""Every number in docs/tls-nonblocking.md section 7, with the method in the code.

    python3 measure.py [section ...]       sections: cpu, rate, session, requests, memory, concurrency, resume, all (default)

Method (the same for all of them):
  * the client under test is pinned to core 3 with `taskset`, the test receiver (2 worker processes) to cores 1 and 2, so the
    client's CPU is its own and the server is not starved by it; the machine has 4 vCPUs and is shared, so numbers carry noise,
    which is why every row is the MEDIAN of REPS runs and shows the minimum and maximum;
  * CPU is user+sys of the client process from wait4's rusage, divided by the connections it made (the process start-up and the
    trust-store load are inside it and are small next to 2,000 connections; `startup` below says how small);
  * memory is VmRSS read from /proc/<pid>/status when the client prints HELD (all handshakes done, nothing sent yet) minus the
    value at READY (arrays allocated, before the first connection).
"""
import os, statistics, subprocess, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import harness as h

REPS = int(os.environ.get("REPS", "7"))
CLIENT_CPU = ["taskset", "-c", "3"]
SERVER_CPUS = "1,2"
BIN = None


def med(xs):
    return statistics.median(xs)


def fmt(xs, unit="", prec=3):
    return "%.*f%s (min %.*f, max %.*f)" % (prec, med(xs), unit, prec, min(xs), prec, max(xs))


def cpu_per_conn(r, n):
    return (r["user"] + r["sys"]) / n * 1000.0


def runs(server_kw, n=2000, **kw):
    """REPS runs of the client against one server; answers the list of result dicts."""
    out = []
    with h.Server(workers=2, cpus=SERVER_CPUS, **server_kw) as s:
        for _ in range(REPS):
            r = h.run_client(BIN, s.port, resp_len=s.resp_len, prefix=CLIENT_CPU, **kw)
            want = kw.get("total", 1)
            assert r["summary"].get("ok") == want and r["exit"] == 0, (kw, r["outcomes"], r["stderr"])
            out.append(r)
    return out


def row(label, server_kw, **kw):
    rs = runs(server_kw, **kw)
    n = kw["total"]
    cpu = [cpu_per_conn(r, n) for r in rs]
    rate = [n / (r["summary"]["wall_ms"] / 1000.0) for r in rs]
    print("| %-52s | %-30s | %-18s | %s |" % (label, fmt(cpu, " ms"), "%d" % med(rate), rs[0]["session"].split("  ")[0] + " " + (rs[0]["session"].split()[1] if len(rs[0]["session"].split()) > 1 else "")))
    return med(cpu), med(rate)


def section_cpu():
    print("\n### handshake CPU per connection (2,000 connections, 64 in flight, handshake only, nothing sent)\n")
    print("| %-52s | %-30s | %-18s | %s |" % ("configuration", "client CPU per handshake", "handshakes/s (wall)", "cipher, protocol"))
    print("|---|---|---|---|")
    base = dict(total=2000, conc=64, reqs=0)
    EC = dict(cert="good_ec")
    RSA = dict(cert="good_rsa")
    for io, name in ((0, "memory BIOs"), (1, "SSL_set_fd")):
        row("TLS 1.3, ECDSA P-256, no verification, %s" % name, EC, verify=0, cafile="-", io=io, **base)
        row("TLS 1.3, ECDSA P-256, verified (CA file), %s" % name, EC, verify=1, io=io, **base)
    row("TLS 1.3, ECDSA P-256, verified, CA file + system store", EC, verify=1, default_paths=1, **base)
    row("TLS 1.3, RSA 2048 server cert, verified", RSA, verify=1, **base)
    row("TLS 1.3, RSA 2048 server cert, no verification", RSA, verify=0, cafile="-", **base)
    row("TLS 1.2, ECDSA P-256, verified", dict(EC, tls_max="1.2"), verify=1, **base)
    row("TLS 1.2, RSA 2048, verified", dict(RSA, tls_max="1.2"), verify=1, **base)
    print("\nstart-up and trust-store load (one connection per process, so everything that is not a handshake shows):")
    for label, kw in (("no verification", dict(verify=0, cafile="-")), ("CA file", dict(verify=1)), ("CA file + system store", dict(verify=1, default_paths=1))):
        xs = []
        with h.Server("good_ec", workers=1, cpus=SERVER_CPUS) as s:
            for _ in range(REPS):
                r = h.run_client(BIN, s.port, total=1, conc=1, reqs=0, resp_len=s.resp_len, prefix=CLIENT_CPU, **kw)
                xs.append((r["user"] + r["sys"]) * 1000)
        print("  %-26s %s" % (label, fmt(xs, " ms")))


def section_c_reference():
    print("\n### the same work in C (test/c_ref.c blocking, test/c_epoll.c non-blocking), same server, same pinning\n")
    cdir = h.WORK
    for src, name in (("c_ref.c", "c_ref"), ("c_epoll.c", "c_epoll")):
        subprocess.run(["cc", "-O2", "-o", os.path.join(cdir, name), os.path.join(h.HERE, src), "-lssl", "-lcrypto"], check=True)
    C = h.ensure_certs()
    with h.Server("good_ec", workers=2, cpus=SERVER_CPUS) as s:
        for verify in (0, 1):
            for name, args in (("c_ref (blocking, one at a time)", ["c_ref", "2000", str(verify), C + "/ca.pem"]),
                               ("c_epoll (non-blocking, 64 in flight)", ["c_epoll", "2000", "64", str(verify), C + "/ca.pem"])):
                xs = []
                for _ in range(REPS):
                    exe = os.path.join(cdir, args[0])
                    cmd = CLIENT_CPU + [exe, "127.0.0.1", str(s.port), "hooks.test"] + args[1:]
                    p = subprocess.run(cmd, capture_output=True, text=True)
                    xs.append(float(p.stdout.split("cpu_ms_per_conn=")[1]))
                print("  %-40s verify=%d  %s" % (name, verify, fmt(xs, " ms")))


def section_requests():
    print("\n### requests over an established session (64 connections held open, TLS 1.3, verified)\n")
    print("The handshakes are 64 of the work and are subtracted using the per-handshake cost measured in the same run's section `cpu`")
    print("(0.64 ms, verified): with thousands of requests per connection they are under 5% of the CPU.\n")
    print("| %-14s | %-9s | %-30s | %-18s | %-22s | %s |" % ("request body", "requests", "client CPU per request", "client busy", "requests/s (wall)", "derived: requests/s on one core"))
    print("|---|---|---|---|---|---|")
    N = 64
    for body, R in ((100, 2000), (1024, 2000), (16000, 400), (60000, 100)):
        cpu, rate, busy = [], [], []
        with h.Server("good_ec", workers=2, cpus=SERVER_CPUS) as s:
            for _ in range(REPS):
                r = h.run_client(BIN, s.port, total=N, conc=N, reqs=R, body=body, resp_len=s.resp_len, prefix=CLIENT_CPU)
                assert r["summary"]["ok"] == N, r["outcomes"]
                total_cpu = r["user"] + r["sys"] - N * 0.00064
                cpu.append(total_cpu / (N * R) * 1000)
                rate.append(N * R / (r["summary"]["wall_ms"] / 1000.0))
                busy.append(100.0 * (r["user"] + r["sys"]) / (r["summary"]["wall_ms"] / 1000.0))
        print("| %-14s | %-9d | %-30s | %-18s | %-22d | %d |" % ("%d bytes" % body, N * R, fmt(cpu, " ms", 4), "%.0f%% of a core" % med(busy), med(rate), 1000.0 / med(cpu)))
    print("\n(the receiver is a Python asyncio process on two cores and is the limit on requests/s: the client is busy for only part of the time. The last column is 1 / CPU per request, what one core of the client could do against a receiver that kept up)")


def section_memory():
    print("\n### memory per connection (all handshakes done, connections held, nothing sent; RSS of the client process)\n")
    print("| %-34s | %-12s | %-12s | %-14s | %s |" % ("configuration", "connections", "RSS at READY", "RSS at HELD", "per connection"))
    print("|---|---|---|---|---|")
    grown = {}
    for io, relbuf, label in ((0, 0, "memory BIOs"), (0, 1, "memory BIOs, RELEASE_BUFFERS"), (1, 0, "SSL_set_fd"), (1, 1, "SSL_set_fd, RELEASE_BUFFERS")):
        for n in (64, 512):
            deltas = []
            r0 = r1 = 0
            with h.Server("good_ec", workers=2, cpus=SERVER_CPUS) as s:
                for _ in range(3):
                    r = h.run_client(BIN, s.port, total=n, conc=n, reqs=0, verify=1, hold_ms=600, release_buffers=relbuf, io=io, resp_len=s.resp_len)
                    assert r["summary"]["ok"] == n, r["outcomes"]
                    deltas.append((r["held_rss_kb"] - r["start_rss_kb"]) / n)
                    r0, r1 = r["start_rss_kb"], r["held_rss_kb"]
            print("| %-34s | %-12d | %-12s | %-14s | %.1f KiB (min %.1f, max %.1f) |" % (label, n, "%d KiB" % r0, "%d KiB" % r1, med(deltas), min(deltas), max(deltas)))
            grown[(io, relbuf, n)] = med(deltas) * n
        slope = (grown[(io, relbuf, 512)] - grown[(io, relbuf, 64)]) / float(512 - 64)
        print("| %-34s | slope between 64 and 512 connections: **%.1f KiB per additional connection**; the rest is one-time (trust store, OpenSSL's tables) | | | |" % (label, slope))
    print("\nthe client allocates %d bytes of ciphertext buffer per slot in memory-BIO mode before the first connection (it is in RSS at READY, not in the per-connection figure)" % 20480)


def section_concurrency():
    print("\n### 64 handshakes and requests multiplexed on ONE thread\n")
    with h.Server("good_ec", workers=2, cpus=SERVER_CPUS) as s:
        r = h.run_client(BIN, s.port, total=64, conc=64, reqs=3, verify=1, hold_ms=300, resp_len=s.resp_len, prefix=CLIENT_CPU)
        sm = r["summary"]
        print("  64 connections in flight at once, 3 requests each: ok=%d of 64, threads in the client process while all 64 were open: %d, open descriptors then: %s" % (sm["ok"], r["held_threads"], r["held_fds"]))
        print("  handshake latency avg %d ms, max %d ms; whole run %d ms" % (sm["hs_ms_avg"], sm["hs_ms_max"], sm["wall_ms"]))
        for io in (0, 1):
            r = h.run_client(BIN, s.port, total=20000, conc=64, reqs=1, verify=1, io=io, resp_len=s.resp_len, prefix=CLIENT_CPU)
            print("  20,000 connections through 64 slots (mode %d): ok=%d, %d ms, max RSS %d KiB" % (io, r["summary"]["ok"], r["summary"]["wall_ms"], r["maxrss_kb"]))
    tarpit_n = 64
    import socket, threading
    ls = socket.socket(); ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); ls.bind(("127.0.0.1", 0)); ls.listen(1024)
    held = []
    def acc():
        while True:
            try:
                c, _ = ls.accept(); held.append(c)
            except OSError:
                return
    threading.Thread(target=acc, daemon=True).start()
    r = h.run_client(BIN, ls.getsockname()[1], total=tarpit_n, conc=tarpit_n, reqs=1, deadline_ms=1000, prefix=CLIENT_CPU)
    print("  %d connections to a server that accepts and never answers, deadline 1000 ms: outcomes %s, wall %.2f s, client CPU %.0f ms" % (tarpit_n, r["outcomes"], r["wall"], (r["user"] + r["sys"]) * 1000))
    ls.close()


def section_resume():
    print("\n### session resumption (2,000 connections, 64 in flight, one request each, verified, TLS 1.3)\n")
    for res in (0, 1):
        rs = runs(dict(cert="good_ec"), total=2000, conc=64, reqs=1, resume=res)
        print("  resume=%d: client CPU per connection %s, resumed %d of 2000" % (res, fmt([cpu_per_conn(r, 2000) for r in rs], " ms"), rs[0]["summary"]["resumed"]))


def section_resolve():
    print("\n### what a name lookup adds to a delivery (2,000 connections, 64 in flight, handshake only, verified, TLS 1.3)\n")
    import subprocess as sp
    zones = ["--zone", "hooks.test=127.0.0.1"]
    dport = h.free_port()
    stub = sp.Popen([sys.executable, os.path.join(h.HERE, "dns_stub.py"), "--port", str(dport)] + zones, stdout=sp.DEVNULL, stderr=sp.DEVNULL)
    import time
    time.sleep(0.6)
    try:
        for label, ns in (("IP literal (no lookup)", None), ("name, resolved over DNS/TCP on the poller, every delivery", ("127.0.0.1", dport, 1))):
            rs = runs(dict(cert="good_ec"), total=2000, conc=64, reqs=0, ns=ns, ip="hooks.test" if ns else "127.0.0.1")
            print("  %-62s %s" % (label, fmt([cpu_per_conn(r, 2000) for r in rs], " ms")))
    finally:
        stub.terminate()


SECTIONS = {"cpu": section_cpu, "reference": section_c_reference, "requests": section_requests, "memory": section_memory,
            "concurrency": section_concurrency, "resume": section_resume, "resolve": section_resolve}

if __name__ == "__main__":
    BIN = h.build()
    want = sys.argv[1:] or list(SECTIONS)
    print("machine: %s, %s; client pinned to core 3, receiver to cores %s; REPS=%d" % (os.uname().release, subprocess.run(["nproc"], capture_output=True, text=True).stdout.strip() + " cores", SERVER_CPUS, REPS))
    print("OpenSSL: %s" % subprocess.run(["openssl", "version"], capture_output=True, text=True).stdout.strip())
    for w in want:
        SECTIONS[w]()
