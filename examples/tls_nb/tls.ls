edition 5;

module tls;

import std.conns;
import std.vec;

// `tls` -- a non-blocking TLS client engine over OpenSSL, for a single thread
// driving many connections from one `Poller` (`docs/tls-nonblocking.md`).
//
// **No file descriptor reaches OpenSSL.** `conn_raw_fd` (`docs/native-sockets.md` §6) is
// designed and not built, so this module uses OpenSSL as a state machine over two memory
// BIOs per connection: bytes read from the socket go into the read BIO, bytes OpenSSL
// wants sent come out of the write BIO. The sockets stay `Conn`s in a `std.conns.Table`,
// which keeps the one place that touches the kernel the one the type system already
// governs; this module never sees a descriptor.
//
// **Every handle is an `int`.** An `SSL *` is a 64-bit pointer returned in a register, so
// declaring the extern with an `int` result carries it exactly. `c_ptr` would be the honest
// type, but it cannot be named in an ordinary function's signature, so no helper could take
// or return one (gap 1, section 9 of the document); that is the whole reason for the `int`s. A C `int`
// result is `c_int`, which sign-extends, because the upper half of `rax` is not defined by
// the ABI and `-1` would otherwise read as 4294967295.
//
// **One authority scope for both libraries**: `Ffi("tls")`. `narrow` consumes an `Ffi`,
// so one program has one scope, and OpenSSL is two libraries (libssl for `SSL_*`, libcrypto
// for `BIO_*`, `ERR_*` and `X509_*`).

// ---------------------------------------------------------------------
// OpenSSL
// ---------------------------------------------------------------------
extern fn TLS_client_method[&f](ffi: &f Ffi("tls")) -> [ffi("tls")] int;

extern fn SSL_CTX_new[&f](ffi: &f Ffi("tls"), method: int) -> [ffi("tls")] int;

extern fn SSL_CTX_free[&f](ffi: &f Ffi("tls"), ctx: int) -> [ffi("tls")] int;

// `SSL_CTX_set_min_proto_version` and `SSL_CTX_set_mode` are macros over this; the last
// argument is a `void *` that is NULL for both.
extern fn SSL_CTX_ctrl[&f](ffi: &f Ffi("tls"), ctx: int, cmd: int, larg: int, parg: int) -> [ffi("tls")] int;

extern fn SSL_CTX_set_verify[&f](ffi: &f Ffi("tls"), ctx: int, mode: int, callback: int) -> [ffi("tls")] int;

extern fn SSL_CTX_set_default_verify_paths[&f](ffi: &f Ffi("tls"), ctx: int) -> [ffi("tls")] c_int;

// `SSL_CTX_load_verify_locations` takes two strings and a byte slice crosses as two arguments,
// so only its last parameter would be right (docs/opaque-pointers.md section 6); OpenSSL 3.0 has
// the one-string form, which is why this is the call that loads a CA file.
extern fn SSL_CTX_load_verify_file[&f, &p](ffi: &f Ffi("tls"), ctx: int, path: &p [byte]) -> [ffi("tls")] c_int;

extern fn SSL_new[&f](ffi: &f Ffi("tls"), ctx: int) -> [ffi("tls")] int;

extern fn SSL_free[&f](ffi: &f Ffi("tls"), ssl: int) -> [ffi("tls")] int;

extern fn SSL_set_fd[&f](ffi: &f Ffi("tls"), ssl: int, fd: int) -> [ffi("tls")] c_int;

extern fn SSL_set_bio[&f](ffi: &f Ffi("tls"), ssl: int, rbio: int, wbio: int) -> [ffi("tls")] int;

extern fn SSL_set_connect_state[&f](ffi: &f Ffi("tls"), ssl: int) -> [ffi("tls")] int;

extern fn SSL_do_handshake[&f](ffi: &f Ffi("tls"), ssl: int) -> [ffi("tls")] c_int;

extern fn SSL_read[&f, &b](ffi: &f Ffi("tls"), ssl: int, buf: &!b [byte]) -> [ffi("tls")] c_int;

extern fn SSL_write[&f, &b](ffi: &f Ffi("tls"), ssl: int, buf: &b [byte]) -> [ffi("tls")] c_int;

extern fn SSL_shutdown[&f](ffi: &f Ffi("tls"), ssl: int) -> [ffi("tls")] c_int;

extern fn SSL_get_error[&f](ffi: &f Ffi("tls"), ssl: int, ret: int) -> [ffi("tls")] c_int;

// `SSL_set_tlsext_host_name(ssl, name)` is a macro over this call: command 55, name type 0
// (`TLSEXT_NAMETYPE_host_name`), and the name as the `void *` argument, NUL-terminated.
extern fn SSL_ctrl[&f, &b](ffi: &f Ffi("tls"), ssl: int, cmd: int, larg: int, parg: &b [byte]) -> [ffi("tls")] int;

extern fn SSL_get0_param[&f](ffi: &f Ffi("tls"), ssl: int) -> [ffi("tls")] int;

extern fn SSL_get_verify_result[&f](ffi: &f Ffi("tls"), ssl: int) -> [ffi("tls")] int;

extern fn SSL_version[&f](ffi: &f Ffi("tls"), ssl: int) -> [ffi("tls")] c_int;

extern fn SSL_get_current_cipher[&f](ffi: &f Ffi("tls"), ssl: int) -> [ffi("tls")] int;

extern fn SSL_CIPHER_description[&f, &b](ffi: &f Ffi("tls"), cipher: int, buf: &!b [byte]) -> [ffi("tls")] int;

extern fn SSL_session_reused[&f](ffi: &f Ffi("tls"), ssl: int) -> [ffi("tls")] c_int;

extern fn SSL_get1_session[&f](ffi: &f Ffi("tls"), ssl: int) -> [ffi("tls")] int;

extern fn SSL_set_session[&f](ffi: &f Ffi("tls"), ssl: int, session: int) -> [ffi("tls")] c_int;

extern fn SSL_SESSION_is_resumable[&f](ffi: &f Ffi("tls"), session: int) -> [ffi("tls")] c_int;

extern fn SSL_SESSION_free[&f](ffi: &f Ffi("tls"), session: int) -> [ffi("tls")] int;

extern fn SSL_CTX_sess_set_cache_size[&f](ffi: &f Ffi("tls"), ctx: int, size: int) -> [ffi("tls")] int;

// `signal(SIGPIPE, SIG_IGN)`: `SIGPIPE` is 13 and `SIG_IGN` is the handler value 1. Needed only by the direct transport (`open`'s `fd`):
// OpenSSL's socket BIO writes with `write(2)`, and a write to a peer that has closed kills the process with `SIGPIPE`, including the
// fatal alert OpenSSL itself sends when a handshake fails because the peer hung up. The memory-BIO transport never writes a socket
// itself: `conn_write` is `MSG_NOSIGNAL`, so a closed peer is an error code (section 3.2 of the document).
extern fn signal[&f](ffi: &f Ffi("tls"), sig: int, handler: int) -> [ffi("tls")] int;

extern fn BIO_s_mem[&f](ffi: &f Ffi("tls")) -> [ffi("tls")] int;

extern fn BIO_new[&f](ffi: &f Ffi("tls"), method: int) -> [ffi("tls")] int;

extern fn BIO_read[&f, &b](ffi: &f Ffi("tls"), bio: int, buf: &!b [byte]) -> [ffi("tls")] c_int;

extern fn BIO_write[&f, &b](ffi: &f Ffi("tls"), bio: int, buf: &b [byte]) -> [ffi("tls")] c_int;

extern fn ERR_clear_error[&f](ffi: &f Ffi("tls")) -> [ffi("tls")] int;

extern fn ERR_get_error[&f](ffi: &f Ffi("tls")) -> [ffi("tls")] int;

extern fn ERR_error_string_n[&f, &b](ffi: &f Ffi("tls"), code: int, buf: &!b [byte]) -> [ffi("tls")] int;

// `X509_VERIFY_PARAM_set1_host(param, name, namelen)`: a pointer and a length, which is how a
// byte slice crosses, so no NUL is needed. (`SSL_set1_host` wants a NUL-terminated string.)
extern fn X509_VERIFY_PARAM_set1_host[&f, &b](ffi: &f Ffi("tls"), param: int, name: &b [byte]) -> [ffi("tls")] c_int;

extern fn X509_VERIFY_PARAM_set_hostflags[&f](ffi: &f Ffi("tls"), param: int, flags: int) -> [ffi("tls")] int;

// ---------------------------------------------------------------------
// The numbers
// ---------------------------------------------------------------------

// Answers of a step.
pub fn done() -> [] int {
    return 0;
}

pub fn pending() -> [] int {
    return 1;
}

pub fn failed() -> [] int {
    return 2;
}

// Where an attempt failed (`stage_of`).
pub fn stage_none() -> [] int {
    return 0;
}

pub fn stage_connect() -> [] int {
    return 1;
}

pub fn stage_handshake() -> [] int {
    return 2;
}

pub fn stage_write() -> [] int {
    return 3;
}

pub fn stage_read() -> [] int {
    return 4;
}

pub fn stage_setup() -> [] int {
    return 5;
}

pub fn stride() -> [] int {
    return 10;
}

// Bytes of ciphertext a slot can hold that the kernel has not taken yet: one maximal TLS record (16,384 bytes of plaintext
// plus up to 325 of header, MAC and padding) with room to spare.
pub fn out_max() -> [] int {
    return 20480;
}

// Size of the one scratch buffer shared by every slot's socket reads.
pub fn net_max() -> [] int {
    return 16640;
}

// Per slot: [ssl, rbio, wbio, out_off, out_len, interest, stage, detail, fd (0 in BIO mode), -]
fn f_ssl() -> [] int {
    return 0;
}

fn f_rbio() -> [] int {
    return 1;
}

fn f_wbio() -> [] int {
    return 2;
}

fn f_off() -> [] int {
    return 3;
}

fn f_len() -> [] int {
    return 4;
}

fn f_interest() -> [] int {
    return 5;
}

fn f_stage() -> [] int {
    return 6;
}

fn f_detail() -> [] int {
    return 7;
}

pub fn stage_of[&a](tt: &a [int], slot: int) -> [] int {
    return tt[slot * 10 + 6];
}

// For a failed handshake: the `X509_V_ERR_*` number if verification failed (10 expired, 18 self-signed, 20 unknown issuer, 62 host
// name mismatch, ...), otherwise the first OpenSSL error on the queue. For a socket failure: the `errno`.
pub fn detail_of[&a](tt: &a [int], slot: int) -> [] int {
    return tt[slot * 10 + 7];
}

pub fn live[&a](tt: &a [int], slot: int) -> [] bool {
    return tt[slot * 10] != 0;
}

fn fail[&a](tt: &!a [int], slot: int, stage: int, detail: int) -> [] int {
    tt[slot * 10 + 6] = stage;
    tt[slot * 10 + 7] = detail;
    return failed();
}

// ---------------------------------------------------------------------
// The context: one per process, shared by every connection
// ---------------------------------------------------------------------

// Make the client context. `verify` turns on `SSL_VERIFY_PEER`. A trust store is loaded when it is on: the system's default
// locations if `default_paths`, and the PEM file `cafile` (NUL-terminated; length 0 for none) if there is one. Minimum protocol
// TLS 1.2. Partial writes are enabled, so `SSL_write` takes one record at a time and never waits for a retry with the same
// buffer. `release_buffers` sets `SSL_MODE_RELEASE_BUFFERS` (a connection's 16 KiB read and write buffers are freed while idle).
// Answers the context, or 0 (and no context is leaked).
pub fn context[&f, &c](ffi: &f Ffi("tls"), verify: bool, default_paths: bool, cafile: &c [byte], release_buffers: bool) -> [ffi("tls")] int {
    let method = TLS_client_method(ffi);
    if method == 0 {
        return 0;
    }
    let ctx = SSL_CTX_new(ffi, method);
    if ctx == 0 {
        return 0;
    }
    // SSL_CTRL_SET_MIN_PROTO_VERSION = 123, TLS1_2_VERSION = 0x0303.
    if SSL_CTX_ctrl(ffi, ctx, 123, 771, 0) != 1 {
        SSL_CTX_free(ffi, ctx);
        return 0;
    }
    // SSL_CTRL_MODE = 33; ENABLE_PARTIAL_WRITE 1, ACCEPT_MOVING_WRITE_BUFFER 2, RELEASE_BUFFERS 16.
    var mode = 3;
    if release_buffers {
        mode = 19;
    }
    SSL_CTX_ctrl(ffi, ctx, 33, mode, 0);
    if verify {
        SSL_CTX_set_verify(ffi, ctx, 1, 0);
        if default_paths {
            if SSL_CTX_set_default_verify_paths(ffi, ctx) != 1 {
                SSL_CTX_free(ffi, ctx);
                return 0;
            }
        }
        if len(cafile) > 0 {
            if SSL_CTX_load_verify_file(ffi, ctx, cafile) != 1 {
                SSL_CTX_free(ffi, ctx);
                return 0;
            }
        }
    }
    return ctx;
}

// ---------------------------------------------------------------------
// One connection
// ---------------------------------------------------------------------

// Start TLS on the connection in `slot` (the caller owns the socket): a new `SSL` over two memory BIOs, the server name for SNI
// and, if `verify`, for the certificate check. `host` is a DNS name without a NUL; it is what the certificate must name, and it
// is independent of the address the socket was connected to. Answers 0, or `failed()` with the stage set to `stage_setup()`
// (nothing is left allocated).
pub fn open[&f, &a, &h](ffi: &f Ffi("tls"), ctx: int, tt: &!a [int], slot: int, host: &h [byte], verify: bool, fd: int, session: int) -> [ffi("tls")] int {
    let b = slot * 10;
    if tt[b] != 0 {
        return fail(tt, slot, stage_setup(), 1);
    }
    ERR_clear_error(ffi);
    let ssl = SSL_new(ffi, ctx);
    if ssl == 0 {
        return fail(tt, slot, stage_setup(), 2);
    }
    var rbio = 0;
    var wbio = 0;
    if fd >= 0 {
        // The socket belongs to the caller's `Conn`; the `SSL` reads and writes it directly and never closes it.
        if SSL_set_fd(ffi, ssl, fd) != 1 {
            SSL_free(ffi, ssl);
            return fail(tt, slot, stage_setup(), 6);
        }
    } else {
        rbio = BIO_new(ffi, BIO_s_mem(ffi));
        wbio = BIO_new(ffi, BIO_s_mem(ffi));
        if rbio == 0 || wbio == 0 {
            // A BIO that was made and not handed to the SSL is not freed by `SSL_free`; there is no BIO_free here, and a
            // failed BIO_new means the process is out of memory, so this path is reported rather than recovered.
            SSL_free(ffi, ssl);
            return fail(tt, slot, stage_setup(), 3);
        }
        SSL_set_bio(ffi, ssl, rbio, wbio);
    }
    SSL_set_connect_state(ffi, ssl);
    if len(host) > 0 {
        var ok = true;
        region r {
            let name = alloc_slice[r](len(host) + 1, byte_of(0));
            var i = 0;
            while i < len(host) {
                name[i] = host[i];
                i = i + 1;
            }
            // SSL_CTRL_SET_TLSEXT_HOSTNAME = 55, TLSEXT_NAMETYPE_host_name = 0.
            if SSL_ctrl(ffi, ssl, 55, 0, name) != 1 {
                ok = false;
            }
        }
        if !ok {
            SSL_free(ffi, ssl);
            return fail(tt, slot, stage_setup(), 4);
        }
        if verify {
            let param = SSL_get0_param(ffi, ssl);
            // X509_CHECK_FLAG_NO_PARTIAL_WILDCARDS = 4.
            X509_VERIFY_PARAM_set_hostflags(ffi, param, 4);
            if X509_VERIFY_PARAM_set1_host(ffi, param, host) != 1 {
                SSL_free(ffi, ssl);
                return fail(tt, slot, stage_setup(), 5);
            }
        }
    }
    if session != 0 {
        // A session saved from an earlier connection to the same server: an abbreviated handshake if the server still accepts it.
        SSL_set_session(ffi, ssl, session);
    }
    tt[b] = ssl;
    tt[b + 1] = rbio;
    tt[b + 2] = wbio;
    tt[b + 3] = 0;
    tt[b + 4] = 0;
    tt[b + 5] = 0;
    tt[b + 6] = 0;
    tt[b + 7] = 0;
    tt[b + 8] = fd + 1;
    return 0;
}

// Free the `SSL` and both BIOs of `slot`. Safe on a slot that has none.
pub fn drop[&f, &a](ffi: &f Ffi("tls"), tt: &!a [int], slot: int) -> [ffi("tls")] int {
    let b = slot * 10;
    if tt[b] != 0 {
        SSL_free(ffi, tt[b]);
    }
    tt[b] = 0;
    tt[b + 1] = 0;
    tt[b + 2] = 0;
    tt[b + 3] = 0;
    tt[b + 4] = 0;
    tt[b + 5] = 0;
    return 0;
}

// Watch the connection for `events` (1 readable, 2 writable) if it is not already.
fn want[&t, &p, &a](tab: &!t conns.Table, poller: &!p Poller, tt: &!a [int], slot: int, token: int, events: int) -> [poll] int {
    if tt[slot * 10 + 5] == events {
        return 0;
    }
    tt[slot * 10 + 5] = events;
    return conns.rewatch(tab, poller, slot, token, events);
}

// Move ciphertext from the write BIO to the socket. 0: nothing is left; 1: the kernel would block (the rest is held in the slot);
// 2: the socket failed (detail = errno).
fn flush[&f, &t, &a, &o](ffi: &f Ffi("tls"), tab: &!t conns.Table, tt: &!a [int], out: &!o [byte], slot: int) -> [ffi("tls"), conn_write] int {
    let b = slot * 10;
    let base = slot * out_max();
    while true {
        if tt[b + 3] >= tt[b + 4] {
            let n = BIO_read(ffi, tt[b + 2], out[base..base + out_max()]);
            if n <= 0 {
                tt[b + 3] = 0;
                tt[b + 4] = 0;
                return 0;
            }
            tt[b + 3] = 0;
            tt[b + 4] = n;
        }
        match conns.write(tab, slot, out[base + tt[b + 3]..base + tt[b + 4]]) {
            Sent::Wrote(k) => {
                tt[b + 3] = tt[b + 3] + k;
            }
            Sent::Again => {
                return 1;
            }
            Sent::Failed(e) => {
                tt[b + 7] = e;
                return 2;
            }
        }
    }
    return 0;
}

// Move ciphertext from the socket to the read BIO. 1: some was fed; 0: nothing to read yet; 2: the peer closed; 3: the socket
// failed (detail = errno).
fn feed[&f, &t, &a, &s](ffi: &f Ffi("tls"), tab: &!t conns.Table, tt: &!a [int], net: &!s [byte], slot: int) -> [ffi("tls"), conn_read] int {
    match conns.read(tab, slot, net[0..net_max()]) {
        Received::Data(k) => {
            BIO_write(ffi, tt[slot * 10 + 1], net[0..k]);
            return 1;
        }
        Received::Again => {
            return 0;
        }
        Received::End => {
            return 2;
        }
        Received::Failed(e) => {
            tt[slot * 10 + 7] = e;
            return 3;
        }
    }
}

// The reason a handshake failed, as one number: the verification result if it is not OK, otherwise the first error on the queue.
fn why[&f, &a](ffi: &f Ffi("tls"), tt: &!a [int], slot: int) -> [ffi("tls")] int {
    let v = SSL_get_verify_result(ffi, tt[slot * 10]);
    let e = ERR_get_error(ffi);
    ERR_clear_error(ffi);
    if v != 0 {
        return v;
    }
    // OpenSSL 3 reports a peer that closed with no `close_notify` as an SSL error (library 20, reason 294,
    // `SSL_R_UNEXPECTED_EOF_WHILE_READING`) on the direct transport; the memory-BIO transport reports the same fact as -1 itself.
    // One code for one fact: -1, "the peer closed".
    if e / 8388608 % 256 == 20 && e % 8388608 == 294 {
        return 0 - 1;
    }
    return e;
}

// What a failed `SSL_*` call means for the step that made it, shared by every operation. `stage` is where we were.
fn ssl_failed[&f, &a](ffi: &f Ffi("tls"), tt: &!a [int], slot: int, stage: int, code: int) -> [ffi("tls")] int {
    if code == 5 {
        // SSL_ERROR_SYSCALL: with memory BIOs this is the peer closing without close_notify (the error queue is empty).
        ERR_clear_error(ffi);
        return fail(tt, slot, stage, 0 - 1);
    }
    return fail(tt, slot, stage, why(ffi, tt, slot));
}

// ---- the direct transport: OpenSSL reads and writes the socket itself (`SSL_set_fd`) ----

fn direct_handshake[&f, &t, &p, &a](ffi: &f Ffi("tls"), tab: &!t conns.Table, poller: &!p Poller, tt: &!a [int], slot: int, token: int) -> [ffi("tls"), poll] int {
    let b = slot * 10;
    ERR_clear_error(ffi);
    let r = SSL_do_handshake(ffi, tt[b]);
    if r == 1 {
        return done();
    }
    let e = SSL_get_error(ffi, tt[b], r);
    if e == 2 {
        want(tab, poller, tt, slot, token, 1);
        return pending();
    }
    if e == 3 {
        want(tab, poller, tt, slot, token, 2);
        return pending();
    }
    return ssl_failed(ffi, tt, slot, stage_handshake(), e);
}

fn direct_write[&f, &t, &p, &a, &d](ffi: &f Ffi("tls"), tab: &!t conns.Table, poller: &!p Poller, tt: &!a [int], slot: int, token: int, data: &d [byte]) -> [ffi("tls"), poll] int {
    let b = slot * 10;
    ERR_clear_error(ffi);
    let r = SSL_write(ffi, tt[b], data);
    if r > 0 {
        return r;
    }
    let e = SSL_get_error(ffi, tt[b], r);
    if e == 2 {
        want(tab, poller, tt, slot, token, 1);
        return 0 - 1;
    }
    if e == 3 {
        want(tab, poller, tt, slot, token, 2);
        return 0 - 1;
    }
    ssl_failed(ffi, tt, slot, stage_write(), e);
    return 0 - 2;
}

fn direct_read[&f, &t, &p, &a, &i](ffi: &f Ffi("tls"), tab: &!t conns.Table, poller: &!p Poller, tt: &!a [int], slot: int, token: int, into: &!i [byte]) -> [ffi("tls"), poll] int {
    let b = slot * 10;
    ERR_clear_error(ffi);
    let r = SSL_read(ffi, tt[b], into);
    if r > 0 {
        return r;
    }
    let e = SSL_get_error(ffi, tt[b], r);
    if e == 6 {
        return 0;
    }
    if e == 2 {
        want(tab, poller, tt, slot, token, 1);
        return 0 - 1;
    }
    if e == 3 {
        want(tab, poller, tt, slot, token, 2);
        return 0 - 1;
    }
    if e == 5 {
        ERR_clear_error(ffi);
        return 0 - 3;
    }
    ssl_failed(ffi, tt, slot, stage_read(), e);
    return 0 - 2;
}

// Run the handshake as far as it goes. `done()` (the session is established), `pending()` (the poller will say when; the slot
// is already watched for the direction it needs) or `failed()`.
pub fn handshake[&f, &t, &p, &a, &o, &s](ffi: &f Ffi("tls"), tab: &!t conns.Table, poller: &!p Poller, tt: &!a [int], out: &!o [byte], net: &!s [byte], slot: int, token: int) -> [ffi("tls"), conn_read, conn_write, poll] int {
    let b = slot * 10;
    if tt[b + 8] != 0 {
        return direct_handshake(ffi, tab, poller, tt, slot, token);
    }
    while true {
        let fl = flush(ffi, tab, tt, out, slot);
        if fl == 2 {
            return fail(tt, slot, stage_handshake(), tt[b + 7]);
        }
        if fl == 1 {
            want(tab, poller, tt, slot, token, 2);
            return pending();
        }
        ERR_clear_error(ffi);
        let r = SSL_do_handshake(ffi, tt[b]);
        if r == 1 {
            flush(ffi, tab, tt, out, slot);
            return done();
        }
        let e = SSL_get_error(ffi, tt[b], r);
        if e == 2 || e == 3 {
            // OpenSSL wants input (or, impossible with a memory BIO, room for output). What it has to say goes out first.
            let fl2 = flush(ffi, tab, tt, out, slot);
            if fl2 == 2 {
                return fail(tt, slot, stage_handshake(), tt[b + 7]);
            }
            if fl2 == 1 {
                want(tab, poller, tt, slot, token, 2);
                return pending();
            }
            let fd = feed(ffi, tab, tt, net, slot);
            if fd == 0 {
                want(tab, poller, tt, slot, token, 1);
                return pending();
            }
            if fd == 2 {
                return fail(tt, slot, stage_handshake(), 0 - 1);
            }
            if fd == 3 {
                return fail(tt, slot, stage_handshake(), tt[b + 7]);
            }
        } else {
            return ssl_failed(ffi, tt, slot, stage_handshake(), e);
        }
    }
    return pending();
}

// Encrypt and send up to one record of `data`. Answers the number of bytes of `data` accepted (more than 0), or a negative:
// -1 nothing yet (the poller will say when), -2 failed.
pub fn write[&f, &t, &p, &a, &o, &d](ffi: &f Ffi("tls"), tab: &!t conns.Table, poller: &!p Poller, tt: &!a [int], out: &!o [byte], slot: int, token: int, data: &d [byte]) -> [ffi("tls"), conn_write, poll] int {
    let b = slot * 10;
    if tt[b + 8] != 0 {
        return direct_write(ffi, tab, poller, tt, slot, token, data);
    }
    // One record at a time: the previous one must be on its way before the next is made.
    let fl = flush(ffi, tab, tt, out, slot);
    if fl == 2 {
        fail(tt, slot, stage_write(), tt[b + 7]);
        return 0 - 2;
    }
    if fl == 1 {
        want(tab, poller, tt, slot, token, 2);
        return 0 - 1;
    }
    ERR_clear_error(ffi);
    let r = SSL_write(ffi, tt[b], data);
    if r > 0 {
        let fl2 = flush(ffi, tab, tt, out, slot);
        if fl2 == 2 {
            fail(tt, slot, stage_write(), tt[b + 7]);
            return 0 - 2;
        }
        if fl2 == 1 {
            want(tab, poller, tt, slot, token, 2);
        }
        return r;
    }
    let e = SSL_get_error(ffi, tt[b], r);
    if e == 2 || e == 3 {
        // A renegotiation or a post-handshake message needs input first; with TLS 1.3 this is a NewSessionTicket and does not stop a write.
        want(tab, poller, tt, slot, token, 1);
        return 0 - 1;
    }
    ssl_failed(ffi, tt, slot, stage_write(), e);
    return 0 - 2;
}

// Decrypt up to `len(into)` bytes. Answers the count (more than 0); 0 for the peer's `close_notify`; -1 nothing yet (watched
// readable); -2 failed; -3 the peer closed the connection with no `close_notify` (a truncation, or a server that does not send one).
pub fn read[&f, &t, &p, &a, &o, &s, &i](ffi: &f Ffi("tls"), tab: &!t conns.Table, poller: &!p Poller, tt: &!a [int], out: &!o [byte], net: &!s [byte], slot: int, token: int, into: &!i [byte]) -> [ffi("tls"), conn_read, conn_write, poll] int {
    let b = slot * 10;
    if tt[b + 8] != 0 {
        return direct_read(ffi, tab, poller, tt, slot, token, into);
    }
    while true {
        ERR_clear_error(ffi);
        let r = SSL_read(ffi, tt[b], into);
        if r > 0 {
            return r;
        }
        let e = SSL_get_error(ffi, tt[b], r);
        if e == 6 {
            return 0;
        }
        if e == 2 || e == 3 {
            let fl = flush(ffi, tab, tt, out, slot);
            if fl == 2 {
                fail(tt, slot, stage_read(), tt[b + 7]);
                return 0 - 2;
            }
            if fl == 1 {
                want(tab, poller, tt, slot, token, 2);
                return 0 - 1;
            }
            let fd = feed(ffi, tab, tt, net, slot);
            if fd == 0 {
                want(tab, poller, tt, slot, token, 1);
                return 0 - 1;
            }
            if fd == 2 {
                return 0 - 3;
            }
            if fd == 3 {
                fail(tt, slot, stage_read(), tt[b + 7]);
                return 0 - 2;
            }
        } else {
            if e == 5 {
                ERR_clear_error(ffi);
                return 0 - 3;
            }
            ssl_failed(ffi, tt, slot, stage_read(), e);
            return 0 - 2;
        }
    }
    return 0 - 2;
}

// Send `close_notify` and push it to the socket as far as the kernel takes it now; does not wait for the peer's. The caller
// closes the connection after.
pub fn shutdown[&f, &t, &a, &o](ffi: &f Ffi("tls"), tab: &!t conns.Table, tt: &!a [int], out: &!o [byte], slot: int) -> [ffi("tls"), conn_write] int {
    ERR_clear_error(ffi);
    SSL_shutdown(ffi, tt[slot * 10]);
    ERR_clear_error(ffi);
    if tt[slot * 10 + 8] == 0 {
        flush(ffi, tab, tt, out, slot);
    }
    return 0;
}

// Describe the session of `slot` into `buf` (cipher name, protocol, key exchange, encryption: `SSL_CIPHER_description`'s line).
// Answers the protocol version number (`0x0304` for TLS 1.3), or 0 if there is none.
pub fn describe[&f, &a, &b](ffi: &f Ffi("tls"), tt: &a [int], slot: int, buf: &!b [byte]) -> [ffi("tls")] int {
    let cipher = SSL_get_current_cipher(ffi, tt[slot * 10]);
    if cipher == 0 {
        return 0;
    }
    SSL_CIPHER_description(ffi, cipher, buf);
    return SSL_version(ffi, tt[slot * 10]);
}

// Whether the handshake of `slot` resumed a session.
pub fn reused[&f, &a](ffi: &f Ffi("tls"), tt: &a [int], slot: int) -> [ffi("tls")] bool {
    return SSL_session_reused(ffi, tt[slot * 10]) == 1;
}

// The text of an OpenSSL error code, into `buf`.
pub fn error_text[&f, &b](ffi: &f Ffi("tls"), code: int, buf: &!b [byte]) -> [ffi("tls")] int {
    return ERR_error_string_n(ffi, code, buf);
}

// The descriptor of the connection in `slot` of a `std.conns.Table`, for `open`'s direct transport.
//
// **This reads a layout, not an interface.** A ticket is `epoch << 32 | descriptor` (`docs/native-sockets.md` section 10.3) and
// `Table.tickets` is a field of a `pub` struct, so the low 32 bits are the descriptor, and nothing needed `Ffi` to get them.
// `conn_raw_fd` (section 6 of that document: costs an `Ffi`, and is the only intended way) is designed and not built; this
// function is the one place that changes when it is. -1 for a slot that holds nothing.
pub fn fd_of[&t](table: &t conns.Table, slot: int) -> [] int {
    if slot < 0 || slot >= vec.size(table.tickets) {
        return 0 - 1;
    }
    let ticket = vec.get(table.tickets, slot);
    if ticket < 0 {
        return 0 - 1;
    }
    return ticket & 4294967295;
}

// A reference to the session of `slot`, to give `open` for a later connection to the same server, or 0 if it cannot be resumed (with
// TLS 1.3 the ticket arrives after the handshake, so ask after the first read). The caller owns the reference: `free_session`.
pub fn save_session[&f, &a](ffi: &f Ffi("tls"), tt: &a [int], slot: int) -> [ffi("tls")] int {
    let session = SSL_get1_session(ffi, tt[slot * 10]);
    if session == 0 {
        return 0;
    }
    if SSL_SESSION_is_resumable(ffi, session) != 1 {
        SSL_SESSION_free(ffi, session);
        return 0;
    }
    return session;
}

pub fn free_session[&f](ffi: &f Ffi("tls"), session: int) -> [ffi("tls")] int {
    if session != 0 {
        SSL_SESSION_free(ffi, session);
    }
    return 0;
}

// Make a write to a closed peer an error instead of a signal, for the whole process (the direct transport needs it: see `signal`).
pub fn ignore_sigpipe[&f](ffi: &f Ffi("tls")) -> [ffi("tls")] int {
    return signal(ffi, 13, 1);
}
