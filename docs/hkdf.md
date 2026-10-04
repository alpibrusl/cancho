# SHA-384, HMAC and HKDF for the TLS 1.3 key schedule

> **Status: built. Not independently reviewed.** Sub-issue 4 (#201) of
> the self-contained TLS 1.3 client (#197). The client's key schedule
> (RFC 8446 §7.1) is HKDF over the suite's hash: SHA-256 for
> `TLS_CHACHA20_POLY1305_SHA256`, and SHA-384 if a SHA-384 suite is
> ever added. Building it found that **every hash in `std` trapped on
> a message of 64 KiB**, and so did `ed25519.sign`; §2 covers that, the
> fix, and the two documents it corrects. §8 lists what is not done,
> including the `lexsys-hooks` half of #201.

---

## 1. Where it lives

| What | Where | Why there |
|---|---|---|
| SHA-384, and the streaming SHA-256/384/512 | `std/crypto.ls` | SHA-384 *is* SHA-512's compression (FIPS 180-4 §6.5) with other initial words and a shorter digest. #201 asks for "reuse, no second copy", and `compress512` is private to this file, so SHA-384 goes beside it. It adds eight initial words and three short functions. |
| HMAC | `std/hmac.ls`, `module std.hmac` | its own file, as #197 asks, so it can move to a package |
| HKDF, Expand-Label, Derive-Secret | `std/hkdf.ls`, `module std.hkdf` | likewise |

The hash is named by its **digest length**: 32 means SHA-256 and 48
means SHA-384. Anything else is refused with `hash-unsupported`. That
is the number HKDF and the key schedule already carry
(`Hash.length`), so a second name for the same choice would only be a
chance for the two to disagree.

```
crypto.sha256 / sha384 / sha512(message, digest)            one-shot, as before
crypto.sha{256,384,512}_{init,update,final}(state, ...)     streaming
hmac.sha256(key, msg, out)   hmac.sha384(...)   hmac.mac(hash_len, ...)
hmac.init / update / final(hash_len, state, ...)            streaming
hkdf.extract(hash_len, salt, ikm, prk)
hkdf.expand(hash_len, prk, info, okm)
hkdf.expand_label(hash_len, secret, label, context, okm)    RFC 8446 §7.1
hkdf.derive_secret(hash_len, secret, label, transcript_hash, out)
```

An output's length is the length of the slice the caller passes, so
there is no second length to contradict it. `derive_secret` takes the
transcript's *hash*, not the messages: the transcript hash runs for the
whole handshake, so its state belongs to the caller (#205). It is the
streaming SHA-256 state below.

Every HMAC and HKDF function answers `0` or a negative code, and
`hkdf.refusal_tag(code)` names it. Each code corresponds to one rule a
caller can act on:

| Code | Tag | When |
|---|---|---|
| -1 | `hash-unsupported` | `hash_len` is not 32 or 48 |
| -2 | `hash-output-length` | an output that must be one digest long is not |
| -3 | `hmac-state-length` | a streaming state is not `hmac.state_len(hash_len)` words |
| -4 | `hkdf-length-too-large` | more than `255 * hash_len` bytes asked of Expand (RFC 5869 §2.3), or more than 65,535 of Expand-Label |
| -5 | `hkdf-prk-length` | a PRK shorter than the hash (RFC 5869 §2.3) |
| -6 | `hkdf-label-length` | a label of 0 or more than 249 bytes (`"tls13 " + label` is `<7..255>`) |
| -7 | `hkdf-context-length` | a context over 255 bytes |
| -8 | `hkdf-transcript-hash-length` | a transcript hash that is not one digest long |

A length outside the structure is refused rather than truncated into
some other, valid structure. The hashes keep their old contract
(`docs/crypto.md` §4): an output buffer that is too short traps on its
bounds check. That is a mistake in a program, and no input can cause
it.

---

## 2. The 64 KiB trap, and the streaming hash that removes it

`crypto.sha256` built its padded copy of the whole message in an arena:
`alloc_slice[m](padded_len, ...)`. An arena is one 64 KiB chunk, and
asking it for more traps (`docs/defined-behaviour.md`). So **SHA-256 of
65,536 bytes trapped** (`SIGILL`, status 132), and so did SHA-512.
`ed25519.sign` trapped earlier, at 65,000 bytes, because it copied
`prefix || msg` into an arena before hashing. Measured, not inferred:

```
sha256 of 1,000 / 65,000 / 65,536 / 70,000 bytes of `a`:  exit 0 / 0 / 132 / 132
ed25519.sign of 1,000 / 65,000 / 70,000 bytes:            exit 0 / 132 / 132
```

This corrects three claims made in place:

- `docs/crypto.md` §4 implied `sha256` hashes any message.
- `docs/sha512.md` §3 said the length field is exact "for every message
  this language can hold". The length field was right; the message
  never reached it.
- `lexsys-hooks` `src/sign.ls` boxes its `ipad || message` copy on the
  heap because "a payload can be 64 KiB and an arena is not". It then
  passes that copy to `crypto.sha256`, which copied it into an arena
  again. A delivery whose signed content is 65,536 bytes or more would
  trap the service (§7).

**The fix is a streaming hash.** A state is a `[int]` the caller owns:
the eight running words, the byte count, the buffer, and the message
schedule. `update` compresses whole blocks straight from the caller's
slice, and copies only a partial block into the buffer. `final` pads
inside the buffer. The one-shot functions are `init`, `update` and
`final` on a state in one small arena (1.1 KiB for SHA-256, 1.7 KiB for
SHA-512), so they hash any length. `ed25519.sign` and `verify` stream
`prefix`, `R`, `A` and the message into one state rather than
concatenating them. HMAC and HKDF are built on the same states, so
nothing in this slice allocates in proportion to its input.

**The schedule and buffer live in the state, not in an arena per
call.** The first version of the streaming code opened an arena in
`update`, in `final`, and in each `compress` (for the message
schedule `w`), as the old code did. That made a 64-byte SHA-256 cost
19 µs against the old 10.7 µs, because every arena is a `malloc`.
Moving the buffer and the schedule into the caller's state removed
every allocation but the one-shot wrapper's. §6 has the numbers: the
same 64-byte hash now takes 0.99 µs.

---

## 3. Secrets

HMAC and HKDF run over secret keys: the PSK, the ECDHE shared secret,
and every traffic secret. The rule from `docs/chacha20.md` §3 applies
here too: no branch and no memory index may depend on a secret.

- **SHA-256's additions are now `wrapping_add` under the mask.**
  `docs/crypto.md` §2 had reasoned that a checked `+` under `mask32`
  never traps, which is true. But the check is still an overflow test
  on the hashed key's words, so it is gone.
- **`not32` is now `x ^ 0xffffffff`**, where it was `0xffffffff - x`.
  The value is the same on `[0, 2^32)`, but the subtraction was checked
  and the XOR is not.
- **The object code was read.** `scripts/chacha20_branches.py`, with
  function names, disassembles `compress`, `compress512`, `rotr32`,
  `rotr64`, `lshr64`, `low_mask64`, `mask32` and `not32` from the LLVM
  build of `tests/programs/kdf_driver.ls`. Every conditional jump goes
  to a bounds trap, except six loop back-edges. Those compare the round
  counter with 64 or 80, or a byte offset with 512 or 640. There are
  also three compares of a shift amount with a constant (the
  `low_mask64(63)` case of `docs/sha512.md` §2), and every shift amount
  is a constant at its call site. None reads the data being hashed.
  Before the `not32` change, `compress` had an overflow jump (`jo`)
  after a subtraction on the round word `e`; after it, there is none.
- **HMAC's branches are on public lengths.** It branches on the key's
  length (hashed first, or padded) and on the hash's block size. The
  key is padded to a block. The `ipad` block is cleared before its
  arena ends, and the `opad` block stays in the caller's state until
  `final` clears it. HKDF-Expand clears its last `T(i)`. All of these
  are best effort, for the reasons in `docs/chacha20.md` §3.2. The
  hash states' own buffers and schedules, which hold key-derived
  words, are not cleared.
- **Not claimed:** constant time measured statistically (that is #208),
  or anything about the Cranelift backend's object code.

---

## 4. Checked, not assumed

`tests/programs/kdf_driver.ls` drives every function from standard
input. `crates/lex-sys/tests/conformance/kdf.rs` runs it in
`cargo test`, offline.

### 4.1 SHA-2: NIST CAVP

Every case of the byte-oriented CAVP response files
(`tests/vectors/cavp/`, NIST's files as pyca's `cryptography_vectors`
carries them):

| File | Cases |
|---|---|
| SHA-384 short messages / long messages / Monte Carlo | 129 / 128 / 100 checkpoints (100,000 hashes) |
| SHA-256 short / long / Monte Carlo | 65 / 64 / 100 checkpoints |
| SHA-512 short / Monte Carlo | 129 / 100 checkpoints |

All pass. The long-message file for SHA-512 (1.7 MB) is left out: the
SHA-384 file runs the same compression and the same streaming code, and
the differential (§4.3) covers SHA-512 at up to 200 KiB. Beyond the
files, the test also checks:

- FIPS 180-4's million-`a` examples for all three hashes, streamed;
- 65,536 and 200,000 bytes, one-shot, which trapped before (§2);
- every length from 0 to 300 bytes, one-shot and streamed in pieces of
  1, 7, 63, 64, 65, 127, 128 and 129 bytes, all equal.

`tests/accept/ed25519_long.ls` signs 70,000 bytes. The signature
equals OpenSSL's and verifies, and a tampered one does not.

### 4.2 HMAC, HKDF and the TLS 1.3 key schedule

All 62 rows of `tests/vectors/kdf.txt` pass on **both backends** (59 when this was written; #205 added RFC 8448's three application-stage secrets):

- **RFC 4231**: HMAC-SHA256 and HMAC-SHA384, test cases 1–4, 6 and 7.
  Case 5, the truncated tag, is missing from the transcription used.
- **RFC 5869**: A.1 to A.3, extract and expand separately, PRK included.
  Plus a 1,200-byte HKDF output from pyca's vectors.
- **The TLS 1.3 key schedule from three sources.**
  - *RFC 8448 §3, the "Simple 1-RTT Handshake".* The early secret, the
    derived secrets, the handshake secret, both handshake traffic
    secrets with their keys and IVs, the server's finished key, the
    master secret and the resumption master secret. The RFC's text was
    not reachable from the machine this was written on, so these
    values were written down from the RFC. Each was kept only if
    Python's `hmac`/`hashlib` reproduces it from the trace's own inputs.
    The client and server application traffic secrets and the exporter
    secret did **not** reproduce from the transcript hash as written, so
    one of the two was written wrong. They were left out rather than
    "fixed" to match. **Settled since, in #205 (`docs/tls-core.md`
    §6.1):** AWS s2n-tls transcribes RFC 8448 §3 in
    `tests/unit/s2n_tls13_secrets_rfc8448_test.c`, with the transcript
    hash through the server's Finished (`9608102a…`). The three secrets
    reproduce from it in Python, and they are now rows of `kdf.txt`. So
    was every other value in that file, from the X25519 keys onwards. The
    values first remembered were not kept, so which of the two was
    written wrong is not known.
  - *NIST ACVP's TLS-v1.3-KDF-RFC8446 case*, as Go's `crypto/tls`
    `TestACVPVectors` carries it. This is a PSK handshake, and it covers
    every secret from the early traffic secret to the resumption
    master secret, the application traffic secrets included.
  - *OpenSSL's `test/tls13secretstest.c`*, the draft-06 vectors that
    OpenSSL's TLS 1.3 passes. They cover every traffic secret, key and
    IV for both sides.

**Wycheproof, every case** (`tests/vectors/wycheproof/`):

| File | Valid | Invalid | Result |
|---|---|---|---|
| `hmac_sha256_test.json` | 66 | 108 | all pass |
| `hmac_sha384_test.json` | 66 | 108 | all pass |
| `hkdf_sha256_test.json` | 83 | 3 | all pass |
| `hkdf_sha384_test.json` | 80 | 3 | all pass |

- For HMAC, a truncated tag is compared with the front of the full tag,
  and every modified tag differs from it.
- For HKDF, every valid case gives its output. The three oversized
  requests per hash are refused with `hkdf-length-too-large`, and the
  largest legal size (`255 * hash_len`) is accepted.

**Every refusal tag** in §1 is reached by its own case, and none writes
its output. The edges just inside each limit are accepted:

- an output of exactly `255 * hash_len`;
- a 249-byte label with a 255-byte context;
- outputs of zero bytes.

### 4.3 Against OpenSSL and Python

`scripts/kdf_differential.py` compares against Python's `hashlib` and
`hmac` (OpenSSL's digests underneath), pyca's `HKDF` (OpenSSL's
`EVP_KDF`), and RFC 8446's `HkdfLabel` built in Python. Each round
checks eight things:

- SHA-256, SHA-384 or SHA-512, one-shot and streamed in random pieces.
  One round in fifty uses 64 to 200 KiB, past the old trap.
- HMAC-SHA256 and HMAC-SHA384, with keys of 0 to 300 bytes, which
  crosses both block sizes.
- A whole HKDF under each hash, with the output size up to the
  RFC's maximum on one round in twenty.
- An Expand-Label under each hash, with random labels and contexts.

| Backend | Rounds | Checks | Differences |
|---|---|---|---|
| LLVM | 100,000 | 800,000 | 0 |
| Cranelift | 2,000 | 16,000 | 0 |

---

## 5. Mutants

`scripts/kdf_mutants.py` builds 19 mutants of the three files, as local
modules beside the driver. It runs each against the evidence of §4: the
vector table, the CAVP short and Monte Carlo files, every Wycheproof
case, the streamed-against-one-shot cases, the refusal edges, and 1,000
differential rounds. **19 mutants, 19 killed**, in 23 seconds.

| File | Mutants |
|---|---|
| `crypto.ls` | SHA-384 started from SHA-512's words; one wrong SHA-384 word; the SHA-256 length in bytes, not bits; the SHA-512 length field one byte early; the padding-room check off by one; a round adding `temp1` twice; a buffered SHA-512 byte dropped |
| `hmac.ls` | the wrong inner pad; the wrong outer pad; a block-length key hashed first; SHA-384's block taken as 64 bytes; the inner digest left out of the outer hash |
| `hkdf.ls` | the counter off by one; `T(i-1)` not chained; `255 * HashLen` allowed past; the label prefix misspelt; the output length's high byte dropped; a 250-byte label accepted; a transcript hash of any length accepted |

**What the run found:**

- *A buffered SHA-512 byte dropped* survived every fixed vector. Fixed
  vectors are hashed in one call, so the code that fills a partly full
  buffer never runs. Only the differential's random pieces killed it.
  The cargo test's streamed-against-one-shot cases now cover that path,
  the mutant script runs them too, and the mutant is killed there.
- *Three hkdf mutants were killed by a trap, not by a wrong answer.*
  With the `255 * HashLen` check loosened, or a 250-byte label allowed,
  `byte_of(256)` traps building the counter or the length byte. The
  language's checked conversion stood behind the missing refusal, so
  even without the check no input could produce wrong bytes. But a
  trap is not a refusal ("no input may reach a panic"), so the checks
  stay, and the refusal test is what pins them.

---

## 6. Cost

One core of an Intel Xeon at 2.80 GHz, LLVM backend, median of 5. The
time is the difference between 1 call and many. "Before" is `main`'s
`std/crypto.ls` from before this change, and hooks' `hmac_sha256` over
it, built beside the new code in one program.

| | 64 bytes | 1 KiB | 16 KiB |
|---|---|---|---|
| SHA-256, before → after | 10.7 → **0.99 µs** | — | 113 → **137 MB/s** |
| SHA-512, before → after | 9.6 → **0.59 µs** | — | 211 → **251 MB/s** |
| HMAC-SHA256: hooks' `sign.hmac_sha256` → `hmac.sha256` | 21.8 → **2.6 µs** | 32.7 → **9.9 µs** | 184 → **120 µs** |

The small-message gain is §2's arenas. The old code opened four for a
64-byte message (the state, the padded copy, and one per block, of
which there are two), and the new code opens one. A TLS 1.3 key
schedule is a few dozen HMAC calls over short inputs, so tens of
microseconds. A webhook signature over a 1 KiB body costs about 10 µs.

---

## 7. `lexsys-hooks`

#201 asks for `lexsys-hooks` to use this HMAC instead of its own
(`src/sign.ls`), and for its `tests/sign_test.py` to still pass against
the reference Standard Webhooks library. That is a second repository,
pinned to a compiler revision (`lex-sys.toml`), and the revision has
to contain `std.hmac`. So it lands after this change: a PR on
`lexsys-hooks` that deletes `sign.hmac_sha256`, calls `hmac.sha256`,
moves the pin, and records the 64 KiB trap of §2 with a test that
reaches it.

---

## 8. Not done here

- **The hooks switch** (§7). It follows this change's merge, as its own
  PR on `lexsys-hooks`.
- ~~**RFC 8448's application traffic secrets** (§4.2).~~ Added by #205
  from s2n-tls's transcription of the RFC (§4.2).
- **A statistical timing test.** That is #208.
- **SHA-512 long messages from CAVP.** They are covered by SHA-384's
  file and the differential, not by NIST's own file (§4.1).
- **CI for the differential and mutant scripts.** Both need Python's
  `cryptography`. Everything else runs in `cargo test`.
- **An independent review.** That is #209.
