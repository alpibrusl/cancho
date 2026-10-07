edition 5;

module ws;

import std.bytes;
import std.http;
import sha1;
import b64;

// `ws` -- the part of RFC 6455 an OCPP server needs: the opening handshake, and frames.
//
// Not here, and counted in `docs/websocket-spike.md` section 8: fragmented messages (a charge point sends each OCPP message as
// one frame; a fragment is refused), extensions (`permessage-deflate`), and anything over `max_payload`.

fn guid() -> [] &static [byte] {
    return "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
}

// `Sec-WebSocket-Accept` for `key` (RFC 6455 section 4.2.2): the base64 of the SHA-1 of the key and a fixed GUID. `out` must hold
// 28 bytes; answers 28.
pub fn accept_key[&k, &o](key: &k [byte], out: &!o [byte]) -> [] int {
    region a {
        let joined = alloc_slice[a](len(key) + 36, byte_of(0));
        var i = 0;
        while i < len(key) {
            joined[i] = key[i];
            i = i + 1;
        }
        let g = guid();
        var j = 0;
        while j < 36 {
            joined[len(key) + j] = g[j];
            j = j + 1;
        }
        let h = alloc_slice[a](20, byte_of(0));
        sha1.digest(joined, h);
        return b64.encode(h, out);
    }
}

fn lower(c: int) -> [] int {
    if c >= 'A' && c <= 'Z' {
        return c + 32;
    }
    return c;
}

// Is `token` one of the comma-separated tokens of `value` (compared ignoring ASCII case, spaces around a token ignored)?
pub fn has_token[&v, &t](value: &v [byte], token: &t [byte]) -> [] bool {
    var start = 0;
    var i = 0;
    while i <= len(value) {
        if i == len(value) || int_of(value[i]) == ',' {
            var a = start;
            var b = i;
            while a < b && int_of(value[a]) == ' ' {
                a = a + 1;
            }
            while b > a && int_of(value[b - 1]) == ' ' {
                b = b - 1;
            }
            if b - a == len(token) {
                var same = true;
                var k = 0;
                while k < len(token) {
                    if lower(int_of(value[a + k])) != lower(int_of(token[k])) {
                        same = false;
                    }
                    k = k + 1;
                }
                if same {
                    return true;
                }
            }
            start = i + 1;
        }
        i = i + 1;
    }
    return false;
}

// Why a handshake request is refused, or 0 if it is good: 1 not a `GET` (or not HTTP/1.1), 2 no `Upgrade: websocket`, 3 no
// `Connection: Upgrade`, 4 no usable `Sec-WebSocket-Key` (24 base64 characters), 5 not version 13, 6 `ocpp1.6` not offered.
pub fn check_handshake[&s, &t](src: &s [byte], table: &t [int]) -> [] int {
    if !bytes.equal(http.method(src, table), "GET") || http.version(table) != 11 {
        return 1;
    }
    if !has_token(http.header(src, table, "upgrade"), "websocket") {
        return 2;
    }
    if !has_token(http.header(src, table, "connection"), "upgrade") {
        return 3;
    }
    let key = http.header(src, table, "sec-websocket-key");
    if len(key) != 24 || int_of(key[22]) != '=' || int_of(key[23]) != '=' {
        return 4;
    }
    if !bytes.equal(http.header(src, table, "sec-websocket-version"), "13") {
        return 5;
    }
    if !has_token(http.header(src, table, "sec-websocket-protocol"), "ocpp1.6") {
        return 6;
    }
    return 0;
}

// The opcodes (RFC 6455 section 5.2).
pub fn text() -> [] int {
    return 1;
}

pub fn binary() -> [] int {
    return 2;
}

pub fn close() -> [] int {
    return 8;
}

pub fn ping() -> [] int {
    return 9;
}

pub fn pong() -> [] int {
    return 10;
}

// Try to read one client frame from `buf[0..n]`, unmasking its payload in place. Answers `(code, opcode, start, length)`:
//
//     code > 0   a whole frame was there: it took `code` bytes, and the payload is `buf[start..start + length]`, unmasked
//     code = 0   not all of it has arrived
//     code < 0   refused: -1 not a valid client frame (not masked, a reserved bit, an opcode that is not defined, a control
//                frame over 125 bytes or fragmented), -2 larger than `max_payload`, -3 a fragmented message
pub fn parse_frame[&b](buf: &!b [byte], n: int, max_payload: int) -> [] (int, int, int, int) {
    if n < 2 {
        return (0, 0, 0, 0);
    }
    let b0 = int_of(buf[0]);
    let b1 = int_of(buf[1]);
    let fin = b0 >> 7 & 1;
    let opcode = b0 & 15;
    if b0 & 112 != 0 || b1 >> 7 != 1 {
        return (0 - 1, 0, 0, 0);
    }
    if opcode == 0 || opcode == 1 && fin == 0 || opcode == 2 && fin == 0 {
        return (0 - 3, 0, 0, 0);
    }
    if opcode > 2 && opcode < 8 || opcode > 10 {
        return (0 - 1, 0, 0, 0);
    }
    var length = b1 & 127;
    var at = 2;
    if opcode >= 8 && (length > 125 || fin == 0) {
        return (0 - 1, 0, 0, 0);
    }
    if length == 126 {
        if n < 4 {
            return (0, 0, 0, 0);
        }
        length = int_of(buf[2]) << 8 | int_of(buf[3]);
        at = 4;
    } else if length == 127 {
        if n < 10 {
            return (0, 0, 0, 0);
        }
        if int_of(buf[2]) != 0 || int_of(buf[3]) != 0 || int_of(buf[4]) != 0 || int_of(buf[5]) != 0 || int_of(buf[6]) >= 128 {
            return (0 - 2, 0, 0, 0);
        }
        length = int_of(buf[6]) << 24 | int_of(buf[7]) << 16 | int_of(buf[8]) << 8 | int_of(buf[9]);
        at = 10;
    }
    if length > max_payload {
        return (0 - 2, 0, 0, 0);
    }
    if n < at + 4 + length {
        return (0, 0, 0, 0);
    }
    let key = at;
    let start = at + 4;
    var i = 0;
    while i < length {
        buf[start + i] = byte_of(int_of(buf[start + i]) ^ int_of(buf[key + i % 4]));
        i = i + 1;
    }
    return (start + length, opcode, start, length);
}

// The header of a server frame (final, unmasked) with `length` payload bytes, into `out`. Answers its size: 2, 4 or 10.
pub fn put_header[&o](out: &!o [byte], opcode: int, length: int) -> [] int {
    out[0] = byte_of(128 | opcode);
    if length < 126 {
        out[1] = byte_of(length);
        return 2;
    }
    if length < 65536 {
        out[1] = byte_of(126);
        out[2] = byte_of(length >> 8 & 255);
        out[3] = byte_of(length & 255);
        return 4;
    }
    out[1] = byte_of(127);
    var i = 0;
    while i < 4 {
        out[2 + i] = byte_of(0);
        i = i + 1;
    }
    out[6] = byte_of(length >> 24 & 255);
    out[7] = byte_of(length >> 16 & 255);
    out[8] = byte_of(length >> 8 & 255);
    out[9] = byte_of(length & 255);
    return 10;
}
