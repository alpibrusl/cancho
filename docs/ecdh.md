# `std.ecdh`: P-256 and P-384 key exchange, constant time

> **Status: built. Not independently reviewed (#209).** #207's PR 3 of 5 (`docs/tls-parity.md` §3.2), for the self-contained
> TLS client (#197): the key exchange OpenSSL's ClientHello offers beside X25519, which a server can ask for with a
> HelloRetryRequest. Unlike `std.ecdsa`, whose inputs are all public, a key exchange multiplies by a **secret** scalar. So
> everything the scalar touches runs the same instructions and the same memory accesses whatever its value. Building that
> found the LLVM backend undoing it (§3), and added a builtin to the language to stop it (`docs/value-barrier.md`).

---

## 1. What is built

| File | What |
|---|---|
| `std/ecdh.ls`, `module std.ecdh` | `public_key(curve, scalar, out, work)` and `shared(curve, scalar, peer, out, work)` on P-256 and P-384 |
| `std/bigmod.ls` | `mul`, `add` and `sub` now reduce without a branch (§2) |
| `value_barrier`, edition 6 | a value the optimiser cannot reason about (`docs/value-barrier.md`) |
| `std/ecdsa.ls` | `curve_param`, so the curve constants are written once |

**The API.**
- **`public_key`** answers scalar × G as an uncompressed point, `04 || x || y`. That is the key share a ClientHello sends.
- **`shared`** answers the x coordinate of scalar × peer, which is TLS's shared secret (RFC 8446 §7.4.2).
- **The scalar** is the curve's size in bytes, big-endian, in [1, n). A random string outside that range is refused
  (`ecdh-scalar-range`), so the caller draws again. The chance of that is under 2^-32 for P-256 and under 2^-190 for P-384.
- **`work`** is `work_len()` words (77 KB), which is more than a region holds, so the caller allocates it with `box_slice`.

**The peer's point is checked before use, as SEC 1 §3.2.2.1 asks:**
- its encoding: exactly `04 || x || y`, since TLS 1.3 sends points uncompressed (RFC 8446 §4.2.8.2);
- both coordinates below p;
- the curve equation.

Each failure has its own tag. P-256 and P-384 have cofactor 1, so a point that passes, multiplied by a scalar in [1, n),
cannot give the point at infinity. `ecdh-result-infinity` is checked anyway, and no input reaches it.

## 2. Constant time

**The scalar multiplication** walks the scalar four bits at a time, highest first:
- a table of 0 to 15 times the point, built first;
- for each window, four doublings, then the addition of the table entry the window names.
- **The additions and doublings are complete:** Renes, Costello and Batina's formulas for a = -3 (eprint 2015/1060,
  algorithms 4 and 6), in projective coordinates. They are right for every input, the point at infinity and P + P included.
  So no branch handles a special case, and a window of zero adds the point at infinity like any other entry.
- **The entry is chosen by reading all sixteen**, each ORed in under a mask that is all ones for the one wanted. No address
  depends on the window.

That is P-256's 64 windows, or P-384's 96, of identical work.

**`std.bigmod`'s reductions were branches. Now they are masks:**
- **`mont_mul` and `add`** end with a subtraction of n when the value is at least n. That was `if compare_n(...) >= 0`, which
  branched on the value and stopped at the first limb that differed. Now `ct_reduce` computes the borrow of x − n over every
  limb, then subtracts `n & mask`.
- **`sub`** added n back behind `if under == 1`. Now it adds `n & mask`, where the mask is made from the borrow.
- **`subtract_n`** took its borrow from an `if d < 0`. Now it is the sign bit.

RSA and ECDSA verification run on the same code, and their tests pass unchanged (§6 has the cost).

**What stays variable time, all of it on public values:**
- the checks on the peer's point;
- the test that the result is not at infinity;
- the inversion's exponent, which is p − 2;
- the scalar's range check, which compares every byte and branches only on the answer.

## 3. What the timing test found

`scripts/ecdh_timing.py` is `scripts/gcm_timing.py`'s dudect test over `ecdh.shared` (`tests/programs/ecdh_timing.ls`): a
fixed scalar against random ones, under one peer point.

**The first run failed.** A scalar of 1 against random scalars gave |t| = 13.2 on P-256 and 9.5 on P-384. Scalar 1 ran 1.3%
faster. A fixed random scalar against random scalars gave no signal. That is what a leak through the reductions looks
like: with scalar 1 the accumulator stays at infinity, (0 : 1 : 0), whose zero coordinates rarely need a reduction.

**Neither the source nor the branch audit's first reading showed it.** `scripts/chacha20_branches.py` listed a `js` in
`ct_reduce` and a `jns` in `select`. Read in the disassembly, both test a mask the source only ever ANDs with:

- `ct_reduce`: `test %r9,%r9; js` around the load of n's limb. `clang -O2` saw that `under - 1` is 0 or -1, rewrote
  `w[n + i] & m` as a select, and unswitched the loop on it.
- `select`: `test %r14,%r14; jns`, the same rewrite of `w[from + j] & m`. The window's table index was a branch.

No spelling of the mask survives that, because LLVM can prove any of them is 0 or -1 (`docs/value-barrier.md` §2). So the
masks now go through `value_barrier`, which the LLVM backend emits as an empty `asm` the optimiser cannot see through.

**After it:**
- **The audit:** `scripts/chacha20_branches.py` over `bigmod.{mont_mul, ct_reduce, add, sub}` and
  `ecdh.{select, add_points, double_point, multiply}` finds every remaining conditional jump compares a loop counter or `k`.
  `mont_mul`'s one `js` tests the sign of `k`, the public limb count.
- **The timing test**, 20,000 samples a test (max |t|):

| Test | LLVM | Cranelift |
|---|---|---|
| P-256, scalar 1 against random | 1.92 | 2.32 |
| P-256, a fixed scalar against random | 2.49 | 1.37 |
| P-384, scalar 1 against random | 3.04 | **11.04** |
| P-384, a fixed scalar against random | 2.70 | 1.75 |

**One test still fails: Cranelift, P-384, scalar 1.** A second run of 3,000 samples gave |t| = 6.2. Scalar 1 runs 0.3% faster
there (100,000 cycles of 30.5 million), and a scalar under 2^32 shows the same direction more weakly (|t| = 2.8).

**It is not a branch or a memory access:**
- the audit of the Cranelift object finds only loop counters;
- `valgrind --tool=callgrind` counts **172,602,288 instructions** for scalar 1 and for two random scalars, identical to the
  instruction;
- every index is a loop counter or a table position read for all sixteen entries.

**What is left depends on the operands' values.** With scalar 1 the accumulator's X and Z stay zero for nearly the whole
ladder, so most limbs multiplied and stored are zero. That is consistent with data-dependent power and so frequency
(Hertzbleed, Wang et al., USENIX Security 2022): `rdtsc` counts at a constant rate, so a core that clocks higher on cheaper
data finishes in fewer ticks. Cranelift's code spills more and runs 2.8 times as long, which would show it more. This is
consistent with the evidence, not proven: the instruction count rules out the software explanations, and nothing here
measures power.

**What it means for TLS.** The client draws a fresh scalar for every connection (RFC 8446 §4.2.8) and uses it once. A
statistical attack needs many timings of one scalar, and a peer gets one. It also needs inputs that keep the accumulator
near zero, which a random scalar does not give. The default backend, LLVM, stays below 4.5 throughout. The result is
reported, not excused: by this test's own threshold, Cranelift's P-384 ladder is not clean.

*Corrected (#208, `docs/tls-assurance.md` §6.1): on the Xeon. On an Apple M4 Max, scalar 1 fails on both backends,
LLVM's P-256 at |t| = 16.19, and with Arm's data-independent-timing bit set LLVM's falls to 1.84 (P-256) and 2.15 (P-384).
That is the CPU's data-dependent timing, which neither backend asks the CPU to turn off.*

## 4. Correctness

Every case runs through `tests/programs/ecdh_driver.ls` (`crates/lex-sys/tests/conformance/ecdh.rs`):

| Evidence | Cases | Result |
|---|---|---|
| Wycheproof `ecdh_secp256r1_ecpoint_test.json` | 355 | 330 valid cases equal; 24 invalid points refused (9 off the curve, 7 out of range, 8 badly encoded); the one `acceptable` case, a compressed point, refused |
| Wycheproof `ecdh_secp384r1_ecpoint_test.json` | 790 | 771 valid equal; 18 invalid refused; the compressed point refused |
| NIST CAVP KAS validity (static unified, Z only), P-256 and P-384, both backends | 60 | 36 pass cases give the IUT's public key and Z; each of 24 fail cases caught by the check its reason names |
| the scalar's edges | 10 | 1 gives G, n − 1 gives −G, 0 and n refused, on both curves |
| every refusal the driver reaches | 9 | each with its own tag |
| `std.bigmod.pow_mod` at moduli of 30 to 330 bits, each a multiple of 30 (`tests/vectors/bigmod_powers.txt`), both backends | 220 | equal to Python's `pow`: the one place the reduction's carry limb is reached, which neither curve does |
| `scripts/ecdh_differential.py` against pyca/cryptography (OpenSSL 4.0.1) | 1,000 key pairs a curve, 4,200 checks | 0 differences |

**Mutants:** `scripts/ecdh_mutants.py` runs 16 mutants, and **16 are killed**.
- **In `ecdh.ls`:** the addition and doubling formulas, the windows, the table, the select, the point at infinity, the curve
  equation and the on-curve check, the scalar's range, the public key's y, and the point's first byte.
- **In `bigmod.ls`:** the reduction's mask, its carry limb, and the add-back of a negative difference.

**Two mutants first survived.** The carry-limb mutant survived the ECDH evidence and RSA's and ECDSA's tests too, because no
modulus in any of them is a multiple of 30 bits long. That is what `bigmod_powers.txt` is for. The first-byte mutant
survived because the script's evidence had no correctly sized point with a wrong first byte; the refusals were added to it.

## 5. Cost

One core of the Xeon in `docs/chacha20.md` §6, LLVM backend, the time of one `shared`:

| | P-256 | P-384 |
|---|---|---|
| `std.ecdh.shared` | 2.6 ms | 6.4 ms |
| `std.x25519` (`docs/x25519.md` §6), for comparison | 1.08 ms | |

TLS spends one `public_key` and one `shared` per handshake on the group the server picks. X25519 stays the share a
ClientHello sends first, as OpenSSL's does (`docs/tls-parity.md` §2), so P-256 and P-384 cost only when a server asks for
them with a HelloRetryRequest. A generic Montgomery multiplication does not use the NIST primes' special form, as
`docs/ecdsa.md` §5.4 says. That is the first place to look if the cost matters.

## 6. What the change to `std.bigmod` cost

RSA and ECDSA verification use the same `mul`, `add` and `sub`, so the masked reductions cost them too. Measured with
`scripts/rsa_bench.py` and `scripts/ecdsa_bench.py` on the LLVM backend, `main` before this PR against after:

| Verification | before | after | |
|---|---|---|---|
| RSA-2048 PKCS#1 v1.5 | 352 µs | 374 µs | +6% |
| RSA-2048 PSS | 363 µs | 384 µs | +6% |
| RSA-4096 PKCS#1 v1.5 | 1,443 µs | 1,415 µs | −2% |
| RSA-4096 PSS | 1,532 µs | 1,414 µs | −8% |
| ECDSA P-256 | 1,541 µs | 1,705 µs | +11% |
| ECDSA P-384 | 4,303 µs | 4,633 µs | +8% |

RSA's changes are within the run-to-run variation of about 10% that `docs/rsa.md` §5.5 reports. RSA's time is in
`mont_mul`'s inner loop, which did not change, so the extra pass of `ct_reduce` is small next to it. ECDSA does relatively
more additions and subtractions, each now with a full pass, and pays about 10%. A TLS handshake verifies one or two
signatures, so this is around 0.2 ms.

## 7. Not done here

- **`std.x25519` and `std.field25519` onto `value_barrier`.** Their audit is clean today only because LLVM happens not to see
  through their masks (`docs/value-barrier.md` §4). Moving them is a change with its own timing run.
- **The scalar's range check rejects instead of reducing.** A TLS client can simply draw again.
