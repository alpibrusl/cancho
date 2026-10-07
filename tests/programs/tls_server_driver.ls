edition 5;

// `docs/tls-server.md` §8, step 2: a `packages/tls` server engine driven
// from standard input, one line at a time, each answered and flushed
// before the next is read, so a harness (`scripts/tls_liar_client.py`) can
// play the client. Byte strings are lowercase hex (`-` for empty). The
// engine has one slot, 0, unless a line says otherwise.
//
//     E <seed>                                tls.seed: `<code> <tag>`
//     I <chain> <key> <names> <now ms>        tls.add_identity (PEM bytes; names text): `<id or code> <tag>`
//     R <id> <chain> <key> <now ms>           tls.replace_identity: `<code> <tag>`
//     A <protocols>                           tls.set_alpn (text): `<code> <tag>`
//     V <now ms>                              tls.serve in slot 0
//     F <bytes>                               tls.feed, then everything `take` and `recv` give
//     W <plaintext>                           tls.send
//     Q                                       tls.finish
//     Z                                       tls.eof
//     D                                       tls.drop: `<code> <tag>`
//     N                                       what the connection chose: `0 ok <event> <server_name> <alpn>
//                                             <handshakes in progress>`
//     O                                       the suite chosen for each mask of offered suites (`tls_hello`),
//                                             with and without AES instructions: `0 ok` and 16 numbers
//     C                                       the calls of the other role: `tls.start` on this engine, and
//                                             `serve`, `add_identity`, `set_alpn` and `server_name` on a client
//                                             engine: `<code> <tag>` each, on one line
//
// `V`, `F`, `W`, `Q` and `Z` answer `<code> <tag> <event> <bytes for the
// socket> <application data received>`, the event once both are taken.
import std.io;
import tls;
import tls_hello;

fn nibble(c: int) -> [] int {
    if c >= 97 {
        return c - 87;
    }
    return c - 48;
}

fn field_end[&s](s: &s [byte], at: int) -> [] int {
    var e = at;
    while e < len(s) && int_of(s[e]) != 32 && int_of(s[e]) != 10 {
        e = e + 1;
    }
    return e;
}

fn next_field[&s](s: &s [byte], at: int) -> [] int {
    return field_end(s, at) + 1;
}

fn hex_len[&s](s: &s [byte], at: int) -> [] int {
    if at >= len(s) || field_end(s, at) - at == 1 && int_of(s[at]) == 45 {
        return 0;
    }
    return (field_end(s, at) - at) / 2;
}

fn hex_into[&s, &o](s: &s [byte], at: int, o: &!o [byte]) -> [] int {
    let n = hex_len(s, at);
    var i = 0;
    while i < len(o) && i < n {
        o[i] = byte_of(nibble(int_of(s[at + i * 2])) * 16 + nibble(int_of(s[at + i * 2 + 1])));
        i = i + 1;
    }
    return n;
}

fn number[&s](s: &s [byte], at: int) -> [] int {
    let e = field_end(s, at);
    var n = 0;
    var p = at;
    while p < e {
        n = n * 10 + int_of(s[p]) - 48;
        p = p + 1;
    }
    return n;
}

fn print_hex[&i, &d](io: &!i Io, d: &d [byte]) -> [io_write] int {
    let digits = "0123456789abcdef";
    var n = 0;
    while n < len(d) {
        let b = int_of(d[n]);
        io.write_all(io, digits[b >> 4..(b >> 4) + 1]);
        io.write_all(io, digits[b & 15..(b & 15) + 1]);
        n = n + 1;
    }
    if len(d) == 0 {
        io.write_all(io, "-");
    }
    return 0;
}

fn tag_line[&i](io: &!i Io, code: int) -> [io_write] int {
    io.print_int(io, code);
    io.space(io);
    if code >= 0 || code == tls.would_block() {
        io.write_all(io, "ok");
    } else {
        io.write_all(io, tls.refusal_tag(code));
    }
    return 0;
}

// Everything `take` has, then everything `recv` has, then the event.
fn drain[&i, &e, &o](io: &!i Io, engine: &!e tls.Engine, out: &!o [byte]) -> [io_write] int {
    var at = 0;
    var n = tls.take(engine, 0, out);
    while n > 0 {
        at = at + n;
        n = tls.take(engine, 0, out[at..len(out)]);
    }
    io.space(io);
    io.print_int(io, tls.event(engine, 0));
    io.space(io);
    print_hex(io, out[0..at]);
    io.space(io);
    var any = false;
    n = tls.recv(engine, 0, out);
    while n > 0 {
        print_hex(io, out[0..n]);
        any = true;
        n = tls.recv(engine, 0, out);
    }
    if !any {
        io.write_all(io, "-");
    }
    io.newline(io);
    return 0;
}

// The calls of the other role, each refused `tls-role`.
fn roles[&h, &i, &e](heap: &!h Heap, io: &!i Io, engine: &!e tls.Engine) -> [heap, io_write] int {
    tag_line(io, tls.start(engine, 0, "x.example", 0));
    io.space(io);
    tag_line(io, tls.trust(engine, "-"));
    var client = tls.open(heap, 1);
    borrow mut client as &!cw in {
        io.space(io);
        tag_line(io, tls.serve(cw, 0, 0));
        io.space(io);
        tag_line(io, tls.add_identity(cw, "", "", "", 0));
        io.space(io);
        tag_line(io, tls.set_alpn(cw, "h2"));
        region r {
            let name = alloc_slice[r](8, byte_of(0));
            io.space(io);
            tag_line(io, tls.server_name(cw, 0, name));
        }
    }
    tls.close(heap, client);
    io.newline(io);
    return 0;
}

fn op_line[&h, &i, &s, &e, &o, &d](heap: &!h Heap, io: &!i Io, s: &s [byte], engine: &!e tls.Engine, out: &!o [byte], held: &!d [byte]) -> [heap, io_write] int {
    let op = int_of(s[0]);
    let f = 2;
    if op == 69 {
        let n = hex_into(s, f, held);
        tag_line(io, tls.seed(engine, held[0..n]));
        io.newline(io);
        return 0;
    }
    if op == 73 || op == 82 {
        var g = f;
        var id = 0;
        if op == 82 {
            id = number(s, g);
            g = next_field(s, g);
        }
        // The chain into `held`, the key and the names into `out`.
        let cn = hex_into(s, g, held);
        g = next_field(s, g);
        let kn = hex_into(s, g, out);
        g = next_field(s, g);
        var code = 0;
        if op == 73 {
            let nn = hex_into(s, g, out[kn..len(out)]);
            g = next_field(s, g);
            code = tls.add_identity(engine, held[0..cn], out[0..kn], out[kn..kn + nn], number(s, g));
        } else {
            code = tls.replace_identity(engine, id, held[0..cn], out[0..kn], number(s, g));
        }
        tls_zero(out);
        tag_line(io, code);
        io.newline(io);
        return 0;
    }
    if op == 65 {
        let n = hex_into(s, f, held);
        tag_line(io, tls.set_alpn(engine, held[0..n]));
        io.newline(io);
        return 0;
    }
    if op == 68 {
        tag_line(io, tls.drop(engine, 0));
        io.newline(io);
        return 0;
    }
    if op == 67 {
        roles(heap, io, engine);
        return 0;
    }
    if op == 79 {
        io.write_all(io, "0 ok");
        var mask = 0;
        while mask < 8 {
            io.space(io);
            io.print_int(io, tls_hello.choose_suite(mask, true));
            io.space(io);
            io.print_int(io, tls_hello.choose_suite(mask, false));
            mask = mask + 1;
        }
        io.newline(io);
        return 0;
    }
    if op == 78 {
        io.write_all(io, "0 ok ");
        io.print_int(io, tls.event(engine, 0));
        io.space(io);
        region r {
            let name = alloc_slice[r](256, byte_of(0));
            let n = tls.server_name(engine, 0, name);
            print_hex(io, name[0..n]);
            io.space(io);
            let a = tls.alpn(engine, 0, name);
            print_hex(io, name[0..a]);
        }
        io.space(io);
        io.print_int(io, tls.handshakes_in_progress(engine));
        io.newline(io);
        return 0;
    }
    var code = 0;
    if op == 86 {
        code = tls.serve(engine, 0, number(s, f));
    } else if op == 70 {
        let n = hex_into(s, f, held);
        var consumed = 0;
        var going = true;
        while going {
            let c = tls.feed(engine, 0, held[consumed..n]);
            if c < 0 {
                code = c;
                going = false;
            } else {
                consumed = consumed + c;
                code = consumed;
                if consumed == n || c == 0 {
                    // Output or received data must be taken first: the
                    // harness reads both from this line.
                    going = false;
                }
            }
        }
    } else if op == 87 {
        let n = hex_into(s, f, held);
        code = tls.send(engine, 0, held[0..n]);
    } else if op == 81 {
        code = tls.finish(engine, 0);
    } else if op == 90 {
        code = tls.eof(engine, 0);
    }
    tag_line(io, code);
    drain(io, engine, out);
    return 0;
}

fn tls_zero[&o](out: &!o [byte]) -> [] int {
    var k = 0;
    while k < len(out) {
        out[k] = byte_of(0);
        k = k + 1;
    }
    return 0;
}

// One line of standard input into `line`; its length, or -1 at the end.
fn read_line[&i, &l](io: &!i Io, line: &!l [byte]) -> [io_read] int {
    var n = 0;
    var c = getchar(io);
    if c < 0 {
        return -1;
    }
    while c >= 0 && c != 10 {
        if n < len(line) {
            line[n] = byte_of(c);
        }
        n = n + 1;
        c = getchar(io);
    }
    return n;
}

fn flush[&i](io: &!i Io) -> [io_write] int {
    match flush_out(io) {
        Done::Ok(n) => {
            return 0;
        }
        Done::Failed(e) => {
            return e;
        }
    }
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var line = box_slice(heap, 262144, byte_of(0));
    var out = box_slice(heap, 131072, byte_of(0));
    var held = box_slice(heap, 131072, byte_of(0));
    var engine = tls.open_server(heap, 1);
    borrow mut line as &!lw in {
        borrow mut out as &!ow in {
            borrow mut held as &!dw in {
                borrow mut engine as &!ew in {
                    let l = contents(lw);
                    var n = read_line(io, l);
                    while n >= 0 {
                        if n > 0 && n <= len(l) {
                            op_line(heap, io, l[0..n], ew, contents(ow), contents(dw));
                        }
                        flush(io);
                        n = read_line(io, l);
                    }
                }
            }
        }
    }
    tls.close(heap, engine);
    unbox_slice(heap, line);
    unbox_slice(heap, out);
    unbox_slice(heap, held);
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
