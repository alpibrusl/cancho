# SHA-256 on the CPU's instructions, and HMAC with the key's pads kept

> **Status: built.** Issues #445 (lever D3 of [`tls-performance.md`](tls-performance.md) §10.8: a SHA-256 builtin) and
> #446 (lever D4: HMAC with the key pads kept), part of #378 (TLS performance). §2 is what was measured before the design was
> fixed; §3 to §6 are the design; §7 is how it is tested; §8 is what it measured as built. The HMAC half is
> [`hkdf.md`](hkdf.md) §9, which owns `std.hmac` and `std.hkdf`; this document says only how the two halves meet. Where a later
> change finds a claim here false, it corrects it here, in place.

---

## 1. What is asked, and why

`std.crypto`'s SHA-256 is scalar code: [`tls-performance.md`](tls-performance.md) §10.2 measured it at 4.5 to 9.4 times
OpenSSL's cycles on x86-64 (SHA-NI) and 5.9 to 11.3 times its time on arm64 (ARMv8 SHA-2), and §10.4 found its compression
function at 7.3% of a full server handshake, 11.6% of a resumed one and 3.8% of a client's (self time, x86-64), because the
transcript and every HMAC of the key schedule run through it. §10.8 ranked a builtin at about 6.5 points of a full server
handshake and 10.3 of a resumed one (arithmetic, at OpenSSL's speed a block), and HMAC's per-call pad hashing at most 1.8 and
3.7 points.

Both CPUs this repository runs on have SHA-256 instructions: x86-64's SHA extensions (`sha256rnds2`, `sha256msg1`,
`sha256msg2`; `sha_ni` in `/proc/cpuinfo` on the i7-1260P of `tls-performance.md` §2.1) and aarch64's (`sha256h`,
`sha256h2`, `sha256su0`, `sha256su1`; `hw.optional.arm.FEAT_SHA256` reads 1 on the M4 Max). As with AES
([`crypto-builtins.md`](crypto-builtins.md) §1), the language cannot name them: its only vector-shaped data is a slice.

**Not asked for, and not done:** SHA-384 and SHA-512 (aarch64 has instructions for them with FEAT_SHA512, which the M4 reports;
the i7 of `tls-performance.md` §2.1 has none; and SHA-384 is 1.4 to 3 times OpenSSL already, §10.2), a vector type (§3.3), any call into C (the
builtins are pure, `[]`, as the AES ones are).

## 2. What was measured before the design was fixed

**LLVM emits the instructions from intrinsics, only with the feature on the function.** `llvm.x86.sha256rnds2`,
`llvm.x86.sha256msg1` and `llvm.x86.sha256msg2` with `"target-features"="+sha,..."` on x86-64, and
`llvm.aarch64.crypto.sha256h`, `sha256h2`, `sha256su0` and `sha256su1` with `+sha2` on aarch64, as the AES builtins do: compiled
by clang 21.1.8 on the i7, Apple clang 21.0.0 on the M4 and clang 18.1.3 in the arm64 VM, and the first test program
(FIPS 180-4's `abc` and two-block examples through both builtins) gave the standard's digests on all three on the first run.

**Cranelift cannot.** `cranelift-codegen` 0.121.2 has no SHA instruction, as it has no AES one (`crypto-builtins.md` §2).

**Valgrind does not have SHA-NI on x86-64, and does have the aarch64 instructions.** Measured with a C program that asks the
CPU and then runs the instruction: under Valgrind 3.26 on the i7, `cpuid` leaf 7's `ebx` bit 29 reads **0** (natively 1), and a
`sha256rnds2` reached anyway is an illegal instruction; under Valgrind 3.22 in the arm64 VM, `getauxval(AT_HWCAP)`'s
`HWCAP_SHA2` reads 1 and `sha256h` runs and answers what it does natively. Two consequences, both used below:
- **The regression guard counts instructions under Valgrind** (`tls-performance.md` §6), so on x86-64 it counts SHA-256's
  software path and on aarch64 its hardware path. Its `linux-x86_64` SHA rows can only move with the software code; its
  `linux-aarch64` ones move with the builtin (§8.3).
- **Valgrind is a CPU without SHA-NI** for an x86-64 program: the whole of `std` runs its software path on LLVM there, with no
  switch in the program (§7).

**Where a handshake's compressions come from** (counted by reading `std.hmac` and `std.hkdf`): an HKDF-Expand-Label of one
digest is four compressions, two of them the key's padded blocks and two the message and the inner digest, each a block that
`std.crypto` assembles in its partial-block buffer, not a block of the caller's bytes. The transcript hash ends the same way
(`sha256_final` on a copy). So the handshake's common case is **one block from the buffer**, and a design that only took
bytes would have to copy the buffer out into a byte slice first (§3.2).

## 3. The builtins

```cancho
hw_sha256() -> [] bool
sha256_blocks(state: &![int], data: &[byte]) -> [] int
sha256_block_words(state: &![int], block: &[int]) -> [] int
```

Edition 9, the latest open one ([`editions.md`](editions.md) §6.4: edition 7 is closed, and `sha256_blocks` is a name a
program may already declare for itself).

**`hw_sha256()`** answers whether this CPU has every instruction the other two use: on x86-64, SHA (`cpuid` leaf 7, sub-leaf 0,
`ebx` bit 29, asked only when leaf 0 says leaf 7 exists), SSSE3 and SSE4.1 (leaf 1, `ecx` bits 9 and 19: the shuffles and
blends around the rounds); on aarch64 Linux `HWCAP_SHA2` (`getauxval(AT_HWCAP)` bit 6), on aarch64 Darwin
`sysctlbyname("hw.optional.arm.FEAT_SHA256")`. Read once and cached in a module global with monotonic atomics, as
`hw_aes_gcm()` is (`crypto-builtins.md` §4). It is public information: branching on it is not a timing leak.

**`sha256_blocks(state, data)`** is FIPS 180-4 §6.2.2's compression of every 64-byte block of `data`, in order, into the eight
32-bit words `state` holds, one to an `int`: the low 32 bits of each are read, and the result is written zero-extended, so a
state of words in `0..2^32` stays one. `state` is exactly 8 words and `data` a whole number of blocks, none allowed, or it traps.

**`sha256_block_words(state, block)`** is the same compression of one block given as its sixteen big-endian words, the low 32
bits of each of sixteen `int`s: the form `std.crypto`'s partial block is kept in (§5). `state` 8 words, `block` 16, or it traps.

Both answer 0, and both trap on a target or a backend without the instructions; a program reaches them only after
`hw_sha256()` said true (§5).

### 3.1 Why a compression over a state and N blocks

- **It is the primitive every user here needs, and nothing more.** `std.crypto`'s streaming state, `std.hmac`'s keyed
  states (a key's padded block hashed once and resumed from its eight words, [`hkdf.md`](hkdf.md) §9) and the TLS transcript
  (copied and finished, `packages/tls/slot.cho`) all hold a state between calls; a builtin that hashed a whole message could
  serve none of them. The padding, the length and the buffering are lengths, which are public: they stay in cancho, where
  they are tested as they were.
- **N blocks a call, not one.** The state is loaded into two vector registers once a call and stays there across every block;
  a one-block builtin would load and store it each block and add a call. On a 16 KiB message that is 256 calls against one.
- **The state is eight `int`s, not 32 bytes,** because `std.crypto`'s states are `[int]` and are copied and resumed as
  such; the conversion is a truncation and a zero extension of a vector, inside the call.
- **A wrong length traps**, as an index out of bounds does and as `crypto-builtins.md` §9 Q1 decided for AES: the lengths are
  `std.crypto`'s, never the network's.

### 3.2 Why a second form for one block of words

`std.crypto`'s state is an `[int]` and cannot hold bytes, so a partial block lives in the state as words. With only
`sha256_blocks`, each buffered block (§2: the handshake's common case) would have to be written out into a byte slice, which
needs a region a call, and #446 is about removing regions a call. With `sha256_block_words` the buffered block is compressed
where it lies: `sha256_block_words(state[0..8], state[8..24])`. The two forms share one compression function in the backend
(`@cancho_sha256_rounds`), so the second costs a load of sixteen words and nothing else.

### 3.3 Not a vector type

`crypto-builtins.md` §9 Q3 answered "not now" for AES, and #445 asked to decide SHA-256 with the vector-type question for
ChaCha20 (`tls-performance.md` §4.2, §7 Q7). The answer is the same, for the same reason: these builtins go straight at a
measured cost with a small, testable change, and need nothing from the checker. A vector type is what ChaCha20's 6 to 11.5
times would need (§10.2), it would not make SHA-256 any faster than an intrinsic does, and it is its own design: the checker,
both backends, and how a vector meets checked arithmetic.

## 4. How the LLVM backend lowers them

`crates/cancho-codegen-llvm/src/crypto/sha256.rs`, beside the AES and GHASH code, with the same rules
(`crypto-builtins.md` §4):

- **Out-of-line functions carry the features** (`"+sha,+sse2,+ssse3,+sse4.1"` on x86-64, `"+sha2,+neon"` on aarch64), and
  LLVM will not inline them into a caller at baseline, so the rest of the program runs on a CPU without them. They are defined
  only in a module that calls them.
- **The call sites check the lengths** (`body/crypto.rs`) with the backend's trap instruction, then call
  `@cancho_sha256_blocks(state, data, n)` or `@cancho_sha256_block_words(state, block)`.
- **One block's rounds** are `@cancho_sha256_rounds`, inlined into both: sixteen groups of four rounds. On x86-64 the state is
  held as `ABEF` and `CDGH` (the instruction's order: the first word in the highest lane), each group two `sha256rnds2` on the
  group's words plus constants (the second on the high two lanes, shuffled down), and the schedule's group `g` is
  `msg2(msg1(W[g-4], W[g-3]) + (W[g-2] || W[g-1]) >> 4 bytes, W[g-1])`. On aarch64 the state is `ABCD` and `EFGH`, each group
  `sha256h` and `sha256h2` on the saved `ABCD`, and the schedule `su1(su0(W[g-4], W[g-3]), W[g-2], W[g-1])`. Then the
  feed-forward addition. Every operation is on a register; LLVM schedules them.
- **Loads and stores:** a block of bytes is four unaligned 16-byte loads and a byte swap in each 32-bit lane; a block of words is
  one load of sixteen `i64`, truncated. The state is one load of eight `i64`, truncated and shuffled into the ISA's form, and one
  store, shuffled back and zero-extended. That store is the only one either function makes (a unit test checks the text), so
  no word of a key's hash is written anywhere but the caller's state.

## 5. Cranelift, other targets, and `std`

- **Cranelift**: `hw_sha256()` is the constant false and the two builtins trap, as the AES ones do (`crypto-builtins.md` §5).
- **WebAssembly, and LLVM on any target but x86-64 and aarch64 Linux or Darwin**: the same, from the LLVM backend. The builtins
  are on WASI's side of the line (`docs/wasm.md`): they need no host, and a WASI program that calls `hw_sha256()` gets false.
- **`std.crypto`** branches on `hw_sha256()` where it compresses: `compress_bytes` (whole blocks straight from the caller's
  `data`, one `sha256_blocks` call for all of them) and `compress_block` (the partial block, `sha256_block_words`). Otherwise
  the software compression, unchanged. Which path runs is a question about the CPU, not the data.
- **The state's layout changed and its length did not.** The partial block was 64 `int`s, one byte each; it is now sixteen
  big-endian words at `state[8..24]`, every byte past the waiting ones zero (a byte is OR-ed into its word as it arrives, and
  the block is cleared after each compression). The count and the waiting bytes moved to `state[24]` and `state[25]`;
  `state[26..74]` is unused; the software schedule stays at `state[74..138]`. `sha256_state_len()` is still 138, so nothing laid
  out around a state moved (the TLS slot keeps two transcripts in its `ints`). The padding is a byte OR-ed in and two words
  written. Nothing outside `std.crypto` reads the layout.
- **Four new functions, for `std.hmac`:** `sha256_save(state, words)` and `sha256_resume(state, words, bytes)`, the eight
  words of a state at a block boundary and a state that goes on from them; `sha256_finish(state)`, the padding with the digest
  left as the running words; and `sha256_update_words(state, words)`, big-endian words fed at a word boundary. And the
  `sha512_` four, for SHA-384 ([`hkdf.md`](hkdf.md) §9).

## 6. Constant time

SHA-256's input is often secret here: an HMAC key, a PSK, every traffic secret of the key schedule, and the messages hashed
under them. The rule of `chacha20.md` §3 applies: no branch and no memory index may depend on a secret.

- **The builtins**: every loop bound is a block count and every branch is on a length; the state and the message are in
  registers from load to store; the instructions are not table lookups. Whether their timing depends on their operands is the
  CPU's. On aarch64 the runtime sets `PSTATE.DIT` where the operating system reports FEAT_DIT, as for AES
  (`crypto-builtins.md` §6); nothing is claimed here from either vendor's documentation. **It is measured** (§8.4).
- **`std.crypto`'s software path** is the code `hkdf.md` §3 audited, unchanged; the new buffer code shifts by `have`, which is
  a byte count, public.
- **`std.hmac`** branches on the key's length and the hash, as before. A prepared key is as secret as the key: `hmac.forget`
  clears one, `final` clears the outer words and the inner digest it kept, and every region these functions open is cleared when it ends
  (`region-chunk-cache.md` §3). Best effort, for `chacha20.md` §3.2's reasons.

## 7. How it is tested

- **Known answers, and a differential, on the builtins** (`crates/cancho/tests/conformance/sha256_builtins.rs`, through
  `tests/programs/sha256_builtins_driver.cho`): FIPS 180-4's `abc` and two-block examples padded by hand, through both forms; 400
  random states (one in three with bits above the 32nd set, which must be ignored) with 0 to 20 blocks each, 400 single blocks
  of words with high bits set, and a 16 KiB run, against the compression function written from FIPS 180-4 §6.2.2 in the test.
- **`hw_sha256()` must say what the CPU says**: true on aarch64; on x86-64 Linux, what `/proc/cpuinfo`'s `sha_ni` says. A silent
  fall back to software cannot hide, and neither can a probe that answers yes on a CPU that says no.
- **A wrong length traps** before any instruction runs: a state of 0, 7 or 9 words, data of 32, 63, 65 or 96 bytes, a block of
  15 or 17 words; and Cranelift answers false and traps.
- **`std` on both paths, without a switch**: the CAVP files (`SHA256ShortMsg`, `SHA256LongMsg`, `SHA256Monte` and the SHA-384
  and SHA-512 ones), the streamed-against-one-shot cases at every length to 300 in pieces of 1 to 129 bytes, and the 64 KiB and
  200 KiB messages now run on **both backends** in `cargo test` (`conformance/kdf.rs`): LLVM on the instructions, Cranelift in
  software. The same files, RFC 4231 and the KDF differential on **LLVM with the CPU's SHA-NI hidden**, under Valgrind on
  x86-64 (§2), are §8.1's.
- **Mutants**: `scripts/sha256_mutants.py` mutates the Rust that writes the IR (the rounds, the schedule, the constants, the
  lanes, the loads and stores, the probe, the call sites' checks) and `scripts/kdf_mutants.py` the three `std` files, each on
  both backends (§8.2).
- **The TLS suites**: RFC 8448's traces, the recorded handshakes, the lying peers, tickets, the pool's `tls.residue` and the
  interop scripts, unchanged.

## 8. As built

### 8.1 Correctness

- **`cargo test`** (the whole workspace on the M4 under macOS; its `sha256_builtins` and `kdf` tests also on the i7 under
  Linux, where the builtins are SHA-NI): the builtins' known answers and 800-case differential, the traps,
  `hw_sha256()` against the CPU (true on both), and `conformance/kdf.rs` (CAVP, RFC 4231, RFC 5869, the key schedules,
  Wycheproof, the prepared-key rows) on both backends. The TLS conformance tests (RFC 8448's traces, every recorded handshake
  replayed byte for byte on both backends, the lying servers and clients, tickets, the pool's `tls.residue` 0) pass unchanged.
- **The KDF differential**, `scripts/kdf_differential.py`, twelve checks a round: **100,000 rounds, 1,200,000 checks, 0
  differences** on x86-64 (LLVM, SHA-NI); 20,000 rounds (240,000 checks) on arm64 Linux (LLVM, ARMv8 SHA-2) and 1,000 on
  Cranelift there, 0 differences.
- **With SHA-NI hidden** (Valgrind on the i7, §2): `scripts/kdf_valgrind.py` runs `kdf_mutants.py`'s evidence (the vector table
  and its prepared rows, the CAVP short-message and Monte Carlo files, every Wycheproof HMAC and HKDF case, the streamed cases,
  the refusals) and the CAVP long-message files through the LLVM driver: **8,373 cases, 0 wrong**; and 300 differential rounds
  (3,600 checks), 0 differences. ([raw](assets/sha256-builtin/x86-64/kdf-valgrind-and-differential.txt))
- **The TLS scripts**, in the arm64 VM against the final code: `tls_liar.py` 84 cases and `--streams` 64, `tls_tickets.py` 20,
  `tls_server_interop.py openssl` 23 rows and `tls_server_tickets_interop.py openssl` 9, all as expected;
  `tls_differential.py` 59 agree, 17 differ in the alert only, 8 as documented, 0 otherwise, and `tls_server_differential.py`
  65, 7, 6: the counts they had (raw: [`assets/sha256-builtin/arm64/`](assets/sha256-builtin/arm64/)).

### 8.2 Mutants

- **`scripts/kdf_mutants.py`: 34 of 34 killed** on arm64 Linux, each mutant run on both backends
  ([raw](assets/sha256-builtin/arm64/kdf-mutants-linux.txt)). Two of the software path's (a round adding `temp1` twice, the
  partial block's words not copied to the schedule) are killed only by the Cranelift build, which is why the script now builds
  both: on a CPU with the instructions the LLVM build never runs that code.
- **`scripts/sha256_mutants.py`** (the IR): §8.6 has the tally of the run on the final tree.

### 8.3 The regression guard

`scripts/tls_perf.py check` counts instructions under Valgrind, so (§2) its `linux-x86_64` rows count the **software** path
and its `linux-aarch64` rows the **hardware** path:
- **`linux-aarch64`** failed nine rows as cheaper than 1/1.3 (SHA-256 at 0.02 to 0.07 of its count, HMAC 0.02 to 0.12, HKDF
  0.16 and 0.18) and the new row `hkdf_expand_label_prepared_32`; re-recorded in the VM, as §1.3 says, and then 34 of 34.
- **`linux-x86_64`** passed 33 of 34 rows with the software path's counts 0.86 to 1.00 of the baseline (the partial-block words
  and D4) and lacked only the new row; it is re-recorded on the i7 for that row (§8.6).
- The CI runner's guard therefore cannot see the x86-64 builtin: a regression in it would show on the arm64 entry, and in
  the cycles of §8.5, not in the x86-64 entry.

### 8.4 Timing

`scripts/hmac_timing.py` (dudect's fixed-against-random test, `scripts/gcm_timing.py`'s statistic) over
`tests/programs/hmac_timing.cho`, on the i7 pinned to CPU 4, **10^6 measurements a test**, the fixed class an all-zero key or
message: `hmac.sha256` with a secret key and with a secret message, `hmac.prepare`, `hmac.mac_prepared` with a secret prepared key
and with a secret message, `crypto.sha256`, and `hkdf.expand_label` with a secret secret. Two runs, and between them the same
tests with both classes random (`--control`), raw in [`assets/sha256-builtin/x86-64/dudect-hmac-1e6.txt`](assets/sha256-builtin/x86-64/dudect-hmac-1e6.txt):

| test | run 1 max \|t\| | run 2 | control |
|---|---|---|---|
| hmac, key | 1.93 | 2.12 | 1.29 |
| hmac, message | 1.24 | 1.98 | 1.49 |
| prepare, key | 2.67 | 1.92 | 1.77 |
| prepared, key | 1.76 | 1.64 | 1.43 |
| prepared, message | 1.92 | 2.83 | 1.82 |
| sha256, message | 3.24 | 1.88 | 1.86 |
| expand-label, secret | 2.54 | 1.75 | 2.22 |

All under 4.5. **Two findings on the way, both in this change:**

1. **`hmac.final` first used the caller's `out` as scratch** (the inner digest written there and hashed back from it). With a
   fixed message, `mac_prepared` failed: |t| 11 to 58 at 2 x 10^5 to 10^6. Bisected: `init_prepared` alone and with `update`
   passed, adding `final` failed; a fixed non-zero message failed as the all-zero one did (|t| 29) and random against random
   passed; and clearing `out` before each measurement, outside the clock, made it pass. So the time depended on
   whether the caller's buffer already held the bytes being written: the CPU's, not the code's, but the code chose to write
   the inner digest there. `final` now keeps the inner digest as words in the state and writes nothing to `out` but the tag
   (`hkdf.md` §9.2).
2. **The harness itself measured where a record lies.** With each class's records in its own half of the pool, `prepared,
   key` still failed (|t| 17 to 32), and so did a control with both classes random (|t| up to 11): a difference of under one
   cycle in 780, with a sign that changed from run to run. Interleaving the classes did not cure it (control |t| 6 to 8.7);
   copying each measurement's record into the same three buffers before the clock starts did (control 1.3 to 2.6). Every
   figure in the table is from that harness. `scripts/gcm_timing.py`'s pooled mode lays its classes out the old way; its
   results were not re-examined here (#468).

**Not measured:** a timing test on aarch64 at 10^6 (a run of 2 x 10^4 a test on the M4 under macOS, with the first harness,
gave max |t| 1.1 to 2.4), the Cranelift build, and anything under DOITM (Intel's data-operand-independent-timing mode, which this
shared machine does not set).

### 8.5 Cost

Before is `origin/main` at `3d7e8e9d` (A1 built), after is this change, built from the same commit beside each other and run
alternately on the machines of [`tls-performance.md`](tls-performance.md) §10.1: x86-64 the i7-1260P (`gram`), **pinned to
CPUs 4 and 5** (one P-core's two threads; load average 4 to 7 from other work on the other cores), cycles and instructions
from `perf stat`; arm64 the M4 Max's Docker VM, CPU time. OpenSSL beside each, on the same core: 3.5.5 on x86-64, 3.0.13 in
the VM. Raw output: [`assets/sha256-builtin/`](assets/sha256-builtin/) (`x86-64/measure-final.txt` and
`arm64/perf-before-after.txt` are the final code's; `x86-64/primitives-before-after.txt`, `handshakes-before-after.txt` and
`arm64/perf-first-run.txt` an earlier run with the first `hmac.final` of §8.4, which agree with them to within a few percent
on every SHA-256 row and handshake but the client's; OpenSSL's arm64 figures are from that first run, in the same VM).

**Each primitive** (`scripts/tls_perf.py table`, the least of two passes; `scripts/tls_perf.py openssl`):

| operation | bytes | x86-64 cycles: before → after | instructions | OpenSSL | **ratio before → after** | arm64 ns: before → after | OpenSSL | **ratio** |
|---|---|---|---|---|---|---|---|---|
| SHA-256 | 64 | 2,565 → **440** | 15,039 → 977 | 562 | 4.56 → **0.78** | 557 → **141** | 96.3 | 5.78 → **1.46** |
| SHA-256 | 1,024 | 20,829 → **2,406** | 121,884 → 3,437 | 2,536 | 8.21 → **0.95** | 4,425 → **455** | 480.5 | 9.21 → **0.95** |
| SHA-256 | 16,384 | 332,622 → **33,857** | 1,831,404 → 42,797 | 34,001 | 9.78 → **1.00** | 66,284 → **6,576** | 5,526 | 12.0 → **1.19** |
| HMAC-SHA-256 (not like for like, `tls-performance.md` §3.1) | 64 | 6,627 → **1,157** | 39,759 → 3,729 | 717 | | 1,445 → **202** | 125.3 | |
| HKDF-Expand-Label, 32-byte secret | | 5,641 → **1,314** | 33,504 → 4,881 | | | 1,205 → **232** | | |
| the same, secret prepared (new row) | | **677** | 2,589 | | | **155** | | |
| HKDF-Extract | | 5,504 → **1,107** | 33,014 → 4,099 | | | 1,186 → **211** | | |
| HKDF-Expand-Label, SHA-384 (D4 only) | | 6,079 → 5,825 | 33,643 → 32,178 | | | 1,277 → 1,434 | | |
| ECDSA P-256 sign (RFC 6979's HMAC) | | 320,668 → 297,549 | 1,632,016 → 1,461,814 | | | 53,222 → 46,997 | | |

- **SHA-256 is at OpenSSL's cost on x86-64** (0.78 to 1.00 of its cycles) and 0.95 to 1.46 of its time on arm64: 6 to 10
  times cheaper than before. At 64 bytes what is left is `std.crypto`'s byte loop into the partial block and the padding; at
  16 KiB it is the instructions.
- **An Expand-Label is 4.3 times cheaper (x86-64), and 8.3 times with its secret prepared.** The prepared form saves the two
  pad compressions and two regions; what is left of its 2,589 instructions is its region's clearing, the byte loops that build
  `HkdfLabel` and fill the partial block, and the length checks.
- SHA-384 does not move (no instruction is used for it, §1); its Expand-Label is 4% cheaper in instructions (the arm64 time
  moved the other way, inside the VM's spread). The ECDSA signature is 7 to 12% cheaper because RFC 6979's nonce is an
  HMAC-DRBG.

**Handshakes** (`scripts/tls_handshake_cost.py <bins> --seconds 8 --rounds 2 --cpu 4 --peer-cpu 5 --sudo-perf`, two passes of
each tree alternating, the least of four rounds; arm64 the same with `--cpu 0 --peer-cpu 1`, CPU a handshake). OpenSSL is the
least of its eight rounds; it did not move between the trees.

| | x86-64 cycles: before → after | instructions | **change** | ratio to OpenSSL | arm64 CPU: before → after | **change** | ratio |
|---|---|---|---|---|---|---|---|
| server, full | 1.848M → **1.636M** | 8.09M → 6.97M | **−11.5%** | 1.86 → **1.65** `s_server` (`oserv` 1.75) | 0.297 → **0.262 ms** | **−11.8%** | 1.59 → **1.40** |
| server, resumed | 0.889M → **0.644M** | 3.31M → 2.14M | **−27.6%** | 1.04 → **0.76** | 0.152 → **0.111 ms** | **−27.0%** | 1.10 → **0.80** |
| client, full | 2.660M → **2.540M** | 10.80M → 10.00M | −4.5% | 1.64 → **1.57** `tconn` | 0.423 → **0.389 ms** | −8.0% | 1.49 → **1.37** |
| client, resumed | 0.954M → **0.827M** | 3.18M → 2.28M | −13.3% | 1.15 → **0.99** | 0.172 → **0.151 ms** | −12.2% | 1.40 → **1.23** |

- **A resumed handshake is now cheaper than OpenSSL's on the server** (0.76 of its cycles on x86-64, 0.80 of its time on arm64),
  and level with it on the x86-64 client.
- The client rows are the difference of two `tls_many` runs and move most between rounds (2.54 to 3.36M); the earlier run of
  §8.4's first `final` measured the client's resumed handshake 23.6% cheaper, this one 13.3%. Instructions do not move with the
  clock: −7.4% and −28.1%.

**Against the projection** (`tls-performance.md` §10.8, arithmetic on the profile before A1): D3 and D4 together were to save
6.5 + 1.8 = 8.3 points of a 2.83M-cycle full server handshake (0.235M cycles) and 10.3 + 3.7 = 14.0 points of a 2.00M resumed
one (0.280M). Measured on the handshake A1 left: **0.212M and 0.245M cycles**, 90% and 88% of the projected cycles, and a larger
share (11.5% and 27.6%) because A1 removed a third of what they were a share of.

### 8.6 The guard re-recorded, and the mutants of the IR

- **`linux-aarch64`**, re-recorded in the VM on the final code: 34 of 34; `scripts/tls_perf_mutant.py` fails exactly its four
  rows. **`linux-x86_64`**, re-recorded on the i7 for the new row (every other row within 0.86 to 1.00 of the old entry, §8.3):
  34 of 34, and the guard's mutant fails exactly its four rows.
- **`scripts/sha256_mutants.py`**: on the M4 (macOS), 23 of 23 run there killed (the rounds, the schedule, the constants, the
  aarch64 lanes, the loads and stores, the Darwin probe, the cache, every length check) ([raw](assets/sha256-builtin/arm64/sha256-mutants-macos.txt)); in the arm64 Linux VM the `HWCAP` probe's mutant, killed (an
  earlier form of it, `HWCAP_AES` for `HWCAP_SHA2`, survived there because that CPU has both, and was replaced by one the CPU
  does not have); on the i7 the x86-64 ones, 10 of 10: eight killed (the schedule's `msg1` and `msg2`, the rounds' operands and lanes, the ABEF and CDGH lanes both ways, the SHA bit of the probe) and two argued equivalent and run all the same (SSE4.1 not asked for, and leaf 7 asked whatever the highest leaf: no CPU with SHA lacks either) ([raw](assets/sha256-builtin/x86-64/sha256-mutants.txt)). The mutants common to both instruction sets were run on the M4.

## 9. Not done

- **SHA-384 and SHA-512** on instructions (§1).
- **`std.crypto`'s state could shrink** from 138 words to 90 (the 48 unused words), which would make every transcript copy a
  third cheaper; it would move the TLS slot's layout, so it is left for a change that measures it.
- **Cranelift** keeps the software path (§5).
