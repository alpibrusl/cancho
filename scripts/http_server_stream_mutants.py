#!/usr/bin/env python3
"""Mutation check of `packages/http-server`'s `stream` on a socket server (docs/http-server.md section 14.4), the shape of `scripts/http_server_body_mutants.py --socket`.

    python3 scripts/http_server_stream_mutants.py [--only <text in a mutant's name>]

Each mutant is `packages/http-server/server.cho` with one deliberate bug at one site of the code section 14 added or changed (`stream`, `space`, the `enqueue` of a
socket stream), copied to a scratch directory and named by `HTTP_SERVER_SOURCE`; the three source-built tests of `conformance/http_server_stream.rs` are then run
(`cargo test`). A mutant is killed when one fails or hangs (150 seconds; the three take 15). The unmutated source is run first and must pass. A mutant argued equivalent (`EQUIVALENT`) must survive. Exit status 1 if another mutant
survives, an `old` text does not occur exactly once, or a mutant does not build.
"""
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SERVER = os.path.join(ROOT, "packages/http-server/server.cho")

MUTANTS = [
    ("stream: a socket server answers -1 again", "    if ticket < 0 {\n        return 0 - 1;\n    }\n    let k = ticket % ticket_span();\n    if k >= srv.core.limit {", "    if ticket < 0 || srv.core.fed != 1 {\n        return 0 - 1;\n    }\n    let k = ticket % ticket_span();\n    if k >= srv.core.limit {"),
    ("stream: the connection is not queued for the write", "        if srv.core.fed == 0 {\n            enqueue(srv.core, k);\n        }\n", ""),
    ("stream: the generation of the ticket is not checked", "st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 {\n        return 0 - 1;\n    }\n    var n = srv.core.osize", "st[p + 25] != 0 {\n        return 0 - 1;\n    }\n    var n = srv.core.osize"),
    ("stream: a request that is not held takes bytes", "st[p + 4] != 1 || st[p + 13] != 1 || st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 {\n        return 0 - 1;\n    }\n    var n = srv.core.osize", "st[p + 4] != 1 || st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 {\n        return 0 - 1;\n    }\n    var n = srv.core.osize"),
    ("stream: a refused connection takes bytes", "st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 {\n        return 0 - 1;\n    }\n    var n = srv.core.osize", "st[p + 14] != ticket / ticket_span() {\n        return 0 - 1;\n    }\n    var n = srv.core.osize"),
    ("stream: the room is not the limit", "    if len(bytes) < n {\n        n = len(bytes);\n    }\n    let pd = contents(srv.core.pends);", "    n = len(bytes);\n    let pd = contents(srv.core.pends);"),
    ("stream: the bytes are not counted as waiting", "    st[p + 2] = st[p + 2] + n;\n    if n > 0 {\n        set_flag(st, p, 4);", "    if n > 0 {\n        set_flag(st, p, 4);"),
    ("stream: the bytes are put at the start, not after what is waiting", "    let at = k * srv.core.osize + st[p + 2];", "    let at = k * srv.core.osize;"),
    ("space: a socket server has no room", "    if k < 0 || k >= srv.core.limit || contents(srv.core.state)[stride() * k + 4] != 1 {", "    if srv.core.fed != 1 || k < 0 || k >= srv.core.limit || contents(srv.core.state)[stride() * k + 4] != 1 {"),
    ("space: a free slot has room", "    if k < 0 || k >= srv.core.limit || contents(srv.core.state)[stride() * k + 4] != 1 {", "    if k < 0 || k >= srv.core.limit {"),
    ("space: the waiting bytes are not taken off", "    return srv.core.osize - contents(srv.core.state)[stride() * k + 2];\n}\n\n// What the server would have written", "    return srv.core.osize;\n}\n\n// What the server would have written"),
]


# A mutant that changes nothing the application can reach, and must survive. The argument is the point.
EQUIVALENT = {
    "stream: a refused connection takes bytes": "`record` (the stall timer, in `wait`) queues the connection and the same `wait` visits it: `step` runs `reject`, which clears the held flag and ends "
    "the request before `wait` returns, so no `stream` call can see a recorded refusal on a still-held request; the check is the same defensive one `answer_in` makes",
}


def run(source):
    env = dict(os.environ, HTTP_SERVER_SOURCE=source)
    try:
        r = subprocess.run(["cargo", "test", "-p", "cancho", "--test", "conformance", "http_server_stream::", "--", "--skip", "stream_through_the_store"], cwd=ROOT, env=env, capture_output=True, text=True, timeout=150)
    except subprocess.TimeoutExpired:
        return "hang"
    if "error[" in r.stderr or "could not compile" in r.stderr:
        return "build"
    return "pass" if r.returncode == 0 else "fail"


def main():
    only = sys.argv[sys.argv.index("--only") + 1] if "--only" in sys.argv else None
    text = open(SERVER).read()
    tmp = tempfile.mkdtemp()
    base = os.path.join(tmp, "server.cho")
    open(base, "w").write(text)
    if run(base) != "pass":
        raise SystemExit("the unmutated source does not pass")
    bad = 0
    for name, old, new in MUTANTS:
        if only and only not in name:
            continue
        if text.count(old) != 1:
            print(f"BAD   {name}: the text to replace occurs {text.count(old)} times")
            bad += 1
            continue
        path = os.path.join(tmp, "mutant.cho")
        open(path, "w").write(text.replace(old, new, 1))
        got = run(path)
        if got in ("fail", "hang"):
            print(f"killed   {name} ({got})")
        elif got == "build":
            print(f"BUILD    {name}: the mutant does not build")
            bad += 1
        elif name in EQUIVALENT:
            print(f"equivalent {name}: {EQUIVALENT[name]}")
        else:
            print(f"SURVIVED {name}")
            bad += 1
    print("all killed" if not bad else f"{bad} not killed")
    sys.exit(1 if bad else 0)


main()
