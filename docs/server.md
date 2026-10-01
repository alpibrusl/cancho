# `examples/api`: a JSON API server, and what it costs

> **Status: built.** `examples/api/api.ls` (about 580 lines) over
> `std.http`, `std.route`, `std.json` and the `net.sockets` package;
> `conformance/api.rs` (11 tests over real sockets); the load generator and the
> references behind §5's figures are in `benches/server/`. One thread,
> `poll(2)`, keep-alive, pipelining, a JSON API with four routes.

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
| **`Connection: close`**, and HTTP/1.0 closing by default | `connection_close_is_honoured` |
| **Refusals close the connection**: ten malformed or ambiguous requests each get their status, `Connection: close`, and EOF | `what_cannot_be_trusted...` |

Two of those tests were written *because a mutation survived*. Deleting the
move-to-the-front passed every test I had first, because none of them put the
start of a request behind a whole one in the same read; deleting the byte copy
in the close path passed too, because none closed a connection while the last
one held a half-sent head. Each now has a test that fails with the mutation and
passes without it.

## 3. Decisions

| Question | Answer | Why |
|---|---|---|
| Threads, an event loop, or processes? | **One thread, `poll`; scale by running copies** | A thread per connection needs a spawn payload of more than one leaf (`threads.md`), which does not exist; `poll` is POSIX and needs no per-OS code. `reuseport` as the second argument lets copies share a port and the kernel spreads the connections |
| `poll`, `epoll` or `kqueue`? | `poll` | It is the one both CI targets have. It is linear in the connections, which at the 1,024 this allows is a few microseconds a wake-up and shows up in §5 as nothing |
| How does a `[byte]`-only foreign boundary pass `struct pollfd[]`? | **A prefix of a byte array, whose length is the record count** | A foreign slice is `(pointer, length)` and only `[byte]` may cross. `poll(fds, nfds, timeout)` wants a count of 8-byte records, so the program hands over `polls[0..n + 1]`: C reads `nfds` records from the pointer, and the allocation behind it is eight times as long as the slice says. Checked on a probe before it was built on |
| Non-blocking writes? | **No** | `fcntl(F_SETFL)` is variadic in C and variadic arguments are not passed like fixed ones on Apple arm64, so calling it as an ordinary function would be wrong on one of the two targets. The cost is §6's first row |
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
| **`examples/api` (lex-sys)** | **117,000 - 129,000** | parses the head strictly, routes, builds the JSON, writes it |
| FastAPI on uvicorn, `uvloop` + `httptools` | 3,400 - 3,550 | the same route and body |
| FastAPI on uvicorn, stock | 2,560 - 2,680 | the same |

So **about ninety percent of what a C loop of the same design does without
parsing anything, and roughly forty times FastAPI** (the extremes of the two
ranges are 33 and 50) on the same core and the same request. Two copies sharing a port with `reuseport` reached
149,000 - 161,000 (the C one 167,000 - 187,000); with two of the four cores
spent on the load generator that is a measure of the generator and the kernel
as much as of the server, and is reported as that and not as scaling.

Memory: **1.5 MB resident idle, 1.6 MB after three seconds under load with 32
connections** -- the 16 MiB of connection buffers are `calloc`ed and touched
only as a connection uses them.

What this does **not** show. It is one request shape, a small answer, on
loopback: no TLS, no body larger than a few bytes, no 10,000 connections, no
tail latencies (the generator reports throughput, not percentiles). The
Python figures are for this container's Python 3.11 and this machine; a faster
interpreter would shrink the ratio, not close it. FastAPI with two workers
measured *lower* than with one in this setup (1,200); that is the way uvicorn
spreads a handful of long-lived connections across workers, not a property of
FastAPI, and **no multi-worker FastAPI figure is claimed**. And the earlier
figure in the conversation that proposed this -- 27,000 requests a second for
lex-sys against 980 for FastAPI -- was a connection **per request**; this one is
keep-alive, which is why both numbers are so much higher and the gap is wider.

## 6. What it does not do

| | |
|---|---|
| **Block-proof writes** | A client that stops reading can stall the loop once its socket buffer is full. The answers are small, so it takes a client trying; the fix is non-blocking writes, which wants a portable `fcntl` first (§3) |
| **More than one core in one process** | Run copies with `reuseport` (second argument) |
| **A body larger than the buffer, or chunked** | 413 and 501; streaming a body is a loop, and nothing here needs one |
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
