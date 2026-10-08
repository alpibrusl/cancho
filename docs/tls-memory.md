# TLS memory per connection

> Issue #383, part of #378 (TLS performance: closing the gap with OpenSSL). Status: **design, measured**. This document, its
> measuring script `scripts/tls_memory.py` and OpenSSL's server `benches/server/ossl_hold.c` are the first commit; §10 is what the second
> built. Extends `docs/tls-pure.md` §7.4 and §10 question 6, `docs/tls-core.md` §9.1 and `docs/tls-server.md` §6 and §11.

## 1. The claim, and what measuring it found

An established connection of `examples/tls_echo` was said to cost about 120 KiB, against 26 to 48 KiB for OpenSSL and 0.34 KiB for a
plain connection. A broker with 100,000 clients is the case this hurts: 12 GB of connections before it holds a message. This document
measures the claim (§2, §4), lists what a slot holds (§3), says what the earlier documents claimed and what the code does (§5), weighs
six ways out (§6) and chooses one (§7).

1. **The figure is right, and it is not the buffers.** An established server connection is **119 KiB** resident in `tls_echo` (arm64
   and x86-64 agree to 2.4 KiB) in a slot that is **263 KiB** in the engine's own accounting. Memory is resident only where it was
   written: a `box_slice` with a zero fill is `calloc` (`docs/zeroed-slices.md`), and a page nobody writes is not in the RSS. So the
   64 KiB handshake buffer costs one or two pages, not sixteen.
2. **Where the 119 KiB is.** About **80 KiB** is the slot's *integer* slice: `std.ecdh`'s work area (9,593 words, 75 KiB), which the
   server's signature and every key exchange use, and the two transcript states (2.8 KiB). Every handshake writes it, so it is resident
   for good. `docs/tls-core.md` §9.1 counted 158 words for that slice; it is **10,241** since the work area was moved into it (a region
   holds at most 64 KiB, `docs/tls-parity.md` §3.3.1). About **25 KiB** is one to three pages written in each of the byte
   buffers. The example's own three buffers a slot add about **14 KiB**. (The engines alone, a client slot and a server slot in one process
   with no example, are 211 KiB for the pair, §4: 105 a slot.)
3. **Nothing is ever released.** `docs/tls-pure.md` §7.4 says the handshake reassembly buffer is "released into the slot's pool after
   `Finished`". There is no pool and no release: the buffer is compacted and never freed or cleared.
4. **A record in flight costs 90 KiB more, for good.** The first 16 KiB record on a connection writes four pages of each of the input,
   plaintext, received-data and output buffers, and the example's three. They are not given back when the record is gone.
5. **`start` writes the whole integer slice** (80 KiB) when a connection begins, before a byte has arrived: `connected` is 85 KiB.

OpenSSL on the same machines, same certificate, a server that only accepts, handshakes and echoes (§2). An established idle connection at
10,000 connections: **13.6 KiB** with `SSL_MODE_RELEASE_BUFFERS` and **24.2 KiB** without on 3.0.13 (arm64); **27.8** and **49.4 KiB** on
3.5.5 (x86-64); `tls_echo` is **119.4** and **121.8**. In the middle of a handshake it is 42 to 54 KiB (arm64) and 56 to 77 (x86-64). The "26
to 48" of `tls-nonblocking.md` §8.3 was a client through memory BIOs, and agrees with 3.0.13's range.

**The choice (§7).** Keep what an established connection needs in a small **core** per slot, and lease the rest, the handshake's
scratch and the record buffers, from a pool of **work areas** for as long as a handshake runs or bytes are in flight. The engine's API
does not change and neither does what it accepts and refuses. A prototype measured **17 KiB** for an established idle connection of
`tls_echo` (from 119) and **54 KiB** after a 16 KiB record on each (from 211), on arm64, with the example's own buffers included.

## 2. How it was measured

`scripts/tls_memory.py` starts a server per count N and opens N connections from Python's `ssl` (memory BIOs, so a handshake can
stop before the client's Finished), and reads the server's `VmRSS` from `/proc/<pid>/status` after each phase. The figure for a phase is
(RSS in the phase - RSS at the server's start) / N. The first server is for the phases in which the handshakes are not finished; the
second is for established connections, handshaken one at a time, as a program that bounds its handshakes in progress makes them:

| phase | what the N connections are doing | server |
|---|---|---|
| connected | TCP connected, nothing sent: the slot is started and waits for a ClientHello | first |
| handshake | ClientHello sent, the server's whole flight read, the client's Finished held back: all N in progress at once | first |
| established | Finished sent, one byte echoed, then idle | second |
| echoed | 16,384 bytes sent and read back on each connection in turn, then idle again: what traffic leaves resident | second |
| partial | 12,000 bytes of a 16,384-byte record sent on every connection at once, the rest held back | second |
| all-echoed | the rest sent and read back on all N, idle again: the high-water mark | second |

N is 50, 100, 200 and 10,000. The server is `examples/tls_echo` built from `origin/main` (`7bf3628`); OpenSSL's is
`benches/server/ossl_hold.c`: one thread, one epoll, accept, handshake, echo, no tickets, `TCP_NODELAY`, with and without
`SSL_MODE_RELEASE_BUFFERS`. The certificate is the committed test identity (`tests/vectors/tls/echo`, ECDSA P-256) for both. Kernel
socket memory is not in the RSS and is the same for both. *One change to the example, for the 10,000:* `tls_echo` refused
`--connections` over 4,096, because a queue entry packs `id * 4096 + slot` into one word; `front.cho` has `slot_base()` = 1,048,576
and the options take that much.

Machines. **arm64:** this Mac's Docker, native Linux 6.8 (not emulated), image `lexsys-hooks-env`, OpenSSL 3.0.13, 4 KiB pages.
**x86-64:** `ssh gram`, Linux 7.0, OpenSSL 3.5.5, 4 KiB pages, the same sources, on cores 6 and 7 with the host's other work on its other
cores. Nothing here is a timing, and gram was not quiet.

## 3. What a slot holds

A slot is two slices of the engine's boxes (`tls_slot`): 10,241 words of integers and 187,191 bytes, **269,119 bytes (262.8 KiB)**, the
same for a client and a server. The sizes are printed by a program that imports `tls_slot` (`ints_len`, `bytes_len`, the offsets).

| what | where | bytes | needed | could be released |
|---|---|---|---|---|
| incoming record, header included | `b_in` | 16,645 | whenever a record is partly read | between records |
| handshake reassembly | `b_hs` | 65,536 | a message in pieces: a server's ClientHello (limit 16 KiB), a client's Certificate (limit 64 KiB); after the handshake, a NewSessionTicket or KeyUpdate in pieces | when empty |
| output queue | `b_out` | 49,218 | up to three full records for `take` | when `take` has emptied it |
| opened record | `b_plain` | 16,640 | while a record is opened and copied out | between calls |
| received data | `b_recv` | 16,640 | from `feed` to `recv` | when `recv` has emptied it |
| the server's leaf (a client's) | `b_leaf` | 16,384 | `Certificate` to `CertificateVerify` | after `CertificateVerify` |
| keys, IVs, secrets, hardware key schedules | `b_keys` | 1,728 | the whole connection | at its end |
| ticket offered, ticket received, its PSK, host | `b_offer` .. | 4,400 | a client's resumption; a server's ALPN and SNI are kept here | a server's: never needed |
| `std.ecdh`'s work area | `i_ecdh_work` | 9,593 words | one key exchange or signature | between them: it is scratch |
| the transcript, SHA-256 and SHA-384 states | `i_transcript` | 356 words | until `Finished` | after the handshake |
| both directions' prepared AEAD keys | `i_read_aead` | 258 words | the whole connection | at its end |
| the rest of the integers: state, sequence numbers, fills, flags, ticket numbers | | 34 words | the whole connection | at its end |

What an established connection must keep, by that table: the keys (1,728 B), the AEAD contexts and the rest of the integers (2,336 B)
and, for a server, its ALPN and SNI (512 B), for a client the ticket state (4,400 B): **4,576 B (4.5 KiB) for a server and 8,464 B
(8.3 KiB) for a client**. The other 258 KiB are for a handshake, or for bytes in flight.

## 4. Measured

KiB of RSS per connection (§2). **The server slot, `examples/tls_echo` as it was:**

*arm64 (Docker on this Mac, Linux 6.8): `examples/tls_echo`, as it was*

| connections | connected | handshake | established | echoed | partial | all-echoed |
|---|---|---|---|---|---|---|
| 50 | 85.0 | 107.0 | 119.0 | 210.7 | 210.7 | 210.7 |
| 100 | 85.1 | 107.2 | 119.2 | 210.8 | 210.8 | 210.8 |
| 200 | 85.0 | 107.2 | 119.2 | 210.8 | 210.8 | 210.8 |
| 10,000 | 85.2 | 107.4 | 119.4 | 210.9 | 210.9 | 210.9 |

*x86-64 (`gram`, Linux 7.0): `examples/tls_echo`, as it was*

| connections | connected | handshake | established | echoed | partial | all-echoed |
|---|---|---|---|---|---|---|
| 50 | 85.0 | 109.4 | 121.4 | 211.9 | 211.9 | 211.9 |
| 100 | 85.1 | 109.6 | 121.6 | 212.0 | 212.0 | 212.0 |
| 200 | 85.0 | 109.5 | 121.6 | 212.0 | 212.0 | 212.0 |
| 10,000 | 85.2 | 109.8 | 121.8 | 212.1 | 212.1 | 212.1 |

**OpenSSL's server**, default and with `SSL_MODE_RELEASE_BUFFERS`:

*arm64, OpenSSL 3.0.13, default*

| connections | connected | handshake | established | echoed | partial | all-echoed |
|---|---|---|---|---|---|---|
| 50 | 9.8 | 54.2 | 28.9 | 51.1 | 51.1 | 51.1 |
| 100 | 9.2 | 52.4 | 26.5 | 48.6 | 48.6 | 48.6 |
| 200 | 8.9 | 51.4 | 25.3 | 47.4 | 47.4 | 47.4 |
| 10,000 | 8.6 | 50.4 | 24.2 | 46.2 | 46.2 | 46.2 |

*arm64, OpenSSL 3.0.13, `SSL_MODE_RELEASE_BUFFERS`*

| connections | connected | handshake | established | echoed | partial | all-echoed |
|---|---|---|---|---|---|---|
| 50 | 9.8 | 46.2 | 18.8 | 19.1 | 34.0 | 20.9 |
| 100 | 9.2 | 44.1 | 16.2 | 16.3 | 31.7 | 17.2 |
| 200 | 8.9 | 43.0 | 14.9 | 15.0 | 30.5 | 15.4 |
| 10,000 | 8.6 | 41.9 | 13.6 | 13.7 | 29.4 | 13.7 |

*x86-64, OpenSSL 3.5.5, default*

| connections | connected | handshake | established | echoed | partial | all-echoed |
|---|---|---|---|---|---|---|
| 50 | 21.1 | 77.1 | 63.7 | 73.9 | 73.9 | 73.9 |
| 100 | 18.0 | 69.8 | 56.1 | 66.8 | 66.8 | 66.8 |
| 200 | 17.4 | 66.7 | 53.2 | 64.0 | 64.0 | 64.0 |
| 10,000 | 16.2 | 63.2 | 49.4 | 60.4 | 60.4 | 60.4 |

*x86-64, OpenSSL 3.5.5, `SSL_MODE_RELEASE_BUFFERS`*

| connections | connected | handshake | established | echoed | partial | all-echoed |
|---|---|---|---|---|---|---|
| 50 | 21.1 | 67.6 | 40.1 | 40.4 | 55.7 | 42.8 |
| 100 | 18.0 | 62.8 | 35.4 | 35.6 | 51.0 | 36.7 |
| 200 | 17.1 | 58.6 | 31.3 | 31.4 | 47.0 | 32.0 |
| 10,000 | 16.2 | 55.9 | 27.8 | 27.8 | 43.5 | 27.8 |

`echoed`, `partial` and `all-echoed` are one figure for `tls_echo` because its buffers are per slot and written to their ends by one record;
for OpenSSL `partial` is a connection holding a record (30 to 34 KiB with the buffers released, nothing more without).

**The client slot** (`tests/programs/tls_many.cho`, `conc` connections to an echo server, each having sent and been sent back a 16 KiB
request and now waiting for the close; the client's RSS over its RSS with one connection, arm64): 185.1 KiB a connection at 50 connections, 183.7 at 100 and 184.5 at 200. It cannot be held idle, so
this is the `echoed` figure, and it includes the program's own 16 KiB a connection.

**The engines alone**, without a socket or an example (`tests/programs/tls_pool.cho` of the second commit, built against the engine as it was
with `open_with_areas` and `open_server_with_areas` replaced by `open_with_tickets` and `open_server`, and `scripts/tls_memory.py engine`): a client
slot and a server slot together, KiB of RSS over the start, for the pair. 

*arm64:*

| connections | areas | established | echoed |
|---|---|---|---|
| 50 | 50 | 210.8 | 325.8 |
| 100 | 100 | 211.3 | 326.2 |
| 200 | 200 | 211.2 | 326.1 |
| 10,000 | 10,000 | 211.4 | 326.3 |

*x86-64:*

| connections | areas | established | echoed |
|---|---|---|---|
| 50 | 50 | 210.8 | 325.8 |
| 100 | 100 | 211.3 | 326.2 |
| 200 | 200 | 211.2 | 326.1 |
| 10,000 | 10,000 | 211.4 | 326.3 |


What they say. `connected` (85 KiB) is `start` writing every word of the integer slice; with `if ints[k] != 0` before the store it is
9 KiB, and the later phases do not move, because the handshake writes those pages anyway (a probe, not a change: connected 9.0, handshake
107.0, established 119.0 on arm64). `handshake` to `established` is 12 KiB: the Finished, the first echo. A record more than doubles the
figure because the engine's four buffers and the example's three are written to their ends by it. A client slot and a server slot are the
same layout, so a pair is twice a slot: 105 KiB each, and the 14 KiB more of `tls_echo`'s server slot is the example's.

## 5. What the earlier documents said

| claim | where | what happens |
|---|---|---|
| the reassembly buffer "is released into the slot's pool after `Finished`" | `tls-pure.md` §7.4 | there is no pool; `compact` moves the unread bytes to the front and nothing is freed or cleared. Its resident part is one or two pages, written by the ClientHello, and stays |
| a slot is "about 179 KiB, or 11.2 MiB for 64"; "181,927 bytes and 158 words" | `tls-pure.md` §7.4, `tls-core.md` §9.1 | the slot is 187,191 bytes and **10,241 words**, 262.8 KiB: the 75 KiB of `std.ecdh`'s work arrived with HelloRetryRequest groups and the server's signing, and the figure was not revisited. 64 slots are 16.4 MiB, 100,000 are 25 GiB of address space |
| "a server slot is the client slot plus nothing significant" | `tls-server.md` §6 | true of the layout; a server never uses the client's 4.4 KiB of ticket state or its 16 KiB leaf buffer, and has them |
| "Peak RSS of `tls_many` is 31.1 MiB with 1 connection and with 64" | `tls-core.md` §10.2 | on this tree the client is 2.1 MiB with one connection, 11.2 with 50 and 38.9 with 200 |

These are corrected in place by the change that builds this (§10), not by this commit.

## 6. The ways out

**A. Stop `start` writing zeros** (read, and store only a nonzero). Free, and it takes `connected` from 85 to 9 KiB. It moves no other
phase: the signature writes the work area anyway. Done with the build, because it falls out of E; alone it is not the answer.

**B. A smaller ceiling for the buffers.** The buffers' *sizes* are not the resident figure (§1): a page nobody writes costs nothing, and a
connection that only sees small records writes one page of each already. A smaller ceiling saves only for connections that send large
records, and it changes what the engine accepts: RFC 8446 §5.2 obliges a receiver to take ciphertext of 2^14 + 256 bytes, peers send full
records routinely, and `max_fragment_length` and `record_size_limit` are not offered (`docs/tls-parity.md`). Excluded by the gate.

**C. Release the handshake buffers into a shared pool after `Finished`, and take one back for a KeyUpdate or a NewSessionTicket.** This is what
`tls-pure.md` §7.4 says is the design. Alone it is the wrong lever: the reassembly buffer is two pages and the work area is 75 KiB. As part
of E it is what E does, for every buffer.

**D. Share the scratch.** One work area for the engine, since a call does not outlive itself: `std.ecdh`'s work and the transcripts. The
five places that use the work area (`new_ecdh_share`, the client's and the server's `ecdh.shared`, the server's `sign_checked`, the TLS 1.2
client's `ecdh.shared`) take a slice of the slot's integers and would take an argument, through `feed` and what it calls, in files being
edited by four other pieces of work. Estimated, not built: established 119 to about 44 KiB, the buffers' pages still in every slot.

**E. A core and leased work areas** (chosen, §7). The slot's layout is reordered so that what an established connection keeps comes
first and the rest after it. The engine keeps a *core* for every slot and a pool of full-size *work areas*; a connection holds an area
while it is in its handshake or has bytes in a buffer, and not otherwise. The functions of `client.cho` and `server.cho` are unchanged:
they are handed the area, which is a slot, with the core copied in. Costs: two copies a call (a core is 4.5 or 8.3 KiB; the bytes go by
`copy_into`, a `memmove`), the reorder of the layout (§7.1), a pool that can run out when it is smaller than the slots (§7.5), and the
engine functions that read a connection reading the core. Gains: established 119 to **17 KiB**, and a record in flight costs an area for
the length of the call, not a slot for good. *Prototype, arm64, N = 100: connected 90.7, handshake 111.6, established 17.4, echoed 54.0,
partial 79.4, all-echoed 139.4.* (`handshake` and `connected` are all N at once, so N areas; `partial` is N areas in use; `echoed` is
traffic one connection at a time, which touches the few areas that are in use.)

**F. Two kinds of slot** (`tls-pure.md` §10 question 6: "share a pool across slots"). A connection starts in a handshake slot and moves to
an established one at `Finished`. It is E without the lease for traffic: an established connection would need its record buffers all its
life, 66 KiB, and a broker's 100,000 connections are exactly those. E is F with the buffers leased too.

## 7. The design

### 7.1 The layout, precisely (for a rebase)

`packages/tls/slot.cho`. No size changes: `ints_len` and `bytes_len` are the same numbers. Two blocks change order.

*Integers.* The transcripts and the work area move after everything else:

- `i_offer_len()` is `20` (it was `i_ecdh_work() + ecdh.work_len()`), so the resumption fields, the AEAD contexts and the server's five words
  follow the header directly;
- `i_transcript()` is `ints_core_len()` (it was `20`); `i_transcript384` and `i_ecdh_work` follow it as before;
- `ints_core_len()` is the old `ints_len()` body, `i_alpn_len() + 5`: **a field added at the end of the integers is added there, and is in the
  core**; `ints_len()` is `i_ecdh_work() + ecdh.work_len()`.

*Bytes.* The core moves to the front:

- `b_keys()` is `0` (it was `b_leaf() + leaf_cap()`), the keys and ticket state follow it unchanged;
- `bytes_core_len()` is `b_ticket_host() + 256` (the old `bytes_len()`), a client's core; `bytes_core_len_server()` is `b_offer() + 512`: a
  server's room is its ALPN and its SNI, and `b_sni()` is `b_offer() + 256` (it was `b_ticket_host()`, the same 256 bytes, now next to the ALPN);
- `b_in()` is `bytes_core_len()` (it was `0`), and `b_hs`, `b_out`, `b_plain`, `b_recv` and `b_leaf` follow as before; `bytes_len()` is
  `b_leaf() + leaf_cap()`.

*Besides:* `keeps_area(ints)`, false, is where a state that must stay in the buffers after the handshake says so; `client.cho`'s `drop` zeroes
`b_offer()..bytes_core_len()` (it was `..bytes_len()`, which is now the buffers); a refusal `tls-pool` (-72) in `record.cho`.

**What the other work in flight must do.** State kept after the handshake goes in the core: a field after `i_session_len`, or before
`b_ticket_host() + 256` for a client or `b_offer() + 512` for a server. State used only while the handshake runs may live in the buffers and
in the integers after `ints_core_len()`. Two cases are known. Server tickets (#379) draw their randomness into `b_ticket()` at `serve`, which
a server's core does not hold: it is used in the flight that the client's Finished brings and the connection holds its area until then, so it
may stay, and if it is needed after the handshake the server core grows. The engine functions that read a connection (its `ints` slice) read
`cints`, the core, not `ints`, the areas. Client certificates (#384) keep the client's leaf in `b_leaf` for `peer_certificate`: `keeps_area`
answers true for a connection that has one, which keeps its area and costs what it costs today.

### 7.2 The engine

`tls.cho`. The engine's `ints` and `bytes` become the **areas** (`areas` strides of `ints_len` and `bytes_len`) and two boxes are added,
`cints` and `cbytes`: every slot's core. `meta` gains each slot's lease, a stack of free areas and a flag for the handshake's scratch. The
stack is last in, first out, so the areas that are touched are the few in use at once.

- Every call that moves bytes (`start`, `start_with`, `serve`, `feed`, `send`, `recv`, `eof`, `finish`) first **leases**: a slot with no area
  takes the top of the stack and has its core copied in. After the call, **settle** copies the core out, so the core is always current, and
  gives the area back if the connection is idle: established, closed or failed; no partial record, no partial handshake message, nothing to
  `take`, nothing to `recv`; and `keeps_area` false.
- The calls that only read (`event`, `failure`, `alert_received`, `resumed`, `suite`, `group`, `retried`, `server_name`, `alpn`,
  `handshakes_in_progress`, and `save_to`'s reads) read the core, take no area, and cost what they cost. This is why the core is the *front* of
  the slot: they are handed a shorter slice with the same offsets.
- `take` on a connection with no area answers 0 (it has nothing queued: that is what idle means). `drop` clears the area if there is one, and
  always the core.
- A handshake's scratch (the transcripts and the work area, which hold intermediate values of the key exchange and the signature) and the last
  record opened are overwritten when a connection that ran a handshake gives its area back; `close` does the same to every area. The plaintext
  of an established connection stays in a free area until the area is used again, as it stayed in the slot until the connection ended;
  `docs/tls-pure.md` §7.3 says the language guarantees no erasure.

### 7.3 API

Unchanged: `open`, `open_with_tickets`, `open_server`, and every call. Added:

```
tls.open_with_areas(heap, slots, tickets, areas) -> Engine    // a client engine whose slots share `areas` work areas
tls.open_server_with_areas(heap, slots, areas) -> Engine
tls.areas(engine) -> int                                      // how many
tls.areas_free(engine) -> int                                 // how many are not leased now
```

`areas` is at least 1 and at most `slots`, and the three existing opens use `slots`, so **with them no call is refused for want of an area** and
the engine accepts and refuses what it did. With fewer: `start`, `start_with` and `serve` answer `tls-pool` and start nothing (the slot stays
free; the program leaves the connection queued, as it does for a handshake bound); `send`, `recv`, `eof` and `finish` answer `tls-pool` and change
nothing; and `feed` on an established connection that needs an area and finds none **fails that connection** `tls-pool`, its keys cleared and no
alert sent (there is no buffer to seal one in), as a connection fails for any refusal.

### 7.4 What does not change

`client.cho`, `server.cho`, `client12.cho`, `hello.cho`, `message.cho` and `record.cho` (but for one refusal and one range in `drop`) are not
touched: they see an area, which is a slot. The record layer, the key schedule, the transcript, the signatures and every refusal are the same
code on the same bytes. `tests/programs/tls_driver.cho`, the fuzz harnesses and the lying peers build their own slots from
`tls_client.ints_len()` and `bytes_len()` and are not affected by the engine's.

### 7.5 Sizing the pool, and what it costs a peer to take one

With `areas` = `slots` (the default) nothing can be starved and the address space is what it was: 263 KiB a slot, 25 GiB for 100,000 (a
`calloc` the kernel may refuse on a small machine, the other reason to size the pool). The memory is what is touched: the areas in use at
the same time. A smaller pool is sized for the connections that can be in a handshake or hold a record at one instant. A program that bounds
its handshakes in progress has that bound for the first; for the second, a peer can hold an area by sending half a record and stopping,
which in the slot design held that slot's memory and in the pool design holds one area of the pool. **A pool smaller than the connections is a
resource a peer can exhaust**, with 100,000 connections each stalled mid-record. `examples/tls_echo` and `https_hello` get `--areas`
(default `--connections`), keep handshakes to half the areas, and rely on `--idle` to close a connection that stalls.

### 7.6 Expected, and the risk

An established idle connection: its core, 4.5 KiB (server) or 8.3 KiB (client), plus the example's pages. A record in flight: an area, 263 KiB of
address space and about 70 KiB touched, for the length of the call. The risk is a field read from a slice that is the core and that sits in the
buffers: a trap, because the slice is shorter. The reorder puts every field a read-only call needs in the first 20 words or before
`ints_core_len()`, and the tests of §8 call each read after every kind of traffic with the areas taken and given back in every order.

## 8. Gates of the second commit

`cargo fmt`, `cargo clippy`, `cargo test --workspace`; the TLS suites unchanged (the lying server and client, the differential against `openssl
s_server` and `s_client`, tickets, the interop matrix, the fuzz harnesses, the mutants); new cases for an engine with fewer areas than slots
(`tls-pool`, a parked connection fed when none is free, data waiting while another connection is served, every order of lease and release, a
connection that ends without an area); mutants for the lease, the copies and the idle test; `scripts/tls_echo_test.py areas` and
`scripts/https_hello_test.py areas`; the measurements of §4 repeated and written here; the corrections of §5 made in place; a comment on #378.

## 9. Open

1. **The example's own buffers.** `pend`, `inq` and `app` are 48 KiB a slot in `tls_echo` and `https_hello`, each written to its end by the first
   full record. They are the example's, not the engine's; leasing them the way the engine does would take an echoed connection from 54 KiB to
   about 20. It is the same pattern in code that four other pieces of work are editing.
2. **The core of a server is 4.5 KiB because its ticket room is not in it.** If #379 needs room after the handshake the core grows by that.
3. **TLS 1.2 (`client12.cho`)** holds the same state in the same places and uses the same engine; it is covered by the traces and by
   `tls_many` against OpenSSL's TLS 1.2 servers, not by a case of its own in the pool test.
