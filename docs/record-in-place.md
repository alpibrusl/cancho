# A kept connection without copies: work areas that rest, and a record layer that seals in place

> **Status: built (#447 and #448, part of #378).** Levers L1 and R1 of [`tls-performance.md`](tls-performance.md) §10.8.
> #395 ([`tls-memory.md`](tls-memory.md) §7) gave each TLS slot a small core and leased a work area while a connection
> needed one, copying the core in on the lease and out on the release; on a kept `https_hello` request those copies and the
> clearing were a quarter of the server's CPU (§1). #392 ([`gcm-wide.md`](gcm-wide.md) §9) left the record layer costing more
> than the cipher for a small record: a `region` and a byte-at-a-time copy around every seal and open. This document measures
> what is left of both on today's tree (§1), designs each (§2 to §5), states the gates before the numbers (§6), and gives
> what was built and measured (§7) and what was not (§8). A claim found false later is corrected here, in place.

---

## 1. What a kept request spends today

Measured on `origin/main` at `3d7e8e9d` (#457 merged), on `gram` (i7-1260P, Linux 7.0, OpenSSL 3.5.5; the server on CPU 6
and the load on CPU 7, its SMT sibling, nothing else of this work on the machine): `https_hello` under `benches/server/tload.c`
(32 keep-alive connections, `GET /hello/42`), `perf record -F 1999 --call-graph dwarf` of the server for 6 s, and `perf stat`
of the server's user and kernel cycles over the load, divided by the requests the load counted.

| | x86-64, share of a request (inclusive) |
|---|---|
| `tls.settle` (the release: `tls.core_out` 8.4, `tls.clear_area` 5.6) | 15.5% |
| `tls.lease` (the core copied in) | 8.5% |
| **the two** | **24.0%** (§10.5 of `tls-performance.md`: 23.6) |
| of which `memmove` and `memset` | 2.8% |

**The copies were not the bytes; they were the loops.** A core is 296 words and 4,864 bytes (a server's). The bytes went by
`copy_into` (a `memmove`) and `tls_slot.zero` (a loop LLVM makes a `memset`); the words went one at a time through
`contents(engine.ints)[ai + k] = contents(engine.cints)[ci + k]`, which reloads both boxes and checks both bounds for every
word, because the store may alias the boxes' own fields. That loop is the 7.6% self time of `tls.lease` and the 7.9% of
`tls.core_out`, and the same shape is most of `tls.clear_area`'s 4.5%.

**A request takes and gives back an area twice.** `https_hello`'s loop (`examples/https_hello/loop.cho`) reads every ready
socket and feeds the engine (`feed` leases: the request's record is in the area), then receives the request (`recv`: the
connection is idle, the area goes back), runs the HTTP server over every connection that has input, and only then sends each
answer (`send` leases again) and writes it (`take`: idle, the area goes back). Between one connection's `recv` and its `send`
the loop serves the other connections that were ready, so the calls on one connection are not consecutive.

**The record layer.** `tls_record.seal` opened a `region`, copied the content into it a byte at a time with the content type
after it, made the nonce in a second slice of it, and sealed from there into the output buffer; `open` opened into `b_plain`,
and the caller (`on_record` in `client.cho` and `server.cho`) copied the content a byte at a time into a second `region`, and
from there, a byte at a time again, to the received data. `tls_slot.copy_bytes`, which carries a socket's bytes into the input
buffer, was a byte loop too. `tls-performance.md` §10.5 measured the layer at 45% of a 64-byte record's seal and 6 to 11% of a
16 KiB one, and that figure is `tls_record.seal` alone; the two copies on the way in were outside it.

## 2. Work areas: the copies in bulk, and a connection that rests in its area

### 2.1 The copies in bulk

`tls_slot.copy_words(into, from)` checks the length once, takes `into[0..n]`, and copies in a loop whose index is provably in
both bounds, which LLVM turns into a vector copy (`ldp`/`stp` of 32 bytes on arm64, `vmovups` on x86-64; read in the object
code); `tls_slot.zero_words` is the same loop storing 0, which becomes `bzero`. `lease`, `core_out`, `clear_area` and `drop`
use them, and `copy_bytes` becomes `copy_into`. Nothing else changes: the same words and bytes are copied and cleared.

### 2.2 Resting

**The idea.** An established connection that goes idle in its area keeps the area, its core in it, and is said to *rest* there.
A later call on it finds its core where it left it and copies nothing. The area is taken from it, its core copied out first,
only when another connection needs an area and none is free (the area passes to that connection, §2.4), or when more connections
rest than are allowed (§2.3), the one that rested longest first. On a server whose busy connections fit the allowance, a kept
connection's core never moves: the request's `feed` finds it resting, `recv` leaves it resting, `send` and `take` find it again.

**What a resting connection holds, and what it does not.** When a connection comes to rest, everything a release would clear
other than its core is cleared at once (`clear_plain`: the received data as far as it reached, the client's leaf on a server,
`b_plain` on a TLS 1.2 connection). So the area holds the connection's current core and nothing else; the ints past the core
(the transcripts and `std.ecdh`'s work) were cleared when its handshake ended, because only a connection whose area holds no
handshake scratch may rest (`m_dirty` 0), and only an established one (`state_connected`): a closed or failed connection gives
its area back as before.

**The invariants, and what checks each.**

| invariant | how it holds | checked by |
|---|---|---|
| `tls.residue` is 0 after every step | a free area is cleared as before; a resting area's words past the core, its bytes past the core, `b_plain`, `b_recv` and `b_leaf` must be zeros, and `residue` counts them | `tls_pool.cho`, after every step, drop and failure, every configuration |
| a released area is cleared | `give_back` is the old release: core out, `clear_area`, back on the stack; an area that passes from a resting connection to another has its core overwritten whole by the new one's (the words and bytes `clear_area` would clear), and the rest of it was cleared when the first came to rest | the mutants "a resting connection's area passed on without its core taken out", "a connection comes to rest with its plaintext" |
| no stale core read | a resting connection's current core is the area's (`in_area` is true: it holds the area); every read (`word`, `core_now`, `event`, `recv`'s answer) reads it there; it goes back to the slot's core box, by `core_out`, in the same step that takes the area | the mutants "...without its core taken out", "a resting connection called again left in its rest" |
| a resting connection needs no area | it is idle: nothing queued for the socket, nothing for `recv`, no record or handshake message half read; every call that could change that (`feed`, `send`, `eof`, `finish`, `start`, `serve`) leases first, and a lease takes it out of its rest; `take` and `recv` on it answer from its core and leave it resting | `holds_area` false and `areas_free` counting it (§2.4) |

### 2.3 How many rest

The memory #395 saved is what idle connections do not touch: they share a few areas, last in first out. A connection resting in
its own area keeps that area's written pages in use, so resting trades memory for copies, and the trade is bounded:
**`rest_default()`, 32, connections rest at most** (or the number of areas, if fewer), and
`tls.set_resting(engine, k)` sets it from 0 (none rests: the engine of #395) to the number of areas. 32 is the number of
connections `https_hello`'s cost run keeps busy; a loop that serves more connections between one connection's `recv` and its
`send` evicts each before it is called again and gains nothing from resting, and loses nothing either (§2.4). The bound on
memory is 32 areas' written pages per engine whatever the number of connections, measured in §7.4.

### 2.4 The area pool with resting connections

- A lease takes a **free** area first (the top of the stack, as before), and passes on the longest-resting connection's area
  only when none is free. So resting never refuses a lease that #395 would have granted: `areas_free` counts resting areas, and
  `serve` and `start` answer `tls-pool` only when no area is free or resting.
- `holds_area(slot)` is false for a resting connection: the programs that end a connection holding an area too long
  (`tls_echo`'s `record-timeout`) do not see an idle connection as one.
- `tls.areas_resting(engine)` says how many rest, for tests.
- The order is a doubly linked list through `meta`: for each slot the next and the one before it, plus 1 while it rests, and
  the first, the last, the count and the allowance: `11 + 8 * slots + areas` words of `meta` (it was `7 + 5 * slots + areas`).
  Resting, being taken out of a rest and being evicted are constant time.
- `drop` of a resting connection takes it out of the order and clears its area as for any connection holding one.

## 3. An AEAD in place

`std.gcm` and `std.chacha20` get the same two functions over one buffer that holds the text and, after it, the 16 bytes of the
tag:

```
gcm.seal_in_place(ctx, hw, nonce, aad, buf) -> int        // buf: plaintext, then 16 bytes; after: ciphertext, then the tag
gcm.open_in_place(ctx, hw, nonce, aad, buf) -> int        // buf: ciphertext, then the tag; after `ok()`: the plaintext
gcm.seal_in_place_software / open_in_place_software       // the software path whatever the CPU, as seal_software
chacha20.seal_in_place(key, nonce, aad, buf) -> int
chacha20.open_in_place(key, nonce, aad, buf) -> int
```

The answers and refusals are `seal_with`'s and `open_with`'s (`chacha20.seal`'s and `open`'s), in their order: an unprepared key,
the nonce's length, a text over the limit; a buffer with no room for a tag is `refused_output_length` to seal (as `seal_with`'s
`out` would be) and `refused_too_short` to open. **On a tag mismatch nothing is written**: the tag is computed over the
ciphertext where it is and compared over all 16 bytes before the cipher runs, so the buffer holds the sealed message it held.

### 3.1 The contract the types do not state

The cipher is handed the same slice as its input and its output: `aes_ctr32(keys, nr, nonce, 2, buf[0..text], buf[0..text])`,
`aes.ctr32_with(..., buf[0..text], buf[0..text], q)`, `chacha20.xor(key, 1, nonce, buf[0..text], buf[0..text])`. The checker
accepts it (a slice may be lent as `&` and `&!` in one call; nothing in the language forbids aliasing), and it is correct
because each of the three reads the input byte at an offset before it writes the output byte at the same offset and never reads
an offset after writing it:

- `@cancho_aes_ctr32` (`crates/cancho-codegen-llvm/src/crypto/aes.rs`) loads each 16-byte block, XORs the keystream into it
  and stores it, block after block; the tail loads each block or byte before storing it; its pointer arguments carry no
  `noalias`, so LLVM keeps every load before the store that may alias it.
- `aes.ctr32_with` and `chacha20.xor` are cancho: `output[at + k] = byte_of(int_of(input[at + k]) ^ ...)`, in order.

This is a contract of those three functions, not of the types, which is why the in-place forms live in `std.gcm` and
`std.chacha20` beside them and callers use those, not the builtin with one slice twice. A change that breaks it (a builtin
that reads ahead into a buffer it has started to write, `noalias` on its pointers) is caught by the in-place cases of
`scripts/gcm_differential.py` and `scripts/aead_differential.py`, which seal and open every length from 0 to 600 bytes and the
lengths around 4 and 16 KiB in place and compare with OpenSSL.

The tag is written to, or read from, `buf[text..text + 16]`, which no cipher call touches; the associated data is a separate
slice (in TLS the record header, `out[0..5]`, beside the buffer).

### 3.2 Constant time, and the tag compare

Nothing new branches or indexes on a secret. The in-place functions run the same builtins and the same cancho as `seal_with`
and `open_with` on the same lengths; the only data-dependent branch is still the one on whether the tag matched, which is public
(whether the record authenticated). The tag compare is unchanged: `gcm_tag_diff` on the hardware path, and on the software path
and in ChaCha20-Poly1305 the same loop over all 16 bytes OR-ing the XORs. dudect (§6) times the in-place functions directly.

## 4. The record layer

### 4.1 Sealing

`tls_record.seal(suite, key, ctx, hw, iv, seq, content_type, plaintext, out)` writes the header, copies the content once to where
it is sent from (`copy_into(out[5..5 + n], plaintext)`: one `memmove`), writes the content type after it, and seals
`out[5..5 + n + 17]` in place with `out[0..5]` as the associated data. **No region:** the nonce (RFC 8446 §5.3, the IV XORed
with the sequence number padded on the left) is made in the IV itself (`xor_seq(iv, seq)`), the AEAD runs, and the same XOR
undoes it, so `iv` is now borrowed mutably and given back as it was. The XOR is on the bytes, with no branch. A refusal from the
AEAD cannot leave it changed: the second XOR runs whatever the AEAD answered.

### 4.2 Opening

`tls_record.open_into(suite, key, ctx, hw, iv, seq, record, out)` opens a record into `out` and answers `size * 256 + type`
(`opened_size`, `opened_type`) or a refusal; `tls_record.open_in_place(suite, key, ctx, hw, iv, seq, record)` opens it where it is.
The engine calls neither directly: `tls_slot.open_record(ints, bytes, n)` opens the record of `n` bytes in `b_in`:

- **straight into the received data** when its inner plaintext fits the room after `i_recv_end` (always, but for padding past
  the 16 KiB `feed` keeps free): the AEAD writes the plaintext to `b_recv + e`, so application data is where `recv` reads it
  with no copy at all. The content type byte after the content is cleared; the padding after it is zeros by definition (the
  content type is the last non-zero byte).
- **in place in the input buffer** otherwise: `open_in_place`, the content moved to `b_recv + e` (`copy_within`), and the input
  buffer's plaintext cleared.
- On a refusal nothing of the plaintext is left: a record that does not authenticate writes nothing; one that opens but is
  refused (content over the limit) is cleared where it was opened.

The caller (`on_record`, in `client.cho` and `server.cho`) adds application data to the received data, or, for a handshake
message or an alert, copies the content into a region (it is handed to code that also writes the slot's buffers) and clears it
from the received data. TLS 1.2 (`client12.cho`) opens into `b_plain` as before (`open_record12`), and its content is moved to
the same place, so both are taken from one.

**What this changes in the slot.** `b_plain` is no longer written for a TLS 1.3 record. A release clears it only on a TLS 1.2
connection (`clear_plain` reads `f_tls12` from the area's core first). Its pages are not touched by a TLS 1.3 connection.

### 4.3 What does not change

The bytes on the wire (the RFC 8448 traces, the lying server's and client's recordings and the differential replay byte for
byte), every refusal and its tag, the order of the checks in `open` (length, sequence, type, then the AEAD), and the slot's
layout: no offset moved, `ints_len()` and `bytes_len()` are what they were, and so are `ints_core_len()` and the cores.

## 5. Files

`std/gcm.cho`, `std/chacha20.cho` (the in-place forms); `packages/tls/record.cho` (`seal`, `open_into`, `open_in_place`,
edition 5 for `copy_into`); `packages/tls/slot.cho` (`open_record`, `open_record12`, `copy_words`, `zero_words`, and the
slice-level pieces of a release moved from `tls.cho`: `idle`, `clear_area`, `clear_plain`, `clear_leaf`, `plain_left`,
`nonzero`, `take_opened`; edition 5); `client.cho`, `server.cho` (`on_record`); `tls.cho` (resting, `set_resting`,
`areas_resting`; 1,944 lines to 1,989, under the 2,000 with the release's slice-level work moved to `slot.cho`: §8 says what
the next change to it needs). Tests: `tests/programs/record_open.cho` (new, `conformance/tls.rs`), `engine_cost.cho` (new, the
measurements of §7), the `rest:` checks of `tls_pool.cho`, the in-place cases of the drivers and the conformance tests of
`gcm.rs` and `aead.rs`.

## 6. Gates, stated before the numbers

1. **Vectors and differentials, both backends, the hardware path on and forced off:** `conformance/gcm.rs`, `aead.rs`;
   `scripts/gcm_differential.py` (now also `P`, `p`, `I`, `i`: in place on both paths), `scripts/aead_differential.py` (`T`,
   `U`), `scripts/tls_record_differential.py` (`O` and the new `J`, in place), byte for byte. "Forced off" is the software path
   (`seal_in_place_software`, `open_in_place_software`) on a CPU that has the instructions, and the Cranelift backend, which has
   none.
2. **The TLS suites unchanged byte for byte:** the RFC 8448 traces, the lying server and client, the differential against
   OpenSSL, tickets, interop, `tls_pool.cho` in every mode (`tickets`, `mutual`, `clientcert`, and the new `rest:` checks).
3. **Mutants:** `scripts/tls_mutants.py` (the retargeted work-area mutants and new ones for resting, the bulk copies and the
   record layer, with `tests/programs/record_open.cho` added to its evidence), `tls_server_mutants.py`, `gcm_mutants.py`,
   `chacha20_mutants.py`: all killed or argued equivalent.
4. **dudect** on the in-place functions at 10^6 measurements a test on x86-64, |t| < 4.5, the raw logs committed.
5. **Cost, before and after, beside OpenSSL on the same machine:** a kept `https_hello` request (cycles and instructions under
   `perf stat`); per record size (`record_cost.cho` against `gcm_cost.cho`); bulk (16 KiB records, `/big`).
6. **Memory per connection** (`scripts/tls_memory.py`) must not grow by more than the resting allowance: at most 32 areas' written
   pages per engine.
7. **`scripts/tls_perf.py check`** within its guard.

## 7. As built, and measured

Raw output of every table is in [`assets/record-in-place/`](assets/record-in-place/) (its README says which file is which).
x86-64 is `gram` as §1 (the measured process on CPU 6, its peer on CPU 7; load average 3 to 5 from other work on the other
cores); arm64 is this M4 Max natively under macOS 26 and its Docker VM (Linux 6.8). "Before" is `3d7e8e9d`, "after" this
change, built by the same compiler; alternating runs.

### 7.1 A kept request (`https_hello`, x86-64)

`perf stat` of the server's user and kernel cycles and instructions over 5 s of `tload` (32 connections, `GET /hello/42`), divided
by the requests, three rounds alternating (`kept-x86-64.txt`):

| | cycles a request | instructions a request | against OpenSSL (`oserv`), cycles |
|---|---|---|---|
| before | 40,004 to 40,143 | 72,215 to 72,616 | **1.20** |
| after | 31,407 to 31,639 | 45,797 to 46,244 | **0.94** |
| OpenSSL's server, `oserv` | 33,282 to 33,511 | 43,360 to 43,697 | |

**21.5% fewer cycles and 36.6% fewer instructions a request; T3's stretch, parity with OpenSSL, is met (0.94).** Step by step,
the same harness, two rounds each (`kept-steps-x86-64.txt`): the copies in bulk (§2.1) alone 33,609 to 33,903 cycles (1.01 times
`oserv`); with the record layer (§4) 33,112 to 33,570; one connection allowed to rest (the first design: it gained nothing,
§7.6) 33,789 to 33,983; resting as built 31,407 to 31,639. **Where the engine's share went** (`perf record`, inclusive, x86-64):
`tls.lease` and `tls.settle` were **24.0%** of a request before, **10.3%** with the copies in bulk, **3.6%** after (`lease` 0.9,
`settle` 2.6, of which `rest` 1.1 and the order's bookkeeping 0.7); `memmove` and `memset` 2.8% before, 0.4% after.

**arm64** (the Docker VM on the M4, which has no PMU: server CPU time from `/proc`, `https_hello_test.py --cost`, two passes
alternating, `cost-arm64.txt`): **5.50 and 5.51 µs a request before, 1.11 and 1.15 times `oserv`'s 4.80 and 4.96; 4.46 and 3.91
after, 0.96 and 0.90 times `oserv`'s 4.64 and 4.35.**

On x86-64 the server CPU time `scripts/https_hello_test.py --cost` reports moved with the clock in these runs (OpenSSL's own plain server
measured 8.2 µs a request in the first pass and 10.1 µs in the second) and is not used; it is in `cost-x86-64.txt`.

### 7.2 A record, and a request with no sockets (`engine_cost.cho`, x86-64)

`tests/programs/engine_cost.cho` drives a client engine and a server engine in one process: `bulk` sends records of a size from
client to server (the client's seal, the server's open, and every engine call around them), `kept` serves a kept request on every
connection phase by phase, as an event loop does (every request in, then every answer out), so a connection's calls are not
consecutive. Cycles and instructions a record or a request, as the difference of two runs (N and 2N) under `perf stat`, two
passes alike to 1% (`engine-cost-x86-64.txt`):

| | before: cycles | instructions | after: cycles | instructions | cycles, after / before |
|---|---|---|---|---|---|
| a 64-byte record | 6,591 to 6,625 | 32,025 | 1,150 to 1,153 | 5,398 | **0.17** |
| a 1 KiB record | 8,368 to 8,494 | 38,820 | 2,561 to 2,568 | 11,151 | **0.30** |
| a 16 KiB record | 38,998 to 39,240 | 139,406 | 26,734 to 26,739 | 91,421 | **0.68** |
| a kept request, 32 connections | 14,330 to 14,374 | 63,952 | 2,380 to 2,389 | 10,855 | **0.17** |
| a kept request, 64 connections (more than rest) | 16,168 to 16,395 | 63,968 | 8,549 to 8,832 | 15,128 | 0.53 |
| a kept request with a 16 KiB answer, 32 connections | 50,984 to 51,815 | 171,509 | 31,013 to 31,365 | 96,984 | 0.61 |

A 16 KiB record through both engines is 26.7k cycles, of which the cipher is about 2 × 10.7k (`tls-performance.md` §10.2:
a seal 10,693, an open 10,643): the rest, 5k cycles, is the engines, the copy into the output queue and the input buffer.
Before it was 17.6k. With 64 connections, more than the 32 that may rest, each lease passes an area on (a copy out and a copy
in, no clearing): half of before.

**On the M4 under macOS** (the same program, two runs each, clock time, the Mac's load average 11 to 14): a 64-byte record
1.49 to 1.54 µs before, 0.24 after; a 16 KiB record 9.4 µs before, 6.2 after (1.75 to 2.64 GB/s of content); a kept request
3.42 to 3.48 µs before, 0.53 after with 32 connections, 3.47 and 1.73 with 64 (`engine-cost-macos-m4.txt`). The 64-byte
record's 1.3 µs was the record layer's regions and byte loops; `gcm-wide.md` §8.2 measured `tls_record.seal` alone at
592 ns on this machine before #457.

**arm64 Linux** (the VM, clock time, two passes, `engine-cost-arm64.txt`): a 64-byte record 2.1 to 2.2 µs before, 0.43 to 0.49
after; 1 KiB 2.9 to 3.5 and 0.95 to 1.15; 16 KiB 14.5 to 18.7 and 9.9 to 13.0; a kept request with 32 connections 4.9 to 6.7 µs
before and 1.0 to 1.3 after, with 64 4.8 to 6.9 and 2.6 to 3.5.

**`tls_record.seal` and `open` alone** (`record_cost.cho`, which `tls-performance.md` §10.5 used): on `gram` today the
clock moved by up to 1.6 times between two runs of the same binary a minute apart (`gcm_cost`'s 16 KiB seal 5.9 µs in one pass,
`record_cost`'s 4.1 µs in the next), so its ratios are not claimed; the runs are in `record-cost-x86-64.txt`. The record layer's
own cost is now the copy of the content into the output queue (sealing) and nothing (opening straight into the received data);
`engine_cost` above is the measurement of it with what surrounds it.

### 7.3 Bulk through a socket (`/big`)

`https_hello`'s `/big/16777216` eight times on one kept connection from Python's `ssl` (`big-x86-64.txt`): server **16.0 cycles a
byte before, 15.7 after** (74.4 and 73.1 instructions a byte). The record layer was not what a big answer costs: a profile of
it puts 54% in `http.server.output`, 17% in the example's `hello_app.feed_streams` and 14% in `http.server.stream` (byte loops
in `packages/http-server` and the example, which make and copy the body), and 6% in the cipher (§8).

### 7.4 Memory per connection

`scripts/tls_memory.py cancho` (`tls_echo`), KiB of RSS a connection, x86-64, before / after (`memory-tls_echo-*.txt`;
`https_hello`'s are `memory-https_hello-*.txt`):

| N | established | echoed | partial | all-echoed |
|---|---|---|---|---|
| 50 | 17.5 / 15.5 | 54.6 / **97.4** | 80.7 / 106.9 | 140.0 / 121.9 |
| 100 | 19.1 / 17.9 | 55.6 / **76.9** | 83.8 / 96.6 | 143.7 / 126.3 |
| 200 | 17.9 / 17.6 | 54.2 / **65.1** | 82.5 / 89.2 | 142.7 / 126.2 |
| 10,000 | 17.0 / 17.0 | 53.0 / 53.3 | 81.5 / 81.6 | 142.0 / **125.8** |

(`connected` 86.4 to 86.7 and `handshake` 110.2 to 112.0 both before and after.) **The bound of §2.3 holds:** `echoed` grows
by 2.1 MiB for the engine whatever N is (43 KiB × 50, 21 × 100, 11 × 200, 0.3 × 10,000): the 32 connections that rest and one
more, each area written by a 16 KiB record (about 65 KiB). An idle established connection is what it was (17.0 KiB at 10,000),
and a connection that has carried a record and gone idle at 10,000 connections is 53.3 KiB against 53.0. **The high-water mark
fell by 16 KiB a connection** (142.0 to 125.8 at 10,000): a TLS 1.3 record no longer writes `b_plain`. `https_hello`
(`--connections` 50 to 200, after a 16 KiB answer each): established 29.0 to 30.9 before and 28.5 to 30.5 after; after the
answer 69.8 to 71.2 before and 75.6 to 93.6 after, the same 1.1 to 1.2 MiB for the engine. `tls.set_resting(engine, 0)`
gives back exactly the memory of #395.

### 7.5 The gates

- **Differentials** (§6.1), x86-64 (OpenSSL 3.5.5) and arm64 Linux (OpenSSL 3.0.13), LLVM (the hardware path for `P`, `I`, the
  software path for `p`, `i`) and Cranelift (no hardware path): `gcm_differential.py` 268,180 checks, `aead_differential.py`
  60,000, `tls_record_differential.py` 3,000 records sealed and opened both ways: **0 differences on each of the four**
  (`differentials-*.txt`).
- **The TLS suites:** the conformance tests (the RFC 8448 and recorded traces, the lying server and client, tickets,
  `tls_pool.cho` in every configuration on both backends, `record_open.cho`) pass unchanged; CI's `tls-assurance` runs the
  interop matrix and the differentials against `openssl s_server` and `s_client`.
- **Mutants** (arm64 Linux): `gcm_mutants.py` 43 of 43 killed (9 new: in place, both paths), `chacha20_mutants.py` 26 of 26
  (8 new), `tls_mutants.py` and `tls_server_mutants.py`: §7.7.
- **The guard:** `scripts/tls_perf.py check` 33 of 33 rows within 1.3 times on x86-64 and on arm64 Linux; no row moved (the
  primitives are not what changed) (`guard-*.txt`).
- **Constant time:** §7.8.

### 7.7 Mutants

On arm64 Linux, from a snapshot of the tree (`mutants-*.txt`):

- `tls_mutants.py`: **207 of 210 killed, 3 argued** (210 with the 27 new ones: resting, the block copies, the record layer). The
  18 work-area mutants of #395 that pointed at code this change moved were retargeted to where it is now, and a mutant of
  `slot.cho` now runs the engine's evidence too (`tls_pool.cho`), since the release's work lives there. The new argued one is
  "a released area's opened record kept": unreachable by these tests (§8), not harmless. `record_open.cho` is part of the
  evidence of every mutant. In the first run nine survived: five moved work-area mutants of `slot.cho` that ran without the
  engine's evidence (fixed in the script), and three record-layer ones and one of resting that the tests could not see, which
  made the tests stronger: `record_open.cho` now checks the received data after each refused record separately, that a record
  opened straight into it leaves the input buffer as it arrived, and the content a handshake message leaves is taken and cleared
  (`tls_slot.take_opened`, moved from `client.cho` and `server.cho` so that it is tested); the `rest:` checks see which
  connection was given back.
- `tls_server_mutants.py`: 223 of 227 killed, 4 argued, as before (two retargeted to the moved clearing of the leaf).
- `gcm_mutants.py`: **43 of 43** (9 new, in place: decrypted before the tag is checked, the tag written over or compared with
  the text, encryption from counter 1, the tag read from the text, a buffer with no room for a tag taken, the wrong refusal
  for one). Two survived the first run, the short-buffer ones: the driver could not hand `seal_in_place` a buffer shorter than
  a tag, so it got ops `Y` and `y` that seal over exactly the buffer given, and the conformance test that uses them.
- `chacha20_mutants.py`: **26 of 26** (8 new).

### 7.8 Constant time

`scripts/gcm_timing.py --in-place` (dudect, Reparaz, Balasch and Verbauwhede) on `gram`, `taskset -c 7`, 10^6 measurements a
test, the in-place functions under a prepared key, 13 bytes of associated data (`timing-*.txt`). Max |t|:

| | seal, data | open, data | open, tag |
|---|---|---|---|
| AES-GCM, hardware path, 64 B | 3.42 | 2.72 | 1.99 |
| AES-GCM, hardware path, 1,000 B | 2.17 | 1.27 | 2.87 |
| AES-GCM, hardware path, 16,384 B | 3.55, 2.39, 2.17 | 3.92, 2.44, 2.83 | **5.39**, 3.19, 2.89 |
| ChaCha20-Poly1305, 64 B | 2.41 | 3.72 | 1.60 |
| AES-GCM, software path (Cranelift), 64 B | 2.15 | 1.48 | 1.01 |

**One run of 16 KiB "open, tag" reached 5.39**, in the first pass, with the machine's load average 4 to 8 from other work; two
reruns gave 3.19 and 2.89. In the same session `--prepared`, which times `open_with`, the unchanged function the in-place one
calls the same builtin as (`gcm_tag_diff` over the same bytes), gave 4.36 and 5.44 for "open, tag" and **22.26 for "open, data"**
(`timing-16k-rerun-x86-64.txt`): the noise of the shared core's sibling, not either function. The gate as stated (|t| below 4.5)
is met by two of the three in-place runs at 16 KiB and by every other row; a quiet machine was not available.



### 7.6 What did not work

**One connection resting, the first version.** It rested one connection and gave its area back as soon as another came to rest.
It cost more than it saved (33,789 to 33,983 cycles a request against 33,112 to 33,570 without it): `https_hello`'s loop serves
every ready connection's input before any answer (§1), so the one resting connection was always displaced before its own next
call. Resting up to 32 then cost more still, 35,817 to 36,034 cycles (55,021 to 55,448 instructions): `https_hello` calls
`take` and `recv` on every connection on every turn, and on a resting connection each ran a whole `settle` and moved it in
the order. Answering `take` and `recv` on a resting connection from its core with no `settle` (it has nothing queued and
nothing waiting, by what resting is) is what is built (`kept-steps-x86-64.txt`).

**A handshake taking a free area.** With resting as first built, `tls_echo` with 50 connections handshaken one at a time was
95.5 KiB a connection established (from 17.5): each handshake took a free, never written area while 32 others rested, and wrote
`std.ecdh`'s 75 KiB of work into it. A handshake now takes the longest-resting connection's area before a free one (§2.4), so
the areas handshakes write stay one or two.

## 8. Not done

- **`tls.cho` is 1,989 lines.** This change moved the release's slice-level work to `slot.cho` to stay under the 2,000; the next
  change to the engine needs a split by concern first. The client's ticket store (its entries' layout and `in_pool`, `held`,
  `pick`, `wipe`, `may_offer`, about 150 lines on the `tickets` and `tmeta` slices alone) is the piece that moves without the
  `Engine` type; it is not done here because it is not this change's concern and its mutants would move with it. *Done in #465 (`packages/tls/saved.cho`, module `tls_saved`): a pure move, the entries' layout and those five
  functions over the two slices, `tls.cho` 1,989 lines to 1,863; the 22 mutants of `scripts/tls_mutants.py` whose text moved or
  was renamed follow it (seven now name `saved.cho`), and all 22 are killed again, with the 21 other ticket, pool and area
  mutants (`assets/key-tables/mutants-tls-arm64.txt`).*
- **`/big` is the HTTP server's bytes, not the record's** (§7.3): `packages/http-server`'s `output` and `stream`, and the
  example's body, copy a byte at a time. Not touched: another package.
- **TLS 1.2 still opens into `b_plain`**, with its region for the associated data and nonce (`open12`, `seal12`). The engine's
  TLS 1.2 path is not measured or tested through `tls_pool` (no TLS 1.2 server engine exists), so the mutant "a released area's
  opened record kept" is unreachable by the tests and is listed as such, not as harmless.
- **The software AES-GCM path's contexts** (2 × 129 words of the core) are copied with the core though the hardware path never
  reads them; shrinking the core is a layout change other work would conflict with, and with resting it is copied rarely.
- **ChaCha20-Poly1305 in place** keeps `std.chacha20`'s own regions (its keystream block and the tag's state): the cipher is
  10 times OpenSSL's cost (`tls-performance.md` §10.2) and its regions are not what a record costs.
- **Resting's allowance is one number for every engine** (32). A program that keeps more connections busy between one
  connection's calls gains nothing from it unless it raises it, and pays 65 KiB an area for each it raises it by.
