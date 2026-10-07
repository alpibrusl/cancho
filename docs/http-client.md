# `http.client`: a non-blocking HTTP/1.1 client for a program with one poller

> **Status: built.** The design was written, and committed, before the code (first commit of this PR); §2 was measured or read in this
> repository on that day, §3 to §8 are the decisions, §9 the gates stated before their numbers existed, and **§10 is what was built, what
> the build corrected in the sections above (each marked *corrected*), the numbers against the gates, and what the gateway still lacks**.
> §11 is the open questions for a person, each with the answer this PR builds on.

`docs/http-server.md` §11.8 lists what `cancho-gateway` (an HTTP/1.1 reverse proxy that terminates HTTPS) still lacks, in the order it
will hit them. Item 4 is this one: *"Upstream connections in the same loop ... `packages/http-request` is blocking; a non-blocking
client is the missing piece, as it was for `cancho-pg`."* The gateway's own `docs/pool.md` shows what it does without one: a client,
a connection pool and a response framer written inside `src/proxy.cho` (1,448 lines), over plain TCP only, with its own response-head
parser (`src/response.cho`, 336 lines), its own pool, and **no upstream TLS, no upstream names** (its §6: "Upstream host names ... a
blocking resolver"). `cancho-hooks` (webhook delivery) has the same pieces a second time (`docs/design.md` §53: keep-alive, strict
framing, the retry on a reused connection) in `src/attempt.cho`. Two programs have written this; the repository's bar for a package
is two askers (`docs/standard-library.md`), and this is the third time it is asked for.

---

## 1. The question

A program with one thread and one `Poller` wants to make many HTTP/1.1 requests **in the same loop** that serves its own clients:
keep several connections to each upstream open, send a request on one without waiting for the one before, stream a request body that
is arriving from somewhere else, stream the response body to somewhere else, reuse a connection afterwards when that is safe, and
never hold more memory than it chose at start. It must do this over plain TCP and over TLS 1.3, and it must be testable without a
socket.

What the language allows constrains the shape (`AGENTS.md` §8): no function values, so no callbacks; no polymorphism over effects;
a resource (`Conn`, `Engine`) cannot sit in a generic container, so connections stay in `std.conns` by ticket; an arena is one 64 KiB
chunk and traps when full, so buffers are heap boxes sized when the program starts.

---

## 2. What exists, and what it is worth

| piece | what it gives | what it does not |
|---|---|---|
| `packages/http-request` (141 lines) | `read_request`: reads **one request** from a blocking socket, `Content-Length` found by a naive substring search, streamed to standard output. Needs `Ffi("libc")` through `net.sockets` | it is a *server-side* helper that happens to have the name; it parses no response and sends nothing |
| `packages/http-response` (61 lines) | `send_all` (blocking write loop) and `status_of` (three digits of a status line) | nothing about headers, framing, reuse |
| `examples/fetch` | a blocking `GET`, the reference for "a cancho client", libc sockets | one request at a time; it waits |
| `std.http` | a strict **request** parser into an integer table (`parse`), `dechunk` (a whole chunked body in one call), response *writers* | no response parser; `dechunk` needs the whole body in memory, so it cannot stream |
| `std.conns` | `Table` of connections by slot, `read`/`write`/`watch`/`connect_status`, `nonblocking`, `nodelay`; with `tcp_connect_start` a **non-blocking connect to an IP literal** (a name stalls the loop: `native-sockets.md` §10.6) | |
| `packages/tls` | a client `Engine`: `start`, `feed`/`take`/`send`/`recv` (bytes in, bytes out, no socket inside), the chain verified against roots the caller gave, the name and the time given at `start`; tickets for resumption | one engine is all clients or all servers; one slot is **269,119 bytes** (measured: `tls_client.ints_len()` is 10,241 words, 81,928 bytes, and `bytes_len()` 187,191 bytes), so 64 TLS connections are 17 MB |
| `packages/http-server` byte-fed mode | the shape to mirror: `attach`/`input`/`output`/`room`/`closing`/`detach`, the clock an argument, every buffer fixed at `open_bytes` | |
| `examples/tls_nb/rtcp.cho` | `rtcp.Resolver`: DNS over TCP as a state machine on the caller's poller (a lookup does not stop the loop: a 2 ms gap against 302 ms for `getaddrinfo`, `tls-nonblocking.md` §7) | it is an example file (with `dns.cho`), not a package, and it is `Net("")`-only |
| `tests/programs/tls_many.cho` | a client of `packages/tls` on one poller: connect, handshake, request, read to close_notify | one request per connection |
| `https_hello`, `tls_echo` | the loops: `socket -> tls.feed -> tls.recv -> rcv -> server.input` and back, each stage reading only when the next has emptied | server side |

So the engine's bytes-in/bytes-out shape already exists on both sides of TLS and for the server's HTTP; the client's HTTP is what is
missing, and the pieces around it (connect, DNS, TLS) are in the repository as examples and engines, not as one thing a gateway can
call.

### 2.1 What the two programs that wrote it agree on

Read from `cancho-hooks` `docs/design.md` §53 and `cancho-gateway` `docs/pool.md` and `src/response.cho` (the rules, not the code):

* **One reading of the framing, or a refusal.** A response head over a bound, a malformed status line, obsolete line folding, a
  repeated `Content-Length` (even with equal values), `Content-Length` with `Transfer-Encoding`, a `Transfer-Encoding` that is not
  exactly `chunked`: each is a refusal with a tag (the gateway's are `response.status-line`, `.version`, `.status`, `.header`, `.fold`,
  `.length`, `.two-lengths`, `.transfer-encoding`, `.head-too-large`; this client uses the same names).
* **A connection is reusable only if** the response is `HTTP/1.1`; there is no `Connection: close`; the body is framed (a length,
  `chunked`, or none: `HEAD`, `1xx`, `204`, `304`), **not** until close; nothing is left unread on the connection; and the
  request's own body was fully sent. Anything else closes it after the outcome.
* **One retry, on a reused connection only, before any response byte**, on a new connection (a pooled connection can be closed by
  the peer at the instant a request is sent). The hooks retries any request, because its events are at-least-once; the gateway only
  an idempotent method with no body, because it cannot know. **That difference is policy, and the client takes it as a per-request
  flag** (§3.5).
* **Bounded in time:** idle connections expire (30 s in hooks, 5 s in the gateway's default), and a connection is retired after a
  number of requests or a lifetime (hooks: 1,000 requests, 5 minutes) so a change of an address is honoured and none lives for ever.
* **A connection idle in the pool is watched for readable**, and a byte or an end on it closes it at once.

---

## 3. Decisions

### 3.1 D1: a new package, `http-client`; `http-request` is not touched

`packages/http-request` is a blocking, server-side reader that depends on `net.sockets` (and so on `Ffi("libc")`), is consumed by two
examples (`collect`, `agent_supervisor`) and is pinned by locks; its name suggests it is the client and it is not. The blocking
`fetch`-style API **keeps working unchanged** and nothing here edits it. Putting a non-blocking client in the same module would bring
`Ffi` into a report that is otherwise bounded; a new package has no `Ffi`, no socket and no capability in it but the `Heap` its buffers are allocated from (§3.2). *(Corrected: "no capability at all" as first written; `open` and `close` take the heap.)*

The package has three modules, split by concern (and so that none nears 2,000 lines): `http_client_slot` (the names of the words of a
connection's slot and the numbers they hold), `http_client_wire` (what is on the wire: validating and writing a request head, parsing a
response head, the incremental chunk decoder; pure functions over slices and integer tables) and `http_client` (the table of connections,
the state machines, the pool, the timers, the events). The store layout is `packages/tls`'s: one store per module under
`packages/http-client/.cancho-vcs/`, published by `scripts/publish_packages.py`. *(Corrected: two modules in §3.1 as first written;
`http_client` reached 1,988 lines and the slot layout moved out of it.)*

---

### 3.2 D2: the shape: a table of `N` connections, bytes in and out, events returned

The package performs **no I/O and reads no clock**. It is a sans-I/O state machine in the shape `http.server`'s byte-fed mode has: the
caller owns the sockets, the TLS engine and the poller; the package owns one slot per connection and tells the caller what to do.

```
var c = http_client.open(heap, slots, in_size, out_size, limits);   // everything allocated here
t = http_client.request(c, now_ms, key, "GET", "/x", "host", "", body_none(), flags);   // a ticket, or a refusal
match http_client.poll(c, now_ms) {                 // one event, or Event::None
    Event::Connect(k)  => { /* dial key(c,k); then connected(c,k,now) or connect_failed(c,k,kind,now) */ }
    Event::Head(t)     => { status(c,t) headers ... }
    Event::Body(t)     => { b = body(c,t); ...; consume(c,t,len(b),now); }
    Event::Done(t)     => { ... }
    Event::Failed(t)   => { failure(c,t) -> a code; refusal_tag(code) }
    Event::Close(k)    => { /* close the socket (and the TLS slot), then detach(c,k) */ }
    ...
}
n = http_client.take(c, k, out, now);   // request bytes for connection k's transport (tls.send, or the socket)
n = http_client.give(c, k, data, now);  // response bytes from it; as many as `room` allows, the caller keeps the rest
```

Why a result enum and not callbacks: the language has no function values (`AGENTS.md` §8), and a callback is the shape that did not
type-check for the server (`http-server.md` §2). Why a **ticket** (request) and a **slot** (transport): a request is addressed by
`ticket = generation * 4096 + slot` so a stale holder (a client that went away, a retried request) cannot read or write the next
request that reuses the slot; the transport belongs to the slot, so `take`/`give`/`eof`/`detach` take the slot, as the server's
`attach`/`input`/`output`/`detach` do. One slot carries one request at a time (no pipelining of *requests*: it is the part of
HTTP/1.1 that proxies and servers disagree about, and neither the gateway nor hooks uses it).

**Time is an argument.** Every call that moves a byte or looks at time is given `now_ms` (any monotonic milliseconds). The caller
reads its clock once a turn, as `https_hello`'s loop does. A fake clock in a test is an integer.

**Back-pressure is a number, the server's rule.** `room(c,k)` is the bytes of response the client will take (the in-buffer's free
space; 0 while the caller has not consumed the body), `send_room(c,t)` the bytes of request body it will take, `pending(c,k)` the
bytes waiting for the transport. A caller reads the socket only for `room`, so a caller that does not consume a body stops the
upstream (TCP's window), and a caller that is slow to take request bytes stops the *client's* upload, never grows a buffer.

**Memory is fixed at `open`:** `slots * (in_size + out_size)` bytes, `slots * 48` words of state and `slots * 272` words of parse table,
all allocated there; nothing is allocated per request or per connection.

### 3.3 D3: the request side

`request(c, now_ms, key, method, target, host, extra, body, flags)` validates and writes the head into the connection's output buffer:
`METHOD SP target SP HTTP/1.1 CRLF Host: host CRLF [extra] [Content-Length | Transfer-Encoding: chunked] [Expect] [Connection: close] CRLF`.

* **The client alone writes the framing.** `extra` is the caller's header lines, each `token ":" value CRLF`; it is refused
  (`request.header`) if a name is not a token, a value holds a control byte, or the name is one the client owns:
  `Host`, `Content-Length`, `Transfer-Encoding`, `Connection`, `Expect`, `Upgrade`, `TE`, `Trailer`, `Keep-Alive`. A caller cannot make this
  client emit a request with two framings, which is the request-smuggling shape `std.http` refuses on the way in. (A proxy that
  forwards a client's headers strips the hop-by-hop ones itself, `cancho-gateway` `docs/headers.md`; the client refuses what it
  must not send rather than hope.)
* **Method** is a token and not `CONNECT` (`request.method`); **target** is origin-form (starts with `/`) or `*`, visible ASCII, no
  `#` (`request.target`); **host** is a non-empty reg-name, IPv4 or bracketed IPv6 up to 255 bytes (`request.host`).
* **The body** is `body_none()` (no body headers), `n >= 0` (`Content-Length: n`; `0` is sent for `POST`, `PUT` and `PATCH` and
  omitted otherwise) or `body_chunked()` (`Transfer-Encoding: chunked`, the client frames each piece). The caller gives it with
  `send_body(c, t, bytes, now)`, which takes as many bytes as `send_room` allows and returns how many, so **a client's upload can be
  piped to the upstream with back-pressure**: the gateway reads at most `send_room` from its client's socket, and when the number
  is 0 stops reading it. For a length body the client refuses bytes past the length (`request.body-too-long`, -1); for a chunked
  body `end_body(c, t, now)` writes the last chunk. Bytes given with the head in one `request` call are not special: the head is the
  first thing in the buffer and `send_body` appends.
* **`Expect: 100-continue`** (`flags` bit 2): the head goes out, `send_room` is **0** and the bytes of the body wait until a `100
  Continue` arrives (`Event::Continue`) or `expect_ms` passes (RFC 9110 §10.1.1: a client does not wait for ever; `Continue` is
  raised then too, `continued_by_timeout` says which), or a final response arrives instead (the body is never sent, §3.4).
* **The head must fit in the output buffer** or the call is refused (`request.too-large`); the caller sizes `out_size` for its largest
  head plus one piece of body. **No input reaches a trap:** every length is checked before any index.

### 3.4 D4: the response side

Status and headers are parsed **as soon as the head is complete**, into the connection's integer table (`std.http`'s shape: offsets
into the buffer, so no copy), and `Event::Head(t)` is raised; `status`, `version`, `header_count`, `header_name`, `header_value`,
`header` (find by name) and `head` (the raw bytes) read it. They are views of the client, valid until the first `consume`. Then:

* **Informational (`1xx`, except 101) is skipped**, never shown: the next head is read from the same bytes. `100` raises
  `Event::Continue` once; at most 8 are accepted per response (`response.too-many-informational`). **`101` is refused**
  (`response.upgrade`): the client does not tunnel, and a `101` that a request did not ask for is a protocol error. Upgrade and
  `CONNECT` are the gateway's item 5, a separate slice (§8).
* **Body framing** follows RFC 9112 §6.3 and the rules of §2.1: none for a `HEAD` request and for `1xx`, `204`, `304`; `chunked`
  (decoded: the caller sees the same bytes whatever the framing; `body_kind` says which it was); `Content-Length`; otherwise *until
  the connection ends*. Chunk-size lines are strict (hex digits only, no sign, no overlong), an extension after `;` is skipped up to
  128 bytes, the line ends CRLF exactly, and trailers are skipped up to 8 KiB (`response.chunk`, `response.trailers-too-large`); a
  length past 2^50 is refused (`response.length`), as `std.http` does.
* **The body is delivered in pieces**: `Event::Body(t)` says bytes are waiting; `body(c,t)` is a view of them (decoded in place for
  `chunked`, so one copy in from the socket and none more inside the client); `consume(c,t,n,now)` takes `n` of them off. The
  caller owns what it does not consume (`avail`). The in-buffer is both the head's room and the body's, so **a head must fit in
  `in_size`** or the response is refused (`response.head-too-large`, 16,384 at most whatever `in_size`).
* **`Event::Done(t)` is raised after the last byte was consumed**, not after it arrived, so the buffer is empty when the connection
  returns to the pool. A response that is complete and unconsumed holds its slot.
* **A response that arrives before the request is fully sent** (a `413` to an upload, a `401`): the final head is delivered
  as usual, `request_complete(c,t)` is false, the unsent request bytes are dropped (`send_body` answers -1: `request.response-arrived`), the
  response is read to its end, and **the connection is not reused** (the upstream may still expect the rest of the body, so it is
  closed after the response, as the gateway does).
* **Close-delimited bodies** end when the transport ends cleanly (`eof`): that is `Done`, not a failure, and the connection is
  closed. A length or chunked body cut short by `eof` is `client.truncated`.

### 3.5 D5: pooling, reuse, and the one retry

**Keyed by an integer the caller chooses** (`key`, a non-negative number: an upstream's index, an endpoint slot). The client does not
know addresses; the caller's table maps `key` to an address, a port, a scheme and a name, so a connection can never be given to
another upstream, and the caller can change what a key means by `retire(c, key)` (every idle connection of the key is closed; active
ones finish and are not pooled), which is `cancho-hooks` §53.5 as one call.

A connection that has finished a response **goes idle in its slot** when all of these hold, else it is closed after the outcome
(`Event::Close`, `close_reason` says why): `HTTP/1.1`; no `Connection: close` token in any `Connection` header of the response, and
none sent; framed by a length, `chunked` or no body (never until close); every byte of the body and nothing after it consumed;
the request fully sent; no early response; fewer than `max_requests` requests carried; younger than `lifetime_ms`.
`HTTP/1.0` is never reused (hooks); that is stricter than the gateway, which reuses `1.0` with `keep-alive`, and it costs nothing a
modern upstream notices.

**`request` takes the most recently used idle connection of the key**; with none, a free slot; with none, it **evicts the least
recently used idle connection of another key** (that connection's transport is closed by the caller on `Event::Close`, and the new
request connects in the same slot when the caller has `detach`ed it); with none of those, it refuses `client.full`, which is
backpressure the caller owns (queue and retry after the next `Done` or `Failed`). Per key at most `idle_per_key` connections stay idle;
the others are closed when they finish. A request whose key has an idle connection is **already sent** when `request` returns: the
head is in the output buffer and `pending` is its size, no `Event::Connect` is raised.

**While idle**, the caller keeps the transport watched for readable (for TLS: it keeps calling `give` with whatever arrives). A byte
or an end on an idle connection closes it at once (`close_reason` `unsolicited`); the package never lets such a byte reach the next
request's response. `tick` expires idle connections after `idle_ms` and any connection `lifetime_ms` after it was made.

**The retry** (hooks §53.4's rule, and the only automatic one): when a request on a **reused** connection fails with the transport
ended (`eof`), reset, or a write failure **before any response byte**, the client closes that transport and **replays the request on
a new connection, once**, under three conditions: the caller allowed it (`flags` bit 1: hooks passes it for every request, the
gateway for an idempotent method with no body only, since it alone knows), the request has not used a retry yet, and the request is
**replayable**: everything the client sent still fits in the output buffer, which it keeps until the first response byte (a streamed
body that has outgrown `out_size` stops being replayable, and the client says so by failing instead of retrying). A retried request
shows no event (`attempts(c,t)` is 1); a failure on a new connection is not retried; a timeout is **never** retried (the request
may be in progress). The request can reach the upstream twice when the peer processed it and closed without answering: the same
at-least-once caveat hooks states in §53.4, and the reason the flag is the caller's.

### 3.6 D6: timeouts, on the caller's clock

Five numbers in `Limits`, each in milliseconds, each `0` for none, each checked by `tick(c, now)` (which the caller calls once a turn)
and each findable in advance by `next_deadline(c, now)` (so the caller's `poller_wait` can be bounded by it: a timer that nobody
wakes for is not a timer):

| limit | runs | ends as |
|---|---|---|
| `connect_ms` | from `Event::Connect` to `connected` (**the handshake of TLS included**: the caller calls `connected` when the transport is ready for bytes) | `client.connect-timeout` |
| `head_ms` | from the request being fully sent, with no byte in or out for this long, while no head has arrived | `client.head-timeout` |
| `body_ms` | between pieces: no byte in for this long while the caller has **nothing waiting unconsumed** | `client.body-timeout` |
| `total_ms` | from `request` to `Done`, the whole exchange | `client.total-timeout` |
| `expect_ms` | an `Expect: 100-continue` head with no answer | `Event::Continue` (not a failure) |

A time the *caller* owns is not charged to the upstream: while the request is waiting for the caller's next piece of body, or the
caller has not consumed what arrived, neither `head_ms` nor `body_ms` runs (`consume` and `send_body` restart them). `total_ms` is the
bound on both. `idle_ms` and `lifetime_ms` (§3.5) are the pool's.

### 3.7 D7: TLS to upstreams

The client is **plaintext-in, plaintext-out**: it knows nothing about TLS, exactly as `http.server`'s byte-fed mode does not. What
TLS needs is the caller's, in the order `https_hello` and `tls_many` do it:

1. `Event::Connect(k)`: the caller dials the upstream's address (`tcp_connect_start`, an IP literal) and, when the poller reports it
   writable and `conns.connect_status` says 0, calls `tls.start(engine, k, name, now_unix_ms)` on the **client engine** it opened
   with the roots it was given (`tls.trust(pem)`: the caller's trust store, nothing built in), pumping `feed`/`take` until
   `tls.event(engine, k)` is `established`.
2. **The name is verified** by `tls.start`'s `host` argument, which is the name the upstream's certificate must carry; the address
   and the name are independent (the caller dialled an address it had resolved or been given; the certificate is checked against
   the name), which is what makes pinning an address possible (`tls-nonblocking.md` §3.8).
3. Only then does the caller call `http_client.connected(c, k, now)`; from then on `take` plaintext goes to `tls.send` and
   `tls.recv` plaintext goes to `give`. A failed handshake is `connect_failed(c, k, kind_tls(), now)` (`client.tls`; the engine's own
   refusal tag, `tls.refusal_tag(tls.failure(engine, k))`, is the caller's to log).
4. A connection that the client says to `Close` gets `tls.finish` (close_notify) and then the socket's close and `tls.drop`.
   An idle pooled TLS connection costs its 269,119-byte engine slot, so `slots` bounds TLS memory directly: 16 TLS connections are 4.3 MB.

**The driver is built in this PR** (`examples/http_fetch_nb/fetch_io.cho`): poller, `std.conns`, the dial, the TLS pump and the client
table in one loop, in the shape of `tls_many`'s `advance`/`flush`. It is an example module and not a package, for the reason
`tls_echo/front.cho` is: the second program that needs it moves it. The package itself stays free of `Net`, `Poller` and
`packages/tls`, so its authority report is the heap and nothing else, and an existing store that imports it gains no `Ffi` and no `tls`.

### 3.8 D8: DNS is the caller's

The caller resolves and gives an address. `Event::Connect(k)` names a **key**, not a host, and the caller's table says what to dial. The
reasons: (1) `tcp_connect_start` takes an IP literal without a lookup, and a name makes it call `getaddrinfo` and stall the loop
(measured, 302 ms for a 300 ms answer: `tls-nonblocking.md` §7); (2) the gateway has a fixed upstream set, so a name is resolved at
start or on a slow timer, not per request; (3) `rtcp.Resolver` is an example, not a package, and a package cannot depend on an example
file; (4) address pinning, and the check an SSRF rule wants, are only possible when the caller holds the address. A program that
needs a lookup per request uses `rtcp.Resolver` on the same poller (it is non-blocking), from its `Connect` handler: it reports a
failed lookup with `connect_failed(c, k, kind_resolve(), now)` (`client.resolve`) and the client treats it as a failed connect. The
example's `--resolve name=ip` is the caller-resolves case; `rtcp` is not wired in (§8).

### 3.9 D9: SSRF is not this package's concern

The client dials nothing: it never sees an address. A program that forwards a client-chosen destination (an open proxy, a webhook
sender) must judge the address *before* it answers a `Connect` (`cancho-hooks` `design.md` §26, `examples/tls_nb/pin.cho`); the
gateway has a fixed, compiled-in upstream set and its authority report says `net_out("")`. This package cannot widen what a program
reaches, because it reaches nothing but its own heap: **its authority report is `heap`** (`cancho authority`, §9, pinned in `conformance/http_client.rs`).

### 3.10 D10: bounds

Every number is fixed by `open`'s arguments and has a refusal: `slots` 1 to 1,024; `in_size` 2,048 to 16 MiB; `out_size` 1,024 to
16 MiB; a response head at most 16,384 bytes and 64 headers (`response.head-too-large`, `response.header`); at most 8
informational responses; a chunk-size line at most 128 bytes; trailers at most 8 KiB; a body of any size, since it is streamed;
`idle_per_key`, `max_requests` and the five timers of §3.6. The only counters that can grow without bound are the per-slot
generation and request counters; the generation is reduced modulo 2^40 so a ticket (`generation * 4096 + slot`) is never negative and
cannot overflow, and a request count is compared with `max_requests` and reset when the connection is.

### 3.11 D11: every refusal has a tag

A refusal is an integer code and `refusal_tag(code)` is its tag, as `tls.refusal_tag` is. §6 lists all of them. Three families:
`request.*` (what `request`/`send_body` refuse: the caller's mistake, answered at the call), `response.*` (what the upstream sent:
the gateway's names), and `client.*` (what happened to the connection: connect, timeouts, ends). A failure is `Event::Failed(t)` and
`failure(c,t)` is the code; `Close(k)` follows for the transport.

---

## 4. The API

(As built, §10.2 lists the differences from the design.)

```
pub res struct Client
pub struct Limits { connect_ms, head_ms, body_ms, total_ms, expect_ms, idle_ms, lifetime_ms, idle_per_key, max_requests }
pub enum Event { None, Connect(int), Continue(int), Head(int), Body(int), Done(int), Failed(int), Close(int) }

open(heap, slots, in_size, out_size, limits) -> Client          // out-of-range numbers are clamped into range
close(heap, client) -> int                    default_limits() -> Limits
request(c, now, key, method, target, host, extra, body, flags) -> int           // a ticket > 0, or 0 - code
send_room(c, t) -> int      send_body(c, t, bytes, now) -> int      end_body(c, t, now) -> int
poll(c, now) -> Event       tick(c, now) -> int                     next_deadline(c, now) -> int
connected(c, k, now)  connect_failed(c, k, kind, now)  take(c, k, out, now) -> int  pending(c, k) -> int  room(c, k) -> int
give(c, k, data, now) -> int   eof(c, k, now)   reset(c, k, now)   detach(c, k, now)
key(c, k)  ticket_of(c, k)  close_reason(c, k)  close_reason_tag(r)  ticket_slot(t)
status(c, t)  version(c, t)  body_kind(c, t)  content_length(c, t)  header_count(c, t)  head(c, t)  header_name(c, t, i)
header_value(c, t, i)  header(c, t, name)  body(c, t) -> &[byte]  avail(c, t)  consume(c, t, n, now) -> int
request_complete(c, t)  reused(c, t)  attempts(c, t)  continued_by_timeout(c, t)  failure(c, t)  refusal_tag(code)
abort(c, t, now)  retire(c, key, now)  close_idle(c, now)  idle(c)  active(c)  free_slots(c)  slots(c)
connects(c)  requests(c)  reuses(c)  retries(c)    body_none()  body_chunked()  flag_retry()  flag_expect()  flag_close()
kind_connect()  kind_tls()  kind_resolve()
```

**A turn of a caller's loop** is: read the clock; `poll` until `Event::None`, acting on each event; for every transport that is
readable and has `room`, read and `give`; for every slot with `pending`, `take` and write (watch it for writable only while
`pending` is not 0); `tick`; wait for the poller up to `next_deadline`. The example's `fetch_io.cho` is exactly that.

---

## 5. What the unit tests can do without a socket

The client is bytes in, bytes out, and a clock that is an integer; that is the point of the shape. A test is a `fn test_*` run by
`cancho test` on both backends (`tests/packages/http_client_test.cho`, pinned by `conformance/http_client.rs`) that drives a client as
the caller's loop would and plays the upstream by hand:

* every response framing (length, chunked, until close, none for `HEAD`/`204`/`304`/`1xx`), **each fed split at every byte** and in
  pieces of 1, 2, 3 and 7 bytes, and the same bytes in one piece, with the body checked byte for byte;
* every refusal with its tag (each `response.*` and each `request.*`), including the heads that are one byte off a valid one;
* `100 Continue` and other `1xx` skipped, `Expect` held and released (by a `100`, by the timeout, and by a final response);
* a response before the request is sent;
* pooling: reuse of the most recent connection, a different key getting its own, `Connection: close`, `HTTP/1.0`, close-delimited,
  leftover bytes, eviction of another key's idle connection, `idle_per_key`, `retire`, expiry by idle time and lifetime and by
  request count; **the one retry**, on a reused connection before any response byte, with the flag and without it, once and not
  twice, not after a response byte, not on a fresh connection, not when the request has outgrown the buffer, not on a timeout;
* timeouts on a fake clock: each of the five at the millisecond they fire and the millisecond before, and that a caller's own slowness is not charged;
* back-pressure: `room` 0 while a body is unconsumed, a body bigger than `in_size` flowing through a small buffer, an upload bigger
  than `out_size` flowing through a small one, a tiny `out_size` that refuses a head;
* a seeded random run of tens of thousands of operations (any call, in any order, with any arguments, including stale tickets and
  slots out of range) that must never trap and never break a bound.

TLS is tested through the driver, not on its own: the client and the repository's own server engine (`examples/https_hello`, on
`packages/tls`'s server) meet in `conformance/http_fetch_nb.rs` and in the live `hello` case, and OpenSSL (Python's `ssl`) in the live
`tls` case. *(Corrected: §5 first promised a test that connects a client `Engine` and a server `Engine` back to back in memory
(`tests/programs/http_client_tls.cho`); it was not built, because the example's driver is what has a TLS boundary to test and it is
covered against two different servers.)*

---

## 6. Rule tags

`request.method`, `request.target`, `request.host`, `request.header`, `request.length`, `request.too-large`, `request.key`,
`request.body-too-long`, `request.response-arrived` (`send_body` after an early final response), `request.no-body`, `request.ticket`,
`client.full`; `response.status-line`, `response.version`, `response.status`, `response.upgrade`, `response.header`, `response.fold`,
`response.length`, `response.two-lengths`, `response.transfer-encoding`, `response.head-too-large`, `response.chunk`,
`response.trailers-too-large`, `response.too-many-informational`; `client.connect`, `client.tls`, `client.resolve`,
`client.connect-timeout`, `client.head-timeout`, `client.body-timeout`, `client.total-timeout`, `client.closed-early` (the transport
ended before any response byte), `client.truncated` (it ended inside a head or a body), `client.reset` (it failed), `client.aborted`
(reserved: an `abort` reports nothing). Close reasons (not failures) are numbers with names too: `connection-close`, `not-reusable`,
`idle-expired`, `lifetime`, `retired`, `evicted`, `unsolicited`, `peer-closed`, `failed`, `retry`, `aborted`, `max-requests`,
`pool-full`.

---

## 7. Cost, stated as a prediction to be checked

`http-server.md` §11.7 measured the server side: 5.6 µs of CPU a request plain (`examples/api`) and 23.4 µs over TLS 1.3 on CI's
x86-64 core, 2.7 µs and 9.8 µs on the aarch64 VM. The client does the mirror work: one parse of a response head (about the same bytes as
a request head), a copy in, a copy out. **Prediction: the client alone, against a local server, is the same order, 5 to 15 µs a request
plain, and a TLS keep-alive request is that plus the record work (about 18 µs on the x86-64 core, §11.7's difference), so 25 to 40 µs.**
§9 states the gate; the numbers are in §10.5. `cancho-hooks` §53.1's arithmetic (a kept TLS delivery 30 to 40 µs) is the same figure.
*(Corrected: the prediction was high. Measured on the aarch64 VM, the client costs 2.9 µs a request plain and 7.5 µs over TLS, and 5 to 6
and 11 on the Mac; §10.5 has the x86-64 figures from CI. The estimate added the server's TLS surcharge to a client that does less than
the server: it has no routing, no answer to build and, in `fetch_io`, no JSON.)*

---

## 8. What is not here

* **Upgrade and `CONNECT`** (`101` refused; the gateway's item 5). A connection that becomes a tunnel needs `detach` plus the bytes
  already buffered behind the head handed over; the client has them (`avail`), but the hand-over is not built.
* **`rtcp` wired in.** DNS is the caller's (§3.8); an example that resolves on the poller is a follow-up once a second program wants it.
* **HTTP/2, trailers delivered to the caller, request pipelining, a connection per request on a `Connection: close` upstream
  faster than the pool allows, proxies (absolute-form targets), `Content-Encoding`, redirects, cookies, retries of anything but §3.5's.**
* **TLS session resumption in the driver.** `tls.save`/`start_with` exist; the example does a full handshake per new connection. The
  pool makes that rare (a handshake per connection, not per request) and §10 measures what it costs.
* **More than one core.**

---

## 9. Gates, stated before the numbers

1. **Unit tests** of §5 pass on both backends; every refusal tag of §6 is reached by a test that names it.
2. **Mutants** of the new code (`scripts/http_client_mutants.py`, as `http_server_bytes_mutants.py` does): each killed, or argued equivalent in the script; a survivor that is not equivalent gets a test.
3. **Live**, over plain TCP and TLS 1.3, against a Python server on raw sockets, so that every byte it sends is its own (keep-alive, chunked, close-delimited, slow, early responses, `Expect`, damaged responses), `examples/https_hello` (this repository's own TLS server on `http.server`), and nginx where it is installed (curl's test server was not used: nginx ends connections the way a real upstream does): `scripts/http_client_test.py`. Connections are **counted at the server**: 1,000 requests to one upstream make no more than `slots` connections plus the retries a test forced.
4. **No input reaches a trap:** the random run, a hostile-upstream run (a million mutated responses through the parser, split at random), and `cancho check` of the package with the checker's own bounds.
5. **Cost:** requests a second and CPU a request through the client against a local server, plain and TLS 1.3 keep-alive, on one core, with the figures that `http-server.md` §11.7 uses for its comparison.
6. **Authority:** the package's own report is the heap and nothing else; the example's report is pinned in `conformance/http_fetch_nb.rs` as `tls_echo`'s is.
7. `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, and `scripts/publish_packages.py --check`; CI's `tls-assurance` job runs the live tests.

---

## 10. As built

Built in this PR; the sections above are corrected in place where the build differed, each marked *corrected*.

### 10.1 What is in the repository

| | |
|---|---|
| `packages/http-client/` | three modules, published as stores by `scripts/publish_packages.py` like `packages/tls`: `http_client_slot` (285 lines: the names of the words of a slot and the numbers they hold), `http_client_wire` (1,105: validating and writing a request head, the response head parser, the incremental chunk decoder) and `http_client` (1,711: the table, the state machine, the pool, the timers, the events). `http_client` was 1,988 lines before the slot layout moved out of it |
| `examples/http_fetch_nb/` | `fetch_io.cho` (the driver of §3.7: sockets, TLS engine, poller; the one place that owns them) and `fetch.cho` (a program that fetches N URLs at once with it, with the options the live tests need) |
| `tests/packages/http_client_*.cho` | 118 `cancho test` tests in seven files, run on both backends by `conformance/http_client.rs` |
| `tests/programs/http_client_{bytes,fuzz,bench}.cho` | the package with no I/O (its authority pinned), a million damaged responses through it, and its cost with nothing around it |
| `scripts/http_client_{upstream,test,mutants}.py` | the servers the live tests use, the 17 live cases and the cost, and 517 mutants |
| `crates/cancho/tests/conformance/http_{client,fetch_nb}.rs` | the unit tests on both backends and the package's authority; the example built and run against a server written in the test and against `examples/https_hello`, and its authority pinned |
| CI | the `tls-assurance` job builds the example and runs the live tests, the fuzz, the bench, the cost and the mutants (`ci.yml`) |

### 10.2 The API as built, against the design

The calls of §4 exist with these differences *(corrected)*:

* `poll(c, now)`, `detach(c, k, now)`, `consume(c, t, n, now)`, `send_body`, `end_body`, `connected`, `connect_failed`, `eof`, `reset`,
  `abort`, `retire`, `tick`, `take` and `give` take the caller's clock. `next_deadline(c, now)` answers the milliseconds to the next timer,
  0 if one is due, -1 if there is none.
* Added: `close_idle(c, now)` (every idle connection is told to close: the program is ending), `ticket_of(c, k)`, `close_reason_tag`, the
  counters `connects`, `requests`, `reuses`, `retries`, `continued_by_timeout`, and `default_limits()`. `Limits` is a struct literal (cancho
  cannot assign one field of a value), so a caller writes all nine fields.
* `request` answers `0 - code`; `client.full` is `-10`. A request is refused whole: nothing about the client changes on a refusal.
* `Event::Done` comes **after** the last byte of the body was consumed (§3.4), and a response complete but unconsumed holds its slot.
* A **free slot with an event the caller has not seen is not given to a new request** (`free_slot` and `pooled` need `events == 0`): a
  caller that detaches a slot before it polled the `Failed` for it would otherwise lose that failure. A request that takes such a slot
  once it is seen is not queued twice (a full queue of N events would otherwise overwrite its oldest).
* A **closing slot takes what it is given and keeps none of it** (`give` answers the length): a caller's loop that reads once more after
  it learned the transport was closing neither loops nor feeds a dead request. A free or dialling slot answers -1.

### 10.3 Decisions the build added

* **A body that runs to the end of a TLS connection needs close_notify.** The driver reports a TLS connection whose socket ended without it as
  `reset` (`client.reset`), not `eof`: RFC 8446 §6.1 says the data may have been cut, and for a body with no length the end of the
  connection is the only thing that says it is whole. A body with a length or chunked is whole without it, and the connection is just not kept.
  `scripts/http_client_upstream.py` ends its TLS connections properly (`unwrap`) except on `/closeabrupt`, which the live test uses.
* **`idle_per_key` bounds the pool, and it is the first thing a benchmark trips over.** Thirty-two lanes on one upstream with the default of 8
  idle connections made 12,957 connections for 81,536 requests (the 24 that did not fit were closed at each `Done` and dialled again), and
  24 of them failed with `client.connect` (the kernel ran out of ports to hand out). The cost rows pass `--pool 32`.
* **The driver must not spin.** The first `fetch_io.step` treated "bytes waiting for the socket" as work to do, so an upload to a peer that
  reads nothing used 0.55 s of CPU in 3 s asking the kernel again and again. A pump now says whether it moved anything, a blocked socket is
  the poller's to wake, and `nospin` in the live tests fails the old loop (0.35 s) and passes the new (0.00 s).
* **A write that fails does not lose the response.** A server that answers `413` as soon as it has the head and closes with the upload unread
  resets the connection while the client is still writing; its answer is in the client's receive buffer. The driver marks the write side dead,
  drops the request and goes on reading (`earlyrst` in the live tests; the loop that reset at the first failed write lost the answer in one run
  of three). RFC 9112 §9.6 is why a *server* should not do this; a client should survive one that does.
* **The driver watches an idle connection for readable** and gives what arrives to `give`, so a server that closes a pooled connection is noticed
  when it happens (`Close`, reason `peer-closed`), not when the next request finds it dead. The retry of §3.5 is for the race that remains.
* **Bytes behind a complete response are noted, not kept:** if they arrive in the same read they are swallowed and the connection is closed, not
  pooled. A server that sends a second response nobody asked for is not believed.

### 10.4 Tests, against the gates of §9

1. **118 unit tests**, on both backends, the number pinned in `conformance/http_client.rs`: every framing (length, chunked with an extension and
   a trailer, until close, none for `HEAD`/`204`/`304`, `HTTP/1.0`, interim responses) **fed in pieces of 1, 2, 3, 5, 7 and the whole**; a body of
   60,000 bytes through a 2,048-byte buffer in either framing to a consumer that takes 100 bytes a turn; 55 refused responses (each fed at three
   piece sizes) and 18 refused chunk framings (at every piece size) with their tags; 34 refused requests; the one retry (allowed or not, on an end
   or a reset, with a body, past the buffer, after a response byte, on a timeout, on a fresh connection); the pool (reuse, the newest, the oldest
   evicted, `idle_per_key`, `retire`, expiry by idle time, life and request count, to the millisecond); every timer at the millisecond it fires and
   the one before, and the caller's slowness not charged; `Expect` released by a `100`, by the wait and by a final response; an early response; a
   stale ticket and every wrong handle; and 16 seeds of 4,000 random operations of every call that must not trap or break a bound. Every tag of §6
   that a request or a response can reach is reached by a test that names it.
2. **Mutants: 461 of 517 killed, 56 argued equivalent, none survives** (`scripts/http_client_mutants.py`, which breaks `wire.cho` and `client.cho` one
   site at a time and runs the tests, as `http_server_bytes_mutants.py` does). The first run killed 356, left 153 alive and had 8 whose text no longer
   matched. What the survivors showed was in the tests, not the package: a harness whose byte assertions carried the label 0 and so could not
   fail (**every `assert_bytes` was inert**; found because `read_body: a body until close counts nothing` survived), a pool test that ended every
   connection with an `eof` and so could not see one pooled that should not be, an `open` test whose failure the next part overwrote, and the
   places the tests walked past: the boundary of every limit, every name the client owns, a header ending in a bare CR, `PUT` and `PATCH` with a
   body of 0, a hex size in lower case, a chunk size past 2^50, the trailer bound to the byte, the Expect clock starting at the last byte of the
   head, a replayed request not complete until it is sent again, a slot freed with events waiting being queued twice, a short final head
   behind interim ones that arrived with the end of the last of them. Each got a test. The 56 equivalents are each argued in the script (a check
   that another makes redundant, a state no caller can reach, a change only in how much is scanned); the one a test cannot show is `open`'s
   1 GiB budget, which was run once by hand.
3. **Live, over plain TCP and TLS 1.3** (`scripts/http_client_test.py`, 17 cases, all passing on macOS and on Linux aarch64 in Docker with nginx
   installed): the framings at sizes from 0 to past the input buffer, alone and 40 at once; 1,000 requests over four URLs that make four
   connections, **counted at the server**; 40 URLs through 16 slots; three upstreams through two slots (an idle connection closed for another
   upstream, and the slot dialling again); 50 MB byte for byte; uploads of 0 bytes to 5 MB with a length and chunked, hashed at the server;
   `Expect: 100-continue` answered, waited out and refused; a 413 read while 5 MB were being sent, and after a reset; the retry (2 connections and
   3 requests with `--retry`, `client.closed-early` without); ten kinds of refused response, each with its tag; timeouts; a name the certificate
   does not carry and roots that are not its CA (`client.tls`); `examples/https_hello` (300 requests on 3 TLS connections, 3 handshakes, each
   ended with close_notify); nginx ending connections with `Connection: close` after 5 requests; a blocked upload that must not spin; and a
   server that damages every response (flipped bytes, cuts, junk, in pieces of 1 to 100,000 bytes) to 640 connections: the client lives and every
   lane ends.
4. **No input reaches a trap:** `tests/programs/http_client_fuzz.cho` feeds **a million damaged responses** through the whole client (0.4 s on the Mac),
   in pieces of one to forty bytes and consuming the body 7 bytes at a time so the buffers shift constantly: 330,281 end in `Done`, 669,719 in `Failed`
   with a tag, none is stuck, none traps (the same counts on macOS and on Linux, which is the point of a generator with a seed).
5. **Cost** is §10.5.
6. **Authority:** the package with a program that makes a request and reads a response reports `heap` and `io_write` (the program printing)
   and nothing else, `bounded`, no foreign code (`the_package_reaches_the_heap_and_nothing_else`); the example reports `args`, `clock`,
   `conn_read`, `conn_write`, `err_write`, `fs_read("/dev/urandom")`, `heap`, `io_read`, `io_write`, `net_out("")` and `poll`, bounded, no
   foreign code, pinned in `conformance/http_fetch_nb.rs` like the TLS echo's.
7. **The gate:** `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` clean; `cargo test --workspace --no-fail-fast` on macOS
   (arm64) and on Linux (aarch64, in Docker: x86-64 is CI's) with the two tests that need a git checkout failing in a worktree and everything
   else passing; `scripts/publish_packages.py --check` clean.

### 10.5 Cost

`scripts/http_client_test.py --cost 5 --pin --server <examples/api> --https-hello <https_hello>`: 32 lanes on 32 connections, one request out on each,
closed loop, the **client** on one core and the **server** on another (`taskset`), three runs of about five seconds; the client's CPU a request is
its `getrusage` divided by the requests, the median of the three. Plain is `GET /users/42` against `examples/api` (a 26-byte JSON answer);
TLS 1.3 is `GET /hello/42` against `examples/https_hello` (99 bytes with its head). `--quiet`, so nothing is printed or hashed.

| | requests a second, three runs | the client's CPU a request |
|---|---|---|
| **aarch64 Linux** (Docker Desktop's VM on an Apple M4 Max, 6 vCPUs, other containers running; cores pinned) | | |
| plain, keep-alive | 324,772 313,368 303,421 | **2.91 µs** |
| plain, a connection for each request (`--close`) | 29,158 49,934 50,588 | 13.09 µs |
| TLS 1.3, keep-alive | 123,094 119,944 125,213 | **7.48 µs** |
| TLS 1.3, a handshake for each request (`--close`) | 101 101 101 | **3,226 µs** |
| **macOS, M4 Max, unpinned**, with a load average of 15 to 30 from other jobs | | |
| plain, keep-alive | 145,015 170,287 157,179 | 5.68 µs |
| plain, a connection for each request | 22,266 30,336 29,094 | 20.16 µs |
| TLS 1.3, keep-alive | 87,379 85,616 81,705 | 10.70 µs |
| TLS 1.3, a handshake for each request | 97 97 97 | 3,069 µs |
@@X86@@

So **a client core serves about 300,000 plain keep-alive requests a second and about 120,000 over TLS 1.3** on the aarch64 VM, against the server's
2.7 µs and 9.8 µs a request (`http-server.md` §11.7, same VM): the client costs about as much as the server it talks to, which is what a mirror
should. What TLS adds to a kept request is **4.6 µs** (the records), and a *new* TLS connection costs **3.2 ms** of the client's CPU, 430 times a kept
request: the pool is the feature, and `idle_per_key` the setting that decides whether it works (§10.3). The VM is not quiet (the same command measured 150,918 / 139,480 /
156,006 plain and 61,494 / 53,419 / 57,126 over TLS with the host busier, 3.42 and 8.15 µs): the CPU a request is the figure to read, and it moved
by up to 18% where the rates moved by a factor of two.

**The state machine alone** (`tests/programs/http_client_bench.cho`: `slots` requests in flight on one upstream, each answered with a canned
response fed in one piece, the body consumed, the next request made on the pooled connection; no socket, no TLS, 400,000 requests):

| slots | 8 | 64 | 256 | 1,024 (macOS) |
|---|---|---|---|---|
| per request | 0.23 µs (93 ms) | 0.29 µs (114 ms) | 0.50 µs (201 ms) | 1.23 µs (491 ms) |

(a 4,000-byte body instead of 26: 0.34 µs at 8 slots). So of the 2.9 µs a request costs in the loop, **0.25 µs is the client**; the rest is
`read`, `write` and `epoll`. The growth with slots is the O(slots) scans of the pool (`pooled`, `free_slot`, `oldest_idle`) and of `tick` and
`next_deadline`, in the bench's transport loop as well: about 1 µs at 1,024 slots.

### 10.6 Memory a connection costs

Fixed when the client and the driver are opened; nothing is allocated per request or per connection.

| | bytes a connection |
|---|---|
| the client's slot | `in_size + out_size` + 48 words of state + 272 words of parse table + 1 word of queue = **84,488** with the example's 65,536 and 16,384 |
| the driver's buffers (`fetch_io`) | 32,768 read + 16,640 plaintext received + 16,384 request + 32,768 write + 20 words = **98,720** |
| the TLS engine's slot, an `https` upstream only | **269,119** |

So a plain connection is **183,208 bytes** (179 KiB) and a TLS one **452,327** (442 KiB): 64 TLS connections are 28.9 MB, 17.2 MB of which is
the engine. The live `big` case moves 50 MB through a client of 8 slots in 2 to 3 MB of resident memory on macOS (19 to 20 MB on Linux, which
counts the program's pages differently).

### 10.7 What `cancho-gateway` still needs

In the order it will hit them, on top of `http-server.md` §11.8:

1. **The server's streamed request body** (the other half of piping an upload: this client's `send_room` is the back-pressure to read the
   client's socket by).
2. **A chunk writer on the way out**, for a response of unknown length to a client that speaks 1.1 (`http-server.md` §11.8 item 2). The client
   dechunks; the server side must frame.
3. **Upgrade and `CONNECT`.** A `101` is refused (`response.upgrade`). A tunnel needs the client to hand over the slot and the bytes already
   buffered behind the head; `avail` has them, the hand-over is not built.
4. **Resolving names** without stopping the loop: `rtcp.Resolver` becomes a package and a driver module uses it from its `Connect` handler;
   until then the gateway resolves at start and calls `retire(key)` when an address changes.
5. **The driver as a package** (§11 question 5): `fetch_io.cho` is the loop of `https_hello` turned to the client's side; the gateway is the
   second asker for it.
6. **A per-request timeout** (§11 question 6) and the gateway's `total_timeout` per route: one `Client` per timeout class today.
7. **TLS session resumption to upstreams.** `tls.save`/`start_with` exist and the driver does not use them: a full handshake is 3.2 ms of the
   client's CPU (§10.5), paid once per connection, not per request. A gateway with many upstreams and short-lived connections would want it.
8. **The pool's scans are O(slots)**: about 1 µs a request at 1,024 slots (§10.5). A free list and a per-key list are the change, and nothing asks yet.
9. **Access logging of the upstream leg** (address, reused, attempts): `reused(c,t)` and `attempts(c,t)` are there; the address is the driver's.
10. **A write that fails mid-body with no answer** surfaces as `client.reset`, after the response was looked for and not found; the gateway answers
    its client `502` for it, as it does for `client.closed-early`, and retries nothing it has sent a body for.

---

## 11. Open questions for a person

Each has the answer this PR proposes and builds on; none blocks.

1. **Is `HTTP/1.0` never reused the right strictness?** The gateway reuses `1.0` with `keep-alive`; hooks never does.
   *Proposed:* never (as hooks, and as §3.5), since the upstreams that matter speak 1.1.
2. **Should the client retry a non-idempotent request?** *Proposed:* never by itself: it is the caller's flag (§3.5). The gateway
   passes it for an idempotent method with no body (as `pool.md` §3), hooks for every request.
3. **Should `101`/`Upgrade` be a first-class body kind now?** *Proposed:* no, refuse it with a tag (`response.upgrade`) and build it
   with the gateway's WebSocket slice, where its requirements (what the client does with the leftover bytes, what `Close` means for
   a tunnel) are known.
4. **Should DNS live in the package?** *Proposed:* no (§3.8); `rtcp.Resolver` becomes a package when a second program needs it, and
   then a driver module can depend on it.
5. **Where does the I/O driver live?** *Proposed:* `examples/http_fetch_nb/fetch_io.cho` for now (§3.7); it becomes a package
   (`http-client-io`, which would import `tls`) when the gateway adopts it, because that is the second asker and its loop shows what
   a shared driver must not assume.
6. **Per-request timeouts.** The limits are the client's, set at `open`. The gateway has them per route. *Proposed:* a per-request
   override is a later addition (a `request` flag word has room); until then the caller opens one client per timeout class, which
   costs only the buffers.
7. **Is `request` returning `client.full` acceptable, or should the client queue?** *Proposed:* return it; a queue is a policy
   with its own bound, the caller already has one (`cancho-gateway` queues clients in its own table).
8. **A body that runs to the end of a TLS connection with no close_notify is `client.reset`** (§10.3). *Proposed:* yes, as RFC 8446 §6.1 says, and
   the driver is where it is decided (`eof` or `reset`), so an upstream that never sends close_notify gets a flag in the driver, not a change in
   the package. No server met in the live tests does that except the one built to.
9. **`idle_per_key` is 8 unless set.** *Proposed:* keep, and have the gateway set it per upstream from the concurrency it allows; a benchmark that does
   not (§10.3) measures the pool churning, not the client.
