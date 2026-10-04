# `packages/tls`: the TLS 1.3 client handshake and record layer

> **Status: built (#205, all three PRs); results in §9 (PR 2) and §10 (PR 3).** Sub-issue 8 (#205) of the self-contained TLS 1.3 client (#197). `docs/tls-pure.md` already fixes the API
> (§2.2), the cipher policy (§3), the threat model (§4), the limits and messages (§7.1) and the refusal tags (§8). This document
> settles what that left open for the code:
> - how the work is split into PRs;
> - how a slot is laid out;
> - the state machine;
> - what "the certificate is verified" means before #206 builds chains;
> - how the issue's byte-for-byte gate is met when RFC 8448's traces use a cipher this client does not have.

---

## 1. Three PRs, not one

#205 is the largest slice of the epic, and each part has its own evidence. Each PR passes the gate on its own:

1. **This design.**
2. **The protocol without a network.**
   - records: framing, ChaCha20-Poly1305 protection, sequence numbers;
   - handshake messages: encoding, parsing, fragmentation and coalescing;
   - the key schedule over the transcript;
   - the state machine.

   Its evidence is offline: the key schedule against RFC 8448 (§5.1), and a recorded handshake replayed byte for byte (§5.2).
3. **The engine and the network.**
   - the API of `docs/tls-pure.md` §2.2 over 64 slots;
   - real handshakes against `openssl s_server`, a Python `ssl` server and tlslite-ng;
   - 64 connections on one poller, fed byte by byte and in large reads;
   - the misbehaving-server cases (§6);
   - the mutants (§7).

## 2. Files

Every source file stays under 2,000 lines (`crates/lex-sys/tests/files.rs`), so the package is split by concern, one module per
file:

| File | Module | What |
|---|---|---|
| `packages/tls/record.ls` | `tls_record` | record framing, the per-direction AEAD key, IV and sequence number, the nonce (RFC 8446 §5.3), `TLSInnerPlaintext` padding |
| `packages/tls/message.ls` | `tls_message` | ClientHello encoding, and parsing of ServerHello, EncryptedExtensions, Certificate, CertificateVerify, Finished, NewSessionTicket and KeyUpdate, each refusing with a tag |
| `packages/tls/client.ls` | `tls_client` | the state machine (§4), the transcript, the key schedule's use of `std.hkdf`, signature checks. *Since #207: TLS 1.3's handshake and the interface; what both handshakes share moved to `slot.ls`* |
| `packages/tls/slot.ls` | `tls_slot` | *#207:* the slot's layout, states, transcript, record queue, alerts, the ECDH share, chain and signature checks (`docs/tls-parity.md` §3.4.1) |
| `packages/tls/client12.ls` | `tls_client12` | *#207:* the TLS 1.2 handshake (`docs/tls-parity.md` §3.4) |
| `packages/tls/tls.ls` | `tls` | the engine: slots, `feed`/`take`/`send`/`recv`/`event` (PR 3) |

The package imports `std`, so it is published with `--std` (`docs/package-system.md` §4.8). It requires `packages/x509`, which is
published with it (`docs/x509.md` §1).

## 3. A slot

As `docs/tls-pure.md` §2.1 requires, the engine owns two boxes, one of `int`s and one of `byte`s, and a slot is a fixed stride
of each. Every offset is a named function, as in `packages/x509`'s view. Nothing is allocated per connection, and no size comes
from the peer. The byte stride holds:
- the 64 KiB handshake reassembly buffer (`docs/tls-pure.md` §7.4);
- one incoming and one outgoing record;
- the queue of bytes for `take`;
- the X25519 secret;
- the four traffic secrets, keys and IVs;
- the transcript hash state;
- the host name.

## 4. The state machine

| State | Waiting for | Then |
|---|---|---|
| `start` | — | ClientHello queued → `wait_sh` |
| `wait_sh` | ServerHello (plaintext) | handshake keys → `wait_ee` |
| `wait_ee` | EncryptedExtensions | → `wait_cert_cr` |
| `wait_cert_cr` | Certificate, or CertificateRequest then Certificate | → `wait_cv` |
| `wait_cv` | CertificateVerify | signature checked → `wait_finished` |
| `wait_finished` | server Finished | checked; client Finished queued; application keys → `connected` |
| `connected` | application data, NewSessionTicket, KeyUpdate, alerts | |
| `closed`, `failed` | — | |

Anything else in a state is `tls-unexpected-message` with an `unexpected_message` alert.
- **HelloRetryRequest** is refused (`tls-hello-retry`, `docs/tls-pure.md` §3.3). The ServerHello random that marks one (RFC 8446
  §4.1.3) is checked before anything else in the message. *Changed by #207 (`docs/tls-parity.md` §3.3): one HelloRetryRequest
  to P-256 or P-384 is followed; the random still tells it apart first.*
- **A downgrade sentinel** in the last 8 bytes of the random is `tls-protocol-version`.
- **`change_cipher_spec`**: one is accepted after ServerHello and before the server's Finished, and only if it is exactly `01`
  (Appendix D.4). *Corrected (§10.3): this said "before the first encrypted record". The code accepts one until the server's
  Finished, which is what Appendix D.4 asks of a receiver.*
- **KeyUpdate** is applied, and answered when `update_requested` (`docs/tls-pure.md` §7.1).

## 5. Before #206: a certificate is accepted only when pinned

Chain building, name matching and validity times are #206. A client that skipped them and still said `established` would be a
client that trusts anyone. So until #206 the trust store is used **as a set of pins**:
- the leaf's DER must equal a certificate in the store, byte for byte;
- the leaf's key must verify `CertificateVerify` (RSA-PSS through `std.rsa`, or ECDSA through `std.ecdsa`);
- anything else is `x509-unknown-issuer`.

A pinned leaf needs no chain and no name check: the operator named that exact certificate. This is enough for every test in
§6, which use certificates made for them. #206 replaces the pin check with real chain validation, behind the same call.

*Ended (#206, `docs/x509-verify.md` §9): `tls.trust` now loads roots, and `Certificate` is verified against them, the host and
the time `start` was given. The pins are gone. The tests of §9 and §10 were re-recorded with a CA for each server; their text
below describes them as they were built.*

## 6. Evidence

### 6.1 The byte-for-byte gate, and why it is not RFC 8448's trace

The issue asks for RFC 8448's traces, replayed byte for byte. Two facts stand in the way:

- **RFC 8448's Simple 1-RTT Handshake negotiates `TLS_AES_128_GCM_SHA256`.** s2n-tls's transcription of it sets that suite
  (below), and as far as I recall, without the RFC to check, the other traces do the same. This client offers only
  `TLS_CHACHA20_POLY1305_SHA256`
  (`docs/tls-pure.md` §3.2), so its ClientHello cannot equal the trace's. Its encrypted messages cannot either, nor can it
  decrypt the trace's server flight.
- **The RFC's text is not reachable from the machine this is built on** (`rfc-editor.org` and `datatracker.ietf.org` are
  outside the network policy). `docs/hkdf.md` §4 already met this: some RFC 8448 values written from memory did not reproduce.

So the gate is met in two parts:

1. **RFC 8448's key schedule, in full**, from AWS s2n-tls's `tests/unit/s2n_tls13_secrets_rfc8448_test.c`. That file
   transcribes RFC 8448 §3 and cites it section by section. It covers:
   - every secret of the Simple 1-RTT Handshake;
   - the transcript hashes they are derived from;
   - the application traffic secrets that `docs/hkdf.md` §4 left out because they did not reproduce from memory.

   Each value is kept only if Python's `hmac` reproduces it from the file's own inputs. Where one disagrees with `docs/hkdf.md`,
   that document is corrected in place.
2. **A recorded ChaCha20 handshake, replayed byte for byte.**
   - A Python TLS 1.3 server (tlslite-ng 0.8.2, pure Python, with ChaCha20-Poly1305 and X25519) runs with its randomness fixed:
     `getRandomBytes` is replaced by a seeded generator.
   - The client is this one, seeded with fixed bytes.
   - The bytes each side sent are written to `tests/vectors/tls/`.
   - The test feeds the server's bytes to the client with no network. Every byte the client produces must equal the recording:
     ClientHello, Finished, an encrypted request and `close_notify`.

   This proves determinism, and it guards against regressions. Interoperability comes from tlslite-ng, an implementation
   written independently of this one, having accepted the client's Finished and decrypted its request when the recording was
   made. There are three recordings:
   - an RSA-PSS server certificate;
   - an ECDSA P-256 server certificate;
   - a server that sends a HelloRetryRequest, which must be refused.

### 6.2 Real servers (PR 3)

The client does a handshake and an HTTP request, and must see the body and `close_notify`, against:
- `openssl s_server -tls1_3 -ciphersuites TLS_CHACHA20_POLY1305_SHA256` (OpenSSL 3.0.13);
- a Python `ssl` server (OpenSSL underneath, so a different configuration path);
- tlslite-ng.

nginx is not installed here, and the issue makes it conditional.

Then 64 connections on one thread through the poller:
- each fed one byte at a time, which tests fragmentation and reassembly;
- each fed in reads of up to 64 KiB, which tests coalescing.

### 6.3 A misbehaving server (PR 3)

tlsfuzzer tests servers; this is a client. The equivalent is a server that lies and changes one thing in its flight. *Corrected
(§10.1): this said the server would be tlslite-ng with hooks. It is written instead on pyca/cryptography's primitives
(`scripts/tls_liar.py`), so that every byte it sends is fixed and each case can be recorded and replayed.* Each case must end
in its own tag:

| Server does | Tag |
|---|---|
| sends EncryptedExtensions before ServerHello, or Finished before CertificateVerify | `tls-unexpected-message` |
| adds an extension the client did not offer (ALPN, `early_data`) | `tls-unsupported-extension` |
| chooses TLS 1.2, or puts a downgrade sentinel in its random | `tls-protocol-version` |
| chooses a cipher suite that was not offered | `tls-no-shared-cipher` |
| sends HelloRetryRequest | `tls-hello-retry` (*since #207, followed when it asks for P-256 or P-384; the HRR cases are in `docs/tls-parity.md` §3.3.1*) |
| sends an all-zero X25519 share | `tls-key-share` |
| sends a record over 2^14 + 256 bytes | `tls-record-overflow` |
| flips one bit in an encrypted record | `tls-bad-record-mac` |
| signs `CertificateVerify` with another key | `tls-bad-certificate-verify` |
| sends a wrong `Finished` | `tls-bad-finished` |
| sends an unpinned certificate | `x509-unknown-issuer` |
| sends a fatal alert | `tls-alert` |

The list goes in the issue's closing comment, each case marked passed, refused for the right reason, or not applicable.

## 7. Mutants (PR 3)

At least 15, each killed by §6 (22 were built; §10.2):
- the server Finished not checked;
- the transcript missing a message;
- the transcript hashed over the record header;
- a sequence number not incremented;
- the nonce built without the sequence;
- the downgrade sentinel ignored;
- the HRR random ignored;
- an unexpected extension accepted;
- the record limit off by one;
- inner-plaintext padding not stripped;
- the content type taken from the outer header;
- the client keys used for reading;
- a KeyUpdate not answered;
- the CertificateVerify context string misspelled;
- a fragmented message assembled in the wrong order;
- the alert level ignored.

## 8. Secrets

`tls.drop` and the end of a connection overwrite the slot's secrets, keys, IVs and X25519 secret with zeros. The end is a
failure, once its alert is sealed, or close_notify both sent and received (§10.3). The language
guarantees no erasure (`docs/tls-pure.md` §7.3): the optimiser may remove stores to memory that is not read again, and freed
memory is not cleared. So this is best effort, and the package says so. Because the slot's boxes live as long as the engine,
the zeroing stores are not to memory about to be freed. That is the case where an optimiser removes them most readily.

## 9. PR 2: the protocol, offline (results)

### 9.1 What was built

| File | Lines | What |
|---|---|---|
| `packages/tls/record.ls` (`tls_record`) | 339 | framing, the nonce, `seal` and `open`, every refusal code of the package |
| `packages/tls/message.ls` (`tls_message`) | 464 | ClientHello, and strict parsers for every server message |
| `packages/tls/client.ls` (`tls_client`) | 1,224 | one connection: `start`, `feed`, `take`, `send`, `recv`, `finish`, `event`, `drop`; the state machine, transcript, key schedule, pins and signature checks |
| `tests/programs/tls_driver.ls` | | the package driven one line at a time, so a harness can put a real server on the other end |

**The slot is larger than `docs/tls-pure.md` §7.4 estimated:** 181,927 bytes and 158 words, about 179 KiB per connection, or
11.2 MiB for 64. The estimate was about 100 KiB. The difference is buffers the estimate did not count:
- room for three outgoing records;
- separate buffers for one opened record, received application data, and the leaf certificate (kept from Certificate to
  CertificateVerify).

`docs/tls-pure.md` §7.4 is corrected to point here. Shrinking it is that document's question 6, and #208's to measure.

### 9.2 Evidence

- **The record layer against pyca/cryptography:** `scripts/tls_record_differential.py`. 2,000 records sealed and 2,000 opened
  (with random padding, and one in three with a bit flipped), sequence numbers up to 2^62 - 1, contents up to 2^14 bytes:
  **0 differences.**
- **Two recorded handshakes against tlslite-ng 0.8.2** (`scripts/tls_trace.py`, `tests/vectors/tls/`): one with an RSA-2048
  certificate (CertificateVerify by RSA-PSS), one with ECDSA P-256. In each recording, tlslite-ng accepted the client's
  Finished, received `GET / HTTP/1.0` and answered, and the client decrypted the body and saw close_notify. `conformance/tls.rs`
  replays both **byte for byte on both backends**.
- **Fragmentation and coalescing.** The same server bytes fed one byte a line, and the whole handshake flight in one line, give
  exactly the bytes the client sent and received before.
- **A wrong pin** is refused (`x509-unknown-issuer`) with an `unknown_ca` alert.
- **15 crafted ServerHellos and records**, each refused with its own tag and a fatal alert:
  - HelloRetryRequest;
  - the downgrade sentinel; a TLS 1.2 ServerHello; `supported_versions` 1.2;
  - AES-128-GCM chosen; ALPN never offered; a duplicate extension; another session id;
  - a P-256 share; an all-zero share; a byte after the extensions;
  - a record over 2^14 + 256; an unknown content type; application data before the handshake.

  Plus a bit flipped in the encrypted flight (`tls-bad-record-mac`). The case "a byte after the extensions" first passed, for
  the wrong reason: the test appended the byte after the record, not inside the message.
- **RFC 8448's key schedule** (§6.1): every value in s2n-tls's transcription reproduces in Python. The three application-stage
  secrets that `docs/hkdf.md` had left out are now rows of `tests/vectors/kdf.txt` (62 rows), and that document is corrected in
  place.

**Tried by hand, not yet a gate.** The same driver, through a Python harness over TCP, completed a handshake and an HTTP request
against:
- `openssl s_server -tls1_3 -ciphersuites TLS_CHACHA20_POLY1305_SHA256 -groups X25519` (OpenSSL 3.0.13);
- a Python `ssl` server.

Each was run with P-256, P-384, RSA-2048 and RSA-4096 certificates, and OpenSSL's NewSessionTickets were parsed and dropped.
PR 3 makes this a committed test, through the engine and a poller rather than a harness.

### 9.3 Found

- **The application secrets of RFC 8448** (§9.2): `docs/hkdf.md`'s open item is closed.
- **The design's memory estimate** (§9.1).
- **Two bugs caught reading the code before it ran:**
  - after the handshake, the client erased the range from the client handshake secret to the master secret, which included the
    application secrets that KeyUpdate needs;
  - the client's Certificate message wrote its length into one byte, when the echoed context can make it 259.

## 10. PR 3: the engine and the network (results)

### 10.1 What was built

| File | Lines | What |
|---|---|---|
| `packages/tls/tls.ls` (`tls`) | 346 | the engine of `docs/tls-pure.md` §2.2: `open`, `trust`, `seed`, `start`, `feed`, `take`, `send`, `recv`, `eof`, `finish`, `event`, `failure`, `drop`, `close`; slots as strides of two boxes; the pins; a fast-key-erasure DRBG over `chacha20.block` |
| `packages/tls/client.ls` | 1,234 | `peer_eof`, and the secrets overwritten when a connection ends (§8) |
| `tests/programs/tls_many.ls` | 467 | up to 256 connections on one thread through the `Poller`: dial, handshake, a 2^14-byte request, read to close_notify; one line per connection with the response's length and SHA-256 |
| `scripts/tls_live.py` | | Python `ssl`, `openssl s_server` and tlslite-ng servers on this machine, 64 connections at once |
| `scripts/tls_liar.py` | | the lying server (§6.3) and the recorded streams for `cargo test` |
| `scripts/tls_mutants.py` | | §7 |

**The engine's API differs from `docs/tls-pure.md` §2.2 in three places**, which that section now records:
- `trust` borrows the engine and answers a count, rather than moving it;
- `eof(engine, slot)` exists: the socket's end is not close_notify, and only the engine can tell a clean end from a truncation;
- `recv` answers `would_block()` when nothing is waiting, because 0 means close_notify.

**The poller reads once a wakeup.** It is level-triggered, so a slot with more waiting is reported again after the others. The
first version read until the socket was empty. With one-byte reads, each slot then ate its whole flight in one turn, and
"64 connections fed one byte at a time" never interleaved them. The engine's slot-overlap mutant survived that version
(§10.2).

### 10.2 Evidence

All of it passes the gate, and the parts that need no Python run in `cargo test` (`conformance/tls.rs`).

- **Live servers** (`scripts/tls_live.py`, by hand). 64 connections at once, each reading 1 byte a socket read and then 65,536,
  each sending a 2^14-byte request and reading to close_notify:
  - **a threaded Python `ssl` server**, with P-256, P-384, RSA-2048, RSA-4096 and Ed25519 certificates, a 64 KiB body per
    connection: **every connection `ok`, every body's SHA-256 the one the server sent**;
  - **`openssl s_server -HTTP`** (OpenSSL 3.0.13, ChaCha20-Poly1305 and X25519 only), P-256 and RSA-2048: every connection
    `ok`, every body the same. It serves one connection at a time, so 63 wait in its backlog;
  - **tlslite-ng 0.8.2, threaded**, P-256 and RSA-2048: every connection `ok`;
  - **a Python `ssl` server that closes without close_notify**: every connection fails `tls-peer-closed`.

  64 connections take 0.4 s at 65,536 a read and 3.8 to 4.2 s at one byte (three runs each), on a 64 KiB body each (the Python server is the larger
  share). Peak RSS of `tls_many` is 31.1 MiB **with 1 connection and with 64**. So the 64 slots (11.2 MiB, §9.1) are not what
  its size is made of; #208 measures where it goes.
- **The lying server** (`scripts/tls_liar.py`, `tests/vectors/tls/liar.txt`). 29 connections, each recorded and replayed
  **byte for byte on both backends**; a second recording is identical. Two are honest, in the hardest legal shape:
  - a change_cipher_spec;
  - EncryptedExtensions sharing a record with Certificate, and Certificate in three records;
  - CertificateVerify and Finished in one record, padded by 300 bytes;
  - a NewSessionTicket, and a record of exactly 2^14 + 256 bytes;
  - a KeyUpdate that asks for an answer, which the server decrypts under the client's old key, then data under both new keys;
  - a KeyUpdate that does not ask, and close_notify;
  - in the second, user_canceled and then close_notify.

  The other 27 each end with their tag and the alert RFC 8446 §6.2 names. During the server's flight the alert is plaintext,
  because the client's write key changes only after the server's Finished (Appendix A.1); after that it is under the client's
  application key.

  | Server does | Tag | Alert |
  |---|---|---|
  | EncryptedExtensions before ServerHello; Certificate before EncryptedExtensions; Finished before CertificateVerify; application data in the flight; a NewSessionTicket in Finished's record | `tls-unexpected-message` | 10 |
  | ALPN, or `early_data`, in EncryptedExtensions | `tls-unsupported-extension` | 110 |
  | no `supported_versions`; `supported_versions` 1.2; the TLS 1.2 or TLS 1.1 downgrade sentinel | `tls-protocol-version` | 70 |
  | AES-128-GCM (*since #207, TLS_AES_128_CCM_SHA256: AES-128-GCM is offered*) | `tls-no-shared-cipher` | 40 |
  | HelloRetryRequest (*since #207, followed; the twelve cases that replace this row are in `docs/tls-parity.md` §3.3.1*) | `tls-hello-retry` | 40 |
  | an all-zero, or a low-order, X25519 share | `tls-key-share` | 47 |
  | an encrypted record over 2^14 + 256; an inner plaintext over 2^14; a Certificate over 64 KiB | `tls-record-overflow` | 22 |
  | one bit flipped; records out of order; data under the key a KeyUpdate replaced | `tls-bad-record-mac` | 20 |
  | CertificateVerify by another key | `tls-bad-certificate-verify` | 51 |
  | a wrong Finished | `tls-bad-finished` | 51 |
  | an unpinned certificate | `x509-unknown-issuer` | 48 |
  | a fatal alert for ServerHello; a warning-level alert in the flight; a fatal alert after the handshake | `tls-alert` | none sent |

  Every row of §6.3's table is passed.
- **64 connections with no Python** (`tests/vectors/tls/streams.txt`). With a fixed seed (a test-only argument of `tls_many`),
  each ClientHello depends only on how many connections started before it. The honest server's bytes depend only on the
  ClientHello. So `scripts/tls_liar.py --streams` records the server's flight and reply for each of the 64, and
  `conformance/tls.rs` serves them back from a Rust thread. One in eight carries a 60 KB chain, nearly filling its slot's
  64 KiB reassembly buffer. Three runs:
  - one byte a read, then 65,536: every connection ends `ok` with its recorded response;
  - close_notify cut off: every connection fails `tls-peer-closed`.
- **22 mutants, 22 killed** (`scripts/tls_mutants.py`, about 2 minutes). §7's sixteen, plus:
  - the write sequence number not incremented;
  - the pin not checked;
  - a message allowed to share a record with the next key;
  - the engine's DRBG key not replaced;
  - the engine's slots overlapping by half;
  - the socket's end taken for close_notify.

  "The transcript hashed over the record header" is built as the ClientHello hashed with its own header, since no other
  message's header ever reaches the transcript code. "A fragmented message assembled in the wrong order" is built as a partial
  message not moved to the front of the buffer. **The overlapping-slots mutant survived twice.** First, the requests were
  small, so no slot's output buffer reached its neighbour's keys. Second, the poller drained each socket in one turn. The
  2^14-byte request and one read a wakeup kill it: 30 of 64 connections then end `ok`.

### 10.3 Found

- **§4's change_cipher_spec rule** was stricter than the code. The code is right (Appendix D.4), and §4 is corrected.
- **§8's claim that the end of a connection erases the slot was false.** Only `drop` did. A failure, and close_notify both
  ways, now overwrite the keys, secrets and last plaintext. No test can observe this; the stores are best effort, as §8 says.
- **§6.3's tlslite-ng hooks became a server of its own.** That is corrected in §6.3. A deterministic server makes each case a
  recording that `cargo test` replays with no Python.
- **The poller's first loop** did not interleave one-byte reads (§10.1).

