# `std.p256`: faster P-256 (#380, part of #378)

> **Status: built; not independently reviewed (#209).** §1 is the profile that came first (the design was committed before the code); §2 to §7 are the design as built, §8 its gates, §9 what it costs, §10 what fell short, §11 the field's multiplications moved onto `mul_wide` (#381). The TLS performance
> work (#378) names P-256 as the largest single cost in a server handshake: `std.ecdsa_sign.sign` is 0.80 ms on an M4 and
> 1.24 ms on the i7 of `gram`, OpenSSL's is 23 µs. `docs/ecdsa.md` §5.4 and `docs/ecdsa-sign.md` §9 said where the time goes
> without measuring it. §1 measures it; §2 to §7 are the design that follows from the numbers; §8 is how it is checked.

---

## 1. Where the time goes

All numbers `python3 scripts/p256_bench.py <driver>` (`tests/programs/p256_profile_before.cho` for this section, built from
`main` at the base of this PR): the time of one operation is `(T(rounds) - T(1)) / (rounds - 1)`, the best of five runs, LLVM
backend, one core. **Machines, with the load when measured:**

- **M4**: Apple M4 Max, macOS 26, native arm64. Load average 4.8 (other agents building): the figures move about 10% between runs.
- **Instructions, which no load moves.** `scripts/tls_perf.py` (`docs/tls-performance.md` §6, which landed on `main` during this
work) counts each primitive's instructions under valgrind. On `gram` (x86-64, valgrind 3.26) at this branch's head:

| Operation (`tests/programs/tls_prims_bench.cho`) | `main` (the recorded baseline) | this branch | |
|---|---|---|---|
| `p256_public` | 23,446,022 | 1,197,245 | 19.6x fewer |
| `p256_shared` | 23,449,254 | 5,002,771 | 4.7x |
| `ecdsa_p256_sign` | 25,330,450 | 1,630,478 | 15.5x |
| `ecdsa_p256_sign_checked` | 50,575,425 | 5,126,392 | 9.9x |
| `ecdsa_p256_verify` | 25,246,146 | 3,488,054 | 7.2x |

These are the five entries of `benches/tls_baseline.json`'s `linux-x86_64` re-recorded in this change, as the guard asks of a
primitive that got more than 1.3 times cheaper; the 28 other operations counted within 2% of their baseline and were left as
they were. **The `linux-aarch64` entries were not re-recorded**: the Docker VM's disk was full and the compiler could not be rebuilt
in it (a build of an older revision of this branch counted the same five about as many times fewer). A `check` on Linux aarch64
will fail on those five until they are; CI runs the guard on x86-64 only.

**Docker arm64**: the image `lexsys-hooks-env` on the same M4 Max, in Docker Desktop's 6-vCPU Linux VM. Load average 4.3.
- **gram**: Intel Core i7-1260P (Alder Lake, performance core 2 of a hyper-threaded pair; `taskset -c 2`), Linux x86-64,
  the `powersave` governor. Load average 5 to 6 (a soak of another project on cores 8 to 15, other agents on 0-1, 4-5 and 6).

| Operation, `main` | M4 | Docker arm64 | gram (i7-1260P) |
|---|---|---|---|
| field multiplication (`bigmod.mul`, mod p) | 115 ns | 114 ns | 167 ns |
| field addition (`bigmod.add`) | 23 ns | 22 ns | 34 ns |
| field subtraction (`bigmod.sub`) | 16 ns | 14 ns | 24 ns |
| inversion mod p (Fermat) | 43.7 µs | 44.7 µs | 64.7 µs |
| inversion mod n (Fermat) | 50.2 µs | 50.1 µs | 73.0 µs |
| `ecdh.public_key` (k·G) | 767 µs | 757 µs | 1,143 µs |
| `ecdh.shared` (k·P) | 779 µs | 746 µs | 1,152 µs |
| `ecdsa_sign.sign` | 803 µs | 808 µs | 1,240 µs |
| `ecdsa_sign.sign_checked` | 1,650 µs | 1,631 µs | 2,476 µs |
| `ecdsa.verify_raw` | 827 µs | 848 µs | 1,231 µs |

### 1.1 Field operations per operation

`python3 scripts/p256_opcount.py` counts the `bigmod.mul`, `add` and `sub` calls in each point function's body, so a count cannot
drift from the source, and composes them as the code runs them:

| | mul | add | sub |
|---|---|---|---|
| RCB complete addition (`ecdh.add_points`) | 14 | 20 | 9 |
| RCB complete doubling (`ecdh.double_point`) | 13 | 15 | 6 |
| Jacobian addition (`ecdsa.add`) | 16 | 4 | 9 |
| Jacobian doubling (`ecdsa.double`) | 8 | 10 | 6 |
| inversion by Fermat | 255 squarings + 127 (mod p) or 168 (mod n) multiplications | | |

| Operation | mul | add | sub | accounted at the M4's unit costs | measured |
|---|---|---|---|---|---|
| `ecdh.shared` / `public_key` (14+64 additions, 256 doublings, one inversion) | 4,810 | 5,403 | 2,239 | 713 µs | 767 to 779 µs |
| `ecdsa.verify` (Shamir, 192 additions expected, two inversions) | 5,954 | 3,335 | 3,274 | 814 µs | 827 µs |
| `ecdsa_sign.sign` (the ladder and two inversions) | 5,239 | 5,400 | 2,238 | 762 µs | 803 µs |

**The field operations account for 93 to 98% of every operation.** The rest is the copies of a point (`copy_point`), the
table select, `bigmod.setup` and the loops around them. So the cost is in the arithmetic, and the question is how cheap a
field operation can be:

- **Inversions are 5 to 13% of an operation**: 43.7 µs of 767 for `public_key`; the two of a signature, 94 µs of 803 (12%);
  the two of a verification, about 94 µs of 827. They are small now and become the largest single item once the ladder is
  fast (§5).
- **The scalar multiplication loop is the rest**: 4,810 multiplications at 115 ns is 553 µs of a 767 µs `public_key`
  (72%); the additions and subtractions, 7,642 of them at 14 to 23 ns, are 150 µs (20%).
- **A subtraction or addition costs 14 to 23 ns against 115 ns for the multiplication: a fifth.** There are 1.6 of them for every
  multiplication, so they are a fifth of the time and, once the multiplication gets cheaper, they would be the largest part (§4.5).

### 1.2 Inside one `bigmod.mul` (115 ns on the M4)

`mont_mul` is the CIOS loop of `docs/rsa.md` §2.1 on 30-bit limbs: for P-256, 9 limbs, 162 multiply-adds in loops of a count
read from memory, with the temporary and the operands in memory, then `ct_reduce` (the masked subtraction of §2 of
`docs/ecdh.md`) and a copy.

- **The reduction is 10%.** The same module with `ct_reduce` removed (a scratch copy, wrong answers, to time only): 103 ns
  against 115. So `docs/ecdh.md` §6's "+11%" for ECDSA verification was this, and no part of the remaining 103 ns is the
  masked reduction.
- **The loop is the rest, and the loop is the language's checked arithmetic.** A straight-line P-256 multiplication on 10
  limbs of 28 bits (prototype of §4.2: the same code written with `+` and `*`, then with `wrapping_add` and `wrapping_mul`),
  100 + 70 multiplications, operands in registers:

| One field multiplication | M4 | gram (i7-1260P) |
|---|---|---|
| `bigmod.mul`, generic | 115 ns | 167 ns |
| straight-line, checked `+` and `*` | 55 ns | 65 ns |
| straight-line, `wrapping_*` | **23 ns** | **44 ns** |

So of the 5-fold on the M4 (3.8-fold on the i7), about 2 is going from loops over memory to straight-line code, and 2.4
(1.5 on the i7) is not testing every addition and multiplication for overflow. §4.3 is why the second is safe here.

---

## 2. What the language allows

- **`int` is 64 bits, signed, and traps on overflow**; `wrapping_add`, `wrapping_sub` and `wrapping_mul` do not. There is no
  128-bit product and no carry flag. A limb product must fit 63 bits with room to add: the widest limb for a *fused* step
  is 30 bits (`docs/rsa.md` §2.1), and for a column sum of ten products, 28.
- **No allocation per call**: `work` is the caller's `[int]`. A table the size of the generator's cannot be in `work`, because
  the caller's `work` is zeroed after every call (§6) and filling it costs more than the signature. **A `static` holds it**
  (`docs/compile-time-data.md`): evaluated while compiling, read-only data in the binary, indexed like any slice. Built from a
  hex string by a loop, a 10,400-word static takes 0.14 s to compile and nothing at run time (measured).
- **A function a `static` calls is emitted into every program built with `--std`**, used or not
  (`modules::std_declarations_cost_nothing_unless_called` found it: the first version of the table module called a helper
  to decode its hex digits). So the tables' bodies hold their literal and decode it inline, and `std` stays free when
  unused. Found, and fixed here, not fixed in the compiler.
- **A builtin for a wide multiply may land later** (#378, another change). Nothing here depends on it, and §4.2 says where it
  would be used.

## 3. The steps, and what each was worth

Each step was measured when it was built (M4, LLVM, `scripts/p256_bench.py`, load average 4 to 8, so each figure moves
about 10%) and is kept because it paid. "Expected" is what this document's first commit computed from §1's parts.

| Step | What | Expected | Measured (cumulative) |
|---|---|---|---|
| 0 | `main` | | `public_key` 767, `shared` 779, `sign` 803, `verify` 827 µs |
| 1 | Ten-limb 28-bit field, straight-line kernels, lazy reduction; the ladder on it (§4) | ladder 767 → about 170 µs | `shared` 161, `public_key` 162 µs; `sign` 229 µs |
| 2 | Arithmetic modulo n on the same kernels (§5) | inversions 94 → about 20 µs | inversion mod p 6.1, mod n 9.8 µs; `sign` 183 µs |
| 3 | Fixed-base table for k·G, signed four-bit digits (§6) | `public_key` about 40 µs | `public_key` 39.5, `sign` 62.7 µs |
| 4 | Verification by wNAF, Shamir's trick, the generator's odd multiples (§7) | 1.3 times fewer multiplications on top of step 1 | `verify` 124 µs |
| 5 | Slices for the operands (one bounds check, not ten), no copy loops, a projective final check | none computed | `shared` 141, `public_key` 34, `sign` 59, `verify` 105 µs |

Every expectation held or was beaten: step 1 and step 3 within 3%, step 2 a little better, step 4 (computed 112 to 130 µs) at
124. The issue's expectation (signing 3 to 4 times faster, verification about 2 times) was a fraction of what the field
alone gives.

*Tried and measured, not kept:*
- **The special form on 30-bit limbs** (9 limbs, row by row): 27 ns a multiplication against 23 for 10×28 (§4.1).
- **Checked arithmetic in the kernels**: 55 ns against 23 (M4), 65 against 44 (i7). The `wrapping_*` is proved (§4.3).
- **A dedicated squaring**: 18 to 21 ns against 22 to 23 for a multiplication, kept (55 products against 100, but the
  reduction and the carries are the same).
- **Addition and subtraction without the carry pass** (timing only, wrong answers): an add and a subtract in 7 ns against 16.
  Not built: it needs the bounds script to track limb widths as well as values, and the ladder's additions are a fifth of its
  time, so it is worth at most 10% for a change to the part that must stay provable. Left as §10's first item.

---

## 4. The field

### 4.1 Representation

A field element is **ten limbs of 28 bits**, least significant first, one limb to a word of the caller's `work`, in **Montgomery
form with R = 2^280**; the same for the group order n. 28 bits and not 30 because:

- **A column of products must fit in 63 bits.** Product scanning (Comba) sums a whole column in one register before taking its
  carry, which removes the carry chain that makes the row-by-row loop slow. Ten products of two 28-bit limbs and the ten of
  the reduction are below 2^61 (proved by the generator, §4.3). With 30-bit limbs, nine products of 2^60 overflow 2^63.
- **R = 2^280 is 2^24 times p.** A multiplication answers below p + x·y/R, which is under 2p for any inputs up to 4096 p. So no
  addition or subtraction needs to reduce, and the formulas can add and subtract freely (§4.4). With 9 limbs of 30 bits
  (R = 2^270) the bound is 128 p; with 9 of 29, 5 p.
- Measured: a 9×30 row-wise kernel (the same special-form reduction) costs 27 ns on the M4 against 23 ns for 10×28.

### 4.2 Using the prime's form

p = 2^256 − 2^224 + 2^192 + 2^96 − 1 is −1 modulo 2^28, so −p⁻¹ = 1 and **the Montgomery quotient digit is the low limb
itself**: no multiplication by n′. In radix 2^28 its limbs are 2^28−1 (limbs 0, 1, 2 and 8), 2^12−1, 2^24 and 2^4−1 (limbs 3, 6, 9)
and zero (4, 5, 7). So `m·p_j` is a shift and a subtraction, and a column's reduction terms are summed first, then shifted
once: **no multiplication at all in the reduction**, on top of the 100 of the product. The generator (`scripts/p256_gen.py`) writes
it; the same generator writes the kernels for n, whose limbs are not special, with the multiplications. That is "Montgomery with
the special-form simplification" and not Solinas reduction: Solinas needs the 32-bit words of 2^256's structure, and a 32-bit
limb's product (2^64) does not fit this language's signed 64-bit `int`.

*Where a wide multiply would be used.* The product of two 28-bit limbs fits in `int`. A builtin giving the high and low
halves of a 64×64 product would let the limbs be 64 bits (4 limbs, 16 products), and this module's kernels would be rewritten
on it by changing the generator; nothing outside `std/p256_kernels.cho` (generated) would change, and its tests
(`tests/vectors/p256.txt`) would be the check. *Corrected (#381, §11): four 64-bit limbs would change more than the kernels.
R = 2^256 leaves no headroom for §4.4's lazy reduction, and the representation is read outside them (the comb's 10,400 words,
`p256_vf`'s wNAF digits, `load`, `store`, `pow`'s nibbles). What was built is five 56-bit limbs inside the kernels only, each
packing two of the 28-bit limbs, R unchanged; §11 measured both.*

### 4.3 `wrapping_*`, and the proof that it cannot wrap

`bigmod` is checked arithmetic on purpose (`docs/ROADMAP.md` #234: "an overflow would be a bug, so it traps"). That costs a
branch after every addition and multiplication, and §1.2 measured it at 2.4 times the multiplication on the M4. The kernels
here are `wrapping_*` because **the generator computes, column by column, the largest value the accumulator can hold when every
limb is at its maximum (2^28 − 1) and refuses to write the file if any reaches 2^63**. The largest is 2^60.0 (the multiplication
modulo n, column 9). So the `wrapping_*` is a statement about code that is straight-line and has no data dependence: the same
bound holds for every input. A wrong bound is a failed generation. And it is tested at the bound: `tests/vectors/p256.txt` has the
four kernels on operands of 280 bits with every limb 2^28 − 1 and other extreme limb patterns, against the exact
identity (x·y + m·p) / R in Python. `std.field25519` made the same choice (its header). Everything else in `std.p256` is checked
arithmetic.

Invariants, stated in `std/p256_kernels.cho`'s header and checked by `scripts/p256_bounds.py` against every formula:
- every input is tight (limbs below 2^28) and every output is tight;
- `mul` and `sqr` answer below p + x·y/R: under 2 p for inputs whose product is below 2^24 p² (4096 p each), and under 1.001 p
  when both are below 128 p, which is every input the formulas give (§4.4: at most 66 p). *Corrected in review: this line said "inputs up to 4096 p, answer below 1.001 p", which holds only for the second case.*
- `add(x, y)` answers x + y, tight, not reduced;
- `subK(x, y)` answers x − y + K p, for y below K p, K = 2, 4, 8, 16, 32 or 64, tight; it needs no mask, no branch, no borrow
  handling: every limb of the constant is above 2^28, so no limb goes negative before the carry pass;
- an element stays below 2^280, which is 16 million p.

### 4.4 No reductions inside the formulas

The formulas of `docs/ecdh.md` §2 add and subtract between multiplications, up to six deep. `scripts/p256_bounds.py` parses each
formula's source (`add_points`, `mixed_add`, `double_point`, the Jacobian `double`, `add_jac`, `madd`, the final check, the
curve equation), propagates an upper bound through every `p256.mul`, `sqr`, `add`, `copy` and `subK`, and fails if a
multiplication input exceeds 4096 p, a subtrahend exceeds its K, or a value 2^280. Functions whose outputs feed each other's
inputs form a group, and the script finds the smallest bound B such that every output is at most B when every input is: 5 p for
the complete formulas of the ladder and the table, 32 p for the Jacobian ones (the largest product is 4,356 p², against a limit
of 16.7 million, the largest value 66 p against 16.7 million). With `--rewrite` it also chooses each bare `p256.sub`'s K. The only
full reductions are `from_mont`, which multiplies by 1 (the result is at most p) and subtracts p once under a mask
(`value_barrier`, `docs/value-barrier.md`), for the output of an operation and for the comparisons that need an exact value.

### 4.5 Additions and subtractions

An addition or subtraction is ten limbs and a carry pass, 5 to 8 ns on the M4. With 1.6 of them for every multiplication they
are about a fifth to a third of the ladder's time (§3 lists the carry-free version as measured and not built).

---

## 5. Inversion and the group order

Fermat's a^(p−2) and a^(n−2), the exponents public, so their windows may be read from memory. Fixed four-bit windows over the
exponent's nibbles: 252 squarings, 14 multiplications for the table and one per nonzero nibble (31 for p − 2, whose runs of
zeros are long, 53 for n − 2). Measured on the M4: p 5.7 µs, n 10.1 µs (against 43.7 and 50.2). An addition chain exploiting the
runs of ones in p − 2 would save about 33 multiplications (0.8 µs of 6): not worth its code.

Signing's `s = k⁻¹ (e + r·d) mod n` is `smul`, `sadd`, `sinvert` and the conversions on the same kernels with n's constants: no
second Montgomery implementation.

## 6. Fixed-base multiplication by the generator

**Signed four-bit digits and a table of every digit at every position.** k = Σ dᵢ·16ⁱ, dᵢ in [−7, 8], 65 digits (the recoding
carries: a nibble above 8 becomes negative and adds one to the next; *corrected in review from "[−8, 8]": −8 never occurs, 8 does*). The table holds (j+1)·16ⁱ·G for i in 0..64, j in 0..7: 520
affine points, in Montgomery form, 83 KB as a `static` of 10,400 words (`std/p256_comb.cho`, 79 KB of hex). **k·G is then the
sum of 65 table entries, no doubling at all**, each a mixed (affine) addition, RCB algorithm 5, 11 multiplications.

**Constant time, for the secret k:**
- the recoding is arithmetic on the nibbles (`(v + 7) >> 4`), no comparison;
- a digit's sign and magnitude come from a shift and an xor, the masks through `value_barrier`;
- **the entry is chosen by reading all eight under masks**, each ORed in under a mask that is all ones for the one wanted
  (`docs/ecdh.md` §2); no address depends on the digit. Each of the 65 positions is a different part of the table, so the
  position is public and only the entry within it is secret;
- the sign is applied by selecting between y and 2p − y under a mask, never by a branch;
- **a zero digit** adds a dummy, the position's first entry (16ⁱ·G, a point of the curve), and keeps the old sum under a mask:
  the same operations whatever the digit, and never zero operands. *Corrected in review: the first version added an all-zero
  entry. That was the same instructions, but zero operands are what `docs/ecdh.md` §3 measured leaking through data-dependent
  timing (|t| = 16 on the M4); a scalar whose digits are all zero but the top one is now one of the timing tests (§8.3).*
- the formulas are complete, so a sum equal to the entry, or the point at infinity, needs no case.

The table is generated by `scripts/p256_tables.py` from Python's integers and checked against OpenSSL's scalar multiplication
(552 points, `--check`); the file's currency is a test.

## 7. Verification

Public data, so variable time is allowed. u1·G + u2·Q with one doubling chain (Shamir), **wNAF**: u1 in width 7 against a table of
the generator's odd multiples 1G, 3G, …, 63G (32 affine points, the second `static`; ≈ 1/8 of the bits nonzero, mixed
additions); u2 in width 5 against 1Q, 3Q, …, 15Q built per call (7 additions and a doubling). Jacobian coordinates with the
doubling of dbl-2001-b (8 multiplications), add-2007-bl and madd-2007-bl, and the cases they do not cover (an equal pair, an
opposite pair) handled by the caller of the formula, as `std.ecdsa` did; whether a lazy value is zero modulo p is asked of its
reduced form, whether the sum is the point at infinity is a flag. About 256 doublings and 75 additions, against 256 and 192
before. **The final check has no inversion**: x = X/Z² equals r modulo n when X = r·Z², or X = (r+n)·Z² when r + n < p (a case
with probability 2^-128 that no real signature reaches, so `check_x` and vectors test it on its own). The inversion mod n for
u1 and u2 stays. P-384 verification is unchanged: it stays on `std.bigmod` (§10).

---

## 8. Constant time, and the gates

### 8.1 The argument, function by function

| Function | Secret | Why it is constant time |
|---|---|---|
| `p256_kernels.mul_*`, `sqr_*`, `add`, `sub_K` | the operands | straight-line, every loop unrolled: no branch, no index |
| `p256_kernels.canon_*` | the value | the borrow over ten limbs, then the subtraction under a mask through `value_barrier` |
| `p256.load`, `store` | a scalar's bytes | the limb a byte lands in, and whether it straddles two, depend on its position; no range check (the caller made a constant-time one) |
| `p256.invert`, `sinvert` (`pow`) | the operand | the exponent p − 2 or n − 2 is public; a window of zero is skipped because the *exponent's* is zero |
| `p256.is_zero` | the value | every limb ORed, one branch on the answer (it used to stop at the first nonzero limb; `affine` asks of a secret's Z) |
| `p256_pt.select`, `multiply` | the scalar | as `docs/ecdh.md` §2: all sixteen entries read under masks, complete formulas |
| `p256_pt.recode`, `comb_multiply` | k | §6 |
| `ecdsa_sign.finish` | d, k | `load`, `sto_mont`, `sinvert`, `smul`, `sadd` as above; `is_zero` of r and s is on public values (`docs/ecdsa-sign.md` §2.1) |
| `p256_vf.*` | none | public data; nothing here is constant time |

What stays variable time, all of it on public values, is what `docs/ecdh.md` §2 listed: the peer's point checks, the infinity test of
the result (which a valid input never reaches), the exponents, the scalar range check's answer.

### 8.2 The audit of the x86-64 object

`scripts/chacha20_branches.py` over the objects of `tests/programs/ecdh_timing.cho` and `ecdsa_sign_timing.cho` (LLVM, on
`gram`), every non-trap conditional jump read in the disassembly (`scripts/p256_ctx.py` prints each with the instructions
before it): 69 non-trap jumps in all (`ecdh.o`) and every one read. None tests a secret.
- **Loop counters, lengths and the code of a call** (`cmp $K,%reg` against 8, 20, 30, 65, 0x40, a slice's length): `select`,
  `multiply`, `comb_multiply`, `recode`, `base_multiply`, `point_multiply`, `store`, `load`, `wipe`, `check_p256`.
- **`p256.load` and `p256.store`'s `cmp $0x15,%rcx; jb`**: `off > 20`, a byte's position in its limb.
- **`p256.pow`**: the `started` flag, `scalar` (which field), and `nib != 0`: all of the public exponent.
- **`comb_multiply`'s one `test %cl,%cl; je`** is not a branch on a digit: `cl` is `setb`/`setb`/`and` of two *pointer* comparisons,
  the overlap test LLVM makes before it vectorises the masked OR loop (the entry in `work` against the row of the static).
  The mask in the loop is `and`/`or` on `xmm` registers.
- **`cmovb`, `cmovne`, `cmovae`, `cmovle`**: `max(len - 0x370, 0)` and similar lengths, the hexadecimal digit of a curve constant, and
  `check_p256`'s answer of `in_range` (a public result of a constant-time comparison, as `docs/ecdh.md` §3).
- **`affine`'s `cmpq $0x0,...; jne` chain** was `is_zero` stopping at the first nonzero limb of a secret's Z: a leak of the position of
  the first nonzero limb, at probability 2^-28 a limb. It is now an OR of all ten limbs and one branch on the answer (§8.1).
- **`p256_kernels.*`** (mul, sqr, add, sub_K, canon): no conditional jump except the overflow and bounds checks to `ud2`; `canon_*` has
  none and its mask is `value_barrier`'s.
- **`ecdsa_sign.finish`**: `is_zero` of r and of s (public once sent) and `store`'s loop; `sign`: the lengths, the attempt counter and the codes
  of public checks.

*Re-read after the review's fixes (commit 3b71b529, `ecdh_timing.cho` and `ecdsa_sign_timing.cho` again):* `comb_multiply` now
has five non-trap jumps: the counters of its three loops (`cmp $0x8`, `$0x14`, `$0xf0`, then the position), and the overlap test,
now `test %sil,%sil` after the same two `setb` and an `and`. The dummy's selection, `eq_mask(j + 1, mag) | eq_mask(j, 0) &
eq_mask(mag, 0)`, compiled to `lea`/`and`/`or`/`sar`, no branch; the one `test %cl,%cl; jne` beside it goes to `ud2` (a hoisted
overflow check, `seto`). The length checks added to `base_multiply`, `point_multiply` and `finish` compare lengths. `finish` and
`sign` otherwise as above; `sign`'s new `test $K,%r9; jne` is the wipe loop's remainder (it now zeroes all of `work`).

### 8.3 Timing: dudect on `gram`

*Which code was timed.* The branch was squashed to two commits before merging, because `scripts/history.py --explain` (the
stability plateau) replays every revision of every `std/` file and the development's intermediate ones would not read today. The
commits the evidence names (3b71b529 timed, d0b35f70 and 0c5e1930 mutated) are kept on the branch `p256-faster-unsquashed`; the
blobs of every file they timed and mutated (`std/p256*.cho`, `ecdh`, `ecdsa`, `ecdsa_sign`, `bigmod`, `hmac`, `crypto`, and
`tests/programs/ecdh_timing.cho` and `ecdsa_sign_timing.cho`) are byte for byte this commit's: `git diff 3b71b529 <head> -- <file>`
is empty for each, and `p256_kernels.cho` is blob 258095cda311, `p256_pt.cho` 320680bcc0ba, `ecdsa_sign.cho` b643f7c30b13.

**The runs, raw:** `tests/evidence/p256/raw_public_key.txt`, `raw_shared.txt` and `raw_sign.txt`, each with its header: the commit
timed (3b71b529; `git diff 3b71b529 HEAD -- std tests/programs` is empty), the machine, the governor, and the load at its start and
end. Intel Core i7-1260P, Linux x86-64, LLVM backend, `rdtscp`; `taskset -c 2-3` (one physical core, both of its threads, for the
script and the program it runs); **the governor of cpu2 and cpu3 set to `performance` for the runs and back to `powersave` after**.
The machine was shared with other jobs on its other cores: **load average 4.3 to 8.0** during the runs (the headers have each). The
classes are interleaved within each batch, so what that load does to the time it does to both; it is why the medians move between
tests run at different times. Fixed against random, 10^6 measurements a test, batches of 50,000. **The gate is |t| < 4.5.**

| Function | Test | Measurements | Median (TSC cycles) | max \|t\| |
|---|---|---|---|---|
| `ecdh.public_key` P-256 (the table) | scalar 1 against random | 1,000,000 | 181,964 | **2.21** |
| | a fixed scalar against random | 1,000,000 | 182,481 | **1.85** |
| | scalar 0x88..88 (every digit 8) against random | 1,000,000 | 325,371 | **2.21** |
| | scalar n − 1 (every digit carried) against random | 1,000,000 | 306,205 | **2.32** |
| | scalar 16^63 (every digit zero but the top: the dummy added 64 times, the sum at infinity throughout) against random | 1,000,000 | 330,297 | **2.18** |
| `ecdh.shared` P-256 (the ladder) | scalar 1 against random | 1,000,000 | 978,555 | **1.73** |
| | a fixed scalar against random | 1,000,000 | 785,486 | **1.78** |
| | scalar 16^63 against random | 1,000,000 | 799,812 | **1.70** |
| `ecdsa_sign.sign` | a fixed key against random | 1,000,000 | 253,981 | **2.40** |
| | the key 1 against random | 1,000,000 | 260,715 | **2.25** |

All ten pass. *Corrected in review:* the first runs of this section were at the `powersave` governor, of code two commits older than
the head they were reported for (a commit had since changed `p256.init` and the constants' layout), and their output was not
committed; their figures (all |t| ≤ 2.07) are replaced by these. **Not timed: Cranelift, and Apple silicon on either backend.**
`docs/tls-assurance.md` §6.1 found scalar 1 failing on the M4 for both backends through the CPU's data-dependent timing (Arm's DIT
bit off); nothing here changes that, and the new P-256 code was not run there.

### 8.4 Mutants

`python3 scripts/p256_mutants.py`, on `gram` (Linux x86-64, `taskset -c 2,3`, pyca/cryptography 46 and `openssl` 3.0.13), at commit
d0b35f70 (the code of the head; the commits since change the driver, the vectors and documents). The raw output is
`tests/evidence/p256/mutants.txt`. **125 mutants: 116 killed, 9 survived, 0 failed to build.**

- **Eight survivors are argued equivalent** in `EQUIVALENT` (each argument is printed beside it in the output): `(v + 8) >> 4` in the
  recoding; the infinity test of `affine` (no valid input reaches it); the wNAF window threshold (the window is odd, the threshold
  even); the early carry (it lands in bits that are skipped); `load_q`'s `k/2` for `(k-1)/2` (k is odd); u1's digits at width 5 (still
  in the table); the fast path skipped (the old path agrees, which is itself a check); `sfrom_mont` unreduced (n·G = O).
- **One was a test gap:** `sign` zeroing only the table's 980 words of `work`. The driver's check looked at those words only; it now
  checks all of `work` after `sign`, and the mutant run alone is killed (`tests/evidence/p256/mutants_rerun.txt`, commit 0c5e1930).

*Corrected in review:* this section said the run was in the arm64 container and on `gram` both. The first full run (127 mutants,
111 killed) was in the container; it found the gaps since closed (the top limb of `is_zero`, the wipes, an equivalent straddle offset
I had written); its output was not kept. The run above is the one this section reports, and the review's "r not reduced mod n" is
now killed by `ecdsa_sign.finish` called with x in [n, p).

### 8.5 Vectors and differentials

- **Every existing vector, byte for byte:** RFC 6979 A.2.5 on both backends; Wycheproof (`ecdh_secp256r1_ecpoint`, 355 cases; ECDSA,
  1,370 valid and 1,728 invalid); NIST CAVP KAS and SigVer; the key-parser files; the x509 matrix; the TLS suites of `cargo test`.
- **`tests/vectors/p256.txt`** (1,823 cases, both backends, in `cargo test`): the field and group-order arithmetic against Python's
  integers; k·G of 100-odd edge and random scalars (8 and 9 in every nibble, n − 1, 2^255, the refusals of 0, n and 2^256 − 1); the
  wNAF digit for digit; signatures crafted for chosen u1 and u2 under the keys 1, 2, n − 1 and a random one, so that Shamir's trick
  adds equal and opposite points and reaches infinity (`-40`), with u1 = 0 too; r and s at the edges of the limb representation;
  the final check of `r + n`; the four kernels at the worst case of their accumulators; `ecdsa_sign.finish` with x in [n, p),
  which no real k·G reaches; every new public entry point given wrong lengths, which must answer its code and not trap.
- **Python, uncapped** (`scripts/p256_field_differential.py <driver> 20000`): 276,671 cases on LLVM, 41,693 on Cranelift, 0 differences.
- **OpenSSL on `gram`** (Linux x86-64, pyca/cryptography 46, OpenSSL 3.5.5 behind it and `openssl` 3.0.13 on the command line):
  `ecdsa_sign_differential.py openssl` **10,000 signatures accepted by both, 10,000 flipped refused by both, 0 differences**;
  `reference` (RFC 6979 in Python, byte for byte) 10,000, 0 differences; `keys` 100 keys, 600 checks, 0 differences;
  `ecdsa_differential.py openssl` 4,000, `registers` 20,000, 0 differences; `ecdh_differential.py` 1,000 key pairs a curve, 4,200
  checks, 0 differences.

### 8.6 What it found

- **A `static`'s callee is emitted into every program** (§2).
- **The lazy zero:** a lazy element has several representations of zero (0, p, 2p), so `bigmod`'s `is_zero` idiom is wrong on it;
  the Jacobian formulas ask the reduced form, and a mutant that skipped the reduction (`sfrom_mont` unreduced) survived every vector, including the digests 0 and n: the value n instead of 0 reaches
  the wNAF, which turned out harmless (n·G is the point at infinity) and is now argued equivalent.
- **The equal mutants:** a byte's offset in a limb is a multiple of 4, so "straddles when the offset is above 20" and "above 21" are
  the same test (an equivalent mutant I wrote and the run reported).
- **Two lines of the first design were wrong:** the duplication test found `eq_mask` and `copy_point` copied between files, and
  the formatting test found that `cancho fmt` drops parentheses the generators had written.

## 9. Cost

`python3 scripts/p256_bench.py <after> --vs <before> ...`: each driver run alternately five times for every operation, the best of five of each,
LLVM backend, one core, minus the run of one round. `<before>` is `tests/programs/p256_profile_before.cho` built by `main`'s compiler
(the worktree `origin/main` at the base of this PR), `<after>` is `p256_profile.cho` built by this branch's.

**Apple M4 Max, macOS 26, native arm64** (load average 12.7 to 13.5, other jobs building; three passes, the median shown, passes within 3%):

| Operation | before | after | |
|---|---|---|---|
| field multiplication mod p | 117 ns | 20 ns | 5.8x |
| field squaring mod p | 117 ns | 19 ns | 6.1x |
| addition + subtraction mod p | 36 ns | 13 ns | 2.7x |
| inversion mod p | 44.4 µs | 5.8 µs | 7.7x |
| inversion mod n | 51.9 µs | 10.0 µs | 5.2x |
| multiplication mod n | 112 ns | 32 ns | 3.5x |
| `ecdh.public_key` (k·G) | 768 µs | 34.6 µs | **22x** |
| `ecdh.shared` (k·P) | 755 µs | 138 µs | **5.5x** |
| `ecdsa_sign.sign` | 820 µs | 59.2 µs | **13.8x** |
| `ecdsa_sign.sign_checked` | 1,644 µs | 163.6 µs | **10.1x** |
| `ecdsa.verify_raw` | 814 µs | 105.8 µs | **7.7x** |

**Intel Core i7-1260P (`gram`), Linux x86-64, performance core 2 (`taskset -c 2`)**: the machine was shared with other jobs the whole
time (load average 6 to 13, the core at 0.5 to 1.8 GHz under `powersave`, on a sibling thread busy with someone's build), so the absolute
times of three passes ranged over a factor of 2 to 4. The ratio of each interleaved pair is steady, and is what is reliable; the times
are the fastest of the three passes:

| Operation | before | after | ratio (range of the three passes) |
|---|---|---|---|
| field multiplication mod p | 255 ns | 64 ns | 3.8 to 4.0x |
| `ecdh.public_key` | 1,575 µs | 84 µs | 16.7 to 18.6x |
| `ecdh.shared` | 2,052 µs | 577 µs | 3.6 to 4.2x |
| `ecdsa_sign.sign` | 1,716 µs | 124 µs | 13.6 to 14.5x |
| `ecdsa_sign.sign_checked` | 3,508 µs | 379 µs | 8.6 to 9.3x |
| `ecdsa.verify_raw` | 1,742 µs | 258 µs | 5.9 to 7.0x |

An earlier, quieter minute (load 5.8, `docs/p256-fast.md` §1's first table) gave `main` at 1,143 µs for `public_key`, 1,240 µs for `sign` and
1,231 µs for `verify`; the prototype multiplication on that machine was 44 ns against 167 ns generic. So an unloaded i7 signs in about
a tenth of 1.24 ms by the ratio, which is not a measurement. **Not measured on a quiet core: it was not available.**

**Docker arm64** (the image `lexsys-hooks-env`, a 6-vCPU Linux VM on the same M4 Max): `main` before, measured early (load 4.3):
`public_key` 757 µs, `shared` 746, `sign` 808, `sign_checked` 1,631, `verify` 848, multiplication 114 ns, inversions 44.7 and 50.1 µs. The
**after** was not measured: the VM's disk filled (other jobs' images; 63 GB of 90, 36 GB of volumes) while the mutant run was in it,
and no container could start. What ran in it before that: the whole `cargo test --workspace --no-fail-fast` of this branch, Linux
aarch64, **627 passed and 2 failed** (`project::a_compiler_other_than_the_one_named_is_refused` and
`vcs_remote::a_lock_holds_a_commit_and_never_a_name`, which need a git checkout the container does not have: the worktree's `.git`
points outside it); every p256, ecdh, ecdsa and tls test passed. The M4 above is the same CPU natively.

**For reference**, OpenSSL 3.0.13 on `docs/ecdsa-sign.md` §7's i7: `ecdsap256` signs in 23 µs and `docs/ecdsa.md` §5.4's Xeon verifies in 75 µs. Against
those, on the M4 (not the same machine): signing is 2.6 times OpenSSL's time, down from 36 times (a different machine, so indicative).

**The check after signing (`docs/tls-server.md` §3.3), re-measured.** `sign_checked` less `sign` is 104 µs on the M4: the check is now
64% of the combined cost, where it was 49% (it was equal to the signature on the i7). Re-measured after the review's fixes (`sign` now zeroes all of `work`, 9,593 words; the comb's dummy is a point): `sign` 58.9 µs,
`sign_checked` 164 µs, `public_key` 35.5 µs, the same within the noise. A full handshake that signs and checks now spends
164 µs there, where it spent 1.64 ms. The check stays, as the design decided: a fault during signing leaks the key, 104 µs is 0.1 ms of a
handshake whose other costs are the TLS engine's, and it is verification alone that is now the larger half.

## 10. Not done, fell short, not verified

- **Carry-free addition and subtraction** (§3): at most 10% of the ladder; it needs the bounds script to track limb widths.
- **P-384** stays on `std.bigmod`: its field would be 15 limbs of 28 bits (225 products) and a table twice the size; the kernels
  are generated, so it is a generator parameter, but no program here signs or exchanges on P-384 in a hot path
  (`docs/tls-server.md` §6: 6 ms a handshake is the P-384 client's choice, not the server's).
- **Against OpenSSL**: signing is still about 2.5 times its time (23 µs on the i7 of `docs/ecdsa-sign.md` §7 against §9's figure), and
  the ladder (`shared`) about five times; both are field-multiplication bound at 20 ns, and OpenSSL's are assembly with a 64-bit
  multiply. *Since #381 (§11.4): on `gram`, in cycles against OpenSSL 3.5.5 on the same core, signing 3.2 times, `shared` 3.2 times,
  a verification 1.8 times.*
- **Not timing-tested:** `p256_vf` (public data) and the key parser (as before). Cranelift was not timed (`docs/ecdh.md` §3 found its
  P-384 noisy; P-256 was not re-run on it here).
- **Not independently reviewed** (#209), and not run: the server interop matrix of `docs/tls-server.md` §10.2 needs the container of
  `scripts/interop/server.Dockerfile` with Go, wolfSSL and mosquitto; CI's `tls-assurance` job runs it (see the PR).

---

## 11. The field on `mul_wide` (#381)

> #381 (of #378) asked for P-256's field on the wide-multiply builtins of [`wide-multiply.md`](wide-multiply.md) (#393), and for
> the choice between four 64-bit limbs (#397's estimate: 16 `mul_wide` a multiplication against 100 products, 10 a squaring
> against 55, at the cost of the lazy reduction's headroom) and five limbs to be made from measurements. §11.1 is what was measured
> first, §11.2 the design that follows, §11.3 its gates, §11.4 its cost. Machine names and loads as §1; `gram` figures are cycles
> (`cpu_core/cycles/u` through `sudo perf stat`, CPU 0 pinned, the least of five runs a driver, two drivers alternating) and
> instructions (exact), because its clock moves under `powersave` and other agents' jobs; `kernel.perf_event_paranoid` was not
> changed (it reads 4; `perf` ran under `sudo`).

### 11.1 Measured before choosing

Three kernels for p, each `p256_bench.py`'s `m`, `q` and `p` loops on `gram`: today's 28-bit field (`main`), five limbs of 56 bits
inside the kernels (each packing two of the 28-bit limbs, so R stays 2^280 and nothing else changes), and a prototype of four
64-bit limbs with R = 2^256 (`mul_wide` product scanning into a three-word accumulator, four reduction rounds, the final
conditional subtraction; add and subtract fully reduced, as R = 2^256 forces: there is no headroom above p). The prototype was
checked against Python on chains of 1,000 operations; it is not in the tree, its figures are
[`assets/wide-multiply/381/prototype_4x64.txt`](assets/wide-multiply/381/prototype_4x64.txt).

| kernel (x86-64, `gram`) | 10 × 28 bits (`main`) | 5 × 56 bits | 4 × 64 bits, prototype |
|---|---|---|---|
| multiplication mod p | 158 cycles, 782 instructions | 98, 451 | **86**, 332 |
| squaring mod p | 114, 573 | **81**, 345 (dedicated) | 84, 331 (a multiplication) |
| an addition and a subtraction | 75, 390 | 36, 220 | **30**, 117 (each reduced) |

On the M4 the first run (load 4.7; later ones, at load 8 to 17, moved by up to two times and are not used) gave 16 ns for the
5 × 56 multiplication (before §11.2's `wrapping_*` additions and length test) against 14 for 4 × 64, 14 against 15 for the squaring, and 13 against 9 for
the pair.

**The choice: five limbs of 56 bits.** Four 64-bit limbs were 12% cheaper on the multiplication and 17% on an addition and a
subtraction, which on the ladder's 4,810 multiplications and 7,642 additions and subtractions (§1.1) is about 10 to 15% of an
operation (arithmetic). For it they change the representation every caller reads: `p256_vf`'s wNAF reads a scalar's bits as 28-bit
limbs, the comb's 10,400 words are 28-bit limbs in R = 2^280, and `load`, `store`, `below` and `pow`'s nibbles are written for
them. The formulas of §4.4 add and subtract without reducing, up to 66 p, which R = 2^256 cannot hold, so every addition would
reduce and `p256_bounds.py`'s proof would be replaced. Those files are being changed by #380 at the same time. Five 56-bit
limbs keep all of it: the change is inside `std/p256_kernels.cho` and its generator. #397's statement that four 64-bit limbs
would be confined to the generator was wrong for this reason (corrected in §4.2).

### 11.2 As built

`scripts/p256_gen.py` writes the four Montgomery kernels (`mul_p`, `sqr_p`, `mul_n`, `sqr_n`) on the builtins:

- **Packing.** Each pair of 28-bit limbs of an input becomes one 56-bit limb (`xs[2i] | xs[2i + 1] << 28`); the five limbs of the
  answer are split back into ten. An element in `work` is what it was.
- **The product, column by column.** Every product of two 56-bit limbs is taken whole with `mul_wide` and added into a two-word
  accumulator (`mac`: `mul_wide`, then `add_carry`), 25 products a multiplication, 15 a squaring (the cross terms against a doubled
  limb). A column's carry is the accumulator shifted right by 56 (`shr56`, one funnel shift).
- **The reduction, in radix 2^56: five steps, the same R = 2^280.** p is -1 modulo 2^96, so -p⁻¹ is 1 modulo 2^56 and the quotient
  digit is the column's low limb, as in §4.2; p's 56-bit limbs are 2^56 − 1, 2^40 − 1, 0, 2^24 and 2^32 − 1, so the first costs
  nothing and the others are three `mac`s a step. n's are general: n' modulo 2^56 is one `wrapping_mul`, then five `mac`s a step.
  The Montgomery quotient below R is unique, so every answer is the 28-bit kernels' answer word for word.
- **The bounds, proved by the generator for every column at the inputs' largest limbs** (the header of the generated file):
  the largest accumulator is 2^114.92 (limit 2^127; `mul_n`, column 4), the largest carry between columns 2^58.92 (limit 2^63).
  A wrong bound is a failed generation. §4.3's invariants are unchanged, so `p256_bounds.py` needs nothing new and still closes.
- **`add` and `sub_K` stay on 28-bit limbs but are `wrapping_*`**: they were checked arithmetic, a test and a branch for each of
  their 20 additions. The generator checks every limb's sum before its carry: under 2^29.99 (limit 2^31).
- **One test of the slices' lengths a kernel**, `if len(xs) < 10 || ... { return -1; }`, which no call can make true (a slice is
  ten words or the slicing trapped). It tells LLVM what it could not prove from `w[x..x + 10]`, and the bounds checks of the
  kernel's thirty constant indices go: 33 compares in `add` before, 6 after (arm64 object).

*Tried, measured, not kept:* the reduction's constants written as shifts instead of `mac` (94 cycles against 98 for a multiplication
on `gram`, 75 against 81 for a squaring, but 36 more instructions, and `shared` within 0.3%); an addition with one parallel carry
round instead of a chain (slower on the M4, 11 ns against 8 for the pair).

### 11.3 Gates

- **Vectors, byte for byte, both backends**: `tests/vectors/p256.txt` (1,823 cases, the four kernels at their worst-case limbs among
  them), RFC 6979 A.2.5, Wycheproof ECDH and ECDSA, NIST CAVP KAS and SigVer, the key files, the TLS suites of `cargo test`.
- **Differentials** (arm64 Linux, OpenSSL 3.0.13; [`wide-multiply.md`](wide-multiply.md) §12.2): the field against Python 444,704
  cases (LLVM) and 69,476 (Cranelift); signing against OpenSSL 10,000 and against RFC 6979 in Python 10,000; keys 600 checks;
  verification 4,000 a curve; ECDH 1,000 key pairs a curve. 0 differences.
- **Bounds**: the generator's (§11.2); `scripts/p256_bounds.py` closes for every formula, unchanged.
- **Mutants**: `scripts/p256_mutants.py`, its kernel mutants rewritten for the new code (a cross product dropped, the quotient
  digit's carry, each special limb of p off, a limb packed at 27 bits, an output limb unmasked, the top limb dropped, the
  accumulator's carry, the column shift at 55 bits and unmasked, the squaring's doubling, a square and a cross term left out, n'
  off by one, a limb of n, the quotient unmasked, the carries of `add` and `sub_4`, and the unchanged `canon_*` and constants
  mutants): **36 selected, 36 killed** at the head (`assets/wide-multiply/381/p256_mutants.txt`).
- **Constant time**: dudect on `gram` at 10^6 measurements a test, the ten tests of §8.3, all below 4.5 (largest 2.29; the table is
  `wide-multiply.md` §12.3). The branch audit of §8.2 repeated over the kernels: each has exactly one conditional jump that is
  not to a trap, the never-taken test of the slices' lengths (`cmp $0xa`, `setge`, `je`), and `canon_*` none
  (`assets/wide-multiply/381/branches_p256.txt`).

### 11.4 Cost

`gram` (x86-64), cycles and instructions as in the note above §11.1, the least of six runs in two sessions (load average 4 to 7;
the branch's kernels at b2651fea and at the head are the same object code); raw:
[`assets/wide-multiply/381/p256_cycles.txt`](assets/wide-multiply/381/p256_cycles.txt):

| operation | `main` | branch | gain (cycles; instructions) |
|---|---|---|---|
| field multiplication mod p | 158 cycles, 782 instructions | 99, 451 | 1.60; 1.73 |
| field squaring mod p | 113, 573 | 80, 345 | 1.41; 1.66 |
| multiplication mod n | 174, 827 | 130, 526 | 1.34; 1.57 |
| an addition and a subtraction | 73, 390 | 34, 220 | 2.15; 1.77 |
| inversion mod p | 34,950, 179,248 | 24,936, 106,566 | 1.40; 1.68 |
| inversion mod n | 49,534, 212,060 | 38,764, 141,395 | 1.28; 1.50 |
| `ecdh.public_key` (k·G) | 233,437, 1,197,041 | 141,568, 713,529 | **1.65**; 1.68 |
| `ecdh.shared` (k·P) | 980,090, 5,002,519 | 567,604, 2,912,470 | **1.73**; 1.72 |
| `ecdsa_sign.sign` | 326,959, 1,632,023 | 224,736, 1,075,660 | **1.45**; 1.52 |
| `ecdsa_sign.sign_checked` | 1,007,407, 5,062,738 | 646,106, 3,101,308 | 1.56; 1.63 |
| `ecdsa.verify_raw` | 682,396, 3,431,653 | 416,757, 2,026,587 | **1.64**; 1.69 |

Against OpenSSL 3.5.5 on the same core (`openssl speed`, cycles from `perf stat`,
[`openssl_gram.txt`](assets/wide-multiply/381/openssl_gram.txt)): ECDH 176,845 cycles, so `shared` is **3.21**
times it (5.54 on `main`); signing 70,906, **3.17** times (4.61); verification 233,829, **1.78** times (2.92).

**Apple M4 Max**, macOS, `scripts/p256_bench.py <branch> --vs <main>` (time, best of five alternating; load average 5.4;
[`m4_p256.txt`](assets/wide-multiply/381/m4_p256.txt)):

| operation | `main` | branch | |
|---|---|---|---|
| field multiplication mod p | 19 ns | 14 ns | 1.3x |
| field squaring mod p | 17 ns | 12 ns | 1.3x |
| an addition and a subtraction | 12 ns | 7 ns | 1.7x |
| inversion mod p / mod n | 5.6 / 10.0 µs | 4.3 / 7.6 µs | 1.3x |
| `ecdh.public_key` | 34.7 µs | 22.5 µs | **1.5x** |
| `ecdh.shared` | 138.4 µs | 97.0 µs | **1.4x** |
| `ecdsa_sign.sign` | 52.7 µs | 41.2 µs | **1.3x** |
| `ecdsa_sign.sign_checked` | 159.7 µs | 113.6 µs | 1.4x |
| `ecdsa.verify_raw` | 105.2 µs | 71.7 µs | **1.5x** |

The M4 gains less than x86-64: its old kernel's 100 multiply-adds were already cheap there (§1.2: 23 ns against 44 on the i7),
and its new one's `mul` and `umulh` are two instructions a product where x86-64's `mul` is one. In a TLS handshake
([`tls-performance.md`](tls-performance.md) §10.12) the server's full handshake is 19.9% cheaper on x86-64 and 14.3% on arm64.

*Not done:* `mulx` and `adcx`/`adox` behind a CPU test (the baseline x86-64 target has no BMI2; `wide-multiply.md` §10.6's first
item); a dedicated reduction for n's form; four 64-bit limbs, which would also need `p256_vf`, the comb and the lazy formulas
changed (§11.1).
