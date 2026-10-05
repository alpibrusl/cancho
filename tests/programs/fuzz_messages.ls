edition 5;

// `docs/tls-assurance.md` §3.3: one handshake message's body, from
// standard input, to the parser its first byte names (modulo 11):
//
//     0 server_hello (with the session id the client sends)
//     1 encrypted_extensions     2 certificate      3 certificate_verify
//     4 finished, SHA-256        5 finished, SHA-384
//     6 new_session_ticket       7 key_update
//     8 certificate12            9 server_key_exchange
//     10 certificate_request12
import fuzz_common;
import std.io;
import tls_message;

// Each parser's `info` is exactly as long as the client makes it, so a
// write past what the client allocates traps here too.
fn info_len(which: int) -> [] int {
    if which == 0 {
        return tls_message.sh_info_len();
    } else if which == 2 || which == 8 {
        return 3 + 2 * tls_message.max_certificates();
    } else if which == 3 {
        return 3;
    } else if which == 9 {
        return tls_message.ske_info_len();
    }
    return 0;
}

fn parse[&b, &r, &f](which: int, b: &b [byte], random: &r [byte], info: &!f [int]) -> [] int {
    if which == 0 {
        return tls_message.server_hello(b, random[32..64], info);
    } else if which == 1 {
        return tls_message.encrypted_extensions(b);
    } else if which == 2 {
        return tls_message.certificate(b, info);
    } else if which == 3 {
        return tls_message.certificate_verify(b, info);
    } else if which == 4 {
        return tls_message.finished(b, 32);
    } else if which == 5 {
        return tls_message.finished(b, 48);
    } else if which == 6 {
        return tls_message.new_session_ticket(b);
    } else if which == 7 {
        return tls_message.key_update(b);
    } else if which == 8 {
        return tls_message.certificate12(b, info);
    } else if which == 9 {
        return tls_message.server_key_exchange(b, info);
    }
    return tls_message.certificate_request12(b);
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read] int {
    var input = box_slice(heap, 65536, byte_of(0));
    var random = box_slice(heap, 96, byte_of(0));
    borrow mut input as &!d in {
        borrow mut random as &!r in {
            fuzz_common.load_random(contents(r));
            let n = fuzz_common.read_all(io, contents(d));
            if n > 0 {
                let which = int_of(contents(d)[0]) % 11;
                region q {
                    let info = alloc_slice[q](info_len(which), 0);
                    parse(which, contents(d)[1..n], contents(r), info);
                }
            }
        }
    }
    unbox_slice(heap, input);
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
