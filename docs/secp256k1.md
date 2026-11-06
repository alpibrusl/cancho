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

**Cost [measured].** One whole process, startup included, signing: 2 ms (`time`). No claim beyond that: not a benchmark.

**Not measured [open].** Constant time in the secret scalar, by dudect as `docs/ecdsa-sign.md` §6 does for P-256. Until then it is a design claim: no
branch or index on the key or nonce in the code paths that see them (`load_secret`, the masked table select, the fixed-length ladder, `in_range`), checked by
reading, not by measuring. The branches left are on public values: lengths, `r`, `s`, the parity bit and the low-s decision (public once the signature is).
RFC 6979's retry path (a candidate out of range, or `r`/`s` zero) is reachable with probability about 2^-128 and is **not tested**.

## 4. Not claimed

Not independently reviewed (#209). Not fast. Not constant time *measured*. No BIP-340 Schnorr, no compressed keys, no ECDH, no DER, no EIP-155 `v`.
A development key is a development key.
