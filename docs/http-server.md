# `http.server`: the server loop as a package

> **Status: built (§7).** `examples/api/api.cho` carried the whole
> loop -- accept, read, parse, frame, pipeline, back-pressure, idle sweep --
> with the application's routes inside it. A second server would copy 600
> lines. This extracts the loop; `docs/server.md` stays the account of what the
> loop does and costs, this says where it lives and why it has this shape.
> **§11 (byte-fed mode, so the loop can sit behind TLS) is built too**, and `examples/https_hello` is the HTTPS server made of it and
> `packages/tls`.

## 1. What was asked

Take the loop out of the example so a server is its routes and a `main`. It is
the piece `cancho-web` would build on, and the first package that imports
`std` (`package-system.md` §4.8).

## 2. The obvious shape does not type-check

The first design is `serve(heap, router, handler, ...)`: the loop calls a
handler it was given. Lex has function values (`function-values.md`), so this
looks free. It is not, and the reason is measured rather than assumed:

```
fn drive[&k](heap: &!k Heap, h: fn(&k [byte]) -> [] int) -> [heap] int {
    let b = box_slice(heap, 8, byte_of(0));
    borrow mut b as &!bw in { let c = contents(bw); n = h(c[0..4]); }
```

is refused -- *`bw` does not outlive `k`, so a reference valid for the first
cannot be used where the second is expected*. A function value's type may
mention only regions already in scope where it is written
(`function-values.md` §4.2, "a type quantified over its own regions (rank 2) is
refused"), and the request the loop would hand a handler lives in a buffer the
loop borrowed *itself*. The handler's parameters would need a region the caller
cannot name. So a callback cannot receive a view of the loop's own buffers, and
copying every request into an owned value to get round that costs an
allocation per request on the path the benchmark measures.

## 3. The shape that does: the loop is the application's

The package owns the state and the work that has no business being rewritten
-- sockets, framing, pipelining, back-pressure, timeouts -- and returns control
at the one place the application has something to say:

```
let srv = server.open(heap, poller, listener, size, chunk, idle);
while true {
    srv = server.wait(heap, srv, clock, listener, 1000);     // I/O, once
    while (k = server.next(heap, srv)) >= 0 {                // a request is in hand
        // server.head / parsed / body: views of the loop's buffers
        // ... the application builds its answer ...
        server.respond(srv, answer);                          // send, or queue
    }
}
```

The views are ordinary borrows of the `Server` (`&s`), valid inside the
`borrow` that reads them -- which is what a rank-1 region system is good at, and
what a callback is not. The application's own state is its own variables, which
a callback with no closures could not reach at all (`function-values.md` §4.1).

What stays in the package: everything `drain`, `emit` and `serve_on` did. What
moves out: the route table and the handler, which were never the loop's.

## 4. The contract

* `open` takes the `Poller` and the listener and registers the listener; the
  server owns the `Poller` from then on and `close` ends it.
* `wait` is the one place that blocks (at most `timeout_ms`). It accepts,
  reads, writes what was queued, sweeps idle connections, and leaves a list of
  connections that hold input. It takes and returns the `Server` by value
  because accepting replaces the connection table (`std.conns.put` grows it).
* `next` answers the slot of a connection holding one complete request, parsed
  and (if chunked) decoded, or `-1` when none is left. Refusals the loop owns --
  400 for a request the parser rejects, 413/431 for one that cannot fit -- are
  answered inside `next` and never reach the application.
* `respond` appends the answer to the current connection: straight to the
  kernel if nothing is queued ahead of it, otherwise to the connection's output
  buffer, in order. Once a connection has output waiting `next` takes no more
  requests from it: back-pressure is the loop's, not the application's.
* The application must call `next` until `-1` before the next `wait`. A request
  skipped would sit unanswered; `wait` therefore finishes any connection
  `next` left half-visited and re-queues unvisited ones rather than losing
  them.

## 5. What does not change

The behaviour. Every test in `conformance/api.rs` runs against the migrated
`examples/api` unchanged, and the throughput and tail-latency figures in
`server.md` are re-measured (§7).

## 6. What it does not do

Streaming bodies, `Expect: 100-continue`, TLS and more than one core are
`server.md` §6's list and remain the list. Handlers see a request whole.
(*TLS: §11 is the byte-fed mode that lets a TLS terminator drive the loop. Streaming a *response* is `stream` there; streaming a request is still not.*)

## 7. Built

`packages/http-server/server.cho` (module `http.server`, published with `vcs
publish --std` into `packages/http-server/.cancho-vcs`), and
`examples/api/api.cho` rewritten onto it: the example is its routes, its
handlers and a 60-line `run` loop, and consumes the package through
`examples/api/server.lock` like every other package consumer.

**The tests.** All 21 `conformance/api.rs` tests pass unchanged against the
migrated server -- keep-alive, pipelining, split requests, slow readers,
vanishing clients, chunked and oversized bodies, refusals, idle timeouts. The
contract's one clause `api` does not follow (`next` until `-1` before the next
`wait`) has its own test over `tests/programs/server_one_per_round.cho`: four
connections, six pipelined requests each, one `next` per `wait`. It **found a
bug the first time it ran**: the connection `next` was part-way through was
finished (its input compacted) but never queued again, and no input would
arrive to wake it, so five of its six requests were never answered. `wait` now
re-queues it when requests remain buffered. Without that fix the test fails;
with the unvisited-connections shift broken it fails too (both checked by
mutation). A second test holds the checked-in store to the checked-in source,
so an edit that is not re-published is red.

**Throughput.** Same machine, same session, the old and new binaries
alternated, server pinned to core 0, `kload` 2x16 connections on cores 2-3:

| | requests a second (three 5 s rounds) |
|---|---|
| before (loop inside `api.cho`) | 71,446 70,979 73,180 / 73,926 70,432 71,683 |
| after (`http.server`) | 81,516 78,185 76,806 / 76,566 76,640 76,755 |

It is not slower; it measured about 7% faster. Why was not investigated, and
the two series overlap in one round, so the claim is "no regression", not
"a speed-up".

**What it cost the application.** The `borrow` nesting moved. Reading a
request is a `borrow srv as &sr` around the handler, sending is a
`borrow mut srv as &!sw` around `respond`, and the answer lives in a buffer of
the application's own between them. The application's variables (the router,
the parameter slots, the output buffer) are plain locals, which is what a
callback could not have given it.

**Left over.** `wait` takes and returns the `Server` by value, because
`std.conns.put` consumes the table to grow it. A `put` that works in place on a
table made with its final capacity would make `wait` take `&!s Server` like the
rest; it is a change to `std.conns` and nothing here asked for it. The package
publishes every declaration in its file, helpers included (`vcs publish`
cannot yet publish only the `pub` ones), so the lock pins the interface and
the rest is reachable but not depended on.

## 8. Reply helpers added after a real service used it

`cancho-web`'s `examples/users` (a CRUD API over this package and `cancho-schema`)
needed three things the package did not have, each now here:

* **`reply_as(heap, out, status, content_type, body, keep, extra)`.** `reply`
  hard-coded `application/json`, so an RFC 9457 `application/problem+json` answer
  had to be assembled by hand. `reply`/`reply_with` are now `reply_as` with that
  type, and nothing that called them changes.
* **`reply_empty(heap, out, status, keep, extra)`**, a `204`/`304` with no
  `Content-Length` and no `Content-Type` (`std.http.respond_no_content`), so a
  `DELETE` can answer `204`.
* `json.put_fragment` (in `std.json`, not here) for assembling a response from
  stored JSON.

`conformance/http_server.rs::replies_of_any_type_and_bodiless_ones_keep_the_connection_framed`
sends four pipelined requests down one connection -- JSON, `204`, `text/plain` with
an extra header, JSON -- and checks the exact bytes: a wrong frame on the bodiless
one would shift every answer after it. The store was republished (a changed body is
refused by incremental publish, so it is regenerated), and `examples/api/server.lock`
re-pins it with the two new names.

## 9. What a page endpoint found in `std.buffer`

`cancho-web`'s `GET /users?limit=20` answered 40,000 requests a second against 99,900 for a
hand-written C server (`cancho-web/docs/benchmarks.md`), the widest gap of its four workloads.
Two causes, one in the application and one here, measured one after the other on the same
machine in the same session (page of 20 users, 1.8 KB, server on core 0, `kload` on cores 2-3,
three 5 s rounds each):

| | requests a second |
|---|---|
| each stored user re-parsed by `json.put_fragment` | 39,404 40,073 40,051 |
| the page spliced as bytes (the users are the program's own canonical output) | 59,830 64,873 63,353 |
| ... and `buffer.append` storing in one pass | 70,931 70,144 70,944 |

The first is the application's, not this repository's: it was validating text it had itself
rendered. The second is `std.buffer.append`, which was `push` once per byte, and `push` checks the
capacity and rebuilds the `Buffer` every time. Appending `n` bytes now makes room once and stores
`n` times (`tests/lex/buffer_test.cho` checks it at every capacity boundary and fails under two
deliberate mutations). Cost per extra user in a page was about 0.36 microseconds, roughly 4 ns a
byte, before the change, which is what two byte-at-a-time copies of a response (into the page, then
into the reply) would cost; that reading was not profiled.

Left: both copies remain byte loops. `bulk-io.md` §4 names the primitive that would fix it for the
whole library (a slice-to-slice copy) and leaves open whether that is the compiler's or the
backend's to do; this change does not decide it.


## 10. Holding a request, and handles of the application's own

A request whose answer is not ready -- it waits on a database -- used to have one choice: block in
`next`'s caller until the answer came, and stop every other connection. Two additions let the
loop go on, and neither makes the server know what it is waiting on (`cancho-pg/docs/nonblocking.md`
is the first user).

**`hold` and `answer`.** `hold(srv)` takes the request `next` handed over and answers a *ticket*
instead of leaving it to `respond`; the loop goes on to the next request. `answer(srv, ticket,
bytes)` is `respond` for that request, at any later time. What the server does meanwhile:

* A held connection is not read from once its buffer is full (the rest of what the client sent
  stays in the kernel), and no request behind the held one is handed out until it is answered: the
  answers must leave in the order the requests came, so the next request on that connection waits.
  Requests already buffered behind it are served after the answer without waiting for new input.
* It is not closed for being idle: the answer is the application's to give, and so is giving up
  (an application that wants a deadline answers with `504`).
* A ticket is a slot and a *generation* of that slot (`ticket % 2048`, `ticket / 2048`; the
  generation increases every time a connection takes the slot). `answer` answers `-1` and writes
  nothing for a ticket that was answered already, or whose connection is gone and whose slot may
  now be someone else's. The `-1` is the whole protection against writing an answer to the wrong
  client, so it is checked, not trusted.

**`poller`, `first_token`, `foreign_count`, `foreign`.** `poller(srv)` is the server's own
poller. The application registers its own handles in it (`std.conns.watch` with a token from
`first_token(srv)` up: 0 is the listener, 1 to `limit` the connections), so that one `wait` serves
them and the connections together. What `wait` reports for those tokens is not acted on; it is
listed by `foreign_count`/`foreign` as (token, readiness) pairs, valid until the next `wait`, for
the application to read from the handle and answer tickets. The list is 64 pairs; a pair beyond
that is dropped, which is safe only for handles watched level-triggered (the poller reports them
again next time), which `std.conns.watch` is.

`tests/programs/server_hold.cho` is the application used in `conformance/http_server.rs`: it
watches a connection to the test and answers one held request for every byte the test writes.

| test | what it fixes |
|---|---|
| `a_held_request_does_not_stop_the_loop_and_what_follows_it_waits_its_turn` | other clients are answered while one is held; a request pipelined behind it, and one written later, wait and come out in order after it |
| `held_requests_are_released_one_each_and_oldest_first` | three held requests; each release answers one |
| `a_held_connection_with_a_full_buffer_waits_without_reading_and_loses_nothing` | 2,500 requests (about 80 KB) behind a held one, 5 buffers' worth: all answered, in order |
| `a_held_request_is_not_the_idle_timeouts_to_close` | held for 12 s, the idle timeout being 9 |
| `a_ticket_answers_once_and_never_reaches_a_connection_that_took_the_slot` | the same ticket twice; and, after the connection is gone and another holds a request in its slot, the old ticket does not answer the new one |

Nine single-edit mutations of the new code, each published to a scratch store and run against
all of the tests above; every one is caught:

| mutation | first test to fail |
|---|---|
| `answer` does not check the generation | `a_ticket_answers_once...` |
| `answer` does not check that the request is held | `a_ticket_answers_once...` |
| the generation is not increased when a slot is taken again | `a_ticket_answers_once...` |
| a held connection with a full buffer is still read | `a_held_connection_with_a_full_buffer...` |
| a held request is produced again when more input arrives | `a_held_request_does_not_stop_the_loop...` |
| the idle sweep closes held connections | `a_held_request_is_not_the_idle_timeouts_to_close` |
| foreign events are not counted | five tests |
| the ready queue is not compacted when full | `answering_more_than_the_table_has_rooms...` |
| no requeue of requests buffered behind an answered one | two tests |

The first attempt left three of the nine standing (the held-twice one, the full-buffer one and the
idle one); the last three tests, and the "nothing was held twice" check in the first, were written
for them. The queue-compaction mutation survived a further round until a test ran the server with
buffers of 64 MiB (room for four connections, so a ready queue of four).

One flake was found and fixed on the way: a release byte written before the server had registered
the request it was meant for was read and lost, so the request was never released. The test
application now keeps a byte as a credit until there is a held request to spend it on.


## 11. Serving it over TLS: the byte-fed mode

> **Status: built** (this section was a proposal when `examples/tls_echo` merged, PR #346; building it corrected the proposal in the
> places marked *Corrected*). `packages/tls` has a server (`tls-server.md`), and `examples/https_hello` puts it, and this package, in
> one program: HTTP/1.1 over TLS 1.3 with keep-alive, pipelining, a 64 MiB streamed body and a client that stops reading, checked
> against curl, `openssl s_client` and Python's `ssl` (§11.6), at a measured cost (§11.7).

### 11.1 What stopped it

A TLS terminator's plaintext does not come from a socket: it comes out of `tls.recv`, and the answer goes into `tls.send`. This package
could not be the plaintext side, for a precise reason: it never saw a connection's bytes except through a socket it held itself.

* `wait` owned the `Listener`'s readiness and `accept_all` called `tcp_accept` and `conns.put`: every connection the server knew was a
  `Conn` in its own `conns.Table`.
* Input reached a connection's buffer in one place, `step`, by `conns.read`. Output left in three: `emit` (the fast path), `step`'s write
  of what was queued, and the refusals `produce` sends through `emit`. `shut` closed the `Conn`; `settle` re-registered it with the poller.

The one way that works with no change is a loopback relay (the TLS side connecting to this server's own listener on 127.0.0.1 for each
client): it doubles the sockets, copies every byte once more each way, gives the HTTP side 127.0.0.1 for every client and adds
`net_out` to the program's row. It was not built, and it is not what the broker or the gateway should copy.

### 11.2 The API as built

A server opened with `open_bytes` has no listener and no sockets: the application owns them and moves the bytes. Everything that is not
about where bytes come from is the socket server's code, unchanged (`next`, `head`, `parsed`, `body`, `respond`, `hold`, `answer`,
`close`, `connections`, the `reply*`/`failure*` helpers).

```
server.open_bytes(heap, size, out, idle, max) -> Opening      // Opening::Ok(Server) | Opening::Failed(errno)
k = server.attach(srv, now_ms)                                  // a connection: its slot, -1 when full
n = server.room(srv, k)                                         // how many bytes input takes now; 0 is back-pressure
n = server.input(srv, k, bytes, now_ms)                         // bytes from the peer; n taken (n <= room), -1 for a dead slot
    server.end_input(srv, k)                                    // the peer will send no more (half-close, close_notify)
    server.ready(srv, now_ms)                                   // `wait` without the poller: requeue, sweep idle; how many are queued
k = server.next(heap, srv) ...                                  // unchanged, with head / parsed / body / respond / hold / answer
n = server.output(srv, k, out, now_ms)                          // what the server would have written: up to len(out) bytes
n = server.pending(srv, k); n = server.space(srv, k)            // bytes of answer waiting; bytes of answer room left
b = server.closing(srv, k); server.detach(srv, k)               // the server is done with k; free the slot
n = server.stream(srv, ticket, bytes)                           // part of a held request's answer; n taken (n <= space)
```

| | |
|---|---|
| `open_bytes` | `size` is each connection's input buffer (a request, head and body, must fit in it); `out` the answer each may have waiting (at least 512); `idle` seconds without progress before it ends; `max` connections at most (at least 1). Memory is `max * (size + out)` and 16 words of state a connection, all allocated here; nothing grows. `Failed(22)` for a `size`, `out` or `max` out of range, else the errno of the one kernel object it makes (below) |
| `attach` | takes a free slot, or answers -1. `now_ms` starts its idle clock |
| `input` | copies at most `room` bytes into the connection's buffer and queues it for `next`. It never takes more than `room`, so the application keeps the rest and asks again: *this is the back-pressure*. Dead or ended slots answer -1 |
| `ready` | what `wait` does either side of its `poller_wait`: connections `next` did not reach, and the one it was part-way through, are queued again; every second, connections with no progress for more than `idle` seconds end |
| `output` | moves up to `len(out)` bytes of the waiting answer to the caller (the bytes for `tls.send`) and counts it as progress. When the last byte of an answer that ends the connection goes, the connection ends |
| `closing` | the connection ended: `Connection: close` was answered, the peer ended and everything is answered, it went idle, an answer did not fit, or what it sent was refused (400, 413, 431). What is left is the application's: close_notify, close the socket, `detach` |
| `detach` | frees the slot, ended or not (the socket failed, the application gave up). A request held for it is forgotten; its ticket answers -1 for ever, even when the slot is taken again (the generation of §10) |
| `stream` | with `hold`: the request stays held, nothing behind it is given out, and the idle timeout spares it *while nothing is waiting to be sent*. `answer(ticket, "")` ends it. This is how an answer larger than `out` is sent |

*Corrected, against the proposal:*

* **`open_bytes(heap, size, chunk, idle)` is `open_bytes(heap, size, out, idle, max)`.** `chunk` limits a socket write and means nothing
  without one. `out`, the bound on buffered output, was the constant 64 KiB; a gateway wants it, and the number of connections, as
  parameters (the TLS engine holds about 40 KiB a connection beside them). The socket server's `open` is unchanged and uses the same
  fields (`osize` is 65,536 for it).
* **It returns `Opening`, and it is not "no Poller".** `Core` has a `Poller` field the socket path registers handles in, and a `res`
  field cannot be absent without making every use of it a `match`. A byte-fed server therefore holds one kernel poller object (one file
  descriptor on Linux) that it never registers anything in; `close` closes it. That object is the only thing that can fail, hence the
  enum. `wait` on a byte-fed server returns it unchanged without blocking; `poller`, `first_token` and `foreign` are meaningless on one.
* **The clock is an argument, not a `Clock`.** `ready(heap, srv, clock)` would have had the package read the clock, and `attach` and
  `input` (which stamp progress) would have had to read it too. The caller reads its clock once a turn and gives the milliseconds to
  every call that moves a byte or looks at time (§11.4).
* **`room`, `end_input`, `pending`, `space` and `stream` are additions.** `input` "how many fit; 0 is back-pressure" cannot be used
  without knowing before `tls.recv` how much to ask for (the plaintext is in the engine; it has no way to hand some back), so there is
  `room`. A half-close needs a call (§11.4). `space` and `pending` let a caller size an answer to what the connection can take. `stream`
  is the one real addition: the proposal had no way to send an answer bigger than the output room, and the example's `/big/<n>`, and the
  gateway's proxied bodies, are exactly that.
* **It is about 330 lines, not a hundred** (444 added, 75 removed, of which about 60 are code moved out of `accept_all`, `serve_events` and
  `wait` into functions both paths call).
* **The effect rows are the socket path's.** `next` is `[heap, conn_write, poll]` because the refusals it sends go through `emit`, and
  the language has no polymorphism over effects (`AGENTS.md` §8). A byte-fed program therefore reports `conn_write` and `poll` in
  `cancho authority` though no socket is written through them. It does not widen the report of a program that has sockets, which a
  terminator does; a program with none would want a second set of names, which nothing has asked for.
* **A server cannot be seen to be the wrong kind in the type.** `attach`, `ready`, `stream` and the rest answer -1 (or do nothing) on a
  socket server, and `wait` does nothing on a byte-fed one; neither can trap.

### 11.3 How the two paths share the code

There is one parser, one framer, one pipeline and one back-pressure rule. `produce` (parse, frame, refuse), `finish`, `advance`,
`deliver_to` / `respond`, `hold` / `answer` and the idle sweep are the functions the socket path always used, and a byte-fed server calls
the same ones. What differs is where bytes cross a boundary, and each of those places is one test of `core.fed`:

| | socket server | byte-fed server |
|---|---|---|
| bytes in | `step`'s `conns.read` into the connection's buffer | `input` copies into the same buffer |
| bytes out | `emit`: `conns.write` to the kernel if nothing is queued, the rest into `pends` | `emit` skips the write: it all goes into `pends`; `output` drains it |
| `shut` | closes the `Conn`, frees the slot | marks the slot ended (`closing`); `detach` frees it |
| `settle` | re-registers the `Conn` with the poller | nothing |
| accepting | `accept_all` | `attach` |
| time | `wait` reads the clock | the caller's `now_ms` |

Four pieces were taken out of the socket path's functions so that both call them (`init_slot` from `accept_all`, `enqueue` from
`serve_events`, `begin_round` from `wait`, `sweep_idle` from `serve_events`). The socket path's behaviour is the same, and the 21
`api` tests and the 9 `http_server` tests pass unchanged. Four things *did* change on it, and are listed because "unchanged" is a claim
the tests only partly carry:

1. The idle sweep spares a held connection only if nothing is waiting to be sent. A socket server never holds with output waiting
   (`produce` hands out nothing while output waits), so for it the condition is the old one.
2. A connection queued for `next` is queued through `enqueue`, which compacts a full queue first (`answer` already did; `serve_events`
   did not, and could in principle have written past the end).
3. Slot word 15, which the socket path did not use, is the peer's end (`end_input`); it starts at 0.
4. **The identity of every function that mentions `Server` changed** (`Core` has four more fields), so `examples/api/server.lock` and
   `tests/programs/server_hold.lock` were re-pinned and the store republished. No signature changed; a consumer pinned to the old store
   keeps working on it.

`scripts/publish_packages.py` now publishes and checks `packages/http-server` too (it did x509 and tls only; the store is flat, `packages/http-server/.cancho-vcs`,
which the locks pin). Republishing removes the old store's files and writes the new: a changed body is refused by an incremental publish, as §8 found.

### 11.4 What the semantics are

**Time.** The package never reads a clock. `attach`, `input`, `output` and `ready` are given `now_ms`, any monotonic milliseconds, and a connection's
last progress is the time of the last byte in or out, in whole seconds, as for a socket. A byte-fed connection that no one feeds and
no one drains is idle after `idle` seconds, which `ready` notices (at most once a second, from the `now_ms` it is given). The caller's TLS
handshake and close timeouts are its own; `https_hello`'s server idle is its own plus two seconds, so its sweep, which also drives the connection through close_notify,
acts first.

**Back-pressure** shows up in three numbers and one rule. `room(k)` is the bytes of input the server will take: 0 while an answer is
waiting (a client that does not take its answers is not read), and 0 once the connection is ending. `pending(k)` and `space(k)` are the answer waiting and the room for
more, and `pending + space = out`. The rule is the socket server's own: **nothing is handed out by `next` for a connection with output
waiting**, so with a client that takes nothing the server holds one answer, and the connection's input buffer (`size`), and no more.
An answer that does not fit the room is refused whole (nothing is half sent) and the connection ends, as a socket's does when the client
has not read a buffer's worth: `respond` answers -1 and `closing` is true. A caller that has more than `space` to say `hold`s the
request and `stream`s it.

**A half-close and an error.** The peer's end (a TCP FIN, or its close_notify) is `end_input(k)`: what it sent is still answered, in order,
and then the connection ends; a request it did not finish is dropped, as for a socket. A socket that fails, a TLS alert, a connection
the application gives up on: `detach(k)`. The application learns the server wants to end a connection from `closing(k)`, and always ends
it by `detach`. Both can happen with answers waiting: `end_input` waits for them to be taken; `detach` drops them.

**Pipelining** is the socket server's: requests in the buffer are served in order, one at a time, because a connection with an answer waiting is not given another; the next
`ready` or `output` that empties the answer queues it again. A request split across any number of inputs is answered once, when `next` says it is whole.

**The memory a connection can make the server hold** is `size + out` bytes and 16 words, fixed at `open_bytes`. Nothing is allocated per connection or per request
by the package; the application allocates what it builds an answer in. In `https_hello` a connection also has four 16 KiB buffers and the TLS engine's slot (§11.6).

**Limits that are refusals, with their tags** (the status, and `failure`'s message, which the client sees; none reaches the application): a request head that fills
`size` without ending, `431 request head too large`; a body that cannot fit, `413 request too large`; a malformed request or an
ambiguous length, `400` with `std.http`'s message. After any of them the connection ends once the answer is taken.

### 11.5 Tests and mutants

`tests/packages/http_server_bytes_test.cho` (19 `cancho test` tests, run on both backends by `conformance/http_server_bytes.rs`) is the byte-fed API without a
socket, which is the point of the mode: a length body, a chunked body, three pipelined requests with a fourth cut short, a request fed one byte at a time in
each framing (answered once, after its last byte and not before), every refusal and its status, half-closes (after a request, after nothing, in the middle of one, with an
answer waiting, with a request held), a client that takes nothing (twenty pipelined requests, one answer held, input refused, `pending` never past `out`), an answer over the room
(by 1 byte and by 100, and one that fits exactly), idle on the caller's clock (progress by input and by output, the boundary second), slots (full, freed, reused, a freed one
skipped over), a held request streamed in pieces and a ticket that never outlives its connection, a stalled stream ended by the idle timeout, the ready queue (its overflow, a slot
freed while queued, the connection in hand detached), answers of no bytes, `ready` giving back a visit's room, every bad slot and ticket, and 8 seeds of 3,000 random operations that
must not trap or break a bound (`pending <= out`, `room <= size`).

`scripts/http_server_bytes_mutants.py` applies 90 single-edit mutations to the new code and the shared code it reaches, one at a time, to `server.cho` and runs those
tests: **79 are killed and 11 survive, each argued equivalent in the script** (ten because the line they change is redundant with another or unobservable: `detach` and `init_slot` both clear a slot's buffered count, `shut` clears a flag that `detach` clears again; and one, `stream` refusing a socket server, because `cancho test` cannot open a socket server, which needs a `Listener`). The first run killed 74 of 91 and showed 11 survivors that were not equivalent: no test detached the connection `next` had in hand, none stopped at the second the idle limit is reached, none took output and then waited past the limit, none gave an answer one byte over the room, none ended a peer while its request was held, none answered with no bytes, and none looked at the room `ready` gives back; each has a test now (CI runs it).

### 11.6 `examples/https_hello`

`examples/https_hello/` is `tls_echo`'s program with `http.server` where the echo was: `hello.cho` (the options, `main`), `loop.cho` (the loop), `app.cho` (the routes). `tls_echo`'s
loop was split so that the two share it (below). Its authority report is `tls_echo`'s, label for label (`conformance/https_hello.rs` pins it: no foreign code, `bounded`).

```
socket -> tls.feed -> tls.recv -> rcv buffer -> server.input         requests
server.output -> app buffer -> tls.send -> tls.take -> socket        answers
```

Each stage reads only when the stage after it has emptied, so the same rule that bounds the server bounds the whole path: a client that stops reading its answers
stops `server.room`, which stops `tls.recv`, which stops `tls.feed`, which stops reading the socket. A connection whose request is held with a full input buffer is not even
watched. A turn is: wait; `pump` every connection the poller named; then rounds of {`ready`, answer up to 64 requests, give the streams what room they have, `pump` every
connection that moved}, at most 16 rounds before returning to the poller (with no wait if work remains).

The routes: `GET /` (hello), `GET /hello/<name>`, `POST /echo` (a JSON body is returned inside `{"method":..,"path":..,"bytes":..,"json":<body>}`; anything else is a 400),
`GET /big/<n>` (n bytes, `a` to `z` repeating, up to 1 GiB: the request is `hold`ed, the head `stream`ed, and the body given as the connection takes it), 404 and 405 with `Allow`.

**What moved to a shared file.** `examples/tls_echo/echo.cho` and `tls_echo.cho` held the options, the per-slot state, the log, the handshake bounds (`admit`), `accept_all`, writing
the engine's bytes, `end_slot`, the reload and the command line (about 450 lines). A second program needed all of it, and the duplication check refuses a copy, so it is
`examples/tls_echo/front.cho` (module `tls_front`) now: every function is byte for byte what it was, with `pub` added (and 4 more slots in `stride`).
`echo.cho` keeps `pump`, `close_with`, `sweep`, `stop_all` and `run`, which touch the plaintext. `main` and `serve` remain in each program (they name their loop; a third
program would move them: link-time selection of the module that holds `run` would do it).

`scripts/https_hello_test.py` (15 cases; CI's `tls-assurance` job runs them):

| case | what it checks |
|---|---|
| `curl` | curl (OpenSSL build), HTTP/1.1 over TLS 1.3: one request; 100 requests on **one** connection (`%{num_connects}` adds up to 1); a JSON `POST` echoed; a 404 and a 405; a 100,000-byte streamed body byte for byte. Skipped, saying so, where curl is a SecureTransport build and cannot be given the test CA |
| `openssl` | `openssl s_client`: a request, a keep-alive one after it on the same connection, three pipelined in one write (answers in order), then `Connection: close`, answered and ended with close_notify |
| `http` | Python `http.client`: 2,000 requests on one connection, each answer checked; the JSON echo with every escape, non-ASCII and nesting; a body that is not JSON (400); a chunked request body; HTTP/1.0 answered and closed |
| `pipelined` | 300 requests in one write, then 300 with length and chunked bodies and a 404 every third, all in one write: 600 answers, in order, and the connection serves after |
| `many` | 200 connections at once (Python `ssl`, one thread each), 10 requests each |
| `big` | `/big/<n>` for 16 sizes from 0 to 64 MiB (the boundaries of the 16 KiB record and the 64 KiB output room among them): every byte checked, the connection serves after each; a request pipelined behind a big one is answered in order; a length past 1 GiB, not a number, negative or empty is refused and the connection goes on |
| `stalled` | a client asks for 1 GiB and reads 100 bytes: the server's resident memory does not grow past 4 MB beside it, another client is served throughout (the slowest of its requests is reported), and the stalled one is ended after `--idle`; the server serves after |
| `slow` | half a request and silence: ended after `--idle`, and the clients beside it are served |
| `ended` | a connection ended by `Connection: close` and then written to every 50 ms (each write refused by the closed socket) beside a keep-alive client making 40 requests: every one is answered and no other connection is closed. The writes come from a thread of their own (below) |
| `reload`, `bound`, `full`, `idle`, `shutdown` | `tls_echo`'s cases, over HTTP: a connection open before `SIGHUP` keeps its certificate and keeps being served, a later one gets the renewed one, a refused reload leaves it; `--handshakes 2` held by two silent peers delays an honest one rather than refusing it; a fifth connection over `--connections 4` is closed at once; an idle keep-alive connection is sent close_notify; `SIGTERM` with 10 open keep-alive connections sends close_notify to each and exits 0 |
| `hostile` | 1,500 connections each sending a mangled request (flipped bytes, cuts, bare LFs, a 100 KB target, a thousand headers, JSON 5,000 deep, lengths past 2^64 or negative, chunk sizes past the end, 1,000 NULs): the server is alive after every one and serves after the last |

**Results.** 14 of 14 on linux-aarch64 (Docker, with curl 8.5 built on OpenSSL 3.0.13), 13 of 14 on macOS (its curl is SecureTransport and the case says so); `tls_echo_test.py` still passes 8 of 8 on both after `front.cho`. 
`conformance/https_hello.rs` (64 connections through `tls_many`, a reload taken and one refused, a stop, the authority pin) and `conformance/http_server_bytes.rs` run in `cargo test` on both CI targets; `cargo test --workspace` on linux-aarch64 in Docker passes everything but the two tests that need a git checkout, as before this change.

**A false report, and why `ended` writes from its own thread.** It was reported that bytes arriving on an ended connection made the server close a different one: a Python
client with D and H in one thread, D ended by `Connection: close` and then written to every 50 ms, saw H's next request fail with `BrokenPipeError` within 0.1 s, the server alive
and logging no close of H (it logged `conn 2 closed tls-peer-closed` only afterwards, when `http.client` closed H on the error). Measured on macOS, Python 3.14.5 with OpenSSL 3.6.4,
the server is not involved:

- when D's `conn 1 closed` is logged its descriptor is gone (`lsof`), so D's later bytes reach only the kernel, which answers them with a reset to D. The server's descriptor
  for H stays `ESTABLISHED`, nothing arrives on H's socket, and a turn of the loop never sees them;
- H in a second process, beside the same D written to the same way: 10 of 10 requests answered;
- the same two clients against a plain-Python TLS 1.3 server (`ssl`, one thread per connection, D closed after one read, H echoed): H's write right after D's refused one
  raises `BrokenPipeError`, the one after that succeeds;
- calling OpenSSL's `ERR_clear_error` (through `ctypes`, on the `libcrypto` that `_ssl` links) between D's refused write and H's makes every write on H succeed.

The cause is the client's: OpenSSL keeps its error queue per thread, D's refused `SSL_write` leaves an `EPIPE` on it, and CPython's `_ssl` reports that entry as the
failure of the next write on another socket in the same thread. There was nothing to correct in `loop.cho` or `front.cho`. The `ended` case keeps the report's sequence, with D's
writes on their own thread: the unchanged server passes it 5 of 5. To check that the case can fail, `end_slot` was made to close the next slot too (a wrong-slot close,
the bug the report suspected): the case failed (`ConnectionResetError` on H's next request). That change was not kept.

### 11.7 Cost

`python3 scripts/https_hello_test.py <https_hello> --cost <seconds> --kload kload --tload tload --plain api` (CI's `tls-assurance` job runs it): the closed-loop keep-alive load of `docs/server.md` §5 (`benches/server/kload.c`:
2 threads of 16 connections, one request outstanding on each, three rounds), plain against `examples/api` (the socket path of this same package, `GET /users/42`, a short JSON answer) and,
through `benches/server/tload.c`, the same loop over TLS 1.3 against `https_hello` (`GET /hello/42`, a 99-byte answer with its head: a different route, so the application's own work is not the same, and small beside what TLS adds).
The server is pinned to one core and the load to two others. The script also reads the server's CPU time from `/proc` and divides it by the requests.

**Measured on** two machines, both with the LLVM backend and OpenSSL 3.0.13 as the client: the x86-64 CI runner (a shared virtual machine, as `tls-server.md` §11.4's 5.5 ms a handshake was), whose three rounds agree to 0.5%; and Docker Desktop's Linux VM on an Apple M4 Max (aarch64 Linux 6.8, 6 vCPUs, other containers running), which is not a quiet machine: its rates moved by up to 1.5 times between two runs of the same command, so there the CPU a request is the figure to read:

| | requests a second, three rounds | the server's CPU a request |
|---|---|---|
| **x86-64, CI's `ubuntu-latest` runner** (the PR's first green run), 5 s rounds | | |
| plain, `examples/api` | 178,451 179,056 178,473 | 5.59 microseconds |
| TLS 1.3, `https_hello` | 42,614 42,313 42,086 | 23.40 microseconds |
| **aarch64, Docker Desktop's Linux VM on an Apple M4 Max** | | |
| plain, `examples/api` | 345,024 317,966 253,457 (10 s rounds); 406,611 260,736 262,227 (5 s rounds, an earlier run) | 2.68 microseconds |
| TLS 1.3, `https_hello` | 97,548 74,635 84,708 (10 s rounds); 84,732 133,024 133,420 (5 s rounds, an earlier run) | 9.77 microseconds |

So over TLS this server costs **3.6 times (aarch64) to 4.2 times (x86-64) the CPU a request** of the plain one, and serves as many times fewer requests a second from one core: **about 42,000 a second of 99-byte
answers on the CI's x86-64 core, against 178,000 plain**, and 75,000 to 130,000 against 250,000 to 400,000 on the aarch64 VM. The 7 microseconds (aarch64) or 18 (x86-64) a request that TLS adds is a record decrypted, a record encrypted, the engine's and the loop's copies, and the loop pumping a connection twice a request (§11.6).
**It was not profiled** and that is the next place to look if the figure matters: the stages copy a request's bytes several times each way (the socket into `inq`, `tls.recv` into `rcv`, `input` into the server's buffer; an answer into `pends`, `output` into `app`, `tls.take` into `pend`) and `server.output`
moves what is left of an answer to the front of its buffer on every call. A body streamed with `/big/<n>` ran at 83 MB a second on CI's x86-64, 104 MB a second on the aarch64 VM and 245 MB a second on the Mac itself, outside Docker, each checked byte for byte.
Not measured: more than 32 connections, handshakes (`tls-server.md` §11.4's 3.0 ms on the M4 and 5.5 ms on CI's x86-64 are unchanged).

### 11.8 What `cancho-gateway` needs next

What the gateway (an HTTP/1.1 reverse proxy that terminates HTTPS) can build on today, and what it still lacks. In the order it will hit them:

1. **Streaming a request body.** A request is handed over whole, so one larger than `size` is refused with a 413, and a proxy forwarding an upload would have to
   buffer it. The gateway needs the body in pieces (a `body_part` view and a way to say "more is coming", with the same bound per connection) and `Expect: 100-continue`.
   This is `server.md` §6's first open item and the largest piece of work left; `stream` only did the response direction.
2. **Chunked responses.** `stream` sends bytes; a proxied answer of unknown length needs `Transfer-Encoding: chunked` framing on the way out (and de-chunking on the way in from the upstream). `std.http`
   has `dechunk` and no chunk writer.
3. **The peer's address.** ~~`std.conns` does not give it~~ **Corrected ([`conn-peer.md`](conn-peer.md)):** `conns.peer(table, slot)` answers it (`std.addr.Peer`, with `text` for a log line and
   an `X-Forwarded-For` entry and `key` for a per-address bound), and `examples/tls_echo` logs it and bounds by it; `examples/https_hello` logs it (the lines are `front.cho`'s). What is not done is
   `packages/http-server`'s use of it: the `X-Forwarded-For` header, and the access log, are the gateway's change after this one.
4. **Upstream connections in the same loop.** The gateway's own sockets to its upstreams go in the poller it owns (a byte-fed server has none of its own; `https_hello`'s `loop.cho` is the place),
   with `hold`/`answer` for the request waiting on the upstream (§10) and `stream` for the answer's body. `packages/http-request` is blocking; a non-blocking client is the missing piece, as it was for `cancho-pg`. *(Built: [`http-client.md`](http-client.md), `packages/http-client` and `examples/http_fetch_nb`; what the gateway still needs of it is §10.7 there.)*
5. **Upgrade.** `Upgrade: websocket` and `CONNECT` need a connection to stop being parsed once the 101 is sent and become a byte tunnel: `detach` frees the HTTP side's slot, but the bytes already buffered behind
   the request are the gateway's to recover (`head`/`body` views end with the request).
6. **Two timeouts.** `idle` is one number for a keep-alive connection waiting for its next request and a client taking a minute to send a head. A proxy wants a header timeout of its own.
7. **ALPN.** The engine selects a protocol (`--alpn`); the loop could offer `http/1.1` and refuse `h2`, which it does not speak. Nothing is done with the negotiated value yet.
8. **More than 1,024 connections** (`max_connections`) and more than one core (a second process on `SO_REUSEPORT`, not tried).
9. **A byte-fed program's authority report lists `conn_write` and `poll`** (§11.2). If the gateway's reviewers want it without them, the package needs a second set of names whose rows have no socket in them; nothing has asked yet.

## 12. Streaming a request body, and `Expect: 100-continue`

> **Status: design, written before the code** (`CONTRIBUTING.md`: design first, claims measured, a claim that turns out false is corrected in place in this section). The two numbers
> below that are measured were measured before any code was written; everything else is a decision, and §12.12 is where building it is to say what turned out different.
> It is the first item of §11.8 and `server.md` §6's: a proxy forwarding an upload cannot buffer it.

### 12.1 What stopped it, measured

A request is handed to the application whole or not at all. `produce` waits until `head + body` is in the connection's `size`-byte input buffer, and refuses what can never fit:
`tests/packages/http_server_bytes_test.cho` pins it (`POST` with `Content-Length: 1000` into `size` 128 is `413 request too large`; a chunked body that outgrows the buffer is the same
`413`), and on the socket path the same code runs. Three consequences for a reverse proxy, each from that one rule:

* an upload larger than `size` cannot be proxied at all (and `size` is memory times connections: `input_budget` caps the sum at 256 MiB);
* a client that sends `Expect: 100-continue` (curl does for any body over 1 MiB; it waits one second, then sends the body anyway) cannot be told to go ahead or refused
  before it sends the body, so a refusal costs the whole upload and the proxy cannot say "no" to a 1 GiB `PUT` cheaply;
* the only timeout is `idle`, one number for a keep-alive connection waiting for its next request and a client taking a minute to send a head or a body
  (`cancho-gateway` `docs/design.md` §5 wants a header timeout of its own, a `408`; §11.8 item 6).

**The per-byte cost of the machine this design needs.** The chunked decoder below is a state machine run once over each byte, in place. Measured on this machine (Apple M4 Max, LLVM backend,
`cancho build --std`; a 64,000-byte buffer, 4,096 passes = 262 MB, a two-state machine that drops every CR and copies the rest, byte by byte): **134 ms, 1.96 GB/s, 0.51 ns a byte**; the same
loop as a plain forward copy of the buffer within itself is turned into a `memmove` by LLVM (2 ms for the 262 MB), so a *content-length* body costs no copy at all and a chunked one costs about
half a nanosecond a byte. The TLS path measured 83 MB/s (§11.7), so the framing is not where an upload's time will go, and this design does not trade clarity for speed in it.

### 12.2 The shape: a pull, and it is the request's ticket that pulls

Three shapes were considered.

1. **A callback per piece** (`serve(.., on_head, on_body_part, on_end)`). Refused for the reason of §2, unchanged: a piece is a view of a buffer the loop borrowed itself, a function value's type
   cannot name that region, and the application's own state would have to be reachable from a closure the language does not have.
2. **`next` answering the same request again for every piece** (the request in hand with a `body` that grows). The request in hand is *one* request per `next`, and a
   proxy has thousands of uploads in flight, each waiting on its own upstream socket; "in hand" has no place to remember which. It would also give `body` two meanings.
3. **The held request's ticket (§10) pulls its own body.** Chosen. `hold` already is "this request goes on while I do other things, name it with a ticket, and the server will not give out anything
   behind it". An upload is exactly that: the application takes the head from `next`, decides (refuses it, or `hold`s it), and then, whenever it can use bytes (its upstream socket is writable) it asks
   the ticket for the next piece. The views are borrows of the `Server`, as every view is.

So the API is five new calls on a ticket and one on the request in hand, and the existing calls do not change:

```
server.limits(srv, max_body, piece, head_ms, body_ms) -> int  // opt in: 0, or -1 for a bound under 1; before this, nothing below happens and §12.4's first row holds
server.streaming(srv) -> bool                                  // the request in hand has a body that is still arriving (or does not fit)
t = server.hold(srv)                                           // unchanged. For a streaming request it also lets go of the head: copy what you need first
server.proceed(srv, t) -> int                                  // "I will read the body": answers 100 Continue if the client asked for it and it was not sent. 1 sent, 0 nothing to send, -1 the request is over
p = server.body_part(srv, t) -> &[byte]                        // the next piece: at most `piece` bytes that have arrived and not been taken; empty when none now
n = server.body_take(srv, t, n) -> int                         // the application has used n bytes of the piece it saw: n, or -1 (n past the piece, or the request is over)
s = server.body_state(srv, t) -> int                           // 0 more is coming, 1 all of it has arrived (bytes may still wait in the buffer), -1 the request is over: stop
n = server.body_total(srv, t) -> int                           // bytes of the body received so far, decoded
```

*The end of a body* is `body_state == 1` with `body_part` empty: no separate `body_end` call, because "all arrived" and "all taken" are two facts and a proxy that is slow to take needs both
(it has seen the end of the stream and still owes the upstream a piece). `body_state == -1` is the one answer for every way a request ends under the application's feet: the server refused it
(framing, size, a timeout), the connection went (a reset, the peer's end in the middle of the body, `detach`), or the ticket was answered. The application finds out by asking, in the same loop that gives
streamed *answers* what room they have (`feed_streams` in `examples/https_hello/app.cho`), because there is no callback to be told by; §12.9 says what the server has done by then.

**Why opt-in (`limits`).** A server that never calls it is, in every byte it reads and writes, the server of §11: the `413`, the ignored `Expect`, `idle` as the only timer. The `api` and `http_server`
tests and every consumer pinned to the store stay as they are. A server that calls it has told the loop what its bounds are, and the loop can then do what a bound makes possible
(hand over a head early, refuse by `Expect`, time a head). The four numbers are bounds, so they are parameters and not defaults the package chose: `max_body` the most body bytes a request may have
(the `Content-Length`, or the decoded total), `piece` the most `body_part` shows at once (clipped to `size`), `head_ms` and `body_ms` the two timers of §12.8.

### 12.3 When `next` hands over a head early

With `limits`, `produce` parses the head as before and then looks at the body:

| the request | what `next` does |
|---|---|
| no body (`Content-Length` 0 or absent, not chunked) | as ever |
| `Content-Length` above `max_body` | refuses `413` (`body.too-large`) *now*: before any `100`, before the client sends a byte of it |
| `Content-Length` and the whole body is in the buffer, and head + body fit `size` | as ever: `head`, `parsed`, `body`, `respond`. `streaming` is false |
| `Content-Length` and any of it has not arrived (or head + body do not fit `size`) | hands over the head at once; `streaming` is true |
| chunked, and the framing in the buffer reaches the final CRLF (§12.4) | as ever: the body is decoded in place, `body` is the whole decoded body, `streaming` is false |
| chunked, and it does not | hands over the head at once; `streaming` is true |
| an `Expect` other than `100-continue` (on any request) | refuses `417` (`expect.unsupported`) |

**What a request that fits does, exactly.** Without `limits`: everything it did. With `limits`: a request whose head *and entire body* are in the buffer when its head is parsed is
handed over whole, with the same `head`, `parsed`, `body` and `respond` and the same bytes in the answer; what is different is a request that fits `size` and whose body arrives in a later
read, which was held back until whole and is now handed over as soon as its head is complete. That is deliberate and is the only change: it is what lets the application see `Expect` before the
client has sent anything, and an application that opted in already handles `streaming`. A fed-at-once request and the same bytes fed one at a time therefore reach the application in different forms; both
are tested to produce the same body bytes and the same answer.

An application that wants no streaming for small bodies reads the body from `body_part` anyway: **`body_part`, `body_take` and `body_state` work for a request that `streaming` called false too,
once it is held** (a whole body is "all arrived" at once), so one loop over pieces serves both forms. (A chunked body held whole is still in the buffer where it was decoded; the legacy chunk scratch
`decoded` is no longer used by a server with `limits`: the decoding is in place, §12.4.)

### 12.4 Framing

**`Content-Length`.** The head's length, parsed and refused by `std.http` as today (400 for a malformed, repeated-and-different or with-`Transfer-Encoding` length). The server counts: `remaining = length - bytes already in the
buffer`. **Input is never taken past the end of the body** (`room` is `min(free space, remaining)`, and the socket path reads only that many), so what follows a body on the wire is never mixed
into it, and a request pipelined behind a streamed body is simply the next bytes the caller offers once `remaining` is 0.

**`Transfer-Encoding: chunked`.** `std.http.parse` accepts exactly `chunked` and refuses every other coding and a `Content-Length` beside it, as today. The decoder is a byte-at-a-time state machine
whose state lives in the connection's words, so a body fed one byte at a time is decoded once, not again from the start at every byte as `std.http.dechunk` does. It decodes **in place**: decoded bytes are written
over the framing that carried them (the write position never passes the read position), so the buffer holds `[decoded, not yet taken][nothing else]` while a body is arriving, and a chunk of any size needs no
buffer of its own. States: size (1 to 8 hex digits, leading zeros count), extension, the LF of a size line, data, the CR and LF after data, then after the zero chunk a trailer section and the final CRLF.

* **Chunk extensions are accepted and ignored** (RFC 9112 §7.1.1: a recipient MUST ignore ones it does not recognise), within bounds: at most 256 bytes after the `;`, each a tab or a visible ASCII or
  high byte, never a control character or a bare CR or LF (`body.chunk-extension`). `std.http.dechunk` refuses them; the gateway's `framing.chunk-extension` does too. This server does not, because
  the framing is its own and an extension that cannot be told apart from data is not a smuggling vector when it is parsed by the same machine that finds the end of the chunk; an application that wants them refused
  has the bound `max_ext` at 0 (§12.12 says if that was built).
* **Trailers are accepted, validated and discarded.** After the zero chunk: zero or more `name: value` lines (name of token characters, a value of tabs and visible bytes, CRLF), then a blank line.
  At most 4,096 bytes of them (`body.trailer-too-large`, `431`); a malformed line is `body.trailer`. The application never sees them: a proxy re-frames the body it forwards, so a trailer is dropped as
  RFC 9110 §6.5.2 allows. `body_state` is 1 only after the blank line.
* **Bounds.** `max_body` on the decoded total, checked when a chunk's size line is read (`size + total > max_body` refuses at once, before the chunk's bytes), so a hostile
  `fffffff0` is refused without waiting for it. Framing overhead per chunk is bounded by the 8 digits, 256 extension bytes and four CRLF bytes, each chunk carrying at least one body byte: at most about 270 wire bytes for
  each body byte, and time-bounded by `body_ms` like any other byte.

**Every refusal**, with its rule tag and status. The tag is sent in an `X-Rule` header beside `failure`'s `{"error": ...}` (a server without `limits` sends what it sent before, with no `X-Rule`). All close the connection after the answer.

| rule | status | when |
|---|---|---|
| `body.too-large` | 413 | `Content-Length` over `max_body`; decoded total over `max_body` |
| `body.chunk-size` | 400 | a size line with no hex digit first, or more than 8 digits |
| `body.chunk-framing` | 400 | anything but CRLF where CRLF belongs: a bare LF, a size line with something after its digits, data not followed by CRLF |
| `body.chunk-extension` | 400 | an extension over 256 bytes, or with a control character |
| `body.trailer` | 400 | a trailer line that is not `token: value` CRLF, or a final line that is not CRLF |
| `body.trailer-too-large` | 431 | more than 4,096 bytes of trailers |
| `timeout.head` | 408 | §12.8 |
| `timeout.body` | 408 | §12.8 |
| `expect.unsupported` | 417 | an `Expect` header with any member other than `100-continue` |
| `head.too-large`, `head.malformed` | 431, 400 | what §11.4 already refuses (a head that fills `size`; `std.http`'s refusals): tagged only when `limits` is on |
| (no answer) `body.truncated` | | the peer ends or the connection resets mid-body: nothing to answer to; the request is over (`-1`) |

A refusal found while reading (framing, size) is *recorded* by `input`/`step` and answered by the next `next`: `input` has no heap to build an answer in and its effect row stays `[]` (a widening would break
every caller). The request is over from the instant it is recorded (`body_state` is -1, `room` is 0), the answer is queued by the `next` that follows, and the connection ends once it is sent.

### 12.5 `Expect: 100-continue`

* **Only HTTP/1.1, only for a request with a body that is streaming.** On a 1.0 request, on a request without a body, and on a request whose body had already arrived whole, it is ignored (RFC 9110 §10.1.1: the client has
  sent it, or has no body to wait to send).
* **The server never sends `100 Continue` by itself.** The application sends it by `proceed(t)`, after it has decided it wants the body: after its route and its limits, after its upstream accepted the connection,
  after a quota check. A gateway that refuses by route or size answers the head with a final status and sends no `100`, and the client does not send the body.
* `proceed` writes `HTTP/1.1 100 Continue\r\n\r\n` into the connection's output (it fits: `out` is at least 512 and nothing of this request's has been queued), once. It answers 0 afterwards, if
  the request had no `Expect`, and if the application has already started its answer (`stream` was called): a `100` after the first byte of a final response is an error on the wire.
* **`417`** for an `Expect` header any of whose values is not `100-continue` (case-insensitively; `100-continue, x` too), by `next`, before the application sees the request.
* A client that sends the body without waiting (curl waits one second, others do not wait) simply has its body buffered up to `size`, taken by `body_part` as usual, and is never sent a `100`.

### 12.6 Back-pressure in both directions

**In.** The rule of §11.4, unchanged and shared: a byte is read only when there is room for it. `room(k)` is the free space in the connection's buffer, for a streaming body bounded by what the body still owes (`Content-Length`) — and
0 while an answer is waiting to go out, while the connection is ending, and while a refusal is recorded. A piece the application has not taken therefore stays in the buffer, the buffer fills, `room` is 0, the
caller stops calling `tls.recv` (or the socket path stops being watched), and the client's TCP window closes. **A proxy passes its upstream's pressure to the client by simply not calling `body_take`.**
The most a streaming connection holds is the same `size + out` of §11.4 whatever `max_body` is; a 1 GiB upload through a 16 KiB buffer is 16 KiB of the server's memory, which the live test measures.

**Out.** Unchanged too, and it applies while a body is still arriving: an answer waiting to be sent stops `room`, so a client that sends a body and does not read the response it is already getting (an
early refusal, `100 Continue`, a streamed echo) stops being read, and a stalled sender and a stalled reader are both timed (§12.8). `body_take` re-arms the socket server's read interest (`settle`) when it makes room;
a byte-fed caller finds the room by asking `room`.

### 12.7 Pipelining after a streamed body

A request behind a streamed body is held back for two reasons that are already the rule: nothing is handed out for a connection while a request is held (§10), and `room` never takes bytes past the end of a
`Content-Length` body. For a chunked body, the machine stops at the final CRLF and the bytes after it are the next request's, moved down to follow the body in the buffer. When the streamed request is answered, whatever of its body
the application did not take is dropped (the body has fully arrived, or the connection is closing, §12.9) and the buffered next request is given out by the next `next`, without waiting for new input.

### 12.8 The timeouts

Both are in milliseconds, on the caller's clock (the one given to `attach`, `input`, `output`, `ready`; `wait` reads its own). Both answer `408` (`timeout.head`, `timeout.body`) with the connection closing after,
unless the application has already begun its answer, in which case the connection is closed with nothing more sent. They are *in addition to* `idle`, which is unchanged: a keep-alive connection with no request in progress ends by it.

* **`head_ms`: a deadline for the head, not an idle limit.** It starts when `next` first looks at a request whose head is incomplete, and ends when the head is complete; a client that sends a byte every
  `head_ms - 1` milliseconds does not extend it (that is the slow-loris shape: many connections, each held open by a trickle). It does not run while the connection has an answer waiting (the server is not reading then).
* **`body_ms`: the longest the server waits for the client to send more of a body.** It restarts when a body byte arrives and when the application takes a piece after the buffer was full, and it does not run while
  the buffer is full and the application has not taken what is in it: waiting for the *application* is the application's to time (a gateway has its upstream timeouts), not the client's fault. A client that sends one byte
  every `body_ms - 1` milliseconds is not stopped by this alone; each such byte is progress by definition. The bound on that case is the application's total deadline for the request, which `hold` always allowed (§10: "an application
  that wants a deadline answers with 504"), and `max_body` bounds the bytes.
* The scan for expired timers runs at most every 50 ms, over the connection table. `ready`/`wait` do it; expiry only *records* the refusal (as in §12.4); `next` sends it.

### 12.9 Answering before the body has been read

The application may `respond`/`answer` a streaming request at any time: a refusal to the head, an `413` after counting bytes, an upstream that answered early. What the server does with the connection afterwards:

* **The body had all arrived** (`body_state` 1): the untaken bytes are dropped and the connection carries on, keep-alive as the request asked.
* **The body had not started** (an `Expect` request, no `100` sent, no body byte received or taken): the client was never told to send it, so the connection carries on keep-alive. If the client sends
  the body anyway its bytes are the next "request" and are refused as one (`400`), which closes; no worse than any client that pipelines garbage.
* **Otherwise the connection closes once the answer has gone** (as if it had said `Connection: close`, whatever the application's head said, because the server cannot rewrite an answer it was handed). The
  alternative, reading and discarding the rest, is a decision about someone else's bandwidth that the application can take by taking the pieces and dropping them before it answers; the server does not
  drain by itself because a 1 GiB `PUT` refused at 100 MB would otherwise cost the server 900 MB of reading to keep one connection. The client may see a reset before it has read the answer if it is still
  sending when the connection closes: that is a property of closing a TCP connection with unread data, and the lingering close that avoids it belongs to the program that owns the socket.
* While the answer is streamed (`hold` + `stream`) and the body is still coming, both flow (§12.6, "Out"); if the body then fails, the answer cannot be replaced by a refusal, so the connection is closed.

A refusal the server makes mid-body (§12.4, §12.8) while the application holds the request: the request is over (`body_state` -1, `answer`/`stream` -1), the refusal is queued unless the application had started
its answer, and the connection ends. The application's ticket is not reusable (§10's generation).

### 12.10 The memory bound per connection

`size` bytes of input buffer, `out` bytes of answer, and 32 words of state (`stride` goes from 16 to 32): `max * (size + out)` and 256 bytes a connection, fixed at `open`/`open_bytes`, as §11.4. A streaming body adds nothing: it *is*
the input buffer. The `decoded` scratch of the legacy chunk path is not used by a server with `limits` and is allocated as before (a socket server or one without `limits` still needs it).

### 12.11 One implementation for both paths

The socket path and the byte-fed path differ in where bytes cross a boundary (§11.3) and that is all they will differ in:

| | |
|---|---|
| classify a head, `Expect`, refuse | `produce`, one function |
| count a body, run the chunk machine | `ingest`, called by `step` (socket) and `input` (fed) after they have put bytes in the buffer; one function |
| how much may be read | `readable`, called by `step`, `settle` and `room` |
| the timers | `sweep_timers`, called by `sweep_idle`'s two callers (`wait`'s `serve_events`, `ready`) |
| `hold`, `answer`, `deliver_to`, end of request, `body_*`, `proceed` | one function each, on the connection's state |

`step` (socket) reads at most `readable` bytes and calls `ingest` on what it got; `input` (fed) takes at most `readable` and calls `ingest`. The duplication test would refuse a copy of either.

### 12.12 Left open, to be filled in as built

(Deviations from this design, corrections, and what building found go here, each marked *Corrected*. The tests, mutants and measurements go in §12.13 once they exist.)
