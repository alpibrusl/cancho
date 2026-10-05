# The pure TLS backend in `lexsys-hooks`: the design (#210)

> **Status: design (#210, PR 1).** #210 makes `packages/tls` selectable in `lexsys-hooks`, runs the whole `https` delivery
> suite on both backends, and measures. Reading both sides to write this found that two sentences of `docs/tls-pure.md` are
> false, so the work is bigger than "switch by dependency". This document corrects them, says what has to be built, and lists
> what a person has to decide. Its claims are from reading the two code bases and from the trials named; where a later PR finds
> one false, that PR corrects it here, in place.

---

## 1. What #210 asks

After #208, which closed the bar for the stack as one:
1. **Selectable by dependency.** "The `packages/tls` API is the same as the OpenSSL backend's."
2. **Every `https` delivery test of `lexsys-hooks`** passes with each backend (a valid chain, a wrong host, expired,
   self-signed, a slow handshake, a server that closes mid-handshake, 64 concurrent deliveries), and **the outcomes are equal**
   (delivered, or failed with which tag).
3. **Measured, side by side, on a quiet machine, with the command:** handshake CPU per connection, handshakes per second per
   core, throughput over an established session, and memory per connection. If the pure backend is slower: by how much, and
   whether it matters at the delivery rates in hooks' `docs/design.md`.
4. **The authority report** of hooks with each backend. "The pure one must need no `Ffi` capability at all", checked.
5. **The default stays OpenSSL** until #209 (the independent review) is closed.

## 2. What reading both sides found

### 2.1 The two interfaces are not the same, and an adapter is needed

`docs/tls-pure.md` §2.1 says "one interface fits both backends with no adapter", and a consumer "switches backend by
dependency, not by code". The two are:

| | hooks' `src/tls.ls` (OpenSSL) | `packages/tls` |
|---|---|---|
| Who owns a connection's state | the caller: `fields()` = 9 integers in the attempt array `at`, from index `b` | the engine: `tls.open(heap, slots)` is one value for the process, and a connection is a slot number in it |
| Who does the socket I/O | the module: `handshake`, `write`, `read` and `shutdown` take the `conns.Table` and the `Poller`, and read and write the socket | the caller: `feed` takes bytes the socket gave, `take` gives bytes for it |
| What a call returns | `done`/`pending`/`failed` for a step; `-1` would block and `-2` reset for `read` | `event`: want_read, want_write, established, closed, failed; `would_block()` |
| Why it failed | `detail_of`: an `X509_V_ERR` number, an OpenSSL error of 16 million or more, `-1` (peer closed) or `-1000 - errno` | `failure` and `refusal_tag`: a tag (`tls-peer-closed`, `x509-name-mismatch`, ...) |
| Capability | takes `ffi: Ffi("libcrypto,libssl")` | takes none |
| Sessions | `save_session`, `free_session`, `open(..., session)` | none (§2.3) |
| Buffers | the caller's `req` slice: `out_max()` = 20,480 bytes for ciphertext waiting for the kernel, `net_max()` = 4,096 for reads | the slot's own |

So the pure backend needs **a module in hooks with `tls.ls`'s public functions, implemented over `packages/tls`**: it does
what `tls.ls`'s private `flush` and `feed` do now (move bytes between the `Conn` and the engine), and maps `event` and
`failure` onto `done`/`pending`/`failed` and `detail_of`. `attempt.ls`'s calls (`open`, `handshake`, `write`, `read`,
`shutdown`, `want`, `watching`, `resumed`, `detail_of`, `stage_of`, `drop`) stay the same shape. It is a translation, not a
rewrite, and `docs/tls-pure.md` §2.1's sentence is corrected below.

### 2.2 One source cannot carry both backends' capability rows

This is the finding that decides the build.

- `attempt.ls` and `hooks.ls` thread `ffi: &f Ffi("libcrypto,libssl")` down to every call into `tls`. In the committed hooks,
  **5 functions of `attempt.ls` and 6 of `hooks.ls` carry an `Ffi` row**: `attempt.advance` and its helpers, and in `hooks.ls`
  `settle`, `conclude`, `sweep`, `delivery_turn`, `run` and `start_tls`. `run` is 1,508 lines.
- A function's row is exact in both directions (`docs/linearity-and-effects.md`): it may not declare an effect it does not
  perform, and it must declare every one it does. So a function that takes an `Ffi("libssl")` and does not use it is not
  accepted as pure, and one that uses the engine instead has a different signature.
- **lex-sys cannot abstract over this.** `docs/effect-polymorphism.md` is a documented no: a row is fixed at the declaration,
  and the language has no function values to be polymorphic over. `docs/package-system.md` has no feature flags and no linking
  options in `[[bin]]`.

So **the two builds cannot share `attempt.ls` and `hooks.ls` as they stand**, and the pure one needs the engine passed where
`ffi` goes. A pure `Engine` is one value for the process (`tls.open(heap, 64)` in `main`), threaded as a `&!` borrow beside
`atab` and `poller`.

### 2.3 The pure client has no session resumption

`docs/tls-pure.md` §7.2 leaves it out, and `docs/tls-parity.md`'s table says "none, as now". Hooks keeps one TLS session per
endpoint, resumes it (about a third less CPU per delivery, `docs/https.md`), counts
`hooks_tls_handshakes_total{result="resumed"}`, and has `tests/sessions_test.py` for it. #210's gates do not list sessions.

### 2.4 "No `Ffi` capability at all" cannot be met by hooks as a whole

Hooks holds `ffi` for `libc` too: its pinned `docs/authority.json` lists `libc:statx` (the modes of the data directory,
`src/perm.ls`) and `libc:prctl`, besides 32 symbols in `libssl` and `libcrypto`. So the property that can be checked, and the
one that matters, is: **in the pure build, no `libssl` and no `libcrypto` scope, none of those 32 symbols, and no `ffi`
reachable from the TLS path**. The `libc` entries stay, and equal the OpenSSL build's. `docs/tls-pure.md` §9's gate 2 ("shows
no `ffi(...)` and no foreign symbols") is corrected below.

### 2.5 Smaller differences

| | OpenSSL backend | pure backend | What it means for the tests |
|---|---|---|---|
| Trust store | the system's default locations, `SSL_CERT_FILE` and `SSL_CERT_DIR` honoured; or exactly `tls-ca-file` | the caller reads a PEM bundle (`tls.trust`): the same `tls-ca-file`, or a bundle file read from a list of known paths and `SSL_CERT_FILE` | `SSL_CERT_DIR` (a directory of hashed certificates) cannot be honoured, and the loss is documented. A bundle of 128 roots takes 138,350 bytes of the engine's 1,048,576 (`packages/tls/tls.ls`) |
| TLS 1.2 | the extended master secret optional | **required** (#207) | a receiver without it fails on the pure backend only: one of the six differences on purpose of `docs/tls-assurance.md` §4.1 |
| Message size | a Certificate over 100 KiB refused | over 64 KiB refused | the same §4.1 list |
| Memory | 26 to 48 KiB a connection (`docs/tls-nonblocking.md` §8.3) | **about 179 KiB a slot, 11.2 MiB for 64** as built (`docs/tls-pure.md` §7.4), plus the roots | measured in §6, not estimated |
| Entropy, the clock | the library's own | the engine is seeded by the caller (32 bytes read from `/dev/urandom` through hooks' `Fs("")`, as its SCRAM nonce is), and `start` takes `now_unix_ms` from hooks' `Clock` | the adapter does both |

The six differences on purpose (§4.1 of `tls-assurance.md`) are the only places the two backends may disagree on accept or
refuse. A test that hits one of them is run on both, and its expected outcome is stated per backend.

## 3. The build

**Packaging first.** Hooks takes a package through the project file as a `.lex-sys-vcs` store pinned by git revision
(`[dependencies.server]` for `packages/http-server`). `packages/tls` and `packages/x509` have no such store. Tried here, on
this machine's compiler: `lex-sys vcs publish --std --dir packages/x509` publishes its modules. `packages/tls` is refused
until x509 is required by lock (`--requires <lock>:<store>`, `docs/vcs-publish.md`), which is the flow the compiler names. The
exact sequence, and whether the store lays out as `lexsys-hooks`'s `[dependencies.*]` expects, is the first step of the build
PR, and a store committed to this repository is that PR's first commit.

**The two builds** are two `[[bin]]` entries in hooks' `lex-sys.toml`: `hooks` (OpenSSL, the default) and `hooks-pure`.

**How the pure build's sources are made** is the decision of §7. The recommendation is that `src/tls.ls`'s OpenSSL module and
a new `src/tls_pure.ls` both declare `module tls`, each in its own source list, and the files of §2.2 are **transformed
mechanically** for the pure build by one script, which is run by the build and is the only place the difference lives. The
transform is:
- `ffi: &f Ffi("libcrypto,libssl")` or `Ffi("libssl")` in a parameter list becomes `engine: &!e tls.Engine`, and `ffi` in a call
  becomes `engine`;
- `ffi("libcrypto")` and `ffi("libssl")` leave a row;
- the functions that make and free the context and the sessions (`start_tls`, `close_tls`, `drop_session`, `keep_session`) are
  replaced by the pure module's own.

The pure build's sources are not committed, so there is no copy to drift. The risk is the other one: a change to the shape of
these functions in `hooks.ls` can defeat the transform. §5's checks catch that at build time, not in production: the transformed
source must compile, and the pure build's authority report must have no `libssl` or `libcrypto`.

## 4. How a failure maps

`attempt.ls`'s `handshake_code` turns `detail_of` into one of hooks' reasons (`cert_untrusted`, `cert_expired`,
`cert_hostname`, `cert_invalid`, `tls_handshake`, `tls_timeout`, `tls_error`). The pure module sets `detail_of` to the number
the **OpenSSL column of `docs/tls-pure.md` §8** gives for the refusal tag, so `handshake_code` is unchanged and a failed
attempt's history means the same under either backend. A tag with no OpenSSL number (`x509-chain-too-large`) is set to a value
that `handshake_code` treats as `tls_error`, and the table says so. `-1` (peer closed) is `tls-peer-closed`, and
`-1000 - errno` a socket failure, as now.

## 5. The tests

Hooks' harnesses start `build/hooks` and drive it from outside (`docs/testing.md` there). Each takes the binary as an argument,
so each runs against `build/hooks` and `build/hooks-pure` unchanged.

- **On both backends, outcomes compared:** `tests/https_test.py`, whose nine groups are #210's list: a good chain, every bad
  certificate (expired, not yet valid, another name, another authority, self-signed, with and without `tls-ca-file`), a receiver
  that closes, speaks garbage, offers TLS 1.1 only or never answers, a 500 and a 410, a retried certificate, the trust store, `kill -9`
  in the middle of a handshake, 64 held handshakes. It needs no database. The comparison is the table of reasons from
  `/metrics` (`hooks_attempt_failures_total`) and the receiver's own record, per case, between the two binaries.
- **On both, beyond the nine groups:** `names_test.py` (no database) and `https_api_test.py` (with one). Whether `attempt_test.py`
  and `reason_test.py` touch TLS is read in the build PR before they are listed.
- **On OpenSSL only, with the reason said:** `sessions_test.py` (no resumption, §2.3).
- **Hooks' unit tests** (`lex-sys test`) and **its mutants** (`tests/mutants/https.py`: 49, 47 killed) are for the OpenSSL
  module. The new module gets its own list, with every survivor argued, as hooks does.
- **The authority check.** `scripts/check-authority.sh` runs for each bin and pins a file each. The pure build's file has no
  `libssl:` or `libcrypto:` line and no `ffi` label for those two, and `docs/authority.json`'s `libc:statx` and `libc:prctl`
  are unchanged. A test asserts it, so it is not a convention.
- **The environment:** hooks' tests are Linux only (`LD_PRELOAD` shims, `gcc`). Here they run in an Ubuntu image on this
  machine's Linux VM, with `libssl-dev` and `standardwebhooks` added.

## 6. The measurements

Both builds on the same machine and day, one at a time, the machine otherwise idle (and saying what else was running if it was
not, as `docs/tls-assurance.md` §6.1 does). `scripts/bench/https_cost.py` in hooks already measures the CPU of a delivery for an
address, a name, `https` and `https` resumed; it is extended for the pure build.

| What | How |
|---|---|
| handshake CPU per connection | `https_cost.py`: the service's CPU per delivery, full handshakes only, both builds; OpenSSL resumed as an extra row |
| handshakes a second per core | the same, pinned to one core, saturated |
| throughput over an established session | one connection, a large response, both builds. ChaCha20-Poly1305 and AES-GCM both, because the two backends may choose differently |
| memory per connection | the service's resident set with 0, 1 and 64 handshakes held, both builds (`https_test.py` already holds 64). The pure engine's 64 slots exist from the start, so the difference at 0 is the finding |

**What is known now, and does not answer it.** Hooks' `docs/https.md` has an `https` delivery at about 1,000 µs of CPU for a
full handshake (and about 700 µs resumed), on a shared 4-core x86-64 VM. The pure client's **X25519 scalar multiplication alone**
takes 0.55 ms on LLVM and 1.32 ms on Cranelift, on an Apple M4 Max (`docs/tls-assurance.md` §6.1), and a handshake also
verifies a certificate chain and signs and verifies more. Those are different machines, so they are not compared, and a
whole handshake has not been measured. The expectation is that the pure backend is several times slower per handshake. That
is a hypothesis for the PR to confirm or correct, and what it means at hooks' rates (about 970 `https` deliveries a second a
core with full handshakes) is §1's last bullet.

## 7. Open questions, for a person

1. **How the pure build's sources are made** (§2.2, §3). *Recommended: a mechanical transform, run by the build, the result not
   committed.* The alternatives:
   - **Duplicate the 11 functions.** Simple, and 1,508 lines of `run` are in the way, so a copy of `hooks.ls` in a second
     source tree would drift from the first within a week.
   - **Refactor first.** Split the loop so that the `Ffi` row sits in a few small functions per backend. The right end state,
     in a 5,357-line file other work is changing at the same time (`lexsys-hooks` has uncommitted work on `feat/dbname`), and a
     large change before any measurement.
   - **Do not ship a second binary:** keep one source, add the engine to the call chain beside `ffi`, and thread both. Hooks
     would then carry `Ffi("libssl")` in the pure build, which defeats the authority gate.
2. **Resumption.** *Assumed: the pure backend reports full handshakes only, and `sessions_test.py` runs on OpenSSL only* (§2.3).
   Resumption in `packages/tls` is a separate decision (`docs/tls-pure.md` §7.2), with the hazard that a resumed session skips
   verification.
3. **`SSL_CERT_DIR`.** *Assumed: not honoured by the pure backend, and the documentation says so* (§2.5).
4. **What "equal outcomes" means where the two differ on purpose** (§2.5). *Assumed: each such case states its expected outcome
   per backend.*
5. **The authority gate.** *Assumed: §2.4's reading, no `libssl` or `libcrypto`, and the `libc` entries unchanged.*

## 8. Not done here

- **Any code.** The packages' stores, the adapter, the transform, the second binary, the tests on both and the measurements are
  the PRs after this one.
- **Making the pure backend the default.** That is #209's, and a person's.
- **Revocation, client certificates, IPv6**: hooks has none today, and this changes none of them.
