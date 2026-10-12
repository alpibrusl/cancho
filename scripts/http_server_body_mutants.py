#!/usr/bin/env python3
"""Mutation check of `packages/http-server`'s streaming request bodies (docs/http-server.md §12.13), the shape of `scripts/http_server_bytes_mutants.py`.

    python3 scripts/http_server_body_mutants.py <cancho binary> [--only <text in a mutant's name>] [--jobs <n>]

Each mutant is `packages/http-server/server.cho` with one deliberate bug at one site, in the code §12 added or changed
(`classify`, `chunk_step`, `chunk_run`, `ingest`, `reclaim`, `readable`, `reject`, `refuse_with`, `end_request`,
`release_head`, `sweep_timers`, the `body_*`, `proceed` and `limits` calls, and the places the shared code was made to
call them: `produce`, `step`, `settle`, `input`, `room`, `hold_in`, `answer_in`, `deliver_to`, `finish`, `enqueue`, `end_input`).
It is copied to a scratch directory and the two test files of `tests/packages/http_server_body*_test.cho` are run against it
(`cancho test`). A mutant is killed when a test fails or traps or hangs (45 seconds; the tests take 6). The unmutated source
is run first and must pass. A mutant that changes nothing the tests can reach is in EQUIVALENT with the argument, and must
survive. Exit status 1 if a mutant survives, an `old` text does not occur exactly once, or a mutant fails to build.

The socket path (`step`, `settle`, `wait`) is exercised by `conformance/http_server_upload.rs`, which builds a server program
and takes minutes; the mutants of the lines only that path runs are marked SOCKET and are checked by running that test
(`--socket`, which needs `cargo`).
"""
import concurrent.futures
import os
import re
import signal
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SERVER = os.path.join(ROOT, "packages/http-server/server.cho")
TESTS = [os.path.join(ROOT, "tests/packages", f) for f in ("http_server_body_test.cho", "http_server_body_flow_test.cho")]
SUPPORT = os.path.join(ROOT, "tests/packages/body_support.cho")

MUTANTS = [
    # ---- flags and helpers ----
    ("has: the wrong bit", "    return flags / bit % 2 == 1;", "    return flags / bit % 3 == 1;"),
    ("set_flag: set twice", "    if !has(st[p + 24], bit) { st[p + 24] = st[p + 24] + bit; }", "    st[p + 24] = st[p + 24] + bit;"),
    ("clear_body: the last word kept", "    while i < stride() { st[stride() * k + i] = 0; i = i + 1; }", "    while i < stride() - 1 { st[stride() * k + i] = 0; i = i + 1; }"),
    ("clear_body: the flags kept", "    var i = 16;\n    while i < stride() { st[stride() * k + i] = 0; i = i + 1; }", "    var i = 25;\n    while i < stride() { st[stride() * k + i] = 0; i = i + 1; }"),
    ("init_slot: a slot's body state kept for the next connection", "    st[p + 15] = 0;\n    clear_body(core, slot);", "    st[p + 15] = 0;"),
    ("detach: a slot's body state kept", "    st[p + 13] = 0;\n    clear_body(srv.core, k);", "    st[p + 13] = 0;"),
    # ---- the rules ----
    # ---- readable, reclaim, room ----
    ("readable: a recorded refusal still reads", "    if st[p + 25] != 0 { return 0; }\n    var free", "    var free"),
    ("readable: past the end of a length body", "        if st[p + 16] == 1 && st[p + 17] < free { return st[p + 17]; }", "        if st[p + 16] == 7 && st[p + 17] < free { return st[p + 17]; }"),
    ("readable: the bytes taken are not room", "        free = free + st[p + 21];", "        free = free + 0;"),
    ("readable: a body that has all arrived is limited", "    if st[p + 16] != 0 && !has(st[p + 24], 8) {\n        // The bytes the application", "    if st[p + 16] != 0 {\n        // The bytes the application"),
    ("readable: a chunked body is limited by its length", "        if st[p + 16] == 1 && st[p + 17] < free { return st[p + 17]; }", "        if st[p + 17] < free { return st[p + 17]; }"),
    ("reclaim: nothing moved", "        st[p] = st[p] - st[p + 21];\n        st[p + 21] = 0;\n    }\n    return st[p];", "    }\n    return st[p];"),
    ("reclaim: the start not reset", "        st[p] = st[p] - st[p + 21];\n        st[p + 21] = 0;\n    }\n    return st[p];", "        st[p] = st[p] - st[p + 21];\n    }\n    return st[p];"),
    ("reclaim: one byte too few moved", "        while i < st[p] - st[p + 21] {\n            bf[base + i] = bf[base + st[p + 21] + i];", "        while i < st[p] - st[p + 21] - 1 {\n            bf[base + i] = bf[base + st[p + 21] + i];"),
    ("reclaim: done bodies moved too", "    if st[p + 16] != 0 && !has(st[p + 24], 8) && st[p + 21] > 0 {", "    if st[p + 16] != 0 && st[p + 21] > 0 {"),
    ("input: reclaim not run", "        if n > srv.core.size - st[p] {\n            reclaim(srv.core, k);\n        }", "        if n > srv.core.size + st[p] {\n            reclaim(srv.core, k);\n        }"),
    ("input: ingest not run", "        ingest(srv.core, k, st[p] - n);", "        st[p] = st[p] + 0;"),
    ("input: ingest given the wrong start", "        ingest(srv.core, k, st[p] - n);", "        ingest(srv.core, k, st[p]);"),
    ("input: the clock not kept", "    srv.core.now = now_ms;\n    var n = room(srv, k);", "    var n = room(srv, k);"),
    ("room: the buffer's free space whatever the body owes", "    return readable(srv.core, k);\n}", "    return srv.core.size - st[p];\n}"),
    # ---- ingest ----
    ("ingest: a body that has all arrived is counted again", "    if st[p + 16] == 0 || has(st[p + 24], 8) || st[p + 25] != 0 { return 0; }", "    if st[p + 16] == 0 || st[p + 25] != 0 { return 0; }"),
    ("ingest: a recorded refusal is run over", "    if st[p + 16] == 0 || has(st[p + 24], 8) || st[p + 25] != 0 { return 0; }", "    if st[p + 16] == 0 || has(st[p + 24], 8) { return 0; }"),
    ("ingest: no progress noted", "    st[p + 27] = core.now;\n    set_flag(st, p, 16);\n    if st[p + 16] == 2 {", "    set_flag(st, p, 16);\n    if st[p + 16] == 2 {"),
    ("ingest: not marked touched", "    st[p + 27] = core.now;\n    set_flag(st, p, 16);\n    if st[p + 16] == 2 {", "    st[p + 27] = core.now;\n    if st[p + 16] == 2 {"),
    ("ingest: a refusal not queued for next", "        chunk_run(core, k, from);\n        if st[p + 25] != 0 { enqueue(core, k); }", "        chunk_run(core, k, from);"),
    ("ingest: the length owed not reduced", "    st[p + 17] = st[p + 17] - got;\n    st[p + 22]", "    st[p + 22]"),
    ("ingest: the bytes available not raised", "    st[p + 22] = st[p + 22] + got;\n    st[p + 23] = st[p + 23] + got;\n    if st[p + 17] <= 0", "    st[p + 23] = st[p + 23] + got;\n    if st[p + 17] <= 0"),
    ("ingest: the total not raised", "    st[p + 23] = st[p + 23] + got;\n    if st[p + 17] <= 0", "    if st[p + 17] <= 0"),
    ("ingest: the end of a length body not noticed", "    if st[p + 17] <= 0 { set_flag(st, p, 8); }", "    if st[p + 17] < 0 { set_flag(st, p, 8); }"),
    ("ingest: the end noticed one byte early", "    if st[p + 17] <= 0 { set_flag(st, p, 8); }", "    if st[p + 17] <= 1 { set_flag(st, p, 8); }"),
    # ---- the chunk machine ----
    ("chunk_step: trailers not bounded", "        if st[p + 29] > 4096 { return 6; }", "        if st[p + 29] > 4096000 { return 6; }"),
    ("chunk_step: trailers bounded one byte early", "        if st[p + 29] > 4096 { return 6; }", "        if st[p + 29] >= 4096 { return 6; }"),
    ("chunk_step: trailer bytes counted in the other states too", "    if cs >= 7 && cs <= 10 {\n        st[p + 29] = st[p + 29] + 1;", "    if cs >= 7 && cs <= 11 {\n        st[p + 29] = st[p + 29] + 1;"),
    ("chunk_step: a ninth hex digit accepted", "            if st[p + 20] >= 8 { return 2; }", "            if st[p + 20] >= 9 { return 2; }"),
    ("chunk_step: a seventh hex digit refused", "            if st[p + 20] >= 8 { return 2; }", "            if st[p + 20] >= 7 { return 2; }"),
    ("chunk_step: the digits not counted", "            st[p + 20] = st[p + 20] + 1;\n            st[p + 18] = 1;", "            st[p + 18] = 1;"),
    ("chunk_step: the size not accumulated", "            st[p + 19] = st[p + 19] * 16 + v;", "            st[p + 19] = v;"),
    ("chunk_step: an empty size line accepted", "        if cs == 0 { return 2; }\n        if c == 59", "        if c == 59"),
    ("chunk_step: an extension refused", "        if c == 59 {\n            st[p + 18] = 2;\n            st[p + 20] = 0;\n            return 0;\n        }", "        if c == 59 { return 4; }"),
    ("chunk_step: the extension length not reset", "            st[p + 18] = 2;\n            st[p + 20] = 0;\n            return 0;", "            st[p + 18] = 2;\n            return 0;"),
    ("chunk_step: junk after the digits accepted as the end of the line", "        if c == 13 { st[p + 18] = 3; return 0; }\n        return 3;\n    }\n    if cs == 2 {", "        st[p + 18] = 3;\n        return 0;\n    }\n    if cs == 2 {"),
    ("chunk_step: a control byte in an extension accepted", "        if c < 32 && c != 9 || c == 127 || st[p + 20] > 256 { return 4; }", "        if st[p + 20] > 256 { return 4; }"),
    ("chunk_step: a tab in an extension refused", "        if c < 32 && c != 9 || c == 127 || st[p + 20] > 256 { return 4; }", "        if c < 32 || c == 127 || st[p + 20] > 256 { return 4; }"),
    ("chunk_step: an extension bounded one byte early", "        if c < 32 && c != 9 || c == 127 || st[p + 20] > 256 { return 4; }", "        if c < 32 && c != 9 || c == 127 || st[p + 20] > 255 { return 4; }"),
    ("chunk_step: an extension bounded one byte late", "        if c < 32 && c != 9 || c == 127 || st[p + 20] > 256 { return 4; }", "        if c < 32 && c != 9 || c == 127 || st[p + 20] > 257 { return 4; }"),
    ("chunk_step: an extension not bounded", "        if c < 32 && c != 9 || c == 127 || st[p + 20] > 256 { return 4; }", "        if c < 32 && c != 9 || c == 127 { return 4; }"),
    ("chunk_step: a bare LF ends the size line", "    if cs == 3 {\n        if c != 10 { return 3; }", "    if cs == 3 {\n        if c != 10 && c != 13 { return 3; }"),
    ("chunk_step: the last chunk is not noticed", "        if st[p + 19] == 0 { st[p + 18] = 7; return 0; }", "        if st[p + 19] == 0 { st[p + 18] = 4; return 0; }"),
    ("chunk_step: a chunk past the maximum accepted", "        if st[p + 23] + st[p + 19] > maxb { return 1; }", "        if st[p + 23] + st[p + 19] > maxb + 1000000 { return 1; }"),
    ("chunk_step: the total ignored when a size is read", "        if st[p + 23] + st[p + 19] > maxb { return 1; }", "        if st[p + 19] > maxb { return 1; }"),
    ("chunk_step: a body of exactly the maximum refused", "        if st[p + 23] + st[p + 19] > maxb { return 1; }", "        if st[p + 23] + st[p + 19] >= maxb { return 1; }"),
    ("chunk_step: a control byte in a trailer value accepted", "        if c < 32 && c != 9 || c == 127 { return 5; }\n        return 0;", "        return 0;"),
    # ---- chunk_run ----
    ("chunk_run: the decoded bytes are written where they were read", "            while i < m { bf[base + w + i] = bf[base + r + i]; i = i + 1; }", "            while i < m { bf[base + r + i] = bf[base + r + i]; i = i + 1; }"),
    ("chunk_run: one data byte too few moved", "            while i < m { bf[base + w + i] = bf[base + r + i]; i = i + 1; }", "            while i < m - 1 { bf[base + w + i] = bf[base + r + i]; i = i + 1; }"),
    ("chunk_run: data past the chunk", "            if m > st[p + 19] { m = st[p + 19]; }", "            if m > st[p + 19] + 1 { m = st[p + 19] + 1; }"),
    ("chunk_run: the write position not advanced", "            r = r + m;\n            w = w + m;", "            r = r + m;"),
    ("chunk_run: the chunk's bytes left not reduced", "            st[p + 19] = st[p + 19] - m;\n            st[p + 23]", "            st[p + 23]"),
    ("chunk_run: the total not raised", "            st[p + 23] = st[p + 23] + m;\n            if st[p + 19] == 0", "            if st[p + 19] == 0"),
    ("chunk_run: data not followed by its CRLF", "            if st[p + 19] == 0 { st[p + 18] = 5; }", "            if st[p + 19] == 0 { st[p + 18] = 0; }"),
    ("chunk_run: it goes on after the end", "    while r < end && st[p + 18] != 12 && code == 0 {", "    while r < end && code == 0 {"),
    ("chunk_run: it goes on after a refusal", "    while r < end && st[p + 18] != 12 && code == 0 {", "    while r < end && st[p + 18] != 12 {"),
    ("chunk_run: the bytes available not recorded", "    st[p + 22] = w - st[p + 21];\n    if code != 0 {", "    if code != 0 {"),
    ("chunk_run: a refusal not recorded", "        st[p + 25] = code;\n        st[p] = w;\n        return code;", "        st[p] = w;\n        return code;"),
    ("chunk_run: the end not marked", "    if st[p + 18] == 12 {\n        set_flag(st, p, 8);", "    if st[p + 18] == 12 {"),
    ("chunk_run: what follows the body left where it was", "        while r + i < end { bf[base + w + i] = bf[base + r + i]; i = i + 1; }\n        st[p] = w + end - r;", "        st[p] = end;"),
    ("chunk_run: what follows the body lost", "        while r + i < end { bf[base + w + i] = bf[base + r + i]; i = i + 1; }\n        st[p] = w + end - r;", "        st[p] = w;"),
    ("chunk_run: the framing not given back", "    } else {\n        st[p] = w;\n    }\n    return 0;\n}\n\n// Until a body is whole", "    } else {\n        st[p] = end;\n    }\n    return 0;\n}\n\n// Until a body is whole"),
    ("chunk_run: the start ignores the bytes not yet taken", "    var w = st[p + 21] + st[p + 22];", "    var w = st[p + 21];"),
    # ---- classify ----
    ("classify: an unsupported expectation passes", "                return 0 - 9;", "                expect = 0;"),
    ("classify: any expectation is unsupported", '            if eq_ci(http.header_value(view, table, i), "100-continue") { expect = 1; } else {', '            if eq_ci(http.header_value(view, table, i), "100-continue") && false { expect = 1; } else {'),
    ("classify: Expect compared with case", 'if eq_ci(http.header_name(view, table, i), "expect") {', 'if bytes.equal(http.header_name(view, table, i), "expect") {'),
    ("classify: the value compared with case", 'if eq_ci(http.header_value(view, table, i), "100-continue") { expect = 1; }', 'if bytes.equal(http.header_value(view, table, i), "100-continue") { expect = 1; }'),
    ("classify: only the first Expect looked at", "        i = i + 1;\n    }\n    let chunked", "        i = http.header_count(table);\n    }\n    let chunked"),
    ("eq_ci: lengths not compared", "    if len(a) != len(lit) { return false; }\n    var i = 0;", "    var i = 0;\n    if len(a) < len(lit) { return false; }"),
    ("classify: a body without a length is whole", "    if !chunked && blen == 0 {\n        // No body", "    if !chunked && blen == 0 || chunked {\n        // No body"),
    ("classify: the maximum is not looked at", "    if !chunked && blen > core.maxb { return 0 - 1; }", "    if !chunked && blen > core.maxb * 1000 { return 0 - 1; }"),
    ("classify: a length of exactly the maximum refused", "    if !chunked && blen > core.maxb { return 0 - 1; }", "    if !chunked && blen >= core.maxb { return 0 - 1; }"),
    ("classify: the body's start", "    st[p + 21] = used + n;", "    st[p + 21] = n;"),
    ("classify: bytes after a length body counted as its own", "        var got = have;\n        if got > blen { got = blen; }", "        var got = have;"),
    ("classify: what is owed", "        st[p + 17] = blen - got;", "        st[p + 17] = blen;"),
    ("classify: a whole length body not noticed", "        if got == blen { set_flag(st, p, 8); }", "        if got == blen + 1 { set_flag(st, p, 8); }"),
    ("classify: keep-alive forgotten", "    if http.keeps_alive(table) { set_flag(st, p, 1); }\n    if !chunked {", "    if !chunked {"),
    ("classify: a chunked refusal leaves the request streaming", "            let code = st[p + 25];\n            clear_body(core, k);\n            return 0 - code;", "            let code = st[p + 25];\n            return 0 - code;"),
    ("classify: Expect honoured on an HTTP/1.0 request", "    if expect == 1 && http.version(table) == 11 { set_flag(st, p, 2); }", "    if expect == 1 { set_flag(st, p, 2); }"),
    ("classify: Expect never honoured", "    if expect == 1 && http.version(table) == 11 { set_flag(st, p, 2); }", "    if expect == 7 && http.version(table) == 11 { set_flag(st, p, 2); }"),
    ("classify: bytes already in not counted as touched", "    if have > 0 { set_flag(st, p, 16); }", "    if have > 100000 { set_flag(st, p, 16); }"),
    ("classify: the body's clock not started", "    if have > 0 { set_flag(st, p, 16); }\n    st[p + 27] = core.now;\n    return 2;", "    if have > 0 { set_flag(st, p, 16); }\n    return 2;"),
    ("classify: a streaming request's body shown to `body`", "    st[p + 9] = 0;\n    if expect == 1", "    if expect == 1"),
    ("classify: a whole body's length", "    st[p + 7] = n;\n    st[p + 9] = st[p + 22];\n    st[p + 10] = 0;\n    if has(st[p + 24], 8) { return 1; }", "    st[p + 7] = n;\n    st[p + 9] = 0;\n    st[p + 10] = 0;\n    if has(st[p + 24], 8) { return 1; }"),
    ("classify: a whole body said to be decoded elsewhere", "    st[p + 9] = st[p + 22];\n    st[p + 10] = 0;\n    if has(st[p + 24], 8) { return 1; }", "    st[p + 9] = st[p + 22];\n    st[p + 10] = 1;\n    if has(st[p + 24], 8) { return 1; }"),
    ("classify: the head's length forgotten for a streaming request", "    st[p + 7] = n;\n    st[p + 9] = st[p + 22];", "    st[p + 9] = st[p + 22];"),
    # ---- the chunk machine, as rewritten ----
    ("chunk_step: data followed by an LF in place of the CR", "if cs == 5 { return want(st, p, c, 13, 6, 3); }", "if cs == 5 { if c == 10 { return want(st, p, c, 10, 6, 3); } return want(st, p, c, 13, 6, 3); }"),
    ("chunk_step: data followed by a CR in place of the LF", "if cs == 6 { return want(st, p, c, 10, 0, 3); }", "if cs == 6 { if c == 13 { return want(st, p, c, 13, 0, 3); } return want(st, p, c, 10, 0, 3); }"),
    ("want: the digit count kept for the next size line", "st[p + 18] = next; st[p + 20] = 0; return 0;", "st[p + 18] = next; return 0;"),
    ("chunk_step: a bare CR ends a trailer line", "if cs == 10 { return want(st, p, c, 10, 7, 5); }", "if cs == 10 { if c == 13 { return want(st, p, c, 13, 7, 5); } return want(st, p, c, 10, 7, 5); }"),
    ("chunk_step: the final LF not required", "if cs == 11 { return want(st, p, c, 10, 12, 5); }", "if cs == 11 { st[p + 18] = 12; return 0; }"),
    ("chunk_step: an LF ends the trailer section", "if cs == 7 || cs == 9 { if c == 13 {", "if cs == 7 || cs == 9 { if c == 13 || c == 10 {"),
    ("chunk_step: a trailer line ends at its CR even in the middle of the section", "st[p + 18] = 11; if cs == 9 { st[p + 18] = 10; } return 0;", "st[p + 18] = 11; return 0;"),
    ("chunk_step: a trailer line's control byte accepted", "st[p + 18] = 9; if c < 32 && c != 9 || c == 127 { return 5; } return 0;", "st[p + 18] = 9; return 0;"),
    ("chunk_step: a trailer line's tab refused", "st[p + 18] = 9; if c < 32 && c != 9 || c == 127 { return 5; } return 0;", "st[p + 18] = 9; if c < 32 || c == 127 { return 5; } return 0;"),
    ("chunk_step: a size digit past f", "let v = http.hex_value(c);", "let v = http.hex_value(c - 1);"),
    ("classify: the head's length forgotten for a request with no body", "if !chunked && blen == 0 { st[p + 7] = n;", "if !chunked && blen == 0 { st[p + 7] = 0;"),
    ("produce: a refusal with the wrong status", "refuse = status;", "refuse = 400;"),
    ("produce: a refusal with the wrong message", "message = text;", 'message = "bad request";'),
    ("produce: a refusal with the wrong rule", "tag = rule_tag;", 'tag = "head.malformed";'),
    ("answer_in: a request that is over answered", "|| st[p + 25] != 0 { return 0 - 1; } let base = k * core.size;", "{ return 0 - 1; } let base = k * core.size;"),
    ("proceed: sent twice", "st[p + 24] = st[p + 24] - 2; st[p + 27] = srv.core.now;", "st[p + 27] = srv.core.now;"),
    ("proceed: the body's clock not restarted", "st[p + 24] = st[p + 24] - 2; st[p + 27] = srv.core.now;", "st[p + 24] = st[p + 24] - 2;"),
    ("proceed: nothing sent", '"HTTP/1.1 100 Continue\\r\\n\\r\\n", pd[', '"", pd['),
    ("proceed: the wrong interim response", '"HTTP/1.1 100 Continue\\r\\n\\r\\n", pd[', '"HTTP/1.1 100 Continue\\r\\n", pd['),
    ("proceed: the answer says nothing was sent", "settle(srv.tab, srv.core, k); return 1; }", "settle(srv.tab, srv.core, k); return 0; }"),
    # ---- produce ----
    ("produce: a recorded refusal not sent", "    if st[p + 25] != 0 {\n        return reject(heap, tab, core, k);\n    }", "    if st[p + 25] != 0 {\n        return 0;\n    }"),
    ("produce: a streaming request in hand parsed again", "    if st[p + 16] != 0 {\n        // A streaming request is still in hand (`next` was called again without `hold` or `respond`).\n        return 1;\n    }", "    if st[p + 16] == 7 {\n        return 1;\n    }"),
    ("produce: limits not honoured", "    } else if core.lim == 1 {\n        // §12.3: what a head with a body is.", "    } else if core.lim == 7 {\n        // §12.3: what a head with a body is."),
    ("produce: the head timer never starts", "            } else if core.lim == 1 && st[p + 26] == 0 {\n                st[p + 26] = core.now + 1;", "            } else if core.lim == 7 && st[p + 26] == 0 {\n                st[p + 26] = core.now + 1;"),
    ("produce: the head timer restarts at every look", "            } else if core.lim == 1 && st[p + 26] == 0 {", "            } else if core.lim == 1 {"),
    ("produce: the head timer not stopped by a complete head", "        // §12.3: what a head with a body is.\n        st[p + 26] = 0;", "        // §12.3: what a head with a body is."),
    # ---- refuse_with, reject, end_request, release_head ----
    ("refuse_with: no rule header", "    if core.lim == 1 {\n        var x = buffer.append", "    if core.lim == 7 {\n        var x = buffer.append"),
    ("refuse_with: the rule header on every server", "    if core.lim == 1 {\n        var x = buffer.append", "    if core.lim >= 0 {\n        var x = buffer.append"),
    ("refuse_with: the connection not ended", "    if st[p + 2] < 0 { st[p + 11] = 1; }\n    st[p + 3] = 1;\n    return 0;\n}\n\n// A refusal was recorded", "    if st[p + 2] < 0 { st[p + 11] = 1; }\n    return 0;\n}\n\n// A refusal was recorded"),
    ("reject: a held request stays held", "    let begun = has(st[p + 24], 4);\n    st[p + 13] = 0;", "    let begun = has(st[p + 24], 4);"),
    ("reject: answered after the answer began", "    if code == 10 || begun || st[p + 2] < 0 || st[p + 3] == 1 {", "    if code == 10 || st[p + 2] < 0 || st[p + 3] == 1 {"),
    ("reject: a peer that is gone is answered", "    if code == 10 || begun || st[p + 2] < 0 || st[p + 3] == 1 {", "    if begun || st[p + 2] < 0 || st[p + 3] == 1 {"),
    ("reject: a connection that is ending is ended over its answer", "        if st[p + 3] == 0 { st[p + 11] = 1; }", "        st[p + 11] = 1;"),
    ("reject: the connection not dropped", "        if st[p + 3] == 0 { st[p + 11] = 1; }", "        st[p + 3] = 1;"),
    ("reject: an answer already ending the connection is added to", "    if code == 10 || begun || st[p + 2] < 0 || st[p + 3] == 1 {", "    if code == 10 || begun || st[p + 2] < 0 {"),
    ("end_request: keep-alive ignored", "    if !has(flags, 1) { st[p + 3] = 1; }", "    if has(flags, 64) { st[p + 3] = 1; }"),
    ("end_request: a body that has all arrived is not skipped", "    if has(flags, 8) { st[p + 6] = st[p + 21] + st[p + 22]; } else", "    if has(flags, 8) { st[p + 6] = st[p + 21]; } else"),
    ("end_request: the untaken bytes are kept", "    if has(flags, 8) { st[p + 6] = st[p + 21] + st[p + 22]; } else", "    if has(flags, 8) { st[p + 6] = st[p + 21] + st[p + 22] - st[p + 22]; } else"),
    ("end_request: an untouched Expect closes", "} else if has(flags, 2) && !has(flags, 16) {", "} else if has(flags, 2) && has(flags, 16) {"),
    ("end_request: an Expect closes when touched too", "} else if has(flags, 2) && !has(flags, 16) {", "} else if has(flags, 2) {"),
    ("end_request: an untouched Expect skips nothing", "        st[p + 6] = st[p + 6] + st[p + 7];\n    } else {\n        st[p + 3] = 1;\n    }\n    clear_body", "        st[p + 6] = st[p + 6];\n    } else {\n        st[p + 3] = 1;\n    }\n    clear_body"),
    ("end_request: an early answer keeps the connection", "    } else {\n        st[p + 3] = 1;\n    }\n    clear_body(core, k);\n    return 0;", "    } else {\n        st[p + 3] = st[p + 3];\n    }\n    clear_body(core, k);\n    return 0;"),
    ("end_request: the body state kept", "    clear_body(core, k);\n    return 0;\n}\n\n// `hold` of a request in a server with `limits`", "    return 0;\n}\n\n// `hold` of a request in a server with `limits`"),
    ("release_head: a whole body not marked arrived", "        st[p + 21] = st[p + 6] + st[p + 7];\n        set_flag(st, p, 8);", "        st[p + 21] = st[p + 6] + st[p + 7];"),
    ("release_head: the body's start", "        st[p + 21] = st[p + 6] + st[p + 7];\n        set_flag", "        st[p + 21] = st[p + 6];\n        set_flag"),
    ("release_head: keep-alive forgotten", "        if http.keeps_alive(contents(core.parsed)) { set_flag(st, p, 1); }\n    }\n    st[p + 6]", "    }\n    st[p + 6]"),
    ("release_head: the head not let go", "    st[p + 6] = st[p + 6] + st[p + 7];\n    st[p + 7] = 0;\n    return 0;", "    st[p + 7] = 0;\n    return 0;"),
    ("release_head: the head's length kept", "    st[p + 6] = st[p + 6] + st[p + 7];\n    st[p + 7] = 0;\n    return 0;", "    st[p + 6] = st[p + 6] + st[p + 7];\n    return 0;"),
    ("hold_in: the head not released", "    if core.lim == 1 {\n        release_head(core, k);\n    }", "    if core.lim == 7 {\n        release_head(core, k);\n    }"),
    ("finish: the body's start not moved with the buffer", "        if st[p + 16] != 0 {\n            st[p + 21] = st[p + 21] - used;\n        }", "        if st[p + 16] == 7 {\n            st[p + 21] = st[p + 21] - used;\n        }"),
    # ---- the timers ----
    ("sweep_timers: never", "    if core.lim == 0 || core.now - core.last_tick < 50 { return 0; }", "    if core.lim >= 0 { return 0; }"),
    ("sweep_timers: on every call", "    if core.lim == 0 || core.now - core.last_tick < 50 { return 0; }", "    if core.lim == 0 { return 0; }"),
    ("sweep_timers: the scan time not kept", "    core.last_tick = core.now;\n    let st", "    let st"),
    ("sweep_timers: a connection with an answer waiting is timed", "        if st[p + 4] == 1 && st[p + 25] == 0 && st[p + 3] == 0 && st[p + 2] == 0 {", "        if st[p + 4] == 1 && st[p + 25] == 0 && st[p + 3] == 0 {"),
    ("sweep_timers: a connection that is ending is timed", "        if st[p + 4] == 1 && st[p + 25] == 0 && st[p + 3] == 0 && st[p + 2] == 0 {", "        if st[p + 4] == 1 && st[p + 25] == 0 && st[p + 2] == 0 {"),
    ("sweep_timers: a refusal recorded twice", "        if st[p + 4] == 1 && st[p + 25] == 0 && st[p + 3] == 0 && st[p + 2] == 0 {", "        if st[p + 4] == 1 && st[p + 3] == 0 && st[p + 2] == 0 {"),
    ("sweep_timers: a free slot is timed", "        if st[p + 4] == 1 && st[p + 25] == 0 && st[p + 3] == 0 && st[p + 2] == 0 {", "        if st[p + 25] == 0 && st[p + 3] == 0 && st[p + 2] == 0 {"),
    ("sweep_timers: a body that has all arrived is timed", "                if !has(st[p + 24], 8) && !has(st[p + 24], 2) && readable(core, s) > 0 &&", "                if !has(st[p + 24], 2) && readable(core, s) > 0 &&"),
    ("sweep_timers: a client told to wait is timed", "                if !has(st[p + 24], 8) && !has(st[p + 24], 2) && readable(core, s) > 0 &&", "                if !has(st[p + 24], 8) && readable(core, s) > 0 &&"),
    ("sweep_timers: the application's slowness is the client's", "                if !has(st[p + 24], 8) && !has(st[p + 24], 2) && readable(core, s) > 0 &&", "                if !has(st[p + 24], 8) && !has(st[p + 24], 2) &&"),
    ("sweep_timers: the body timer at the limit", "core.now - st[p + 27] > core.body_ms {", "core.now - st[p + 27] >= core.body_ms {"),
    ("sweep_timers: the body timer a millisecond late", "core.now - st[p + 27] > core.body_ms {", "core.now - st[p + 27] > core.body_ms + 1 {"),
    ("sweep_timers: the body timer is the head's", "core.now - st[p + 27] > core.body_ms {", "core.now - st[p + 27] > core.head_ms {"),
    ("sweep_timers: the body timer records the wrong refusal", "                    record(core, s, 8);", "                    record(core, s, 7);"),
    ("sweep_timers: the head timer records the wrong refusal", "                record(core, s, 7);", "                record(core, s, 8);"),
    ("sweep_timers: the head timer at the limit", "core.now + 1 - st[p + 26] > core.head_ms {", "core.now + 1 - st[p + 26] >= core.head_ms {"),
    ("sweep_timers: the head timer a millisecond late", "core.now + 1 - st[p + 26] > core.head_ms {", "core.now + 1 - st[p + 26] > core.head_ms + 1 {"),
    ("sweep_timers: the head timer is the body's", "core.now + 1 - st[p + 26] > core.head_ms {", "core.now + 1 - st[p + 26] > core.body_ms {"),
    ("sweep_timers: a held request's head is timed", "            } else if st[p + 26] > 0 && st[p + 13] == 0 && core.now", "            } else if st[p + 26] > 0 && core.now"),
    ("sweep_timers: no timer without a head started", "            } else if st[p + 26] > 0 && st[p + 13] == 0 && core.now", "            } else if st[p + 13] == 0 && core.now"),
    ("record: not queued", "    contents(core.state)[stride() * s + 25] = code;\n    enqueue(core, s);", "    contents(core.state)[stride() * s + 25] = code;"),
    ("record: not recorded", "    contents(core.state)[stride() * s + 25] = code;\n    enqueue(core, s);", "    enqueue(core, s);"),
    ("ready: the clock not kept", "    srv.core.now = now_ms;\n    begin_round(srv.tab, srv.core);", "    begin_round(srv.tab, srv.core);"),
    ("ready: no timers", "    sweep_idle(srv.tab, srv.core, now_ms / 1000);\n    sweep_timers(srv.tab, srv.core);", "    sweep_idle(srv.tab, srv.core, now_ms / 1000);"),
    ("attach: the clock not kept", "    init_slot(srv.core, k, now_ms / 1000);\n    srv.core.now = now_ms;", "    init_slot(srv.core, k, now_ms / 1000);"),
    ("output: the clock not kept", "    srv.core.now = now_ms;\n    var n = st[p + 2];", "    var n = st[p + 2];"),
    # ---- end_input, answer, stream, deliver ----
    ("end_input: a body that stops is not an end", "    if st[stride() * k + 16] != 0 && !has(st[stride() * k + 24], 8) {", "    if st[stride() * k + 16] == 7 && !has(st[stride() * k + 24], 8) {"),
    ("end_input: a body that has all arrived is dropped", "    if st[stride() * k + 16] != 0 && !has(st[stride() * k + 24], 8) {", "    if st[stride() * k + 16] != 0 {"),
    ("end_input: the end is answered", "        st[stride() * k + 25] = 10;", "        st[stride() * k + 25] = 3;"),
    ("deliver_to: an answer given to a request that is over", "    if st[p + 25] != 0 {\n        // Its request is over (a refusal is recorded): the next `next` sends that.\n        return 0 - 1;\n    }", ""),
    ("deliver_to: the request not ended", "    if st[p + 16] != 0 {\n        end_request(core, k);\n        return st[p + 2];\n    }", "    if st[p + 16] == 7 {\n        end_request(core, k);\n        return st[p + 2];\n    }"),
    ("answer_in: a streamed request parsed again", "    if st[p + 16] == 0 {\n        http.parse(", "    if st[p + 16] >= 0 {\n        http.parse("),
    ("stream: a request that is over streamed to", "st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 {\n        return 0 - 1;\n    }\n    var n = srv.core.osize", "st[p + 14] != ticket / ticket_span() {\n        return 0 - 1;\n    }\n    var n = srv.core.osize"),
    ("stream: the answer's beginning not noted", "    if n > 0 {\n        set_flag(st, p, 4);\n    }", "    if n > 100000 {\n        set_flag(st, p, 4);\n    }"),
    # ---- the API ----
    ("limits: a maximum of 0 accepted", "    if max_body < 1 || piece < 1 || head_ms < 1 || body_ms < 1 {", "    if max_body < 0 || piece < 1 || head_ms < 1 || body_ms < 1 {"),
    ("limits: a piece of 0 accepted", "    if max_body < 1 || piece < 1 || head_ms < 1 || body_ms < 1 {", "    if max_body < 1 || piece < 0 || head_ms < 1 || body_ms < 1 {"),
    ("limits: a head timer of 0 accepted", "    if max_body < 1 || piece < 1 || head_ms < 1 || body_ms < 1 {", "    if max_body < 1 || piece < 1 || head_ms < 0 || body_ms < 1 {"),
    ("limits: a body timer of 0 accepted", "    if max_body < 1 || piece < 1 || head_ms < 1 || body_ms < 1 {", "    if max_body < 1 || piece < 1 || head_ms < 1 || body_ms < 0 {"),
    ("limits: a refused call turns it on", "    if max_body < 1 || piece < 1 || head_ms < 1 || body_ms < 1 { return 0 - 1; }\n    srv.core.lim = 1;", "    srv.core.lim = 1;\n    if max_body < 1 || piece < 1 || head_ms < 1 || body_ms < 1 { return 0 - 1; }"),
    ("limits: the maximum not kept", "    srv.core.maxb = max_body;", ""),
    ("limits: the head timer not kept", "    srv.core.head_ms = head_ms;", ""),
    ("limits: the body timer not kept", "    srv.core.body_ms = body_ms;", ""),
    ("streaming: a body that has all arrived is streaming", "    return st[stride() * k + 16] != 0 && !has(st[stride() * k + 24], 8);", "    return st[stride() * k + 16] != 0;"),
    ("streaming: nothing is", "    return st[stride() * k + 16] != 0 && !has(st[stride() * k + 24], 8);", "    return st[stride() * k + 16] == 7;"),
    ("held_slot: a ticket of another generation", "|| st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 || st[p + 16] == 0 {\n        return 0 - 1;\n    }\n    return k;", "|| st[p + 25] != 0 || st[p + 16] == 0 {\n        return 0 - 1;\n    }\n    return k;"),
    ("held_slot: a request that is not held", "    if st[p + 4] != 1 || st[p + 13] != 1 || st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 || st[p + 16] == 0 {", "    if st[p + 4] != 1 || st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 || st[p + 16] == 0 {"),
    ("held_slot: a request that is over", "|| st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 || st[p + 16] == 0 {\n        return 0 - 1;\n    }\n    return k;", "|| st[p + 14] != ticket / ticket_span() || st[p + 16] == 0 {\n        return 0 - 1;\n    }\n    return k;"),
    ("held_slot: a request with no body state", "|| st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 || st[p + 16] == 0 {\n        return 0 - 1;\n    }\n    return k;", "|| st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 {\n        return 0 - 1;\n    }\n    return k;"),
    ("held_slot: a slot past the table", "    let k = ticket % ticket_span();\n    if k >= core.limit { return 0 - 1; }\n    let st = contents(core.state);\n    let p = stride() * k;\n    if st[p + 4] != 1 || st[p + 13] != 1 || st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 || st[p + 16] == 0", "    let k = ticket % ticket_span();\n    let st = contents(core.state);\n    let p = stride() * k;\n    if st[p + 4] != 1 || st[p + 13] != 1 || st[p + 14] != ticket / ticket_span() || st[p + 25] != 0 || st[p + 16] == 0"),
    ("held_slot: a negative ticket", "fn held_slot[&c](core: &c Core, ticket: int) -> [] int {\n    if ticket < 0 { return 0 - 1; }\n", "fn held_slot[&c](core: &c Core, ticket: int) -> [] int {\n"),
    ("proceed: sent without being asked for", "    if !has(st[p + 24], 2) || has(st[p + 24], 4) { return 0; }", "    if has(st[p + 24], 4) { return 0; }"),
    ("proceed: sent after the answer began", "    if !has(st[p + 24], 2) || has(st[p + 24], 4) { return 0; }", "    if !has(st[p + 24], 2) { return 0; }"),
    ("body_part: more than a piece", "    var n = st[p + 22];\n    if n > core.piece { n = core.piece; }\n    return bf[", "    var n = st[p + 22];\n    return bf["),
    ("body_part: from the wrong place", "    return bf[k * core.size + st[p + 21]..k * core.size + st[p + 21] + n];", "    return bf[k * core.size..k * core.size + n];"),
    ("body_part: a request that is over", "    let k = held_slot(core, ticket);\n    if k < 0 { return bf[0..0]; }", "    var k = held_slot(core, ticket);\n    if k < 0 { k = 0; }"),
    ("body_take: more than was shown", "    if n < 0 || n > can {", "    if n < 0 || n > st[p + 22] {"),
    ("body_take: a negative count", "    if n < 0 || n > can {", "    if n > can {"),
    ("body_take: nothing taken", "    st[p + 21] = st[p + 21] + n;\n    st[p + 22] = st[p + 22] - n;\n    st[p + 27] = srv.core.now;", "    st[p + 27] = srv.core.now;"),
    ("body_take: the start not moved", "    st[p + 21] = st[p + 21] + n;\n    st[p + 22] = st[p + 22] - n;\n    st[p + 27] = srv.core.now;", "    st[p + 22] = st[p + 22] - n;\n    st[p + 27] = srv.core.now;"),
    ("body_take: the count not reduced", "    st[p + 21] = st[p + 21] + n;\n    st[p + 22] = st[p + 22] - n;\n    st[p + 27] = srv.core.now;", "    st[p + 21] = st[p + 21] + n;\n    st[p + 27] = srv.core.now;"),
    ("body_take: the body's clock not restarted", "    st[p + 22] = st[p + 22] - n;\n    st[p + 27] = srv.core.now;", "    st[p + 22] = st[p + 22] - n;"),
    ("body_state: always more to come", "    if has(contents(srv.core.state)[stride() * k + 24], 8) { return 1; }\n    return 0;", "    return 0;"),
    ("body_state: always arrived", "    if has(contents(srv.core.state)[stride() * k + 24], 8) { return 1; }\n    return 0;", "    return 1;"),
    ("body_total: the wrong word", "    return contents(srv.core.state)[stride() * k + 23];", "    return contents(srv.core.state)[stride() * k + 22];"),
]

def rule_mutants():
    """One mutant for each record of the refusal table that a client can see: its status wrong, its message wrong, its rule wrong."""
    text = open(SERVER).read()
    m = re.search(r'"(400\|bad request\|head\.malformed;[^"]*)"', text)
    table = m.group(1)
    out = []
    for i, record in enumerate(table.split(";")):
        status, message, rule_tag = record.split("|")
        if i in (0, 10):
            continue
        if True:
            out.append((f"rule {i}: the wrong status", f'"{table}"', '"' + table.replace(record, "499|" + message + "|" + rule_tag, 1) + '"'))
            out.append((f"rule {i}: the wrong message", f'"{table}"', '"' + table.replace(record, status + "|bad|" + rule_tag, 1) + '"'))
            out.append((f"rule {i}: the wrong rule", f'"{table}"', '"' + table.replace(record, status + "|" + message + "|x.y", 1) + '"'))
    return out


MUTANTS += rule_mutants()

# Mutants the byte-fed tests cannot reach, each with the argument. They must survive.
EQUIVALENT = {
    "clear_body: the last word kept": "word 31 is spare",
    "init_slot: a slot's body state kept for the next connection": "`detach` clears the same words and a slot is only taken after it: either line alone is enough",
    "detach: a slot's body state kept": "`init_slot` clears the same words when the slot is taken again, and an ended or free slot answers 0 or -1 to every call meanwhile",
    "reclaim: done bodies moved too": "the buffer is a valid state either way (the body's untaken bytes and what follows move down together): it only costs a copy",
    "ingest: a recorded refusal is run over": "`readable` is 0 once a refusal is recorded, so no byte arrives to be counted",
    "ingest: a refusal not queued for next": "`input` and `serve_events` queue the connection themselves right after `ingest`",
    "classify: a chunked refusal leaves the request streaming": "the connection ends with the refusal, and its words are cleared when the slot is taken again",
    "reject: a connection that is ending is ended over its answer": "a connection that is ending has no body state left to record a refusal (`end_request` and every refusal clear it)",
    "reject: an answer already ending the connection is added to": "the same: `closing` and a recorded refusal are never both set",
    "sweep_timers: a connection that is ending is timed": "the same: nothing it could time is left",
    "sweep_timers: a refusal recorded twice": "`readable` is 0 once a refusal is recorded, so the body timer cannot fire again, and a request that has body state has no head timer",
    "sweep_timers: a free slot is timed": "a free slot's words are all 0",
    "sweep_timers: a held request's head is timed": "a head timer is only started for an incomplete head, and a request with an incomplete head cannot be held",
    "attach: the clock not kept": "`input` and `ready` set it before anything reads it",
    "answer_in: a request that is over answered": "`deliver_to` refuses it on the same word",
    "answer_in: a streamed request parsed again": "`deliver_to` reads nothing from the parse table for a request with body state",
    "held_slot: a request that is not held": "a request that is not held has no body state (`end_request` and `reject` clear it), and `held_slot` refuses that",
}


# Mutants of the lines only the socket path runs (`step`, `settle`, `wait`, `serve_events`): checked with
# `--socket`, which builds `tests/programs/server_upload.cho` against the mutated source and runs the
# four tests of `conformance/http_server_upload.rs` (the `HTTP_SERVER_SOURCE` variable names the source).
SOCKET = [
    ("step: a length body is read past its end", "        match conns.read(tab, k, bf[base + st[p]..base + st[p] + room]) {", "        match conns.read(tab, k, bf[base + st[p]..base + size]) {"),
    ("step: a full buffer that could be reclaimed closes the connection", "    } else if readable(core, k) == 0 {\n        // No room to read into.", "    } else if st[p] >= size {\n        // No room to read into."),
    ("step: a recorded refusal is read past", "    } else if st[p + 25] != 0 {\n        // A refusal is recorded: nothing is read, and the next `next` sends it.\n    } else if", "    } else if st[p + 25] == 7 {\n    } else if"),
    ("step: ingest not run", "                ingest(core, k, st[p] - got);\n                code = 1;", "                code = 1;"),
    ("step: the room freed by `body_take` not reclaimed", "        if room > size - st[p] {\n            reclaim(core, k);\n        }\n        match conns.read", "        match conns.read"),
    ("settle: a held connection with room is not watched", "} else if st[p + 13] == 1 && readable(core, k) == 0 {", "} else if st[p + 13] == 1 {"),
    ("wait: the clock not kept", "    borrow mut state as &!cw in {\n        cw.now = clock_ms(clock);\n    }", ""),
    ("body_take: the socket not watched again", "    st[p + 27] = srv.core.now;\n    settle(srv.tab, srv.core, k);\n    return n;", "    st[p + 27] = srv.core.now;\n    settle(srv.tab, srv.core, 0);\n    return n;"),
    ("serve_events: no timers", "    sweep_idle(tab, core, now);\n    sweep_timers(tab, core);\n    return 0;", "    sweep_idle(tab, core, now);\n    return 0;"),
]


LIMIT = 45

# The source is compared and mutated as tokens, not as text: `cancho fmt` decides where a line breaks and a mutant says what it changes, not how
# that is laid out. Comments go, a trailing comma before a closing bracket goes, everything is joined by single spaces; the result is a program.
TOKEN = re.compile(r'//[^\n]*|"(?:[^"\\]|\\.)*"|\'(?:[^\'\\]|\\.)\'|\d+(?:\.\d+)?|[A-Za-z_]\w*|==|!=|<=|>=|&&|\|\||->|=>|\.\.|::|\S')


def tokens(text):
    found = [m.group(0) for m in TOKEN.finditer(text) if not m.group(0).startswith("//")]
    return [t for i, t in enumerate(found) if not (t == "," and i + 1 < len(found) and found[i + 1] in (")", "]", "}"))]


def find(source, pattern):
    """Where the token list `pattern` occurs in the token list `source`."""
    n = len(pattern)
    return [i for i in range(len(source) - n + 1) if source[i:i + n] == pattern]


class Ran:
    def __init__(self, returncode, stdout, stderr):
        self.returncode, self.stdout, self.stderr = returncode, stdout, stderr


def run(exe, server_text):
    """`cancho test` on each test file against the (mutated) source. A run that hangs past LIMIT seconds is killed, with the
    test program it started, and counts as a kill."""
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "server.cho")
        with open(path, "w") as f:
            f.write(server_text)
        for test in TESTS:
            p = subprocess.Popen([exe, "test", "--std", path, SUPPORT, test], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                 start_new_session=True)
            try:
                out, err = p.communicate(timeout=LIMIT)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid, signal.SIGKILL)
                p.communicate()
                return Ran(4, "FAILED: hung past the limit", "")
            if p.returncode != 0:
                return Ran(p.returncode, out, err)
    return Ran(0, "", "")


def mutate(source, old, new):
    """`source` (tokens) with the one occurrence of `old` replaced by `new` (text); (text, how many times `old` occurs)."""
    old_t, new_t = tokens(old), tokens(new)
    at = find(source, old_t)
    if len(at) != 1:
        return None, len(at)
    return " ".join(source[:at[0]] + new_t + source[at[0] + len(old_t):]), 1


def judge(job):
    exe, name, old, new, source = job
    text, n = mutate(source, old, new)
    if text is None:
        return name, "BAD", f"`old` occurs {n} times"
    r = run(exe, text)
    if r.returncode == 4 or "FAILED" in r.stdout:
        return name, "killed", ""
    if r.returncode == 0:
        return name, "SURVIVED", ""
    return name, "NOBUILD", (r.stderr or r.stdout)[:300]


def judge_socket(job):
    name, old, new, source = job
    text, n = mutate(source, old, new)
    if text is None:
        return name, "BAD", f"`old` occurs {n} times"
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "server.cho")
        with open(path, "w") as f:
            f.write(text)
        env = dict(os.environ, HTTP_SERVER_SOURCE=path)
        p = subprocess.Popen(["cargo", "test", "--release", "-p", "cancho", "--test", "conformance", "http_server_upload"], cwd=ROOT, env=env,
                             stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
        try:
            out, _ = p.communicate(timeout=300)
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.communicate()
            return name, "killed", ""
    if p.returncode == 0:
        return name, "SURVIVED", ""
    if "test result: FAILED" in out or "panicked" in out:
        return name, "killed", ""
    return name, "NOBUILD", out[-300:]


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    exe = os.path.abspath(args.pop(0))
    only = None
    jobs = 4
    socket_mode = "--socket" in args
    if socket_mode:
        args.remove("--socket")
    if "--only" in args:
        k = args.index("--only")
        only = args[k + 1]
        del args[k:k + 2]
    if "--jobs" in args:
        k = args.index("--jobs")
        jobs = int(args[k + 1])
        del args[k:k + 2]
    source = tokens(open(SERVER).read())
    results = []
    if socket_mode:
        chosen = [m for m in SOCKET if only is None or only in m[0]]
        for name, verdict, detail in map(judge_socket, [(n, o, w, source) for n, o, w in chosen]):
            results.append((name, verdict))
            print(f"{verdict:10} {name}" + (f"   {detail}" if detail and verdict != "killed" else ""), flush=True)
    else:
        base = run(exe, " ".join(source))
        if base.returncode != 0:
            print("the unmutated source does not pass:\n" + base.stdout + base.stderr)
            return 1
        chosen = [m for m in MUTANTS if only is None or only in m[0]]
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
