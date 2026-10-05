# Hardware AES and carry-less multiply: two builtins, and a way to ask whether the CPU has them

> **Status: design, not built.** `docs/tls-parity.md` §3.1 measured AES-GCM at about 170 times slower than OpenSSL's, and
> said the gap is the instructions lex-sys cannot emit. This document says what it would take to emit them, what is
> measured so far, and what is not. Where a later PR finds a claim here false, that PR corrects it here, in place.

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

**Not measured:** how fast the hardware path would be in lex-sys. §8 sets that as a gate for PR 3, not a claim.

## 3. The builtins

lex-sys has no 128-bit value: the scalar types are `int` (64-bit), `byte`, `bool` and `float`. An AES block and a GHASH
product are 128 bits. Two ways round that were considered:

| Option | What it is | Verdict |
|---|---|---|
| **Block-level builtins over byte slices** | the compiler loads the block into a vector register, does the whole operation there, and stores it | **chosen**: no new type, and no round-trip through memory between rounds |
| A vector type (`v128`) with `aes_round`, `clmul` | a language change: the checker, both backends, arithmetic rules, and how a vector meets checked arithmetic | the general answer, and the one ChaCha20's SIMD would need. Its own design, later |

The builtins, as `edition N;` (the latest, 6 today, as `value_barrier` was: `docs/editions.md` §5, a name a program could
already declare):

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

**What a wrong argument does.** A slice of the wrong length, or `rounds` other than 10, 12 or 14, answers a negative code, as
`std.gcm.seal` already answers `refused_key_length`, with a rule tag; it never reaches an instruction. No input reaches a
panic (CLAUDE.md). Whether a code or a trap is right here is the first question for the PR that builds it (§9).

## 4. How the LLVM backend lowers them

`crates/lex-sys-codegen-llvm/src/body/expr.rs` already lowers `value_barrier` to inline assembly. These three lower to
intrinsics instead, which LLVM can schedule:

- **The functions that contain them carry `"target-features"="+aes,+pclmul,+ssse3"`** (x86-64) or `+aes` (aarch64). Because LLVM
  refuses to inline a function with more features into one with fewer, the lowering is an out-of-line function per builtin,
  called directly, and the feature never reaches the caller.
- **aarch64 `aes_encrypt_block`** loops `aese` then `aesmc` for all but the last round, then `aese` and an XOR with the final
  key. **x86-64** loops `aesenc` and ends with `aesenclast`. Both are checked against FIPS-197's vectors (§7).
- **`ghash_update`** is four carry-less multiplies a block and a reduction by the GCM polynomial, on both targets. The
  aarch64 form byte-reverses with `rev64` and `ext`; the x86-64 form with `pshufb`.
- **`hw_aes_gcm()`:**
  - x86-64: `cpuid` leaf 1, `ecx` bits 25 (AES), 1 (PCLMULQDQ) and 9 (SSSE3), by inline assembly, as `value_barrier` is.
  - aarch64 Linux: `getauxval(AT_HWCAP)`, bits `HWCAP_AES` and `HWCAP_PMULL`.
  - aarch64 macOS: `sysctlbyname("hw.optional.arm.FEAT_AES")` and `FEAT_PMULL`.
  - the last two are libc calls. Whether the backend already links them, and whether a builtin that calls libc is still `[]`,
    are not checked here, and §9 asks the second.

## 5. The Cranelift backend

No instruction exists (§2), so:
- **`hw_aes_gcm()` answers `false`.** `std.aes` and `std.gcm` then take the software path, which is today's code, unchanged.
- **`aes_encrypt_block` and `ghash_update` are never reached by a correct program on Cranelift.** If one is, they answer the
  software result (an interpreter for the same function in the backend's runtime), so a program is never wrong, only slow. That
  keeps the two backends agreeing, the rule `docs/tls-pure.md` §9 gate 1 states.

**What this does not fix:** Cranelift's AES-GCM is slower than LLVM's (12.1 against 7.1 µs for a 64-byte seal on the M4, 3.1 times
at 16 KiB on the Xeon), and its data tests failed their timing test on the M4
(`docs/tls-assurance.md` §6.1). The hardware path does not reach Cranelift, so that result stands.

## 6. The code that uses them

`std.aes` and `std.gcm` change in three ways, in this order of risk:
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
- **Forcing the software path** on a machine that has the instructions, so CI covers both. A caller cannot do this today, so
  the PR adds an environment variable read only in a test build, or a second entry point, and says which.
- **Both CI runners:** linux-x86_64 and darwin-aarch64 run the hardware path. A runner without the instructions would show up as
  `hw_aes_gcm()` answering `false`. A test asserts `true` on the two CI runners, once PR 2 has measured that they answer it, so a
  silent fallback cannot hide.
- **The timing test, rerun** (`scripts/gcm_timing.py`), on both paths. The hardware instructions are specified as
  data-independent, but this CPU's own result (`docs/tls-assurance.md` §6.1) is the reminder that the specification and the
  machine can disagree, so it is measured.
- **Mutants:** at least 12 killed, as every package here has (`scripts/tls_mutants.py` is the form).
- **The record-layer corpus:** once #272 is merged, its fuzz corpus (`conformance/tls_fuzz.rs`) and the recorded handshakes
  replay on the new path.

## 8. Gates

The numbers PR 3 must show, each with its command, and an honest "not met" if it is not:
- **Speed:** the 16 KiB and 64-byte rows of §1, on the same two machines as `tls-parity.md` §3.1 and
  `tls-assurance.md` §6.1. No target is claimed here. The measurement decides how far `tls-parity.md` §3.1's "OpenSSL is about
  170 times faster" is corrected, in place.
- **Correctness:** §7's known answers and differential, on both backends.
- **No new capability:** `lex-sys authority` on the pure backend shows no `ffi(...)`, as #210 requires.
- **The gate:** `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, and no source
  file over 2,000 lines.

## 9. Open questions, for a person

1. **Return a code or trap on a bad length?** `value_barrier` cannot fail; these can. *Assumed: a negative code with a rule tag,
   as `std.gcm` does.*
2. **Is a builtin that reads the CPU `[]`?** It has no effect a program can observe, and its answer is the same for the life of
   the process. *Assumed: yes, as `value_barrier` is. If not, it needs a capability, and the pure backend's empty row is lost.*
3. **A vector type instead?** It would also serve ChaCha20, and it is the larger design. *Assumed: no, not here.*
4. **Does a Cranelift-only user accept the software speed?** *Assumed: yes. LLVM is the default backend.*
5. **Where the tests force the software path** (§7). *Assumed: an environment variable read in a test build only.*

## 10. Not done here

- **Any code.** This document is the design; the builtins, the lowering and the `std` change are three PRs after it.
- **ChaCha20 SIMD** (`docs/chacha20.md` §6).
- **AES-CBC, AES-CCM or any mode but GCM.** The block builtin would serve them; nothing here uses it.
- **SHA instructions** (`SHA256H`, `SHA256RNDS2`). The same shape. Whether they matter has not been measured.
