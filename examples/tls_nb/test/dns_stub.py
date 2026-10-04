#!/usr/bin/env python3
"""A DNS server for the resolver tests: UDP and TCP on one port, A records from the command line, delays and rebinding on demand.

    dns_stub.py --port 5300 --zone hooks.test=127.0.0.1 --zone many.test=10.0.0.1,93.184.216.34 \
                --delay slow.test=300 --rebind flip.test=93.184.216.34,169.254.169.254 [--ttl 60] [--default-delay 0]

  --zone NAME=IP[,IP...]   NAME has these A records (`*.suffix` is a wildcard for the suffix)
  --delay NAME=MS          answer for NAME after this many milliseconds (the default delay otherwise)
  --rebind NAME=IP,IP,...  the n-th query for NAME is answered with the n-th address, cycling: DNS rebinding in one flag
  Anything else is NXDOMAIN. One line per query on standard output, `Q <name> <udp|tcp>`, flushed, so a test can count them.
"""
import argparse, socket, struct, sys, threading, time

LOG_LOCK = threading.Lock()

def parse_name(msg, at):
    labels = []
    while True:
        n = msg[at]
        if n == 0:
            return ".".join(labels).lower(), at + 1
        labels.append(msg[at + 1:at + 1 + n].decode("ascii", "replace"))
        at += 1 + n

class Zone:
    def __init__(self, a):
        self.ttl = a.ttl
        self.default_delay = a.default_delay / 1000.0
        self.zone = {}
        self.delay = {}
        self.rebind = {}
        self.counts = {}
        self.lock = threading.Lock()
        for z in a.zone:
            n, v = z.split("=")
            self.zone[n.lower()] = v.split(",")
        for d in a.delay:
            n, v = d.split("=")
            self.delay[n.lower()] = int(v) / 1000.0
        for r in a.rebind:
            n, v = r.split("=")
            self.rebind[n.lower()] = v.split(",")

    def addrs(self, name):
        if name in self.rebind:
            with self.lock:
                k = self.counts.get(name, 0)
                self.counts[name] = k + 1
            lst = self.rebind[name]
            return [lst[k % len(lst)]]
        if name in self.zone:
            return self.zone[name]
        for pat, v in self.zone.items():
            if pat.startswith("*.") and name.endswith(pat[1:]):
                return v
        return None

    def answer(self, q, proto):
        """The response to the query message `q`, and the delay to wait before sending it."""
        qid = q[0:2]
        name, at = parse_name(q, 12)
        qtype, qclass = struct.unpack(">HH", q[at:at + 4])
        question = q[12:at + 4]
        with LOG_LOCK:
            sys.stdout.write("Q %s %s\n" % (name, proto))
            sys.stdout.flush()
        delay = self.delay.get(name, self.default_delay)
        if qtype != 1:
            return qid + struct.pack(">HHHHH", 0x8180, 1, 0, 0, 0) + question, delay
        addrs = self.addrs(name)
        if addrs is None:
            return qid + struct.pack(">HHHHH", 0x8183, 1, 0, 0, 0) + question, delay
        out = qid + struct.pack(">HHHHH", 0x8180, 1, len(addrs) + 1, 0, 0) + question
        # one CNAME first, to make the parser follow a chain: `name` is an alias of itself's pointer (a pointer to the question)
        out += struct.pack(">HHHIH", 0xC00C, 5, 1, self.ttl, 2) + struct.pack(">H", 0xC00C)
        for ip in addrs:
            out += struct.pack(">HHHIH", 0xC00C, 1, 1, self.ttl, 4) + socket.inet_aton(ip)
        return out, delay

def serve_udp(zone, host, port):
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind((host, port))
    def reply(data, peer):
        try:
            out, delay = zone.answer(data, "udp")
            if delay:
                time.sleep(delay)
            s.sendto(out, peer)
        except Exception as e:
            print("udp error", e, file=sys.stderr, flush=True)
    while True:
        data, peer = s.recvfrom(4096)
        threading.Thread(target=reply, args=(data, peer), daemon=True).start()

def serve_tcp(zone, host, port):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind((host, port))
    s.listen(256)
    def handle(c):
        try:
            while True:
                hdr = c.recv(2, socket.MSG_WAITALL)
                if len(hdr) < 2:
                    return
                n = struct.unpack(">H", hdr)[0]
                q = c.recv(n, socket.MSG_WAITALL)
                out, delay = zone.answer(q, "tcp")
                if delay:
                    time.sleep(delay)
                c.sendall(struct.pack(">H", len(out)) + out)
        except Exception:
            pass
        finally:
            c.close()
    while True:
        c, _ = s.accept()
        threading.Thread(target=handle, args=(c,), daemon=True).start()

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, required=True)
    p.add_argument("--zone", action="append", default=[])
    p.add_argument("--delay", action="append", default=[])
    p.add_argument("--rebind", action="append", default=[])
    p.add_argument("--ttl", type=int, default=60)
    p.add_argument("--default-delay", type=int, default=0)
    a = p.parse_args()
    z = Zone(a)
    threading.Thread(target=serve_udp, args=(z, a.host, a.port), daemon=True).start()
    threading.Thread(target=serve_tcp, args=(z, a.host, a.port), daemon=True).start()
    print("listening", flush=True)
    while True:
        time.sleep(3600)

main()
