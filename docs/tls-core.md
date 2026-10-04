# `packages/tls`: the TLS 1.3 client handshake and record layer

> **Status: design.** Sub-issue 8 (#205) of the self-contained TLS 1.3 client (#197). `docs/tls-pure.md` already fixes the API
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
   - real handshakes against `openssl s_server` and a Python `ssl` server;
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
| `packages/tls/client.ls` | `tls_client` | the state machine (§4), the transcript, the key schedule's use of `std.hkdf`, signature checks |
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
  §4.1.3) is checked before anything else in the message.
- **A downgrade sentinel** in the last 8 bytes of the random is `tls-protocol-version`.
- **`change_cipher_spec`**: one is accepted between ServerHello and the first encrypted record, and only if it is exactly `01`
  (Appendix D.4).
- **KeyUpdate** is applied, and answered when `update_requested` (`docs/tls-pure.md` §7.1).

## 5. Before #206: a certificate is accepted only when pinned

Chain building, name matching and validity times are #206. A client that skipped them and still said `established` would be a
client that trusts anyone. So until #206 the trust store is used **as a set of pins**:
- the leaf's DER must equal a certificate in the store, byte for byte;
- the leaf's key must verify `CertificateVerify` (RSA-PSS through `std.rsa`, or ECDSA through `std.ecdsa`);
- anything else is `x509-unknown-issuer`.

A pinned leaf needs no chain and no name check: the operator named that exact certificate. This is enough for every test in
§6, which use certificates made for them. #206 replaces the pin check with real chain validation, behind the same call.

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

tlsfuzzer tests servers; this is a client. The equivalent is a server that lies, made from tlslite-ng with hooks that change
one thing in its flight. Each case must end in its own tag:

| Server does | Tag |
|---|---|
| sends EncryptedExtensions before ServerHello, or Finished before CertificateVerify | `tls-unexpected-message` |
| adds an extension the client did not offer (ALPN, `early_data`) | `tls-unsupported-extension` |
| chooses TLS 1.2, or puts a downgrade sentinel in its random | `tls-protocol-version` |
| chooses a cipher suite that was not offered | `tls-no-shared-cipher` |
| sends HelloRetryRequest | `tls-hello-retry` |
| sends an all-zero X25519 share | `tls-key-share` |
| sends a record over 2^14 + 256 bytes | `tls-record-overflow` |
| flips one bit in an encrypted record | `tls-bad-record-mac` |
| signs `CertificateVerify` with another key | `tls-bad-certificate-verify` |
| sends a wrong `Finished` | `tls-bad-finished` |
| sends an unpinned certificate | `x509-unknown-issuer` |
| sends a fatal alert | `tls-alert` |

The list goes in the issue's closing comment, each case marked passed, refused for the right reason, or not applicable.

## 7. Mutants (PR 3)

At least 15, each killed by §6:
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

`tls.drop` and the end of a connection overwrite the slot's secrets, keys, IVs and X25519 secret with zeros. The language
guarantees no erasure (`docs/tls-pure.md` §7.3): the optimiser may remove stores to memory that is not read again, and freed
memory is not cleared. So this is best effort, and the package says so. Because the slot's boxes live as long as the engine,
the zeroing stores are not to memory about to be freed. That is the case where an optimiser removes them most readily.
