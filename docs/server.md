# `examples/api`: a JSON API server, and what it costs

> **Status: built, migrated (§8), and the loop extracted into a package
> ([`http-server.md`](http-server.md)).** `examples/api/api.cho` over `std.http`,
> `std.route`, `std.json`, `std.conns` and the native socket builtins of
> [`native-sockets.md`](native-sockets.md) -- no `Ffi`, no `extern fn`;
> `conformance/api.rs` (21 tests over real sockets); the load generator and the
> references behind §5's figures are in `benches/server/`. One thread,
> keep-alive, pipelining, a JSON API with four routes. **§§1-7 describe the
> first version, a `poll(2)` loop over `Ffi("libc")`; §8 says what replaced it
> and what that cost.** Where §§1-7 name `poll`, `send`, `MSG_DONTWAIT` or the
> `net.sockets` package they are history, not description.

## 1. Why

`json.md`, `map.md` and `http.md` built the pieces of a FastAPI-shaped stack
and each said the same thing in its last section: no socket loop, so no
requests-per-second, so no honest answer to *how fast would this be?* This is
the loop, and §5 is the answer.

It is an **example program**, not a `std` module. A server needs `Ffi("libc")`
for `read`, `write`, `poll` and the rest, and `std` does not hold foreign
declarations; the `net.sockets` package does, and the repository's own test
(`the_network_programs_are_counted`) insists that package stay the only file
that declares `bind`, `listen` and `accept`. So the program `import`s it, as
`serve/` and `collect/` do, and adds the three declarations nothing else wants:
`poll`, `signal`, `time`.

## 2. The shape

```
poll(listener + every connection)
  listener readable      -> accept, put the connection in the next slot
  connection readable    -> read what has arrived into its buffer
                            -> answer every complete request at the front of it
                            -> move what is left (the start of the next) to the front
  connection silent too long -> close it
```

Connections live in a **dense array**: the poll records, the buffers (16 KiB
each, one slab), the fill counts and the last-activity times are indexed
together, and closing connection `k` moves the last one into slot `k`, bytes
and all. The loop visits connections **last first** so that move only ever
touches one already looked at.

What that buys, and `conformance/api.rs` checks each of them:

| Property | Test |
|---|---|
| **Keep-alive**: sixty requests down one connection | `one_connection_serves_many_requests...` |
| **Pipelining**: thirty requests in one write, answered in order | same |
| **A request split across segments** (inside the request line, a header, at the blank line, inside the body) is answered once, whole | `a_request_split_across_segments...` |
| **The front of the next request after a whole one** survives being moved to the front of the buffer | `the_start_of_the_next_request...` |
| **Closing a connection** hands its slot to the last one *with that one's half-received bytes* | `closing_a_connection_moves_the_last_one...` |
| **A silent client does not hold anyone up**: fifty open and speechless, one half a head, forty other requests served inside three seconds | `a_silent_client_does_not_hold_up...` |
| **Sixty-four concurrent clients**, forty requests each, every answer checked against what that client asked | `many_clients_at_once...` |
| **Clients that vanish**: ask and leave without reading (the write to a closed socket is `SIGPIPE`), leave in a head, leave in a body | `clients_that_vanish...` |
| **Idle timeout**: a quiet connection is closed, a busy one never is | `an_idle_connection_is_closed...` |
| **A client that stops reading stalls only itself**: a thousand requests for 32 KiB (32 MB of answers) and none read; thirty other requests served inside two seconds; then the stalled client reads every answer, whole and in order | `a_client_that_stops_reading_stalls_only_itself` |
| **A slow reader gets every byte in the right order**: a thousand answers of different sizes, `send` limited to 1,500 bytes so each goes in pieces, bodies a repeating alphabet so a misplaced byte shows | `a_slow_reader_gets_every_byte...` |
| **A client that never reads is closed** after the idle time, and the server is unharmed | `a_client_that_never_reads_is_closed...` |
| **A closing connection closes only once its large answer has gone** (and a refusal bigger than one piece still arrives) | `a_connection_that_is_to_close_closes_only_once...` |
| **`Connection: close`**, and HTTP/1.0 closing by default | `connection_close_is_honoured` |
| **Refusals close the connection**: ten malformed or ambiguous requests each get their status, `Connection: close`, and EOF | `what_cannot_be_trusted...` |

Two of those tests were written *because a mutation survived*. Deleting the
move-to-the-front passed every test I had first, because none of them put the
start of a request behind a whole one in the same read; deleting the byte copy
in the close path passed too, because none closed a connection while the last
one held a half-sent head. Each now has a test that fails with the mutation and
passes without it.

The write path went through the same exercise, and it was harder, for reasons
worth keeping. Making `send` blocking again failed the stall tests at once (the
other clients starve). But deleting the shift of unsent bytes after a partial
send **survived three versions of the test**:

1. The first never made a `send` partial at all. **On Linux loopback a `send`
   is whole or refused, never partial** -- the kernel accepts a whole skb once
   any room is free -- so the resume code ran only for the last hundred bytes of
   an answer and its shift loop ran zero times. A small receive buffer on the
   client made the test crawl (a window under one segment and delayed ACKs: about
   100 KB/s), and a small `SO_SNDBUF` still gave whole sends.
2. So the program gained a fourth argument, a **send quantum**: no `send` is
   handed more than that many bytes. It is a plausible fairness knob, and the
   only deterministic way to make partial sends certain on both operating
   systems.
3. Even then the mutation survived, because the `/blob` body was one repeated
   byte and a chunk sent twice is indistinguishable from the right one. The
   body is now `abcdefghij...` repeating, and the test checks every byte's
   position.

The last case, a connection that is to close after a large answer, was caught
by neither the stall tests nor the refusal tests; its first test could not tell
"closed promptly" from "closed by the idle timeout nine seconds later" and
passed on the mutation anyway. It now runs with a thirty-second timeout, so only
a prompt close passes.

**And then macOS failed, which none of the above could have found.** The first
push went green on Linux and red on macOS, on the two tests about a client that
stops reading. The reasoning that followed went wrong twice before the data
settled it, and the order is worth keeping:

1. *Wrong flag?* `0x40` is `MSG_WAITALL` on macOS. A test of the flag the server
   chose passed: it picked `0x80`, correctly.
2. *Bigger kernel buffers, so 32 MB never stalled anything?* A Rust probe of how
   much a loopback connection queues for a non-reading peer: **540 KB** on the
   macOS runner. No.
3. So the only way a client that never reads could be delivered all 32 MB was
   for the server to have been *blocked in `send`* and let go when the client
   finally read, which also explains the other client's read timing out. A probe
   with nothing of ours in the path -- a blocking socket, a peer that does not
   read, `send(MSG_DONTWAIT)` -- **blocked on macOS.** The flag is not honoured
   there for a send.

The fix is the timeout, and it was checked the way the rest were: with the flag
removed and the timeout kept, the stall tests pass on Linux (Linux behaving as
macOS does); with both removed they fail with the same `EAGAIN` the macOS job
reported. That reproduces the failure on the machine I could run on and shows
the fix is what cures it.

`a_send_to_a_peer_that_is_not_reading_returns_instead_of_waiting` is that probe,
kept: it sets the timeout and flag as the server does and fails, naming the
flags, on a system where a send still waits.

## 3. Decisions

| Question | Answer | Why |
|---|---|---|
| Threads, an event loop, or processes? | **One thread, `poll`; scale by running copies** | A thread per connection needs a spawn payload of more than one leaf (`threads.md`), which does not exist; `poll` is POSIX and needs no per-OS code. `reuseport` as the second argument lets copies share a port and the kernel spreads the connections |
| `poll`, `epoll` or `kqueue`? | `poll` | It is the one both CI targets have. It is linear in the connections, which at the 1,024 this allows is a few microseconds a wake-up and shows up in §5 as nothing |
| How does a `[byte]`-only foreign boundary pass `struct pollfd[]`? | **A prefix of a byte array, whose length is the record count** | A foreign slice is `(pointer, length)` and only `[byte]` may cross. `poll(fds, nfds, timeout)` wants a count of 8-byte records, so the program hands over `polls[0..n + 1]`: C reads `nfds` records from the pointer, and the allocation behind it is eight times as long as the slice says. Checked on a probe before it was built on |
| Non-blocking writes? | **A `send` that never waits for room: `MSG_DONTWAIT` where the OS honours it, a one-millisecond `SO_SNDTIMEO` where it does not. Not `fcntl(O_NONBLOCK)`** | `fcntl(F_SETFL)` is variadic in C, and variadic arguments are not passed like fixed ones on Apple arm64, so declaring it as an ordinary function would be wrong on one of the two targets; `setsockopt` and `send` have fixed signatures. **macOS ignores `MSG_DONTWAIT` on a send to a blocking socket** (§2 has how that was found), so on macOS it is the timeout that does the work: a `send` into a full buffer returns after at most a millisecond with what it managed or `EAGAIN`, which the output buffer already handles. The flag is `0x40` on Linux and `0x80` on macOS -- where `0x40` is `MSG_WAITALL`, so the program asks which it is running on (`SO_REUSEADDR` at level 1, option 2 succeeds only on Linux) |
| What happens to what the kernel did not take? | **It waits in the connection's own 64 KiB output buffer, and the connection is read no more** | The record asks `poll` for `POLLOUT` instead of `POLLIN`, so a client that does not take its answers cannot make the server buffer without bound: its next requests wait in the kernel. A connection that cannot hold even one more answer is closed, and one that makes no progress for the idle time (a byte read, a byte sent) is closed too |
| Several answers queued behind a partial send? | **Never: `drain` stops at the first answer that did not go whole** | At most one answer is ever waiting, so order cannot be wrong and the buffer needs to hold one answer, which `/blob`'s 32 KiB maximum bounds |
| After a refusal, or `Connection: close` | Close **once the output has gone**, not at the first partial send | Closing early would cut the answer short; closing never would hold the slot for ever |
| `SIGPIPE` | `signal(13, SIG_IGN)` at start | 13 and 1 are the same on both targets, and the alternative is a client that can kill the server by hanging up |
| `SO_REUSEADDR`, `SO_REUSEPORT` | Both operating systems' numbers are set; the wrong pair is refused harmlessly | Linux: level 1, options 2 and 15. macOS: level `0xffff`, options 4 and `0x200`. There is no way to ask which OS this is, and ignoring the result is correct |
| Where is a request's body? | In the same buffer as its head, so head and body together must fit 16 KiB | A request that cannot is refused at once: 413 for a body, 431 for a head. Not waited on |
| After a malformed request | Answer 400 and **close** | There is no telling where the next request starts, which is the whole of the smuggling problem |
| The access log | None | A `write` a request is the most expensive thing the loop could add; a program that wants one knows where |

## 4. What it answers

```
GET  /health        {"ok":true}
GET  /users/:id     {"id":42,"name":"user-42"}      400 if :id is not a number
POST /add           {"a":1,"b":2}  ->  {"sum":3}     422 if not two integers; a body that is
                                                     not JSON says what is wrong with it
GET  /search?q=...  {"q":"a b","length":3}           q percent-decoded, `+` a space; length in bytes
```

and `{"error":...}` for a path no route has (404), a path with another method
(405), a request the parser refuses (400, and the connection closes), a chunked
body (501), a request that cannot fit (413) and a head that cannot (431).

## 5. Measurements

The machine is a shared four-core container, so every figure is the spread of
three five-second runs and the **ratios** are the claim, not the absolute
numbers. The load generator is a C program written for this
(`benches/server/kload.c`; closed loop: 2 threads, 16 connections each, every
connection sends one request and waits for the whole response, `GET /users/42`,
keep-alive), pinned to two cores; each server to one.

| Server, one core | requests/second | what it does per request |
|---|---|---|
| C, `poll`, **no parsing** | 128,000 - 137,000 | finds the blank line, writes a fixed answer |
| **`examples/api` (cancho)** | **117,000 - 129,000** | parses the head strictly, routes, builds the JSON, writes it |
| FastAPI on uvicorn, `uvloop` + `httptools` | 3,400 - 3,550 | the same route and body |
| FastAPI on uvicorn, stock | 2,560 - 2,680 | the same |

So **about ninety percent of what a C loop of the same design does without
parsing anything, and roughly forty times FastAPI** (the extremes of the two
ranges are 33 and 50) on the same core and the same request. Two copies sharing a port with `reuseport` reached
149,000 - 161,000 (the C one 167,000 - 187,000); with two of the four cores
spent on the load generator that is a measure of the generator and the kernel
as much as of the server, and is reported as that and not as scaling.

**After non-blocking writes** (`send` with `MSG_DONTWAIT` and a send timeout, per-connection state,
`POLLOUT` bookkeeping), measured in the same session against the blocking
version it replaced, three runs each, twice: the blocking version
128,000 - 151,000, the new one 119,000 - 141,000, with the C ceiling itself
spread over 123,000 - 150,000 in that session. **No measurable regression
within the machine's noise**; the point estimates are about four percent apart,
which three five-second runs cannot resolve. The figures in the table above are
from an earlier, quieter session and are kept as measured.

Memory: **1.5 MB resident idle, 1.6 MB after three seconds under load with 32
connections** -- the 16 MiB of connection buffers are `calloc`ed and touched
only as a connection uses them.

What this does **not** show. It is one request shape, a small answer, on
loopback: no TLS, no body larger than a few bytes, no 10,000 connections, no
tail latencies (the generator reported throughput, not percentiles; §10 has them now). The
Python figures are for this container's Python 3.11 and this machine; a faster
interpreter would shrink the ratio, not close it. FastAPI with two workers
measured *lower* than with one in this setup (1,200); that is the way uvicorn
spreads a handful of long-lived connections across workers, not a property of
FastAPI, and **no multi-worker FastAPI figure is claimed**. And the earlier
figure in the conversation that proposed this -- 27,000 requests a second for
cancho against 980 for FastAPI -- was a connection **per request**; this one is
keep-alive, which is why both numbers are so much higher and the gap is wider.

## 6. What it does not do

| | |
|---|---|
| **A stalled client can still cost a millisecond** | Where `MSG_DONTWAIT` is ignored (macOS), a `send` into a full buffer waits out its one-millisecond timeout before returning; the connection then waits for `POLLOUT`, so it costs the loop one millisecond per stall, not per request. A thousand connections stalling at once would cost a second, once. On Linux `send` returns at once |
| **More than one core in one process** | Run copies with `reuseport` (second argument) |
| **A body larger than the buffer** | 413 -- but the buffer is the fifth argument now (4 KiB to 1 MiB; fewer connections at a bigger one, so the input buffers together stay under 256 MiB), and a chunked body is decoded (§9). Streaming a body to a handler as it arrives is still not done: it would need a handler that takes a stream, and nothing here needs one |
| **TLS** | `examples/tls_client` is the client half; nothing serves it |
| **Refusing a malformed head before its blank line arrives** | `http.md` §3: garbage with no terminator waits for the idle timeout, nine seconds by default |
| **More than 1,024 connections** | A constant. Past it new connections are closed on accept |
| **HTTP/2, `Expect: 100-continue`, upgrades** | nothing has asked |
| **What makes FastAPI feel like FastAPI** | request models, validation, generated OpenAPI. This language has no reflection and no macros: a model is a hand-written function over the `std.json` tape, as `add` is |

## 7. Found along the way

Nothing in the compiler this time, which is worth stating given the last two
pieces each found a bug: after two signature slips the compiler caught (a
`json.is_int` call with one argument too many, a local named like a function),
the server answered every route correctly on its first run. What the tests found was two places the
*tests* were thin (§2), and one wrong expectation of mine (`"a b cé"` is seven
bytes, not six).

## 8. Over native sockets: what changed, and the number that fell

`native-sockets.md` is the account of the builtins; this is what they did to
this program.

**What went.** Four `extern fn`s of its own and eight imported from
`packages/net-sockets` -- twelve libc symbols behind `Ffi("libc")`, which made
the authority report say `ffi("libc")` and nothing else. `signal(SIGPIPE)`
(`conn_write` cannot raise it), the `SO_SNDTIMEO` workaround and the per-OS
`MSG_DONTWAIT` flag with the `is_linux` probe (`conn_write` does not wait on a
non-blocking connection, on either kernel), the `SO_REUSE*` guesses (a flag on
`tcp_listen`), the hand-built `sockaddr_in`, and the poll-record byte array and
its `set_record`/`move_record` (a `Poller` and tokens). `net.lock` and the
package fetch are gone from the build. `cancho authority` now reports
`net_in` -- naming no port, because the port is an argument -- `conn_accept`,
`conn_read`, `conn_write`, `poll`, `clock`, `heap`, `args` and the console, and
`the_server_holds_no_foreign_authority` pins it.

**What changed in the loop.** Connections are a `std.conns.Table`, found by
slot, and the `Poller`'s token is the slot plus one. Closing one frees its slot
instead of moving the last connection into its place (so the swap-remove, and
the test named for it, now check only that closing one never disturbs another).
The idle timeout is a once-a-second sweep over the slots with a monotonic
clock, not a check made as each connection is visited.

**The number.** Same machine, same load (2 threads x 16 connections, closed
loop, `GET /users/42`), server on one core, three five-second runs:

| | `poll` + `Ffi` (before) | `epoll` + handles (after) |
|---|---|---|
| `examples/api` | 134,600 - 137,700 | **72,700 - 74,400** |
| C, no parsing | 145,000 - 156,000 (`poll`) | 74,000 - 84,000 (`epoll`) |

**A 47% fall, and it is the kernel, not the program.** The C reference with
the same loop and no parsing falls the same way when its `poll` is replaced by
`epoll` -- about 150,000 to about 80,000 -- so the cancho server holds about
nine tenths of the C ceiling for either, as it did before. What was ruled out,
in order: the extra builtin calls and the ticket round trip (`conns.read` and
`conns.write` are about one percent of the instructions each, under
`callgrind`); extra syscalls (`strace -c`: one receive and one send per request
and a wait every 20-odd, for both); the clock (19 ns a call, no syscall);
`O_NONBLOCK` (the same with blocking connections); and server CPU (identical --
about 390 ticks in the window for both, so each request costs the kernel twice
as much). What is left is `epoll` against `poll` in this environment: a
Firecracker VM, four cores, kernel 6.18, where a persistent wait-queue
registration costs every arriving packet a callback that `poll` -- registering
only while it waits -- never pays. That last step is an inference from the C
comparison, not something observed inside the kernel.

**What this corrects.** `native-sockets.md` §4 said `epoll` was *"also the
scalable answer ... a performance change as well as a safety one"*. Measured
here it is the reverse: `poll` is faster by about 1.8x at 32 connections and by
about 1.8x at 300 (C: 186,000 and 165,000 against 96,000 and 105,000; cancho
before and after: 173,000 and 159,000 against 86,000 and 88,000). The crossover
where `epoll`'s O(ready) beats `poll`'s O(registered) was **not found at or
below 300 connections**; 800 could not be measured (the load generator reported
short reads against every server, C included). So the claim is withdrawn until
it is measured on hardware that is not a VM. The `Poller` is the same
abstraction either way: a `poll(2)`-backed one would need a registration table
in memory the runtime owns, a larger change than this was, and it is recorded in
`native-sockets.md` §10.4 as the open question it now is.

**Still true.** Against FastAPI (2,560 - 3,550 in §5) the migrated server is
still about **twenty-five times faster** on the same core and request, down from
forty, and at 73,000 it holds about nine tenths of what a C `epoll` loop does
without parsing anything.

## 9. Gaps closed since

`Allow` on a 405 (`route.allowed`, `http.respond_head_with`) and typed route
parameters (`route.param_nat`, `param`, `param_decoded`) -- both in `http.md` §8
-- and `examples/api` uses them: `GET /add` answers `405` with `Allow: POST`,
and `/users/:id` and `/blob/:n` read their number with `param_nat` instead of a
private parser.

## 10. Tail latency

§5 said *"no tail latencies (the generator reports throughput, not
percentiles)"*. `benches/server/kload.c` now takes a sixth argument, `lat`, and
prints percentiles in microseconds: from just before a request is written to the
whole response having been read, one sample per request, every sample kept and
sorted (no sketch). **It is a closed loop with K requests in flight a thread, so
a sample is a request's service time plus its wait behind the others in its
round** -- what a client of a busy server sees, not what an idle one does, and
the reason these numbers grow with the connection count below. Same machine and
placement as §5 (server on one core, load on two others), 32 connections, four
seconds, one run each:

| Server, one core | req/s | p50 | p90 | p99 | p99.9 | max |
|---|---|---|---|---|---|---|
| C, `poll`, no parsing | 149,000 | 137 us | 220 us | 349 us | 1.1 ms | 3.3 ms |
| **cancho, `poll` + `Ffi` (before §8)** | 139,000 | 147 us | 229 us | 418 us | 1.7 ms | 33 ms |
| C, `epoll`, no parsing | 84,000 | 228 us | 374 us | 591 us | 2.0 ms | 5.1 ms |
| **cancho, `Poller` + handles (now)** | 71,000 | 285 us | 449 us | 647 us | 1.8 ms | 7.8 ms |
| FastAPI on uvicorn, stock | 2,100 | 12.7 ms | 18.3 ms | 53 ms | 62 ms | 64 ms |

What it says. **cancho tracks the C loop of the same kind at every
percentile** -- its p99 is within 10% of C's `epoll` and its p99.9 is below it --
so the loop has no pathological tail of its own; the shape is the kernel's, which
§8 found. The one thing worth noticing is the `max` of 33 ms in the `poll`
version: one sample in 555,000, a single stall (a scheduler hiccup in a shared VM
is the likeliest cause; it did not recur in the later runs and is reported rather
than explained away). Against FastAPI the gap is **about 45 times at the median
and about 80 times at p99**, wider than the throughput gap because Python's tail is
long -- garbage collection and the event loop's own scheduling -- where this
program allocates from a heap it controls.

With 400 connections (the generator could not open more than 256 a thread until
this change; it now allocates its connection table) the same server does 84,000 a
second at **p50 2.9 ms, p99 7.5 ms, p99.9 9.9 ms, max 10.9 ms**. That is queueing,
not slowness: 400 requests in flight at 84,000 a second is 4.8 ms each by Little's
law, and the p50 is 2.9.

Caveats, and they are the §5 ones plus this: one run each, so a ratio is the
claim and a digit is not; a shared VM with a noisy tail of its own; loopback, so
no network; FastAPI measured here with the versions this container installs
(0.142, uvicorn 0.54) at 2,100 requests a second, a little under §5's 2,560 --
which is the point of keeping the ratio and not the figure.

**Chunked request bodies and larger bodies.** `std.http.dechunk` decodes a chunked
body into a buffer -- strictly: it refuses chunk extensions and trailers (where
request smuggling lives, and nothing here needs either), a bare line feed, a size
that is not 1-8 hex digits, and a body that would not fit -- and the server hands
the handler the decoded bytes, so `/add` cannot tell a chunked POST from one with
a length (`http.md` §5.1). A body that arrives in pieces is decoded again from its
first chunk each time more of it does, which costs in proportion to the body and
is bounded by the buffer. `api`'s fifth argument sets the per-connection input
buffer from 4 KiB to 1 MiB: a 40 KB JSON body is refused with 413 by default and
answered with `api 8080 no 9 0 65536`. **Measured**: the decoder agrees with a
second, independently written decoder on 3,604 generated and mutated bodies
(every outcome reached; two mutations of the decoder each fail the test), and the
server's throughput and tail are unchanged (69,000-72,000 a second, p99 about
0.65 ms).

What is **still not** done *in this server*: streaming a body to a handler as it arrives (so a
100 MB upload does not need a 100 MB buffer), `Expect: 100-continue`, and
trailers. (*`packages/http-server` has all three since `http-server.md` §12: a server that calls `limits`
streams, answers `100 Continue` when its application asks, and accepts and discards trailers and
chunk extensions. `examples/api` does not call it.*)
