# TLS performance: where the time goes, and the design for closing the gap with OpenSSL

> **Status: measured, with a design. First deliverable of #378.** Nothing here changes the TLS stack. What it adds is
> a measured profile of a full server handshake and a full client handshake on x86-64 and arm64, a ranked list of
> levers with the gain of each computed from the measured parts, the targets and gates stated before the numbers
> (§1), a regression guard that is built and runs in CI (§6), and the questions a person has to answer (§7). Every
> figure says whether it was measured or is arithmetic from measured parts. Not independently reviewed (#209).
>
> **As built (§10):** after #392 to #398 the same profile was measured again on the same machines: a full server handshake is
> now 2.86 times OpenSSL's cycles on x86-64 (2.12 times its CPU on arm64), the client's 2.77 (2.26), and the allocator is the
> largest single cost of every handshake. §10 compares each lever with its projection, gives the targets' state and ranks the
> next levers; claims below that the new measurements show false carry a note in place.
>
> **A1 as built (§10.11):** the allocator lever, built (`region-chunk-cache.md`): the full server handshake 1.86 times OpenSSL's
> cycles on x86-64 (1.50 times its CPU on arm64), a resumed one 1.04 (1.12), which meets T5 as §7 Q9 restates it for the server; the
> client's resumed handshake is 1.21 (1.33), just outside it.

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
   glibc not to trim made a request **1.9 to 2.5 times cheaper on x86-64** (two alternating pairs) and took `https_hello`
   to the cost of an OpenSSL server answering the same request, measured; on arm64 one pair gave 1.78 times, in a VM whose
   load changed between the two runs, so that figure is weaker. This is the first lever, a few lines, and not in any sub-issue.
   *As built (§10.4, §10.5): a kept request no longer reaches the allocator (no `brk` in a 5-second trace, 1% of the samples)
   and costs 1.11 to 1.15 times OpenSSL's without A1; a handshake still does (112 `brk` calls), and the same 0.9M cycles are
   now 32% of it. A1 is not built.* *As built (§10.11, `region-chunk-cache.md`): A1 is built; a whole server run makes 9 `brk`
   calls instead of 112 a handshake.*
3. **The 3 to 5 times target is reachable without a new compiler builtin.** Fixed-base tables for the signature and for
   the server's own check, X25519 on 10 limbs of 25.5 bits (which the language's 64-bit `int` allows), the allocator, and
   NIST-prime reduction together take the server's handshake from **15 times OpenSSL's cycles to about 4.9 times**
   (§4.3, arithmetic from measured parts): inside the 5 times, by 1%, so the target is reachable but has no margin, and two
   of the assumptions (2.5 times on X25519's field arithmetic, 1.7 times on P-256's) decide whether it is met. Wide
   multiply (#381) is the one lever the language cannot do today; at an assumed 4 times on the field multiplications it
   gives about 3.4 times, and 3 times needs about 5.
   *Corrected (§10.6): the target was reached by another route, and further. Wide multiply was built (#393) and moved X25519;
   #397 moved P-256 on a 28-bit field without it. The server's handshake is 2.86 times OpenSSL's cycles with neither A1 nor B2
   nor C1: the operations became 6.8 (the check) to 17 (the ladders) times cheaper where this assumed 1.7 to 4 times on their
   field multiplications; "3 times needs about 5" was false: 3 times was reached with wide multiply on X25519 only.*
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
| **T5.** resumed handshake (#379) | no more than the full handshake's two X25519 operations **plus 10 percentage points of a full handshake** (41.8% of today's full handshake at today's shares, §4.3). Not 1.1 times the X25519 operations: that reading (35.0%) is not met by tickets with A1 alone. *Restated 2026-10-13 (§7 Q9, decided as proposed): **at most 1.2 times OpenSSL's resumed handshake, server and client; stretch: parity**, the same form as T1 to T3. The bound above fell every time the full handshake got cheaper. As built (§10.3): 2.34 and 2.37 times on x86-64, 2.17 and 2.12 on arm64; with glibc not trimming (A1's experiment) 1.05 and 1.14 times.* *With A1 built (§10.11): server 1.04 and client 1.21 on x86-64, 1.12 and 1.33 on arm64: met for the server, not for the client.* | |

T1 does not give up the verify-before-send check (`tls-server.md` §3.3): OpenSSL does not make one, so T1 compares a
server that does more with one that does less. That is the honest comparison and is why the target is 5, not 3.

### 1.2 The figures per primitive that make T1 and T2 add up

A handshake is its parts (§3.3 shows it is), so T1 is met by the parts below, with OpenSSL's cycles from §3.1. These are
the bounds a change to a primitive is measured against, not goals to stop at:

| primitive | OpenSSL (cycles) | bound | in cycles | what reaches it (projected with §4.1's assumptions) |
|---|---|---|---|---|
| X25519, one scalar multiplication | 114k | at most 6 times | 0.68M | C2 (0.65M, 5.7 times); C1 alone projects 0.99M, 8.7 times |
| ECDSA P-256 sign | 77k | at most 8 times | 0.61M | B1 + C2 (0.60M, 7.7 times); B1 + B5 project 0.99M, 12.9 times |
| ECDSA P-256 verify, the server's own key (the check) | 243k | at most 6 times | 1.46M | B2 + B5 (1.28M, 5.3 times) |
| ECDSA P-256 verify, a key never seen before | 243k | at most 15 times | 3.6M | B4 + B5 (2.19M, 9 times) |
| P-256 ECDH, one scalar multiplication | 189k | at most 10 times | 1.9M | C2 (1.82M, 9.6 times); B5 alone projects 3.0M, 16 times |
| `brk` calls a handshake / a request | | at most 2 / 0 (from 108 / 4) | | A1 (*as built, §10.11: 9 in a whole run of 5,148 handshakes / 0*) |
| everything else in a handshake (HKDF, hashes, records, parsing, loop) | about 0.8M for OpenSSL's whole non-asymmetric part | at most 1M | | today's 0.8M |

Two X25519 operations at 6 times, a signature at 8, a check at 6 and today's 0.8M of the rest come to 4.2M cycles, which is
3.9 times OpenSSL's 1.09M and inside T1's 5.45M. These bounds are tighter than T1 needs, on purpose: they leave room for what
the projections get wrong. The levers without a builtin (§4.3) meet the check's bound but not X25519's or the signature's, and
reach T1 by 1% only; the bounds for X25519, the signature and P-256 ECDH need wide multiply.
*Corrected (§10.2): as built every bound is met, and only X25519's needed wide multiply: X25519 1.40 times OpenSSL (bound 6),
the signature 4.58 (bound 8), a verification 2.98 (bounds 6 and 15), P-256 ECDH 5.51 (bound 10), all on x86-64 in cycles;
the signature and ECDH on #397's 28-bit field, with no builtin. The rest of the handshake is not 0.8M under its 1M bound
but about 0.6M (§10.4), and `brk` is 112 calls a handshake against a bound of 2.*

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
| pinned to | CPU 0 for the primitives (its sibling, CPU 1, idle); server on CPU 0 and client on CPU 1, its SMT sibling, for handshakes. Tested both ways: cancho's server 15.5 and 16.0M cycles with the client on the sibling, 15.8 and 17.8M with it on the same CPU (no systematic effect); OpenSSL's lean server (`oserv`) 0.96M with the client on the sibling and 0.89 to 0.90M on the same CPU, so the sibling costs OpenSSL about 7% and the ratios of §3.3 are about 7% kinder to cancho than a quiet sibling would make them | CPU 0 and 1 of the VM |
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
| OpenSSL's server | `openssl s_server -tls1_3 -num_tickets 0 -groups X25519`, AES-128-GCM, the same certificate, the same `s_time -new`; and `benches/server/oserv.c`, a leaner OpenSSL server (accept, `SSL_accept`, `epoll`, nothing else), under the same `s_time -new` |
| a full client handshake | `tests/programs/tls_many.cho` (the engine's own client, certificates verified against the CA, the name checked) against `openssl s_server -HTTP`, 200 connections one run and 1 the other, the difference over 199 |
| OpenSSL's client | `benches/server/tconn.c` against `oserv` (above): `SSL_VERIFY_PEER`, the host name set, aborts on the first handshake that does not verify (`s_time` does not fail on a verification error, so it cannot say the check ran). It sends no request: `tls_many`'s 16 KiB request is about 0.1M of cancho's 19.4M cycles and is not subtracted |
| where the time goes | `perf record -F 1999 --call-graph dwarf,32768 -p <server>` for 6 s while `s_time -new` runs (the compiled code keeps no frame pointers; DWARF unwinding gave a complete stack in all but 0.04 to 0.7% of the samples), sorted into the categories of §3.3 by `scripts/tls_perf.py profile` |
| a request on a kept connection | `scripts/https_hello_test.py --cost 5` with `--kload --tload --plain --oserv` (server on one core, two load threads on another, 32 connections) |
| allocator experiment | the same runs with `GLIBC_TUNABLES=glibc.malloc.trim_threshold=33554432:glibc.malloc.top_pad=1048576`, alternating, and `LD_PRELOAD` counting `malloc`, `strace -c` counting `brk` |

On arm64 the same commands, without `perf stat`; the profile is `perf record -e cpu-clock` from a privileged container.

---

## 3. The measurements

*Every table below is from one of the two machines of §2.1: x86-64 is the i7-1260P, Ubuntu 26.04, Linux 7.0.0-38, `powersave`, THP `madvise`, load average 4 to 10, shared; arm64 is the M4 Max VM, Ubuntu 24.04, Linux 6.8.0-100, THP `madvise`, load average 1 to 7. A table says where it is from only if it differs.*

### 3.1 Each primitive alone

`tls_prims_bench`, x86-64, user-space cycles (the least of three passes) and instructions (exact), against OpenSSL's
cycles from `openssl speed` on the same core. "Ratio" is cancho's cycles over OpenSSL's.

| operation | cancho: instructions | cancho: cycles | OpenSSL: cycles | ratio |
|---|---|---|---|---|
| X25519 key pair | 12.19M | 2.34M | 114k | **20.6** |
| X25519 shared secret | 12.19M | 2.52M | 114k | **22.2** |
| P-256 ECDH, one scalar multiplication | 23.45M | 4.53M | 189k | **24.0** |
| ECDSA P-256 sign | 25.25M (25.33M under callgrind) | 4.66M | 77k | **60.8** |
| ECDSA P-256 verify | 25.25M | 4.58M | 243k | **18.9** |
| sign, then verify before sending | 50.50M (50.58M under callgrind) | 9.27M | | |
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
  measured with `perf stat`), and the cycle ratio is 20 to 24 because the compiled code runs at an IPC of 5.2 (X25519, ECDH) to 5.5 (ECDSA) where
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

**How much of OpenSSL's 1.09M is public-key work.** Its two X25519 operations and its signature are about 0.31M (§3.1); the
other 0.78M is `s_server`, the TLS state machine, the kernel's `accept`, `read`, `write` and `close`, and OpenSSL's own
allocation. A leaner OpenSSL server, `oserv` (no BIO chain or `s_server` options, just `SSL_accept` and `epoll`), measured in a
second session (2026-10-12, load average 2.4 to 3.4): **0.89 to 0.96M cycles and 1.77M instructions a handshake**, against
`s_server`'s 1.04 to 1.05M and 1.85M in the same minutes and `tls_echo`'s 15.5 to 17.8M and 77.8M. So the lean figure is 10 to
15% smaller and the ratio against it about 17 times, not 15; T1 is stated against `s_server`, which is what #378 measured, and
the 5.45M bound is about 4.7M against `oserv`, which §4.3's projection (5.4M) does not meet.

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

**On x86-64 `https_hello` costs 2.3 times OpenSSL's server per request, and after the allocator experiment about 1.05 times**
(1.9 and 2.5 times cheaper in the two pairs). On arm64 the gap was 1.3 times, and the one pair there gave 1.78 times
cheaper, confounded: the VM was busier for the second run (the plain row rose by 69%), so it says only that the effect is
there, not how large. TLS's own cost over plain (record seal, open, copies) is small: OpenSSL's is 9 us of 21 on
x86-64.

Where the request goes (`perf record`, 8,366 samples on x86-64, 8,812 on arm64):

| category | x86-64 | arm64 |
|---|---|---|
| **memory: `malloc`, `free`, `brk`, page faults** | **52.9%** | **45.1%** |
| system calls (`send`, `recv`, `epoll`; the loopback delivery of a `send` is charged to the sender) | 29.7% | 35.8% |
| HTTP parser, application, loop | 11.4% | 13.0% |
| record AEAD | 5.8% | 5.9% |

*As built (§10.5): no `brk` at all in a 5-second trace of a loaded `https_hello`, and memory 1% of a request's samples; the
paragraph below describes `edfca60`.* **A request opens about 6 regions** (`LD_PRELOAD` counting: 851,022 mallocs of 65,536 bytes for 140,640 requests and 32
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
| **A1** | **Stop `region` returning its chunk to the kernel**: keep a few freed 64 KiB chunks (a per-thread free list, or `mallopt(M_TRIM_THRESHOLD)` in the emitted `main` as a stopgap). Compiler change, both backends | **new** | measured: 5.7% / 9.9% of a handshake; experiment 1.9 to 2.5 times per request on x86-64 (arm64: one confounded pair, 1.78) | **5.7%** (1.06 times) | **9.9%** | **48 to 60%** on x86-64 (1.9 to 2.5 times) | small |
| **B1** | **Fixed-base comb for k·G**, signing: 64 windows of 4 bits, 64 mixed additions instead of 256 doublings and 78 additions | #380 | `callgrind`: a signature is 5,280 Montgomery multiplications, of which the scalar multiplication is about 4,430; a comb needs 64 × 13 = 830, plus 850 for the two inversions | **19.7 points** (sign ÷ 3.1) | | | medium |
| **B2** | **A fixed-base table per identity for the check**: the server's public key is fixed for the life of the certificate, so `u1·G + u2·Q` is two combs, no doublings, no table built per call | **new** (in #380's area) | a verification is 6,015 multiplications; two combs 1,664 plus *two* inversions (s⁻¹ mod n, `std/ecdsa.cho` `scalars`, and the affine x mod p), 850 as in B1: about 2,510 | **16.6 points** (check ÷ 2.4) | | | medium |
| **B3** | The same table per **trust anchor** on the client, built when the root is added to the store | **new** | the chain's verification is 27.2% of a client handshake; a fixed key is B2's case (÷ 2.4) | | **15.8 points** | | small after B2 |
| **B4** | `u1·G + u2·Q` for a key never seen: G by the comb, Q by 5-bit wNAF with the cheaper non-complete formulas (public data) | #380 | 2,048 doublings at 8 multiplications, about 43 additions, 64 mixed additions, two inversions: about 4,300 against 6,015 | | 7.8 points (`CertificateVerify` ÷ 1.4) | | medium |
| **C1** | **X25519 on 10 limbs of 25.5 bits** with the 64-bit `int` the language has (`x25519.md` §6 names it), a dedicated square, products kept in registers | **new** (#381's baseline) | 100 products a multiplication instead of 256; today about 4,000 instructions a multiplication; assumed 2.5 times on the field multiplications, which the profile puts at 96% of a ladder (`field25519.mul` 78%, `product_into` 18.5%) | **18.4 points** | 16.1 points | | medium |
| **B5** | NIST-prime reduction for P-256 in place of generic Montgomery (and 8 limbs of 32 bits) | #380 | `mont_mul` and `ct_reduce` are 79% of the signature's time; 64 products plus a fold in place of 162; assumed 1.7 times on that 79% only | 6.9 points after B1 and B2 (their 21.1 × 0.79 × (1 − 1/1.7)) | 10.1 points after B3 and B4 | | medium |
| **D1** | **Tickets**: a resumed handshake skips the signature, the check and the chain | #379 | a resumed handshake is X25519 + memory + the rest = 42.5% of a full one | resumed: **57.5%** (2.35 times); only for returning clients | resumes already (`tls-resumption.md`) | | large |
| **C2** | **Wide multiply and add-with-carry** | #381 | assumed 4 times on the field multiplications (96% of X25519, 79% of the ECDSA operations), after A1, B1, B2 | with A1 + B1 + B2: **77.6%** removed (4.5 times in all) | | | large, compiler |
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
  *As built: #393 added `mul_wide`, `add_carry` and `sub_borrow` (edition 7, `wide-multiply.md`) and X25519 runs on 51-bit limbs;
  this bullet describes the language at `edfca60`. P-256, `bigmod` and Poly1305 are not on them yet (§10.8 P1, M1).*
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
| + B2 (per-identity table, the check) | 58.0% | 9.6 | 8.8 |
| + C1 (X25519 on 25.5-bit limbs: 2.5 times on 96% of it) | 39.6% | 6.5 | 6.0 |
| + B5 (NIST-prime reduction: 1.7 times on 79% of the ECDSA operations) | **32.6%** | **5.4** | **4.9** |
| *alternatively* A1 + B1 + B2 + C2 (wide multiply, 4 times on the same field multiplications) | 22.4% | 3.7 | 3.4 |
| A1 + B1 + B2 + C2 at 5 times | 20.0% | 3.3 | 3.0 |

- **T1 (5 times) is reached by A1, B1, B2, C1 and B5, none of which needs a compiler builtin, by 1%** (5.39M against 5.45M): no
  margin. It rests on two assumptions the prototypes have to bear out (2.5 times on X25519's field multiplications, 1.7 times on
  P-256's); C3 (3.2 points) and A2 (0.9) are the small levers that would give it some. Against the leaner `oserv` it is not reached
  (§3.3). **The 3-times stretch needs wide multiply at about 5 times on the field multiplications.**
  *Corrected (§10.6, §10.7): as built the server's handshake is 2.86 times `s_server` (3.04 times `oserv`) without A1, B2 or C1;
  the stretch was reached because the operations became 6.8 (the check) to 17 (the ladders) times cheaper, where this assumed
  2.5 to 5 times on their field multiplications.*
- *Corrected (review of #391):* this table said 28.9% and 4.4 times. B2 was priced with one inversion where a verification
  does two, and B5's 1.7 times was applied to the whole ECDSA time where the basis column says 79% of it.
- The client handshake with A1, B3, B4, C1 and B5: 100 − 9.9 − 15.8 − 7.7 − 16.1 − 10.1 = **40.4%**, which is **7.8M cycles,
  4.6 times OpenSSL's 1.70M**: inside T2's 5, by 8%. (Arithmetic; the same assumptions. It was 35.7% and 4.1 times before the
  review's correction.)
- **Tickets (D1)** give a returning client a handshake of 42.5% of a full one (7.0M cycles, 6.4 times a *full* OpenSSL handshake; the
  client side, which resumes already, measured 2.2 times cheaper in hooks, `pure-tls.md`, against the 2.35 here);
  with A1 36.8% (6.1M), which meets T5 as §1.1 defines it (41.8%) and would not meet a 1.1-times reading (35.0%), and with C1 as
  well 18.4% (3.0M): the resumed
  handshake is the X25519 operations plus the rest, and nothing else is left to cut but the ladders.
  *As built (§10.3, §10.7): a resumed server handshake is 2.00M cycles, 0.71 of the as-built full one and 2.34 times OpenSSL's
  resumed one; the ladders are 15% of it and the allocator 52%. T5 is not met, with or without A1's experiment.* *As built
  (§10.11): with A1 a resumed server handshake is 0.884M cycles, 1.04 times OpenSSL's resumed one.*
- **A kept request** is 2 to 2.5 times cheaper with A1 alone, at OpenSSL's cost (§3.5): T3 needs only A1.
  *Corrected (§10.5): T3 did not need A1. As built a request opens no region that reaches the kernel and costs 1.11 to 1.15
  times OpenSSL's server, and glibc told not to trim no longer changes it.*
- The arm64 shares give the same ranking (X25519 39%, sign 26%, check 25%, memory 5%); with the larger X25519 share C1 matters more
  there (22.7 points removed by C1 alone, against 18.4).

### 4.4 Bulk

The cancho AEADs are at OpenSSL's cost for a 64-byte record and 4 to 9 times it at 1 and 16 KiB (§3.2). T4 asks for 3 times:
D2 (#382). The 102k cycles for 16 KiB are 6.2 cycles a byte at IPC 3.7: AES-NI's `aesenc` has a latency of 3 to 4 cycles and the
code is one block at a time, with a GHASH reduction a block. Four-block interleaving hides the latency (4 times at the AES) and
aggregating the reductions removes three of four; **2.5 to 3.5 times is the arithmetic**, which is #382's own hypothesis (2 to 4
times) and still to be measured with the prototype of the four-block path it asks for.
*As built (#392, §10.6): 9.5 times at 16 KiB, 9.0 at 1 KiB and 4.7 at 64 bytes on x86-64; AES-128-GCM is now 0.21, 0.57 and 1.15
times OpenSSL's cycles at those sizes, inside T4.*

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
python3 scripts/tls_perf.py record ./tls_prims_bench                  # a new baseline for this machine's <system>-<architecture>
python3 scripts/tls_perf_mutant.py <cancho>                          # shows the guard fails when two primitives get twice as dear
python3 scripts/tls_perf.py profile server.script.txt --total-us 4510   # §3.3's table from a perf script (--memory: §10.4's split)
python3 scripts/tls_perf.py openssl                                  # OpenSSL's cost of the same primitives (§10.2)
python3 scripts/tls_handshake_cost.py <bin> [--sudo-perf]            # full and resumed handshakes, both sides, beside OpenSSL (§10.3)
```

**What it compares.** With `valgrind` (callgrind), `check` counts the instructions each row executes, from the difference of two
runs with different numbers of calls, and fails a row more than **1.3 times** its recorded count, or less than 1/1.3 of it (a primitive that got that much cheaper
re-records its baseline in the same change, §1.3, so the improvement cannot drift back), a row the baseline has and the run
lacks, and a row the run has and the baseline lacks. A run under valgrind that does not exit 0 and print its `checksum` line is
an error, not a count. Instruction counts do not
depend on the clock or the load (§2.1), they equal the hardware's (12,193,474 for an X25519 under both), and the run takes 40
seconds on x86-64 and 12 on arm64. *As built (§10.10): 11.5 and 8.3 seconds, the primitives being cheaper to run under valgrind.* Without `valgrind` it falls back to the time of each row over the calibration's, at 2.0 times,
because that quotient moved 1.6 times between two runs minutes apart on the shared machine, which is why it is not the first
choice; and only against an entry recorded on the same system and architecture (`linux-x86_64`, `linux-aarch64`): the
`aarch64` entry from the Linux VM failed 11 of 33 rows on unchanged code when timed natively on macOS (found by the review of
#391), so time mode refuses a foreign entry. The calibration row is not what its source says on x86-64: LLVM folds the multiply-add chain out of the loop
(its constants are not in the binary) and leaves a counted loop of about one instruction and one cycle a step, 1,007
instructions a call. It is still independent of every primitive, which is all the fallback needs.

**What it catches, shown and not claimed.** `scripts/tls_perf_mutant.py` builds a copy of the benchmark with X25519 and SHA-256
each done twice and runs the guard on it; CI runs it after the guard. It failed exactly those four rows (`x25519_public`, `sha256` at 64, 1,024 and 16,384 bytes) at a ratio of 2.00 and passed the other 29. A change
that makes a primitive twice as slow is caught; one that adds 20% is not (1.3 is wide for what CI showed: the compiler and CPU differences between the machine that recorded the baseline and GitHub's
runner moved the counts by 2% down to 11% up).
**What it cannot catch:** a change in instruction *count* that leaves time alone or the reverse (a code layout that halves
the IPC), the handshake as a whole (a primitive added to the path), and memory. The allocator (A1) and the engine are in no row;
`scripts/tls_echo_test.py --cost` and `scripts/https_hello_test.py --cost` remain where those are measured.

**CI.** The `tls-perf` job builds the compiler and the benchmark and runs `check`. `valgrind` is not on the runner by default: the
job installs it. The baseline file holds one entry per system and architecture (`linux-x86_64` from the machine of §2.1, `linux-aarch64` from
the M4 VM; a macOS entry would be recorded on macOS); **the x86-64 entry carries over to GitHub's runner**: on #391's first CI runs (ubuntu-latest, its own clang and valgrind) all 33 rows
were between 0.98 and 1.11 of the instruction counts recorded on the i7, two runs alike. That is the evidence for the factor:
1.3 leaves room for compiler differences of that size, and 1.2 would too. If a later runner image moves a row past it, the entry
is re-recorded from the runner's own log in the same PR that notices, not loosened.

---

## 7. Open questions, for a person

**Q1. Does the verify-before-send check stay, and in what form?** It is 28.5% of the server's handshake, as expensive as the
signature. *Proposed:* **it stays** (a fault in the signer that leaks the key is the harm, and the check is the only defence
the design has, `tls-server.md` §3.3), and B2 makes it cheaper rather than optional: a check against a fixed key is two combs and two inversions,
about 2.4 times cheaper. That does not make it small: by this document's own numbers it is still **about 21% of the handshake after
B1 and B2, and about 25% after every lever of §4.3**, the largest single cost left. So the question is not deferred to a later
measurement; the proposal is to keep it at that price, because no cheaper check gives the same protection (recomputing the
signature by a second route costs as much as the signature), and to say so in `tls-server.md` §3.3. A flag to switch it off is
not proposed: a default no one reads is how the check goes missing.
*As built (§10.4): the check is 24.4% of the server's handshake on x86-64 and 25.6% on arm64, as projected here, and twice the
signature, since the signature has the comb and the check does not. B2 (§10.8) would take it to about 10% after P1.*

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
*As built (§10.8): still open, now the first lever (34% of a server handshake, 55% of a resumed one, measured by the
experiment). #375 asks for the same per-thread cache for macOS's sake; a cache of one chunk would not do here, because the key
schedule nests regions (`hmac.init` inside `hkdf.expand` inside `hkdf.expand_label`).*

**Q4. Is C1 built before #381's prototype?** *Proposed:* **yes, first.** It needs no compiler change, it is the baseline the
wide-multiply prototype must beat, and without it #381 will report a 2.5 times that belongs to the limb size and not to the
builtin. The prototype's question becomes "what does 51-bit limbs with a wide multiply give over 25.5-bit limbs with a 64-bit
multiply", which is the question the issue means to ask.
*As built: #393 built the wide multiply first and measured it against the 16-bit code (23 times on the M4) and against a C
estimate of 51-bit limbs (`wide-multiply.md` §2); C1 was not built, and is no longer needed.*

**Q5. Are the targets of §1 the right ones?** In particular, T1 counts cycles of the whole process (kernel included) against
`s_server` and accepts 5 times because OpenSSL does not run the check. *Proposed:* as written; a wall-clock handshake rate on a
named machine is quoted beside it in each PR but is not the gate, since §2.1 says the clock is not stable.

**Q6. Is the guard a required check, and at what factor?** *Proposed:* **required**: its first CI runs showed the x86-64 baseline
carries over (0.98 to 1.11 on all rows). 1.3 on instruction counts is kept (a primitive that gets 30% dearer is a finding), and
the baseline is re-recorded in the same PR by anything that moves a primitive. If a runner image or clang ever needs a second
x86-64 entry, record it under its own tag rather than raise the factor.

**Q7. Does the SHA-256 instruction (D3) get a builtin?** *Proposed:* **not yet.** SHA-2 is 0.7% of a handshake today and
becomes about 2% after the levers of §4.3 (0.67 of 32.6); decide then, with the vector-type question for ChaCha20 (§4.4), since both
are "a CPU feature the language can't name" and want one design.

**Q8. What does "within 3 to 5 times" mean for P-384, RSA and TLS 1.2?** Not measured here (§5). *Proposed:* P-256 and X25519
are the targets; P-384 is 1.7 times the P-256 handshake (`tls-server.md` §6) and follows B5; RSA signing is #385's.

**Q9. Is T5 restated against OpenSSL's resumed handshake?** (Added with §10.) *Decided as proposed (2026-10-13): T5 is now at most 1.2 times OpenSSL's resumed handshake, server and client, stretch parity (the T5 row in §1.1 says so).* T5 bounds a resumed handshake by the full one's
two ladders plus 10 points of it; as built it is not met (2.00M cycles against 0.58M on x86-64) and the bound falls every time
the full handshake gets cheaper. *Proposed:* **at most 1.2 times OpenSSL's resumed handshake, server and client, stretch parity**:
the same form as T1 to T3. As built it is 2.34 and 2.37 times; with glibc not trimming (A1's experiment) 1.05 and 1.14 times
(§10.3).

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

*As built:* §10.8 is the list now; #378's checklist carries it.

## 9. Not done

- No lever is built; every gain in §4 is arithmetic from the profile. *As built (§10.6): B1, B4 and B5 (#397), C2 on X25519
  (#393), D1 (#394), D2 (#392) and E1 (#395) have been; A1, B2, B3, C1, C3, A2, D3 and D4 have not.*
- A baseline for macOS (`darwin-aarch64`): the job runs on ubuntu-latest only, and the `linux-aarch64` entry is guarded where someone runs it on Linux (§6).
- Cycle counts on arm64, a quiet x86-64 machine, a steady-state client, memory per connection, P-384, RSA, TLS 1.2, Cranelift
  and latency (§5).
- `tls_serve`'s 0.9 ms difference from `tls_echo` (§3.3).

---

## 10. As built: the profile again, after #392 to #398

> **Measured on `0e40f430`** (`origin/main` after #398), on the same two machines as §2, with the same tools and the same
> OpenSSL beside each figure. Since §3 was measured (`edfca60`), these landed: #393 (`mul_wide`, `add_carry`, `sub_borrow`;
> X25519 on five 51-bit limbs), #397 (P-256 on a ten-limb 28-bit field using the prime's form, a fixed-base comb for k·G,
> wNAF verification), #392 (multi-block AES-GCM, aggregated GHASH), #394 (server session tickets), #395 (a core per slot and
> leased work areas), #396 and #398 (client certificates and ALPN; mutual TLS in the server). Not built: A1, B2, B3, C1, C3,
> A2, D3, D4 *(A1 built since, §10.11; D3 and D4, §10.14)*. Raw output of every table is in
> [`assets/tls-performance/`](assets/tls-performance/), one directory a machine.

**What it found, in the order it matters:**

1. **A full server handshake is now 2.86 times OpenSSL's `s_server` in cycles on x86-64 (2.83M against 0.99M) and 2.12
   times in CPU time on arm64; the client's is 2.77 and 2.26 times.** §4.3 projected 4.9 and 4.6 times, with the allocator
   and two levers that were not built (§10.6). T1 and T2 are met, and their 3-times stretch too (by 5% on x86-64 against
   `s_server`; not against the leaner `oserv`, 3.04 times). A resumed handshake is 2.34 (x86-64) and 2.17 (arm64) times
   OpenSSL's resumed one; a request on a kept connection 1.11 to 1.15 times.
2. **The allocator is now the largest cost of every handshake, and nothing else comes close on a resumed one.** `malloc`,
   `free` and `brk` are 31.6% of a full server handshake on x86-64 (28.3% on arm64), 52.5% of a resumed one (48.5%), and
   35.2% of a client's (23.7%) (§10.4). It is the same 0.9M cycles §3.3 measured as 5.7%: the rest shrank around it. glibc
   still trims and regrows the heap: **112 `brk` calls a handshake** (§3.3 counted 108). Telling glibc not to trim, alternating
   with the default, made the server's full handshake **34% cheaper** (2.83M to 1.86M cycles, 1.87 times OpenSSL), the resumed
   one **55%** (1.06 times OpenSSL's resumed), and the client's **40%** (1.65 times) on x86-64; 27%, 50% and 32% on arm64.
   **A1 is now the first lever by a factor of two** (§10.8), and it is the same mechanism as #375's chunk cache.
   *As built (§10.11): A1 took the full server handshake to 1.844M cycles (1.86 times `s_server`), a resumed one to 0.884M
   (1.04 times OpenSSL's resumed), the client's to 2.664M (1.64 times `tconn`); arm64 1.50, 1.12, 1.51 times.*
3. **A kept request no longer touches the allocator** (no `brk` in a 5-second trace on either machine, 1% of the samples),
   so A1 does nothing for it; it is within T3 without it. What it spends instead, beside the sockets, is #395's work areas:
   copying a connection's core into an area and back out and clearing the area is **24% (x86-64) to 27% (arm64) of a request**.
   *As built (§10.12): 3.6%, and a kept request 0.94 times OpenSSL's cycles on x86-64 (from 1.20).*
4. **Each primitive is 1.3 to 5.5 times OpenSSL now, not 19 to 71**: X25519 1.40 (x86-64 cycles) and 1.28 (arm64), the ECDSA
   signature 4.6 and 4.3, a verification 3.0 and 3.0, a P-256 ECDH 5.5 and 5.2. AES-128-GCM is below OpenSSL's cost at
   64 bytes and 1 KiB and within 1.15 times at 16 KiB. SHA-256 (4.5 to 11 times) and ChaCha20-Poly1305 (2.6 to 11.5) did
   not move. Each costs inside a handshake what it costs alone, as in §3.
5. **The next levers, ranked by measured gain** (§10.8): A1 (34% of a server handshake, measured by the experiment), P-256 on
   `mul_wide` (about 15 points, arithmetic; *as built, §10.15: 19.9% of the A1-built handshake, 13 points of this one*), the per-identity table for the check (about 7.5), a SHA-256 builtin (about 6.5,
   and 10 of a resumed handshake), then small ones; for a kept request, the work-area copies (up to 24%). Applied in that order
   the server's full handshake comes to about 1.1 to 1.2 times OpenSSL's (arithmetic from measured parts).

### 10.1 How it was measured

| | x86-64 | arm64 |
|---|---|---|
| machine | `gram`, Intel Core i7-1260P, as §2.1 | Apple M4 Max, Docker Desktop 29.2.1 (a VM of 6 vCPUs), image `lexsys-hooks-env`, as §2.1 |
| system | Ubuntu 26.04.1, Linux 7.0.0-38-generic, governor `powersave`, THP `madvise`; clang 21.1.8, valgrind 3.26 | Ubuntu 24.04.5, Linux 6.8.0-100-generic, THP `madvise`; clang 18.1.3, valgrind 3.22 |
| clock | 2.6 to 4.25 GHz in these runs (cycles over time); quieter than §2.1's session | not visible from the VM |
| load | load average 0.2 to 3.5 (2026-10-12, 21:19 to 22:07 CEST); a soak's database and other containers on the box; this work on CPUs 0 to 3 only | VM load average 1.4 to 4.5; the Mac's own 4.6 to 11 (other containers and builds) |
| pinned to | the process measured on CPU 0, its peer on CPU 1 (CPU 0's SMT sibling, as §2.1), load generators on CPUs 2 and 3 | the same CPU numbers in the VM |
| counters | primitives: `cycles:u` and `instructions:u` (`kernel.perf_event_paranoid` lowered to 2 for the run and restored to 4); handshakes: user **and kernel** cycles and instructions, through `sudo perf stat -p` (as §2.1's); profiles: `cycles` | none (no PMU): CPU time, and `perf record -e cpu-clock` as root in a privileged container |
| OpenSSL | 3.5.5 | 3.0.13 |
| compiler | cancho at `0e40f430`, LLVM backend | the same |

The commands are §2.2's, now in two scripts so that the next profile is one command: `scripts/tls_perf.py openssl` runs the
`openssl speed` commands §2.2 lists, and `scripts/tls_handshake_cost.py` runs the handshake rows (`openssl s_time -new` against
`tls_echo`, `openssl s_server -num_tickets 0` and `oserv`; `tls_many` with 200 connections less `tls_many` with 1; `tconn`), each
row alternating with the others for three or four rounds, the least kept. Three things differ from §2.2:

- **The resumed rows are new.** `openssl s_time -reuse` does not resume a TLS 1.3 session (`scripts/tls_server_cost.py` says
  so), so `benches/server/tconn.c` has a `resume` mode: one full handshake that reads the server's tickets, then every connection
  offers that session and the run fails on the first that does not resume. The server's resumed handshake is `tconn ... resume`
  against `tls_echo --tickets 2` and against `openssl s_server` (two tickets, its default); the client's is `tls_many ... resume`
  (200 full, then 200 resumed) less `tls_many` with 200, over 200, and `tconn ... resume`, both against `openssl s_server`.
- **Both clients run against `openssl s_server`** (§3.4 ran `tconn` against `oserv`): the same server for both sides of the ratio.
- **`tls_perf.py profile`** now finds a P-256 verification by `std.p256_vf.verify` (#397; the server's check reaches it from
  `tls_server.sign` with `sign_checked` and `verify_raw` inlined away) and X25519 by `std.field25519_51` as well as
  `x25519.scalarmult` (a ladder inlined into its caller leaves only its field); `--memory` splits the memory category by
  kernel and by caller.

### 10.2 Each primitive

`tls_prims_bench` and `openssl speed` on the same core; x86-64 cycles are the least of two `table` passes of three repeats
(`instructions:u` exact), arm64 times the least of two passes (three and five repeats). The ratio of §3.1 is beside each.

| operation | x86-64: instructions | cycles | OpenSSL cycles | **ratio** | §3.1 | arm64: ns | OpenSSL ns | **ratio** | §3.1 |
|---|---|---|---|---|---|---|---|---|---|
| X25519, shared secret | 694k | 148.8k | 106.4k | **1.40** | 22.2 | 24,963 | 19,452 | **1.28** | 31.4 |
| X25519, key pair | 694k | 148.7k | 106.4k | **1.40** | 20.6 | 31,005 | 19,452 | 1.59 | |
| P-256 ECDH, k·P | 5.00M | 971.9k | 176.4k | **5.51** | 24.0 | 134,765 | 26,102 | **5.16** | 29.4 |
| P-256 k·G (`ecdh.public_key`) | 1.20M | 231.7k | | | | 32,836 | | | |
| ECDSA P-256 sign | 1.63M | 326.6k | 71.3k | **4.58** | 60.8 | 50,781 | 11,691 | **4.34** | 70.9 |
| ECDSA P-256 verify | 3.49M | 686.8k | 230.8k | **2.98** | 18.9 | 104,003 | 34,811 | **2.99** | 23.8 |
| sign, then verify before sending | 5.12M | 1,011.3k | 302.1k (sign + verify) | 3.35 | | 154,296 | 46,502 | 3.32 | |
| HKDF-Expand-Label (32 bytes out) | 34.5k | 8.8k | | | | 4,417 | | | |
| HKDF-Extract | 33.4k | 5.6k | | | | 1,144 | | | |
| AES-128-GCM key preparation | 36.9k | 6.2k | | | | 1,243 | | | |

On the arm64 rows the key pair and the shared secret are the same work (525,613 instructions each under valgrind) and the
first row of every pass is the slower one in both passes; the shared secret is the figure compared.

| operation | bytes | x86-64: instructions | cycles | OpenSSL cycles | **ratio** | §3.1, §3.2 | arm64: ns | OpenSSL ns | **ratio** | §3.1, §3.2 |
|---|---|---|---|---|---|---|---|---|---|---|
| SHA-256 | 64 | 15.2k | 2,513 | 563 | 4.46 | 4.6 | 530 | 91 | 5.85 | 6.5 |
| SHA-256 | 1,024 | 122k | 20,985 | 2,526 | 8.31 | 8.4 | 4,249 | 415 | 10.2 | 10.9 |
| SHA-256 | 16,384 | 1.83M | 318.1k | 33.9k | 9.38 | 10.0 | 63,476 | 5,599 | 11.3 | 12.1 |
| SHA-384 | 64 | 8.5k | 1,491 | 1,039 | 1.43 | 2.0 | 307 | 124 | 2.47 | 2.6 |
| SHA-384 | 1,024 | 58.8k | 10,811 | 5,753 | 1.88 | | 2,201 | 732 | 3.01 | |
| SHA-384 | 16,384 | 824k | 151.6k | 77.0k | 1.97 | 1.8 | 30,273 | 9,880 | 3.06 | 3.3 |
| HMAC-SHA-256 (not like for like, §3.1) | 64 | 40.2k | 6,703 | 687 | | | 1,411 | 125 | | |
| **AES-128-GCM seal** | 64 | 738 | 169 | 802 | **0.21** | 0.9 | 28 | 96 | **0.29** | 1.2 |
| AES-128-GCM seal | 1,024 | 3,143 | 745 | 1,307 | **0.57** | 4.1 | 171 | 218 | **0.79** | 4.6 |
| AES-128-GCM seal | 16,384 | 43.5k | 10,693 | 9,337 | **1.15** | 9.1 | 2,311 | 2,135 | **1.08** | 7.0 |
| AES-128-GCM open | 64 | 746 | 170 | 802 | 0.21 | 0.9 | 28 | 96 | 0.29 | 1.3 |
| AES-128-GCM open | 1,024 | 3,151 | 751 | 1,307 | 0.57 | | 172 | 218 | 0.79 | |
| AES-128-GCM open | 16,384 | 43.5k | 10,643 | 9,337 | 1.14 | 9.1 | 2,334 | 2,135 | 1.09 | 7.1 |
| ChaCha20-Poly1305 seal | 64 | 14.6k | 2,434 | 402 | 6.05 | 7.0 | 616 | 237 | 2.60 | 2.7 |
| ChaCha20-Poly1305 seal | 1,024 | 119k | 20,171 | 1,756 | 11.5 | 11.5 | 5,111 | 673 | 7.59 | 8.0 |
| ChaCha20-Poly1305 seal | 16,384 | 1.79M | 292.3k | 27.5k | 10.6 | 11.2 | 76,904 | 7,911 | 9.72 | 10.3 |
| ChaCha20-Poly1305 open | 16,384 | 1.79M | 293.4k | 27.5k | 10.7 | 10.8 | 78,125 | 7,911 | 9.88 | 10.3 |

- **The asymmetric operations moved by 4 to 18 times** and are now 1.3 to 5.5 times OpenSSL. The instruction counts moved with
  them (X25519 12.19M to 0.69M, a signature 25.25M to 1.63M, a verification 25.25M to 3.49M) and the code still runs at an
  IPC of 4.7 to 5.2: still too many instructions, not stalls (§3.1's reading holds).
- **AES-128-GCM is below OpenSSL's cost up to 1 KiB** (OpenSSL's `speed -aead` pays its EVP calls per record, which is most of
  a 64-byte record) and 1.08 to 1.15 times it at 16 KiB.
- Not moved: SHA-2, HMAC, HKDF and ChaCha20-Poly1305, whose instruction counts are §3.1's to within 1%.

### 10.3 Handshakes: full and resumed, server and client

x86-64: cycles and instructions (user and kernel) of the process measured, the least of three or four rounds, and the CPU a
handshake (user and system, `/proc` or `wait4`) over the rounds; arm64: CPU a handshake, the least and the range.
`scripts/tls_handshake_cost.py <bin> --seconds 8 --rounds 3 [--sudo-perf]`.

| | x86-64: cancho | OpenSSL | **ratio (cycles)** | §3 | arm64: cancho | OpenSSL | **ratio (CPU)** | §3 |
|---|---|---|---|---|---|---|---|---|
| **server, full** | 2.834M cycles, 9.67M instructions; 0.79 to 0.92 ms | `s_server` 0.990M, 1.78M; 0.28 to 0.34 ms | **2.86** (5.44 in instructions) | 15.1 | 0.404 ms (to 0.427) | `s_server` 0.191 (to 0.195) | **2.12** | 12 to 15 |
| server, full, against the lean `oserv` | | `oserv` 0.932M, 1.76M | 3.04 | about 17 | | `oserv` 0.141 (to 0.144) | 2.87 | |
| **server, resumed** | 2.004M, 5.25M; 0.57 to 0.72 ms | `s_server` 0.858M, 1.73M | **2.34** | | 0.297 ms (to 0.312) | 0.137 (to 0.142) | **2.17** | |
| **client, full** (chain and name verified) | 4.513M, 13.9M; 1.25 to 1.56 ms | `tconn` 1.628M, 3.36M | **2.77** | 11.4 | 0.634 ms (to 0.699) | `tconn` 0.280 (to 0.285) | **2.26** | 11.6 |
| **client, resumed** | 1.957M, 5.0M | `tconn` 0.826M, 1.65M | **2.37** | | 0.261 ms (to 0.325; a difference of two runs) | 0.123 (to 0.129) | **2.12** | |

- **The full server handshake is 5.8 times cheaper in cycles than §3.3's** (16.5M to 2.83M) and 8.0 times in instructions
  (77.8M to 9.67M); on arm64 7.6 times in CPU (3.08 to 0.404 ms). The client's: 4.3 times in cycles (19.4M to 4.51M).
- **A resumed handshake is 0.71 of a full one on the server** (x86-64; 0.74 on arm64) and 0.43 on the client, against 0.87 and
  0.51 for OpenSSL. #394 measured 0.35 to 0.53 before #393 and #397 made the full handshake's signature and ladders cheap: what a
  resumed handshake saves is now small, and what it keeps (the allocator, the key schedule, the tickets) is most of it (§10.4).
- On arm64 the OpenSSL server is 1.35 times dearer than `oserv` (0.191 against 0.141 ms) where on x86-64 it is 1.06 times:
  `s_server`'s own work costs more in the VM. T1 is stated against `s_server`; both are given.
- The x86-64 cycles moved by 0.3% between rounds (2.834 to 2.841M for the full handshake); the CPU times by 17%. The
  arm64 client's resumed row is the difference of two runs' `wait4` times and moved by 25%.

**The allocator experiment, repeated** (A1, §3.5): the same rows with
`GLIBC_TUNABLES=glibc.malloc.trim_threshold=33554432:glibc.malloc.top_pad=1048576`, alternating with the rows above. The
OpenSSL rows, which run under the same setting, are the control: they did not move (x86-64: `s_server` 0.990M against
0.992 to 0.994M, `tconn` 1.628M against 1.620 to 1.634M).

| | x86-64: default → no trim | **change** | ratio to OpenSSL, no trim | arm64: default → no trim | **change** | ratio, no trim |
|---|---|---|---|---|---|---|
| server, full | 2.834M → 1.860M cycles (8.12M instructions) | **−34.4%** | **1.87** (`oserv` 1.99) | 0.404 → 0.294 ms | **−27.2%** | 1.50 (`oserv` 2.03) |
| server, resumed | 2.004M → 0.904M | **−54.9%** | **1.05 to 1.06** | 0.297 → 0.150 ms | **−49.5%** | 1.06 |
| client, full | 4.513M → 2.694M | **−40.3%** | 1.65 to 1.66 | 0.634 → 0.431 ms | −32.0% | 1.56 |
| client, resumed | 1.957M → 0.948M | −51.6% | 1.14 | 0.261 → 0.159 ms | −39% | 1.30 |

This is the experiment, not the fix (§3.5 says why); A1 is the fix (§10.8).

### 10.4 Where the time goes now

`perf record -F 1999 --call-graph dwarf,32768` for 6 s, sorted by `tls_perf.py profile --memory`. x86-64: 9,462 samples of
cycles, applied to the 2.834M cycles of §10.3; arm64: 5,931 samples of `cpu-clock`, applied to 404 µs.

**A full server handshake**

| category | x86-64: share | M cycles | arm64: share | µs | §3.3 (x86-64) |
|---|---|---|---|---|---|
| **memory: `malloc`, `free`, `brk`** | **31.6%** | 0.90 | **28.3%** | 114 | 5.7% (0.94M) |
| ECDSA verify: the server's check before sending | **24.4%** | 0.69 | **25.6%** | 103 | 28.5% (4.71M) |
| ECDSA sign | 11.2% | 0.32 | 13.9% | 56 | 29.0% (4.78M) |
| X25519: the key pair and the shared secret | 10.8% | 0.31 | 12.2% | 49 | 31.8% (5.25M) |
| HKDF and HMAC (key schedule, Finished) | 6.2% | 0.18 | 6.5% | 26 | 1.0% |
| engine and event loop | 5.3% | 0.15 | 4.0% | 16 | 1.6% |
| system calls (sockets, `epoll`) | 4.2% | 0.12 | 5.3% | 21 | 1.2% |
| SHA-256/384 (transcript, digests) | 3.2% | 0.09 | 2.3% | 9 | 0.7% |
| page faults | 1.5% | 0.04 | 0.4% | 2 | 0.3% |
| record AEAD and traffic-key setup | 1.4% | 0.04 | 1.5% | 6 | 0.3% |

- **Each primitive still costs inside the handshake what it costs alone**: on x86-64 the check 0.69M against a verification's
  0.687M, the signature 0.32M against 0.327M, the two ladders 0.31M against 2 × 0.149M; on arm64 103 µs against 104, 56 against
  51, 49 against 2 × 25.
- **Memory is the same cost as in §3.3, now six times the share.** 0.90M cycles of `malloc`, `free` and `brk` against §3.3's
  0.94M: nothing changed in how a `region` gets its chunk. Of the 31.6%, 26.4 points are inside the `brk` system call and 20.2
  under glibc's `systrim`, giving the top of the heap back (arm64: 24.2 and 21.4). A 5-second `perf trace` of the server under
  `s_time -new` counted 424,593 `brk` calls for 3,804 handshakes: **112 a handshake**
  ([raw](assets/tls-performance/x86-64/syscalls-server-full.txt)). The functions that open the regions are the key schedule's:
  `hmac.init` 8.1 points, `hkdf.expand` 5.2, `hkdf.expand_label` 4.7, `aes.sub_word` 2.1 (x86-64).
- **The rest grew from 5% to 22%** (HKDF and HMAC, the engine, the system calls, SHA-2, faults, records): 0.62M cycles, against
  §3.3's 0.84M in the same categories. §4.5 said these become worth doing after the big levers; they now are.
- **The check is a quarter of the handshake** (24.4% and 25.6%): §7 Q1 projected about 25% after every lever. It is now twice
  the signature (0.69M against 0.32M), because the signature has the comb and the check does not (B2).

**A resumed server handshake** (x86-64 11,364 samples, arm64 9,174): memory **52.5%** (arm64 48.5%; 43.3 points in `brk`),
X25519 15.5% (16.6%), HKDF and HMAC 13.6% (15.1%), system calls 6.3% (8.0%), engine 4.0% (5.3%), record and traffic keys 2.9%
(3.6%), SHA-2 2.5% (2.4%), page faults 2.3% (0.4%). Making and sealing the two tickets is 15.8% inclusive (12.8% on arm64),
half of it their regions. With glibc told not to trim, the resumed handshake is 0.904M cycles: two ladders (0.30M), and 0.60M of
key schedule, tickets, records and sockets.

**A full client handshake** (x86-64 10,184 samples, `tls_many` 200 at a time in a loop; arm64 6,633, of which perf lost 21% to
its buffer): memory 35.2% (23.7%), the chain's verification 15.4% (14.9%), `CertificateVerify` 15.1% (15.1%), page faults 9.0%
(10.0%; the first touch of 200 slots, §3.4), X25519 6.6% (9.6%), system calls 5.8% (10.9%), engine 5.0% (4.2%), HKDF and HMAC 3.9%
(4.2%), SHA-2 1.6% (1.8%), record 1.1% (1.2%), certificate parsing 1.0% (0.9%). The x86-64 regions are opened most by
`x509.well_formed` (8.9 points).

### 10.5 A request on a kept connection

`scripts/https_hello_test.py --cost 5` with `--kload --tload --plain --oserv`, two passes, each followed by the same with glibc
told not to trim; server CPU a request in microseconds:

| | x86-64: pass 1 | pass 2 | arm64: pass 1 | pass 2 |
|---|---|---|---|---|
| `examples/api`, plain | 6.17 | 5.79 | 2.26 | 2.22 |
| `oserv`, plain | 6.01 | 5.30 | 2.31 | 2.30 |
| **`https_hello`, TLS** | **9.20** | **9.81** | **4.49** | **4.49** |
| **`oserv`, TLS (OpenSSL)** | 8.30 | 8.66 | 3.89 | 3.98 |
| **ratio** | **1.11** | **1.13** | **1.15** | **1.13** |
| `https_hello`, no trim | 9.13 | 11.18 | 4.52 | 4.38 |
| `oserv`, TLS, the same runs | 8.10 | 11.12 | 3.92 | 3.79 |

- **A request costs 1.11 to 1.15 times OpenSSL's server, on both machines; §3.5 measured 2.3 (x86-64) and 1.3 (arm64).** Telling
  glibc not to trim changes nothing now (1.13 and 1.01 times on x86-64, 1.15 and 1.16 on arm64): a request opens no region that
  reaches the kernel. A 5-second `perf trace` of the loaded server counted no `brk` at all on either machine (x86-64: 146,988
  `sendto`, 439,590 `recvfrom`, 9,195 `epoll_wait`).
- Where it goes (x86-64 11,959 samples, arm64 11,445): system calls 53.0% (50.3%), the engine and the loop 42.6% (46.0%), the
  record AEAD 1.5% (2.1%), memory 1.0% (0.8%). Inside the engine, **`tls.lease` and `tls.settle`, which copy a connection's
  4.5 KiB core into a work area and back out and clear the area (#395), are 23.6% of a request inclusive (arm64 26.6%)**:
  `tls.core_out` 9.1%, `tls.lease` 8.8%, `tls.clear_area` 5.0%. #395 measured this as 16% more CPU a request on arm64.
- **The record layer around the cipher** (`tests/programs/record_cost.cho` less `gcm_cost.cho 16 hw`, best of two runs, ns a
  record): x86-64, seal 80 against 44 at 64 bytes (the layer 45% of the record), 252 against 188 at 1 KiB (25%), 2,838 against
  2,664 at 16 KiB (6%), open within a few ns of the cipher at 16 KiB; arm64 Linux, 69 against 38 (45%), 225 against 172 (24%),
  2,776 against 2,479 (11%). On Linux the layer's `region` is cheap (a 64 KiB `malloc` that does not reach the kernel); on
  macOS it is not (`gcm-wide.md` §8.2: 592 ns against 37 at 64 bytes on the M4).

### 10.6 The levers against §4.3's projection

| lever | projected (§4.1, §4.3) | built | measured as built (x86-64 server handshake unless named) | against the projection |
|---|---|---|---|---|
| **A1** the allocator | 5.7% of the handshake (0.94M cycles), 9.9% of the client's, 48 to 60% of a kept request | **no** | the experiment now removes **34.4%** (0.97M cycles), 40.3% of the client's, 54.9% of a resumed one, and nothing of a kept request | not realised. The cycles it removes are the projected ones (0.97M against 0.94M); the share is six times larger because everything around it shrank. The kept request no longer needs it |
| **B1** comb for k·G | the signature ÷ 3.1 (4.78M to 1.54M) | #397, with B5's field | 4.78M → **0.32M**, ÷ 15.1 (alone 4.66M → 0.327M, instructions ÷ 15.5) | **4.9 times the projected gain**, with the field in it |
| **B2** table per identity for the check | the check ÷ 2.4; ÷ 3.6 with B5 | **no**; #397 made verification wNAF on the new field | the check 4.71M → **0.69M**, ÷ 6.8 | 1.9 times the projection of B2 with B5, without B2 |
| **B3, B4** client verifications | the chain ÷ 2.4 (B3), `CertificateVerify` ÷ 1.4 (B4) | B4 as #397's wNAF; B3 no | each 5.3M → 0.69M, ÷ 7.7 | above both |
| **C1** X25519 on 25.5-bit limbs | ÷ 2.36 (2.5 times on 96%) | no, superseded by C2 | | |
| **C2** wide multiply | ÷ 3.6 on the field multiplications (4 times on 96%); "3 times needs about 5 times" | #393, **X25519 only** | the ladders 5.25M → **0.31M**, ÷ 17.1 (alone 2.34M → 0.149M, instructions ÷ 17.6) | **4.7 times the projected gain**; 51-bit limbs remove the 16-bit representation's 256-product schoolbook, which the projection's 4 times did not count |
| **B5** NIST-prime reduction | 1.7 times on 79% of the ECDSA operations | #397: a 28-bit field using the prime's form (not 32-bit limbs) | in B1's and B2's rows: the field multiplication 3.8 to 4.0 times faster on this machine (`p256-fast.md` §9) | above |
| **D1** tickets | a resumed handshake 42.5% of a full one (7.0M); 36.8% with A1 | #394 | **2.00M**: 0.71 of the as-built full handshake; 0.90M and 0.49 of it with A1's experiment | 3.5 times cheaper in cycles than projected, but a larger share: the projection kept X25519 at 5.25M |
| **D2** multi-block AES-GCM | 2.5 to 3.5 times at 16 KiB, nothing at 64 bytes | #392 | 16 KiB 102k → **10.7k** cycles (9.5 times), 1 KiB 6.7k → 0.75k (9.0), 64 bytes 0.8k → 0.17k (4.7) | about 3 times the projection |
| **E1** memory per connection | not projected | #395 | 119 to 17 KiB idle (`tls-memory.md`); its CPU is 24% of a kept request (§10.5) | |
| A2, C3, D3, D4 | 0.9, 3.2, under 1, 0.3 points | no | | |

**The handshakes against the projection:**

| | §4.3's projection (arithmetic) | as built (measured) | as built with glibc not trimming (measured) |
|---|---|---|---|
| server, full, x86-64 | 5.4M cycles, **4.9 times** `s_server` (A1, B1, B2, C1, B5) | 2.83M, **2.86 times** (B1, B5, C2; no A1, B2) | 1.86M, **1.87 times** |
| client, full, x86-64 | 7.8M, **4.6 times** `tconn` | 4.51M, **2.77 times** | 2.69M, **1.65 times** |
| server, resumed, x86-64 | 7.0M (6.1M with A1) | 2.00M, 2.34 times OpenSSL's resumed | 0.90M, 1.05 times |

The projection was conservative on every operation and right about the allocator's cycles. It assumed 2.5 to 4 times on
X25519's field multiplications and 1.7 on P-256's. X25519 became 17 times cheaper in the handshake: 51-bit limbs remove the
16-bit representation's 256-product schoolbook and its bounds-checked loads and stores (`wide-multiply.md` §10.5). P-256's
field multiplication became 3.8 to 4.0 times faster on this machine (`p256-fast.md` §9), and with the comb that made the
signature 15 times cheaper, and with wNAF the check 6.8 times. It was wrong about the order: it put A1 first for its gain over
effort, and the changes that came first, and gave most, were the fields and the comb.

### 10.7 The targets of §1.1, as built

| target | x86-64 (cycles) | arm64 (CPU time) | met? |
|---|---|---|---|
| **T1** server, full, at most 5 times `s_server` (stretch 3) | **2.86** (`oserv` 3.04) | **2.12** (`oserv` 2.87) | **met, and the stretch** (against `oserv` on x86-64, not the stretch) |
| **T2** client, full, at most 5 times a verifying OpenSSL client (stretch 3) | **2.77** | **2.26** | **met, and the stretch** |
| **T3** a kept request, at most 1.2 times `oserv` (stretch parity) | **1.11, 1.13** | **1.15, 1.13** | **met**; not parity |
| **T4** AES-128-GCM at most 3 times at 1 and 16 KiB, 1.5 at 64 bytes (stretch 2) | 0.57, 1.15, 0.21 | 0.79, 1.08, 0.29 | **met, and the stretch**. ChaCha20-Poly1305 documented: 6.1 to 11.5 (x86-64), 2.6 to 9.9 (arm64) |
| **T5** resumed: at most the two X25519 operations plus 10 points of a full handshake | bound 0.30M + 0.28M = 0.58M; **measured 2.00M** (0.90M with glibc not trimming, against a bound of 0.48M) | 0.297 ms against a bound of about 0.09 ms | **not met** (*as built with A1, §10.11: 0.884M; restated as at most 1.2 times OpenSSL's resumed handshake (§7 Q9), met by the server: 1.04 on x86-64, 1.12 on arm64; not by the client: 1.21, 1.33*) |

T5 is not met, and the levers of §10.8 do not meet it either (the arithmetic in §10.8 leaves a resumed handshake at about 0.69M
cycles). It was written when the rest of a handshake was 5% of it; it is now 22% of a full one and most of a resumed one, and
the bound moves down whenever the full handshake gets cheaper. Against OpenSSL's resumed handshake the as-built one is 2.34
times (x86-64) and 2.17 (arm64), and **1.05 to 1.06 times with glibc not trimming**: §7 Q9 asks whether T5 should be restated
that way.

### 10.8 The next levers, ranked by measured gain

Gain is the share of the as-built x86-64 handshake (§10.3, §10.4) the lever removes, and of a kept request (§10.5). The issues
are #378's sub-issues; #380 and #381 were reopened for what of their scope is left (B2 and B3, P-384; P-256, `bigmod` and
Poly1305 on the builtins). A1's is
measured (the experiment of §10.3); the others are **arithmetic from measured shares and the stated assumption**. The gains are
of today's handshake and overlap where two act on the same code (P1 and B2 on the check, D3 and D4 on HMAC's compressions); the
sequence below the table applies each to what the ones before it leave.

| # | lever | issue | basis | server full | server resumed | client full | kept request |
|---|---|---|---|---|---|---|---|
| **A1** | **keep freed region chunks per thread** (a small stack of them: the key schedule nests regions three deep, `hmac.init` in `hkdf.expand` in `hkdf.expand_label`), so `free` never trims and `malloc` never grows the heap. Compiler change, both backends | #375 | **measured**: glibc told not to trim, alternating (§10.3); *built (§10.11): 34.6% (arm64 23.8), 55.8% (45.6), 40.5% (38.0)* | **34.4%** (arm64 27.2) | **54.9%** (49.5) | **40.3%** (32.0) | 0 (no `brk` now) |
| **P1** | **P-256's field on `mul_wide`**: four 64-bit limbs in place of ten of 28 bits (`wide-multiply.md` §11) | #381 | the multiplication kernels (`p256_kernels.mul_p`, `sqr_p`, `sqr_n`) are 24.7% of the server's handshake and 22.2% of the client's by the profile (self time; all kernels 33.0%); assumed **2.5 times** on them, the low end of `wide-multiply.md` §11's 3 to 3.5 for `bigmod`; *built (§10.15): five 56-bit limbs inside the kernels, 1.4 to 1.6 times on them; the full handshake 19.9% cheaper on the server (0.37M cycles; this row projected 0.42M), 20.3% on the client* | 14.8 points | 0 | 13.3 | 0 |
| **B2** | **a fixed-base table for the server's own key** (the check as two combs) | #380 | the check is 24.4%; two combs are about 1.44 times cheaper than a wNAF verification: 2 × `public_key` less one inversion mod p plus one mod n, against `verify_raw`, from `p256-fast.md` §9's M4 parts (34.6, 5.8, 10.0 against 105.8 µs); *built (§10.13) as a split-wNAF table, not two combs: the check 2.03 times cheaper, the full server handshake 24% fewer instructions, the client's 18% (B3)* | 7.5 points | 0 | (B3, per trust anchor: 4.7 on the chain's 15.4%) | 0 |
| **D3** | **a SHA-256 builtin** (SHA-NI, ARMv8 SHA-2) | #445 | `crypto.compress` is 7.3% of the server's handshake, 11.6% of a resumed one, 3.8% of the client's (self time); assumed OpenSSL's speed a block (9.4 times on x86-64 at 16 KiB, 11.3 on arm64); *built with D4 (§10.14): SHA-256 at 0.78 to 1.00 of OpenSSL's cycles, the two together 11.5% of a full server handshake and 27.6% of a resumed one, 90% and 88% of the cycles projected here* | 6.5 points | 10.3 | 3.4 | 0 |
| **D4** | **HMAC with the key pads kept** (HKDF reuses a keyed state); *built (§10.14)* | #446 | `hmac.init`'s two compressions, outside its regions: at most 9.9 − 8.1 = 1.8% of the server's handshake, 16.5 − 12.8 = 3.7% of a resumed one, 1.8% of the client's | at most 1.8 | 3.7 | 1.8 | 0 |
| **L1** | **#395's work areas without the copy a call**: keep a busy connection's lease across calls, or copy only what the call changed | #447 | `tls.lease` + `tls.settle` are 23.6% of a kept request inclusive (arm64 26.6%); the measured upper bound | 0 | 0 | 0 | **up to 23.6%** (26.6) |
| X1 | X25519 to OpenSSL's cost; one inversion for the two ladders (C3) | | 10.8% of the handshake at 1.40 times OpenSSL: at most 3.1 points; the two inversions are 1.4% (C3: 0.7) | at most 3.1 | 4.4 | 1.9 | 0 |
| **R1** | **an AEAD that seals in place**, so the record layer stops copying | #448 | the layer around the cipher (§10.5): 36 of 80 ns at 64 bytes, 174 of 2,838 at 16 KiB (x86-64); `tls_record.*` is 1.3% of a kept request inclusive | under 1 | under 1 | under 1 | under 1.3%; **45% of a 64-byte record's seal, 6 to 11% of a 16 KiB one** on Linux; more on macOS |
| M1 | `std.bigmod` on `mul_wide` (P-384, RSA) | #381 | not in this handshake; 3 to 3.5 times on `mont_mul` by `wide-multiply.md` §11's arithmetic; *built (§10.15): RSA-2048 verification 6.8 times fewer cycles, P-384 3.9 to 5.1* | 0 | 0 | 0 | 0 |

**Applied in order to the x86-64 server's full handshake (arithmetic, from the shares above):**

| after | handshake | M cycles | against `s_server` |
|---|---|---|---|
| as built | 100% | 2.83 | 2.86 times |
| + A1 (measured; *as built, §10.11: 65.4%, 1.844M, 1.86 times*) | 65.6% | 1.86 | 1.87 |
| + P1 (2.5 times on the kernels: 24.7 × 0.6; *as built, §10.15: 52.4%, 1.483M, 1.48 times*) | 50.8% | 1.44 | 1.45 |
| + B2 (1.44 times on what is left of the check, about 14.2%; *as built, 2.03 times on the check, §10.13*) | 46.5% | 1.32 | 1.33 |
| + D4 (1.8) then D3 (on the 5.5 points of compression left: × 0.89) | 39.8% | 1.13 | **1.14** |

- **A1 first, by twice the next lever, on every kind of handshake**, and for the resumed one it is most of what there is: with A1
  measured and D3 and D4 by arithmetic, a resumed server handshake comes to about 0.90 − 0.07 − 0.14 = 0.69M cycles, about
  0.8 times OpenSSL's resumed one (arithmetic).
- **P1 before B2**: it speeds the signature, the check, ECDH and the client's verifications at once, where B2 speeds the check.
  Together they leave the check at about 10% of the handshake.
- **For a kept connection, L1 is the only lever**; T3 is met without it, and halving the copies would take a request from 1.11 to
  about 1.0 times OpenSSL's (arithmetic): T3's stretch. *As built (§10.12): 0.94 times, the stretch met.*
- **R1 matters to bulk transfer and to macOS**, not to a handshake or a small request on Linux.

### 10.9 Not done, and what this does not show

- **No lever was built here.** A1's gain is the experiment's, not a fix's; the rest is arithmetic. *A1 was built afterwards
  (§10.11), and its gain measured as a fix's.*
- **arm64 has no cycle counts** (the VM has no PMU) and its client profile lost 21% of its samples to `perf`'s buffer.
- **The client's figures include the first touch of 200 slots** (§3.4); a steady-state client was not measured.
- **The arm64 resumed client** is the difference of two `wait4` times over 200 connections and moved by 25% between runs.
- Not measured: P-384, RSA, TLS 1.2, Cranelift, latency, macOS natively (the guard's `darwin-aarch64` entry, §9).
- The first arm64 run's resumed rows (`assets/tls-performance/arm64/hs_arm64.txt` and `hs_arm64_tunables.txt`, `server-resumed`
  and the `tconn` `client-resumed` rows) used a `tconn` built before its `resume` mode, which ignored the argument: they are full
  handshakes and are not used; `hs_arm64_resumed.txt` replaces them.

### 10.10 The guard, completed

The `linux-aarch64` entry of `benches/tls_baseline.json` had kept the counts from before #393 and #397 (the Docker VM's disk was full
when those merged; #392 re-recorded its six AES-GCM rows). `scripts/tls_perf.py check` on arm64 Linux at `0e40f430` failed exactly
those seven rows as cheaper than 1/1.3 ([raw](assets/tls-performance/arm64/check_arm64_before.txt)): X25519 11.09M to 0.53M
instructions, the signature 26.26M to 1.06M, sign-and-check 52.88M to 3.36M, a verification 26.63M to 2.30M, k·G 24.35M to 0.76M,
k·P 24.36M to 3.23M. `tls_perf.py record` re-recorded the entry on arm64 Linux; the other 26 rows came back within 1.01 of what was
recorded. Then, at this change's head, `check` passes **33 of 33 rows on both entries** (x86-64 on `gram`, arm64 in the VM) and
`scripts/tls_perf_mutant.py` fails exactly its four rows at 2.00 on both. `check` now takes 11.5 s on x86-64 and 8.3 s on arm64
(§6 said 40 and 12: the primitives are cheaper to run under valgrind).

### 10.11 A1 as built

[`region-chunk-cache.md`](region-chunk-cache.md) built A1 (#375): each thread keeps the chunks its regions give back, at most 16,
last in first out, cleared when a region ends. Measured against its base (`2cb860b3`), alternating, on the machines of §10.1
(x86-64: load average 1.2 to 3.6; arm64: the VM at 1.6 to 3.4); `scripts/tls_handshake_cost.py --seconds 8 --rounds 2`, the
least of four rounds; raw output in [`assets/region-chunk-cache/`](assets/region-chunk-cache/).

| | x86-64 cycles: before → after | **ratio to OpenSSL** | arm64 CPU: before → after | **ratio** |
|---|---|---|---|---|
| server, full | 2.821M → **1.844M** (−34.6%) | 2.87 → **1.86** `s_server` (`oserv` 1.98) | 0.382 → **0.291 ms** (−23.8%) | 1.93 → **1.50** (`oserv` 2.09) |
| server, resumed | 2.001M → **0.884M** (−55.8%) | 2.33 → **1.04** | 0.270 → **0.147 ms** (−45.6%) | 1.99 → **1.12** |
| client, full | 4.474M → **2.664M** (−40.5%) | 2.74 → **1.64** `tconn` | 0.682 → **0.423 ms** (−38.0%) | 2.44 → **1.51** |
| client, resumed | 1.901M → **0.994M** (−47.7%) | 2.32 → **1.21** | 0.284 → **0.159 ms** (−44.0%) | 2.35 → **1.33** |

- **`brk`**: 112 a full server handshake and 136 a resumed one before; **9 in a whole 5-second run** after, on both machines
  (`strace -c`, `region-chunk-cache.md` §8.1). §1.2's bound of 2 a handshake is met.
- **The fix matches the experiment** of §10.3 (1.860M, 0.904M, 2.694M with glibc told not to trim): it is a little cheaper,
  since it skips `malloc` and `free` as well as the kernel. On arm64 the server's gain is smaller than the experiment's
  (23.8 against 27.2%, 45.6 against 49.5%) and the client's larger (38.0 against 32.0%); the VM moves about 10% between rounds.
- **The targets**: T1 1.86 (x86-64) and 1.50 (arm64), inside its 3-times stretch; T2 1.64 and 1.51; **T5, as §7 Q9 restates it
  (at most 1.2 times OpenSSL's resumed handshake, server and client), is met for the server: 1.04 and 1.12**, and not for the
  client: 1.21 and 1.33 (from 2.32 and 2.35). As first written (the two ladders plus 10 points of
  a full handshake: 0.30M + 0.18M = 0.48M), it is not: 0.884M.
- **Clearing each region's bytes when it ends costs +0.1% of a full handshake's instructions and +0.3% of a resumed one's**,
  inside the cycles' spread (`region-chunk-cache.md` §8.3).
- **The next lever is now P1** (§10.8's order). With A1 measured, a server's full handshake is 1.844M cycles and the
  arithmetic of §10.8's sequence starts from it.

### 10.12 L1 and R1 as built

[`record-in-place.md`](record-in-place.md) built L1 (#447) and R1 (#448). Measured against `3d7e8e9d` (A1 merged) on `gram`
(the server on CPU 6, its load on CPU 7), alternating; raw output in [`assets/record-in-place/`](assets/record-in-place/).

- **L1 was the loop, not the bytes.** The 24% of §10.5 was the 296 words of the core copied one at a time through the engine's
  boxes (the store may alias the boxes, so every word reloaded both and checked both bounds). Block copies took it to 10.3% of a
  request; an established connection that **rests** in its area until another needs it (at most 32 by default,
  `tls.set_resting`) took it to 3.6%.
- **A kept `https_hello` request: 40.0k cycles (72.2k instructions) to 31.4k (45.8k); OpenSSL's `oserv` 33.3k (43.4k): 1.20 to
  0.94 times. T3's stretch, parity, is met.**
- **R1:** a record is sealed where it is sent from and opened straight into the received data, with no region and no byte loop.
  Through both engines with no sockets (`tests/programs/engine_cost.cho`): a 64-byte record 6.6k cycles to 1.15k, 1 KiB 8.4k to
  2.6k, 16 KiB 39.1k to 26.7k, of which the cipher is about 21k. §10.8 put R1 under 1% of a request: on its own it was about 1%
  (33.6k to 33.9k cycles with L1's block copies, 33.1k to 33.6k with R1 added), as projected; for a record it is the 5.7 times
  and 1.5 times above.
- **Memory:** an idle connection is what it was (17.0 KiB at 10,000); resting keeps at most 33 areas more written per engine,
  2.1 MiB after a 16 KiB record on each connection (`tls-memory.md` §12); the high-water mark fell 16 KiB a connection.
- **Not changed:** `/big` (16.0 to 15.7 cycles a byte), which is `packages/http-server`'s and the example's byte loops, 85% of it.


### 10.13 B2 and B3 as built

[`p256-fast.md`](p256-fast.md) §11 built both (#380): a table for a P-256 key that verifies many signatures, kept once per engine
and per key, never per slot. Not two combs, as §10.8 assumed: the 257 width-7 wNAF digits of each scalar are cut into four runs of
65, and a table holds the odd multiples 1 to 63 of Q, 2^65 Q, 2^130 Q and 2^195 Q (128 affine points, **10,305 bytes**), so the
two scalars share one chain of 64 doublings where `p256-fast.md` §7's verification has 256. Two combs would have added 61 times a scalar and
taken 83 KB a key; this adds 32 times and takes an eighth of that. The server keeps one per identity (built by `add_identity` and
`replace_identity`, `tls-server.md` §3.5); the client one per P-256 trust anchor, at most 16 (built by `trust`; the system bundle has
3, 30,939 bytes; `x509-verify.md` §11).

Measured on `gram` (core 2 and its sibling, load average 3.5 to 5.1, other agents' benchmarks on cores 0, 1, 4 and 5), against
`origin/main` at `3d7e8e9d` (A1 included), alternating; raw figures and conditions in `p256-fast.md` §11.4 and §11.5:

| | before | after | change | ratio to OpenSSL, before → after |
|---|---|---|---|---|
| a verification (`tls_prims_bench`, cycles) | 688.1k | **326.4k** with the key's table | −53% (instructions 3.49M → 1.54M) | 2.72 → **1.29** |
| the check after a signature (`sign_checked` less `sign`) | 687k | **339k** | −51% | |
| sign and check (`sign_checked`) | 1,014.5k | **666.5k** | −34% | 2.99 → **1.97** (OpenSSL's sign + verify) |
| **server, full handshake** | 1.923M cycles, 8.092M instructions | **1.467M, 6.127M** | **−23.7%**, −24.3% | `s_server` **1.85 → 1.46**, `oserv` 1.98 → 1.56 |
| **client, full handshake** (under a P-256 CA) | 2.743M, 10.717M | **2.303M, 8.767M** | **−16.0%**, −18.2% | `tconn` **1.62 → 1.41** |

arm64 (the M4's Docker VM, CPU time, the least of four rounds, VM load average 7.2 to 8.2): the server's full handshake 0.292 →
**0.232 ms** (−20.5%; 1.52 → **1.23** times `s_server`), the client's 0.422 → **0.364 ms** (−13.7%; 1.52 → **1.31** times `tconn`).

- **Against §10.8:** B2 was 7.5 points of the pre-A1 handshake, the check 1.44 times cheaper; as built the check is 2.03 times
  cheaper and saves 24% of the instructions of the A1-era handshake (1.965M of 8.092M; the check's own saving is 1.946M). B3 was 4.7
  points; as built 18% of the client's instructions, one verification of its chain (the leaf's CertificateVerify has no table).
- **T1** (at most 5 times `s_server`, stretch 3) is now 1.46 on x86-64, and **T2** 1.41.
- **What B3 does not reach:** the measured client verifies under a private P-256 CA. Of the 31 signatures of `x509-verify.md` §8.2's
  14 real chains, 22 are RSA, 7 P-384 and 2 P-256, and none ends at a P-256 root; for public sites the next cost is P-384
  verification, 9 to 16 times OpenSSL's cost (`p256-fast.md` §12, which decides it is worth building, on #381's field).
- **The guard** has two new rows on both entries, `ecdsa_p256_verify_tabled` and `ecdsa_p256_sign_checked_tabled`, recorded on each
  entry's own machine; the other 33 were not re-recorded and pass, 35 of 35 on both.
- **The next levers**, by §10.8's order: P1 (P-256's field on `mul_wide`, #381), now on a check half the size it was; D3 and D4
  (#445, #446).

### 10.14 D3 and D4 as built

[`sha256-builtin.md`](sha256-builtin.md) built D3 (#445: `hw_sha256`, `sha256_blocks` and `sha256_block_words`, SHA-256's
compression on SHA-NI and ARMv8 SHA-2, `std.crypto` on them) and D4 (#446, [`hkdf.md`](hkdf.md) §9: a key's two padded blocks
hashed once, HKDF and the TLS key schedule's traffic keys and handshake secrets on prepared keys, and `hmac.final` with no
region). Measured against `3d7e8e9d` (A1 built), alternating, on the machines of §10.1, x86-64 pinned to CPUs 4 and 5; raw
output in [`assets/sha256-builtin/`](assets/sha256-builtin/). `sha256-builtin.md` §8.5 has every row; the ones that move a
handshake:

| | x86-64: before → after | **ratio to OpenSSL** | arm64: before → after | **ratio** |
|---|---|---|---|---|
| SHA-256, 64 / 1,024 / 16,384 bytes (cycles; arm64 ns) | 2,565 / 20,829 / 332,622 → **440 / 2,406 / 33,857** | 4.56 / 8.21 / 9.78 → **0.78 / 0.95 / 1.00** | 557 / 4,425 / 66,284 → **141 / 455 / 6,576** | 5.78 / 9.21 / 12.0 → **1.46 / 0.95 / 1.19** |
| HKDF-Expand-Label (cycles; arm64 ns) | 5,641 → **1,314**; **677** prepared | | 1,205 → **232**; **155** prepared | |
| server, full (cycles; arm64 CPU) | 1.848M → **1.636M** (−11.5%) | 1.86 → **1.65** `s_server` | 0.297 → **0.262 ms** (−11.8%) | 1.59 → **1.40** |
| server, resumed | 0.889M → **0.644M** (−27.6%) | 1.04 → **0.76** | 0.152 → **0.111 ms** (−27.0%) | 1.10 → **0.80** |
| client, full | 2.660M → **2.540M** (−4.5%) | 1.64 → **1.57** `tconn` | 0.423 → **0.389 ms** (−8.0%) | 1.49 → **1.37** |
| client, resumed | 0.954M → **0.827M** (−13.3%) | 1.15 → **0.99** | 0.172 → **0.151 ms** (−12.2%) | 1.40 → **1.23** |

- **§10.8's arithmetic held in cycles**: 0.212M of the 0.235M projected for a full server handshake and 0.245M of the 0.280M
  for a resumed one. The shares are larger than projected because A1 came first.
- **A resumed server handshake is now cheaper than OpenSSL's** (0.76 of its cycles, 0.80 of its time on arm64), and T5 as §7 Q9
  restated it (at most 1.2 times OpenSSL's resumed handshake) is now met by the client too: 0.99 on x86-64, 1.23 on arm64
  (so not on arm64 by 0.03).
- **The guard cannot see the x86-64 builtin**: Valgrind does not have SHA-NI and hides it, so the `linux-x86_64` entry counts
  the software path (`sha256-builtin.md` §2, §8.3); the `linux-aarch64` entry counts the instructions, and was re-recorded.
  A new row, `hkdf_expand_label_prepared_32`, is in both.
- **The next lever** of §10.8 is P1 (the field under the check and the signature, untouched here); L1 and R1 are §10.12's, B2 and B3
  §10.13's. SHA-384
  (a TLS_AES_256_GCM_SHA384 handshake's hash, and the second transcript every handshake keeps until the suite is known) stays in
  software at 1.4 to 3 times OpenSSL.

### 10.15 P1 and M1 as built, and Poly1305 (#381)

[`p256-fast.md`](p256-fast.md) §13 and [`wide-multiply.md`](wide-multiply.md) §12 built P1, M1 and Poly1305 on the wide-multiply
builtins. P1 is not what §10.8's row describes: four 64-bit limbs were measured and not chosen (they change the representation
#380's files read and lose §4.4 of `p256-fast.md`'s lazy reduction); five 56-bit limbs inside the kernels were, and the
additions and subtractions became `wrapping_*`. Measured against `main` at `3d7e8e9d` (A1 built, before §10.12's L1 and R1 merged), alternating, on the machines of
§10.1; raw output in [`assets/wide-multiply/381/`](assets/wide-multiply/381/).

**Handshakes** (`scripts/tls_handshake_cost.py <bin> --seconds 8 --rounds 2 --cpu 0 --peer-cpu 1 server-full client-full`, the two
builds alternating twice, the least of four rounds; x86-64 `gram` with `--sudo-perf`, user and kernel cycles, load average 3.1 to
3.6 from other agents' jobs; arm64 the Docker VM of §10.1, CPU time, load 3.0 to 6.7):

| | x86-64 cycles: `main` → branch | **change** | **ratio to OpenSSL** | instructions | arm64 CPU: `main` → branch | **change** | **ratio** |
|---|---|---|---|---|---|---|---|
| server, full | 1.852M → **1.483M** | **−19.9%** | 1.85 → **1.48** `s_server` (`oserv` 1.96 → 1.57) | 8.09M → 6.11M | 0.293 → **0.251 ms** | **−14.3%** | 1.57 → **1.34** (`oserv` 2.12 → 1.79) |
| client, full | 2.673M → **2.130M** | **−20.3%** | 1.63 → **1.31** `tconn` | 10.5 to 10.9M → 7.9M | 0.423 → **0.358 ms** | **−15.4%** | 1.53 → **1.28** |

The resumed rows were not run: a resumed handshake does no P-256 operation and no ChaCha20-Poly1305 (the cipher is AES-128-GCM).
§10.8 projected P1 at 14.8 points of the pre-A1 server handshake, 0.42M cycles; it took 0.37M (13 points of it, 19.9% of the
A1-built one), with the kernels 1.4 to 1.6 times faster rather than the 2.5 assumed: the additions, the subtractions and the
inversions, which the projection left out, paid the rest. The client's projected 13.3 points (0.60M) came to 0.54M. T1 is now 1.48
times `s_server` on x86-64 and 1.34 on arm64; T2 1.31 and 1.28.

**Primitives** (x86-64 cycles on `gram`, the least of six runs; [`p256-fast.md`](p256-fast.md) §13.4, [`wide-multiply.md`](wide-multiply.md)
§12): P-256 ECDH 980k → 568k cycles (5.54 → **3.21** times OpenSSL), the signature 327k → 225k (4.61 → **3.17**), a verification
682k → 417k (2.92 → **1.78**); RSA-2048 verification 1.05M → 0.15M (17.8 → **2.61**), P-384 ECDH 11.9M → 3.1M (12.1 → **3.10**);
Poly1305 over 16 KiB 89.6k → 39.0k. ChaCha20-Poly1305 is otherwise ChaCha20's (its rows below moved 7 to 9% in instructions).

**The guard** (§6). After the rebase onto #465 (35 rows, its two tabled rows included), `scripts/tls_perf.py check` failed
exactly the P-256 rows as cheaper than 1/1.3: on x86-64 all seven (0.58 to 0.66), on arm64 five (0.73 to 0.76; the signature
0.81 and the tabled check 0.78, inside); ChaCha20-Poly1305 0.91 to 0.95, inside. Both entries were re-recorded on their own
machines (`record` on `gram`, and in the arm64 VM), keeping their descriptions; `check` then passes 35 of 35 on each and
`scripts/tls_perf_mutant.py` fails exactly its four rows on each (`assets/wide-multiply/381/tls_perf_*.txt`). The handshakes above
were measured before #463 and #465 merged.
