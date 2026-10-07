#!/usr/bin/env python3
"""Mutation check of `packages/http-server`'s byte-fed mode (docs/http-server.md §11.5), the shape of `scripts/tls_server_mutants.py`.

    python3 scripts/http_server_bytes_mutants.py <cancho binary> [--only <text in a mutant's name>] [--jobs <n>]

Each mutant is `packages/http-server/server.cho` with one deliberate bug at one site, in the code the byte-fed mode added
or changed (`open_bytes`, `attach`, `room`, `input`, `end_input`, `ready`, `output`, `closing`, `detach`, `stream`, and
the shared parts they reach: `emit`, `shut`, `sweep_idle`, `enqueue`, `begin_round`, `advance`, `answer_in`). It is
copied to a scratch directory and `tests/packages/http_server_bytes_test.cho` is run against it (`cancho test`). A mutant
is killed when a test fails or traps. The unmutated source is run first and must pass. A mutant that changes nothing the
tests can reach is in EQUIVALENT with the argument, and must survive. Exit status 1 if a mutant survives, an
`old` text does not occur exactly once, or a mutant fails to build (a mutant that does not compile proves nothing). A run that hangs for 45 seconds (the tests take 3) counts as killed.

The socket path's own tests (`conformance/api.rs`, `http_server.rs`) are not run here: they need a listener and take
minutes. They run, unchanged, with the rest of `cargo test`.
"""
import concurrent.futures
import os
import signal
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SERVER = os.path.join(ROOT, "packages/http-server/server.cho")
TESTS = os.path.join(ROOT, "tests/packages/http_server_bytes_test.cho")

# (name, the text replaced, its replacement). Each `old` must occur exactly once.
MUTANTS = [
    # ---- open_bytes ----
    ("open_bytes: a size of 0 accepted", "    if size < 1 || out < least_output() || max < 1 {", "    if size < 0 || out < least_output() || max < 1 {"),
    ("open_bytes: an output room under 512 accepted", "    if size < 1 || out < least_output() || max < 1 {", "    if size < 1 || out < 0 || max < 1 {"),
    ("open_bytes: no connections accepted", "    if size < 1 || out < least_output() || max < 1 {", "    if size < 1 || out < least_output() || max < 0 {"),
    ("open_bytes: the limit is the larger of the two", "            if max < limit {\n                limit = max;", "            if max > limit {\n                limit = max;"),
    ("open_bytes: the output room is the socket server's", "fed: 1, osize: out, nlive: 0, scan: 0 };", "fed: 1, osize: 65536, nlive: 0, scan: 0 };"),
    ("open_bytes: not marked byte-fed", "fed: 1, osize: out, nlive: 0, scan: 0 };", "fed: 0, osize: out, nlive: 0, scan: 0 };"),
    # ---- attach ----
    ("attach: one connection too many", "    if srv.core.fed != 1 || srv.core.nlive >= srv.core.limit {", "    if srv.core.fed != 1 || srv.core.nlive > srv.core.limit {"),
    ("attach: an occupied slot taken", "    while tried < srv.core.limit && st[stride() * k + 4] != 0 {", "    while tried < srv.core.limit && st[stride() * k + 4] == 7 {"),
    ("attach: the slot is not initialised", "    init_slot(srv.core, k, now_ms / 1000);\n    srv.core.scan", "    srv.core.scan"),
    ("attach: the scan does not move on", "    srv.core.scan = (k + 1) % srv.core.limit;\n    srv.core.nlive = srv.core.nlive + 1;", "    srv.core.scan = k;\n    srv.core.nlive = srv.core.nlive + 1;"),
    ("attach: the count not kept", "    srv.core.nlive = srv.core.nlive + 1;\n    return k;", "    return k;"),
    ("attach: the time in milliseconds taken for seconds", "    init_slot(srv.core, k, now_ms / 1000);", "    init_slot(srv.core, k, now_ms);"),
    ("init_slot: the peer's end remembered by the next connection", "    st[p + 14] = st[p + 14] + 1;\n    st[p + 15] = 0;", "    st[p + 14] = st[p + 14] + 1;"),
    ("init_slot: the generation not increased", "    st[p + 14] = st[p + 14] + 1;\n    st[p + 15] = 0;", "    st[p + 15] = 0;"),
    ("init_slot: bytes buffered by the last connection kept", "    let p = stride() * slot;\n    st[p] = 0;\n    st[p + 1] = now;", "    let p = stride() * slot;\n    st[p + 1] = now;"),
    ("init_slot: the closing flag kept", "    st[p + 2] = 0;\n    st[p + 3] = 0;\n    st[p + 4] = 1;", "    st[p + 2] = 0;\n    st[p + 4] = 1;"),
    # ---- room and input ----
    ("room: answers waiting do not stop input", "    if st[p + 2] > 0 || st[p + 3] == 1 || st[p + 15] == 1 {", "    if st[p + 3] == 1 || st[p + 15] == 1 {"),
    ("room: a closing connection takes input", "    if st[p + 2] > 0 || st[p + 3] == 1 || st[p + 15] == 1 {", "    if st[p + 2] > 0 || st[p + 15] == 1 {"),
    ("room: an ended connection takes input", "    if st[p + 2] > 0 || st[p + 3] == 1 || st[p + 15] == 1 {", "    if st[p + 2] > 0 || st[p + 3] == 1 {"),
    ("room: the whole buffer, whatever it holds", "    return srv.core.size - st[p];\n}\n\n// Bytes the peer sent", "    return srv.core.size;\n}\n\n// Bytes the peer sent"),
    ("input: more than was given", "    if len(data) < n {\n        n = len(data);\n    }\n    if n > 0 {\n        let st = contents(srv.core.state);\n        let bf", "    if n > 0 {\n        let st = contents(srv.core.state);\n        let bf"),
    ("input: the connection not queued", "        st[p + 1] = now_ms / 1000;\n        enqueue(srv.core, k);\n    }\n    return n;", "        st[p + 1] = now_ms / 1000;\n    }\n    return n;"),
    ("input: bytes written over the buffered ones", "        let at = k * srv.core.size + st[p];\n        var i = 0;\n        while i < n {\n            bf[at + i] = data[i];", "        let at = k * srv.core.size;\n        var i = 0;\n        while i < n {\n            bf[at + i] = data[i];"),
    ("input: the buffered count not advanced", "        st[p] = st[p] + n;\n        st[p + 1] = now_ms / 1000;", "        st[p + 1] = now_ms / 1000;"),
    ("input: not counted as progress", "        st[p] = st[p] + n;\n        st[p + 1] = now_ms / 1000;", "        st[p] = st[p] + n;"),
    ("input: the time in milliseconds taken for seconds", "        st[p] = st[p] + n;\n        st[p + 1] = now_ms / 1000;", "        st[p] = st[p] + n;\n        st[p + 1] = now_ms;"),
    ("input: a dead connection answers 0, not -1", "    if !live_slot(srv.core, k) {\n        return 0 - 1;\n    }\n    var n = room(srv, k);", "    if !live_slot(srv.core, k) {\n        return 0;\n    }\n    var n = room(srv, k);"),
    ("live_slot: one slot past the end", "    if core.fed != 1 || k < 0 || k >= core.limit {\n        return false;\n    }\n    return contents(core.state)", "    if core.fed != 1 || k < 0 || k > core.limit {\n        return false;\n    }\n    return contents(core.state)"),
    ("live_slot: an ended connection is live", "    return contents(core.state)[stride() * k + 4] == 1;\n}\n\n// A new connection", "    return contents(core.state)[stride() * k + 4] != 0;\n}\n\n// A new connection"),
    # ---- the peer's end ----
    ("end_input: the end not recorded", "    contents(srv.core.state)[stride() * k + 15] = 1;\n    enqueue(srv.core, k);", "    enqueue(srv.core, k);"),
    ("end_input: not queued", "    contents(srv.core.state)[stride() * k + 15] = 1;\n    enqueue(srv.core, k);", "    contents(srv.core.state)[stride() * k + 15] = 1;"),
    ("advance: an ended connection with an answer waiting closes at once", "                if st[q + 15] == 1 && st[q + 2] == 0 && st[q + 13] == 0 {", "                if st[q + 15] == 1 && st[q + 13] == 0 {"),
    ("advance: an ended connection with a held request closes", "                if st[q + 15] == 1 && st[q + 2] == 0 && st[q + 13] == 0 {", "                if st[q + 15] == 1 && st[q + 2] == 0 {"),
    ("advance: an ended connection never closes", "                if st[q + 15] == 1 && st[q + 2] == 0 && st[q + 13] == 0 {", "                if st[q + 15] == 7 && st[q + 2] == 0 && st[q + 13] == 0 {"),
    ("begin_round: an ended connection not visited again", "        if st[p + 4] == 1 && (st[p] > 0 || st[p + 15] == 1) && st[p + 2] == 0 && st[p + 3] == 0 {\n            enqueue(core, half);", "        if st[p + 4] == 1 && st[p] > 0 && st[p + 2] == 0 && st[p + 3] == 0 {\n            enqueue(core, half);"),
    ("answer_in: an ended connection not visited again", "    if st[p + 4] == 1 && (st[p] > 0 || st[p + 15] == 1) && st[p + 2] == 0 && st[p + 3] == 0 && st[p + 11] == 0 {", "    if st[p + 4] == 1 && st[p] > 0 && st[p + 2] == 0 && st[p + 3] == 0 && st[p + 11] == 0 {"),
    ("answer_in: requests buffered behind an answer not visited again", "    if st[p + 4] == 1 && (st[p] > 0 || st[p + 15] == 1) && st[p + 2] == 0 && st[p + 3] == 0 && st[p + 11] == 0 {", "    if st[p + 4] == 1 && st[p + 15] == 1 && st[p + 2] == 0 && st[p + 3] == 0 && st[p + 11] == 0 {"),
    # ---- output ----
    ("output: nothing taken from a dead connection is not 0", "    if !live_slot(srv.core, k) {\n        return 0;\n    }\n    let st = contents(srv.core.state);\n    let pd = contents(srv.core.pends);\n    let p = stride() * k;\n    let base = k * srv.core.osize;", "    if !live_slot(srv.core, k) {\n        return 0 - 1;\n    }\n    let st = contents(srv.core.state);\n    let pd = contents(srv.core.pends);\n    let p = stride() * k;\n    let base = k * srv.core.osize;"),
    ("output: more than `out` can hold", "    var n = st[p + 2];\n    if len(out) < n {\n        n = len(out);\n    }\n    if n == 0 {", "    var n = st[p + 2];\n    if n == 0 {"),
    ("output: what is left not moved to the front", "    i = 0;\n    while i < st[p + 2] - n {\n        pd[base + i] = pd[base + n + i];\n        i = i + 1;\n    }\n    st[p + 2] = st[p + 2] - n;", "    st[p + 2] = st[p + 2] - n;"),
    ("output: the count waiting not reduced", "    st[p + 2] = st[p + 2] - n;\n    st[p + 1] = now_ms / 1000;", "    st[p + 1] = now_ms / 1000;"),
    ("output: not counted as progress", "    st[p + 2] = st[p + 2] - n;\n    st[p + 1] = now_ms / 1000;", "    st[p + 2] = st[p + 2] - n;"),
    ("output: the time in milliseconds taken for seconds", "    st[p + 2] = st[p + 2] - n;\n    st[p + 1] = now_ms / 1000;", "    st[p + 2] = st[p + 2] - n;\n    st[p + 1] = now_ms;"),
    ("output: a connection that is to close does not", "        if st[p + 3] == 1 {\n            shut(srv.tab, srv.core, k);\n        } else if", "        if st[p + 3] == 7 {\n            shut(srv.tab, srv.core, k);\n        } else if"),
    ("output: requests waiting behind the answer not visited", "        } else if st[p] > 0 || st[p + 15] == 1 {\n            // Held back while the answer was being sent.\n            enqueue(srv.core, k);", "        } else if st[p + 15] == 1 {\n            // Held back while the answer was being sent.\n            enqueue(srv.core, k);"),
    ("output: an ended connection not visited when its answer has gone", "        } else if st[p] > 0 || st[p + 15] == 1 {\n            // Held back while the answer was being sent.\n            enqueue(srv.core, k);", "        } else if st[p] > 0 {\n            // Held back while the answer was being sent.\n            enqueue(srv.core, k);"),
    ("output: the connection ends before the answer has gone", "    st[p + 2] = st[p + 2] - n;\n    st[p + 1] = now_ms / 1000;\n    if st[p + 2] == 0 {", "    st[p + 2] = st[p + 2] - n;\n    st[p + 1] = now_ms / 1000;\n    if st[p + 2] >= 0 {"),
    ("pending: the wrong field", "    return contents(srv.core.state)[stride() * k + 2];\n}\n\n// How many more bytes", "    return contents(srv.core.state)[stride() * k + 1];\n}\n\n// How many more bytes"),
    ("space: the output room whatever is waiting", "    return srv.core.osize - contents(srv.core.state)[stride() * k + 2];", "    return srv.core.osize;"),
    # ---- ending, and freeing ----
    ("closing: a connection in use is closing", "    return contents(srv.core.state)[stride() * k + 4] == 2;", "    return contents(srv.core.state)[stride() * k + 4] != 0;"),
    ("closing: never", "    return contents(srv.core.state)[stride() * k + 4] == 2;", "    return contents(srv.core.state)[stride() * k + 4] == 7;"),
    ("shut: the slot freed, not ended", "        st[stride() * k + 2] = 0;\n        st[stride() * k + 4] = 2;", "        st[stride() * k + 2] = 0;\n        st[stride() * k + 4] = 0;"),
    ("shut: what was waiting to be sent kept", "        st[stride() * k + 2] = 0;\n        st[stride() * k + 4] = 2;", "        st[stride() * k + 4] = 2;"),
    ("shut: a held request stays held", "        st[stride() * k + 12] = 0;\n        st[stride() * k + 13] = 0;\n        return 0;\n    }\n    conns.close", "        st[stride() * k + 12] = 0;\n        return 0;\n    }\n    conns.close"),
    ("detach: the queue not cleaned", "    if st[p + 12] == 1 {\n        // Out of the queue", "    if st[p + 12] == 7 {\n        // Out of the queue"),
    ("detach: the request in hand kept", "    if srv.core.cur == k {\n        srv.core.cur = 0 - 1;\n    }", "    if srv.core.cur == 99 {\n        srv.core.cur = 0 - 1;\n    }"),
    ("detach: the count not kept", "    st[p + 13] = 0;\n    srv.core.nlive = srv.core.nlive - 1;\n    return 0;", "    st[p + 13] = 0;\n    return 0;"),
    ("detach: a slot not in use detached again", "    if st[p + 4] == 0 {\n        return 0 - 1;\n    }\n    if srv.core.cur == k {", "    if srv.core.cur == k {"),
    ("detach: buffered input kept for the next connection", "    st[p] = 0;\n    st[p + 2] = 0;\n    st[p + 4] = 0;\n    st[p + 12] = 0;", "    st[p + 2] = 0;\n    st[p + 4] = 0;\n    st[p + 12] = 0;"),
    ("detach: the slot not freed", "    st[p + 2] = 0;\n    st[p + 4] = 0;\n    st[p + 12] = 0;\n    st[p + 13] = 0;\n    srv.core.nlive", "    st[p + 2] = 0;\n    st[p + 12] = 0;\n    st[p + 13] = 0;\n    srv.core.nlive"),
    ("detach: the cursor not moved back past a removed entry", "            } else if from < srv.core.cursor {\n                srv.core.cursor = srv.core.cursor - 1;\n            }", "            } else if from < 0 {\n                srv.core.cursor = srv.core.cursor - 1;\n            }"),
    ("detach: a ticket outlives the connection", "    st[p + 13] = 0;\n    srv.core.nlive = srv.core.nlive - 1;", "    srv.core.nlive = srv.core.nlive - 1;"),
    # ---- time ----
    ("ready: no sweep", "    begin_round(srv.tab, srv.core);\n    sweep_idle(srv.tab, srv.core, now_ms / 1000);", "    begin_round(srv.tab, srv.core);"),
    ("ready: the time in milliseconds taken for seconds", "    sweep_idle(srv.tab, srv.core, now_ms / 1000);\n    return srv.core.nready", "    sweep_idle(srv.tab, srv.core, now_ms);\n    return srv.core.nready"),
    ("ready: begin_round not run", "    begin_round(srv.tab, srv.core);\n    sweep_idle", "    if srv.core.fed == 7 {\n        begin_round(srv.tab, srv.core);\n    }\n    sweep_idle"),
    ("sweep_idle: a byte-fed server not swept", "        var count = core.limit;\n        if core.fed == 0 {", "        var count = 0;\n        if core.fed == 0 {"),
    ("sweep_idle: closes at the limit, not past it", "&& now - st[p + 1] > core.idle {", "&& now - st[p + 1] >= core.idle {"),
    ("sweep_idle: closes a second late", "&& now - st[p + 1] > core.idle {", "&& now - st[p + 1] > core.idle + 1 {"),
    ("sweep_idle: a held stream nobody takes is spared", "            if st[p + 4] == 1 && (st[p + 13] == 0 || st[p + 2] > 0) && now", "            if st[p + 4] == 1 && st[p + 13] == 0 && now"),
    ("sweep_idle: a held request is not spared", "            if st[p + 4] == 1 && (st[p + 13] == 0 || st[p + 2] > 0) && now", "            if st[p + 4] == 1 && now"),
    ("sweep_idle: an ended connection swept again", "            if st[p + 4] == 1 && (st[p + 13] == 0 || st[p + 2] > 0) && now", "            if st[p + 4] != 0 && (st[p + 13] == 0 || st[p + 2] > 0) && now"),
    # ---- the queue ----
    ("enqueue: the full queue not compacted", "    if core.nready >= core.limit {\n        var left = 0;\n        while core.cursor + left < core.nready {\n            queue[left] = queue[core.cursor + left];\n            left = left + 1;\n        }\n        core.nready = left;\n        core.cursor = 0;\n    }\n    st[p + 12] = 1;", "    st[p + 12] = 1;"),
    ("enqueue: a connection queued twice", "    if st[p + 12] == 1 {\n        return 0;\n    }\n    let queue = contents(core.ready);", "    let queue = contents(core.ready);"),
    ("enqueue: the queue compacted without moving the cursor", "        core.nready = left;\n        core.cursor = 0;\n    }\n    st[p + 12] = 1;", "        core.nready = left;\n    }\n    st[p + 12] = 1;"),
    ("begin_round: what `next` did not reach is lost", "    core.nready = left;\n    core.cursor = 0;\n    let half = core.cur;", "    core.nready = 0;\n    core.cursor = 0;\n    let half = core.cur;"),
    # ---- answers: emit, stream ----
    ("emit: a byte-fed server writes to a socket", "    if pending == 0 && fed == 0 {", "    if pending == 0 {"),
    ("emit: an answer exactly filling the room refused", "    if pending + len(data) - at > len(pend) {", "    if pending + len(data) - at >= len(pend) {"),
    ("emit: an answer one byte over the room accepted", "    if pending + len(data) - at > len(pend) {", "    if pending + len(data) - at > len(pend) + 1 {"),
    ("deliver_to: the output room is the socket server's", "    let osize = core.osize;\n    st[p + 2] = emit(", "    let osize = output_size();\n    st[p + 2] = emit("),
    ("produce: a refusal queued in the socket server's room", "pd[k * core.osize..(k + 1) * core.osize], st[p + 2]);\n        }\n        buffer.drop(heap, o);", "pd[k * output_size()..(k + 1) * output_size()], st[p + 2]);\n        }\n        buffer.drop(heap, o);"),
    ("connections: the socket table's count", "    if srv.core.fed == 1 {\n        return srv.core.nlive;\n    }", "    if srv.core.fed == 7 {\n        return srv.core.nlive;\n    }"),
    ("stream: a socket server's connection streamed to", "    if srv.core.fed != 1 || ticket < 0 {\n        return 0 - 1;\n    }\n    let k = ticket % ticket_span();\n    if k >= srv.core.limit {", "    if ticket < 0 {\n        return 0 - 1;\n    }\n    let k = ticket % ticket_span();\n    if k >= srv.core.limit {"),
    ("stream: a ticket of another connection in the slot", "    if st[p + 4] != 1 || st[p + 13] != 1 || st[p + 14] != ticket / ticket_span() {\n        return 0 - 1;\n    }\n    var n = srv.core.osize", "    if st[p + 4] != 1 || st[p + 13] != 1 {\n        return 0 - 1;\n    }\n    var n = srv.core.osize"),
    ("stream: a request that is not held", "    if st[p + 4] != 1 || st[p + 13] != 1 || st[p + 14] != ticket / ticket_span() {\n        return 0 - 1;\n    }\n    var n = srv.core.osize", "    if st[p + 4] != 1 || st[p + 14] != ticket / ticket_span() {\n        return 0 - 1;\n    }\n    var n = srv.core.osize"),
    ("stream: a connection that has ended", "    if st[p + 4] != 1 || st[p + 13] != 1 || st[p + 14] != ticket / ticket_span() {\n        return 0 - 1;\n    }\n    var n = srv.core.osize", "    if st[p + 4] == 0 || st[p + 13] != 1 || st[p + 14] != ticket / ticket_span() {\n        return 0 - 1;\n    }\n    var n = srv.core.osize"),
    ("stream: the room not looked at", "    var n = srv.core.osize - st[p + 2];\n    if len(bytes) < n {\n        n = len(bytes);\n    }\n    let pd", "    var n = len(bytes);\n    let pd"),
    ("stream: more than was given", "    var n = srv.core.osize - st[p + 2];\n    if len(bytes) < n {\n        n = len(bytes);\n    }\n    let pd", "    var n = srv.core.osize - st[p + 2];\n    let pd"),
    ("stream: bytes written over those waiting", "    let at = k * srv.core.osize + st[p + 2];\n    var i = 0;\n    while i < n {\n        pd[at + i] = bytes[i];", "    let at = k * srv.core.osize;\n    var i = 0;\n    while i < n {\n        pd[at + i] = bytes[i];"),
    ("stream: the count waiting not raised", "    st[p + 2] = st[p + 2] + n;\n    return n;", "    return n;"),
    ("stream: a slot past the table", "    let k = ticket % ticket_span();\n    if k >= srv.core.limit {\n        return 0 - 1;\n    }\n    let st = contents(srv.core.state);\n    let p = stride() * k;\n    if st[p + 4] != 1 || st[p + 13] != 1 || st[p + 14]", "    let k = ticket % ticket_span();\n    let st = contents(srv.core.state);\n    let p = stride() * k;\n    if st[p + 4] != 1 || st[p + 13] != 1 || st[p + 14]"),
]

# Mutants the tests cannot reach, each with the argument. They must survive.
EQUIVALENT = {
    "attach: one connection too many": "the scan below it finds no free slot when `nlive` is `limit` and answers -1 the same; the count is a second guard",
    "attach: the scan does not move on": "which free slot is taken is not part of the contract (a ticket's generation tells connections apart, not the slot); the hint only spreads reuse",
    "init_slot: bytes buffered by the last connection kept": "a byte-fed slot is zeroed by `detach` before it can be taken again, so either line alone is enough (the socket server's `shut` leaves it and `accept_all` relies on this one, which `api`'s test of a closed connection's buffered bytes covers)",
    "detach: buffered input kept for the next connection": "`init_slot` zeroes it when the slot is taken again, and an ended or free slot answers 0 or -1 to every call meanwhile: either line alone is enough",
    "room: a closing connection takes input": "the closing flag with nothing waiting is ended by the visit that set it (`finish`), and with something waiting `st[p + 2] > 0` already says 0: the flag is never seen alone",
    "shut: what was waiting to be sent kept": "an ended slot answers 0 to `pending`, `space`, `room` and `output`, and `detach` clears the count: nothing can read it",
    "shut: a held request stays held": "an ended slot is refused by `answer` and `stream` for not being in use, and `detach` and `init_slot` clear the flag: nothing can read it",
    "detach: a ticket outlives the connection": "the in-use check refuses the ticket until the slot is taken again, and `init_slot` clears the flag then, with a new generation",
    "sweep_idle: an ended connection swept again": "`shut` on an ended connection sets what it already is",
    "stream: a connection that has ended": "an ended slot is not held (`shut` clears that), and `stream` refuses a request that is not held",
    "stream: a socket server's connection streamed to": "NOT REACHABLE FROM `cancho test`: a socket server needs a `Listener`, which needs the `Net` capability a test is not given; the guard is one comparison",
}


LIMIT = 45


class Ran:
    def __init__(self, returncode, stdout, stderr):
        self.returncode, self.stdout, self.stderr = returncode, stdout, stderr


def run(exe, server_text, name):
    """`cancho test` on the (mutated) source. A run that hangs past LIMIT seconds is killed, with the test program it
    started (a mutant that makes a loop in a test endless is caught by the test not finishing), and answers `Ran(4, ..)`."""
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "server.cho")
        with open(path, "w") as f:
            f.write(server_text)
        p = subprocess.Popen([exe, "test", "--std", path, TESTS], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                             start_new_session=True)
        try:
            out, err = p.communicate(timeout=LIMIT)
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.communicate()
            return Ran(4, "FAILED: hung past the limit", "")
    return Ran(p.returncode, out, err)


def judge(job):
    exe, name, old, new, source = job
    if source.count(old) != 1:
        return name, "BAD", f"`old` occurs {source.count(old)} times"
    r = run(exe, source.replace(old, new), name)
    if r.returncode == 4 or "FAILED" in r.stdout:
        return name, "killed", ""
    if r.returncode == 0:
        return name, "SURVIVED", ""
    return name, "NOBUILD", (r.stderr or r.stdout)[:300]


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    exe = os.path.abspath(args.pop(0))
    only = None
    jobs = 4
    if "--only" in args:
        k = args.index("--only")
        only = args[k + 1]
        del args[k:k + 2]
    if "--jobs" in args:
        k = args.index("--jobs")
        jobs = int(args[k + 1])
        del args[k:k + 2]
    source = open(SERVER).read()
    base = run(exe, source, "unmutated")
    if base.returncode != 0:
        print("the unmutated source does not pass:\n" + base.stdout + base.stderr)
        return 1
    chosen = [m for m in MUTANTS if only is None or only in m[0]]
    results = []
    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        for name, verdict, detail in pool.map(judge, [(exe, n, o, w, source) for n, o, w in chosen]):
            if name in EQUIVALENT:
                verdict = "equivalent" if verdict == "SURVIVED" else verdict + " (argued equivalent, but killed: remove it)"
            results.append((name, verdict))
            print(f"{verdict:10} {name}" + (f"   {detail}" if detail and verdict not in ("killed", "equivalent") else ""), flush=True)
    killed = sum(1 for _, v in results if v == "killed")
    bad = [n for n, v in results if v not in ("killed", "equivalent")]
    print(f"{killed} of {len(results)} killed" + (f", {len(results) - killed - len(bad)} argued equivalent" if len(results) - killed - len(bad) else ""))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
