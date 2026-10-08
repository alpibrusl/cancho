# Multi-block AES-GCM and aggregated GHASH

> **Status: built (#382, part of #378).** `docs/crypto-builtins.md` gave `std.gcm` the hardware AES and carry-less multiply
> instructions, one block at a time through out-of-line builtin calls, and reduced GHASH after every block. It reached 0.56 to
> 1.1 GB/s against OpenSSL's 4 to 8. This document profiles where the time goes (§2), says what was built and computes what to
> expect from the measured parts (§3), and gives what it measured (§8) and what it did not do (§9). Where a later commit finds
> a claim here false, that commit corrects it here, in place.

---

## 1. What is asked

`std.gcm`'s hardware path calls `aes_encrypt_block` once for every 16 bytes of keystream and XORs the keystream in cancho, then
calls `ghash_update` over the ciphertext, which multiplies and reduces for every block. The issue asks for the two things
OpenSSL does that this does not: **several blocks of counter mode at once** (so that one block's `aesenc` latency is hidden
behind seven other blocks), and **GHASH aggregated** (one reduction for eight blocks, from precomputed powers of H held in the
prepared key of `docs/crypto-builtins.md` §6). Expected: 2 to 4 times, "a hypothesis to measure first".

**Not asked for, and noted only:** ChaCha20-Poly1305 for machines without AES instructions needs vector arithmetic (four
ChaCha20 blocks in four lanes, or Poly1305 in vector multiplies), which the language does not have. It is the same decision
`docs/crypto-builtins.md` §9 (3) and `docs/chacha20.md` §6 left open: a vector type is a language change (the checker, both
backends, how a vector meets checked arithmetic), larger than anything here, and nothing in this document depends on it or
makes it harder. The software AES-GCM path is unchanged, and so is what Cranelift and a CPU without the instructions run.

## 2. Where the time goes today

**Method.** `tests/programs/gcm_cost.cho` seals and opens records of 64, 1,024 and 16,384 bytes (13 bytes of associated data, a
TLS record header's length) under one prepared key, three runs of 64 MB, and prints the best. The split is a sampling profile of
the same program: `perf record` on x86-64 Linux, `sample` on macOS. Cycles are `perf stat -e cpu_core/cycles/u` over the whole
run divided by the bytes sealed and opened, so they do not depend on the clock the governor chose.

**Machines.** *gram*: Intel Core i7-1260P (Alder Lake, a P-core thread, `taskset -c 7`, the `powersave` governor: 1.9 to 2.0 GHz
while measured), Linux x86-64, load average 6.5 to 9.5 from other users' jobs, OpenSSL 3.5.5. *This Mac*: Apple M4 Max, macOS
26, load average 14 to 22 (other sessions). Neither was quiet, so each number below is the best of three runs, and the
comparisons that matter are made on the same machine in the same minute.

### 2.1 What a record costs today (AES-128-GCM, a prepared key)

| Size | gram: cycles/record | cycles/byte | Mac: ns/record | Mac: MB/s |
|---|---|---|---|---|
| 64 B | 832 | 13.0 | 658 | 97 |
| 1,024 B | 6,772 | 6.6 | 1,556 | 657 |
| 16,384 B | 102,320 | 6.2 | 15,625 | 1,048 |

(Seal and open cost the same within 5%: `gcm_cost` prints both.) On gram that is 6.2 cycles a byte, 100 cycles for every 16-byte
block; OpenSSL 3.5.5 on the same core in the same minute does 0.55 cycles a byte at 16 KiB (424,608 operations of 16,384 bytes
in 3.8 billion cycles). The gap is **11 times** there. The first-order cost is the same on both CPUs, 65 cycles a block on the
M4 Max at about 4.5 GHz and 100 on the i7.

### 2.2 The split

Share of the samples by function, seal and open together:

| gram | 64 B | 1,024 B | 16,384 B |
|---|---|---|---|
| `ctr_hw`: the counter, the 16-iteration XOR loop with its checks, the call into the cipher | 32.7% | 36.7% | 39.9% |
| `cancho_ghash_update`: bit reversal of each block, four multiplies, the reduction | 27.8% | 27.1% | 27.7% |
| `cancho_aes_encrypt_block`: the AES instructions themselves | 16.0% | 33.7% | 17.5% |
| `ghash_hw`, `tag_hw`, `seal_hardware`, `open_hardware`: padding, the length block, the call glue | 15.7% | 1.7% | 8.5% |
| `malloc` and `free` (a `region` is one 64 KiB `malloc`) | 6.6% | 0.4% | 3.8% |

| Apple M4 Max | 64 B | 16,384 B |
|---|---|---|
| `ctr_hw` | 6% | 39% |
| `cancho_ghash_update` | 2% | 43% |
| `cancho_aes_encrypt_block` | 1% | 16% |
| `malloc` and `free`, and the kernel's page-reclaim accounting under them | 88% | 3% |

What that says:

1. **The instructions are the smallest part.** The AES instructions are 16 to 18% of a 16 KiB record on both machines (the
   1,024-byte row is an outlier of that one run; the 16 KiB and the 64 B rows agree with each other). The rest is cancho code
   and the way the builtins are shaped: **a call a block, with eight length checks at the call site and a byte-at-a-time XOR
   loop** (40%), and **GHASH reduced a block at a time, with a 128-bit bit reversal of every block** that the reflected form of
   §3.2 does not need (28 to 43%). The issue's hypothesis that the call overhead matters is right, but it is not the cost of a
   call: it is that the keystream goes through memory and a cancho loop on its way to being XORed.
2. **A wider cipher builtin alone would not do.** Eight blocks of `aesenc` per call with the XOR left in cancho would still
   leave the 40% in `ctr_hw`. The builtin has to take the data and the output and do the XOR itself.
3. **At 64 bytes the cipher is not the cost on either machine.** On Darwin 88% of a 64-byte record is the region: 64 KiB
   `malloc` and `free` (`docs/crypto-builtins.md` §8.1 measured it at a third of a record; it is now most of one, with the
   page-reclaim trap under it). On Linux it is 6.6% and the rest is glue. So the hardware path must **not allocate at all**.
   That decides the shape of §3.4: no builtin needs scratch from its caller, and the tag is one call.

This redirects part of the work, as the issue asked it to if the profile said so: the multi-block builtins are the main part,
but the larger gain at the record sizes a TLS connection spends most of its bytes on (1 to 16 KiB) is the XOR loop and the
GHASH reductions, and at small sizes it is the allocation. All three are in this design.

### 2.3 The record layer around the cipher

`tests/programs/record_cost.cho` times `tls_record.seal` and `tls_record.open` (TLS_AES_128_GCM_SHA256, a prepared key) the
same way, so the difference from `gcm_cost`'s row is what the record layer adds. It is measured here, before the change, as the
issue asked, and again in §8.2 after it:

| Apple M4 Max, before | 64 B | 1,024 B | 16,384 B |
|---|---|---|---|
| `tls_record.seal`, ns a record | 1,235 | 2,120 | 16,113 |
| `std.gcm`'s seal alone (§2.1) | 658 | 1,556 | 15,625 |
| the record layer's own | 577 | 564 | 488 |

The record layer's own cost is about 0.5 microseconds a record at every size. It is `region r { ... }` (one 64 KiB `malloc` and
`free`, which is what 88% of a 64-byte record was on Darwin in §2.2), the byte-at-a-time copy of the plaintext into the inner
plaintext, and the 12-byte nonce. Before this change it was a third of a 64-byte record and 3% of a 16 KiB one. **After it, it
is most of a small record** (§8.2), which is the redirect the issue allowed for: it does not change what is built here, and
the cipher was the right thing to fix first, since it was 53 to 97% of every row. What follows from it is a separate change to
`packages/tls/record.cho` (an AEAD that seals in place, so no inner copy and no region), noted in §9 and not made here: it changes `packages/tls`, which other
work is changing at the same time, and wants a design of its own (§9).

## 3. The design

### 3.1 `aes_ctr32`: counter mode, eight blocks at a time

```cancho
aes_ctr32(round_keys: &[byte], rounds: int, nonce: &[byte], counter: int, input: &[byte], out: &![byte]) -> [] int
```

`out[i] = input[i] XOR E(nonce || be32(counter + i / 16))[i % 16]` for every `i`, where `E` is AES under `round_keys`
(`rounds + 1` blocks of 16 bytes, `rounds` 10, 12 or 14, as `aes_encrypt_block` takes them), `nonce` is 12 bytes, and the
counter is a 32-bit value that wraps modulo 2^32 as SP 800-38D's `inc32` does. `input` and `out` are the same length, which is
any length (the last block may be partial). It answers 0.

Eight 16-byte blocks are 128 bytes: the loop builds eight counter blocks (the nonce with a counter lane, added in the machine's
byte order and swapped to big-endian by one byte shuffle), runs them through the rounds together, XORs 128 bytes of input, and
stores 128 bytes of output. Eight is what keeps `aesenc` busy: it takes 3 to 4 cycles to answer on Golden Cove and Apple's
cores take 3, and issues two a cycle on Golden Cove, so one block in flight leaves the unit at 1/6 and eight fill it. (OpenSSL's
AES-NI code interleaves six and eight.)

The last `len mod 128` bytes use the same eight-block cipher into a 128-byte stack buffer, XOR the whole 16-byte blocks of it,
then the remaining bytes one at a time, then **overwrite the buffer** with volatile stores, since the keystream is secret. Every
loop bound and every branch is on `len`, which is public (§5). Computing eight blocks for a one-block tail costs about the
latency of one, so a 64-byte record does not pay for being short.

### 3.2 GHASH: one reduction for eight blocks, in the reflected order

A GCM block is a polynomial of degree under 128 in which the *first* bit is the coefficient of `x^0`. The existing builtin
reversed the bits of every block (and of H, and the state) to get the natural order, which a bit reversal on a 128-bit integer
makes expensive. The reflected form of Gueron and Kounavis needs none: **read the block as a big-endian integer (one byte
reversal)**, so that bit `127 - i` is the coefficient of `x^i`, and multiply as integers.

With `a` and `b` in that form, `clmul(a, b)` has the coefficient of `x^m` at bit `254 - m`. Read as a 256-bit window with
its top bit as `x^0` (bit `255 - m` is `x^m`), that is `x` times the product, so the product of `a` and `b * x^-1` is the
window's polynomial exactly: **store each power of H times `x^-1`** (`H^k` shifted left by one, with `0xC2000...01`, which is
`x^127 + x^6 + x + 1` and so `x^-1` itself, XORed in if a bit left the top) and no product is ever shifted. The upper 128
bits of the window are then the polynomial's coefficients 0 to 127 in the same order and the lower 128 bits are coefficients
128 to 255 (the part to fold back by `x^128 = x^7 + x^2 + x + 1`).

**The reduction** is the one the Linux kernel's `ghash-clmulni-intel` uses, on 64-bit lanes only (`psllq`, `psrlq` and one
byte shift of the whole register: on aarch64 `shl`, `ushr` and `ext`), with `(T1 : D)` the window, `D` the lower half:

```
phase 1:  T3 = ((((D << 1) ^ D) << 5) ^ D) << 57          every shift of each 64-bit lane
          D  = D ^ (T3 moved up one lane)                  T1 = T1 ^ (T3 moved down one lane)
phase 2:  T2 = ((((D >> 5) ^ D) >> 1) ^ D) >> 1
          result = T1 ^ D ^ T2
```

The first phase folds each lane's low bits into the other lane, the second is `D ^ D>>1 ^ D>>2 ^ D>>7` (the polynomial's
shifts 0, 1, 2 and 7) added to the upper half. An earlier version of this builtin did the same fold on 128-bit integers in the
general registers, from a left shift of every product by one: the vector version is the same arithmetic in about half the
instructions and none of the transfers between register files (a 2% faster 16 KiB record on x86-64, measured; the reduction
was a third of that record's cycles when removed outright, 0.23 of 0.68 a byte, so most of it is not the arithmetic but the
multiplies and loads around it).

This was checked against SP 800-38D §6.3's bit-by-bit algorithm in a model (300 random single products and an eight-block
aggregation) before any IR was written: `scripts/ghash_reflected_model.py`, whose steps are the code's.

**Aggregation.** The recurrence `Y_i = (Y_{i-1} ^ X_i) * H` unrolls over eight blocks to
`Y_8 = (Y_0 ^ X_1) * H^8 ^ X_2 * H^7 ^ ... ^ X_8 * H`. Carry-less multiplication distributes over XOR, so the eight unreduced
256-bit products can be summed, and **the fold is done once for the sum**. For the product of two 128-bit values
the builtin keeps three accumulators, the sums of the four 64-by-64 products: low halves, high halves, and the two cross terms
together; the 256-bit window is `lo ^ (mid << 64) ^ (hi << 128)` and is built once per group. Four `pclmulqdq` (or `pmull`) a
block and no Karatsuba: Karatsuba saves one multiply a block for extra XORs and a table of the halves' sums, and is a
measured option for later (§8), not a first design.

A group of fewer than eight blocks (the end of a message, a short record, the associated data) uses the same code with the
powers shifted: a group of `n` blocks multiplies block `j` by `H^(n-j)`, so groups of 8, 4, 2 and 1 are four short pieces of
one routine, all reading the same table. The tail of `m` blocks (1 to 7) is split by the bits of `m` into groups of 4, 2 and 1:
at most three reductions, not seven.

**The table.** `ghash_powers` computes `H^1` to `H^8` once per key (each the previous times the twisted H) and stores each
times `x^-1`, 16 bytes, `H^1` first.

### 3.3 The prepared key (`hw_len()`)

The hardware part of a prepared key (`std.gcm`'s `hw`, held by the record layer per direction) was 256 bytes: the round keys in
FIPS 197's form (up to 240) and H. It becomes **368 bytes**: the 240 bytes of round keys, then the eight powers (128 bytes),
H being the first power. `hw_len()` is that number and the callers that size by it follow it. The one that does not is
`packages/tls/slot.cho`, which lays the two directions' prepared keys out at fixed offsets (`k_read_hw`, `k_write_hw`,
`keys_len`); it moves by 112 bytes per direction, 224 per connection. The software context (`context_len()` words) and the
software path are **unchanged**, and `forget` still overwrites both. Nothing in this design needs the word layout of the
software path to change, so a CPU without the instructions, and Cranelift, run today's code.

### 3.4 `ghash_powers`, `gcm_tag` and `gcm_tag_diff`: the tag in one call, with no scratch

```cancho
ghash_powers(h: &[byte], table: &![byte]) -> [] int
gcm_tag(round_keys: &[byte], rounds: int, table: &[byte], nonce: &[byte], aad: &[byte], text: &[byte], tag: &![byte]) -> [] int
gcm_tag_diff(round_keys: &[byte], rounds: int, table: &[byte], nonce: &[byte], aad: &[byte], text: &[byte], expected: &[byte]) -> [] int
```

`ghash_powers` fills the 128-byte `table` from the 16-byte `h` (`H = AES(K, 0)`) with the twisted powers of §3.2. `gcm_tag` writes the 16-byte tag of `text`
(the ciphertext) and `aad` under `nonce`: GHASH of `aad`, zero-padded to a multiple of 16, then of `text`, zero-padded, then the
length block (both lengths in bits, 64 each), XORed with `AES(K, nonce || 1)`. The encryption of `nonce || 1` is issued first so
its latency runs under the hashing. `gcm_tag_diff` computes the same tag and answers 0 if it equals `expected` and nonzero if
not, by ORing the two halves of the XOR of the tags, so every byte counts whatever the earlier ones were.

**Why not `ghash_update` with a table.** The previous builtin, `ghash_update`, left the padding, the length block, the mask, the
scratch for each of them and the comparison to cancho. Every one of those needed a 16-byte buffer, hence a region, hence 88% of
a 64-byte record on Darwin (§2.2). With `gcm_tag` and `gcm_tag_diff`, **seal is two calls and open is two calls, and `std.gcm`'s
hardware path allocates nothing**. The old `ghash_update` is removed: nothing else called it, and keeping a slower, differently
shaped GHASH primitive would be two ways to do one thing. `aes_encrypt_block` stays (the key schedule's `H`, and a single block
for a test), and so does `hw_aes_gcm`.

`seal_hardware`: `aes_ctr32(keys, nr, nonce, 2, plaintext, out[0..n])`, then `gcm_tag(..., out[0..n], out[n..n+16])`.
`open_hardware`: `gcm_tag_diff(..., sealed[0..n], sealed[n..n+16])`; if it is not 0, the refusal and **nothing written** (as
before); else `aes_ctr32(keys, nr, nonce, 2, sealed[0..n], out)`.

### 3.5 What the expected gain is, from the measured parts

At 16 KiB the measured 100 cycles a block are 17.5 for the instructions, 28 for GHASH and 55 for the cancho code and the call
around the cipher. The design replaces them with:

- **Counter mode:** `aesenc` issues two a cycle on Golden Cove and one on Apple's cores (Apple issues more per cycle but at a
  higher clock; call it 1.5): AES-128 is 10 rounds of 8 blocks, 80 instructions per 128 bytes, **about 40 cycles a group**, or 5
  a block (0.31 cycles a byte), plus the counter arithmetic and the XOR, 1 to 2 a block. About **7 cycles a block**.
- **GHASH:** four multiplies a block at one a cycle is 4, the accumulating XORs 3, the byte reversal and loads 1, and the
  shift-and-fold, about 40 cycles for a group, 5 a block. About **9 cycles a block**.
- **Together, if the two passes do not overlap: about 16 cycles a block, 1 cycle a byte, 6 times faster than today's 6.2**; the
  out-of-order core overlaps the tail of one pass with the head of the next, so somewhat better. OpenSSL stitches the two into
  one loop and reaches 0.55; this design does two loops over a buffer that stays in L1 (16 KiB of a 48 KiB L1d), and does not
  expect to match it. **Predicted: 5 to 8 times at 16 KiB, 0.7 to 1.2 cycles a byte; well above the issue's 2 to 4.**
- **1,024 bytes:** the same per-block cost plus a fixed cost of two calls and the 40-cycle latency of the first block and of
  `AES(K, J0)`, about 150 cycles in all: **about 1 to 1.2 cycles a byte**, 5 to 6 times faster than today's 6.6.
- **64 bytes:** no allocation, two builtin calls, four blocks of AES and five of GHASH: **about 150 to 250 cycles a record on
  x86-64 (4 times), and about 40 ns on the M4 where today's is 658 ns (15 times, for the malloc alone is 580).**

(These were computed from the baseline's parts. A first prototype of the IR was already running when this document was written,
and §8 says what it and the finished code measured.)

## 4. The builtins in both backends

The names are `edition 7` (the latest, as `hw_aes_gcm` is; a name a program could already declare).

| Builtin | Type | Regions |
|---|---|---|
| `aes_ctr32` | `(&[byte], int, &[byte], int, &[byte], &![byte]) -> [] int` | keys, nonce, input, out |
| `ghash_powers` | `(&[byte], &![byte]) -> [] int` | h, table |
| `gcm_tag` | `(&[byte], int, &[byte], &[byte], &[byte], &[byte], &![byte]) -> [] int` | keys, table, nonce, aad, text, tag |
| `gcm_tag_diff` | `(&[byte], int, &[byte], &[byte], &[byte], &[byte], &[byte]) -> [] int` | keys, table, nonce, aad, text, expected |

All are `[]` (pure): no capability is added, and the pure backend's `cancho authority` shows no `ffi(...)` as before.

**LLVM** (`crates/cancho-codegen-llvm/src/crypto.rs`, `crypto/aes.rs`, `crypto/ghash.rs`, `body/crypto.rs`). As in
`docs/crypto-builtins.md` §4, each builtin is a call to an **out-of-line function defined only in a module that calls it,
carrying the target features on its own definition**, never the module's: `+aes,+pclmul,+ssse3,+sse2` on x86-64 and
`+aes,+neon` on aarch64. The functions that hold the intrinsics but are shared (`@cancho_aes1`, `@cancho_aes8`, the GHASH
group routines and `@cancho_gh_reduce`) are `alwaysinline` with the same features, so they fold into the one out-of-line
function that calls them and never into a caller at baseline. The call site checks every length and the round count, **trapping
as an out-of-bounds index does** (`rounds` is 10, 12 or 14 and the key `16 * (rounds + 1)` bytes; `nonce` 12; `table` 128;
`h` and `tag` 16; `counter` in `0..2^32`; `input` and `out` equal), then calls. No *input* reaches those traps: the lengths are
fixed by the suite (`std.gcm` refuses a bad key or nonce with its rule tag first), the counter is `2` and the lengths are the
caller's slices'. On a target without the instructions (`hardware_target` false) the call site is a trap. The vectors are all
`<2 x i64>` in the text and the aarch64 ones bitcast around the intrinsic, which costs nothing. The x86 multiplies are
`llvm.x86.pclmulqdq` with the half selected by the immediate; the aarch64 ones `llvm.aarch64.neon.pmull64` on the extracted
lanes, which LLVM selects as `pmull2` when both are the high halves.

**Cranelift** (`crates/cancho-codegen/src/body/expr.rs`). As for the two builtins of `docs/crypto-builtins.md` §5: no
instruction exists, `hw_aes_gcm()` answers false, `std.gcm` takes the software path, and a call that is reached anyway **traps**.
**WebAssembly** has none either (`target.rs`: `HwAesGcm` and the rest are on the list of builtins that need no host).

## 5. Constant time

- **No branch or index depends on a secret.** The key, the counter blocks, the keystream, H and its powers, the state and the
  tag are only ever operands of `aesenc`/`aese`, `pclmulqdq`/`pmull`, XOR, constant shifts and shuffles. Every loop bound and
  every conditional jump is on a length (`len`, the number of 128-byte groups, the bits of the tail's block count). A length
  is public: the record layer sends it in the clear. The audit of the object code (§7) reads every conditional jump that is
  not a trap and names what it tests.
- **The tail.** A message whose length is not a multiple of 128 is finished by a fixed-shape path whose shape depends only on
  `len mod 128`: eight blocks of keystream computed whatever the tail's length, then 16-byte XORs and byte XORs counted by the
  length. The GHASH tail is padded with zeros in a stack buffer and then goes through the same group routines as full blocks;
  a ragged last block is the only branch (on `len mod 16`).
- **Tag comparison.** `gcm_tag_diff` XORs the two tags and ORs the halves: no early exit. Its caller branches on the answer,
  which is public by construction (it is whether the record authenticated), as the previous comparison's was.
- **What is left behind, and what is not.** The keystream tail buffer is overwritten before the function returns (volatile
  stores; a unit test reads the generated text for them). The ciphertext copied for padding is public. The table of powers is
  part of the prepared key and `forget` overwrites it. Registers the compiler spills to the builtin's own stack frame (round
  keys, counter blocks) are not erased, as in every compiled implementation, OpenSSL's included; that was equally so for the
  builtins this replaces.
- **DIT.** On aarch64 the LLVM backend sets `PSTATE.DIT` in `main` (`docs/crypto-builtins.md` §6, `dit.rs`). This design does
  not touch that code, and the builtins' instructions (`aese`, `aesmc`, `pmull`, `eor`, `rev`, `ext`, `shl`) are all in the DIT
  list of the Arm ARM or data-independent by definition; the timing run is repeated on the M4 with and without it (§7).

## 6. The tag and the associated data for lengths that are not a multiple of 16

GCM pads the associated data and the ciphertext **each** to a multiple of 16 with zeros and hashes them one after the other,
then the length block. `@cancho_gh_absorb` does one of them: whole groups of eight go straight from the caller's slice; the
rest (under 128 bytes) is read from the slice when its length is a multiple of 16 and otherwise **copied into a zeroed 128-byte
stack buffer** (`memset` then `memcpy` of `len mod 128` bytes) and read from there. Either way the group routines see whole
16-byte blocks and the block count is `ceil(len mod 128 / 16)`, up to eight. The length block is built in registers (text bits
in the low lane, associated-data bits in the high one: the reflected form of the big-endian `aad_bits || text_bits`), so there
is no buffer for it. An empty `aad` or `text` hashes nothing. The 32-bit counter of `aes_ctr32` starts at 2 for the data and
`AES(K, nonce || 1)` is the tag mask, as before; the largest message under one nonce stays 2^32 - 2 blocks
(`gcm.max_text()`), so the counter never wraps for a correct caller, and the wrap is nevertheless defined (§3.1) and tested.

## 7. How it is checked

- **Known answers and a differential on every length, in the compiler's own suite**
  (`crates/cancho/tests/conformance/crypto_builtins.rs`, through `tests/programs/crypto_builtins_driver.cho`): FIPS 197,
  NIST's test case 2 from the builtins, and references written from FIPS 197 and SP 800-38D in the test: `aes_ctr32` on
  **every length 0 to 300** and with counters about to wrap; the eight powers; `gcm_tag` on every text length 0 to 300 with
  associated data of lengths 0 to 70, every associated-data length 0 to 300, and a spread up to 16 KiB; `gcm_tag_diff`
  equal and with one bit changed; every wrong length, counter or round count a trap. Each output is followed by 32 guard
  bytes that must stay as they were.
- **`std.gcm`'s vectors** (FIPS 197, CAVP, Wycheproof; `conformance/gcm.rs`) on both paths and both backends, as before:
  the hardware path now runs them through the new builtins.
- **`scripts/gcm_differential.py`** against OpenSSL (pyca/cryptography) and the software path: 10,000 random cases and a sweep
  of **every message length 0 to 600, every associated-data length 0 to 300, and the lengths around 4 KiB and 16 KiB**, both
  directions, a flipped bit refused, 60,950 checks.
- **`scripts/gcm_ctgrind.sh`** (Valgrind's Memcheck, aarch64 Linux): the key and the message marked undefined, a seal and an
  open for AES-128 and AES-256 at 0, 64, 1,000 and 16,384 bytes: no report from the preparation or the seal, and exactly one
  from the open, the branch on whether the tag matched (§5). Then a sweep of **every length in buffers of exactly that size**
  for an `Invalid read` or `Invalid write`: an input read past its end, or an output written past it, which no answer shows.
- **`scripts/gcm_mutants.py`** (the cancho of `std/gcm.cho` and `std/aes.cho`: the hardware path's mutants follow the new
  code) and **`scripts/gcm_wide_mutants.py`** (the Rust that writes the IR: 61 mutants of the counter blocks, the cipher, the
  reduction, the powers, the tag, the tail and the call sites' checks, run on each instruction set).
- **`scripts/gcm_timing.py`**, the dudect test, at 10^6 measurements a test on x86-64 (|t| below 4.5), and
  **`scripts/gcm_branches.py`**, the object-code audit.
- **`scripts/aead_differential.py`, `scripts/tls_record_differential.py`**, the lying server's 84 recordings, and the TLS suites
  that negotiate AES-GCM (the interop matrix, the differential beside `openssl s_client`).
- **`cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`**, every `.cho` file
  checked by the new and the old compiler with the same answer (but the driver that uses `aes_ctr32`), the package stores
  republished (`scripts/publish_packages.py --check`), the selfhost tables regenerated.

## 8. Results

### 8.1 What a record costs, before and after, beside OpenSSL

`scripts/gcm_cost.py` (`tests/programs/gcm_cost.cho` built by the compiler before this change and after it, and
`openssl speed -evp aes-128-gcm aes-256-gcm`), a seal with a prepared key and 13 bytes of associated data, the best of five
runs of each in turn. MB/s are 10^6 bytes a second. **The machines were shared and the numbers move**, so they are marked:
the comparisons are made in the same minute, and on Linux x86-64 the cycles a byte (`perf stat`, user cycles, best of three) are
given beside them since they do not depend on the clock the governor picked.

**Apple M4 Max, macOS 26, load average 17 (other sessions' work), OpenSSL 3.6.4 (Homebrew):**

| AES-128-GCM seal | before | after | after / before | OpenSSL | after / OpenSSL |
|---|---|---|---|---|---|
| 64 B | 98 MB/s | 1,789 MB/s | **18.3** | 612 MB/s | 2.9 |
| 1,024 B | 674 | 6,391 | **9.5** | 5,658 | 1.13 |
| 16,384 B | 1,073 | 7,064 | **6.6** | 10,372 | 0.68 |

AES-256-GCM: 97 to 1,789 MB/s (18.4 times), 636 to 5,592 (8.8), 986 to 6,100 (6.2), against OpenSSL's 756, 5,046 and 8,833. An
open costs the same as a seal within 5%.

**aarch64 Linux, the same M4 Max in Docker (`lexsys-hooks-env`, 6 cores, load average 3), OpenSSL 3.0.13:**

| AES-128-GCM seal | before | after | after / before | OpenSSL | after / OpenSSL |
|---|---|---|---|---|---|
| 64 B | 554 MB/s | 2,236 MB/s | **4.0** | 3,960 MB/s | 0.56 |
| 1,024 B | 1,065 | 6,710 | **6.3** | 7,567 | 0.89 |
| 16,384 B | 1,137 | 6,710 | **5.9** | 7,999 | 0.84 |

AES-256-GCM: 528 to 1,917, 1,001 to 5,592, 1,048 to 6,100, against OpenSSL's 3,632, 7,569 and 7,993. (At 64 bytes `openssl speed`
on this build does not do an `EVP_CipherInit` for each message and the Linux x86-64 build below does; it is a different
measurement of the same name.)

**Linux x86-64, `ssh gram`: Intel Core i7-1260P (Alder Lake), one P-core thread (`taskset -c 7`) whose sibling thread another
user's work shared, the `powersave` governor (the clock moved between about 1.9 and 4.5 GHz during these runs), load average
4 to 8, OpenSSL 3.5.5.** The fastest of three sessions' best runs, MB/s, then cycles a byte (before / after / OpenSSL):

| AES-128-GCM seal | before | after | after / before | OpenSSL | cycles/byte before / after / OpenSSL | OpenSSL's cycles / ours |
|---|---|---|---|---|---|---|
| 64 B | 218 | 932 | **4.3** | 173 | 20.6 / 3.87 / 18.5 | 4.8 |
| 1,024 B | 341 | 3,728 | **10.9** | 1,220 | 8.69 / 1.02 / 2.00 | 2.0 |
| 16,384 B | 373 | 4,194 | **11.2** | 3,332 | 8.35 / 0.98 / 0.88 | 0.89 |

AES-256-GCM: 163 to 688 MB/s, 313 to 2,917, 338 to 3,441, against OpenSSL's 147, 1,287 and 2,963; cycles a byte 21.1 / 9.56 /
8.83 before and 4.27 / 1.20 / 1.09 after, OpenSSL's 19.3 / 2.24 / 0.99.

What these say:
- **16 KiB: 5.9 to 11.2 times faster** (the issue expected 2 to 4), 0.98 cycles a byte on x86-64 where today's was 8.35, **at 89%
  of OpenSSL's speed on x86-64 (by cycles), 84% on aarch64 Linux and 68% on the M4 under macOS**.
  The best single x86-64 run was 0.68 cycles a byte (OpenSSL's 0.55 in the same minute): the ratio of the two stayed between 1.1
  and 1.25 across every session, the clock and the neighbour moving both.
- **1 KiB: 6.3 to 10.9 times, at or above OpenSSL's `speed`** (113%, 89%, 200% on the three machines).
- **64 B: 4.0 to 18.3 times.** The cost of a small record is now a fixed 190 to 250 cycles on x86-64 and 30 ns on the M4, in
  two builtin calls with nothing allocated; OpenSSL's `speed` figure includes a context set-up per message on the x86-64 build
  (the 18.5 cycles a byte) and not on the aarch64 one.
- **Predicted (§3.5): 5 to 8 times at 16 KiB, 0.7 to 1.2 cycles a byte; 5 to 6 times at 1 KiB; about 4 times at 64 B on
  x86-64 and about 15 on the M4.** Measured: 5.9 to 11.2, 6.3 to 10.9, 4.3 to 5.3 and 18. The prediction held, and was
  conservative on x86-64.
- **Where the rest of the gap to OpenSSL is.** On the M4 a sampling profile of a 16 KiB seal is 60% GHASH and 40% counter mode.
  Both are two passes over the record, not the one stitched loop OpenSSL's `aes-gcm-armv8` and `aesni-gcm` run, in which the
  multiplier and the AES units work at the same time. Removing the reduction altogether on x86-64 (a measurement, not a
  build) gave 0.45 cycles a byte against 0.68, so the multiplies and the loads are most of what GHASH costs and a cheaper
  reduction (§3.2's, which made it 2% faster) could not have made up the difference. Karatsuba (three multiplies a block,
  not four) and stitching are the next steps and each is its own change (§9).

### 8.2 The record layer, before and after

`tests/programs/record_cost.cho`, `tls_record.seal` and `open` (TLS_AES_128_GCM_SHA256, a prepared key), one content type, ns a
record, before this change / after it:

| | 64 B | 1,024 B | 16,384 B |
|---|---|---|---|
| Apple M4 Max, seal | 1,235 / 605 | 2,120 / 750 | 16,113 / 3,200 |
| Apple M4 Max, open | 1,235 / 600 | 2,105 / 747 | 16,357 / 2,930 |
| gram (x86-64), seal | 377 / 150 | 3,723 / 396 | 53,710 / 4,394 |
| gram (x86-64), open | 398 / 146 | 2,624 / 366 | 38,085 / 3,906 |

A TLS record seals 2 to 12 times faster. **What dominates it now is the record layer, not the cipher** (§2.3): on the M4 a
64-byte record is 605 ns of which the cipher is 36, and a 16 KiB one 3.2 microseconds of which it is 2.3. The cost that
remains is a `region` (a 64 KiB `malloc` and `free`, slow on Darwin), the byte-at-a-time copy of the content into the inner
plaintext, and the nonce. Removing them needs an AEAD that seals in place (the builtin writing the ciphertext over the
plaintext), which is a change to `packages/tls/record.cho` and to the builtins' contract, and is §9's first item.

### 8.3 The gates

- **Known answers and the differential (§7):** the compiler's suite, on this Mac (aarch64) and on Linux x86-64 (gram, and CI's
  runner): all pass; `scripts/gcm_differential.py` over every length: **60,950 checks, 0 differences** on x86-64 (OpenSSL
  3.5.5) and 60,950 on aarch64 (OpenSSL 4.0.3), software path included; `aead_differential.py` 15,000 and 9,000 checks, 0
  differences; `tls_record_differential.py` 3,000 rounds on each, 0 differences; the lying server's 84 cases pass and its
  recording is byte-for-byte the committed one, on both. The interop matrix (`tls_interop.py`: Go, rustls, wolfSSL, mbedTLS,
  Botan, BoringSSL, nginx, GnuTLS) **115 rows ok, 0 failed** on aarch64 Linux, and `tls_differential.py`: 21 handshakes agree,
  0 differ; 59 agree, 17 differ in the alert only, 8 as documented, 0 otherwise.
- **Timing (`scripts/gcm_timing.py`, x86-64, gram, `taskset -c 7`, 64-byte message, 13 bytes of associated data):
  10^6 measurements a test, max |t| 2.36 (seal, fixed key against random), 1.87 (seal, data), 1.26 (open, data) and 1.48 (open,
  tag position), all under 4.5.** Beyond the issue's gate: 200,000 a test at 1,000 bytes (a length that ends inside a block and
  a group of eight): 1.54, 2.17, 1.47, 1.73; 10,000 a test at 16,384 bytes: 2.42, 1.47, 1.81, 2.79. On the M4 under macOS
  with `PSTATE.DIT` set by the LLVM backend's `main` (code this change does not touch): 200,000 a test, 2.58, 1.93, 1.18, 1.91.
- **Object-code audit (`scripts/gcm_branches.py`, x86-64):** 71 conditional jumps that do not go to a trap in `seal_hardware`,
  `open_hardware` and the six builtin functions (12, 11, 5, 23, 10, 0, 5, 5). Each is after a comparison of a register with
  a constant or a register, never of memory, and reading them (the group counts, `len & 127`, `len & 15`, the block count's
  bits, the round count 10, 12 or 14, the unrolled remainders of the rounds loop and of the tail's bytes, the slice bounds) none
  tests a byte of the key, the data, a counter block or a hash value, which are only operands of vector instructions. The
  script fails on a comparison of any other shape.
- **ctgrind (aarch64, Valgrind's Memcheck):** 0 reports from the preparation and the seal in 8 cases, 1 from each open (the
  tag branch); 0 invalid reads or writes over every length in exact-size buffers.
- **Mutants:** `gcm_mutants.py`: 34 of 34 killed on aarch64 and on x86-64 (the hardware path's 8 among them);
  `gcm_wide_mutants.py`: 61 mutants of the generated code (58 run on each instruction set: 55 shared and 3 of each), **59
  killed and 2 argued equivalent**, on aarch64 Linux and on x86-64. The two that no answer can show are the tail's
  whole-block loop counted wrong and the tail's byte loop started early: the byte loop finishes every byte the first leaves, with the
  same keystream, so only the work moves between them. A third that survived the conformance test, the 64-byte group
  counted for the 128-byte one (a wrong answer nowhere, a read and a write past the buffer), is what the guard bytes and
  Memcheck's sweep were added for.

## 9. Not done, and what fell short

1. **The record layer is now the cost of a TLS record** (§8.2). A change to `packages/tls/record.cho` that removes the `region`
   and the byte-at-a-time copy of the content wants the cipher to seal in place: `aes_ctr32` with the input and the output
   one slice (a contract the borrow rules cannot express as two arguments, so a builtin of its own or a `gcm.seal_in_place`),
   and the content type written after the content in the output buffer. It is a design of its own, in a package other work
   is changing, so it is not made here. Until it is, a 64-byte record costs 600 ns on Darwin whatever the cipher does, and
   `https_hello` and `tls_echo` (the 83 to 245 MB/s of the issue) are not remeasured here: they would show the record layer.
2. **Stitching AES and GHASH, and Karatsuba.** The two passes over a record leave the carry-less multiplier idle while the AES
   units work and the reverse; OpenSSL runs them in one loop. That is the 11% (x86-64, 16 KiB) to 32% (M4 under macOS) that
   separates this from OpenSSL, and §8.1 says where the M4's time goes (60% GHASH). Three multiplies a block for four, with
   the sums of the halves of each power in the table, would cut GHASH's multiplies by a quarter; both are measured options.
3. **ChaCha20-Poly1305 for machines without AES instructions** (the issue's second bullet) needs vector arithmetic; noted in §1,
   not decided.
4. **x86-64 under Valgrind.** `gram` has no Valgrind, so the ctgrind and addressability checks ran on aarch64 Linux only, with
   the same IR generator for the shared parts; x86-64's evidence is the dudect test (10^6 measurements a test), the object-code
   audit, and the conformance suite on a real x86-64 CPU (gram, and the CI runner).
5. **A prepared key is 224 bytes larger a connection** (`hw_len()` 256 to 368 bytes, in each of two directions;
   `packages/tls/slot.cho`'s offsets moved and the package stores were republished). Other changes to the slot will conflict
   with it mechanically; the offsets are `k_read_hw()`, `k_write_hw()` and `keys_len()`.
6. **Numbers on shared machines.** The M4 and `gram` were loaded by other work throughout (load averages 17 and 4 to 8) and
   `gram`'s governor moved the clock between 1.9 and 4.5 GHz; a quiet machine would show higher absolute figures. The
   cycles a byte on x86-64 are the steadier measure, and the ratio to OpenSSL's, taken in the same minute, stayed within
   0.8 to 0.9 at 16 KiB.
7. **The compiler's `cargo test --workspace` on Linux ran on CI's x86-64 runner**, which passed; the aarch64 Linux container
   this work used had a full disk and a host-mounted work directory, on which 13 filesystem tests fail (11 of them also
   on `origin/main` in the same container; the other two write `/tmp`, which was full). Nothing in them is near this change.
