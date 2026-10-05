# TLS 1.3 session resumption for `packages/tls`: the design

> **Status: design, not built.** #210 measured the pure client's full handshake at about 2.8 ms of CPU in `lexsys-hooks`, 4 to 7
> times OpenSSL's, with no resumption (`docs/tls-hooks.md` §9). `docs/tls-pure.md` §7.2 left resumption out "until it is designed with
> its hazard: a resumed session skips verification". This is that design. Its claims are measured on the machine named, or say they
> are arithmetic from measured parts; where a later PR finds one false, that PR corrects it here, in place.

---

## 1. Where the 2.8 ms goes

Each operation measured alone, on the machine of #210's handshake measurement (a 6-vCPU Linux VM on an Apple M4 Max, LLVM backend,
lex-sys `7c1bd08`), with the existing benchmarks:

| Operation | Command | Cost | In a full handshake | Total |
|---|---|---|---|---|
| X25519 | `scripts/curve25519_bench.py` (200 rounds) | 0.569 ms | twice: the client's share, then the shared secret | 1.14 ms |
| ECDSA P-256 verification | `scripts/ecdsa_bench.py` | 0.817 ms | twice: the CA's signature on the leaf, and CertificateVerify | 1.63 ms |
| **sum** | | | | **2.77 ms** |

hooks' `https_cost.py` measured a full handshake at 2.9 ms per delivery with a floor of 0.03 to 0.12 ms without TLS
(`lexsys-hooks` `docs/pure-tls.md`). So **these four operations are the handshake**: everything else (hashing, the key schedule,
the AEAD, parsing) is a few percent. That decides what resumption can save. *A certificate chain with an intermediate adds one
verification per certificate; an RSA chain costs differently and was not measured.*

## 2. The options, and what each saves

The savings are arithmetic from §1, not measured end to end. §8 makes measuring them the build's gate.

| Option | What it removes | Handshake after it | Forward secrecy | Size of the change |
|---|---|---|---|---|
| **A. PSK with (EC)DHE** (`psk_dhe_ke`, RFC 8446 §4.2.9) | both ECDSA verifications: no Certificate, no CertificateVerify | **about 1.2 ms** (2.3 times cheaper) | **kept**: a fresh X25519 exchange | the protocol change below |
| B. PSK only (`psk_ke`) | all four operations | about 0.1 ms | **lost** for resumed connections: the PSK alone protects them | the same, minus the key share |
| C. Cache a verified chain | the CA signature on a leaf verified before (same bytes, same trust store) | about 2.0 ms | kept | small, no protocol change |
| D. Keep the connection (in hooks) | the whole handshake, for every delivery after the first on that connection | 0.01 ms a request (`docs/tls-nonblocking.md` §8.2) | kept | a change to hooks' attempt table, deadlines and retry accounting, not to TLS |

**The recommendation is A.** It keeps forward secrecy, which matters for webhooks: a payload is the customer's data, and a
ticket stolen later should not decrypt it. B saves more
and is refused: a client that offers only `psk_ke` gives up forward secrecy for every resumed connection, and the extra 1.1 ms is
not worth that. C is compatible with A and helps the connections A cannot (the first, and servers that issue no ticket), so it is
§9's question 3. D is the larger saving and belongs to hooks (`docs/tls-nonblocking.md` §10.4 already says "resumption first").

**What A gives hooks, by the same arithmetic:** about 1.2 ms a resumed delivery, about 800 `https` deliveries a second a core,
against about 340 now and 4,000 to 5,000 for OpenSSL resumed on the same machine. **OpenSSL stays faster** (its X25519 took 0.038 ms
against this client's 1.08 ms on the Xeon of `docs/x25519.md` §6), and resumption does not change that.

## 3. The hazard, and the rules that bound it

**A resumed connection skips certificate verification.** The server proves it holds the PSK instead, and the PSK came from a
connection that *was* verified. So a ticket is a stored verdict, and every way that verdict could go stale is a rule:

1. **Bound to the name.** A ticket is offered only for the exact host name it was received for (the SNI and the name checked
   against the certificate). Never for another name, whatever the address. (`docs/tls-nonblocking.md` §10.4 said this for OpenSSL.)
2. **Bound to the trust store.** The engine counts its `trust` calls. A ticket records the count when it was stored, and is not
   offered after the trust store changes: a root removed must not stay trusted through a ticket.
3. **Bound to the certificate's life.** A ticket records the leaf's `notAfter`, and is not offered after it, even if the server's
   `ticket_lifetime` says it may be. A full handshake would refuse an expired leaf, and a resumption must not accept one.
4. **Bounded in time.** The shortest of the server's `ticket_lifetime`, RFC 8446's 7-day ceiling (§4.6.1), and the caller's own
   limit (§4, `max_age_s`), which defaults to **1 hour**. *Why 1 hour: a certificate revoked is still trusted by every ticket issued
   before; there is no revocation checking at all (`docs/tls-pure.md`), and the window should be short.*
5. **Used once.** A ticket is removed when it is offered. RFC 8446 Appendix C.4 says clients SHOULD NOT reuse a ticket, so a
   passive observer cannot link connections. A server that sends several tickets lets the client keep a few (§4).
6. **No early data.** 0-RTT is not offered: its data can be replayed (RFC 8446 §8), and hooks' requests are not idempotent on the
   receiver's side.
7. **TLS 1.3 only.** TLS 1.2 resumption (session ids, RFC 5077 tickets) is not offered. A TLS 1.2 connection is always a full
   handshake.
8. **Nothing about it is trusted from the server.** The ServerHello's `pre_shared_key` must name the one identity offered (index
   0), the suite must hash with the PSK's hash, `psk_dhe_ke` means a `key_share` must be there, and a server that does not resume
   is a full handshake with the full verification, never a failure.

## 4. The interface

hooks' OpenSSL module already has the shape (`save_session`, `open(..., session)`, `free_session`, one integer a session). The
engine keeps the secrets, so the caller holds **a handle**, never a PSK:

```
tls.save(engine, slot) -> handle | 0        // after a connection: keep its newest ticket (0: none, or not resumable)
tls.start_with(engine, slot, host, now_unix_ms, handle) -> 0 | refusal   // offer the ticket if every rule of §3 allows; else a full handshake
tls.forget(engine, handle)                   // the endpoint changed, or the caller is done with it; overwrites the secret
tls.resumed(engine, slot) -> bool            // known once established
tls.set_ticket_max_age(engine, seconds)      // §3 rule 4; default 3,600
```

- **Storage is the engine's, bounded:** a ticket table of `slots` entries (64 for hooks), each up to **2,048 bytes of ticket**
  plus the PSK and §3's fields. A ticket over 2,048 bytes is not stored (the connection is fine; it is not resumable), so a
  server cannot make the engine grow. That is about 140 KiB for 64, beside the 11.2 MiB of slots (`docs/tls-pure.md` §7.4). *The
  2,048 is a guess at what real servers send; the build measures what each server of §5's matrix sends and corrects it here.*
- **A handle is a generation-tagged index,** so a handle that was forgotten, or whose entry was reused, is refused (`0`), never
  someone else's ticket.
- **`start` is unchanged.** `start_with` with handle 0 is `start`.

## 5. The protocol work in `packages/tls`

- **NewSessionTicket is kept, not dropped** (`message.ls` parses it and drops it today): `ticket_lifetime`, `ticket_age_add`,
  `ticket_nonce`, the ticket, and an `early_data` extension, which is ignored.
- **The resumption secret** (RFC 8446 §7.1: `resumption_master_secret`, from the master secret and the transcript through the
  client's Finished), and each ticket's PSK, `HKDF-Expand-Label(resumption_master_secret, "resumption", ticket_nonce, Hash.length)`.
- **The ClientHello**, when resuming: `psk_key_exchange_modes` with `psk_dhe_ke` only; a `key_share` as now; `pre_shared_key`
  last, with one identity, its `obfuscated_ticket_age` (`ticket_age_add` added, mod 2^32), and the binder: an HMAC over the
  truncated ClientHello with the binder key (RFC 8446 §4.2.11.2).
- **The server's choice:** a ServerHello with `pre_shared_key` selecting 0 goes straight to EncryptedExtensions and Finished, no
  Certificate or CertificateVerify, with the PSK in the key schedule (`early_secret` from the PSK, not zeros). Without it, the full
  handshake, verified.
- **HelloRetryRequest with a PSK:** the second ClientHello recomputes the binder over the new transcript (§4.2.11.2), and the
  ticket age is recomputed.

**Refusals, each with its tag** (CLAUDE.md), all `tls-` and all a failed connection:
- the ServerHello selects an identity other than 0 (`tls-illegal-psk`, a new tag);
- `pre_shared_key` in a ServerHello to a ClientHello that offered none, or with a suite whose hash is not the PSK's (`tls-illegal-psk`);
- a resumed ServerHello without `key_share` (`tls-key-share`: `psk_ke` was not offered);
- a Certificate or CertificateRequest after a resumed ServerHello (`tls-unexpected-message`);
- a NewSessionTicket that does not parse is still `tls-decode-error`, as now.

## 6. In `lexsys-hooks`

`tlsx` maps its `save_session`, `free_session` and `open(..., session)` onto §4, as `src/tls.ls` maps them onto OpenSSL's. The
generated build changes in one place (`open`'s session argument is kept, not removed), and `tests/sessions_test.py` runs on both
builds. `docs/pure-tls.md`'s "differences" row for resumption goes, and the cost table gains a measured "resumed" row.

## 7. How it is tested

- **The key schedule against RFC 8448 §3 and §4.** §3's simple handshake issues a ticket, and §4 resumes with it. Both use
  `TLS_AES_128_GCM_SHA256`, which the client has since #207 (`docs/tls-core.md` §6.1 could not use them before for that reason). The
  resumption master secret, the PSK, the binder key and the binder are checked byte for byte. *§4 is a 0-RTT resumption, and this
  client sends no early data, so its traffic is not replayed; its values are what is checked.*
- **Interop:** resumption against every server of `docs/tls-assurance.md` §5 that issues TLS 1.3 tickets: the row resumes, with no
  Certificate on the wire, and fetches the body.
- **Differential:** the same resumption by `openssl s_client -sess_out` / `-sess_in` and by this client against the same server, and
  §3's rules against `scripts/tls_liar.py`: a liar that selects identity 1, omits `key_share`, changes the suite's hash, sends a
  Certificate after resuming, sends a 64 KiB ticket, a ticket with a lifetime over 7 days.
- **The rules, each a test that fails if broken:** a ticket never offered to another name, after a `trust` call, after the leaf's
  `notAfter`, after `max_age_s`, or twice.
- **Fuzzing:** `fuzz_flight` (`docs/tls-assurance.md` §3) gains NewSessionTicket bodies, and a new harness resumes from a fixed
  ticket into a fuzzed server flight.
- **Mutants:** at least 12 killed, every survivor argued.
- **The timing test** is unchanged: no new secret-dependent arithmetic, but the binder's HMAC compares nothing (the server checks
  it, not this client).

## 8. Gates

The build PR shows, each with its command:
- §7's tests, on both backends;
- a resumed handshake's CPU, measured, beside §2's arithmetic (about 1.2 ms), with the machine;
- hooks: `sessions_test.py` passing on both builds, `https_both.py` with the resumption difference gone, and the cost table's
  resumed row;
- `docs/tls-pure.md` §7.2 corrected (resumption is no longer left out), and the ticket table's memory in §7.4.

## 9. Open questions, for a person

1. **`max_age_s`'s default.** *Proposed: 1 hour* (§3 rule 4), because there is no revocation checking. A day would resume more
   often for an endpoint delivered to rarely.
2. **Several tickets per server.** *Proposed: keep only the newest.* RFC 8446 Appendix C.4's single use needs a new ticket per
   connection, and servers send one or two after each handshake, so the newest is always fresh for the next connection.
3. **Option C (cache a verified chain) as well.** *Proposed: not now.* It saves 0.8 ms on connections A cannot resume, and it
   is a cache of a security verdict with its own rules (§3's rules 2 and 3 again). Decide after measuring how often A resumes.
4. **Resumption on by default in hooks' pure build.** *Proposed: yes, as OpenSSL's is* (`tls-resume`, default on).

## 10. Not done here

- **Any code.** After this: the protocol and the engine API (`packages/tls`), then hooks' adapter and the measurement.
- **0-RTT, TLS 1.2 resumption, `psk_ke`:** refused by §3, not deferred.
- **Keeping connections open in hooks** (option D): the bigger saving, and hooks' design to make.
- **Making X25519 faster:** the 1.14 ms that resumption keeps. That is arithmetic work in `std.x25519` and `std.field25519`, and is
  measured there.
