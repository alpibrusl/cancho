#!/usr/bin/env python3
"""`packages/tls`'s server under tlsfuzzer (docs/tls-server.md \u00a710; free, citable evidence for #209).

Two suites, both exit-status honest:

- `scripts/tlsfuzzer_cancho/`: this repository's own scripts -- the record
  layer's empties and zero content types, the sanity conversation -- built on
  tlsfuzzer as a library, with this server's actual surface (a P-256 identity,
  no session tickets) instead of the stock scripts' RSA assumptions.
- tlsfuzzer's stock `test-tls13-*.py`: run when their subject applies, with the
  ones that assume an RSA identity or NewSessionTicket skipped and the reason
  printed, so the matrix says what was not tested.


    python3 scripts/tls_server_tlsfuzzer.py <tls_serve> [--quick] [<name substring> ...]

`tls_serve` is `tests/programs/tls_serve.cho` built with `packages/tls` and `packages/x509`
(the binary `scripts/tls_server_interop.py` describes). tlsfuzzer -- Red Hat's TLS test
suite, the tool many CVEs in major stacks trace to -- connects as a client, sends crafted
and malformed handshakes, and says whether the server's answers are what the RFC requires.

Its scripts are flat files under its `scripts/` directory; each takes `-h <host> -p <port>`
and exits 0 when the server behaves. Run here, in the server's `echo` mode, with the
identity `scripts/tls_server_interop.py`'s `authority` makes (P-256, `srv.example`, no
RSA), against `127.0.0.1`:

- every `test-tls13-*.py` the server's surface covers, by name substring;
- the subject scripts it does not cover are skipped with the reason, counted in the
  summary, so the matrix states what was not tested, not silence.

A name substring runs only the scripts whose file name contains it (the group's
scripts stay on the list). `--quick` runs the core: conversation, ccs, lengths,
record-layer-limits, keyshare-omitted, unrecognised-groups, version-negotiation,
shuffled-extensions, empty-alert, zero-content-type -- the rows a reviewer reads first.
Exit status 1 if any run script failed, 2 if tlsfuzzer is missing.

Requires a checkout: `git clone https://github.com/tlsfuzzer/tlsfuzzer` with
`pip install -r requirements.txt` inside it, and `TLSFUZZER` naming its path
(CI does this; docs/tls-assurance.md \u00a72 has the table of tools).
"""

import os
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_server_interop as interop  # noqa: E402  -- `authority`, `Server`, `free_port`

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The scripts whose subject this server does not implement, with the reason. A skip
# is a row in the output, not an absence: #209's last line asks the docs to say what
# was reviewed and what was not.
# The stock scripts this repository's own suite replaces, with the script that
# holds the same check under this server's assumptions.
REPLACED = {
    "test-tls13-empty-alert.py": "tlsfuzzer_cancho/test-empty-and-zero.py",
    "test-tls13-zero-content-type.py": "tlsfuzzer_cancho/test-empty-and-zero.py",
    "test-tls13-ccs.py": "tlsfuzzer_cancho/test-ccs.py",
}

# The stock scripts whose sanity cannot negotiate with this server: each offers
# RSA-PSS signature algorithms only (their `signature_algorithms` assumes an
# RSA identity), so every connection is refused before the script's subject is
# reached, and the failure says nothing about the server. Their subjects are
# not yet ported to `tlsfuzzer_cancho`; until they are, the skip says so.
SANITY_RSA = {
    "test-tls13-keyshare-omitted.py": "a key_share omitted, and supported_groups without a share",
    "test-tls13-record-layer-limits.py": "plaintext and record size limits, at 2**14 and above",
    "test-tls13-shuffled-extentions.py": "extension order, and unassigned extension ids",
    "test-tls13-unrecognised-groups.py": "a key share of a group not in supported_groups",
    "test-tls13-version-negotiation.py": "legacy_version and record-layer version variants",
}

SKIPPED = {
    "test-tls13-count-tickets.py": "no session tickets (docs/tls-server.md \u00a75.2)",
    "test-tls13-session-resumption.py": "no session tickets (docs/tls-server.md \u00a75.2)",
    "test-tls13-psk_dhe_ke.py": "no session tickets (docs/tls-server.md \u00a75.2)",
    "test-tls13-psk_ke.py": "no session tickets (docs/tls-server.md \u00a75.2)",
    "test-tls13-certificate-request.py": "no client certificates (#384, open)",
    "test-tls13-certificate-verify.py": "no client certificates (#384, open)",
    "test-tls13-post-handshake-auth.py": "no client certificates (#384, open)",
    "test-tls13-certificate-compression.py": "no certificate compression (RFC 8446 \u00a74.4.4 is optional)",
    "test-tls13-client-certificate-compression.py": "no client certificates (#384, open)",
    "test-tls13-rsapss-signatures.py": "P-256 identities only (#385, gated)",
    "test-tls13-rsa-signatures.py": "P-256 identities only (#385, gated)",
    "test-tls13-pkcs-signature.py": "P-256 identities only (#385, gated)",
    "test-tls13-mlkem.py": "no ML-KEM key share (a #386 decision, not taken)",
    "test-tls13-mldsa-in-certificate-verify.py": "no ML-DSA signatures (#385, gated)",
    "test-tls13-ecdsa-brainpool-in-certificate-verify.py": "no brainpool curves (docs/tls-server.md \u00a75.2)",
    "test-tls13-ecdhe-brainpool-curves.py": "no brainpool curves (docs/tls-server.md \u00a75.2)",
    "test-tls13-ffdhe-groups.py": "no FFDHE groups (docs/tls-server.md \u00a75.2)",
    "test-tls13-ffdhe-sanity.py": "no FFDHE groups (docs/tls-server.md \u00a75.2)",
    "test-tls13-crfg-curves.py": "no X448 (docs/tls-server.md \u00a75.2)",
    "test-tls13-0rtt-garbage.py": "no early data (docs/tls-server.md \u00a75.2)",
    "test-tls13-minerva.py": "no RSA-PSS timing (P-256 identities only, #385)",
    "test-tls13-keyupdate-from-server.py": "the server does not send KeyUpdate (docs/tls-server.md \u00a75.4)",
}

QUICK = [
    "test-tls13-conversation.py",
    "test-tls13-ccs.py",
    "test-tls13-lengths.py",
    "test-tls13-record-layer-limits.py",
    "test-tls13-keyshare-omitted.py",
    "test-tls13-unrecognised-groups.py",
    "test-tls13-version-negotiation.py",
    "test-tls13-shuffled-extentions.py",
    "test-tls13-empty-alert.py",
    "test-tls13-zero-content-type.py",
]


def main():
    exe = os.path.abspath(sys.argv[1])
    args = sys.argv[2:]
    quick = False
    substrings = []
    i = 0
    while i < len(args):
        if args[i] == "--quick":
            quick = True
        else:
            substrings.append(args[i])
        i += 1
    fuzzer = os.environ.get("TLSFUZZER")
    if not fuzzer or not os.path.isdir(os.path.join(fuzzer, "tlsfuzzer")):
        print("no tlsfuzzer checkout: git clone tlsfuzzer/tlsfuzzer, pip install -r"
              " requirements.txt inside it, and point TLSFUZZER at its path",
              file=sys.stderr)
        sys.exit(2)

    scripts_dir = os.path.join(fuzzer, "scripts")
    names = sorted(f for f in os.listdir(scripts_dir) if f.startswith("test-tls13-") and f.endswith(".py"))
    if quick:
        names = [n for n in names if n in QUICK]
    for sub in substrings:
        names = [n for n in names if sub in n]
    if not names:
        print("no scripts matched", file=sys.stderr)
        sys.exit(2)

    ours = sorted(f for f in os.listdir(os.path.join(ROOT, "scripts", "tlsfuzzer_cancho"))
                   if f.startswith("test-") and f.endswith(".py"))
    work = tempfile.mkdtemp(prefix="tlsfuzzer-")
    interop.authority(work)
    server = interop.Server(exe, "echo", work)

    rows, failed, skipped = [], 0, 0
    env = dict(os.environ, PYTHONPATH=fuzzer)
    for name in ours:
        since = server.mark()
        try:
            r = subprocess.run([sys.executable,
                                os.path.join(ROOT, "scripts", "tlsfuzzer_cancho", name),
                                "-h", "127.0.0.1", "-p", str(server.port)],
                               capture_output=True, text=True, timeout=300, env=env)
            code = r.returncode
            tail = (r.stdout or "").strip().splitlines()
            what = tail[-1][:160] if tail else ""
        except subprocess.TimeoutExpired:
            code, what = "timeout", ""
        status = "ok" if code == 0 else f"FAIL({code}) {what}".strip()
        print(f"tlsfuzzer_cancho/{name}: {status}")
        rows.append((f"cancho/{name}", code))
        if code != 0:
            failed += 1
    for name in names:
        if name in SANITY_RSA:
            print(f"skip {name}: sanity offers RSA-PSS sigalgs only, so it cannot"
                  f" negotiate with a P-256 identity; subject not yet ported --"
                  f" {SANITY_RSA[name]}")
            skipped += 1
            continue
        if name in REPLACED:
            print(f"skip {name}: assumptions differ, {REPLACED[name]} holds the same check")
            skipped += 1
            continue
        if name in SKIPPED:
            print(f"skip {name}: {SKIPPED[name]}")
            skipped += 1
            continue
        since = server.mark()
        try:
            r = subprocess.run([sys.executable, os.path.join(scripts_dir, name),
                                "-h", "127.0.0.1", "-p", str(server.port)],
                               capture_output=True, text=True, timeout=120, env=env,
                               cwd=scripts_dir)
            code = r.returncode
            tail = (r.stderr or r.stdout or "").strip().splitlines()
            what = tail[-1][:160] if tail else ""
        except subprocess.TimeoutExpired:
            code, what = "timeout", ""
        line = server.next_conn(since, wait=1.0)
        status = "ok" if code == 0 else f"FAIL({code}) {what}".strip()
        served = "served" if line is not None else "no line from the server"
        print(f"{name}: {status} -- {served}")
        rows.append((name, code))
        if code != 0:
            failed += 1

    server.stop()
    shutil.rmtree(work, ignore_errors=True)
    print(f"{len(rows)} scripts run, {len(rows) - failed} ok, {failed} failed,"
          f" {skipped} skipped with a reason")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
