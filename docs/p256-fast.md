# `std.p256`: faster P-256 (#380, part of #378)

> **Status: design (this commit), then built step by step; §9 is filled in as each step is measured.** The TLS performance
> work (#378) names P-256 as the largest single cost in a server handshake: `std.ecdsa_sign.sign` is 0.80 ms on an M4 and
> 1.24 ms on the i7 of `gram`, OpenSSL's is 23 µs. `docs/ecdsa.md` §5.4 and `docs/ecdsa-sign.md` §9 said where the time goes
> without measuring it. §1 measures it; §2 to §7 are the design that follows from the numbers; §8 is how it is checked.

---

## 1. Where the time goes

All numbers `python3 scripts/p256_bench.py <driver>` (`tests/programs/p256_profile_before.cho` for this section, built from
`main` at the base of this PR): the time of one operation is `(T(rounds) - T(1)) / (rounds - 1)`, the best of five runs, LLVM
backend, one core. **Machines, with the load when measured:**

- **M4**: Apple M4 Max, macOS 26, native arm64. Load average 4.8 (other agents building): the figures move about 10% between runs.
- **Docker arm64**: the image `lexsys-hooks-env` on the same M4 Max, in Docker Desktop's 6-vCPU Linux VM. Load average 4.3.
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
  the caller's `work` is zeroed after every call (§6) and filling it costs more than the signature. **A `static` can hold it**
  (`docs/compile-time-data.md`): evaluated while compiling, read-only data in the binary, indexed like any slice. Built from a
  hex string by a loop, a 10,400-word static takes 0.14 s to compile and nothing at run time (measured).
- **A builtin for a wide multiply may land later** (#378, another change). Nothing here depends on it, and §4.2 says where it
  would be used.

## 3. The plan, in the order that pays most per change

| Step | What | Expected, from §1's measured parts |
|---|---|---|
| 1 | A P-256 field: ten limbs of 28 bits, straight-line kernels, lazy reduction (§4) | multiplication 115 → 23 ns on the M4: the ladder 767 → about 170 µs |
| 2 | Arithmetic modulo n on the same kernels; inversion with the exponent's structure (§5) | inversions 94 → about 20 µs of a signature |
| 3 | A fixed-base table for k·G, signed four-bit digits, no doublings (§6) | `public_key` and `sign`: 65 mixed additions instead of 320 operations: about 40 µs |
| 4 | Verification: Shamir's trick with wNAF and the generator's odd multiples (§7) | the 192 expected additions of 16 multiplications become about 75 of 11 to 16: 1.3 times fewer multiplications on top of step 1 |
| 5 | Cheaper additions and subtractions (§4.5), squarings, if they pay | whatever §9 measures |

Each step is measured, and kept only if it pays (§9).

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
on it; nothing outside `std/p256_kernels.cho` (generated) would change.

### 4.3 `wrapping_*`, and the proof that it cannot wrap

`bigmod` is checked arithmetic on purpose (`docs/ROADMAP.md` #234: "an overflow would be a bug, so it traps"). That costs a
branch after every addition and multiplication, and §1.2 measured it at 2.4 times the multiplication on the M4. The kernels
here are `wrapping_*` because **the generator computes, column by column, the largest value the accumulator can hold when every
limb is at its maximum (2^28 − 1) and refuses to write the file if any reaches 2^63**. The largest is 2^60.0 (the multiplication
modulo n, column 9). So the `wrapping_*` is a statement about code that is straight-line and has no data dependence: the same
bound holds for every input. A wrong bound is a failed generation. `std.field25519` made the same choice (its header). Everything
else in `std.p256` is checked arithmetic.

Invariants, stated in `std/p256_kernels.cho`'s header and checked by `scripts/p256_bounds.py` against every formula:
- every input is tight (limbs below 2^28) and every output is tight;
- `mul` and `sqr` accept inputs up to 4096 p and answer below 1.001 p;
- `add(x, y)` answers x + y, tight, not reduced;
- `sub(x, y)` answers x − y + 32 p, for y below 32 p, tight; it needs no mask, no branch, no borrow handling: every limb of
  the constant is above 2^28, so no limb goes negative before the carry pass;
- an element stays below 2^280, which is 16 million p.

### 4.4 No reductions inside the formulas

The formulas of `docs/ecdh.md` §2 add and subtract between multiplications, up to six deep. With R = 2^280, the values stay
below 100 p. `scripts/p256_bounds.py` parses each formula's source, propagates an upper bound through every `p256.mul`, `sqr`,
`add` and `sub`, and fails if a multiplication input exceeds 4096 p, a subtrahend 32 p, or a value 2^280. The only full
reductions are at the end: `from_mont`, which multiplies by 1 (the result is at most p) and subtracts p once under a mask
(`value_barrier`, `docs/value-barrier.md`), for the output of an operation and for the one comparison that needs an exact value.

### 4.5 Additions and subtractions

Measured with the multiplication at 23 ns: an addition and a subtraction take 8 to 9 ns each on the M4, in a dependent chain:
ten limbs and a carry pass. With 1.6 per multiplication they would be 35% of the ladder's time (§1.1: 65 µs of 161 µs after
step 1). Two ways to reduce that, tried after the steps above and kept if they pay: fewer (the formulas have chains such as
x + x + x); and carries only where a multiplication needs tight limbs (a sum of two tight limbs is below 2^29, and
`scripts/p256_bounds.py` would then track the limb bound as well as the value's).

---

## 5. Inversion

Fermat's a^(p−2) and a^(n−2), the exponents public, so their windows may be read from memory. Fixed four-bit windows over the
exponent's nibbles: 252 squarings, 14 multiplications for the table and one per nonzero nibble (31 for p − 2, whose runs of
zeros are long, 53 for n − 2). Measured on the M4 after step 1: p 6.1 µs, n 9.8 µs. An addition chain exploiting the runs of
ones in p − 2 would save about 33 multiplications (0.8 µs of 6): not worth its code until the rest is cheap, and §9 says whether
it is then.

## 6. Fixed-base multiplication by the generator

**Signed four-bit digits and a table of every digit at every position.** k = Σ dᵢ·16ⁱ, dᵢ in [−8, 8], 65 digits (the recoding
carries: a nibble above 8 becomes negative and adds one to the next). The table holds (j+1)·16ⁱ·G for i in 0..64, j in 0..7: 520
affine points, in Montgomery form, 83 KB as a `static` of 10,400 words. **k·G is then the sum of 65 table entries, no doubling
at all**, each a mixed (affine) addition, RCB algorithm 5, 11 multiplications.

**Constant time, for the secret k:**
- the digit's sign and magnitude come from arithmetic (a shift and an xor), not a comparison;
- **the entry is chosen by reading all eight under masks**, each ORed in under a mask that is all ones for the one wanted
  (`docs/ecdh.md` §2), the masks through `value_barrier`; no address depends on the digit. Each of the 65 positions is
  a different part of the table, so the position is public and only the entry within it is secret;
- the sign is applied by selecting between y and −y under a mask, never by a branch;
- **a zero digit** adds a dummy and keeps the old sum by a mask: the same operations whatever the digit;
- the formulas are complete (RCB), so the sum being equal to the entry, or the point at infinity, needs no case.

The table is a **`static`**, generated by `scripts/p256_tables.py` into `std/p256_comb.cho` from Python's integers and checked
against OpenSSL's point arithmetic (`--check` fails when the file is stale). It is built at compile time and costs nothing at
run time. That is what the language allows without allocation per call.

**Expected** (M4, from §1 and step 1): 65 × (11 multiplications × 23 ns + 23 additions or subtractions × 8.5 ns + a select of
8 × 20 words) ≈ 65 × 0.5 µs = 33 µs, plus the inversion 6 µs: `public_key` about 40 µs, against 160 after step 1 and 767 now.

## 7. Verification

Public data, so variable time is allowed. `u1·G + u2·Q` with one doubling chain (Shamir), **wNAF**: u1 in width 7 against a
table of the generator's odd multiples 1G, 3G, …, 63G (32 affine points, a `static`, ≈ 1/8 of the bits nonzero, mixed
additions); u2 in width 5 against 1Q, 3Q, …, 15Q, built per call (7 additions and a doubling). Jacobian coordinates and the
doubling of `ecdsa.double` (8 multiplications) with its explicit cases for infinity and equal points as `std.ecdsa` has them. About 256 doublings and 75 additions,
against 256 and 192 now; the two inversions (mod n for u1, u2; mod p for the x coordinate) as in §5. P-384 verification is unchanged:
it stays on `std.bigmod` (§10).

## 8. Constant time, and the gates

**Every function that handles a secret, and why it is constant time** (the audit of the x86-64 object is §9):

| Function | Secret | Why |
|---|---|---|
| `p256_kernels.mul_*`, `sqr_*`, `add`, `sub` | the operands | straight-line, every loop unrolled: no branch, no index |
| `p256_kernels.canon_*` | the value | the borrow over ten limbs, then the subtraction under a mask through `value_barrier` |
| `p256.load`, `store` | a scalar's bytes | the limb a byte lands in, and whether it straddles two, depend on its position; no range check (the caller made a constant-time one) |
| `p256.invert`, `sinvert` | the operand | the exponent p − 2 or n − 2 is public; a window of zero is skipped because the *exponent's* is zero |
| `p256_pt.select`, `multiply` | the scalar | as `docs/ecdh.md` §2: all sixteen entries read under masks, complete formulas |
| the comb (§6) | k | above |
| `ecdsa_sign.finish` | d, k | `load`, `sto_mont`, `sinvert`, `smul`, `sadd` as above; `is_zero` of r and s is on public values (`docs/ecdsa-sign.md` §2.1) |

**Gates**, all of them, on every step: the vectors byte for byte (RFC 6979 A.2.5; `ecdsa_sign_differential.py` `openssl` and
`reference` at 10,000; `ecdsa_differential.py`; `ecdh_differential.py`; the key parser; Wycheproof and CAVP); the x509 matrix and the TLS
suites that use these; new differentials of the field (`scripts/p256_field_differential.py`) and of the table; mutants of the
new code; `scripts/ecdh_timing.py` and `scripts/ecdsa_sign_timing.py` at 10^6 measurements with |t| < 4.5 on `gram`; the audit of
the x86-64 object; costs before and after on all three machines, in this document and as a comment on #378.

## 9. Results

*(Filled in as each step is measured; §1 is the "before" of every row.)*

## 10. Not done

- P-384: it stays on `std.bigmod`. The kernels are generated, so a 14-limb P-384 field would be a generator parameter; no
  program in this repository signs or exchanges on P-384 in a hot path (`docs/tls-server.md` §6: 6 ms a handshake is the
  P-384 client's choice, not the server's).
