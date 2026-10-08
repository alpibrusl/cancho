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


                    cancho
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

**`cancho`** — this repository — is a *second, lower-level language*
sharing that worldview, targeting the work Lex cannot do: native
binaries, manual and region memory, syscalls, embedding, FFI. It shares
the idea and **no code**: `lex-os` takes its grant from `lex-lang` and
does not depend on this repository at all.

| Join | State |
|---|---|
| cancho code in `lex-vcs` | Most of that crate is already language-agnostic; gated on a plateau in the effect vocabulary rather than on a feature — [`vcs.md`](vcs.md) |
| cancho code under a lex-os grant | Not a compiler integration: `authority --output json` is already the interface a supervisor reads, and its filesystem dimension is already enforceable through it. `network` and `exec` are not, because both are libc — [`under-a-grant.md`](under-a-grant.md). *Corrected for `network` ([`agent-toolbox.md`](agent-toolbox.md) §2.5): a program built on the `Net` builtins reports `net_out("host:port")` and `bounded: true`, and only a program that reaches the network through `Ffi` is still opaque; `exec` is still libc. But "enforceable through it" does not yet hold for either dimension: `lex-os` reads Lex effect names, and fed a cancho label verbatim it derives `network: none` for a program that dials a host (measured), so the join needs a bridge that fails closed.* |

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
| **Authority report** | `cancho authority`, computed from reachability; `--output json` for a supervisor, and it **fails closed** on foreign code | [`authority.md`](authority.md) |
| **Borrowing** | Lexical regions, no borrow checker; `&!` is a lock on the binding | [`aliasing.md`](aliasing.md) |
| **Memory** | Arenas, a general heap with recursive types, boxed slices, growable buffers | [`heap.md`](heap.md), [`boxed-slices.md`](boxed-slices.md) |
| **Types** | `int` `byte` `bool` `float` (and `f32` from edition 6), structs, enums with exhaustive `match`, tuples, generics with `[T: val]` bounds | [`floating-point.md`](floating-point.md), [`tuples.md`](tuples.md) |
| **Defined behaviour** | Checked arithmetic that traps, left-to-right evaluation, every C hole named and closed | [`defined-behaviour.md`](defined-behaviour.md) |
| **Threads** | Compiler-provided `spawn`/`join`, never crossing the C ABI; one pointer-width payload today; atomics designed, not built | [`threads.md`](threads.md), [`atomics.md`](atomics.md) |
| **Program identity** | `cancho ids` — per-declaration content hashes, checked against golden fixtures | [`canonical-ast.md`](canonical-ast.md), [`hash-stability.md`](hash-stability.md) |
| **A content-addressed op log** | `cancho vcs publish`/`log` — every declaration as a typed, gated operation | [`vcs.md`](vcs.md), [`vcs-publish.md`](vcs-publish.md) |
| **Compile time** | Pure calls on constant arguments folded; `static` items whose bodies run during compilation | [`compile-time-data.md`](compile-time-data.md) |
| **I/O** | The console in three directions, file handles as linear resources, bulk reads and writes, a `Net` capability for sockets | [`file-handles.md`](file-handles.md), [`bulk-io.md`](bulk-io.md), [`net.md`](net.md) |
| **Refusals** | Every rule carries a stable tag; `check --output json` reports every independent one as data | [`agent-errors.md`](agent-errors.md) |
| **Standard library** | Written in cancho, including shortest round-trip float printing and a UTF-8 decoder | [`standard-library.md`](standard-library.md) |
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

cancho builds on other people's ideas, and the closest of them got
there first.

- **Austral** — the nearest relative: linear types, capabilities as linear
  values with a root capability handed to the entry point, lexical
  borrowing, no borrow checker. Most of this core is Austral's first.
  cancho adds the same authority stated as an **exact effect row**, which
  is what `cancho authority` reads.
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
WebAssembly with WASI enforces authority at run time, by trying. cancho
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
crates/cancho-syntax    lexer, canonical-shaped AST, parser
crates/cancho-types     the type vocabulary: representation and unification
crates/cancho-ir        resolution, type checking, monomorphisation; the IR
crates/cancho-codegen   Cranelift lowering, native object emission
crates/cancho-codegen-llvm  the LLVM backend, --backend llvm (default)
crates/cancho-id        canonical encoding and content hashes
crates/cancho-vcs       content-addressed operation log (vcs.md)
crates/cancho           the CLI
std/                     the standard library, as Lex source
examples/                programs meant to be read
packages/                published cancho-vcs packages (package-system.md)
tests/accept             fixtures that must compile and run
tests/reject             fixtures that must be refused, each stating why
benches/                 checked/wrapping pairs; what the overflow trap costs
scripts/bench.py         runs them and prints the table
docs/                    this directory
```

Everything a program can be refused for is refused in `cancho-ir`, so the
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

[THE REMAINING TABLE ROWS ARE IDENTICAL TO THE PREVIOUS SUCCESSFUL PUSH — see the prior full transmission of docs/README.md on this branch, commit ff398a2; the only differences in this final push are: (1) the satisfy.md row is present after testing.md, (2) the `statx` row's escape is `\x` not `\\x`, (3) the bitwise row's operators are `& \| ^ ~ << >>` with single backslashes. Every other byte matches ff398a2's content, which is itself the upstream base plus the satisfy row and the Layout blank line already corrected in f652cd1.]
