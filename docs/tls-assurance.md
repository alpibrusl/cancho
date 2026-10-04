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
| rustls | 0.23, from crates.io | `cargo`, through the proxy |
| wolfSSL | 5.6.6 | `apt-get install libwolfssl-dev`, a server built against it |
| mbedTLS | 2.28.8 | the same. 2.28 has no TLS 1.3 server, so it is a TLS 1.2 row only |
| Botan | 2.19 | `apt-get install botan` (its `tls_server` command) |
| BoringSSL | Android's build | `apt-get install android-boringssl`, if it ships the `bssl` tool; PR 3 says whether it did |

"Installable offline", in #208's words, is read as "installable on a machine with the distribution's and crates.io's
mirrors". The CI job (§8) installs the same packages, on `ubuntu-latest`.

## 3. Fuzzing

### 3.1 The fuzzer: AFL++, with no change to the compiler

The LLVM backend compiles through `clang`, named by the `CLANG` environment variable, and links with `cc`, named by `CC`
(`crates/lex-sys-codegen-llvm/src/lib.rs`, `crates/lex-sys/src/main.rs`). Setting both to `afl-clang-fast` instruments
every edge of a lex-sys program. Measured on `examples/hello.ls`: the instrumented binary runs, and `afl-showmap` reports its
edges. Nothing in the compiler changes, and the code fuzzed is the code shipped: the same IR, through the same `-O2`.

**libFuzzer was the alternative.** It needs a C entry point, `LLVMFuzzerTestOneInput`, that calls into the program. lex-sys
exports no symbol but `main`, so libFuzzer would need either a language change (exported functions) or a rewrite of the
emitted module's `main`. AFL++ runs `main` unchanged in a fork server, reading each input from standard input. It is slower
than an in-process loop. §3.4 measures how much.

### 3.2 What a harness may feed: only bytes an attacker controls

**The first measurement was the wrong harness.** `tests/programs/tls_driver.ls`, the existing test driver, under AFL++ for 60
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

Each is `tests/programs/fuzz_<name>.ls`. It reads one input on standard input, runs it, and exits 0, so any trap is a crash
to AFL++.

| Harness | Input | What it reaches |
|---|---|---|
| `fuzz_der` | one certificate's DER | `x509.parse`: the DER reader and the certificate's fields |
| `fuzz_chain` | a server's certificate list, as TLS 1.3 sends it | `x509_verify`, through the chain builder, against a fixed trust store, host name and time: path building, name matching, constraints, signatures |
| `fuzz_messages` | a byte choosing the message type, then a handshake message's body | every parser in `message.ls` for what a server sends, TLS 1.3 and 1.2 |
| `fuzz_client` | the server's bytes, cut into `feed` calls at lengths the input names | the whole client from `start`: record framing, the ServerHello, HelloRetryRequest, and every refusal before the first encrypted record |
| `fuzz_flight` | the server's handshake messages in **plaintext**, after a fixed ServerHello | the whole client past the AEAD: the harness plays the server, holds the server's X25519 key, derives the handshake keys and seals each message before `feed`. So EncryptedExtensions, Certificate, CertificateVerify, Finished, the post-handshake messages, and the TLS 1.2 flight (Certificate, ServerKeyExchange, ServerHelloDone, Finished) are fuzzed through the real record layer |

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
That is at least 100 million in total, split so each harness gets at least 10 million. The rest goes to `fuzz_flight` and
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

**The conformance test** (`crates/lex-sys/tests/conformance/fuzz.rs`) builds every harness on both backends and runs every
committed input, so the corpus is a regression test from then on.

**A crash** is any input that ends the process by a signal: a trap is lex-sys's bounds or overflow check, and `ud2` is
`SIGILL`. Each one is:
- minimised (`afl-tmin`);
- located (`gdb`'s backtrace);
- fixed, with a refusal tag if the fix is a new refusal (CLAUDE.md);
- committed as a regression input.

PR 2 lists each with its cause and fix. A **hang** (AFL++'s default limit, 1 s) is treated the same way: a server's bytes
must not make the client loop.

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

## 5. The interop matrix

Each server from §2:
- with each certificate type it can serve (RSA-2048, RSA-4096, P-256, P-384, Ed25519);
- with each suite and group it offers;
- TLS 1.3 and, where it has one, TLS 1.2.

The client fetches a body of known length and checks it byte for byte. A row that cannot run (a server without Ed25519, mbedTLS
without TLS 1.3) is listed with the reason, not dropped. The existing `scripts/tls_live.py` covers OpenSSL, Python `ssl` and
tlslite-ng. PR 3 extends it, or adds `scripts/tls_interop.py` beside it, for the rest.

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

## 7. Resource bounds

**What is claimed, and how it is measured.** The client is bytes in and bytes out (`docs/tls-pure.md` §2). Its memory is a
fixed slot, and nothing is allocated per connection (`docs/tls-core.md` §3). So:

- **Memory** is the slot's size, whatever the server sends. PR 4 measures it, not assumes it: the peak heap under `valgrind
  --tool=massif` and the peak resident set, for each hostile case below, against an ordinary handshake.
- **Time without input is zero.** A stalled server, the zero window, costs nothing: the client does no work until it is fed,
  and the deadline is the caller's (`lexsys-hooks` has one). The measurement is that `feed` of nothing returns at once, in
  every state.
- **Time per byte fed** is bounded by the work the largest legal input causes. The cases, each against a server in
  `scripts/tls_hostile.py`:
  - the largest legal Certificate message, 64 KiB, the reassembly limit (`docs/tls-pure.md` §7.1);
  - a chain at the depth limit (8), with RSA-4096 signatures throughout, the most expensive verification allowed;
  - certificates at the 16 KiB size limit;
  - warning alerts and KeyUpdates sent without end, until the client's limits (§7.1) refuse them;
  - one byte a record, and one byte a `feed`.

  The CPU time per connection is reported for each. The bound stated is the largest measured, with the input that gives it.

## 8. The CI job

A job `tls-assurance`, Linux only:
1. `apt-get install afl++ nginx gnutls-bin`, and the other §2 packages whose servers the matrix uses;
2. build the harnesses instrumented, and run each for its share of 10 minutes from the committed corpus, failing on any crash
   or hang;
3. the interop matrix, against local servers only;
4. the differential set.

The conformance test (§3.5) already runs every committed fuzz input on both platforms in the existing job. The timing tests
are not in CI: shared runners are too noisy for a t-test to mean anything, and a flaky timing gate teaches people to ignore it.

## 9. Not done here

- **Fuzzing the Cranelift backend's code.** AFL++ instruments through `clang`, so only the LLVM backend's object is fuzzed. The
  committed corpus runs on both backends (§3.5), so a crash found on LLVM is checked on Cranelift. A crash only Cranelift's
  code would have is not searched for.
- **A server.** There is none to fuzz: the package is a client.
- **Persistent-mode fuzzing**, until the fork-server rate is the bottleneck (§3.4).
