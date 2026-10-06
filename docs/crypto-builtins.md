# Hardware AES and carry-less multiply: two builtins, and a way to ask whether the CPU has them

> **Status: design, decided; the DIT bit and the builtins built.** §9's five answers were accepted as proposed (2026-10-06),
> and the four PRs of §10 follow in that order: the DIT bit (§6) and the builtins with their LLVM lowering (§3 to §5, as built
> below) are done; per-key caching and the `std` change are next. `docs/tls-parity.md` §3.1 measured AES-GCM at about 170 times slower than OpenSSL's, and said the gap
> is the instructions lex-sys cannot emit. This document says what it would take to emit them, what is measured so far, and
> what is not. Where a later PR finds a claim here false, that PR corrects it here, in place.

---

## 1. What is asked, and why

`std.aes` and `std.gcm` are constant-time in software: bitsliced AES (BearSSL's `aes_ct`) and GHASH by masked integer
multiplies. That is the safe software choice, and it is slow (`docs/tls-parity.md` §3.1, one core of an x86-64 Xeon):

| Message | AES-128-GCM here (LLVM) | `openssl speed -evp aes-128-gcm` |
|---|---|---|
| 16,384 bytes | 32.6 MB/s | 5,546 MB/s |
| 64 bytes | 12.8 MB/s | 1,356 MB/s |

OpenSSL uses AES-NI and PCLMULQDQ on x86-64. The same operations exist on aarch64 (`AESE`, `AESMC`, `PMULL`), which is the
other target here. They are both fast and, by design, take a time independent of their operands. A 16 KiB TLS record costs
about half a millisecond to seal here, and a client that is offered AES-GCM only pays that on every record.

**Not asked for:** a vector type, SIMD for ChaCha20 (its gap to OpenSSL is 17 times, and it needs vectors, not special
instructions: `docs/chacha20.md` §6), or any call into C. #210's gate (`docs/tls-pure.md` §9, gate 2) needs the pure backend to have
no `Ffi`; the builtins here are pure (`[]`), as `value_barrier` is.

## 2. What is measured, and what it decides

Each result is from the machine named. The rest of this document is a design built on them.

**Cranelift cannot emit them.** Neither `cranelift-codegen` 0.121.2 (the version pinned in
`crates/lex-sys-codegen/Cargo.toml`) nor 0.132.0 mentions `aesenc`, `aesdec`, `pclmulqdq`, `aese`, `aesmc` or `pmull64`
anywhere in `src/` or `meta/`. So on the Cranelift backend the builtins have no lowering, and §5 gives them an answer that
keeps today's code.

**LLVM can, and only with the target feature on.** Compiled with `clang -O2 -c` from hand-written IR on an Apple M4 Max
(Homebrew's LLVM, targets named with `-target`):

| Target | Features | Result |
|---|---|---|
| `aarch64-unknown-linux-gnu` | baseline | **fails**: `Cannot select: AArch64ISD::PMULL` |
| `aarch64-unknown-linux-gnu` | `+aes` | `aese`, `aesmc`, `pmull` |
| `x86_64-unknown-linux-gnu` | baseline | **fails**: `Cannot select: intrinsic llvm.x86.pclmulqdq` |
| `x86_64-unknown-linux-gnu` | `+aes`, `+pclmul` | `aesenc`, `aesenclast`, `pclmulqdq` |

The intrinsics used were `llvm.aarch64.crypto.aese`, `llvm.aarch64.crypto.aesmc`, `llvm.aarch64.neon.pmull64`,
`llvm.x86.aesni.aesenc`, `llvm.x86.aesni.aesenclast` and `llvm.x86.pclmulqdq`. (A first attempt without `-target` compiled the
aarch64 file for the host and said nothing about x86, which is why §2 names the targets.)

**What that decides:**
- **The feature must be on per function, not per module.** Turning `+aes` on for the whole module would let `clang` use it
  anywhere, and the binary would fault on a CPU without it. A `"target-features"="+aes,+pclmul"` attribute on only the
  functions that contain the intrinsic keeps the rest of the module at baseline.
- **Whether to take the hardware path is a run-time answer, not a build-time one.** A program built on one machine runs on
  others (`scripts/package-release.sh` builds for a target, not for a CPU).

**This CPU has them:** `hw.optional.arm.FEAT_AES`, `FEAT_PMULL` and `FEAT_DIT` all read 1 on the M4 Max of
`docs/tls-assurance.md` §6.1.

**Not measured:** how fast the hardware path would be in lex-sys. §8 sets that as a gate for the `std` PR, not a claim.

## 3. The builtins

lex-sys has no 128-bit value: the scalar types are `int` (64-bit), `byte`, `bool` and `float`. An AES block and a GHASH
product are 128 bits. Two ways round that were considered:

| Option | What it is | Verdict |
|---|---|---|
| **Block-level builtins over byte slices** | the compiler loads the block into a vector register, does the whole operation there, and stores it | **chosen**: no new type, and no round-trip through memory between rounds |
| A vector type (`v128`) with `aes_round`, `clmul` | a language change: the checker, both backends, arithmetic rules, and how a vector meets checked arithmetic | the general answer, and the one ChaCha20's SIMD would need. Its own design, later |

The builtins, as `edition N;` (the latest, 6 today, as `value_barrier` was: `docs/editions.md` §5, a name a program could
already declare). *As built: edition 7, the latest by then.*

```lex-sys
hw_aes_gcm() -> [] bool
aes_encrypt_block(round_keys: &[byte], rounds: int, block: &[byte], out: &![byte]) -> [] int
ghash_update(h: &[byte], y: &![byte], data: &[byte]) -> [] int
```

**`hw_aes_gcm()`** answers whether this CPU has everything the other two need: on x86-64, AES, PCLMULQDQ and SSSE3 (the
byte-swap GHASH needs); on aarch64, AES and PMULL. It reads the CPU once and caches the answer in a module-local. It is public
information, so branching on it is not a timing leak.

**`aes_encrypt_block(round_keys, rounds, block, out)`** is the FIPS-197 cipher: `round_keys` is the expanded key in its
standard form (`rounds + 1` blocks of 16 bytes, `rounds` 10 for AES-128 and 14 for AES-256), `block` is 16 bytes, `out` is 16
bytes. The instructions differ in where they place AddRoundKey (`AESE` first, `AESENC` last), and the lowering handles that:
the program sees the one standard layout on both targets.

**`ghash_update(h, y, data)`** is `y = (y XOR block) * h` in GF(2^128) for every 16-byte block of `data`, in GCM's bit
order, with `y` updated in place. The reduction stays in registers. It takes `data` whole, and `std.gcm` does its own
padding and length block as it does now.

**Key expansion stays in software.** x86 has `AESKEYGENASSIST` and aarch64 has nothing equivalent, so one portable key
schedule (the existing bitsliced `SubWord`) is used on both paths. It runs once per key, and §6 moves it to once per connection.

**What a wrong argument does.** A slice of the wrong length, or `rounds` other than 10, 12 or 14, **traps**, as an
out-of-bounds index does; it never reaches an instruction. No *input* may reach that trap (CLAUDE.md), and none can: the
lengths are fixed by the suite, not read from the network, and `std.gcm` already refuses a bad key or nonce length with its
rule tag (`refused_key_length`) before it would call the builtin. The trap is for a caller's bug, which the fuzz harnesses
(`docs/tls-assurance.md` §3) would report as a crash.

## 4. How the LLVM backend lowers them

`crates/lex-sys-codegen-llvm/src/body/expr.rs` already lowers `value_barrier` to inline assembly. These three lower to
intrinsics instead, which LLVM can schedule:

- **The functions that contain them carry `"target-features"="+aes,+pclmul,+ssse3"`** (x86-64) or `+aes` (aarch64). Because LLVM
  refuses to inline a function with more features into one with fewer, the lowering is an out-of-line function per builtin,
  called directly, and the feature never reaches the caller.
- **aarch64 `aes_encrypt_block`** loops `aese` then `aesmc` for all but the last round, then `aese` and an XOR with the final
  key. **x86-64** loops `aesenc` and ends with `aesenclast`. Both are checked against FIPS-197's vectors (§7).
- **`ghash_update`** is four carry-less multiplies a block and a reduction by the GCM polynomial, on both targets. The
  aarch64 form byte-reverses with `rev64` and `ext`; the x86-64 form with `pshufb`. *As built
  (`crates/lex-sys-codegen-llvm/src/crypto.rs`): the instruction is used only for the four 64-by-64-bit products; the bit
  order (each byte's bits reversed, into the polynomial's natural order) and the reduction (the high half folded twice by
  `x^7 + x^2 + x + 1`) are LLVM `i128` and `i256` arithmetic, every shift by a constant. Simpler to check than a hand-scheduled
  register version, and it leaves the choice of instructions to LLVM. Its speed is the `std` PR's measurement.*
- **`hw_aes_gcm()`:**
  - x86-64: `cpuid` leaf 1, `ecx` bits 25 (AES), 1 (PCLMULQDQ) and 9 (SSSE3), by inline assembly, as `value_barrier` is.
  - aarch64 Linux: `getauxval(AT_HWCAP)`, bits `HWCAP_AES` and `HWCAP_PMULL`.
  - aarch64 macOS: `sysctlbyname("hw.optional.arm.FEAT_AES")` and `FEAT_PMULL`.
  - the last two are libc calls. Whether the backend already links them, and whether a builtin that calls libc is still `[]`,
    are not checked here, and §9 asks the second. *Checked when built: the DIT start-up (§6) already declares `getauxval` and
    `sysctlbyname` on those targets, and every program links libc; §9 answered the second yes. The answer is cached in a
    module global, read and written with monotonic atomics.*
- *As built, the length checks are at the call site, with the backend's own trap instruction, and the out-of-line functions
  are defined only in a module that calls them.*

## 5. The Cranelift backend

No instruction exists (§2), so:
- **`hw_aes_gcm()` answers `false`.** `std.aes` and `std.gcm` then take the software path, which is today's code, unchanged.
- **`aes_encrypt_block` and `ghash_update` are never reached by a correct program on Cranelift.** If one is, they answer the
  software result (an interpreter for the same function in the backend's runtime), so a program is never wrong, only slow. That
  keeps the two backends agreeing, the rule `docs/tls-pure.md` §9 gate 1 states. *Corrected when built: they trap instead, as
  `trap()` does (and so does the LLVM backend on a target without the instructions, WebAssembly). The software result would
  have been AES and GHASH written a third time, in the backend, for code no correct program reaches: `std.aes` and `std.gcm`
  call the builtins only after `hw_aes_gcm()` answered true, which on Cranelift it never does. The two backends still agree
  on every program that asks first, and the differential of §7 compares the hardware path with the software one inside one
  LLVM program, where both run.*

**A TLS build should use LLVM.** That is already the default backend. Cranelift's AES-GCM is also the one place its timing test failed
with Arm's data-independent-timing bit set (`docs/tls-assurance.md` §6.1).

**What this does not fix:** Cranelift's AES-GCM is slower than LLVM's (12.1 against 7.1 µs for a 64-byte seal on the M4, 3.1 times
at 16 KiB on the Xeon), and its data tests failed their timing test on the M4
(`docs/tls-assurance.md` §6.1). The hardware path does not reach Cranelift, so that result stands.

## 6. The code that uses them, and what goes first

Two changes need no builtin and come first, each its own PR, because each helps the software path that Cranelift and a CPU
without the instructions will keep running, and because the builtins' gain should be measured against that improved baseline:
- **Arm's data-independent-timing bit.** Setting `PSTATE.DIT` in the runtime's start-up is a few lines. On the M4 it removed
  the ECDH timing failure on LLVM (|t| 16.19 to 1.84 and 10.40 to 2.15) and most of AES-GCM's on Cranelift
  (`docs/tls-assurance.md` §6.1). What it costs in speed is not measured here, and the DIT PR measures it.
  *Done (the first PR after this design): the LLVM backend's `main` sets it on aarch64 Linux and Darwin when the OS reports
  FEAT_DIT (`crates/lex-sys-codegen-llvm/src/dit.rs`). Measured: P-256 with scalar 1 fails without it (|t| 9.11) and passes
  with it (2.11); the symmetric ciphers cost 3 to 6% more (`docs/tls-assurance.md` §6.1). Two things this paragraph did not
  know: a Darwin thread starts with DIT clear, so only `main`'s thread has it there (a Linux thread inherits it), and
  Cranelift has no inline assembly, so its programs do not set it.*
- **Per-key work once per connection** (step 1 below).

`std.aes` and `std.gcm` then change in three ways, in this order of risk:
1. **Per-key work once per connection.** The key schedule, and `H = AES(0)`, are 25% and 14% of a 64-byte seal today
   (`docs/tls-parity.md` §3.1). They depend only on the key, so the record layer keeps them. This helps the software path too,
   and is a change of its own.
2. **A branch on `hw_aes_gcm()`** at the top of `seal` and `open`. Both paths produce the same bytes, and a test says so.
3. **The software path stays, whole.** It is what Cranelift runs, what a CPU without the instructions runs, and what the
   differential test checks the hardware path against.

## 7. How it is tested

- **Known answers, on both paths and both backends:** FIPS-197 Appendix B and C (AES-128 and AES-256), NIST's GCM test vectors,
  and the vectors `std.gcm` already has.
- **A differential:** the hardware path against the software path over random keys, blocks and lengths, which is a test
  neither backend alone can give.
- **Forcing the software path** on a machine that has the instructions, so CI covers both: `std.aes` and `std.gcm` keep their
  software functions public, and the differential calls each path directly. There is no switch, because a switch that changes
  the path at run time could ship in a release by accident.
- **Both CI runners:** linux-x86_64 and darwin-aarch64 run the hardware path. A runner without the instructions would show up as
  `hw_aes_gcm()` answering `false`. A test asserts `true` on the two CI runners, once the builtins PR has measured that they answer it, so a
  silent fallback cannot hide.
- **The timing test, rerun** (`scripts/gcm_timing.py`), on both paths. The hardware instructions are specified as
  data-independent, but this CPU's own result (`docs/tls-assurance.md` §6.1) is the reminder that the specification and the
  machine can disagree, so it is measured.
- **Mutants:** at least 12 killed, as every package here has (`scripts/tls_mutants.py` is the form).
- **The record-layer corpus:** once #272 is merged, its fuzz corpus (`conformance/tls_fuzz.rs`) and the recorded handshakes
  replay on the new path.

## 8. Gates

The numbers the `std` PR must show, each with its command, and an honest "not met" if it is not:
- **Speed:** the 16 KiB and 64-byte rows of §1, on the same two machines as `tls-parity.md` §3.1 and
  `tls-assurance.md` §6.1. No target is claimed here. The measurement decides how far `tls-parity.md` §3.1's "OpenSSL is about
  170 times faster" is corrected, in place.
- **Correctness:** §7's known answers and differential, on both backends.
- **No new capability:** `lex-sys authority` on the pure backend shows no `ffi(...)`, as #210 requires.
- **The gate:** `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, and no source
  file over 2,000 lines.

### 7.1 Results (the builtins PR)

`crates/lex-sys/tests/conformance/crypto_builtins.rs`, through `tests/programs/crypto_builtins_driver.ls`:
- **Known answers:** FIPS 197 Appendix C.1 and C.3, and NIST's GCM test case 2 assembled from the two builtins (`H`,
  `AES(K, J0)` and the tag each the standard's value).
- **A differential:** 600 random AES blocks (AES-128, -192 and -256) and 600 random GHASH runs of 1 to 12 blocks against plain
  reference implementations written from FIPS 197 and SP 800-38D in the test. Before it, by hand: 603 cases against Python
  (`cryptography` for AES, SP 800-38D's algorithm for GHASH) on an Apple M4 and on aarch64 Linux, all equal.
- **`hw_aes_gcm()` is true** on the suite's hosts with LLVM, asserted, and **false on Cranelift**, where a block builtin traps.
- **A wrong length or round count traps** before any instruction runs, and writes nothing.
- **x86-64** is checked by CI's linux-x86_64 runner, which runs the same test; it was not run by hand.

## 9. Open questions, and the answers proposed

Each is a decision for a person. The answer given is the one this PR proposes, with its reason; a reviewer who disagrees changes
the answer here before anything is built. *Decided: all five as proposed (2026-10-06).*

1. **A bad length: a trap or a return code?** *A trap.* No attacker's bytes reach it (§3), the key length is refused earlier with
   a rule tag, and a return code would add a check that can only fail on a programming error.
2. **Is a builtin that reads the CPU `[]`?** *Yes, as `value_barrier` is.* Its answer is the same for the life of the process
   and nothing a program does can observe the read. A capability would cost the pure backend its empty row (#210's gate) for
   nothing. The builtins PR must confirm that the backend already links the libc calls aarch64 needs (`getauxval`, `sysctlbyname`);
   x86-64's `cpuid` needs none.
3. **A vector type instead?** *Not now.* The block builtins go straight at the AES gap with a small, testable change. A vector
   type is the answer if ChaCha20's 17-times gap starts to matter, and it is a larger design of its own: the checker, both
   backends, and how a vector meets checked arithmetic.
4. **Does a Cranelift-only user accept the software speed?** *Yes, and a TLS build should use LLVM* (§5), the default.
5. **How the tests force the software path.** *They do not switch it: both paths stay public and are called directly* (§7).
   This replaces the first draft's environment variable read in a test build.

**Order proposed** (§6): this design merged with these answers, then Arm's DIT bit, then per-key caching, then the builtins and
the LLVM lowering, then the `std` change and the speed measurement against OpenSSL.

## 10. Not done here

- **Any code.** This document is the design. After it come four PRs, in §9's order: Arm's DIT bit, per-key caching, the builtins
  with their lowering, and the `std` change.
- **ChaCha20 SIMD** (`docs/chacha20.md` §6).
- **AES-CBC, AES-CCM or any mode but GCM.** The block builtin would serve them; nothing here uses it.
- **SHA instructions** (`SHA256H`, `SHA256RNDS2`). The same shape. Whether they matter has not been measured.
