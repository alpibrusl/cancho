import std.buffer;
import std.bytes;
import std.http;
import std.test;

fn test_a_get_request[&h](heap: &!h Heap) -> [heap] int {
    let src = "GET /users/42?x=1&y=two HTTP/1.1\r\nHost: example.com\r\nAccept: */*\r\n\r\n";
    let table = box_slice(heap, http.slots(16), 0);
    borrow mut table as &!w in {
        let t = contents(w);
        let r = http.parse(src, t);
        test.assert(r > 0);
        test.assert_eq(r, len(src));
        test.assert_eq(len(http.method(src, t)), 3);
        test.assert(http.method(src, t)[0] == byte_of('G'));
        test.assert_eq(len(http.path(src, t)), 9);
        test.assert_eq(len(http.query(src, t)), 9);
        test.assert_eq(len(http.target(src, t)), 19);
        test.assert_eq(http.version(t), 11);
        test.assert_eq(http.header_count(t), 2);
        test.assert_eq(http.content_length(t), 0 - 1);
        test.assert(http.keeps_alive(t));
        test.assert(!http.is_chunked(t));
        test.assert_eq(len(http.header(src, t, "host")), 11);
        test.assert_eq(len(http.header(src, t, "accept")), 3);
        test.assert_eq(http.find_header(src, t, "x-nope"), 0 - 1);
    }
    unbox_slice(heap, table);
    return 0;
}

// `parse` on `src` with room for 8 headers, answering the raw result.
fn parsed[&h, &s](heap: &!h Heap, src: &s [byte]) -> [heap] int {
    let table = box_slice(heap, http.slots(8), 0);
    var r = 0;
    borrow mut table as &!w in {
        r = http.parse(src, contents(w));
    }
    unbox_slice(heap, table);
    return r;
}

fn refused[&h, &s](heap: &!h Heap, src: &s [byte], code: int, position: int) -> [heap] int {
    let r = parsed(heap, src);
    test.assert(r < 0);
    test.assert_eq(http.error_code(r), code);
    test.assert_eq(http.error_position(r), position);
    return 0;
}

fn test_every_refusal_has_its_code_and_position[&h](heap: &!h Heap) -> [heap] int {
    // 1: not all here yet -- including at exactly the head's end.
    refused(heap, "GET / HTTP/1.1\r\nHost: x\r\n", 1, 25);
    refused(heap, "", 1, 0);
    // 3: method.
    refused(heap, "\r\n\r\n", 3, 0);
    refused(heap, " / HTTP/1.1\r\n\r\n", 3, 0);
    // 2: request line.
    refused(heap, "G@T / HTTP/1.1\r\nHost: x\r\n\r\n", 2, 1);
    refused(heap, "GET\r\n\r\n", 2, 3);
    // 4: target.
    refused(heap, "GET  HTTP/1.1\r\nHost: x\r\n\r\n", 4, 4);
    refused(heap, "GET /a#b HTTP/1.1\r\nHost: x\r\n\r\n", 4, 6);
    // 5: version.
    refused(heap, "GET / HTTP/2.0\r\nHost: x\r\n\r\n", 5, 6);
    refused(heap, "GET / HTTP/1.2\r\nHost: x\r\n\r\n", 5, 13);
    refused(heap, "GET / HTTP/1\r\nHost: x\r\n\r\n", 5, 6);
    // 2 again: the version must be followed by CRLF, nothing else.
    refused(heap, "GET / HTTP/1.1 \r\nHost: x\r\n\r\n", 2, 14);
    // 6: headers -- no colon, whitespace before it, an empty name, a bare
    // line feed or NUL in a value.
    refused(heap, "GET / HTTP/1.1\r\nHost x\r\n\r\n", 6, 20);
    refused(heap, "GET / HTTP/1.1\r\nHost : x\r\n\r\n", 6, 20);
    refused(heap, "GET / HTTP/1.1\r\n: x\r\nHost: y\r\n\r\n", 6, 16);
    refused(heap, "GET / HTTP/1.1\r\nHost: x\ny\r\n\r\n", 6, 23);
    refused(heap, "GET / HTTP/1.1\r\nHost: x\0y\r\n\r\n", 6, 23);
    // 9: obsolete line folding, the classic smuggling vector.
    refused(heap, "GET / HTTP/1.1\r\nHost: x\r\n y\r\n\r\n", 9, 25);
    refused(heap, "GET / HTTP/1.1\r\nHost: x\r\n\ty\r\n\r\n", 9, 25);
    // 8: Content-Length -- not digits, signed, empty, too long, repeated
    // with different values.
    refused(heap, "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: abc\r\n\r\n", 8, 42);
    refused(heap, "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: +5\r\n\r\n", 8, 42);
    refused(heap, "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 5, 5\r\n\r\n", 8, 42);
    refused(heap, "POST / HTTP/1.1\r\nHost: x\r\nContent-Length:\r\n\r\n", 8, 41);
    refused(heap, "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 1234567890123456\r\n\r\n", 8, 42);
    refused(heap, "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\nContent-Length: 6\r\n\r\n", 8, 61);
    // 11: Transfer-Encoding we cannot honour, or together with a length.
    refused(heap, "POST / HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: gzip\r\n\r\n", 11, 45);
    refused(heap, "POST / HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\nContent-Length: 4\r\n\r\n", 11, 0);
    // 12: Host.
    refused(heap, "GET / HTTP/1.1\r\n\r\n", 12, 0);
    refused(heap, "GET / HTTP/1.1\r\nHost: a\r\nHost: b\r\n\r\n", 12, 0);
    return 0;
}

fn test_two_content_lengths_that_agree_are_fine[&h](heap: &!h Heap) -> [heap] int {
    let src = "POST / HTTP/1.1\r\nHost: x\r\ncontent-length: 5\r\nCONTENT-LENGTH: 5\r\n\r\nhello";
    let table = box_slice(heap, http.slots(8), 0);
    borrow mut table as &!w in {
        let t = contents(w);
        let r = http.parse(src, t);
        test.assert_eq(r, len(src) - 5);
        test.assert_eq(http.content_length(t), 5);
        test.assert_eq(http.body_start(t), r);
    }
    unbox_slice(heap, table);
    return 0;
}

fn test_chunked_and_the_connection_rules[&h](heap: &!h Heap) -> [heap] int {
    let table = box_slice(heap, http.slots(8), 0);
    borrow mut table as &!w in {
        let t = contents(w);
        let chunked = "POST / HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: Chunked\r\n\r\n";
        test.assert(http.parse(chunked, t) > 0);
        test.assert(http.is_chunked(t));
        test.assert_eq(http.content_length(t), 0 - 1);

        // HTTP/1.1 keeps the connection unless it says close.
        test.assert(http.parse("GET / HTTP/1.1\r\nHost: x\r\n\r\n", t) > 0);
        test.assert(http.keeps_alive(t));
        test.assert(http.parse("GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n", t) > 0);
        test.assert(!http.keeps_alive(t));
        test.assert(http.parse("GET / HTTP/1.1\r\nHost: x\r\nConnection: Upgrade, CLOSE\r\n\r\n", t) > 0);
        test.assert(!http.keeps_alive(t));
        // HTTP/1.0 closes unless it asks to keep, and needs no Host.
        test.assert(http.parse("GET / HTTP/1.0\r\n\r\n", t) > 0);
        test.assert_eq(http.version(t), 10);
        test.assert(!http.keeps_alive(t));
        test.assert(http.parse("GET / HTTP/1.0\r\nConnection: Keep-Alive\r\n\r\n", t) > 0);
        test.assert(http.keeps_alive(t));
    }
    unbox_slice(heap, table);
    return 0;
}

// Pipelined requests: `parse` reads the first head and answers where the
// next thing starts, and never looks past it.
fn test_parse_stops_at_the_blank_line[&h](heap: &!h Heap) -> [heap] int {
    let src = "GET /a HTTP/1.1\r\nHost: x\r\n\r\nGET /b HTTP/1.1\r\nHost: x\r\n\r\n";
    let table = box_slice(heap, http.slots(8), 0);
    borrow mut table as &!w in {
        let t = contents(w);
        let first = http.parse(src, t);
        test.assert_eq(first, 28);
        test.assert_eq(len(http.path(src, t)), 2);
        let rest = src[first..len(src)];
        let second = http.parse(rest, t);
        test.assert_eq(second, 28);
        test.assert(http.path(rest, t)[1] == byte_of('b'));
    }
    unbox_slice(heap, table);
    return 0;
}

fn test_too_many_headers_and_a_head_that_never_ends[&h](heap: &!h Heap) -> [heap] int {
    // Nine headers into a table sized for eight.
    let nine = "GET / HTTP/1.1\r\nHost: x\r\na: 1\r\nb: 1\r\nc: 1\r\nd: 1\r\ne: 1\r\nf: 1\r\ng: 1\r\nh: 1\r\n\r\n";
    let r = parsed(heap, nine);
    test.assert_eq(http.error_code(r), 7);

    // More than `max_head` bytes with no blank line is "too large", not
    // "incomplete": a peer cannot make a server buffer for ever.
    let big = box_slice(heap, http.max_head() + 10, byte_of('a'));
    borrow mut big as &!w in {
        let s = contents(w);
        s[0] = byte_of('G');
        s[1] = byte_of('E');
        s[2] = byte_of('T');
        s[3] = byte_of(' ');
        let r2 = parsed(heap, s);
        test.assert_eq(http.error_code(r2), 10);
    }
    unbox_slice(heap, big);
    return 0;
}

fn test_percent_decode_and_the_query[&h](heap: &!h Heap) -> [heap] int {
    let out = box_slice(heap, 16, byte_of(0));
    borrow mut out as &!w in {
        let o = contents(w);
        test.assert_eq(http.percent_decode("a%20b%2Fc", o, false), 5);
        test.assert(o[1] == byte_of(' '));
        test.assert(o[3] == byte_of('/'));
        test.assert_eq(http.percent_decode("a+b", o, true), 3);
        test.assert(o[1] == byte_of(' '));
        test.assert_eq(http.percent_decode("a+b", o, false), 3);
        test.assert(o[1] == byte_of('+'));
        test.assert_eq(http.percent_decode("%7e%7E", o, false), 2);
        // Malformed or truncated escapes are refused, not passed through.
        test.assert_eq(http.percent_decode("%zz", o, false), 0 - 1);
        test.assert_eq(http.percent_decode("%2", o, false), 0 - 1);
        test.assert_eq(http.percent_decode("abc%", o, false), 0 - 1);
        // No room.
        test.assert_eq(http.percent_decode("0123456789abcdefg", o, false), 0 - 1);
    }
    unbox_slice(heap, out);

    let q = "a=1&b=two&c&a=3&=x&d=";
    let (a_start, a_end) = http.query_value(q, "a");
    test.assert_eq(a_start, 2);
    test.assert_eq(a_end, 3);
    let (bs, be) = http.query_value(q, "b");
    test.assert_eq(be - bs, 3);
    let (cs, ce) = http.query_value(q, "c");
    test.assert_eq(cs, ce);
    test.assert(cs >= 0);
    let (ds, de) = http.query_value(q, "d");
    test.assert_eq(ds, de);
    let (ms, me) = http.query_value(q, "missing");
    test.assert_eq(ms, 0 - 1);
    test.assert_eq(me, 0 - 1);
    let (es, ee) = http.query_value("", "a");
    test.assert_eq(es, 0 - 1);
    return 0;
}

fn test_the_response_head[&h](heap: &!h Heap) -> [heap] int {
    var out = buffer.empty(heap, 16);
    out = http.respond_head(heap, out, 404, "text/plain", 9, false);
    borrow out as &b in {
        let want = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: 9\r\nConnection: close\r\n\r\n";
        test.assert(bytes.equal(buffer.bytes(b), want));
    }
    buffer.drop(heap, out);
    var again = buffer.empty(heap, 16);
    again = http.respond_head(heap, again, 200, "application/json", 0, true);
    borrow again as &b in {
        let want = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n";
        test.assert(bytes.equal(buffer.bytes(b), want));
    }
    buffer.drop(heap, again);
    return 0;
}

fn test_extra_header_lines_go_before_the_blank_line[&h](heap: &!h Heap) -> [heap] int {
    var out = buffer.empty(heap, 16);
    out = http.respond_head_with(heap, out, 405, "application/json", 2, true, "Allow: GET, POST\r\nCache-Control: no-store\r\n");
    borrow out as &b in {
        let want = "HTTP/1.1 405 Method Not Allowed\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: keep-alive\r\nAllow: GET, POST\r\nCache-Control: no-store\r\n\r\n";
        test.assert(bytes.equal(buffer.bytes(b), want));
    }
    buffer.drop(heap, out);
    // No extra lines is exactly `respond_head`.
    var plain = buffer.empty(heap, 16);
    plain = http.respond_head_with(heap, plain, 200, "text/plain", 0, false, "");
    var same = buffer.empty(heap, 16);
    same = http.respond_head(heap, same, 200, "text/plain", 0, false);
    borrow plain as &p in {
        borrow same as &s in {
            test.assert(bytes.equal(buffer.bytes(p), buffer.bytes(s)));
        }
    }
    buffer.drop(heap, plain);
    buffer.drop(heap, same);
    return 0;
}

fn test_a_chunked_body_is_decoded[&h](heap: &!h Heap) -> [heap] int {
    let scratch = box_slice(heap, 64, byte_of(0));
    borrow mut scratch as &!sw in {
        let o = contents(sw);
        // Two chunks, a hex size in either case, and the final blank line.
        let body = "5\r\nhello\r\nA\r\n, world!!!\r\n0\r\n\r\n";
        let (used, n) = http.dechunk(body, o);
        test.assert_eq(used, 30);
        test.assert_eq(n, 15);
        test.assert(bytes.equal(o[0..n], "hello, world!!!"));
        // Bytes after the body are not touched and not counted: a pipelined
        // request follows, and `used` says where it starts.
        let (used2, n2) = http.dechunk("3\r\nabc\r\n0\r\n\r\nGET / HTTP/1.1\r\n", o);
        test.assert_eq(used2, 13);
        test.assert_eq(n2, 3);
        // An empty body is just the last chunk.
        let (used3, n3) = http.dechunk("0\r\n\r\n", o);
        test.assert_eq(used3, 5);
        test.assert_eq(n3, 0);
    }
    unbox_slice(heap, scratch);
    return 0;
}

// Every prefix of a good body is "not all here yet" and never an error: that
// is what lets a server call it again after each read.
fn test_a_chunked_body_arriving_in_pieces_is_never_wrongly_refused[&h](heap: &!h Heap) -> [heap] int {
    let scratch = box_slice(heap, 64, byte_of(0));
    borrow mut scratch as &!sw in {
        let o = contents(sw);
        let body = "5\r\nhello\r\nA\r\n, world!!!\r\n0\r\n\r\n";
        var cut = 0;
        while cut < len(body) {
            let (used, n) = http.dechunk(body[0..cut], o);
            test.assert(http.dechunk_incomplete(used));
            cut = cut + 1;
        }
        let (used, n) = http.dechunk(body[0..len(body)], o);
        test.assert_eq(used, len(body));
    }
    unbox_slice(heap, scratch);
    return 0;
}

fn test_every_chunked_refusal_has_its_code[&h](heap: &!h Heap) -> [heap] int {
    let scratch = box_slice(heap, 8, byte_of(0));
    borrow mut scratch as &!sw in {
        let o = contents(sw);
        // -2: a size that is not hex, is empty, or is nine digits.
        let (a, an) = http.dechunk("zz\r\nxx\r\n0\r\n\r\n", o);
        test.assert_eq(a, 0 - 2);
        let (b, bn) = http.dechunk("\r\n0\r\n\r\n", o);
        test.assert_eq(b, 0 - 2);
        let (c, cn) = http.dechunk("000000001\r\nx\r\n0\r\n\r\n", o);
        test.assert_eq(c, 0 - 2);
        // -3: a bare line feed, a chunk not followed by CRLF, junk after the size.
        let (d, dn) = http.dechunk("1\nx\r\n0\r\n\r\n", o);
        test.assert_eq(d, 0 - 3);
        let (e, en) = http.dechunk("1\r\nxyz0\r\n\r\n", o);
        test.assert_eq(e, 0 - 3);
        let (f, fn_) = http.dechunk("1 \r\nx\r\n0\r\n\r\n", o);
        test.assert_eq(f, 0 - 3);
        // -4: the decoded body would not fit (8 bytes of room).
        let (g, gn) = http.dechunk("9\r\n123456789\r\n0\r\n\r\n", o);
        test.assert_eq(g, 0 - 4);
        let (h2, hn) = http.dechunk("4\r\n1234\r\n5\r\n56789\r\n0\r\n\r\n", o);
        test.assert_eq(h2, 0 - 4);
        // -5: an extension, a trailer.
        let (i, in_) = http.dechunk("1;ext=1\r\nx\r\n0\r\n\r\n", o);
        test.assert_eq(i, 0 - 5);
        let (j, jn) = http.dechunk("1\r\nx\r\n0\r\nX-Trailer: 1\r\n\r\n", o);
        test.assert_eq(j, 0 - 5);
        // Every refusal has a message.
        test.assert(len(http.dechunk_message(0 - 2)) > 0);
        test.assert(len(http.dechunk_message(0 - 5)) > 0);
    }
    unbox_slice(heap, scratch);
    return 0;
}

// `204` and `304` have no body, so no `Content-Length` and no `Content-Type`.
fn test_a_response_with_no_body[&h](heap: &!h Heap) -> [heap] int {
    var out = buffer.empty(heap, 16);
    out = http.respond_no_content(heap, out, 204, true, "");
    borrow out as &b in {
        let want = "HTTP/1.1 204 No Content\r\nConnection: keep-alive\r\n\r\n";
        test.assert(bytes.equal(buffer.bytes(b), want));
    }
    buffer.drop(heap, out);
    var cached = buffer.empty(heap, 16);
    cached = http.respond_no_content(heap, cached, 304, false, "ETag: \"v1\"\r\n");
    borrow cached as &b in {
        let want = "HTTP/1.1 304 Not Modified\r\nConnection: close\r\nETag: \"v1\"\r\n\r\n";
        test.assert(bytes.equal(buffer.bytes(b), want));
    }
    buffer.drop(heap, cached);
    return 0;
}
