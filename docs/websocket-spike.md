# A WebSocket spike: what it takes to hold ten thousand charge points

> **Status: S1 measured (§8), S2 half done (§9), gaps catalogued (§10).** The design and
> the gates (§1 to §7) were written before the server; the results and the gaps are added
> after them, and a claim that turns out false is corrected where it stands. The one change
> to the gates after the first run is recorded in §8.

## 1. The question

`lex-csms` (the OCPP central system of the EV-charging services) holds a WebSocket per charge
point, answers a few messages on each, and sends commands back down one. The question
that was asked is whether lex-sys is a better fit for that than the runtime it is written on.
Two things are wanted from one program:

1. **A measurement**, not an opinion: the same connection-holding job on lex-sys and on the
   `lex` runtime `lex-csms` runs on.
2. **A list of what lex-sys lacks** to write an equivalent service, found by writing the
   smallest piece of one: every missing piece is a line in §8, with where it would live.

The job is the *connection layer*: accept, upgrade to WebSocket (RFC 6455) with the `ocpp1.6`
subprotocol, and answer the OCPP-J calls a charge point sends on a schedule
(`BootNotification`, `Heartbeat`, `StatusNotification`, `MeterValues`). No database, no
authorisation, no commands down: `lex-csms` does those too, so a comparison with it is
**not** a comparison of two equal programs, and §5 says how that is handled.

## 2. What is built

`examples/ocpp_ws/`: one lex-sys program, single threaded, on the poller: a listener, a table
of connections (`std.conns`), a read buffer and a write buffer for each, and a state per
connection (handshake, open, closing). It answers:

| a charge point sends | the server answers |
|---|---|
| `[2,"<id>","BootNotification",{...}]` | `[3,"<id>",{"currentTime":"<ISO 8601>","interval":300,"status":"Accepted"}]` |
| `[2,"<id>","Heartbeat",{}]` | `[3,"<id>",{"currentTime":"<ISO 8601>"}]` |
| `[2,"<id>","StatusNotification"` or `"MeterValues",{...}]` | `[3,"<id>",{}]` |
| any other call | `[4,"<id>","NotImplemented","",{}]` |
| a WebSocket ping, close | a pong; a close, and the connection is closed |

A fragmented message, a frame over the buffer, a binary frame and a client that does not ask for
`ocpp1.6` are refused (close code 1002/1003/1009, or HTTP 400); that is a limit of the spike and §8
lists it. It needs three things `std` does not have, which the example carries itself for now and §8
counts: SHA-1, base64, and an ISO 8601 time from a Unix one.

## 3. What is measured, and how

* **Load generator**: `scripts/ws_load.py`, Python `asyncio` and raw sockets (so it is the same for every
  server), several processes. It does the handshake, sends a `BootNotification`, checks the answer
  (id echoed, `Accepted`, a parseable `currentTime`, an integer `interval`), then does what the
  scenario says.
* **Server numbers** are read from the kernel, not from the program: resident memory from
  `/proc/<pid>/status` and CPU time from `/proc/<pid>/stat`, before and after.
* **The machine** has four cores, shared by the server and the generators: the server is pinned to one core
  and the generators to the other three, and the numbers are the median of five runs.
* **An independent check** of the protocol: the `websockets` library (a WebSocket implementation that is
  not ours) connects to the server and speaks to it, and the RFC 6455 example (`dGhlIHNhbXBsZSBub25jZQ==` gives
  `s3pPLMBiTxaQ9kYGzzhZRbK+xOo=`) and SHA-1's own test vectors are checked against `hashlib`.
* **Runs.** The plan said "median of five"; what was run is **one** full 10,000-connection run of the lex-sys server (the
  scenario is nine minutes with the idle and heartbeat phases) and one run of each `lex` runtime point. §8 says which
  numbers are one run; none is a median. Treat differences under about 2x as noise and read only the large ones.

## 4. The gates, fixed before any run

| | gate |
|---|---|
| **G1 correct** | the `websockets` client completes the handshake with the `ocpp1.6` subprotocol and gets the right answer to each of the four calls; a ping is answered with the same payload; the SHA-1 and base64 of the example match `hashlib` for 1,000 random inputs of every length from 0 to 130. |
| **G2 scale, idle** | 10,000 connections open and each accepted by a `BootNotification`: the server's resident memory is **at most 100 MB (10 KB a connection)**, and with nothing sent for 30 s its CPU time rises by **less than 1% of a core**. |
| **G3 latency** | with those 10,000 connections each sending a `Heartbeat` every 30 s (333 a second) for two minutes: **p99 round trip under 20 ms**. And all 10,000 sending a `Heartbeat` in the same instant: **every one answered within 2 s**. |
| **G4 refusal** | a message larger than the buffer, a fragmented one, a ping with a 200-byte payload, and 1,000 connections that connect and send nothing (no handshake): none crashes or wedges the server; each is refused or closed, and a good connection still works afterwards. |
| **G5 throughput** | reported, **no gate**: 200 connections in a closed loop, each sending a 1 KiB `MeterValues` as soon as the last is answered, for ten seconds: messages a second. |

## 5. The comparison, and what it may not be used for

The same scenarios run against two servers on the `lex` runtime, each of which is a *different* thing and is
labelled as such:

* **`lex-csms` itself**, if it can be started on this machine (its dependencies are git repositories; some may
  not be reachable from here). It does more per message than the spike (a SQLite write on `BootNotification`),
  so on `Heartbeat` it is the fairer comparison and on `BootNotification` it is not.
* **A minimal WebSocket echo of the same four answers written on the `lex` runtime** (`net.serve_ws_fn_actor`),
  which separates "the runtime" from "what `lex-csms` does". If `lex-csms` cannot be started, this is the only
  baseline, and the report says so.

**What was actually run:** the stand-in only (`scripts/ws_standin.lex`, run with `lex run --allow-effects concurrent,io,net,time`, `lex` built from `lex-lang` `main` at the time; it listens on 9201). `lex-csms` was **not** started: the stand-in already cannot hold the
load (§8), `lex-csms` does strictly more per message than the stand-in, so a number from it could not change the
conclusion about the connection layer, and it would have added a SQLite and a dependency-resolution variable. That is an
inference, not a measurement, and it is the first thing to repeat if the conclusion is ever disputed. The stand-in
answers with a **constant** `currentTime` (cheaper than the real clock, so it flatters the `lex` runtime) and registers no
actor name (`name_of` returns `""`; `lex-csms` registers one per connection).

**What the numbers may not be used for:** to say lex-sys is faster or smaller than `lex-csms` as a service. They
say what the connection layer costs in each, on one machine, with these scenarios.

## 6. Redis, and whether `lexsys-cache` can stand in for it

What `lex-csms` uses it for, read from its source (`src/frames.lex`, `src/server.lex`): with `REDIS_URL` set, a command for
a charge point is **published** (`PUBLISH csms:cmd:<cp_id>`), and a `subscribe_loop` is meant to `PSUBSCRIBE csms:cmd:*`
so the pod that holds the connection delivers it. With `REDIS_URL` unset, and in the recommended single-process
`main_all`, Redis is not used at all: dispatch is an in-process registry lookup. And the multi-pod path **does not work
today**: `frames.lex` says the subscriber is started with `conc.spawn`, which does not run it in the background, so
cross-process delivery "remains unfixed", and the README says the same of the 2.0.1/2.1 servers. So Redis is, today, a
publish with no working listener, in an optional mode.

`lexsys-cache` answers 38 of Redis's commands (strings, expiry, an LRU policy, `INCR`) and **not `PUBLISH`,
`SUBSCRIBE` or `PSUBSCRIBE`**; §9 confirms it against the running server. Two ways to get the same effect:

1. **A presence registry instead of a bus.** A pod writes `SET cp:<id> <pod address> EX 90` when a charge point
   connects and on every `Heartbeat` (the TTL is the liveness detector: a pod that dies stops refreshing), and `DEL`s it on
   disconnect. A command is routed with `GET cp:<id>` and sent to that pod **directly** (HTTP). This uses only
   commands `lexsys-cache` has, and replaces the thing that does not work today. What it needs from lex-sys is a **RESP
   client**, which does not exist (`lexsys-cache` is a server; `lexsys-pg` is the Postgres client).
2. **Add `PUBLISH`/`PSUBSCRIBE` to `lexsys-cache`.** A server that already holds connections and pushes to them in one
   loop is the easy case; it is a slice of `lexsys-cache`, not of lex-sys, and it is only worth doing if something needs
   a bus rather than a registry. Not started.

## 7. Order of work

S1: SHA-1 and base64 (differential tests first), the frame codec (tested without a network), the server, G1 to G4, the load
generator, the baseline, G2 to G5. S2: a RESP client in lex-sys and the registry. S1 is done. S2 is done only as far as the
**server side** of the registry (§9); the lex-sys RESP client is not written (§10).

## 8. S1 results

All numbers: one run, server pinned to core 0, generators (3 processes) on cores 1 to 3, `scripts/ws_load.py`, server CPU and
RSS from `/proc`. Reproduce with the commands in the header of each script.

**G1 correct: passed.** `scripts/ws_conformance.py` (the independent `websockets` client, 1,000 silent connections
underneath, every action, ping, close, bad JSON, non-OCPP JSON, a 1.5 KiB message) passes all checks;
`scripts/ws_codec_check.py` finds 0 differences in 2,096 SHA-1/base64 comparisons against `hashlib`/`base64`.
**G4 refusal: passed**, same script (oversize, fragmented, 200-byte-ping, 1,000 silent connections; the server runs on and a
good connection still works).

| (one run) | gate | lex-sys server, 10,000 connections | `lex` runtime stand-in |
|---|---|---|---|
| **G2** memory | at most 100 MB | **79.8 MB** (38 MB of it is the slabs allocated before the first connection) | 187 MB at **1,000** connections, 356 MB at 2,000, 778 MB at 4,500: **~0.18 MB a connection** (a thread each); 10,000 would be about 1.8 GB (extrapolated, not run) |
| **G2** idle CPU | under 1% of a core in 30 s | **0.0%** | **72% at 1,000**, 98% at 1,500, 99% at 2,000 (saturated): about 0.07% of a core per idle connection |
| **G3** burst | all answered in under 2 s | 9,999 answered in **0.21 s** (p99 121 ms) | 1,000: 0.18 s (p99 123 ms); 2,000: 0.48 s (p99 315 ms); 4,500: **36.9 s** |
| **G3** heartbeats | p99 under 20 ms | 333/s offered for 150 s: p50 0.34 ms, **p99 1.2 ms**, max 55 ms, server at 1.7% of a core | not comparable (see below); at 1,000 connections and 5 s interval, p99 8.5 ms with the server at 75% of a core |
| **can it open them** | | 9,999 of 9,999 | opened 1,000, 1,500, 2,000; **failed to open 2,500, 3,000, 5,000, 7,500 and 10,000** (the load generator got `Connection reset by peer`; not diagnosed). 4,500 did open when the generators opened only 15 at a time, but took about 20 minutes and 1,246 CPU-seconds |
| **G5** throughput | reported | 40,015 msg/s at 56% of the core: **generator-bound**, so a lower bound | 16,576 msg/s at **100%** of the core |

G2 and G3 pass for lex-sys. **The one change to the gates after the first run:** `ws_load.py` read the handshake
reply with a single `recv`, and compared the accept header case-sensitively; both are bugs in the generator that
the `lex` runtime's server (which writes the reply in pieces, with lower-case header names) exposed. They were
fixed (read to the blank line; compare the header case-insensitively and the key exactly), and the lex-sys numbers
above were taken before the fix with a server whose reply arrives in one piece, which the fix does not change.
No gate was edited.

**What this says, and does not.**
* On one core the `lex` runtime's WebSocket server holds about **1,000 connections before it spends 70% of the core
  doing nothing**, and does not hold 10,000 at all here; lex-sys holds 10,000 in 80 MB with the core idle. The
  mechanism the numbers fit is in `crates/lex-runtime/src/ws.rs`: each connection is a thread whose read has a
  **50 ms timeout** (`set_read_timeout(Some(Duration::from_millis(50)))`, six places), so each idle connection wakes 20
  times a second. Idle CPU rose linearly with connections until it saturated, which is what that predicts. I did not
  profile it, so it is the likely cause, not a proven one.
* **Pinning to one core is the comparison's weak point.** The `lex` runtime is multi-threaded and a production
  deployment gives it cores. Linear extrapolation of 0.07% of a core a connection says 10,000 idle connections need
  about **seven cores** just to sit there; that is extrapolated, not measured, and the real figure could differ.
  lex-sys's server is single threaded by design and was given one core.
* **It does not say** lex-sys is a better *service*: the lex-sys program answers four messages and does no
  authorisation, storage, TLS or commands down. It says the **connection layer**, the part that must hold ten thousand sockets, is
  cheap on lex-sys and expensive on this runtime, by a margin (0.0% against 72% of a core idle at 1,000 connections; 80 MB against a
  projected gigabyte-plus) far outside the noise of single runs.

## 9. S2: the registry workload, against `lexsys-cache` and Redis

`scripts/ws_presence.py` runs what the registry in §6 needs against any RESP server (10,000 `SET cp:<id> <pod> EX 90`,
then 333 refreshes a second for 20 s with a `GET` every tenth, one request in flight, then the two commands
`lex-csms` uses). Redis 7.0.15 (not pinned to a core) and `lexsys-cache` (pinned to core 0) on the same machine, one run each:

| | Redis 7.0.15 | `lexsys-cache` |
|---|---|---|
| register 10,000 keys with a TTL | 1.57 s | 1.39 s |
| refresh/lookup latency p50 / p99 / max | 0.34 / 0.86 / 8.6 ms | 0.33 / 0.79 / 9.8 ms |
| server CPU at 333 refreshes a second | 3.2% of a core | 2.4% of a core |
| resident memory | **14 MB** | **146 MB** (a fixed arena and index sized at start: the defaults are 64 MiB and 1,000,000 keys; `cache <port> <MiB> <keys>` sizes them, not tried) |
| `PUBLISH` | `0` (no subscribers) | `unknown command 'PUBLISH'` |
| `PSUBSCRIBE` | enters subscribe mode | `unknown command 'PSUBSCRIBE'` |

For a **presence registry, `lexsys-cache` is a drop-in**: same latency, same work, with the TTL doing the liveness. It is **not** a
drop-in for the publish/subscribe path, which it cannot answer at all. Measured with a Python client; the lex-sys side
of the registry (a RESP client) is not built, so this shows the *server* is adequate, not that a lex-sys charge-point service
could use it yet.

## 10. What lex-sys lacks to write the equivalent service

Found by writing the spike (**found**), or read from the service and not tried (**inferred**). Where it would live in brackets.

| # | gap | how found | where |
|---|---|---|---|
| 1 | no SHA-1 and no base64 in `std` (the WebSocket accept key needs both); `examples/ocpp_ws/{sha1,b64}.ls` carry them, differentially tested | found | `std.crypto` (SHA-1 is also what Git and many protocols want) |
| 2 | no ISO 8601 / calendar from a Unix time (OCPP's `currentTime`); `timefmt.ls` | found | `std.time` |
| 3 | no WebSocket codec: handshake, framing, masking, close codes; `ws.ls` is single-purpose (no fragmentation, no extensions, text only) | found | a `packages/ws`, sans-io like `http-server` |
| 4 | a region chunk is 64 KiB: one larger `alloc_slice` traps, so the per-connection slabs need `box_slice` and nested borrows (`server.ls`) | found | a documented limit; a `box_slice`-backed slab helper |
| 5 | no RESP client (the registry needs `SET EX`/`GET`/`DEL`) | found | `packages/resp`, beside `lexsys-pg` |
| 6 | `lexsys-cache` has no `PUBLISH`/`PSUBSCRIBE` | found | `lexsys-cache`, only if a bus is wanted |
| 7 | no TLS **server** (a charge point connects over `wss` in production; lex-sys has a TLS client through `Ffi("libc")`/OpenSSL) | inferred | a terminating proxy, or a TLS server (`opaque-pointers.md` covers the client) |
| 8 | `lex-csms` stores in SQLite; lex-sys has Postgres (`lexsys-pg`) and no SQLite | inferred | port to Postgres, or a foreign-linked SQLite |
| 9 | the HTTP API that sends commands **down** a socket must live in the same loop as the connections (one thread, one poller): `std.conns` has the table and `http-server` has the loop, but a server that runs both on one poller is untried | inferred | an example first; the design is `http-server.md` plus this one |
| 10 | single thread: 10,000 idle is free, but CPU-bound work (JSON of a large `MeterValues`, authorisation) shares the one core; the answer is the threads strategy doc, not measured here | inferred | [`threads.md`](threads.md) |
| 11 | small friction while writing it: `Split` has seven fields and every `main` must name them; `text`, `live` and `res` are not usable as local names; `http.version` answers 10 or 11, not a string | found | docs and diagnostics, not features |
| 12 | OCPP itself: only four actions, no schemas; the 1.6, 2.0.1 and 2.1 message sets and their validation are `lex-ocpp`'s | inferred | a lex-sys port of `lex-ocpp`, the largest piece by far |

Reading the table: 1 to 6 are what a **connection layer** needed, and are small; 7 to 12 are what the **service** needs and are
where the work is. The spike supports "lex-sys can hold the sockets"; it does not support "lex-sys can replace `lex-csms`
soon". The sensible first step, if one is wanted, is the *connection layer only* behind `lex-csms` (a proxy that terminates
WebSocket and forwards OCPP frames), which needs 1 to 4 and none of 7 to 12.
