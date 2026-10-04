#!/usr/bin/env python3
"""The three resolvers against a name server that can be slow (dns_stub.py), and what each does to the loop that runs beside it.

    python3 resolve_test.py      # exits 0 only if every check holds; prints the table of docs/tls-nonblocking.md section 6

`blocking` is `tcp_connect_start(name)` (getaddrinfo inside); `tcp` is rtcp.ls (DNS over TCP on the poller); `thread` is
rthread.ls (four workers, `res_query`). libc's two read /etc/resolv.conf, so they run in a private mount namespace in which that
file names 127.0.0.1, where a stub on port 53 listens (`with_resolver.sh`); the `tcp` resolver is given the stub's address. The
numbers that matter: `max_gap_ms`, how long the main loop went without running (a loop that waits for DNS has a gap as long as the
wait), and `ms`, how long each lookup took.
"""
import os, re, subprocess, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import harness as h

ZONES = ["--zone", "hooks.test=127.0.0.1", "--zone", "many.test=10.0.0.1,93.184.216.34", "--zone", "slow.test=127.0.0.1",
         "--delay", "slow.test=300", "--zone", "slow2.test=127.0.0.1", "--delay", "slow2.test=300", "--rebind", "flip.test=93.184.216.34,169.254.169.254"]

def stub(port):
    log = os.path.join(h.WORK, "dns_stub_%d.log" % port)
    f = open(log, "w")
    p = subprocess.Popen([sys.executable, os.path.join(h.HERE, "dns_stub.py"), "--port", str(port)] + ZONES, stdout=f, stderr=subprocess.STDOUT)
    time.sleep(0.6)
    return p, log

def queries(log, name=None):
    with open(log) as f:
        qs = [l.split()[1] for l in f if l.startswith("Q ")]
    return qs if name is None else [q for q in qs if q == name]

def run(binary, mode, count, names, ns_port):
    conf = os.path.join(h.WORK, "resolv.conf")
    with open(conf, "w") as f:
        f.write("nameserver 127.0.0.1\noptions timeout:2 attempts:1\n")
    cmd = [binary, mode, "127.0.0.1", str(ns_port), str(count)] + names
    if mode != "tcp":
        cmd = ["unshare", "-m", os.path.join(h.HERE, "with_resolver.sh"), conf] + cmd
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    lookups = {}
    for l in p.stdout.splitlines():
        m = re.match(r"lookup (\d+) (\S+) code=(-?\d+) addrs=(\S*) ttl=(\d+) ms=(\d+)", l)
        if m:
            lookups[int(m.group(1))] = (m.group(2), int(m.group(3)), m.group(4), int(m.group(5)), int(m.group(6)))
    g = re.search(r"loop iterations=(\d+) max_gap_ms=(\d+) total_ms=(\d+)", p.stdout)
    return lookups, (int(g.group(1)), int(g.group(2)), int(g.group(3))) if g else None, p

def main():
    w = h.WORK
    os.makedirs(w, exist_ok=True)
    binary = os.path.join(w, "resolve_demo")
    srcs = [os.path.join(h.EXAMPLE, s) for s in ("dns.ls", "rtcp.ls", "rthread.ls", "nat.ls", "resolve_demo.ls")]
    # The thread resolver's sockets are the repository's own packages (`net.sockets`, `net.connect`), whose scope is `Ffi("libc")`.
    srcs += [os.path.join(h.ROOT, "packages", "net-sockets", "sockets.ls"), os.path.join(h.ROOT, "packages", "net-connect", "connect.ls")]
    subprocess.run([h.LEXSYS, "build"] + srcs + ["--std", "-o", binary], check=True)
    bad = 0
    def check(label, ok, detail=""):
        nonlocal bad
        print("  %-4s %s %s" % ("ok" if ok else "FAIL", label, detail))
        bad += 0 if ok else 1
    s53, log53 = stub(53)
    s53b, log5300 = stub(5300)
    try:
        names = ["hooks.test", "many.test", "slow.test", "nx.test", "hooks.test"]
        print("five lookups at once, one of them 300 ms slow (a name server that answers it late):")
        rows = {}
        for mode in ("blocking", "tcp", "thread"):
            lk, loop, p = run(binary, mode, 5, names, 5300)
            rows[mode] = (lk, loop)
            check("%s: the program ended with status 0 (for thread: every worker joined)" % mode, p.returncode == 0, "(status %d)" % p.returncode)
            print("  %-9s loop: %s iterations, longest gap %d ms, whole run %d ms;  lookups ms: %s" % (mode, loop[0], loop[1], loop[2], [lk[i][4] for i in range(5)]))
        for mode in ("tcp", "thread"):
            lk, loop = rows[mode]
            check("%s: hooks.test -> 127.0.0.1" % mode, lk[0][2] == "127.0.0.1" and lk[0][1] == 1)
            check("%s: many.test -> two addresses" % mode, lk[1][2] == "10.0.0.1,93.184.216.34" and lk[1][3] == 60)
            check("%s: the slow name took its 300 ms and the others did not wait for it" % mode, 295 <= lk[2][4] <= 450 and lk[0][4] < 100 and lk[1][4] < 100)
            check("%s: an unknown name is an error, not an address" % mode, lk[3][2] == "" and lk[3][1] < 0)
            check("%s: the main loop never stopped for more than 15 ms" % mode, loop[1] <= 15, "(longest gap %d ms)" % loop[1])
        lk, loop = rows["blocking"]
        check("blocking: the loop stopped for the whole 300 ms", 290 <= loop[1] <= 450, "(longest gap %d ms)" % loop[1])

        print("eight slow (300 ms) lookups at once:")
        for mode in ("blocking", "tcp", "thread"):
            lk, loop, p = run(binary, mode, 8, ["slow.test", "slow2.test"] * 4, 5300)
            print("  %-9s whole run %d ms, longest loop gap %d ms, per-lookup ms %s" % (mode, loop[2], loop[1], [lk[i][4] for i in range(8)]))
            if mode == "tcp":
                check("tcp: eight at once cost one wait", loop[2] < 600, "(%d ms)" % loop[2])
            if mode == "thread":
                check("thread: four workers make two waves", 580 <= loop[2] < 1000, "(%d ms)" % loop[2])
            if mode == "blocking":
                check("blocking: one after another", loop[2] >= 2300, "(%d ms)" % loop[2])

        print("DNS rebinding: a name that answers 93.184.216.34 the first time and 169.254.169.254 the second:")
        before = len(queries(log5300, "flip.test"))
        lk, loop, p = run(binary, "tcp", 2, ["flip.test", "flip.test"], 5300)
        print("  two lookups of flip.test -> %s and %s (one of them is the cloud metadata address)" % (lk[0][2], lk[1][2]))
        check("the resolver sees the change, so a check must be made on the answer that is used", {lk[0][2], lk[1][2]} == {"93.184.216.34", "169.254.169.254"})
    finally:
        s53.terminate(); s53b.terminate()
    print("FAILED: %d" % bad if bad else "all checks hold")
    return 1 if bad else 0

sys.exit(main())
