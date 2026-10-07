edition 5;

import std.bytes;
import std.test;
import std.http;
import std.json;
import sha1;
import b64;
import ws;
import timefmt;
import ocpp;

// The pieces of the OCPP WebSocket example, without a network: the handshake's key (RFC 6455 section 1.3's own example), frames
// (RFC 6455 section 5.7's examples), the time, and the OCPP messages.

fn test_the_rfc_accept_key() -> [] int {
    region a {
        let out = alloc_slice[a](28, byte_of(0));
        test.assert_eq(ws.accept_key("dGhlIHNhbXBsZSBub25jZQ==", out), 28);
        test.assert(bytes.equal(out, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
    }
    return 0;
}

fn test_sha1_of_abc_and_the_empty_string() -> [] int {
    region a {
        let out = alloc_slice[a](20, byte_of(0));
        sha1.digest("abc", out);
        test.assert_eq(int_of(out[0]), 0xa9);
        test.assert_eq(int_of(out[1]), 0x99);
        test.assert_eq(int_of(out[19]), 0x9d);
        sha1.digest("", out);
        test.assert_eq(int_of(out[0]), 0xda);
        test.assert_eq(int_of(out[1]), 0x39);
        test.assert_eq(int_of(out[19]), 0x09);
    }
    return 0;
}

fn test_base64_of_every_padding() -> [] int {
    region a {
        let out = alloc_slice[a](16, byte_of(0));
        test.assert_eq(b64.encode("", out), 0);
        test.assert_eq(b64.encode("f", out), 4);
        test.assert(bytes.equal(out[0..4], "Zg=="));
        test.assert_eq(b64.encode("fo", out), 4);
        test.assert(bytes.equal(out[0..4], "Zm8="));
        test.assert_eq(b64.encode("foo", out), 4);
        test.assert(bytes.equal(out[0..4], "Zm9v"));
        test.assert_eq(b64.encode("foobar", out), 8);
        test.assert(bytes.equal(out[0..8], "Zm9vYmFy"));
    }
    return 0;
}

// RFC 6455 section 5.7: a single-frame masked text message containing "Hello": 81 85 37 fa 21 3d 7f 9f 4d 51 58.
fn test_the_rfc_masked_hello() -> [] int {
    region a {
        let f = alloc_slice[a](16, byte_of(0));
        f[0] = byte_of(0x81);
        f[1] = byte_of(0x85);
        f[2] = byte_of(0x37);
        f[3] = byte_of(0xfa);
        f[4] = byte_of(0x21);
        f[5] = byte_of(0x3d);
        f[6] = byte_of(0x7f);
        f[7] = byte_of(0x9f);
        f[8] = byte_of(0x4d);
        f[9] = byte_of(0x51);
        f[10] = byte_of(0x58);
        test.assert_eq(ws.parse_frame(f, 10, 100).0, 0);
        let r = ws.parse_frame(f, 11, 100);
        test.assert_eq(r.0, 11);
        test.assert_eq(r.1, 1);
        test.assert_eq(r.3, 5);
        test.assert(bytes.equal(f[r.2..r.2 + r.3], "Hello"));
    }
    return 0;
}

fn test_a_frame_that_is_not_allowed_is_refused_not_misread() -> [] int {
    region a {
        let f = alloc_slice[a](300, byte_of(0));
        // not masked
        f[0] = byte_of(0x81);
        f[1] = byte_of(0x01);
        test.assert_eq(ws.parse_frame(f, 8, 100).0, 0 - 1);
        // a reserved bit
        f[0] = byte_of(0xc1);
        f[1] = byte_of(0x81);
        test.assert_eq(ws.parse_frame(f, 8, 100).0, 0 - 1);
        // an opcode that is not defined
        f[0] = byte_of(0x83);
        test.assert_eq(ws.parse_frame(f, 8, 100).0, 0 - 1);
        // fragmented text, and a continuation
        f[0] = byte_of(0x01);
        test.assert_eq(ws.parse_frame(f, 8, 100).0, 0 - 3);
        f[0] = byte_of(0x80);
        test.assert_eq(ws.parse_frame(f, 8, 100).0, 0 - 3);
        // a ping of 126 bytes
        f[0] = byte_of(0x89);
        f[1] = byte_of(0xfe);
        test.assert_eq(ws.parse_frame(f, 8, 1000).0, 0 - 1);
        // a text frame larger than the limit
        f[0] = byte_of(0x81);
        f[1] = byte_of(0x80 + 101);
        test.assert_eq(ws.parse_frame(f, 8, 100).0, 0 - 2);
        // 64-bit length with the high bytes set
        f[1] = byte_of(0xff);
        f[2] = byte_of(1);
        test.assert_eq(ws.parse_frame(f, 14, 100).0, 0 - 2);
    }
    return 0;
}

fn test_a_16_bit_length_and_the_server_header() -> [] int {
    region a {
        let f = alloc_slice[a](400, byte_of(0));
        f[0] = byte_of(0x81);
        f[1] = byte_of(0xfe);
        f[2] = byte_of(1);
        f[3] = byte_of(0x2c);
        let r = ws.parse_frame(f, 8 + 300, 1000);
        test.assert_eq(r.0, 8 + 300);
        test.assert_eq(r.3, 300);
        let h = alloc_slice[a](10, byte_of(0));
        test.assert_eq(ws.put_header(h, 1, 5), 2);
        test.assert_eq(int_of(h[0]), 0x81);
        test.assert_eq(int_of(h[1]), 5);
        test.assert_eq(ws.put_header(h, 8, 300), 4);
        test.assert_eq(int_of(h[0]), 0x88);
        test.assert_eq(int_of(h[1]), 126);
        test.assert_eq(int_of(h[2]) * 256 + int_of(h[3]), 300);
        test.assert_eq(ws.put_header(h, 1, 70000), 10);
        test.assert_eq(int_of(h[1]), 127);
        test.assert_eq(int_of(h[7]) * 65536 + int_of(h[8]) * 256 + int_of(h[9]), 70000);
    }
    return 0;
}

fn test_has_token() -> [] int {
    test.assert(ws.has_token("keep-alive, Upgrade", "upgrade"));
    test.assert(ws.has_token("Upgrade", "upgrade"));
    test.assert(!ws.has_token("upgrades", "upgrade"));
    test.assert(!ws.has_token("", "upgrade"));
    test.assert(ws.has_token("ocpp2.0.1 , ocpp1.6", "ocpp1.6"));
    return 0;
}

fn test_iso_time() -> [] int {
    region a {
        let o = alloc_slice[a](30, byte_of(0));
        // 2026-10-03T17:55:41.123Z
        test.assert_eq(timefmt.iso(o, 0, 1791050141123), 24);
        test.assert(bytes.equal(o[0..24], "2026-10-03T17:55:41.123Z"));
        timefmt.iso(o, 0, 0);
        test.assert(bytes.equal(o[0..24], "1970-01-01T00:00:00.000Z"));
        // a leap day, and the day after
        timefmt.iso(o, 0, 951782400000);
        test.assert(bytes.equal(o[0..24], "2000-02-29T00:00:00.000Z"));
        timefmt.iso(o, 0, 951868800000 + 86399999);
        test.assert(bytes.equal(o[0..24], "2000-03-01T23:59:59.999Z"));
        timefmt.iso(o, 0, 4102444799999);
        test.assert(bytes.equal(o[0..24], "2099-12-31T23:59:59.999Z"));
    }
    return 0;
}

fn test_a_boot_notification_and_a_heartbeat() -> [] int {
    region a {
        let text = "[2,\"19223201\",\"BootNotification\",{\"chargePointVendor\":\"V\",\"chargePointModel\":\"M\"}]";
        let tape = alloc_slice[a](json.tape_len(text), 0);
        let m = ocpp.read_message(text, tape);
        test.assert_eq(m.0, 2);
        test.assert(bytes.equal(text[m.1..m.2], "19223201"));
        test.assert(bytes.equal(text[m.3..m.4], "BootNotification"));
        let out = alloc_slice[a](256, byte_of(0));
        let n = ocpp.reply(out, 0, text, m.1, m.2, m.3, m.4, 1791050141123, 300);
        test.assert(bytes.equal(out[0..n], "[3,\"19223201\",{\"currentTime\":\"2026-10-03T17:55:41.123Z\",\"interval\":300,\"status\":\"Accepted\"}]"));
        let hb = "[2,\"x\",\"Heartbeat\",{}]";
        let tape2 = alloc_slice[a](json.tape_len(hb), 0);
        let h = ocpp.read_message(hb, tape2);
        let n2 = ocpp.reply(out, 0, hb, h.1, h.2, h.3, h.4, 0, 300);
        test.assert(bytes.equal(out[0..n2], "[3,\"x\",{\"currentTime\":\"1970-01-01T00:00:00.000Z\"}]"));
    }
    return 0;
}

fn test_other_calls_and_what_is_not_a_call() -> [] int {
    region a {
        let out = alloc_slice[a](256, byte_of(0));
        let sn = "[2,\"7\",\"StatusNotification\",{\"connectorId\":1}]";
        let t1 = alloc_slice[a](json.tape_len(sn), 0);
        let m = ocpp.read_message(sn, t1);
        let n = ocpp.reply(out, 0, sn, m.1, m.2, m.3, m.4, 0, 300);
        test.assert(bytes.equal(out[0..n], "[3,\"7\",{}]"));
        let other = "[2,\"8\",\"Authorize\",{}]";
        let t2 = alloc_slice[a](json.tape_len(other), 0);
        let o = ocpp.read_message(other, t2);
        let n2 = ocpp.reply(out, 0, other, o.1, o.2, o.3, o.4, 0, 300);
        test.assert(bytes.equal(out[0..n2], "[4,\"8\",\"NotImplemented\",\"\",{}]"));
        // results and errors from the charge point are recognised (and not answered)
        let rs = "[3,\"9\",{}]";
        let t3 = alloc_slice[a](json.tape_len(rs), 0);
        test.assert_eq(ocpp.read_message(rs, t3).0, 3);
        let err = "[4,\"9\",\"NotSupported\",\"\",{}]";
        let t4 = alloc_slice[a](json.tape_len(err), 0);
        test.assert_eq(ocpp.read_message(err, t4).0, 4);
        // not JSON, not an array, wrong shape, an id that is too long
        let bad1 = "[2,\"1\",\"Heartbeat\",{";
        let t5 = alloc_slice[a](json.tape_len(bad1), 0);
        test.assert_eq(ocpp.read_message(bad1, t5).0, 0 - 1);
        let bad2 = "{\"a\":1}";
        let t6 = alloc_slice[a](json.tape_len(bad2), 0);
        test.assert_eq(ocpp.read_message(bad2, t6).0, 0 - 2);
        let bad3 = "[2,\"1\",\"Heartbeat\"]";
        let t7 = alloc_slice[a](json.tape_len(bad3), 0);
        test.assert_eq(ocpp.read_message(bad3, t7).0, 0 - 2);
        let bad4 = "[2,\"0123456789012345678901234567890123456\",\"Heartbeat\",{}]";
        let t8 = alloc_slice[a](json.tape_len(bad4), 0);
        test.assert_eq(ocpp.read_message(bad4, t8).0, 0 - 3);
        let r1 = ocpp.refusal(out, 0, 0 - 1);
        test.assert(bytes.equal(out[0..r1], "[4,\"-1\",\"FormationViolation\",\"\",{}]"));
    }
    return 0;
}

// Parse `head` as a request and say what the handshake check thinks of it.
fn judged[&s](head: &s [byte]) -> [] int {
    region a {
        let table = alloc_slice[a](http.slots(32), 0);
        if http.parse(head, table) <= 0 {
            return 0 - 1;
        }
        return ws.check_handshake(head, table);
    }
}

fn test_the_handshake_check() -> [] int {
    let good = "GET /ocpp/CP1 HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: ocpp2.0.1, ocpp1.6\r\n\r\n";
    test.assert_eq(judged(good), 0);
    test.assert_eq(judged("POST /ocpp/CP1 HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n"), 1);
    test.assert_eq(judged("GET /ocpp/CP1 HTTP/1.0\r\nHost: x\r\n\r\n"), 1);
    test.assert_eq(judged("GET /ocpp/CP1 HTTP/1.1\r\nHost: x\r\nConnection: Upgrade\r\n\r\n"), 2);
    test.assert_eq(judged("GET /ocpp/CP1 HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\n\r\n"), 3);
    test.assert_eq(judged("GET /ocpp/CP1 HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: short\r\n\r\n"), 4);
    test.assert_eq(judged("GET /ocpp/CP1 HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 8\r\n\r\n"), 5);
    test.assert_eq(judged("GET /ocpp/CP1 HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: ocpp2.0.1\r\n\r\n"), 6);
    return 0;
}
