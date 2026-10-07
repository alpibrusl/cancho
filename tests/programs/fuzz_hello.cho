edition 5;

// `docs/tls-server.md` §7: a ClientHello's body from standard input (after
// the 4-byte handshake header), through `tls_hello.client_hello` and every
// choice made from what it finds: suite with and without AES
// instructions, group, retry group, and ALPN against a fixed list. The
// parser alone, so it runs many times faster than `fuzz_server`.
import std.io;
import tls_hello;

fn read_all[&i, &o](io: &!i Io, into: &!o [byte]) -> [io_read] int {
    var n = 0;
    var c = getchar(io);
    while c >= 0 && n < len(into) {
        into[n] = byte_of(c);
        n = n + 1;
        c = getchar(io);
    }
    return n;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read] int {
    var input = box_slice(heap, 65536, byte_of(0));
    borrow mut input as &!d in {
        let total = read_all(io, contents(d));
        let body = contents(d)[0..total];
        region r {
            let info = alloc_slice[r](tls_hello.ch_info_len(), 0);
            let ours = alloc_slice[r](12, byte_of(0));
            // "h2" and "http/1.1" in ALPN's wire form.
            ours[0] = byte_of(2);
            ours[1] = byte_of(104);
            ours[2] = byte_of(50);
            ours[3] = byte_of(8);
            let h11 = "http/1.1";
            var k = 0;
            while k < 8 {
                ours[4 + k] = h11[k];
                k = k + 1;
            }
            if tls_hello.client_hello(body, info) == 0 {
                tls_hello.choose_suite(info[tls_hello.ch_suites()], true);
                tls_hello.choose_suite(info[tls_hello.ch_suites()], false);
                tls_hello.share_group(info);
                tls_hello.retry_group(info);
                let s = info[tls_hello.ch_alpn_start()];
                if s != 0 {
                    tls_hello.choose_alpn(ours, body[s..info[tls_hello.ch_alpn_end()]]);
                }
            }
        }
    }
    unbox_slice(heap, input);
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
