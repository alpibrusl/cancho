# The guide

`README.md` is a thin pointer. This is where the depth lives: what the
language is, why it's built this way, what exists, and an index of
every design document. History — what shipped when, and what each
slice found, including corrections — is [`ROADMAP.md`](ROADMAP.md).
Neither belongs in the index below, which says what each document
*settles*, not how it got there.

Design lands here **before** the code that implements it, which is the
cheap place for it to be wrong. When it turns out wrong anyway, the
document that made the claim is corrected in place rather than quietly
edited — `ROADMAP.md` says which.

---

## Where this sits

Three repositories share one idea. **Two of them are wired together, and
this is the third.**

```
                    lex-lang
             the high-level language
       16 crates: syntax, ast, types, store,
          vcs, jit, lsp, bytecode, trace
                       │
                       │  Grant, and the real Lex front end
                       ▼
                    lex-os
           the autonomous-agent runtime
     manifest + grant → static check → perimeter
            → supervisor → audit chain


                    lex-sys
                    this repo
       a second, native language, same worldview
```

**`lex-lang`** is the high-level, functional, GC'd, interpreted language
the ecosystem's libraries are written in.

**`lex-os`** is the runtime that takes an agent's goal, seals it in a
microVM, and mediates everything it does against **one** declaration —
the trust `Grant`, enforced twice: statically by `lex-os-check` before
the program loads, and at run time by a supervisor the agent cannot
reach.

**`lex-sys`** — this repository — is a *second, lower-level language*
sharing that worldview, targeting the work Lex cannot do: native
binaries, manual and region memory, syscalls, embedding, FFI. It shares
the idea and **no code**: `lex-os` takes its grant from `lex-lang` and
does not depend on this repository at all.

| Join | State |
|---|---|
| lex-sys code in `lex-vcs` | Most of that crate is already language-agnostic; gated on a plateau in the effect vocabulary rather than on a feature — [`vcs.md`](vcs.md) |
| lex-sys code under a lex-os grant | Not a compiler integration: `authority --output json` is already the interface a supervisor reads, and its filesystem dimension is already enforceable through it. `network` and `exec` are not, because both are libc — [`under-a-grant.md`](under-a-grant.md). *Corrected for `network` ([`agent-toolbox.md`](agent-toolbox.md) §2.5): a program built on the `Net` builtins reports `net_out("host:port")` and `bounded: true`, and only a program that reaches the network through `Ffi` is still opaque; `exec` is still libc. But "enforceable through it" does not yet hold for either dimension: `lex-os` reads Lex effect names, and fed a lex-sys label verbatim it derives `network: none` for a program that dials a host (measured), so the join needs a bridge that fails closed.* |

---

## Why

Three things this philosophy buys that no systems language currently
combines:

1. **Capabilities all the way down.** Ownership and effects are the same
   idea — both are resource tracking. Allocation is an effect, a heap
   value is a linear resource, FFI is a capability you must be granted.
   Owning a capability *discharges* its effects and borrowing *declares*
   them, so `main`'s row is `[]` however much it does.
2. **Determinism as a language property.** No UB, defined evaluation
   order, deterministic layout — which is what makes replay, attestation
   and content-addressing mean anything, and exactly what C throws away.
   Written out operation by operation in
   [`defined-behaviour.md`](defined-behaviour.md).
3. **A checker that is fast and total,** because the guarantee is only
   worth what it costs to verify.

---

## What exists

| | | Settled by |
|---|---|---|
| **Capabilities** | `World`, `Io`, `Fs(prefix)`, `Ffi(lib)`, `Heap`, `Args`, `File`, `Net(bound)` — linear values, `split` once, released by name | [`linearity-and-effects.md`](linearity-and-effects.md) |
| **Effect rows** | A canonically ordered set, exact in both directions, every label tracing to a builtin | [`linearity-and-effects.md`](linearity-and-effects.md) |
| **Narrowing** | Prefix extension, one way, and it *consumes* what it attenuates | [`filesystem.md`](filesystem.md), [`reach.md`](reach.md) |
| **Authority report** | `lex-sys authority`, computed from reachability; `--output json` for a supervisor, and it **fails closed** on foreign code | [`authority.md`](authority.md) |
| **Borrowing** | Lexical regions, no borrow checker; `&!` is a lock on the binding | [`aliasing.md`](aliasing.md) |
| **Memory** | Arenas, a general heap with recursive types, boxed slices, growable buffers | [`heap.md`](heap.md), [`boxed-slices.md`](boxed-slices.md) |
| **Types** | `int` `byte` `bool` `float` (and `f32` from edition 6), structs, enums with exhaustive `match`, tuples, generics with `[T: val]` bounds | [`floating-point.md`](floating-point.md), [`tuples.md`](tuples.md) |
| **Defined behaviour** | Checked arithmetic that traps, left-to-right evaluation, every C hole named and closed | [`defined-behaviour.md`](defined-behaviour.md) |
| **Threads** | Compiler-provided `spawn`/`join`, never crossing the C ABI; one pointer-width payload today | [`threads.md`](threads.md) |
| **Program identity** | `lex-sys ids` — per-declaration content hashes, checked against golden fixtures | [`canonical-ast.md`](canonical-ast.md), [`hash-stability.md`](hash-stability.md) |
| **A content-addressed op log** | `lex-sys vcs publish`/`log` — every declaration as a typed, gated operation | [`vcs.md`](vcs.md), [`vcs-publish.md`](vcs-publish.md) |
| **Compile time** | Pure calls on constant arguments folded; `static` items whose bodies run during compilation | [`compile-time-data.md`](compile-time-data.md) |
| **I/O** | The console in three directions, file handles as linear resources, bulk reads and writes, a `Net` capability for sockets | [`file-handles.md`](file-handles.md), [`bulk-io.md`](bulk-io.md), [`net.md`](net.md) |
| **Refusals** | Every rule carries a stable tag; `check --output json` reports every independent one as data | [`agent-errors.md`](agent-errors.md) |
| **Standard library** | Written in lex-sys, including shortest round-trip float printing and a UTF-8 decoder | [`standard-library.md`](standard-library.md) |
| **Two backends** | Cranelift (dev) and LLVM (release, **default**) — an opt-in second backend became the default once nothing it refused had an asker left | [`llvm-backend.md`](llvm-backend.md) |

What is **not** there yet, and why, is [`ROADMAP.md`](ROADMAP.md).

---

## The language in one page

**A value is `res` or `val`.** A `res` value is consumed exactly once on
every path. Mode is structural — a `res` member makes the whole aggregate
`res` — so `Held[File]` is `res` where `Held[int]` is `val`.

```
res struct Ticket { serial: int }

fn redeem(t: Ticket) -> [] int {
    let Ticket { serial } = t;      // the whole is spent, the parts produced
    return serial;                  // `int` is `val`, so nothing is owed now
}
```

There is no `drop` and no destructor: a resource is destroyed by naming the
function that knows how, which is what keeps an effect row honest once there
are effect rows.

**Looking without spending is a borrow.** `borrow` freezes for a read,
`borrow mut` *locks* for a write — nothing else may touch the value at all,
not even a read.

```
fn serial_of[&r](t: &r Ticket) -> [] int { return t.serial; }

borrow held as &r in {
    putchar(i, 48 + serial_of(r));    // `held` is frozen; `r` reads it
}                                     // owned again here
```

There is **no borrow checker**. A region is a block, so a reference's
validity is lexical rather than inferred: no non-lexical lifetimes, no
variance, no dataflow. A binding is `Owned`, `Frozen` or `Locked`, set at
block entry and restored at block exit; `r_inner <= r_outer` holds exactly
when the outer block encloses the inner one, which is a walk up a stack; and
escape is an occurs-check over one type.

**An effect *is* a borrowed capability.**

```
fn triple(n: int) -> [] int { return n * 3; }                // pure, and says so
fn show[&i](io: &!i Io, n: int) -> [io_write] int { ... }    // borrows the console

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);    // the one source of authority
    release(args); release(heap); release(fs); release(ffi); // unused authority is still a resource
    borrow mut io as &!i in { show(i, 7); }
    release(io);                                             // destroyed exactly once
    return 0;
}
```

`[io_write]` on a signature means the function was handed an `&!i Io` it did
not create — so reading the row and reading the parameter list are the same
act, and a function that was given nothing cannot print however much it
wants to. There is no ambient constructor: `Io { }` is refused, `main`'s
`World` is the only authority in the program, and it is linear, so forgetting
to release it does not compile.

`main`'s own row is `[]` even though it prints, because it *owns* the
capability rather than borrowing one — and ownership is already visible in
the parameter list.

A row is a canonically ordered **set**, so it hashes, which is what per-unit
identity is made of. It is exact in both directions: performing an effect you
did not declare and declaring one you never perform are both errors, because
an inexact row means `[]` stops meaning pure. And there is no list of legal
labels — every label traces back to a builtin that performs one, so a label
with nothing underneath it is refused the moment it is written.

**A foreign call is the same idea pointed at C.**

```
extern fn labs[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int;

let libc = narrow(ffi, "libc");        // `Ffi("")` -> `Ffi("libc")`, one way only
borrow libc as &f in { n = labs(f, 0 - 7); }
```

Narrowing is prefix extension and goes one way. `Ffi("")` names no library,
so it authorises nothing until narrowed; an `Ffi("libcrypto")` can never
become an `Ffi("libc")`. It also *consumes* what it attenuates, so there is
no way back to the wider capability. The capability is checked and then
erased: what libc receives is the integer and nothing else.

**Memory has three shapes**, and the same region machinery checks all of
them.

```
region a {                                   // an arena: released in one call
    let xs = alloc_slice[a](5, 0);           // xs : &!a [int]
    xs[0] = 3;                               // bounds-checked; out of range traps
}

let node = box(h, Node { value: 1 });        // the heap: `Box[T]` is `res`
let tail = unbox(h, node);                   // the only thing that ends one
```

An arena *is* a region — same block, same parent chain, same occurs-check.
Nothing whose type mentions `a` leaves the block, and release is one `free`
whatever was allocated. `Box[T]` is `res`, so the exactly-once rule written
for capabilities and file handles turns out to be a *memory safety* rule
for free: no leaks, no double frees, no use-after-free, none of them checked
by anything new.

**A string is a run of bytes and claims no encoding.** `str` is not a type;
a string is `&r [byte]`, an ordinary slice and therefore an ordinary
reference, so regions, the escape check and the unique-to-shared coercion
all came for free.

**Arithmetic is checked.** `+`, `-`, `*` and unary `-` produce the right
answer or **trap** — they never wrap. Wrapping is expressible but has to be
asked for by name. Division traps on a zero divisor and on `int::MIN / -1`
rather than being undefined. Evaluation order is left to right everywhere,
including a struct literal's fields, and that is enforced rather than merely
intended.

**Real programs run on this**, byte-for-byte checked against the originals:
GNU coreutils `base64` and `sort` ([`porting.md`](porting.md)), a REST
endpoint answered over a real socket ([`net.md`](net.md)), and
`examples/seek/`, not a port — a search tool shaped for an agent calling it
rather than a person typing at a shell ([`agent-tools.md`](agent-tools.md)).

---

## Related work

lex-sys builds on other people's ideas, and the closest of them got
there first.

- **Austral** — the nearest relative: linear types, capabilities as linear
  values with a root capability handed to the entry point, lexical
  borrowing, no borrow checker. Most of this core is Austral's first.
  lex-sys adds the same authority stated as an **exact effect row**, which
  is what `lex-sys authority` reads.
- **Koka** — effects as a row of labels. Kept the row; no handlers, no row
  polymorphism.
- **Effekt** — *effects as capabilities*, from the effect-handler side:
  the closest statement of "an effect is a borrowed capability".
- **Cyclone** — lexical regions; here without Rust's inference.
- **Rust** — ownership as move; the borrow checker declined.
- **Vale** — generational references, which `Gen` is.
- **Zig** — `defer`.
- **Pony**, **Hylo** — other answers to aliasing and ownership. Pony's
  `val` means something different from this one.
- **The object-capability model** (E, *Robust Composition*) and
  **Capsicum** — no ambient authority.
- **Lex** — the parent language, and the worldview.

**The competitor is WASI, not Rust.** For running code you did not write,
WebAssembly with WASI enforces authority at run time, by trying. lex-sys
knows it **before execution**, from the program's text, with no runtime
cost. What each project contributed, traced to the document that used it:
[`related-work.md`](related-work.md).

---

## Design commitments

| Area | Commitment | Why |
|---|---|---|
| Memory | Linear/affine types + regions/arenas — **not** an NLL borrow checker | Local, cheap, total to check |
| Effects | Capability-typed effects in the type system, unified with linearity | One resource system, not two |
| Behaviour | No UB, defined evaluation order, deterministic layout | Reproducibility is load-bearing |
| AST | Canonical, content-addressable, stable per-unit identity | Designed in, never retrofitted |
| Types | Fast, total, decidable inference | The guarantee must be cheap |
| Metaprogramming | Hygienic deterministic `comptime` — **no** textual or proc macros | Macros break stable content-addressing |
| Generics | Monomorphised | Zero-cost, matches Rust's codegen |
| FFI | Explicit, capability-gated | C's effects must not be invisible |
| Backend | LLVM by default, Cranelift for fast dev builds | Reuse; own backend only if zero-C becomes a goal |

Carried over from Lex: examples-as-tests, `[budget]`, effect declarations as
the function's contract.

### Explicit non-goals

- **Not a Rust clone.** No trait-system maximalism, no GATs, no
  specialisation, no borrow checker. Chasing Rust's *power* means inheriting
  Rust's implementation cost and abandoning totality — the failure mode this
  design exists to avoid.
- **Not self-hosting-first.** Porting the lex-lang toolchain is a possible
  end-state, not a starting point.
- **Not a replacement for Lex.** Different layer, different job.

---

## Performance

Linearity and effects are erased at compile time; generics monomorphise.
The gap to C is a backend question, not a price of ownership: Rust sits
within 4% of C on the same kernels
([`against-c-and-rust.md`](against-c-and-rust.md)), and once the overflow
trap and a vectorising backend (`--backend llvm`, now the default) are
both in place, this language measures statistically indistinguishable
from C on the one kernel checked directly against it.

The derivation — what the trap costs, what each backend can and cannot
express, and every kernel measured — lives where it was measured rather
than restated here: [`overflow-cost.md`](overflow-cost.md),
[`check-cost.md`](check-cost.md), [`backend-limits.md`](backend-limits.md),
[`llvm-backend.md`](llvm-backend.md),
[`benchmarks-game.md`](benchmarks-game.md).

---

## Layout

```
crates/lex-sys-syntax    lexer, canonical-shaped AST, parser
crates/lex-sys-types     the type vocabulary: representation and unification
crates/lex-sys-ir        resolution, type checking, monomorphisation; the IR
crates/lex-sys-codegen   Cranelift lowering, native object emission
crates/lex-sys-codegen-llvm  the LLVM backend, --backend llvm (default)
crates/lex-sys-id        canonical encoding and content hashes
crates/lex-sys-vcs       content-addressed operation log (vcs.md)
crates/lex-sys           the CLI
std/                     the standard library, as Lex source
examples/                programs meant to be read
packages/                published lex-sys-vcs packages (package-system.md)
tests/accept             fixtures that must compile and run
tests/reject             fixtures that must be refused, each stating why
benches/                 checked/wrapping pairs; what the overflow trap costs
scripts/bench.py         runs them and prints the table
docs/                    this directory
```

Everything a program can be refused for is refused in `lex-sys-ir`, so the
backend has no error path for a *program* — only for the environment. Each
fixture declares its own expectation in its header (`//~ STDOUT`, `//~
EXIT`, `//~ ERROR`), so adding a rule to the language means adding a
fixture.

---

## Every design document

One line of purpose, one short phrase of status. What each slice found —
the measurements, the bugs, the corrections — is [`ROADMAP.md`](ROADMAP.md),
not repeated here; a doc's own header carries its own detail.

| Doc | Purpose | Status |
|---|---|---|
| [`linearity-and-effects.md`](linearity-and-effects.md) | Core type-system rules: linear/affine ownership, capability-typed effects, how they unify | settled and built ([#2](https://github.com/alpibrusl/lex-sys/issues/2)) |
| [`bootstrap.md`](bootstrap.md) | What M0 settled: host language, file extension, the M0 surface | written ([#3](https://github.com/alpibrusl/lex-sys/issues/3)) |
| [`canonical-ast.md`](canonical-ast.md) | AST shape, canonicalisation, per-unit identity (`lex-sys ids`) | written and built |
| `memory-model.md` | Regions/arenas, escape, the escape hatches | not written — settled piecemeal by `heap.md`/`sharing.md`; nothing left to write on its own |
| [`strings.md`](strings.md) | A string is bytes, not an encoding; `byte` as storage | settled and built |
| [`boxed-slices.md`](boxed-slices.md) | `Box[[T]]`: a pointer and a length | settled and built |
| [`many-files.md`](many-files.md) | A program in more than one file; identity by content | settled and built |
| [`arguments.md`](arguments.md) | The `Args` capability | settled and built |
| [`reading-references.md`](reading-references.md) | Reading through a reference; a reference gives references | settled and built |
| [`heap.md`](heap.md) | `Heap` and `Box[T]`: one value, one allocation | settled and built |
| [`filesystem.md`](filesystem.md) | `Fs(prefix)`, the path check | settled and built |
| [`sharing.md`](sharing.md) | Why `Rc` is not expressible, and `Gen` | settled and built — corrects `linearity-and-effects.md` §9 |
| [`tuples.md`](tuples.md) | `(A, B)` as a structural anonymous struct | settled and built |
| [`shadowing.md`](shadowing.md) | Shadowing: allowed exactly when the binding is dead | settled and built |
| [`standard-input.md`](standard-input.md) | `getchar`: a second label on `Io`, not a new capability | settled and built |
| [`modules.md`](modules.md) | `module`/`import`/`pub`; a module is a namespace, not a trust boundary | settled and built |
| [`standard-library.md`](standard-library.md) | What belongs in `std`, and how a function earns its way in | written and built |
| [`mode-polymorphism.md`](mode-polymorphism.md) | `[T: val]` bounds | settled and built — found a leak and a double free |
| [`collections.md`](collections.md) | Which collections hold a resource, and why it's about shape | settled and built |
| [`json.md`](json.md) | `std.json`: a strict, zero-copy tape parser and a Writer that cannot write bad JSON | built; 13,000 numbers bit-exact against Rust, 1,500 mutated documents against `serde_json`, 355 MB/s to the tape |
| [`map.md`](map.md) | `std.map`: a hash map from byte strings to copyable values, iterating in insertion order | built; tested against a model over 30,000 random operations; found the LLVM backend growing the stack inside loops |
| [`http.md`](http.md) | `std.http` and `std.route`: a strict request parser (refuses what request smuggling lives on) and a router that answers with an id you `match` on | built; 1,089 requests identical to `httparse` field for field, none accepted that it refuses; 1.0-1.4 µs a request, flat to 5,000 routes |
| [`server.md`](server.md) | `examples/api`: a JSON API server over `std.http`, `std.route`, `std.json` and native sockets -- one thread, a `Poller`, keep-alive, pipelining, no `Ffi` | built; 17 tests over real sockets; migrated off `poll(2)`/libc (§8): authority now exact, throughput 134,000 → 73,000 a second -- the kernel's `epoll` in this VM, not the program (a C server falls the same way) -- still ~25× FastAPI |
| [`http-server.md`](http-server.md) | `http.server`: the server loop as a package -- `wait`/`next`/`respond`, the application owns its loop; why a callback does not type-check; `hold`/`answer` for a request whose answer is not ready, and the server's poller shared with the application's own handles (§10) | built; first package that imports `std`; `examples/api` migrated, 21 + 2 tests, ~7% faster than the loop inside the example; §10 tested against 7 mutations |
| [`parallelism.md`](parallelism.md) | threads and vectorization: what exists, what was measured (LLVM vectorizes what cannot trap; a multi-field payload crosses by reference; DuckDB on one core), two ways of keeping the overflow guarantee that did **not** work, and the order to do it in, each step with a gate that can fail | strategy, with its measurements (`benches/parallel/`) |
| [`zeroed-slices.md`](zeroed-slices.md) | `box_slice(h, n, 0)` (and `byte_of(0)`, `false`, `0.0`) is `calloc` with no fill loop; untouched pages are never resident. `jsonq`: 0.46 s to 0.15 s, 427 MB to 89 MB peak | built on both backends; freed-memory and residency checks; seven of eight mutants killed, the eighth equivalent |
| [`checked-output.md`](checked-output.md) | `flush_out(io) -> [io_write] Done`, edition 5: flush standard output and learn whether what was written arrived -- `fflush` and then `ferror`, because `fflush` answers 0 after an earlier failure emptied the buffer (measured); gap L1 of `agent-toolbox.md` | built on both backends; `/dev/full`, a closed descriptor and an earlier failure each observed; seven mutants killed |
| [`memory-moves.md`](memory-moves.md) | `copy_within(buf, dst, src, n)`: a bounds-checked `memmove` inside one slice, edition 5; why a byte loop cannot do it (overlap, direction, 0.5 GB/s: 114.7 ms to compact a 64 MiB arena) and what the two backends emit | built, with a fixture, nine trapping shapes on both backends and three mutants checked |
| [`byte-search.md`](byte-search.md) | `index_of_byte(text, b)`: the first `b` in a slice as one `memchr`, edition 5, and `std.bytes`' `find` and `count_byte` on it; four to five times faster when the byte is rare, and the measured LLVM regression when a needle's first byte is common | built, with fixtures on both backends and mutants checked |
| [`bulk-copy.md`](bulk-copy.md) | `copy_into(dst, src)`: all of one slice onto the front of another as one `memmove`, edition 5; why overlap is possible, and `std.buffer`'s copies rewritten on it (640 MiB of appends: 2.00 s to 0.026 s on Cranelift, 0.47 s to 0.026 s on LLVM) | built, with a fixture on both backends, six trapping and edge shapes, and mutants checked |
| [`native-sockets.md`](native-sockets.md) | Servers without `Ffi("libc")`: typed `Listener`/`Conn`/`Poller` handles, `Clock`, and the four-stage road to a toolchain with no C | slice 1 built (edition 5): `tcp_listen`/`tcp_accept`/`conn_read`/`conn_write` over typed handles on both backends, no `Ffi`; fixed-signature fd builtins rejected because an `int` descriptor is forgeable (§2); `Poller`, `Clock`, `tcp_connect` next |
| [`signals.md`](signals.md) | `Signals`/`SignalWatch` (edition 6): knowing you were asked to stop without `Ffi("libc")` -- a capability narrowed to the signals it claims, so the authority report names them and stays bounded; a bitmask `signals_pending`, a handle the `Poller` waits on, and `signals_close` for "a second signal kills at once" | built on both backends: `signalfd` on Linux, measured; `kqueue` `EVFILT_SIGNAL` on macOS, written and **not run** |
| [`directory-handles.md`](directory-handles.md) | `Dir` (edition 6): a path opened beneath a directory one component at a time, following no link -- the fix for a symlink inside a narrowed prefix or a tool's `--root` reading outside it (#227, gap L6). `open_dir`, `dir_enter`, `dir_open_read`, `dir_close`, and `std.dirs` for a whole path; measured against `openat2` | slices 1 and 2 built on both backends (read beneath; create, append, rename, remove, sync beneath); `lexsys-tools` on top is slice 3 |
| [`directory-listing.md`](directory-listing.md) | Listing a directory and a file's status on a `Dir` handle rather than a path (#222, gaps L2 and L3): `dir_list`, `dir_next`, `dir_list_close`, `dir_stat`, and `std.dirs.list` sorted bytewise; the `dirent`/`stat` layout table for three targets; measured cost per entry | slices 1 and 2 built on both backends (`dir_list`, `dir_next`, `dir_list_close`, `std.dirs.list`; `dir_stat`); the `list` tool is slice 3 |
| [`foreign-authority.md`](foreign-authority.md) | What a program that calls C can reach, symbol by symbol: `Ffi("libc,libssl")` is a set of libraries (one capability, lent narrower), the report lists every reachable foreign symbol as `scope:symbol` (`unbounded_by`, one a line, so a CI pin diffs an added symbol), and a foreign function must borrow exactly one `Ffi` | built on both backends; **closes a hole**: an `extern fn` with no capability was accepted, ran a shell and reported `bounded: true`. Symbol-level rows and capabilities weighed and declined; the `statx` argument shape and `\x` found, pinned, not fixed |
| [`websocket-spike.md`](websocket-spike.md) | A WebSocket/OCPP connection layer on lex-sys against the `lex` runtime: 10,000 connections, measured, plus the gaps found and the `lexsys-cache`-for-Redis question | measured: 80 MB and 0% idle CPU at 10,000 connections (the runtime's thread-per-connection stand-in saturates a core near 1,500); the service-level gaps are listed in §10 |
| [`formatting.md`](formatting.md) | `lex-sys fmt`: the canonical layout with comments, blank lines and literal spellings kept | built; refuses, rather than risks, a file it cannot reproduce |
| [`testing.md`](testing.md) | `trap()` and `std.test`'s `assert` — what a program needed to state "this must be true" at all | `trap()`, `std.test`, and `lex-sys test` are built; no per-test timeout, no message on a failed assertion |
| [`slicing.md`](slicing.md) | `s[a..b]`, half-open and trapping | settled and built |
| [`defer.md`](defer.md) | `defer E;` as sugar, expanded during lowering | settled and built |
| [`authority.md`](authority.md) | `lex-sys authority`; `release` at `main` is the declaration | settled and built |
| [`budget.md`](budget.md) | Whether `[budget]` is a type-system feature | settled — no |
| [`reach.md`](reach.md) | What a program can reach, via a real REST endpoint | settled and built; corrected by `opaque-pointers.md` and `foreign-linking.md` |
| [`opaque-pointers.md`](opaque-pointers.md) | `c_ptr`: one opaque foreign-pointer shape | settled and built (edition 3) |
| [`foreign-linking.md`](foreign-linking.md) | `-l`/`-L`: linking beyond libc | settled and built — `examples/tls_client/`, a real handshake |
| [`tls-nonblocking.md`](tls-nonblocking.md) | `https` for a service with one thread: a non-blocking OpenSSL client on the `Poller` (memory BIOs), certificate verification with every failure told apart, the authority it costs, a resolver that does not stop the loop and a pinned address, what `lexsys-hooks` would change, and 12 gaps with reproducers | spiked and measured (`examples/tls_nb/`): 64 handshakes on one thread, ~0.6 ms of CPU each (0.3 resumed), 26-48 KiB a connection; not built into any service; OpenSSL through FFI by decision, a pure lex-sys TLS is a research project |
| [`overflow-cost.md`](overflow-cost.md) | What the overflow trap costs, measured | measured — corrects the README and `defined-behaviour.md` §2.1 |
| [`bitwise.md`](bitwise.md) | `& \| ^ ~ << >>`, hex literals | settled and built |
| [`porting.md`](porting.md) | Real programs ported and checked byte-for-byte: `base64`, `sort` | done twice |
| [`against-c-and-rust.md`](against-c-and-rust.md) | lex-sys against C and Rust on the same algorithm | measured |
| [`purity.md`](purity.md) | The checked purity proof C can only promise and Rust can't state | measured and unspent |
| [`floating-point.md`](floating-point.md) | `float`, IEEE-754 binary64 | settled and built |
| [`float-printing.md`](float-printing.md) | Shortest round-trip decimal printing | settled and built |
| [`f32.md`](f32.md) | `f32`, IEEE-754 binary32, asked by `lexsys-gpu`: the type, the `f32` suffix, `f32_of`/`float_of32`/`bits_of32`/`f32_of_bits`, the bit-for-bit gate against binary64, `sqrt32` and the `int` conversions, and `std.fmt32` (Rust's `{:?}` and `{:.N}` and a decimal read straight to the nearest `f32`, every 2³² pattern checked) | F1, F2 and F3 built (edition 6); F4 `lexsys-gpu` not |
| [`compile-time.md`](compile-time.md) | Constant folding and pure-call evaluation | settled and built |
| [`compile-time-data.md`](compile-time-data.md) | `static` items evaluated during compilation | settled and built |
| [`layout.md`](layout.md) | What every leaf costs; packing and transposing | measured, deferred with a trigger (`lex-sys layout`) |
| [`benchmarks-game.md`](benchmarks-game.md) | Five kernels from the Computer Language Benchmarks Game | measured |
| [`bulk-io.md`](bulk-io.md) | `write_bytes`: a whole slice in one call | settled and built |
| [`file-handles.md`](file-handles.md) | An open file as a linear resource | settled and built |
| [`file-writes.md`](file-writes.md) | The write side of a file handle: append, positional read/write, `fsync`, truncate, rename, lock — what a durable log needs; `fopen`-based opens to avoid variadic `open` | slice 1 built (edition 5): `open_append`/`open_write`/`open_new`/`open_rw`, `file_write`/`file_pwrite`/`file_pread`/`file_sync`/`file_truncate`/`file_size` on both backends; the cost of sync measured (0.4 µs written, ~190 µs durable, 4.2 µs batched by 100); slice 2 adds `fs_rename`, `fs_remove` and `file_lock` (a lock the kernel drops when the holder dies) |
| [`utf8.md`](utf8.md) | Decoding `&r [byte]` into code points | settled and built |
| [`standard-error.md`](standard-error.md) | `err_write`: a third label on `Io`, not an eighth capability | settled and built |
| [`../AGENTS.md`](../AGENTS.md) | How to write lex-sys in one page | written and enforced |
| [`float-math.md`](float-math.md) | `sqrt` as a builtin, then `exp`/`log`/`pow`, then `floor`/`ceil`/`round`/`fabs`/`fmin`/`fmax`/`sin`/`cos`, then `exp`/`log`/`pow` fixed to 1–4 ulp and `expm1`/`log1p`/`log2`/`log10`/the hyperbolics added, then `tan`/`atan`/`atan2`/`asin`/`acos` and a Payne–Hanek reduction for every finite argument | settled and built; every function measured against libm in ulps |
| [`gpu.md`](gpu.md) | Whether lex-sys can run on a GPU, and whether `lex-gpu` should exist | measured; decided |
| [`line-reading.md`](line-reading.md) | Whether `std` needs a line reader | measured — no; found and fixed a silent truncation bug |
| [`agent-errors.md`](agent-errors.md) | Refusals a machine can read: stable rule tags, `check --output json` | settled and built |
| [`agent-tools.md`](agent-tools.md) | A tool genuinely shaped for an agent's own loop | built (`examples/seek/`) |
| [`agent-toolbox.md`](agent-toolbox.md) | Whether a *set* of unix-like tools in lex-sys, with a JSON contract, rule-tagged errors with repair hints and a compiler-derived authority, is worth building — and the protocol that would say so | design; the probes it rests on are run and recorded, nothing is built. Found: `Fs` extent cannot be static (`narrow` takes a literal), no directory listing or regex, `sha256` traps past 64 KiB, a failed `stdout` write is invisible, and lex-os reads a lex-sys `net_out` label as *no network*; corrects `bulk-io.md` §3.3 and `agent-tools.md` §1 |
| [`aliasing.md`](aliasing.md) | Whether `&!` should mean Rust's `&mut` | measured — no |
| [`check-cost.md`](check-cost.md) | The price of every check this language emits | measured — corrects `overflow-cost.md` and `gpu.md` |
| [`poison.md`](poison.md) | A per-lane flag instead of a trap, on a vector ISA | measured — depends on the check |
| [`backend-limits.md`](backend-limits.md) | What Cranelift can and cannot be asked for | an audit, checked against source |
| [`llvm-backend.md`](llvm-backend.md) | A second backend, `--backend llvm` | **complete, and the default** |
| [`effect-polymorphism.md`](effect-polymorphism.md) | Whether an effect row can be polymorphic | a documented no, counted by reading |
| [`hash-stability.md`](hash-stability.md) | How often a content hash actually moves | measured — the plateau `vcs.md` needed |
| [`vcs.md`](vcs.md) | How much of `lex-vcs` lex-sys can reuse | plateau answered yes; foundation, gate, op log and attestation built |
| [`vcs-publish.md`](vcs-publish.md) | The first real caller of `lex-sys-vcs`: publish, no diffing needed | built (`lex-sys vcs publish`/`log`) |
| [`package-system.md`](package-system.md) | What a package would be, built from what already exists rather than invented fresh | `lex-sys vcs resolve`/`vcs lock`/`vcs fetch` are the first three real slices; `net.sockets`, `net.connect`, `agent.wire`, `http.request`, and `http.response` (`packages/`) are five real packages, `examples/fetch/fetch.ls` composes two of them at once with no new tooling, and `http.request`/`net.connect`/`http.response` each depend on `net.sockets` -- a true closure of stores, resolved recursively by `vcs publish --requires`/`vcs resolve`/`vcs fetch` (§4.6), the one thing this row used to call missing. **§7 (packages across repositories) is built for its first two steps:** a lock can carry an `origin` (a git URL, a full commit hash and a path) and `vcs fetch`/`resolve`/`lock --git` fetch it into a cache of checkouts keyed by commit, with every pin re-checked as for a local store; `vcs publish --dir` publishes a library of several files in dependency order, and `static` declarations now publish. **§8 (the project file) is built, `lex-sys test` in a project included:** `lex-sys.toml`, `lex-sys install`, `lex-sys add`, `lex-sys build` with no files, and a compiler revision in `--version` that a project checks. **§9 (prebuilt compilers) is built, unreleased:** `scripts/package-release.sh`, `scripts/install.sh` (checksum and reported commit both checked) and a release workflow whose tag is the commit. Still missing: the first published release, and a human-readable version string |
| [`character-literals.md`](character-literals.md) | `'a'`: a third spelling of an integer | settled and built |
| [`emitted-checks.md`](emitted-checks.md) | The price list, read out of the binary | an audit; found a real compiler bug |
| [`flags.md`](flags.md) | `std.flags`: a cursor, not a `getopt_long` table | settled and built |
| [`first-page.md`](first-page.md) | What a reader of `README.md` actually learns | measured, and acted on — this guide is the result |
| [`under-a-grant.md`](under-a-grant.md) | Whether a `lex-os` grant can decide a lex-sys authority report | measured — filesystem is enforceable; network and exec are not |
| [`net.md`](net.md) | `Net`: sockets, taken out of libc | settled and built |
| [`related-work.md`](related-work.md) | What lex-sys took from Cyclone, Koka, Rust, Vale, Zig, Lex, Austral | written |
| [`differential.md`](differential.md) | The constant folder against the backend | a test; found and fixed one disagreement |
| [`connect.md`](connect.md) | `examples/fetch/`: an HTTP client, and what `connect` needed | a probe |
| [`listen.md`](listen.md) | `examples/collect/`: the inbound counterpart | a probe; cleared `Net`'s last bar |
| [`internal-errors.md`](internal-errors.md) | A backend failure as a located refusal, not an environment error | settled and built |
| [`fuzzing.md`](fuzzing.md) | A mutation fuzzer over the whole corpus | a test; found four printer bugs |
| [`function-values.md`](function-values.md) | Whether closures exist, and what mode a capturing one has | settled and built |
| [`threads.md`](threads.md) | Compiler-provided `spawn`/`join`, never crossing the C ABI | single-leaf slice and owned-capability case built; multi-field payload decided not yet |
| [`editions.md`](editions.md) | An edition marker, a path for renames | design, measured |
| [`defined-behaviour.md`](defined-behaviour.md) | Every place C/Rust leave behaviour open, defined instead | written and enforced |
| [`crypto.md`](crypto.md) | `std.crypto`'s first slice: SHA-256 | built |
| [`sha512.md`](sha512.md) | `std.crypto`'s second slice: SHA-512 | built |
| [`ed25519.md`](ed25519.md) | `std.ed25519`: sign and verify | built |
| [`chacha20.md`](chacha20.md) | `std.chacha20`: ChaCha20, Poly1305 and the AEAD (RFC 8439), for the pure TLS client | built; not independently reviewed |
| [`hkdf.md`](hkdf.md) | SHA-384, `std.hmac` and `std.hkdf` for the TLS 1.3 key schedule; the 64 KiB trap every `std` hash had | built; not independently reviewed |
| [`tls-pure.md`](tls-pure.md) | The TLS 1.3 client with no C library: where the code lives, the sans-io API both backends share, cipher policy, threat model, trust, refusal tags, open questions | design; the protocol built in `packages/tls` (#205, `tls-core.md`), chain validation not yet (#206) |
| [`x25519.md`](x25519.md) | `std.x25519`, and `std.field25519`: one constant-time field for X25519 and Ed25519 | built; checked with Valgrind (ctgrind); not independently reviewed |
| [`x509.md`](x509.md) | `packages/x509`: a strict DER reader and an X.509 v3 parser, the pure TLS client's certificates | built; not independently reviewed |
| [`rsa.md`](rsa.md) | `std.bigmod` (Montgomery arithmetic to 4,096 bits) and `std.rsa` (PKCS#1 v1.5 and PSS verification) | built; not independently reviewed |
| [`ecdsa.md`](ecdsa.md) | `std.ecdsa` (P-256 and P-384 verification, DER and raw signatures) on `std.bigmod`'s new register API | built; not independently reviewed |
| [`tls-core.md`](tls-core.md) | `packages/tls`: the TLS 1.3 client handshake and record layer -- the PR split, a slot, the state machine, pinned certificates until #206, and why the byte-for-byte gate uses a recorded ChaCha20 handshake instead of RFC 8448's AES trace | built: the protocol, the 64-slot engine, live servers, a lying server, 22 mutants killed |
| [`x509-verify.md`](x509-verify.md) | `packages/x509`'s chain verification for #206: the API over a sent chain's ranges, path building with backtracking and a budget, what leaf, intermediate and root are each checked for, name constraints and the types it cannot read, and the four gates (x509-limbo, an OpenSSL matrix, saved real chains against the system roots, mutants) | built: 9,743 of limbo's 9,802 cases pass (every BetterTLS one), 39 disagreements each explained against OpenSSL, 22 mutants killed; `packages/tls` verifies with it |
| [`tls-parity.md`](tls-parity.md) | #207: parity with the OpenSSL backend ("AEAD parity"): what OpenSSL 3.0.13's ClientHello offers, measured; constant-time AES-GCM, P-256/P-384 key exchange and HelloRetryRequest, TLS 1.2 with ECDHE and AEAD suites only; what stays different and why | built: AES-GCM (§3.1.1), P-256/P-384 (`ecdh.md`), TLS 1.3's AES-GCM and HelloRetryRequest (§3.3.1), TLS 1.2 (§3.4.1) |
| [`ecdh.md`](ecdh.md) | `std.ecdh`: P-256 and P-384 key exchange with a secret scalar -- complete formulas, a masked table, `std.bigmod`'s reductions made constant time, and the timing test that found LLVM turning the masks into branches | built; not independently reviewed |
| [`value-barrier.md`](value-barrier.md) | `value_barrier`, edition 6: a value `clang -O2` cannot reason about, so a constant-time mask stays an `and` | built |
| [`self-hosting.md`](self-hosting.md) | Whether lex-sys could host its own toolchain | run; decided not yet — no asker |
| [`agent-cli.md`](agent-cli.md) | `lex-sys introspect`/`skill`: the CLI surface as data, imported from `lex-lang`'s ACLI integration | built; found a false-familiarity bug in the process (`-o`/`-l`/`-L` would have rendered as `--o`/`--l`/`--L`) |
| [`next-phase.md`](next-phase.md) | Replacing episodic duplication/staleness hunts with a mechanical check, argued from `MANIFESTO.md` and three of this session's own mistakes | §3's real instance (10 examples duplicating `std.io`) migrated, 7 of them (§3.1 has the three exceptions); §4's own standing check, the actual proposal, not built yet |

`linearity-and-effects.md` was the gating artifact: the decision set that
determined whether this is a three-month prototype or a three-year project.
It was settled before M2 started, its must-reject list was read as what it is
— the M2 conformance suite, stated in advance — and M2 was then built against
it slice by slice.
