# `http.server`: the server loop as a package

> **Status: built (§7).** `examples/api/api.cho` carried the whole
> loop -- accept, read, parse, frame, pipeline, back-pressure, idle sweep --
> with the application's routes inside it. A second server would copy 600
> lines. This extracts the loop; `docs/server.md` stays the account of what the
> loop does and costs, this says where it lives and why it has this shape.

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
(*TLS: §11 says what stops it and proposes the change.*)

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


## 11. Serving it over TLS

`packages/tls` has a server since `tls-server.md` step 2, and `examples/tls_echo` (`tls-server.md` §11) is how a
program puts the engine between a socket and its own loop: the socket's bytes go to `tls.feed`, the plaintext comes out
of `tls.recv`, the answer goes into `tls.send`, and `tls.take` gives the bytes for the socket. **This package cannot be
that loop's plaintext side today**, and the reason is precise: it never sees a connection's bytes except through a
socket it holds itself.

* `wait` owns the `Listener`'s readiness and `accept_all` calls `tcp_accept` and `conns.put`: every connection the
  server knows is a `Conn` in its own `conns.Table`.
* Input reaches a connection's buffer in one place, `step`, by `conns.read(tab, k, bufs[...])`. There is no other way
  into `bufs`.
* Output leaves in three: `emit` (the fast path: `conns.write` straight to the kernel, the rest queued), `step`'s write
  of what was queued, and the refusals `produce` sends through `emit`. `deliver_to` (`respond`, `answer`) calls `emit`.
* `shut` closes the `Conn`, and `settle` re-registers it with the server's poller.

So a TLS terminator in front of it would have to give it a socket: the one way that works with no change is a
loopback relay, the TLS side registered in the server's own poller (`poller`, `first_token`, §10) and connecting to
the server's listener on 127.0.0.1 for each client. It doubles the sockets, copies every byte once more each way, adds
`net_out` to the program's row, and gives the HTTP side 127.0.0.1 for every client. It is not built here, and it is
not what the broker or the gateway should copy.

**The smallest change: a server with no sockets.** The connection state, the parser, the framing, pipelining,
back-pressure, `hold`/`answer` and the idle sweep are already independent of where bytes come from; only the five
places above touch a socket. The proposal is a second way to open a `Server`, and four calls:

```
srv = server.open_bytes(heap, size, chunk, idle)   // no Poller, no Listener: the application owns the sockets
k   = server.attach(heap, srv)                      // a connection: its slot, -1 when full (wait's accept, without the socket)
n   = server.input(srv, k, plaintext)               // bytes into k's buffer (step's read): how many fit; 0 is back-pressure
server.ready(heap, srv, clock)                      // wait without poller_wait: sweep idle, queue the connections with input
//    next / head / parsed / body / respond / hold / answer: unchanged
n   = server.output(srv, k, out)                    // what the server would have written: bytes for tls.send
server.closing(srv, k) -> bool; server.detach(srv, k)   // shut: the application closes, after close_notify
```

In a server opened this way the `Core` carries a flag and no `Poller`; `emit` and `step` append to the connection's
output buffer (`pends`, which is already there and already bounded) instead of calling `conns.write`, `output` drains
it, `shut` frees the slot without a `Conn`, and `settle` does nothing. The application's loop is then
`tls_echo`'s with its echo replaced: `tls.recv` into `server.input`, `server.ready`, `next` until -1 with the
application's handlers, `server.output` into `tls.send`. The idle timeout stays the package's, the handshake bounds
and the TLS timeouts stay the application's, which is the split `tls-server.md` §7 asks for.

It is about a hundred lines in `server.cho`, a republished store, a second test program, and the 21 `api` tests run
again on the socket path; it changes a published package's interface (four functions added, none changed). That is
not small enough to be obviously right inside the PR that builds the example, so it is not made here: it is
`cancho-gateway`'s to ask for, and the PR that makes it measures it against `examples/api`'s figures. Until then, a
program that must answer HTTP over TLS parses with `std.http` on the plaintext itself, as `tests/programs/tls_serve.cho`'s
`http` mode does, and gives up the pipelining and back-pressure this package would give it.

