edition 5;

// `docs/tls-assurance.md` §3.3: the client past the AEAD. The harness
// plays a TLS 1.3 server: it feeds one of `fuzz_fixture`'s recorded
// ServerHellos (the input's first byte, modulo 3, says which), then each
// record the input names, sealed under the keys the client will open it
// with. The input after the first byte is a run of records, each a type
// byte, a 2-byte big-endian length and the plaintext:
//
//     type < 128    sealed as a TLS 1.3 record of inner type `type`, under
//                   the client's current read key, IV and sequence number,
//                   read from its slot just before. So the handshake keys,
//                   the application keys and every KeyUpdate are followed
//                   without the harness keeping a key schedule of its own.
//     type >= 128   sent in the clear, as a record of type `type - 128`
//
// Reading the keys from the client is enough here: the claim under test
// is that nothing traps. Whether the client derives the right keys is
// what RFC 8448's vectors and the interop matrix check.
import fuzz_common;
import fuzz_fixture;
import std.io;
import tls_client;
import tls_record;
import tls_slot;

// One record of the input, as the server would send it, into `out`; its
// length, or 0 when it cannot be made (a plaintext over the limit, or no
// keys yet).
fn record[&n, &b, &p, &o](ints: &n [int], bytes: &b [byte], kind: int, plaintext: &p [byte], out: &!o [byte]) -> [] int {
    let n = len(plaintext);
    if kind >= 128 {
        if 5 + n > len(out) {
            return 0;
        }
        out[0] = byte_of(kind - 128);
        out[1] = byte_of(3);
        out[2] = byte_of(3);
        out[3] = byte_of(n >> 8);
        out[4] = byte_of(n & 255);
        var k = 0;
        while k < n {
            out[5 + k] = plaintext[k];
            k = k + 1;
        }
        return 5 + n;
    }
    let suite = ints[tls_slot.i_suite()];
    if suite == 0 {
        return 0;
    }
    let key = bytes[tls_slot.k_read_key()..tls_slot.k_read_key() + tls_slot.key_len(ints)];
    let iv = bytes[tls_slot.k_read_iv()..tls_slot.k_read_iv() + 12];
    let made = tls_record.seal(suite, key, iv, ints[tls_slot.i_read_seq()], kind, plaintext, out);
    if made < 0 {
        return 0;
    }
    return made;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read] int {
    var input = box_slice(heap, 262144, byte_of(0));
    var slot_bytes = box_slice(heap, tls_client.bytes_len(), byte_of(0));
    var slot_ints = box_slice(heap, tls_client.ints_len(), 0);
    var out = box_slice(heap, 65536, byte_of(0));
    var wire = box_slice(heap, 65536, byte_of(0));
    var store = box_slice(heap, 131072, byte_of(0));
    var random = box_slice(heap, 96, byte_of(0));
    borrow mut input as &!d in {
        borrow mut slot_bytes as &!b in {
            borrow mut slot_ints as &!n in {
                borrow mut out as &!o in {
                    borrow mut wire as &!w in {
                        borrow mut store as &!s in {
                            borrow mut random as &!r in {
                                let used = fuzz_common.load_store(contents(s));
                                fuzz_common.load_random(contents(r));
                                let total = fuzz_common.read_all(io, contents(d));
                                let s0 = contents(s)[0..used];
                                let ints = contents(n);
                                let bytes = contents(b);
                                let input = contents(d);
                                var code = tls_client.start(ints, bytes, fuzz_fixture.host(), contents(r), fuzz_fixture.now());
                                fuzz_common.drain(ints, bytes, contents(o));
                                if total > 0 {
                                    let hello = fuzz_fixture.server_hello_hex(int_of(input[0]) % 3);
                                    let m = fuzz_common.from_hex(hello, contents(w));
                                    code = fuzz_common.feed_all(ints, bytes, contents(w)[0..m], contents(o), s0);
                                }
                                var at = 1;
                                while code >= 0 && at + 3 <= total {
                                    let kind = int_of(input[at]);
                                    var size = int_of(input[at + 1]) * 256 + int_of(input[at + 2]);
                                    at = at + 3;
                                    if at + size > total {
                                        size = total - at;
                                    }
                                    let made = record(ints, bytes, kind, input[at..at + size], contents(w));
                                    at = at + size;
                                    if made > 0 {
                                        code = fuzz_common.feed_all(ints, bytes, contents(w)[0..made], contents(o), s0);
                                    }
                                }
                                if tls_client.event(ints) == tls_client.event_established() {
                                    tls_client.send(ints, bytes, "GET / HTTP/1.0\r\n\r\n");
                                    fuzz_common.drain(ints, bytes, contents(o));
                                }
                                tls_client.peer_eof(ints, bytes);
                                fuzz_common.drain(ints, bytes, contents(o));
                                tls_client.finish(ints, bytes);
                                fuzz_common.drain(ints, bytes, contents(o));
                                tls_client.drop(ints, bytes);
                            }
                        }
                    }
                }
            }
        }
    }
    unbox_slice(heap, input);
    unbox_slice(heap, slot_bytes);
    unbox_slice(heap, slot_ints);
    unbox_slice(heap, out);
    unbox_slice(heap, wire);
    unbox_slice(heap, store);
    unbox_slice(heap, random);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(args);
    release(ffi);
    release(fs);
    release(net);
    release(clock);
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            run(h, i);
        }
    }
    release(heap);
    release(io);
    return 0;
}
