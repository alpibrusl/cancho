#!/usr/bin/env python3
"""Mutation fuzzing of `packages/tls`'s client (docs/tls-parity.md §3.4).

    python3 scripts/tls_fuzz.py <driver> [<runs>]
    python3 scripts/tls_fuzz.py --server <server driver> [<runs>]

`driver` is `tests/programs/tls_driver.cho` built with the package's files. Each
run takes one of the recorded handshakes in `tests/vectors/tls/` (TLS 1.3
against tlslite-ng, TLS 1.2 against OpenSSL), mutates the bytes the server sent
in one of its `F` lines -- a bit flipped, bytes set at random, the data cut
short, a slice duplicated or dropped, random bytes inserted, or a record or
handshake length changed -- and replays it. The client must answer every line
and never trap: a refusal is the expected end, a crash is the failure. Runs go
100 to a driver process; a process that exits non-zero, or answers fewer lines
than it was given, is re-run one connection at a time to name the input.
Exit status 1 on any trap.

With `--server` (docs/tls-server.md §7), the driver is `tests/programs/tls_server_driver.cho` and the connections are
the honest ones of `tests/vectors/tls/liar_client.txt` (`scripts/tls_liar_client.py`: the 26 honest connections and, with
session tickets, the 55 ticket cases that end `ok`, each of several connections on one engine) and of `liar_client_auth.txt`
(`scripts/tls_liar_client_auth.py`: the client-certificate ones): what is mutated is a line of
the client's bytes, its ClientHello most of all, and the server must answer every line and never trap. Each
connection starts by dropping the driver's slot, so a batch of them runs in one process.
"""
import glob
import os
import random
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def traces():
    out = []
    for path in sorted(glob.glob(os.path.join(ROOT, "tests/vectors/tls/*_*.txt"))):
        name = os.path.basename(path)
        if not (name.startswith("tlslite_") or name.startswith("openssl12_")):
            continue
        asked = [l.rstrip("\n") for l in open(path) if l[:2] in ("C ", "F ", "W ", "Q")]
        out.append((name, asked))
    return out


def server_traces():
    out, current = [], None
    # The lying client's honest connections, its ticket cases, and the client-certificate ones (docs/tls-server.md §13.11): the
    # Certificate, the CertificateVerify and the Finished are lines of the client's bytes to mutate, like the ClientHello.
    for name in ("liar_client.txt", "liar_client_auth.txt"):
        for line in open(os.path.join(ROOT, "tests/vectors/tls", name)):
            line = line.rstrip("\n")
            if line.startswith("## "):
                current = [] if line.startswith(("## ok honest", "## ok tickets")) else None
                if current is not None:
                    out.append((line[3:], current))
            elif current is not None and not line.startswith("#") and not line.startswith("= "):
                current.append(line)
    # Each connection begins by freeing the slot the last one used.
    return [(name, ["D"] + asked) for name, asked in out]


def mutate(rng, data):
    b = bytearray(data)
    kind = rng.randrange(7)
    if kind == 0:
        for _ in range(rng.randint(1, 4)):
            b[rng.randrange(len(b))] ^= 1 << rng.randrange(8)
    elif kind == 1:
        for _ in range(rng.randint(1, 8)):
            b[rng.randrange(len(b))] = rng.randrange(256)
    elif kind == 2:
        b = b[:rng.randrange(len(b))]
    elif kind == 3:
        i, j = sorted(rng.sample(range(len(b) + 1), 2))
        b = b[:j] + b[i:j] + b[j:]
    elif kind == 4:
        i, j = sorted(rng.sample(range(len(b) + 1), 2))
        b = b[:i] + b[j:]
    elif kind == 5:
        i = rng.randrange(len(b) + 1)
        b = b[:i] + bytes(rng.randrange(256) for _ in range(rng.randint(1, 64))) + b[i:]
    else:
        # A length: a record header's, or a handshake message's, set at random.
        i = rng.randrange(min(len(b), 64))
        b[i] = rng.choice([0, 1, 0x7F, 0x80, 0xFF])
    return bytes(b)


def one_run(rng, trace):
    name, asked = trace
    lines = list(asked)
    fs = [i for i, l in enumerate(lines) if l.startswith("F ")]
    at = rng.choice(fs)
    data = bytes.fromhex(lines[at][2:])
    lines[at] = "F " + (mutate(rng, data).hex() or "")
    if lines[at] == "F ":
        lines[at] = "F 00"
    # After the mutated line, the rest of the connection as recorded.
    return name, lines


def drive(driver, lines):
    r = subprocess.run([driver], input="\n".join(lines) + "\n", capture_output=True, text=True, timeout=600)
    return r.returncode, len(r.stdout.splitlines())


def main():
    server = sys.argv[1] == "--server"
    args = sys.argv[2:] if server else sys.argv[1:]
    driver = args[0]
    runs = int(args[1]) if len(args) > 1 else 2000
    rng = random.Random(197)
    all_traces = server_traces() if server else traces()
    batch, traps, done = [], [], 0
    while done < runs:
        batch = [one_run(rng, rng.choice(all_traces)) for _ in range(min(100, runs - done))]
        lines = [l for _, ls in batch for l in ls]
        code, answered = drive(driver, lines)
        if code != 0 or answered != len(lines):
            for name, ls in batch:
                c, a = drive(driver, ls)
                if c != 0 or a != len(ls):
                    traps.append((name, c, ls))
        done += len(batch)
    print(f"{runs} mutated connections over {len(all_traces)} recorded handshakes, {len(traps)} traps")
    for name, c, ls in traps[:5]:
        print(f"TRAP {name}: exit {c}; " + " | ".join(l[:60] for l in ls))
    sys.exit(1 if traps else 0)


if __name__ == "__main__":
    main()
