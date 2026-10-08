# TLS parity with the OpenSSL backend: AES-GCM, P-256/P-384 key exchange, TLS 1.2

> **§6 (client certificates, ALPN, and the revocation and ML-KEM decisions, #386): design, then built in the same PR; see §6.**
>
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

## 6. What the client did not do (#386): client certificates, ALPN, and two decisions

> **Status: design, before the code (#386).** The issue lists four gaps. A gateway needs two of them now, **client certificates**
> (mutual TLS to an upstream) and **ALPN** (naming the protocol the connection will carry), and both are designed here and
> built in the same PR. The other two, **revocation** and the **post-quantum hybrid key share**, are decisions: §6.8 and
> §6.9 give the evidence and a proposed answer, for a person to accept. Nothing in §6.1 to §6.7 is measured yet; the build
> fills in "as built" results, and corrects here, in place, whatever it finds false.

### 6.1 The four gaps, and what is decided

| Gap | Decision | Where |
|---|---|---|
| Client certificates | **build**: TLS 1.3 and TLS 1.2, P-256 keys, up to 4 identities chosen by the host name | §6.2 to §6.5 |
| ALPN | **build**: an offer list, the protocol chosen reported, a protocol not offered refused, TLS 1.3 and TLS 1.2 | §6.6 |
| Revocation (CRL, OCSP, stapled OCSP) | **decide**: proposed, do not build now | §6.8 |
| X25519MLKEM768 | **decide**: proposed, do not build now | §6.9 |

Refused on purpose, not deferred, and unchanged: 0-RTT, PSK-only resumption, TLS 1.2 resumption, post-handshake authentication
(`docs/tls-resumption.md` §3). A server that sends a `CertificateRequest` after the handshake is `tls-unexpected-message`, as now,
because `post_handshake_auth` is never offered.

### 6.2 Client certificates: the API

`docs/tls-pure.md` §7.2 left them out and answered every `CertificateRequest` with an empty `Certificate` (§7.1). The signer
they need is built (`std.ecdsa_sign.sign_checked`, `docs/ecdsa-sign.md`), and so is the key parser (`x509_key.parse_pem`) and the
shape of a held identity (`docs/tls-server.md` §4, `tls_identity`).

```
tls.open_mutual(heap, slots) -> Engine                  // a client engine that holds identities: open_mutual_with_tickets(heap, slots, tickets) too
tls.add_client_identity(engine, chain_pem, key_pem, hosts, now_unix_ms) -> id | refusal
tls.replace_client_identity(engine, id, chain_pem, key_pem, now_unix_ms) -> 0 | refusal      // keeps its hosts
tls.remove_client_identity(engine, id) -> 0 | refusal                                         // overwrites the key
tls.client_auth(engine, slot) -> 0 | 1 | 2                                                    // see §6.3
```

- **The identity is the engine's, chosen by the host.** A gateway talks to several upstreams, so one engine-wide identity is not
  enough, and a per-pool one (a pool is a ticket pool, `docs/tls-resumption.md` §12) names the wrong thing: a pool is a
  place to find a ticket, an identity is a thing you prove. `hosts` is a space-separated list, `*.example.com` standing for
  one label as a certificate name does (`x509_names.dns_matches`), or `*` alone for every host. `start` picks the first identity
  whose hosts match the name it is given, in the order they were added, and none otherwise. **An identity is never sent to a host
  it does not name**: an upstream that asks for a certificate it should not get (a public site with optional client
  authentication) learns nothing. `hosts` may not be empty (`tls-client-names`), so a forgotten argument is a refusal, not an identity that
  is silently never used.
- **What the engine holds**, per identity: the chain as `Certificate` sends it (each certificate's DER, up to 12 KiB in all: a message of the client's has to fit one record, and `tls_identity`'s 16 KiB for a server
  would not), the key (32 bytes), its public point (65), the leaf's `notAfter`, and the hosts (up to 512
  bytes). Four identities; the engine never gives a key back, and there is no function that answers one. **Memory:** an
  `open_mutual` engine adds 51,636 bytes for the four (12,909 each) and the key parser's work (`x509_key.work_len()` words, 77 KB);
  an engine opened with `open` adds nothing. **A slot holds none of it**: it holds which identity it chose, so the chain's
  12 KiB is not paid 64 times. The chain is read from the engine's store when the client's flight is built, which is one
  call of `feed`, so a `replace_client_identity` between two calls cannot give a connection a chain and a signature from two
  identities.
- **Overwritten**: `remove_client_identity` and `replace_client_identity` overwrite the old key and chain first, and
  `close` overwrites all four (best effort, `docs/tls-pure.md` §7.3). `tls.drop` of a slot is unchanged.
- **The key** comes from `x509_key.parse_pem`: an unencrypted PKCS#8 `PRIVATE KEY` or SEC 1 `EC PRIVATE KEY`, **P-256 only**, as the
  server's. It must be the leaf's key (`tls-client-key-mismatch`), the leaf must parse and not have expired at `now_unix_ms`
  (`tls-client-cert-expired`), and a refused replacement leaves the identity as it was. **What other keys need**, and why they are
  not built: *P-384* needs `std.ecdsa_sign` for that curve with SHA-384 (the ladder, the order and the RFC 6979 HMAC are
  parameters of `std.ecdh` and `std.bigmod` already, so this is a port, not new arithmetic), and a P-384 key parser path; *Ed25519*
  needs a signer in `std.ed25519` (verification is all it has) and PKCS#8's `1.3.101.112`; *RSA* needs a modular exponentiation by a
  **secret** exponent, which `std.bigmod` is not (`docs/tls-parity.md` §3.2: variable time, built for public values), so it needs
  a constant-time, blinded private operation first. A mutual-TLS backend whose policy accepts only RSA client certificates is
  out of reach until then, and the refusal says so (`tls-client-key-type`).

### 6.3 Which `CertificateRequest`s are answered with a certificate

The identity is chosen at `start` (§6.2). When the server's `CertificateRequest` comes, the client sends the chain only if **all** of
these hold; otherwise it sends the empty `Certificate` it sends today (RFC 8446 §4.4.2: a client with no suitable certificate sends
an empty one), and the server decides:

1. an identity was chosen for this host;
2. **`signature_algorithms`** lists `ecdsa_secp256r1_sha256` (0x0403), the only scheme this key signs with. (TLS 1.2: the
   `certificate_types` list also holds `ecdsa_sign`, 64.) A request with no `signature_algorithms` is not satisfiable:
   RFC 8446 §4.3.2 requires it, and a client cannot know what a server will verify;
3. **`certificate_authorities`** is empty or absent, or one of its names is the issuer of a certificate in our chain (byte for byte,
   as the DER `Name`; `x509.issuer_start` and `issuer_end` are whole TLVs). RFC 8446 §4.2.4 says the server SHOULD be guided by it, so a
   name that matches nothing is "not satisfiable", not a protocol error.

**Not checked**, and said: `signature_algorithms_cert` and `oid_filters`. The chain's own signature algorithms are not compared to
`signature_algorithms_cert`; if the server cannot verify our CA's signature it ends the handshake with an alert, and the client reports
`tls-alert`. `client_auth` says what happened (and `tls-alert`'s detail the alert):

| `tls.client_auth` | meaning |
|---|---|
| 0 | the server asked for nothing (or the connection resumed, which asks for nothing, §6.5) |
| 1 | asked, and the chain was sent with a `CertificateVerify` |
| 2 | asked, and the empty `Certificate` was sent: no identity for the host, or the request was not satisfiable |

**A server that requires a certificate when none is configured** ends the handshake with `certificate_required` (116; TLS 1.2 servers send
`handshake_failure`, 40). The client fails with `tls-alert`, detail 116, and `client_auth` is 2. That is the clean refusal: no new tag,
because the alert is the server's word and `client_auth` is ours. **Optional mode** (OpenSSL `-verify`, nginx `ssl_verify_client optional`,
Go `VerifyClientCertIfGiven`) completes whether a certificate was sent or not.

### 6.4 The signature, and TLS 1.2

- **TLS 1.3.** The client's flight is `change_cipher_spec`, `Certificate` (the request's context echoed, then the chain), `CertificateVerify`,
  `Finished`. The signature is over 64 bytes of 0x20, the context string `TLS 1.3, client CertificateVerify`, a zero byte, and the transcript
  hash through the client's `Certificate` (RFC 8446 §4.4.3), under `ecdsa_secp256r1_sha256`: `SHA-256` of that content, then
  `std.ecdsa_sign.sign_checked` under the identity's key and point, then `to_der`. The transcript hash is the suite's (SHA-256 or
  SHA-384); the digest signed is always SHA-256. `sign_checked` verifies the signature before it is used, as the server's does
  (`docs/tls-server.md` §3.3), so a fault sends nothing (`tls-client-sign`, never reached by an input).
- **The nonce is hedged** with 32 bytes derived from a secret the slot holds at that moment (`HKDF-Expand-Label` of the client's handshake traffic
  secret, or of the TLS 1.2 master secret, labelled `client sign hedge`): RFC 6979 §3.6, so a fault in one signature does not recur and
  nothing new is drawn from the engine's generator (the ClientHello's 96 bytes are all it draws, and `start` is unchanged).
- **The work area** is the slot's `std.ecdh` work, which the key exchange has finished with by then. No allocation grows.
- **TLS 1.2: built, not refused.** The client speaks TLS 1.2 to the upstreams a TLS 1.3 client cannot reach, and an mTLS backend that old
  is the likeliest to ask for a certificate. The slot already runs the SHA-256 and the SHA-384 transcript side by side
  (`tls_slot.transcript_add`), so the one thing 1.2 needs that 1.3 does not, a SHA-256 hash of all the handshake messages whatever the
  suite's hash, is already there. The client sends `Certificate` (a list of DER certificates, with no per-certificate extensions),
  `ClientKeyExchange`, `CertificateVerify` (RFC 5246 §7.4.8: the scheme and a signature over **all handshake messages so far, hashed with SHA-256**,
  not the content string), `change_cipher_spec`, `Finished`. The extended master secret is over the session hash through
  `ClientKeyExchange` (RFC 7627 §4), so it is computed before `CertificateVerify` and is not affected by it.

### 6.5 Resumption: what a ticket obtained with client authentication binds

`docs/tls-resumption.md` §3 gives eight rules and none says what a client certificate does to a ticket. A resumed connection sends no
certificate (a PSK handshake has no `CertificateRequest`); the server carries the client's identity in the ticket. So the ticket is a stored
verdict about *our* identity as well as the server's, and every way it could go stale is a rule:

- **9. Bound to the identity configuration.** `add_client_identity`, `replace_client_identity` and `remove_client_identity` each count as a
  `trust` call (they raise the same generation, `tmeta[t_trust]`), so **no ticket saved before is offered after**. A resumption must not be
  presented as an identity that changed or was removed, and a server that issued the ticket to an anonymous session must not be asked to
  resume it as the one we now hold. The cost: after a certificate renewal the next connection to each upstream is a full handshake. That is one
  handshake per upstream per renewal; certificates are renewed in weeks.
- **10. Bound to the client certificate's life.** The ticket's `notAfter` (rule 3) becomes the **earlier** of the server leaf's and the client leaf's,
  when a client certificate was sent. A full handshake would send an expired certificate and be refused; a resumption must not outlive it either.
- **11. Not across a host** is rule 1 unchanged, and it is what keeps an identity chosen by host name from reaching a ticket for another.
- **TLS 1.2** has no tickets here (rule 7); nothing to bind.

`start_with` with a pool whose ticket the rules refuse overwrites the ticket and does a full handshake with the identity now configured
(`docs/tls-resumption.md` §12). `tls.resumed` is true and `client_auth` 0 on a resumed connection.

### 6.6 ALPN

RFC 7301. **The API:**

```
tls.set_alpn_offer(engine, protocols) -> 0 | refusal           // the default offer, space-separated: "h2 http/1.1"; empty clears it
tls.start_alpn(engine, slot, host, now_unix_ms, pool, protocols) -> 0 | refusal   // this connection's own offer (pool 0: a new pool); empty offers none
tls.alpn(engine, slot, out) -> n                                // the protocol the server chose, into out; 0 for none
```

`start` and `start_with` offer the engine's default; `start_alpn` overrides it for one connection (a gateway talks h2 to one upstream and
HTTP/1.1 to another). `alpn` is the server's call too (`tls_identity`'s chosen protocol, unchanged); on a client engine it answers the
server's choice once EncryptedExtensions (TLS 1.3) or the ServerHello (TLS 1.2) has been read.

- **The offer** is at most 256 bytes in ALPN's wire form (each name after its length byte), each name 1 to 255 bytes. A name that is empty, over 255
  bytes, or a list over the cap is `tls-alpn-list` and the list is as it was. The offer is copied into the slot (256 bytes, §6.7) so a
  later `set_alpn_offer` does not change a handshake in flight, and so the server's choice can be checked against it.
- **The extension** goes in the ClientHello between `signature_algorithms_cert` and `supported_versions`, only when the offer is not empty.
  With none offered, the ClientHello is byte for byte today's, so every recording and ticket test stands.
- **The server's answer.** TLS 1.3: in EncryptedExtensions. TLS 1.2: in the ServerHello's extensions. Exactly one name (RFC 7301 §3.1).
  - one that is **not in the offer** is `tls-alpn-selected` (alert `illegal_parameter`, 47). The server's answer is the only thing that could make
    the client run a protocol it never offered, so it is checked byte for byte against the offer. A name longer than the offer's cap can
    never be in it, so it is the same refusal; a length that does not fit the extension is `tls-decode-error`, and the extension twice `tls-extension-repeat`;
  - an ALPN answer when **nothing was offered** is `tls-unsupported-extension` (RFC 8446 §4.2), as now;
  - ALPN in a TLS 1.3 `ServerHello` (not EncryptedExtensions) is `tls-unsupported-extension`;
  - **no answer** is a connection with no protocol: `alpn` answers 0, and the caller decides. A program that needs h2 closes. (The server is the one that
    ends a handshake with `no_application_protocol`, 120, and that arrives as `tls-alert`.)
- **ALPN and resumption.** A ticket is *not* bound to the offer. RFC 8446 §4.6.1 ties a ticket's use to the protocol only for early data, which
  is not sent; a resumed handshake carries ALPN in EncryptedExtensions again and the client checks that choice against **this** connection's offer.
  So `start_with` after a changed offer resumes and reports the new choice, and the check is the same.
- **`hooks` and `http-client` consumers.** `packages/http-client` speaks HTTP/1.1 only. Its I/O driver (`examples/http_fetch_nb/fetch_io.cho`)
  calls `tls.start`; with `set_alpn_offer("http/1.1")` a server that would pick `h2` for a client that said nothing picks `http/1.1`, and the answer is
  readable. `cancho-hooks` maps `open(...)` onto `tls.start`; it can offer `http/1.1` the same way. Neither changes unless it asks.

### 6.7 Hostile input, memory, and the refusals

- **`CertificateRequest`** (TLS 1.3 and 1.2) is at most the handshake reassembly buffer, 64 KiB, as every message. Nothing is allocated from a length it
  names: the context is at most 255 bytes (the slot's `k_context`, 256), the extensions and the authorities list are walked in place, and each is
  checked to end where it says. `signature_algorithms` and `certificate_authorities` twice in one request are `tls-extension-repeat` (RFC 8446 §4.2, alert 47 as OpenSSL sends);
  an authorities list whose names do not tile it, a name of length 0, or a `signature_algorithms` list of odd length, are `tls-decode-error`. Matching the authorities
  against our chain is at most (the chain's certificates) times (the list's bytes), about 8 × 64 KiB comparisons; it reads and compares, writes nothing.
  A second `CertificateRequest` is `tls-unexpected-message` (the flag that is set at the first, as now); one after `Finished` is out of order for the same
  reason; one in a resumed handshake is out of order too.
- **Slot layout (the shared files, as little as possible):**
  - `tls_slot.ints_len()`: **+2 words** (10,241 to 10,243), appended after `i_session_len`: `i_alpn_offer_len` (0 to 256) and `i_alpn_at` (where the server's choice starts in the offer, valid
    when `i_alpn_len`, which the server already has for its own choice, is more than 0). The slot's chosen identity is `i_identity` (the server's field for the same
    idea; a client slot is free to hold it, as id + 1, 0 for none).
  - `tls_slot.bytes_len()`: **+256 bytes** (187,191 to 187,447), appended at the end (`b_alpn_offer`, after the ticket host name). A slot is
    269,391 bytes, 272 more; 64 slots: 17 KiB.
  - `tls_slot`: one flag, `f_cert_send`, bit 24 (16,777,216; the server's client-authentication work uses bits 15 to 18). A ClientHello offering
    ALPN adds at most 4 + 2 + 256 bytes to `max_client_hello`.
  - `tls_record`: refusal codes **-120 to -131** (the server's are -40 to -71), `tls-client-*`, `tls-alpn-*` and `tls-extension-repeat`, below.
  - `tls.cho`: no field is added to `Engine`; a client engine opened with `open_mutual` keeps its identities in the field `ids` (empty for a client) and the parser's
    work in `srv` after the role word (which is 2 for it). `t_fields` is unchanged: rule 9 reuses `t_trust`, rule 10 reuses the entry's `notAfter`.
- **New tags** (alert in the last column; every refusal has a rule tag, CLAUDE.md):

| Code | Tag | When | Alert |
|---|---|---|---|
| -120 | `tls-client-key-type` | `add_client_identity`: a key, or a leaf, that is not P-256 | n/a |
| -121 | `tls-client-key-format` | a key that is not an unencrypted PKCS#8 or SEC 1 PEM block | n/a |
| -122 | `tls-client-key-mismatch` | the key is not the leaf's | n/a |
| -123 | `tls-client-cert-expired` | the leaf has expired at `now_unix_ms` | n/a |
| -124 | `tls-client-chain` | no certificate, a block that does not decode, a leaf that does not parse, or over 16 KiB | n/a |
| -125 | `tls-client-names` | no hosts, or over 512 bytes | n/a |
| -126 | `tls-client-identities-full` | four identities held | n/a |
| -127 | `tls-client-no-identity` | `replace` or `remove` of an id never added | n/a |
| -128 | `tls-client-sign` | the signer refused or its check failed: a fault, not an input | 80 |
| -129 | `tls-alpn-list` | `set_alpn_offer` or `start_alpn`: an empty name, a name over 255 bytes, a list over 256 | n/a |
| -130 | `tls-alpn-selected` | the server chose a protocol that is not in the offer | 47 |
| -131 | `tls-extension-repeat` | `signature_algorithms` or `certificate_authorities` twice in a CertificateRequest, or ALPN twice in a server's answer (RFC 8446 §4.2) | 47 |

  A client call on an engine not opened with `open_mutual`, or a server engine, is `tls-role` (-57), as the server's calls on a client engine are.

### 6.8 Revocation: the decision, proposed

**Proposed: build nothing now. Say plainly that revocation is not checked (`docs/tls-pure.md` §5.4 already does), and keep the controls that do work for a gateway.**
Evidence, from public sources read for this section (summaries of vendor and CA pages; where a default could not be read in a primary document it is marked *unverified*):

- **Few clients check by default.** Chrome does no online OCSP or CRL check and uses CRLSets; Firefox uses CRLite from version 137 and turned OCSP off for DV
  certificates in 142 (Mozilla, Aug 2025); curl checks a stapled response only with `--cert-status`, and then a missing one is a hard failure; Go's `crypto/tls` and
  `x509.Verify` check nothing; rustls checks only CRLs the program passes. *Unverified:* OpenSSL and Python `ssl` check nothing unless `X509_V_FLAG_CRL_CHECK` is
  set (as the issue says; the manual was not re-read here).
- **The public web is leaving OCSP.** Let's Encrypt removed OCSP URLs from certificates on 7 May 2025 and shut its responders on 6 Aug 2025, and Must-Staple requests
  failed from January 2025 ("Must Staple has failed to get wide browser support"; letsencrypt.org, 5 Dec 2024). CA/Browser Forum ballot SC-063 made OCSP optional and CRLs
  mandatory (effective 15 Mar 2024). Stapling was seen on about 8% of Firefox connections in early 2023 (Firefox telemetry, CA/B Forum list, Feb 2023) and has
  fallen with Let's Encrypt's change: no 2024 to 2026 measurement was found.
- **Certificates are getting short.** Let's Encrypt offers 6-day certificates (generally available 15 Jan 2026); CA/B Forum SC-081v3 cuts the maximum to 200 days
  (15 Mar 2026), 100 (2027) and 47 (2029). A short life is revocation by expiry, and it needs no code here.
- **Soft-fail defends against little.** A client that accepts a connection when no response comes is defeated by an attacker who removes the staple; only
  Must-Staple made it hard, and it is being abandoned.
- **The attack surface is real.** Honouring a stapled response means a BasicOCSPResponse parser (an ASN.1 structure with optional, tagged and nested parts), the
  responder's certificate (a delegated responder needs `id-kp-OCSPSigning` and the CA's signature), a CertID match (SHA-1 is still common, so SHA-1 would be
  needed for this one use), `thisUpdate`/`nextUpdate` with a skew rule, and the choice of what a missing or stale response means. OpenSSL itself had
  CVE-2022-1343 (`OCSP_basic_verify` reported success when the signer failed to verify). An estimate, not a measurement: 400 to 800 lines of security-critical
  code and a second signature entry point, for a check that is soft by nature and that a growing part of the web no longer feeds.

**What a gateway should do instead** (in the README of the package and in `docs/tls-pure.md` §5.4): use short-lived certificates or a private CA for
upstreams it controls; keep `trust` to the roots it needs, not the system bundle; keep `set_ticket_max_age` short (resumption is the other way a
revoked certificate stays trusted, `docs/tls-resumption.md` §3 rule 4); and, for a revoked upstream key, remove its root or replace its certificate.

**If a person wants more, the smallest useful step** is an **opt-in `require stapled OCSP`** mode, off by default: send `status_request` (RFC 6066 §8) in the ClientHello,
and when the server's `CertificateStatus` (TLS 1.2) or the leaf's `status_request` extension (TLS 1.3) carries a response, verify it against the issuer in the
chain already built, and refuse the connection if it says `revoked`, has expired, or fails to verify; a missing response is accepted unless the mode is
`require`. That is the code above, and its gate would be a differential against `openssl ocsp` and `openssl s_server -status` with Wycheproof-style malformed
responses. A CRL path (operator-supplied files, checked against the leaf's serial) is the other candidate, and is larger. **Not proposed**: live OCSP (it leaks
the host to the CA and needs an HTTP client inside the TLS client) and CRLSet or CRLite (a data pipeline, not code).

**Decision for a person:** accept "not checked, documented, mitigations as above" (proposed), or ask for the opt-in stapled mode as its own change.

### 6.9 X25519MLKEM768: the decision, proposed

**Proposed: do not build it now; do not claim the client reaches every server; add a check to the interop matrix that reports `tls-alert` 40 or 70 from a
server with a post-quantum-only policy, and revisit on the triggers below.**

- **No server that requires it was found.** No public evidence turned up of a server, CDN or cloud policy that refuses a client without the hybrid share. AWS's
  load balancer policies with the hybrid are opt-in and the console default still accepts classical clients; Cloudflare supports it and does not require it.
  The standards track has it (RFC 10024, August 2026: `X25519MLKEM768`, code point 0x11EC; key share = ML-KEM-768 encapsulation key 1,184 bytes then the X25519
  share, 1,216 bytes; the server's = ciphertext 1,088 then X25519, 1,120; the shared secret = the two secrets, ML-KEM first, 64 bytes). NSA's CNSA 2.0
  makes post-quantum key exchange mandatory for new national-security systems from 1 Jan 2027 and exclusive by 2033, with ML-KEM-1024, a different group
  and a different audience. So a *requirement* is plausible first on a government or private upstream, and not observed anywhere on the public web.
- **What share of servers *supports* it** (not the same as requires): Cloudflare reports 39% of the top 100,000 domains supporting post-quantum key agreement in
  September 2025, up from 28% six months before, and 12.8% of origins it scanned in September 2026, up from 3.7% a year earlier and 0.5% in 2023. On the
  client side more than half of human traffic to Cloudflare used the hybrid by October 2025. (Secondary readings of vendor blog posts, dated as given; Cloudflare
  Radar itself could not be read from here.) **Because servers that support it also accept X25519, the share that *needs* it is, on this evidence, near
  zero.** That is a measurement of absence from public sources, not of every server: it says nothing about a private upstream.
- **What ML-KEM-768 would cost.** Go's implementation is about 500 lines of code with 200 of comments and 650 of tests, with Barrett reduction (no
  division) from the start; on ARM64 it takes about 56 µs to encapsulate and 109 µs to decapsulate (Valsorda, words.filippo.io/mlkem768). A client needs key
  generation and **decapsulation**, the secret-handling half. In this repository that is: the NTT and polynomial arithmetic mod 3329 on `int` (no
  overflow trap risk: every product is under 2^24), SHA-3 (Keccak, which `std` does not have: a second hash with its own constant-time argument and
  vectors), the centred binomial sampler for the **secret** noise (branch-free and index-free), the Fujisaki-Okamoto re-encryption with a constant-time
  ciphertext compare and a constant-time select of the key or the implicit-rejection key, byte encoding of 12-bit coefficients, and a ClientHello that grows by 1,216
  bytes and no longer fits one packet (Chrome 124 broke middleboxes that hard-code ClientHello sizes). It also needs a bigger slot (the decapsulation key is 2,400 bytes, the
  encapsulation key 1,184) and a ClientHello buffer that is no longer 2,688 bytes.
- **The constant-time risk is the reason to be careful, more than the lines.** KyberSlash (Bernstein et al., 2024) recovered keys in minutes or hours from secret-dependent
  *divisions* in widely used implementations, including the reference code; the same failure is available to any port that writes `x / 3329` on a secret. This
  language has the tools (`value_barrier`, `docs/value-barrier.md`; `scripts/chacha20_branches.py`; the dudect harness) but each is a new audit. mlkem-native
  (CBMC and HOL Light proofs) and libcrux-ml-kem (hax) are the verified references; a port should follow one of them, with the NIST ACVP vectors and
  Valsorda's CCTV cases (negative and "unlucky" XOF cases) as gates, plus mutants and a branch audit as `docs/ecdh.md` §3 did.
- **The reason not to wait forever:** harvest-now-decrypt-later is a threat to the confidentiality of what a gateway sends today, not to a requirement the
  upstream imposes. That is an argument for *offering* the hybrid, which is a separate question from *needing* it; offering it adds the 1,216-byte share to
  every ClientHello for a benefit only if the upstream supports it. For upstreams an operator controls, who can say which they run, this is a decision per
  deployment.

**Triggers to revisit** (proposed): a named upstream announces a post-quantum-only policy; origin support passes about half of the top domains; an
OpenSSL-compatible peer the gateway must reach drops the classical groups from its default; or the hooks receivers' stack asks. **Estimate if built:** one module
for ML-KEM (about 700 lines with Keccak), one for the hybrid share in `message.cho` and `slot.cho`, a growth of `max_client_hello` by 1,216 bytes, a
dozen mutants, ACVP and CCTV vectors, a branch audit, one PR of the size of `docs/ecdh.md`'s.

**Decision for a person:** accept "not now, with triggers" (proposed), or ask for the build as its own epic with the constant-time audit as its first gate.

### 6.10 Gates

Tests for the build, each against a server that asks for a client certificate or negotiates ALPN, and a case for every rule above:
`openssl s_server -Verify` and `-verify`, nginx `ssl_verify_client on|optional`, Go `ClientAuth: RequireAndVerifyClientCert`, wolfSSL; success, wrong
CA, an expired client certificate, a server that requires one when none is configured, optional mode; the lying server for the new rules
(`scripts/tls_liar_auth.py`); the differential against `openssl s_client` for the same flights; fuzz corpora extended; `scripts/tls_mutants.py` extended and all
killed or argued; the existing suites byte for byte as they were; the cost of the extra signature, measured; `examples/http_fetch_nb/fetch_io.cho` with
`--client-cert`, `--client-key` and ALPN, live against `openssl s_server -Verify` and nginx.

### 6.11 As built (#386)

*Every number is from the command beside it. Where building found §6.1 to §6.9 wrong, the section says so in place (marked "corrected").*

**What was built**

| File | What |
|---|---|
| `packages/tls/cident.cho` (`tls_cident`), new | the identities: four, each a chain, a P-256 key, its point, the leaf's `notAfter` and the hosts it names, in one byte slice (51,636 bytes); `load` (the checks of §6.2), `remove`, `select` by host, `names_issuer` (the authorities against the chain's issuers), `list12`, `alpn_wire` |
| `packages/tls/message.cho` | the ClientHello's ALPN extension; `certificate_request` and `certificate_request12_info` (the shape, the scheme, the authorities); `encrypted_extensions_alpn`; the ServerHello's ALPN (TLS 1.2); `alpn_name` |
| `packages/tls/client.cho` | `start_alpn`, `feed_with`, the CertificateRequest handler, `send_identity` (Certificate and CertificateVerify), `client_auth`, `alpn` |
| `packages/tls/client12.cho` | `answer_request` (shared by both versions), `certificate_verify_message` (the one caller of the signer), `take_alpn`, TLS 1.2's Certificate and CertificateVerify |
| `packages/tls/tls.cho`, `slot.cho`, `record.cho` | `open_mutual`, `add_client_identity`, `replace_client_identity`, `remove_client_identity`, `client_auth`, `set_alpn_offer`, `start_alpn`, `alpn` on a client engine; the slot fields and flag of §6.7; 12 refusal codes |
| `examples/http_fetch_nb/fetch.cho`, `fetch_io.cho` | `--client-cert`, `--client-key` and `--alpn`; a response line ends ` alpn=<protocol or -> auth=<0, 1 or 2>` when either is given |

*Corrected:* §6.2 said an `open_mutual` engine keeps its identities in `ids`; it does, and in `roots` after the trust store it also keeps the default
ALPN offer (258 bytes), which a client engine has no other field for. No field was added to `Engine`.

**The signer's caller.** `certificate_verify_message` is the one place `std.ecdsa_sign` is called by the client. It signs a SHA-256 digest with
`sign_checked` (the signature verified before it is used), the nonce hedged with `HKDF-Expand-Label(secret, "client sign hedge")` of the client's
handshake traffic secret (TLS 1.3) or the master secret (TLS 1.2), into the slot's `std.ecdh` work area. TLS 1.3's content is the 64 spaces, the
context string and the transcript hash; TLS 1.2's digest is the slot's SHA-256 transcript hash whatever the suite's (the slot keeps both,
`tls_slot.transcript_add`), taken after `ClientKeyExchange` and before the extended master secret's key block, so neither is disturbed by the other.

**The lying server** (`scripts/tls_liar_auth.py`, the same server code as `tls_liar.py` imported unchanged; `tests/vectors/tls/liar_auth.txt`, replayed byte for
byte on both backends by `conformance/tls_auth.rs`): **77 connections**. 39 end `ok`, and every one that sends a chain has its Certificate, its CertificateVerify
(under TLS 1.3's context string, or TLS 1.2's SHA-256 over every message) and its Finished checked by pyca/cryptography. The rest, by rule:

| The server does | Tag | Alert |
|---|---|---|
| asks, and the client answers: ChaCha20, AES-256-GCM-SHA384, a retry to P-256, a context to echo, the identity named by a wildcard host or by `*`, an unknown extension, a chain of two certificates, the authorities naming the issuer or only the second certificate's issuer, 2,000 authorities (about 60 KB) with ours last; TLS 1.2 under three suites and P-256, a chain of two, the authorities | `ok` | none |
| asks, and the client cannot or will not: Ed25519 or RSA-PSS only, no `signature_algorithms`, authorities naming another CA, 2,000 authorities none ours, a name of the issuer's length and other bytes, no identity configured, an identity for another host; TLS 1.2 without `ecdsa_sign`, Ed25519 only, another CA, 2,000 authorities over four records | `ok`, the empty Certificate | none |
| requires one and none is configured (`certificate_required`) | `tls-alert` | |
| a second CertificateRequest; one after Finished; one in a resumed handshake; TLS 1.2: a second, one after ServerHelloDone | `tls-unexpected-message` | 10 |
| `signature_algorithms` or `certificate_authorities` twice; ALPN twice (1.3 and 1.2) | `tls-extension-repeat` | 47 |
| authorities that do not tile their list, a name of length 0, an odd `signature_algorithms`, a context longer than the message, extensions longer than it, ALPN with a list or name length that does not fit, two names, or an empty one (1.3 and 1.2) | `tls-decode-error` | 50 |
| ALPN offered h2 and http/1.1 and answered spdy/3; a prefix of an offered name; h3 for h2; 255 bytes never offered (1.3), spdy/3 (1.2) | `tls-alpn-selected` | 47 |
| ALPN in a TLS 1.2 ServerHello when nothing was offered; in a TLS 1.3 ServerHello | `tls-unsupported-extension` | 110 |
| the identity's own refusals: an expired leaf, another key, a key that is not PEM, a P-384 key, an Ed25519 leaf, no certificate, no hosts; an offer of 256 bytes, a name of 256 | `tls-client-*`, `tls-alpn-list` | (a call) |

The server also checks the ClientHello: the ALPN extension is there once, with exactly the offer, between `signature_algorithms_cert` and
`supported_versions`, and absent when nothing was offered. **The engine's rules** (`scripts/tls_tickets_auth.py`, `tests/vectors/tls/tickets_auth.txt`,
10 cases): an identity added, replaced or removed keeps a saved ticket back (rule 9), and a ticket saved after is offered and resumed with `client_auth` 0;
the client certificate's `notAfter` bounds a ticket whose server certificate lasts ten years (rule 10), offered at `notAfter` and not after; the identity is
chosen by the host; four identities, a fifth, a replace and a remove of ones never added, a mismatched key, an expired leaf and no hosts are refused through the
engine with their tags; the default offer is what `start` sends and a connection's own replaces it, even by none; a ticket resumes under another offer and the new
choice is reported.

**Recorded against real servers** (`scripts/tls_trace.py --mutual`, replayed by `conformance/tls.rs` with the other eleven, now fourteen, and in one-byte and
coalesced splits): tlslite-ng 0.8.2 (TLS 1.3, `reqCert`), `openssl s_server -tls1_2 -Verify 1` and `-tls1_3 -Verify 1` (the OpenSSL 3.6.4 of the machine that
recorded them, macOS; the other traces are 3.0.13). Each server verified the client's CertificateVerify and Finished, and said it saw the certificate.

**Interop** (`scripts/tls_auth_interop.py`, in the `lexsys-interop` image's Ubuntu 24.04 on linux-aarch64 for OpenSSL 3.0.13, nginx 1.24.0, Go 1.22.2 and
wolfSSL 5.6.6; `fetch` built by this PR's compiler): **92 rows, 92 ok**, each a server and a `fetch`. Per server and per version (1.3, 1.2): a valid
certificate accepted; none configured against `require` refused (`tls-alert`; nginx answers `400 No required SSL certificate was sent` after the handshake);
a certificate from another CA refused (by the CA list, and for OpenSSL also with no authorities named, so the server's own verification refuses what is sent);
`optional` completes with `auth=2` without a certificate and `auth=1` with one; a certificate configured and never asked for is `auth=0`; the client's own
refusal of an expired leaf and of an RSA key before any connection; OpenSSL with its clock three days on (`-attime`) refuses a certificate that expires after one; Go, wolfSSL and nginx put what they saw
in the body (`client=client-good alpn=...`) and its SHA-256 is checked. ALPN: the offer the server speaks chosen and reported; two offered, the server's order
(OpenSSL, Go) or the client's (wolfSSL: it chose h2 for `h2,http/1.1` against a list of `http/1.1,h2`) decides; no offer to a server with a list, none chosen;
**nothing in common: OpenSSL's `s_server`, nginx and Go end the handshake (`fetch` says `client.tls`; the server's `no_application_protocol` alert), wolfSSL
completes with no protocol.**

**The differential against `openssl s_client` 3.0.13** (`scripts/tls_differential_auth.py`, the liar's cases beside OpenSSL's client with `-cert`, `-key` and `-alpn`,
OpenSSL's TLS 1.3 and TLS 1.2 signatures verified by the same server code): **67 connections (the 77, less 9 that configure the engine and one that needs OpenSSL's
saved session), 58 agree, 3 differ in the alert only, 6 differ as documented, 0 otherwise.**

| Case | `packages/tls` | OpenSSL | Why |
|---|---|---|---|
| ALPN: the server chooses a name that is not in the offer (four 1.3 cases, one 1.2) | `tls-alpn-selected` | accepts | RFC 7301 §3.1: the choice is one of the client's; OpenSSL 3.0 takes any name |
| a CertificateRequest with no `signature_algorithms` | the empty Certificate | refuses the request | RFC 8446 §4.3.2 requires it; this client treats the request as one it cannot satisfy |
| duplicate extensions, an empty ALPN name, an odd `signature_algorithms` (1.2), ALPN in a TLS 1.3 ServerHello | 47 / 50 / 50 / 110 | 47 / 80 / 80 / 47 | alerts only |

**What the differential also showed, not as a difference of outcome.** OpenSSL's `s_client -cert` **sends its certificate to a request that does not name its issuer**
(authorities naming another CA, 2,000 names none ours, a name of the issuer's length), to one whose `signature_algorithms` lacks its scheme in TLS 1.2 (no `ecdsa_sign`),
and to every host. This client sends the empty Certificate in each, which is what §6.3 says: the server asked for certificates from CAs we are not under (RFC 8446
§4.2.4), and sending it a certificate it cannot accept discloses the identity to a server that cannot use it. Both complete against the liar, which does not
require one. A deployment whose server lists the wrong authorities (some do) would find this client stricter than OpenSSL; the answer is to fix the list,
or, if that is the policy a person wants, to send anyway (§6.12, question 1).

**Fuzzing.** `fuzz_messages` takes four more kinds (a TLS 1.3 CertificateRequest; the TLS 1.2 one with its info; EncryptedExtensions with an ALPN offer sent; the
authorities of a request matched against the fixture identity's chain); `fuzz_flight` and `fuzz_client` configure the fixture identity, and `fuzz_flight` offers ALPN
when the input's first byte is 128 or more; the fixture holds the mutual traces' roots and identity and a fourth recorded flight, the mutual one, whose Finished
verifies under the harness's ClientHello, so the client's Certificate and CertificateVerify are produced from real flights; 3 recorded mutual handshakes and 23 new
inputs join the corpora (`tests/vectors/fuzz`), all replayed on both backends by `conformance/tls_fuzz.rs`. `scripts/tls_fuzz.py` mutates the three mutual traces
and the 39 honest connections of the lying server (identities and ALPN offers included): **20,000 mutated connections over 53 recorded handshakes, 0 traps.** AFL++ was
not run for this PR (see §6.12).

**Mutants** (`python3 scripts/tls_mutants.py target/release/cancho`): **148, of which 146 killed and 2 argued equivalent, none survived** (103 before,
45 new). Each new one is a bug in one place: the context string, the transcript missing the Certificate or the CertificateVerify, the request's context not echoed, the
digest, the hedge, the notAfter bound on a ticket, the authorities ignored or compared by length alone or only for the first certificate, the scheme not required,
TLS 1.2's digest under the suite's hash, its chain with TLS 1.3's extensions or cut short, `ecdsa_sign` not required, `*` never matching, any identity for any host, an
expired leaf or another key accepted, a request that does not parse accepted (authorities that do not tile, a name of length 0, a scheme list twice), the engine not choosing by
host or leaving tickets offerable after an add, a replace or a remove, an unoffered ALPN choice accepted or compared by length alone or when nothing was offered, the
start of the choice lost, the extension left out of the ClientHello, each of the answer's three length checks, its empty form, twice in either version, in a TLS 1.3 ServerHello,
the alert, the default offer never used, the cap on the offer and the offer truncated. **Two are equivalent, each argued in `EQUIVALENT`:** an ALPN answer in
EncryptedExtensions accepted by the parser when nothing was offered (`take_alpn` refuses it with the same tag), and a name over 255 bytes accepted by `alpn_wire` (the offer's 256-byte
cap refuses it with the same tag). Four of the new ones survived a first run and each got the case that kills it: a name of the issuer's length and other bytes (the comparison
by length alone), a chain whose second certificate has another issuer (only the first considered), a selection of an offered name's length (h3 for h2), and the engine's default
offer through `start` rather than `start_alpn`. Three older mutants' texts (`server_name` in a TLS 1.3 ServerHello, an unexpected extension, resumption never advertised) and
one of the server's (`start` on a server engine, now in `start_plain`) were brought up to the new code. `scripts/tls_server_mutants.py`: **71 of 71 killed.**

**Cost of the extra signature** (`scripts/tls_client_auth_cost.py`; the machine is the M-series Mac this was built on, arm64 macOS, load average
12 to 17 while it ran, the LLVM backend, `fetch` against `openssl s_server -www` 3.6.4 on loopback; the Docker VM had no disk left to run it
in the `lexsys-hooks-env` image, and `gram` was not used): the client's own CPU (user and system, `wait4`) per full handshake, the best of 3 runs of
200, a connection for each request and no tickets:

| | the server asks for nothing | asks, the client declines (empty Certificate) | asks, the client answers | **the signature** |
|---|---|---|---|---|
| TLS 1.3 | 3.462 ms | 3.511 ms | 5.308 ms | **1.797 ms** (51% of a handshake) |
| TLS 1.2 | 3.965 ms | 3.911 ms | 5.713 ms | **1.802 ms** (46%) |

That is what `docs/ecdsa-sign.md` §7 measured for a `sign_checked` on an M4 Max, 1.62 ms (a signature 0.82 and its verification 0.80), plus about 0.2 ms
for the chain's bytes, the digest and the message. Asking is free (the first two columns agree within the noise of a loaded machine); answering costs two
P-256 multiplications. It is the price of the safety check (`sign_checked` verifies before it sends); `sign` alone would be half of it. A mutual handshake costs about 1.5 times a one-way one here, and a resumed connection pays none of it (§6.5).

**Existing suites.** The 84 connections of `liar.txt`, the 20 ticket cases of `tickets.txt`, the eleven earlier traces, the 64 streams and the RFC 8448 record
all replay as recorded: nothing was re-recorded, and the ClientHello is byte for byte today's when no ALPN is offered. `scripts/tls_differential.py` and
`scripts/tls_interop.py` were not changed.

### 6.12 Not done, not verified, and for a person

1. **Should a client send its certificate to a request whose authorities do not name its issuer?** *Proposed: no, as built* (§6.3, and the differential above): it
   is what RFC 8446 §4.2.4 says a server wants, and it discloses nothing to a server that cannot accept it. OpenSSL sends. A deployment whose server mis-lists its
   authorities is the case for a switch.
2. **Not built:** P-384, Ed25519 and RSA client keys (§6.2 says what each needs); `signature_algorithms_cert` and `oid_filters` are not checked (a server that cannot verify our
   CA's signature ends the handshake and the client reports `tls-alert`); post-handshake authentication; client authentication on a resumed connection (a ticket carries it).
3. **Not run:** the merged `packages/tls` server's client certificates (#384 had not landed; its branch is `tls-server-client-certs`); wolfSSL, Go and nginx in TLS 1.2 with an
   RSA client certificate (the client refuses one before connecting); AFL++ over the new harnesses for hours (the corpora are seeds plus 20,000 mutations); the cost on
   x86-64.
4. **Timing.** The signature is `std.ecdsa_sign`'s, whose constant-time argument and branch audit are `docs/ecdsa-sign.md` §2 and §6; the client adds no new
   secret-dependent branch (the code that reads the key is `tls_cident.key` and `point`, two slices handed to the signer). No dudect test of the client handshake as a whole.
5. **Revocation and X25519MLKEM768** are §6.8 and §6.9, for a person.
6. **Not independently reviewed (#209),** as the rest of the client.
