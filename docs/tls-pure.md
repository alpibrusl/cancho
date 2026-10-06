# A TLS 1.3 client with no C library: the design

> **Status: design, before any protocol code** (#198, sub-issue 1 of #197). Two primitives it needs are already built, under the
> epic's provisional placement rule (§1): ChaCha20-Poly1305 (`docs/chacha20.md`, #212) and SHA-384/HMAC/HKDF (`docs/hkdf.md`, #229).
> Everything else here is a decision about code that does not exist yet. Each decision gives the alternatives and the reason.
> Each claim about existing code names the file it was read from. The things a person has to decide are listed in §10, not
> settled by default.
>
> **Nothing built from this design may be called production-ready before #209's independent review.** Until then
> `cancho-hooks` keeps the OpenSSL backend (`docs/tls-nonblocking.md`) as its default.

---

## 1. Where the code lives

### 1.1 The decision

| What | Where | Status |
|---|---|---|
| ChaCha20-Poly1305 | `std/chacha20.cho` | built (#212) |
| SHA-384, streaming SHA-2, HMAC, HKDF, Expand-Label | `std/crypto.cho`, `std/hmac.cho`, `std/hkdf.cho` | built (#229) |
| X25519 | `std/x25519.cho`, sharing a field module with `std/ed25519.cho` | built (#231) |
| a modular bignum, RSA verification | `std/bigmod.cho`, `std/rsa.cho` | built (#234) |
| ECDSA P-256/P-384 verification | `std/ecdsa.cho`, on `std/bigmod.cho` | built (#239) |
| DER, X.509 parsing, chain building, name matching | **`packages/x509`** | parsing built (#233); the rest #206 |
| the handshake, the record layer, the key schedule | **`packages/tls`** | #205 |

**Primitives with fixed test vectors stay in `std`. Protocol and policy go in packages.**

### 1.2 The alternatives and their costs

| | everything in `std` | **the split (chosen)** | everything in packages |
|---|---|---|---|
| how a change ships | a compiler release, since `std/` is compiled in (`STD` in `crates/cancho/src/main.rs`, one `include_str!` per file) | primitives with the compiler; protocol and policy on their own schedule | on their own schedule |
| who must move a pin | every user of the change: `cancho-hooks` pins one compiler commit (`cancho.toml`, `[package] cancho`), so a root-store or cipher-policy fix becomes a coordinated two-repo bump | a CVE fix in the handshake or in name matching is a package bump; a primitive fix is a compiler bump | a package bump |
| test-vector and differential checks | in `cargo test`, offline (`crates/cancho/tests/conformance/aead.rs`, `kdf.rs`) | the same for primitives; packages carry their own harnesses (as `examples/tls_nb/test/` does) | each package must build its own harness |
| what other programs get | everything | primitives anyone can use: `cancho-hooks` already moved its HMAC to `std.hmac` (cancho-hooks#23) | packages they must find and pin |
| cost when unused | nothing (`std_declarations_cost_nothing_unless_called`, `docs/crypto.md` §6) | nothing | nothing |

**The cost #197 named, written down.** Every primitive in `std` is a compiler change, and each fix to one needs a coordinated
compiler bump in every consumer. Two things make that cost acceptable for primitives and not for protocol code:

- Primitives are fixed by their specifications and their vectors, so they change rarely. RFC 8439 and FIPS 180-4 do not move;
  hardening (constant-time work) changes how they compute, not their interface.
- Protocol policy changes often: cipher suites, root stores, name rules, CVE fixes. Its users need to take a fix without taking
  a compiler.

**One cost was paid already, and it argues for the split.** The 64 KiB trap of `docs/hkdf.md` §2 was a bug in `std/crypto.cho`. It
reached `cancho-hooks` through its compiler pin, and its fix reached hooks the same way, with no change to hooks' own code. A
primitive in `std` is shared, and so is its fix.

**What is *not* settled by this, and goes to §10:** whether the bignum and the curves (#203, #204) belong in `std` too. They will
change while they are hardened, which is the argument for a package. They have fixed vectors, and RSA/ECDSA verification is useful
outside TLS (`cancho-hooks` verifies nothing today), which is the argument for `std`. This document assumes `std`, in their own
files, so that a later move is mechanical.

---

## 2. The API of `packages/tls`: bytes in, bytes out

### 2.1 What it has to fit

The OpenSSL backend already chose **memory BIOs** over a descriptor (`docs/tls-nonblocking.md` §3.2, decision D1). Its program
reads and writes the socket and hands OpenSSL bytes. That *is* a sans-io interface: OpenSSL's `BIO_write(rbio)` is "bytes in",
and `BIO_read(wbio)` is "bytes out". So one interface fits both backends with no adapter, and a consumer (hooks' `attempt.cho`)
switches backend by dependency, not by code (#210's gate). *Corrected (#210, `docs/tls-hooks.md` §2.1 and §2.2): the byte
movement is the same, the interfaces are not. Hooks' `tls.cho` owns the socket I/O and a connection's state is the caller's
integers, where this engine owns its slots, so hooks needs an adapter module. And hooks' functions carry `Ffi` rows that one
source cannot also carry without them (`docs/effect-polymorphism.md`), so a pure build cannot share `attempt.cho` and
`hooks.cho` unchanged.*

The second constraint is how `cancho-hooks` holds connections. It keeps 64 attempts as **slots**: integer arrays indexed by slot,
the `Conn`s in a `std.conns.Table`, one `Poller` (`docs/tls-nonblocking.md` §4.2 and §10.3, read from `src/attempt.cho`). A
connection cannot be its own linear value inside a container, because containers hold only copyable things
(`examples/tls_nb/gaps/g10_thread_in_container.cho`, the same rule). So the engine owns its slots, as `rtcp.Resolver` does
(`examples/tls_nb/rtcp.cho`).

### 2.2 The interface

```
// Once, in main.
tls.open(heap, slots) -> Engine                           // owns every slot's state: one Box of ints, one of bytes
tls.trust(heap, engine, pem_bundle) -> (Engine, int)      // roots from a PEM bundle the caller read; count, or a refusal
tls.seed(engine, seed)                                    // 32 bytes of entropy the caller read (§6)
tls.close(heap, engine)

// Per connection, in slot s.
tls.start(engine, s, host, now_unix_ms) -> 0 | refusal   // ClientHello queued; `host` is checked against the certificate
tls.feed(engine, s, bytes) -> consumed | refusal         // bytes the socket gave; may complete handshake steps
tls.take(engine, s, out) -> n                            // bytes for the socket (0: none waiting)
tls.send(engine, s, plaintext) -> accepted | refusal     // once established: one record at a time
tls.recv(engine, s, into) -> n | 0 (close_notify) | refusal
tls.finish(engine, s)                                    // queue close_notify
tls.event(engine, s) -> want_read | want_write | established | closed | failed
tls.failure(engine, s) -> code;  tls.refusal_tag(code) -> name
tls.drop(engine, s)                                      // the slot is free (keys overwritten as far as the language allows, §7.3)
```

*As built (`docs/tls-core.md` §10.1), three differences:*
- *`tls.trust(engine, pem_bundle) -> int` borrows the engine rather than moving it;*
- *`tls.eof(engine, s)` tells the engine the socket ended: without a close_notify first, the connection fails
  `tls-peer-closed`;*
- *`tls.recv` answers `tls.would_block()` when nothing is waiting, as 0 already means close_notify.*

- **No capability is taken.** Time is a number the caller passes (`now_unix_ms`, from `clock_unix_ms`). Entropy is bytes the
  caller passes. The root store is bytes the caller read. So the pure backend's authority row is empty, and #210's gate (the
  pure backend needs no `Ffi` at all) is a property of the signatures that the checker enforces (rows are exact in both directions:
  `examples/tls_nb/gaps/a3_row_exact.cho`, `docs/tls-nonblocking.md` §6).
- **`event` is the only thing a poller loop needs.** It returns:
  - `want_write` when `take` has bytes;
  - `want_read` when the engine needs input;
  - `established` once the client's `Finished` is queued;
  - `closed` after a `close_notify`;
  - `failed` with a code.

  This is the `done | pending | failed` shape of `tls.handshake` in the spike (`examples/tls_nb/tls.cho`), with the direction
  made explicit.
- **The OpenSSL backend fits it.** `feed` = `BIO_write(rbio)` and then `SSL_do_handshake` or `SSL_read`. `take` =
  `BIO_read(wbio)`. `send` = `SSL_write` with partial writes. `recv` = `SSL_read`. `failure` maps (stage, detail) of
  `docs/tls-nonblocking.md` §3.6 onto the tags of §8. That backend takes `Ffi("tls")`, and the pure one takes nothing. That
  difference is the point.

### 2.3 Alternatives considered

| | why not |
|---|---|
| the engine owns the socket (`tls.connect(net, ...)`, as `SSL_set_fd` does) | it puts `Net` and the poller inside the package, it cannot be tested without a network, and it is the shape #211 measured and rejected for OpenSSL (`SIGPIPE`, §3.2 there) |
| one value per connection, returned and threaded like `buffer.Buffer` | 64 of them cannot sit in a container (§2.1), and hooks would need 64 named locals |
| callbacks (`on_read`, `on_write`) | a callback cannot capture the caller's state (`docs/function-values.md`), and a function value cannot be named across modules as `module.function` (`examples/tls_nb/gaps/g9_qualified_function_value.cho`) |

---

## 3. Cipher policy

### 3.1 The decision

| | offered | refused, with a tag |
|---|---|---|
| protocol | TLS 1.3 only (`supported_versions` = `0x0304`) | a ServerHello for 1.2 or below; the downgrade sentinel in `ServerHello.random` (RFC 8446 §4.1.3). *Corrected (#208): in a ServerHello for TLS 1.2 or below, as the RFC says; `docs/tls-assurance.md` §4* |
| cipher suite | `TLS_CHACHA20_POLY1305_SHA256` only. *Since #207: all three TLS 1.3 suites (`docs/tls-parity.md` §3.3)* | any other |
| key exchange | X25519 only, one key share. *Since #207: one X25519 share, and a HelloRetryRequest to P-256 or P-384 followed (`docs/tls-parity.md` §3.3)* | a HelloRetryRequest (§3.3) |
| signatures in `CertificateVerify` | `ecdsa_secp256r1_sha256`, `ecdsa_secp384r1_sha384`, `rsa_pss_rsae_sha256/384/512`, `ed25519` | the rest |
| signatures on certificates (`signature_algorithms_cert`) | the above, plus `rsa_pkcs1_sha256/384/512` (what most public CAs sign with) | SHA-1 anywhere in a chain that is verified |

### 3.2 Why ChaCha20 only, and what it costs

*Superseded (#207, `docs/tls-parity.md`): the maintainer requires parity with the OpenSSL backend, so AES-GCM (constant-time),
P-256/P-384 key exchange with HelloRetryRequest, and TLS 1.2 with ECDHE and AEAD suites are added. §3.2 to §3.4 record the
first decision.*

AES-GCM is what most servers prefer, and every server that speaks TLS 1.3 must implement `TLS_AES_128_GCM_SHA256`
(RFC 8446 §9.1). But constant-time AES and GHASH in software are slow:

- A table-based AES leaks its key through cache timing.
- A bitsliced AES is about an order of magnitude slower than table-based.
- GHASH needs a carry-less multiply, and this language has no intrinsics at all, so a constant-time GHASH is a long sequence of
  masked shifts and XORs.

ChaCha20-Poly1305 is constant-time with nothing but adds, XORs and rotates (`docs/chacha20.md` §3), and runs at 135 MB/s here (§6
there). **Cost:** a server that does not offer ChaCha20-Poly1305 refuses the handshake. ChaCha20 is mandatory only as a "SHOULD"
(RFC 8446 §9.1), and FIPS-mode servers and some load balancers offer AES-GCM only. **How many receivers that is has not been
measured.** #207 measures it before anything is built for it. The OpenSSL backend, which offers AES-GCM, stays the default in
the meantime.

### 3.3 X25519 only, and HelloRetryRequest

*Superseded (#207, `docs/tls-parity.md` §3.3): a HelloRetryRequest to P-256 or P-384 is followed, with `std.ecdh`
(`docs/ecdh.md`). This section records the first decision.*

A server that does not support X25519 answers the client's single key share with a HelloRetryRequest for another group,
typically P-256. A P-256 key exchange handles a **secret** scalar, so it needs constant-time P-256 arithmetic. #204 builds only
*verification*, which is variable-time and public-data only.

**Decision:** a HelloRetryRequest is refused (`tls-hello-retry`). A server that supports X25519 never sends one to a client that
offered an X25519 share. Alternative: implement HRR and P-256 ECDHE, which costs a constant-time P-256 implementation and is
deferred with #207's measurement.

### 3.4 What a TLS 1.2 fallback would add (#207)

*Superseded (#207, `docs/tls-parity.md` §3.4): TLS 1.2 is built, with ECDHE, AES-GCM and ChaCha20-Poly1305 and the extended
master secret required. This section records the first decision.*

It would add the TLS 1.2 handshake (a second state machine), the PRF, the extended master secret (required), both downgrade
sentinels, and ECDHE on X25519 with ChaCha20-Poly1305 only. It would not add RSA key exchange, CBC or renegotiation. It is
justified only by a measured number of receivers that need it (#207's gate). It is not part of this design.

---

## 4. Threat model

### 4.1 Who the attackers are

| attacker | can | the design's answer |
|---|---|---|
| **the network** (active, on path) | read, drop, delay, reorder, inject and replay bytes; downgrade attempts; redirect to another server | authenticated key exchange and the transcript in `Finished`; AEAD on every record with sequence numbers; the downgrade sentinels; certificate and name verification |
| **the server itself** (hooks delivers to customer URLs: the receiver is untrusted input) | send any bytes: malformed messages, a 64 KiB certificate chain, endless alerts or KeyUpdates, nothing at all | strict parsing with a tag for each refusal; every size bounded (§5); no input reaches a trap (`CONTRIBUTING.md`); the caller's deadline bounds the time (`docs/tls-nonblocking.md` §10.3) |
| **a certificate** (mis-issued, or for the wrong name, or a CA's key misused) | present a chain that a lenient verifier accepts | SAN-only name matching with strict wildcards, basicConstraints and pathLen, keyUsage/EKU, name constraints, no SHA-1, minimum key sizes (§5) |
| **a timing observer** (remote, or another process on the same host) | measure how long secret-dependent work takes | constant-time rules on the secret path only (§4.2) |

**Out of scope, and said so:**

- revocation (a stolen key with a valid certificate is accepted until it expires);
- a compromised root CA;
- an attacker with code execution in the process;
- physical and power side channels;
- Certificate Transparency.

### 4.2 Which parts handle secrets

| handles secrets: constant-time rules apply | handles only public data: correctness and refusal apply |
|---|---|
| the X25519 ladder (#200); HKDF and the key schedule (`docs/hkdf.md` §3); the AEAD (`docs/chacha20.md` §3); the DRBG (§6); the `Finished` MAC comparison | DER and X.509 parsing; chain building; signature verification (RSA, ECDSA, Ed25519); name matching; record framing and handshake parsing (lengths are public) |

The rule for the left column is the one `docs/chacha20.md` §3 applied and checked in object code: no branch and no memory index
depends on a secret. The right column may branch freely. Being fast and being correct is what matters there, and #203's "compare
the whole encoded block" is a correctness rule, not a timing one. `Finished` is compared over all its bytes, as `chacha20.open`
compares a tag.

---

## 5. Trust and certificates

### 5.1 The root store

- **Where it comes from.** It is the system PEM bundle (`/etc/ssl/certs/ca-certificates.crt` on Debian-family systems) or a file
  the operator names, read by the caller once at start and passed as bytes (§2.2). It is never embedded in a package.
  `cancho-hooks` would read it as it reads its endpoints file (`src/hooks.cho`, `read_endpoints_file`, `Fs("")`).
- **What happens if it is unusable.** An unreadable or empty store stops the start, with the reason. It never falls back to an
  unverified client. That is #211's rule for OpenSSL (`docs/tls-nonblocking.md` §10.5).
- **How roots are treated.** Their self-signatures are not checked. A root is trusted because the operator put it in the store,
  not because it signed itself. *Corrected (`docs/x509-verify.md` §8.3): this said a root is accepted as an anchor whatever its
  own validity dates, "which matches OpenSSL's default". It does not: OpenSSL 3.0.13 refuses an expired root (x509-limbo's
  `rfc5280::validity::expired-root`, error 10 at the root's depth). The verifier checks a root's dates as OpenSSL does. §10's
  question 4 is answered by that.*

### 5.2 Building the chain

- **Which certificates are used.** Only the certificates the server sent, plus the roots: no AIA fetching and no intermediates
  cache.
- **Order.** The leaf is first, and the rest may come in any order, as RFC 8446 §4.4.2 allows.
- **Limits:**
  - depth at most 8 certificates, leaf included;
  - the `Certificate` message at most 64 KiB (§7.1);
  - each certificate at most 16 KiB;
  - no certificate used twice in a path, so a loop is refused, not followed. *Corrected (review finding D-2, #209): no position
    in the `Certificate` message used twice; two copies of one certificate can both stand in a path, bounded by the signature
    budget and the depth (`docs/x509-verify.md` §3).*
- **Per certificate in the path:**
  - the signature, with the issuer's key;
  - the validity: `notBefore <= now <= notAfter`, both inclusive (RFC 5280 §4.1.2.5);
  - `basicConstraints cA` and `pathLenConstraint` on every issuer;
  - `keyUsage` `keyCertSign` on issuers and `digitalSignature` on the leaf, when present;
  - `extendedKeyUsage` containing `serverAuth` on the leaf, when present;
  - name constraints (`dNSName` and `iPAddress` subtrees, permitted and excluded), where present on an issuer;
  - an unknown **critical** extension is refused.
- **Keys:**
  - RSA from 2048 to 4096 bits (#203's capacity), with any other size refused with its own tag;
  - EC on P-256 and P-384;
  - Ed25519.

### 5.3 Name matching

- **Source.** `subjectAltName` only, and **no common-name fallback**. A certificate with no SAN does not match anything.
- **DNS names:**
  - ASCII, compared without case;
  - a wildcard only as the whole left-most label (`*.example.com`). It never matches across a dot, never matches the bare parent
    (`*.example.com` does not match `example.com`), and is never partial (`f*.example.com` is refused, as OpenSSL's
    `NO_PARTIAL_WILDCARDS` in `docs/tls-nonblocking.md` §3.5);
  - a wildcard needs at least two labels after it (`*.com` and `*.co` never match). That is weaker than a public-suffix list
    (`*.co.uk` would pass), and it is §10's question 5;
  - internationalised names are compared as A-labels, byte for byte, and U-labels are not supported.
- **IP addresses.** An `iPAddress` SAN, exact match. No SNI is sent for an IP (RFC 6066 §3).

### 5.4 Not checked, and the API says so

- **Revocation:** CRL, OCSP, stapled OCSP.
- **Certificate Transparency.**
- **Policy constraints and certificate policies** beyond refusing them when they are critical.

`tls.trust`'s documentation and the package README say "revocation is not checked". #211 said the same for OpenSSL
(`docs/tls-nonblocking.md` §3.5): a revoked certificate that is otherwise valid is accepted.

---

## 6. Randomness

The client needs entropy for two things:

- its X25519 private key, 32 bytes per handshake, which is secret;
- the `ClientHello` random and the legacy session id, 64 bytes per handshake, which are public but must not be predictable.

**What exists.** There is no randomness builtin; `crates/cancho-ir/src/builtin.rs` lists none. Two things work already:

- **`Fs` reading `/dev/urandom`.** Measured for this document: `fs_read(fs, "/dev/urandom", b)` on an `Fs("/dev/urandom")` fills
  a 32-byte buffer and returns, and `cancho authority` reports `fs_read("/dev/urandom")` and nothing else.
- **Precedent in hooks.** `cancho-hooks` already reads its PostgreSQL SCRAM nonce this way, with its `Fs("")`
  (`src/history.cho`, `fresh_nonce`).

**Decision.** The caller seeds the engine once (`tls.seed`, 32 bytes from `/dev/urandom`). The engine draws everything else from
a **fast-key-erasure DRBG**: ChaCha20 keystream under a 32-byte key, and the first 32 bytes of each draw replace the key, so a
later compromise of the state does not reveal earlier outputs. It uses `std.chacha20.block`, already built and tested, and needs
no capability. If the engine was never seeded it refuses to start a connection (`tls-no-entropy`); it never runs on a zero key.

**Alternatives:**

- **Read `/dev/urandom` per handshake.** This puts `Fs` in the package and a system call in every handshake.
- **A `random` builtin over `getrandom(2)`.** This is a compiler change with its own effect label. It is the cleaner end state,
  and §10's question 1.
- **libc through `Ffi`.** This is `UNBOUNDED` in the authority report (`docs/tls-nonblocking.md` §6), which is what the pure
  backend exists to avoid.

**Not handled:** a process that forks after seeding would share DRBG state between parent and child. `cancho-hooks` does not
fork. The package README must say that a forking program reseeds in each child.

---

## 7. The protocol, concretely

### 7.1 Messages and limits

- **ClientHello extensions:**
  - `server_name` (only for a DNS name);
  - `supported_versions`;
  - `supported_groups` (`x25519`);
  - `key_share` (one X25519 share);
  - `signature_algorithms` and `signature_algorithms_cert` (§3.1).

  It also sends a 32-byte legacy session id and accepts one `change_cipher_spec` record before the encrypted handshake (RFC 8446
  Appendix D.4, middlebox compatibility).
- **Not offered:** PSK, early data, ALPN, `max_fragment_length`, `status_request`. An extension in the ServerHello or in
  EncryptedExtensions that the client did not offer is refused (`tls-unsupported-extension`, RFC 8446 §4.2).
- **Records:**
  - a plaintext record is at most 2^14 bytes, and a ciphertext record at most 2^14 + 256, with anything larger refused
    (`tls-record-overflow`);
  - the 64-bit sequence number is per direction and per key, and a connection that would wrap it is closed.
- **Handshake messages:**
  - they are reassembled across records, and several may share one record;
  - one message is at most **64 KiB**, the reassembly buffer per slot;
  - a message must not span a key change (RFC 8446 §5.1).
- **Order.** `ServerHello`, `EncryptedExtensions`, `Certificate`, `CertificateVerify`, `Finished`, and anything else at that
  point is `tls-unexpected-message`.
- **CertificateRequest.** It is answered with an empty `Certificate`, since there are no client certificates (§4.1), and the
  server decides.
- **After the handshake:**
  - `NewSessionTicket` is parsed and dropped (no resumption, §7.2); *since #286's build
    (`docs/tls-resumption.md`): kept, the newest one, for the engine to save;*
  - a received `KeyUpdate` is applied, and answered when `update_requested`. The client never initiates one. Alternative: refuse
    `KeyUpdate`, which would break long connections to servers that rotate keys; the cost of supporting it is one more key
    derivation;
  - more than 32 KeyUpdates, or more than 16 warning alerts, on one connection is refused as a hostile peer
    (`tls-too-many-messages`).
- **Alerts.** Every received alert ends the connection, except `close_notify`, which is a clean end, and `user_canceled`, which is
  ignored until the following `close_notify`. *Corrected (review finding E-3, #209): a `close_notify` is a clean end only once
  the handshake is done. Before that it may come in the clear, from anyone on the path, and nothing was authenticated to end, so
  the connection fails with `tls-peer-closed`, as a socket closed mid-handshake does.*

### 7.2 Not built, and why

- **Session resumption and 0-RTT.** These are a separate decision (#197's non-goals). The OpenSSL spike measured resumption
  halving the handshake CPU (`docs/tls-nonblocking.md` §8.5), and that saving is given up here until it is designed with its
  hazard: a resumed session skips verification, as §10.4 there says. *Since #286: TLS 1.3 resumption with (EC)DHE is designed
  and built, with eight rules for that hazard (`docs/tls-resumption.md`). 0-RTT, PSK-only resumption and TLS 1.2 resumption
  stay out, refused by that design, not deferred.*
- **Client certificates. Post-handshake authentication**, which is refused if requested, because it is not offered.

### 7.3 Secrets in memory

The language guarantees no erasure. A region's memory is freed without being cleared, and the optimiser may delete stores to
memory that is about to be freed (`docs/chacha20.md` §3.2). `tls.drop` overwrites keys, IVs and DRBG output it owns, which is
**best effort**, and the package says exactly that. A guaranteed-store primitive is §10's question 2.

### 7.4 Memory per connection

| | bytes |
|---|---|
| handshake reassembly (released into the slot's pool after `Finished`) | 65,536 |
| one record in and one record out | about 2 × 16,640 |
| the transcript hash state (`crypto.sha256_state_len()` = 138 words) | 1,104 |
| keys, IVs, sequence numbers, the HMAC state for `Finished` (`hmac.state_len(32)` = 202 words), the X25519 secret | about 2,000 |
| **total, during the handshake** | **about 100 KiB** |

That is against 26 to 48 KiB for OpenSSL (`docs/tls-nonblocking.md` §8.3), so for 64 slots it is about 6.3 MiB. **As built
(#205, `docs/tls-core.md` §9.1), a slot is about 179 KiB, or 11.2 MiB for 64**: the estimate left out room for three outgoing
records and the separate buffers for an opened record, received data and the leaf certificate. *Resumption (#286,
`docs/tls-resumption.md` §4) adds 4,496 bytes a slot (a ticket offered, the newest received with its PSK and host name, and two
secrets), and the engine's ticket table 2,440 bytes a ticket.* Shrinking the
reassembly buffer to the largest certificate message actually seen is §10's question 6. The number is a design estimate and
#208 measures it.

---

## 8. Refusal tags

Each tag is one failure a caller can act on: retry, alert an operator, or fix a configuration. A tag is stored in the attempt's
history, as `cancho-hooks` stores `attempts.status` today (`docs/tls-nonblocking.md` §10.1).

**Protocol** (`packages/tls`):

| tag | when | what an operator does |
|---|---|---|
| `tls-peer-closed` | the connection ended before the handshake did | retry |
| `tls-alert` (detail: the alert number) | the server sent a fatal alert | read the alert: `handshake_failure` and `protocol_version` usually mean no shared suite or version |
| `tls-protocol-version` | the server chose TLS 1.2 or below, or set a downgrade sentinel (*since #208, in a TLS 1.2 ServerHello*) | the receiver needs TLS 1.3, or the OpenSSL backend |
| `tls-no-shared-cipher` | the ServerHello names a suite that was not offered | the receiver lacks ChaCha20-Poly1305 (§3.2). *Since #207: it lacks all three TLS 1.3 suites* |
| `tls-hello-retry` | a HelloRetryRequest (§3.3). *Since #207: one the client cannot follow (it changes nothing, or its cookie is over 2,048 bytes), or a ServerHello after it with another suite* | the receiver lacks X25519. *Since #207: a broken peer, or a cookie the limit should allow* |
| `tls-unexpected-message` | a message out of order | a broken or hostile peer |
| `tls-decode-error` | a malformed message | the same |
| `tls-unsupported-extension` | an extension that was not offered | the same |
| `tls-record-overflow` | a record or a handshake message over its limit (§7.1) | the same |
| `tls-bad-record-mac` | an AEAD tag did not verify | the path is corrupting or tampering; retry |
| `tls-key-share` | an X25519 share that gives the all-zero secret | a hostile peer |
| `tls-extended-master-secret` | *#207:* a TLS 1.2 server without the extended master secret (RFC 7627) | the receiver's TLS 1.2 is old; it needs TLS 1.3, or the OpenSSL backend |
| `tls-renegotiation` | *#207:* a TLS 1.2 HelloRequest | the receiver renegotiates; none is done |
| `tls-bad-certificate-verify` | the server's signature over the transcript does not verify | a hostile peer, or the wrong key |
| `tls-bad-finished` | the server's `Finished` does not verify | the same |
| `tls-too-many-messages` | the KeyUpdate or warning-alert limits (§7.1) | the same |
| `tls-illegal-psk` | *#286:* a `pre_shared_key` in a ServerHello that names an identity other than the one offered, or comes with a suite whose hash is not the ticket's (one answering a ClientHello that offered none is `tls-unsupported-extension`, RFC 8446 §4.2) (`docs/tls-resumption.md` §5) | a broken or hostile peer |
| `tls-no-entropy` | the engine was never seeded | a program bug: seed it in `main` |
| `tls-slot` | a slot number out of range, or a slot already in use | a program bug |

**Certificates** (`packages/x509`, through `packages/tls`):

| tag | when | OpenSSL's code for the same thing (`X509_V_ERR_*`, `docs/tls-nonblocking.md` §5) |
|---|---|---|
| `x509-decode` | a certificate is not strict DER (#202) | 6 (`UNABLE_TO_DECODE_ISSUER_PUBLIC_KEY`), 13 and 14 (a bad validity field) |
| `x509-unknown-issuer` | no path to a trusted root | 2, 18, 19, 20 |
| `x509-expired` | `now > notAfter` | 10 |
| `x509-not-yet-valid` | `now < notBefore` | 9 |
| `x509-bad-signature` | a signature in the path does not verify | 7 |
| `x509-name-mismatch` | no SAN matches the host (§5.3) | 62 |
| `x509-not-ca` | an issuer without `cA` | 79 (`INVALID_CA`) |
| `x509-path-too-long` | over `pathLenConstraint`, or over depth 8 | 25 |
| `x509-name-constraint` | a name outside a permitted subtree, or inside an excluded one | 47, 48 (and 51 to 53, a constraint it cannot read) |
| `x509-key-usage` | keyUsage or EKU forbids this use | 26 (`INVALID_PURPOSE`), 32 (`KEYUSAGE_NO_CERTSIGN`) |
| `x509-unsupported-algorithm` | a key or signature algorithm outside §5.2, SHA-1 included | 68 (`CA_MD_TOO_WEAK`), 76 (`UNSUPPORTED_SIGNATURE_ALGORITHM`) |
| `x509-key-size` | an RSA key outside 2048 to 4096 bits | 66 (`EE_KEY_TOO_SMALL`), 67 (`CA_KEY_TOO_SMALL`) |
| `x509-critical-extension` | an unknown critical extension | 34 (`UNHANDLED_CRITICAL_EXTENSION`) |
| `x509-chain-too-large` | a certificate over 16 KiB, or a chain over 8 | none: OpenSSL refuses differently |

The right-hand column is the mapping the OpenSSL backend's `failure` uses (§2.2), so hooks' history means the same thing under
either backend. #210's gate checks that the two backends give equal outcomes. The numbers were checked against OpenSSL 3.0.13's `/usr/include/openssl/x509_vfy.h`
when this was written (a first draft from memory had three of them wrong). #206 tests the mapping by running both backends on the same chains.

---

## 9. Gates, per sub-issue, and what they guard

Each sub-issue (#199 to #210) states its own gate as a command. This design adds three that span sub-issues:

1. **The two backends agree.** For the certificate matrix of `docs/tls-nonblocking.md` §5, both backends give the same tag, plus
   #206's wildcard and constraint rows (#210).
2. **No capability in the pure backend.** `cancho authority` on a hooks build with the pure backend shows no `ffi(...)` and no
   foreign symbols. This is checked by a test, not asserted (#210). *Corrected (#210, `docs/tls-hooks.md` §2.4): hooks as a whole
   also holds `libc` (`statx`, `prctl`: the modes of the data directory), so "none at all" cannot hold for it. The check is no
   `libssl` and no `libcrypto` scope, none of the 32 symbols, and the `libc` entries unchanged.*
3. **No input reaches a trap.** It is fuzzed at the record, handshake, DER and chain levels (#208), as `dns.cho` was over a million
   damaged answers (`docs/tls-nonblocking.md` §7). *#208's plan and results, for this and for the rest of its bar, are in `docs/tls-assurance.md`. Fuzzing is in §3.6 (no crash
   and no hang). The differential and interop matrices are in §4.1 and §5.1. Timing is in §6.1: X25519 and ChaCha20-Poly1305 pass
   on both backends, and three tests fail on an Apple M4 with its data-independent-timing bit clear. Resource bounds are in
   §7.1.*

---

## 10. Open questions, for a person

Each is a decision this design does not make by default, with the default it assumes until someone decides:

1. **A randomness builtin.** Add `random(rng: &Rng, into)` over `getrandom(2)`, with its own capability and effect label, instead
   of `Fs` and `/dev/urandom` (§6)? *Assumed: no; the caller seeds through `Fs`.*
2. **A guaranteed erasure.** Add a store the optimiser may not remove (a `secure_zero` builtin), so §7.3 can promise erasure
   instead of attempting it? *Assumed: no; best effort, said so.*
3. **Where the bignum and curves go.** `std` or a package (§1.2)? *Assumed: `std`, in their own files.*
4. **Expired roots.** Accept an expired root as an anchor (OpenSSL's default) or refuse it? *Assumed: accept, matching OpenSSL, so
   the two backends agree.* *Corrected (`docs/x509-verify.md` §8.3): OpenSSL's default refuses an expired root, so matching it
   means refusing, and the verifier does. The two backends agree, which was the point of the assumption.*
5. **Public-suffix wildcards.** Is "two labels after the wildcard" enough, or does the package carry a public-suffix list (which
   then needs updating)? *Assumed: two labels; mis-issuance of `*.co.uk` is a CA failure the rest of the ecosystem polices.*
6. **The 64 KiB handshake buffer per slot.** Keep it per slot (100 KiB a connection, §7.4), or share a pool across slots, which
   makes the code harder? *Assumed: per slot until #208 measures.*
7. **ChaCha20 only.** Is losing AES-GCM-only receivers acceptable until #207's measurement? *Assumed: yes, with OpenSSL the default
   until #209.*
8. **Who reviews (#209).** A person or a firm, named before the work it reviews is called finished.

---

## 11. Corrected elsewhere

Nothing yet. This document makes claims about code that is not written. Where the code turns out different, the code's PR corrects
the claim here, in place.
