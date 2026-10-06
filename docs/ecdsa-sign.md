# `std.ecdsa_sign`: ECDSA P-256 signing in constant time, and P-256 private keys

> **Status: built. Not independently reviewed (#209).** Step 1 of the TLS server (`docs/tls-server.md` §8): the signature a
> server sends in every full handshake's `CertificateVerify`, made with a long-lived secret key, and the parsers that read
> that key from a PEM file. The design is `docs/tls-server.md` §3 and §4; this document is what was built, how it was
> checked, and what building it found.

---

## 1. What is built

| File | What |
|---|---|
| `std/ecdsa_sign.ls`, `module std.ecdsa_sign` | `sign`, `sign_checked` and `to_der` on P-256 with SHA-256 (`ecdsa_secp256r1_sha256`) |
| `packages/x509/key.ls`, `module x509_key` | `parse_pem`, `parse_der`, `public_point` and `matches_certificate`, for P-256 keys in PKCS#8 and SEC 1 |
| `std/bigmod.ls` | `load_secret`: a register loaded from a secret without a branch or an index on its value (§2.2) |
| `std/ecdh.ls` | `scalar_ok`: `in_range`'s constant-time check of a scalar against n, public |
| `tests/programs/ecdsa_sign_driver.ls`, `ecdsa_sign_timing.ls` | the drivers of §5 and §6 |
| `scripts/ecdsa_sign_{differential,keys,mutants,timing,bench}.py` | the evidence (§5 to §7) |

**The API.**
- **`sign(digest, key, extra, sig, work)`** signs a SHA-256 digest (32 bytes) under a private key (32 bytes, big-endian, in
  [1, n)), into `sig` as r || s (64 bytes). `extra` is the 32 bytes of added randomness RFC 6979 §3.6 allows, from the
  caller's DRBG, or empty for RFC 6979's deterministic nonce. `work` is `work_len()` words, `std.ecdh`'s, and is zeroed before
  the answer.
- **`sign_checked(digest, key, point, extra, sig, work)`** signs, then verifies the signature with `std.ecdsa.verify_raw` under
  `point` before giving it out (`docs/tls-server.md` §3.3). A signature that does not verify is zeroed and refused. So is one
  made with a `point` that is not the key's, which the engine's own check (§4) should already have ruled out.
- **`to_der(sig, out)`** writes r || s as the DER `SEQUENCE { INTEGER r, INTEGER s }` TLS sends, at most `der_max()` = 72 bytes.

| Code | Tag | When |
|---|---|---|
| -70 | `ecdsa-sign-digest-length` | the digest is not 32 bytes |
| -71 | `ecdsa-sign-key-length` | the key is not 32 bytes |
| -72 | `ecdsa-sign-key-range` | the key is 0 or not below n |
| -73 | `ecdsa-sign-extra-length` | the added randomness is neither empty nor 32 bytes |
| -74 | `ecdsa-sign-output-length` | `sig` is not 64 bytes; for `to_der`, `out` shorter than the encoding |
| -75 | `ecdsa-sign-work-length` | `work` shorter than `work_len()` |
| -76 | `ecdsa-sign-nonce` | 16 nonces in a row were out of range or gave r = 0 or s = 0: each has a probability near 2^-32, so no input reaches it |
| -77 | `ecdsa-sign-check` | `sign_checked`: the signature did not verify under `point` |

**Why a module of its own.** The design says `std.ecdsa.sign`. But signing multiplies with `std.ecdh`'s ladder, and
`std.ecdh` imports `std.ecdsa` for the curve constants (`curve_param`, `copy_point`), so `std.ecdsa` cannot import `std.ecdh`
back. A new module that imports both is the change that moves nothing; `std.ecdsa` stays verification, on public data, as
`docs/ecdsa.md` describes it.

## 2. Constant time

### 2.1 The signature

SEC 1 §4.1.3, with every operation that touches d or k one that `std.ecdh` already made constant time:

```
k        = RFC 6979's nonce (§3)                         HMAC-SHA-256 over d, h and the added randomness
(x, y)   = k·G       std.ecdh.public_key                 the 4-bit window over complete formulas, the table read under masks
r        = x mod n   bigmod.load_reduced                 x is public once r is sent
s        = k^-1 (e + r·d) mod n                          bigmod: load_secret, to_mont, inverse, mul, add
```

- **k·G is `std.ecdh.public_key`**, the same ladder the key exchange uses (`docs/ecdh.md` §2), with G as the point. No second
  ladder was written; the fixed-base table `docs/tls-server.md` §3.1 names as a later optimisation is still later. Its scalar
  range check is constant time and answers only in range or not.
- **The arithmetic mod n is `std.bigmod`'s registers**: `mul` (Montgomery, with the masked `ct_reduce`), `add` and `sub`
  (masked), and `inverse`, which is Fermat's `a^(n-2)`: its square-and-multiply branches on the bits of n − 2, which are
  public, so its time does not depend on k.
- **d and k are loaded with `bigmod.load_secret`** (§2.2), not `load_reg`.
- **The key's range** is checked with `std.ecdh.scalar_ok`, `std.ecdh`'s borrow over every byte; only the answer is public.

**What stays variable time, all of it on public values:**
- the lengths of the inputs and the curve's sizes;
- whether a candidate k is in [1, n): one that is not is discarded and the next drawn (RFC 6979 §3.2 step h), and whether a
  discarded candidate was out of range says nothing about the k kept;
- r == 0 and s == 0, each with a probability of about 2^-256, and both public once the signature is: `is_zero` stops at the
  first nonzero limb of a value about to be sent;
- the DER encoding of r and s;
- the digest, reduced mod n with `load_reduced`: it is the hash of a message the peer sees.

### 2.2 What reading a secret into a register needed

`bigmod.load_reg` was written for public values (`docs/rsa.md`): `load` skips zero bytes (`if v != 0`) and `compare_n`
stops at the first limb that differs from n. Either one, given a private key, would be a branch on it. ECDH never met this,
because its scalar is read a nibble at a time by the ladder and never enters a register. `load_secret` places every byte by
its position only: which limb it lands in, and whether it straddles two (`off > 22`), depend on where it is, not what it is.
It does no range check; the caller has made one in constant time.

### 2.3 The audit

`scripts/chacha20_branches.py` (it now also takes a package's symbol, `lexs_x509_key.within`) over the object of
`tests/programs/ecdsa_sign_driver.ls`, built by the LLVM backend on Linux x86-64. Every conditional jump that is not to a
trap was read in the disassembly:

| Function | The jumps that are not to a trap compare |
|---|---|
| `bigmod.load_secret` | the limb count, the length, the loop counter, and `off > 22` (`cmp $0x17,%rcx`): positions |
| `ecdh.in_range`, `ecdh.scalar_ok` | the loop counter and the curve's size; the borrow and the OR are `seta`/`setg`, not jumps |
| `ecdsa_sign.finish` | `is_zero` on r and on s (public, above), `store_reg`'s positions, `set_small`'s counter |
| `ecdsa_sign.reseed`, `rekey`, `candidate`, `copy32`, `wipe` | the HMAC state's length (`cmp $0xca`), and counters |
| `ecdsa_sign.sign` | the lengths, the attempt counter, `public_key`'s answer and the code: §2.1's public values |
| `lexs_x509_key.within`, `base64_value` | nothing but overflow traps: the masks stay arithmetic through `value_barrier` |
| `lexs_x509_key.base64_decode` | whether a character is base64 at all (`cmp $0xff`), and only for one that is not: whitespace (`bt`), `=`; the count of padding and of bits, which are positions; and the final check that the bits left over are zero, whose answer is the file's validity |

The functions `std.ecdh` and `std.bigmod` already had (`mont_mul`, `ct_reduce`, `add`, `sub`, `select`, `add_points`,
`double_point`, `multiply`) show the same jumps `docs/ecdh.md` §3 read: loop counters and `k`. `inverse`'s `bt %rax,%rcx`
is its test of a bit of n − 2.

## 3. The nonce

RFC 6979 §3.2 over HMAC-SHA-256, with §3.6's additional data. For P-256 with SHA-256, qlen = hlen = 256, so `bits2int` is
the 32 bytes as they are, `int2octets(x)` is the key, and `bits2octets(h1)` is the digest reduced mod n once.

```
V = 01 × 32,  K = 00 × 32
K = HMAC_K(V || 00 || d || h || extra);  V = HMAC_K(V)
K = HMAC_K(V || 01 || d || h || extra);  V = HMAC_K(V)
loop: V = HMAC_K(V); k = V; if 1 <= k < n and r, s != 0: done
      else K = HMAC_K(V || 00); V = HMAC_K(V)
```

- **Deterministic** in d and h, so a broken DRBG cannot give two messages the same k: that is the failure that gives a key
  away (Sony's PlayStation 3, Android's `SecureRandom` in 2013).
- **Hedged** with the caller's 32 bytes, so a fault injected into one signature does not recur when the same message is
  signed again, and the same message is not signed twice with the same k when the randomness is good.
- With `extra` empty the nonce is RFC 6979's exactly, which is what lets Appendix A.2.5's vectors test it (§5.1).

The DRBG's K and V, the candidate, and k·G's y coordinate are zeroed before `sign` answers, best effort as `docs/hkdf.md` §3
says. `work`, which held d, k and the ladder's table, is zeroed by `std.ecdh` after the multiplication and by `sign` at the
end.

## 4. Keys

### 4.1 Where they are read, and why there

**In `packages/x509`, as `x509_key`**, not in `std` and not in `packages/tls`:
- the DER is read with `x509.tlv`, the strict reader the certificate parser uses, and the OIDs come from its table: a
  second DER reader in `std` would be a copy (`conformance/duplication.rs`);
- a key file is the same kind of thing as a certificate file, PEM around DER, and `matches_certificate` needs the
  certificate's parsed public key, which is `x509`'s view;
- `packages/tls` already imports `x509`; the engine will call `x509_key` from `tls.add_identity` (step 2). Like
  `x509_verify`, it imports `std` (`std.ecdh` for the point), and `docs/package-system.md` §4.8 already covers that.

### 4.2 The formats

| Label | Format | Accepted when |
|---|---|---|
| `PRIVATE KEY` | PKCS#8 PrivateKeyInfo (RFC 5208 §5), or RFC 5958's v2 with the public key | version 0 or 1; algorithm id-ecPublicKey with the named curve prime256v1 (RFC 5480); the private key an ECPrivateKey; `[0]` attributes and `[1]` publicKey optional |
| `EC PRIVATE KEY` | SEC 1 ECPrivateKey (RFC 5915) | version 1; the key 1 to 32 bytes; `[0]` the named curve prime256v1, required, as RFC 5915 §3 says; `[1]` the public key optional |

- **The first private-key block is used.** Blocks that are not a key (`EC PARAMETERS`, which `openssl ecparam -genkey`
  writes first, or a certificate) are skipped. The END line must name the BEGIN line's label.
- **A key shorter than 32 bytes is left-padded.** OpenSSL before 1.1.0 wrote a key whose top byte was zero one byte short;
  this accepts it rather than refusing one key in 256 from an old tool.
- **A public key in the file must be the key's own**: the point is derived with `public_point` (k·G on the ladder) and
  compared. PKCS#8 v2 can carry it twice, inside and outside; both must agree.
- **`matches_certificate(cert, point)`** parses the certificate and compares its public key with the point: the check
  `docs/tls-server.md` §4 puts in `add_identity` (`tls-server-key-mismatch`).

| Code | Tag | When |
|---|---|---|
| -80 | `key-pem` | no private-key block; no END line or a different label; a character outside base64; bad padding |
| -81 | `key-encrypted` | `ENCRYPTED PRIVATE KEY`, or a traditional block with `Proc-Type: 4,ENCRYPTED` |
| -82 | `key-algorithm` | not an EC key: `RSA PRIVATE KEY`, an RSA or Ed25519 PKCS#8 key |
| -83 | `key-curve` | an EC key on another curve, explicit parameters, or SEC 1 without its curve |
| -84 | `key-der` | not the DER structure the format names: truncated, trailing bytes, a wrong tag, an extra element |
| -85 | `key-version` | PKCS#8 version not 0 or 1; SEC 1 version not 1 |
| -86 | `key-length` | the private key empty or longer than 32 bytes |
| -87 | `key-range` | the key 0 or not below n |
| -88 | `key-public-mismatch` | the file's public key is not the key's |
| -89 | `key-size` | the block decodes to more than 8,192 bytes |
| -90 | `key-buffer-length` | the caller's `key` or `point` not 32 and 65 bytes, or `work` short |
| -91 | `key-certificate` | `matches_certificate`: the certificate does not parse, or its key is not P-256 |
| -92 | `key-certificate-mismatch` | the certificate's key is another P-256 key |

The TLS server maps them onto `docs/tls-server.md` §5.4's three: `key-encrypted`, `key-algorithm` and `key-curve` are
`tls-server-key-type`; `key-certificate-mismatch` is `tls-server-key-mismatch`; the rest are `tls-server-key-format`.

### 4.3 Reading a secret

The rest of `packages/x509` reads public data and has no constant-time rule. `x509_key` reads a private key, so:
- **base64 is decoded without a branch or an index on a key character's value.** Each character's six bits are computed
  from range masks (`within`, through `value_barrier`) the same way for every character; what branches is whether it is a
  base64 character at all, which for a key's body is the file's layout (its line breaks), not its content. That is the leak
  "Util::Lookup" (Sieck et al., CCS 2021) used to recover RSA keys from table-driven base64 decoding inside SGX enclaves.
- **The DER's tags and lengths are the format's**, the same for every P-256 key; the key's bytes are only copied.
- **The decoded DER is zeroed** before `parse_pem` answers, and the key on a refusal.

This runs once, when a program loads its key, from a file the operator gives it, and nothing a peer sends reaches it. It is
audited (§2.3) but not timing-tested.

## 5. How it is tested, and the results

`tests/programs/ecdsa_sign_driver.ls` runs everything, built with `packages/x509/x509.ls` and `key.ls`.
`crates/lex-sys/tests/conformance/ecdsa_sign.rs` has the fixed cases; the scripts run OpenSSL and Python.

### 5.1 RFC 6979 and the refusals (`conformance/ecdsa_sign.rs`)

- **RFC 6979 Appendix A.2.5, P-256 with SHA-256**, both messages ("sample" and "test"), with the added randomness empty: r
  and s exactly as the RFC prints them, from `sign` and from `sign_checked`, on **both backends**, and their DER ("test"'s s
  starts `01`, so its INTEGER is 32 bytes and the SEQUENCE 69). The other hashes of A.2.5 use HMAC with that hash, which this
  signer does not offer.
- Four values of added randomness give four signatures, none RFC 6979's deterministic one, each verifying; the same
  randomness twice gives the same signature.
- Every refusal of §1 the driver can reach, with its tag, and the key range's edges: 0 and n refused, n − 1 signs.

### 5.2 Against OpenSSL and against RFC 6979 in Python

On Linux x86-64 (`ubuntu:24.04`, OpenSSL 3.0.13), in Docker:

| `python3 scripts/ecdsa_sign_differential.py …` | Cases | Result |
|---|---|---|
| `openssl`: random keys (1, 2, n − 2 and n − 1 drawn often), random messages, randomness empty one time in ten; each DER signature verified by `openssl dgst -sha256 -verify` under a public key computed by the script, and by `std.ecdsa.verify_der`; then one bit flipped | 10,000 signatures | **all 10,000 accepted by both; all 10,000 flipped refused by both; 0 differences** |
| `reference`: the signature byte for byte against RFC 6979 written in Python (`hmac`, `hashlib`), with digests 0, n − 1, n and 2^256 − 1 drawn often so `bits2octets`' reduction is reached; raw and DER | 10,000 signatures | **0 differences**, raw and DER |
| `keys`: `openssl genpkey` keys (PKCS#8), the same converted by `openssl ec` (SEC 1), and `openssl ecparam -genkey` files (an `EC PARAMETERS` block first); each parsed to the key and point `openssl pkey -text` prints, signed with and verified by `openssl dgst -verify`, and matched against its `openssl req -x509` certificate and not the next key's | 100 keys (67 PKCS#8, 33 after `EC PARAMETERS`), 600 checks | **0 differences** |

**Why `reference` as well as `openssl`.** A signature that OpenSSL verifies shows that r and s are right for *some* k. It does
not show that k is RFC 6979's, that the added randomness reached the DRBG, or that the digest was reduced before it was fed
to it. Six of the mutants of §5.5 sign signatures OpenSSL accepts with a nonce that is not RFC 6979's (the DRBG's separator,
its key, its randomness, its digest unreduced, K taken for V, V not stepped). RFC 6979's own vectors kill four; the
randomness and the unreduced digest only the byte-for-byte comparison does.

### 5.3 Key files (`tests/vectors/ecdsa_sign/`)

`scripts/ecdsa_sign_keys.py` made 43 key files and 5 certificates with OpenSSL 3.6.4, then damaged them one rule at a time.
Every one answers its tag, or the key and point OpenSSL printed for it: 7 accepted (PKCS#8, SEC 1, CRLF line ends, PKCS#8 v2
with its public key, PKCS#8 with attributes, after an `EC PARAMETERS` block, a 31-byte key), and 36 refused, which reach
every tag of §4.2 a file can reach. `matches_certificate` matches its key's certificate and refuses another P-256 key's, a
P-384 one, an RSA one and a broken one.

### 5.4 The audit

§2.3.

### 5.5 Mutants

`python3 scripts/ecdsa_sign_mutants.py target/release/lex-sys` runs **37 mutants**: 17 in `std/ecdsa_sign.ls`, 2 in
`bigmod.load_secret`, 1 in `ecdh.scalar_ok` and 17 in `packages/x509/key.ls`, each against §5.1 and §5.3's cases and the
three differentials of §5.2 at 300, 100 and 6. **37 are killed**, on macOS (OpenSSL 3.6.4) and in the Linux container (OpenSSL 3.0.13).

**Two first survived**, both in the PEM reader, both test gaps:
- **a stray `=` inside the base64 accepted.** The only bad-padding file had its `=` at the end, which the final count catches
  anyway. Now `padding-inside`: a SEC 1 key's closing `==` moved into its first line.
- **the END line's label not compared.** The only file with a wrong END label had one of another length, which the check of
  the closing `-----` caught. Now `end-label-same-length` (`END PRIVATE KEX`).

`std.ecdh`'s ladder and `std.bigmod`'s arithmetic are not mutated here: `scripts/ecdh_mutants.py` covers them, and nothing
in them changed.

## 6. Timing

`scripts/ecdsa_sign_timing.py`, `scripts/ecdh_timing.py`'s dudect test over `ecdsa_sign.sign` (`tests/programs/
ecdsa_sign_timing.ls`): one message and one 32-byte added randomness for every call, the key drawn from one of two classes at
random. A fixed key then gives the same nonce every time and a random key a random nonce, so the whole secret input is fixed
against random.

- **a fixed random key against random keys;**
- **the key 1 against random keys**, the shape that found `std.ecdh`'s leak (`docs/ecdh.md` §3). Here it gives r·d with a
  one-limb d, which a multiplication whose time depended on its operands would show.

**The machine:** MACHINE. **The gate:** |t| below 4.5 at 10^6 measurements a test (`docs/tls-server.md` §3.4).

| Test, LLVM backend | Measurements | Median | max \|t\| |
|---|---|---|---|
RESULT_TIMING

## 7. Cost

`python3 scripts/ecdsa_sign_bench.py <driver>`: 2,000 signatures with random keys and digests, best of three runs, minus a
run of as many DER encodings for the driver's own reading and printing. LLVM backend, one core:

| Machine | `sign` | `sign_checked` | the check |
|---|---|---|---|
RESULT_COST

Almost all of it is the k·G ladder (`std.ecdh.shared` on the Xeon of `docs/chacha20.md` §6 was 2.6 ms): the HMACs, the
inversion and the two `bigmod.setup`s are the rest. OpenSSL's `openssl speed ecdsap256` on the same machine signs in
RESULT_OPENSSL_SPEED, with a fixed-base table and arithmetic specialised to P-256's prime (`docs/ecdsa.md` §5.4).
`docs/tls-server.md` §6 now uses these numbers.

## 8. Found

- **`bigmod.load_reg` cannot take a secret** (§2.2): `load` skips zero bytes and `compare_n` exits early. Nothing had
  loaded a secret into a register before; ECDH never does. Hence `load_secret`.
- **`std.ecdsa` cannot hold the signer**, against the design's wording (§1): the import would be a cycle.
- **Two test gaps in the PEM reader** (§5.5).
- **The design's branch had conflict markers in `docs/README.md`**, from a merge of `main` into it. Resolved here, keeping
  every row.

## 9. Not done

- **A fixed-base table for k·G** and **arithmetic specialised to P-256's prime**: the two speed-ups `docs/tls-server.md` §8
  step 7 names. The ladder is the variable-base one with G as its point.
- **P-384, Ed25519 and RSA-PSS signing**: `docs/tls-server.md` §8 step 7, each with its own timing test.
- **A timing test of the key parser**: §4.3 says why it is audited only.
- **Zeroing that a test could see.** The wipes of §3 are best effort and no test reads `work` after `sign`; a mutant that
  removes one survives by construction, so none is listed.
- **The independent review** (#209).
