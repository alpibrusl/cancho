edition 5;

// `docs/tls-assurance.md` §3.3: a server's certificate list, a TLS 1.3
// Certificate message's body, from standard input. It is split as the
// client splits it (`tls_message.certificate`) and verified as the client
// verifies it (`tls_slot.verify_chain`), against `fuzz_fixture`'s roots,
// host and time.
import fuzz_common;
import fuzz_fixture;
import std.io;
import tls_message;
import x509_verify;

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read] int {
    var input = box_slice(heap, 65536, byte_of(0));
    var store = box_slice(heap, 131072, byte_of(0));
    var info = box_slice(heap, 3 + 2 * tls_message.max_certificates(), 0);
    borrow mut input as &!d in {
        borrow mut store as &!s in {
            borrow mut info as &!f in {
                let used = fuzz_common.load_store(contents(s));
                let n = fuzz_common.read_all(io, contents(d));
                let body = contents(d)[0..n];
                let fi = contents(f);
                if tls_message.certificate(body, fi) == 0 {
                    region r {
                        let ranges = alloc_slice[r](2 * fi[2], 0);
                        var k = 0;
                        while k < 2 * fi[2] {
                            ranges[k] = fi[3 + k];
                            k = k + 1;
                        }
                        x509_verify.verify(contents(s)[0..used], body, ranges, fuzz_fixture.host(), fuzz_fixture.now(), x509_verify.tls_max_intermediates());
                    }
                }
            }
        }
    }
    unbox_slice(heap, input);
    unbox_slice(heap, store);
    unbox_slice(heap, info);
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
