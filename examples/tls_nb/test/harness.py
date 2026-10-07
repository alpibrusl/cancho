"""Shared helpers for the tls_nb tests and measurements: certificates, a test receiver, and the client with its CPU cost.

Nothing here is specific to one experiment; every script in this directory imports it.
"""
import os, re, signal, socket, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
EXAMPLE = os.path.dirname(HERE)
ROOT = os.path.dirname(os.path.dirname(EXAMPLE))
CANCHO = os.environ.get("CANCHO", "/home/user/cancho/target/release/cancho")
WORK = os.environ.get("TLS_NB_WORK", os.path.join(os.environ.get("TMPDIR", "/tmp"), "tls_nb_work"))
CERTS = os.path.join(WORK, "certs")


def ensure_certs():
    if not os.path.exists(os.path.join(CERTS, "ca.pem")):
        os.makedirs(WORK, exist_ok=True)
        subprocess.run([os.path.join(HERE, "certs.sh"), CERTS], check=True, stdout=subprocess.DEVNULL)
    return CERTS


def build(name="tls_nb", sources=("tls.cho", "dns.cho", "rtcp.cho", "pin.cho", "nat.cho", "tls_nb.cho"), extra=()):
    """Compile the client with the checked-in compiler; answers the binary's path."""
    os.makedirs(WORK, exist_ok=True)
    out = os.path.join(WORK, name)
    cmd = [CANCHO, "build"] + [os.path.join(EXAMPLE, s) for s in sources] + ["--std", "-l", "ssl", "-l", "crypto", "-o", out] + list(extra)
    subprocess.run(cmd, check=True)
    return out


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


class Server:
    """The test receiver (server.py) as a context manager. `cert` is a name in the certs directory (`good_ec`)."""

    def __init__(self, cert, chain=None, workers=1, port=None, cpus=None, **opts):
        self.port = port or free_port()
        c = ensure_certs()
        cmd = (["taskset", "-c", cpus] if cpus else []) + [sys.executable, os.path.join(HERE, "server.py"), "--port", str(self.port),
               "--cert", os.path.join(c, cert + ".pem"), "--key", os.path.join(c, cert + ".key"), "--workers", str(workers)]
        if chain:
            cmd += ["--chain", os.path.join(c, chain)]
        for k, v in opts.items():
            if v is True:
                cmd.append("--" + k.replace("_", "-"))
            elif v is not None and v is not False:
                cmd += ["--" + k.replace("_", "-"), str(v)]
        self.p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.resp_len = None
        for line in self.p.stdout:
            if line.startswith("resp_len="):
                self.resp_len = int(line.split("=")[1])
            if line.startswith("listening"):
                break
        time.sleep(0.15)

    def __enter__(self):
        return self

    def __exit__(self, *a):
        self.p.terminate()
        try:
            self.p.wait(5)
        except subprocess.TimeoutExpired:
            self.p.kill()


def run_client(binary, port, host="hooks.test", total=1, conc=1, reqs=1, verify=1, cafile="ca.pem", default_paths=0, body=100,
               resp_len=40, hold_ms=0, release_buffers=0, verbose=0, io=0, deadline_ms=15000, resume=0, ns=None, sigpipe=False, ip="127.0.0.1", prefix=(), on_held=None, timeout=300, extra_env=None):
    """Run the client; answers a dict with the parsed outcomes, summary, session line, CPU and peak memory of the process."""
    ca = "-" if cafile in (None, "-") else (cafile if os.path.isabs(cafile) else os.path.join(ensure_certs(), cafile))
    cmd = list(prefix) + [binary, ip, str(port), host, str(total), str(conc), str(reqs), str(verify), ca, str(default_paths),
                          str(body), str(resp_len), str(hold_ms), str(release_buffers), str(verbose)]
    cmd += ["io=%d" % io, "deadline=%d" % deadline_ms, "resume=%d" % resume]
    if ns:
        cmd += ["ns=%s" % ns[0], "ns-port=%d" % ns[1], "allow-private=%d" % ns[2]]
    if sigpipe:
        cmd.append("sigpipe=ignore")
    env = dict(os.environ)
    if extra_env:
        env.update(extra_env)
    t0 = time.time()
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
    import threading as _t
    _timer = _t.Timer(timeout, lambda: p.kill())
    _timer.daemon = True
    _timer.start()
    held_rss = None
    start_rss = None
    held_threads = None
    held_fds = None
    # standard error carries the markers (standard output is block-buffered by the program when it is a pipe)
    import threading
    out_chunks = []
    t_out = threading.Thread(target=lambda: out_chunks.append(p.stdout.read()))
    t_out.start()
    err_lines = []
    for line in p.stderr:
        err_lines.append(line.rstrip("\n"))
        if line.startswith("READY"):
            start_rss = rss_kb(p.pid)
        if line.startswith("HELD"):
            held_rss = rss_kb(p.pid)
            held_threads = threads(p.pid)
            held_fds = fd_count(p.pid)
            if on_held:
                on_held(p.pid)
    _timer.cancel()
    err = "\n".join(err_lines)
    t_out.join()
    lines = "".join(out_chunks).splitlines()
    _, status, ru = os.wait4(p.pid, 0)
    wall = time.time() - t0
    res = {"cmd": cmd, "exit": os.waitstatus_to_exitcode(status), "stderr": err, "wall": wall, "user": ru.ru_utime, "sys": ru.ru_stime,
           "maxrss_kb": ru.ru_maxrss, "start_rss_kb": start_rss, "held_rss_kb": held_rss, "held_threads": held_threads, "held_fds": held_fds, "outcomes": {}, "summary": {}, "session": "", "lines": lines}
    for l in lines:
        if l.startswith("outcome "):
            kv = dict(x.split("=") for x in l.split()[1:])
            res["outcomes"][(int(kv["stage"]), int(kv["detail"]), int(kv["status"]))] = int(kv["count"])
        elif l.startswith("summary "):
            res["summary"] = {k: int(v) for k, v in (x.split("=") for x in l.split()[1:])}
        elif l.startswith("session "):
            res["session"] = l[len("session "):].strip()
    return res


def rss_kb(pid):
    try:
        with open("/proc/%d/status" % pid) as f:
            for l in f:
                if l.startswith("VmRSS:"):
                    return int(l.split()[1])
    except OSError:
        return None


def fd_count(pid):
    try:
        return len(os.listdir("/proc/%d/fd" % pid))
    except OSError:
        return None


def threads(pid):
    try:
        with open("/proc/%d/status" % pid) as f:
            for l in f:
                if l.startswith("Threads:"):
                    return int(l.split()[1])
    except OSError:
        return None
