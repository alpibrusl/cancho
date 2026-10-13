# Region chunks kept per thread (A1, #375)

> **Status: design, with the measurements it rests on.** Lever A1 of `tls-performance.md` §10.8 and the per-thread chunk
> cache #375 asks for, one mechanism. Today a `region` is a `malloc` of 64 KiB on the way in and a `free` on the way out
> (`linearity-and-effects.md` §6), and glibc gives the top of its heap back to the kernel and takes it again for nearly
> every one: **112 `brk` calls a full TLS server handshake and 136 a resumed one** (§1). The design: each thread keeps the
> chunks its regions give back, **at most 16 (1 MiB)**, in a last-in-first-out stack; a region takes the last one given
> back; the bytes a region used are **cleared when it ends**, before the chunk is kept or freed; a thread's chunks are freed
> when it ends. Same chunk size, same trap, same programs; both backends, the same runtime. §8 is what was built and
> measured.

Related: [`tls-performance.md`](tls-performance.md) §3.5, §7 Q3, §10.3, §10.4, §10.8; [`threads.md`](threads.md) §7 (the
macOS half, #375); [`linearity-and-effects.md`](linearity-and-effects.md) §6; [`zeroed-slices.md`](zeroed-slices.md) §1;
[`chacha20.md`](chacha20.md) §3.2 and [`hkdf.md`](hkdf.md) §3 (clearing secrets, best effort); [`tls-memory.md`](tls-memory.md).

---

## 1. Where the `brk` calls come from today

Measured on `gram` (i7-1260P, Linux 7.0.0-38, glibc 2.43; the machine of `tls-performance.md` §10.1, load average 0.1 to 0.2,
CPUs 0 to 3) at `2cb860b3` (`origin/main`), `examples/tls_echo` and `tests/programs/tls_many.cho` built with the LLVM backend
as CI builds them. System calls are `strace -f -c -e trace=brk,mmap,munmap,madvise` over the whole run; chunks are an
`LD_PRELOAD` shim counting `malloc(65536)` and the `free` of each, and the most live on one thread at once.

| | handshakes | `brk` | `mmap` / `munmap` | 64 KiB `malloc`s | most chunks live at once |
|---|---|---|---|---|---|
| server, full (`openssl s_time -new`, 5 s) | 3,001 | **336,165: 112.0 a handshake** | 17 / 10 in all | 227 a handshake | **10** |
| server, resumed (`tconn ... resume`, 5 s) | 3,180 | **432,665: 136.1 a handshake** | 17 / 10 in all | 275 a handshake | 10 |
| client, `tls_many` 200 connections | 200 | 39,022 (195 a handshake) | 16 / 9 in all | 371 a handshake | **15** |

**The path.** `region_stmt` in both backends (`cancho-codegen-llvm/src/body/memory.rs`, `cancho-codegen/src/body/memory.rs`)
calls `malloc(ARENA_CHUNK)` and, at the end of the block or on a `return` out of it, `free`. `ARENA_CHUNK` is 64 KiB, below
glibc's `M_MMAP_THRESHOLD` (128 KiB), so the chunk comes from the `brk` heap, and in a program that is not allocating much
else it sits at the top of it. Its `free` merges it into the top chunk; when the top passes `M_TRIM_THRESHOLD` (128 KiB by
default) `free` calls `systrim`, which gives the excess back with `brk`, and the next region's `malloc` finds the top too small
and grows it with `brk` again, faulting the pages back in. The profile says so: of the 31.6% that memory is of a full server
handshake, 26.4 points are inside `brk` and 20.2 under `systrim` (`tls-performance.md` §10.4). `mmap` and `munmap` are not on
the path at all (17 and 10 calls in a whole run: start-up and the libraries).

**What it costs.** The experiment of `tls-performance.md` §10.3 (`GLIBC_TUNABLES=glibc.malloc.trim_threshold=33554432:
glibc.malloc.top_pad=1048576`, alternating with the default, OpenSSL's rows unmoved) took the x86-64 server's full handshake
from 2.834M to 1.860M cycles (−34.4%), a resumed one from 2.004M to 0.904M (−54.9%), the client's from 4.513M to 2.694M
(−40.3%); arm64 −27.2%, −49.5%, −32.0%.

**On macOS it is a different cost with the same cause.** libmalloc serves a request above 32 KiB from the kernel's VM
allocator, not from a per-thread magazine (`threads.md` §7). Measured on this M4 Max today (`scripts/region_scaling.c`, mode 1,
100,000 pairs a thread, load average 2 to 3): **0.07 s on one thread (0.7 µs a `malloc`/`free` pair), 1.24 s on eight (sys
6.6 s)**, and 0.00 s at 32,768 bytes. Every region pays it, and threads queue on it.

**Nesting is why one chunk is not enough.** The key schedule opens regions inside regions (`hmac.init` inside `hkdf.expand`
inside `hkdf.expand_label`, `tls-performance.md` §10.4); the server holds **10** chunks at its deepest and the client 15. A
cache that keeps fewer than the depth hands the excess back to `free` every time the stack unwinds, and takes it from `malloc`
again on the way down.

## 2. The options

**(a) A per-thread free list of chunks, kept by the runtime the compiler emits.** `region` takes a chunk from the list, or
`malloc`s one if it is empty; the end of the region puts it back, or `free`s it if the list is full. Bounded by a number of
chunks, so by memory. Portable: it asks libc for nothing but `malloc`, `free` and a thread-specific value, which glibc, musl
and Darwin all have. It removes the allocator from the path entirely, not only the kernel: a region costs a few loads and
stores.

**(b) Tell the allocator not to trim.** `mallopt(M_TRIM_THRESHOLD, ...)` and `M_TOP_PAD` in the emitted `main`, or a
`malloc_trim` policy. Its gain is measured: it *is* §1's experiment (−34 / −55 / −40% on x86-64). Against it:
- **glibc only.** musl's `mallopt` does nothing, and Darwin has none; the compiler targets macOS (`threads.md` §7's cliff is
  libmalloc's large-allocation path, which no `mallopt` reaches) and WASI.
- **It changes the policy for every allocation the program makes**, `box_slice`, `Box`, `std.buffer`, not only the arenas:
  a service that frees a large heap buffer would keep it resident too. The cache keeps only chunks, and only so many.
- **It still pays `malloc` and `free`** (glibc's `_int_malloc` and `_int_free` for a 64 KiB request, which skips the
  per-thread `tcache` above 1,032 bytes); the cache pays neither on a hit.
- A program cannot do it itself: `mallopt` needs `Ffi`, and the TLS stack is built to need none (`tls-performance.md` §4.2).

**(c) A per-thread region stack that reuses the last chunks first.** A thread's regions are strictly nested: a region is a
block, `return` leaves the innermost first, and there is no `break` (`linearity-and-effects.md` §5.2: the outlives relation
is a stack). So the region opened at depth *d* is opened when exactly the regions at depths 0 to *d − 1* are open, and the
chunk given back last is the one a region at that depth used last time. **A list used last-in-first-out is this stack**, with
no depth counter to keep: the two options are one design when (a) pops what it pushed last. LIFO also keeps the warm chunk
(its pages resident, its lines likely cached) for the next region.

**Chosen: (a) used as (c).** A stack of at most `REGION_CACHE_SLOTS` = **16** chunks per thread. 16 because the deepest
measured nesting is 10 (the server) and 15 (the client, its test program's own region included), and a stack shallower than the
depth puts the excess back on the allocator's path every time it unwinds (§1). 16 chunks are 1 MiB of address space a thread
at most; what is resident is the pages the regions touched (§4).

**What happens at the bound.** A chunk given back to a full stack is cleared (§3) and `free`d, as every chunk is today. A
recursion that opens a region at each of 1,000 levels holds 1,000 chunks while it is deep, as today, and keeps 16 when it
returns. Nothing traps or waits.

## 3. The runtime

Four functions, emitted into the program by each backend when a function in it opens a region, and nowhere else:

| | what it does |
|---|---|
| `cancho_region_init()` | called first in `main`, before any cancho code: `pthread_key_create(&key, cancho_region_drop)`. If it fails (`PTHREAD_KEYS_MAX`), the key stays unset and every region behaves as today: `malloc` in, `free` out |
| `cancho_region_take() -> ptr` | the thread's stack (`pthread_getspecific`); if it has a chunk, pop it; otherwise `malloc(ARENA_CHUNK)`. Null still traps at the call site, as today |
| `cancho_region_give(base, end)` | clear `[base, end)`, the bytes the bump pointer handed out; then push the chunk on the thread's stack, making the stack on the thread's first release (`malloc` of 8 + 8 × 16 bytes, `pthread_setspecific`), or `free` it if the stack is full or cannot be made |
| `cancho_region_drop(stack)` | the key's destructor, run by the thread library when a thread that has a stack ends: `free` every chunk, then the stack |

`region_stmt` calls `take` where it called `malloc`; the end of a region, and every `return` out of one (innermost first), calls
`give` with the arena's base and its bump pointer where it called `free`. The chunk size, the bump pointer, the bounds check
and the trap on exhaustion do not change, so `allocating_past_an_arenas_chunk_traps_with_sigill` and every program's
behaviour are the same.

### 3.1 Secrets: what a reused chunk shows

**The language has no uninitialised memory** (`zeroed-slices.md` §1), and an arena is no exception. Both ways to take bytes from
one write them before handing them out: `alloc_slice[a](n, fill)` is `Expr::AllocSlice`, whose `fill` is not optional
(`cancho-ir/src/ir.rs`), lowered as a bump and then a loop that writes `fill` into every element (`fill_slice` in both
backends); `alloc[a](v)` is `Expr::Alloc`, a bump and then a store of `v`'s leaves (`store_leaves`). No other expression bumps.
So **cancho code cannot read a reused chunk's old bytes**: there is no read before the write. A slice handed to C (`&r [byte]`
across `extern fn`) shows C its own bytes, which the program filled; what C reads beyond them is `Ffi`'s unbounded authority,
and today's `malloc` already hands back an untrimmed chunk's old contents in the same way.

**But reuse lengthens how long the old bytes live, so the cache clears them.** Today a chunk at the top of the heap is mostly
given back to the kernel when its region ends (that is the 112 `brk` calls), and a page given back is gone; with the stack it
would stay in the process, holding whatever the region left in it, until the next region at that depth writes over the part it
uses. The best-effort clearing the TLS stack does (`chacha20.md` §3.2, `hkdf.md` §3: the Poly1305 key, HMAC's pads, the last
`T(i)`) clears named buffers, not the hash states and schedules beside them. So `give` **clears `[base, end)`**, every byte
the region handed out, before the chunk is kept or freed. Three properties follow:

- **It is stronger than today, not as strong**: every region's bytes are cleared when it ends, on both backends, whether or not
  the program cleared them; what `chacha20.md` §3.2 calls impossible to promise ("a region's memory is freed without being
  cleared") becomes true of every region the program leaves normally or by `return`. A trap ends the process.
- **The optimiser cannot delete it**, which it could do to a program's own clearing before a `free` (`chacha20.md` §3.2): the
  cleared chunk is then stored into the thread's stack, where any later `take` reads it, so the clearing is not a dead store.
  §8 checks the object code.
- **It costs at most what the region's fill cost**, since the fill already wrote every byte cleared: the clearing is one
  `memset` over the same range. §8 measures it in the handshake.

Not claimed: the chunk's bytes on the path where a thread's stack is full and the chunk is `free`d are cleared first too, but
whatever glibc then does with the freed memory is glibc's; and memory outside regions (`box_slice`, `Box`, the stack) is not
touched by this.

### 3.2 Threads

- **One stack a thread, found through a `pthread` key**, so no lock and nothing shared: `pthread_getspecific` reads the
  calling thread's own slot (glibc: an array in the thread descriptor; Darwin: the TSD area). A spawned thread starts with no
  stack and makes its own on its first release.
- **The key is made in `main` before any cancho code runs**, so before any `spawn` (`threads.md` §2); every thread that runs
  cancho code after that sees it, whoever created the thread.
- **When a thread ends** the thread library runs the key's destructor on its stack (`cancho_region_drop`): its chunks and the
  stack are freed. This holds for any thread `pthread` made, `spawn`'s or not. `main`'s thread is not one of these: when
  `main` returns the process exits and the memory goes with it.
- Native thread-local storage (`thread_local` in LLVM, `tls_value` in Cranelift) was the other way. It would save a call a
  region, but needs a thread-exit hook of its own to free the chunks (which is the key again), and Cranelift's TLS models differ
  by object format. The key gives both backends the same runtime, with the same calls.

### 3.3 Both backends, WASI, the report

- **Identical in behaviour**: the same four functions, the same order of calls, the same bound (`cancho_ir::REGION_CACHE_SLOTS`,
  one constant both read), the same clearing. The LLVM backend writes them as IR text (`region_cache.rs`); Cranelift builds them
  with its function builder (`region_cache.rs` there). `pthread_key_t` is 4 bytes on glibc and 8 on Darwin; the key is kept in
  a zeroed 8-byte global, which holds either, as the key plus one (zero means "no cache").
- **WASI** (`wasm32-wasip1`, LLVM only): there are no threads (`spawn` is refused on that target, `wasm.md`), so the stack is
  one module global and there is no key, `init` or destructor. Same bound, same clearing.
- **The authority report does not change.** These are calls the compiler emits for `region`, like its `malloc` and `free`
  today, not `extern fn` declarations: no effect label, no foreign symbol, `bounded` unchanged. The `pthread` names are declared
  only when the program does not declare them itself, as every libc name is (#388).

## 4. Memory growth

- **The bound is 16 chunks, 1 MiB of address space, a thread** that has opened regions. Resident is less: a chunk's pages are
  resident only where a region touched them, and the clearing touches nothing the region did not.
- **What a thread holds** is the most regions it has had open at once, up to 16: the server above holds 10 chunks (640 KiB of
  address space) whatever its number of connections, since its connections share one thread.
- **Long-running services.** `tls_echo` with 10,000 connections is one thread: at most 1 MiB beside `tls-memory.md`'s
  16.7 KiB a connection (167 MB), so **at most 0.1 KiB a connection at 10,000**; that is the bound its figures must stay within.
  cancho-hooks spawns a thread per worker: at most 1 MiB each. A program that wants the old behaviour has no switch; one region
  per batch rather than per item (`threads.md` §7) is unaffected.

## 5. Gates, stated before the numbers

1. **System calls** (`strace -c`, as §1): a full and a resumed server handshake, and a client's, make **O(1) `brk` calls**:
   at most one a hundred handshakes, from 112 and 136; `mmap`/`munmap` not more than today. A conformance test checks it on
   Linux in CI (§6).
2. **Cycles** on `gram` (`scripts/tls_handshake_cost.py --sudo-perf`, x86-64, CPUs 0 to 3) and CPU time on the M4's arm64 Linux
   VM, before and after alternating, OpenSSL beside each: the server's full handshake **at most 2.0 times `s_server`** (the
   experiment reached 1.87), the resumed one **at most 1.2 times OpenSSL's resumed** (T5 as restated in `tls-performance.md`
   §7 Q9 and #455; the experiment reached 1.05), the client's at most 1.8 times `tconn` (1.65). T1 and T5 written into §10.
3. **macOS**: `region` per unit of work flat across threads (`threads.md` §7's table, the Cranelift row, re-run).
4. **Memory**: `scripts/tls_memory.py` at 10,000 connections on both machines within 0.1 KiB a connection of before (§4).
5. **Secrets**: the clearing is in the optimised object (§3.1), on both backends; the TLS suites unchanged.
6. **The whole gate**: `cargo fmt`, `clippy -D warnings`, `cargo test --workspace` (Linux in Docker and CI; macOS natively),
   `scripts/publish_packages.py --check`, `scripts/history.py --explain`, `scripts/tls_perf.py check` (instruction counts may
   drop; a row beyond the guard is re-recorded on its own machine, `tls-performance.md` §1.3).

## 6. Tests

- **O(1) system calls**: a program that opens three nested regions 20,000 times, built on both backends, run under `strace -f -c`
  on Linux: at most a handful of `brk` calls in all (today about 20,000). Where `strace` is missing it is skipped, except in CI,
  which installs it.
- **Chunks reused**: the same program under an `LD_PRELOAD` shim counting 64 KiB `malloc`s (Linux): at most 3, against 60,000.
- **Threads**: eight spawned threads each run the nested loop and end; counted the same way, each makes its own stack (the
  count is at most 3 a thread, so no thread took another's chunk) and every chunk is freed by the time the threads are joined
  (the shim counts the frees).
- **The bound**: a recursion 40 regions deep, repeated: the shim sees at most 40 − 16 = 24 chunk `malloc`s a repetition after
  the first and never more than 16 kept.
- **Clearing**: the function `give` keeps its `memset` in the object `clang -O2` makes (a unit test on the LLVM backend's
  object); a program that fills a region with a byte, ends it, and opens another reads only its own fill (the language's
  guarantee, checked on both backends).
- **The trap and every existing arena test unchanged.**

## 7. Not in scope

- Growing a region past 64 KiB (`linearity-and-effects.md` §6 leaves it to M3); smaller chunks (they change which programs trap,
  `threads.md` §7).
- `box_slice` and `Box`, which use the heap directly and are freed when the program says.
- Returning cached chunks to the system while a thread lives (a service that once nested 16 deep keeps 1 MiB of address space
  on that thread).
