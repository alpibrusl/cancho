# Multi-block AES-GCM and aggregated GHASH

> **Status: design (#382, part of #378).** `docs/crypto-builtins.md` gave `std.gcm` the hardware AES and carry-less multiply
> instructions, one block at a time through out-of-line builtin calls, and reduced GHASH after every block. It reached 0.56 to
> 1.1 GB/s against OpenSSL's 4 to 8. This document profiles where the time goes, says what to build, and computes what to
> expect from the measured parts. Where a later commit finds a claim here false, that commit corrects it here, in place.

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
- **Nothing secret is left behind.** The keystream tail buffer is overwritten before the function returns (volatile stores).
  The ciphertext copied for padding is public. The table of powers is part of the prepared key and `forget` overwrites it.
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

- **Known answers and a differential on every length, in the compiler's own suite** (`crates/cancho/tests/conformance/
  crypto_builtins.rs`, through `tests/programs/crypto_builtins_driver.cho`): FIPS 197, NIST's test case 2 from the builtins, and
  references written from FIPS 197 and SP 800-38D in the test: `aes_ctr32` on **every length 0 to 300** and with counters about
  to wrap; the eight powers; `gcm_tag` on every text length 0 to 300 with associated data of every length class and a spread
  up to 16 KiB; `gcm_tag_diff` equal and with one byte changed in each of the tag's 16 positions; every wrong length or round
  count a trap.
- **`std.gcm`'s vectors** (FIPS 197, CAVP, Wycheproof; `conformance/gcm.rs`) on both paths as before, and the hardware path now
  runs them through the new builtins.
- **`scripts/gcm_differential.py`** against OpenSSL (pyca/cryptography), now over **every message length 0 to 600 and the
  neighbourhood of 16 KiB**, associated data of every length class, 24,000 checks; run on x86-64 and aarch64.
- **`scripts/gcm_mutants.py`**, extended with mutants of the new generated code (the compiler's, in
  `crates/cancho-codegen-llvm/src/crypto/`) as well as of `std/gcm.cho`: all killed or argued equivalent.
- **`scripts/gcm_timing.py`**, the dudect test at 10^6 measurements on x86-64 (|t| below 4.5), and the object-code audit.
- **`scripts/aead_differential.py`, `scripts/tls_record_differential.py`**, and the TLS suites that negotiate AES-GCM.

## 8. Results

*To be filled in by the commits that build and measure it.*
