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
0 takes a mask (`x >> 51 & 0x1fff`); §8 shows it costs nothing in practice.

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
   and fails on any jump, call or return other than the last `ret`. What it found (the function, in full, for x86-64 with
   LLVM; the other three are in the document's §10 results):

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

*(Filled in by the prototype's measurements; see the end of this document.)*

## 11. What this does not do, and what each user would gain

Not done here, and each a decision for a person: P-256, P-384, RSA and Poly1305 are not moved. The estimates are **by
arithmetic from the inner loops, not measured**, and §10's X25519 figure is the only evidence about how well this language turns
such a loop into instructions.

*(To be completed with §10.)*
