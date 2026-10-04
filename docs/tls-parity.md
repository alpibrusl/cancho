# TLS parity with the OpenSSL backend: AES-GCM, P-256/P-384 key exchange, TLS 1.2

> **Status: design (#207, PR 1 of 5).** Sub-issue 10 of the self-contained TLS 1.3 client (#197). The issue asked for a
> measured number of receivers needing TLS 1.2 or AES-GCM before anything is built. That number cannot be measured here, and
> the requirement replaces it: **the maintainer's requirement is that the pure client be equivalent to the OpenSSL backend**
> it is to replace in `lexsys-hooks` (#210). The maintainer chose the scope below, "AEAD parity", over full parity with
> OpenSSL's defaults and over the issue as written. This document settles what that means and how it is built and tested.
> It supersedes `docs/tls-pure.md` §3.2 to §3.4, which are marked so.

---

## 1. The decision, and the number the issue asked for

**The number.** The issue's gate is "receivers needing 1.2 or AES-GCM, out of how many". For `lexsys-hooks` that means its
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
| extended master secret | **required** (the issue's rule) | a TLS 1.2 server without it (`tls-protocol-version`) |
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
- **GHASH** with a constant-time carry-less multiply: 64-bit operands split into masked quarters, as BearSSL's `ghash_ctmul64`
  does, with no table indexed by the key-derived `H`.
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

### 3.2 P-256 and P-384 key exchange, constant-time (PR 3)

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
- the existing tlslite-ng traces and lying server replay unchanged;
- new traces recorded for each suite, and for an HRR to each group;
- the lying server gains HRR cases: twice, an unoffered group, the group already sent, a changed suite;
- live against `openssl s_server -ciphersuites` each suite and `-groups P-256` (forcing the HRR), Python `ssl` and tlslite-ng;
- RFC 8448's key schedule rows extended to the AES-128-GCM record keys of its trace.

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
- **The flight:**
  - ServerHello;
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
  - the explicit nonce reused;
- the mutant and fuzz bars of #205: at least 15 mutants killed, and the driver fuzzed with no trap.

## 4. Files

| File | PR | What |
|---|---|---|
| `std/aes.ls`, `std/gcm.ls` | 2 | bitsliced AES-128/256; GHASH and GCM |
| `std/ecdh.ls`, and `std/bigmod.ls` where it branches on data | 3 | P-256 and P-384 key exchange |
| `packages/tls/record.ls` | 4, 5 | the AEAD chosen by the suite; TLS 1.2's AES-GCM nonce |
| `packages/tls/message.ls` | 4, 5 | the new ClientHello; HRR; TLS 1.2's messages |
| `packages/tls/client.ls`, a new `packages/tls/client12.ls` | 4, 5 | the 1.3 changes; the 1.2 handshake in its own file, under 2,000 lines |

## 5. What stays different from OpenSSL, after all five PRs

- **Suites:** the 21 TLS 1.2 suites of §2 that are not ECDHE with an AEAD.
- **Groups:** X448, P-521 and FFDHE.
- **The extended master secret** is required (§2).
- **SHA-1 and SHA-224 signatures** are refused.
- **No resumption:** the OpenSSL backend can resume sessions, which halves its handshake CPU (`docs/tls-nonblocking.md` §8.5,
  §10.4); this client does a full handshake every time. That costs time, not reach.

Each is refused with its own tag, so an operator can see which one a receiver needs. #210's comparison against OpenSSL
reports how many receivers each one costs.
