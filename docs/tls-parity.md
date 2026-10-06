# TLS parity with the OpenSSL backend: AES-GCM, P-256/P-384 key exchange, TLS 1.2

> **Status: built (#207, all five PRs): AES-GCM (§3.1.1), P-256 and P-384 key exchange (`docs/ecdh.md`), TLS 1.3's AES-GCM suites and HelloRetryRequest (§3.3.1), TLS 1.2 (§3.4.1).** Sub-issue 10 of the self-contained TLS 1.3 client (#197). The issue asked for a
> measured number of receivers needing TLS 1.2 or AES-GCM before anything is built. That number cannot be measured here, and
> the requirement replaces it: **the maintainer's requirement is that the pure client be equivalent to the OpenSSL backend**
> it is to replace in `cancho-hooks` (#210). The maintainer chose the scope below, "AEAD parity", over full parity with
> OpenSSL's defaults and over the issue as written. This document settles what that means and how it is built and tested.
> It supersedes `docs/tls-pure.md` §3.2 to §3.4, which are marked so.

---

## 1. The decision, and the number the issue asked for

**The number.** The issue's gate is "receivers needing 1.2 or AES-GCM, out of how many". For `cancho-hooks` that means its
operators' receivers. Neither they nor the first backend's negotiation logs are reachable from this repository. Nor can public
servers be surveyed from here: this container's outbound HTTPS goes through an intercepting proxy (`docs/x509-verify.md` §6.3),
which would show its own TLS, not the server's. So **no number is given, and none is claimed.**

**The requirement that replaces it.** The pure client must reach what the OpenSSL backend reaches, short of its weakest
options. The maintainer chose among three scopes:

| Scope | Adds | Chosen |
|---|---|---|
| the issue as written | TLS 1.2 with ECDHE and ChaCha20-Poly1305 only | |
| **AEAD parity** | TLS 1.2 ECDHE with AES-GCM and ChaCha20-Poly1305; TLS 1.3's AES-GCM suites; P-256 and P-384 key exchange | **yes** |
| full default parity | also CBC, static-RSA key exchange and finite-field DHE | |

## 2. What the OpenSSL backend offers (measured)

`docs/tls-nonblocking.md` §3.4 configures OpenSSL with a minimum of TLS 1.2 and OpenSSL's default cipher list. The ClientHello
of `openssl s_client` (OpenSSL 3.0.13), captured here, offers:
- **31 cipher suites:**
  - the 3 TLS 1.3 suites (`1302`, `1303`, `1301`);
  - 27 TLS 1.2 suites: ECDHE and DHE with AES-GCM and ChaCha20, CBC with SHA-1 and SHA-2, and static RSA;
  - the renegotiation SCSV (`00ff`).
- **groups:** X25519, P-256, X448, P-521, P-384 and FFDHE 2048 to 8192;
- **one key share:** X25519;
- **20 signature schemes**, including SHA-1 and SHA-224 ones for TLS 1.2;
- **extended master secret** offered (RFC 7627), and a session ticket extension.

**What AEAD parity offers, and what it does not:**

| | offered | not offered, and refused with a tag if a server needs it |
|---|---|---|
| TLS 1.3 suites | all three: `TLS_AES_256_GCM_SHA384`, `TLS_CHACHA20_POLY1305_SHA256`, `TLS_AES_128_GCM_SHA256` | — |
| TLS 1.2 suites | `ECDHE-{ECDSA,RSA}-AES256-GCM-SHA384`, `ECDHE-{ECDSA,RSA}-CHACHA20-POLY1305`, `ECDHE-{ECDSA,RSA}-AES128-GCM-SHA256` | the other 21: DHE, CBC, static RSA (`tls-no-shared-cipher`) |
| groups | X25519, P-256, P-384, one X25519 share, and HelloRetryRequest to P-256 or P-384 | X448, P-521, FFDHE (`tls-key-share`) |
| TLS 1.2 signatures | `rsa_pkcs1_sha256/384/512`, `rsa_pss_rsae_sha256/384/512`, `ecdsa_secp256r1_sha256`, `ecdsa_secp384r1_sha384`, `ed25519` | SHA-1 and SHA-224 schemes (`tls-bad-certificate-verify` if a server uses one) |
| renegotiation | the SCSV, so a server knows; a server's HelloRequest is refused | renegotiation |
| extended master secret | **required** (the issue's rule) | a TLS 1.2 server without it (`tls-protocol-version`; *corrected in PR 5: `tls-extended-master-secret`, a tag of its own, as the next paragraph says*) |
| resumption | none, as now | session tickets are not sent or used |

**One deliberate difference from OpenSSL beyond the suites.** OpenSSL offers the extended master secret but does not require
it. The issue requires it, because without it a TLS 1.2 session can be synchronised across two connections (the triple
handshake attack). It stays required. The interop gate (§5) shows which servers that refuses, and a server refused for it
gets a tag of its own.

## 3. What has to be built

### 3.1 AES-GCM, constant-time (PR 2)

`docs/tls-pure.md` §3.2 kept AES out because table-based AES leaks the key through cache timing. AEAD parity needs it, so it is
built without tables:
- **AES-128 and AES-256** (FIPS 197), **bitsliced**: the S-box as a fixed boolean circuit over machine words (Boyar–Peralta's
  113 gates), with no table lookup and no branch or index that depends on the key or data. The key schedule is bitsliced too.
- **GHASH** with a constant-time carry-less multiply, with no table indexed by the key-derived `H`. *Corrected in PR 2:* this
  said 64-bit operands, as BearSSL's `ghash_ctmul64` does. That needs the high half of a 64×64-bit product, and cancho's
  `int` is a checked 64-bit signed integer, so a product over 2^63 traps. It is built as BearSSL's `ghash_ctmul32` instead:
  32-bit operands split into masked quarters, every product of two quarters under 2^63 (§3.1.1).
- **GCM** (NIST SP 800-38D) with a 96-bit nonce only, which is all TLS uses.
- **In `std`**, beside `std.chacha20`: `std.aes` and `std.gcm`, each with a refusal tag for every rejected input.

**Gates:**
- NIST CAVP `gcmEncryptExtIV` and `gcmDecrypt` for 128- and 256-bit keys, filtered to 96-bit IVs;
- Wycheproof `aes_gcm_test.json`;
- 10,000 random cases against pyca/cryptography;
- a dudect-style timing test, a Welch t-test over fixed versus random keys and inputs, on both backends, reported with its t;
- at least 12 mutants killed;
- the cost per byte, against ChaCha20-Poly1305's 135 MB/s (`docs/chacha20.md` §6).

A bitsliced AES may be several times slower than ChaCha20 here. The cost is measured and written, not assumed.

#### 3.1.1 Results (PR 2)

**Built.** `std/aes.cho` is a port of BearSSL's `aes_ct` and `std/gcm.cho` of its `ghash_ctmul32` (Thomas Pornin, MIT licence,
quoted in the files):
- **Two blocks per pass.** They are encrypted as eight 32-bit words with the blocks' bits regrouped (`ortho`), and the S-box
  is the 113-gate circuit run on all eight words.
- **The key schedule runs that S-box on one word at a time.** It is built in place in the caller's `skey`.
- **Counter mode encrypts two counter blocks per pass.**
- **GHASH uses Karatsuba.** Each 128×128-bit product is nine 32×32-bit carry-less products, done on the words and on their bit
  reversals.

**What each module answers, and refuses:**
- `std.gcm.seal` and `std.gcm.open` take and answer what `std.chacha20`'s do: the tag follows the ciphertext, and `open`
  writes nothing on a mismatch.
- A 192-bit key, or a nonce that is not 96 bits, is refused (`gcm-key-length`, `gcm-nonce-length`). TLS uses neither.
- `std.aes` exposes the block and CTR functions with `_with` variants that take the caller's eight-word scratch.

**Every call allocates once.** The first version gave each helper its own `region`. Nested three deep, they made glibc's
`free` trim the heap and the next `malloc` regrow it: 8 `brk` system calls per `seal` (`strace -c`), and 23 µs for a 64-byte
message. After the change, one region holds all of a call's scratch:
- no `brk` per call;
- 5.0 µs for the same 64-byte message.

**Correctness, every case through `tests/programs/gcm_driver.cho`** (`crates/cancho/tests/conformance/gcm.rs`):

| Evidence | Cases | Result |
|---|---|---|
| FIPS 197 C.1 and C.3, one block, both backends | 2 | equal |
| NIST CAVP `gcmEncryptExtIV` and `gcmDecrypt`, 128- and 256-bit keys, 96-bit IVs and 128-bit tags (`tests/vectors/cavp/README.md`), both backends | 1,500, 387 of them `FAIL` | all equal; every `FAIL` refused without writing |
| Wycheproof `aes_gcm_test.json` | 316 | 79 valid cases sealed and opened; 54 bad tags refused; 103 with a 192-bit key and 80 with another nonce size refused with their tags |
| each bit of a 67-byte sealed message and its 20 bytes of associated data flipped | 696 | all refused, none writes |
| every reachable refusal | 11 | each with its own tag |
| `scripts/gcm_differential.py`, random cases against pyca/cryptography (OpenSSL 4.0.1) | 10,000 cases, 30,000 checks | 0 differences |

**Mutants:** `scripts/gcm_mutants.py` runs 24 mutants across both files, and **24 are killed**. Each is one bug planted in
the S-box, ShiftRows, MixColumns, the key schedule, `ortho`, the counter, the carry-less multiply, Karatsuba, the reduction,
the lengths block, the order of the hashed data, J0, the zero padding, the tag compare, or the nonce check.

**Timing.** There are two checks.

*Branches.* `scripts/chacha20_branches.py` lists every conditional jump in the 26 functions that touch the key, the keystream
or GHASH's state, on both backends' objects. Each jump either goes to a trap (a bounds or overflow check) or compares:
- a constant;
- a loop counter;
- the counter block;
- a length, as in GHASH's zero padding, where the register compared is `len(data)`.

No index depends on the key or the data either.

*Statistics.* `scripts/gcm_timing.py` runs a dudect-style Welch t-test (Reparaz et al., 2017) over
`tests/programs/gcm_timing.cho`:
- each call is timed with `rdtscp`, through a three-line C library linked with `-l tick`;
- the inputs are 64-byte messages with 13 bytes of associated data;
- |t| below 4.5 is no evidence of a leak.

| Test | LLVM, 10^6 samples | Cranelift, 10^6 samples |
|---|---|---|
| seal: fixed key against random key | 2.24 | 1.15 |
| seal: fixed message and associated data against random | 2.06 | 3.13 |
| open: fixed sealed message against random (both refused) | 2.98 | 2.31 |
| open: tag wrong in its first byte against its last | 2.31 | 2.38 |

*Corrected (#208, `docs/tls-assurance.md` §6.1): on the Xeon. On an Apple M4 Max at 20,000 samples, Cranelift's two data
tests fail (|t| up to 34.39), with equal instruction counts for both classes, and its open-data test still fails with Arm's
data-independent-timing bit set (5.62). LLVM passes there too.*

**The harness's first version found a false leak.** It decoded each case's hex just before timing it. On the Cranelift
backend, that gave t = 31 and 43 for the data tests, and t = 174 for a loop of plain XORs timed the same way. The cause was
the decoding, not the code under test: its branch on each hex digit is always predicted for an all-zero input and
mispredicts for a random one, and the state that leaves behind reaches the next timed call. The harness now decodes every
case before it times any.

**Cost.** `tests/programs/gcm_bench.cho` seals one message repeatedly, as `aead_bench.cho` does for ChaCha20 (median of 5,
one core of the same Xeon at 2.80 GHz). ChaCha20-Poly1305 was re-measured in the same session at 129 MB/s for 16 KiB,
against `docs/chacha20.md` §6's 135 MB/s.

| Message | AES-128-GCM, LLVM | AES-256-GCM, LLVM | AES-128-GCM, Cranelift | ChaCha20-Poly1305, LLVM | `openssl speed -evp aes-128-gcm` |
|---|---|---|---|---|---|
| 16,384 bytes | **32.6 MB/s** (502 µs) | 27.3 MB/s (600 µs) | 10.4 MB/s (1,569 µs) | 129 MB/s (127 µs) | 5,546 MB/s |
| 1,024 bytes | 30.9 MB/s (33 µs) | 23.5 MB/s (44 µs) | 9.2 MB/s (111 µs) | 124 MB/s (8.3 µs) | 3,984 MB/s |
| 64 bytes | 12.8 MB/s (5.0 µs) | 9.4 MB/s (6.8 µs) | 4.1 MB/s (16 µs) | 61 MB/s (1.0 µs) | 1,356 MB/s |

**What the costs mean:**
- **AES-GCM costs about 4 times ChaCha20-Poly1305 here.** A full 16 KiB TLS record takes half a millisecond to seal. A server
  that picks an AES-GCM suite costs that much per record.
- **OpenSSL is about 170 times faster.** It uses the AES-NI and PCLMULQDQ instructions, which cancho cannot emit.
  *Corrected (`docs/crypto-builtins.md`, step 4): cancho now emits them, on LLVM, where the CPU has them. AES-128-GCM seal,
  measured with `cancho`'s hardware path against OpenSSL on the same machine: on aarch64 Linux (the Apple M4's VM, OpenSSL
  3.0.13) 557 against 4,007 MB/s at 64 bytes and 1,110 against 8,035 MB/s at 16 KiB, **7.2 times at both**; on the M4 under
  macOS (OpenSSL 3.6.4) 99 against 611 MB/s and 1,050 against 10,497 MB/s, 6.2 and 10 times. Not measured on this section's
  Xeon. The software path, still what Cranelift and a CPU without the instructions run, is the table above.*
- **Most of the cost is instructions.** `valgrind --tool=callgrind` puts GHASH at 28% of a 16 KiB seal and the AES rounds at
  most of the rest. GHASH keeps its 74 working words in a bounds-checked array. Holding them in locals is the first place to
  look if the cost matters.
- **The key is expanded, and H computed, on every call.** In a 64-byte seal they are 25% and 14% of the instructions
  (callgrind, inclusive). Both depend only on the key, so PR 4 can keep them per connection if the record layer needs that.

### 3.2 P-256 and P-384 key exchange, constant-time (PR 3)

*Built: `docs/ecdh.md` has the results. Building it found the LLVM backend turning constant-time masks into branches, and
added `value_barrier` to the language (`docs/value-barrier.md`).*

`std.ecdsa` verifies with public data, and is variable-time. Key exchange multiplies by a **secret** scalar. `std.ecdh` will:
- use `std.bigmod`'s registers (`docs/ecdsa.md` §2), with field operations that do not branch on their values;
- multiply with a fixed-window ladder over complete addition formulas (Renes–Costello–Batina), so the same operations run
  whatever the scalar;
- refuse a point not on the curve, or the point at infinity, before using it.

`std.bigmod`'s reduction is checked for data-dependent branches, and any it has is replaced. `docs/rsa.md` and `docs/ecdsa.md`
document `bigmod` as variable-time, because it was only ever given public data.

**Gates:**
- Wycheproof `ecdh_secp256r1_test.json` and `ecdh_secp384r1_test.json`, including the invalid-point cases;
- NIST CAVP `KAS_ECC_CDH_PrimitiveTest`;
- the same timing test;
- at least 12 mutants killed.

### 3.3 TLS 1.3: AES-GCM and HelloRetryRequest (PR 4)

- **The record layer** takes its AEAD from the suite. A suite's hash, SHA-256 or SHA-384, runs the key schedule:
  `std.hkdf` already takes 48 (`docs/hkdf.md`).
- **The transcript** keeps both a SHA-256 and a SHA-384 state until ServerHello names the suite, as RFC 8446 §4.4.1 permits.
- **HelloRetryRequest** is accepted once, for P-256 or P-384:
  - the transcript is replaced by `message_hash` (RFC 8446 §4.4.1);
  - the cookie is echoed, and the second ClientHello carries the new share;
  - a second HRR, an HRR for a group not offered, or for the group already sent, is refused.
- **ServerHello's key share** may now be P-256 or P-384, as the HRR asked.

**Gates:**
- the existing tlslite-ng traces and lying server replay unchanged; *(corrected in PR 4: they cannot. The ClientHello now offers
  three suites and three groups, so every recorded byte the client sends changed. They were re-recorded; §3.3.1)*
- new traces recorded for each suite, and for an HRR to each group;
- the lying server gains HRR cases: twice, an unoffered group, the group already sent, a changed suite;
- live against `openssl s_server -ciphersuites` each suite and `-groups P-256` (forcing the HRR), Python `ssl` and tlslite-ng;
- RFC 8448's key schedule rows extended to the AES-128-GCM record keys of its trace.

#### 3.3.1 Results (PR 4)

**Built:**
- **`packages/tls/record.cho`** knows the three suites, their hash and key lengths, and seals and opens under the suite's AEAD:
  `std.chacha20` or `std.gcm`.
- **`message.cho`** offers the suites in OpenSSL's order (`1302`, `1303`, `1301`) and the groups X25519, P-256 and P-384, with one
  X25519 share. Its ServerHello parser reads a HelloRetryRequest too: the group it names, and its cookie.
- **`client.cho`** keeps both transcripts until a suite is named. It sizes every secret, the Finished MAC and the
  CertificateVerify content by the suite's hash.
- **On a HelloRetryRequest, `client.cho`:**
  - restarts the transcript from `message_hash`;
  - sends a second ClientHello with a P-256 or P-384 share, echoing the cookie;
  - checks the second ServerHello against the retry: the same suite, a share of the group asked for, no second retry.
- **A server's `change_cipher_spec`** is accepted after a HelloRetryRequest, as OpenSSL sends one there. There is still only
  one per connection.

**Decisions made here:**
- **The P-256 or P-384 scalar is drawn from the X25519 secret,** as HKDF-Expand-Label(secret, "ecdh scalar", [attempt]).
  It is redrawn while it is not below n, which fails once in 2^32 for P-256, up to sixteen times. Once a retry has come, that
  X25519 secret is never used for anything else. So `start` still takes 96 bytes of entropy, and every caller and recording is
  unchanged.
- **A cookie is limited to 2,048 bytes** (RFC 8446 allows 65,535). A longer one is `tls-hello-retry`, so the ClientHello's buffer is
  2,688 bytes and no slot grows by 64 KB for a cookie no server sends.
- **The work for `std.ecdh` (77 KB) is part of the slot's `ints`,** because a region holds at most 64 KiB (`docs/ecdh.md` §1).
- **`tls-hello-retry` now sends `illegal_parameter` (47),** not `handshake_failure` (40). It now means a retry the client
  cannot follow, which RFC 8446 §4.1.4 answers with that alert.

**Evidence:**
- **Five tlslite-ng traces, replayed byte for byte on both backends** (`conformance/tls.rs`). Every one was re-recorded,
  because the ClientHello changed:
  - RSA and P-256 certificates under ChaCha20 with X25519, as before;
  - AES-256-GCM-SHA384 with X25519;
  - AES-128-GCM with a HelloRetryRequest to P-256;
  - AES-256-GCM with a HelloRetryRequest to P-384.

  The one-byte and coalesced splits run over all five.
- **The lying server** (`scripts/tls_liar.py`) gained P-256 and P-384, AES-GCM and SHA-384, and a retry step. It has **41
  cases**, up from 29: 14 new ones, replacing the two that refused AES-128-GCM and any HelloRetryRequest. Every case
  ends with its tag and its alert:

  | Server does | Tag | Alert |
  |---|---|---|
  | AES-256-GCM-SHA384; a retry to P-256 with a cookie and a `change_cipher_spec`, under AES-128-GCM; a retry to P-384 under AES-256-GCM, each through the honest case's whole connection (a 2^14 + 256 record, two KeyUpdates) | `ok` | none |
  | TLS_AES_128_CCM_SHA256; a retry with it | `tls-no-shared-cipher` | 40 |
  | a P-256 share never sent; a retry for X25519, whose share was sent; a retry for X448; after a retry to P-256, an X25519 share, or a point not on the curve | `tls-key-share` | 47 |
  | a retry that changes nothing; a cookie over 2,048 bytes; after a retry, a ServerHello with another suite | `tls-hello-retry` | 47 |
  | a second HelloRetryRequest | `tls-unexpected-message` | 10 |

  The 64 streams `tls_many` replays were re-recorded too.
- **The ServerHello rules test** (`conformance/tls.rs`) builds a HelloRetryRequest for P-256, one with a share in it, one for
  X25519, and one that changes nothing. AES-128-GCM and AES-256-GCM are now accepted, and TLS_AES_128_CCM_SHA256 is refused.
- **RFC 8448's AES-128-GCM record.** §3's protected server flight (EncryptedExtensions to Finished, 674 bytes) opens with
  `tls_record.open` under `1301`, with RFC 8448's handshake key and IV, to the RFC's plaintext. With one bit flipped, or under
  ChaCha20, it is refused (`tests/vectors/tls/rfc8448_record.txt`, from s2n-tls's transcription, as `kdf.txt`'s rows are).
  That is the RFC 8448 gate this section set.
- **`scripts/tls_record_differential.py`** now picks one of the three suites per record. Of 2,000 records sealed and 2,000
  opened against pyca, **0 differ**.
- **Live** (`scripts/tls_live.py`), every connection `ok`:
  - **`openssl s_server` (OpenSSL 3.0.13)** with each of the three suites against each of X25519, P-256 and P-384, 16
    connections each. P-256 and P-384 make it answer the X25519 share with a HelloRetryRequest.
  - **Python `ssl`** with the five certificate types, 64 connections at both read sizes. The client now lists AES-256-GCM first,
    as OpenSSL does, so these connections run AES-256-GCM-SHA384, not ChaCha20 as before.
  - **`openssl s_server` and tlslite-ng** with their earlier settings, 64 connections at both read sizes.
  - **The refusals** (a missing close_notify, another host, another CA's root) end with the same tags as before.
- **Mutants:** `scripts/tls_mutants.py` has **38, all killed**.
  - 15 are new: the SHA-384 transcript, each suite's hash, key length and AEAD, the Finished MAC's hash, `message_hash`, the
    transcript restart, the cookie, a second retry, the retry's suite and group checks, a retry that changes nothing, the
    `change_cipher_spec` after a retry, and the retry share's curve.
  - **One was dropped as equivalent: the ServerHello's group compared with the share sent.** A share of another group always
    has the wrong length for the key the client holds, so the key exchange refuses it with the same tag. The comparison is
    defence in depth.

**RFC 8448's trace is still not replayable byte for byte**, AES-GCM or not. Its ClientHello has an empty session id and offers
groups and suites this client does not, so the server's flight answers a transcript this client never produces. #205's
decision on that gate stands.

### 3.4 TLS 1.2 (PR 5)

A second handshake in `tls_client`, sharing the record layer, the certificate path (`docs/x509-verify.md`), the slot and the
engine:
- **ClientHello** offers 1.3 and 1.2 in `supported_versions` and `legacy_version` 1.2. It carries the six suites of §2,
  `extended_master_secret`, the renegotiation SCSV, `ec_point_formats` (uncompressed), and the groups and signatures of §2.
- **The downgrade sentinels:**
  - a TLS 1.2 ServerHello whose random ends `DOWNGRD\x01` is refused, since this client offered 1.3 (RFC 8446 §4.1.3);
  - one ending `DOWNGRD\x00` is refused too.
  - *#208: the code refused them in a TLS 1.3 ServerHello as well, which this never said; corrected to match it (`docs/tls-assurance.md` §4).*
- **The flight:**
  - ServerHello, whose extensions may be `renegotiation_info` (empty), `extended_master_secret` and `ec_point_formats`.
    *Corrected (#208, `docs/tls-assurance.md` §5): and `server_name`, empty, which RFC 6066 §3 has a server send when
    it used the name. This list left it out, and the client refused every TLS 1.2 handshake with nginx
    (`tls-unsupported-extension`); the interop matrix found it;*
  - Certificate (verified as in 1.3);
  - ServerKeyExchange, whose ECDHE parameters are signed with the leaf's key over both randoms;
  - ServerHelloDone;
  - then the client's ClientKeyExchange, ChangeCipherSpec and Finished, and the server's ChangeCipherSpec and Finished.
- **The keys:**
  - the PRF of RFC 5246 §5 over the suite's hash;
  - the extended master secret over the session hash (RFC 7627 §4);
  - the key block;
  - `verify_data` of 12 bytes.
- **The nonces:**
  - ChaCha20-Poly1305 per RFC 7905 §2: the sequence number XORed into the 12-byte IV, as in 1.3;
  - AES-GCM per RFC 5288 §3: a 4-byte implicit salt and the 8-byte sequence number as the explicit nonce.
- **A CertificateRequest** is answered with an empty Certificate, as in 1.3: no client certificates.
- **Refused:**
  - a HelloRequest (no renegotiation);
  - a server without extended master secret;
  - a NewSessionTicket the client did not ask for;
  - compression;
  - a record that is not an AEAD record after ChangeCipherSpec.

**Gates.** The issue names "RFC 7905 vectors". **RFC 7905 has no test vectors**: it defines the suites and the nonce, and
nothing to replay. The gates are:
- the PRF against RFC 5246's reference computation in pyca/cryptography and OpenSSL's `kdf -kdfopt` TLS1-PRF, 1,000 cases each;
- the extended master secret against the same;
- traces recorded against `openssl s_server -tls1_2 -cipher` each of the six suites, replayed byte for byte on both backends;
- live, 64 connections at once:
  - `openssl s_server -tls1_2` for each suite;
  - Python `ssl` with `maximum_version` TLS 1.2;
  - tlslite-ng with TLS 1.2;
- the lying server's 1.2 cases:
  - each sentinel;
  - ServerKeyExchange signed by another key, and over the wrong randoms;
  - no extended master secret;
  - a wrong Finished;
  - a HelloRequest;
  - CBC and static-RSA suites chosen;
  - an unoffered group;
  - the explicit nonce reused; *(dropped in PR 5: a receiver reads the explicit nonce from each record and cannot tell a reused
    one from a fresh one without keeping every nonce it has seen. RFC 5288 §3 puts uniqueness on the sender, and this client's
    nonces are its sequence numbers. Nothing for a lying server to test.)*
- the mutant and fuzz bars of #205: at least 15 mutants killed, and the driver fuzzed with no trap.

#### 3.4.1 Results (PR 5)

**Built:**
- **`packages/tls/slot.cho` (`tls_slot`) is new.** It holds what both handshakes share: the states, the slot's layout, the
  transcript, the record queue, alerts, the ECDH share, and the chain and signature checks. It was moved out of `client.cho`,
  which would otherwise have passed 2,000 lines.
- **`packages/tls/client12.cho` (`tls_client12`) is new: the TLS 1.2 handshake.**
- **`record.cho`** gained:
  - the six suites;
  - TLS 1.2 records: the additional data carries the sequence number. AES-GCM's 8-byte explicit nonce is sent in each
    record and is the sequence number, and ChaCha20's nonce is the IV XORed with the sequence number, as in 1.3;
  - the PRF.
- **`message.cho`:**
  - the ClientHello offers TLS 1.2 beside 1.3, with the six suites, the renegotiation SCSV, `ec_point_formats`,
    `extended_master_secret`, and `rsa_pkcs1_*` among the signature algorithms;
  - a ServerHello with no `supported_versions` is read as TLS 1.2;
  - new parsers for TLS 1.2's Certificate, ServerKeyExchange and CertificateRequest.
- **A TLS 1.2 ServerKeyExchange may use RSASSA-PKCS1-v1_5,** and its ECDSA schemes name the hash, not the curve
  (RFC 5246 §7.4.1.4.1). The leaf's key must be the suite's kind: ECDSA or Ed25519 for ECDHE_ECDSA, RSA for ECDHE_RSA.
- **Two refusal tags are new:**
  - `tls-extended-master-secret` (alert 40);
  - `tls-renegotiation`, for a HelloRequest (alert 100).
- **The TLS 1.2 ECDHE scalar** is drawn as a HelloRetryRequest's is (§3.3.1).

**Evidence:**
- **Six traces against `openssl s_server -tls1_2 -www` (OpenSSL 3.0.13),** one for each suite, recorded by
  `scripts/tls_trace.py --openssl12`. They replay byte for byte on both backends, and in one-byte and coalesced splits.
- **The lying server has a TLS 1.2 server** (`scripts/tls_liar.py`, `Server12`) with **22 cases**, so 63 in all:

  | Server does | Tag | Alert |
  |---|---|---|
  | ECDHE-ECDSA with ChaCha20-Poly1305; with AES-256-GCM-SHA384; with a CertificateRequest (answered with an empty Certificate) under AES-128-GCM; with P-256 | `ok` | none |
  | either downgrade sentinel; TLS 1.2 after a HelloRetryRequest | `tls-protocol-version` | 70 |
  | no extended master secret | `tls-extended-master-secret` | 40 |
  | a CBC suite; a static-RSA suite | `tls-no-shared-cipher` | 40 |
  | a ServerHello echoing the client's session id; a renegotiated connection in `renegotiation_info` | `tls-decode-error` | 50 |
  | a key_share in a TLS 1.2 ServerHello | `tls-unsupported-extension` | 110 |
  | the key exchange signed by another key, or over the randoms in the wrong order; an RSA suite with an Ed25519 key | `tls-bad-certificate-verify` | 51 |
  | a key exchange on X448 | `tls-key-share` | 47 |
  | ServerHelloDone before ServerKeyExchange; a NewSessionTicket, never asked for | `tls-unexpected-message` | 10 |
  | a wrong Finished | `tls-bad-finished` | 51, encrypted |
  | a plaintext Finished after change_cipher_spec | `tls-bad-record-mac` | 20, encrypted |
  | a HelloRequest | `tls-renegotiation` | 100, encrypted |

- **The PRF and the extended master secret.** `scripts/tls12_prf_differential.py` runs 1,000 cases, each checked against
  both OpenSSL's TLS1-PRF (`openssl kdf`) and RFC 5246's P_hash on Python's `hmac`. They include the extended master secret
  (48 bytes over a session hash), the key block, and both Finished labels. There are **0 differences**, and 40 of the cases
  are `tests/vectors/tls/prf12.txt`, run on both backends.
- **Records.** `seal12` and `open12` match records built on pyca's AES-GCM and ChaCha20-Poly1305.
- **Live** (`scripts/tls_live.py`), every connection `ok`:
  - **`openssl s_server -tls1_2`** with each of the six suites, and with `-sigalgs RSA+SHA256` (a PKCS#1 v1.5 key exchange
    signature). Each suite was also run by hand against X25519, P-256 and P-384: all 18 pass.
  - **Python `ssl` capped at TLS 1.2,** with P-256, P-384, RSA-2048 and Ed25519 certificates.
  - **tlslite-ng with TLS 1.2.**
- **A fuzzer.** `scripts/tls_fuzz.py` mutates the server's bytes in the eleven recorded handshakes: bits flipped, bytes set,
  the data cut, slices duplicated or dropped, bytes inserted, lengths changed. Over 20,000 connections the driver **never
  trapped**. This is the "driver fuzzed with no trap" bar. The fuzzing over a million inputs is still #208's
  (`docs/tls-pure.md` §9).
- **Mutants:** `scripts/tls_mutants.py` has **59, all killed**. 22 are TLS 1.2's:
  - the PRF's chaining and label;
  - the extended master secret's label;
  - the key block's order, and which key is whose;
  - the explicit nonce sent, and the explicit nonce read;
  - the additional data's sequence number and length;
  - ChaCha20's 1.2 nonce;
  - the extended master secret required;
  - the session id echo;
  - a key_share in a 1.2 ServerHello;
  - the signed randoms;
  - the suite's kind of key;
  - both of Finished's checks;
  - HelloRequest;
  - the ClientKeyExchange's length;
  - TLS 1.2 after a retry;
  - the server's point kept.

**Corrected in place:**
- **§2's table gave the missing extended master secret `tls-protocol-version`.** The text after it promised a tag of its own,
  which is what was built.
- **§3.4's "the explicit nonce reused" case** is something a client cannot detect; the note there says why.

## 4. Files

| File | PR | What |
|---|---|---|
| `std/aes.cho`, `std/gcm.cho` | 2 | bitsliced AES-128/256; GHASH and GCM |
| `std/ecdh.cho`, and `std/bigmod.cho` where it branches on data | 3 | P-256 and P-384 key exchange |
| `packages/tls/record.cho` | 4, 5 | the AEAD chosen by the suite; TLS 1.2's AES-GCM nonce |
| `packages/tls/message.cho` | 4, 5 | the new ClientHello; HRR; TLS 1.2's messages |
| `packages/tls/client.cho`, a new `packages/tls/client12.cho` | 4, 5 | the 1.3 changes; the 1.2 handshake in its own file, under 2,000 lines |

## 5. What stays different from OpenSSL, after all five PRs

- **Suites:** the 21 TLS 1.2 suites of §2 that are not ECDHE with an AEAD.
- **Groups:** X448, P-521 and FFDHE.
- **The extended master secret** is required (§2).
- **SHA-1 and SHA-224 signatures** are refused.
- **No resumption:** the OpenSSL backend can resume sessions, which halves its handshake CPU (`docs/tls-nonblocking.md` §8.5,
  §10.4); this client does a full handshake every time. That costs time, not reach.

Each is refused with its own tag, so an operator can see which one a receiver needs. #210's comparison against OpenSSL
reports how many receivers each one costs.
