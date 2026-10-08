# A TLS 1.3 server for `packages/tls`: the design

> **Status: steps 1 to 3 and 5 built: the signer (`docs/ecdsa-sign.md`), the TLS 1.3 server (§10, as built), the example
> the broker and the gateway copy, `examples/tls_echo` (§11), and session tickets (§12); its open questions (§9) answered as proposed (2026-10-07).
> Not independently reviewed (#209).** `packages/tls` is a client (`docs/tls-pure.md`). Two programs of the toolbox need the other
> side: `cancho-mqtt`, a broker whose clients connect on 8883, and `cancho-gateway`, a reverse proxy that terminates HTTPS. Both
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
| ~~session tickets (resumption)~~ | *built:* saves the signature, which is most of the server's cost (§6); stateless tickets with a ticket key and its rotation | step 5 (§8), §12 |
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
- *As built, the room (`packages/tls/identity.cho`): a chain of at most 16 KiB as Certificate sends it (each certificate's DER
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

*As built (`packages/tls/record.cho`, `slot.cho`): six more, for refusals the list had no tag for, and the alerts. The
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
| `tls-server-binder` | decrypt_error (51) | *added (§12.5):* a ticket this server made, passing every rule, whose PSK binder does not match |
| `tls-server-ticket-config` | none: `set_tickets`' answer | *added (§12.5):* a count over 8 or a lifetime outside one second to seven days |
| `tls-server-ticket-key` | none: `set_ticket_keys`' answer | *added (§12.5):* no key, a length that is not a whole number of 32-byte keys, or more than 4 |

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

*Resumed handshakes (step 5, §12.9): 1.52 ms against 3.15 ms of `examples/tls_echo` on the M4, and about a third to a half of a
full handshake on an x86-64 box too loaded to give an absolute figure.*

**Measured (step 2), in place of the estimate.** `python3 scripts/tls_server_cost.py <tls_serve> 20`: the server
(`tests/programs/tls_serve.cho`, LLVM backend, `echo` mode, one P-256 identity) under `openssl s_time -new`, full
handshakes one after another for 20 seconds a row, the server process's CPU (user and system, `/proc/<pid>/stat`)
divided by the handshakes `s_time` completed. **The machine:** Ubuntu 24.04, linux-aarch64, in Docker's 6-vCPU VM on the
Apple M4 Max of `docs/tls-assurance.md` §6.1, the host busy with other builds (load average 4 to 5 of 6). Suite
AES-128-GCM (the server's choice: the CPU has AES instructions). Two runs:

| client's share | ms of server CPU a handshake | handshakes a second a core |
|---|---|---|
| X25519 | 3.86, 4.06 | 259, 246 |
| P-256 | 3.44, 3.55 | 290, 282 |
| P-384 | 6.09, 6.06 | 164, 165 |
| P-521, then a HelloRetryRequest to P-256 | 3.65, 3.47 | 274, 288 |
| *`openssl s_server` 3.0.13, X25519, the same client* | *0.20* | *4,975* |

So **about 3.5 to 4 ms a handshake on X25519 or P-256, 6 ms on P-384**: inside the design's 3 to 5 ms for the first two,
and above step 1's arithmetic for the M4 (2.8 ms), since the measured figure is the whole process (the socket, the
poller, the transcript, HKDF and the records) and not the three operations alone. A HelloRetryRequest costs nothing
measurable (one more message hashed). OpenSSL's server is **17 to 20 times** cheaper, at the bottom of the "10 to 50
times" said here. Not measured: x86-64 (the i7 of `docs/ecdsa-sign.md` would be slower: its signature and check are 3 ms
against the M4's 1.6), and the Cranelift backend.

*Corrected (step 3, §11.4): about 0.9 ms of the X25519 row is `tls_serve`'s own loop, not the handshake. On the same
machine, in one session, alternating, `examples/tls_echo` costs **3.0 ms** a handshake where `tls_serve` costs 3.8 to
3.9 ms, with the same engine, client and identity. Where `tls_serve` spends the difference was not investigated; the
engine's share is at most the 3.0 ms.*

The paragraph that follows was the estimate's: about 250 full handshakes a second a core on the i7, by the arithmetic above. OpenSSL's P-256 signing is tens of microseconds with its NIST-prime
arithmetic; this server's handshake is in the order of 10 to 50 times OpenSSL's. For the two programs that is acceptable:
an MQTT client and an HTTP keep-alive connection handshake once and then stay. **What changes it** is in §8: session tickets
(no signature on a resumed connection), a fixed-base table for k·G, and arithmetic specialised to the NIST primes
(`docs/ecdsa.md` §5.4). Memory: a server slot is the client slot (about 179 KiB, `docs/tls-pure.md` §7.4) plus nothing
significant; each identity holds its chain (a few KiB) and key in the engine. *As built: five words a slot; 274 KiB of identities and 75 KiB
of key-parsing work in a server engine (§10.1).*

## 7. Threat model, the server's side

`docs/tls-pure.md` §4 holds for the record layer. What a server adds:

- **CPU exhaustion.** A client can make the server sign by sending one ClientHello, and abandon the connection. At about 4 ms a
  handshake, 250 ClientHellos a second take a core. The engine cannot bound this: it does not see the network. The program
  does, and the design asks each to bound **handshakes started per second and in progress**, with a counter the engine keeps
  (`tls.handshakes_in_progress`). The broker and the gateway design their limits with it. *As built (step 3, §11.2):
  `examples/tls_echo` bounds both, delays the excess rather than refusing it, and gives a handshake a deadline; the
  whole server process measured 3.0 ms a handshake on the M4 (about 330 ClientHellos a second take a core, not 250) and
  5.5 ms on CI's x86-64 (about 180).*
- **Memory exhaustion.** Bounded by the slots: a ClientHello cannot grow a slot past its 16 KiB reassembly limit.
- **Timing of the key.** §3. The record layer and the key exchanges are constant time already.
- **Choosing the identity.** SNI is attacker-chosen: a name with no identity gets the default, never an error that tells names
  apart, and the name is compared as bytes after lower-casing ASCII, as `packages/x509/names.cho` does for the client.
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
     against a cancho echo program), each across the three suites and three groups, with and without HelloRetryRequest;
   - a lying client: `scripts/tls_liar_client.py`, the shape of `scripts/tls_liar.py`, one case per refusal tag and per rule
     of §5.2;
   - a differential: the same malformed ClientHellos to `openssl s_server`, the alerts compared (an `EXPECTED` table for the
     cases where the RFC allows either);
   - the fuzz corpus of §7, mutants over the new code, and the cost of §6 measured.
   Gate: every case passes; the cost measured and written here.
3. **An example** that the two programs can copy: `examples/tls_echo` (a server over `std.conns` and a poller), and a section
   in `docs/http-server.md` on serving `packages/http-server` over TLS. *Built (§11): the example, its tests and its cost;
   `http.server` could not take bytes that did not come from its own sockets; *corrected: it can now* (`docs/http-server.md`
   §11, a byte-fed mode), and `examples/https_hello` is the HTTPS server built on it.*
4. **Client certificates**: CertificateRequest, the client's chain verified against a configured trust store with no host
   name, the verified subject and SANs given to the program. §9's question 2 decides whether it moves before step 3.
5. **Session tickets**: stateless, sealed with a ticket key from the DRBG, rotated, `psk_dhe_ke` only (a fresh key exchange
   every time, so forward secrecy stays), never 0-RTT. Saves the signature and its check: about 2 to 3.5 ms of the 3 to 5.
   *Built (§12, #379): the design is §12.1 to §12.10, what was built and measured §12.11 to §12.15.*
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
| `packages/tls/hello.cho` | `tls_hello` | the ClientHello parsed with the rules of §5.2, the choices (suite, group, retry group, ALPN), and the messages a server sends: ServerHello, HelloRetryRequest, EncryptedExtensions, Certificate, CertificateVerify |
| `packages/tls/server.cho` | `tls_server` | one server connection: `start`, `feed` and its record loop, the key schedule, the flight, the one function that calls the signer, the client's Finished compared in constant time, KeyUpdate |
| `packages/tls/identity.cho` | `tls_identity` | a server engine's configuration in one byte slice: 16 identities (key, public point, names, the certificate_list as Certificate sends it) and the ALPN list |
| `packages/tls/tls.cho` | `tls` | `open_server`, `add_identity`, `replace_identity`, `set_alpn`, `serve`, `server_name`, `alpn`, `handshakes_in_progress`, `suite`, `group`, `retried`, `alert_received`; the role; `feed` sends a server slot to `tls_server` |
| `packages/tls/slot.cho`, `record.cho`, `message.cho` | | the server's states, flags and five slot fields; its 24 refusal codes and their alerts; `hrr_random` made public |

**Reused unchanged:** the record layer and its AEADs, `tls_slot`'s transcript, keys, record queue, `fail` and `forget`, the
ECDH share a HelloRetryRequest needs (`new_ecdh_share`: the P-256 or P-384 scalar comes from the X25519 secret by HKDF, as for
the client), and `tls_client`'s `take`, `send`, `recv`, `finish`, `event`, `peer_eof` and `drop`, which serve a server slot as
they are, and its `finished_mac`, `on_alert` and `compact_recv`, made public for the server. `client.cho` changes in nothing
else, so its mutants' texts still match.

**What is not shared, and why.** The record loop (`feed`, `on_record`, the handshake reassembly) is the server's own,
about 200 lines the shape of the client's. The client's loop calls the client's message handler, and cancho
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

**Editions.** `server.cho` is edition 7, for `hw_aes_gcm()`; `identity.cho` edition 6, as `x509_key` is.

### 10.2 Interop

`python3 scripts/tls_server_interop.py <tls_serve>`, in the image `scripts/interop/server.Dockerfile` describes (Ubuntu 24.04
on linux-aarch64, in Docker on the M4 Max of `docs/tls-assurance.md` §6.1), `tests/programs/tls_serve.cho` built with the
LLVM backend. Each row is one connection: the client verifies the chain and the name against the row's CA, its data comes
back, and the server's own line for the connection says the suite, group, name and protocol the row asked for, and
whether a HelloRetryRequest came first. For a `retry` row the client's only share is P-521's, with the group after it in
`supported_groups`. curl and mosquitto have no option for TLS 1.3's groups or suites; they were set through an
`OPENSSL_CONF`. **88 rows, 88 ok:**

| Client | Version | Rows | What |
|---|---|---|---|
| `openssl s_client` | OpenSSL 3.0.13 | 23 | 3 suites × 3 groups, each direct and after a HelloRetryRequest; SNI choosing the second identity, a wildcard name, no SNI (the default); ALPN `http/1.1` agreed; ALPN `h3` alone refused, `no_application_protocol` |
| curl | 8.5.0, on OpenSSL 3.0.13 | 18 | 3 × 3 × 2, an HTTP request to the server's `http` mode |
| Go `crypto/tls` | 1.22.2 | 9 | 3 groups × 2 (Go does not let a client choose TLS 1.3's suites: the server's, AES-128-GCM); SNI; ALPN `h2,mqtt` agreed on `mqtt`, the server's order; ALPN `h3` refused |
| wolfSSL | 5.6.6 | 19 | 3 × 3 × 2 (`scripts/interop/wolfssl_client.c`), ALPN `mqtt` |
| mosquitto | 2.0.18 | 19 | 3 × 3 × 2, each row `mosquitto_sub` receiving the server's message and `mosquitto_pub` publishing one the server prints, against `tls_serve`'s `mqtt` mode; ALPN `mqtt` |

**Not run:** Firefox and Chrome (no browser in the image). A Chromium ClientHello was caught instead (the Claude desktop
app's built-in browser, a Chromium, sent to a listener on this machine) and answered by the server in the driver: it chose
AES-128-GCM, X25519 over the X25519MLKEM768 hybrid, and `h2`. It is in the fuzzing corpus (§10.5).

### 10.3 The lying client

`python3 scripts/tls_liar_client.py <server driver> tests/vectors/tls/liar_client.txt`: **110 connections**, each a client
that changes one thing, recorded and replayed byte for byte on both backends by `conformance/tls_server.rs` (and the honest
ones again with the client's bytes fed one byte a line). The client is pyca/cryptography's primitives and RFC 8446, written
apart from the server; it checks every byte the server sends, the CertificateVerify signature under the leaf's key
included. 29 end `ok`: 26 honest connections (every suite and group, a HelloRetryRequest to each group, SNI by name, by wildcard, unknown and
absent, ALPN in the server's order and ignored without a list, early data skipped before and after a HelloRetryRequest
and at exactly 16 KiB, a ClientHello in one-byte records and one of exactly 16 KiB, a KeyUpdate answered, close_notify both
ways, legacy_version 0x0301, GREASE, a hybrid share, `pre_shared_key` ignored), the server's suite order with and without AES
instructions, and an identity replaced and a replacement refused. The other 81 each end with their tag and, for a connection, the alert §5.4 names, in the clear or under the key the server then holds. **Every
tag of §5.4 is reached but `tls-server-sign-check`,** which needs the signer to fault; its path is shown by the mutant
that checks the signature under another identity's point (§10.6), which every honest connection kills.

### 10.4 The differential against `openssl s_server`

`python3 scripts/tls_server_differential.py`: the lying client's connections decided on a ClientHello (72 of the 110: the
client's bytes there do not depend on the server's) sent as recorded to `openssl s_server` 3.0.13 with the same identity,
groups, suites and ALPN list, and the outcomes compared. **59 agree, 7 differ in the alert only, 6 differ as `EXPECTED`
says, 0 otherwise.**

| Case | `packages/tls` | OpenSSL | Why |
|---|---|---|---|
| a host name of 300 bytes | accepted, the default identity | unrecognized_name | RFC 6066 §3 allows either; this server never tells names apart (§7) |
| an unknown extension twice | illegal_parameter | accepted | RFC 8446 §4.2: no extension twice; OpenSSL checks only those it knows |
| two X25519 shares | illegal_parameter | accepted | RFC 8446 §4.2.8 lets a server refuse it |
| a ClientHello over 16 KiB (two cases) | decode_error | waits, or accepts | this server's limit (§5.2); OpenSSL's is larger |
| a second ClientHello with another session id | illegal_parameter | accepted | RFC 8446 §4.1.2 lets a server check it |

The alerts that differ: an empty `supported_versions` and an odd `cipher_suites` length (decode_error here, protocol_version
there); two host names (illegal_parameter against decode_error); a low-order X25519 share (illegal_parameter against
internal_error); a record of version 2.0 (protocol_version against none); a fatal alert or close_notify instead of a
ClientHello (none here, unexpected_message there). **Found by it, and fixed:** this server refused a ClientHello whose
legacy_version was 0x0301, as §5.2 said to; OpenSSL takes it, and RFC 8446 §4.2.1 forbids a server to negotiate with that
field once `supported_versions` is there. §5.2 is corrected, and the case is an honest one.

### 10.5 Fuzzing

- **AFL++** (`scripts/fuzz_afl.py`, 4.09c in the image of §10.2, one core a harness), two new harnesses: `fuzz_hello`, a
  ClientHello's body through `tls_hello.client_hello` and every choice made from it, and `fuzz_server`, the engine from
  `serve` with a fixed identity, fed a client's bytes in chunks the input names (odd-length inputs without an ALPN list).
  Seeded from the ClientHellos of §10.2's clients and Chromium (`tests/vectors/fuzz/server/`, `hello/`). Two runs, the second resuming the first's queue on the final code:

  | Harness | Executions | Hours | Per second | Edges | Crashes | Hangs |
  |---|---|---|---|---|---|---|
  | `fuzz_hello` | 11,501,751 | 0.75 | 4,246 | 130 of 348 | 0 | 0 |
  | `fuzz_server` | 1,391,722 | 1.25 | 309 | 1,283 of 11,069 | 0 | 0 |
  | **total** | **12,893,473** | | | | **0** | **0** |

  **No crash and no hang.** `fuzz_server` is slow because every input loads the identity and signs: 3 ms of each 3.2.
  `--minimize` kept 74 and 131 inputs, committed with the six real ClientHellos beside them. CI's `tls-assurance` job
  fuzzes both for two minutes on x86-64 (its first run: 352,994 and 18,029 executions, 0 crashes, 0 hangs).
- **Mutation** (`scripts/tls_fuzz.py --server`): 20,000 of the lying client's honest connections, one line of the
  client's bytes mutated in each (bits flipped, bytes set, cut short, slices duplicated or dropped, bytes inserted, a
  length set), replayed: **0 traps**.
- **The corpus is a regression test:** `conformance/tls_fuzz.rs` runs every committed input of both harnesses on both
  backends.

### 10.6 Mutants

`python3 scripts/tls_server_mutants.py <cancho>`: **71 of 71 killed**, none argued equivalent. Each is one bug in
`hello.cho`, `server.cho`, `identity.cho`, or the server's part of `tls.cho` and `slot.cho`, against the 110 recorded
connections. Two survived a first run, and each got the case that kills it: a leaf of another curve was caught later
anyway, by the key's match against the certificate (a P-384 leaf with a key that is not PEM now says `key-type`, not
`key-format`), and `handshakes_in_progress` was never read mid-handshake. The client's `scripts/tls_mutants.py` still kills
**103 of 103**, after two of its mutants' texts were brought up to date with `set_read_keys` and `set_write_keys` as the
hardware AES change (#334) left them: on main the script stopped with "the text occurs 0 times".

### 10.7 The existing suites

Unchanged by this PR, run again: the client's 84 lying-server cases and 20 ticket cases re-recorded byte for byte identical;
`scripts/tls_differential.py` 59 agree, 17 alert only, 8 as documented, 0 otherwise, and `--handshakes` 21 of 21;
`scripts/x509_matrix.py` 35 cases, 0 wrong; `scripts/publish_packages.py --check` with the server's three modules
published.

### 10.8 Where this differs from the design

- `tls.accept` is `tls.serve` (a builtin owns `accept`), §5.1.
- `legacy_version` is not read, §5.2 (the differential found it).
- Six refusal tags more than §5.4 named, and the "record-layer tags" are the client's, §5.4.
- The record loop is the server's own, not the client's, §10.1.
- `suite`, `group`, `retried` and `alert_received` added, for logs.
- An identity's chain is at most 16 KiB, names 1 KiB, the ALPN list 512 bytes, §4.

### 10.9 Not done, and not verified

- **`tls-server-sign-check` is not reached by any input**: it needs the signer to fault. A mutant reaches its path.
- **Firefox's and Chrome's own clients** were not run; a Chromium ClientHello was answered in the driver, not over a socket
  to a completed handshake.
- **x86-64**: the cost ran on linux-aarch64 only. CI's `tls-assurance` job (ubuntu-latest, x86-64) runs the interop
  matrix (88 of 88 there too; its Go sends a second share, so its three `retry` rows are taken directly, which the
  harness allows for Go alone), the differential (the same 59, 7 and 6) and two minutes of each fuzzer.
- **Timing:** no new secret-dependent code here but the Finished comparison, which is the client's pattern; the signer's
  timing is step 1's. No dudect test of the server's handshake as a whole.
- **Not independently reviewed (#209)**, as the client; the broker and the gateway must say so in their READMEs.

## 11. As built: step 3, the example

*PR #346. The numbers are measured, each with the command that gives it.*

### 11.1 What was built

`examples/tls_echo/`, four files, edition 6, no foreign code (*corrected, `http-server.md` §11: it was three; `examples/https_hello` needed the same
pieces, so what was in `echo.cho` and `tls_echo.cho` and has nothing to do with an echo moved, unchanged, to `front.cho`*):

| File | Module | What |
|---|---|---|
| `tls_echo.cho` | (root) | `main`: the engine, the identities, the listener, the signals, and the loop; `usage` |
| `front.cho` | `tls_front` | what any program that terminates TLS here has: the options and their parsing (`std.flags`), the per-slot state, the log, admission (the bounds), accepting, writing the engine's bytes, ending a connection, reload |
| `echo.cho` | `echo_loop` | the loop: one `Poller`, `std.conns`, each connection in the same slot of the table and of the engine; the echo with back-pressure (`pump`), the timeouts, the stop |
| `identity.cho` | `echo_identity` | an identity's `chain.pem`, `key.pem` and `names` read beneath the directory handle and given to `add_identity` or `replace_identity`; the key's buffer overwritten after |

```
tls_echo --port <n> --dir <directory> [--identity <subdirectory>]... [--alpn <p1,p2>]
         [--connections <n>] [--handshakes <n>] [--rate <per second>] [--handshake-timeout <ms>] [--idle <ms>]
         [--per-address <n>] [--per-address-rate <per second>]
```

It logs, per connection, what was negotiated and how it ended:

```
conn 7 peer=127.0.0.1:51109 established suite=TLS_AES_128_GCM_SHA256 group=x25519 sni=echo.lex-sys.test alpn=echo hrr=no waited=0 handshake=5
conn 7 peer=127.0.0.1:51109 closed ok in=11 ms=1009
```

**The authority report** is bounded and pinned by `conformance/tls_echo.rs`: `args`, `clock`, `conn_accept`, `conn_read`,
`conn_write`, `dir_read`, `err_write`, `file_read`, `fs_read("")`, `heap`, `io_write`, `net_in("")`, `poll`,
`signals("HUP,INT,TERM")`, `signals_read`; no `ffi`, no foreign symbol. The other examples pin theirs in a test, not a
file, and so does this one.

**Reading the files, as narrowly as the language allows.** `narrow` takes a literal, and the operator names the
directory, so a path read is `fs_read("")` whatever the program does (`docs/agent-toolbox.md` §2.2). What the language
does allow is to spend it once: `main` reads 32 bytes of `/dev/urandom` and opens the directory with `open_dir`, and
the rest of the program, the reload included, is handed the `Dir` and never the `Fs`. So `serve`, `run` and `reload`
say `dir_read` and `file_read`, not `fs_read`: the code that faces the network can read beneath that one directory and
nothing else, one component at a time and following no link (`docs/directory-handles.md`). The cost is that a symbolic
link there is refused (`ELOOP`): certbot's `live/` is links into `archive/` and a Kubernetes secret volume is links into
`..data/`, so a deployment copies the files in (a certbot `--deploy-hook` does, and sets the permissions this process
needs at the same time). A program that knew its directory at build time could narrow to the literal instead
(`Fs("/etc/cancho-mqtt")`), and its row would say so.

*(Built, PR #364, `narrowing-into-several.md` §11.1: `examples/tls_echo_fixed` is that program. Its `main` is
`narrow(fs, "/dev/urandom", "/etc/cancho/tls_echo")`, one `narrow` for the two paths it reads, so its pinned report
(`conformance/tls_echo_fixed.rs`) has `fs_read("/dev/urandom")` and `fs_read("/etc/cancho/tls_echo")` where `tls_echo`'s has
`fs_read("")`, and the same labels otherwise. It shares `serve` and the loop with `tls_echo`; the cost is that the directory is
fixed when it is built and `--dir` is refused.)*

### 11.2 The two decisions

**Reload is `SIGHUP`, not a file's modification time.** Both are possible today (`dir_stat` answers an mtime beneath a
`Dir`, #263). The signal is chosen because a renewal replaces two files, and a poll can see one changed and not the
other: the engine refuses that pair (`tls-server-key-mismatch`, and the old identity keeps serving), so a poll is not
unsafe, but it reloads at a moment nobody chose, logs a refusal for a correct deployment, and costs a `stat` per file
per interval. A signal is sent by the deploy hook after both files are in place, which is the moment the operator
means, and it is what nginx, HAProxy and mosquitto do. On `SIGHUP` every identity is read again and given to
`tls.replace_identity`; a connection whose ClientHello was answered keeps the certificate it was sent, and a refused
replacement is logged (`reload 0 refused tls-server-key-mismatch`) with the old identity still serving. Names are not
reloaded (`replace_identity` keeps them); a change of names is a restart.

**The handshake bounds delay; the table refuses.** Every connection is accepted into a slot of the table and a queue,
unwatched, so its ClientHello waits in the kernel and costs nothing. A queued connection starts (`tls.serve`, then it
is watched) when fewer than `--handshakes` are in progress (`tls.handshakes_in_progress`) and fewer than `--rate` were
started in this second; oldest first. A connection over the table (`--connections`) is accepted and closed at once,
before any TLS. Each phase has `--handshake-timeout` (queued, and in progress, separately), so a peer that connects and
sends nothing holds a handshake place for that long and no longer; an established connection gets close_notify after
`--idle`. Why delay and not refuse: a burst of honest clients (a broker restarted, a thousand MQTT clients reconnecting
at once) is the common case of "too many handshakes", and delaying it costs each a wait while refusing costs each a
retry with backoff; the attacker's case is bounded either way, by the same two numbers. The alternative of leaving the
excess in the kernel's listen queue is not available: there is no way to stop watching a `Listener` short of closing it
(`poller_remove` takes a `Conn`), so a listener that is not accepted from wakes `poller_wait` at once, every time.

**Per address** ([`conn-peer.md`](conn-peer.md)). Every line the example prints about a connection says who it is from
(`peer=<address>:<port>`: `conn 3 peer=127.0.0.1:51109 established ...`, `refused 7 peer=... full`), from `conns.peer`
asked once after the connection is put in its slot. Two more bounds count under the peer's key (`addr.key`: an IPv4
address, an IPv6 /64), both off unless given: `--per-address <n>` closes a connection at once, as the table does
(`refused <id> peer=... per-address`), when `<n>` from its key are already held; `--per-address-rate <n>` leaves a queued
connection queued while `<n>` handshakes from its key were already started in this second, and does not hold up the
connections behind it. The second keeps a table of 1,024 keys per one-second window; a 1,025th key in a window waits for
the next. A client behind a proxy or NAT is the proxy's or the NAT's address and counts with everything behind it.

The defaults: 256 connections, 32 handshakes in progress, 100 started a second, 10 s to finish a handshake, 60 s
idle. *Corrected on this PR:* the rate's default was 200, "most of one core" at 4 ms; CI's x86-64 runner measured
5.5 ms a handshake (§11.4), where 200 a second is more than the one core the loop has and the bound bounds nothing. At
100 it is a third of a core on the M4 (34%) and 59% on that runner, both measured. A deployment sets it from its own machine's
figure.

**Back-pressure.** A connection's socket is read only when everything read before it has been echoed and written, so
a peer that does not read its echo stops being read, and its slot's three 16 KiB buffers are the most it can make the
server hold. A stop (`SIGINT`, `SIGTERM`) refuses new connections, closes queued and handshaking ones, sends
close_notify on established ones, and exits once those are written or after two seconds.

### 11.3 Tests

- **`conformance/tls_echo.rs`**, in `cargo test` on both CI targets, against `packages/tls`'s own client
  (`tests/programs/tls_many.cho`), so it needs no TLS library outside the repository: the authority report pinned; 64
  connections at once with at most 4 handshakes in progress, each verifying the chain and the name, sending 16,384 bytes,
  getting them back and getting close_notify when idle; a reload that takes and one that is refused; then 8 more
  connections on the renewed identity; then `SIGTERM`, exit status 0.
- **`python3 scripts/tls_echo_test.py <tls_echo>`**, in CI's `tls-assurance` job (linux-x86_64) and run here in the
  image of §10.2 (linux-aarch64) and on macOS 26 (darwin-aarch64, OpenSSL 3.6): **8 of 8 cases on each of the three**:

  | case | what |
  |---|---|
  | `suites` | `openssl s_client`, 3 suites × 3 groups and a HelloRetryRequest (P-521 then P-256): the echo comes back, the log line says the suite, group, SNI, ALPN and `hrr` asked for |
  | `many` | 200 connections at once (Python `ssl`), 20 echoes of 1 to 20,000 bytes each, every byte compared, each closed `ok` with close_notify both ways |
  | `reload` | a connection opened before `SIGHUP` keeps echoing; one after it gets the renewed certificate (by serial); after a refused reload the renewed one still serves |
  | `bound` | `--handshakes 2`: two peers that send nothing hold both places; an honest client is delayed 1.26 s, not refused, and completes when their 1.5 s deadline frees one |
  | `rate` | `--rate 5`: 15 clients at once all complete in 2.0 s, the last having waited 2.0 s |
  | `full` | `--connections 4`: a fifth connection is closed at once (0 ms), before a handshake |
  | `idle` | `--idle 1000`: close_notify after 1.00 s |
  | `shutdown` | `SIGTERM` with 10 connections: 10 close_notify, exit 0 |
  | `peer` | the established, closed and refused lines each say `peer=127.0.0.1:<the client's source port>` |
  | `per-address` | `--per-address 3`: a fourth connection from 127.0.0.1 is closed at once (0 ms) as `per-address`; a client from 127.0.0.2 (Linux only; macOS has no second loopback address, and the step says it was skipped) is served; a place freed by a close is reusable |
  | `addr-rate` | `--per-address-rate 2`: six clients from one address complete in 2.0 s (the last waited 1.99 s); a client from 127.0.0.2 meanwhile took 48 ms (Linux) |

  curl and mosquitto are not in it: they speak HTTP and MQTT, which an echo answers with their own request; their
  interop with this engine is §10.2's matrix.
- **Mutants.** Eight single edits to `echo.cho`, each built and run against the case that should catch it; **8 of 8
  killed**: the handshake bound ignored (`bound`: not delayed), the rate ignored (`rate`), no handshake deadline
  (`bound`), no idle timeout (`idle`, and `conformance/tls_echo.rs`), a stop that leaves established connections to the
  drain (`shutdown`), a reload that adds an identity instead of replacing it (`reload`: the old certificate still
  served), the connection ended as soon as the peer's close_notify is read (`many`), and no table bound (`full`).
- **Found while building it**, both in the example and fixed there: `tls.event` says `closed` as soon as the peer's
  close_notify is read, before the server's own is queued, so a loop that ends a connection on `closed` closes the
  socket without answering (Python's `ssl` saw `UNEXPECTED_EOF` on every `unwrap`); the example ends a connection on
  `closed` only once it has queued its own. And a client's Finished and its end of file read in one turn made the
  connection fail before the loop had logged it established (`openssl s_time`'s connections were never logged); the
  socket is now read only when nothing else moved.

### 11.4 Cost

`python3 scripts/tls_echo_test.py <tls_echo> --cost 20`: `openssl s_time -new` (OpenSSL 3.0.13, its default X25519
share, the server's AES-128-GCM) for 20 s a row, the server's CPU from `/proc/<pid>/stat` divided by the handshakes.
LLVM backend; the machine of §6 (Ubuntu 24.04, linux-aarch64, Docker's 6-vCPU VM on the Apple M4 Max, the host's load
average 2 to 4):

| clients, server bounds | handshakes a second | server CPU | ms of CPU a handshake |
|---|---|---|---|
| 1, none | 316 | 94% of a core | **2.98** |
| 4, none | 341 | 104% of a core (one thread: saturated) | 3.04 |
| 4, `--rate 200` (the first default) | 210 (4,204 in 20 s, 21 windows) | 66% of a core | 3.15 |
| 4, the defaults (`--rate 100`) | 105 (2,104 in 20 s) | 34% of a core | 3.27 |

So **about 3.0 ms of CPU a full handshake, 330 to 340 handshakes a second on the one core the loop runs on**, and the
rate bound holds a flood of handshakes to the share of a core it was set for. Beside it, alternating in one session,
`tls_serve` (§6) under the same client measured 3.82 and 3.94 ms against the example's 3.03 and 2.99: the correction
in §6.

**x86-64**, CI's `tls-assurance` job on this PR (GitHub's ubuntu-latest runner, OpenSSL 3.0.13, the same command), the
first measurement of the server on x86-64:

| clients, server bounds | handshakes a second | server CPU | ms of CPU a handshake |
|---|---|---|---|
| 1, none | 170 | 94% of a core | **5.49** |
| 4, none | 189 | 104% of a core | 5.53 |
| 4, `--rate 200` | 188 | 104% of a core: the bound above what the core can do | 5.55 |
| 4, the defaults (`--rate 100`), the next run | 105 (2,104 in 20 s) | 59% of a core | 5.58 |

**About 5.5 ms a handshake there, 1.8 times the M4's**, in line with `docs/ecdsa-sign.md` §7's signer (1.5 ms on an
i7 against 0.82 on the M4, and as much again for the check). It is why the rate's default is 100, not 200 (§11.2). The
8 cases passed there too (`many` in 1.9 s).

### 11.5 Not done, and not verified

- **`http.server` over TLS**: *built after this section* (`docs/http-server.md` §11): the package has a byte-fed mode and
  `examples/https_hello` is this example with it where the echo is.
- **More than one core.** One thread, as the design's programs are today; a second core is a second process on the
  same port, which `tcp_listen`'s `SO_REUSEPORT` flag allows, not tried.
- **A flood from many addresses** was not run; the bounds are per process, not per peer. **Corrected
  ([`conn-peer.md`](conn-peer.md)):** `std.conns` now gives the address (`conns.peer`), and `examples/tls_echo` has the
  per-address bounds (`--per-address`, `--per-address-rate`, `front.cho`), tested with clients from two addresses
  (`scripts/tls_echo_test.py`: `per-address`, `addr-rate`). A flood from *many* addresses against the table of
  1,024 keys is still not run.
- **Cranelift**: the tests in CI build the example with the LLVM backend. Built with Cranelift by hand on darwin-aarch64,
  it passes the same 8 cases (`many` in 3.4 s against LLVM's 0.8); its cost is not measured.
- **Not independently reviewed (#209)**, as the engine.

## 12. Session tickets: the design

*Step 5 of §8, issue #379, part of #378. Written before the code. The sizes are arithmetic from the layout below and are
checked by the build (§12.10); no cost is claimed here, §12.9 holds what is measured. Where building finds this false, the
section that said it is corrected in place.*

**What it buys.** A full handshake costs 3.0 to 5.5 ms of CPU, of which the certificate signature and its check are 2 to 3.5
ms (§6, §11.4). A client that comes back should not pay them again. A resumed handshake still does one key exchange and
no signature: the same shape as the client's option A (`docs/tls-resumption.md` §2), seen from the server.

### 12.1 The ticket: stateless, sealed

A ticket is the server's own state, handed to the client and read back (RFC 8446 §2.2, §4.6.1; RFC 5077's idea). The server
keeps nothing per ticket, so a restart, a second process and a second host resume each other's tickets when they hold the
same ticket key and the same identity (§12.3).

```
ticket  = key_name (16) || salt (32) || AEAD(subkey, nonce = 0, aad = version || key_name || salt, plaintext) || tag (16)
subkey  = HKDF-Expand(HKDF-Extract(salt, ticket_key), "cancho tls ticket v1", 32)
plaintext (version 1) =
    version (1) = 1          the format; a ticket of another version is not read
    auth (1) = 0             how the client was authenticated: 0 none (§12.5 g)
    issue_ms (8)             the engine's clock when the ticket was made
    expiry_ms (8)            issue_ms + lifetime, never past the certificate's notAfter
    age_add (4)              this ticket's ticket_age_add, so the client's obfuscated age can be undone
    suite (2)                the cipher suite of the connection
    hash_len (1), psk        the resumption PSK of this ticket, 32 or 48 bytes
    fp (32)                  SHA-256 of the leaf certificate (DER) of the identity that issued it
    sni_len (1), sni         the host name the client sent, lowercased, as the engine kept it (0 bytes: none)
    alpn_len (1), alpn       the protocol chosen, if any
```

- **The AEAD is ChaCha20-Poly1305** (`std.chacha20`): constant time in software on every CPU, so a ticket opens in the same
  time whatever the key, and the same code on both backends.
- **A nonce that cannot repeat: no nonce is shared.** The 12-byte nonce is zero, and the key is not: every ticket has its
  own subkey, derived from the ticket key and 32 fresh bytes (`salt`) of the engine's DRBG. A (key, nonce) pair then
  seals one message because the key is new each time, and two tickets share a subkey only if two draws of 256 bits
  agree. This is why a counter was rejected: a counter in each process collides across the processes of §12.3, and a
  96-bit random nonce under one shared key reaches a 2^-32 collision after 2^32 tickets, which a busy fleet makes in
  days, and a repeated ChaCha20-Poly1305 nonce gives away the Poly1305 key and the XOR of two PSKs. The cost is one
  HKDF-Extract and Expand (a few HMAC-SHA-256 blocks) a ticket.
- **The ticket is 64 bytes of overhead and at most 681 bytes in all**: 16 + 32 + (1+1+8+8+4+2+1+48+32+1+255+1+255 = 617)
  + 16 = 681 at the largest (a 255-byte name and protocol, SHA-384); typically a name of 20 bytes and `h2`: about 250.
  The client keeps tickets of up to 2,048 bytes (`docs/tls-resumption.md` §4), so every ticket fits.
- **What the client sees** is opaque bytes. It learns the lifetime, `ticket_age_add` and a nonce (the index of the
  ticket on the connection) in the clear, all of which it knows anyway.
- **The PSK** is RFC 8446's: `HKDF-Expand-Label(resumption_master_secret, "resumption", ticket_nonce, Hash.length)`, with
  `ticket_nonce` the one byte index of the ticket on the connection, so tickets of one connection have different PSKs.
  Storing the PSK in the ticket, not the resumption master secret, means a ticket opens to exactly one thing and a stolen
  ticket's contents give nothing about its siblings.

### 12.2 The ticket key, and its rotation

- **A key** is 32 bytes. Its **name** is the first 16 bytes of HMAC-SHA-256(key, "cancho tls ticket key name v1"), so the
  name is a function of the secret and every host given the same key computes the same name; nothing else needs to be
  agreed.
- **The engine holds up to 4 keys:** one *current*, which seals, and up to 3 *previous*, which only open. A previous key
  has an `open_until` of the moment it stopped being current plus the ticket lifetime: a ticket it sealed is not valid longer
  than that anyway (the ticket carries its own expiry), so the key is dead weight afterwards and refuses (`unknown`).
  That is the lifetime bound: a key is useful for at most **two lifetimes** (one as current, one as previous).
- **The engine makes the key** when the program gives none: 32 bytes from its DRBG, the first time a connection is
  served with tickets on, and again every **lifetime** (so a ticket sealed at the end of a key's life is still opened
  during the next). The program calls `tls.set_time(engine, now_unix_ms)` or `serve`, and the engine rotates on its
  own clock. A key made this way is the engine's alone: **it is never shown**, as the identity keys are not (§4). So tickets
  made by it are good for this process only, and for a restart none survive.
- **The program supplies keys** for anything wider than one process: `tls.set_ticket_keys(engine, keys, now_ms)` takes
  `n` × 32 bytes (1 ≤ n ≤ 4), the first current and the rest previous. It is declarative: the engine reconciles with what
  it holds (a key already held keeps its history; a key not listed is overwritten; a listed key it did not hold opens
  tickets for one lifetime from `now_ms`), and a call with the keys it already has changes nothing, so a program may
  call it on every `SIGHUP`. A program that supplies keys turns the engine's own rotation off; rotating is then the
  operator's: prepend a new key to the file, keep the old ones for a lifetime, drop them. `tls.rotate_ticket_key(engine,
  now_ms)` is the single-process version: draw a key, make it current, keep the old as previous.
- **A fleet** gives every process the same file. The processes agree on the key name by construction (above), so one
  process's ticket opens in another, provided they also hold the same identity (§12.3). The ticket key is the only secret
  they share, and it protects tickets, not the sessions of the full handshake. **The file is as sensitive as an
  identity's key**: anyone who has the ticket key can make tickets, and so can make a client believe, for the lifetime, that
  it is resuming a session it never had. (They cannot read past sessions: a resumed connection does a fresh key
  exchange, §12.5 a.)

### 12.3 What a ticket is bound to, and what it is refused for

A ticket is a stored verdict that a full handshake happened, as the client's tickets are (`docs/tls-resumption.md` §3):
every way the verdict could be stale is a rule. The model is the client's eight rules; the server's are these. **A ticket
that fails any rule is not an error: the server ignores `pre_shared_key` and does a full handshake**, which is always
possible and always safe, and the connection's *verdict* says why (§12.7). The exceptions are the two failures RFC 8446
demands as aborts, below.

| | rule | the client's rule it mirrors | the decision |
|---|---|---|---|
| **a** | **psk_dhe_ke only.** The ClientHello's `psk_key_exchange_modes` must list `psk_dhe_ke` (1). A hello that lists only `psk_ke` (0) is a full handshake; `pre_shared_key` without `psk_key_exchange_modes` is `missing_extension` (RFC 8446 §4.2.9: a MUST). The ServerHello always carries a `key_share`. Tickets are sent only to a client that listed `psk_dhe_ke`, as Go's and rustls's servers do (§4.2.9 says a server SHOULD NOT send tickets the client's modes cannot use). | rule 8 (the ServerHello must carry a `key_share`; a resumption without one is refused) | A fresh X25519, P-256 or P-384 exchange on every resumption: forward secrecy is kept; the ticket key and the PSK together do not give a resumed session's keys. |
| **b** | **Never early data.** `early_data` in a ClientHello that offers a ticket is accepted as an extension and *not accepted as 0-RTT*: EncryptedExtensions has no `early_data`, and the client's early records are skipped by trial decryption (RFC 8446 §4.2.10), up to 16 KiB, as §5.2 already does for a full handshake. A NewSessionTicket never carries `early_data` (`max_early_data_size`), so no compliant client sends any. | rule 6 | |
| **c** | **Lifetime and age.** `expiry_ms` is `issue_ms + lifetime`, capped at the leaf's notAfter; a ticket is refused (`expired`) at or after it. The ClientHello's `obfuscated_ticket_age` minus the ticket's `age_add`, mod 2^32, is the age the client claims in ms; the server's own age is `now - issue_ms`; they must agree to within **30 s** (`age`). A ticket from the future (another host's clock ahead) is the same test: its server age is negative and the two ages differ by more than the window. The lifetime sent is the setting (default 1 hour) capped at 7 days (RFC 8446 §4.6.1) and at the certificate's remaining life. | rules 3 and 4 | The window is wide because the client's age is counted from when it *received* the ticket (this package's client counts from the start of the connection that got it: `docs/tls-resumption.md` §11) and the server's from when it *issued* it, and the engine's clock is the one the program last gave it. It is not a replay defence (§12.4), only a freshness one. |
| **d** | **Bound to the name, the identity, the suite.** The ticket carries the host name the client sent and a SHA-256 of the issuing identity's leaf certificate. A resumption must send the same name (`name`), and the identity that name selects now must have that same certificate (`identity`): **`replace_identity` with a different certificate refuses every ticket of that identity**, because the fingerprint changes; with the same certificate (a reload that found nothing new) it refuses none, and tickets stay good across a process restart that loads the same files. The suite chosen for the new connection must hash as the ticket's (`suite`, RFC 8446 §4.2.11: the PSK's hash must be the cipher suite's); the ALPN protocol the new connection chooses must be the one the ticket records (`alpn`). | rules 1, 2, 3 (name, trust store, certificate) | The fingerprint, not an identity number or a generation counter: a counter is per process and would make §12.3's fleet refuse its own tickets, and a number can be reused. The certificate's *chain* is not part of the fingerprint: the leaf is what the client verified the name against, and a changed intermediate is the operator's affair (§4). |
| **e** | **Replay.** §12.4. | rule 5 (used once) | A ticket is **not single use** on the server; the argument and the bound are in §12.4. |
| **f** | **The binder, in constant time.** The binder is recomputed over the transcript through the ClientHello's `pre_shared_key` truncated before its binders (RFC 8446 §4.2.11.2) and compared with every byte examined, as the client's Finished is (§10.1). A ticket that opened and passed the rules above but whose binder is wrong aborts the handshake with `decrypt_error` (`tls-server-binder`), as RFC 8446 §4.2.11.2 says, after a HelloRetryRequest too (the transcript then holds it). *OpenSSL aborts too, but with `illegal_parameter` (47); the differential, §12.12, records it.* | the client's own binder check, which the client does not need | An *unopenable* ticket is no abort (the server cannot know it was ours): a full handshake. Only a ticket the server authored and a binder that does not match are an abort: nothing a bystander can use as an oracle, since opening the ticket needs the ticket key. |
| **g** | **No client certificate yet.** A resumed session skips every step of a full handshake that is not the PSK's proof, among them the *client's* certificate (#384). The ticket's `auth` byte is 0 today because this server asks for none. **When #384 lands, a connection whose client authenticated carries a ticket only if the program asks for it, and the ticket then holds the client's certificate fingerprint and `notAfter`, is refused after the client trust store changes, and its `auth` is 1.** Until then a ticket with `auth` ≠ 0 is refused (`auth`), so an older server that meets a newer server's ticket in a fleet never resumes a connection the client authenticated and the older cannot represent. | rule 2 (trust store) and the "verify-nothing" hazard of `docs/tls-resumption.md` §3 | The safe default for #384 is **no tickets on authenticated connections**; the conservative path costs a client-authenticated peer a full handshake (the signature is the server's, which is the cost this document removes, but the client's certificate chain verification is the other half, and it too is saved only if the ticket carries the verdict). |
| **h** | **How many, when, and KeyUpdate.** §12.6. | | |

### 12.4 Replay (rule e)

**What RFC 8446 requires.** Replay protection is a requirement of *early data*: §8 ("Replay Attacks on 0-RTT") exists because
0-RTT data is accepted before the handshake completes and can be delivered twice. The three defences it describes
(single-use tickets, a ClientHello record, freshness checks) are for a server that accepts early data. A server that
**never accepts early data and only accepts `psk_dhe_ke`** has no data that a replay can deliver twice. Section 8 asks it for
nothing further, and §4.6.1 does not require a server to make tickets single use (it asks the *client* to use each once,
Appendix C.4, to avoid linking).

**Why a replayed resumption is harmless.** An attacker who records a ClientHello with a valid `pre_shared_key` and sends it
again can make the server do three things: open the ticket, compute a key exchange against the client's *old* key share,
and send its flight. It cannot continue: the handshake keys depend on the DHE secret, which needs the client's ephemeral
private key, so the attacker can neither read the server's EncryptedExtensions nor produce the client Finished, and the
server's flight is encrypted to a key only the original client could derive. The server then waits for a Finished that never
comes. Nothing was authenticated, no application data was exchanged, no state changed. What the attacker gains is **one key
exchange of the server's CPU a replay** (about 1.1 ms; a fresh full ClientHello costs the attacker nothing more and the
server a signature besides, so the replay is cheaper for the server than the attack the program must already bound): it is
inside the handshake bounds the program sets (§7: `tls.handshakes_in_progress`, the rate, the per-address limits).

**The bound.** (1) The ticket's expiry (`lifetime`, 1 hour by default) bounds the whole ticket. (2) Because the ClientHello
repeats the original age, a *captured* ClientHello is accepted only while the server's own age is within **30 s** of the
age it claims: after that, `age` refuses it, so a recording is replayable for 30 seconds, not for the lifetime. (A client
that reuses a ticket gives a fresh age each time and is accepted until expiry: that is a client reusing a ticket, which the
packages/tls client does not do, and which the server cannot tell from a replay and does not need to.) (3) A program that
wants single use can keep state of its own, a set of binder values seen in the window, and refuse a repeat before `serve`
(the binder is in the ClientHello it reads); the engine keeps none, because state shared by a fleet is exactly what a
stateless ticket exists to avoid. **Tested** with a lying client that replays one ticket N times, and a captured
ClientHello twice, in and out of the window (§12.8).

*What this does not claim:* a stolen ticket **key** is a different attack (§12.2), and a replay is not a denial-of-service
defence: that is the program's bounds.

### 12.5 The refusals that are not fallbacks, and the tags

The ones that end the connection, each a `tls-server-` tag and the alert RFC 8446 names, all reached by a lying-client
case:

| tag | alert | when |
|---|---|---|
| `tls-server-binder` | decrypt_error (51) | the ticket opened and passed the rules, and the binder does not match (RFC 8446 §4.2.11.2) |
| `tls-server-missing-extension` (existing) | missing_extension (109) | `pre_shared_key` without `psk_key_exchange_modes` (§4.2.9) |
| `tls-server-client-hello-format` (existing) | decode_error (50) | `pre_shared_key` or `psk_key_exchange_modes` that does not parse; a number of binders that is not the number of identities is `tls-server-illegal-parameter` (47) |

The configuration calls answer `tls-server-ticket-config` (a count over 8 or a lifetime outside 1 s to 7 days) and
`tls-server-ticket-key` (a key list that is empty, not a multiple of 32 bytes or over 4 keys), on the line, as
`set_alpn`'s `tls-server-alpn-list` does.

### 12.6 Number of tickets, NewSessionTicket, KeyUpdate (rule h)

- **Count.** `count` tickets (default 2, as OpenSSL; 0 turns tickets off; at most 8) are sent on each connection that
  listed `psk_dhe_ke`, after the client's Finished has been checked (RFC 8446 §4.6.1: a NewSessionTicket is a
  post-handshake message, under the server's application key), in the same turn as the check, so a client that starts to
  read finds them. A resumed connection is sent `count` new tickets too (a new ticket per connection keeps the client's use
  of a ticket single, as the client does). They are not sent on a connection that failed.
- **`event` says `want_write` while the engine has output, so a connection that ends its handshake with tickets queued
  does not report `established` until they are taken.** *Found by the interop run, not by design:* `tests/programs/tls_serve.cho`
  answered a request only when `event` said `established` after a read, and a client that sent its Finished and its request in
  one segment (OpenSSL does) hung in about one connection in six with tickets on and never without. The engine is as
  documented (`docs/tls-pure.md` §2.2: the event once the output is taken); the program, which has data waiting, must not
  wait for the event (the harness now answers whenever it has data), and `examples/tls_echo` and `examples/https_hello`, which
  flush before they look at the event, never had it: 1,440 connections of 120 clients at once, 1,320 resumed, no error.
- **A connection that never reads them** costs `count` × about 250 to 700 bytes of the slot's output, which `feed` already
  bounds (it stops reading while less than 1 KiB is free).
- **KeyUpdate.** A ticket is made once, at the end of the handshake, under the server's first application key; it does not
  depend on a later key. A KeyUpdate from the client changes the keys of records after it, not the ticket, which is already
  queued under the old; a NewSessionTicket is never sent after one. No interplay.
- **Settings:** `tls.set_tickets(engine, count, lifetime_s)`: count 0 is off (the default, so a program that has not asked
  sends none, and the 110 recorded connections of §10.3 are unchanged), lifetime 3,600 s unless set.

### 12.7 The interface and the engine's verdict

```
tls.set_tickets(engine, count, lifetime_s) -> 0 | tls-server-ticket-config  // server engines: tls-role otherwise
tls.set_ticket_keys(engine, keys, now_ms)  -> 0 | tls-server-ticket-key      // n x 32 bytes, first current (§12.2)
tls.rotate_ticket_key(engine, now_ms)      -> 0 | tls-no-entropy             // draw a key, make it current
tls.set_time(engine, now_ms)               -> 0                              // the engine's clock for tickets (§12.3 c)
tls.ticket_keys(engine)                    -> int                            // how many keys open tickets now
tls.resumed(engine, slot)                  -> bool                           // as for a client
tls.ticket_verdict(engine, slot)           -> int                            // why a connection did or did not resume
tls.ticket_verdict_tag(verdict)            -> "tls-ticket-..."
```

The engine keeps no clock of its own (§5.3): **tickets are issued and checked at the time the program last gave it**,
through `serve`, `set_time` or `set_ticket_keys`. A program with connections that last longer than the lifetime calls
`set_time` each turn of its loop, as the example does; otherwise a ticket's issue time would be the start of its
connection. The verdict is a small integer with a tag, for logs, tests and the differential; it is not a failure. The
verdicts, in the order the rules are tried (§12.3): `resumed`, `off`, `no-psk-dhe-ke`, `unknown` (not our key, a key past its
`open_until`, or a size no ticket has), `tampered` (our key, and the AEAD says no), `format`, `auth`, `expired`, `age`,
`name`, `identity`, `suite`, `alpn`, and `none` for a hello with no `pre_shared_key`. At most the **first 4 identities** of
a `pre_shared_key` are tried, a ticket of more than 681 bytes is `unknown` without being opened, so a hello cannot make
the server do more than 4 small AEAD opens.

### 12.8 How it will be tested

- **Unit and conformance, both backends:** the ticket module sealed and opened (every field, every rule), run through
  `tests/programs/tls_server_driver.cho`'s new lines and recorded with the rest of `liar_client.txt`
  (`crates/cancho/tests/conformance/tls_server.rs`).
- **The lying client** (`scripts/tls_liar_client.py`, one case per rule): an expired ticket, a wrong binder, a wrong name, a
  wrong suite hash, a tampered and a truncated ticket, a ticket from a rotated-out key, a ticket after an identity was replaced
  (and after it was reloaded unchanged), an early-data attempt with a good ticket, `psk_ke` offered, `pre_shared_key` without
  modes, a ticket replayed N times and a ClientHello replayed in and out of the age window, a ticket of 64 KiB, a wrong age, an
  honest resumption (every suite, each group, after a HelloRetryRequest). Every refusal reached, every verdict reached.
- **Interop** (`scripts/tls_server_interop.py`): OpenSSL (`s_client -sess_out/-sess_in`), curl, Go, wolfSSL, GnuTLS,
  rustls, mosquitto if its clients resume, and `packages/tls`'s own client resumes against the server.
- **Differential** against `openssl s_server`: the same resumption ClientHellos, the outcome (resumed or not, the alert)
  compared.
- **Fuzz:** the corpora gain ClientHellos with a ticket; `fuzz_hello` and `fuzz_server` run them.
- **Mutants:** `scripts/tls_server_mutants.py` gains one mutant a decision above.
- **Cost:** a resumed handshake against a full one, on x86-64 and arm64 (§12.9).

### 12.9 Cost

**Measured** (the expectation written before the build was a resumed handshake of about one key exchange, 1.1 to 1.5 ms,
against 3.0 to 5.5 ms). `python3 scripts/tls_echo_test.py <tls_echo> --cost 6 --tickets --rounds 7`: the server of
`examples/tls_echo` (LLVM backend, one thread, a P-256 identity, AES-128-GCM chosen by the server on both machines), a Python
`ssl` client making connections one at a time for 6 s, the server's CPU (user and system, `/proc/<pid>/stat` or `ps`) divided
by the handshakes its own `established` lines count. Three rows, **alternating, 7 rounds each**, so a machine whose speed
changes over the minutes moves all three together; the median and the best of the 7:

| | full, tickets off | full, 2 tickets sent | **resumed** | resumed / full |
|---|---|---|---|---|
| **arm64**: Apple M4 Max, macOS 26.2, native; the host busy with other builds (load average 11 to 17) | 3.15 ms (best 3.07) | 3.20 ms (best 3.12) | **1.52 ms** (best 1.50) | **0.48** |
| **x86-64**: Intel i7-1260P, Linux, pinned to **core 6** (`taskset -c 6`), the box shared (load average 4 to 10) | 9.74 ms (best 5.47) | 9.63 ms (best 6.82) | **3.46 ms** (best 2.91) | **0.35** of the median, 0.53 of best to best |

- **A resumed handshake costs about half a full one on arm64 (1.52 against 3.15 ms), and about a third to a half on x86-64.**
  What it still pays is the key exchange (the X25519 key pair and shared secret, 1.14 ms on the M4 of `docs/tls-resumption.md` §1)
  and the key schedule, the records and the transcript: 1.5 ms, inside the 1.1 to 1.5 expected.
- **Issuing two tickets on a full handshake costs 1.6% on arm64** (3.20 against 3.15 ms) **and nothing a noisy x86-64 box can see**
  (9.63 against 9.74; the best of 7 is 6.82 against 5.47, which is the box).
- **The x86-64 figures move more than a factor of three between runs**: five single runs of the same rows (an earlier form of the script, with `openssl s_time -new` for
  the full handshakes), minutes apart, gave a full handshake of 19.6, 19.9, 12.5, 5.2 and 5.8 ms and a resumed one of 8.3, 7.0,
  4.7, 3.2 and 2.5 while other jobs on the box came and went (load average 4 to 10 on 16 cores; the other cores are theirs),
  and CI's runner measured 5.5 ms for a full handshake of the same code in §11.4. The ratio ranged from 0.35 to 0.62 and the
  absolute figures are the box's; the quiet runs, 5.2 and 5.8 ms full against 3.2 and 2.5 resumed, are those of CI. The row
  `tls_serve` gives (`python3 scripts/tls_server_cost.py <tls_serve> 6 --tickets --rounds 9`, whose loop costs more than the
  example's, §6) is the same shape: X25519, full 14.7 ms median (best 7.5), 15.4 with tickets (best 7.6), resumed 6.3 (best 3.3),
  0.43; P-256, full 21.3 (best 15.6), resumed 7.0 (best 2.8).
- **Not measured**: the Docker linux-aarch64 of §6 and §11.4 (its disk was full of other jobs' images when the run was due);
  the Cranelift backend; a resumption through a HelloRetryRequest (it costs one more message hashed, §6).

### 12.10 Memory

*Per slot: nothing new.* A server slot reuses the client's resumption fields it never uses (the PSK, the resumption master
secret, the ticket's randomness area), all of which `forget` already overwrites. *Per engine:* the ticket keys and settings,
4 keys of 72 bytes and 8 bytes of header, **296 bytes**, allocated by `open_server` only; and an identity gains its
certificate fingerprint and `notAfter`, 40 bytes each, 640 bytes for 16. A ticket needs no storage on the server.

### 12.11 What was built

| File | What |
|---|---|
| `packages/tls/ticket.cho` (`tls_ticket`) | the ticket (§12.1): sealed and opened; the ring of 4 ticket keys, `make_current`, `set_keys`, the engine's own rotation (`due`); the rules (§12.3) as one function, `check`; the settings; the verdicts and their tags |
| `packages/tls/hello.cho` | the ClientHello's `pre_shared_key` and `psk_key_exchange_modes` parsed (`ch_psk_*`, `psk_entry`), the ServerHello's `pre_shared_key`, the NewSessionTicket encoded |
| `packages/tls/server.cho` | `try_resume` (the identities tried, the rules, the binder), the key schedule from the PSK, a flight with no Certificate, `issue_tickets` after the client's Finished |
| `packages/tls/identity.cho` | an identity records its leaf's SHA-256 and `notAfter` |
| `packages/tls/tls.cho` | `set_tickets`, `set_ticket_keys`, `rotate_ticket_key`, `set_time`, `ticket_keys`, `ticket_verdict`, `ticket_verdict_tag`; the engine's clock; `resumed` answers for a server slot |
| `packages/tls/record.cho`, `slot.cho` | three tags (`tls-server-binder`, `-ticket-config`, `-ticket-key`) and the alert of the first |
| `examples/tls_echo`, `examples/https_hello` | `--tickets <n>`, `--ticket-lifetime <s>`, `--ticket-keys <file>`; `SIGHUP` reads the key file again; `tls.set_time` each turn of the loop; `resumed=yes|no` on the `established` line |

`server.cho` is 880 lines, `ticket.cho` 655, `tls.cho` 1,025; no file passes 2,000.

### 12.12 Tests and results

All commands from the repository root; the Linux ones in the image of §10.2 (Ubuntu 24.04, linux-aarch64, Docker on the Apple
M4 Max), the x86-64 ones on a Linux box (an Intel i7-1260P, `taskset -c 6`).

- **The lying client** (`python3 scripts/tls_liar_client.py <server driver> tests/vectors/tls/liar_client.txt`):
  **183 connections**, the 110 of §10.3 unchanged byte for byte (but the role case, which gained four calls) and **73 new**, in
  `scripts/tls_liar_tickets.py`: 55 end `ok` (resumptions, and fallbacks each checked against the verdict the engine gave:
  `U` in the driver) and 18 end in a refusal or a configuration answer (6 `tls-server-binder`, 6
  `tls-server-client-hello-format`, and one each of `-finished`, `-illegal-parameter`, `-missing-extension`,
  `-ticket-config`, `-ticket-key` and `tls-no-entropy`), listed below by rule. Recorded; `conformance/tls_server.rs`
  replays every one on both backends, and the recording made on x86-64 is identical to the one made on arm64.

  | rule | cases |
  |---|---|
  | honest | a full handshake sent 2 tickets with distinct PSKs; resumed under AES-256-GCM (SHA-384, a 48-byte PSK), ChaCha20 and AES-128, with X25519 and P-256, after a HelloRetryRequest, for the second identity with ALPN, the good ticket second of three and fifth of five (only four are tried), one ticket used three times, a chain of resumptions, early data offered and skipped, a ticket cut to the certificate's remaining life, one sealed by the lying client itself in §12.1's format (a cross-implementation check of the format and the key schedule), the clock moved during a long connection |
  | a (psk_dhe_ke only) | `psk_ke` only: full, no ticket sent; no modes at all: none sent; `pre_shared_key` without modes: `missing_extension`; an empty mode list |
  | b (no early data) | the early-data attempt above; a NewSessionTicket never carries an extension (asserted for every ticket the client reads) |
  | c (lifetime, age) | expired, at exactly the expiry, one millisecond before; age 60 s high, 31 s low, 29 s off; from a clock 60 s ahead; a lifetime cut by the certificate |
  | d (name, identity, suite) | another name, no name, `replace_identity` with a new certificate (refused) and with the same one (kept), the SHA-384 offered a SHA-256 ticket, another ALPN protocol, a made-up fingerprint |
  | e (replay) | one ticket three times; a captured ClientHello replayed inside the window (resumed) and 60 s later (`age`) |
  | f (binder) | wrong first byte, wrong last byte only, one byte too long, wrong PSK, after a HelloRetryRequest with the retry left out of the transcript, a forged ticket with a wrong binder; and a wrong binder on a ticket that fails a rule is *not* an abort |
  | keys | previous key opens, a dropped key does not, the same keys given again change nothing, a ring of four (the fifth rotation evicts the oldest; three leave it), the end of a previous key to the millisecond, the engine's own rotation after exactly a lifetime, supplied keys never replaced by the engine's |
  | tampering | one bit of the text, the tag, the salt; truncated by 10 and to 40 bytes; the key name; 15,000 bytes; one byte appended; a key holder's ticket of version 2, with trailing bytes, a 20-byte PSK, `auth` 1 |
  | settings | count 9, lifetime 0 and 8 days, keys of 0, 31, 33 and 160 bytes refused; rotation before the engine is seeded; the ticket calls on a client engine (`tls-role`); the count raised mid-handshake |
  | malformed | identities and binders of different numbers, an identity running past its list, an empty identity (alone and first of two), a binder of 31 bytes (alone and two) |

- **Mutants** (`python3 scripts/tls_server_mutants.py target/release/cancho`): **143 of 143 killed**, none argued equivalent:
  the 71 of §10.6 and 72 new, one per decision of §12.3, the key ring, the parser and the settings. A first run killed 140 and
  left three, each a gap in the cases and not in the code: a binder of 31 bytes was caught by the list's own minimum before the
  per-binder one; an empty identity alone was too short for the list's; and the "binder too long" case had a wrong binder
  anyway, because **the lying client's own binder was computed over a header one byte short** (a bug in the test, found by the
  mutant that survived it). Each got a case that only its mutant's line can fail. `scripts/tls_mutants.py` (the client's) still
  kills **103 of 103**.

- **Interop** (`python3 scripts/tls_server_tickets_interop.py <tls_serve> --many <tls_many>`, `tls_serve` with tickets on):
  16 rows, 16 ok:

  | client | rows |
  |---|---|
  | `openssl s_client -sess_out` / `-sess_in`, OpenSSL 3.0.13 | AES-128, AES-256 and ChaCha20 with X25519; AES-128 with P-256 and P-384; a HelloRetryRequest first: each resumes (`Reused, TLSv1.3`) |
  | curl 8.5.0 | two URLs in one run resume |
  | Go `crypto/tls` 1.22.2 | 3 connections: full, resumed, resumed |
  | wolfSSL 5.6.6 | the same |
  | GnuTLS `gnutls-cli --resume` 3.8.3 | resumes |
  | rustls 0.23 on ring | full, resumed, resumed (`handshake_kind`) |
  | mosquitto 2.0.18 | **does not resume**: libmosquitto never sets a session on its SSL object; two `mosquitto_sub` connections are both full, and the row asserts it, so a version that resumes fails it and says so |
  | `packages/tls`'s own client (`tls_many resume`) | 8 connections, then 8 resumed |
  | two server processes sharing a key file | a session from A resumes at B; at a third process with another key, a full handshake with the verdict `unknown` |
  | a ticket of 2 s lifetime, 3.5 s later | a full handshake: OpenSSL's client does not offer an expired session (`none`); a client that did is the lying client's `expired` |

  The example's own live tests (`scripts/tls_echo_test.py`, `scripts/https_hello_test.py`, §12.13) add Python's `ssl`.

- **The differential** (`python3 scripts/tls_server_tickets_differential.py <tls_serve>`, against `openssl s_server`
  3.0.13 with the same identity and tickets on): the same client behaviour against each server's own tickets, 12 cases:
  **9 agree, 1 differs in the alert only, 2 differ as `EXPECTED` says.** They agree on an honest resumption, a ticket used
  twice, `psk_ke` only, **`pre_shared_key` without `psk_key_exchange_modes` (both abort, `missing_extension`)**, a ticket with one
  bit changed, a truncated one, one of 15,000 bytes, early data offered (both resume and skip it), and a good ticket second of
  two behind a made-up one. A wrong binder: this server `decrypt_error` (51, as RFC 8446 §4.2.11.2 says), OpenSSL `illegal_parameter` (47). The two
  that differ on purpose: OpenSSL resumes a ticket for another host name (it does not compare it; this server does), and one
  whose claimed age is 60 s off (OpenSSL does not check the age without early data; this server does, within 30 s).
  `scripts/tls_server_differential.py` (the ClientHello cases of §10.4) is unchanged: 59 agree, 7 alert, 6 as documented,
  skipping the ticket cases, which are several connections.

- **Live tests of the examples** (`scripts/tls_echo_test.py`, 4 new cases of 15, all ok; `scripts/https_hello_test.py`, 1 new of 20; and
  `conformance/tls_echo.rs`, `https_hello.rs` against `packages/tls`'s own client): a session kept by Python's `ssl`
  resumes three times from one session; **a session from one process resumes at another that holds the same key file, not
  at one with another**; a rotation by `SIGHUP` (a new key on top) keeps the old session, a second rotation that drops its key
  refuses it, a file that is not hex is refused (`reload tickets refused tls-server-ticket-key`) and the keys stay; **a
  certificate renewed and loaded by `SIGHUP` refuses the tickets of the old one and a reload of the same certificate keeps
  them**; a session offered after its 2 s lifetime does not resume.

### 12.14 Where this differs from the design, and what building it found

- **The ticket is 681 bytes at most**, not 705: the arithmetic of §12.1 was right and its first sum wrong (corrected in place).
- **A ticket from the future is not a separate test.** The first draft had `age < -window` beside the difference of the two
  ages; the mutant that removed it survived, because with a client age that is never negative the difference alone is more
  than the window whenever the server's age is less than minus the window. The clause was dead code and is gone.
- **`event` and the tickets' output** (§12.6): the interop run hung one connection in six in the test harness, not in the
  engine; the harness is fixed and §12.6 says so.
- **The wrong-binder alert** is `decrypt_error` here and `illegal_parameter` in OpenSSL (the differential, above); §12.3 f says
  which the RFC names.
- **The lying client's own binder was wrong** by one byte in its first version of the padded case (§12.12). The server was
  right to refuse; it was refusing for the wrong reason, and only a mutant that removed the right reason showed it.
- **`tls.ticket_keys`** was not in the design's interface; the program that supplies keys wants to log how many open.
- **mosquitto does not resume** (§12.12), against the issue's hope that it would; it is the client's, not this server's.

### 12.15 Not done, and not verified

- **A resumption count or a rate limit.** A ticket resumes as often as its client offers it, until it expires (§12.4); the
  engine counts nothing. A program that wants a single-use ticket keeps the binders it has seen, or does not send tickets.
- **Client certificates (#384).** Tickets carry `auth` 0 and refuse anything else (§12.3 g); what a ticket of an
  authenticated client holds is #384's design.
- **P-384, Ed25519 and RSA identities** (step 7): a ticket binds a leaf's SHA-256, which is key-type independent, so
  nothing here changes; not exercised.
- **0-RTT**: refused, not deferred (§2.2).
- **TLS 1.2 tickets (RFC 5077)**: step 6, if it is ever built.
- **Browsers**: Chrome and Firefox were not run, here as in §10.9; their ClientHello carries `psk_key_exchange_modes` and
  resumes with the standard extension, and the interop matrix has OpenSSL, curl, Go, wolfSSL, GnuTLS and rustls.
- **Timing of the ticket path**: no dudect test. The AEAD tag and the binder are compared with every byte examined; the ticket
  key is never an index or a branch; the rules compare public values. The key schedule from a PSK is the client's.
- **Independent review**: none (#209), as the engine.
- **A long soak.** The fuzzers ran for the times in §12.12; no multi-day run.
