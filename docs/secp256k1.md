# `secp256k1`: ECDSA on the Bitcoin and Ethereum curve, with public-key recovery

> **Status: design, written before the code.** A package, `packages/secp256k1`, module `secp256k1`. Asked for by `cancho-chain`
> (EVM transaction signing; its `docs/design.md` §3). Not independently reviewed (#209). Every claim is **[measured]** (with the
> command), **[read]**, a **[choice]** or **[open]**; a claim found false is corrected here, in place.

## 1. What it is, and why a package

secp256k1 is `y^2 = x^3 + 7` over p = 2^256 - 2^32 - 977, with a prime group order n and cofactor 1. Ethereum signs every transaction with
ECDSA on it over a Keccak-256 digest and identifies the signer by *recovering* the public key from `(r, s, v)`, so recovery is part of the job.

A package and not `std/` **[choice]**: a `std` module is also named in `crates/cancho/src/main.rs`'s embedded list and in the self-hosting
tables (`examples/selfhost/stdorder.cho`, `stdsource.cho`), and the bar for `std` is two askers (`docs/standard-library.md`). `packages/x509`
(`x509_key`) is the precedent for a crypto module outside `std`. If a second asker appears it moves.

## 2. Design

- **On `std.bigmod`'s registers [read: `std/bigmod.cho`]**, which is generic over any odd modulus up to 4,096 bits and whose register arithmetic
  (`mul`, `add`, `sub`, the reduction) is constant time. This is `std.ecdh`'s generic path, the one P-384 uses, not `std.p256_*`'s hand-unrolled
  kernels. **[choice]** No speed claim: the 20x of `docs/p256-fast.md` is a later, separate piece of work.
- **Points are projective, added with Renes, Costello and Batina's complete formulas for a = 0** (eprint 2015/1060, algorithms 7 and 9, `b3 = 21`),
  which have no special case: infinity and doubling go through the same operations. Cofactor 1, so a valid point and a scalar in [1, n) never give
  infinity.
- **The scalar is secret in `sign` and `public_key`**: taken four bits at a time, four doublings then one of sixteen table multiples read under masks
  (`std.ecdh.multiply`, copied). Nothing branches on it or indexes by it. In `verify` and `recover` every input is public and the same code is reused.
- **Two moduli, one workspace.** The field phase (modulus p) and the scalar phase (modulus n) each call `bigmod.setup`; values cross between phases as
  bytes. The exponent of every inversion (p - 2, n - 2) and of the square root ((p + 1)/4, p = 3 mod 4) is public.
- **Nonce: RFC 6979 §3.2 with HMAC-SHA256**: the HMAC-DRBG is seeded with `key || bits2octets(digest)`, the digest reduced mod n once (it is below 2n), and
  optional 32 bytes of caller randomness as §3.6's additional data. **Corrected in place:** this document first said libsecp256k1 does *not* reduce the
  digest, from memory. **Measured** (`scripts/secp256k1_differential.py`): with the digest unreduced, every digest of n or more gave a different signature
  from `coincurve`'s; with it reduced they agree. Only digests of n or more tell the two apart, so the first 2,720 checks agreed either way. **[choice]**
- **Signature: `r || s || recid`, 65 bytes, s low (s <= n/2).** Ethereum requires low s (EIP-2). recid bit 0 is the parity of R.y, bit 1 is "R.x >= n"
  (probability about 2^-128, handled). `v = 27 + recid` / the EIP-155 form is the caller's.
- **Shared code.** `copy_point` is `std.ecdsa`'s and the field arithmetic is `std.bigmod`'s. The RFC 6979 steps (`update`), the table build, the window of the
  ladder and `multiply` are the package's own: the `std.ecdh` and `std.ecdsa_sign` ones are private, and the duplication gate
  (`no_function_body_is_duplicated_across_files`) passes with no entry for this package. **[open]** a generic RFC 6979 helper and a curve-generic ladder in `std`,
  once a second asker confirms their shape; that is a `std` change and so a pull request of its own.
- **Every function answers 0 or a negative code**, `refusal_tag(code)` its stable name; no input reaches a trap.

API: `work_len()`; `public_key(key, out65, work)`; `sign(digest, key, extra, sig65, work)`; `recover(digest, sig65, out65, work)`;
`verify(digest, pub65, sig65, work)`. Public keys are uncompressed, `04 || x || y`.

## 3. How it is checked

**Status: built.** All **[measured]**, Linux x86-64, the self-hosted `cancho` (stage 2 = stage 3) at `162633a7`, `coincurve`
(libsecp256k1) and `ecdsa` (python-ecdsa) from pip:

`CANCHO=<compiler> python3 -I scripts/secp256k1_differential.py --cases 60`: **3,237 of 3,237** checks agree.
- Public keys of 70 private keys (1, 2, 3, n-1, n-2, n/2, n/2+1, 2^255, 2^128, 0xFFFF and random), against libsecp256k1, and against python-ecdsa for the
  smaller ones.
- Signatures `r || s || recid` **byte for byte** against libsecp256k1 (`sign_recoverable`) for 25 keys by 9 digests, including digests of 0, 1, n-1, n, n+1
  and 2^256-1 (RFC 6979 and low s make them deterministic).
- Signatures with RFC 6979 §3.6 added randomness against python-ecdsa's `extra_entropy` (r and s, low s applied).
- Recovery: of every signature above, and of **synthetic** `(r, s, recid)` with every recid 0 to 3, small r (so recid 2 and 3 are reachable) and large, against
  libsecp256k1: the same key, or both refuse.
- Verification of valid signatures, high-s signatures, and of single-bit flips of r (two positions), s (two positions), the digest and the key: refused.
- 22 refusals: key 0, n, n+1, 2^256-1; lengths; r or s equal to 0 or n; recid 4; a point off the curve; a coordinate not below p; a bad prefix; an x with no point.

`... --mutants`: 38 deliberate faults (a constant of each kind, each formula step I tried, the table select, the window shift, the nonce's separators and
inputs, the `s` formula, low s and its parity bit, recovery's parity, negation and `r + n`, verification's comparisons and checks): **36 killed, 2 unreachable
by any input and said so in the script** (an `s` of exactly n/2 has probability 2^-256; `R.x >= n` when *signing* has probability about 2^-128, though the same
fault in *recovery* is killed). A first run killed 34 and left four; two of those were gaps in the checks (no added-randomness case, no synthetic recovery),
closed above.

**Found by the gate.** My note that libsecp256k1 does not reduce the digest before the nonce was wrong (§2, corrected in place). The test for
"verification refuses a wrong key" was wrong for a digest of 0 mod n: then u1 = 0 and ECDSA cannot tell Q from -Q, and `n/2 + 1` is exactly `-(n/2)`.


**Not measured [open].** Constant time in the secret scalar, by dudect as `docs/ecdsa-sign.md` §6 does for P-256. Until then it is a design claim: no
branch or index on the key or nonce in the code paths that see them (`load_secret`, the masked table select, the fixed-length ladder, `in_range`), checked by
reading, not by measuring. The branches left are on public values: lengths, `r`, `s`, the parity bit and the low-s decision (public once the signature is).
RFC 6979's retry path (a candidate out of range, or `r`/`s` zero) is reachable with probability about 2^-128 and is **not tested**.

## 4. Performance, against the best there is

**[measured]** 2026-11-06, `python3 -I scripts/secp256k1_bench.py DRV` (cancho: `B` loops of `tests/programs/secp256k1_driver.cho`, (T(rounds) - T(1)) / (rounds - 1),
best of five, as `scripts/p256_bench.py`; references in process, best of five batches). Intel Xeon @ 2.1 GHz, 4 vCPU, Linux x86-64, LLVM backend, a full-width
key and digest. Python adds a few microseconds to the references.

| operation | cancho (µs) | libsecp256k1 (µs) | OpenSSL 3, generic secp256k1 (µs) | vs libsecp256k1 | vs OpenSSL |
|---|---|---|---|---|---|
| public key (k G) | 324 | 35 | 609 | 9.2x slower | 0.5x (faster) |
| sign | 352 | 42 | 321 | 8.4x | 1.1x |
| recover | 686 | 36 | -- (none) | 19x | -- |
| verify | 644 | 29 | 302 | 22x | 2.1x |

How far that is from what the same machine does with cancho's *tuned* code: `python3 scripts/p256_bench.py` on `p256_profile` gives P-256 public key 46 µs, sign 63 µs,
verify 135 µs, so this package is 5 to 7 times slower than cancho's own hand-unrolled P-256 on the same hardware. OpenSSL's assembly P-256, measured the same way in
process (an ad hoc loop, not in the repository: 26 µs sign, 78 µs verify, 29 µs public key), is 1.6 to 2.4 times faster than cancho's tuned P-256. Three observations, the first two measured:
1. **Where we stand:** about 8 to 22 times libsecp256k1, level with OpenSSL's *generic* curve code. 2,800 signatures a second on one core is more than an agent
   moving assets will ever ask for, so for `cancho-chain` this is not the constraint.
2. **Why it is far:** libsecp256k1 has a hand-written 5x52 field, the GLV endomorphism (half-length scalars), Strauss/wNAF and precomputed tables for G; this
   package has none of them. Verification does two full 4-bit-window multiplications (about 2 x 320 µs), which is exactly where the gap is widest.
3. **What is within reach without new language features:** a fixed-base comb for G (`docs/p256-fast.md` §1: P-256 public keys took 19.6 times fewer instructions
   once they had one), unrolled field kernels for p = 2^256 - 2^32 - 977, and a joint (Strauss) multiplication for verify and recover.
   Estimate, **not measured**: 5 to 7x, which would put signing near 50-70 µs, still 1.5x libsecp256k1. The GLV endomorphism would be the rest. None of this is built.

## 5. Not claimed

Not independently reviewed (#209). Not fast: section 4. Not constant time *measured*. No BIP-340 Schnorr, no compressed keys, no ECDH, no DER, no EIP-155 `v`.
A development key is a development key.
