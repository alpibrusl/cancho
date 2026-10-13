# Wide multiply and add-with-carry: three builtins, prototyped on X25519

> **Status: built, on X25519 (§10), P-256, `std.bigmod` and Poly1305 (§12).** Issue #381, part of #378 (TLS performance: closing the gap with OpenSSL). The question it
> answers is whether the language is missing two instructions that every public-key operation here is waiting for. §2 is the
> measurement that was made *before* the compiler was touched; §3 to §9 are the design; §10 is what the prototype (X25519
> only) measured; §11 is what each other user of narrow limbs would gain, by arithmetic, for a person to decide. Where a later
> change finds a claim here false, it corrects it here, in place.

---

## 1. What is asked, and why

X25519, P-256 and P-384 (`std.ecdh`, `std.ecdsa`), RSA (`std.bigmod`) and Poly1305 do their multi-word arithmetic in
**narrow limbs**: 16 bits in `std.field25519` (TweetNaCl's representation, `docs/x25519.md` §2), 30 bits in `std.bigmod`
(`docs/rsa.md` §2.1), 26 bits in the Poly1305 of `std.chacha20`. The reason is the same each time and is written at the top of
each file: the language has `int`, which is 64 bits, checked or `wrapping_*`, and nothing else. There is no 64 x 64 -> 128-bit
multiply and there is no carry flag. A limb must therefore be narrow enough that a product, and the sum of a column of
products, still fits in 64 bits.

Hardware does not have that limit. Every 64-bit CPU multiplies two words into two words (`mul` on x86-64; `mul` and `umulh` on
aarch64) and adds with a carry (`adc`, `adcs`). A 255-bit field element is 5 limbs of 51 bits there and 16 limbs of 16 bits
here, so a field multiplication is 25 products, not 256.

The cost is visible. `std.x25519` takes **0.58 ms** on an Apple M4 and **0.66 ms** on the x86-64 machine of §2, where OpenSSL 3.5 takes
26.7 µs (issue #381 put it at "0.57 ms against OpenSSL's tens of µs"; §2 has the measurements).
X25519 is one key-exchange operation in every TLS 1.3 handshake, and the same shortage is behind P-256's cost (the common
choice for servers) and RSA's.

**Not asked for:** a 128-bit integer type, any change to checked arithmetic, SIMD (`docs/chacha20.md` §6), or any call into
C. The builtins are pure (`[]`), as `value_barrier` and `wrapping_add` are (`docs/crypto-builtins.md` §1; #210's gate needs the
pure backend to have no `Ffi`).

## 2. What is measured before the compiler is touched

The question this section decides is whether to build anything at all: **what would a field multiplication cost on 51-bit
limbs, against the 16-bit limbs the language has today?** It is answered with C, because C already has `unsigned __int128` and
so shows what the instructions give without this language's own overheads (bounds checks, slices, loops) in the way.
`scripts/field25519_widths.c` holds both versions: TweetNaCl's 16 x 16-bit multiplication, carried and folded as
`std/field25519.cho` does it, and curve25519-donna-c64's 5 x 51-bit one. Each ladder is the same Montgomery ladder
(RFC 7748 §5) and each is checked against RFC 7748 §5.2's first vector before it is timed. A third version writes the 51-bit
multiplication the way this language will be able to write it, with no 128-bit variable: a `(hi, lo)` pair per product and an
explicit `(sum, carry)` addition into a two-word accumulator, which is `mul_wide` and `add_carry`. Minimum of 25 runs; `cc -O2`.

| Machine (CPU, load) | 16-bit limbs, field mul | 51-bit, `__int128` | 51-bit, `(hi, lo)` pairs | 16-bit, X25519 | 51-bit, X25519 | ratio |
|---|---|---|---|---|---|---|
| gram, x86-64: Intel i7-1260P, pinned to cores 4-5, load average 3 to 5 from other agents | 155 ns | 15.8 ns | 15.1 ns | 496 µs | 49.9 µs | 9.9 |
| Apple M4 Max, macOS, idle | 77.6 ns | 10.3 ns | 9.6 ns | 237 µs | 27.1 µs | 8.8 |

(`clang -O2` on gram, and `-O2 -march=native`, which lets the compiler use `mulx`, changes nothing: 15.6 ns. gram's clock is a
laptop's and its load is other agents', so its figures move by up to 30% between runs; the table is the minimum, and §10 repeats
every gram figure the same way.)

What the language does today, on the same two machines, with the same ladder (`scripts/curve25519_bench.py`, median of five,
less the cost of starting the process):

| Machine | `std.x25519` today | OpenSSL 3.5 / 3.6 `speed ecdhx25519` | today / OpenSSL |
|---|---|---|---|
| gram | **0.664 ms** | 26.7 µs | 25 |
| Apple M4 Max | **0.577 ms** | 19.0 µs | 30 |

So the **ceiling** on 51-bit limbs with a wide multiply, from C, is about 10 times the C code on 16-bit limbs, and the code
here is slower than C on 16-bit limbs by 1.3 times (gram) to 2.4 times (M4), which is room for the new code to land nearer the
C figures than the old code does. A target of **3 times** (the issue's) asks for X25519 at 0.22 ms on gram; the C estimate says
the instructions allow 0.05 ms before this language's own overhead. That is a wide margin, and it is why §3 onward builds the
prototype rather than stopping here. (§10 says where it landed.)

## 3. The builtins

```cancho
mul_wide(a: int, b: int) -> [] (int, int)
add_carry(a: int, b: int, carry: int) -> [] (int, int)
sub_borrow(a: int, b: int, borrow: int) -> [] (int, int)
```

Every word is an `int` **read as an unsigned 64-bit integer**: `-1` is 2^64 - 1. There is no new type. A word is stored in the
`int` the language has, and the `int` operations that do not care about the sign (`&`, `|`, `^`, `<<`, `wrapping_add`,
`wrapping_sub`, `wrapping_mul`) already treat it as a bit pattern. `>>` is arithmetic, so extracting a field of bits above bit
0 takes a mask (`x >> 51 & 0x1fff`); §10.6 shows LLVM folds it into one funnel shift on aarch64.

- **`mul_wide(a, b)`** is the exact product `a * b` as `(hi, lo)`: `hi * 2^64 + lo`. It cannot overflow, so it never traps.
- **`add_carry(a, b, carry)`** is `a + b + (carry != 0)` as `(sum, carry_out)`: `sum` the low 64 bits, `carry_out` 0 or 1.
- **`sub_borrow(a, b, borrow)`** is `a - b - (borrow != 0)` as `(difference, borrow_out)`: the low 64 bits and 0 or 1 (1 when the
  true result is negative).

The carry in is **"nonzero"**, not "one": any other value counts as 1, so that no input can reach a trap and the instruction
sequence has no check on it. A caller that holds a carry as 0 or 1, as the builtins return it, never sees the difference.

**Why tuples.** A result is two words, and the language has tuples (`docs/tuples.md`): `let (hi, lo) = mul_wide(a, b);`. A
tuple of two `int`s is two leaves, so it lives in two registers (§4) and nothing is allocated or copied. The alternatives were
a struct (a type to name for each builtin) and an out-parameter (`&![int]`, a store and a load through memory, which is what
the builtin exists to avoid).

### 3.1 Why exactly this set

The set is the smallest one the inner loops of the algorithms that want it can be written in. Each row is the loop body as it
is written with 64-bit limbs.

| Algorithm | Inner step | Needs |
|---|---|---|
| **X25519, Ed25519** (5 x 51 bits; ref10, donna-c64) | `column += a_i * b_j`, 25 times, into a 128-bit accumulator; then carry each column's top into the next | `mul_wide`, and `add_carry` to add the low word and carry into the high |
| **P-256, P-384** (Montgomery on 64-bit limbs; or Solinas) | CIOS: `t[j] + a_i * b[j] + c`, then `t[j] + m * n[j] + c`; add and subtract mod p; the final conditional subtraction | `mul_wide`, `add_carry`, `sub_borrow` |
| **`std.bigmod`, RSA** (Montgomery, up to 4,096 bits) | the same CIOS step, over up to 64 limbs | `mul_wide`, `add_carry`, `sub_borrow` (the final subtraction) |
| **Poly1305** (3 limbs of 44 bits, poly1305-donna-64; or 130 bits as 64-bit limbs) | `h = (h + m) * r`: nine products into three 128-bit accumulators; a final `h - p` | `mul_wide`, `add_carry`, `sub_borrow` |

Three builtins cover all four, and each is needed by at least two. **What was left out, and why:**
- **`mul_add_wide(a, b, c, d) -> (hi, lo)`**, `a * b + c + d`, the CIOS step as one operation. It cannot overflow (the maximum is
  2^128 - 1) and it is what the loops above are made of. It is `mul_wide` and two `add_carry`s with a zero carry, and on both
  targets those three become `mul; add; adc; add; adc`, the same instructions a fused builtin would end in (§6 shows the object
  code). A fourth name that adds no instruction is a fourth name to keep.
- **A funnel shift** (`(hi << 13) | (lo >> 51)`, `shrd` on x86-64, `extr` on aarch64). The 51-bit carry extraction is one. It is
  three instructions in the language and one in the instruction set. Not added, for the reason above, and recorded in §10 as
  the place a further step would look first.
- **A 128-bit type.** The same answer as `docs/crypto-builtins.md` §3: a language change (the checker, both backends, how it
  meets checked arithmetic) for a need that a pair of words meets.
- **A carry as a separate flag value** (`adc` as an instruction that reads the flags a previous one left). It cannot be
  expressed without a type that is a flag; threading the carry as an `int` is what the compiler can already see through.

## 4. How the LLVM backend lowers them

`crates/cancho-codegen-llvm/src/body/wide.rs`. Each builtin is straight-line IR on registers, inline at the call.

- **`mul_wide`**, on x86-64 and aarch64 (the architectures with a widening multiply): `zext` both words to `i128`, one
  `mul i128`, `trunc` for the low word and `lshr 64; trunc` for the high one. LLVM selects one `mul` (x86-64) or `mul` and
  `umulh` (aarch64) for it. It is not `llvm.umul.with.overflow`: that returns the low word and a flag, and the high word is the
  point.
- **`add_carry`, `sub_borrow`**: `llvm.uadd.with.overflow.i64` (`usub`) twice, once for `a + b` and once for adding the carry,
  and the two overflow bits are `or`ed (they cannot both be set: after the first overflow the value is at most 2^64 - 2). The
  carry in is `icmp ne 0; zext`.
- **Elsewhere (WebAssembly, 32-bit targets)**: the same IR would become a call to `__multi3`, which the WASI sysroot does not
  ship (`docs/wasm.md`, W0.3). The product is built from four 32 x 32 -> 64 multiplies (Knuth's Algorithm M in Hacker's
  Delight's `mulhu` order). One thing about it was found by running it: LLVM's AggressiveInstCombine recognises that idiom and
  turns it back into an `i128` multiply, so `__multi3` returned. The low halves of the operands therefore pass through an empty
  `asm`, as `value_barrier`'s argument does, which hides the idiom and costs no instruction. The wasm32 module was linked and
  run under wasmtime (§9).
- The module declares the two new intrinsics once, with the three it already declares.

## 5. How the Cranelift backend lowers them

`crates/cancho-codegen/src/body/expr.rs`. The pinned Cranelift is 0.121.2.

- **`mul_wide`**: `umulhi` for the high word and `imul` for the low. `umulhi` has lowerings for `i64` on x86-64 and aarch64
  (`isa/x64/lower.isle`: `mul`, or `mulx` where BMI2 is enabled, which it is not by default; `isa/aarch64/lower.isle`: `umulh`).
- **`add_carry`, `sub_borrow`**: `iadd` (`isub`) twice and an unsigned `icmp` against the first operand, which is the carry
  out of each addition, `bor`ed, `uextend`ed. The flag instructions exist in 0.121.2 (`uadd_overflow`, `usub_overflow`,
  `uadd_overflow_cin`, `usub_overflow_bin`), but `isa/aarch64/lower.isle` has no rule for them (checked by searching the file,
  not by running one), and `uadd_overflow_cin` has no rule in either ISA's `lower.isle`. An `icmp` used as a value is `setcc`
  or `cset`, never a branch, on both. It costs more instructions than LLVM's `adc` chain, and §10 measures what that costs.

Cranelift does not set Arm's `PSTATE.DIT` and cannot emit inline assembly (`docs/crypto-builtins.md` §6), so its constant-time
story for these is the instruction's own, not the platform's (§6).

## 6. Constant time

A builtin used on a secret (`mul_wide` of two limbs of a scalar-derived field element) must take the same time whatever its
operands are. Three things are claimed and checked, none by argument alone.

1. **No branch, no call, no memory access** in the code of the builtins. `scripts/wide_multiply_objdump.py` builds
   `tests/programs/wide_multiply_ct.cho`, whose function `words` is nothing but the three builtins, disassembles the object
   and fails on any jump, call or return other than the last `ret`. What it found (the function, in full, for x86-64 and
   aarch64 with LLVM; all four are in §10.3):

   ```
   x86-64, LLVM:    mul %rdi ; setne ; setne ; lea ; add ; adc ; setb ; adc ; movzbl ; add ; sbb ; adc ; ret
   aarch64, LLVM:   mul ; umulh ; cmp ; cset ; cmp ; cset ; cmp ; adcs ; cset ; add ; adc ; cmp ; sbcs ; cset ; cmp ; adc ; ret
   ```

   Both contain the widening multiply and a carry chain, and no `__multi3`: the script also fails if the object names it.
   `crates/cancho-codegen-llvm/src/wide_tests.rs` checks the same on the IR for x86-64, aarch64 and wasm32, so that a change that
   introduces a `select`, a branch or a libcall fails without a disassembler.
2. **The instructions themselves.** `mul`, `adc` and `sbb` have no operand-dependent timing on the x86-64 cores this runs on;
   `mul`, `umulh`, `adc` and `sbc` are in the set Arm's data-independent timing bit makes data-independent. Both are the
   architectures' documented behaviour, which this document did not re-verify from the manuals, so the measured evidence is the
   third item.
3. **A timing test of the whole.** `scripts/x25519_timing.py`, dudect, on the prototype (§10), on every machine, at 10^6
   measurements, with the bar |t| < 4.5.

**DIT.** On aarch64 the LLVM backend's `main` sets `PSTATE.DIT` where the OS reports it (`docs/crypto-builtins.md` §6), which
covers `umulh` like every other integer instruction. The builtins add nothing to that code.

**The value barrier.** The builtins return values and make no selection, so there is no mask for a barrier to protect; a
program that selects on a carry still uses `value_barrier` on the mask, as `std.field25519.cswap` does. `docs/value-barrier.md`
§4's audit of the new field module is in §10.

## 7. Targets, editions, effects, tables

- **Targets.** They are accepted everywhere. On WebAssembly under WASI they build and run (§4), so there is no
  `unsupported-on-target` refusal and `wasi_gap` (`crates/cancho-ir/src/target.rs`) answers `None` for all three. A refusal
  would have been wrong: nothing about the target is missing.
- **Editions.** Edition 7, the latest, as `hw_aes_gcm` was (`docs/editions.md` §5, additive): `mul_wide`, `add_carry` and
  `sub_borrow` are names a program may already declare. `tests/reject/wide_multiply_is_edition_seven.cho` shows the file in
  edition 6 refused with `not-a-function`, and none of this repository's files declares one of the three (searched).
- **Effects.** None: `Effects::pure()`. No region parameters.
- **The self-hosted tables.** `examples/selfhost/tables.cho` holds the builtin names and the edition each is visible from;
  regenerated by `UPDATE_SELFHOST_TABLES=1 cargo test -p cancho-ir selfhost_tables`.
- **The new `std` module** is registered in `crates/cancho/src/main.rs`'s list.

## 8. What happens to every existing user of narrow limbs

Nothing changes for them. The builtins are additive and edition-gated, no existing function is edited, and a file in edition 6
or earlier cannot see them. The prototype moves one user, `std.x25519`, onto a new module, `std.field25519_51`; `std.field25519`
stays, whole, for `std.ed25519`, which has its own point arithmetic over 16 limbs and is not moved here. `std.bigmod`
(P-256, P-384, RSA) and Poly1305 are not touched. §11 lists what each would gain.

## 9. How it is tested

- **`tests/programs/wide_multiply_driver.cho`** and `crates/cancho/tests/conformance/wide_multiply.rs`, on **both backends**,
  against Rust's `u128` as the oracle: every pair of 15 operand edges (0, 1, 2, around 2^32, around 2^63, 2^64 - 1, alternating
  bits) with carries of 0, 1, 2 and 2^64 - 1 (the rule that a carry is "nonzero"); an 8-word ripple of carries and of
  borrows, each carry-in the previous carry-out; known answers; and **three seeds of 2,000,000 random, edge-biased cases of
  each of the three builtins** (6,000,000 products), whose answers the driver folds into one checksum that the test recomputes from
  the same generator.
- **The same driver on wasm32-wasip1**, built with the compiler and run under wasmtime, giving the native checksum.
- **The object code** (§6) and the IR (`wide_tests.rs`).
- **The reject fixture** for edition 6, and an accept fixture (`tests/accept/wide_multiply.cho`) on the values the carries turn on.
- **X25519**: RFC 7748's eight vectors including the 1,000-iteration one, Wycheproof, `scripts/curve25519_differential.py`
  against OpenSSL, mutants, ctgrind-style and dudect checks (§10).

## 10. Results

### 10.1 What was built

`std/field25519_51.cho` (`std.field25519_51`): `add`, `sub`, `mul`, `square`, `mul_small`, `cswap`, `invert`, `pack`, `unpack`, over
five 51-bit limbs; `mul` is 25 `mul_wide`s, each added into a two-word accumulator by `add_carry`, and `square` is 15.
`std.x25519` imports it instead of `std.field25519`; its ladder, clamping and refusals are the same lines. The inversion is
ref10's addition chain (254 squarings and 11 multiplications) where the old field did 253 squarings and 251 multiplications by a bit
loop, so part of the gain below is not the builtins; §10.5 separates it. `std.field25519` is unchanged and `std.ed25519` still uses it.
`tests/lex/field25519_51_test.cho` pins the corners a ladder reaches with probability near 2^-51 (a result needing the second carry
pass of `pack`, p and p + 1, (p - 1)^2, 2 * 2^-1).

### 10.2 Correctness

- RFC 7748's eight vectors (including the 1,000-iteration one) and every Wycheproof X25519 case, on both backends, on macOS arm64
  and x86-64 Linux (`crates/cancho/tests/conformance/x25519.rs`).
- `scripts/curve25519_differential.py` against OpenSSL (pyca/cryptography 50.0.2): **200,000 X25519 and 1,000 Ed25519 rounds, 404,000
  checks, 0 differences** on macOS arm64 with LLVM; 20,000 and 300, 0 differences, on aarch64 Linux.
- `mul_wide`, `add_carry`, `sub_borrow`: §9, on all of LLVM, Cranelift and wasm32, on macOS arm64 and on x86-64 Linux (both backends
  there too; wasm32 only on macOS).
- **A bound that was false, found in review and corrected here.** The first version of this module said `mul` accepts limbs under
  2^54. It did not: with every limb at 2^54 - 1, the carry out of column 4 times 19 is above 2^63, `finish`'s `r0 >> 51` is an
  arithmetic shift, and `mul(a, a)` gave wrong bytes. X25519 was not affected (its limbs stay under about 3 * 2^51), a port of Ed25519's
  larger sums would have been. The shift is now masked to the 13 bits it means, and `test_mul_and_square_at_the_input_bound` checks
  the bound itself against Python's `(v * v) % p`.
- Mutants (raw output in `docs/assets/wide-multiply/`). `scripts/wide_multiply_mutants.py`: **20 of 20 killed** (the LLVM lowering
  and its wasm32 path, the Cranelift lowering, the edition), with the wasm32 toolchain set. `scripts/curve25519_mutants.py`: **39
  mutants of the four files, 36 killed**; both scripts now match a mutant's text as tokens, so `cancho fmt` cannot stop them (it
  did once: the first count, 33 of 38, could not be re-run after the formatting commit). The three that survive:
  the X25519 "final swap left out" (equivalent: clamping clears bit 0) and Ed25519's `point_equal` (`docs/x25519.md` §5) were
  known; "`pack` of the new field does one weak carry pass, not two" is, I believe, equivalent: the later `q` step absorbs the one
  extra bit a single pass can leave (seen on the crafted element of the unit test, not proved for every input), and the second pass
  stays as margin. Two `std.field25519` mutants ("`pack` does one trial subtraction", "`unpack` keeps the top bit") used to be
  killed only by X25519's vectors, which no longer run on that field; `tests/lex/field25519_test.cho` now kills them directly.

### 10.3 Constant time

Raw logs, with the commit and the machine, are in `docs/assets/wide-multiply/`.

| What | x86-64 gram (i7-1260P, cores 4-5) | Apple M4 Max, macOS | aarch64 Linux (Docker on the M4) |
|---|---|---|---|
| dudect (`scripts/x25519_timing.py`), LLVM, 10^6 samples a test, fixed / sparse scalar, max abs t (bar 4.5) | 1.27 / 1.41 | 1.78 / 1.90 | 1.40 / 1.21 (before the review's fix to `finish`) |
| dudect, **Cranelift** | 1.32 / 1.37 (load 7.8) | 1.15 / **9.20**, again 1.75 / **15.24: fails**; with `PSTATE.DIT` set, 1.44 / 1.72 | not run |
| ctgrind (`scripts/curve25519_ctgrind.sh`, Valgrind Memcheck, scalar marked undefined), LLVM | **0 reports**; the control (an `if` on the secret bit planted in the ladder) **251 errors**; Ed25519 signing, the known exception, 1,937 | not run: no Valgrind on macOS | 0 reports (control not run there) |

**Cranelift carries no constant-time claim for X25519 on Apple silicon.** Its X25519 fails dudect on the sparse scalar on the M4,
twice, at |t| 9.2 and 15.2, and passes when `PSTATE.DIT` is set from outside the program (a constructor in `libtick.a`, the method of
`docs/tls-assurance.md` §6.1). So what varies is the CPU's own timing of some instruction on these operands with DIT clear, which the
LLVM backend's `main` sets and Cranelift's does not (it cannot emit the instruction). The 16-bit field on Cranelift passed (200,000
samples, |t| 1.28 / 1.17; fewer samples, as it is 20 times slower, so a weaker test). This is the limitation `docs/crypto-builtins.md`
§5 already states for Cranelift's AES-GCM: a TLS build uses LLVM, the default.

- Branch audit (`scripts/chacha20_branches.py`, x86-64): in every function of `std.field25519_51` the only conditional jumps are
  bounds-check traps, plus `square_n`'s loop counter; `pack` has three `cmovb`, on the output slice's length. In `x25519.scalarmult`
  the 11 jumps that are not traps are on the loop counter `i`, `scalar_bit`'s tests of it, and the lengths. None is on a field value.
- Value barrier (`docs/value-barrier.md` §4): `cswap`, the one selection, makes its mask through `value_barrier`, as before.
- The builtins' own object code (`scripts/wide_multiply_objdump.py`; `words` is the three builtins and nothing else):

  | Backend, machine | instructions | widening multiply | carry instructions | branch or call |
  |---|---|---|---|---|
  | LLVM, x86-64 | 18 | `mul` | `adc`, `sbb`, `setb` | none |
  | LLVM, aarch64 | 17 | `mul`, `umulh` | `adc`, `adcs`, `sbcs`, `cset` | none |
  | Cranelift, x86-64 | 43 | `mul`, `imul` | `setb` | none |
  | Cranelift, aarch64 | 32 | `umulh` | `cset` | none |

  The x86-64 LLVM code is `mulx`-free: the baseline x86-64 target has no BMI2, and `-march=native` bought nothing in C (§2).

### 10.4 Speed

`scripts/x25519_before_after.py` (rerunnable): "before" is `std/x25519.cho` as it was before this change, taken from git and built as a
local module over the unchanged `std.field25519` with the same compiler; "after" is the tree. The two alternate, each figure
`scripts/curve25519_bench.py`'s (median of five runs of N operations, less one). Raw output in `docs/assets/wide-multiply/before-after-*.log`.
OpenSSL is `openssl speed ecdhx25519` on the same machine.

| Machine, CPU, load | Backend | before | after | speed-up (medians) | OpenSSL |
|---|---|---|---|---|---|
| Apple M4 Max, macOS, load 6 to 9 from other agents, 5 alternating runs | LLVM | 0.575 ms | **0.025 ms** | **23x** | 0.019 ms (OpenSSL 3.6.4) |
| | Cranelift | 1.277 ms | 0.065 ms | 20x | |
| aarch64 Linux, Docker on the M4 Max (an earlier run, before the review's fix) | LLVM | 0.60 ms | 0.026 ms | 23x | 0.019 ms (OpenSSL 3.0.13) |
| gram, Intel i7-1260P, cores 4-5, **load average 7 from other agents**, 7 alternating runs | LLVM | median 2.36 ms (min 1.58) | median 0.132 ms (min 0.051) | 18x | 0.0267 ms, measured earlier when quiet |
| | Cranelift, 5 runs | median 3.81 ms (min 2.56) | median 0.327 ms (min 0.224) | 12x | |
| gram, earlier runs: load 3.6 before the squaring change; load 6 to 12 | LLVM | 0.664 to 0.93 ms; median 1.87 | 0.051 to 0.063 ms; median 0.108 | 12 to 13x; 17x | |
| | Cranelift, load 6 to 12 | median 3.98 ms | median 0.398 ms | 10x | |

gram's absolute figures are not reproducible: it ran at one third of its early speed by the end of the work, other agents' jobs
included one on a sibling hardware thread of the pinned cores. The ratios within one interleaved run are the evidence, not the
milliseconds; the three quiet-time anchors are the early 0.664 ms before, the C figures of §2 (16-bit 0.50 ms, 51-bit 0.050 ms), and OpenSSL's 0.0267 ms.

### 10.5 Where the gain came from

On the M4, LLVM: 0.575 to 0.585 ms before. The new field with the old bit-loop inversion would cost 253 + 251 = 504 field operations for the
inversion against 265; with 2,550 in the ladder that is about 8%, so 1.1x of the 23x is the addition chain, and the squaring
(15 products against 25, measured 8% on the whole) another 1.1x. The rest, about 19x, is the representation:
**25 products in 5 limbs against 256 in 16, and `mul_wide` and `add_carry` letting each be two instructions.** That is more than §2's C
ratio (8.8x) because the code in this language on 16 limbs was 2.4 times slower than the same code in C (loop control, bounds checks and
every limb a load and a store through a slice), and the 51-bit code, being straight-line on locals, has none of that to lose:
0.025 ms is the C `unsigned __int128` figure (0.027 ms) to within 8%.

### 10.6 Verdict

**It reaches 3x, and by a wide margin: 23x on arm64 and 17 to 18x on a loaded x86-64 with LLVM; 20x on arm64 and 10 to 12x on the loaded
x86-64 with Cranelift.** (Cranelift's X25519 is not constant time on the M4 without DIT, §10.3; a TLS build uses LLVM.) X25519 costs 1.3 times OpenSSL's hand-written assembly on the M4 (0.025 against 0.019 ms) and about 2.5 to 4 times on
gram (0.051 to 0.132 ms under load against 0.0267 ms quiet; the comparison is unfair to this language until gram is quiet).
The wide builtins are therefore worth keeping, and the larger win for every public-key operation exists *if* the others behave
like X25519 (§11).

What limits it from here, in order of what I would check, none of it measured:
1. **x86-64 baseline instructions.** `mul`, not `mulx`, and one carry chain, not two (`adcx`/`adox`). OpenSSL's x86-64 X25519 uses both.
   Reaching them needs `"target-features"="+bmi2,+adx"` on the functions that use the builtins and a run-time CPU test, the shape
   `docs/crypto-builtins.md` §4 built for AES. C at `-march=native` (§2) gained nothing, so the expected gain is small.
2. **Limbs through memory.** `add`, `sub` and `cswap` are separate passes over 5-element slices, as the old ladder's were; the ladder step is
   about 10 field operations, and fusing them by hand would keep the limbs in registers. A language-level fix is an array value type, not a builtin.
3. **The carry extraction.** `(hi << 13) | (lo >> 51)` is one `extr` on aarch64 (LLVM found it; 5 in `mul`'s object code), so no funnel-shift builtin is needed there. On x86-64 it is `shrd` when LLVM finds it; not checked.
4. **Bounds checks** in each function (`udf`/`ud2` branches): cheap and predicted, about 15 in `mul`.
The language's loop overhead is *not* what limits it: the 51-bit code has no inner loops.

## 11. What this does not do, and what each user would gain

Not done here, and each a decision for a person: P-256, P-384, RSA and Poly1305 are not moved. The estimates are **by arithmetic from
the inner loops, not measured**; §10 is the only evidence about how well this language turns such a loop into instructions, and
it was better than its C estimate.

| User | Today | With the builtins | Arithmetic |
|---|---|---|---|
| `std.bigmod` Montgomery multiplication (P-256, P-384, RSA), `mont_mul` | 30-bit limbs: k = 9, 13, 69 for 256, 384, 2,048 bits; the inner step does two multiplies and fits 62 bits | 64-bit limbs: k = 4, 6, 32; the inner step is two `mul_wide`s and three or four `add_carry`s | inner steps k x k: 81 to 16, 169 to 36, 4,761 to 1,024 (5.1x, 4.7x, 4.6x fewer); each step costs about 1.4x more instructions (10 against 7); so 3 to 3.5x on `mont_mul` |
| P-256 ECDH, ECDSA | the same `mont_mul` plus table selection that reads all 16 entries under masks, additions, copies | | at most the `mont_mul` gain; the share of `mont_mul` in `std.ecdh` was not measured here. If it is 80%, 2.2 to 2.5x on the whole |
| RSA-2048 private-key operation | `pow_mod` on 69 limbs | on 32 limbs | 3 to 3.5x |
| Poly1305 (`std.chacha20`) | 5 limbs of 26 bits: 25 products and reductions a 16-byte block | 3 limbs of 44 bits (poly1305-donna-64): 9 `mul_wide`s | 2 to 2.5x on the MAC, which is a minority of ChaCha20-Poly1305's per-byte cost; ChaCha20 itself is untouched |
| `std.ed25519` | 16-bit `std.field25519`, 3 ms to sign | `std.field25519_51` ported into its point arithmetic | its field multiplications are most of a signature, so the X25519 factor is the ceiling; the bit-serial scalar arithmetic mod L, which does not use them, is the remainder |

The ordering by value, which is a decision for a person: `std.bigmod` first (one change speeds P-256, P-384, ECDSA verification and RSA),
then Ed25519, then Poly1305.

*As built (§12): P-256 had left `std.bigmod` before this was built (#397's own field), so P-256, `std.bigmod` and Poly1305 were three
changes. The table's arithmetic was low for `std.bigmod` (RSA-2048 verification 6.8 times fewer instructions on x86-64 against 3 to
3.5, P-384 4.2 times) and for Poly1305 (2.3 times on a 16 KiB MAC against 2 to 2.5), and does not apply to P-256, whose field was
already straight-line: 1.6 times on the multiplication, 1.4 to 1.7 on the operations. Ed25519 was not moved.*

## 12. As built: P-256, `std.bigmod`, Poly1305 (#381)

The three other users of narrow limbs, moved in that order, each a commit kept because it paid. All figures are on `gram` (the
i7-1260P of §2, Linux x86-64, LLVM backend, CPU 0 pinned) in user-space cycles and instructions through `sudo perf stat`
(`kernel.perf_event_paranoid` was left at 4), each the least of several runs alternating the two builds. Other agents' jobs ran on the
machine throughout (load average 4 to 7, some scheduled on the measured core's sibling), so cycles moved by up to two times between
runs; the instruction counts are exact and are the steady figure. OpenSSL is 3.5.5 `openssl speed` on the same core, cycles from
`perf stat` over its run. Raw output: [`assets/wide-multiply/381/`](assets/wide-multiply/381/).

*The commits named here and in the raw output* (b2651fea, 6b5299e5, 8a596168, 541a03cc, d4436b62, 028ce885) are the history as
measured, kept on the branch `p256-wide-multiply-evidence`; the PR's commits are the same changes rebased onto `main`. At 541a03cc
and 028ce885 every `std` file is byte for byte the PR's. At 6b5299e5 (the P-384 timing) `std/bigmod.cho` is; at b2651fea (the
P-256 timing) `std/p256_kernels.cho` differs only in calling its own `mac` where the PR calls `std.field25519_51`'s, and its
kernels' object code is the PR's instruction for instruction (`kernels_object_identity.txt`).

| user | operation | before: cycles, instructions | after | gain (cycles; instructions) | OpenSSL cycles | after / OpenSSL |
|---|---|---|---|---|---|---|
| **P-256** ([`p256-fast.md`](p256-fast.md) §13) | field multiplication | 158, 782 | 99, 451 | 1.60; 1.73 | | |
| | ECDH `shared` (k·P) | 980,090, 5,002,519 | 567,604, 2,912,470 | **1.73**; 1.72 | 176,845 | **3.21** (5.54 before) |
| | `public_key` (k·G) | 233,437, 1,197,041 | 141,568, 713,529 | 1.65; 1.68 | | |
| | ECDSA sign | 326,959, 1,632,023 | 224,736, 1,075,660 | **1.45**; 1.52 | 70,906 | **3.17** (4.61) |
| | ECDSA verify | 682,396, 3,431,653 | 416,757, 2,026,587 | **1.64**; 1.69 | 233,829 | **1.78** (2.92) |
| **`std.bigmod`** | RSA-2048 verification (PKCS#1 v1.5, e = 65537) | 1,052,321, 5.36M | 154,232, 0.79M | **6.8**; 6.8 | 59,173 | 2.61 (17.8) |
| | RSA-4096 verification | 4,096,539, 21.05M | 570,300, 2.95M | **7.2**; 7.1 | 193,384 | 2.95 (21.2) |
| | P-384 ECDSA verify | 13,341,161, 67 to 69M | 2,608,260, 13.8 to 14.2M | **5.1**; 4.9 | 1,290,556 | 2.02 (10.3) |
| | P-384 ECDH `shared` | 11,950,097, 60.84M | 3,066,688, 14.55M | **3.9**; 4.2 | 989,481 | 3.10 (12.1) |
| | P-384 `public_key` | 12,209,672, 60.82M | 3,217,849, 14.54M | 3.8; 4.2 | | |
| **Poly1305** | 16-byte message | 209, 712 | 186, 598 | 1.12; 1.19 | | |
| | 1 KiB | 5,810, 17,470 | 2,577, 7,150 | **2.25**; 2.44 | | |
| | 16 KiB | 89,605, 272,830 | 39,039, 106,990 | **2.30**; 2.55 | | |

The P-256 rows are `tests/programs/p256_profile.cho` (`scripts/p256_bench.py`'s ops); the `std.bigmod` rows the conformance drivers
of `std.rsa`, `std.ecdsa` and `std.ecdh` run the same input many times (a random key a run, so the P-384 verification's instructions
move with it; the outputs of the two builds compared equal on every run); Poly1305 a driver that chains `chacha20.poly1305` over one
message. In the TLS benchmark of `docs/tls-performance.md` §6, ChaCha20-Poly1305's rows came down by 7 to 9% in instructions
(1.790M to 1.628M at 16 KiB on x86-64, 1.790M to 1.698M on arm64): that is Poly1305's part of the AEAD; ChaCha20 is the rest,
untouched (§10.15 there).

### 12.1 What each change is

- **P-256** ([`p256-fast.md`](p256-fast.md) §13): five limbs of 56 bits inside the generated kernels, each packing two of the
  element's 28-bit limbs, so R = 2^280, the representation, the constants, the tables and every caller stay; 25 `mul_wide` a
  multiplication and 15 a squaring against 100 and 55 products; the reduction in radix 2^56 using the prime's form. Four 64-bit
  limbs were measured first and not chosen (there, §13.1).
- **`std.bigmod`**: 64-bit limbs read as unsigned words. CIOS with the reduction fused as a second carry chain: a step is two
  `mul_wide` and four `add_carry`, and neither chain's sum can exceed two words. The reductions, additions and subtractions are
  `add_carry` and `sub_borrow` chains; a limb comparison is a borrow, never `<` on a signed `int` (a mutant that compares them as
  signed is killed by every RSA modulus, whose top limb has its top bit set). A byte no longer straddles two limbs. The register
  slot stays 139 words so every caller's `work` keeps its layout. `tests/vectors/bigmod_powers.txt` gains 180 cases at moduli a
  multiple of 64 bits long (400 in all).
- **Poly1305** (`std.chacha20`): poly1305-donna-64's three limbs of 44, 44 and 42 bits, nine `mul_wide` a block against 25
  products, `20 r1` and `20 r2` made once per key, and a block loop (`poly_blocks`) that keeps the accumulator in registers from one
  block to the next instead of storing it in the state between calls; `poly1305` and the AEAD's `poly_padded` call it once for
  their whole blocks. The bounds, from the clamped key: a column under 2^92, a carry under 2^48, h1 under 2^44 + 48 between blocks.
- **One two-word accumulator.** `mac` and `addw` are `std.field25519_51`'s, made public; the P-256 kernels and Poly1305 call them
  (the duplication check found the copies). The program is one LLVM module, so this changed no instruction: the P-256 kernels'
  x86-64 object code is the same before and after, instruction for instruction (`kernels_object_identity.txt`).

*Why the gains differ from §11's arithmetic.* `std.bigmod` gained more than the k² count: its 30-bit loops were checked arithmetic
over `w[x + i]` indices, each addition and index tested, which #397 measured at 2.4 times on P-256's multiplication; the new loops are
`wrapping_*` and the builtins over slices, and LLVM's inner loop is two `mul`s and their carries with no bounds check left in it.
P-256 gained less: its field was already straight-line with a multiplication-free reduction (#397), and on x86-64 without BMI2 every
`mul` writes `rax:rdx`, so the 35 `mul`s of a multiplication keep the register allocator spilling (161 `mov`s among about 440
instructions in `mul_p`); §10.6's first item, `mulx` behind a CPU test, is where that would go.

### 12.2 Gates

- **Vectors**, byte for byte, both backends where the suite runs both: `tests/vectors/p256.txt` (the kernels at their worst-case
  limbs), RFC 6979, Wycheproof ECDH (P-256, P-384), ECDSA and RSA, NIST CAVP KAS and SigVer, the RSA NIST files, RFC 8439's
  ChaCha20-Poly1305 and Poly1305 vectors and Wycheproof's, the new bigmod limb boundaries. `cargo test --workspace` in the arm64
  Linux container at 541a03cc: 685 passed, and the one known container failure (`project::a_compiler_other_than_the_one_named_is_refused`,
  which needs a git checkout); CI runs both targets.
- **Differentials** (`assets/wide-multiply/381/differentials.txt`, at d4436b62, before the shared accumulator, which changed no
  instruction; arm64 Linux, OpenSSL 3.0.13, pyca/cryptography 41): ECDSA signing
  against OpenSSL 10,000 signed and 10,000 flipped, against RFC 6979 in Python 10,000, keys 600 checks; ECDSA verification 4,000 a
  curve and `registers` 20,000; ECDH 1,000 key pairs a curve (4,200 checks); the P-256 field against Python 444,704 cases (LLVM) and
  69,476 (Cranelift); `pow_mod` against Python 20,000 (LLVM) and 2,000 (Cranelift); RSA against OpenSSL 2,000 and 2,000 flipped;
  ChaCha20-Poly1305 against OpenSSL 20,000 cases, 60,000 checks (LLVM) and 2,000 (Cranelift), and after the rebase onto #463
  20,000 cases with its in-place forms, 120,000 checks (`aead_differential_rebased.txt`); ECDH on Cranelift 100 key pairs a curve.
  **0 differences in all.**
- **Bounds**: `scripts/p256_gen.py` proves every kernel's accumulators and carries (above); `scripts/p256_bounds.py` closes for every
  formula, unchanged, since the element and §4.3's invariants are.
- **Mutants**, all killed or argued: `p256_mutants.py`, the 36 its kernel names select (32 on the kernels, the shared accumulator and
  the constants, rewritten for the new code, and four formula mutants whose names share a fragment), 36 killed at the head;
  `rsa_mutants.py` 26 of 26; `ecdh_mutants.py` 17 of 17 (one stale since #397 re-anchored, P-384's encoding check reached again);
  `chacha20_mutants.py` 28 of 28 after the rebase onto #463 (whose in-place AEAD calls the same `aead_tag`; its eight mutants
  included), and one dropped as equivalent with its argument in the script (a single final carry pass: h1 cannot reach 2^44 after
  one).
- **Constant time**: below.

### 12.3 Constant time

dudect on `gram`, LLVM, `rdtscp`, CPUs 0 and 1 (one physical core, both threads; the script and the program), governor
`powersave` (not changed), 10^6 measurements a test, the classes interleaved; the bar is |t| < 4.5. Raw logs with the commit, machine,
governor and load at start and end are in [`assets/wide-multiply/381/`](assets/wide-multiply/381/).

| path | test | median (TSC cycles) | max \|t\| |
|---|---|---|---|
| P-256 `ecdh.public_key` (b2651fea) | scalar 1 / a fixed scalar / 0x88..88 / n − 1 / 16^63, each against random | 90,541 to 95,517 | 1.35 / 1.61 / 1.29 / 1.78 / 2.10 |
| P-256 `ecdh.shared` | scalar 1 / fixed / 16^63 | 486,934 to 565,173 | 2.14 / 1.59 / 1.98 |
| P-256 `ecdsa_sign.sign` | a fixed key / the key 1 | 213,032 / 215,300 | 1.61 / 2.29 |
| P-384 `ecdh.shared` (`std.bigmod`, 6b5299e5) | scalar 1 / fixed / 16^95 | 2,478,068 to 3,571,888 | 1.82 / 1.34 / 2.10 |
| P-384 `ecdh.public_key` (the same) | scalar 1 / fixed / 0x88..88 / n − 1 / 16^95 | 2,085,241 to 2,593,354 | 1.79 / 1.46 / 2.50 / 1.72 / 2.43 |
| ChaCha20-Poly1305, 64 bytes and 13 of associated data (541a03cc) | seal: a fixed key / fixed data; open: fixed data / a tag wrong first or last, each against random (`gcm_timing.py --chacha20`) | 1,154 to 2,622 | 2.45 / 2.02 / 1.55 / 1.69 |
| the same at 1 KiB (`--size 1024`) | the same four | 2,433 to 20,022 | 2.11 / 2.18 / 2.31 / 2.08 |
| #463's in-place forms (`--chacha20 --in-place`), after the rebase (ed51aed6), 64 bytes, CPUs 0-1 | seal: fixed data; open: fixed data / a tag wrong first or last | 1,461 / 862 / 883 | **7.52** / 2.86 / 1.77 |
| the same, 1 KiB | the same three | 1,917 to 13,053 | 2.08 / 1.54 / 1.35 |

The P-256 runs timed b2651fea; the kernels' object code at the head is that commit's, instruction for instruction
(`kernels_object_identity.txt`). `std.bigmod` has not changed since 6b5299e5. P-384's `public_key` tests ran last (stopped once
to free the core for the other measurements, then run whole). **Not run:** Cranelift; Apple silicon.

**One row fails the bar, and what was done about it.** #463 (the in-place AEAD) merged while this was in review, and its
`--in-place` test of ChaCha20-Poly1305 on the rebased branch gave 7.52 for "seal, data" in its first run. It was then run
against `main` with #463 (the old Poly1305), alternating, 64 bytes, 10^6 a test (`dudect_inplace_main_vs_branch.txt`). On CPUs
0-1 the branch reached 5.11 ("open, tag") and 8.65 ("open, data") in two of six more runs and `main` none in six; pinned to CPU 0
alone, as #463's own runs were (`record-in-place.md` §7.8), the branch passed four of four (largest 2.98) and `main` failed one
of four (5.56, "open, tag"). In "open, tag" the two classes give Poly1305 the same key and the same bytes (only the tag
compared differs), so a failure there is not Poly1305's, and it happened on both builds; #463 recorded 22.26 on an unchanged
function in the same harness on this machine. I read the failures as the shared core's noise, which the in-place harness (its
classes on separate pools of buffers) is more exposed to than the one-shot one, which passed all eight tests on the branch.
That is a reading, not a proof: the table's bar is not met by every run of this row, and a quiet machine was not available.

**The branch audit** (`scripts/chacha20_branches.py` and `p256_ctx.py` over the x86-64 objects; `assets/wide-multiply/381/branches*.txt`):
every P-256 kernel has exactly one conditional jump that is not to a trap, the `je` after `cmp $0xa` and `setge` on the three slice
lengths, the never-taken test of §13.2 of `p256-fast.md`; `canon_*` none. In `std.chacha20` (`aead_driver.cho`) none of the functions
that do arithmetic on the key or the accumulator has one (`poly_block`, `poly_finish`, `le64`, `shr44`, `shr42`, `field25519_51.mac`
and `addw` among them); `poly_blocks`'s jumps are its loop over the message's length and the never-taken test of the block's. In
`std.bigmod` (`ecdh_driver.cho`), `mont_mul`, `ct_reduce`, `add` and `sub` have 29, every one read in the disassembly: the limb
count k (`test`, `cmp $1`, its parity for the unrolled loops), loop counters, and LLVM's overlap and remainder tests of the copy it
vectorised; none tests a limb, a borrow or the mask, which is made without a branch (`setb`, `sete`, `neg`) and passes through
`value_barrier`.
