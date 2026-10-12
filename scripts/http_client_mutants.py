#!/usr/bin/env python3
"""Mutation check of `packages/http-client` (docs/http-client.md §9), the shape of `scripts/http_server_bytes_mutants.py`.

    python3 scripts/http_client_mutants.py <cancho binary> [--only <text in a mutant's name>] [--jobs <n>]

Each mutant is `packages/http-client/wire.cho` or `client.cho` (`slot.cho` is only names for integers) with one deliberate bug at one site, copied to a
scratch directory with the other, and the tests of `tests/packages/http_client_*.cho` are run against it (`cancho test`). A mutant is killed when a test
fails or traps. The unmutated source is run first and must pass. A mutant that changes nothing the tests can reach is in EQUIVALENT
with the argument, and must survive. Exit status 1 if a mutant survives, an `old` text does not occur exactly once, or a mutant fails
to build (a mutant that does not compile proves nothing). A run that hangs for 60 seconds (the tests take 3) counts as killed.

The live tests (`scripts/http_client_test.py`) are not run here: they need servers and take a minute.
"""
import concurrent.futures
import os
import signal
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WIRE = os.path.join(ROOT, "packages/http-client/wire.cho")
SLOT = os.path.join(ROOT, "packages/http-client/slot.cho")
CLIENT = os.path.join(ROOT, "packages/http-client/client.cho")
TESTS = [os.path.join(ROOT, "tests/packages", f) for f in (
    "http_client_harness.cho", "http_client_test.cho", "http_client_pool_test.cho", "http_client_time_test.cho",
    "http_client_wire_test.cho", "http_client_flow_test.cho", "http_client_edge_test.cho")]
LIMIT = 60

W, C = "wire", "client"
MUTANTS = []


def m(file, name, old, new):
    MUTANTS.append((file, name, old, new))


# ======================================================================================================================
# wire.cho: the request
# ======================================================================================================================
m(W, "valid_method: CONNECT allowed", '    return !bytes.equal(method, "CONNECT");', "    return true;")
m(W, "valid_method: the empty method allowed", "    if len(method) < 1 || len(method) > 32 {", "    if len(method) < 0 || len(method) > 32 {")
m(W, "valid_method: a method of 33 bytes allowed", "    if len(method) < 1 || len(method) > 32 {", "    if len(method) < 1 || len(method) > 33 {")
m(W, "valid_method: a space allowed in a method", "        if !is_tchar(int_of(method[i])) {\n            return false;", "        if int_of(method[i]) == 0 {\n            return false;")
m(W, "valid_target: a target that does not start with / allowed", "    if int_of(target[0]) != 47 {", "    if int_of(target[0]) == 0 {")
m(W, "valid_target: a fragment allowed", "        if !is_target_char(c) || c == 35 {", "        if !is_target_char(c) {")
m(W, "valid_target: a space allowed", "        if !is_target_char(c) || c == 35 {", "        if c == 35 {")
m(W, "valid_target: `*` refused", '    if bytes.equal(target, "*") {\n        return true;', '    if bytes.equal(target, "?") {\n        return true;')
m(W, "valid_target: 8,193 bytes allowed", "    if len(target) < 1 || len(target) > 8192 {", "    if len(target) < 1 || len(target) > 8193 {")
m(W, "valid_target: the empty target allowed", "    if len(target) < 1 || len(target) > 8192 {", "    if len(target) < 0 || len(target) > 8192 {")
m(W, "valid_host: the empty host allowed", "    if len(host) < 1 || len(host) > 255 {", "    if len(host) < 0 || len(host) > 255 {")
m(W, "valid_host: 256 bytes allowed", "    if len(host) < 1 || len(host) > 255 {", "    if len(host) < 1 || len(host) > 256 {")
m(W, "valid_host: a slash allowed", "if !(letter || digit || c == 46 || c == 45 || c == 95 || c == 58 || c == 91 || c == 93 || c == 37 || c == 126) {", "if !(letter || digit || c == 46 || c == 45 || c == 95 || c == 58 || c == 91 || c == 93 || c == 37 || c == 126 || c == 47) {")
m(W, "valid_host: a colon refused", "if !(letter || digit || c == 46 || c == 45 || c == 95 || c == 58 || c == 91 || c == 93 || c == 37 || c == 126) {", "if !(letter || digit || c == 46 || c == 45 || c == 95 || c == 91 || c == 93 || c == 37 || c == 126) {")
m(W, "valid_host: a bracket refused", "if !(letter || digit || c == 46 || c == 45 || c == 95 || c == 58 || c == 91 || c == 93 || c == 37 || c == 126) {", "if !(letter || digit || c == 46 || c == 45 || c == 95 || c == 58 || c == 93 || c == 37 || c == 126) {")
m(W, "valid_host: a percent sign refused", "if !(letter || digit || c == 46 || c == 45 || c == 95 || c == 58 || c == 91 || c == 93 || c == 37 || c == 126) {", "if !(letter || digit || c == 46 || c == 45 || c == 95 || c == 58 || c == 91 || c == 93 || c == 126) {")
m(W, "valid_host: an at sign allowed", "if !(letter || digit || c == 46 || c == 45 || c == 95 || c == 58 || c == 91 || c == 93 || c == 37 || c == 126) {", "if !(letter || digit || c == 46 || c == 45 || c == 95 || c == 58 || c == 91 || c == 93 || c == 37 || c == 126 || c == 64) {")
for name in ["host", "content-length", "transfer-encoding", "connection", "expect", "upgrade", "te", "trailer", "keep-alive", "proxy-connection"]:
    m(W, f"client_owns: `{name}` allowed in the caller's lines", f' || eq_lower(src, start, end, "{name}")' if name != "host" else 'return eq_lower(src, start, end, "host") || ', " " if name != "host" else "return ")
m(W, "valid_extra: a header with no colon allowed", "        if i == name_start || i >= len(extra) || int_of(extra[i]) != 58 {", "        if i >= len(extra) || int_of(extra[i]) != 58 {")
m(W, "valid_extra: a name of no characters allowed", "        if i == name_start || i >= len(extra) || int_of(extra[i]) != 58 {", "        if i >= len(extra) || int_of(extra[i]) != 58 || i < 0 {")
m(W, "valid_extra: a line with no CRLF allowed", "        if i + 1 >= len(extra) || int_of(extra[i]) != 13 || int_of(extra[i + 1]) != 10 {\n            return false;\n        }\n        i = i + 2;", "        if i + 1 >= len(extra) || int_of(extra[i]) != 13 {\n            return false;\n        }\n        i = i + 2;")
m(W, "valid_extra: a bare LF as a line end allowed", "        if i + 1 >= len(extra) || int_of(extra[i]) != 13 || int_of(extra[i + 1]) != 10 {\n            return false;\n        }\n        i = i + 2;", "        if i + 1 >= len(extra) || int_of(extra[i]) != 10 && int_of(extra[i]) != 13 {\n            return false;\n        }\n        i = i + 2;")
m(W, "wants_zero_length: POST has no Content-Length: 0", '    return bytes.equal(method, "POST") || bytes.equal(method, "PUT") || bytes.equal(method, "PATCH");', '    return bytes.equal(method, "PUT") || bytes.equal(method, "PATCH");')
m(W, "wants_zero_length: PUT has no Content-Length: 0", '    return bytes.equal(method, "POST") || bytes.equal(method, "PUT") || bytes.equal(method, "PATCH");', '    return bytes.equal(method, "POST") || bytes.equal(method, "PATCH");')
m(W, "wants_zero_length: PATCH has no Content-Length: 0", '    return bytes.equal(method, "POST") || bytes.equal(method, "PUT") || bytes.equal(method, "PATCH");', '    return bytes.equal(method, "POST") || bytes.equal(method, "PUT");')
m(W, "wants_zero_length: every method has one", '    return bytes.equal(method, "POST") || bytes.equal(method, "PUT") || bytes.equal(method, "PATCH");', "    return true;")
m(W, "write_head: Content-Length written for a body of 0 on a GET", "    if body >= 0 && (body > 0 || wants_zero_length(method)) {", "    if body >= 0 {")
m(W, "write_head: no Content-Length for a body of 1", "    if body >= 0 && (body > 0 || wants_zero_length(method)) {", "    if body > 1 || body >= 0 && wants_zero_length(method) {")
m(W, "write_head: chunked not announced", '        at = put(out, at, "Transfer-Encoding: chunked\\r\\n");', "")
m(W, "write_head: Expect not written", '        at = put(out, at, "Expect: 100-continue\\r\\n");', "")
m(W, "write_head: Connection: close not written", '        at = put(out, at, "Connection: close\\r\\n");', "")
m(W, "write_head: the caller's lines not written", "    at = put(out, at, extra);", "")
m(W, "write_head: no blank line", '    return put(out, at, "\\r\\n");\n}\n\n// How many hex digits', "    return at;\n}\n\n// How many hex digits")
m(W, "write_head: the target is written twice", "    at = put(out, at, target);", "    at = put(out, at, target);\n    at = put(out, at, target[0..1]);")
m(W, "put: a write that does not fit is partial", "    if at < 0 || at + len(s) > len(out) {\n        return 0 - 1;\n    }\n    copy_into(out[at..at + len(s)], s);", "    if at < 0 || at + len(s) > len(out) + 1 {\n        return 0 - 1;\n    }\n    copy_into(out[at..at + len(s)], s);")
m(W, "put_number: the number is one digit short", "    var digits = 1;\n    var rest = n / 10;\n    while rest > 0 {\n        digits = digits + 1;\n        rest = rest / 10;\n    }\n    if at + digits > len(out) {", "    var digits = 1;\n    var rest = n / 100;\n    while rest > 0 {\n        digits = digits + 1;\n        rest = rest / 10;\n    }\n    if at + digits > len(out) {")
m(W, "put_number: digits written in the wrong order", "        out[at + i - 1] = byte_of(48 + v % 10);", "        out[at + digits - i] = byte_of(48 + v % 10);")
m(W, "hex_digits: a size of 16 takes one digit", "    var d = 1;\n    var r = n / 16;\n    while r > 0 {\n        d = d + 1;\n        r = r / 16;\n    }", "    var d = 1;\n    var r = n / 17;\n    while r > 0 {\n        d = d + 1;\n        r = r / 16;\n    }")
m(W, "chunk_overhead: the CRLFs not counted", "    return hex_digits(n) + 4;", "    return hex_digits(n) + 2;")
m(W, "chunk_fit: the largest piece is one too big", "    while n > 0 && n + chunk_overhead(n) > room {", "    while n > 0 && n + chunk_overhead(n) > room + 1 {")
m(W, "chunk_fit: one byte of room is enough for a chunk", "    var n = room - 5;\n    if n < 1 {\n        return 0;\n    }", "    var n = room - 5;\n    if n < 0 {\n        return 0;\n    }")
m(W, "write_chunk: the size in lower case is upper", "            out[at + i - 1] = byte_of(87 + h);", "            out[at + i - 1] = byte_of(55 + h);")
m(W, "write_chunk: a chunk that does not fit is written", "    if at < 0 || at + n + digits + 4 > len(out) {", "    if at < 0 || at + n + digits + 3 > len(out) {")
m(W, "write_chunk: no CRLF after the data", "    out[at + digits + 2 + n] = byte_of(13);\n    out[at + digits + 3 + n] = byte_of(10);", "    out[at + digits + 2 + n] = byte_of(10);\n    out[at + digits + 3 + n] = byte_of(10);")
m(W, "write_last_chunk: no trailer section", '    return put(out, at, "0\\r\\n\\r\\n");', '    return put(out, at, "0\\r\\n");')

# ======================================================================================================================
# wire.cho: the response head
# ======================================================================================================================
m(W, "head_end: a bare LF accepted", "            if i == 0 || int_of(src[i - 1]) != 13 {\n                return 0 - 2 - i;\n            }", "            if i == 0 {\n                return 0 - 2 - i;\n            }")
m(W, "head_end: the blank line is looked for from the start only", "    var i = from;\n    if i < 0 {\n        i = 0;\n    }", "    var i = 0;\n    if from < 0 {\n        i = 0;\n    }")
m(W, "head_end: a blank line is found one byte early", "            if i >= 3 && int_of(src[i - 2]) == 10 && int_of(src[i - 3]) == 13 {", "            if i >= 3 && int_of(src[i - 2]) == 10 && int_of(src[i - 3]) == 10 {")
m(W, "head_end: the limit is ignored", "    while i < limit {\n        if int_of(src[i]) == 10 {", "    while i < len(src) {\n        if int_of(src[i]) == 10 {")
m(W, "looks_like_http: anything is HTTP", "        if int_of(src[i]) != int_of(want[i]) {\n            return false;\n        }", "        if int_of(src[i]) == 0 {\n            return false;\n        }")
m(W, "parse_head: a head over the limit is incomplete, not too large", "        if n >= cap {\n            return fail(cap, c_head_too_large());\n        }", "        if n > cap {\n            return fail(cap, c_head_too_large());\n        }")
m(W, "parse_head: the limit is ignored", "    var cap = limit;\n    if cap > max_head() {\n        cap = max_head();\n    }", "    var cap = limit;")
m(W, "parse_head: HTTP/2 is a status line error", "        if bytes.is_digit(major) {\n            return fail(at, c_version());\n        }", "        if bytes.is_digit(major) && major == 0 {\n            return fail(at, c_version());\n        }")
m(W, "parse_head: HTTP/1.2 accepted", "    } else if bytes.is_digit(minor) {\n        return fail(at, c_version());", "    } else if bytes.is_digit(minor) && minor == 0 {\n        return fail(at, c_version());")
m(W, "parse_head: HTTP/1.0 read as 1.1", "    } else if minor == 48 {\n        table[1] = 10;", "    } else if minor == 48 {\n        table[1] = 11;")
m(W, "parse_head: a status of two digits accepted", "        if !bytes.is_digit(c) {\n            return fail(at + d, c_status_line());\n        }\n        code = code * 10 + (c - 48);", "        if !bytes.is_digit(c) && d == 2 {\n            return fail(at + d, c_status_line());\n        }\n        code = code * 10 + (c - 48);")
m(W, "parse_head: a status under 100 accepted", "    if code < 100 || code > 599 {", "    if code < 0 || code > 599 {")
m(W, "parse_head: a status over 599 accepted", "    if code < 100 || code > 599 {", "    if code < 100 || code > 999 {")
m(W, "parse_head: 599 refused", "    if code < 100 || code > 599 {", "    if code < 100 || code > 598 {")
m(W, "parse_head: 100 refused", "    if code < 100 || code > 599 {", "    if code < 101 || code > 599 {")
m(W, "parse_head: no space after the status", "    if int_of(src[at]) != 32 {\n        return fail(at, c_status_line());\n    }\n    at = at + 1;\n    table[6] = at;", "    if int_of(src[at]) == 0 {\n        return fail(at, c_status_line());\n    }\n    at = at + 1;\n    table[6] = at;")
m(W, "parse_head: a control byte in the reason accepted", "    while at < lines_end && is_value_char(int_of(src[at])) {\n        at = at + 1;\n    }\n    table[7] = at;", "    while at < lines_end && int_of(src[at]) != 13 {\n        at = at + 1;\n    }\n    table[7] = at;")
m(W, "parse_head: the status line need not end CRLF", "    if at + 1 >= lines_end || int_of(src[at]) != 13 || int_of(src[at + 1]) != 10 {\n        return fail(at, c_status_line());\n    }", "    if at + 1 >= lines_end {\n        return fail(at, c_status_line());\n    }")
m(W, "parse_head: obs-fold accepted", "        if is_ows(int_of(src[at])) {\n            return fail(at, c_fold());\n        }", "        if int_of(src[at]) == 0 {\n            return fail(at, c_fold());\n        }")
m(W, "parse_head: a header name with a space accepted", "        while at < lines_end && is_tchar(int_of(src[at])) {\n            at = at + 1;\n        }\n        if at == name_start || at >= lines_end || int_of(src[at]) != 58 {", "        while at < lines_end && int_of(src[at]) != 58 && int_of(src[at]) != 13 {\n            at = at + 1;\n        }\n        if at == name_start || at >= lines_end || int_of(src[at]) != 58 {")
m(W, "parse_head: an empty header name accepted", "        if at == name_start || at >= lines_end || int_of(src[at]) != 58 {", "        if at >= lines_end || int_of(src[at]) != 58 {")
m(W, "parse_head: a header with no colon accepted", "        if at == name_start || at >= lines_end || int_of(src[at]) != 58 {\n            return fail(at, c_resp_header());", "        if at == name_start || at >= lines_end {\n            return fail(at, c_resp_header());")
m(W, "parse_head: a control byte in a value accepted", "        while at < lines_end && is_value_char(int_of(src[at])) {\n            at = at + 1;\n        }\n        if at + 1 >= lines_end || int_of(src[at]) != 13", "        while at < lines_end && int_of(src[at]) != 13 {\n            at = at + 1;\n        }\n        if at + 1 >= lines_end || int_of(src[at]) != 13")
m(W, "parse_head: a header need not end CRLF", "        if at + 1 >= lines_end || int_of(src[at]) != 13 || int_of(src[at + 1]) != 10 {\n            return fail(at, c_resp_header());\n        }\n        var value_end", "        if at + 1 >= lines_end || int_of(src[at]) != 13 {\n            return fail(at, c_resp_header());\n        }\n        var value_end")
m(W, "parse_head: trailing blanks kept in a value", "        while value_end > value_start && is_ows(int_of(src[value_end - 1])) {\n            value_end = value_end - 1;\n        }", "")
m(W, "parse_head: a 65th header accepted into the table", "        if count >= capacity {\n            return fail(name_start, c_resp_header());\n        }", "        if count > capacity {\n            return fail(name_start, c_resp_header());\n        }")
m(W, "parse_head: the header count not kept", "    table[2] = count;\n    table[3] = body_start;", "    table[3] = body_start;")
m(W, "parse_head: the body starts one byte early", "    table[2] = count;\n    table[3] = body_start;", "    table[2] = count;\n    table[3] = body_start - 1;")
m(W, "digits_value: a Content-Length of 16 digits accepted", "    let n = end - start;\n    if n < 1 || n > 15 {", "    let n = end - start;\n    if n < 1 || n > 16 {")
m(W, "digits_value: an empty Content-Length accepted", "    let n = end - start;\n    if n < 1 || n > 15 {", "    let n = end - start;\n    if n < 0 || n > 15 {")
m(W, "digits_value: a Content-Length with a sign accepted", "        if d < 0 {\n            return 0 - 1;\n        }\n        value = value * 10 + d;", "        if d < 0 && int_of(src[start + i]) != 43 {\n            return 0 - 1;\n        }\n        value = value * 10 + d;")
m(W, "parse_head: a repeated Content-Length with the same value accepted", "            if v < 0 || length >= 0 {", "            if v < 0 || length >= 0 && length != v {")
m(W, "parse_head: a repeated Content-Length with another value accepted", "            if v < 0 || length >= 0 {", "            if v < 0 {")
m(W, "parse_head: Content-Length is case sensitive", '        if eq_lower(src, ns, ne, "content-length") {', '        if eq_lower(src, ns, ne, "Content-Length") {')
m(W, "parse_head: Transfer-Encoding gzip, chunked accepted", '            if chunked || !eq_lower(src, vs, ve, "chunked") || table[1] == 10 {', "            if chunked || table[1] == 10 {")
m(W, "parse_head: a repeated Transfer-Encoding accepted", '            if chunked || !eq_lower(src, vs, ve, "chunked") || table[1] == 10 {', '            if !eq_lower(src, vs, ve, "chunked") || table[1] == 10 {')
m(W, "parse_head: chunked accepted from an HTTP/1.0 server", '            if chunked || !eq_lower(src, vs, ve, "chunked") || table[1] == 10 {', '            if chunked || !eq_lower(src, vs, ve, "chunked") {')
m(W, "parse_head: Transfer-Encoding is case sensitive", '        } else if eq_lower(src, ns, ne, "transfer-encoding") {', '        } else if eq_lower(src, ns, ne, "Transfer-Encoding") {')
m(W, "parse_head: a length and chunked accepted together", "    if chunked && length >= 0 {\n        return fail(0, c_two_lengths());\n    }", "")
m(W, "parse_head: `Connection: close` not noticed", '            if lists_token(src, vs, ve, "close") {\n                closing = true;\n            }', "")
m(W, "parse_head: `Connection: keep-alive` not noticed", '            if lists_token(src, vs, ve, "keep-alive") {\n                keeping = true;\n            }', "")
m(W, "parse_head: the flags not kept", "    table[5] = flags;\n    return body_start;", "    return body_start;")
m(W, "parse_head: chunked is flag 2", "    if chunked {\n        flags = flags + 1;\n    }", "    if chunked {\n        flags = flags + 2;\n    }")
m(W, "parse_head: the length not kept", "    table[4] = length;\n    var flags = 0;", "    var flags = 0;")
m(W, "lists_token: only the first token is looked at", "            from = i + 1;\n        }", "            return false;\n        }")
m(W, "lists_token: blanks around a token matter", "            while a < z && is_ows(int_of(src[a])) {\n                a = a + 1;\n            }\n            while z > a && is_ows(int_of(src[z - 1])) {\n                z = z - 1;\n            }\n", "")
m(W, "find_header: the first header only", "    while i < table[2] {\n        if eq_lower(src, table[16 + 4 * i], table[16 + 4 * i + 1], name) {\n            return i;\n        }\n        i = i + 1;\n    }\n    return 0 - 1;", "    if table[2] > 0 && eq_lower(src, table[16], table[17], name) {\n        return 0;\n    }\n    return 0 - 1;")

# ======================================================================================================================
# wire.cho: the chunk decoder
# ======================================================================================================================
m(W, "chunk_run: data not moved to the front", "            if wp != i {\n                copy_within(buf, wp, i, take);\n            }", "")
m(W, "chunk_run: more data taken than the chunk has", "            var take = size;\n            if take > to - i {\n                take = to - i;\n            }", "            var take = to - i;\n            if take > size {\n                take = to - i;\n            }")
m(W, "chunk_run: data taken beyond what has arrived", "            var take = size;\n            if take > to - i {\n                take = to - i;\n            }", "            var take = size;")
m(W, "chunk_run: a chunk's data is not followed by CRLF", "            if size == 0 {\n                state = 4;\n            }", "            if size == 0 {\n                state = 0;\n            }")
m(W, "chunk_run: hex digits are decimal", "                    size = size * 16 + v;", "                    size = size * 10 + v;")
m(W, "chunk_run: a size line of 129 bytes accepted", "                    if size > max_chunk() || line > max_line() {", "                    if size > max_chunk() || line > max_line() + 1 {")
m(W, "chunk_run: a size over 2^50 accepted", "                    if size > max_chunk() || line > max_line() {", "                    if size > max_chunk() * 4 || line > max_line() {")
m(W, "chunk_run: an extension with no size accepted", "                } else if c == 59 && digits > 0 {", "                } else if c == 59 {")
m(W, "chunk_run: a size line with no digits accepted", "                } else if c == 13 && digits > 0 {", "                } else if c == 13 {")
m(W, "chunk_run: an extension of any length", "                line = line + 1;\n                if line > max_line() {\n                    err = c_chunk();\n                } else if c == 13 {", "                line = line + 1;\n                if c == 13 {")
m(W, "chunk_run: a control byte in an extension accepted", "                } else if !is_value_char(c) {\n                    err = c_chunk();\n                }\n            } else if state == 2 {", "                }\n            } else if state == 2 {")
m(W, "chunk_run: a size line need not end with LF", "            } else if state == 2 {\n                if c != 10 {\n                    err = c_chunk();\n                } else if size == 0 {", "            } else if state == 2 {\n                if c == 0 {\n                    err = c_chunk();\n                } else if size == 0 {")
m(W, "chunk_run: the last chunk starts data", "                } else if size == 0 {\n                    state = 6;\n                    line = 0;\n                } else {\n                    state = 3;\n                    line = 0;\n                }", "                } else {\n                    state = 3;\n                    line = 0;\n                }")
m(W, "chunk_run: the CR after data not required", "            } else if state == 4 {\n                if c != 13 {\n                    err = c_chunk();\n                } else {\n                    state = 5;\n                }", "            } else if state == 4 {\n                state = 5;")
m(W, "chunk_run: the LF after data not required", "            } else if state == 5 {\n                if c != 10 {\n                    err = c_chunk();\n                } else {\n                    state = 0;", "            } else if state == 5 {\n                if c == 0 {\n                    err = c_chunk();\n                } else {\n                    state = 0;")
m(W, "chunk_run: the next size line starts with the old size", "                    state = 0;\n                    size = 0;\n                    digits = 0;\n                    line = 0;", "                    state = 0;\n                    digits = 0;\n                    line = 0;")
m(W, "chunk_run: the next size line starts with the old digits", "                    state = 0;\n                    size = 0;\n                    digits = 0;\n                    line = 0;", "                    state = 0;\n                    size = 0;\n                    line = 0;")
m(W, "chunk_run: the next size line starts with the old length", "                    state = 0;\n                    size = 0;\n                    digits = 0;\n                    line = 0;", "                    state = 0;\n                    size = 0;\n                    digits = 0;")
m(W, "chunk_run: trailers not skipped", "                } else {\n                    state = 7;\n                    line = line + 1;\n                }", "                } else {\n                    err = c_chunk();\n                }")
m(W, "chunk_run: a bare LF in the trailer section accepted", "                if c == 13 {\n                    state = 9;\n                } else if c == 10 {\n                    err = c_chunk();\n                } else {", "                if c == 13 {\n                    state = 9;\n                } else {")
m(W, "chunk_run: trailers of any size", "                if line > max_trailers() {\n                    err = c_trailers_too_large();\n                } else if c == 13 {", "                if c == 13 {")
m(W, "chunk_run: a trailer line may be one byte over", "                if line > max_trailers() {", "                if line > max_trailers() + 1 {")
m(W, "chunk_run: a bare LF inside a trailer accepted", "                } else if c == 10 {\n                    err = c_chunk();\n                }\n            } else if state == 8 {", "                }\n            } else if state == 8 {")
m(W, "chunk_run: the trailer line's LF not required", "            } else if state == 8 {\n                if c != 10 {\n                    err = c_chunk();\n                } else {\n                    state = 6;", "            } else if state == 8 {\n                if c == 0 {\n                    err = c_chunk();\n                } else {\n                    state = 6;")
m(W, "chunk_run: the final LF not required", "            } else if state == 9 {\n                if c != 10 {\n                    err = c_chunk();\n                } else {\n                    state = 10;", "            } else if state == 9 {\n                if c == 0 {\n                    err = c_chunk();\n                } else {\n                    state = 10;")
m(W, "chunk_run: the body does not end at the last chunk", "    while i < to && state != chunk_done() && err == 0 {", "    while i < to && err == 0 {")
m(W, "chunk_run: the bytes used not reported", "    st[at + 4] = i - from;", "    st[at + 4] = to - from;")
m(W, "chunk_run: the write end not reported", "    st[at + 3] = wp;", "    st[at + 3] = i;")
m(W, "chunk_run: the state not kept between calls", "    st[at] = state;\n    st[at + 1] = size;", "    st[at + 1] = size;")
m(W, "chunk_run: the size not kept between calls", "    st[at + 1] = size;\n    st[at + 2] = line;", "    st[at + 2] = line;")
m(W, "chunk_run: the line length not kept between calls", "    st[at + 2] = line;\n    st[at + 3] = wp;", "    st[at + 3] = wp;")
m(W, "chunk_run: the digits not kept between calls", "    st[at + 5] = digits;\n    return err;", "    return err;")

EQUIVALENT = {
    # ---- wire.cho ----
    "chunk_fit: one byte of room is enough for a chunk": "`room - 5` is 0 for a room of 5, the loop is not entered for 0, and 0 is answered either way",
    "head_end: the blank line is looked for from the start only": "the same line is found; only how much of the bytes already seen is scanned again differs, which no answer shows",
    "digits_value: a Content-Length with a sign accepted": "a `+` is read as a digit of value -1, which makes the number negative (and later digits only keep it so), and a negative number is refused by the caller's `v < 0` just the same",
    "chunk_run: the next size line starts with the old size": "`size` is already 0 when a chunk's data has all been taken (it counts down to 0), so setting it again changes nothing",
    "chunk_run: the next size line starts with the old length": "`line` is 0 when a chunk's data starts (the size line's LF sets it) and data does not count it, so it is 0 again at the end",
    # ---- client.cho ----
    "open: the memory budget ignored": "shown only by an open of more than 1 GiB; run by hand once (64 slots of 16 MiB + 16 MiB: it clamps to 32), and not put in a test that must allocate a gibibyte",
    "slot_of_ticket: ticket 0 names slot 0": "every function that takes a ticket also needs a live request or a head that was read, which a slot never used has not: the generation test is a second guard",
    "close_slot: a waiting Connect is still delivered": "`poll` delivers a Connect only for a slot that is dialling, and a closing slot is not",
    "fail: a replay still waiting": "`fail` is reached from a dial or a request in progress, where `reconnect` is 0; a replay is a closing slot, and `transport_ended` returns before `fail` for one",
    "fail: a Done already waiting is still delivered": "`fail` is reached only for a response not yet complete and consumed (a finished one ignores the transport), so no Done is waiting",
    "pooled: a connection with an event waiting taken": "an idle connection has no event waiting: `poll` delivers every lower event before `Done`, and `Done` is what makes it idle",
    "oldest_idle: the key's own connection is closed": "`oldest_idle` is asked only when `pooled` found no idle connection of the key, and an idle one has no event waiting",
    "send_room: a request with no body has room": "a request with no body has `bleft` 0, and the clamp to what a length body has left answers 0 whatever the room",
    "send_room: a full replayable buffer has no room again": "`take` reclaims the moment everything is sent, which drops replayability and empties the buffer, so the state this branch describes is never seen",
    "send_body: a held body taken": "`send_room` is 0 while held, so `room <= 0` answers 0 first",
    "send_body: a chunk that did not fit is counted": "`n` is at most `chunk_fit(room)`, which fits by construction, so `write_chunk` does not refuse",
    "pending: a request not live has bytes": "a request that is not live is not on an active connection: `abort`, `fail` and `Done` each end the active phase",
    "pending: the body behind a held head is offered": "while held `send_room` is 0, so nothing has been written behind the head",
    "take: a held request is done": "a request is held only with a body to come, so `body_done` is false until the hold is over",
    "take: the buffer is not reclaimed": "`send_room` counts the bytes taken as room for a request that cannot be replayed, and `send_body` reclaims before it writes: the same answers",
    "room: a closing connection has room": "an active connection whose request is not live does not exist: `abort`, `fail` and `Done` each end the active phase",
    "room: a response waiting to be reported has room": "`rs_done` is only ever set together with `complete`, which is tested next to it",
    "give: bytes for a request that is not live are kept": "the same: an active connection whose request is not live does not exist",
    "finish_body: Done twice": "`finish_body` runs once per response: after `rs_done` no path (`give`, `consume`, `transport_ended`) reaches it",
    "read_head: the blank line is looked for from the start of the buffer each time": "as `head_end`: the same answers, more scanning",
    "read_head: the scan position not kept": "as `head_end`: the same answers, more scanning",
    "read_head: the chunk decoder not started afresh": "`request` zeroes all of a slot's words, the decoder's among them, before every request",
    "read_body: a complete body is read again": "`give` and `room` stop at a complete response, so `read_body` is not called again for it",
    "read_body: bytes behind a chunked body are lost": "what is behind a complete chunked body is never read: only that there is some (`dirty`) is used",
    "read_body: the fill after a chunked body is wrong": "the same: the bytes behind the body, and so the fill that counts them, are never read",
    "head_readable: the head is readable before it has come": "before the head `hl` is 0",
    "body: a body once the response is done": "`avail` is 0 then",
    "avail: bytes before the head": "`avail` is 0 before the head",
    "consume: the clock restarts for any bytes taken": "the body timer is off while any byte waits, so a restart at a partial `consume` is never seen; the last `consume` restarts it either way",
    "can_replay: a request is replayed after a response byte": "`transport_ended` asks the same of `anyresp` itself, in the line above the call",
    "can_replay: a request is replayed without the caller's leave": "a request is replayable only if the caller allowed a retry: `request` sets both together",
    "can_replay: a request is replayed twice": "after a replay the connection is a new one, which is not `reused`, and `can_replay` asks that too",
    "replay: the new connection is called reused": "`connected` sets `reused` to 0 before anything reads it",
    "replay: the fill of the old response is kept": "a replay needs no response byte, so the fill is 0",
    "transport_ended: a failed request fails again": "the phase test above it already returns for a closing slot",
    "transport_ended: a replay after a response byte": "`can_replay` asks for no response byte itself",
    "transport_ended: a body to the end of the connection is not complete": "`complete` is read only by `give` and `room`, which a caller does not use on a connection that has ended",
    "transport_ended: a body to the end of the connection can be reused": "`reusable` also refuses a body that runs to the end of the connection",
    "detach: a request that was given up reconnects": "`abort` and `fail` clear `reconnect`",
    "detach: the progress clock not restarted": "the clock is read only on an active connection, and `connected` sets it",
    "detach: the response state not restarted": "the state is already `head`: a replay happens before any byte of a response, and `request` sets it for an evicted slot",
    "detach: the reconnect flag is kept": "the next detach of the slot is after a failure or a `Done`, which clear it or end the request (`live` 0)",
    "abort: a replay still waiting": "`detach` also needs the request to be live, which `abort` ends",
    "abort: the dial still announced": "`poll` delivers a Connect only for a slot that is dialling",
    "reusable: a body to the end of the connection is reused": "such a body ends only with the connection, which sets `eof`, tested next to it",
    "reusable: a request not fully sent is reused": "a request not fully sent when its head arrives has an early response (`early`), tested next to it, and it is not fully sent at `Done` only then",
    "reusable: an early response is reused": "an early response is one whose request was not fully sent (`req_done`), tested next to it",
    "poll: a stale Connect is delivered": "`close_slot`, `fail` and `abort` each clear a waiting Connect",
    "due: a total timer for a request that is over": "a request that is over is neither dialling nor active (Done leaves the connection idle or closing), and while its `Done` waits `rs_done` excludes it",
    "due: a body timer on a complete body": "a complete body that was consumed is `rs_done`, which has no timers, and one that was not has bytes waiting, which have none",
    "next_deadline: a timer due is not 0": "at `best == now` the answer is `best - now`, which is 0 either way",
}


# ======================================================================================================================
# client.cho
# ======================================================================================================================
m(C, "open: one slot too many allowed", "    if n > 1024 {\n        n = 1024;\n    }", "    if n > 1025 {\n        n = 1025;\n    }")
m(C, "open: the memory budget ignored", "    while n > 1 && n * (insz + outsz) > budget() {", "    while n > 1 && n * (insz + outsz) > budget() * 1000 {")
m(C, "open: a small input buffer accepted", "    if insz < 2048 {\n        insz = 2048;\n    }", "    if insz < 1 {\n        insz = 1;\n    }")
m(C, "open: a small output buffer accepted", "    if outsz < 1024 {\n        outsz = 1024;\n    }", "    if outsz < 1 {\n        outsz = 1;\n    }")
m(C, "slot_of_ticket: ticket 0 names slot 0", "    if k >= c.nslots || t / span() == 0 || get(c, k, sl.f_gen()) != t / span() {", "    if k >= c.nslots || get(c, k, sl.f_gen()) != t / span() {")
m(C, "slot_of_ticket: any generation names the slot", "    if k >= c.nslots || t / span() == 0 || get(c, k, sl.f_gen()) != t / span() {", "    if k >= c.nslots || t / span() == 0 {")
m(C, "slot_of_ticket: a slot out of range", "    if k >= c.nslots || t / span() == 0 || get(c, k, sl.f_gen()) != t / span() {", "    if t / span() == 0 || get(c, k, sl.f_gen()) != t / span() {")
m(C, "head_cap: the buffer's size is the limit", "    if c.in_size < wire.max_head() {\n        return c.in_size;\n    }\n    return wire.max_head();", "    return c.in_size;")
m(C, "post: an event queued twice", "    if st[p + sl.f_queued()] == 0 {\n        st[p + sl.f_queued()] = 1;", "    if st[p + sl.f_queued()] == 7 {\n        st[p + sl.f_queued()] = 1;")
m(C, "post: the queue's tail is wrong", "        ring[(c.qhead + c.qcount) % c.nslots] = k;", "        ring[c.qhead % c.nslots] = k;")
m(C, "post: the count not kept", "        c.qcount = c.qcount + 1;\n    }\n    return 0;\n}\n\nfn unpost", "    }\n    return 0;\n}\n\nfn unpost")
m(C, "post: an event set twice adds up", "    if st[p + sl.f_events()] / ev % 2 == 0 {\n        st[p + sl.f_events()] = st[p + sl.f_events()] + ev;\n    }\n    if st[p + sl.f_queued()] == 0 {", "    st[p + sl.f_events()] = st[p + sl.f_events()] + ev;\n    if st[p + sl.f_queued()] == 0 {")
m(C, "unpost: an event cleared twice", "    if st[p + sl.f_events()] / ev % 2 == 1 {\n        st[p + sl.f_events()] = st[p + sl.f_events()] - ev;\n    }\n    return 0;\n}\n\n// The slot is ending", "    st[p + sl.f_events()] = st[p + sl.f_events()] - ev;\n    return 0;\n}\n\n// The slot is ending")
m(C, "close_slot: the reason not kept", "    put(c, k, sl.f_reason(), reason);\n    unpost(c, k, sl.ev_connect());", "    unpost(c, k, sl.ev_connect());")
m(C, "close_slot: the caller is not told", "    unpost(c, k, sl.ev_connect());\n    post(c, k, sl.ev_close());\n    return 0;\n}\n\n// The request in slot", "    unpost(c, k, sl.ev_connect());\n    return 0;\n}\n\n// The request in slot")
m(C, "close_slot: a waiting Connect is still delivered", "    put(c, k, sl.f_reason(), reason);\n    unpost(c, k, sl.ev_connect());", "    put(c, k, sl.f_reason(), reason);")
m(C, "fail: the code not kept", "    put(c, k, sl.f_fail(), code);\n    put(c, k, sl.f_live(), 0);", "    put(c, k, sl.f_live(), 0);")
m(C, "fail: the request stays live", "    put(c, k, sl.f_fail(), code);\n    put(c, k, sl.f_live(), 0);", "    put(c, k, sl.f_fail(), code);")
m(C, "fail: a replay still waiting", "    put(c, k, sl.f_live(), 0);\n    put(c, k, sl.f_reconnect(), 0);\n    unpost(c, k, sl.ev_body());", "    put(c, k, sl.f_live(), 0);\n    unpost(c, k, sl.ev_body());")
m(C, "fail: a Body already waiting is still delivered", "    put(c, k, sl.f_reconnect(), 0);\n    unpost(c, k, sl.ev_body());\n    unpost(c, k, sl.ev_done());", "    put(c, k, sl.f_reconnect(), 0);\n    unpost(c, k, sl.ev_done());")
m(C, "fail: a Done already waiting is still delivered", "    unpost(c, k, sl.ev_body());\n    unpost(c, k, sl.ev_done());\n    unpost(c, k, sl.ev_continue());\n    unpost(c, k, sl.ev_connect());\n    post(c, k, sl.ev_failed());", "    unpost(c, k, sl.ev_body());\n    unpost(c, k, sl.ev_continue());\n    unpost(c, k, sl.ev_connect());\n    post(c, k, sl.ev_failed());")
m(C, "fail: the caller is not told", "    post(c, k, sl.ev_failed());\n    close_slot(c, k, r_failed());", "    close_slot(c, k, r_failed());")
m(C, "fail: the transport not closed", "    post(c, k, sl.ev_failed());\n    close_slot(c, k, r_failed());", "    post(c, k, sl.ev_failed());")
m(C, "idle_with_key: other keys counted", "        if get(c, k, sl.f_phase()) == sl.ph_idle() && get(c, k, sl.f_key()) == key {\n            n = n + 1;", "        if get(c, k, sl.f_phase()) == sl.ph_idle() {\n            n = n + 1;")
m(C, "pooled: the oldest connection is taken, not the newest", "            if best < 0 || get(c, k, sl.f_t_phase()) > get(c, best, sl.f_t_phase()) {\n                best = k;", "            if best < 0 || get(c, k, sl.f_t_phase()) < get(c, best, sl.f_t_phase()) {\n                best = k;")
m(C, "pooled: another key's connection taken", "        if get(c, k, sl.f_phase()) == sl.ph_idle() && get(c, k, sl.f_key()) == key && get(c, k, sl.f_events()) == 0 {\n            if best < 0 || get(c, k, sl.f_t_phase()) >", "        if get(c, k, sl.f_phase()) == sl.ph_idle() && get(c, k, sl.f_events()) == 0 {\n            if best < 0 || get(c, k, sl.f_t_phase()) >")
m(C, "pooled: a connection with an event waiting taken", "        if get(c, k, sl.f_phase()) == sl.ph_idle() && get(c, k, sl.f_key()) == key && get(c, k, sl.f_events()) == 0 {\n            if best < 0 || get(c, k, sl.f_t_phase()) >", "        if get(c, k, sl.f_phase()) == sl.ph_idle() && get(c, k, sl.f_key()) == key {\n            if best < 0 || get(c, k, sl.f_t_phase()) >")
m(C, "free_slot: a slot with an event waiting is taken", "        if get(c, k, sl.f_phase()) == sl.ph_free() && get(c, k, sl.f_events()) == 0 {\n            return k;", "        if get(c, k, sl.f_phase()) == sl.ph_free() {\n            return k;")
m(C, "free_slot: a busy slot is taken", "        if get(c, k, sl.f_phase()) == sl.ph_free() && get(c, k, sl.f_events()) == 0 {\n            return k;", "        if get(c, k, sl.f_events()) == 0 {\n            return k;")
m(C, "oldest_idle: the newest is closed, not the oldest", "            if best < 0 || get(c, k, sl.f_t_phase()) < get(c, best, sl.f_t_phase()) {\n                best = k;\n            }\n        }\n        k = k + 1;\n    }\n    return best;\n}\n\n// Starts a request", "            if best < 0 || get(c, k, sl.f_t_phase()) > get(c, best, sl.f_t_phase()) {\n                best = k;\n            }\n        }\n        k = k + 1;\n    }\n    return best;\n}\n\n// Starts a request")
m(C, "oldest_idle: the key's own connection is closed", "        if get(c, k, sl.f_phase()) == sl.ph_idle() && get(c, k, sl.f_key()) != key && get(c, k, sl.f_events()) == 0 {", "        if get(c, k, sl.f_phase()) == sl.ph_idle() && get(c, k, sl.f_events()) == 0 {")
m(C, "request: a negative key accepted", "    if key < 0 {\n        return 0 - wire.c_key();\n    }", "")
m(C, "request: a body past 2^50 accepted", "    if body < 0 - 2 || body > 1125899906842624 || flags < 0 || flags > 7 {", "    if body < 0 - 2 || flags < 0 || flags > 7 {")
m(C, "request: a body of -3 accepted", "    if body < 0 - 2 || body > 1125899906842624 || flags < 0 || flags > 7 {", "    if body > 1125899906842624 || flags < 0 || flags > 7 {")
m(C, "request: flags of 8 accepted", "    if body < 0 - 2 || body > 1125899906842624 || flags < 0 || flags > 7 {", "    if body < 0 - 2 || body > 1125899906842624 || flags < 0 {")
m(C, "request: negative flags accepted", "    if body < 0 - 2 || body > 1125899906842624 || flags < 0 || flags > 7 {", "    if body < 0 - 2 || body > 1125899906842624 || flags > 7 {")
m(C, "request: the pool is not used", "    var k = pooled(c, key);\n    var how = 0;\n    if k < 0 {", "    var k = 0 - 1;\n    var how = 0;\n    if k < 0 {")
m(C, "request: a full client evicts nothing", "        k = oldest_idle(c, key);\n        how = 2;", "        how = 2;")
m(C, "request: a full client is not refused", "    if k < 0 {\n        return 0 - wire.c_full();\n    }", "    if k < 0 {\n        k = 0;\n    }")
m(C, "request: Expect holds a request with no body", "    let wants_hold = flags / sl.rf_expect() % 2 == 1 && (body > 0 || body == wire.body_chunked());", "    let wants_hold = flags / sl.rf_expect() % 2 == 1;")
m(C, "request: Expect never holds", "    let wants_hold = flags / sl.rf_expect() % 2 == 1 && (body > 0 || body == wire.body_chunked());", "    let wants_hold = false;")
m(C, "request: a head that does not fit is sent", "    if head < 0 {\n        return 0 - wire.c_too_large();\n    }", "")
m(C, "request: the connection's use count is reset by a reuse", "    var uses = 0;\n    if how == 0 {\n        uses = st[p + sl.f_uses()];\n    }", "    var uses = 0;")
m(C, "request: the connection's age is reset by a reuse", "    var opened = 0;\n    if how == 0 {\n        opened = st[p + sl.f_t_open()];\n    }", "    var opened = 0;")
m(C, "request: the generation not advanced", "    var gen = (st[p + sl.f_gen()] + 1) % gen_mod();", "    var gen = st[p + sl.f_gen()];")
m(C, "request: the old state of the slot kept", "    var i = 0;\n    while i < sl.stride() {\n        st[p + i] = 0;\n        i = i + 1;\n    }\n    st[p + sl.f_gen()] = gen;", "    st[p + sl.f_gen()] = gen;")
m(C, "request: a queued slot is forgotten", "    st[p + sl.f_queued()] = queued;\n    st[p + sl.f_key()] = key;", "    st[p + sl.f_key()] = key;")
m(C, "request: the key not kept", "    st[p + sl.f_key()] = key;\n    st[p + sl.f_uses()] = uses;", "    st[p + sl.f_uses()] = uses;")
m(C, "request: the request's start not kept", "    st[p + sl.f_t_req()] = now;\n    st[p + sl.f_t_prog()] = now;", "    st[p + sl.f_t_prog()] = now;")
m(C, "request: the progress clock not started", "    st[p + sl.f_t_req()] = now;\n    st[p + sl.f_t_prog()] = now;", "    st[p + sl.f_t_req()] = now;")
m(C, "request: the hold clock starts at once", "    st[p + sl.f_t_hold()] = 0 - 1;\n    st[p + sl.f_out_len()] = head;", "    st[p + sl.f_t_hold()] = now;\n    st[p + sl.f_out_len()] = head;")
m(C, "request: the head's length not kept", "    st[p + sl.f_out_len()] = head;\n    st[p + sl.f_head_wire()] = head;", "    st[p + sl.f_head_wire()] = head;")
m(C, "request: the head's wire length not kept", "    st[p + sl.f_out_len()] = head;\n    st[p + sl.f_head_wire()] = head;", "    st[p + sl.f_out_len()] = head;")
m(C, "request: the request is not live", "    st[p + sl.f_head_wire()] = head;\n    st[p + sl.f_live()] = 1;", "    st[p + sl.f_head_wire()] = head;")
m(C, "request: HEAD not noticed", "    if wire.is_head(method) {\n        rf = rf + sl.rf_head_method();\n    }", "")
m(C, "request: a retry is always replayable", "    if flags % 2 == 1 {\n        rf = rf + sl.rf_replayable();\n    }", "    rf = rf + sl.rf_replayable();")
m(C, "request: a retry is never replayable", "    if flags % 2 == 1 {\n        rf = rf + sl.rf_replayable();\n    }", "")
m(C, "request: a body of none still wants a body", "    if body == wire.body_none() {\n        rf = rf + sl.rf_body_done();\n    } else if body == 0 {", "    if body == 7 {\n        rf = rf + sl.rf_body_done();\n    } else if body == 0 {")
m(C, "request: a body of 0 waits for more", "    } else if body == 0 {\n        rf = rf + sl.rf_body_done();\n        st[p + sl.f_bmode()] = 1;", "    } else if body == 0 {\n        st[p + sl.f_bmode()] = 1;")
m(C, "request: the length not kept", "        st[p + sl.f_bmode()] = 1;\n        st[p + sl.f_bleft()] = body;", "        st[p + sl.f_bmode()] = 1;")
m(C, "request: the body mode of a length not kept", "        st[p + sl.f_bmode()] = 1;\n        st[p + sl.f_bleft()] = body;", "        st[p + sl.f_bleft()] = body;")
m(C, "request: a chunked body is a length body", "        st[p + sl.f_bmode()] = 2;\n        st[p + sl.f_bleft()] = 1;", "        st[p + sl.f_bmode()] = 1;\n        st[p + sl.f_bleft()] = 1;")
m(C, "request: the response state not started", "    st[p + sl.f_rs()] = sl.rs_head();\n    c.n_requests", "    c.n_requests")
m(C, "request: requests not counted", "    c.n_requests = c.n_requests + 1;", "")
m(C, "request: a reuse is not marked", "        st[p + sl.f_phase()] = sl.ph_active();\n        st[p + sl.f_reused()] = 1;", "        st[p + sl.f_phase()] = sl.ph_active();")
m(C, "request: a reuse is not counted", "        st[p + sl.f_uses()] = uses + 1;\n        c.n_reuses = c.n_reuses + 1;", "        st[p + sl.f_uses()] = uses + 1;")
m(C, "request: the uses of a reused connection not counted", "        st[p + sl.f_uses()] = uses + 1;\n        c.n_reuses = c.n_reuses + 1;", "        c.n_reuses = c.n_reuses + 1;")
m(C, "request: a new connection's dial clock not started", "        st[p + sl.f_phase()] = sl.ph_connect();\n        st[p + sl.f_t_phase()] = now;", "        st[p + sl.f_phase()] = sl.ph_connect();")
m(C, "request: a dial is not announced", "        c.n_connects = c.n_connects + 1;\n        post(c, k, sl.ev_connect());\n    } else {", "        c.n_connects = c.n_connects + 1;\n    } else {")
m(C, "request: dials not counted", "        c.n_connects = c.n_connects + 1;\n        post(c, k, sl.ev_connect());\n    } else {", "        post(c, k, sl.ev_connect());\n    } else {")
m(C, "request: an evicted slot does not reconnect", "        st[p + sl.f_reconnect()] = 1;\n        st[p + sl.f_reason()] = r_evicted();", "        st[p + sl.f_reason()] = r_evicted();")
m(C, "request: the eviction's reason not kept", "        st[p + sl.f_reason()] = r_evicted();\n        st[p + sl.f_phase()] = sl.ph_closing();", "        st[p + sl.f_phase()] = sl.ph_closing();")
m(C, "request: the old connection is not closed", "        st[p + sl.f_phase()] = sl.ph_closing();\n        post(c, k, sl.ev_close());\n    }\n    return ticket_for(c, k);", "        st[p + sl.f_phase()] = sl.ph_closing();\n    }\n    return ticket_for(c, k);")
m(C, "send_room: a held body has room", " || flag(c, k, sl.rf_body_done()) || flag(c, k, sl.rf_hold()) || flag(c, k, sl.rf_early()) {\n        return 0;", " || flag(c, k, sl.rf_body_done()) || flag(c, k, sl.rf_early()) {\n        return 0;")
m(C, "send_room: an early response leaves room", " || flag(c, k, sl.rf_body_done()) || flag(c, k, sl.rf_hold()) || flag(c, k, sl.rf_early()) {\n        return 0;", " || flag(c, k, sl.rf_body_done()) || flag(c, k, sl.rf_hold()) {\n        return 0;")
m(C, "send_room: a finished body has room", " || flag(c, k, sl.rf_body_done()) || flag(c, k, sl.rf_hold()) || flag(c, k, sl.rf_early()) {\n        return 0;", " || flag(c, k, sl.rf_hold()) || flag(c, k, sl.rf_early()) {\n        return 0;")
m(C, "send_room: a request with no body has room", "get(c, k, sl.f_live()) != 1 || get(c, k, sl.f_bmode()) == 0 || flag(c, k, sl.rf_body_done())", "get(c, k, sl.f_live()) != 1 || flag(c, k, sl.rf_body_done())")
m(C, "send_room: bytes already taken are not room", "    if !flag(c, k, sl.rf_replayable()) {\n        free = free + get(c, k, sl.f_out_sent());\n    } else if", "    if flag(c, k, sl.rf_hold()) {\n        free = free + get(c, k, sl.f_out_sent());\n    } else if")
m(C, "send_room: a replayable request reuses sent bytes", "    if !flag(c, k, sl.rf_replayable()) {\n        free = free + get(c, k, sl.f_out_sent());\n    } else if", "    if true {\n        free = free + get(c, k, sl.f_out_sent());\n    } else if")
m(C, "send_room: a full replayable buffer has no room again", "    } else if free == 0 && get(c, k, sl.f_out_sent()) == get(c, k, sl.f_out_len()) {\n        free = c.out_size;\n    }", "    }")
m(C, "send_room: chunk framing not allowed for", "    if get(c, k, sl.f_bmode()) == 2 {\n        return wire.chunk_fit(free);\n    }", "    if get(c, k, sl.f_bmode()) == 2 {\n        return free;\n    }")
m(C, "send_room: more than a length body has left", "    if free > get(c, k, sl.f_bleft()) {\n        return get(c, k, sl.f_bleft());\n    }", "")
m(C, "reclaim: a full replayable request stays replayable", "        set_flag(c, k, sl.rf_replayable(), false);\n    }\n    if !flag(c, k, sl.rf_replayable()) && st[p + sl.f_out_sent()] > 0 {", "    }\n    if !flag(c, k, sl.rf_replayable()) && st[p + sl.f_out_sent()] > 0 {")
m(C, "reclaim: bytes taken are kept", "    if !flag(c, k, sl.rf_replayable()) && st[p + sl.f_out_sent()] > 0 {\n        let rest", "    if false && st[p + sl.f_out_sent()] > 0 {\n        let rest")
m(C, "reclaim: bytes taken are dropped from a replayable request", "    if !flag(c, k, sl.rf_replayable()) && st[p + sl.f_out_sent()] > 0 {\n        let rest", "    if st[p + sl.f_out_sent()] > 0 {\n        let rest")
m(C, "reclaim: the unsent bytes not moved to the front", "        if rest > 0 {\n            copy_within(outbuf(c, k), 0, st[p + sl.f_out_sent()], rest);\n        }", "")
m(C, "reclaim: the length not reduced", "        st[p + sl.f_out_len()] = rest;\n        st[p + sl.f_head_wire()] = 0;", "        st[p + sl.f_head_wire()] = 0;")
m(C, "reclaim: the head's wire length kept", "        st[p + sl.f_head_wire()] = 0;\n        st[p + sl.f_out_sent()] = 0;", "        st[p + sl.f_out_sent()] = 0;")
m(C, "reclaim: the sent count kept", "        st[p + sl.f_head_wire()] = 0;\n        st[p + sl.f_out_sent()] = 0;", "        st[p + sl.f_head_wire()] = 0;")
m(C, "send_body: a stale ticket takes a body", "    let k = slot_of_ticket(c, t);\n    if k < 0 || get(c, k, sl.f_live()) != 1 {\n        return 0 - wire.c_ticket();\n    }\n    if flag(c, k, sl.rf_early()) {\n        return 0 - wire.c_response_arrived();\n    }\n    if get(c, k, sl.f_bmode()) == 0 {\n        return 0 - wire.c_no_body();\n    }\n    if flag(c, k, sl.rf_body_done()) {\n        if len(bytes) > 0 {", "    let k = slot_of_ticket(c, t);\n    if k < 0 {\n        return 0 - wire.c_ticket();\n    }\n    if flag(c, k, sl.rf_early()) {\n        return 0 - wire.c_response_arrived();\n    }\n    if get(c, k, sl.f_bmode()) == 0 {\n        return 0 - wire.c_no_body();\n    }\n    if flag(c, k, sl.rf_body_done()) {\n        if len(bytes) > 0 {")
m(C, "send_body: a body after an early response taken", "    if flag(c, k, sl.rf_early()) {\n        return 0 - wire.c_response_arrived();\n    }\n    if get(c, k, sl.f_bmode()) == 0 {\n        return 0 - wire.c_no_body();\n    }\n    if flag(c, k, sl.rf_body_done()) {\n        if len(bytes) > 0 {", "    if get(c, k, sl.f_bmode()) == 0 {\n        return 0 - wire.c_no_body();\n    }\n    if flag(c, k, sl.rf_body_done()) {\n        if len(bytes) > 0 {")
m(C, "send_body: a body for a request with none taken", "    if get(c, k, sl.f_bmode()) == 0 {\n        return 0 - wire.c_no_body();\n    }\n    if flag(c, k, sl.rf_body_done()) {\n        if len(bytes) > 0 {", "    if flag(c, k, sl.rf_body_done()) {\n        if len(bytes) > 0 {")
m(C, "send_body: bytes after the end of the body taken", "        if len(bytes) > 0 {\n            return 0 - wire.c_body_too_long();\n        }\n        return 0;\n    }\n    if get(c, k, sl.f_bmode()) == 1 && len(bytes)", "        return 0;\n    }\n    if get(c, k, sl.f_bmode()) == 1 && len(bytes)")
m(C, "send_body: more than the length taken", "    if get(c, k, sl.f_bmode()) == 1 && len(bytes) > get(c, k, sl.f_bleft()) {\n        return 0 - wire.c_body_too_long();\n    }", "")
m(C, "send_body: a held body taken", "    if len(bytes) == 0 || flag(c, k, sl.rf_hold()) {\n        return 0;\n    }", "    if len(bytes) == 0 {\n        return 0;\n    }")
m(C, "send_body: more than the room taken", "    var n = len(bytes);\n    if n > room {\n        n = room;\n    }\n    reclaim(c, k);", "    var n = len(bytes);\n    reclaim(c, k);")
m(C, "send_body: no room reclaimed", "    reclaim(c, k);\n    let st = contents(c.state);\n    let p = sl.stride() * k;\n    let waiting = st[p + sl.f_out_len()] - st[p + sl.f_out_sent()];\n    if get(c, k, sl.f_bmode()) == 1 {", "    let st = contents(c.state);\n    let p = sl.stride() * k;\n    let waiting = st[p + sl.f_out_len()] - st[p + sl.f_out_sent()];\n    if get(c, k, sl.f_bmode()) == 1 {")
m(C, "send_body: the length not reduced", "        st[p + sl.f_bleft()] = st[p + sl.f_bleft()] - n;\n        if st[p + sl.f_bleft()] == 0 {", "        if st[p + sl.f_bleft()] == 0 {")
m(C, "send_body: the end of a length body not noticed", "        if st[p + sl.f_bleft()] == 0 {\n            set_flag(c, k, sl.rf_body_done(), true);\n        }", "")
m(C, "send_body: bytes written at the front", "        copy_into(outbuf(c, k)[st[p + sl.f_out_len()]..st[p + sl.f_out_len()] + n], bytes[0..n]);\n        st[p + sl.f_out_len()] = st[p + sl.f_out_len()] + n;", "        copy_into(outbuf(c, k)[0..n], bytes[0..n]);\n        st[p + sl.f_out_len()] = st[p + sl.f_out_len()] + n;")
m(C, "send_body: the length of the buffer not advanced", "        copy_into(outbuf(c, k)[st[p + sl.f_out_len()]..st[p + sl.f_out_len()] + n], bytes[0..n]);\n        st[p + sl.f_out_len()] = st[p + sl.f_out_len()] + n;", "        copy_into(outbuf(c, k)[st[p + sl.f_out_len()]..st[p + sl.f_out_len()] + n], bytes[0..n]);")
m(C, "send_body: a chunk that did not fit is counted", "        if end < 0 {\n            return 0;\n        }\n        st[p + sl.f_out_len()] = end;\n    }\n    if waiting == 0 {\n        st[p + sl.f_t_prog()] = now;\n    }\n    return n;", "        if end < 0 {\n            return n;\n        }\n        st[p + sl.f_out_len()] = end;\n    }\n    if waiting == 0 {\n        st[p + sl.f_t_prog()] = now;\n    }\n    return n;")
m(C, "send_body: handing over a body does not restart the clock", "    if waiting == 0 {\n        st[p + sl.f_t_prog()] = now;\n    }\n    return n;", "    return n;")
m(C, "send_body: the clock restarts whatever was waiting", "    if waiting == 0 {\n        st[p + sl.f_t_prog()] = now;\n    }\n    return n;", "    st[p + sl.f_t_prog()] = now;\n    return n;")
m(C, "end_body: a stale ticket ends a body", "    let k = slot_of_ticket(c, t);\n    if k < 0 || get(c, k, sl.f_live()) != 1 {\n        return 0 - wire.c_ticket();\n    }\n    if flag(c, k, sl.rf_early()) {\n        return 0 - wire.c_response_arrived();\n    }\n    if get(c, k, sl.f_bmode()) == 0 {\n        return 0 - wire.c_no_body();\n    }\n    if flag(c, k, sl.rf_body_done()) {\n        return 1;", "    let k = slot_of_ticket(c, t);\n    if k < 0 {\n        return 0 - wire.c_ticket();\n    }\n    if flag(c, k, sl.rf_early()) {\n        return 0 - wire.c_response_arrived();\n    }\n    if get(c, k, sl.f_bmode()) == 0 {\n        return 0 - wire.c_no_body();\n    }\n    if flag(c, k, sl.rf_body_done()) {\n        return 1;")
m(C, "end_body: an early response leaves a body to end", "    if flag(c, k, sl.rf_early()) {\n        return 0 - wire.c_response_arrived();\n    }\n    if get(c, k, sl.f_bmode()) == 0 {\n        return 0 - wire.c_no_body();\n    }\n    if flag(c, k, sl.rf_body_done()) {\n        return 1;", "    if get(c, k, sl.f_bmode()) == 0 {\n        return 0 - wire.c_no_body();\n    }\n    if flag(c, k, sl.rf_body_done()) {\n        return 1;")
m(C, "end_body: a request with no body is ended", "    if get(c, k, sl.f_bmode()) == 0 {\n        return 0 - wire.c_no_body();\n    }\n    if flag(c, k, sl.rf_body_done()) {\n        return 1;", "    if flag(c, k, sl.rf_body_done()) {\n        return 1;")
m(C, "end_body: a length body can be cut short", "    if get(c, k, sl.f_bmode()) == 1 {\n        return 0 - wire.c_length();\n    }", "")
m(C, "end_body: a held request is ended", "    if flag(c, k, sl.rf_hold()) {\n        return 0;\n    }\n    reclaim(c, k);\n    let st = contents(c.state);\n    let p = sl.stride() * k;\n    let waiting", "    reclaim(c, k);\n    let st = contents(c.state);\n    let p = sl.stride() * k;\n    let waiting")
m(C, "end_body: a last chunk that did not fit is counted", "    let end = wire.write_last_chunk(outbuf(c, k), st[p + sl.f_out_len()]);\n    if end < 0 {\n        return 0;\n    }", "    let end = wire.write_last_chunk(outbuf(c, k), st[p + sl.f_out_len()]);\n    if end < 0 {\n        return 1;\n    }")
m(C, "end_body: the end not recorded", "    st[p + sl.f_out_len()] = end;\n    set_flag(c, k, sl.rf_body_done(), true);\n    if waiting == 0 {", "    st[p + sl.f_out_len()] = end;\n    if waiting == 0 {")
m(C, "end_body: the length not advanced", "    st[p + sl.f_out_len()] = end;\n    set_flag(c, k, sl.rf_body_done(), true);\n    if waiting == 0 {", "    set_flag(c, k, sl.rf_body_done(), true);\n    if waiting == 0 {")
m(C, "key: a free slot has a key", "    if !slot_ok(c, k) || get(c, k, sl.f_phase()) == sl.ph_free() {\n        return 0 - 1;\n    }\n    return get(c, k, sl.f_key());", "    if !slot_ok(c, k) {\n        return 0 - 1;\n    }\n    return get(c, k, sl.f_key());")
m(C, "ticket_of: a slot never used has a ticket", "    if !slot_ok(c, k) || get(c, k, sl.f_gen()) == 0 {\n        return 0 - 1;\n    }", "    if !slot_ok(c, k) {\n        return 0 - 1;\n    }")
m(C, "close_reason: the slot's own reason is not given", "    return get(c, k, sl.f_reason());\n}\n\n// The caller's dial", "    return 0;\n}\n\n// The caller's dial")
m(C, "connected: a slot that is not dialling is connected", "    if !slot_ok(c, k) || get(c, k, sl.f_phase()) != sl.ph_connect() {\n        return 0 - 1;\n    }\n    put(c, k, sl.f_phase(), sl.ph_active());", "    if !slot_ok(c, k) {\n        return 0 - 1;\n    }\n    put(c, k, sl.f_phase(), sl.ph_active());")
m(C, "connected: the connection's age not started", "    put(c, k, sl.f_phase(), sl.ph_active());\n    put(c, k, sl.f_t_open(), now);", "    put(c, k, sl.f_phase(), sl.ph_active());")
m(C, "connected: the progress clock not restarted", "    put(c, k, sl.f_t_open(), now);\n    put(c, k, sl.f_t_phase(), now);\n    put(c, k, sl.f_t_prog(), now);", "    put(c, k, sl.f_t_open(), now);\n    put(c, k, sl.f_t_phase(), now);")
m(C, "connected: the connection is marked reused", "    put(c, k, sl.f_uses(), 1);\n    put(c, k, sl.f_reused(), 0);", "    put(c, k, sl.f_uses(), 1);\n    put(c, k, sl.f_reused(), 1);")
m(C, "connected: the connection carries no request yet", "    put(c, k, sl.f_uses(), 1);\n    put(c, k, sl.f_reused(), 0);", "    put(c, k, sl.f_reused(), 0);")
m(C, "connect_failed: a slot that is not dialling fails", "    if !slot_ok(c, k) || get(c, k, sl.f_phase()) != sl.ph_connect() {\n        return 0 - 1;\n    }\n    var code = wire.c_connect();", "    if !slot_ok(c, k) {\n        return 0 - 1;\n    }\n    var code = wire.c_connect();")
m(C, "connect_failed: a TLS failure is a connect failure", "    if kind == kind_tls() {\n        code = wire.c_tls();\n    }", "")
m(C, "connect_failed: a name failure is a connect failure", "    if kind == kind_resolve() {\n        code = wire.c_resolve();\n    }", "")
m(C, "pending: a request not live has bytes", "    if !slot_ok(c, k) || get(c, k, sl.f_phase()) != sl.ph_active() || get(c, k, sl.f_live()) != 1 {\n        return 0;\n    }\n    var limit", "    if !slot_ok(c, k) || get(c, k, sl.f_phase()) != sl.ph_active() {\n        return 0;\n    }\n    var limit")
m(C, "pending: the body behind a held head is offered", "    if flag(c, k, sl.rf_hold()) {\n        limit = get(c, k, sl.f_head_wire());\n    }\n    return limit", "    return limit")
m(C, "take: more than `out` holds", "    if len(out) < n {\n        n = len(out);\n    }\n    if n == 0 {\n        return 0;\n    }\n    let st = contents(c.state);\n    let p = sl.stride() * k;\n    copy_into(out[0..n]", "    let st = contents(c.state);\n    let p = sl.stride() * k;\n    copy_into(out[0..n]")
m(C, "take: the sent count not advanced", "    st[p + sl.f_out_sent()] = st[p + sl.f_out_sent()] + n;\n    st[p + sl.f_t_prog()] = now;\n    if flag(c, k, sl.rf_hold())", "    st[p + sl.f_t_prog()] = now;\n    if flag(c, k, sl.rf_hold())")
m(C, "take: taking is not progress", "    st[p + sl.f_out_sent()] = st[p + sl.f_out_sent()] + n;\n    st[p + sl.f_t_prog()] = now;\n    if flag(c, k, sl.rf_hold())", "    st[p + sl.f_out_sent()] = st[p + sl.f_out_sent()] + n;\n    if flag(c, k, sl.rf_hold())")
m(C, "take: the hold clock not started", "        st[p + sl.f_t_hold()] = now;\n    }\n    if st[p + sl.f_out_sent()] == st[p + sl.f_out_len()] {", "    }\n    if st[p + sl.f_out_sent()] == st[p + sl.f_out_len()] {")
m(C, "take: the hold clock starts before the head has gone", "    if flag(c, k, sl.rf_hold()) && st[p + sl.f_out_sent()] == st[p + sl.f_head_wire()] {", "    if flag(c, k, sl.rf_hold()) && st[p + sl.f_out_sent()] > 0 {")
m(C, "take: a request is done before its body is", "        if flag(c, k, sl.rf_body_done()) && !flag(c, k, sl.rf_hold()) {", "        if !flag(c, k, sl.rf_hold()) {")
m(C, "take: a held request is done", "        if flag(c, k, sl.rf_body_done()) && !flag(c, k, sl.rf_hold()) {", "        if flag(c, k, sl.rf_body_done()) {")
m(C, "take: the end of the request is not noticed", "            set_flag(c, k, sl.rf_req_done(), true);\n        }\n        reclaim(c, k);", "        }\n        reclaim(c, k);")
m(C, "take: the buffer is not reclaimed", "            set_flag(c, k, sl.rf_req_done(), true);\n        }\n        reclaim(c, k);", "            set_flag(c, k, sl.rf_req_done(), true);\n        }")
m(C, "room: a closing connection has room", "    if phase != sl.ph_active() || get(c, k, sl.f_live()) != 1 {\n        return 0;\n    }\n    if get(c, k, sl.f_rs()) == sl.rs_done()", "    if phase != sl.ph_active() {\n        return 0;\n    }\n    if get(c, k, sl.f_rs()) == sl.rs_done()")
m(C, "room: a complete response has room", "    if get(c, k, sl.f_rs()) == sl.rs_done() || get(c, k, sl.f_complete()) == 1 {\n        return 0;\n    }\n    return c.in_size", "    return c.in_size")
m(C, "room: a response waiting to be reported has room", "    if get(c, k, sl.f_rs()) == sl.rs_done() || get(c, k, sl.f_complete()) == 1 {\n        return 0;\n    }\n    return c.in_size", "    if get(c, k, sl.f_complete()) == 1 {\n        return 0;\n    }\n    return c.in_size")
m(C, "room: the buffer's fill ignored", "    return c.in_size - get(c, k, sl.f_fill());\n}", "    return c.in_size;\n}")
m(C, "room: an idle connection has no room to notice an end", "    if phase == sl.ph_idle() {\n        return c.in_size;\n    }", "    if phase == sl.ph_idle() {\n        return 0;\n    }")
m(C, "give: bytes on an idle connection do not close it", "        if len(data) > 0 {\n            close_slot(c, k, r_unsolicited());\n        }\n        return len(data);", "        return len(data);")
m(C, "give: no bytes on an idle connection close it", "        if len(data) > 0 {\n            close_slot(c, k, r_unsolicited());\n        }\n        return len(data);", "        close_slot(c, k, r_unsolicited());\n        return len(data);")
m(C, "give: bytes for a closing connection are kept", "    if phase == sl.ph_closing() {\n        return len(data);\n    }", "    if phase == sl.ph_closing() {\n        return 0;\n    }")
m(C, "give: a slot with no transport takes bytes", "    if phase != sl.ph_active() {\n        return 0 - 1;\n    }", "    if phase == sl.ph_free() {\n        return 0 - 1;\n    }")
m(C, "give: bytes for a request that is not live are kept", "    if get(c, k, sl.f_live()) != 1 {\n        return len(data);\n    }", "    if get(c, k, sl.f_live()) != 1 {\n        return 0;\n    }")
m(C, "give: bytes behind a complete response not noted", "        if len(data) > 0 {\n            put(c, k, sl.f_dirty(), 1);\n        }\n        return len(data);", "        return len(data);")
m(C, "give: more than the room taken", "    var n = room(c, k);\n    if n > len(data) {\n        n = len(data);\n    }\n    if n <= 0 {\n        return 0;\n    }", "    var n = len(data);\n    if n <= 0 {\n        return 0;\n    }")
m(C, "give: the fill not advanced", "    st[p + sl.f_fill()] = st[p + sl.f_fill()] + n;\n    st[p + sl.f_t_prog()] = now;\n    if st[p + sl.f_anyresp()] == 0 {", "    st[p + sl.f_t_prog()] = now;\n    if st[p + sl.f_anyresp()] == 0 {")
m(C, "give: bytes are not progress", "    st[p + sl.f_fill()] = st[p + sl.f_fill()] + n;\n    st[p + sl.f_t_prog()] = now;\n    if st[p + sl.f_anyresp()] == 0 {", "    st[p + sl.f_fill()] = st[p + sl.f_fill()] + n;\n    if st[p + sl.f_anyresp()] == 0 {")
m(C, "give: a first byte of response is not noted", "    if st[p + sl.f_anyresp()] == 0 {\n        st[p + sl.f_anyresp()] = 1;\n        set_flag(c, k, sl.rf_replayable(), false);\n        reclaim(c, k);\n    }", "")
m(C, "give: a request is still replayable after a response byte", "        st[p + sl.f_anyresp()] = 1;\n        set_flag(c, k, sl.rf_replayable(), false);\n        reclaim(c, k);", "        st[p + sl.f_anyresp()] = 1;")
m(C, "give: the response is not read", "    advance(c, k, now);\n    return n;", "    return n;")
m(C, "finish_body: Done before the body is consumed", "    if get(c, k, sl.f_avail()) == 0 && get(c, k, sl.f_rs()) != sl.rs_done() {", "    if get(c, k, sl.f_rs()) != sl.rs_done() {")
m(C, "finish_body: Done twice", "    if get(c, k, sl.f_avail()) == 0 && get(c, k, sl.f_rs()) != sl.rs_done() {", "    if get(c, k, sl.f_avail()) == 0 {")
m(C, "finish_body: the state not advanced", "        put(c, k, sl.f_rs(), sl.rs_done());\n        post(c, k, sl.ev_done());", "        post(c, k, sl.ev_done());")
m(C, "finish_body: the caller is not told", "        put(c, k, sl.f_rs(), sl.rs_done());\n        post(c, k, sl.ev_done());", "        put(c, k, sl.f_rs(), sl.rs_done());")
m(C, "read_head: the blank line is looked for from the start of the buffer each time", "    var from = st[p + sl.f_scan()] - 3;", "    var from = st[p + sl.f_scan()];")
m(C, "read_head: a bare LF is not refused", "    if blank <= 0 - 2 {\n        fail(c, k, wire.c_resp_header());\n        return 0 - 1;\n    }", "")
m(C, "read_head: a response that is not HTTP waits for a blank line", "    if !wire.looks_like_http(src) {\n        fail(c, k, wire.c_status_line());\n        return 0 - 1;\n    }", "")
m(C, "read_head: a head over the limit waits", "        if fill >= cap {\n            fail(c, k, wire.c_head_too_large());\n            return 0 - 1;\n        }", "")
m(C, "read_head: the scan position not kept", "        st[p + sl.f_scan()] = fill;\n        return 0;", "        return 0;")
m(C, "read_head: a refused head is not failed", "    if n < 0 {\n        fail(c, k, wire.error_code(n));\n        return 0 - 1;\n    }", "    if n < 0 {\n        return 0;\n    }")
m(C, "read_head: a refusal is failed with the wrong code", "        fail(c, k, wire.error_code(n));", "        fail(c, k, wire.c_resp_header());")
m(C, "read_head: a 101 is a response", "        if status == 101 {\n            fail(c, k, wire.c_upgrade());\n            return 0 - 1;\n        }", "")
m(C, "read_head: interim responses not counted", "        st[p + sl.f_info()] = st[p + sl.f_info()] + 1;\n        if st[p + sl.f_info()] > max_informational() {", "        if st[p + sl.f_info()] > max_informational() {")
m(C, "read_head: nine interim responses accepted", "        if st[p + sl.f_info()] > max_informational() {", "        if st[p + sl.f_info()] > max_informational() + 1 {")
m(C, "read_head: eight interim responses refused", "        if st[p + sl.f_info()] > max_informational() {", "        if st[p + sl.f_info()] >= max_informational() {")
m(C, "read_head: a 100 does not release the body", "        if status == 100 && flag(c, k, sl.rf_hold()) {\n            set_flag(c, k, sl.rf_hold(), false);\n            post(c, k, sl.ev_continue());\n        }", "")
m(C, "read_head: any interim response releases the body", "        if status == 100 && flag(c, k, sl.rf_hold()) {", "        if flag(c, k, sl.rf_hold()) {")
m(C, "read_head: a 100 releases without telling the caller", "            set_flag(c, k, sl.rf_hold(), false);\n            post(c, k, sl.ev_continue());", "            set_flag(c, k, sl.rf_hold(), false);")
m(C, "read_head: a 100 tells the caller and does not release", "            set_flag(c, k, sl.rf_hold(), false);\n            post(c, k, sl.ev_continue());", "            post(c, k, sl.ev_continue());")
m(C, "read_head: an interim head is not dropped", "        copy_within(inbuf(c, k), 0, n, fill - n);\n        st[p + sl.f_fill()] = fill - n;\n        st[p + sl.f_scan()] = 0;", "        st[p + sl.f_scan()] = 0;")
m(C, "read_head: the fill after an interim head is wrong", "        st[p + sl.f_fill()] = fill - n;\n        st[p + sl.f_scan()] = 0;", "        st[p + sl.f_fill()] = fill;\n        st[p + sl.f_scan()] = 0;")
m(C, "read_head: the scan after an interim head is not restarted", "        st[p + sl.f_fill()] = fill - n;\n        st[p + sl.f_scan()] = 0;", "        st[p + sl.f_fill()] = fill - n;")
m(C, "read_head: the head's length not kept", "    st[p + sl.f_hl()] = n;\n    st[p + sl.f_rs()] = sl.rs_body();", "    st[p + sl.f_rs()] = sl.rs_body();")
m(C, "read_head: the body state not entered", "    st[p + sl.f_hl()] = n;\n    st[p + sl.f_rs()] = sl.rs_body();", "    st[p + sl.f_hl()] = n;")
m(C, "read_head: a HEAD response has a body", "    if flag(c, k, sl.rf_head_method()) || status == 204 || status == 304 {", "    if status == 204 || status == 304 {")
m(C, "read_head: a 204 has a body", "    if flag(c, k, sl.rf_head_method()) || status == 204 || status == 304 {", "    if flag(c, k, sl.rf_head_method()) || status == 304 {")
m(C, "read_head: a 304 has a body", "    if flag(c, k, sl.rf_head_method()) || status == 204 || status == 304 {", "    if flag(c, k, sl.rf_head_method()) || status == 204 {")
m(C, "read_head: chunked is not noticed", "    } else if table[5] % 2 == 1 {\n        st[p + sl.f_bkind()] = sl.bk_chunked();", "    } else if table[5] % 2 == 7 {\n        st[p + sl.f_bkind()] = sl.bk_chunked();")
m(C, "read_head: the chunk decoder not started afresh", "        st[p + sl.f_bkind()] = sl.bk_chunked();\n        wire.chunk_reset(contents(c.state), p + sl.f_chunk());", "        st[p + sl.f_bkind()] = sl.bk_chunked();")
m(C, "read_head: a length is not noticed", "    } else if table[4] > 0 {\n        st[p + sl.f_bkind()] = sl.bk_length();\n        st[p + sl.f_brem()] = table[4];", "    } else if table[4] > 100000000 {\n        st[p + sl.f_bkind()] = sl.bk_length();\n        st[p + sl.f_brem()] = table[4];")
m(C, "read_head: the length not kept", "        st[p + sl.f_bkind()] = sl.bk_length();\n        st[p + sl.f_brem()] = table[4];", "        st[p + sl.f_bkind()] = sl.bk_length();")
m(C, "read_head: a length of 0 runs to the end of the connection", "    } else if table[4] == 0 {\n        st[p + sl.f_bkind()] = sl.bk_none();\n    } else {", "    } else if table[4] == 7 {\n        st[p + sl.f_bkind()] = sl.bk_none();\n    } else {")
m(C, "read_head: no framing is no body", "    } else {\n        st[p + sl.f_bkind()] = sl.bk_close();\n    }\n    if !flag(c, k, sl.rf_req_done()) {", "    } else {\n        st[p + sl.f_bkind()] = sl.bk_none();\n    }\n    if !flag(c, k, sl.rf_req_done()) {")
m(C, "read_head: an early response is not noticed", "        set_flag(c, k, sl.rf_early(), true);\n        set_flag(c, k, sl.rf_hold(), false);", "        set_flag(c, k, sl.rf_hold(), false);")
m(C, "read_head: an early response leaves the request held", "        set_flag(c, k, sl.rf_early(), true);\n        set_flag(c, k, sl.rf_hold(), false);", "        set_flag(c, k, sl.rf_early(), true);")
m(C, "read_head: an early response leaves the rest of the request to send", "        set_flag(c, k, sl.rf_hold(), false);\n        st[p + sl.f_out_len()] = st[p + sl.f_out_sent()];", "        set_flag(c, k, sl.rf_hold(), false);")
m(C, "read_head: an early response with a request that is whole", "    if !flag(c, k, sl.rf_req_done()) {\n        // The upstream answered before", "    if true {\n        // The upstream answered before")
m(C, "read_head: the head is not announced", "    post(c, k, sl.ev_head());\n    return 1;", "    return 1;")
m(C, "read_body: a complete body is read again", "    if st[p + sl.f_complete()] == 1 {\n        return 0;\n    }\n    if kind == sl.bk_none() {", "    if kind == sl.bk_none() {")
m(C, "read_body: a response with no body is not complete", "    if kind == sl.bk_none() {\n        st[p + sl.f_complete()] = 1;\n        if fill - hl > 0 {", "    if kind == sl.bk_none() {\n        if fill - hl > 0 {")
m(C, "read_body: bytes behind a response with no body not noted", "        if fill - hl > 0 {\n            st[p + sl.f_dirty()] = 1;\n        }\n    } else if kind == sl.bk_length() {", "    } else if kind == sl.bk_length() {")
m(C, "read_body: more than the length counted as body", "        if take > st[p + sl.f_brem()] {\n            take = st[p + sl.f_brem()];\n        }", "")
m(C, "read_body: the length not reduced", "        st[p + sl.f_brem()] = st[p + sl.f_brem()] - take;\n        if st[p + sl.f_brem()] == 0 {", "        if st[p + sl.f_brem()] == 0 {")
m(C, "read_body: the end of a length body not noticed", "        if st[p + sl.f_brem()] == 0 {\n            st[p + sl.f_complete()] = 1;", "        if st[p + sl.f_brem()] == 7 {\n            st[p + sl.f_complete()] = 1;")
m(C, "read_body: bytes behind a length body not noted", "            if fill - hl - st[p + sl.f_avail()] > 0 {\n                st[p + sl.f_dirty()] = 1;\n            }", "")
m(C, "read_body: bytes behind a length body noted though none", "            if fill - hl - st[p + sl.f_avail()] > 0 {\n                st[p + sl.f_dirty()] = 1;", "            if true {\n                st[p + sl.f_dirty()] = 1;")
m(C, "read_body: the chunk decoder reads from the start of the buffer", "        let from = hl + st[p + sl.f_avail()];\n        let code = wire.chunk_run(", "        let from = hl;\n        let code = wire.chunk_run(")
m(C, "read_body: a chunk error is not failed", "        if code != 0 {\n            fail(c, k, code);\n            return 0 - 1;\n        }\n        let wrote", "        let wrote")
m(C, "read_body: the available count after a chunk is wrong", "        st[p + sl.f_avail()] = wrote - hl;\n        if st[p + sl.f_chunk()] == wire.chunk_done() {", "        st[p + sl.f_avail()] = used;\n        if st[p + sl.f_chunk()] == wire.chunk_done() {")
m(C, "read_body: the end of a chunked body not noticed", "        if st[p + sl.f_chunk()] == wire.chunk_done() {\n            st[p + sl.f_complete()] = 1;", "        if st[p + sl.f_chunk()] == 99 {\n            st[p + sl.f_complete()] = 1;")
m(C, "read_body: bytes behind a chunked body are lost", "            if left > 0 {\n                copy_within(inbuf(c, k), wrote, from + used, left);\n                st[p + sl.f_dirty()] = 1;\n            }\n            st[p + sl.f_fill()] = wrote + left;", "            if left > 0 {\n                st[p + sl.f_dirty()] = 1;\n            }\n            st[p + sl.f_fill()] = wrote + left;")
m(C, "read_body: bytes behind a chunked body not noted", "                copy_within(inbuf(c, k), wrote, from + used, left);\n                st[p + sl.f_dirty()] = 1;", "                copy_within(inbuf(c, k), wrote, from + used, left);")
m(C, "read_body: the fill after a chunked body is wrong", "            st[p + sl.f_fill()] = wrote + left;\n        } else {", "            st[p + sl.f_fill()] = from + used;\n        } else {")
m(C, "read_body: the fill in the middle of a chunked body is wrong", "        } else {\n            st[p + sl.f_fill()] = wrote;\n        }", "        } else {\n            st[p + sl.f_fill()] = fill;\n        }")
m(C, "read_body: a body until close counts nothing", "    } else {\n        st[p + sl.f_avail()] = fill - hl;\n    }\n    if st[p + sl.f_avail()] > before {", "    } else {\n    }\n    if st[p + sl.f_avail()] > before {")
m(C, "read_body: more bytes are not announced", "    if st[p + sl.f_avail()] > before {\n        post(c, k, sl.ev_body());\n    }", "    if st[p + sl.f_avail()] > before + 1000000 {\n        post(c, k, sl.ev_body());\n    }")
m(C, "read_body: the end is not reported", "    if st[p + sl.f_complete()] == 1 {\n        finish_body(c, k);\n    }\n    return 0;\n}\n\n// Runs a connection's", "    return 0;\n}\n\n// Runs a connection's")
m(C, "advance: the head is read once only", "                if read_head(c, k, now) == 1 {\n                    going = true;\n                }", "                read_head(c, k, now);")
m(C, "advance: a head is not followed by its body", "            } else if get(c, k, sl.f_rs()) == sl.rs_body() {\n                read_body(c, k, now);\n            }", "            }")
m(C, "status: before the head", "    if k < 0 || get(c, k, sl.f_rs()) < sl.rs_body() {\n        return 0 - 1;\n    }\n    return contents(c.tables)[k * wire.slots(max_headers())];", "    if k < 0 {\n        return 0 - 1;\n    }\n    return contents(c.tables)[k * wire.slots(max_headers())];")
m(C, "version: the status is given", "    return contents(c.tables)[k * wire.slots(max_headers()) + 1];", "    return contents(c.tables)[k * wire.slots(max_headers())];")
m(C, "body_kind: the version is given", "    return get(c, k, sl.f_bkind());\n}", "    return get(c, k, sl.f_complete());\n}")
m(C, "content_length: the status is given", "    return contents(c.tables)[k * wire.slots(max_headers()) + 4];", "    return contents(c.tables)[k * wire.slots(max_headers())];")
m(C, "head_readable: the head is readable after it is gone", "    return get(c, k, sl.f_hl()) > 0 && get(c, k, sl.f_rs()) >= sl.rs_body();", "    return get(c, k, sl.f_rs()) >= sl.rs_body();")
m(C, "head_readable: the head is readable before it has come", "    return get(c, k, sl.f_hl()) > 0 && get(c, k, sl.f_rs()) >= sl.rs_body();", "    return get(c, k, sl.f_hl()) > 0;")
m(C, "header_count: the count of another response", "    return contents(c.tables)[k * wire.slots(max_headers()) + 2];\n}", "    return contents(c.tables)[k * wire.slots(max_headers()) + 1];\n}")
m(C, "head: the whole buffer", "    return contents(c.ins)[k * c.in_size..k * c.in_size + get(c, k, sl.f_hl())];", "    return contents(c.ins)[k * c.in_size..(k + 1) * c.in_size];")
m(C, "header_name: another slot's buffer", "    let at = k * wire.slots(max_headers()) + 16 + 4 * i;\n    let base = k * c.in_size;\n    return contents(c.ins)[base + contents(c.tables)[at]..base + contents(c.tables)[at + 1]];", "    let at = k * wire.slots(max_headers()) + 16 + 4 * i;\n    return contents(c.ins)[contents(c.tables)[at]..contents(c.tables)[at + 1]];")
m(C, "header_name: the value's span", "    return contents(c.ins)[base + contents(c.tables)[at]..base + contents(c.tables)[at + 1]];", "    return contents(c.ins)[base + contents(c.tables)[at + 2]..base + contents(c.tables)[at + 3]];")
m(C, "header_value: another slot's buffer", "    let base = k * c.in_size;\n    return contents(c.ins)[base + contents(c.tables)[at + 2]..base + contents(c.tables)[at + 3]];", "    return contents(c.ins)[contents(c.tables)[at + 2]..contents(c.tables)[at + 3]];")
m(C, "header_name: the last header out of range", "    if k < 0 || !head_readable(c, k) || i < 0 || i >= contents(c.tables)[k * wire.slots(max_headers()) + 2] {\n        return contents(c.ins)[0..0];\n    }\n    let at = k * wire.slots(max_headers()) + 16 + 4 * i;\n    let base = k * c.in_size;\n    return contents(c.ins)[base + contents(c.tables)[at]..", "    if k < 0 || !head_readable(c, k) || i < 0 || i > contents(c.tables)[k * wire.slots(max_headers()) + 2] {\n        return contents(c.ins)[0..0];\n    }\n    let at = k * wire.slots(max_headers()) + 16 + 4 * i;\n    let base = k * c.in_size;\n    return contents(c.ins)[base + contents(c.tables)[at]..")
m(C, "header: a header that is not there is the first", "    if i < 0 {\n        return contents(c.ins)[0..0];\n    }\n    return header_value(c, t, i);", "    if i < 0 {\n        return header_value(c, t, 0);\n    }\n    return header_value(c, t, i);")
m(C, "body: the bytes of the head", "    let from = k * c.in_size + get(c, k, sl.f_hl());\n    return contents(c.ins)[from..from + get(c, k, sl.f_avail())];", "    let from = k * c.in_size;\n    return contents(c.ins)[from..from + get(c, k, sl.f_avail())];")
m(C, "body: a body once the response is done", "    if k < 0 || get(c, k, sl.f_rs()) != sl.rs_body() {\n        return contents(c.ins)[0..0];\n    }\n    let from", "    if k < 0 || get(c, k, sl.f_rs()) < sl.rs_body() {\n        return contents(c.ins)[0..0];\n    }\n    let from")
m(C, "avail: bytes before the head", "pub fn avail[&r](c: &r Client, t: int) -> [] int {\n    let k = slot_of_ticket(c, t);\n    if k < 0 || get(c, k, sl.f_rs()) != sl.rs_body() {", "pub fn avail[&r](c: &r Client, t: int) -> [] int {\n    let k = slot_of_ticket(c, t);\n    if k < 0 {")
m(C, "consume: more than is waiting", "    if k < 0 || get(c, k, sl.f_rs()) != sl.rs_body() || n < 0 || n > get(c, k, sl.f_avail()) {\n        return 0 - 1;\n    }", "    if k < 0 || get(c, k, sl.f_rs()) != sl.rs_body() || n < 0 {\n        return 0 - 1;\n    }")
m(C, "consume: a negative count", "    if k < 0 || get(c, k, sl.f_rs()) != sl.rs_body() || n < 0 || n > get(c, k, sl.f_avail()) {\n        return 0 - 1;\n    }", "    if k < 0 || get(c, k, sl.f_rs()) != sl.rs_body() || n > get(c, k, sl.f_avail()) {\n        return 0 - 1;\n    }")
m(C, "consume: the head is not dropped", "    if hl > 0 {\n        copy_within(inbuf(c, k), 0, hl, st[p + sl.f_fill()] - hl);\n        st[p + sl.f_fill()] = st[p + sl.f_fill()] - hl;\n        st[p + sl.f_hl()] = 0;\n    }", "")
m(C, "consume: the head's length is kept after it is dropped", "        st[p + sl.f_fill()] = st[p + sl.f_fill()] - hl;\n        st[p + sl.f_hl()] = 0;", "        st[p + sl.f_fill()] = st[p + sl.f_fill()] - hl;")
m(C, "consume: the fill is not reduced by the head", "        st[p + sl.f_fill()] = st[p + sl.f_fill()] - hl;\n        st[p + sl.f_hl()] = 0;", "        st[p + sl.f_hl()] = 0;")
m(C, "consume: what is left is not moved to the front", "    if left > 0 {\n        copy_within(inbuf(c, k), 0, n, left);\n    }", "")
m(C, "consume: the fill not reduced", "    st[p + sl.f_fill()] = left;\n    st[p + sl.f_avail()] = st[p + sl.f_avail()] - n;", "    st[p + sl.f_avail()] = st[p + sl.f_avail()] - n;")
m(C, "consume: the available count not reduced", "    st[p + sl.f_fill()] = left;\n    st[p + sl.f_avail()] = st[p + sl.f_avail()] - n;", "    st[p + sl.f_fill()] = left;")
m(C, "consume: emptying the buffer is not progress", "    if st[p + sl.f_avail()] == 0 {\n        st[p + sl.f_t_prog()] = now;\n        if st[p + sl.f_complete()] == 1 {", "    if st[p + sl.f_avail()] == 0 {\n        if st[p + sl.f_complete()] == 1 {")
m(C, "consume: the clock restarts for any bytes taken", "    if st[p + sl.f_avail()] == 0 {\n        st[p + sl.f_t_prog()] = now;\n        if st[p + sl.f_complete()] == 1 {", "    st[p + sl.f_t_prog()] = now;\n    if st[p + sl.f_avail()] == 0 {\n        if st[p + sl.f_complete()] == 1 {")
m(C, "consume: the end of the body is not reported", "        if st[p + sl.f_complete()] == 1 {\n            finish_body(c, k);\n        }\n    }\n    return n;", "    }\n    return n;")
m(C, "request_complete: any ticket is complete", "    return k >= 0 && flag(c, k, sl.rf_req_done());", "    return flag(c, 0, sl.rf_req_done());")
m(C, "reused: a stale ticket is reused", "    return k >= 0 && get(c, k, sl.f_reused()) == 1;", "    return get(c, 0, sl.f_reused()) == 1;")
m(C, "attempts: the retry not reported", "    return get(c, k, sl.f_attempts());\n}\n\n// Did `Event::Continue`", "    return 0;\n}\n\n// Did `Event::Continue`")
m(C, "continued_by_timeout: always", "    return k >= 0 && get(c, k, sl.f_ctimeout()) == 1;", "    return k >= 0;")
m(C, "failure: the code not given", "    return get(c, k, sl.f_fail());\n}", "    return 0;\n}")
m(C, "can_replay: a fresh connection is replayed", "    return get(c, k, sl.f_reused()) == 1 && get(c, k, sl.f_anyresp()) == 0", "    return get(c, k, sl.f_anyresp()) == 0")
m(C, "can_replay: a request is replayed after a response byte", " && get(c, k, sl.f_anyresp()) == 0 && flag(c, k, sl.rf_retry())", " && flag(c, k, sl.rf_retry())")
m(C, "can_replay: a request is replayed without the caller's leave", " && flag(c, k, sl.rf_retry()) && get(c, k, sl.f_attempts()) == 0", " && get(c, k, sl.f_attempts()) == 0")
m(C, "can_replay: a request is replayed twice", " && get(c, k, sl.f_attempts()) == 0 && flag(c, k, sl.rf_replayable())", " && flag(c, k, sl.rf_replayable())")
m(C, "can_replay: a request that outgrew its buffer is replayed", " && get(c, k, sl.f_attempts()) == 0 && flag(c, k, sl.rf_replayable()) && get(c, k, sl.f_live()) == 1;", " && get(c, k, sl.f_attempts()) == 0 && get(c, k, sl.f_live()) == 1;")
m(C, "replay: the attempt is not counted", "    st[p + sl.f_attempts()] = 1;\n    st[p + sl.f_reconnect()] = 1;", "    st[p + sl.f_reconnect()] = 1;")
m(C, "replay: the connection is not made again", "    st[p + sl.f_attempts()] = 1;\n    st[p + sl.f_reconnect()] = 1;", "    st[p + sl.f_attempts()] = 1;")
m(C, "replay: the request is not sent again", "    st[p + sl.f_reconnect()] = 1;\n    st[p + sl.f_out_sent()] = 0;", "    st[p + sl.f_reconnect()] = 1;")
m(C, "replay: the new connection is called reused", "    st[p + sl.f_out_sent()] = 0;\n    st[p + sl.f_reused()] = 0;", "    st[p + sl.f_out_sent()] = 0;")
m(C, "replay: the fill of the old response is kept", "    st[p + sl.f_reused()] = 0;\n    st[p + sl.f_fill()] = 0;", "    st[p + sl.f_reused()] = 0;")
m(C, "replay: the request is not reset to unsent", "    set_flag(c, k, sl.rf_req_done(), false);\n    c.n_retries", "    c.n_retries")
m(C, "replay: retries not counted", "    c.n_retries = c.n_retries + 1;\n    close_slot(c, k, r_retry());", "    close_slot(c, k, r_retry());")
m(C, "replay: the old connection is not closed", "    close_slot(c, k, r_retry());\n    return 0;\n}\n\n// The transport ended", "    return 0;\n}\n\n// The transport ended")
m(C, "transport_ended: an idle connection that ends stays", "    if phase == sl.ph_idle() {\n        close_slot(c, k, r_peer_closed());\n        return 0;\n    }", "    if phase == sl.ph_idle() {\n        return 0;\n    }")
m(C, "transport_ended: a dial that ends is not failed", "    if phase == sl.ph_connect() {\n        fail(c, k, wire.c_connect());\n        return 0;\n    }", "    if phase == sl.ph_connect() {\n        return 0;\n    }")
m(C, "transport_ended: a complete response that ends can be reused", "    if rs == sl.rs_done() || get(c, k, sl.f_complete()) == 1 {\n        put(c, k, sl.f_eof(), 1);\n        return 0;\n    }", "    if rs == sl.rs_done() || get(c, k, sl.f_complete()) == 1 {\n        return 0;\n    }")
m(C, "transport_ended: a failed request fails again", "    if phase != sl.ph_active() || get(c, k, sl.f_live()) != 1 {\n        return 0;\n    }\n    let rs", "    if phase != sl.ph_active() {\n        return 0;\n    }\n    let rs")
m(C, "transport_ended: no replay on an end", "        if get(c, k, sl.f_anyresp()) == 0 && can_replay(c, k) {\n            replay(c, k);\n            return 0;\n        }", "")
m(C, "transport_ended: a replay after a response byte", "        if get(c, k, sl.f_anyresp()) == 0 && can_replay(c, k) {", "        if can_replay(c, k) {")
m(C, "transport_ended: a reset in a head is an end", "        if reset {\n            fail(c, k, wire.c_reset());\n        } else if get(c, k, sl.f_anyresp()) == 0 {", "        if false {\n            fail(c, k, wire.c_reset());\n        } else if get(c, k, sl.f_anyresp()) == 0 {")
m(C, "transport_ended: an end before any byte is truncated", "        } else if get(c, k, sl.f_anyresp()) == 0 {\n            fail(c, k, wire.c_closed_early());", "        } else if get(c, k, sl.f_anyresp()) == 7 {\n            fail(c, k, wire.c_closed_early());")
m(C, "transport_ended: an end inside a head is closed early", "        } else {\n            fail(c, k, wire.c_truncated());\n        }\n        return 0;\n    }\n    // In the body.", "        } else {\n            fail(c, k, wire.c_closed_early());\n        }\n        return 0;\n    }\n    // In the body.")
m(C, "transport_ended: a reset in a body is an end", "    // In the body.\n    if reset {\n        fail(c, k, wire.c_reset());", "    // In the body.\n    if false {\n        fail(c, k, wire.c_reset());")
m(C, "transport_ended: a body to the end of the connection is truncated", "    } else if get(c, k, sl.f_bkind()) == sl.bk_close() {\n        // A body that runs", "    } else if get(c, k, sl.f_bkind()) == 7 {\n        // A body that runs")
m(C, "transport_ended: a body to the end of the connection is not complete", "        put(c, k, sl.f_complete(), 1);\n        put(c, k, sl.f_eof(), 1);\n        finish_body(c, k);", "        put(c, k, sl.f_eof(), 1);\n        finish_body(c, k);")
m(C, "transport_ended: a body to the end of the connection can be reused", "        put(c, k, sl.f_complete(), 1);\n        put(c, k, sl.f_eof(), 1);\n        finish_body(c, k);", "        put(c, k, sl.f_complete(), 1);\n        finish_body(c, k);")
m(C, "transport_ended: a body to the end of the connection is not finished", "        put(c, k, sl.f_eof(), 1);\n        finish_body(c, k);\n    } else {\n        fail(c, k, wire.c_truncated());", "        put(c, k, sl.f_eof(), 1);\n    } else {\n        fail(c, k, wire.c_truncated());")
m(C, "transport_ended: a cut length body is not failed", "    } else {\n        fail(c, k, wire.c_truncated());\n    }\n    return 0;\n}\n\n// The peer ended", "    } else {\n    }\n    return 0;\n}\n\n// The peer ended")
m(C, "eof: an end on a free slot is accepted", "pub fn eof[&r](c: &!r Client, k: int, now: int) -> [] int {\n    if !slot_ok(c, k) || get(c, k, sl.f_phase()) == sl.ph_free() {", "pub fn eof[&r](c: &!r Client, k: int, now: int) -> [] int {\n    if !slot_ok(c, k) {")
m(C, "eof: an end is a reset", "    transport_ended(c, k, false, now);\n    return 0;\n}\n\n// The transport failed", "    transport_ended(c, k, true, now);\n    return 0;\n}\n\n// The transport failed")
m(C, "reset: a reset is an end", "    transport_ended(c, k, true, now);\n    return 0;\n}\n\n// The caller has closed", "    transport_ended(c, k, false, now);\n    return 0;\n}\n\n// The caller has closed")
m(C, "reset: a reset on a free slot is accepted", "pub fn reset[&r](c: &!r Client, k: int, now: int) -> [] int {\n    if !slot_ok(c, k) || get(c, k, sl.f_phase()) == sl.ph_free() {", "pub fn reset[&r](c: &!r Client, k: int, now: int) -> [] int {\n    if !slot_ok(c, k) {")
m(C, "detach: a slot that is not closing is freed", "    if !slot_ok(c, k) || get(c, k, sl.f_phase()) != sl.ph_closing() {\n        return 0 - 1;\n    }\n    unpost(c, k, sl.ev_close());", "    if !slot_ok(c, k) {\n        return 0 - 1;\n    }\n    unpost(c, k, sl.ev_close());")
m(C, "detach: the Close event is still delivered", "    unpost(c, k, sl.ev_close());\n    if get(c, k, sl.f_reconnect()) == 1", "    if get(c, k, sl.f_reconnect()) == 1")
m(C, "detach: a request that was given up reconnects", "    if get(c, k, sl.f_reconnect()) == 1 && get(c, k, sl.f_live()) == 1 {", "    if get(c, k, sl.f_reconnect()) == 1 {")
m(C, "detach: a replay does not reconnect", "    if get(c, k, sl.f_reconnect()) == 1 && get(c, k, sl.f_live()) == 1 {", "    if get(c, k, sl.f_reconnect()) == 7 && get(c, k, sl.f_live()) == 1 {")
m(C, "detach: the dial clock not started", "        put(c, k, sl.f_phase(), sl.ph_connect());\n        put(c, k, sl.f_t_phase(), now);\n        put(c, k, sl.f_t_prog(), now);", "        put(c, k, sl.f_phase(), sl.ph_connect());\n        put(c, k, sl.f_t_prog(), now);")
m(C, "detach: the progress clock not restarted", "        put(c, k, sl.f_t_phase(), now);\n        put(c, k, sl.f_t_prog(), now);\n        put(c, k, sl.f_rs(), sl.rs_head());", "        put(c, k, sl.f_t_phase(), now);\n        put(c, k, sl.f_rs(), sl.rs_head());")
m(C, "detach: the response state not restarted", "        put(c, k, sl.f_t_prog(), now);\n        put(c, k, sl.f_rs(), sl.rs_head());\n        c.n_connects", "        put(c, k, sl.f_t_prog(), now);\n        c.n_connects")
m(C, "detach: the new dial is not counted", "        put(c, k, sl.f_rs(), sl.rs_head());\n        c.n_connects = c.n_connects + 1;", "        put(c, k, sl.f_rs(), sl.rs_head());")
m(C, "detach: the new dial is not announced", "        c.n_connects = c.n_connects + 1;\n        post(c, k, sl.ev_connect());\n        return 0;", "        c.n_connects = c.n_connects + 1;\n        return 0;")
m(C, "detach: the reconnect flag is kept", "        put(c, k, sl.f_reconnect(), 0);\n        put(c, k, sl.f_phase(), sl.ph_connect());", "        put(c, k, sl.f_phase(), sl.ph_connect());")
m(C, "detach: the slot is not freed", "    put(c, k, sl.f_phase(), sl.ph_free());\n    put(c, k, sl.f_reconnect(), 0);\n    return 0;", "    put(c, k, sl.f_reconnect(), 0);\n    return 0;")
m(C, "abort: a stale ticket aborts", "    let k = slot_of_ticket(c, t);\n    if k < 0 || get(c, k, sl.f_live()) != 1 {\n        return 0 - 1;\n    }\n    put(c, k, sl.f_live(), 0);", "    let k = slot_of_ticket(c, t);\n    if k < 0 {\n        return 0 - 1;\n    }\n    put(c, k, sl.f_live(), 0);")
m(C, "abort: the request stays live", "    put(c, k, sl.f_live(), 0);\n    put(c, k, sl.f_reconnect(), 0);\n    unpost(c, k, sl.ev_head());", "    put(c, k, sl.f_reconnect(), 0);\n    unpost(c, k, sl.ev_head());")
m(C, "abort: a replay still waiting", "    put(c, k, sl.f_live(), 0);\n    put(c, k, sl.f_reconnect(), 0);\n    unpost(c, k, sl.ev_head());", "    put(c, k, sl.f_live(), 0);\n    unpost(c, k, sl.ev_head());")
m(C, "abort: the Head still delivered", "    unpost(c, k, sl.ev_head());\n    unpost(c, k, sl.ev_body());", "    unpost(c, k, sl.ev_body());")
m(C, "abort: the Body still delivered", "    unpost(c, k, sl.ev_body());\n    unpost(c, k, sl.ev_done());\n    unpost(c, k, sl.ev_continue());\n    unpost(c, k, sl.ev_connect());\n    let phase", "    unpost(c, k, sl.ev_done());\n    unpost(c, k, sl.ev_continue());\n    unpost(c, k, sl.ev_connect());\n    let phase")
m(C, "abort: the Done still delivered", "    unpost(c, k, sl.ev_done());\n    unpost(c, k, sl.ev_continue());\n    unpost(c, k, sl.ev_connect());\n    let phase", "    unpost(c, k, sl.ev_continue());\n    unpost(c, k, sl.ev_connect());\n    let phase")
m(C, "abort: the Continue still delivered", "    unpost(c, k, sl.ev_continue());\n    unpost(c, k, sl.ev_connect());\n    let phase", "    unpost(c, k, sl.ev_connect());\n    let phase")
m(C, "abort: the dial still announced", "    unpost(c, k, sl.ev_continue());\n    unpost(c, k, sl.ev_connect());\n    let phase", "    unpost(c, k, sl.ev_continue());\n    let phase")
m(C, "abort: the transport not closed", "    if phase == sl.ph_closing() {\n        return 0;\n    }\n    close_slot(c, k, r_aborted());\n    return 0;", "    return 0;")
m(C, "abort: a closing transport closed again", "    if phase == sl.ph_closing() {\n        return 0;\n    }\n    close_slot(c, k, r_aborted());", "    close_slot(c, k, r_aborted());")
m(C, "retire: other upstreams retired", "        if get(c, k, sl.f_phase()) != sl.ph_free() && get(c, k, sl.f_key()) == key {\n            if get(c, k, sl.f_phase()) == sl.ph_idle() {", "        if get(c, k, sl.f_phase()) != sl.ph_free() {\n            if get(c, k, sl.f_phase()) == sl.ph_idle() {")
m(C, "retire: an idle connection stays", "            if get(c, k, sl.f_phase()) == sl.ph_idle() {\n                close_slot(c, k, r_retired());\n                closed = closed + 1;\n            } else {\n                put(c, k, sl.f_nopool(), 1);\n            }", "            if get(c, k, sl.f_phase()) == sl.ph_idle() {\n                put(c, k, sl.f_nopool(), 1);\n            } else {\n                put(c, k, sl.f_nopool(), 1);\n            }")
m(C, "retire: the closed ones not counted", "                closed = closed + 1;\n            } else {\n                put(c, k, sl.f_nopool(), 1);", "            } else {\n                put(c, k, sl.f_nopool(), 1);")
m(C, "retire: a busy connection is closed", "            } else {\n                put(c, k, sl.f_nopool(), 1);\n            }\n        }\n        k = k + 1;\n    }\n    return closed;", "            } else {\n                close_slot(c, k, r_retired());\n            }\n        }\n        k = k + 1;\n    }\n    return closed;")
m(C, "retire: a busy connection is still pooled", "            } else {\n                put(c, k, sl.f_nopool(), 1);\n            }\n        }\n        k = k + 1;\n    }\n    return closed;", "            }\n        }\n        k = k + 1;\n    }\n    return closed;")
m(C, "close_idle: a busy connection is closed", "        if get(c, k, sl.f_phase()) == sl.ph_idle() {\n            close_slot(c, k, r_retired());\n            closed = closed + 1;\n        }\n        k = k + 1;\n    }\n    return closed;\n}\n\n// ---", "        if get(c, k, sl.f_phase()) != sl.ph_free() {\n            close_slot(c, k, r_retired());\n            closed = closed + 1;\n        }\n        k = k + 1;\n    }\n    return closed;\n}\n\n// ---")
m(C, "close_idle: the closed ones not counted", "            close_slot(c, k, r_retired());\n            closed = closed + 1;\n        }\n        k = k + 1;\n    }\n    return closed;\n}\n\n// ---", "            close_slot(c, k, r_retired());\n        }\n        k = k + 1;\n    }\n    return closed;\n}\n\n// ---")
m(C, "reusable: HTTP/1.0 is reused", "    if table[1] != 11 || table[5] / 2 % 2 == 1", "    if table[5] / 2 % 2 == 1")
m(C, "reusable: `Connection: close` is reused", "    if table[1] != 11 || table[5] / 2 % 2 == 1 || flag(c, k, sl.rf_close())", "    if table[1] != 11 || flag(c, k, sl.rf_close())")
m(C, "reusable: a request that said close is reused", "    if table[1] != 11 || table[5] / 2 % 2 == 1 || flag(c, k, sl.rf_close()) || get(c, k, sl.f_bkind()) == sl.bk_close() {", "    if table[1] != 11 || table[5] / 2 % 2 == 1 || get(c, k, sl.f_bkind()) == sl.bk_close() {")
m(C, "reusable: a body to the end of the connection is reused", "    if table[1] != 11 || table[5] / 2 % 2 == 1 || flag(c, k, sl.rf_close()) || get(c, k, sl.f_bkind()) == sl.bk_close() {", "    if table[1] != 11 || table[5] / 2 % 2 == 1 || flag(c, k, sl.rf_close()) {")
m(C, "reusable: a request not fully sent is reused", "    if !flag(c, k, sl.rf_req_done()) || flag(c, k, sl.rf_early())", "    if flag(c, k, sl.rf_early())")
m(C, "reusable: an early response is reused", "    if !flag(c, k, sl.rf_req_done()) || flag(c, k, sl.rf_early()) || get(c, k, sl.f_dirty()) == 1", "    if !flag(c, k, sl.rf_req_done()) || get(c, k, sl.f_dirty()) == 1")
m(C, "reusable: bytes behind the response are reused", " || flag(c, k, sl.rf_early()) || get(c, k, sl.f_dirty()) == 1 || get(c, k, sl.f_eof()) == 1", " || flag(c, k, sl.rf_early()) || get(c, k, sl.f_eof()) == 1")
m(C, "reusable: a connection that ended is reused", " || get(c, k, sl.f_dirty()) == 1 || get(c, k, sl.f_eof()) == 1 || get(c, k, sl.f_nopool()) == 1 {", " || get(c, k, sl.f_dirty()) == 1 || get(c, k, sl.f_nopool()) == 1 {")
m(C, "reusable: a retired upstream's connection is reused", " || get(c, k, sl.f_eof()) == 1 || get(c, k, sl.f_nopool()) == 1 {", " || get(c, k, sl.f_eof()) == 1 {")
m(C, "reusable: no limit on requests", "    if c.limits.max_requests > 0 && get(c, k, sl.f_uses()) >= c.limits.max_requests {", "    if false && get(c, k, sl.f_uses()) >= c.limits.max_requests {")
m(C, "reusable: one request too many", "    if c.limits.max_requests > 0 && get(c, k, sl.f_uses()) >= c.limits.max_requests {", "    if c.limits.max_requests > 0 && get(c, k, sl.f_uses()) > c.limits.max_requests {")
m(C, "reusable: a limit of 0 requests refuses all", "    if c.limits.max_requests > 0 && get(c, k, sl.f_uses()) >= c.limits.max_requests {", "    if get(c, k, sl.f_uses()) >= c.limits.max_requests {")
m(C, "reusable: no limit on life", "    if c.limits.lifetime_ms > 0 && now - get(c, k, sl.f_t_open()) >= c.limits.lifetime_ms {\n        return r_lifetime();", "    if false && now - get(c, k, sl.f_t_open()) >= c.limits.lifetime_ms {\n        return r_lifetime();")
m(C, "reusable: a connection a millisecond short of its life is refused", "    if c.limits.lifetime_ms > 0 && now - get(c, k, sl.f_t_open()) >= c.limits.lifetime_ms {\n        return r_lifetime();", "    if c.limits.lifetime_ms > 0 && now - get(c, k, sl.f_t_open()) + 1 >= c.limits.lifetime_ms {\n        return r_lifetime();")
m(C, "reusable: a connection past its life is kept", "    if c.limits.lifetime_ms > 0 && now - get(c, k, sl.f_t_open()) >= c.limits.lifetime_ms {\n        return r_lifetime();", "    if c.limits.lifetime_ms > 0 && now - get(c, k, sl.f_t_open()) > c.limits.lifetime_ms {\n        return r_lifetime();")
m(C, "reusable: no limit on idle connections of an upstream", "    if idle_with_key(c, get(c, k, sl.f_key())) >= c.limits.idle_per_key {", "    if false && idle_with_key(c, get(c, k, sl.f_key())) >= c.limits.idle_per_key {")
m(C, "reusable: one idle connection too many", "    if idle_with_key(c, get(c, k, sl.f_key())) >= c.limits.idle_per_key {", "    if idle_with_key(c, get(c, k, sl.f_key())) > c.limits.idle_per_key {")
m(C, "poll: a stale Connect is delivered", "        if ev == sl.ev_connect() && get(c, k, sl.f_phase()) == sl.ph_connect() {", "        if ev == sl.ev_connect() {")
m(C, "poll: Connect not delivered", "        if ev == sl.ev_connect() && get(c, k, sl.f_phase()) == sl.ph_connect() {\n            return Event::Connect(k);\n        }", "")
m(C, "poll: Continue not delivered", "        if ev == sl.ev_continue() {\n            return Event::Continue(ticket);\n        }", "")
m(C, "poll: Head not delivered", "        if ev == sl.ev_head() {\n            return Event::Head(ticket);\n        }", "")
m(C, "poll: a Body with nothing to take is delivered", "        if ev == sl.ev_body() && get(c, k, sl.f_avail()) > 0 {", "        if ev == sl.ev_body() {")
m(C, "poll: Body not delivered", "        if ev == sl.ev_body() && get(c, k, sl.f_avail()) > 0 {\n            return Event::Body(ticket);\n        }", "")
m(C, "poll: Failed not delivered", "        if ev == sl.ev_failed() {\n            return Event::Failed(ticket);\n        }", "")
m(C, "poll: Close not delivered", "        if ev == sl.ev_close() {\n            return Event::Close(k);\n        }", "")
m(C, "poll: Done not delivered", "            return Event::Done(ticket);\n        }\n        if ev == sl.ev_failed()", "        }\n        if ev == sl.ev_failed()")
m(C, "poll: events of a slot out of order", "        } else if bits / 4 % 2 == 1 {\n            ev = sl.ev_head();\n        } else if bits / 8 % 2 == 1 {\n            ev = sl.ev_body();", "        } else if bits / 8 % 2 == 1 {\n            ev = sl.ev_body();\n        } else if bits / 4 % 2 == 1 {\n            ev = sl.ev_head();")
m(C, "poll: a slot leaves the queue with events waiting", "        if bits == 0 {\n            put(c, k, sl.f_queued(), 0);\n            c.qhead = (c.qhead + 1) % c.nslots;\n            c.qcount = c.qcount - 1;\n        }", "        put(c, k, sl.f_queued(), 0);\n        c.qhead = (c.qhead + 1) % c.nslots;\n        c.qcount = c.qcount - 1;")
m(C, "poll: a slot stays in the queue when empty", "        if bits == 0 {\n            put(c, k, sl.f_queued(), 0);\n            c.qhead = (c.qhead + 1) % c.nslots;\n            c.qcount = c.qcount - 1;\n        }", "        if bits == 7 {\n            put(c, k, sl.f_queued(), 0);\n            c.qhead = (c.qhead + 1) % c.nslots;\n            c.qcount = c.qcount - 1;\n        }")
m(C, "poll: the queued flag not cleared", "        if bits == 0 {\n            put(c, k, sl.f_queued(), 0);", "        if bits == 0 {")
m(C, "poll: the event is not cleared when taken", "        if ev != 0 {\n            unpost(c, k, ev);\n            bits = bits - ev;\n        }", "        if ev != 0 {\n            bits = bits - ev;\n        }")
m(C, "poll: Done does not return the connection to the pool", "            if why == 0 {\n                put(c, k, sl.f_phase(), sl.ph_idle());\n                put(c, k, sl.f_t_phase(), now);", "            if why == 7 {\n                put(c, k, sl.f_phase(), sl.ph_idle());\n                put(c, k, sl.f_t_phase(), now);")
m(C, "poll: Done pools a connection that must not be", "            if why == 0 {\n                put(c, k, sl.f_phase(), sl.ph_idle());", "            if true {\n                put(c, k, sl.f_phase(), sl.ph_idle());")
m(C, "poll: the pooled connection's idle clock not started", "                put(c, k, sl.f_phase(), sl.ph_idle());\n                put(c, k, sl.f_t_phase(), now);\n            } else {", "                put(c, k, sl.f_phase(), sl.ph_idle());\n            } else {")
m(C, "poll: Done does not close what must not be pooled", "            } else {\n                close_slot(c, k, why);\n            }\n            return Event::Done(ticket);", "            }\n            return Event::Done(ticket);")
m(C, "poll: the request stays live after Done", "            let why = reusable(c, k, now);\n            put(c, k, sl.f_live(), 0);", "            let why = reusable(c, k, now);")
m(C, "due: a connect without a timer", "        if c.limits.connect_ms > 0 {\n            at = get(c, k, sl.f_t_phase()) + c.limits.connect_ms;\n        }", "")
m(C, "due: a connect timer from the request's start", "        if c.limits.connect_ms > 0 {\n            at = get(c, k, sl.f_t_phase()) + c.limits.connect_ms;", "        if c.limits.connect_ms > 0 {\n            at = get(c, k, sl.f_t_req()) + c.limits.connect_ms;")
m(C, "due: an idle connection without a timer", "        if c.limits.idle_ms > 0 {\n            at = get(c, k, sl.f_t_phase()) + c.limits.idle_ms;\n        }", "")
m(C, "due: an idle timer from the connection's age", "            at = get(c, k, sl.f_t_phase()) + c.limits.idle_ms;", "            at = get(c, k, sl.f_t_open()) + c.limits.idle_ms;")
m(C, "due: no lifetime timer", "        if c.limits.lifetime_ms > 0 {\n            let end = get(c, k, sl.f_t_open()) + c.limits.lifetime_ms;\n            if at < 0 || end < at {\n                at = end;\n            }\n        }", "")
m(C, "due: the lifetime replaces the idle timer", "            if at < 0 || end < at {\n                at = end;\n            }", "            at = end;")
m(C, "due: the later of lifetime and idle", "            if at < 0 || end < at {\n                at = end;\n            }", "            if at < 0 || end > at {\n                at = end;\n            }")
m(C, "due: no total timer", "        if c.limits.total_ms > 0 {\n            t2 = get(c, k, sl.f_t_req()) + c.limits.total_ms;\n        }", "")
m(C, "due: a total timer that runs after Done", "    if (phase == sl.ph_connect() || phase == sl.ph_active()) && get(c, k, sl.f_live()) == 1 && get(c, k, sl.f_rs()) != sl.rs_done() {", "    if (phase == sl.ph_connect() || phase == sl.ph_active()) && get(c, k, sl.f_live()) == 1 {")
m(C, "due: a total timer for a request that is over", "    if (phase == sl.ph_connect() || phase == sl.ph_active()) && get(c, k, sl.f_live()) == 1 && get(c, k, sl.f_rs()) != sl.rs_done() {", "    if (phase == sl.ph_connect() || phase == sl.ph_active()) && get(c, k, sl.f_rs()) != sl.rs_done() {")
m(C, "due: an Expect timer that never runs", "            if flag(c, k, sl.rf_hold()) && get(c, k, sl.f_t_hold()) >= 0 && c.limits.expect_ms > 0 {", "            if false && flag(c, k, sl.rf_hold()) && get(c, k, sl.f_t_hold()) >= 0 && c.limits.expect_ms > 0 {")
m(C, "due: an Expect timer before the head has gone", "            if flag(c, k, sl.rf_hold()) && get(c, k, sl.f_t_hold()) >= 0 && c.limits.expect_ms > 0 {", "            if flag(c, k, sl.rf_hold()) && c.limits.expect_ms > 0 {")
m(C, "due: an Expect timer from the request's start", "                let t3 = get(c, k, sl.f_t_hold()) + c.limits.expect_ms;", "                let t3 = get(c, k, sl.f_t_req()) + c.limits.expect_ms;")
m(C, "due: no head timer", "            if rs == sl.rs_head() && c.limits.head_ms > 0 && (flag(c, k, sl.rf_req_done()) || pending(c, k) > 0) {", "            if false && rs == sl.rs_head() && c.limits.head_ms > 0 && (flag(c, k, sl.rf_req_done()) || pending(c, k) > 0) {")
m(C, "due: a head timer while the caller is slow", "            if rs == sl.rs_head() && c.limits.head_ms > 0 && (flag(c, k, sl.rf_req_done()) || pending(c, k) > 0) {", "            if rs == sl.rs_head() && c.limits.head_ms > 0 {")
m(C, "due: a head timer that ignores a stalled transport", "            if rs == sl.rs_head() && c.limits.head_ms > 0 && (flag(c, k, sl.rf_req_done()) || pending(c, k) > 0) {", "            if rs == sl.rs_head() && c.limits.head_ms > 0 && flag(c, k, sl.rf_req_done()) {")
m(C, "due: a head timer that ignores a finished request", "            if rs == sl.rs_head() && c.limits.head_ms > 0 && (flag(c, k, sl.rf_req_done()) || pending(c, k) > 0) {", "            if rs == sl.rs_head() && c.limits.head_ms > 0 && pending(c, k) > 0 {")
m(C, "due: the head timer from the request's start", "                let t4 = get(c, k, sl.f_t_prog()) + c.limits.head_ms;", "                let t4 = get(c, k, sl.f_t_req()) + c.limits.head_ms;")
m(C, "due: no body timer", "            if rs == sl.rs_body() && get(c, k, sl.f_complete()) == 0 && get(c, k, sl.f_avail()) == 0 && c.limits.body_ms > 0 {", "            if false && rs == sl.rs_body() && get(c, k, sl.f_complete()) == 0 && get(c, k, sl.f_avail()) == 0 && c.limits.body_ms > 0 {")
m(C, "due: a body timer on a complete body", "            if rs == sl.rs_body() && get(c, k, sl.f_complete()) == 0 && get(c, k, sl.f_avail()) == 0 && c.limits.body_ms > 0 {", "            if rs == sl.rs_body() && get(c, k, sl.f_avail()) == 0 && c.limits.body_ms > 0 {")
m(C, "due: a body timer while the caller is slow", "            if rs == sl.rs_body() && get(c, k, sl.f_complete()) == 0 && get(c, k, sl.f_avail()) == 0 && c.limits.body_ms > 0 {", "            if rs == sl.rs_body() && get(c, k, sl.f_complete()) == 0 && c.limits.body_ms > 0 {")
m(C, "due: the body timer from the request's start", "                let t5 = get(c, k, sl.f_t_prog()) + c.limits.body_ms;", "                let t5 = get(c, k, sl.f_t_req()) + c.limits.body_ms;")
m(C, "due: the earliest timer is not taken", "        if t2 >= 0 && (at < 0 || t2 < at) {\n            at = t2;\n        }", "        if t2 >= 0 && at < 0 {\n            at = t2;\n        }")
m(C, "tick: a timer that is not yet due fires", "        if at >= 0 && now >= at {\n            fired = fired + 1;", "        if at >= 0 && now + 1 >= at {\n            fired = fired + 1;")
m(C, "tick: a timer fires a millisecond late", "        if at >= 0 && now >= at {\n            fired = fired + 1;", "        if at >= 0 && now > at {\n            fired = fired + 1;")
m(C, "tick: timers not counted", "            fired = fired + 1;\n            let phase", "            let phase")
m(C, "tick: an idle connection past its life is closed as idle", "                if c.limits.lifetime_ms > 0 && now - get(c, k, sl.f_t_open()) >= c.limits.lifetime_ms {\n                    close_slot(c, k, r_lifetime());\n                } else {\n                    close_slot(c, k, r_idle_expired());\n                }", "                close_slot(c, k, r_idle_expired());")
m(C, "tick: an expired idle connection is kept", "                    close_slot(c, k, r_idle_expired());\n                }\n            } else if phase == sl.ph_connect() {", "                }\n            } else if phase == sl.ph_connect() {")
m(C, "tick: a dial past the total bound is called a connect timeout", "                if c.limits.total_ms > 0 && now - get(c, k, sl.f_t_req()) >= c.limits.total_ms {\n                    fail(c, k, wire.c_total_timeout());\n                } else {\n                    fail(c, k, wire.c_connect_timeout());\n                }", "                fail(c, k, wire.c_connect_timeout());")
m(C, "tick: a dial that times out is failed with the wrong code", "                    fail(c, k, wire.c_connect_timeout());\n                }\n            } else {", "                    fail(c, k, wire.c_head_timeout());\n                }\n            } else {")
m(C, "tick: the total bound is called a head timeout", "                if c.limits.total_ms > 0 && now - get(c, k, sl.f_t_req()) >= c.limits.total_ms {\n                    fail(c, k, wire.c_total_timeout());\n                } else if flag(c, k, sl.rf_hold())", "                if false {\n                    fail(c, k, wire.c_total_timeout());\n                } else if flag(c, k, sl.rf_hold())")
m(C, "tick: an Expect that times out fails the request", "                    set_flag(c, k, sl.rf_hold(), false);\n                    put(c, k, sl.f_ctimeout(), 1);\n                    post(c, k, sl.ev_continue());", "                    fail(c, k, wire.c_head_timeout());")
m(C, "tick: an Expect that times out is not released", "                    set_flag(c, k, sl.rf_hold(), false);\n                    put(c, k, sl.f_ctimeout(), 1);\n                    post(c, k, sl.ev_continue());", "                    put(c, k, sl.f_ctimeout(), 1);\n                    post(c, k, sl.ev_continue());")
m(C, "tick: an Expect that times out is not announced", "                    put(c, k, sl.f_ctimeout(), 1);\n                    post(c, k, sl.ev_continue());", "                    put(c, k, sl.f_ctimeout(), 1);")
m(C, "tick: an Expect that times out is not marked", "                    put(c, k, sl.f_ctimeout(), 1);\n                    post(c, k, sl.ev_continue());", "                    post(c, k, sl.ev_continue());")
m(C, "tick: a head timeout is a body timeout", "                } else if rs == sl.rs_head() {\n                    fail(c, k, wire.c_head_timeout());\n                } else {\n                    fail(c, k, wire.c_body_timeout());", "                } else if rs == sl.rs_head() {\n                    fail(c, k, wire.c_body_timeout());\n                } else {\n                    fail(c, k, wire.c_body_timeout());")
m(C, "tick: a body timeout is a head timeout", "                } else if rs == sl.rs_head() {\n                    fail(c, k, wire.c_head_timeout());\n                } else {\n                    fail(c, k, wire.c_body_timeout());", "                } else if rs == sl.rs_head() {\n                    fail(c, k, wire.c_head_timeout());\n                } else {\n                    fail(c, k, wire.c_head_timeout());")
m(C, "next_deadline: no timers is 0", "    if best < 0 {\n        return 0 - 1;\n    }\n    if best <= now {", "    if best < 0 {\n        return 0;\n    }\n    if best <= now {")
m(C, "next_deadline: a timer due is not 0", "    if best <= now {\n        return 0;\n    }\n    return best - now;", "    if best < now {\n        return 0;\n    }\n    return best - now;")
m(C, "next_deadline: the latest timer", "        if at >= 0 && (best < 0 || at < best) {\n            best = at;\n        }", "        if at >= 0 && (best < 0 || at > best) {\n            best = at;\n        }")
m(C, "next_deadline: the time to the timer is wrong", "    return best - now;\n}", "    return best;\n}")
m(C, "idle: busy connections counted", "pub fn idle[&r](c: &r Client) -> [] int {\n    var n = 0;\n    var k = 0;\n    while k < c.nslots {\n        if get(c, k, sl.f_phase()) == sl.ph_idle() {", "pub fn idle[&r](c: &r Client) -> [] int {\n    var n = 0;\n    var k = 0;\n    while k < c.nslots {\n        if get(c, k, sl.f_phase()) == sl.ph_active() {")
m(C, "active: dials not counted", "        if phase == sl.ph_connect() || phase == sl.ph_active() || phase == sl.ph_closing() {", "        if phase == sl.ph_active() || phase == sl.ph_closing() {")
m(C, "active: closing connections not counted", "        if phase == sl.ph_connect() || phase == sl.ph_active() || phase == sl.ph_closing() {", "        if phase == sl.ph_connect() || phase == sl.ph_active() {")
m(C, "free_slots: closing slots counted", "pub fn free_slots[&r](c: &r Client) -> [] int {\n    var n = 0;\n    var k = 0;\n    while k < c.nslots {\n        if get(c, k, sl.f_phase()) == sl.ph_free() {", "pub fn free_slots[&r](c: &r Client) -> [] int {\n    var n = 0;\n    var k = 0;\n    while k < c.nslots {\n        if get(c, k, sl.f_phase()) != sl.ph_active() {")
m(C, "ticket_slot: the generation is the slot", "    return t % span();\n}\n\n// The slot's input buffer.", "    return t / span();\n}\n\n// The slot's input buffer.")


def run(exe, texts, name):
    """`cancho test` on the (mutated) sources. A run that hangs past LIMIT seconds is killed, with the test program it started
    (a mutant that makes a loop endless is caught by the tests not finishing), and answers `Ran(4, ..)`."""
    with tempfile.TemporaryDirectory() as d:
        paths = []
        for key, fname in ((W, "wire.cho"), (C, "client.cho")):
            path = os.path.join(d, fname)
            with open(path, "w") as f:
                f.write(texts[key])
            paths.append(path)
        p = subprocess.Popen([exe, "test", "--std", SLOT] + paths + TESTS, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                             start_new_session=True)
        try:
            out, err = p.communicate(timeout=LIMIT)
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.communicate()
            return Ran(4, "FAILED: hung past the limit", "")
    return Ran(p.returncode, out, err)


class Ran:
    def __init__(self, returncode, stdout, stderr):
        self.returncode, self.stdout, self.stderr = returncode, stdout, stderr


def judge(job):
    exe, file, name, old, new, texts = job
    source = texts[file]
    if source.count(old) != 1:
        return name, "BAD", f"`old` occurs {source.count(old)} times in {file}"
    mutated = dict(texts)
    mutated[file] = source.replace(old, new)
    r = run(exe, mutated, name)
    if r.returncode == 4 or "FAILED" in r.stdout:
        return name, "killed", ""
    if r.returncode == 0:
        return name, "SURVIVED", ""
    return name, "NOBUILD", (r.stderr or r.stdout)[:400]


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
    texts = {W: open(WIRE).read(), C: open(CLIENT).read()}
    base = run(exe, texts, "unmutated")
    if base.returncode != 0:
        print("the unmutated source does not pass:\n" + base.stdout + base.stderr)
        return 1
    names = [m[1] for m in MUTANTS]
    if len(set(names)) != len(names):
        dup = sorted({n for n in names if names.count(n) > 1})
        print("duplicate mutant names:", dup)
        return 1
    chosen = [m for m in MUTANTS if only is None or only in m[1]]
    results = []
    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        for name, verdict, detail in pool.map(judge, [(exe, f, n, o, w, texts) for f, n, o, w in chosen]):
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
