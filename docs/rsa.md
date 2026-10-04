# `std.bigmod` and `std.rsa`: modular arithmetic and RSA signature verification

> **Status: built. Not independently reviewed (#209).** Sub-issue 6 (#203) of the self-contained TLS 1.3 client
> (#197). A TLS 1.3 server whose certificate chain is RSA signs its `CertificateVerify` with RSASSA-PSS, and most web roots
> and intermediates sign certificates with RSASSA-PKCS1-v1_5 (`docs/x509.md` §5.1: 89 of the 128 system roots have RSA keys).
> So the client has to verify both.

---

## 1. What is built

| File | What |
|---|---|
| `std/bigmod.ls`, `module std.bigmod` | unsigned integers mod an odd `n` of up to 4,096 bits, in Montgomery form: `pow_mod(n, e, a, out, work)` over big-endian bytes |
| `std/rsa.ls`, `module std.rsa` | `pkcs1_verify` (RSASSA-PKCS1-v1_5, RFC 8017 §8.2.2) and `pss_verify` (RSASSA-PSS, §8.1.2), over SHA-256, SHA-384 and SHA-512 |

`std/bignum.ls` already exists. It is a small decimal number type for float printing, not modular arithmetic, so the new module
gets its own name rather than growing that one.

**Verification only.** No signing, no decryption, no key generation. Nothing here touches a secret: the modulus, the exponent,
the signature and the message are all public. So constant time is not a requirement (`docs/tls-pure.md` §4.2). What is required
is correctness, and refusing every malformed input with a tag.

## 2. `std.bigmod`

### 2.1 Representation

A number is a `[int]` of **30-bit limbs**, least significant first. That is 137 limbs for 4,096 bits.

The width is set by the multiplication step. Montgomery multiplication (CIOS, one fused inner loop) computes
`t[j] + a_i*b[j] + m*n[j] + carry` for each limb. With 30-bit limbs that is below 2^30 + 2^60 + 2^60 + 2^31 < 2^62, so it fits
an `int` with checked arithmetic and no `wrapping_*`. With 31-bit limbs it would reach 2^63 and overflow. With 32-bit limbs
even one product would. Checked arithmetic is kept on purpose: an overflow here would be a bug, and a trap shows it, where
wrapping would give a wrong answer silently.

### 2.2 Montgomery multiplication

`R = 2^(30k)` for a `k`-limb modulus. `mont_mul(a, b) = a·b·R^-1 mod n`, for `a, b < n`.
- `n' = -n^-1 mod 2^30` comes from Newton's iteration on the lowest limb: it starts right to 3 bits (n is odd) and doubles each
  step.
- The result of one CIOS pass is below `2n`, and one conditional subtraction brings it below `n`.

**R² mod n without division.**
- `2^(bits(n)-1)` is below `n`. Doubling it (subtracting `n` when the result reaches `n`) at most 30 times reaches
  `R mod n`; one more doubling gives `2R mod n`.
- That is 2 in Montgomery form. Raising it to the power `30k` with `mont_mul` gives `2^(30k)·R = R² mod n`.
- This costs about 12 squarings, against the 30k doublings, each over all k limbs, that a plain doubling from 1 would take.

### 2.3 `pow_mod`

`pow_mod(n, e, a, out, work)` sets `out = a^e mod n`, all as big-endian bytes, with `out` exactly as long as `n`.
- **Refused:** an even `n`, `n < 3`, `n` over 4,096 bits, `a >= n`, an empty `e`, `out` the wrong length, or `work` shorter than
  `work_len()`.
- **Leading zero bytes** in `n`, `e` and `a` are allowed, because DER INTEGERs carry one before a high bit.
- **Method:** left-to-right square-and-multiply in Montgomery form, then one `mont_mul` by 1 to leave it.
- **Not constant time** (§1): it branches on the exponent's bits, which are public.
  *Since #207 (`docs/ecdh.md` §2):* the reduction under `mul`, `add` and `sub` is constant time, because `std.ecdh` multiplies
  secret values with it. `pow_mod` still branches on the exponent.

`work` is the caller's `[int]` of `work_len()` words. Nothing is allocated from a size the input names.

## 3. `std.rsa`

### 3.1 The key

- **The modulus** must be odd and from 2,048 to 4,096 bits (`docs/tls-pure.md` §5.2).
- **The exponent** must be odd and at least 3, and shorter than the modulus.
- The modulus and exponent come in as big-endian bytes, as `packages/x509` locates them (`view[rsa_modulus_*]`).

### 3.2 RSASSA-PKCS1-v1_5: build the block and compare it

`pkcs1_verify(hash_len, n, e, digest, sig, work)`:
1. `sig` must be exactly `k` bytes, where `k` is the modulus length in bytes (RFC 8017 §8.2.2 step 1).
2. As an integer it must be below `n` (§5.2.2 step 1).
3. `m = sig^e mod n`, written as `k` bytes.
4. **The expected block is built, not parsed:** `00 01 FF … FF 00 || DigestInfo || digest`, using the fixed DER DigestInfo
   prefix for the hash (with its NULL parameters), padded to `k` bytes. The two blocks are compared in full.

Comparing the whole block, instead of parsing `m`, is what rules out the Bleichenbacher-2006 class of forgeries. Those depend on
a lenient parser: one that skips the padding, accepts a short `FF` run, reads the DigestInfo's lengths, or allows trailing
garbage. Here there is nothing to parse, so there is one refusal, `rsa-pkcs1-mismatch`. Telling a caller *which* byte
differed would help no one but an attacker.

**The DigestInfo without its NULL** is refused. RFC 8017 §9.2 note 2 says verifiers *may* accept it, and Wycheproof marks
one such case per file "acceptable". Accepting it would mean building and comparing a second block, for a form no current
signer produces.

### 3.3 RSASSA-PSS

`pss_verify(hash_len, mgf_hash_len, salt_len, n, e, digest, sig, work)` follows EMSA-PSS-VERIFY (RFC 8017 §9.1.2) exactly:
`emBits = modBits - 1`, MGF1 over `mgf_hash_len`, and a fixed salt length.

TLS 1.3's `rsa_pss_rsae_sha256/384/512` are `pss_verify(h, h, h, ...)`: the MGF1 hash is the message hash, and the salt is as
long as the digest (RFC 8446 §4.2.3). The salt length is a parameter, not inferred, because a verifier that recovers it from
the padding accepts more than the signer meant. Each PSS step that can fail has its own tag:

| Tag | Step of §9.1.2 |
|---|---|
| `rsa-pss-length` | 3: `emLen < hLen + sLen + 2` |
| `rsa-pss-trailer` | 4: the last byte is not `0xbc` |
| `rsa-pss-top-bits` | 6: the bits above `emBits` are not zero |
| `rsa-pss-padding` | 10: `DB` is not zeros, then `0x01` |
| `rsa-pss-mismatch` | 14: `H != H'` |

### 3.4 Refusals

| Code | Tag | When |
|---|---|---|
| -1 | `bigmod-even-modulus` | `n` even |
| -2 | `bigmod-modulus-size` | `n < 3` or over 4,096 bits |
| -3 | `bigmod-not-reduced` | `a >= n` |
| -4 | `bigmod-exponent` | `e` empty |
| -5 | `bigmod-output-length` | `out` not as long as `n` |
| -6 | `bigmod-work-length` | `work` shorter than `work_len()` |
| -10 | `rsa-modulus-size` | the modulus outside 2,048 to 4,096 bits |
| -11 | `rsa-even-modulus` | the modulus even |
| -12 | `rsa-exponent` | `e` even, below 3, or not shorter than the modulus |
| -13 | `rsa-hash` | a hash length other than 32, 48 or 64 |
| -14 | `rsa-digest-length` | the digest not `hash_len` bytes |
| -15 | `rsa-signature-length` | the signature not exactly `k` bytes |
| -16 | `rsa-signature-range` | the signature not below the modulus |
| -17 | `rsa-pkcs1-mismatch` | §3.2 |
| -18 to -22 | `rsa-pss-*` | §3.3 |

`std.rsa` checks the key itself (-10 to -12) before it calls `std.bigmod`, so a caller of `std.rsa` sees an `rsa-*` tag for a
bad key, never a `bigmod-*` one.

## 4. How it is tested

- **Wycheproof:**
  - `rsa_signature_{2048,3072,4096}_sha{256,384,512}_test.json` (PKCS#1 v1.5);
  - `rsa_pss_*_test.json` for 2,048, 3,072 and 4,096 bits;
  - `rsa_pss_misc_test.json`, whose SHA-1 and SHA-224 cases are counted as not supported.

  Every valid case must verify and every invalid one must be refused. The counts are listed.
- **NIST CAVP** `SigVer15_186-3.rsp` and `SigVerPSS_186-3.rsp`, for SHA-256/384/512. Moduli of 2,048 and above must give the
  file's P/F; 1,024 and 1,536 must be refused as `rsa-modulus-size`.
- **`pow_mod` against Python's `pow(a, e, n)`** on 100,000 random inputs, with moduli of random odd sizes from 2 to 4,096
  bits.
- **Against OpenSSL:** 1,000 signatures made by `openssl dgst -sign` and 1,000 with one bit flipped, each verified by both
  `openssl dgst -verify` and `std.rsa`. The two must give the same answer every time.
- **Cost:** verifications per second for 2,048 and 4,096 bits, on both backends, with the command used.

All of it is run by `tests/programs/rsa_driver.ls`, which reads one case per line. `crates/lex-sys/tests/conformance/rsa.rs`
runs the vectors and the refusals; `scripts/rsa_differential.py` runs Python and OpenSSL; `scripts/rsa_mutants.py` checks
that the vectors would notice a bug (§5.4).

## 5. Results

### 5.1 Wycheproof

The 17 files in `tests/vectors/wycheproof/` (`rsa_signature_*` and `rsa_pss_*`, copied unchanged), 3,368 cases:

| Expected | Answer | Cases |
|---|---|---|
| valid | `ok` | 694 |
| invalid | `rsa-pkcs1-mismatch` | 2,201 |
| invalid | `rsa-pss-mismatch` | 112 |
| invalid | `rsa-pss-padding` | 106 |
| invalid | `rsa-pss-trailer` | 54 |
| invalid | `rsa-signature-length` | 53 |
| invalid | `rsa-signature-range` | 41 |
| invalid | `rsa-pss-top-bits` | 2 |
| acceptable (`MissingNull`) | `rsa-pkcs1-mismatch`, as §3.2 decides | 9 |
| valid, over SHA-1 or SHA-224 (`rsa_pss_misc`) | not run: no such hash in `std` | 96 |

Every valid case verifies and every invalid one is refused. Most of the 2,201 PKCS#1 refusals are DigestInfo encodings that
a lenient parser might accept: indefinite lengths, long-form or overflowing lengths, garbage prepended or appended, a
modified OID, changed tags, a truncated digest, a missing DigestInfo. There are also an unhashed message and an unreduced
signature. The one comparison catches all of them. `rsa_pss_misc`'s 54 groups over SHA-256/384/512 use
every pairing of message hash, MGF1 hash and salt length (0, 20, 28, 32, 48, 64), so the `mgf_hash_len` and `salt_len`
parameters are checked against cases where they differ from the hash.

The directory could not be listed from here, so the files were fetched by name. Five names tried do not exist under
`testvectors_v1/` (404): `rsa_pss_2048_sha512_mgf1_64`, `rsa_pss_3072_sha384_mgf1_48`, `rsa_pss_3072_sha512_mgf1_64`,
`rsa_pss_2048_sha512_256_mgf1_28` and `rsa_signature_test`. A Wycheproof file with another name would have been missed.

### 5.2 NIST CAVP SigVer

`SigVer15_186-3.rsp` and `SigVerPSS_186-3.rsp`, with the SHA-256/384/512 cases (`tests/vectors/cavp/README.md`), 270 each:

- for 2,048, 3,072 and 4,096 bits, 27 P and 135 F a file, every one given the file's result;
- for 1,024 and 1,536 bits, 108 a file, every one refused as `rsa-modulus-size`.

The files give a reason for each F: the message changed, `e` changed, the signature changed, or the encoded block is malformed
(the hash moved left, or the `00` after the padding removed), 45 cases each in the PKCS#1 file. The exponents are 3, 17 and
65537.

### 5.3 Against Python and OpenSSL

`python3 scripts/rsa_differential.py <driver> pow 100000`:

- **`pow_mod` against Python's `pow`: 100,000 cases, 0 differences**, in 3 min 17 s on Cranelift.
- Moduli have random odd sizes from 2 to 4,096 bits, with limb boundaries drawn more often.
- 1% of the exponents are as long as the modulus.
- 1,000 cases have `a` = 0, 1 or `n - 1`.
- 1,000 have `n = p^j` and `a = p^i`, where `a^e mod n` is often 0 (§5.4).

`python3 scripts/rsa_differential.py <driver> openssl 1000`:

- **Against OpenSSL 3.0.13: 1,000 signatures made by `openssl dgst -sign` and the same 1,000 with one random bit flipped.**
- Keys: three of each of 2,048, 3,072 and 4,096 bits. Hashes: SHA-256, SHA-384 and SHA-512. Padding: PKCS#1 v1.5, or PSS with
  `rsa_pss_saltlen:digest`.
- OpenSSL accepted all 1,000 as made and refused all 1,000 flipped. `std.rsa` gave the same answer every time: **0
  differences**.

### 5.4 Mutants

`python3 scripts/rsa_mutants.py target/release/lex-sys`: **23 mutants, 23 killed**. Each mutant is one plausible bug in
`std/bigmod.ls` or `std/rsa.ls`. The script builds it and runs §5.1 and §5.2's cases, the refusal rows and 2,000 `pow_mod`
rounds. Three mutants survived the first version of the script, and each one taught something:

- **Lazy final reduction.** Montgomery multiplication was changed to subtract `n` only when the result overflows a limb. It
  survived 3,800 cases and the 100,000-round differential, because the result is still right mod `n`. Unreduced values feed
  the next multiplication harmlessly, and the last multiplication by 1 brings the result below `n + 1`. The one wrong answer is
  `n` where 0 is right: `a^e ≡ 0 mod n`, which no random case reaches. The differential now has the `p^j` cases, and they kill
  it.
- **Two limb-straddling mutants were equivalent**, not survivors. A byte's offset in a limb is `8j mod 30`, which is always
  even, so `off > 23` selects exactly what `off > 22` does. They were replaced by `off > 24`, which drops a real case, and
  both are killed.

The mutant "a short signature accepted" was killed by a **trap**, not a wrong answer. With the length check loosened to
`len(sig) > k`, a signature of 0 or 1 bytes (Wycheproof has such cases) makes `pkcs1_verify` lay out its `00 01` past the end
of the block buffer: the block is placed by the signature's length. A one-byte-short signature does not trap; it is refused
as a mismatch. So the length check also keeps the block's indexing in range, and it stays before the block is built. The
driver's buffered output was lost with the trap, so the script reports the case where the answers stop, not the case that
trapped.

### 5.5 Cost

`python3 scripts/rsa_bench.py <driver>`, on one core of an Intel Xeon at 2.80 GHz, with `e` = 65537 and SHA-256:

| | Cranelift | LLVM | OpenSSL 3.0.13 (`openssl speed rsa2048 rsa4096`) |
|---|---|---|---|
| RSA-2048 PKCS#1 v1.5 | 1,109/s (0.90 ms) | 2,808/s (0.36 ms) | 49,794/s |
| RSA-2048 PSS | 1,104/s | 2,582/s | |
| RSA-4096 PKCS#1 v1.5 | 291/s (3.4 ms) | 665/s (1.5 ms) | 12,851/s |
| RSA-4096 PSS | 281/s | 702/s | |

On LLVM, verification is 18 to 19 times slower than OpenSSL's: 30-bit limbs against 64-bit ones with assembly, and a bounds
check on every limb read. A handshake verifies one signature per certificate in the chain plus `CertificateVerify`, usually 2
to 4. With RSA-2048 keys on LLVM that is about 1 ms. Repeated runs vary by about 10%.

By count, about 40% of each verification's Montgomery multiplications compute `R² mod n` (§2.2). For RSA-2048 that is 14,
against 19 for the exponent 65537 and the conversions; this is counted, not timed. Caching `R²` per key would not help a TLS
client, which uses each key once per handshake.

## 6. Found

- **`load` refused a value whose top byte straddles two limbs, even when its high bits were zero.** It refused 648 of the
  first 3,000 random `pow_mod` cases, among them `a` = `0x10530d0f` mod `0x246dd613`, which fits in one 30-bit limb. The
  check was on the byte, not on the bits that spill over. It was found by the first differential run, before any vector, and
  fixed.
- **A mutant equivalent to the code it mutates**, and **a bug class no random input reaches** (§5.4).
- **Five Wycheproof file names that do not exist** (§5.1).

## 7. Not done

- **Signing and decryption.** Not needed by a TLS client, and those would need constant time.
- **SHA-1 and SHA-224.** `std` has neither hash. A SHA-1-signed certificate fails in #206 as `x509-unsupported-algorithm`.
- **The independent review.** #209.
