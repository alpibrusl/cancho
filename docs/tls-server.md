# A TLS 1.3 server for `packages/tls`: the design

> **Status: steps 1 and 2 built: the signer (`docs/ecdsa-sign.md`) and the TLS 1.3 server (§10, as built); its open
> questions (§9) answered as proposed (2026-10-07). Not independently reviewed (#209).** `packages/tls` is a client (`docs/tls-pure.md`). Two programs of the toolbox need the other
> side: `lexsys-mqtt`, a broker whose clients connect on 8883, and `lexsys-gateway`, a reverse proxy that terminates HTTPS. Both
> are at the design stage and both list TLS as out of scope because "it needs foreign code and would make the authority report
> unbounded". A server in `packages/tls` removes that reason. This document is the design; its numbers are measured where it
> names a measurement, and are otherwise arithmetic from measured parts, marked so. Where a later PR finds a claim false, that PR
> corrects it here, in place.

---

## 1. What a server needs that the client does not have

Most of the client is reused as it stands: the record layer and its AEADs (`tls_record`: ChaCha20-Poly1305 and AES-GCM, on the
hardware path since #334), the key schedule, the handshake message codec (`tls_message`), the transcript hash, the slots, the
DRBG, and the key exchanges (`std.x25519`, `std.ecdh` on P-256 and P-384, both constant time). What is new:

| | the client today | a server needs |
|---|---|---|
| **a signature with a secret key** | verifies only | signs `CertificateVerify` in every full handshake. **No signing in `std` is constant time today**: `std.ecdsa` verifies only, `std.rsa` verifies only, and `std.ed25519.sign` branches on secret data (`docs/ed25519.md`, "Not constant-time"). This is the prerequisite, §3. *Built since: `std.ecdsa_sign` (`docs/ecdsa-sign.md`).* |
| **a private key** | none | read from a PEM file, held in the engine, never readable back (§4) |
| **the handshake state machine** | ClientHello out, ServerHello in | ClientHello in, choose, ServerHello to Finished out, client Finished in (§5) |
| **choices** | the server makes them | suite, group, HelloRetryRequest, certificate by SNI, ALPN (§5.2) |
| **attacker input at scale** | one server, chosen by the program | any peer that can reach the port, many at once, each able to cost a signature (§7) |

## 2. Scope

### 2.1 Version 1

- **TLS 1.3 only.** A ClientHello without `0x0304` in `supported_versions` is refused with a `protocol_version` alert
  (`tls-server-version`). TLS 1.2 is §9's question 1.
- **All three TLS 1.3 suites.** The server picks, in its own order: on a machine where `hw_aes_gcm()` is true,
  `TLS_AES_128_GCM_SHA256` first, then `TLS_CHACHA20_POLY1305_SHA256`, then `TLS_AES_256_GCM_SHA384`; without it, ChaCha20 first
  (the software AES-GCM is several times slower: `docs/crypto-builtins.md`). This is OpenSSL's and Go's rule.
- **Groups X25519, P-256, P-384**, in that order of preference. The server uses the first of its groups the client sent a key
  share for; if none, but the client's `supported_groups` names one, it sends one HelloRetryRequest for it (stateful: no cookie).
  A second ClientHello that still has no share for it is refused (`tls-server-retry-share`). Browsers send an
  `X25519MLKEM768` share with an X25519 share; the server ignores the hybrid and takes the X25519 one.
- **One certificate type: ECDSA P-256** (`ecdsa_secp256r1_sha256`), signed in constant time (§3). It is what Let's Encrypt and
  most ACME clients issue on request, and what every TLS 1.3 client must verify (RFC 8446 §9.1). P-384, Ed25519 and RSA-PSS
  keys are later (§8), each behind its own constant-time signer.
- **Certificates chosen by SNI**: up to 16 identities (a chain and its key, with the names it serves), the first being the
  default for a ClientHello without `server_name` or with a name no identity has. An identity can be replaced while the engine
  runs (a renewed certificate): connections already started keep the old one.
- **ALPN**: the program gives a list (`http/1.1` for the gateway, `mqtt` for the broker if it wants one). A client that offers
  ALPN with no protocol in the list is refused with `no_application_protocol` (RFC 7301 §3.2, `tls-server-alpn`); a client that
  offers none is accepted and the program sees none.
- **Middlebox compatibility** (RFC 8446 Appendix D.4): the legacy session id is echoed and a `change_cipher_spec` record is sent
  after the ServerHello, as OpenSSL does, so a client in compatibility mode is not surprised. *As built: once, after the first
  ServerHello or HelloRetryRequest the server sends (so after a HelloRetryRequest, not again before the ServerHello), and
  whatever the session id; every TLS 1.3 client must drop one (RFC 8446 §5).*

### 2.2 Not in version 1, and why

| | why not yet | where |
|---|---|---|
| client certificates (mutual TLS) | MQTT deployments use them, so it is wanted; it is a second verification path (`x509_verify` without a host name) and an authorization interface | step 4 (§8), §9's question 2 |
| session tickets (resumption) | saves the signature, which is most of the server's cost (§6); stateless tickets need a ticket key and its rotation | step 5 (§8) |
| **0-RTT early data** | replayable by design; no program here needs it | never, unless a design argues for it |
| TLS 1.2 | a second server state machine; who needs it is §9's question 1 | step 6, if answered yes |
| renegotiation, compression, PSK without (EC)DHE, post-handshake authentication | not in TLS 1.3, or not needed | never |
| OCSP stapling, certificate transparency | the server would send what it is given; nothing reads it here | if a program asks |

## 3. The prerequisite: ECDSA P-256 signing in constant time

### 3.1 The pieces already constant time

`std.ecdh` multiplies a point by a **secret** scalar in constant time (`docs/ecdh.md` §2): a four-bit window over complete
formulas, the table read under masks, `std.bigmod`'s register arithmetic with masked reductions, and a timing test that found
and fixed a leak. `std.bigmod.inverse` is Fermat's `a^(n-2)`: it branches on the exponent's bits, and `n - 2` is public, so it is
constant time in `a`. ECDSA signing is built from these:

```
k      = the nonce (§3.2), 1 <= k < n
(x, _) = k·G                              std.ecdh's scalar multiplication
r      = x mod n;  r == 0 -> next k       public once the signature is sent
s      = k^-1 · (e + r·d) mod n           bigmod: inverse (public exponent), mul, add; d and k secret
s == 0 -> next k
```

The new code is a fixed-base multiplication (the existing variable-base one, given G; a fixed-base table is a later
optimisation) and a few register operations mod n. **What stays variable time is on public values only**: the r == 0 and
s == 0 tests (probability about 2^-256), and the encoding of the signature.

*Corrected (step 1): "a few register operations" left one out. `bigmod.load_reg`, the only way into a register, skips zero
bytes and compares with n limb by limb, stopping at the first that differs: a branch on a private key or a nonce. ECDH never
loads its scalar into a register, so nothing had needed more. `bigmod.load_secret` places bytes by position only
(`docs/ecdsa-sign.md` §2.2).*

### 3.2 The nonce

A biased or repeated nonce gives the key away. The nonce is **RFC 6979's deterministic nonce with added randomness** (§3.6
there): HMAC-DRBG over SHA-256, seeded with the key, the message hash and 32 bytes from the engine's DRBG. Deterministic so a
bad DRBG cannot repeat a nonce for two messages; hedged so that a fault injected into one signature does not recur for the
same message. RFC 6979 Appendix A.2.5's P-256 vectors test the deterministic half, with the added randomness empty.

### 3.3 After signing

The server verifies its own signature with `std.ecdsa.verify` before sending it (about 0.8 ms, `docs/tls-resumption.md` §1),
and refuses the handshake with `tls-server-sign-check` if it fails. A fault during signing then sends nothing that would help
recover the key. §9's question 3 asks whether that cost stays.

### 3.4 How it is tested

- RFC 6979 A.2.5 vectors, and 10,000 signatures from random keys and messages, each verified by OpenSSL (`openssl dgst
  -verify`) and by `std.ecdsa`; each with a bit flipped, refused by both.
- A dudect timing test, `scripts/ecdsa_sign_timing.py`, the shape of `scripts/ecdh_timing.py`: a fixed key against random keys,
  and a key of 1 against random ones (the case that found `std.ecdh`'s leak). The gate is |t| below 4.5 at 10^6 measurements,
  as there.
- The value-barrier audit (`docs/value-barrier.md`) over the new functions.
- Mutants over the new code (`scripts/mutate.py`), every one killed or argued equivalent.

*Corrected (step 1): there is no `scripts/mutate.py`; each crypto module has its own mutants script, and the signer's is
`scripts/ecdsa_sign_mutants.py`. OpenSSL verifying a signature does not show that its nonce is RFC 6979's, so the 10,000
signatures are also compared byte for byte with RFC 6979 written in Python (`docs/ecdsa-sign.md` §5.2). Results:
`docs/ecdsa-sign.md` §5 and §6.*

## 4. Keys and certificates

- **Formats.** A chain as PEM certificates, leaf first. A key as PEM `PRIVATE KEY` (PKCS#8, unencrypted) or `EC PRIVATE KEY`
  (SEC 1), P-256 only; anything else is refused with its tag (`tls-server-key-type`, `tls-server-key-format`). The engine
  checks that the key is the leaf's (its public point equals the certificate's) before it accepts the identity
  (`tls-server-key-mismatch`).
- **The engine never reads files.** The program reads them (it holds the capability), and gives the bytes; so the authority
  report shows the reads where the program makes them, and the engine stays a pure package.
- **Held, not shown.** There is no function that returns the key. Replacing or removing an identity overwrites its key, best
  effort, as `tls.drop` does with session keys (`docs/tls-pure.md` §7.3).
- **The chain is not verified by the server.** It is sent as given; a chain that clients refuse is the operator's to fix.
  The engine checks only that the leaf parses, that its key is P-256, and that it is not expired at the time the identity is
  added (`tls-server-cert-expired`), so a stale file fails at start, not at the first client.
- *As built, the room (`packages/tls/identity.ls`): a chain of at most 16 KiB as Certificate sends it (each certificate's DER
  and 5 bytes), names of at most 1 KiB, an ALPN list of at most 512 bytes; over them, `tls-server-chain`, `tls-server-names`
  and `tls-server-alpn-list`. A chain block that does not decode, or no certificate at all, is `tls-server-chain` too. 16
  identities take 274 KiB of the engine, allocated by `open_server` only.*

## 5. The interface and the protocol

### 5.1 The interface

The same shape as the client: slots, bytes in and bytes out, no socket and no capability inside.

```
srv = tls.open_server(heap, slots)         // an Engine whose slots are server slots
tls.seed(srv, entropy)                     // 32 bytes, as for the client
id  = tls.add_identity(srv, chain_pem, key_pem, names, now_unix_ms)   // names: "a.example b.example *.example"
tls.replace_identity(srv, id, chain_pem, key_pem, now_unix_ms)
tls.set_alpn(srv, "http/1.1")              // a list, in the server's order of preference
tls.serve(srv, slot, now_unix_ms)          // a new connection in `slot` (*as built: not `accept`, below*)
... tls.feed / take / send / recv / event / finish / eof / drop, as for a client ...
tls.server_name(srv, slot, out)            // the SNI the client sent, once established
tls.alpn(srv, slot, out)                   // the protocol chosen
```

A separate `Server` type was considered, with the shared calls duplicated; one `Engine` with a role per slot keeps one set of
byte-moving calls and the existing tests of them. An engine is all clients or all servers: `accept` on a client engine and
`start` on a server engine are refused (`tls-role`).

*Corrected (step 2): `accept` is a builtin's name (`tcp_accept`'s family: `accept` itself, edition 2), and a function may not
take one, so the call is `tls.serve`. `tls-role` also refuses `trust` on a server engine and `add_identity`, `replace_identity`,
`set_alpn`, `server_name` and `alpn` on a client one. `server_name` and `alpn` answer once the ClientHello is answered, not only
once established. Added, for a program's log and the interop matrix: `tls.suite`, `tls.group` and `tls.retried` (what the
connection negotiated), and `tls.alert_received` (the alert the peer sent, when `failure` is `tls-alert`), on either role.*

### 5.2 The handshake

1. **ClientHello**, reassembled up to 16 KiB (an ML-KEM-768 share alone is 1,184 bytes, so a browser's ClientHello is a few KiB; step 2 records
   the largest of the interop clients; *measured: Chromium's, 1,818 bytes with its X25519MLKEM768 share; of the command-line
   clients curl's, 512; OpenSSL and mosquitto 308, wolfSSL 395, Go 240, §10.4*),
   parsed with the bounds `tls_message` already applies to server messages, plus the rules the server owns: ~~`legacy_version`
   0x0303~~ (*corrected: RFC 8446 §4.2.1 says a server MUST NOT negotiate with `legacy_version` once `supported_versions` is
   there, and `openssl s_server` takes 0x0301; it is not read*); `0x0304` in `supported_versions`; compression `null` only; `signature_algorithms` including
   `ecdsa_secp256r1_sha256`; no extension twice; `pre_shared_key` last if present (and ignored in version 1, with
   `psk_key_exchange_modes`); `early_data` ignored, so its data is skipped as RFC 8446 §4.2.10 allows for a server that rejects
   it, up to 16 KiB, then refused. Each refusal is an alert and a tag (§5.4).

   *As built, the rules the list left out:* `signature_algorithms`, `supported_groups` and `key_share` all present
   (`tls-server-missing-extension`, RFC 8446 §9.2); a share of this server's groups at its group's length, at most one a group,
   and only for a group in `supported_groups`, and one `host_name` in `server_name` (`tls-server-illegal-parameter`, which
   also takes a compression method other than null and `pre_shared_key` not last); an ALPN list of non-empty names. A share
   of a group the server does not have (an X25519MLKEM768 hybrid, GREASE) is skipped, as is any extension it does not use. A
   `server_name` over 255 bytes is kept as none, so the default identity answers it. The rules about what is offered are
   checked after the whole message is read, in a fixed order: version, compression, a missing extension, a share outside
   `supported_groups`, suite, signature scheme, group. *The second ClientHello*, after a HelloRetryRequest, must keep the
   session id, still offer the suite the retry named, send a share of the group it named, and not offer early data
   (`tls-server-retry-share`; RFC 8446 §4.1.2 lets a server check this). *Records:* one `change_cipher_spec` of `01` is
   dropped after the first ClientHello and before the client's Finished (Appendix D.4), and any other is
   `tls-unexpected-message`; a ClientHello, a Finished or a KeyUpdate must end its record (§5.1), checked before the message
   is handled, so nothing is sent for one that does not; a plaintext alert is read after the server's flight, since a client
   that refuses the flight before it has the handshake key alerts in the clear. *Early data* is skipped as RFC 8446 §4.2.10
   says for a server that rejects it: after the flight, every record that does not open under the client's handshake key
   is dropped until one opens, and before a second ClientHello every `application_data` record is; at most 16 KiB of
   ciphertext in all, then `tls-server-early-data-size`.
2. **Choose**: suite, group (or HelloRetryRequest, then back to 1 once), identity by SNI, ALPN.
3. **ServerHello**, `change_cipher_spec`, then under the handshake keys **EncryptedExtensions** (ALPN, `server_name` empty if it
   was used), **Certificate**, **CertificateVerify** (§3), **Finished**, all in one flight from `take`.
4. **Client Finished** checked in constant time against the expected MAC; then the application keys, and `event` says
   established.

The handshake needs one ECDHE key pair and one shared secret on the chosen group, and one signature.

### 5.3 Limits

A slot holds one connection. A program that accepts more connections than slots holds them unaccepted or closes them; the
engine does not queue. The ClientHello must arrive within the program's own deadline; the engine has no clock of its own
beyond the `now_unix_ms` it is given.

### 5.4 Refusal tags

Each refusal sends the alert RFC 8446 names and has a tag beginning `tls-server-`: `version`, `suite`, `group`,
`retry-share`, `sigalg`, `alpn`, `client-hello-format`, `client-hello-length`, `extension-repeat`, `early-data-size`,
`finished`, `sign-check`, `key-type`, `key-format`, `key-mismatch`, `cert-expired`, `identities-full`, and `tls-role`. The
existing record-layer tags (`tls-record-*`) apply unchanged.

*As built (`packages/tls/record.ls`, `slot.ls`): six more, for refusals the list had no tag for, and the alerts. The
"record-layer tags" are the client's (`tls-unexpected-message`, `tls-record-overflow`, `tls-bad-record-mac`,
`tls-decode-error`, `tls-protocol-version`, `tls-key-share`, `tls-too-many-messages`, `tls-alert`, `tls-peer-closed`);
there is no `tls-record-*`.*

| tag | alert | when |
|---|---|---|
| `tls-server-version` | protocol_version (70) | no `supported_versions`, or one without 0x0304, or no extensions at all |
| `tls-server-suite` | handshake_failure (40) | no TLS 1.3 suite |
| `tls-server-group` | handshake_failure (40) | no share and no supported group of X25519, P-256 or P-384 |
| `tls-server-sigalg` | handshake_failure (40) | no `ecdsa_secp256r1_sha256` |
| `tls-server-retry-share` | illegal_parameter (47) | the second ClientHello changed what it may not, or has no share of the group asked for |
| `tls-server-extension-repeat` | illegal_parameter (47) | an extension twice |
| `tls-server-illegal-parameter` | illegal_parameter (47) | *added:* a value RFC 8446 forbids (§5.2 above) |
| `tls-server-missing-extension` | missing_extension (109) | *added:* no `signature_algorithms`, `supported_groups` or `key_share` |
| `tls-server-alpn` | no_application_protocol (120) | ALPN offered, none in common |
| `tls-server-client-hello-format` | decode_error (50) | a ClientHello that does not parse |
| `tls-server-client-hello-length` | decode_error (50) | a ClientHello over 16 KiB |
| `tls-server-early-data-size` | unexpected_message (10) | over 16 KiB of early data |
| `tls-server-finished` | decrypt_error (51) | the client's Finished |
| `tls-server-sign-check` | internal_error (80) | the signature did not verify before it was sent |
| `tls-server-key-type`, `-key-format`, `-key-mismatch`, `-cert-expired`, `-identities-full` | none: `add_identity`'s answer | §4 |
| `tls-server-chain`, `-names` | none: `add_identity`'s answer | *added:* §4, as built |
| `tls-server-alpn-list` | none: `set_alpn`'s answer | *added:* a name over 255 bytes, or a list over 512 |
| `tls-server-no-identity` | none: `serve`'s or `replace_identity`'s answer | *added:* no identity added, or none of that number |
| `tls-role` | none | a call for the other role (§5.1) |

## 6. Cost

*The signature is measured (step 1, `docs/ecdsa-sign.md` §7); the total is arithmetic from measured parts, and step 2
measures it.* One full handshake on the server, P-256 certificate:

| operation | cost | source |
|---|---|---|
| X25519 key pair and shared secret | 1.14 ms | `docs/tls-resumption.md` §1 (6-vCPU Linux VM on an M4 Max) |
| ECDSA P-256 sign (`std.ecdsa_sign.sign`) | **1.5 ms** on an Intel i7-1260P, **0.82 ms** on an Apple M4 Max | measured, LLVM backend, `docs/ecdsa-sign.md` §7 |
| verifying the signature (§3.3) | **1.5 ms** on the i7-1260P, **0.80 ms** on the M4 Max | measured: `sign_checked` less `sign`, same section |
| **total** | **about 4 ms of CPU on the i7, about 2.8 ms on the M4** | the X25519 row is the M4 VM's; the i7's X25519 is not measured |

*Corrected (step 1): the estimate was "about 1.3 to 2.6 ms" to sign and 3 to 5 ms in all. Measured, the signature is at the
bottom of that range, and the check of §3.3 costs as much as the signature on the i7 (not "about a fifth of the cost" as
§9's question 3 has it; on the M4 it is about 0.8 ms, as estimated). OpenSSL 3.0.13 signs in 23 µs on the same i7
(`openssl speed ecdsap256`, 44,132 a second), 65 times faster.*

So about 250 full handshakes a second a core on the i7, by the arithmetic above. OpenSSL's P-256 signing is tens of microseconds with its NIST-prime
arithmetic; this server's handshake is in the order of 10 to 50 times OpenSSL's. For the two programs that is acceptable:
an MQTT client and an HTTP keep-alive connection handshake once and then stay. **What changes it** is in §8: session tickets
(no signature on a resumed connection), a fixed-base table for k·G, and arithmetic specialised to the NIST primes
(`docs/ecdsa.md` §5.4). Memory: a server slot is the client slot (about 179 KiB, `docs/tls-pure.md` §7.4) plus nothing
significant; each identity holds its chain (a few KiB) and key in the engine.

## 7. Threat model, the server's side

`docs/tls-pure.md` §4 holds for the record layer. What a server adds:

- **CPU exhaustion.** A client can make the server sign by sending one ClientHello, and abandon the connection. At about 4 ms a
  handshake, 250 ClientHellos a second take a core. The engine cannot bound this: it does not see the network. The program
  does, and the design asks each to bound **handshakes started per second and in progress**, with a counter the engine keeps
  (`tls.handshakes_in_progress`). The broker and the gateway design their limits with it.
- **Memory exhaustion.** Bounded by the slots: a ClientHello cannot grow a slot past its 16 KiB reassembly limit.
- **Timing of the key.** §3. The record layer and the key exchanges are constant time already.
- **Choosing the identity.** SNI is attacker-chosen: a name with no identity gets the default, never an error that tells names
  apart, and the name is compared as bytes after lower-casing ASCII, as `packages/x509/names.ls` does for the client.
- **Parsing.** The ClientHello is now the input every peer controls. It goes to the fuzz harness (`scripts/tls_fuzz.py`) and
  the AFL setup (`scripts/fuzz_afl.py`) with a corpus of real ClientHellos (OpenSSL, Go, curl, Firefox, Chrome, mosquitto),
  and the gate is no panic and no trap. *As built (§10.5): `scripts/tls_fuzz.py --server` mutates the lying client's honest
  connections; AFL++ runs two harnesses, `fuzz_hello` (the parser) and `fuzz_server` (the engine from `serve`), from the
  ClientHellos of OpenSSL, curl, Go, wolfSSL, mosquitto and Chromium. Firefox's is not in it: no Firefox could be run here.*

## 8. Steps, each its own PR with its gates

1. **`std.ecdsa.sign` on P-256** (§3; *built as `std.ecdsa_sign`, since `std.ecdh` imports `std.ecdsa` and the signer needs
   `std.ecdh`'s ladder: `docs/ecdsa-sign.md` §1*), the key parsers (§4), with the vectors, the 10,000 OpenSSL cross-checks, the timing
   test, the audit and the mutants. Gate: all pass; the timing test's |t| below 4.5.
2. **The TLS 1.3 server** (§5), with:
   - interop: `openssl s_client`, curl, Go `crypto/tls`, wolfSSL's client, and mosquitto's clients (`mosquitto_pub` over 8883,
     against a lex-sys echo program), each across the three suites and three groups, with and without HelloRetryRequest;
   - a lying client: `scripts/tls_liar_client.py`, the shape of `scripts/tls_liar.py`, one case per refusal tag and per rule
     of §5.2;
   - a differential: the same malformed ClientHellos to `openssl s_server`, the alerts compared (an `EXPECTED` table for the
     cases where the RFC allows either);
   - the fuzz corpus of §7, mutants over the new code, and the cost of §6 measured.
   Gate: every case passes; the cost measured and written here.
3. **An example** that the two programs can copy: `examples/tls_echo` (a server over `std.conns` and a poller), and a section
   in `docs/http-server.md` on serving `packages/http-server` over TLS.
4. **Client certificates**: CertificateRequest, the client's chain verified against a configured trust store with no host
   name, the verified subject and SANs given to the program. §9's question 2 decides whether it moves before step 3.
5. **Session tickets**: stateless, sealed with a ticket key from the DRBG, rotated, `psk_dhe_ke` only (a fresh key exchange
   every time, so forward secrecy stays), never 0-RTT. Saves the signature and its check: about 2 to 3.5 ms of the 3 to 5.
6. **TLS 1.2**, ECDHE-ECDSA with AEAD suites and the extended master secret required, if §9's question 1 says so.
7. **More key types**, each with its own constant-time signer and timing test: P-384, Ed25519 (on `std.field25519`, which is
   constant time, replacing `std.ed25519`'s big-number signing), RSA-PSS (needs blinding; `std.bigmod.pow_mod` branches on its
   exponent, so it cannot take a private exponent as it is).

## 9. Open questions, for a person

*Decided: all five as proposed (2026-10-07).*

1. **TLS 1.2 for the broker.** Many embedded MQTT clients (older mbedTLS and wolfSSL builds on microcontrollers) speak TLS 1.2
   only. *Proposed: version 1 is 1.3 only; the broker's design counts the clients it must serve, and step 6 is built if any
   need 1.2.*
2. **Client certificates before the example?** *Proposed: no; step 4 follows step 3, unless the broker's design makes them
   its default way to authenticate, in which case step 4 moves ahead of step 3.*
3. **Verify every signature before sending it** (§3.3): 0.8 ms a handshake, about a fifth of the cost. *Proposed: yes, until
   step 7's fixed-base and NIST-prime work makes signing cheap enough that the check's share is worth measuring again.*
4. **One engine type, or two** (§5.1). *Proposed: one, with a role.*
5. **Independent review.** The client is "not independently reviewed" (#209). A server that signs with a long-lived key is a
   larger exposure than a client with ephemeral keys. *Proposed: the server carries the same notice, and the broker and the
   gateway say so in their READMEs until #209 is answered.*

## 10. As built: step 2, the server

*PR #339. The numbers are measured, each with the command that gives it; where building found this document wrong, the
section that said so is corrected in place, marked "corrected (step 2)" or "as built".*

### 10.1 What was built

| File | Module | What |
|---|---|---|
| `packages/tls/hello.ls` | `tls_hello` | the ClientHello parsed with the rules of §5.2, the choices (suite, group, retry group, ALPN), and the messages a server sends: ServerHello, HelloRetryRequest, EncryptedExtensions, Certificate, CertificateVerify |
| `packages/tls/server.ls` | `tls_server` | one server connection: `start`, `feed` and its record loop, the key schedule, the flight, the one function that calls the signer, the client's Finished compared in constant time, KeyUpdate |
| `packages/tls/identity.ls` | `tls_identity` | a server engine's configuration in one byte slice: 16 identities (key, public point, names, the certificate_list as Certificate sends it) and the ALPN list |
| `packages/tls/tls.ls` | `tls` | `open_server`, `add_identity`, `replace_identity`, `set_alpn`, `serve`, `server_name`, `alpn`, `handshakes_in_progress`, `suite`, `group`, `retried`, `alert_received`; the role; `feed` sends a server slot to `tls_server` |
| `packages/tls/slot.ls`, `record.ls`, `message.ls` | | the server's states, flags and five slot fields; its 24 refusal codes and their alerts; `hrr_random` made public |

**Reused unchanged:** the record layer and its AEADs, `tls_slot`'s transcript, keys, record queue, `fail` and `forget`, the
ECDH share a HelloRetryRequest needs (`new_ecdh_share`: the P-256 or P-384 scalar comes from the X25519 secret by HKDF, as for
the client), and `tls_client`'s `take`, `send`, `recv`, `finish`, `event`, `peer_eof` and `drop`, which serve a server slot as
they are. `client.ls` is not touched, so its 88 mutants' texts still match.

**What is not shared, and why.** The record loop (`feed`, `on_record`, the handshake reassembly, the alert reader) is the
server's own, about 250 lines the shape of the client's. The client's loop calls the client's message handler, and lex-sys
has no function values (`docs/function-values.md`), so sharing it would mean `tls_client` importing `tls_server`, and every
client build (`tls_driver`, hooks) taking the server and `x509_key` with it. The server's loop also differs where it must:
early data, a plaintext alert after the flight, one `change_cipher_spec` between the ClientHello and the Finished, and the
"message ends its record" rule checked before a message is handled.

**The signer.** `tls_server.sign` is the one caller: SHA-256 of CertificateVerify's content, `ecdsa_sign.sign_checked` under
the identity's key, hedged with 32 bytes of the engine's DRBG drawn at `serve` (RFC 6979 §3.6), verified under the identity's
public point before it is used (§3.3, `tls-server-sign-check`), then `ecdsa_sign.to_der`. Its work is the slot's `std.ecdh`
work area, which the key exchange has finished with. Each connection draws 96 bytes at `serve`: the ServerHello random, the
X25519 secret, and the hedge; all three are in the slot's keys, which `forget` and `drop` overwrite.

**Memory.** A server slot is the client's (`docs/tls-core.md` §9.1) and five words. A server engine adds its identities, 274 KiB
for 16, and 75 KiB of work for the key parser (`std.ecdh`'s), and gives up the client's 1 MiB of roots; a client engine adds
nothing (`open` allocates neither).

**Editions.** `server.ls` is edition 7, for `hw_aes_gcm()`; `identity.ls` edition 6, as `x509_key` is.
