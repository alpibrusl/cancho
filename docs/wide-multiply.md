# Wide multiply and add-with-carry: three builtins, prototyped on X25519

> **Status: design and prototype.** Issue #381, part of #378 (TLS performance: closing the gap with OpenSSL). The question it
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
- Mutants. `scripts/wide_multiply_mutants.py`: **20 of 20 killed** (the LLVM lowering and its wasm32 path, the Cranelift lowering, the
  edition). `scripts/curve25519_mutants.py`: 38 mutants of the four files, **33 killed**, 5 survive: three of them were known
  (`docs/x25519.md` §5: the X25519 "final swap left out", which is equivalent because clamping clears bit 0, and Ed25519's
  `point_equal`); two are **new, and a loss of coverage**: `std.field25519`'s "`pack` does one trial subtraction" and "`unpack` keeps
  the top bit" were killed by X25519's vectors, and X25519 no longer runs on that field. They are bugs in a module only Ed25519 now
  uses, and Ed25519's vectors do not reach them; a test of `std.field25519` through Ed25519 would. The fifth, "`pack` of the new field does one weak
  carry pass, not two", is, I believe, equivalent: the later `q` step absorbs the one extra bit that a single pass can leave
  (checked on the crafted element of the unit test, not proved for every input bound), and the second pass is kept as margin.

### 10.3 Constant time

| What | x86-64 gram, LLVM | Apple M4 Max, LLVM | aarch64 Linux (Docker on the M4), LLVM |
|---|---|---|---|
| dudect (`scripts/x25519_timing.py`), 10^6 samples a test, fixed scalar / sparse scalar, max abs t (bar 4.5) | 1.94 / 1.78 | 1.69 / 2.47 | 1.40 / 1.21 |
| ctgrind (`scripts/curve25519_ctgrind.sh`, Valgrind Memcheck, scalar marked undefined) | not run: no Valgrind on gram | not run: no Valgrind on macOS | **0 reports** (the planted-`if` control of `docs/x25519.md` §3.1 not re-run; Ed25519 signing, the known exception, 1,300) |

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
- **Not run:** a dudect test of Cranelift's X25519, on either machine. Cranelift does not set `PSTATE.DIT`, and `docs/tls-assurance.md`
  §6.1 found Cranelift failing it for other primitives; the instructions it chooses are the ones above, but no timing test says so.

### 10.4 Speed

`scripts/curve25519_bench.py`, median of five runs of N operations less one, N chosen so a run is about a second; "before" is
the unchanged ladder on `std.field25519` (the same source built as a local module on the same compiler), "after" the final tree. OpenSSL is
`openssl speed ecdhx25519` on the same machine.

| Machine, CPU, load | Backend | before | after | speed-up | OpenSSL |
|---|---|---|---|---|---|
| Apple M4 Max, macOS, idle | LLVM | 0.585 ms | **0.025 ms** | **23x** | 0.019 ms (OpenSSL 3.6.4) |
| | Cranelift | 1.362 ms | 0.071 ms | 19x | |
| aarch64 Linux, Docker on the M4 Max, idle | LLVM | 0.60 ms | 0.026 ms | 23x | 0.019 ms (OpenSSL 3.0.13) |
| gram, Intel i7-1260P, cores 4-5, **load average 6 to 12 from other agents**, seven interleaved runs | LLVM | median 1.87 ms (min 1.32) | median 0.108 ms (min 0.068) | 17x on medians, 19x on minima | 0.0267 ms, measured earlier when quiet |
| | Cranelift | median 3.98 ms (min 2.69) | median 0.398 ms (min 0.336) | 10x | |
| gram, an earlier run before the squaring change, load 3.6 | LLVM | 0.664 to 0.93 ms | 0.051 to 0.063 ms | 12 to 13x | |

gram's absolute figures are not reproducible: it ran at one third of its early speed by the end of the work, other agents' jobs
included one on a sibling hardware thread of the pinned cores. The ratios within one interleaved run are the evidence, not the
milliseconds; the three quiet-time anchors are the early 0.664 ms before, the C figures of §2 (16-bit 0.50 ms, 51-bit 0.050 ms), and OpenSSL's 0.0267 ms.

### 10.5 Where the gain came from

On the M4, LLVM: 0.585 ms before. The new field with the old bit-loop inversion would cost 253 + 251 = 504 field operations for the
inversion against 265; with 2,550 in the ladder that is about 8%, so 1.1x of the 23x is the addition chain, and the squaring
(15 products against 25, measured 8% on the whole) another 1.1x. The rest, about 19x, is the representation:
**25 products in 5 limbs against 256 in 16, and `mul_wide` and `add_carry` letting each be two instructions.** That is more than §2's C
ratio (8.8x) because the code in this language on 16 limbs was 2.4 times slower than the same code in C (loop control, bounds checks and
every limb a load and a store through a slice), and the 51-bit code, being straight-line on locals, has none of that to lose:
0.025 ms is the C `unsigned __int128` figure (0.027 ms) to within 8%.

### 10.6 Verdict

**It reaches 3x, and by a wide margin: 23x on arm64, 17 to 19x on x86-64 under load, 19x on Cranelift/arm64 and 10x on Cranelift/x86-64
(load).** X25519 costs 1.3 times OpenSSL's hand-written assembly on the M4 (0.025 against 0.019 ms) and about 2.5 to 4 times on
gram (0.068 to 0.108 ms under load against 0.0267 ms quiet; the comparison is unfair to this language until gram is quiet).
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
