# The assembled TLS stack under test: fuzzing, interop, differential, timing and resource bounds

> **Status: design (#208, PR 1).** #208 is the bar the self-contained TLS client (#197) has to clear as one stack, after each
> part has cleared its own: `std.der` and `packages/x509` (#202, #206), the record layer and the handshake (#205, #207). This
> document says how each of #208's five bars is met, with what tools, and in which PR. Its claims about tools and rates were
> measured on the machine in §2; where a later PR finds one false, that PR corrects it here, in place.

---

## 1. What #208 asks, and the plan

| Bar | What #208 asks | Where here | PR |
|---|---|---|---|
| No input reaches a panic | a coverage-guided or structure-aware fuzzer over the record layer, the handshake messages, the DER reader and the chain builder; the corpus and harnesses committed; at least 100 million executions in total, with the hours and the machine; every crash listed with its fix | §3 | 2 |
| Differential | the same handshakes against OpenSSL and rustls servers; a malformed-input matrix on which this client and `openssl s_client` agree on accept or refuse | §4 | 3 |
| Interop | OpenSSL 3, a Python `ssl` server, nginx, and anything else installable offline, listing what was and was not available | §5 | 3 |
| Timing | a dudect-style test of the X25519 ladder and the AEADs with two secret classes, its method, its sample size, and what it can and cannot show | §6 | 4 |
| Resource bounds | a hostile server (the largest legal handshake, endless alerts, a stalled connection, a chain at the depth and size limits) cannot make one connection use more than a stated memory or time | §7 | 4 |

**The gate:** the numbers in `docs/tls-pure.md`, each with the command that produces it, and a CI job running the short version
(fuzzing for 10 minutes, the interop matrix, the differential set). Each PR adds its numbers to this document and its section
of the CI job (§8). The last one points `docs/tls-pure.md` §9 at them.

## 2. The tools, measured

On the session machine: 4 cores, 15 GiB, Ubuntu 24.04, x86-64.

| Tool | Version | How it got here |
|---|---|---|
| AFL++ | 4.09c, on clang 17 | `apt-get install afl++` |
| libFuzzer's runtime | clang 18 | `apt-get install libclang-rt-18-dev` |
| OpenSSL | 3.0.13 | present |
| Python `ssl` | on OpenSSL 3.0.13 | present |
| tlslite-ng | 0.8.2 | present |
| nginx | 1.24.0 | `apt-get install nginx` |
| GnuTLS (`gnutls-serv`) | 3.8.3 | `apt-get install gnutls-bin` |
| Go `crypto/tls` | Go 1.24.7 | present |
| rustls | 0.23.45, from crates.io, on `ring` | `cargo`, through the proxy (`scripts/interop/rustls_server`, its own workspace) |
| wolfSSL | 5.6.6 | `apt-get install libwolfssl-dev`, a server built against it (`scripts/interop/wolfssl_server.c`) |
| mbedTLS | 2.28.8 | the same (`scripts/interop/mbedtls_server.c`). 2.28 has no TLS 1.3 server, so it is a TLS 1.2 row only |
| Botan | 2.19 | `apt-get install botan` (its `tls_server` command). *Corrected (PR 3): that command has no mode that answers and closes; `scripts/interop/botan_server.cpp` is a server on `libbotan-2-dev` instead* |
| BoringSSL | Android's build, 14.0.0+r11 | *PR 3: it ships `bssl-tool`, whose server binds only IPv6, which this container lacks; `scripts/interop/boringssl_server.c` is a server on `android-libboringssl-dev` instead* |

"Installable offline", in #208's words, is read as "installable on a machine with the distribution's and crates.io's
mirrors". The CI job (§8) installs the same packages, on `ubuntu-latest`.

## 3. Fuzzing

### 3.1 The fuzzer: AFL++, with no change to the compiler

The LLVM backend compiles through `clang`, named by the `CLANG` environment variable, and links with `cc`, named by `CC`
(`crates/cancho-codegen-llvm/src/lib.rs`, `crates/cancho/src/main.rs`). Setting both to `afl-clang-fast` instruments
every edge of a cancho program. Measured on `examples/hello.cho`: the instrumented binary runs, and `afl-showmap` reports its
edges. Nothing in the compiler changes, and the code fuzzed is the code shipped: the same IR, through the same `-O2`.

**libFuzzer was the alternative.** It needs a C entry point, `LLVMFuzzerTestOneInput`, that calls into the program. cancho
exports no symbol but `main`, so libFuzzer would need either a language change (exported functions) or a rewrite of the
emitted module's `main`. AFL++ runs `main` unchanged in a fork server, reading each input from standard input. It is slower
than an in-process loop. §3.4 measures how much.

### 3.2 What a harness may feed: only bytes an attacker controls

**The first measurement was the wrong harness.** `tests/programs/tls_driver.cho`, the existing test driver, under AFL++ for 60
seconds from one recorded handshake: 116,338 executions, 1,939 a second, and **107 inputs that trap**. Every one of them, sorted by
the backtrace's top two frames under `gdb`:

- in the driver's own hex decoding, given a line that is not hex;
- in the driver's `S`, `O`, `T` and `U` operations, which call `tls_record.seal` and `open` on keys and IVs taken from the line.
  Given a key or IV of the wrong length, `nonce12` indexes past it. The client never does this: it derives every key at its
  suite's length.

**None is in code a server's bytes reach.** They are a harness feeding inputs the API's contract excludes. So every harness
here reads **raw bytes** (no text encoding to mutate past) and feeds them **only where an attacker's bytes go**: into `feed`,
into a parser of what a server sends, or into the chain builder as a server's certificate list. The caller's own arguments
(keys, the host name, the trust store, the time) are fixed by the harness, at values the client itself would use.

### 3.3 The harnesses

Each is `tests/programs/fuzz_<name>.cho`. It reads one input on standard input, runs it, and exits 0, so any trap is a crash
to AFL++.

| Harness | Input | What it reaches |
|---|---|---|
| `fuzz_der` | one certificate's DER | `x509.parse`: the DER reader and the certificate's fields |
| `fuzz_chain` | a server's certificate list, as TLS 1.3 sends it | `x509_verify`, through the chain builder, against a fixed trust store, host name and time: path building, name matching, constraints, signatures |
| `fuzz_messages` | a byte choosing the message type, then a handshake message's body | every parser in `message.cho` for what a server sends, TLS 1.3 and 1.2 |
| `fuzz_client` | the server's bytes, cut into `feed` calls at lengths the input names | the whole client from `start`: record framing, the ServerHello, HelloRetryRequest, and every refusal before the first encrypted record |
| `fuzz_flight` | the server's handshake messages in **plaintext**, after a fixed ServerHello | the whole client past the AEAD: the harness plays the server, holds the server's X25519 key, derives the handshake keys and seals each message before `feed`. So EncryptedExtensions, Certificate, CertificateVerify, Finished, the post-handshake messages, and the TLS 1.2 flight (Certificate, ServerKeyExchange, ServerHelloDone, Finished) are fuzzed through the real record layer. *Corrected (PR 2): TLS 1.3 only. A TLS 1.2 server sends Certificate, ServerKeyExchange and ServerHelloDone in the clear, so `fuzz_client` reaches them from the recorded TLS 1.2 handshakes. And the harness holds no key: it reads the client's current read key, IV and sequence number from its slot before sealing each record, which follows every KeyUpdate with no key schedule of its own* |

**`fuzz_flight` is the structure-aware one.** Without it a mutation of an encrypted record fails its tag, and nothing past
the record layer is reached. The server side's key derivation uses the package's own key schedule (`tls_record`, `std.hkdf`).
That is enough for the no-trap bar: whether the schedule is *right* is what the RFC 8448 vectors and the interop matrix check,
not the fuzzer.

**Determinism.** The client's randomness is fixed (the 96 bytes the traces use), so the same input always takes the same
path, and a crash replays.

### 3.4 Rates, and the budget

Measured for 60 seconds each, one core, fork-server mode:

| Harness | Executions a second | Edges found of those instrumented |
|---|---|---|
| the text driver (§3.2, for comparison) | 1,939 | 1,222 of 8,527 |
| a prototype of `fuzz_der` | 5,518 | 411 of 843 |

At those rates, four cores give about 8,000 to 22,000 executions a second, so **100 million executions take 1.3 to 3.6 hours**.
That is at least 100 million in total, split so each harness gets at least 10 million. *Corrected (PR 2): the full handshakes run at 135 to 197 executions a second (§3.6), so 10 million for `fuzz_client` and `fuzz_flight` would take about 15 hours a core. The split follows the rates instead.* The rest goes to `fuzz_flight` and
`fuzz_chain`, which reach the most code. PR 2 reports, per harness:
- the executions;
- the hours;
- the edges found;
- the crashes and hangs.

It also gives the command that reproduces the run. AFL++'s persistent mode would be several times faster, but it needs its
`__AFL_LOOP` macro in C. That is the same entry-point problem as libFuzzer, so it waits until the rate matters.

### 3.5 The corpus, and what happens to a crash

**The corpus.**
- **Seeds:** the recorded handshakes (`tests/vectors/tls/`), cut into each harness's input format, and `tests/vectors/x509/`'s
  certificates.
- **What is committed:** after the run, `afl-cmin` minimises the queue, and that goes in under `tests/vectors/fuzz/<harness>/`.

**The conformance test** (`crates/cancho/tests/conformance/tls_fuzz.rs`; *corrected (PR 2): first named `fuzz.rs` here*) builds every harness on both backends and runs every
committed input, so the corpus is a regression test from then on.

**A crash** is any input that ends the process by a signal: a trap is cancho's bounds or overflow check, and `ud2` is
`SIGILL`. Each one is:
- minimised (`afl-tmin`);
- located (`gdb`'s backtrace);
- fixed, with a refusal tag if the fix is a new refusal (CLAUDE.md);
- committed as a regression input.

PR 2 lists each with its cause and fix. A **hang** (AFL++'s default limit, 1 s) is treated the same way: a server's bytes
must not make the client loop.

### 3.6 The campaign (PR 2)

**The machine:** a 6-core Ubuntu 24.04 VM (colima, Apple Virtualization.framework) on the Apple M4 Max of §6.1, linux-aarch64,
with AFL++ 4.09c, whose `afl-clang-fast` is on clang 17, from `apt-get install afl++`. One core a harness, five at once, seeded from
`scripts/fuzz_corpus.py` and the corpus §3.5 committed after the first campaign below. The command:

```
cargo build --release -p cancho
python3 scripts/fuzz_afl.py <work> der:14400 messages:14400 chain:10800 client:10800 flight:10800
python3 scripts/fuzz_afl.py <work> --report
python3 scripts/fuzz_afl.py <work> --minimize
```

| Harness | Executions | Hours | Per second | Edges | Crashes | Hangs |
|---|---|---|---|---|---|---|
| `fuzz_der` | 75,747,286 | 4.00 | 5,260 | 434 of 853 | 0 | 0 |
| `fuzz_messages` | 71,991,797 | 4.00 | 4,999 | 199 of 469 | 0 | 0 |
| `fuzz_chain` | 22,958,418 | 3.00 | 2,126 | 781 of 4,358 | 0 | 0 |
| `fuzz_client` | 3,468,601 | 3.00 | 321 | 1,767 of 7,998 | 0 | 0 |
| `fuzz_flight` | 2,058,320 | 3.00 | 191 | 1,419 of 8,047 | 0 | 0 |
| **total** | **176,224,422** | 17 core-hours | | | **0** | **0** |

**No crash and no hang, so there is no fix to list.** This run alone is past #208's 100 million.

**The first campaign** ran on the 4-core Xeon of §2 and was stopped after 1.07 hours, when its container was reclaimed:

| Harness | Executions | Per second | Crashes | Hangs |
|---|---|---|---|---|
| `fuzz_messages` | 16,309,924 | 4,240 | 0 | 0 |
| `fuzz_chain` | 3,259,293 | 847 | 0 | 0 |
| `fuzz_client` | 758,408 | 197 | 0 | 0 |
| `fuzz_flight` | 519,727 | 135 | 0 | 0 |
| **total** | **20,847,352** | | **0** | **0** |

That makes 197,071,774 executions across both, on two machines, and on code before and after #270's two fixes to
`message.cho`. This one ran on the code as merged.

**What the edges say, and do not.** `fuzz_der` reaches about half of its harness's instrumented edges, and `fuzz_client` and
`fuzz_flight` about a fifth. Each counts every instrumented edge in the program, the harness's own and code no fuzzed input can reach, so
the fractions are not coverage of the parsers. **Edges were still being found late:** the last new one came at 1.4 hours
(`fuzz_der`), 2.6 (`fuzz_chain`), 2.6 (`fuzz_flight`), 2.7 (`fuzz_client`) and 3.0 (`fuzz_messages`, of 4). A longer run
might find more. The rates make `fuzz_client` and `fuzz_flight`, at a few million executions each, the least explored.

**The corpus.** `--minimize` keeps 1,387 inputs, 1.7 MB: `der` 236, `chain` 236, `messages` 112, `client` 395 and `flight`
408. It replaces the first campaign's 1,089. `conformance/tls_fuzz.rs` replays every one on both backends.

## 4. Differential testing

**The same handshakes.** The client and `openssl s_client` connect to the same server with the same parameters. The
negotiated version, suite, group and signature scheme are compared, along with the received application data. Servers:
- OpenSSL `s_server` and rustls's example server;
- every suite and group each offers.

**The malformed-input matrix.** `scripts/tls_liar.py` is a scripted server that tells 63 kinds of lie (`docs/tls-parity.md`).
Each case is run twice: against this client, and against `openssl s_client` (TLS 1.3 and 1.2, with the same trust store and
host). The outcomes compared are accept or refuse, and for a refusal, the alert sent.

**What counts as agreement** is decided per case before running, and written in the table:
- **Accept or refuse must match.**
- **The alert should match**, with exceptions written in:
  - RFC 8446 lets an implementation choose between `decode_error` and `illegal_parameter` for some faults;
  - OpenSSL sends `unexpected_message` where this client sends something more specific.

A disagreement on accept or refuse is a bug in one of them. PR 3 fixes it here, or says why OpenSSL is the one that is wrong.

### 4.1 Results (PR 3)

**The same handshakes:** `python3 scripts/tls_differential.py --handshakes <tls_many>`, **21 rows, 21 agree.**
- **The rows:** `openssl s_server` with each TLS 1.3 suite, a HelloRetryRequest to P-256 and to P-384, an RSA-2048
  certificate, and each of the six TLS 1.2 suites; rustls with each TLS 1.3 suite and each TLS 1.2 suite.
- **What was compared:** the version, suite and group, a HelloRetryRequest's group, and TLS 1.2's ServerKeyExchange curve
  and signature scheme. They were the same for both clients in every row, and both completed every handshake.
- **What a signature scheme showed:** rustls signs TLS 1.2's key exchange with `rsa_pss_rsae_sha512` (0806), and OpenSSL
  with `rsa_pss_rsae_sha256` (0804). Each server picked the same scheme for both clients.

**The lying server:** `python3 scripts/tls_differential.py`, the 66 connections of `scripts/tls_liar.py` (63 before
this PR) against `openssl s_client`, offering what `packages/tls` offers.

| Outcome | Cases |
|---|---|
| the same: accepted, or refused with the same alert | 44 |
| both refuse, with different alerts | 16 |
| they differ on accept or refuse, on purpose (below) | 6 |
| they differ otherwise | **0** |

The alerts differ where RFC 8446 §6.2 leaves the choice open. Mostly OpenSSL sends `illegal_parameter` (47) where this
client names the fault: `protocol_version`, `handshake_failure`, `decode_error` or `unsupported_extension`. For an all-zero
or low-order X25519 share, OpenSSL sends `internal_error` (80).

**The six differences on purpose** (*eight since review finding E-3: the last row*). `scripts/tls_differential.py` names each
in `EXPECTED`, so one more, or one of these going away, fails the run.

| Case | `packages/tls` | OpenSSL | Why |
|---|---|---|---|
| a HelloRetryRequest cookie of 2,049 bytes | refused | accepted | the slot keeps at most 2,048 (`docs/tls-parity.md` §3.3) |
| a Certificate message over 64 KiB | refused | accepted | the slot's handshake buffer is 64 KiB (`docs/tls-pure.md` §7.4); OpenSSL's default is 100 KiB (`SSL_MAX_CERT_LIST_DEFAULT`) |
| `DOWNGRD\x00` in a TLS 1.2 ServerHello | refused | accepted | RFC 8446 §4.1.3: a client that offered TLS 1.3 MUST refuse either sentinel there. OpenSSL refuses `DOWNGRD\x01` and takes `\x00` |
| TLS 1.2 without the extended master secret | refused | accepted | required, by #207's decision |
| a TLS 1.2 ServerHello echoing the client's session id | refused | accepted | RFC 5246 §7.4.1.3: an echoed id resumes that session, and the client offered none |
| a TLS 1.2 HelloRequest | refused | renegotiates | no renegotiation, by #207's decision |
| *Added after review finding E-3 (#209):* a `close_notify` instead of ServerHello, or in the encrypted flight | refused, `tls-peer-closed` | ends quietly, no alert or error | before the handshake completes a `close_notify` may be forged, and nothing authenticated has ended |

**Found, and fixed here.** OpenSSL accepted a TLS 1.3 ServerHello whose random ends in a downgrade sentinel, and this client
refused it. RFC 8446 §4.1.3 has the check only in a ServerHello for TLS 1.2 or below. `docs/tls-parity.md` §3.4 said so too,
but the code checked every ServerHello. It now checks as the RFC says, and the two cases are honest connections.

**Two legal differences the harness absorbs**, so that the liar's script, written for this client, can run against OpenSSL:
- **A KeyUpdate that asks for an answer.** This client answers at once. OpenSSL answers with its next write, which RFC 8446
  §4.6.3 allows. A case that completed its handshake with no alert is counted as accepted, with the step where OpenSSL left
  the script printed beside it.
- **The middlebox change_cipher_spec.** After a HelloRetryRequest, OpenSSL sends it before its second ClientHello, as
  Appendix D.4 allows. This client sends it before its Finished. The harness moves it to where the script expects it, in
  TLS 1.3 only.

**Resumption (#286).** The liar now holds 78 cases, 12 of them about tickets and resumption (`docs/tls-resumption.md`).
For those, the harness resumes OpenSSL for real: the first `s_client` saves the session from the liar's ticket
(`-sess_out`), a second one offers it (`-sess_in`), and `openssl sess_id` reads back the ticket, PSK and lifetime the liar
checks against its own. The liar checks OpenSSL's binder as it checks this client's. With OpenSSL 3.0.13:
- **11 agree.** OpenSSL resumes, takes a declined ticket as a full handshake, and refuses identity 1, a suite of another
  hash, and a Certificate after a resumed ServerHello, with the same alerts as this client.
- **1 differs in the alert only.** A `pre_shared_key` without `key_share`: OpenSSL sends `missing_extension` (109), this
  client `illegal_parameter` (47); RFC 8446 names no alert for this fault in a ServerHello.
- **Found, and fixed here.** A `pre_shared_key` when no ticket was offered: OpenSSL sent `unsupported_extension` (110) and
  this client `illegal_parameter` (47). RFC 8446 §4.2 requires `unsupported_extension` for an extension the client did
  not send, so this client now sends it (`tls-unsupported-extension`), and the two agree.
- **"tickets not kept"** is counted as accepted with a step printed: OpenSSL keeps a ticket of 3,000 bytes, which this
  client does not (`docs/tls-resumption.md` §3), and that is a limit, not a protocol rule.

## 5. The interop matrix

Each server from §2:
- with each certificate type it can serve (RSA-2048, RSA-4096, P-256, P-384, Ed25519);
- with each suite and group it offers;
- TLS 1.3 and, where it has one, TLS 1.2.

The client fetches a body of known length and checks it byte for byte. A row that cannot run (a server without Ed25519, mbedTLS
without TLS 1.3) is listed with the reason, not dropped. The existing `scripts/tls_live.py` covers OpenSSL, Python `ssl` and
tlslite-ng. PR 3 extends it, or adds `scripts/tls_interop.py` beside it, for the rest.

### 5.1 Results (PR 3)

`python3 scripts/tls_interop.py <tls_many>` (86 seconds here, the servers built from source included), and
`scripts/tls_live.py` for OpenSSL's `s_server`, Python `ssl` and tlslite-ng as before. Each row is 8 concurrent connections,
reading one byte at a time and then 65,536.

**126 rows, 126 ok:**

| Server | Version | TLS 1.3 rows | TLS 1.2 rows | Not run, and why |
|---|---|---|---|---|
| Go `crypto/tls` | 1.24.7 | 5 | 11 | TLS 1.3 suites cannot be chosen in Go |
| rustls | 0.23.45 | 8 | 11 | |
| nginx, on OpenSSL 3.0.13 | 1.24.0 | 8 | 11 | |
| GnuTLS `gnutls-serv` | 3.8.3 | 8 | 11 | |
| wolfSSL | 5.6.6 | 7 | 10 | Ed25519: this build has none |
| BoringSSL (Android's) | 14.0.0+r11 | 5 | 11 | TLS 1.3 suites cannot be chosen in BoringSSL |
| mbedTLS | 2.28.8 | none | 10 | TLS 1.3, which 2.28 has no server for; Ed25519 |
| Botan | 2.19.3 | none | 10 | TLS 1.3, which Botan 2 lacks; Ed25519, which its TLS 1.2 does not serve (`openssl s_client` gets `handshake_failure` from it too) |

The rows are:
- every certificate type the server can use (P-256, P-384, RSA-2048, RSA-4096, Ed25519) with its default suite;
- every TLS 1.3 suite it can be told to use;
- every TLS 1.2 suite, ECDSA with P-256 and RSA with RSA-2048.

**Found, and fixed here: nginx's TLS 1.2.** Every TLS 1.2 connection to nginx was refused, `tls-unsupported-extension`.
nginx answers a ClientHello that names a host with an empty `server_name` in its ServerHello, as RFC 6066 §3 says a
server "SHALL" when it used the name. The TLS 1.2 ServerHello parser accepted only `renegotiation_info`,
`extended_master_secret` and `ec_point_formats`, the three `openssl s_server` sends. It now accepts one empty
`server_name`, in TLS 1.2 only: TLS 1.3 sends it in EncryptedExtensions, which already accepted it.

The new rules have their own tests:
- three crafted ServerHellos in `conformance/tls.rs`;
- three lying-server cases;
- two mutants, both killed. `scripts/tls_mutants.py` now has 61 of 61 killed.

**Not available:** none of the servers §2 names is missing. BoringSSL and Botan needed servers of their own (§2).

## 6. Timing

`scripts/gcm_timing.py` (AES-GCM, `docs/tls-parity.md` §3.1) and `scripts/ecdh_timing.py` (P-256 and P-384, `docs/ecdh.md` §3) are
already dudect tests:
- Welch's t on `rdtsc` cycle counts between two classes of secret input, the classes interleaved at random;
- with |t| < 4.5 as the pass.

PR 4 adds the two #208 names that have not had one:
- **X25519's ladder.** The classes: a fixed scalar against random scalars, and a scalar with few bits set against random
  scalars, under one peer point.
- **ChaCha20-Poly1305 `seal` and `open`.** A fixed key against random keys, at a fixed length. For `open`, a valid tag
  against an invalid one, which checks that the tag comparison does not stop early.

Each runs at least 20,000 samples a class, on both backends, and the AES-GCM and ECDH tests are run again on the same machine
for one table.

**What a pass cannot show.** It cannot show a leak smaller than the test's resolution at that sample size, a leak under other
inputs than the two classes, or one on another CPU. The ECDH result on Cranelift (|t| = 11, an identical instruction count,
consistent with data-dependent power, `docs/ecdh.md` §3) is the reminder: cycle counts measure the machine as well as the code.
PR 4 says this beside the table.

### 6.1 Results (PR 4)

**The machine.** An Apple M4 Max, 16 cores, 64 GiB, macOS 26.2, natively (darwin-aarch64). It has no `rdtscp`, so `tick` reads
the generic timer, `cntvct_el0` (the variant in `scripts/gcm_timing.py`'s docstring). `cntfrq_el0` gives 1 GHz and 0.5 s of
`usleep` counted 502,518,167 ticks, so one tick is a nanosecond, not a cycle. **It was not idle**, which is what §6 asks. A
Linux VM holding about twenty idle service containers, Ollama and an iOS simulator were running, and the one-minute load
average was 2.0 to 2.4 of 16 cores throughout. Noise from them widens the variances, which makes a leak harder to see, not
easier.

**The commands**, each program built with `cancho build --std --backend B tests/programs/<x>_timing.cho -l tick -L <dir>`:

```
python3 scripts/x25519_timing.py <exe> 20000
python3 scripts/gcm_timing.py <exe> 20000 --chacha20
python3 scripts/gcm_timing.py <exe> 20000
python3 scripts/ecdh_timing.py <exe> 20000
```

20,000 samples a test, max |t| over dudect's crops, the median in nanoseconds. Three tests fail. The "DIT" column is the same
test with Arm's data-independent-timing bit (`PSTATE.DIT`) set by a constructor in `libtick.a`, run once for the failures and
for LLVM's AES-GCM:

| Test | LLVM | Cranelift | With DIT |
|---|---|---|---|
| X25519, a fixed scalar against random (0.55 ms; 1.32 ms) | 1.31 | 2.50 | |
| X25519, a sparse scalar against random | 1.16 | 1.02 | |
| ChaCha20-Poly1305 seal, key (1.5 µs; 3.0 µs) | 0.91 | 2.11 | |
| ChaCha20-Poly1305 seal, data | 2.75 | 1.17 | |
| ChaCha20-Poly1305 open, data | 1.65 | 2.71 | |
| ChaCha20-Poly1305 open, tag | 1.50 | 2.39 | |
| AES-128-GCM seal, key (7.1 µs; 12.1 µs) | 2.26 | 1.67 | LLVM 1.81, Cranelift 1.29 |
| AES-128-GCM seal, data | 2.21 | **10.73**, again 2.93 | LLVM 2.80, Cranelift 4.46 |
| AES-128-GCM open, data | 1.95 | **34.39**, again **17.54** | LLVM 1.54, Cranelift **5.62** |
| AES-128-GCM open, tag | 1.64 | 2.30 | LLVM 1.67, Cranelift 2.78 |
| P-256, scalar 1 against random (0.71 ms; 2.67 ms) | **16.19** | **8.25** | LLVM 1.84 |
| P-256, a fixed scalar against random | 2.39 | 2.05 | LLVM 2.68 |
| P-384, scalar 1 against random (1.89 ms; 6.56 ms) | **10.40** | **5.84** | LLVM 2.15 |
| P-384, a fixed scalar against random | 2.40 | 1.43 | LLVM 2.50 |

**What passes.** Both of #208's new names, X25519's ladder and ChaCha20-Poly1305, pass on both backends, the tag comparison
included. So does every fixed-key or fixed-scalar test.

**What fails, and what it is not.** On this CPU, with DIT clear, three tests fail:
- **ECDH with scalar 1**, on both backends. On the Xeon only Cranelift's P-384 failed (`docs/ecdh.md` §3).
- **AES-GCM's data tests on Cranelift.** On the Xeon they passed at 10^6 samples (`docs/tls-parity.md` §3.1).

The evidence says the cause is below the instruction stream, not a branch or a secret index:
- **The instruction counts are equal.** Under `valgrind --tool=callgrind --toggle-collect=lexs_std.gcm.open`, on
  linux-aarch64, 200 calls of `gcm.open` on the all-zero input and 200 on random inputs execute **25,392,417** instructions
  each on Cranelift, and **6,937,292** each on LLVM. `docs/ecdh.md` §3 found the same equality for the ladder.
- **DIT removes most of it.** With DIT set, LLVM's scalar-1 tests fall from 16.19 and 10.40 to 1.84 and 2.15, both passes.
  Cranelift's AES-GCM open-data test falls from 34.39 and 17.54 to 5.62, still a fail, and its seal-data test to 4.46, just
  under.

Arm's DIT is the CPU's promise that certain instructions take a time independent of their data. That it moves the result this
much says the M4 runs some instructions faster on these operands (mostly zero limbs and words) when DIT is clear. What it does
not say is which instructions, or what remains in Cranelift's AES-GCM with DIT set. Cranelift's code keeps more in memory
(`docs/tls-parity.md` §3.1: GHASH's words in a bounds-checked array), so a data-dependent effect in the load and store path is
one candidate, and nothing here tests it.

**Since `docs/crypto-builtins.md`'s first PR, a program the LLVM backend builds for aarch64 Linux or Darwin sets DIT itself**,
first thing in `main`, when the operating system says the CPU has it (`crates/cancho-codegen-llvm/src/dit.rs`). Only `main`'s
thread: measured with a C probe, a thread a Linux process makes inherits the bit and one a Darwin process makes starts with it
clear, and hooks makes none. Cranelift has no inline assembly to set it with, so its rows above stand. Re-run on the same M4
(LLVM, 20,000 samples, max |t|, the medians in timer ticks; the machine loaded, load average 3.4 to 4.8), the same compiler
with and without this change:

| Test | Without DIT | With DIT (now the default) | Median, with against without |
|---|---|---|---|
| X25519, fixed / sparse scalar | 1.86 / 1.14 | 2.81 / 1.77 | +3.6% / +2.7% |
| ChaCha20-Poly1305 seal key, seal data, open data, open tag | 2.27, 1.89, 1.88, 2.46 | 1.27, 2.01, 2.76, 1.84 | +5.5%, +5.5%, +3.3%, +3.3% |
| AES-128-GCM seal key, seal data, open data, open tag | 1.76, 3.15, 1.90, 1.89 | 2.19, 2.25, 1.82, 1.81 | +5.8%, +4.6%, +4.7%, +5.0% |
| P-256 scalar 1 / fixed | **9.11** / 1.55 | 2.11 / 1.63 | −4.7% / −6.7% |
| P-384 scalar 1 / fixed | 4.33 / 2.15 | 2.03 / 1.88 | −2.4% / +3.3% |

So the one LLVM failure this run reproduced (P-256 with scalar 1) passes with DIT, as in the table above, and the cost is a
few per cent on the symmetric ciphers. The ECDH medians moving both ways is within this loaded machine's noise.

**What it means.**
- **For ECDH**, what `docs/ecdh.md` §3 says for TLS holds: the client uses a fresh scalar once, and the leak needs a scalar
  that keeps the accumulator at zero, which a random one does not.
- **For AES-GCM**, the secret classes are a record's plaintext (seal) and ciphertext (open). The ciphertext is not secret.
  The plaintext is, and on Cranelift, without DIT, an all-zero 64-byte record and a random one are not equally fast at
  |t| = 10.7 in one run of two.
- **Neither backend sets DIT.** Doing so, in the runtime's start-up or around the AEAD and ladder, is the fix these numbers
  point to, and it is not in this PR. LLVM is the default backend. Cranelift's AES-GCM data tests fail on this machine even
  with DIT set.

**What a pass cannot show:**
- a leak smaller than the test resolves at 20,000 samples;
- a leak under inputs other than the two classes;
- a leak on another CPU. This table is one M4 Max, and the Xeon's tables in `docs/ecdh.md` and `docs/tls-parity.md` disagree
  with it in both directions.

The counter also measures the machine. A constant-rate counter turns a faster clock or a faster instruction on cheap operands
into fewer ticks, whatever the code does.

## 7. Resource bounds

**What is claimed, and how it is measured.** The client is bytes in and bytes out (`docs/tls-pure.md` §2). Its memory is a
fixed slot, and nothing is allocated per connection (`docs/tls-core.md` §3). So:

- **Memory** is the slot's size, whatever the server sends. PR 4 measures it, not assumes it: the peak heap under `valgrind
  --tool=massif` and the peak resident set, for each hostile case below, against an ordinary handshake.
- **Time without input is zero.** A stalled server, the zero window, costs nothing: the client does no work until it is fed,
  and the deadline is the caller's (`cancho-hooks` has one). The measurement is that `feed` of nothing returns at once, in
  every state.
- **Time per byte fed** is bounded by the work the largest legal input causes. The cases, each against a server in
  `scripts/tls_hostile.py`:
  - the largest legal Certificate message, 64 KiB, the reassembly limit (`docs/tls-pure.md` §7.1);
  - a chain at the depth limit (8), with RSA-4096 signatures throughout, the most expensive verification allowed;
  - certificates at the 16 KiB size limit;
  - warning alerts and KeyUpdates sent without end, until the client's limits (`docs/tls-pure.md` §7.1) refuse them;
  - one byte a record, and one byte a `feed`.

  The CPU time per connection is reported for each. The bound stated is the largest measured, with the input that gives it.

### 7.1 Results (PR 4)

`python3 scripts/tls_hostile.py <driver> <tls_many>`, then the same with `--massif` (valgrind 3.22), on linux-aarch64: Ubuntu
24.04 in a 6-core Linux VM on the M4 Max of §6.1, the fuzzing campaign of §3.6 running on five of its cores. Both programs are
built on the LLVM backend with the file lists of `conformance/tls.rs`. Every case ended as the script requires it to.

| Case | Outcome | CPU s | Peak resident KiB | Mapped KiB (massif) |
|---|---|---|---|---|
| an honest handshake | ok | 0.007 | 1,536 | 6,140 |
| the same, one byte a `feed` | ok | 0.008 | 1,416 | 6,140 |
| 32 KeyUpdates, then a 33rd | `tls-too-many-messages` | 0.008 | 1,432 | 6,140 |
| 16 `user_canceled` warnings, then a 17th | `tls-too-many-messages` | 0.007 | 1,436 | 6,140 |
| 10,000 NewSessionTickets | ok | **0.055** | 1,436 | 6,140 |
| a Certificate message of 64 KiB | ok | 0.008 | **1,812** | 6,140 |
| a chain at the depth limit, RSA-4096 throughout | ok | 0.011 | 1,544 | 6,140 |
| a server that sends nothing, 31 s | `timeout` | 0.003 | 2,000 | |
| a server that never reads, 31 s | `timeout` | 0.003 | 1,996 | |

The CPU and resident columns are from the run without valgrind. The stalls run on `tls_many`, a different program, so their
resident set is not comparable with the rows above them.

**The bounds, the largest measured:**
- **Memory: 6,140 KiB mapped, the whole process, in every case.** What a hostile server sends does not change what a
  connection maps. The peak resident set is at most 1,812 KiB, for the 64 KiB Certificate, 276 KiB above the honest
  handshake. That difference is the driver's own: it holds each `feed`'s hex line on its heap before decoding it. The
  mapped peak, which counts that heap, does not move.
- **CPU: 0.055 s for one connection, the 10,000 NewSessionTickets**, about 5 µs a ticket. The two cases refused at a
  limit cost 0.007 and 0.008 s, as the honest handshake does.
- **A stalled server costs 0.003 s of CPU over 31 s.** The client does no work until it is fed. The connection ends at the
  caller's deadline, `tls_many`'s 30 s.

*Corrected (PR 4): an earlier run, in PR 2's session on the x86-64 machine of §2, gave 6,460 KiB mapped in every case and 0.01
to 0.14 CPU seconds a connection, with each stall about 0.006 s of CPU over 30 s. The numbers above are this machine's. The
claim they support, that a connection's memory is the same whatever the server sends, holds on both.*

**Two things the measurement found:**
- **The test driver trapped on a `feed` over 64 KiB.** The 64 KiB Certificate made `tests/programs/tls_driver.cho` trap, on
  its own region allocation: one allocation over 64 KiB in a region traps by design. The client was not at fault. The
  driver now keeps its input on the heap.
- **The client accepts filler certificate entries that are not on the chain's path.** The 64 KiB case is the leaf and seven
  entries of zero bytes, which are not DER. The verifier parses every entry, refuses the connection if the leaf does not
  parse, and leaves any other entry that does not parse out of path building (`packages/x509/verify.cho`, `x509_verify`).
  RFC 8446 §4.4.2 allows a server to send certificates the path does not use. The cost of the junk is bounded by the
  64 KiB handshake limit and the eight-entry limit, and the table measures that cost.

## 8. The CI job

A job `tls-assurance`, Linux only:
1. `apt-get install afl++ nginx gnutls-bin`, and the other §2 packages whose servers the matrix uses;
2. build the harnesses instrumented, and run each for its share of 10 minutes from the committed corpus, failing on any crash
   or hang. The share is 2 minutes each, all five at once (`fuzz_afl.py ... der:120 chain:120 messages:120 client:120
   flight:120`), so the job spends 2 minutes of wall time on it;
3. the interop matrix, against local servers only;
4. the differential set.

The conformance test (§3.5) already runs every committed fuzz input on both platforms in the existing job. The timing tests
are not in CI: shared runners are too noisy for a t-test to mean anything, and a flaky timing gate teaches people to ignore it.

## 9. Not done here

- **Fuzzing the Cranelift backend's code.** AFL++ instruments through `clang`, so only the LLVM backend's object is fuzzed. The
  committed corpus runs on both backends (§3.5), so a crash found on LLVM is checked on Cranelift. A crash only Cranelift's
  code would have is not searched for.
- **A server.** There is none to fuzz: the package is a client. *Since `docs/tls-server.md` step 2 there is one, and it has
  two harnesses of its own, `fuzz_hello` and `fuzz_server` (`docs/tls-server.md` §10.5).*
- **Persistent-mode fuzzing**, until the fork-server rate is the bottleneck (§3.4).
