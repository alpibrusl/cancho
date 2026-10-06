# `std.chacha20`: ChaCha20, Poly1305 and the AEAD (RFC 8439)

> **Status: built. Not independently reviewed.** This is sub-issue 2
> (#199) of the self-contained TLS 1.3 client (#197): the one cipher
> suite the pure client will offer, `TLS_CHACHA20_POLY1305_SHA256`,
> needs this AEAD and nothing else from symmetric cryptography. Nothing
> here should be called production-ready before #209's review, and
> §7 lists what this slice did not do.

---

## 1. Where it lives

`std/chacha20.ls`, `module std.chacha20`. **It is a file of its own,
not part of `std/crypto.ls`.** #197 asks for that until the design
document (#198) settles whether these primitives belong in `std` or in a
package: a file of its own can move to `packages/` with a one-line
change to its `module` header, and it does not collide with the SHA-384
and HKDF work (#201), which edits `std/crypto.ls`.

The cost #197 names is real and is paid here: `std/` is compiled into
the compiler (`STD` in `crates/lex-sys/src/main.rs`, one
`include_str!` per file), so this file ships with a compiler release.
It costs nothing in a program that does not import it
(`std_declarations_cost_nothing_unless_called`, `docs/crypto.md` §6).

The functions, each answering `0` or a negative refusal code:

| Function | What it does |
|---|---|
| `block(key, counter, nonce, out)` | one 64-byte keystream block (RFC 8439 §2.3) |
| `xor(key, counter, nonce, input, output)` | ChaCha20 encryption and decryption (§2.4) |
| `poly1305(key, msg, tag)` | the one-time authenticator (§2.5) |
| `seal(key, nonce, aad, plaintext, out)` | AEAD encryption; `out` is ciphertext then tag (§2.8) |
| `open(key, nonce, aad, sealed, out)` | AEAD decryption; checks the tag first, and writes nothing if it is wrong |
| `refusal_tag(code)` | the stable name of a code |

The output is a buffer the caller passes in, as for `crypto.sha256` and
`ed25519.sign`. Every refusal has its own tag, and each is one the
caller can act on:

| Code | Tag | When |
|---|---|---|
| -1 | `chacha20-key-length` | the key is not 32 bytes |
| -2 | `chacha20-nonce-length` | the nonce is not 12 bytes |
| -3 | `chacha20-output-length` | `out` is not exactly the size the operation writes |
| -4 | `chacha20-counter-exhausted` | the message would need the 32-bit block counter to wrap |
| -5 | `aead-too-short` | `sealed` is shorter than a tag |
| -6 | `aead-tag-mismatch` | authentication failed |

A wrong length is refused rather than left to a bounds trap so that no
input from the network reaches a trap through this module. A
`-4` is refused rather than encrypted because a wrapped counter repeats
keystream. RFC 8439 §2.8 caps one message at 2^32 - 1 blocks.

---

## 2. Representation

**ChaCha20** is 32-bit arithmetic on masked 64-bit `int`s, as in
`crypto.sha256` (`docs/crypto.md` §2). Every word lives in
`[0, 2^32)`. An addition is `m32(wrapping_add(a, b))`, and a rotate is
`m32(x << n | x >> 32 - n)`, where the right shift is logical because
the word's sign bit is never set (`docs/sha512.md` §2).

**Poly1305** works modulo 2^130 - 5 in five 26-bit limbs, the layout of
poly1305-donna's 32-bit code. The bound that makes it safe on a signed
64-bit `int`: clamping leaves each limb of `r` under 2^26, so `5r` is
under 2^29. Between blocks every limb of the accumulator `h` is under
2^26, except `h1`, which takes the last carry and is under 2^26 + 2^10.
After a message limb is added, every limb is under 2^28. A product is
then under 2^57, and the sum of five is under 2^60. The carries out of
those sums are under 2^34, and `5 * c` folded into `h0` is under 2^37.
No addition or multiplication can overflow on any input.

The arithmetic is still written `wrapping_add` and `wrapping_mul`, which
#199 asks for. A checked `+` compiles to an overflow test whose
condition is computed from the operands. Here the operands are secret,
so the test would be a branch on secret data, never taken but present.
`wrapping_*` emits no test (§3).

The final reduction (`poly_finish`) computes `g = h + 5 - 2^130`. It
takes `keep = g4 >> 63`, which is all ones when `g` is negative (that
is, when `h < p`), and selects `h & keep | g & ~keep`. There is no `if`
on the accumulator. *Corrected (review finding B-1, #209): the mask is
now `value_barrier(g4 >> 63)`, under `edition 6;`. `x >> 63` is exactly
the spelling LLVM proves to be 0 or -1 and turns back into a `select`
(`docs/value-barrier.md` §2), and the rule that every secret mask passes
through the barrier where it is made (§4 there) postdates this module.*

---

## 3. Secrets

The key, the keystream, the one-time Poly1305 key and the plaintext are
secret. The rule, from #199: **no branch and no memory index depends on
a secret byte.** Lengths, counters, slice indices and the *result* of
the tag comparison are public, and the code branches on them.

**What was checked:** the object code. `scripts/chacha20_branches.py`
disassembles every function that touches secret words (`quarter`,
`rotl32`, `m32`, `le32`, `dot5`, `poly_block`,
`poly_finish`, and `bytes.store_le32`, which writes key-dependent
words) and lists every conditional jump with the instruction
that sets its flags. On the LLVM backend (x86-64), **every conditional
jump goes to a `ud2` trap**. Each one follows a compare of a slice
length with a constant, a compare of a constant index with a length, an
overflow check on index arithmetic (`at + 3`), or a range check on a
shift amount. None of these reads secret data. The masked select in
`poly_finish` was compiled into five `cmov` instructions, not a branch.
`cmov` takes the same time whichever way it goes on current x86 cores.
That is a property of the processor, not one this language promises. No
function uses a table indexed by data. Command:

```sh
lex-sys build --std tests/programs/aead_bench.ls --emit obj -o bench.o
python3 scripts/chacha20_branches.py bench.o     # 0 conditional jumps that are not traps
```

*Corrected (review findings B-1 and B-6, #209).* "Every function that
touches secret words" was wider than the check: the eight functions above
are where the secret arithmetic is, but `block_into`, `xor`, `poly_init`,
`poly_padded`, `poly1305`, `aead_tag`, `seal` and `open` also hold the
key, the keystream or `r`, and were not disassembled. The script now
lists them too (`LOOPS`). They cannot meet the rule above, since they loop
over a message on purpose, so their conditional jumps other than traps
are printed for the reader and not counted. Read on LLVM, x86-64 (the
object built for `x86_64-unknown-linux-gnu` from `aead_driver.ls`, which
unlike `aead_bench.ls` calls `poly1305` and `open`): 37 such jumps, each
after a compare of a key, nonce, message or output length with a
constant or another length, a loop counter, an overflow check on index
arithmetic jumped over (`jno`), a pointer difference (`xor`'s
vectorised loop checks whether input and output overlap), or, once in
`open`, the result of the 16-byte tag comparison (`diff != 0`, public).
None reads a secret byte. And with the barrier, `poly_finish` compiles
to no `cmov` at all: the select is the `&`, `|` and `~` written in the
source. The command is now:

```sh
lex-sys build --std tests/programs/aead_driver.ls --emit obj -o aead.o
python3 scripts/chacha20_branches.py aead.o      # 0 conditional jumps that are not traps
```

**What is not claimed:** constant time beyond that. No statistical
timing test (dudect-style) has been run. #208 runs one over the
assembled stack. The Cranelift backend's object code was not read. The
check is for one compiler version: a later LLVM could turn the masked
select back into a branch, and only rerunning the script would show it.

### 3.1 `open` never hands out a forged plaintext

`open` computes the expected tag over the associated data and the
ciphertext. It compares all sixteen bytes by OR-ing their XORs, so the
time does not depend on where the tags first differ. Only then does it
decrypt. On a mismatch `out` is not written at all, not even partly
(refuse, don't downgrade). Every refused case in §4 checks this: the
driver fills `out` with `0xaa` first and prints it back.

### 3.2 Erasing keys: best effort, not a guarantee

`aead_tag` overwrites the one-time Poly1305 key before its region ends.
That is all this module does, and it is not a guarantee. A region's
memory is freed without being cleared. The language has no "secure
zero" whose stores the optimiser must keep, and LLVM may delete stores
to memory that is freed straight afterwards. The keystream buffer in
`xor` and the caller's own key are not erased. #205 has to decide what
"no secret outlives the connection" can mean in this language. Without
a guaranteed-store builtin, the honest answer is "it cannot be
promised", and that is an open question for #198.

---

## 4. Checked, not assumed

`tests/programs/aead_driver.ls` drives every function from standard
input, one case per line. `crates/lex-sys/tests/conformance/aead.rs`
runs it in `cargo test`:

- **RFC 8439, 28 vectors** (`tests/vectors/rfc8439.txt`): §2.3.2,
  §2.4.2, §2.5.2, §2.6.2 and §2.8.2, and Appendix A.1 (5), A.2 (3), A.3
  (11), A.4 (3) and A.5. All pass on **both backends**. The RFC's own
  text could not be reached from the machine this was written on, so
  the vectors came from two independent transcriptions of RFC 7539
  (whose vectors RFC 8439 keeps): tlslite-ng's unit tests and pyca's
  `cryptography_vectors`. §2.6.2 and A.4 were written down from the RFC
  and checked against OpenSSL. Every line was checked against OpenSSL
  before it was committed. §2.1.1 and §2.2.1 test the quarter round and
  the state before the final addition, which are private here. They are
  covered only through §2.3.2 and A.1, which they produce.
- **Wycheproof, every case** of `chacha20_poly1305_test.json` (325
  cases, committed with its licence in `tests/vectors/wycheproof/`):
  - 256 valid cases each seal to the expected ciphertext and tag, and
    open to the expected message;
  - 60 modified tags are refused with `aead-tag-mismatch`;
  - 9 invalid nonce sizes are refused with `chacha20-nonce-length` by
    both `seal` and `open`;
  - no refused `open` wrote its output.

  The test asserts these counts exactly.
- **Every refusal tag** is reached by its own case, and so is the
  last block before the counter wraps, which is allowed.
- **One flipped bit anywhere**: §2.8.2's sealed message with each of
  its 1,040 bits flipped in turn, and each of the 96 bits of its
  associated data. All 1,136 are refused, and none writes output. A tag
  check that skipped a byte, or ignored part of the authenticated data,
  would let at least eight of them through.

### 4.3 Against OpenSSL

`scripts/aead_differential.py` uses OpenSSL's EVP implementation
through pyca/cryptography, which bundles its own OpenSSL (4.0.1 here).
For random (key, nonce, aad, message) it checks three things:

- a message sealed here opens in OpenSSL, and the sealed bytes are
  equal to OpenSSL's, since the AEAD is deterministic;
- a message sealed by OpenSSL opens here;
- OpenSSL's sealed message with one random bit flipped is refused by
  both.

Messages are 0 to 300 bytes, with one case in ten up to 4,100 bytes.

| Backend | Cases | Checks | Differences |
|---|---|---|---|
| LLVM | 100,000 | 300,000 | 0 |
| Cranelift | 10,000 | 30,000 | 0 |

```sh
lex-sys build --std tests/programs/aead_driver.ls -o aead
python3 scripts/aead_differential.py ./aead 100000
```

#199 named `openssl enc` or a C harness. `openssl enc` has no AEAD
mode, and pyca's AEAD is a thin binding over `EVP_aead`/`EVP_CIPHER`,
so it is OpenSSL's code that gives the answers. The script is not run in
CI because pyca/cryptography is not a dependency of this repository.

---

## 5. Mutants

A test that cannot fail proves nothing (`CONTRIBUTING.md`).
`scripts/chacha20_mutants.py` builds 18 copies of `std/chacha20.ls`,
each with one deliberate bug, as a local module beside the driver. It
runs each against the RFC table, every Wycheproof case, the bit flips,
the counter edge, and 2,000 differential cases. It first checks that
the unmutated file passes.

| Mutant | Killed by |
|---|---|
| rotation 16 becomes 15; rotation 7 becomes 8 | §2.3.2 |
| nine double rounds, not ten | §2.3.2 |
| a wrong constant; the input words not added back | §2.3.2 |
| a skipped Poly1305 carry; `r` not clamped; a wrong product | §2.5.2 |
| never reduced below `p`; `h - p` computed as `h + 5` | §2.5.2 |
| the carry into the tag's second word dropped | A.3 #4 |
| the 2^128 bit missing from a `pad16` block | §2.8.2 |
| the lengths block in the wrong order | §2.8.2 |
| encryption from counter 0, not 1 | §2.8.2 |
| a one-byte-short tag compare | Wycheproof (modified tag) |
| plaintext released on a bad tag | Wycheproof (`open` wrote its output) |
| the block counter allowed to wrap | the counter edge |
| a nonce longer than 12 bytes accepted | Wycheproof (nonce size) |

**18 mutants, 18 killed**, in 9 seconds:
`python3 scripts/chacha20_mutants.py target/release/lex-sys`.

The first run had one survivor, the long nonce. The script then
accepted any refusal for an invalid Wycheproof case, while `aead.rs`
already checked the exact tag: a 16-byte nonce was refused with
`aead-tag-mismatch` instead of `chacha20-nonce-length`, and the script
let that pass. The script now checks the tag, as the test does, and the
mutant is killed.

---

## 6. Throughput

`tests/programs/aead_bench.ls` seals one message repeatedly, each
round over the previous round's ciphertext, so no round can be skipped.
The time is the difference between 1 round and many (median of 5). The
copy back into the input is inside the timed loop, so these numbers
slightly understate `seal` itself. One core of an Intel Xeon at
2.80 GHz:

| Message | LLVM backend | Cranelift backend | `openssl speed -evp chacha20-poly1305` |
|---|---|---|---|
| 16,384 bytes | **135 MB/s** (121 µs) | 26.6 MB/s (616 µs) | 2,290 MB/s |
| 1,024 bytes | 123 MB/s (8.3 µs) | 25.0 MB/s (41 µs) | 2,129 MB/s |
| 64 bytes | 62 MB/s (1.04 µs) | 12.5 MB/s (5.1 µs) | 533 MB/s |

```sh
lex-sys build --std tests/programs/aead_bench.ls -o aead_bench
time ./aead_bench 1 16384; time ./aead_bench 4096 16384
openssl speed -evp chacha20-poly1305 -seconds 2 -bytes 16384
```

OpenSSL is about 17 times faster on full records. It uses SIMD
assembly (AVX2/AVX-512 on this machine), and this is scalar code. The
cost that matters for #197 is per record: a full 16 KiB TLS record
costs 121 µs to seal on the default backend. A webhook delivery of a
few kilobytes costs tens of microseconds, small next to a handshake's
public-key operations. #210 measures that against OpenSSL end to end.
Nothing here vectorises: `quarter` is called through a function, not
inlined across the four columns of the state. That is the first place
to look if the cost ever matters.

---

## 7. Not done here

- **A statistical timing test** (§3). Deferred to #208, which runs one
  over the X25519 ladder and the AEAD together.
- **A guaranteed erase of key material** (§3.2). The language has no
  primitive for it. That is an open question for #198, not something
  this module can fix alone.
- **The `std`-or-package decision** (§1). That belongs to #198. This
  file is placed so either answer costs one line.
- **CI for the differential and mutant scripts** (§4.3, §5). Both need
  Python's `cryptography`. The Wycheproof cases, the RFC vectors, the
  refusals and the bit flips run in `cargo test`, with no network.
- **An independent review.** #209.
