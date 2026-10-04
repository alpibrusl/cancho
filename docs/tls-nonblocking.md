# Non-blocking TLS: `https` delivery for a service with one thread

> **Status: spiked and measured; not built into any service.** `lexsys-hooks` (the webhook delivery service) delivers over plain
> TCP: one thread, one `Poller`, up to 64 attempts in flight as a state machine each (`docs/design.md` sections 16 and 26 there).
> This document is what it takes to make `https` endpoints possible, written as design (§3) and then checked by building the pieces
> in `examples/tls_nb/` and running them: a non-blocking TLS client driven by the `Poller` (§4), certificate verification with the
> failures told apart (§5), the authority it costs (§6), a name resolver that does not stop the loop and a pinned address (§7), the
> numbers (§8), and every compiler and runtime gap met on the way, each with a reproducer (§9). §10 is what `lexsys-hooks` would
> change, in slices with gates. Nothing here is claimed that was not run; §11 lists what was not.
>
> **The user chose OpenSSL through FFI (a system C library) over a pure lex-sys TLS implementation, which is a research project.**
> §3.1 says what the second would be and why it is not attempted here. Everything below is the first.

---

## 1. The question, and the constraints that make it hard

`examples/tls_client/` (`opaque-pointers.md`, `foreign-linking.md`) is a real TLS handshake and an encrypted round trip against
real OpenSSL. It is also **blocking** (`SSL_connect` on a blocking descriptor waits for the peer), **unverified** (it never calls
`SSL_CTX_set_verify`, so it trusts whoever answers), and a single connection. `lexsys-hooks` needs the opposite on all three:

* **One thread.** The ingest server, the log, the retry schedule and every delivery attempt share one loop. An attempt that waits
  is the whole service waiting (`design.md` section 15 measured a request held for 10 s by one receiver). TLS adds three places to
  wait that plain TCP does not have: the handshake (one or two round trips, and the peer's CPU), the name lookup that `https`
  endpoints are given by (certificates name hosts, not addresses), and the OpenSSL calls themselves, which must be driven in steps.
* **Many at once.** Up to 64 attempts, each in its own state, none blocking another.
* **The check is the point.** A client that does not verify the certificate is not TLS, it is encryption to a stranger. Verification
  has to say *why* it failed in a form a retry schedule and an operator can use (an expired certificate is not a wrong host name).
* **The authority report is a product feature.** `lexsys-hooks` says "no `Ffi`, no `unsafe`; the authority report names what the
  program can do". Whatever is added has to say what it costs there (§6).

## 2. What existed, and what running it found

`examples/tls_client/tls_client.ls` declares eleven OpenSSL functions with `c_ptr` handles, links with `-l ssl -l crypto`, and talks to
`openssl s_server -rev`. Run again for this document it still works (`sys-xel olleh`). Two things in it were wrong, found by pointing it
at something other than a friendly server:

1. **A C `int` result declared `int` is read wrongly.** `SSL_connect`, `SSL_write`, `SSL_read`, `SSL_set_fd` and `SSL_shutdown` were declared
   `-> int`. The upper half of `rax` after a C function returns an `int` is not defined by the ABI, and here a failing call (which returns
   -1) was read as **4294967295** on both backends (`examples/tls_nb/gaps/g13_int_vs_c_int.ls`: `close(-1)` declared `int` prints
   `4294967295`, declared `c_int` prints `-1`). So `if SSL_connect(...) <= 0` was false exactly when it mattered, and a handshake that
   failed was treated as one that worked. **Corrected in place**: the five declarations are now `c_int`, which sign-extends once at the
   boundary (`reach.md` section 3.4's purpose).
2. **OpenSSL's own socket BIO kills the process.** With the first fixed, the example, run against a server that accepts and closes, still
   dies: `strace` shows OpenSSL answering the end of file with a fatal alert written with plain `write(2)`, `EPIPE`, `SIGPIPE`, and no
   message (`killed by SIGPIPE`, exit status -13). A blocking client run once does not care; a service that dials strangers does (§3.2,
   decision D1). **Not changed in the example**, which is a one-connection demonstration; `tls_client.ls`'s header says what it does not do.

## 3. Decisions, with the alternatives

### 3.1 D0: OpenSSL through FFI, not a TLS written in lex-sys

The decision is the user's; this records what the alternative is so the choice is a choice. A TLS 1.2/1.3 client that a service can
trust is not a protocol parser. It is the record layer and handshake state machines, a key exchange (X25519 or ECDH over P-256), an AEAD
(AES-GCM, with hardware or constant-time software, or ChaCha20-Poly1305), HMAC and HKDF, signature verification for the algorithms real
certificates use (ECDSA P-256/P-384 and RSA; Ed25519 is rare in public PKI), X.509 DER parsing, path building and validation (validity
dates, basic constraints, name constraints, key usage), host-name matching (SAN, wildcards, the rules about the CN), a trust store, and a
source of randomness. `std` has SHA-256, SHA-512 and Ed25519 (`crypto.md`, `sha512.md`, `ed25519.md`: `pub fn sha256`, `sha512`,
`public_key_from_seed`, `sign`, `verify`) and no random source, no AES, no key exchange, no HMAC, no X.509; `crypto.md` section 1 says each
of those "is a separate, harder correctness problem with its own attack surface", and `ed25519.md` answers "no" to a constant-time
posture. Every one of them is security-critical and every one is invisible when wrong. That is a research project with its own
verification story (test vectors, differential tests against OpenSSL, a timing-side-channel review). It is the right long-term direction
for `native-sockets.md` section 7's "no C in the path"; it is not what delivers `https` webhooks. **Not attempted.**

The cost of the choice, stated once because §6 and §9 keep returning to it: a system C library behind `Ffi` is **an authority the checker
cannot bound** (`lex-sys authority` prints `UNBOUNDED`), its handles are integers here (§3.3), and everything it does that the language
guards against (writing to sockets itself, a `SIGPIPE`, memory it owns) is back.

### 3.2 D1: how the connection reaches OpenSSL: memory BIOs, not a descriptor

| | memory BIOs (`BIO_s_mem` x2, `SSL_set_bio`) | the descriptor (`SSL_set_fd`) |
|---|---|---|
| available today | **yes**: the program reads and writes the socket as `conn_read`/`conn_write` and hands bytes to OpenSSL | **no `conn_raw_fd`**: designed in `native-sockets.md` section 6, listed there as "not yet built". The descriptor is reachable only through a layout leak (below) |
| authority | the socket stays a `Conn`; only bytes cross | the program holds a raw descriptor number |
| closed peer | `conn_write` is `MSG_NOSIGNAL`: an error code | OpenSSL's socket BIO calls `write(2)`: **`SIGPIPE` kills the process** (§2, `test/verify_matrix.py`); the fix is `signal(SIGPIPE, SIG_IGN)` for the whole process, which is what `native-sockets.md` section 3 set out to stop needing |
| cost | one 16 KiB read buffer shared by every slot, a 20 KiB ciphertext buffer per slot for what the kernel has not taken yet, two copies of the bytes | none of those |
| measured CPU per handshake | within noise of the other (§8.1) | |

**How the descriptor can be had without `conn_raw_fd`**, found by trying: a `std.conns.Table` holds *tickets*, a ticket is
`epoch << 32 | descriptor` (`native-sockets.md` section 10.3), and `Table.tickets` is a field of a `pub` struct, so
`vec.get(table.tickets, slot) & 0xffffffff` is the descriptor, **and no `Ffi` was needed to read it** (`gaps/g5_table_ticket.ls`;
`tls.fd_of`). That is a reading of a layout and not an interface, and it falsifies one sentence of `native-sockets.md` section 6 ("a
program without `Ffi` cannot reach the hatch"; corrected there in place). The number alone is no authority (calling anything on it needs
`Ffi`), but it is a reason not to build on it.

**Decision:** memory BIOs. It is the only transport that works with today's compiler and today's `std.conns` without a signal
disposition, and the numbers say it costs nothing measurable. `tls.ls` carries both (`open`'s `fd` argument), because the second is what a
`conn_raw_fd` slice would switch to, and the comparison (§8.1) and the `SIGPIPE` row of the test are the evidence either way. It is the
default; `io=1` in the client selects the other.

### 3.3 D2: handles are integers

An `SSL *` is a pointer returned in a register. `c_ptr` is the honest type and cannot be used: **it cannot be named in an ordinary
function's signature or a struct field** (`gaps/g1_cptr_param.ls`, `g2_cptr_field.ls`: `unknown type c_ptr`), so a helper could not take
or return one, and a table of connections could not hold them. (A `[c_ptr]` slice and a generic parameter instantiated at `c_ptr` both
compile and run, `g3`, `g4`, but a generic body cannot *call* anything with its `T`, so they store a handle and nothing more.) Declaring
the extern with `int` for a pointer result carries all 64 bits and works on both backends. The price is the forgeability `c_ptr` exists
to prevent: `SSL_free(ffi, 12345)` type-checks. Under `Ffi` the program can already call anything in the library, so the extra
guarantee `c_ptr` would add is small, but it is a guarantee given up, and §9 asks for `c_ptr` in signatures.

### 3.4 D3: one context, one `SSL` per connection, a step function per operation

`tls.context` makes **one** `SSL_CTX` for the process: minimum protocol TLS 1.2 (`SSL_CTX_ctrl(ctx, SSL_CTRL_SET_MIN_PROTO_VERSION, 0x0303)`:
`SSL_CTX_set_min_proto_version` is a macro), `SSL_MODE_ENABLE_PARTIAL_WRITE | ACCEPT_MOVING_WRITE_BUFFER` (so `SSL_write` takes one record at
a time and never insists on being retried with the same buffer: the program's request buffer does not move, but the contract stays
simple), `SSL_CTX_set_verify(SSL_VERIFY_PEER)` and the trust store. Loading the system store costs about 25 ms (§8.1): once per process, never per
attempt. `tls.open` makes an `SSL` over two memory BIOs for a slot; `handshake`, `write`, `read` and `shutdown` each run **as far as they go
without waiting** and answer `done`, `pending` (the slot is already watched for the direction OpenSSL needs) or `failed`, in the same shape as
`attempt.advance` (`design.md` section 16). `ERR_clear_error` is called before each SSL operation: the error queue is per thread, and one
thread serves every connection, so a stale error from one connection would otherwise be read as another's.

### 3.5 D4: what verification is, in calls

| what | call | notes |
|---|---|---|
| verify the peer | `SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, 0)` | the callback argument is a function pointer: `0` |
| trust store, system | `SSL_CTX_set_default_verify_paths(ctx)` | honours `SSL_CERT_FILE` and `SSL_CERT_DIR` (tested) |
| trust store, a file | `SSL_CTX_load_verify_file(ctx, path)` | OpenSSL 3.0's one-string form; `SSL_CTX_load_verify_locations` has two strings and **cannot be declared** (§9, gap 4) |
| host name | `X509_VERIFY_PARAM_set1_host(SSL_get0_param(ssl), name, len)`, flag `X509_CHECK_FLAG_NO_PARTIAL_WILDCARDS` | takes a pointer and a length, which is how a byte slice crosses: no NUL needed. `SSL_set1_host` wants a C string |
| SNI | `SSL_ctrl(ssl, SSL_CTRL_SET_TLSEXT_HOSTNAME = 55, TLSEXT_NAMETYPE_host_name = 0, name)` | `SSL_set_tlsext_host_name` is a macro over `SSL_ctrl`; the name is a NUL-terminated `void *` in the last parameter, so a slice with a NUL after it |
| minimum protocol | `SSL_CTX_ctrl(ctx, SSL_CTRL_SET_MIN_PROTO_VERSION = 123, TLS1_2_VERSION = 0x0303, 0)` | |
| why it failed | `SSL_get_verify_result(ssl)` after a failed handshake | an `X509_V_ERR_*` number |

Revocation (CRL, OCSP, stapling) is **out of scope** and not attempted: OpenSSL does not check revocation unless asked and asking needs a
CRL source. A revoked certificate that is otherwise valid is accepted (§11).

### 3.6 D5: every outcome is a code (stage, detail)

An attempt ends as `(stage, detail)`: stage 0 is success; 1 connect (detail the `errno`); 2 handshake (detail the `X509_V_ERR_*` number if
verification failed, otherwise the first OpenSSL error code, or -1 if the peer closed, a code both transports produce for the same fact);
3 write; 4 read; 5 setup; 6 name lookup (detail a `dns.ls`/`rtcp.ls` code); 7 destination refused (detail the packed address that was
refused); 102 deadline. An OpenSSL error code is `library << 23 | reason` in 3.0 (`0x0A00042E` is library 20, reason 1070: an alert received
from the peer, `1000 + 70`, `protocol_version`). Nothing panics and every connection is closed on every path (§5).

### 3.7 D6: the name is looked up by the program, on the poller

`tcp_connect_start` calls `getaddrinfo`, which blocks, and a name it resolves can differ from the one a check judged (`design.md` section 26 is why
`lexsys-hooks` accepts only IPv4 literals). Three ways to have names:

| | stops the loop | needs | what it gives up |
|---|---|---|---|
| `getaddrinfo` as today | **yes, for the whole lookup** (302 ms for a 300 ms answer, §7) | nothing | the address is not known before the connection: no check, no pin |
| a thread (`spawn`) running libc's `res_query`, answers through a loopback `Conn` | no (3 ms gap) | `Ffi`, a fixed port, four workers named in the text (§7, gap 9) | `/etc/hosts`, search list; needs a thread per pool slot written out |
| **a resolver of our own: DNS over TCP, a state machine on the same poller** (`rtcp.ls`) | **no** (2 ms gap) | the name server's address; a `Table` and buffers | `/etc/hosts`, search list, UDP, trying a second server |

**Decision:** the resolver on the poller. It needs nothing the language lacks, adds no thread and no `Ffi`, fits the attempt's deadline, and its
parser is pure lex-sys (`dns.ls`, tested on a million damaged answers, §7). The thread version was built because the question asked for it; its
cost in the language (§9) is larger than its benefit.

### 3.8 D7: connect to the address that was checked

The destination is resolved, **every** address in the answer is judged against the ranges of `destination.is_public` (`pin.ls`, a copy checked
against the original), one is chosen, and the connection is made to it **as an IP literal** (no second lookup), with the name used afterwards
only for SNI and the certificate. A check and a connection that each resolve are two lookups and a name can answer differently to the second
(rebinding); here there is one and the connection goes to its answer (§7 shows it by counting queries). The rule is strict: if any address is
private the delivery is refused, not "connect to the first public one" (a name that has a private address in its answer is one resolver
change from being sent there).

## 4. What was built

All in `examples/tls_nb/`, compiled with the checked-in compiler:

```sh
lex-sys build tls.ls dns.ls rtcp.ls pin.ls nat.ls tls_nb.ls --std -l ssl -l crypto -o tls_nb        # the TLS client (scope Ffi("tls"))
lex-sys build dns.ls rtcp.ls rthread.ls nat.ls resolve_demo.ls ../../packages/net-sockets/sockets.ls ../../packages/net-connect/connect.ls --std -o resolve_demo   # the three resolvers (scope Ffi("libc"))
```


| file | what | lines |
|---|---|---|
| `tls.ls` | the engine: the OpenSSL declarations, `context`, `open`, `handshake`/`write`/`read`/`shutdown`, `describe`, session save/restore, both transports | 750 |
| `tls_nb.ls` | the client: up to 4,096 connections in flight on one `Poller`, each a state machine (lookup, connect, handshake, requests, close), options for verification, resumption, I/O mode, a name server | 822 |
| `dns.ls` | pure: `build_query`, `parse` (total on any bytes), `put_dotted` | 255 |
| `rtcp.ls` | the resolver on the poller: `open`, `start`, `advance`, `finish`, `close`, a `Resolver` that owns its table and buffers | 304 |
| `pin.ls`, `pin_check.ls` | `is_public` (the ranges of `lexsys-hooks`' `destination.ls`), `choose` (every address must pass); and the check of the copy against the original | 83, 128 |
| `nat.ls` | `nat.parse`, the argument reader the two programs here share | 25 |
| `rthread.ls` | the thread resolver: the worker body (`res_query`) and the loopback channel, through the repository's `net.sockets` and `net.connect` | 168 |
| `resolve_demo.ls` | three resolvers behind one loop, to measure what each does to it | 498 |
| `dns_fuzz.ls` | `dns.parse` on a known answer, the errors, and a million damaged answers | 185 |
| `gaps/` | 23 reproducers (§9) and `check_gaps.py`, which runs them all (two are kept as `.ls.txt`, see below) | |
| `test/` | `certs.sh`, `server.py` (a TLS receiver), `dns_stub.py`, `harness.py`, and the tests and measurements below | |

The repository checks its `examples/` tree, and three of those checks shaped this directory: no function body may be duplicated across files (`parse_nat` and a copy loop were; `nat.ls` and a differently-written
loop are why they no longer are), every `.ls` must be formatted by `lex-sys fmt` (so the two reproducers the formatter cannot read, `t2_ref_field` and `g11_connect_is_a_builtin`, are `.ls.txt`, which `check_gaps.py` copies to
`.ls` to run), and `extern fn connect` may be declared in `packages/net-connect/connect.ls` alone (`net::the_network_programs_are_counted`): the thread resolver's first version declared its own in an edition-1 file, and now
uses the package, which is why the resolver demo's scope is `Ffi("libc")` and not `Ffi("tls")`, and why it is a program of its own (gap 10).

### 4.1 The state machine

```
connecting --(writable, SO_ERROR 0)--> handshake --(SSL_do_handshake = 1)--> sending --(request out)--> reading --(status line)--> closed
    |                                       |                                    |                           |
 errno                               WANT_READ: flush what OpenSSL queued,     one record at a time:       SSL_read until the whole
                                     feed what the socket has; none: watch     flush, SSL_write(partial)   response (or the first 12 bytes,
                                     readable. Failure: stage 2, X509 number   watch writable while the    as `attempt.advance` reads them)
                                                                               kernel is full
```

One `Poller`, level-triggered, a token per slot; a slot is watched **writable only while ciphertext waits for the kernel** and readable
otherwise (a connection watched writable with nothing to write would spin). `SSL_ERROR_WANT_WRITE` cannot occur with a memory BIO and
is handled as "flush first". `SSL_shutdown` sends `close_notify` and does not wait for the peer's: the attempt is over when the status
line is in. A TLS 1.3 server sends `NewSessionTicket` messages after the handshake; they are read (and dropped, or kept: §10.4) by the
first `SSL_read`, so a connection that sends nothing and waits does not spin on unread input (`hold` reads them).

### 4.2 The API a consumer sees

What `lexsys-hooks` would call, in the shape `attempt.ls` already has (functions over arrays and a slot number, the `Conn` in a `std.conns` table, the token the caller registered):

```
// once, in main: 0 if the context cannot be made (a trust store that will not load): refuse to start
let ctx = tls.context(f, verify, default_paths, cafile /* NUL-terminated, length 0 for none */, release_buffers);

// per attempt, slot s; tt, out and net are the caller's arrays: tls.stride() integers a slot, tls.out_max() bytes a slot, one tls.net_max() scratch
tls.open(f, ctx, tt, s, host /* the name the certificate must carry */, verify, fd /* -1: memory BIOs */, session /* 0, or a saved one */)  -> 0 | failed (stage 5)
tls.handshake(f, tab, poller, tt, out, net, s, token)             -> done | pending | failed
tls.write(f, tab, poller, tt, out, s, token, data)                -> n > 0 bytes taken (one record) | -1 pending | -2 failed
tls.read(f, tab, poller, tt, out, net, s, token, into)            -> n > 0 | 0 close_notify | -1 pending | -2 failed | -3 closed without it
tls.shutdown(f, tab, tt, out, s);  tls.drop(f, tt, s)             // close_notify, best effort; free the SSL and its BIOs (safe on a slot with none)
tls.stage_of(tt, s), tls.detail_of(tt, s)                         // where it failed and why (§3.6)
tls.save_session(f, tt, s) -> int;  tls.free_session(f, session)  // resumption (§10.4)

rtcp.open(heap) -> Resolver;  rtcp.start(heap, rs, net, poller, ns_ip, ns_port, name, token0, lookup, query_id, deadline) -> (Resolver, slot, code)
rtcp.advance(rs, poller, slot, token0) -> pending | count | a negative code;  rtcp.count_of/addr_of/ttl_of(rs, slot, ...);  rtcp.finish(rs, slot);  rtcp.close(heap, rs)
pin.choose(addrs, count, allow_private) -> index | -1;  pin.first_refused(addrs, count)
```

Every function takes the `&f Ffi("tls")` it needs and says `ffi("tls")` in its row; `rtcp`, `pin` and `dns` need none. Two things a package should change that the spike does not: `tls` takes the caller's arrays because the
spike's driver keeps its per-slot state in its own, where a package would own them the way `rtcp.Resolver` owns its table and buffers (`Table` and `Box` fields, accessors, one `open`/`close`); and the integer handles of §3.3
stay inside it, so that when `c_ptr` can be named (gap 1) only `tls.ls` changes.

## 5. Certificates: every outcome, against a real server

`test/certs.sh` makes the certificates with the `openssl` CLI (a CA, leaves for `hooks.test`, an intermediate, another PKI); `test/server.py` is an
asyncio TLS receiver that serves any of them. Run: `python3 examples/tls_nb/test/verify_matrix.py`, exit 0 only if every row came out as expected,
on **both** transports. The client is given the CA file `ca.pem` unless the row says otherwise and expects the name `hooks.test`.

| the server presents | the client reports (stage, detail) | meaning |
|---|---|---|
| a valid certificate (ECDSA P-256, and RSA 2048) | 0, 0, status 200 | verified against the name, request answered |
| a certificate for `other.test` | 2, **62** | `X509_V_ERR_HOSTNAME_MISMATCH` |
| expired (2020-01-02) | 2, **10** | `CERT_HAS_EXPIRED` |
| not yet valid (2090) | 2, **9** | `CERT_NOT_YET_VALID` |
| self-signed, not in the trust store | 2, **18** | `DEPTH_ZERO_SELF_SIGNED_CERT` |
| a leaf, an intermediate and a root of **another PKI**, all sent | 2, **19** | `SELF_SIGNED_CERT_IN_CHAIN` |
| a leaf under an intermediate of the trusted CA, the intermediate **not sent** | 2, **20** | `UNABLE_TO_GET_ISSUER_CERT_LOCALLY` |
| the CA file holds the wrong CA; no trust store at all; the system store only | 2, **20** | same: nothing that issued it is trusted |
| a server that offers only TLS 1.1 (`openssl s_server -max_protocol TLSv1.1`) | 2, 167773230 (**0x0A00042E**) | the peer's `protocol_version` alert: the minimum protocol holds |
| a server that closes without an alert | 2, **-1** | the peer closed |
| nothing listening | 1, **111** | `ECONNREFUSED`, a connect failure, not a TLS one |

Controls, each one showing that a failure above it was the check and not the setup: the same wrong-host certificate with the client
expecting `other.test` succeeds; the self-signed server succeeds with verification off; the intermediate, once sent, verifies; `SSL_CERT_FILE`
pointed at `ca.pem` makes the system-store call succeed. A `SIGPIPE` row: a peer that closes before the handshake is reported (stage 2) by the
memory-BIO transport, **kills the process** on `io=1` unless `sigpipe=ignore` is given, and is reported on `io=1` with it (the three rows are tested,
§3.2). **Revoked** is out of scope (§3.5, §11).

Nothing leaks, on any path (`test/leak_test.py`, exit 0 only if every check holds): 10,000 connections through 64 slots under `ulimit -n 200` (a leaked descriptor per
connection would stop it at connection 136) for success, an expired certificate and a server that never answers, on both transports; **the bytes malloc still has outstanding when the client exits** (an `LD_PRELOAD` shim, `test/shim_heap.c`, `mallinfo2`) **rise over the
first few thousand connections to 244,992 bytes, the same number in every configuration, and stay there**: 177,664 after 500 connections, 221,776 after 2,000, 244,992 after 5,000, 10,000 and 20,000
(an expired certificate on the descriptor transport; something in OpenSSL fills to a bound, and what it is was not found). The test compares 10,000 with 40,000 connections, for success and for an expired
certificate on **both** transports (a leak of one byte a connection would put them 30,000 bytes apart): **244,992 and 244,992 in all four**; and `valgrind --leak-check=full` reports nothing on 40
connections through 16 slots, success and failure. (Peak resident size is **not** a usable detector and was tried first: it grew by 40 bytes a connection, which is the client's own
record of each connection (`tls_nb` keeps one for the report), not a leak.) One path is *not* covered: `tls.open` frees the `SSL` but not a BIO if the second `BIO_new` fails (an out-of-memory
condition; there is no `BIO_free` declared and the process is about to fail anyway: stated in the source).

**Partial and refused I/O** (`test/partial_io_test.py`; an `LD_PRELOAD` shim, `test/shim_io.c`, on `send` and `recv`). An ordinary run on loopback never reaches the branches of `tls.ls` that handle a write the
kernel takes in pieces, a write that has to wait, a TLS record that arrives in pieces and a read with nothing yet (`lexsys-hooks`' own partial-write branch has never run in a test either, `design.md` section 16).
The shim makes `send` take at most 700 bytes and `recv` return at most 300, and every third call of either fail with `EAGAIN`: **64 connections, each sending three 60,000-byte POSTs over one session, all succeed with
the bytes intact** (the receiver reads exactly `Content-Length`, and a lost or reordered byte is a MAC failure, not a quiet pass); 500 handshakes the same way; and a harsher setting (64-byte sends, 17-byte reads, every second
call refused). Three mutants of `tls.ls` against it: a write that would block counts as flushed, and a partial write counts as whole, are both **killed by the shim run** (every connection runs to its deadline) and
**invisible without it** (they pass the ordinary run); a read with nothing yet counts as data **survives the shim run** (it spins, and the data arrives) and is **killed by the silent-peer check** in the same file: 64 connections
to a peer that never answers must all end at the deadline, in 1.0 s, and with that mutant the loop never returns to the poller and the test's timeout kills the process.

## 6. Authority: what `Ffi` scope this needs, and what the report then says

**Which scope.** The scope in `Ffi("...")` is a label. Nothing connects it to the library a symbol lives in (`gaps/a2_scope_is_nominal.ls`
declares libc's `system` under `Ffi("openssl")`, and it type-checks, builds and runs), and `narrow` consumes the one `Ffi` that `split`
hands out, so **one program has one scope** (`gaps/g12_scopes_do_not_compose.ls`: a second `narrow` is `linear-use-after-move`, and a function
that takes both an `Ffi("libc")` and an `Ffi("tls")` can never be called). OpenSSL is two libraries (`libssl` for `SSL_*`, `libcrypto` for
`BIO_*`, `ERR_*`, `X509_*`), so the scope cannot be "the library"; `tls.ls` names it by **purpose**, `Ffi("tls")`: the C symbols the TLS code
needs. Two consequences: the packages that hard-code `Ffi("libc")` (`net.sockets`, `net.connect`) cannot be mixed with it (`lexsys-hooks`
does not use them: its sockets are `Conn`s), and the name promises nothing about what a function holding it can call.

**How a program like `lexsys-hooks` holds it narrowly.** Today `main` says `release(ffi)`. It would say `let tls = narrow(ffi, "tls")` and lend
`&f Ffi("tls")` down the one chain that needs it: `main` -> `run` -> `delivery_turn` -> `start_attempts`/`start_one`/`start_replay` ->
`attempt.begin`/`advance`/`finish`. Each function on the chain takes the capability and declares `ffi("tls")` in its row, and the checker holds
the row **exact in both directions** (`gaps/a3_row_exact.ls`: a caller of a function that performs `ffi("tls")`, with a row that does not say so,
is `effect-not-declared`; and `tls_nb.ls` has no function whose row names it without using it). A function that is not handed the capability
cannot reach OpenSSL, and that is visible in its signature. `main` itself stays `[]` (owning discharges, as for `Io`: `tls_nb.ls`'s `main` owns
and borrows it and is `-> [] int`). With the thread resolver `main` would also need `conc` (§7); with the resolver on the poller it does not.

**What the report says.** `lex-sys authority examples/tls_nb/*.ls --std`:

```
UNBOUNDED: this program calls foreign code, and a library is
not an authority domain -- the labels below do not bound what
it can reach. See docs/under-a-grant.md.
performs
    args, clock, conn_read, conn_write, err_write, ffi("tls") <- unbounded, heap, io_write, net_out(""), poll
foreign symbols (37)
    BIO_new  BIO_read  BIO_s_mem  BIO_write  ERR_clear_error  ERR_get_error  SSL_CIPHER_description  SSL_CTX_ctrl  SSL_CTX_free
    SSL_CTX_load_verify_file  SSL_CTX_new  SSL_CTX_set_default_verify_paths  SSL_CTX_set_verify  SSL_SESSION_free
    SSL_SESSION_is_resumable  SSL_ctrl  SSL_do_handshake  SSL_free  SSL_get0_param  SSL_get1_session  SSL_get_current_cipher
    SSL_get_error  SSL_get_verify_result  SSL_new  SSL_read  SSL_session_reused  SSL_set_bio  SSL_set_connect_state  SSL_set_fd
    SSL_set_session  SSL_shutdown  SSL_version  SSL_write  TLS_client_method  X509_VERIFY_PARAM_set1_host
    X509_VERIFY_PARAM_set_hostflags  signal
```

So the authority `lexsys-hooks` has today, which names every label and no foreign symbol (its `main` does `release(ffi)`, `src/hooks.ls:2481`, and its
README says so), becomes `UNBOUNDED`, with a list of foreign symbols a reviewer can read in a minute. (This was read from its source and README; `lex-sys authority` was
not run on `lexsys-hooks` itself, whose dependencies are fetched by `lex-sys build`.) That is a real change to what `lexsys-hooks`' README says ("No `Ffi`,
no `unsafe`") and to what a supervisor can check against a grant (`under-a-grant.md`: it "is unable to refuse"). The type checker allows the row
to say `ffi("tls")` precisely; it cannot make the report mean anything about what the symbols do. The alternative that keeps `lexsys-hooks`' report
exact is to put TLS in a **separate program** (a loopback proxy: it holds `Ffi("tls")` and `Net`, `lexsys-hooks` dials it in plain TCP and says
where to go). That costs a hop and a protocol, and it moves the unbounded row to a program that does nothing else. Not built, not measured; it is
the answer if the report matters more than the hop.

## 7. Names: resolve without stopping the loop, check, pin

`resolve_demo` runs a poller loop (a wait of 1 ms and a tick counter) and, after 50 ms, starts lookups against a name server that answers one
name after 300 ms (`test/dns_stub.py`). "Longest gap" is the longest time between two iterations of the loop: a loop that waits for DNS has a gap as
long as the wait. `python3 examples/tls_nb/test/resolve_test.py` runs all of it against the three resolvers, **checks** the numbers below and
exits 0 only if they hold. The two libc resolvers read `/etc/resolv.conf`, so they run in a private mount namespace in which that file names
127.0.0.1 (`test/with_resolver.sh`, `unshare -m`: the host's file is not touched), where a stub on port 53 listens.

| five lookups at once, one of them slow (300 ms) | the loop's longest gap | whole run | the fast lookups |
|---|---|---|---|
| `blocking`: `tcp_connect_start(name)`, `getaddrinfo` inside, **what hooks has** | **302 ms** | 412 ms | 0-1 ms each, but only after the loop returned from the slow one |
| `thread`: four workers, `res_query`, answers through loopback `Conn`s | 2 ms | 351 ms | 1-2 ms |
| `tcp`: `rtcp.ls`, DNS over TCP on the poller | 2 ms | 351 ms | 0-2 ms |

| eight slow (300 ms) lookups at once | whole run | the loop's longest gap |
|---|---|---|
| `blocking` (one after another) | 2,519 ms | 304 ms |
| `thread` (four workers: two waves) | 653 ms | 3 ms |
| `tcp` (all eight at once, `rtcp.slots()` is 16) | 352 ms | 2 ms |

**The parser is checked, not trusted** (`dns_fuzz.ls`, `lex-sys run dns.ls dns_fuzz.ls --std`, exit 0): a known answer (a CNAME and two A records),
the error codes it must name (wrong id, too short, cut inside an answer, a query where a response is expected, the TC bit, NXDOMAIN), the query
builder on empty names, empty labels and a 64-byte label, then **1,000,000** damaged answers (600,000 with 1 to 4 random bytes replaced and a random cut, 400,000 aimed
at the header and first record, 200,000 random strings), each passed as a slice **exactly as long as the message** so a read past its end is a bounds
trap and not a quiet read of the buffer's tail: none traps (0.23 s). Four mutants of `dns.ls`: an A record accepted whatever its data length, and the
record's data length not checked against the message: both **killed** (illegal instruction); `byte_at` without the check against `len(msg)`, and the record
header's length check dropped: **survive, and are equivalent**, because every read of those bytes goes through a second check (`u16_at` calls `byte_at`).

**The copy of the range test is checked against its original** (`test/pin_test.py <lexsys-hooks checkout>`: `pin.is_public` against `lexsys-hooks`' `destination.is_public` on both edges of every
range and their neighbours, 17 addresses for each first octet and 2,000,000 pseudo-random ones, **2,004,434 addresses, 0 disagreements**). Four mutants of `pin.ls` (169.254/16 moved by one, 172.16/12
short of 31, 224/4 short of 224, 100.64/10 short of 127) each produce disagreements (66, 2, 7,827 and 2 of them): all four killed.

**The pinned delivery** (`python3 examples/tls_nb/test/pinned_test.py`, exit 0 only if every check holds; `tls_nb` with `ns=<ip>` resolves its first
argument as a name over DNS/TCP, judges **every** address with `pin.choose`, and dials the one chosen as an IP literal):

* 16 deliveries to `hooks.test` all succeed with the certificate verified against the **name**, and every connection went to the pinned address
  (127.0.0.1): `pinned=` in the per-connection lines. The name server saw **exactly 16 queries**: one lookup per delivery, not two.
* Refused before any connection (stage 7, detail the address): `hooks.test` -> 127.0.0.1 (loopback, with `allow-private=0`), `meta.test` ->
  169.254.169.254 (the cloud metadata address), `mixed.test` -> 10.0.0.1 and 93.184.216.34, and `mixed2.test` -> 93.184.216.34 and 10.0.0.1 (one private address refuses the whole answer, and
  it is the private one that is named, wherever it is in the answer). A counting listener behind those names was **never connected to** (0 connections for 32 deliveries); the control, the same name with `allow-private=1`, is
  dialled (4 of 4).
* NXDOMAIN is stage 6, detail -103; no name server listening is stage 6, detail -200.
* **Rebinding**: `flip.test` answers 127.0.0.1 and 127.0.0.2 in turn, with TLS receivers on both. Eight deliveries one at a time: each connected to the
  address its own lookup returned (`127.0.0.1, 127.0.0.2, ...`) and each verified the certificate for `hooks.test`, which names no IP address at all. A check
  that resolves and a connect that resolves again would have seen both answers for one delivery.
* 64 deliveries behind 64 lookups of 300 ms finish in 1.24 s (the resolver has 16 slots: four waves, the rest of the 64 wait for a slot, they do not fail).

**What the thread version costs in the language** (`rthread.ls`, `resolve_demo.ls`; the reproducers are in §9), which is why it is not the decision:

1. `getaddrinfo` cannot be declared (a list of structs behind a `struct addrinfo **`), so the thread calls `res_query` and parses the DNS answer
   itself with `dns.ls`; that has no `/etc/hosts`, no search list and no `nsswitch`.
2. `res_query`'s name is a string that is not the last parameter, so it cannot be declared with a slice; the worker passes a pointer it gets from
   `basename`, which answers a pointer into its own argument (gap 6), as an `int`. (`strdup` and `free` would do; a declaration of `free` is an
   internal error on the LLVM backend, gap 8.)
3. A thread gets **one** pointer-wide payload. It needs the authority to call libc *and* a channel to the poller; they cannot both cross. A shared
   reference to the `Ffi` crosses (and the main thread keeps using the same `Ffi` for OpenSSL: `gaps/t3`); a struct that owns the `Ffi` crosses and
   the main thread loses it until `join` (`gaps/t1`); a `Conn` does not cross (`g17`). So the worker makes its own channel with libc's `socket` and
   `connect`, which needs a raw `connect` that only an edition-1 file may declare (gap 7: it comes from the `net.connect` package, so this demo's scope is `Ffi("libc")`), to a loopback listener on **a port both sides know**, because nothing says
   which port an ephemeral listener got (gap 9).
4. The poller cannot wait for a thread: `join` is the only way to learn it finished (`g16`), so completion has to arrive as bytes on that loopback
   connection, which is a second protocol written in the example.
5. A `Thread` is a resource and the containers hold only copyable things (`g10`), so four workers are four named locals joined by name; the pool's size
   is in the program text. A function value cannot be written `module.worker` (`g9`).
6. `res_query` answers -1 for NXDOMAIN and for a dead server alike; the reason is in `h_errno`, a thread-local reached through a pointer nothing can read
   (gap 5). The `tcp` resolver tells them apart (-103, -200, -203).

What the thread resolver does give: libc's UDP, retries and server rotation for free, and a resolver that works with whatever `resolv.conf` says
without our reading it. If names are wanted and a resolver of one's own is not, it works today; it needs `conc` in `main`'s row, four named workers, and
a fixed loopback port.


## 8. Numbers

**Method, for all of them** (`test/measure.py`; the script is the method, and prints these tables; `REPS=7`, each row is the **median** of seven runs with the minimum and maximum beside it):
the client under test is pinned to core 3 with `taskset`, the receiver (`test/server.py`, Python `asyncio`, two worker processes) to cores 1 and 2, on a 4-vCPU Firecracker VM
(`Intel Xeon @ 2.10GHz`, with `aes`, `avx2`, `pclmulqdq` and `sha_ni`; Linux 6.18.44) that other work was also using; CPU is the client process's `user + sys` from
`wait4`'s `rusage` divided by the connections it made; memory is `VmRSS` read from `/proc/<pid>/status` at the client's `READY` and `HELD` markers. **The protocol state machine is
TLS 1.3, `TLS_AES_256_GCM_SHA384`** (the server's choice from the client's OpenSSL defaults, printed by `SSL_CIPHER_description`) for every row not marked TLS 1.2; the TLS 1.2 rows negotiate
`ECDHE-ECDSA-AES256-GCM-SHA384` and `ECDHE-RSA-AES256-GCM-SHA384`. The certificates are ECDSA P-256 unless marked RSA 2048, signed by a P-256 CA. OpenSSL 3.0.13. Differences under about
15% between rows are noise on this machine: the same configuration measured on different runs of this document's own script moved by that much.

### 8.1 Handshake CPU per connection

2,000 connections through 64 slots, handshake only (a `close_notify` and nothing sent):

| configuration | client CPU per handshake | handshakes per second per core (wall, client pinned and saturated) |
|---|---|---|
| TLS 1.3, ECDSA, **no verification**, memory BIOs | 0.549 ms (min 0.487, max 0.633) | 1,796 |
| TLS 1.3, ECDSA, **verified** (CA file, name checked), memory BIOs | **0.648 ms** (0.610, 0.749) | **1,529** |
| TLS 1.3, ECDSA, no verification, `SSL_set_fd` | 0.526 ms (0.466, 0.576) | 1,885 |
| TLS 1.3, ECDSA, verified, `SSL_set_fd` | 0.661 ms (0.611, 0.804) | 1,503 |
| TLS 1.3, ECDSA, verified, CA file **and** the system store loaded | 0.635 ms (0.589, 0.725) | 1,566 |
| TLS 1.3, RSA 2048, no verification | 0.483 ms (0.439, 0.536) | 1,956 |
| TLS 1.3, RSA 2048, verified | 0.567 ms (0.539, 0.621) | 1,719 |
| TLS 1.2, ECDSA, verified | 0.641 ms (0.595, 0.836) | 1,549 |
| TLS 1.2, RSA 2048, verified | 0.560 ms (0.542, 0.624) | 1,752 |

* **A verified handshake costs about 0.6 ms of CPU, one core does about 1,500 of them a second, and the two I/O transports are indistinguishable** (0.648 against 0.661 ms verified, 0.549 against
  0.526 unverified: inside the noise of the rows). The protocol version makes no difference either (1.3 against 1.2: 0.648 against 0.641 ms with ECDSA).
* **Verification costs about 0.1 ms per handshake (about 20%)**: 0.549 to 0.648 ms (memory BIOs), 0.526 to 0.661 (descriptor), 0.483 to 0.567 with an RSA certificate. Most of it is **not** the signature
  (§8.7: one ECDSA P-256 verification is 0.076 ms and one RSA-2048 verification 0.019 ms, yet the RSA row pays nearly as much as the ECDSA rows): it is building the chain, the date and name checks and the
  store lookups, which were not itemized. Verification being **off** is a 20% saving and not a reason to turn it off.
* **The trust store, once per process, not per connection** (`tls_nb` making a single connection, so everything that is not a handshake shows; seven runs): no verification 6.0 ms
  (5.4 to 7.0) for the whole process, a CA file 7.5 ms (6.1 to 8.2), the CA file **and** `SSL_CTX_set_default_verify_paths` 32.9 ms (29.3 to 35.6). Loading the system store is about **25 ms**: do it
  once, in `main`.
* **Against C**: the same work, written the way a C programmer would for many connections on one thread (`test/c_epoll.c`: non-blocking sockets, `SSL_set_fd`, one `epoll` set, 64 in
  flight), measures 0.507 ms (0.466 to 0.604) unverified and 0.598 ms (0.566 to 0.671) verified, against 0.526 and 0.661 ms for lex-sys on the same transport: within the noise, a few percent
  at most. The simplest C (`test/c_ref.c`: blocking, one connection at a time) measures **0.80 ms** unverified and 0.84 verified, *more*, because a core that waits for the peer between every
  handshake runs colder: handshake CPU depends on how busy the core is kept (§11).

### 8.2 Requests over an established session

64 connections held open, TLS 1.3, verified, thousands of requests each (128,000 for the small bodies); the 64 handshakes (0.64 ms each) are subtracted:

| request body | client CPU per request | requests per second on one core of the client (1 / CPU) | wall rate achieved, receiver-limited |
|---|---|---|---|
| 100 bytes | 0.0105 ms (0.0097 to 0.0114) | ~95,600 | 56,214 (client 58% busy) |
| 1,024 bytes | 0.0103 ms (0.0099 to 0.0111) | ~96,900 | 53,962 |
| 16,000 bytes | 0.0168 ms (0.0153 to 0.0189) | ~59,500 | 36,312 |
| 60,000 bytes (the most a `lexsys-hooks` request can be) | 0.0476 ms (0.0451 to 0.0488) | ~21,000 (1.26 GB/s of body) | 1,407 (client 8% busy) |

A request on a session that exists costs **0.01 ms**, a sixtieth of the handshake: what a delivery to one endpoint spends is the handshake, and §10.4's resumption and keep-alive are about that.
The receiver is a Python process and is what limits the wall rate; the "on one core" column is a derivation (`1 / CPU per request`), not a measurement of a server that kept up.

### 8.3 Memory per connection

All handshakes done, connections held, nothing sent; RSS of the client process; the slope between 64 and 512 connections separates what each connection costs from what the process cost once
(`measure.py memory`; the per-slot ciphertext buffer of the memory-BIO transport, 20,480 bytes a slot, is allocated before the first connection and is **not** in these figures):

| configuration | at 64 connections | at 512 connections | slope per additional connection |
|---|---|---|---|
| memory BIOs | 56.1 KiB each (52 to 59) | 48.5 KiB (38 to 51) | 47.5 KiB |
| memory BIOs, `SSL_MODE_RELEASE_BUFFERS` | 50.4 KiB | 29.1 KiB (28 to 32) | **26.0 KiB** |
| `SSL_set_fd` | 50.6 KiB | 36.8 KiB (36.7 to 36.9) | 34.8 KiB |
| `SSL_set_fd`, `RELEASE_BUFFERS` | 40.0 KiB | 26.4 KiB | **24.4 KiB** |

About **26 to 48 KiB a connection** over the whole of what OpenSSL keeps (the `SSL`, its 16 KiB read and write buffers unless released, the peer's certificate chain, the session), and about
one MiB of one-time cost. `SSL_MODE_RELEASE_BUFFERS` takes it to about 26 KiB and costs nothing measured here: use it. The memory-BIO transport is noisier than the descriptor one (38 to 51 KiB at
512 connections against a steady 36.8) and I did not find out why: the descriptor transport has no BIO buffers, which is a reason, not a measurement. For `lexsys-hooks` at 64 slots: 64 x
(26 + 20) KiB = **2.9 MiB** with the release mode (3.6 MiB without) beside the 4.1 MiB its request buffers already take.

### 8.4 64 concurrent connections, one thread

`tls_nb 127.0.0.1 <port> hooks.test 64 64 3 ...` with a 300 ms hold after the handshakes (`measure.py concurrency`): **64 handshakes and 3 requests each in flight at once, all 64 succeed, and `/proc/<pid>/status` says `Threads: 1` while all 64 are open**
(68 descriptors: the 64 sockets, the `Poller`, and the standard streams). With the 64 handshakes started together against the Python receiver the handshake latency averages 28 ms (maximum 40 ms: the
receiver's CPU, not the client's) and the whole run takes 463 ms. 20,000 connections through 64 slots all succeed, on both transports (14.9 s and 14.8 s: receiver-limited), at a peak RSS of 13.8 and 14.1 MiB.
**64 connections to a server that accepts and never answers, deadline 1,000 ms, all end at the deadline: 1.02 s of wall time and 16 ms of client CPU**: the stuck ones do not run in turn and cost
nothing while they wait.

### 8.5 Resumption

2,000 connections, 64 in flight, one request each, verified, TLS 1.3: **0.635 ms of CPU per connection without resumption (0.591 to 0.826) and 0.306 ms with it (0.291 to 0.436), 1,936 of 2,000
resumed** (the first 64 start before any session exists). Resumption halves the CPU of a delivery to an endpoint it has delivered to before; §10.4.

### 8.6 What a name lookup adds

Handshake only, 2,000 connections, 64 in flight, verified: **0.690 ms (0.624 to 0.731) to an IP literal, 0.793 ms (0.709 to 0.895) when the name is resolved over DNS/TCP on the poller for every delivery**:
about 0.1 ms of CPU for a lookup (a TCP connection, a query, an answer, `dns.parse`). A per-endpoint cache with the answer's TTL makes it a rare cost (§10.4).

### 8.7 The public-key work, alone

`taskset -c 3 openssl speed -seconds 3 ecdsap256 ecdhx25519 rsa2048`, the machine otherwise quiet: ECDSA P-256 **verify 0.076 ms** (13,107 a second), sign 0.024 ms (41,107 a second); X25519 one operation 0.035 ms (28,313 a
second); RSA 2048 **verify 0.019 ms** (51,680 a second), sign 0.33 ms (3,010 a second). A TLS 1.3 client does one key generation and one derivation (X25519, OpenSSL's first group by default; the negotiated group was not checked: about 0.07 ms), one signature verification for the
server's `CertificateVerify` and, when it verifies, a second for the certificate: about 0.15 ms of public-key work unverified and 0.22 ms verified for ECDSA of the 0.55 and 0.65 ms measured. **The rest, about
0.4 ms, is not public-key work**: parsing certificates, the key schedule, allocating and freeing an `SSL` and its buffers, and the system calls. It was not itemized further (no profiler was run). What is
the same in C and in lex-sys is OpenSSL; what the language adds is a few percent (§8.1).

## 9. Every gap met, with a reproducer

Nothing below was worked around silently. Each row names the file in `examples/tls_nb/gaps/` that shows it (`python3 examples/tls_nb/gaps/check_gaps.py` runs
all 23 and exits 0 only if each still behaves as recorded here: a gap that gets fixed turns a row red, which is the prompt to update this table and the
document that depended on it), what was done instead, and where the fix would live. **C** is the compiler, **R** the runtime, **L** the language or `std`,
**P** a package, **T** the tooling.

| # | gap | reproducer | what this spike did | where |
|---|---|---|---|---|
| 1 | **`c_ptr` cannot be named in an ordinary function's signature or a struct field** (`unknown type c_ptr`); a `[c_ptr]` slice and a generic `T` instantiated at `c_ptr` compile and run, but a generic body cannot call anything with its `T` | `g1_cptr_param`, `g2_cptr_field`; what works: `g3_cptr_array`, `g4_generic` | every handle is an `int` (§3.3), declared as the extern's result: forgeable, and every `SSL *` type error is lost | C: allow `c_ptr` wherever a `val` type may stand |
| 2 | **no `conn_raw_fd`**; and the descriptor of a `Conn` in a `std.conns.Table` is readable **without `Ffi`** as the low 32 bits of its ticket (`Table.tickets` is a field of a `pub` struct) | `g5_table_ticket` | memory BIOs (§3.2); `tls.fd_of` reads the ticket for the `io=1` comparison only | R: build `conn_raw_fd` (`native-sockets.md` section 6) and make `Table`'s fields private |
| 3 | **a C `int` result declared `int` is wrong**: `-1` reads as 4294967295 on both backends; `examples/tls_client/` declared five so | `g13_int_vs_c_int` | `c_int` everywhere; the example is fixed | L: nothing to fix in the language; a lint on `extern` results named after an `int`-returning libc/OpenSSL symbol would have caught it |
| 4 | **a foreign function with two string parameters, a string that is not last, or a pointer out-parameter that is not last cannot be declared**: a slice crosses as a pointer **and** a length, so the next parameter's register holds the length (`strcmp(a, b)` crashes with SIGSEGV; `BIO_new_bio_pair` has two out-parameters) | `g7_two_strings`, `g14_out_params` | `SSL_CTX_load_verify_file` (one string) instead of `..._locations`; `X509_VERIFY_PARAM_set1_host` (pointer and length) instead of `SSL_set1_host`; memory BIOs made one at a time with `BIO_new(BIO_s_mem())` instead of `BIO_new_bio_pair` | C: a parameter kind that is a bare pointer (a `NUL`-terminated slice), or an `extern` form that says "this slice is a pointer only" |
| 5 | **nothing C returns can be read**: a `const char *` (`SSL_get_version`), a struct, `getaddrinfo`'s list, `h_errno` (a thread-local reached through a pointer) | `g8_c_string_result` | text comes out only through functions that fill a buffer the caller owns (`SSL_CIPHER_description`, `ERR_error_string_n`); the thread resolver loses NXDOMAIN versus failure (§7, item 6) | C: a read of `n` bytes through a `c_ptr` into a slice, under `Ffi` |
| 6 | **no address of a buffer**: a pointer to the caller's own bytes can be made only by a libc function that returns its argument (`basename`) | `rthread.ls` (`basename`) | used once, in the thread resolver | C: the same builtin as 5, the other way |
| 7 | **`connect` is a builtin from edition 2 on** and a foreign declaration's Lex name is the linked symbol, so a raw `connect(2)` can only be declared in a file of edition 1 | `g11_connect_is_a_builtin` (`.ls.txt`) | the thread resolver calls `net.connect`'s `connect_to`: the package declares `connect` in edition 1; the first version had a file of its own, which the repository's count of network programs refuses (§4) | known (`foreign-linking.md`); an `extern` that can name its symbol differently from its Lex name |
| 8 | **a foreign declaration of libc's `free` (and `malloc`) is accepted by `check` and refused by the LLVM backend as an `internal` error** (`invalid redefinition of function 'free'`): the emitted module already declares them for its heap; Cranelift accepts | `g6_extern_free` | `basename` instead of `strdup`/`free` | C: the backend should declare with the program's signature or refuse with a rule tag (`CONTRIBUTING.md`: every refusal has one) |
| 9 | **threads**: (a) a payload is one pointer-wide leaf, so a thread cannot be given a capability and a channel (a shared `&Ffi` crosses, a struct owning the `Ffi` crosses and moves it, a `Conn` does not); (b) `join` is the only way to wait, a `Poller` cannot watch a thread; (c) no `listener_port`, so a loopback channel needs a port agreed in advance; (d) a `Thread` cannot be stored in a container; (e) a function value cannot be written `module.function`; (f) every caller up to `main` declares `conc` | `t1_spawn_job`, `t3_shared_ffi_payload`, `g17_conn_payload`, `g16_no_thread_poll`, `g15_no_listener_port`, `g10_thread_in_container`, `g9_qualified_function_value`, `t2_ref_field` (`.ls.txt`) | the poller resolver (§3.7) needs none of this; the thread resolver (§7) works around each | R: `thread_done`, a `Poller` registration for a thread or a `Conn` from `socketpair`/`eventfd`, `listener_port`; L: struct payload (`threads.md` section 5 says "decided not yet: no asker"; this is one) |
| 10 | **`Ffi` scope**: nominal (nothing ties it to a library), one per program, hard-coded as `"libc"` by `net.sockets`/`net.connect`, and the authority report is `UNBOUNDED` whatever it is called | `a1_ffi_scope`, `a2_scope_is_nominal`, `g12_scopes_do_not_compose` | one scope named by purpose (`tls`) (§6) | L/P: a scope per capability value instead of per program; packages that take the scope as a parameter |
| 11 | **`[[bin]]` in the project file has no linking options** (`package-system.md`: "no linking options in `[[bin]]`"), so `lex-sys build` in a project cannot link OpenSSL | (below) | a `CC` wrapper: `CC=./cc_ssl lex-sys build ...` where `cc_ssl` is `exec cc "$@" -lssl -lcrypto`: **verified** to build and link `tls_nb` (`ldd` shows `libssl.so.3`, `libcrypto.so.3`) | T: `links = ["ssl", "crypto"]` in `[[bin]]` |
| 12 | **standard output is fully buffered when it is not a terminal and there is no flush** (`std.io`'s own comment says so): a progress marker written to a pipe arrives at exit | `test/harness.py` (markers on standard error) | `READY` and `HELD` go to standard error | L: `io.flush` |

(Row 11's reproducer is one command: `printf '#!/bin/sh\nexec cc "$@" -lssl -lcrypto\n' > cc_ssl; chmod +x cc_ssl; CC=./cc_ssl lex-sys build tls.ls dns.ls rtcp.ls pin.ls tls_nb.ls --std -o tls_nb`.)

Not a gap of the language but found by the integration, and recorded in §2 and §3.2: OpenSSL's socket BIO writes with `write(2)` and a closed peer kills the process
with `SIGPIPE`; and `SSL_CTX_set_default_verify_paths` costs about 25 ms and must run once per process.

## 10. What `lexsys-hooks` would need

All of this was read from `lexsys-hooks` (`src/attempt.ls`, `src/endpoints.ls`, `src/destination.ls`, `src/hooks.ls`, `sql/`, `docs/design.md` sections 16 and 26) and
**none of it was changed or run there**.

### 10.1 The endpoints: a scheme, a name, a pinned address

Today an endpoint is `<id> <host> <port> <secret>` (`endpoints.conf`, and the `endpoints (id, host, port, secret)` table), `host` must be a public IPv4 literal
(section 26), the request carries `Host: receiver` (`src/hooks.ls:1293`, a constant), and an attempt dials `host:port`.

| change | detail |
|---|---|
| **a scheme and a name** | a fifth field `https` and an optional sixth, `<name>`, in the file (`<id> <host> <port> <secret> [https [<name>]]`); columns `tls smallint not null default 0` and `tls_name text` in the table; `POST /endpoints` and `PATCH /endpoints/:id` take `"scheme": "https"` and `"name"`. `port` defaults to 443 for `https`. **`name` is what the certificate must name, what goes in SNI and what the `Host` header says**; it defaults to `host` when `host` is a DNS name |
| **`host` for an `https` endpoint** | slice T1 (no resolver): an IPv4 literal, with `name` required (the address is dialled, the name is verified: §7's pinned design with the pin written by the operator). Slice T2: `host` may be a DNS name, resolved and judged at each attempt (§10.2) |
| **the `Host` header** | the endpoint's own host (and `:port` when not 443) instead of `receiver`, for every endpoint, plain ones included; the signed request is built before the connection, so this is `request_for`'s argument |
| **in memory** | `endpoints.stride()` is 7 integers (`slot, port, host_start, host_len, key_start, key_len, id`); add `tls`, `addr` (the pinned packed IPv4, 0 when unresolved) and `addr_expiry` (clock ms): 10 |
| **history** | `attempts.status` already holds negative reasons (-1 to -4). Add: -5 TLS handshake failed (the peer closed or spoke badly), -6 certificate refused, -7 name did not resolve, -8 destination refused; and a second column `detail int` for the `X509_V_ERR_*` number or the DNS code, so "expired" and "wrong host" are told apart in `GET /events/:id/attempts` |

### 10.2 The SSRF rule (design section 26)

Section 26's rule is "an IPv4 literal, public, nothing else, **because a name cannot be judged before the connection**". With a resolver that judges the answer and pins it
(D7, §7) the rule becomes, for a name:

1. the name must be a syntactically valid host name (labels of letters, digits and hyphens, 253 bytes at most) at the moment an endpoint is written; **that is all that can be
   checked then**, and it is a weaker write-time guarantee than today's (a name that resolves to 10.0.0.5 is stored, and every attempt to it fails with status -8);
2. at **every** attempt (or once per TTL: §10.4), the name is resolved, **every** A record is judged by the ranges of section 26 (`pin.choose`: one private address refuses the whole
   answer), and the connection is made to the address chosen as an IP literal; `allow-private-hosts 1` keeps its meaning (any address passes) and is what the tests and demos use;
3. a redirect is still never followed; the port is still not limited; IPv6 (AAAA) is not asked for, so a name with only an AAAA record fails as "no address" (-7).

"Refuse, don't downgrade" changes character: for IP literals the refusal is still at the write (a `400` that stores nothing; a bad file stops the start), for names it is at the
attempt (a recorded failure that is retried like any other and that an operator reads in the history). A deployment that wants the old guarantee for everything keeps
`https` endpoints to literals plus a name (slice T1's form) and does not enable names.

### 10.3 The attempt state machine (`src/attempt.ls`)

Today: `connecting -> sending -> reading`, a stride of 8 integers a slot (`state, endpoint, event, deadline, sent, held, request length`, seven used), the request copied into a 66,560-byte slot buffer, the
connection in a `std.conns` table watched on the server's poller under `token0 + slot`. With TLS and names it becomes

```
(resolving)  -> connecting -> (handshake) -> sending -> reading -> (shutdown) -> finish
 name only      TCP             tls only       now SSL_write      SSL_read       close_notify, close
```

* **resolving** is not a slot: a lookup is a `Resolver` slot (`rtcp.ls`, 16 of them, tokens above the pg pool's) and the attempt waits in the queue of attempts for an endpoint whose
  address is unknown or expired; when the answer is judged and pinned the attempt takes a slot and goes to `connecting`. (The spike does the same: `st[7]` counts lookups, an
  attempt takes its slot only when it dials.) One lookup serves every attempt that waits for the same endpoint.
* **handshake** and the sending and reading phases call `tls.handshake`/`write`/`read` (§3.4) instead of `conns.write`/`conns.read`; `advance` keeps its shape (a loop that runs until
  something has to wait). Per slot: 10 more integers (`tls.stride()`) and `tls.out_max()` = 20,480 bytes of ciphertext; **TLS costs 64 x (37 KiB + 20 KiB) = 3.6 MiB at 64 slots**
  (26 KiB each with `SSL_MODE_RELEASE_BUFFERS`), on top of the 4.1 MiB the request buffers already take.
* the **deadline** (2 s by default, covering connect, send and the status line) now also covers lookup and handshake; the handshake alone is one or two round trips plus about
  0.6 ms of CPU (§8.1). A per-stage budget (lookup, connect, handshake, request) is better than raising the one number.
* `finish` calls `tls.shutdown` (a best-effort `close_notify`) before the close, `tls.drop` always, on every path (the leak tests in §5 are the gate).
* the read path stays "the first 12 bytes of the response", as it is: a `close_notify` before the status line is `no_answer`, as an end of file is today.
* `conn_write`'s `MSG_NOSIGNAL` is why the memory-BIO transport needs no signal disposition (§3.2); `hooks` must not switch to `SSL_set_fd` without `sigpipe=ignore`.

### 10.4 TLS session handling

* **One `SSL_CTX` per process**, made in `main` (the trust store is read once: about 25 ms with the system store, a millisecond or two with a file, §8.1) and freed at exit; `SSL` per attempt.
* **Resumption pays**: 0.64 ms of CPU per connection falls to 0.31 ms (§8.5), 1,936 of 2,000 connections resumed. It needs a saved session per endpoint: `tls.save_session` after
  the **first complete response** (with TLS 1.3 the ticket arrives after the handshake and the first `SSL_read` is what reads it), `tls.open(..., session)` on the next attempt, one
  integer per endpoint in the table and a `tls.free_session` when it is replaced, when the endpoint's host or scheme changes (`PATCH`), and at exit.
* **A resumed session skips certificate verification**, which is what makes it cheap and what makes it a hazard: key a session by (endpoint, host name, trust-store generation) and never
  offer one for a different host name or after the trust configuration changed. The spike has one host and does not model this.
* **Keeping the connection** is the larger saving (a request on an established session is 0.01 ms of CPU, §8.2, against 0.6 ms for the handshake) and a bigger change than this document
  scopes: `attempt` closes after each response today; a per-endpoint pool changes the table, the deadlines and the retry accounting. Resumption first.

### 10.5 Certificate trust configuration

`--tls-trust system|file:<path>|both` (default `system`) and a `GET /config` field; at start `tls.context(verify, default_paths, cafile)`; a failure to read the store **stops the start**
with a status and the reason (as an unreadable endpoints table does, section 24), never an unverified client. `SSL_CERT_FILE`/`SSL_CERT_DIR` override the system store when set
(tested in §5): a deployment that does not want the environment to change what is trusted sets `file:`. The minimum protocol is TLS 1.2 and is not configurable in v1. Revocation, client
certificates and pinning a certificate or key per endpoint are not in v1.

### 10.6 Slices, each with a gate that can fail

| slice | what | gate |
|---|---|---|
| **T0** (`lex-sys`, optional hardening) | `links` in `[[bin]]` (gap 11), `c_ptr` in signatures (gap 1), the `free` internal error (gap 8), `conn_raw_fd` (gap 2) | the compiler's own: `cargo test --workspace`; none is needed for T1 to start (a `CC` wrapper links; `int` handles work) |
| **T1** `tls` package | `tls.ls` as a package (`packages/tls`, or `lexsys-tls`) with the verify matrix, leak, partial-I/O and fuzz tests moved with it; **hooks** gets `tls`, `attempt.ls` phases `handshake`, the `tls` flag, `Host`, trust config, the history codes, `ffi("tls")` on the chain, IP-literal `https` endpoints only | `verify_matrix` (every row, both reasons told apart in `/events/:id/attempts`), the existing suites **unchanged and green** (retry across kills, signatures, ingest under power cuts, isolation), a new isolation row (64 silent TLS peers, ingest p99 within the 80 ms of section 16), `lex-sys authority` shows `ffi("tls")` only on the chain, the mutants of the new phase (handshake never advanced, verify off, session never freed) killed |
| **T2** names | `dns.ls`, `rtcp.ls`, `pin.ls` into the package; `resolving` queue, per-endpoint pinned address with a TTL clamp (30 s to 5 min), the SSRF rule of §10.2, `tests/ssrf_test.py` extended | `pinned_test` rows (private refused with no connection made, mixed answer refused, rebinding: one lookup per attempt, NXDOMAIN and unreachable server as recorded outcomes), the 41 refused hosts of section 26 still refused, a slow-DNS isolation test (the loop's longest gap under 15 ms), `dns_fuzz` in CI |
| **T3** sessions | the per-endpoint session table, `/stats` counters (handshakes, resumed, verification failures by code), invalidation on `PATCH` | resumption rate in a delivery run, a changed host name never resumes (a test that fails if it does), the leak test still green |

**Recommendation: TLS in `lexsys-hooks` is achievable now, with the compiler as it is, in three slices of the service (T1, T2, T3) and no compiler slice required.** T1 alone delivers
`https` to endpoints an operator gives as an address plus the name its certificate carries; names that the service looks up itself are T2, and an operator with customers' hostnames needs
T1 and T2 together (T2 is where the SSRF rule changes, so it is the one to review hardest). T0 removes workarounds (`int` handles, a `CC` wrapper, a descriptor reached through a ticket)
and should follow, not precede. The risk that does not go away is §6: the authority report becomes `UNBOUNDED`.

## 11. Honest limits: what was not verified

* **Only local servers.** Every handshake was against `server.py` (Python's `ssl`, OpenSSL 3.0.13) or `openssl s_server`. Nothing was connected to a real public host: outbound traffic in this
  environment goes through a proxy, so a public certificate chain (a real intermediate, a real root in the system store, a wildcard, an SNI-routed host) was **not** exercised; the system
  store was exercised only by pointing `SSL_CERT_FILE` at the test CA.
* **One machine, shared.** 4 vCPUs of a Firecracker VM, other work running; the client pinned to a core, the receiver to two others. Rows carry the minimum and maximum of seven runs; differences
  under about 15% are noise (the verified-versus-unverified row is one of them: §8.1). The receiver is Python and is the limit on requests per second.
* **Backends.** Everything above ran on the default (LLVM) backend. The Cranelift build of `tls_nb` was spot-checked (200 verified connections, 64 on the descriptor transport, an expired certificate: all as expected), and `dns_fuzz` and
  the reproducer `g8` were run on both; the verify matrix and the measurements were not repeated on Cranelift.
* **No Darwin.** `-l ssl -l crypto` needs OpenSSL on the linker's path, which `foreign-linking.md` says macOS does not give by default; `EINPROGRESS`, `SO_ERROR` and the socket constants of
  `tcp_connect_start` are Linux-run only.
* **TLS 1.2 renegotiation, early data, post-handshake authentication, session tickets refused by a server on reuse, IPv6, internationalised host names, wildcard certificates, IP SANs,
  revocation (CRL, OCSP, stapling), client certificates and certificate pinning**: not exercised, and the last four not built.
* **Large responses.** The client reads until a known response length (the test receivers are fixed-size) or, as hooks does, a status line; a chunked or very large body was not read through TLS.
* **`hooks` itself.** Nothing was changed or run in `lexsys-hooks`; §10 is a design read from its source.
* **The `fd` transport's partial-I/O behaviour** is not under the shim (OpenSSL's own socket BIO calls `read`/`write`, which the shim does not wrap); only the memory-BIO transport, the chosen one, is.
* **Out-of-memory** paths: `tls.open` leaks a BIO if the second `BIO_new` fails after the first succeeded (§5); not exercised.
* **Handshake CPU depends on the load on the core**: a blocking C client, one handshake at a time, measures 0.80 ms where the same work with 64 in flight measures 0.51 (§8.1): the work is the same and
  the idle core is not free. Quote a figure with its concurrency.

## 12. Reproduce

```sh
# once: the certificates, the compiler is the checked-in one
cd examples/tls_nb && export TLS_NB_WORK=/tmp/tls_nb_work
python3 test/verify_matrix.py          # §5   every certificate outcome, both transports, the SIGPIPE rows
python3 test/leak_test.py              # §5   descriptors, memory, valgrind
python3 test/partial_io_test.py        # §3.2 partial and refused I/O, a silent peer (needs `cc`)
python3 test/resolve_test.py           # §7   three resolvers against a slow name server (needs `unshare -m`, root)
python3 test/pinned_test.py            # §7   resolve, judge, pin, connect, verify
python3 test/pin_test.py               # §7   pin.ls against lexsys-hooks' destination.ls (needs a checkout beside this one; exit 77 without)
lex-sys run dns.ls dns_fuzz.ls --std   # §7   a million damaged answers
python3 gaps/check_gaps.py             # §9   every reproducer
REPS=7 python3 test/measure.py         # §8   every number (about 20 minutes); sections: cpu reference requests memory concurrency resume resolve
```

## 13. Corrected elsewhere, in place

* `native-sockets.md` section 6 said a program without `Ffi` cannot reach the descriptor hatch: it can read the number from a ticket (§3.2, gap 2).
* `examples/tls_client/tls_client.ls` declared C `int` results as `int` (§2): fixed; `foreign-linking.md` section 4 notes it.
