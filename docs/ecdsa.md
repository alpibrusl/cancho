# `std.ecdsa`: ECDSA verification on P-256 and P-384

> **Status: built. Not independently reviewed (#209).** Sub-issue 7 (#204) of the self-contained TLS 1.3 client (#197). Most
> web certificates are now ECDSA: 39 of the 128 system roots have EC keys (`docs/x509.md` §5.1), and a server with an EC key
> signs its `CertificateVerify` with `ecdsa_secp256r1_sha256` or `ecdsa_secp384r1_sha384`. P-521 is not in
> `docs/tls-pure.md` §5.2's list.

---

## 1. What is built

| File | What |
|---|---|
| `std/ecdsa.cho`, `module std.ecdsa` | `verify_der` and `verify_raw` on P-256 and P-384, over a digest of any length |
| `std/bigmod.cho` | a register API beside `pow_mod`: `setup`, then `mul`, `add`, `sub`, `inverse`, `to_mont`, `from_mont` on numbers held in the caller's `work` (§2) |
| `scripts/ecdsa_params.py` | prints the curve constants from `openssl ecparam -param_enc explicit`, after checking them (§2.1) |

Verification only, on public data, so variable time is allowed (`docs/tls-pure.md` §4.2), and the code branches on the
scalars' bits.

## 2. Arithmetic

**The field arithmetic is `std.bigmod`'s, not a second copy.** ECDSA needs arithmetic modulo the field prime `p` and the
group order `n`. `std.bigmod` already has Montgomery multiplication on 30-bit limbs (`docs/rsa.md` §2), and the duplication
check (`conformance/duplication.rs`) would refuse a copy. So `std.bigmod` gains a register API:
- `setup(n, w)` checks `n` and stores `k`, `n'`, `n` and `R² mod n` in `w`;
- numbers then live in registers `reg(i)` of the same `w`;
- `inverse` is Fermat's `a^(n-2)`, correct only for a prime modulus, which `p` and `n` are.

`pow_mod` now calls `setup`, so the modulus checks are in one place.

A P-256 number is 9 limbs and a P-384 number 13. A generic Montgomery multiplication does not use the special form of the
NIST primes, and is slower than one that does (§5.4). The design chooses one well-tested multiplication over a fast one per
prime.

### 2.1 Constants

`p`, `b`, `G` and `n` for both curves are printed by `scripts/ecdsa_params.py` from OpenSSL's explicit parameters. The script
also checks that:
- `a = p - 3`;
- `G` is on the curve;
- `p` and `n` are prime (64 Miller-Rabin rounds);
- `n·G` is the point at infinity.

They are kept in the source as hex strings and decoded at each verification: five decodings of 32 or 48 bytes, small next to
the thousands of multiplications a verification does (not separately measured).

### 2.2 Points

Points are in Jacobian coordinates `(X, Y, Z)` in Montgomery form, with `Z = 0` the point at infinity. Doubling uses
`dbl-2001-b` for `a = -3`, which gives `Z = 0` by itself for infinity and for `Y = 0`. Addition uses `add-2007-bl`, with the
cases its formula does not cover handled by branches:
- either input infinite;
- the same x (`H = 0`): a doubling when the points are equal, infinity when they are opposite.

An attacker chooses `Q` and the signature, so `Q = ±G` and `u1·G = ±u2·Q` part-way through the ladder are inputs to expect, not
corner cases.

`u1·G + u2·Q` is computed by Shamir's trick: one shared doubling chain, adding `G`, `Q` or the precomputed `G + Q` at each bit.

## 3. Verification

`verify_raw(curve, digest, point, sig, work)` takes `sig = r || s`, each exactly the curve's size. `verify_der` takes
`SEQUENCE { INTEGER r, INTEGER s }` in strict DER, as `packages/x509` reads DER: a definite length in its fewest bytes,
minimal INTEGERs, nothing after the SEQUENCE. Both then run SEC 1 §4.1.4:

1. The key `point` is `04 || x || y` (uncompressed, as `packages/x509` requires), with `x, y < p` and on the curve. The curves
   have cofactor 1, so on the curve means in the group.
2. `1 <= r, s < n`.
3. `e` is the digest's leftmost `bits(n)` bits, which is whole bytes for both curves. It is reduced mod `n` once (`e < 2n`).
4. `w = s^-1`, `u1 = e·w`, `u2 = r·w`, all mod `n`.
5. `R = u1·G + u2·Q`. Infinity is refused.
6. Valid when `x(R) mod n = r`.

| Code | Tag | When |
|---|---|---|
| -30 | `ecdsa-curve` | a curve other than P-256 or P-384 |
| -31 | `ecdsa-point-encoding` | the key is not `04 || x || y` of the curve's size (a compressed point included) |
| -32 | `ecdsa-point-infinity` | the key is the point at infinity (the one byte `00`) |
| -33 | `ecdsa-point-range` | a coordinate not below `p` |
| -34 | `ecdsa-point-not-on-curve` | `y² != x³ - 3x + b` |
| -35 | `ecdsa-raw-length` | a raw signature not twice the curve's size |
| -36 | `ecdsa-der-structure` | not one SEQUENCE of two INTEGERs and nothing else; an indefinite length |
| -37 | `ecdsa-der-non-minimal` | a length or an INTEGER not in its fewest bytes |
| -38 | `ecdsa-r-range` | `r` zero, negative, or not below `n` |
| -39 | `ecdsa-s-range` | the same for `s` |
| -40 | `ecdsa-result-infinity` | `u1·G + u2·Q` is the point at infinity |
| -41 | `ecdsa-mismatch` | `x(R) mod n != r` |
| -42 | `ecdsa-work-length` | `work` shorter than `work_len()` |

## 4. How it is tested

- **Wycheproof:**
  - `ecdsa_secp256r1_sha256_test.json` and `ecdsa_secp384r1_sha384_test.json` (DER), and their `_p1363` raw forms;
  - `ecdsa_secp256r1_sha512`, `ecdsa_secp384r1_sha256` and `ecdsa_secp384r1_sha512`, for a digest longer and shorter than
    `n`.

  Every valid case must verify, and every invalid one must be refused. The files have no "acceptable" cases.
- **NIST CAVP** FIPS 186-3 `SigVer.rsp`, for P-256 and P-384 with SHA-256/384/512.
- **Against OpenSSL:**
  - 1,000 signatures made by `openssl dgst -sign` on each curve, and each with one bit flipped;
  - both verified by `openssl dgst -verify` and by `std.ecdsa`, which must agree every time.
- **The register API against Python:** random `mul`, `add`, `sub` and `inverse` on both primes.
- **Mutants:** a mutation run, as for RSA.
- **Cost:** verifications per second per core.

`tests/programs/ecdsa_driver.cho` runs all of it. `crates/cancho/tests/conformance/ecdsa.rs` has the vectors and refusals,
`scripts/ecdsa_differential.py` runs Python and OpenSSL, and `scripts/ecdsa_mutants.py` the mutants.

## 5. Results

### 5.1 Wycheproof

The seven files, 3,098 cases, copied unchanged into `tests/vectors/wycheproof/`:

| Expected | Answer | Cases |
|---|---|---|
| valid | `ok` | 1,370 |
| invalid | `ecdsa-der-structure` | 875 |
| invalid | `ecdsa-r-range` | 462 |
| invalid | `ecdsa-s-range` | 152 |
| invalid | `ecdsa-mismatch` | 119 |
| invalid | `ecdsa-der-non-minimal` | 45 |
| invalid | `ecdsa-raw-length` | 40 |
| invalid | `ecdsa-result-infinity` | 35 |

Every valid case verifies and every invalid one is refused, on the first run of the code.
- The 875 DER refusals are Wycheproof's BER and malformed encodings: indefinite lengths, long-form lengths, wrong tags,
  garbage and missing parts.
- The 35 `ecdsa-result-infinity` cases are signatures built so that `u1·G + u2·Q` is infinity. `std.ecdsa` reaches that point
  and refuses it there, rather than miscomputing it.
- The SHA-512 files check that a digest longer than `n` is truncated, and `ecdsa_secp384r1_sha256` that a shorter one is
  used whole.

Two file names tried, `ecdsa_secp256r1_sha256_bitflip_test.json` and its P-384 form, do not exist (404).

### 5.2 NIST CAVP

`ECDSA_SigVer_186-3.rsp` (`tests/vectors/cavp/README.md`), 90 cases: P-256 and P-384, each with SHA-256, SHA-384 and SHA-512.
- 18 P, all verify.
- 72 F, all `ecdsa-mismatch`: the file's reasons are "message changed", "R changed", "S changed" and "Q changed", 18 each.
  NIST's changed Q is another point on the curve, so it fails as a mismatch, not as an invalid point.

### 5.3 Against Python and OpenSSL; mutants

- **`python3 scripts/ecdsa_differential.py <driver> registers`:** `std.bigmod`'s `mul`, `add`, `sub`, `inverse` and
  `load_reduced`, modulo P-256's and P-384's `p` and `n`, against Python's integers. **100,000 operations, 0 differences.**
- **`python3 scripts/ecdsa_differential.py <driver> openssl`:**
  - **1,000 signatures on each curve**, from three `openssl genpkey` keys a curve, signed by `openssl dgst -sign` over random
    messages, and the same with one bit of the DER flipped;
  - OpenSSL 3.0.13 accepted the 2,000 unflipped signatures and refused the 2,000 flipped ones;
  - **`std.ecdsa` gave the same answer all 4,000 times.**
- **`python3 scripts/ecdsa_mutants.py target/release/cancho`: 21 mutants, 21 killed.** One of them,
  `load_reduced` not subtracting when its input equals `n` exactly, first survived. That run had every vector and 2,000
  register rounds, so two things were added:
  - a fixed case whose digest is exactly `n`, so `e = 0`. Its signature was made in Python with a key and nonce from
    `random.Random(2041)`. pyca/cryptography accepts it over that digest and refuses it over `00…01`.
  - `load_reduced` in the register differential, with `n` itself drawn often.

  The mutant then died only in the second. Left unreduced, `n` behaves as 0 inside every Montgomery multiplication, which is
  the right answer here, so ECDSA's results never showed it. The register API's contract, that a value is below the modulus,
  is still now tested directly.

### 5.4 Cost

`python3 scripts/ecdsa_bench.py <driver>`, on one core of an Intel Xeon at 2.80 GHz:

| | Cranelift | LLVM | OpenSSL 3.0.13 (`openssl speed ecdsap256 ecdsap384`) |
|---|---|---|---|
| P-256, SHA-256 | 271/s (3.7 ms) | 675/s (1.5 ms) | 13,282/s |
| P-384, SHA-384 | 101/s (9.9 ms) | 229/s (4.4 ms) | 1,209/s |

On LLVM, P-256 is 20 times slower than OpenSSL and P-384 5 times. OpenSSL's P-256 is a hand-tuned implementation that uses
the prime's special form; its P-384 is generic, closer to this. A handshake with an ECDSA chain verifies two or three
signatures plus `CertificateVerify`: 4 to 6 ms with P-256 on LLVM. That is the figure #210 must weigh against OpenSSL's.
§2 names the speed-up left untaken: arithmetic specialised to each prime.

## 6. Found

- **Nothing wrong in the first run** against 3,098 Wycheproof cases, 90 NIST cases, 4,000 OpenSSL cases and 100,000 register
  operations. The bugs the RSA slice found were in the shared limb code, which this slice reuses, so they were already fixed
  (`docs/rsa.md` §6).
- **A test gap:** `load_reduced`'s contract was not tested directly (§5.3).

## 7. Not done

- **P-521.** `docs/tls-pure.md` §5.2 lists P-256 and P-384 only. One system root has a P-521 key, and a chain through it would
  be refused by #206.
- **Compressed points.** `packages/x509` refuses them already.
- **Prime-specific reduction** (§5.4).
- **The independent review.** #209.
