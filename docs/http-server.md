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
3. **The peer's address.** `std.conns` does not give it, so there is no `X-Forwarded-For`, no per-address bound on handshakes or connections (`tls-server.md` §11.5), and no access log with
   an address. A `Conn` accessor in `std.conns` is the change.
4. **Upstream connections in the same loop.** The gateway's own sockets to its upstreams go in the poller it owns (a byte-fed server has none of its own; `https_hello`'s `loop.cho` is the place),
   with `hold`/`answer` for the request waiting on the upstream (§10) and `stream` for the answer's body. `packages/http-request` is blocking; a non-blocking client is the missing piece, as it was for `cancho-pg`.
5. **Upgrade.** `Upgrade: websocket` and `CONNECT` need a connection to stop being parsed once the 101 is sent and become a byte tunnel: `detach` frees the HTTP side's slot, but the bytes already buffered behind
   the request are the gateway's to recover (`head`/`body` views end with the request).
6. **Two timeouts.** `idle` is one number for a keep-alive connection waiting for its next request and a client taking a minute to send a head. A proxy wants a header timeout of its own.
7. **ALPN.** The engine selects a protocol (`--alpn`); the loop could offer `http/1.1` and refuse `h2`, which it does not speak. Nothing is done with the negotiated value yet.
8. **More than 1,024 connections** (`max_connections`) and more than one core (a second process on `SO_REUSEPORT`, not tried).
9. **A byte-fed program's authority report lists `conn_write` and `poll`** (§11.2). If the gateway's reviewers want it without them, the package needs a second set of names whose rows have no socket in them; nothing has asked yet.
