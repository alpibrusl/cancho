module std.http;

import std.buffer;
import std.bytes;

// `std.http` — one HTTP/1.x request read from bytes, and the head of a
// response written to a buffer. No sockets and no allocation of its own:
// it is a parser over a slice and a writer over a `Buffer`, so it works
// the same on a request that arrived over a socket, was read from a file,
// or was built by a test.
//
// `docs/http.md` is the design. The posture is the one `std.json` took:
// **strict, and a refusal says where and why.** A request that is
// ambiguous about where its body ends is how request smuggling works, so
// this refuses it rather than guessing: obsolete line folding, whitespace
// before a colon, a `Content-Length` that is not plain digits or appears
// twice with different values, a `Transfer-Encoding` it cannot honour, or
// one that arrives *with* a `Content-Length`. It does not try to be
// liberal in what it accepts.
//
// The result is a small table of integers the caller provides, the way
// `std.json` fills a tape: offsets into the source, nothing copied. See
// `slots` for how big it must be and the accessors below for reading it.

// ---------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------

// `parse` answers the offset where the body starts (always positive), or
// `0 - (position * 16 + code)`.
fn err_incomplete() -> [] int {
    return 1;
}

fn err_request_line() -> [] int {
    return 2;
}

fn err_method() -> [] int {
    return 3;
}

fn err_target() -> [] int {
    return 4;
}

fn err_version() -> [] int {
    return 5;
}

fn err_header() -> [] int {
    return 6;
}

fn err_too_many() -> [] int {
    return 7;
}

fn err_length() -> [] int {
    return 8;
}

fn err_fold() -> [] int {
    return 9;
}

fn err_too_large() -> [] int {
    return 10;
}

fn err_encoding() -> [] int {
    return 11;
}

fn err_host() -> [] int {
    return 12;
}

// Where in the input the problem is, for a log line.
pub fn error_position(result: int) -> [] int {
    if result >= 0 {
        return 0;
    }
    return (0 - result) / 16;
}

// Which problem, `1` to `12`; `0` for a successful result.
pub fn error_code(result: int) -> [] int {
    if result >= 0 {
        return 0;
    }
    return (0 - result) % 16;
}

// `true` for the one refusal that is not a refusal: the head has not all
// arrived. A server reads more and calls `parse` again.
pub fn is_incomplete(result: int) -> [] bool {
    return result < 0 && error_code(result) == 1;
}

// The one-line reason, for a `400` body or a log.
pub fn error_message(code: int) -> [] &static [byte] {
    if code == 1 {
        return "request head not complete";
    }
    if code == 2 {
        return "malformed request line";
    }
    if code == 3 {
        return "invalid method";
    }
    if code == 4 {
        return "invalid request target";
    }
    if code == 5 {
        return "unsupported HTTP version";
    }
    if code == 6 {
        return "malformed header";
    }
    if code == 7 {
        return "too many headers";
    }
    if code == 8 {
        return "invalid Content-Length";
    }
    if code == 9 {
        return "obsolete line folding is refused";
    }
    if code == 10 {
        return "request head too large";
    }
    if code == 11 {
        return "unsupported Transfer-Encoding";
    }
    if code == 12 {
        return "missing or repeated Host";
    }
    return "no error";
}

// ---------------------------------------------------------------------
// The table
// ---------------------------------------------------------------------

// The largest request head accepted: past this with no blank line, the
// answer is "too large" rather than "incomplete", so a peer cannot make a
// server buffer without bound.
pub fn max_head() -> [] int {
    return 65536;
}

// Integers the table needs for up to `max_headers` headers.
//
//     0  method ends here (it starts at 0)
//     1  target starts      2  target ends      3  path ends
//     4  version, 10 or 11
//     5  header count
//     6  where the body starts
//     7  Content-Length, or -1 if there is none
//     8  flags: 1 chunked, 2 keep-alive
//     16 + 4*i  header i: name start, name end, value start, value end
pub fn slots(max_headers: int) -> [] int {
    return 16 + 4 * max_headers;
}

// ---------------------------------------------------------------------
// Characters
// ---------------------------------------------------------------------

// What each byte may be, one table lookup instead of a chain of
// comparisons: bit 1 a token character (RFC 9110 §5.6.2: letters, digits
// and `!#$%&'*+-.^_`|~`), bit 2 a header-value character (tab, space,
// visible ASCII, and the high bytes §5.5 calls opaque -- not the other
// controls, and not the carriage return and line feed that end the line),
// bit 4 a request-target character (visible ASCII only; anything else
// must have been percent-encoded). Built when the program is compiled.
static classes: [int] {
    let t = alloc_slice[static](256, 0);
    var c = 0;
    while c < 256 {
        var v = 0;
        let letter = c >= 65 && c <= 90 || c >= 97 && c <= 122;
        let digit = c >= 48 && c <= 57;
        if letter || digit || c == 33 || c == 35 || c == 36 || c == 37 || c == 38 || c == 39 || c == 42 || c == 43 || c == 45 || c == 46 || c == 94 || c == 95 || c == 96 || c == 124 || c == 126 {
            v = v + 1;
        }
        if c == 9 || c >= 32 && c != 127 {
            v = v + 2;
        }
        if c > 32 && c < 127 {
            v = v + 4;
        }
        t[c] = v;
        c = c + 1;
    }
    return t;
}

fn is_tchar(c: int) -> [] bool {
    return classes[c] & 1 != 0;
}

fn is_value_char(c: int) -> [] bool {
    return classes[c] & 2 != 0;
}

fn is_ows(c: int) -> [] bool {
    return c == 32 || c == 9;
}

fn is_target_char(c: int) -> [] bool {
    return classes[c] & 4 != 0;
}

// Does `src[start..end]` equal `lit` ignoring ASCII case?
fn eq_lower[&s, &l](src: &s [byte], start: int, end: int, lit: &l [byte]) -> [] bool {
    if end - start != len(lit) {
        return false;
    }
    var i = 0;
    while i < len(lit) {
        if bytes.to_lower(int_of(src[start + i])) != int_of(lit[i]) {
            return false;
        }
        i = i + 1;
    }
    return true;
}

// ---------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------

fn fail(position: int, code: int) -> [] int {
    return 0 - (position * 16 + code);
}

// Whether `src[start..end]`, a `Connection` value, lists `token` among its
// comma-separated tokens.
fn lists_token[&s, &l](src: &s [byte], start: int, end: int, token: &l [byte]) -> [] bool {
    var at = start;
    while at < end {
        var stop = at;
        while stop < end && int_of(src[stop]) != 44 {
            stop = stop + 1;
        }
        var from = at;
        while from < stop && is_ows(int_of(src[from])) {
            from = from + 1;
        }
        var to = stop;
        while to > from && is_ows(int_of(src[to - 1])) {
            to = to - 1;
        }
        if eq_lower(src, from, to, token) {
            return true;
        }
        at = stop + 1;
    }
    return false;
}

// Digits only, one to fifteen of them. Fifteen keeps the product below
// 2^50 so it cannot overflow, and no request body is a petabyte.
fn digits_value[&s](src: &s [byte], start: int, end: int) -> [] int {
    if end <= start || end - start > 15 {
        return 0 - 1;
    }
    var value = 0;
    var i = start;
    while i < end {
        let c = int_of(src[i]);
        if !bytes.is_digit(c) {
            return 0 - 1;
        }
        value = value * 10 + (c - 48);
        i = i + 1;
    }
    return value;
}

// Where the blank line that ends the head starts (the `\r` of the final
// `\r\n\r\n`) within the first `limit` bytes, or -1. Looks hard only at a
// line feed, which is one byte in thirty of a real head, where a general
// substring search compares at every position.
fn head_end[&s](src: &s [byte], limit: int) -> [] int {
    var i = 3;
    while i < limit {
        if int_of(src[i]) == 10 && int_of(src[i - 1]) == 13 && int_of(src[i - 2]) == 10 && int_of(src[i - 3]) == 13 {
            return i - 3;
        }
        i = i + 1;
    }
    return 0 - 1;
}

// Read one request head from `src` into `table`.
//
// `src` may hold more than the head -- the start of the body, or the next
// request of a pipelined connection -- and `parse` does not look past the
// blank line. On success the answer is the offset where the body starts.
// `table` must be at least `slots(n)` long for the `n` headers a caller is
// willing to accept; more than that is refused (`error_code` 7).
//
// Strictness is the point (the module comment). Every refusal says where.
pub fn parse[&s, &t](src: &s [byte], table: &!t [int]) -> [] int {
    let n = len(src);
    var limit = n;
    if limit > max_head() {
        limit = max_head();
    }
    let blank = head_end(src, limit);
    if blank < 0 {
        if n >= max_head() {
            return fail(max_head(), err_too_large());
        }
        return fail(n, err_incomplete());
    }
    // Every line, the last header's included, ends before this.
    let lines_end = blank + 2;
    let body_start = blank + 4;
    let capacity = (len(table) - 16) / 4;

    // Request line: method SP target SP version CRLF.
    var at = 0;
    while at < lines_end && is_tchar(int_of(src[at])) {
        at = at + 1;
    }
    if at == 0 {
        return fail(0, err_method());
    }
    if int_of(src[at]) != 32 {
        return fail(at, err_request_line());
    }
    table[0] = at;
    at = at + 1;

    table[1] = at;
    var path_end = 0 - 1;
    while at < lines_end && is_target_char(int_of(src[at])) {
        // A fragment is never sent to a server; one here is a client bug
        // or an attempt to say two different things to two parsers.
        if int_of(src[at]) == 35 {
            return fail(at, err_target());
        }
        if path_end < 0 && int_of(src[at]) == 63 {
            path_end = at;
        }
        at = at + 1;
    }
    if at == table[1] {
        return fail(at, err_target());
    }
    if int_of(src[at]) != 32 {
        return fail(at, err_request_line());
    }
    table[2] = at;
    if path_end < 0 {
        path_end = at;
    }
    table[3] = path_end;
    at = at + 1;

    if at + 10 > lines_end || !bytes.starts_with(src[at..at + 7], "HTTP/1.") {
        return fail(at, err_version());
    }
    let minor = int_of(src[at + 7]);
    if minor == 49 {
        table[4] = 11;
    } else if minor == 48 {
        table[4] = 10;
    } else {
        return fail(at + 7, err_version());
    }
    at = at + 8;
    if int_of(src[at]) != 13 || int_of(src[at + 1]) != 10 {
        return fail(at, err_request_line());
    }
    at = at + 2;

    // Headers: name ":" OWS value OWS CRLF.
    var count = 0;
    while at < lines_end {
        if is_ows(int_of(src[at])) {
            return fail(at, err_fold());
        }
        let name_start = at;
        while at < lines_end && is_tchar(int_of(src[at])) {
            at = at + 1;
        }
        if at == name_start || at >= lines_end || int_of(src[at]) != 58 {
            return fail(at, err_header());
        }
        let name_end = at;
        at = at + 1;
        while at < lines_end && is_ows(int_of(src[at])) {
            at = at + 1;
        }
        let value_start = at;
        while at < lines_end && is_value_char(int_of(src[at])) {
            at = at + 1;
        }
        if at + 1 >= lines_end {
            return fail(at, err_header());
        }
        if int_of(src[at]) != 13 || int_of(src[at + 1]) != 10 {
            return fail(at, err_header());
        }
        var value_end = at;
        while value_end > value_start && is_ows(int_of(src[value_end - 1])) {
            value_end = value_end - 1;
        }
        if count >= capacity {
            return fail(name_start, err_too_many());
        }
        table[16 + 4 * count] = name_start;
        table[16 + 4 * count + 1] = name_end;
        table[16 + 4 * count + 2] = value_start;
        table[16 + 4 * count + 3] = value_end;
        count = count + 1;
        at = at + 2;
    }
    table[5] = count;
    table[6] = body_start;

    // What the headers say about the body and the connection.
    var length = 0 - 1;
    var chunked = false;
    var hosts = 0;
    var closing = false;
    var keeping = false;
    var i = 0;
    while i < count {
        let ns = table[16 + 4 * i];
        let ne = table[16 + 4 * i + 1];
        let vs = table[16 + 4 * i + 2];
        let ve = table[16 + 4 * i + 3];
        if eq_lower(src, ns, ne, "content-length") {
            let v = digits_value(src, vs, ve);
            if v < 0 {
                return fail(vs, err_length());
            }
            if length >= 0 && length != v {
                return fail(vs, err_length());
            }
            length = v;
        } else if eq_lower(src, ns, ne, "transfer-encoding") {
            if !eq_lower(src, vs, ve, "chunked") {
                return fail(vs, err_encoding());
            }
            chunked = true;
        } else if eq_lower(src, ns, ne, "host") {
            hosts = hosts + 1;
        } else if eq_lower(src, ns, ne, "connection") {
            if lists_token(src, vs, ve, "close") {
                closing = true;
            }
            if lists_token(src, vs, ve, "keep-alive") {
                keeping = true;
            }
        }
        i = i + 1;
    }
    if chunked && length >= 0 {
        return fail(0, err_encoding());
    }
    if table[4] == 11 && hosts != 1 {
        return fail(0, err_host());
    }
    table[7] = length;
    var flags = 0;
    if chunked {
        flags = flags + 1;
    }
    if table[4] == 11 && !closing || table[4] == 10 && keeping {
        flags = flags + 2;
    }
    table[8] = flags;
    return body_start;
}

// ---------------------------------------------------------------------
// Reading the table
// ---------------------------------------------------------------------

pub fn method[&s, &t](src: &s [byte], table: &t [int]) -> [] &s [byte] {
    return src[0..table[0]];
}

// The whole request target, `/a/b?c=d` as sent.
pub fn target[&s, &t](src: &s [byte], table: &t [int]) -> [] &s [byte] {
    return src[table[1]..table[2]];
}

// The part before any `?`, still percent-encoded.
pub fn path[&s, &t](src: &s [byte], table: &t [int]) -> [] &s [byte] {
    return src[table[1]..table[3]];
}

// The part after the `?`, or empty.
pub fn query[&s, &t](src: &s [byte], table: &t [int]) -> [] &s [byte] {
    if table[3] >= table[2] {
        return src[table[2]..table[2]];
    }
    return src[table[3] + 1..table[2]];
}

// 10 or 11.
pub fn version[&t](table: &t [int]) -> [] int {
    return table[4];
}

pub fn header_count[&t](table: &t [int]) -> [] int {
    return table[5];
}

// Where the body starts in the source `parse` was given.
pub fn body_start[&t](table: &t [int]) -> [] int {
    return table[6];
}

// The declared body length, or -1 when there is none or it is chunked.
pub fn content_length[&t](table: &t [int]) -> [] int {
    return table[7];
}

pub fn is_chunked[&t](table: &t [int]) -> [] bool {
    return table[8] % 2 == 1;
}

// Whether the connection stays open after this request: HTTP/1.1 unless it
// said `close`, HTTP/1.0 only if it said `keep-alive`.
pub fn keeps_alive[&t](table: &t [int]) -> [] bool {
    return table[8] / 2 % 2 == 1;
}

pub fn header_name[&s, &t](src: &s [byte], table: &t [int], i: int) -> [] &s [byte] {
    return src[table[16 + 4 * i]..table[16 + 4 * i + 1]];
}

pub fn header_value[&s, &t](src: &s [byte], table: &t [int], i: int) -> [] &s [byte] {
    return src[table[16 + 4 * i + 2]..table[16 + 4 * i + 3]];
}

// The index of the first header named `name` (compared ignoring ASCII
// case; `name` is written lowercase), or -1.
pub fn find_header[&s, &t, &n](src: &s [byte], table: &t [int], name: &n [byte]) -> [] int {
    var i = 0;
    while i < table[5] {
        if eq_lower(src, table[16 + 4 * i], table[16 + 4 * i + 1], name) {
            return i;
        }
        i = i + 1;
    }
    return 0 - 1;
}

// The value of the first header named `name`, or an empty slice. A header
// that is present and empty and one that is absent look the same here;
// `find_header` tells them apart.
pub fn header[&s, &t, &n](src: &s [byte], table: &t [int], name: &n [byte]) -> [] &s [byte] {
    let i = find_header(src, table, name);
    if i < 0 {
        return src[0..0];
    }
    return header_value(src, table, i);
}

// ---------------------------------------------------------------------
// Percent-encoding and the query string
// ---------------------------------------------------------------------

fn hex_value(c: int) -> [] int {
    if c >= 48 && c <= 57 {
        return c - 48;
    }
    if c >= 97 && c <= 102 {
        return c - 87;
    }
    if c >= 65 && c <= 70 {
        return c - 55;
    }
    return 0 - 1;
}

// Decode `%XX` escapes of `text` into `out`, answering how many bytes were
// written, or -1 for a malformed escape or no room. `plus_is_space` is for
// a query string, where `+` means a space; a path must not set it.
//
// A decoded `%00` is written like any other byte: it is the caller's to
// refuse, and a router that matches on it as a path segment simply will
// not find a route.
pub fn percent_decode[&t, &o](text: &t [byte], out: &!o [byte], plus_is_space: bool) -> [] int {
    var at = 0;
    var written = 0;
    while at < len(text) {
        if written >= len(out) {
            return 0 - 1;
        }
        let c = int_of(text[at]);
        if c == 37 {
            if at + 2 >= len(text) {
                return 0 - 1;
            }
            let hi = hex_value(int_of(text[at + 1]));
            let lo = hex_value(int_of(text[at + 2]));
            if hi < 0 || lo < 0 {
                return 0 - 1;
            }
            out[written] = byte_of(hi * 16 + lo);
            at = at + 3;
        } else if c == 43 && plus_is_space {
            out[written] = byte_of(32);
            at = at + 1;
        } else {
            out[written] = text[at];
            at = at + 1;
        }
        written = written + 1;
    }
    return written;
}

// The raw (still encoded) value of query parameter `key`, as `(start,
// end)` in `query`, or `(-1, -1)`. The first of repeated keys wins. A key
// with no `=` has an empty value. `key` is compared as written, so an
// encoded key in the query matches only its encoded spelling.
pub fn query_value[&q, &k](query: &q [byte], key: &k [byte]) -> [] (int, int) {
    var at = 0;
    while at <= len(query) {
        var stop = at;
        while stop < len(query) && int_of(query[stop]) != 38 {
            stop = stop + 1;
        }
        var eq = at;
        while eq < stop && int_of(query[eq]) != 61 {
            eq = eq + 1;
        }
        if bytes.equal(query[at..eq], key) {
            if eq < stop {
                return (eq + 1, stop);
            }
            return (stop, stop);
        }
        at = stop + 1;
    }
    return (0 - 1, 0 - 1);
}

// ---------------------------------------------------------------------
// Writing a response head
// ---------------------------------------------------------------------

// The standard reason phrase for the codes a server commonly sends.
pub fn reason(status: int) -> [] &static [byte] {
    if status == 200 {
        return "OK";
    }
    if status == 201 {
        return "Created";
    }
    if status == 204 {
        return "No Content";
    }
    if status == 301 {
        return "Moved Permanently";
    }
    if status == 302 {
        return "Found";
    }
    if status == 304 {
        return "Not Modified";
    }
    if status == 400 {
        return "Bad Request";
    }
    if status == 401 {
        return "Unauthorized";
    }
    if status == 403 {
        return "Forbidden";
    }
    if status == 404 {
        return "Not Found";
    }
    if status == 405 {
        return "Method Not Allowed";
    }
    if status == 408 {
        return "Request Timeout";
    }
    if status == 413 {
        return "Content Too Large";
    }
    if status == 414 {
        return "URI Too Long";
    }
    if status == 415 {
        return "Unsupported Media Type";
    }
    if status == 422 {
        return "Unprocessable Content";
    }
    if status == 429 {
        return "Too Many Requests";
    }
    if status == 500 {
        return "Internal Server Error";
    }
    if status == 501 {
        return "Not Implemented";
    }
    if status == 503 {
        return "Service Unavailable";
    }
    return "Unknown";
}

// ---------------------------------------------------------------------
// A chunked request body
// ---------------------------------------------------------------------

// Decode the chunked body that starts at `src[0]` -- just past the head, where
// `parse` said the body starts -- into `out`.
//
// Answers `(consumed, decoded)`: how many bytes of `src` the whole body took,
// the terminating chunk and its blank line included, and how many bytes of
// `out` it filled. `consumed` is negative when there is no body to return:
//
//     -1  not all of it has arrived (decode again when more has)
//     -2  a chunk size that is not 1-8 hex digits
//     -3  framing that is not exactly CRLF where CRLF belongs (a bare LF, a
//         chunk that is not followed by one, a size line with something after
//         its digits)
//     -4  the decoded body would not fit in `out`
//     -5  a chunk extension (`;...`) or trailer fields, which this refuses
//
// **Strict on purpose.** Chunk extensions and trailers are where request
// smuggling lives, nothing here needs them, and refusing is one line where
// interpreting them correctly is a page. A decoded byte is copied once, from
// `src` to `out`; `src` is not modified. When more bytes arrive the decode
// starts again from the first chunk -- cost proportional to the body, which is
// bounded by `out`.
pub fn dechunk[&s, &o](src: &s [byte], out: &!o [byte]) -> [] (int, int) {
    var at = 0;
    var wrote = 0;
    while true {
        var size = 0;
        var digits = 0;
        while at < len(src) && hex_value(int_of(src[at])) >= 0 {
            if digits == 8 {
                return (0 - 2, 0);
            }
            size = size * 16 + hex_value(int_of(src[at]));
            digits = digits + 1;
            at = at + 1;
        }
        if at >= len(src) {
            return (0 - 1, 0);
        }
        if digits == 0 {
            return (0 - 2, 0);
        }
        if int_of(src[at]) == 59 {
            return (0 - 5, 0);
        }
        if int_of(src[at]) != 13 {
            return (0 - 3, 0);
        }
        if at + 1 >= len(src) {
            return (0 - 1, 0);
        }
        if int_of(src[at + 1]) != 10 {
            return (0 - 3, 0);
        }
        at = at + 2;
        if size == 0 {
            // The trailer section must be empty: straight to the blank line.
            if at >= len(src) {
                return (0 - 1, 0);
            }
            if int_of(src[at]) != 13 {
                return (0 - 5, 0);
            }
            if at + 1 >= len(src) {
                return (0 - 1, 0);
            }
            if int_of(src[at + 1]) != 10 {
                return (0 - 3, 0);
            }
            return (at + 2, wrote);
        }
        if wrote + size > len(out) {
            return (0 - 4, 0);
        }
        if at + size + 2 > len(src) {
            return (0 - 1, 0);
        }
        var i = 0;
        while i < size {
            out[wrote + i] = src[at + i];
            i = i + 1;
        }
        at = at + size;
        if int_of(src[at]) != 13 || int_of(src[at + 1]) != 10 {
            return (0 - 3, 0);
        }
        at = at + 2;
        wrote = wrote + size;
    }
    return (0 - 1, 0);
}

// Whether `dechunk` is only waiting for more bytes.
pub fn dechunk_incomplete(consumed: int) -> [] bool {
    return consumed == 0 - 1;
}

// What a refusal from `dechunk` means, for a response body.
pub fn dechunk_message(consumed: int) -> [] &static [byte] {
    if consumed == 0 - 2 {
        return "bad chunk size";
    }
    if consumed == 0 - 3 {
        return "bad chunk framing";
    }
    if consumed == 0 - 4 {
        return "request too large";
    }
    if consumed == 0 - 5 {
        return "chunk extensions and trailers are not supported";
    }
    return "bad chunked body";
}

// Whether `extra` is zero or more complete header lines -- `name: value`
// and a CRLF each -- and nothing else: a name of token characters, a value of
// the characters a value may hold, and no bare CR or LF anywhere. It is what
// `respond_head_with` insists on before it lets a caller's text into a
// response head.
fn valid_extra[&e](extra: &e [byte]) -> [] bool {
    var i = 0;
    while i < len(extra) {
        let start = i;
        while i < len(extra) && is_tchar(int_of(extra[i])) {
            i = i + 1;
        }
        if i == start || i >= len(extra) || int_of(extra[i]) != 58 {
            return false;
        }
        i = i + 1;
        while i < len(extra) && (is_value_char(int_of(extra[i])) || is_ows(int_of(extra[i]))) {
            i = i + 1;
        }
        if i + 1 >= len(extra) || int_of(extra[i]) != 13 || int_of(extra[i + 1]) != 10 {
            return false;
        }
        i = i + 2;
    }
    return true;
}

// `respond_head`, with more header lines: `extra` is a block of complete
// lines -- `Allow: GET, POST\r\n` -- written between `Connection` and the
// blank line. It is the one place a response head takes text from a caller
// that is not a content type, so it is checked as strictly: **anything but
// whole `name: value` lines traps**, for the reason a header injection does.
// `Content-Type`, `Content-Length` and `Connection` are this function's own;
// naming one of them again in `extra` is the caller's bug and is not
// detected (two of them is a malformed response, which a client is entitled
// to refuse).
pub fn respond_head_with[&h, &c, &e](heap: &!h Heap, out: buffer.Buffer, status: int, content_type: &c [byte], length: int, keep_alive: bool, extra: &e [byte]) -> [heap] buffer.Buffer {
    if status < 100 || status > 999 || length < 0 || !valid_extra(extra) {
        trap();
    }
    var i = 0;
    while i < len(content_type) {
        let c = int_of(content_type[i]);
        if c == 13 || c == 10 || c == 0 {
            trap();
        }
        i = i + 1;
    }
    var b = buffer.append(heap, out, "HTTP/1.1 ");
    b = buffer.push_nat(heap, b, status);
    b = buffer.push(heap, b, byte_of(32));
    b = buffer.append(heap, b, reason(status));
    b = buffer.append(heap, b, "\r\nContent-Type: ");
    b = buffer.append(heap, b, content_type);
    b = buffer.append(heap, b, "\r\nContent-Length: ");
    b = buffer.push_nat(heap, b, length);
    if keep_alive {
        b = buffer.append(heap, b, "\r\nConnection: keep-alive\r\n");
    } else {
        b = buffer.append(heap, b, "\r\nConnection: close\r\n");
    }
    b = buffer.append(heap, b, extra);
    return buffer.append(heap, b, "\r\n");
}

// Append `HTTP/1.1 <status> <reason>`, `Content-Type`, `Content-Length`,
// `Connection` and the blank line to `out`. The body is the caller's to
// append after it, and its length is `length` -- this module never sees
// the body, so it cannot check that they agree.
//
// Traps if `content_type` holds a carriage return or line feed, or if
// `status` is not three digits: both are a program writing a bug into its
// own output, and a header injection is the worse one to find out about
// from a client.
pub fn respond_head[&h, &c](heap: &!h Heap, out: buffer.Buffer, status: int, content_type: &c [byte], length: int, keep_alive: bool) -> [heap] buffer.Buffer {
    return respond_head_with(heap, out, status, content_type, length, keep_alive, "");
}
