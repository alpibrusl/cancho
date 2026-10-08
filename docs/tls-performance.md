# TLS performance: where the time goes, and the design for closing the gap with OpenSSL

> **Status: measured, with a design. First deliverable of #378.** Nothing here changes the TLS stack. What it adds is
> a measured profile of a full server handshake and a full client handshake on x86-64 and arm64, a ranked list of
> levers with the gain of each computed from the measured parts, the targets and gates stated before the numbers
> (§1), a regression guard that is built and runs in CI (§6), and the questions a person has to answer (§7). Every
> figure says whether it was measured or is arithmetic from measured parts. Not independently reviewed (#209).

Related: [`tls-server.md`](tls-server.md) (§6, §10 and §11 have the figures this extends),
[`tls-resumption.md`](tls-resumption.md) §1, [`ecdh.md`](ecdh.md), [`ecdsa.md`](ecdsa.md) §5,
[`ecdsa-sign.md`](ecdsa-sign.md), [`x25519.md`](x25519.md) §6, [`crypto-builtins.md`](crypto-builtins.md),
[`tls-hooks.md`](tls-hooks.md) and cancho-hooks' `docs/pure-tls.md` (the client's per-delivery cost: 2.9 ms on the M4 VM, which
§3.4's 3.4 ms of handshake CPU, with a 16 KiB request, is the same order as).

The tracking issue's table of the gap (`tls-server.md` §6, §10, §11) came from a shared CI runner and an Apple VM. This
document measures it on one x86-64 machine and one arm64 machine, each with OpenSSL beside it, and goes inside the handshake.

**What the profile found, in the order it matters:**

1. **Three operations are 77 to 91% of every full handshake** (§3.3, §3.4): the two X25519 operations (28 to 39%), the
   ECDSA signature (26 to 29%, server only) and the ECDSA verifications (the server's own check before it sends, 25 to 29%;
   on the client, the chain's signature and `CertificateVerify`, 22 to 27% each). Hashing, HKDF, the record layer, parsing, the
   engine's copies and zeroing, and the event loop together are **about 5%**. The transcript is not hashed twice in any way
   that shows (SHA-2 is 0.4 to 0.7% in all). Each primitive costs the same inside the handshake as alone, to within 8%.
2. **An unexpected 6% to 10% is the allocator, and on a kept connection it is half the CPU.** Every `region` is a
   `malloc` and a `free` of 64 KiB; a handshake opens 222 of them, a request on an established connection about 6, and
   glibc gives the memory back to the kernel (`brk`) and takes it again each time: 108 `brk` calls a handshake. On a
   kept connection `malloc`, `free` and `brk` are **52% of the server's CPU on x86-64 and 45% on arm64** (§3.5). Telling
   glibc not to trim made a request **1.9 to 2.5 times cheaper** and took `https_hello` to the cost of an OpenSSL
   server answering the same request, measured. This is the first lever, a few lines, and not in any sub-issue.
3. **The 3 to 5 times target is reachable without a new compiler builtin.** Fixed-base tables for the signature and for
   the server's own check, X25519 on 10 limbs of 25.5 bits (which the language's 64-bit `int` allows), the allocator, and
   NIST-prime reduction together take the server's handshake from **15 times OpenSSL's cycles to about 4.4 times**
   (§4.3, arithmetic from measured parts). Wide multiply (#381) takes it further, to about 3 times, and is the one lever
   the language cannot do today; it is not required for the stated target.
4. **The guard has to count instructions, not time.** On the x86-64 machine the clock moved between 1.4 and about 4 GHz within
   minutes, because other work shared the package's power budget, and the same primitive's time moved by two. Instruction
   counts under `valgrind` do not move, are the same as the hardware's, and catch a primitive made twice slower (§6).

---

## 1. Targets and gates, stated before the numbers

They come first so that the tables are read against them. T1 to T4 are #378's words made into figures; the per-primitive bounds of
§1.2 were worked out afterwards, by adding up to T1 from OpenSSL's measured cycles, so they were written knowing which
operations dominate. Nothing in them was changed to suit a later result.

### 1.1 What "within 3 to 5 times of OpenSSL on new connections" means

#378 says: *within 3–5× of OpenSSL on new connections, parity on kept ones, bulk AES-GCM within 2–3×*. As figures,
measured **on one machine, in one session, with the same cipher (AES-128-GCM), group (X25519) and certificate (an ECDSA
P-256 leaf signed by a P-256 CA)**, and in **cycles** where the machine can count them (§2 says why):

| target | the figure | stretch |
|---|---|---|
| **T1.** a full handshake on the server, CPU of the process, verify-before-send included | at most **5 times** OpenSSL's `s_server` under `s_time -new` | 3 times |
| **T2.** a full handshake on the client, chain and host name verified | at most **5 times** a verifying OpenSSL client (`benches/server/tconn.c`) | 3 times |
| **T3.** a request on an established connection, server CPU | at most **1.2 times** OpenSSL's server answering the same request (`benches/server/oserv.c`) | parity |
| **T4.** bulk AEAD, seal and open | AES-128-GCM at most **3 times** OpenSSL at 1 KiB and 16 KiB, **1.5 times** at 64 bytes; ChaCha20-Poly1305 documented as it is until a vector-type decision (§4.4) | 2 times |
| **T5.** resumed handshake (#379) | no more than the full handshake's X25519 operations plus 10% (§4.3) | |

T1 does not give up the verify-before-send check (`tls-server.md` §3.3): OpenSSL does not make one, so T1 compares a
server that does more with one that does less. That is the honest comparison and is why the target is 5, not 3.

### 1.2 The figures per primitive that make T1 and T2 add up

A handshake is its parts (§3.3 shows it is), so T1 is met by the parts below, with OpenSSL's cycles from §3.1. These are
the bounds a change to a primitive is measured against, not goals to stop at:

| primitive | OpenSSL (cycles) | bound | in cycles | what it needs |
|---|---|---|---|---|
| X25519, one scalar multiplication | 114k | at most 6 times | 0.68M | C1 or C2 |
| ECDSA P-256 sign | 77k | at most 8 times | 0.61M | B1 |
| ECDSA P-256 verify, the server's own key (the check) | 243k | at most 6 times | 1.46M | B2 |
| ECDSA P-256 verify, a key never seen before | 243k | at most 15 times | 3.6M | B4, B5 |
| P-256 ECDH, one scalar multiplication | 189k | at most 10 times | 1.9M | B5 |
| `brk` calls a handshake / a request | | at most 2 / 0 (from 108 / 4) | | A1 |
| everything else in a handshake (HKDF, hashes, records, parsing, loop) | about 0.8M for OpenSSL's whole non-asymmetric part | at most 1M | | today's 0.8M |

Two X25519 operations at 6 times, a signature at 8, a check at 6 and today's 0.8M of the rest come to 4.2M cycles, which is
3.9 times OpenSSL's 1.09M and inside T1's 5.45M.

### 1.3 Gates every change keeps

From #378, unchanged: the constant-time properties (`ecdh.md` §2, `ecdsa-sign.md`) with the dudect tests and the
value-barrier audit re-run on every changed secret-handling function; the TLS suites (liar, differential, tickets,
interop, fuzz, mutants) unchanged; numbers measured on a named machine and written into the document that made the claim.
Added here: **the guard of §6 passes, and a change that makes a primitive cheaper re-records its baseline in the same PR**,
so the improvement cannot be given back unnoticed.

---

## 2. How it was measured

### 2.1 The machines

| | x86-64 | arm64 |
|---|---|---|
| machine | `gram`: Intel Core i7-1260P (12th gen, hybrid: 4 P-cores with 2 threads, 8 E-cores), 16 logical CPUs | Apple M4 Max, native arm64 under Docker Desktop 29.2.1 (a VM of 6 vCPUs), image `lexsys-hooks-env` |
| system | Ubuntu 26.04.1, Linux 7.0.0-38-generic | Ubuntu 24.04.5, Linux 6.8.0-100-generic |
| clock | `intel_pstate`, governor `powersave`, 0.4 to 4.7 GHz; **measured 1.4 to 2.1 GHz in the runs that counted cycles** (cycles over task-clock); the first runs of the day were about twice as fast (OpenSSL signed 59,000 times a second then, 24,000 to 38,000 an hour later) | not visible from the VM |
| THP | `madvise` | `madvise` |
| load | load average **4 to 10** during the runs: other people's builds and benchmarks on the other cores (turbostat showed cores 0 to 6 all at 100% busy and 1.5 GHz at one moment), and a database container of a soak | load average **1 to 7** from other containers |
| OpenSSL | 3.5.5 (system) | 3.0.13 (system) |
| pinned to | CPU 0 for the primitives (its sibling, CPU 1, idle); server on CPU 0 and client on CPU 1 for handshakes (27.9M and 27.8M cycles with the client on either: no effect) | CPU 0 and 1 of the VM |
| counters | `perf stat` for user and kernel cycles and instructions; `perf record --call-graph dwarf` | none (the VM has no PMU): time, and `perf record -e cpu-clock` |
| compiler | cancho at `edfca60` (`origin/main`), LLVM backend (the default), clang from the system | the same |

The two machines also differ in the OpenSSL release: 3.5.5 on x86-64, 3.0.13 on arm64. The AEAD rows use
`openssl speed -aead`, which seals the way a TLS record does (set the IV, update, take the tag); the 3.0 and 3.5
releases' plain `-evp` rows do not do the same work, and the first version of these tables used them.

**This machine is not quiet and could not be made so** (cores 2 to 7 were in use by other agents and 8 to 15 by a soak, and changing the governor
is a system setting on a shared box). Two consequences shape everything below.

- **Time is not the unit on x86-64; cycles and instructions are.** The same X25519 took 0.6 to 1.3 ms in three passes within
  a few minutes, and a handshake 4.5 ms in a quiet minute and 10 ms when the clock fell to 1.6 GHz. The instruction count of
  an operation does not move (12,193,474 for an X25519, run after run), and the least cycle count over three passes is the
  least-disturbed one; a cycle count whose IPC would exceed 6, impossible for this core, is a glitch of the counter and is
  dropped. **Where a time is given for x86-64 it is the fastest seen and is labelled.** Ratios to OpenSSL are in cycles.
- **Shares are taken from profiles and are stable where times are not**: the three server captures (6 s each, at different
  clocks) agree on the X25519 share to within 2.1 points (33.1%, 31.1%, 31.7%).

The arm64 times are of a VM on a busy Mac, minimum of three passes of three runs; its figures vary by about 15% between runs
and by more when another container builds.

### 2.2 What was run

| what | how |
|---|---|
| each primitive alone | `tests/programs/tls_prims_bench.cho`, driven by `scripts/tls_perf.py table`: a loop that doubles its calls until it has run 200 ms, three repeats, least time; then twice under `perf stat` with a fixed number of calls, and cycles and instructions a call from the *difference* (the setup cancels) |
| OpenSSL, the same primitives | `openssl speed` (`ecdsap256`, `ecdhx25519`, `ecdhp256`, `-evp sha256`/`sha384`, `-hmac sha256`, `-aead -evp aes-128-gcm`/`chacha20-poly1305` at 64, 1024 and 16384 bytes) under `perf stat`, cycles a call = time a call × cycles over task-clock |
| a full server handshake | `examples/tls_echo` (`--connections 256 --handshakes 256 --rate 100000`) under `openssl s_time -new` for 6 to 10 s; server CPU from `/proc/<pid>/stat` and, with `perf stat -p`, cycles and instructions; per handshake |
| OpenSSL's server | `openssl s_server -tls1_3 -num_tickets 0 -groups X25519`, AES-128-GCM, the same certificate, the same `s_time -new` |
| a full client handshake | `tests/programs/tls_many.cho` (the engine's own client, certificates verified against the CA, the name checked) against `openssl s_server -HTTP`, 200 connections one run and 1 the other, the difference over 199 |
| OpenSSL's client | `benches/server/tconn.c`: `SSL_VERIFY_PEER`, the host name set, aborts on the first handshake that does not verify (`s_time` does not fail on a verification error, so it cannot say the check ran) |
| where the time goes | `perf record -F 1999 --call-graph dwarf,32768 -p <server>` for 6 s while `s_time -new` runs (the compiled code keeps no frame pointers; DWARF unwinding gave a complete stack in all but 0.04 to 0.7% of the samples), sorted into the categories of §3.3 by `scripts/tls_perf.py profile` |
| a request on a kept connection | `scripts/https_hello_test.py --cost 5` with `--kload --tload --plain --oserv` (server on one core, two load threads on another, 32 connections) |
| allocator experiment | the same runs with `GLIBC_TUNABLES=glibc.malloc.trim_threshold=33554432:glibc.malloc.top_pad=1048576`, alternating, and `LD_PRELOAD` counting `malloc`, `strace -c` counting `brk` |

On arm64 the same commands, without `perf stat`; the profile is `perf record -e cpu-clock` from a privileged container.

---

## 3. The measurements

### 3.1 Each primitive alone

`tls_prims_bench`, x86-64, user-space cycles (the least of three passes) and instructions (exact), against OpenSSL's
cycles from `openssl speed` on the same core. "Ratio" is cancho's cycles over OpenSSL's.

| operation | cancho: instructions | cancho: cycles | OpenSSL: cycles | ratio |
|---|---|---|---|---|
| X25519 key pair | 12.19M | 2.34M | 114k | **20.6** |
| X25519 shared secret | 12.19M | 2.52M | 114k | **22.2** |
| P-256 ECDH, one scalar multiplication | 23.45M | 4.53M | 189k | **24.0** |
| ECDSA P-256 sign | 25.25M | 4.66M | 77k | **60.8** |
| ECDSA P-256 verify | 25.25M | 4.58M | 243k | **18.9** |
| sign, then verify before sending | 50.50M | 9.27M | | |
| HKDF-Expand-Label (32 bytes out) | 34.5k | 8.6k | | |
| HKDF-Extract | 33.4k | 5.6k | | |
| AES-128-GCM key preparation | 36.6k | 6.8k | | |

arm64 (Apple M4 Max in the VM), minimum time of three passes; the machine of #378's table:

| operation | cancho | OpenSSL 3.0.13 | ratio |
|---|---|---|---|
| X25519 key pair, shared secret | 598 us | 19.0 us | **31.4** |
| P-256 ECDH, one scalar multiplication | 756 us | 25.7 us | **29.4** |
| ECDSA P-256 sign | 828 us | 11.7 us | **70.9** |
| ECDSA P-256 verify | 816 us | 34.3 us | **23.8** |
| sign, then verify before sending | 1,625 us | | |

What is in these:

- **Signing is the worst ratio, 61 to 71 times,** because OpenSSL signs with a precomputed table (tens of microseconds) and
  verifies with a generic double-scalar multiplication (about three times a signature). Verification is already within 19 to
  24 times; the gap between sign and verify is the table.
- **The instruction count is 37 to 44 times OpenSSL's** (OpenSSL's X25519 is 333k instructions and its P-256 ECDH 536k,
  measured with `perf stat`), and the cycle ratio is 20 to 24 because the compiled code runs at an IPC of 5.4 to 5.5 where
  OpenSSL's assembly runs at 2.4 (a chain of dependent multiplications). **The cancho code is not stalling: it executes too
  many instructions.** Better scheduling would gain nothing; fewer instructions (§4) are the whole of the lever.
- `valgrind --tool=callgrind` says what the instructions are for. A P-256 scalar multiplication is about 4,800
  Montgomery multiplications (`bigmod.mont_mul`, which the profile says is 63% of the time, with 16% in the constant-time
  reduction `ct_reduce`, 6% in the table selection and 11% in the additions and subtractions); a signature 5,280, a
  verification 6,015. An X25519 is 3,057 field multiplications at **about 4,000 instructions each**: a 16 by 16 limb
  schoolbook product (256 products), each with a load, a multiply, an add and a store through a slice with its bounds checks.
- **Calibration against the older harnesses**, same minute: `scripts/curve25519_bench.py` 1.185 ms for X25519, this bench
  1.125 ms; `scripts/ecdsa_sign_bench.py` 2.047 ms signing and 3.986 checked, this bench 2.06 and 4.05.

Hashes, HMAC and the AEADs, by size. x86-64 in cycles, arm64 in nanoseconds (OpenSSL uses the CPU's SHA-2 and AES instructions
on both; the cancho code uses hardware for AES and GHASH only, `crypto-builtins.md`):

| operation | bytes | cancho: instructions | cancho: cycles | OpenSSL: cycles | ratio | arm64: cancho ns | arm64: OpenSSL ns | arm64 ratio |
|---|---|---|---|---|---|---|---|---|
| SHA-256 | 64 | 15.2k | 2.6k | 0.6k | 4.6 | 566 | 87 | 6.5 |
| SHA-256 | 1,024 | 122k | 21k | 2.5k | 8.4 | 4,486 | 411 | 10.9 |
| SHA-256 | 16,384 | 1,832k | 324k | 32k | 10.0 | 67,871 | 5,597 | 12.1 |
| SHA-384 | 64 | 8.5k | 2.1k | 1.1k | 2.0 | 321 | 125 | 2.6 |
| SHA-384 | 16,384 | 824k | 156k | 85k | 1.8 | 32,470 | 9,902 | 3.3 |
| HMAC-SHA-256 (OpenSSL reuses the keyed context) | 64 | 40k | 10k | 0.7k | 14.5 | 1,457 | 124 | 11.8 |

SHA-384 is within 2 times on x86-64 because OpenSSL has no SHA-512 instruction on this CPU either (its code is AVX2), and
3 times behind on arm64, where it has; SHA-256 is 10 times off because OpenSSL uses SHA-NI. OpenSSL's HMAC row measures `update` and `final` on a keyed context; a cancho HMAC does the two key-pad
compressions every time, so that row is not like for like and is not used below.

### 3.2 Bulk: the AEADs by record size

`openssl speed -aead` seals a record the way TLS does (IV, update, tag) and so does `tls_prims_bench`.

| | bytes | cancho: cycles | cycles a byte | OpenSSL: cycles | ratio | arm64: cancho ns | arm64: OpenSSL ns | arm64 ratio |
|---|---|---|---|---|---|---|---|---|
| AES-128-GCM seal | 64 | 0.8k | 12.5 | 0.9k | **0.9** | 115 | 94 | 1.2 |
| AES-128-GCM seal | 1,024 | 6.7k | 6.6 | 1.6k | **4.1** | 991 | 216 | 4.6 |
| AES-128-GCM seal | 16,384 | 102k | 6.2 | 11k | **9.1** | 15,014 | 2,133 | 7.0 |
| AES-128-GCM open | 64 | 0.8k | 12.8 | 0.9k | 0.9 | 123 | 94 | 1.3 |
| AES-128-GCM open | 16,384 | 102k | 6.2 | 11k | 9.1 | 15,075 | 2,133 | 7.1 |
| ChaCha20-Poly1305 seal | 64 | 3.1k | 48.7 | 0.4k | 7.0 | 642 | 239 | 2.7 |
| ChaCha20-Poly1305 seal | 1,024 | 22k | 21.5 | 1.9k | 11.5 | 5,386 | 675 | 8.0 |
| ChaCha20-Poly1305 seal | 16,384 | 312k | 19.1 | 28k | 11.2 | 81,298 | 7,867 | 10.3 |
| ChaCha20-Poly1305 open | 16,384 | 300k | 18.3 | 28k | 10.8 | 81,298 | 7,867 | 10.3 |

On arm64 the AES-GCM bandwidth at 16 KiB is 1.09 GB/s against OpenSSL's 7.7 GB/s (7 times, as #378 says); on x86-64
it is 6.2 cycles a byte against 0.7.

- **At the size of an HTTP request (64 bytes) AES-GCM is already at OpenSSL's cost** (0.9 times on x86-64, 1.2 on arm64):
  OpenSSL's per-record setup costs as much as the hardware path. The ratio grows with the record: the code runs one AES
  block and one GHASH reduction at a time with the AES latency in the way (IPC 3.7), where OpenSSL interleaves eight.
- ChaCha20-Poly1305 is the cipher a machine without AES instructions uses. At 19 cycles a byte, 11 times OpenSSL's AVX2
  code, it is scalar by necessity (no vector types, §4.4).

### 3.3 A full server handshake

`examples/tls_echo`, P-256 certificate (one certificate, signed by the CA), X25519, AES-128-GCM, TLS 1.3, one handshake
at a time. Totals, per handshake:

| | x86-64 | arm64 |
|---|---|---|
| **cancho server, CPU** | **4.51 ms** in the quiet minute (2,001 handshakes in 10 s); 9.96 to 10.46 ms when the clock was 1.6 GHz | **3.08 to 3.14 ms** (three runs) |
| cancho server, cycles / instructions | **16.5M** (least of three: 16.5, 17.1, 18.8) / **77.8M** | not countable |
| **OpenSSL `s_server`, CPU** | 0.59 to 0.65 ms (throttled) | **0.21 to 0.26 ms** |
| OpenSSL, cycles / instructions | **1.09M** (least of three) / **1.86M** | |
| **ratio** | **15.1 in cycles** (41.8 in instructions); 16 in time within the throttled session | **12 to 15** |

The 4.51 ms and the 3.1 ms agree with #378's 5.5 ms (a shared CI runner) and 3.0 to 3.9 ms (the M4 VM) as the order, not as a repeat of them.

Where it goes. `perf record --call-graph dwarf`, 28,014 samples in three x86-64 captures and 6,618 in one on arm64. A
sample is in the first category whose frames its stack passes through, memory first. "us" applies the share to the totals
above (x86-64: 4.51 ms, which is arithmetic from the profile's shares and a time measured in another minute).

| category | x86-64: share | x86-64: M cycles | x86-64: us | arm64: share | arm64: us |
|---|---|---|---|---|---|
| X25519: the key pair and the shared secret (two ladders) | **31.8%** | 5.25 | 1,434 | **39.2%** | 1,216 |
| ECDSA sign (`ecdsa_sign.sign`) | **29.0%** | 4.78 | 1,306 | **26.4%** | 818 |
| ECDSA verify: the server's check before it sends | **28.5%** | 4.71 | 1,287 | **25.0%** | 775 |
| memory: `malloc`, `free`, `brk` | **5.7%** | 0.94 | 256 | **5.0%** | 154 |
| engine and event loop (parsing, copies, bookkeeping) | 1.6% | 0.26 | 71 | 0.7% | 22 |
| system calls (sockets, `epoll`) | 1.2% | 0.20 | 56 | 1.8% | 55 |
| HKDF and HMAC (key schedule, Finished) | 1.0% | 0.16 | 44 | 1.0% | 32 |
| SHA-256/384 (transcript, digests) | 0.7% | 0.11 | 30 | 0.4% | 11 |
| record AEAD and traffic-key setup | 0.3% | 0.04 | 12 | 0.4% | 12 |
| page faults, not attributed | 0.3% | 0.05 | 12 | 0.2% | 6 |

- **The three asymmetric operations are 89% (x86-64) and 91% (arm64).** Each costs inside the handshake what it costs alone
  (§3.1): on arm64 1.216 ms against 2 × 0.598 for the two ladders, 818 against 828 for the signature, 775 against 816 for
  the check; on x86-64 5.25M against 4.86M, 4.78M against 4.66M, 4.71M against 4.58M. **There is no hidden cost around
  them to hunt.** The sign and the check are the same size: the check is as expensive as the signature (`tls-server.md`
  §6's correction).
- **The engine's own copies, zeroing, parsing and the loop are 1.6% (x86-64) and 0.7% (arm64)**, and the leaf `memcpy`/`memset`
  calls are 0.24% of all samples. The transcript hash plus every digest is **0.7%**: SHA-256 is 20 cycles a byte but a
  handshake hashes under 6 KiB. The record layer is 0.3%. None of these is a lever now (§4.5).
- **`tls_serve` is not the number to quote**: `tls-server.md` §11.4 measured it 0.9 ms dearer than `tls_echo` with the
  same engine and left it uninvestigated; this document profiles `tls_echo` and did not investigate `tls_serve`.
- **Memory is the one surprise** and has its own section, §3.5: 222 regions open in a handshake, each a `malloc` and `free` of
  64 KiB, 108 `brk` calls (`strace -c`: 24,410 for 226 handshakes, 76% of the time spent in system calls).

### 3.4 A full client handshake

`tls_many`, the engine's client, the CA and the name checked, against `openssl s_server -HTTP` (a small answer); the
difference of a 200-connection run and a 1-connection run, over 199. Each connection also seals a 16 KiB request, which is
0.1M cycles of the figures below.

| | x86-64 | arm64 |
|---|---|---|
| **cancho client, CPU** | 10.7 to 14.6 ms (throttled; no quiet minute was caught) | **3.38 to 3.49 ms** |
| cancho client, cycles / instructions | **19.4M** (least of three: 19.4, 22.3, 23.1) / **80.3M** | not countable |
| **OpenSSL client (`tconn`, verified)** | 0.93 to 1.02 ms | **0.296 to 0.306 ms** |
| OpenSSL client, cycles / instructions | **1.70M** / **3.39M** | |
| **ratio** | **11.4 in cycles** (23.7 in instructions) | **11.6** |

#378's 3.5 to 7 times for the client is hooks' per-delivery CPU against a service that uses OpenSSL, and covers everything a
delivery does (cancho-hooks' `docs/pure-tls.md`); the ratio of handshake CPU alone, with the same cipher and group, a verified
chain and the same name check, is 11 here.

| category | x86-64: share | x86-64: M cycles | arm64: share | arm64: us at 3.48 ms |
|---|---|---|---|---|
| X25519 (key pair, shared secret) | **27.9%** | 5.4 | **33.1%** | 1,151 |
| ECDSA verify: the chain (the CA's signature on the leaf) | **27.2%** | 5.3 | **22.3%** | 776 |
| ECDSA verify: `CertificateVerify` (the server's key) | **27.1%** | 5.3 | **22.2%** | 772 |
| memory: `malloc`, `free`, `brk` | 9.9% | 1.9 | 5.0% | 172 |
| page faults (first touch of 200 slots in one process) | 1.6% | 0.3 | **12.1%** | 420 |
| system calls | 2.1% | 0.4 | 2.2% | 78 |
| engine and loop | 1.6% | 0.3 | 0.5% | 16 |
| record AEAD | 0.9% | 0.2 | 0.8% | 29 |
| HKDF, HMAC | 0.9% | 0.2 | 0.6% | 21 |
| SHA-256/384 | 0.4% | 0.1 | 0.3% | 12 |
| certificate parsing | 0.3% | 0.06 | 0.2% | 8 |

- **Four operations are 82% (x86-64) and 77% (arm64) of the client's handshake**, `tls-resumption.md` §1's "these four
  operations are the handshake", with the same costs inside as alone (arm64: 776 against 816 for a verification, 1.151 ms
  against 1.196 for the two ladders).
- The page faults are the 200 slots of `tls_many` being touched for the first time in one process (about 120 KiB each established, #378); a long-lived client reuses its slots, so in steady state they are not in the handshake. On arm64 in the
  VM they cost 12%, on x86-64 1.6%. It is #383's memory and not a CPU lever.
- The client has no signature; its second and third costs are verifications. A chain through an intermediate adds a third
  (`ecdsa.md` §5.4), so a typical public chain costs the client more than this one.

### 3.5 A request on an established connection

`https_hello` over TLS 1.3 and keep-alive, `GET /hello/42`, server CPU per request, 2 load threads and 32 connections. The
"plain" row is a different application (`examples/api`, `GET /users/42`), the only plain server in the repository; the
OpenSSL rows are `benches/server/oserv.c`, a one-thread OpenSSL server that answers the same nine bytes, plain and over TLS.

| | x86-64 (throttled) | arm64 |
|---|---|---|
| `examples/api`, plain | 12.2 and 14.7 us | 2.90 us |
| `oserv`, plain | 11.5 and 13.1 us | 3.67 us |
| **`https_hello`, TLS** | **47.7 and 52.8 us** | **8.81 us** |
| **`oserv`, TLS (OpenSSL)** | 20.9 and 22.8 us | 6.73 us |
| `https_hello` with glibc told not to trim | **24.7 and 21.5 us** | 4.96 us (one pair; the plain row moved from 2.90 to 4.90 between the two runs, so the VM was busier) |
| `oserv`, TLS, in the same runs as that row | 23.6 and 20.2 us | 5.07 us |

**On x86-64 `https_hello` costs 2.3 times OpenSSL's server per request, and after the allocator experiment about 1.05 times.**
On arm64 the gap was 1.3 times. TLS's own cost over plain (record seal, open, copies) is small: OpenSSL's is 9 us of 21 on
x86-64.

Where the request goes (`perf record`, 8,366 samples on x86-64, 8,812 on arm64):

| category | x86-64 | arm64 |
|---|---|---|
| **memory: `malloc`, `free`, `brk`, page faults** | **52.9%** | **45.1%** |
| system calls (`send`, `recv`, `epoll`; the loopback delivery of a `send` is charged to the sender) | 29.7% | 35.8% |
| HTTP parser, application, loop | 11.4% | 13.0% |
| record AEAD | 5.8% | 5.9% |

**A request opens about 6 regions** (`LD_PRELOAD` counting: 851,022 mallocs of 65,536 bytes for 140,640 requests and 32
handshakes at 222 each), and each is a `malloc` of 64 KiB and a `free`. The reason is in the generated code:
`cancho-codegen-llvm/src/emit.rs` declares `malloc` and `free` for "one `malloc` per arena, one `free` on the way out" and
`ARENA_CHUNK` is 64 KiB. glibc frees the chunk back when the top of the heap grows past 128 KiB and then grows it again for
the next region, with a page fault for each page touched: **108 `brk` calls a handshake and 4 a request** (`strace -c`: 40,594 `brk` for 10,163 responses). The
cost was known in part (`tls-parity.md`: "nested regions cost 8 `brk` calls per seal"); it was not known to be the largest
cost of a kept connection.

The experiment is `GLIBC_TUNABLES=glibc.malloc.trim_threshold=33554432:glibc.malloc.top_pad=1048576` on the unchanged
binaries, alternating with the default, two pairs each. It is an experiment and not the fix (§4.1 A1): it shows what the
fix is worth. The handshake gain (5.7% by the profile) could not be confirmed by the same alternation, because the
handshake's cycle count moved by 20% between runs; it is arithmetic from the profile.

### 3.6 What was looked for and is not there

#378 asked about specific suspects. By the profile of §3.3 and §3.4:

| suspect | what the profile says |
|---|---|
| the engine's copies and zeroing | `memcpy`/`memset` are 0.24% of server samples; the engine and loop together 1.6% |
| the transcript hashed twice | all hashing is 0.7% of a server handshake, 0.4% of a client's |
| parsing and certificate handling | 0.3% of a client handshake |
| the event loop | system calls 1.2 to 2.2%, `epoll_wait` 0.15% |
| allocation | **yes**: 5.7% to 9.9% of a handshake, 45 to 53% of a request (§3.5) |
| the verify-before-send check | **yes**, 28.5% of the server's handshake, as expensive as the signature |
| HKDF/HMAC recomputing pads | 1.0%: `hmac.init` does two compressions a call; small now, relatively larger once the rest is faster (D4) |
| `bigmod.setup`/`r_squared` per signature or verification | 0.9% of the handshake, inside the ECDSA rows (A2) |
| the field inversion at the end of each ladder | 3.2% of the handshake each, 19% of an X25519 (C3) |

---

## 4. The levers

### 4.1 The list, ranked

Gain is the percentage of today's x86-64 server handshake removed (shares of §3.3), computed from the measured share and the
stated assumption. Nothing in the *gain* columns has been measured as a gain except A1's per-request figure; the *basis* column
says what each was computed from.

| # | lever | issue | basis | gain: server handshake | gain: client handshake | gain: kept request | effort |
|---|---|---|---|---|---|---|---|
| **A1** | **Stop `region` returning its chunk to the kernel**: keep a few freed 64 KiB chunks (a per-thread free list, or `mallopt(M_TRIM_THRESHOLD)` in the emitted `main` as a stopgap). Compiler change, both backends | **new** | measured: 5.7% / 9.9% of a handshake; experiment 1.9 to 2.5 times per request | **5.7%** (1.06 times) | **9.9%** | **48 to 60%** (2 to 2.5 times) | small |
| **B1** | **Fixed-base comb for k·G**, signing: 64 windows of 4 bits, 64 mixed additions instead of 256 doublings and 78 additions | #380 | `callgrind`: a signature is 5,280 Montgomery multiplications, of which the scalar multiplication is about 4,430; a comb needs 64 × 13 = 830, plus 850 for the two inversions | **19.7 points** (sign ÷ 3.1) | | | medium |
| **B2** | **A fixed-base table per identity for the check**: the server's public key is fixed for the life of the certificate, so `u1·G + u2·Q` is two combs, no doublings, no table built per call | **new** (in #380's area) | a verification is 6,015 multiplications; two combs 1,664 plus the inversion 380 and a few: 2,060 | **18.8 points** (check ÷ 2.9) | | | medium |
| **B3** | The same table per **trust anchor** on the client, built when the root is added to the store | **new** | the chain's verification is 27.2% of a client handshake; a fixed key is B2's case | | **18.0 points** | | small after B2 |
| **B4** | `u1·G + u2·Q` for a key never seen: G by the comb, Q by 5-bit wNAF with the cheaper non-complete formulas (public data) | #380 | 2,048 doublings at 8 multiplications, about 43 additions, 64 mixed additions, two inversions: about 4,300 against 6,015 | | 7.8 points (`CertificateVerify` ÷ 1.4) | | medium |
| **C1** | **X25519 on 10 limbs of 25.5 bits** with the 64-bit `int` the language has (`x25519.md` §6 names it), a dedicated square, products kept in registers | **new** (#381's baseline) | 100 products a multiplication instead of 256; today about 4,000 instructions a multiplication; assumed 2.5 times | **19.1 points** (X25519 ÷ 2.5) | 16.7 points | | medium |
| **B5** | NIST-prime reduction for P-256 in place of generic Montgomery (and 8 limbs of 32 bits) | #380 | `mont_mul` and `ct_reduce` are 79% of the signature's time; 64 products plus a fold in place of 162; assumed 1.7 times on the field multiplications | 7.9 points after B1 and B2 | 11.9 points | | medium |
| **D1** | **Tickets**: a resumed handshake skips the signature, the check and the chain | #379 | a resumed handshake is X25519 + memory + the rest = 42.5% of a full one | resumed: **57.5%** (2.35 times); only for returning clients | resumes already (`tls-resumption.md`) | | large |
| **C2** | **Wide multiply and add-with-carry** | #381 | assumed 4 times on field multiplications: X25519, sign and check from 31.8 + 9.2 + 9.8 to 12.7 after A1, B1, B2 | with A1 + B1 + B2: **82%** removed (5.65 times in all) | | | large, compiler |
| **D2** | Multi-block AES-GCM, aggregated GHASH | #382 | 102k cycles for 16 KiB at IPC 3.7 is the latency of one block at a time; 4 to 8 blocks hides it | none | none | none (64-byte records are at OpenSSL's cost already); bulk 16 KiB 2.5 to 3.5 times | medium, builtins |
| **D3** | A hardware SHA-256 builtin (SHA-NI, ARMv8 SHA-2), the shape of #328 | **new** | SHA-256 is 10 and 12 times OpenSSL's; 0.4 to 0.7% of a handshake today | under 1 point | under 1 point | | medium, builtins |
| **D4** | HMAC with the key pads kept (HKDF reuses a keyed state) | **new** | `hmac.init` is two of the compressions of a 64-byte HMAC; HKDF-Expand-Label is 8.6k cycles | 0.3 points | | | small |
| **A2** | The Montgomery constants for p and n of P-256/P-384 as constants, not computed per call | **new** | `bigmod.setup` + `r_squared` 0.9% | 0.9 points | | | small |
| **C3** | One inversion for the two ladders (Montgomery's trick); the server computes both back to back | **new** | the profile puts the inversion at 6.4% of the handshake for the two, 19% of each ladder; one is saved | 3.2 points | | | small |
| **E1** | Memory per connection | #383 | not measured here; the client's first touch of a slot costs 12% on arm64, 1.6% on x86-64 | | (first touch) | | per #383 |
| F1 | Drop the verify-before-send check | | would remove 28.5% | 28.5 points (1.40 times) | | | **a decision for a person, §7 Q1** |

Order, by gain over effort and by what unlocks what: **A1; then B1 and B2 together (the same comb code, one table-making
routine); then C1; then B3 and B5; then D1 in parallel; C2 as a prototype that has to beat C1; D2 for bulk.**

The changes to the order in #378's checklist (§8): the profile moves **A1 first** and adds it as a new issue; **#380 grows
the per-identity table (B2) and moves above #379** (tickets help only a client that comes back, and the comb helps every
connection); **#381's prototype is measured against C1, not against today's code**, or it takes credit for a 2.5 times that
10 limbs of 25.5 bits give with no compiler change.

### 4.2 What the language cannot do today

- **A 64 × 64 → 128-bit multiply and an add with carry (C2).** The only products are 64-bit and signed and trap on overflow
  (`wrapping_mul` drops the high half). That is why the limbs are 16 bits (`field25519`) and 30 bits (`bigmod`): so that a
  product and its sums fit in 63 bits. No amount of code in the language gets 51- or 64-bit limbs. #381.
- **Vectors.** There are no SIMD types, so ChaCha20 (4 or 8 blocks at once) and Poly1305 cannot be vectorised; ChaCha20-Poly1305
  stays 11 times OpenSSL's AVX2 (§3.2). It is the cipher chosen when the CPU has no AES instructions, so the case is real, and
  needs the decision #382 mentions: a vector type or a builtin.
- **The CPU's SHA instructions (D3).** There is no builtin for them; #328 made hardware AES and GHASH.
- **Choosing how memory is returned (A1).** A program cannot call `mallopt` without a foreign-function capability, and this
  engine is built to need none; the chunk policy is in the code the compiler emits, so A1 is a compiler change.
- **A constant table in the binary.** A region is 64 KiB (and traps beyond it), so the 60 KiB of a comb table lives on the
  heap (`box_slice`), built at engine open, not in a `region` and not as a `static`; a `&static [byte]` literal of 60 KiB would
  work and has not been tried (§7 Q2).
- **A guarantee of no branch in the wide-multiply lowering** is #381's design question, not a limit.

What the language can do and nobody has: C1 (25.5-bit limbs), the combs (B1, B2, B3), wNAF (B4) and the reduction (B5) are all
expressible with the 64-bit `int`, `wrapping_*` and slices that exist.

### 4.3 What the levers add up to

Applying the levers one after another to the x86-64 server handshake (shares of §3.3: X25519 31.8, sign 29.0, check 28.5,
memory 5.7, the rest 5.0; 16.5M cycles; OpenSSL 1.09M). **Arithmetic from the measured shares and the stated assumptions, not a
measurement:**

| after | handshake | M cycles | against OpenSSL |
|---|---|---|---|
| today | 100% | 16.5 | 15.1 times |
| + A1 | 94.3% | 15.6 | 14.3 |
| + B1 (comb, signing) | 74.6% | 12.3 | 11.3 |
| + B2 (per-identity table, the check) | 55.8% | 9.2 | 8.5 |
| + C1 (X25519 on 25.5-bit limbs) | 36.7% | 6.1 | 5.6 |
| + B5 (NIST-prime reduction, 1.7 times) | **28.9%** | **4.8** | **4.4** |
| *alternatively* A1 + B1 + B2 + C2 (wide multiply, 4 times on the field multiplications) | 17.7% | 2.9 | 2.7 |
| A1 + B1 + B2 + C1 + B5, then C2 on top (1.6 times further) | 19.9% | 3.3 | 3.0 |

- **T1 (5 times) is reached by A1, B1, B2, C1 and B5, none of which needs a compiler builtin**, and reached (4.4) with a small
  margin that rests on two assumptions the prototypes have to bear out (2.5 times on X25519, 1.7 times on the field multiplications).
  **The 3-times stretch needs wide multiply.**
- The client handshake with A1, B3, B4, C1 and B5: 100 − 9.9 − 18.0 − 7.8 − 16.7 − 11.9 = **35.7%**, which is **6.9M cycles,
  4.1 times OpenSSL's 1.70M**: inside T2's 5. (Arithmetic; the same assumptions.)
- **Tickets (D1)** give a returning client a handshake of 42.5% of a full one (7.0M cycles, 6.4 times a *full* OpenSSL handshake);
  with A1 36.8% (6.1M), which meets T5 (the X25519 operations, 31.8%, plus 10%: 41.8%), and with C1 as well 17.7% (2.9M): the resumed
  handshake is the X25519 operations plus the rest, and nothing else is left to cut but the ladders.
- **A kept request** is 2 to 2.5 times cheaper with A1 alone, at OpenSSL's cost (§3.5): T3 needs only A1.
- The arm64 shares give the same ranking (X25519 39%, sign 26%, check 25%, memory 5%); with the larger X25519 share C1 matters more
  there (23.5 points removed by C1 alone, against 19.1).

### 4.4 Bulk

The cancho AEADs are at OpenSSL's cost for a 64-byte record and 4 to 9 times it at 1 and 16 KiB (§3.2). T4 asks for 3 times:
D2 (#382). The 102k cycles for 16 KiB are 6.2 cycles a byte at IPC 3.7: AES-NI's `aesenc` has a latency of 3 to 4 cycles and the
code is one block at a time, with a GHASH reduction a block. Four-block interleaving hides the latency (4 times at the AES) and
aggregating the reductions removes three of four; **2.5 to 3.5 times is the arithmetic**, which is #382's own hypothesis (2 to 4
times) and still to be measured with the prototype of the four-block path it asks for.

### 4.5 Levers the profile rules out

The transcript hash, the HKDF, the record layer, parsing, copies and zeroing, and the event loop are together 5% of a server
handshake (§3.3, §3.6). Even a perfect version gains 5%; after A1, B1, B2, C1 and B5 they are 17% of what remains, and D3 and D4
become worth doing then, not before. They are listed so that nobody starts with them.

---

## 5. Open questions about the measurements

- **The x86-64 machine was shared.** Times moved by 2; cycles by 20 to 30% in the worst passes (the least is used); instruction
  counts did not move. A second session on a machine of one's own would tighten the cycle columns, not change the ranking.
- **arm64 has no cycle counts** (the VM has no PMU). Its time figures are minimums of three passes on a busy Mac.
- **The client profile includes first-touch page faults** (§3.4). A steady-state client with slot reuse was not run.
- **OpenSSL releases differ** (3.5.5 and 3.0.13); the AEAD rows use `-aead` to compare like with like; the HMAC row does not
  and is not used.
- **Cranelift was not measured** (the LLVM backend is the default and the one in the issue's numbers); **P-384, RSA and
  TLS 1.2** were not measured; **latency** (the loop held for 4.5 ms by one handshake) was not.
- **The handshake gain of A1 is arithmetic**; the per-request gain is measured (§3.5).
- **Memory per connection (#383)** was not measured here.

---

## 6. The regression guard

`tests/programs/tls_prims_bench.cho` times 33 rows (every primitive of §3.1, the three AEAD and hash sizes, and a calibration
loop). `scripts/tls_perf.py` drives it.

```
cancho build --std tests/programs/tls_prims_bench.cho -o tls_prims_bench
python3 scripts/tls_perf.py table  ./tls_prims_bench                 # ns, cycles and instructions a call (cycles: Linux with perf)
python3 scripts/tls_perf.py check  ./tls_prims_bench                 # the guard
python3 scripts/tls_perf.py record ./tls_prims_bench --machine x86_64   # a new baseline, in benches/tls_baseline.json
python3 scripts/tls_perf.py profile server.script.txt --total-us 4510   # §3.3's table from a perf script
```

**What it compares.** With `valgrind` (callgrind), `check` counts the instructions each row executes, from the difference of two
runs with different numbers of calls, and fails a row more than **1.3 times** its recorded count. Instruction counts do not
depend on the clock or the load (§2.1), they equal the hardware's (12,193,474 for an X25519 under both), and the run takes 40
seconds on x86-64 and 12 on arm64. Without `valgrind` it falls back to the time of each row over the calibration's, at 2.0 times,
because that quotient moved 1.6 times between two runs minutes apart on the shared machine, which is why it is not the first
choice.

**What it catches, shown and not claimed.** A copy of the benchmark with X25519 and SHA-256 each done twice failed exactly
those four rows (`x25519_public`, `sha256` at 64, 1,024 and 16,384 bytes) at a ratio of 2.00 and passed the other 29. A change
that makes a primitive twice as slow is caught; one that adds 20% is not (1.3 is wide for what CI showed: the compiler and CPU differences between the machine that recorded the baseline and GitHub's
runner moved the counts by 2% down to 11% up).
**What it cannot catch:** a change in instruction *count* that leaves time alone or the reverse (a code layout that halves
the IPC), the handshake as a whole (a primitive added to the path), and memory. The allocator (A1) and the engine are in no row;
`scripts/tls_echo_test.py --cost` and `scripts/https_hello_test.py --cost` remain where those are measured.

**CI.** The `tls-perf` job builds the compiler and the benchmark and runs `check`. `valgrind` is not on the runner by default: the
job installs it. The baseline file holds one entry per architecture (`x86_64` from the machine of §2.1, `aarch64` from the
M4 VM); **the x86-64 entry carries over to GitHub's runner**: on #391's first CI runs (ubuntu-latest, its own clang and valgrind) all 33 rows
were between 0.98 and 1.11 of the instruction counts recorded on the i7, two runs alike. That is the evidence for the factor:
1.3 leaves room for compiler differences of that size, and 1.2 would too. If a later runner image moves a row past it, the entry
is re-recorded from the runner's own log in the same PR that notices, not loosened.

---

## 7. Open questions, for a person

**Q1. Does the verify-before-send check stay, and in what form?** It is 28.5% of the server's handshake, as expensive as the
signature. *Proposed:* **it stays** (a fault in the signer that leaks the key is the harm, and the check is the only defence
the design has, `tls-server.md` §3.3), and B2 makes it cheap rather than optional: a check against a fixed key is two combs,
2.9 times cheaper. Revisit only if, after B1 and B2, it is still more than 10% of the handshake. A flag to switch it off is not
proposed: a default no one reads is how the check goes missing.

**Q2. Where does a 60 KiB table live, and what may it cost?** B1's table for G, B2's for each identity and B3's for each trust
anchor are 61,440 bytes at 4-bit windows (38.5 KiB at 3 bits, 86 windows). A region is 64 KiB. *Proposed:* **the heap, owned by
the thing it belongs to**: G's table built once at engine open (about 15,000 field multiplications, three scalar multiplications' worth,
5 to 15 ms), an identity's when it is loaded or reloaded on `SIGHUP`, before the swap, and a trust anchor's when it is added.
Budget: 64 KiB for each of at most 16 identities (1 MiB) and a table per root, lazily. Whether a literal `&static [byte]` for
G's table is better (no start-up cost, 60 KiB of binary) is a measurement for the first PR to make.

**Q3. How does A1 land: a chunk free list in the codegen, or `mallopt` in the emitted `main`?** *Proposed:* **the free list**: a
small number (8) of freed chunks kept per thread in a thread-local, taken and returned where `malloc` and `free` are emitted
(`cancho-codegen-llvm/src/body/memory.rs`, and `cancho-codegen`'s equivalent), because `mallopt` is glibc's and the compiler
also targets macOS, and musl is a fair thing to ask for; `mallopt(M_TRIM_THRESHOLD, 16 MiB)` is the one-line stopgap that
proves the gain in the meantime. Programs that `spawn` threads need the list per thread or a lock; per thread is the
proposal. This is a compiler change, so it takes the full gate (both backends, the conformance and differential tests).

**Q4. Is C1 built before #381's prototype?** *Proposed:* **yes, first.** It needs no compiler change, it is the baseline the
wide-multiply prototype must beat, and without it #381 will report a 2.5 times that belongs to the limb size and not to the
builtin. The prototype's question becomes "what does 51-bit limbs with a wide multiply give over 25.5-bit limbs with a 64-bit
multiply", which is the question the issue means to ask.

**Q5. Are the targets of §1 the right ones?** In particular, T1 counts cycles of the whole process (kernel included) against
`s_server` and accepts 5 times because OpenSSL does not run the check. *Proposed:* as written; a wall-clock handshake rate on a
named machine is quoted beside it in each PR but is not the gate, since §2.1 says the clock is not stable.

**Q6. Is the guard a required check, and at what factor?** *Proposed:* **required**: its first CI runs showed the x86-64 baseline
carries over (0.98 to 1.11 on all rows). 1.3 on instruction counts is kept (a primitive that gets 30% dearer is a finding), and
the baseline is re-recorded in the same PR by anything that moves a primitive. If a runner image or clang ever needs a second
x86-64 entry, record it under its own tag rather than raise the factor.

**Q7. Does the SHA-256 instruction (D3) get a builtin?** *Proposed:* **not yet.** SHA-2 is 0.7% of a handshake today and
becomes about 4% after the levers of §4.3; decide then, with the vector-type question for ChaCha20 (§4.4), since both
are "a CPU feature the language can't name" and want one design.

**Q8. What does "within 3 to 5 times" mean for P-384, RSA and TLS 1.2?** Not measured here (§5). *Proposed:* P-256 and X25519
are the targets; P-384 is 1.7 times the P-256 handshake (`tls-server.md` §6) and follows B5; RSA signing is #385's.

---

## 8. Changes to the tracking issue's list

- Add **A1** (the allocator) as a new sub-issue, first.
- #380: add B2 (a table for the server's own key, for the check) and B3 (for each trust anchor), and reorder: the comb, then the
  per-key tables, then the reduction.
- Add **C1** (X25519 on 25.5-bit limbs) as a sub-issue or fold it into #381 as the baseline of its prototype.
- #379 stays and moves below A1, B1, B2: it helps only a client that returns.
- #382 stays, for bulk; it does not move the handshake or a small request.
- New small items: A2, C3, D3, D4.
- Add the guard (§6) to the gates of every sub-issue.

## 9. Not done

- No lever is built; every gain in §4 is arithmetic from the profile.
- A baseline for GitHub's arm64 runner (`macos-latest`): the job runs on ubuntu-latest only, so `aarch64` is guarded where someone runs it (§6).
- Cycle counts on arm64, a quiet x86-64 machine, a steady-state client, memory per connection, P-384, RSA, TLS 1.2, Cranelift
  and latency (§5).
- `tls_serve`'s 0.9 ms difference from `tls_echo` (§3.3).
