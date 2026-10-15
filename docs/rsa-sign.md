<!-- Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2 (see /LICENSE). -->

# `std.rsa_sign`: RSASSA-PKCS1-v1_5 signing, and RSA private keys

> **Status: built (steps S0 to S4 of the B2 plan), not approved for production use.** This document was written and committed before
> any gate or code (`git log`: the design, then the gates with their red run, then the code); §1 to §12 are the plan the gates were
> judged against, §13 is what the gates found, and where the work departed from the plan §14 has a dated correction rather than a
> silent edit (`CONTRIBUTING.md`, "How a slice is done", rule 2). Provenance of every number: **measured** (a command in this
> repository, output under `docs/assets/rsa-sign/`).
>
> **NOT APPROVED FOR PRODUCTION USE.** This implementation must not guard a real key until (a) an external cryptographic review
> of §7 to §9 and of the code has been done by someone who did not write it, and (b) the hardening §14 lists as open is done.
> Like every cryptographic module here it is "not independently reviewed (#209)", and it would be the first one to hold an RSA
> private key. It makes **no constant-time claim beyond what §13 reports as measured, with the conditions of that measurement**.
> Numbers from this slice are from one shared Apple-silicon Mac and are not publishable.
>
> Provenance, as in the B2 spike: **measured** (as above), **read** (a
> named file), **[B2]** (stated in `alpibrusl/cancho-spikes` `docs/b2-design.md` and `docs/b2-results.md`, branch `spikes/b2-rsa-sign`,
> PR 2: measured there on a throwaway prototype), **[recalled]**, **[unverified]**.

---

## 1. Why, and the bar

`CONTRIBUTING.md` says a feature earns its way in "when a program asks for it, counted by reading the programs. The bar is two
askers" (`docs/standard-library.md` §5). **That bar is not met in its stated form**: no program *in this repository* signs with
RSA, and the repository has no program that holds an RSA private key. Saying so, and not counting a hypothetical, is what the
rule is for. What exists instead, and why the owner has decided to extend `std` regardless:

| Consumer (outside this repository) | What it needs | Status of the claim |
|---|---|---|
| `cancho-verifactu`: the record signature of the non-VERI\*FACTU Spanish e-invoicing regime | an RSA/SHA-256 (and SHA-512) PKCS#1 v1.5 signature over a digest of the canonicalised record, with the taxpayer's key from a file | **[B2]**: reported by that project's primary-source check; **[unverified]** by this slice. Whether an AEAT endpoint or a XAdES validator *requires* RSA was not established by B2 and is not by this slice |
| `cancho-delivery`: XAdES and WS-Security signatures | the same primitive over `SignedInfo` | named by the owner for this task; I could not find a repository of that name under `alpibrusl` and have read nothing of it: **[unverified]** |
| `packages/tls`: an RSA client certificate (TLS 1.2 `CertificateVerify` with `rsa_pkcs1_sha256`) | the same primitive, over the transcript hash | **[B2]** G1 measured that the client can present only P-256 ECDSA client certificates today (`docs/tls-parity.md` §6, read); **TLS is out of scope for this slice** |

Two or three consumers exist on paper, none is a program in this repository, and all depend on a fact about a third party that
this slice did not check. The decision to build is therefore the **owner's**, not the bar's; it was taken explicitly, and prefers a
native implementation to libssl through FFI (B2 §7 option (i); option (iii), FFI, makes `cancho authority` report `bounded:
false` and puts the private key in C memory). The cost of the decision is stated up front: a module that holds a long-lived
private key, with an ongoing constant-time burden on every backend and architecture, and a review it has not had.

## 2. Scope

**In (B2 `docs/b2-results.md` §6.3, steps S0 to S4):**
- RSASSA-PKCS1-v1_5 signing (RFC 8017 §8.2.1, EMSA-PKCS1-v1_5 §9.2) with SHA-256, SHA-384 and SHA-512; keys of 2,048 to 4,096 bits
  whose modulus is a multiple of 128 bits (§3.2); public exponents that `std.rsa` verifies (odd, 3 up to 64 bits).
- Two-prime private keys through the Chinese remainder theorem (Garner), with exponent blinding (§8) and verification of every
  signature before it is returned (§9).
- Reading unencrypted PKCS#1 (`RSA PRIVATE KEY`) and PKCS#8 (`PRIVATE KEY`, `rsaEncryption`) keys, PEM or DER, with a named
  refusal for each way a key can be wrong, including algebraic consistency (§6).
- The changes to `std.bigmod` that a secret modulus needs (§4.1).

**Out, and not started:** RSASSA-PSS signing (TLS 1.3 needs it; B2 step S6), RSA decryption and OAEP, key generation, SHA-1 and
SHA-224, moduli over 4,096 bits or under 2,048, multi-prime keys, encrypted keys and PKCS#12 (B2 S7: no PBKDF2, CBC or SHA-1 here),
PKCS#11, any change to `packages/tls`, any XML, and the external-signer interface (B2 M6: separate, wanted regardless).

## 3. The API

### 3.1 `std.bigmod` (additions and one change)

| Function | What | Existing code it touches |
|---|---|---|
| `setup_secret(n, w)` | `setup` for a **secret** odd modulus whose byte length is a multiple of 8 and whose top bit is set (a balanced prime): loads with `load_secret`, no branch or index on its value except the two refusals below, R² by the masked doubling | none: a new function |
| `reduce_once(w, a)` | `a` (below 2n) reduced into [0, n) under a mask: `ct_reduce` made public | none |
| `select(w, first, count, idx, dst)` | `dst` = register `first + idx`, reading all `count` registers; the index is in no branch and no address | none |
| `pow_secret(w, base, exp, nwin, dst, table)` | `base^exp` with a **secret** exponent, `nwin` fixed 4-bit windows, every window four squarings and one multiplication, the table read with `select` | none |
| `blind_exponent(w, d, kappa, dst)` | `dst = d + kappa·(n - 1)` for a prime modulus n in `w`, `kappa` a 64-bit word | none |
| `r_squared` (private) | the doubling `if compare_n(..) >= 0 { subtract_n(..) }` becomes `ct_reduce` | **`setup` and so `pow_mod`, `std.rsa`, `std.ecdsa`, `std.ecdh`, `std.p256` users: same values, branch-free** |

**The one change to existing behaviour** is the last row, and it is the reason `setup_secret` does not fork the file as the
B2 prototype did (`b2/proto/bigmod_spike.cho` was a 770-line copy of `std/bigmod.cho` with four functions added). `r_squared`
computes `2^(bits-1)` doubled `64k + 1 - (bits - 1)` times, each time subtracting n when the result reaches it. For a public n
a branch on that comparison is harmless; for a secret n it is a branch on the secret. `ct_reduce` computes the same function for
an x below 2n (which every doubling of an x below n is), so replacing the branch changes **no value** and removes the only reason
`r_squared` could not take a secret. `setup` is called once per `pow_mod`; the doubling loop runs at most 65 times for a public
modulus, so the cost is the difference between a compare-and-maybe-subtract and `ct_reduce`'s three passes over k limbs, to be
and was **measured** (§13.2) on `std.rsa` verification, which is the user that matters.

*Who uses `std.bigmod` (read, `grep` of the tree):* `std/rsa.cho` (`pow_mod`, `bit_length`, `work_len`), `std/ecdsa.cho`
(`setup`, the registers), `std/ecdh.cho`, `std/p256.cho`, `std/p256_pt.cho`, `packages/tls` through those, and the drivers
`tests/programs/{bigmod,rsa,ecdsa,ecdsa_sign,p256}_driver.cho`; tests: `conformance/{rsa,ecdsa,ecdh,ecdsa_sign,p256,tls*}.rs`,
`tests/vectors/bigmod_powers.txt` (100,000 `pow_mod` cases against Python, `docs/rsa.md` §5.3). **All of them must stay green**;
§13.2 runs the whole `cargo test --workspace`, and `scripts/rsa_differential.py` and `scripts/rsa_mutants.py` again.

**What does not change:** `pow_mod`, `inverse`, `load_secret`, `mul`, `add`, `sub`, the slot layout and the register stride of 139
words. `setup`'s *public* contract (it branches on the value of n in `bit_length`, `load` and the parity test) is unchanged and
still documented as public-modulus only.

### 3.2 `std.rsa_sign` (new, `std/rsa_sign.cho`, `edition 7`)

A key is a **block of bytes** in a fixed layout, because a struct cannot hold a slice (`AGENTS.md` §6) and one buffer is one thing
to zero:

```
h = len(p)  (128 to 256, a multiple of 8);   L = 2h = len(n)   (256 to 512 bytes: 2,048 to 4,096 bits, a multiple of 128)
key = n (L) || e (8, big-endian, leading zeros) || p (h) || q (h) || dp (h) || dq (h) || qinv (h)       len = 7h + 8
```

Everything is big-endian, fixed width, left-padded with zeros; `d` is not in the block (CRT signing does not use it; §6 says where it
is checked). A modulus that is not exactly `8L` bits with p and q each exactly `8h` bits and their top bit set is not representable:
it is refused, not repacked (§6, `rsa-key-prime`). That excludes keys such as a 2,047-bit modulus or a 2,056-bit one. OpenSSL, Go and
the JDK generate primes with the top bit set [recalled]; the **policy is a decision for the owner** (§15 q.3) because a
generalised setup costs about 128k doublings that the balanced one avoids (a few percent of a signature [B2]).

| Function | What |
|---|---|
| `pkcs1_sign(hash_len, key, digest, entropy, sig, work) -> int` | EMSA-PKCS1-v1_5, CRT with exponent blinding (§8), verify-after-sign (§9). `hash_len` 32, 48 or 64 as `std.rsa` names hashes; `digest` the hash of the message; `entropy` exactly 16 bytes from the caller; `sig` exactly L bytes; `work` `work_len()` words. 0 or a refusal. `work` and every temporary holding a secret are zeroed before it answers |
| `check_key(key, d, work) -> int` | the algebraic consistency of a key (§6), run when a key is loaded, not on each signature |
| `emsa_pkcs1(hash_len, digest, out)` | the encoding, `len(out)` = L; shares `std.rsa`'s `digest_info` (which becomes `pub`: a visibility change, no behaviour) |
| `pkcs1_sign_test(.., mode, ..)` | **not part of the API**: the same with a fault injected and/or the check or the blinding switched off, so that the fault gate and the timing test can run (as `ecdsa_sign.finish` is public "for tests") |
| `key_len(modulus_len)`, `key_modulus_len(key)`, `refusal_tag(code)`, `work_len()` | layout and names |

`work_len()` is two registers files (one per prime) of `bigmod.registers_len(20)` words, 7,232 words (57.9 KB). **That is under one
region chunk (64 KiB) but not by much**: a caller that allocates `work` and its other buffers from one region can exhaust it, which
traps (`AGENTS.md` §6). The signature recommends `box_slice` on the heap. The 139-word register stride is `bigmod`'s and was built for
4,096-bit moduli; a prime is at most 2,048 bits, so about 70% of this is padding. Shrinking the stride changes every caller's layout
(`bigmod.cho`, "stride"), so it is **not** done here (§15 q.5).

### 3.3 `x509_rsa_key` (new, `packages/x509/rsa_key.cho`)

| Function | What |
|---|---|
| `parse_pem(pem, key, work) -> int` | the first private-key block of a PEM file into `key` (`max_key_len()` = 1,800 bytes); the block's length (> 0) or a refusal |
| `parse_der(format, der, key, work) -> int` | the same from DER, `format_pkcs8()` or `format_pkcs1()` |
| `refusal_tag(code)`, `max_key_len()`, `work_len()` | |

It is in `packages/x509` for the reasons `docs/ecdsa-sign.md` §4.1 gives for `x509_key`: the strict DER reader (`x509.tlv`) is
`x509`'s, and a second one in `std` would be a copy (`conformance/duplication.rs`). It imports `std.rsa_sign` (for `check_key` and the
layout) and `x509_key` for the base64 decoder that does not branch on a key character (`x509_key.base64_decode` and `find_from`, `pub` instead of
private: a visibility change, no behaviour; the alternative is a copy of 40 lines of secret-handling code, which is worse).
`x509_key` is otherwise untouched and still refuses `RSA PRIVATE KEY` as `key-algorithm`: **its behaviour does not change**.

## 4. Algorithm

### 4.1 The private operation

Per prime r in {p, q} with exponent `d_r` and half-length h (R = 2^(8h), balanced prime so R < 2r):

1. `setup_secret(r)`.
2. EM (L bytes, public) = `hi·R + lo`; both halves are below R < 2r, so `load_reduced`; `base = hi·R + lo mod r` as
   `to_mont(hi)` plus `lo`, then `to_mont` again (the spike's shape, B2 `rsa_spike.cho` `crt_half`).
3. `e' = d_r + kappa_r·(r - 1)` (§8), `s_r = base^e' mod r` by `pow_secret`: `nwin = 16(k + 1)` windows for k limbs, whatever
   the value of `kappa` or `d_r`.

Then Garner: `h = qinv·(s_p - s_q mod p) mod p` (with `s_q < q < 2p`, one `reduce_once`), and `s = s_q + q·h` as a **plain integer**
(schoolbook, `mul_wide`, no modulus), which is below n for any `h < p`. The spike did this last step modulo n with a third register
file; the plain product needs none and so no third `work`. Then the check (§9).

*Constant time, the design:* no branch and no index depends on a secret **value**, only on lengths, limb counts and loop counters.
The refusals (`setup_secret`'s top bit and parity, `check_key`'s) read secret bits and branch, but for a valid key their outcome is
fixed, so the executed path does not vary across valid keys; an invalid key learns that it is invalid. That is an **argument**, to be
backed by §13.1's branch audit and timing test; it is not a proof (§7).

### 4.2 Encoding

EM = `00 01 FF.. 00 || DigestInfo || digest` (`std.rsa`'s `digest_info`), L bytes. The digest is public, so this is variable-time
code on public data. SHA-1 and SHA-224 are not offered (`std.rsa` has neither).

## 5. Effects and authority

Every function in `std.rsa_sign`, `bigmod`'s additions and `x509_rsa_key` has the **empty effect row `[]`**: no capability, no
`heap`, no I/O. Temporaries live in `region` blocks inside the function (as `std.rsa.pkcs1_verify` does), which are not a capability.
Randomness is a **parameter** (`entropy`), because cancho has no randomness builtin (`docs/tls-pure.md` §6, read): the caller reads
`/dev/urandom` under a narrowed `Fs("/dev/urandom")` and the program's `cancho authority` report says so. Consequence, to be
**checked** (§13.1, G8): a program that parses a key from a string and signs reports `bounded: true` and *no* label from these modules;
the label set of the test driver is exactly its console (`io_read`, `io_write`) and `heap`, and no foreign symbol.

## 6. Refusals: every one has a named rule and a fixture

Codes are per module (as `ecdsa_sign` -70.., `x509_key` -80..); `refusal_tag(code)` names them, and `std.rsa_sign.refusal_tag` falls
through to `std.rsa`'s and `std.bigmod`'s. The **expected rule for each malformed key is fixed in the corpus before the parser
exists** (§11), because OpenSSL is not an oracle for refusals: B2 measured that OpenSSL 4.0.3 `pkey` accepts 16 of 28 malformed
keys [B2].

`std.rsa_sign` (signing, -140 to -149):

| Tag | When |
|---|---|
| `rsa-sign-hash` | `hash_len` not 32, 48 or 64 |
| `rsa-sign-digest-length` | `len(digest) != hash_len` |
| `rsa-sign-key-layout` | `len(key)` is not `7h + 8` for an h in {128, 136, .., 256} |
| `rsa-sign-entropy` | `entropy` not 16 bytes |
| `rsa-sign-output-length` | `sig` not L bytes |
| `rsa-sign-work-length` | `work` shorter than `work_len()` |
| `rsa-sign-key-shape` | a prime's top bit is not set, or a prime is even (`setup_secret`'s refusals), or n is not `8L` bits |
| `rsa-sign-check` | verify-after-sign refused the signature: a fault, or a key that passed no `check_key` |

`std.rsa_sign.check_key` (-150 to -159):

| Tag | When |
|---|---|
| `rsa-key-size` | n is not exactly `8L` bits in [2,048, 4,096], or L is not a multiple of 16 |
| `rsa-key-modulus` | n zero, even or negative (as encoded) |
| `rsa-key-exponent` | e even, below 3, over 64 bits, or (as `std.rsa` requires) not below n |
| `rsa-key-prime` | p or q even, p = q, or not exactly `8h` bits with the top bit set |
| `rsa-key-product` | `p·q != n` |
| `rsa-key-crt-exponent` | `e·dp mod (p - 1) != 1` or `e·dq mod (q - 1) != 1`, or dp or dq not in [1, p - 2] (resp. q) |
| `rsa-key-crt-coefficient` | `q·qinv mod p != 1`, or qinv not in [1, p - 1] |
| `rsa-key-private-exponent` | d is 0, not below n, or `d mod (p - 1) != dp`, or `d mod (q - 1) != dq` |

`x509_rsa_key` (-170 to -179): `rsa-key-pem` (no block, no END line or another label, a character outside base64, bad padding),
`rsa-key-encrypted` (`ENCRYPTED PRIVATE KEY`, `Proc-Type: 4,ENCRYPTED`), `rsa-key-algorithm` (another `* PRIVATE KEY` label, a
PKCS#8 algorithm that is not rsaEncryption), `rsa-key-parameters` (rsaEncryption's parameters not NULL), `rsa-key-der` (truncated,
trailing bytes, wrong tag, non-minimal or empty INTEGER, an element missing or extra), `rsa-key-version` (PKCS#1 version other than 0
or 1; PKCS#8 version other than 0), `rsa-key-multiprime` (PKCS#1 version 1), `rsa-key-buffer-length` (the caller's `key` shorter
than `max_key_len()` or `work` short), `rsa-key-block-size` (more than 8,192 decoded bytes). It also returns `check_key`'s tags.

**What is deliberately not checked, and why it is safe to say so:** primality of p and q (a composite p whose exponents satisfy `e dp = 1 mod (p - 1)` parses, and
its signatures fail the check at signing, `rsa-sign-check`: *corrected 2026-10-14, §14*; a probable-prime test is 1 exponentiation each and was not asked for), that the key belongs to any certificate, and key strength beyond the size and exponent rules. `d` is checked against `dp`
and `dq` (an exact long division, no branch on the operands) but is then discarded.

## 7. Constant time: what is required, what is achieved, what is only argued, what is untested

**Required of the signer:** no branch, memory index or variable-latency instruction whose operand is derived from p, q, dp, dq, qinv
or `entropy`, on the targets and backends in use, on the path from the key block to the signature.

| | Status as of this document |
|---|---|
| Montgomery multiplication, the masked reduction, add and sub | **Achieved by existing code, with caveats on record**: `std.bigmod` registers are "constant time" and carry `std.ecdh`'s secret scalar (`std/bigmod.cho` header, `docs/ecdh.md` §2, read); the same documents record that `clang -O2` turned masks back into branches (|t| = 13) until `value_barrier` was added, and that Cranelift's P-384 ladder still failed at |t| = 11 on an M4 (`docs/ROADMAP.md` #264, read) |
| Table read by `select` over 16 registers | **Argued** by reading the source, **to be audited** in the object code (§13.1) |
| `pow_secret`, `blind_exponent`, `setup_secret`, Garner's plain product | **Argued**; new code; nothing measured yet. B2 measured the prototype's equivalents once: no secret-dependent branch found by classification of 113 conditional branches, |t| <= 1.9 at process granularity, positive control t = -633 [B2] |
| Key loading (`check_key`, the parser) | **Branches on validity** by design (§4.1), base64 without a branch on a character; runs once at load; audited, **not** timing-tested (as `docs/ecdsa-sign.md` §4.3) |
| Cranelift backend; x86-64; any CPU with a data-dependent multiplier; cache and microarchitectural channels; power, EM, Hertzbleed-type frequency channels | **Untested.** `docs/tls-parity.md` §3.1.1 and `docs/tls-assurance.md` §6.1 record that cancho's own timing work twice found backend- and CPU-specific failures that a first pass missed |
| Compiler upgrades | **No protection**: a new LLVM can turn a mask into a branch; only a re-run of the branch audit and the timing test (§13.1) detects it |

The honest summary to be repeated in the PR: *a timing test at process granularity found no difference on one backend and one
architecture, a branch audit read the object code of the same, and the rest is argument.*

## 8. Blinding and entropy

**What is implemented: exponent blinding**, per CRT half: `d_r' = d_r + kappa·(r - 1)` with `kappa` a 64-bit word, top bit forced
(63 bits of entropy, the exponent length fixed), taken from the caller's 16 bytes (8 per half). By Fermat `x^(r-1) = 1` for
x not divisible by r, and for x ≡ 0 the result is 0 either way, so the signature is unchanged. It costs 64 bits on a 1,024 to
2,048-bit exponent, about 3 to 6% of the multiplications [B2, estimate]. It needs no inversion.

**What is not implemented: message (base) blinding** (multiply by `rᵉ`, divide by `r` afterwards). It needs `r⁻¹ mod n`: a second
full exponentiation per prime (a signature takes about twice as long) or a constant-time extended Euclid (about as long as the
signature [B2, estimate, not measured]). The decision to take exponent blinding only is the author's and **open for the reviewer**
(§15 q.4). Exponent blinding randomises the exponent; it does **not** randomise the base, so an attack whose signal is the base's
value is not addressed.

**What is claimed about blinding:** only that (i) a blinded signature equals the unblinded one, byte for byte (the signature is
deterministic, so this is checkable against OpenSSL); (ii) two calls with different entropy use different blinded exponents
(observable through a test hook); (iii) the timing test of §13.1 was run with it on and off. **Nothing is claimed about how much
protection it gives**: the attacks it targets (power, EM, finer timing) were not tested.

**Entropy** is the caller's. Zero bytes are accepted (the API cannot tell) and then give a fixed kappa, i.e. a fixed blinded exponent:
the signature is still correct and the key is no worse protected than unblinded. The documentation says that a caller must pass fresh
random bytes per signature.

## 9. Fault attacks

A single fault in one CRT half gives a signature `s` with `gcd(sᵉ - EM, n)` = a prime factor (Boneh, DeMillo and Lipton 1997;
Lenstra 1996 [recalled]). **Every signature is verified with `std.rsa.pkcs1_verify` under (n, e) from the block before it is
returned**; a signature that does not verify is zeroed and refused (`rsa-sign-check`). The B2 prototype measured the check at 4% of
a signature (27 to 67 µs) and showed the factor falls out of an unchecked faulty signature on 6 of 6 keys [B2]. What this covers:
a fault anywhere between the key block and the result. What it does not: a fault in the verification itself, in the final copy,
or in the branch that decides to return (a glitch attack on the comparison); the cost of the check is the price of the block's `e`
being public. The fault gate (§11) injects into both halves, the Garner step, and the key block.

## 10. The Python and OpenSSL references, and what they cannot show

Judges of correctness (§11) are OpenSSL 4.0.3 (Homebrew, the B2 oracle) and a pure-Python RFC 8017 reference with Python's `pow`.
A bug shared by both (a misreading of the key file) is controlled by the third and fourth judges, the standard's known-answer
vectors, and by testing several `e`, sizes and `p < q` and `p > q` orders. None of these can show constant-time behaviour, a refusal's
rule name, or anything about the other compiler backend.

## 11. Gates, written before the code they judge, each shown failing once

Each gate is committed with a **stub** of the module (every function answers a "not built" code) and run: the red output is kept under
`docs/assets/rsa-sign/red/`. Only then is the code written; the same commands are then run again (`.../green/`).

| # | Gate | Judges | Red run |
|---|---|---|---|
| G1 | **Known-answer vectors**: NIST CAVP `SigGen15_186-3` (n, e, d, message, S; SHA-256/384/512, 2,048 and 3,072 bits), Wycheproof `rsa_pkcs1_{2048,3072,4096}_sig_gen_test` (valid cases: byte-identical; the file's refused sizes 1,024 and 1,536: refused) — `crates/cancho/tests/conformance/rsa_sign.rs`, both backends | the encoding, the private operation, the standard's keys | stub: all fail |
| G2 | **Differential** against OpenSSL (`openssl dgst -sign`: byte-identical; `openssl dgst -verify`: accepted; a flipped bit refused) and the Python reference, over throwaway keys: 2,048/3,072/4,096 bits, e = 65537 and 3, unbalanced primes (q/p about 1.7, both orders), p/q swapped, SHA-256/384/512; also `std.rsa.pkcs1_verify` accepts and refuses the flipped one — `scripts/rsa_sign_differential.py` | the whole path | stub: all fail |
| G3 | **Malformed-key corpus with its own expected rule names**, fixed in `tests/vectors/rsa_sign/malformed.txt` before the parser; generated by `scripts/rsa_sign_keys.py`; OpenSSL's verdict is recorded only to show it disagrees | the parser's refusals | stub: every case answers the stub's code |
| G4 | **Fault injection**: a bit flipped in each half (and in `qinv`, in `n` of the block) with the check on must refuse and return no signature; with the check off the faulty signature must reveal a prime factor by one gcd | §9 | stub / the check removed (mutant) |
| G5 | **Mutants**: one plausible bug each in `bigmod`'s additions, `rsa_sign` and `x509_rsa_key`, as `scripts/*_mutants.py`; the gates must go red; a survivor is reported and either killed by a new case or argued equivalent | the gates themselves | n/a: a mutant run is the red run |
| G6 | **Branch audit** of the object code (arm64 as in B2; the Linux x86-64 audit script exists and needs a Linux host) | §7 | n/a |
| G7 | **Process-level timing** (as B2, `dudect_proc`): fixed key against random valid keys, and low-weight against random exponents, blinding on and off, with the **positive control** (`bigmod.pow_mod` on a secret exponent) that must fail the same test | §7 | the control must fail; a harness that cannot flag it makes the pass meaningless |
| G8 | **Authority**: `cancho authority` on the driver | §5 | stub reaches nothing: the driver's own check is the gate |
| G9 | **Registry and style**: `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace` (file budget, link check, duplication, every README row), `cancho fmt --check` of the new `.cho` files | the repository's own rules | n/a |
| G10 | **Existing suite**: everything that touches `std.bigmod` stays green; `scripts/rsa_differential.py`, `rsa_mutants.py` re-run | §3.1's blast radius | n/a |
| G11 | **Mutational fuzz of key files** (deterministic, in Python; cancho's AFL++ setup is not available here): thousands of mutations of valid and malformed keys must each answer a code, never a trap | "input never reaches a panic" | n/a |

Not run in this slice, and said so: the Linux x86-64 audit and timing, a per-call
cycle-counter timing test (`ecdsa_sign_timing`-style, needs the `tick` library), cache-timing and power analyses, AFL++.

## 12. Vectors, licences and keys

Downloaded from the official sources only, into a scratch directory, treated as data (never executed; Python `-I`), with URL, version
or commit, retrieval date and SHA-256 recorded in `THIRD_PARTY.md`: NIST's `186-3rsatestvectors.zip` from `csrc.nist.gov` (works of
the US government; the repository already carries NIST's SigVer files on that footing, `tests/vectors/cavp/README.md`), and Wycheproof
(`C2SP/wycheproof` at a pinned commit, Apache-2.0: the repository already carries those files with the Apache licence text, so the
same arrangement is kept: data files, unmodified, with `LICENSE` beside them; **no code is derived from them**). RFC 8017 publishes no
signing vectors and RSA Laboratories' `pkcs1v15sign-vectors.txt` is not on an official source I may use (the FTP site is gone): not
used, reported. **Every key in the repository is a throwaway test key**: the NIST and Wycheproof keys are published test keys; the
rest are generated by `scripts/rsa_sign_keys.py` from a fixed seed and are not used anywhere. `.gitguardian.yml` is not extended: the
committed keys are real key material by shape and keep scanning, as `tests/vectors/tls/echo/*/key.pem` do.

Copyright in the code and documents of this slice is Alpibru S.L.; the licence is EUPL-1.2 like the rest of the repository
(`/LICENSE` names "lex-lang contributors" as holder and is not edited). Apache-2.0 data and EUPL-1.2 code sit side by side as they
already do; if the owner reads that as a conflict, that is for the owner (§15 q.6), not for the author, to decide.

## 13. What will be reported, and how

Each of the following becomes a section "Results" of this document, with the command, the machine (`uname -a`), the backend, the
commit, the sample size and the conditions, **in the PR and here**; a number without them is not reported.

1. Gates G1 to G4 and G10: counts of cases and of failures, both backends.
2. G6: the conditional branches of the secret path classified, with the ones **not** proved named.
3. G7: |t| per test and the control's t.
4. G5: killed/total, and each survivor.
5. Speed against `openssl speed rsa2048/3072/4096` in the same session; the cost of the check and of blinding; the cost of
   `ct_reduce` in `r_squared` on `std.rsa` verification.
6. The whole test suite, before (the base commit) and after.
7. What the reviewer is asked: §7's table, §8's choice, §15.
8. G8's report.

### 13.1 Results

All on **one shared Apple M4 Max Mac** (`uname -a`: Darwin 25.2.0 arm64, load average about 10 during every run, nothing pinned), commit
`54152c8c` plus the working tree of the next commit, OpenSSL 4.0.3, LLVM backend unless a line says Cranelift. Raw outputs, with the date, the
command and the commit, are under `docs/assets/rsa-sign/{red,green}/`. **Not publishable.**

**Gates shown failing first** (`docs/assets/rsa-sign/red/`): against stubs that answer "not built", the conformance suite failed 9 of 10 tests (the
tenth, authority, passes against a stub that reaches nothing, as §11 G8 said it would; its own red is a mutant that takes a capability), and the
scripts failed every case (`sign` 152/152, `parse` 84/126, `blinding` 189/189, `bigmod` 400/400, the fault gate 17/17, the fuzz harness 600/600).

**Correctness, green:**

| Gate | Result |
|---|---|
| G1 NIST CAVP `SigGen15_186-3` (through the PEM parser, blinded, checked), 2,048 and 3,072 bits, SHA-256/384/512 | **60 of 60** byte-identical to NIST's S, on **both backends** (the Python reference also reproduces all 60) |
| G1 Wycheproof `RsassaPkcs1Generate` | **72 valid cases** (2,048/3,072/4,096 bits, SHA-256/384/512) byte-identical; PKCS#8 and PKCS#1 forms of every key give one block; the 1,024- and 1,536-bit keys refused `rsa-key-size`. **5 `SmallPublicKey` cases refused `rsa-key-prime`**: those keys have e = 3 and primes of 1,364 and 684 bits (or 2,047 and 1,025), outside the balanced-prime policy of §3.2 (§15 q.3). 16 SHA-1/SHA-224 cases counted, not run |
| G1 our own keys (`valid.txt`): e = 3, 17, 65537, 2,176-bit n, p above and below q, primes 1.1 and 1.9 apart | **24 of 24**, both backends |
| G2 `rsa_sign_differential.py sign`: 21 throwaway keys (2,048 to 4,096 bits, e = 3/17/65537, unbalanced both orders, swapped, a 2,176-bit modulus), fresh entropy a call | **388 cases, 0 failures**: each byte-identical to `openssl dgst -sign` and to the Python reference, accepted by `std.rsa.pkcs1_verify` and by `openssl dgst -verify`, and the same signature with one bit flipped refused by both |
| G2 `parse`, `blinding` | 126 and 189 cases, 0 failures; the blinded exponent of both halves is `d + kappa (r - 1)`, exactly, for every key |
| G2 `bigmod`: `setup_secret` + `pow_secret`, `reduce_once`, `blind_exponent` against Python integers, moduli of 8 to 512 bytes (all ones, 2^k + 1, random) | **20,000 cases, 0 failures** |
| G3 malformed-key corpus, 105 files, each with its own expected rule fixed before the parser: 89 refused with **16 distinct rules**, 16 accepted (one of which must, and does, fail at signing with `rsa-sign-check`) | **all 105 as expected**, both backends. `openssl pkey` accepts **48 of the 89** that must be refused, `openssl rsa -check` 20 (`corpus_openssl.txt`): OpenSSL is not an oracle for refusals |
| G4 fault gate, 17 keys of e = 65537 | every fault (p-half, q-half, qinv, the result) **refused with `-147` and no signature**; unchecked, the faults in the p-half, the q-half and qinv give a signature from which one gcd yields a prime factor (**51 of 51**), the result fault does not |
| G5 mutants (77: 14 in `bigmod`, 36 in `rsa_sign`, 27 in `rsa_key`) | **77 of 77 killed** (`mutants.txt`). Mutants run before this one found 8 gaps, all closed by new cases and not by weakening a mutant: see §14 |
| G11 deterministic mutational fuzz, 20,000 mutations of the corpus through `parse_pem` and both `parse_der` formats | 60,000 answers, **0 traps, hangs or unknown tags** (18 distinct tags seen); not coverage-guided |
| G8 `cancho authority` on the driver | `bounded: true`; effects exactly `heap`, `io_read`, `io_write` (the driver's own); no foreign symbol; never touches the filesystem, network, command line, signals, other programs |

**Speed** (`rsa_sign_bench.py`, best of 5, same session as `openssl speed`; ms a signature, 2,048 / 3,072 / 4,096 bits):
OpenSSL 0.334 / 0.968 / 2.121; `pkcs1_sign` 0.655 / 1.929 / 4.277, i.e. **1.96 to 2.02 times OpenSSL**; the verify-after-sign is **1.5 to 2.0%** of it; the
exponent blinding **2.5 to 4.9%**; the same signature with no CRT (`bigmod.pow_mod`, the leaky control) 2.219 / 7.382 / 17.096. About 1,500 signatures a second at 2,048
bits on one core. These match the B2 prototype (0.651 / 1.916 / 4.233 ms with the check) [B2].

**Side channels, with their conditions:**
- *Branch audit* (G6), arm64 object code of the driver, LLVM backend: 23 functions of the secret path, **180 conditional branches that do not go to a trap
  (87 back, 93 forward)**; on Cranelift 92 (3 back, 89 forward) in the same functions. Every compare's operand was classified from the disassembly and by a
  straight-line scan for the instruction that last wrote it (`rsa_sign_branches.py`; not exact data flow). Operands loaded from memory are 41 (LLVM) and 18 (Cranelift),
  and all are: the limb count `w[0]`, the lengths and the `mode` and `blind` parameters (spilled to the stack), the region allocator's chunk cache (public runtime
  state), `n[0]` (public), and **the two refusal tests of `setup_secret` (the top bit and the low bit of a prime), which read one bit each of a secret and whose outcome
  is fixed for every valid key**. Everything else compares an immediate, a count, a position or a return code. **I did not hand-prove each of the 180 branches**, and the audit
  sees branches, not memory addresses (those were read in the source: `select` reads all 16 entries; every other index is a loop counter or a position). One compiler
  version, one architecture; **x86-64 not run**.
- *Timing* (G7), process granularity, 2,048 bits unless stated; |t| < 4.5 passes; `rounds` signatures a sample, so a leak that averages out within a sample is hidden:

| Test | samples a class | Welch t (all / slowest 10% cropped) |
|---|---|---|
| **positive control**: `bigmod.pow_mod` on a secret exponent, low-weight against random | 800 | **-422.5 / -638.2** (LLVM); -263.8 / -504.9 at 4,096 bits; -312.5 / -351.2 on Cranelift |
| signer, dp and dq of 64 set bits against random (the key is then inconsistent, so check off), blinded | 1,500 | -0.65 / -0.90 |
| the same, **not blinded** | 1,500 | -0.21 / -0.05 |
| signer as shipped, key 0 against a random other valid key (balanced, unbalanced and swapped among them) | 1,500 | 0.24 / 0.74 |
| the same, not blinded | 1,500 | 1.14 / 1.83 |
| 4,096 bits: exponents / keys | 800 | -0.98 / -1.79; -0.60 / -1.78 |
| **Cranelift**, 2,048: exponents / keys | 500 | -0.99 / -1.78; -0.19 / -0.16 |

  The harness sees the leak a bit-branching exponentiation has (the control) and saw no difference in any test of the signer. **That is "no evidence of a leak at this
  sensitivity", not "constant time".** Not tested: a per-call cycle-counter test, x86-64, other CPUs, the dependence of time on the message (the digest is public), cache
  and microarchitectural channels, power, EM, frequency channels. The load was about 10 on a 16-core machine throughout; nothing was pinned.

### 13.2 What changed in existing code, and what that touched

| Change | Effect | Checked by |
|---|---|---|
| `std/bigmod.cho` `r_squared`: `if compare_n(..) >= 0 { subtract_n(..) }` becomes `ct_reduce(..)` | same values; reaches `setup`, so `pow_mod`, `std.rsa`, `std.ecdsa`, `std.ecdh`, `std.p256`'s callers, `packages/tls` | the whole suite below; `rsa_differential.py pow` **100,000 cases, 0 differences**; `rsa_mutants.py` **26 of 26 killed**; **RSA verification cost, base against branch, interleaved, best of 9: 25.25 against 25.24 us (2,048 bits), 54.33 against 54.27 (3,072), 92.62 against 93.45 (4,096, +0.9%)**: no change beyond noise (`verify_cost.txt`) |
| `std/rsa.cho` `digest_info`, `packages/x509/key.cho` `base64_decode` and `find_from`: `pub` | none | the suite; `x509_key` still refuses `RSA PRIVATE KEY` as `key-algorithm` |
| `crates/cancho/src/main.rs`: `std/rsa_sign.cho` in the table of std modules | one more module compiled in with `--std` | the suite |
| `scripts/rsa_mutants.py` | two fixes it needed: it globbed Wycheproof files by the prefix `rsa_` and so read the new signing vectors, and one mutant's text now also occurs in `setup_secret` | its own run above |
| `tests/vectors/{cavp,wycheproof}/README.md` | new sections for the new files | the documentation link check |

**The whole suite** (`cargo test --workspace --no-fail-fast -- --test-threads=3`, this branch): 699 passed and 1 failed in the conformance binary, all other
binaries passing. The base commit (`4e619793`) gives 688 passed and the **same one failure**, `backends::spawn_and_join_run_concurrently_not_sequentially`: it times
four 200 ms sleeps and calls 400 ms "sequential", and fails on this Mac at a load average of about 9 to 11 for both commits and both backends (436, 417 and 441 ms
in three repeats alone). It is unrelated to this change and is not fixed by it. The 11 more passing tests are `rsa_sign::*`.
`cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` are clean; `cancho fmt --check` on the new `.cho` files is clean.
Not run: `scripts/ecdh_mutants.py` and the `openssl` mode of `rsa_differential.py` (both import pyca/cryptography, which is not installed on this Mac), the whole
suite on Linux x86-64, and CI.

## 14. Corrections made while working

*(each is added here, dated, with the commit that made it.)*

- *2026-10-14, §4.1 step 3.* The design said the blinded exponent needs `16(k + 1) + 1` windows (64 (k + 1) + 1 bits). It is below
  `2^(64 (k + 1))` (d < 2^(64k), kappa (r - 1) < (2^64 - 1) 2^(64k)), so `16(k + 1)` windows are enough and the extra one only
  multiplied by the table's first entry. Found by asking whether the mutant that drops that window was equivalent. The code uses
  `16(k + 1)`.

- *2026-10-14, §11 G3/G5, the corpus grew from 88 to 105 files while the mutants were run.* Mutants that survived, each a gap in a gate (the code was not wrong):
  `h` need not be a multiple of 8 (no key of h = 132 in the corpus: `modulus-2112-bits-...`); dp and qinv range checks (only congruent values that still fit
  the slot can reach them: `dp-plus-p-minus-1-fits-the-slot`, `qinv-plus-p-fits-the-slot`); d below n and d longer than n (`d-congruent-to-d-but-not-below-n`,
  `d-congruent-to-d-and-longer-than-n`); d checked against dq as well as dp (`d-matches-dp-only`, `d-matches-dq-only`); the PKCS#8 NULL followed by another
  element (`pkcs8-parameters-null-then-an-extra-null`); a negative INTEGER in each field (six `*-negative` files); a non-minimal FF FF; and a 9-byte e that the
  parser skipped silently, which only a key buffer filled with 0xa5 instead of 0 shows (the driver now fills it). Two conditions were **removed** from the code as
  redundant, because no input can reach them: `dp = 0` and `qinv = 0` and `d = 0` are already refused by the congruence tests (`e * 0` is not 1 mod anything).
  Two mutants were **equivalent and were replaced**: dropping a leading zero byte of EM or of e changes nothing.
- *2026-10-14, §3.2 and §6, the balanced-prime policy refuses real test keys.* Wycheproof's e = 3 `SmallPublicKey` keys (5 of its 16 usable key groups) have primes of
  very different sizes (1,364 and 684 bits; 2,047 and 1,025). They are refused as `rsa-key-prime`. This is the policy of §3.2 doing what it says, and the first
  evidence that it excludes keys somebody generated; §15 q.3 stands.
- *2026-10-14, §6, composite primes.* The design (§6) said a composite p with consistent exponents signs correctly. It does not: the CRT with a composite "p" is
  wrong, the signature fails its check, and the corpus case `composite-p-with-consistent-exponents` is a key that **parses** (primality is not checked) and is refused
  at **signing** with `rsa-sign-check`. A key whose `e * dp = 1 mod (p - 1)` fails is refused when loaded.

**Open, not done in this slice (hardening the review is expected to ask about):** message blinding; a per-call cycle-counter timing
test; timing on Cranelift and on x86-64; a probable-prime check; shrinking the register stride; PSS; the external signer interface;
the TLS integration; the independent review itself.

## 15. Questions for the owner

1. **The bar** (§1): proceed on the owner's decision, as stated. Is the second real asker something that can be pointed to
   (`cancho-delivery` was not found)?
2. **Review**: who, and before which consumer ships.
3. **Balanced primes only** (§3.2): refuse keys whose primes do not have the top bit set, or pay for the general setup?
4. **Exponent blinding only** (§8): enough, or is message blinding required before any use?
5. **`work` of 57.9 KB**: accept, or shrink the register stride for everyone?
6. **Apache-2.0 data in an EUPL-1.2 repository**: same as the existing Wycheproof files; confirm.
