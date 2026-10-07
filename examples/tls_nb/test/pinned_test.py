#!/usr/bin/env python3
"""Resolve, check, pin, connect, verify: a delivery to a NAME, end to end, with the address it connected to as the evidence.

    python3 pinned_test.py       # exits 0 only if every check holds

The client resolves the name with DNS over TCP on its poller (rtcp.cho), judges every address in the answer (pin.cho: the rule of
cancho-hooks' src/destination.cho), connects to the address it chose **as an IP literal** (no second lookup), and uses the name only
for SNI and for the certificate check. The name server is dns_stub.py; the receivers are TLS servers for the name `hooks.test`
(the certificate says nothing of any IP address), and plain listeners that count the connections they are given.
"""
import os, socket, subprocess, sys, threading, time, re
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import harness as h

def stub(port, *zones):
    log = os.path.join(h.WORK, "dns_stub_pinned.log")
    args = []
    for z in zones:
        args += z
    p = subprocess.Popen([sys.executable, os.path.join(h.HERE, "dns_stub.py"), "--port", str(port)] + args, stdout=open(log, "w"), stderr=subprocess.STDOUT)
    time.sleep(0.6)
    return p, log

def count_queries(log, name):
    with open(log) as f:
        return sum(1 for l in f if l.startswith("Q " + name + " "))

class Counter:
    """A listener that only counts the connections made to it (the 'internal service' that must not be reached)."""
    def __init__(self, ip="127.0.0.1"):
        self.n = 0
        self.s = socket.socket()
        self.s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.s.bind((ip, 0))
        self.s.listen(256)
        self.port = self.s.getsockname()[1]
        self.held = []
        threading.Thread(target=self.run, daemon=True).start()
    def run(self):
        while True:
            try:
                c, _ = self.s.accept()
            except OSError:
                return
            self.n += 1
            self.held.append(c)
    def close(self):
        self.s.close()

def main():
    h.ensure_certs()
    binary = h.build()
    bad = 0
    def check(label, ok, detail=""):
        nonlocal bad
        print("  %-4s %s %s" % ("ok" if ok else "FAIL", label, detail))
        bad += 0 if ok else 1
    dns_port = h.free_port()
    p, log = stub(dns_port,
                  ["--zone", "hooks.test=127.0.0.1", "--zone", "mixed.test=10.0.0.1,93.184.216.34", "--zone", "meta.test=169.254.169.254", "--zone", "mixed2.test=93.184.216.34,10.0.0.1",
                   "--rebind", "flip.test=127.0.0.1,127.0.0.2", "--zone", "slow.test=127.0.0.1", "--delay", "slow.test=300"])
    try:
        with h.Server("good_ec", workers=1, host="0.0.0.0") as s:
            def run(name, n=8, conc=4, allow=1, port=None, verify=1, cafile="ca.pem", verbose=1, host="hooks.test", **kw):
                return h.run_client(binary, port or s.port, ip=name, host=host, total=n, conc=conc, reqs=1, verify=verify, cafile=cafile,
                                    resp_len=s.resp_len, ns=("127.0.0.1", dns_port, allow), verbose=verbose, **kw)
            print("a name that resolves to an allowed address:")
            q0 = count_queries(log, "hooks.test")
            r = run("hooks.test", n=16, conc=8)
            q1 = count_queries(log, "hooks.test")
            check("16 deliveries to hooks.test (allow-private) all succeed with the certificate verified against the NAME", r["outcomes"] == {(0, 0, 200): 16}, str(r["outcomes"]))
            pins = set(re.findall(r"pinned=(\S+)", "\n".join(r["lines"])))
            check("every connection went to the pinned address 127.0.0.1", pins == {"127.0.0.1"}, str(pins))
            check("one lookup per delivery, not two (a check and a connect that each resolve are two)", q1 - q0 == 16, "(%d queries)" % (q1 - q0))
            print("destinations that must be refused before any connection:")
            c = Counter()
            for name, why, detail in (("hooks.test", "127.0.0.1 is loopback", 127 * 16777216 + 1), ("meta.test", "169.254.169.254 is the cloud metadata address", 169 * 16777216 + 254 * 65536 + 169 * 256 + 254),
                                      ("mixed.test", "one of two answers is 10.0.0.1 (the whole answer is refused)", 10 * 16777216 + 1),
                                      ("mixed2.test", "the private one is the second answer: it is the one named", 10 * 16777216 + 1)):
                r = run(name, n=8, conc=4, allow=0, port=c.port, host="hooks.test", cafile="ca.pem")
                got = list(r["outcomes"].items())
                ok = got == [((7, detail, -1), 8)]
                check("%s -> refused, stage 7 with the address (%s)" % (name, why), ok, str(r["outcomes"]))
            time.sleep(0.3)
            check("the receiver behind those names was never connected to", c.n == 0, "(%d connections)" % c.n)
            r = run("hooks.test", n=4, conc=4, allow=1, port=c.port)
            time.sleep(0.5)
            check("control: with allow-private the same name is dialled (the listener sees it)", c.n == 4, "(%d connections; the handshake then fails, it is a counter and not a TLS server)" % c.n)
            c.close()
            print("a name that is not in DNS, and a name server that is not there:")
            r = run("nx.test", n=4, conc=4)
            check("NXDOMAIN is stage 6 with the DNS code -103", list(r["outcomes"]) == [(6, -103, -1)], str(r["outcomes"]))
            r = h.run_client(binary, s.port, ip="hooks.test", host="hooks.test", total=2, conc=2, reqs=1, resp_len=s.resp_len, ns=("127.0.0.1", h.free_port(), 1), verbose=0)
            check("no name server listening is stage 6 (-200: cannot connect to it)", list(r["outcomes"]) == [(6, -200, -1)], str(r["outcomes"]))
            print("rebinding: a name that answers 127.0.0.1 and 127.0.0.2 in turn (receivers on both):")
            r = run("flip.test", n=8, conc=1)
            pins = re.findall(r"pinned=(\S+)", "\n".join(r["lines"]))
            check("every delivery succeeded", r["outcomes"] == {(0, 0, 200): 8}, str(r["outcomes"]))
            check("each used the address its own lookup returned (alternating), and verified the certificate for the name", pins == ["127.0.0.1", "127.0.0.2"] * 4, str(pins))
            print("a lookup is slow and the others go on:")
            t0 = time.time()
            r = run("slow.test", n=64, conc=64, verbose=0)
            check("64 deliveries behind 64 slow (300 ms) lookups finish together, not in turn", r["outcomes"] == {(0, 0, 200): 64} and r["wall"] < 3.0, "(%.2f s, %s)" % (r["wall"], r["outcomes"]))
    finally:
        p.terminate()
    print("FAILED: %d" % bad if bad else "all checks hold")
    return 1 if bad else 0

sys.exit(main())
