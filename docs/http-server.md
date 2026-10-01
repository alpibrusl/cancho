# `http.server`: the server loop as a package

> **Status: design, then built (§7).** `examples/api/api.ls` carried the whole
> loop -- accept, read, parse, frame, pipeline, back-pressure, idle sweep --
> with the application's routes inside it. A second server would copy 600
> lines. This extracts the loop; `docs/server.md` stays the account of what the
> loop does and costs, this says where it lives and why it has this shape.

## 1. What was asked

Take the loop out of the example so a server is its routes and a `main`. It is
the piece `lex-sys-web` would build on, and the first package that imports
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
