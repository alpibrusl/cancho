# Threads: why `pthread_create` cannot be the primitive, and what can be

> **Status: §2's single-leaf slice is built.** `docs/reach.md` §3.3 and
> `docs/function-values.md` §5 both point at the same wall and stop
> there: a thread needs "more than a pointer", and neither document
> says what more. This document finishes that sentence. The answer is
> not "add function values and declare `pthread_create`" — §1 shows
> exactly why that specific shape cannot work in this language, for a
> reason no amount of FFI cleverness fixes. The way through is a
> compiler-provided `spawn`/`join` pair, the same kind of primitive
> `fork`, `box` and `split` already are, built on an insight the
> existing checker did not need to be extended to state: **the
> aliasing rule that makes a `borrow` block sound was never a
> single-threaded rule.** It forbids a second writer while a reference
> is live, full stop, and a spawned thread joined before its region
> closes is just a second writer the checker already refuses to admit
> exists.
>
> **What is built, and what is deliberately narrower than §2's own
> illustrative signature:** `spawn`'s `payload` and `body`'s return type
> are each restricted to one pointer-width leaf — `int`, `bool`, `c_ptr`,
> a captureless function value, a single non-slice reference, or an
> *owned* zero-or-one-leaf capability (`Io`, `File`, `Ffi`, `Fs`, `Args`,
> `Heap`) — checked by `Rule::ThreadPayloadType`
> (`crates/lex-sys-ir/src/lower/conc.rs`). `body`'s own compiled entry
> point becomes `pthread_create`'s start routine directly; the reason a
> general struct payload is still refused is not a missing feature but a
> missing *prerequisite* feature — this language has no
> compiler-synthesised trampoline function yet (nothing builds a
> hand-crafted `Func` outside a parsed source body), which is what a
> multi-field struct payload would need. **§5 step 3's owned-capability
> case turned out to need none of that**: `File` is already one leaf at
> the ABI level (the fd) and every zero-field capability (`Io` and the
> rest) is already zero, so both cross through the exact same paths this
> slice already built for `int` and `()` — the fix was widening the
> checker's allowlist, not new codegen, confirmed by running real
> capability-moving programs on both backends before writing it down.
> `res Thread[T, R]` — two type parameters, not the one sketched below,
> because `T` rides along purely so its region is tracked, getting §3's
> entire escape-check argument for free from `Type::Named`'s
> already-generic `mentions`/`regions_into` walk into its type arguments
> rather than needing any new checker code. Real threads
> (`std::thread`-adjacent: a genuine `pthread_create`/`pthread_join` pair
> on both backends), not a simulation — verified under §5 step 2,
> distinct thread IDs and real wall-clock parallelism, and under step 3,
> a real file read and a real console write each performed by a second
> OS thread holding the only reference to the capability that authorised
> it, all checked directly rather than trusted from the type checker
> alone. Step 4 is also built, folded into steps 2 and 3's own fixtures
> rather than needing a separate one.
>
> **§5 steps 3's struct-shaped case and 5, checked and closed as "not
> yet," not left open.** A genuinely multi-field payload needs the same
> compiler-synthesised trampoline (§4 below) whether or not it happens
> to be a capability, and `Rc[h] T` is not an implemented type in this
> compiler at all, so no fixture can even construct what step 5 would
> refuse. Rather than build the trampoline speculatively, this project's
> own rule (`AGENTS.md` §7: a feature earns its way in when a program
> asks) was applied and checked directly: nothing across `lex-sys`,
> `lex-os`, `lex-lang` or `lex-gpu` asks for a multi-field spawn
> payload today — `lex-os-guest`'s own reasoning loop is single-threaded
> by design with no expressed want for concurrency, and `lex-lang`'s own
> `conc.spawn_thread` (a parallel, independent design for the same
> problem in a different language) sidesteps the question entirely with
> a zero-argument closure rather than an explicit payload, which is a
> mild signal that real background-task askers in this shape of problem
> tend to route around a fat payload rather than demand one. Decided,
> the same way `self-hosting.md` decided **not yet**: revisit when a
> concrete asker exists, not on a schedule.

---

## 1. Why `pthread_create` cannot be an `extern fn`

`function-values.md` §5 already worked out the three conditions a
callback into C needs, checking `pthread_create`'s real signature
against them:

```c
int pthread_create(pthread_t *thread, const pthread_attr_t *attr,
                    void *(*start_routine)(void *), void *arg);
```

1. **The row must be `[]`.** C calls `start_routine` with exactly one
   `void *` and nothing else. There is no capability parameter for an
   authorised call to be checked against, and no context pointer
   `reach.md` §3.1's rule would let carry one anyway.
2. **Its parameters must be scalars.** `start_routine`'s `void *arg`
   is `reach.md` §3.1's wall by another name: a pointer *from* C
   carries no region, and there is no cast, no `from_raw`, no
   transmute anywhere in this language to manufacture one
   (§3.1.1). `c_ptr` (`opaque-pointers.md`) does not help either — it
   is opaque by design, equal to itself and nothing else, never
   dereferenced. Smuggling a real capability inside one and reading
   it back out the other side is exactly the "smuggling one does not
   help" argument `reach.md` §3.1.1 already made about `malloc`,
   applied to a thread's argument instead of a foreign result.
3. **The calling convention matches.** True, and irrelevant, because
   (1) and (2) already stop the declaration.

So the honest reading of `function-values.md` §5's own list is not
"threads need function values plus one more thing." It is: **the
`void *arg` C hands a thread's start routine is the same
un-crossable pointer `reach.md` §3.1 refuses everywhere else**, and no
amount of function-value design closes that gap, because the gap is
not about calling a function — it is about the one argument C's own
ABI allows across, which is exactly the shape `c_ptr` was built to be
safe about *by refusing to be anything else*. A `c_ptr`-typed
`start_routine` argument would type-check and be useless: nothing can
turn it back into the capability or reference the spawning side meant
to send, for the same reason nothing can turn a smuggled `int` back
into a reference (`reach.md` §3.1.1).

**The conclusion `function-values.md` §5 already states is the right
one**: `pthread_create` is unreachable through the FFI mechanism this
language has, permanently, not provisionally. A design for threads
that starts from "declare `pthread_create` as an `extern fn`" is
solving the wrong problem.

---

## 2. The primitive is `spawn`, not a declaration

`fork` (`reach.md` §3.3), `box` (`heap.md`), and `split` (§8.2 of
several documents) all share a shape: each is a builtin the compiler
implements directly in Rust, not a foreign function a program
declares and calls through `Ffi`. None of them is bound by the C ABI
a real `extern fn` call has to cross, because none of them *makes* a
C call in the way an `extern fn` does — the compiler is free to
implement each however it needs to, the same freedom that lets `box`
hand back a real, checker-tracked pointer while `malloc` (declared by
hand) cannot (`reach.md` §3.1).

A thread should be exactly this kind of primitive. Call it `spawn`:

```
fn spawn[T: res](payload: T, body: fn(T) -> [row] R) -> [] res Thread[R];
fn join[R](handle: res Thread[R]) -> [row] R;
```

(Illustrative signatures — `spawn`/`join` are builtins with call-site
checked types, the same way `split`/`release`/`box` are, not written
functions; `[T: res]`/`[R]` here mean "however the real type-checker
already spells an unbounded generic parameter", `mode-polymorphism.md`
§2.)

Because `spawn` never goes through libc, none of §1's three
conditions apply to it. What replaces them:

- **`body` is a captureless function value** (`function-values.md`
  §4.2, unchanged) — this is the asker §6 of that document said would
  justify building them. `payload` is its one argument, and unlike a
  C callback's `void *`, it is an ordinary lex-sys value of any
  type the checker already understands, because the compiler runtime
  calls `body`'s own compiled entry point directly — the same
  `lexs_<name>` symbol a normal call already targets — rather than
  going through a C function pointer with a fixed, foreign signature.
- **`payload` is moved, not borrowed.** `spawn` consumes it exactly
  the way any function call consumes an argument whose type is not
  `val` (`linearity-and-effects.md`'s existing rule, nothing new).
  After the call, the spawning code cannot read `payload` again —
  which is the whole soundness argument, in §3.
- **`body`'s row is whatever it needs to be**, not `[]`. If `payload`
  carries a capability, `body`'s row can perform whatever that
  capability authorises, checked exactly as a normal call's row is
  checked against what its arguments carry (`reach.md` §8.4's
  authorisation check, unmodified).
- **`join` is mandatory.** `Thread[R]` is `res`: obliged to be
  consumed, exactly like a `File` handle or any other capability
  (`linearity-and-effects.md` §4). A program that spawns and never
  joins does not compile, for the reason `file_handle_unclosed.ls`
  already exists.

## 3. Why this needs no new soundness rule, only a new obligation

The question `function-values.md` §4.1 raised about closures — "a
captured capability is authority no parameter names" — does not apply
here, because `payload` *is* a parameter, of `spawn` itself, checked
by the ordinary rule that already governs every parameter. The real
question is different, and it is the one this document's status line
opens with: **can two OS threads touch the same region without the
checker knowing anything about "threads" at all?**

The existing rule is `linearity-and-effects.md` §5's aliasing
discipline: while a unique reference `&!r T` is live, no other
reference to the same value exists; while a shared reference `&r T`
is live, every other reference to it is also shared, and nothing
through any of them writes. This rule is stated over **lifetimes**,
not over **control flow** — it never says "and no other call frame
touches it," because in a single-threaded language every other call
frame is either an ancestor (blocked on this one returning) or has
already returned. A second OS thread is the first call frame that is
neither: it can run *while* the spawning frame is also running.

That sounds like a gap. It is not one, provided exactly one thing
holds: **`payload`'s references cannot escape `join`.** If a
reference `&r T` inside `payload` is only valid because a `borrow r`
block is open in the spawning function, and `join` on that thread's
handle must run before that `borrow r` block ends, then the two
"threads" of execution are, from the aliasing rule's point of view,
just two readers of `r` whose lifetimes are both nested inside the
same open block — indistinguishable from two ordinary calls each
taking `&r T`, which the rule already allows and already forbids a
concurrent *unique* reference to. The rule was never about time; it
was about which reference exists when. Enforcing "the handle cannot
outlive the region its payload mentions" is exactly
`linearity-and-effects.md` §5 rule 4, the escape check
`Type::mentions` already performs (`crates/lex-sys-types/src/lib.rs`)
— applied to `Thread[R]`'s own type the same way it is applied to a
`borrow` block's result today. No new check; the existing one, asked
about one more kind of value.

What this buys, precisely:

- **A shared reference (`&r T`) may cross into `payload`.** Two
  readers, one on each OS thread, is what the shared-reference rule
  already permits — it is unsound only if a *writer* could run
  concurrently, and the rule already refuses to admit a unique
  reference exists while a shared one is live, regardless of which
  thread holds either.
- **A unique reference (`&!r T`) may cross into `payload`, and then
  the spawning function holds no reference to that value at all**
  until `join` returns it back (if `R` carries it out) — this is not
  a new rule either; it is what "moved" already means for a unique
  borrow's own referent once the borrow ends, applied to the borrow
  itself.
- **A capability may cross into `payload`**, checked by the existing
  authorisation rule (`reach.md` §8.4): the spawned `body`'s row is
  whatever the capability inside `payload` authorises, and the
  spawning function no longer holds that capability (moved), so it
  cannot also use it — the same "authority is a resource, spent once"
  rule `linearity-and-effects.md` §4 already states for `release`.

## 4. What this does not solve

- **`Rc[h] T` must not cross — moot today, checked rather than
  assumed.** `linearity-and-effects.md` §9 names it "non-atomic
  reference count" as a deliberate, single-threaded simplicity trade.
  Two threads each holding an `Rc` to the same value could race its
  increment/decrement, corrupting the count — the exact hazard the
  "non-atomic" word warns about, latent until something lets an `Rc`
  reach a second thread. The exclusion needs no new check at all right
  now: `Rc` is not an implemented type in this compiler (`sharing.md`),
  so `crosses_to_a_thread`'s allowlist already excludes anything shaped
  like it, the same way it excludes every other multi-field type. Worth
  a real, narrow check and a fixture the day `Rc` (or an atomic variant
  of it) exists — not before, and not a hatch this document opens in
  the meantime.
- **No shared *mutable* state.** `&!r T` crossing means the unique
  reference moved, not that two threads can now both mutate the same
  memory. A `Mutex`-shaped capability — lock, get a unique reference
  scoped to the lock, unlock — is a real, separate design, parallel
  to how `Fs(prefix)` turned "files" into a capability rather than a
  raw handle (`filesystem.md`). Not proposed here.
- **No cancellation, no thread-local state, no thread pool.** `join`
  blocks until the thread returns; there is no way to ask it to stop
  early, and nothing about a pool of reusable threads is addressed.
  A pool is `spawn`/`join` called in a loop today, at whatever cost
  real OS thread creation has — measuring that cost is part of §5's
  verification, not assumed here.
- **Does not make `pthread_create` reachable.** A program that
  specifically wants to call real `pthread_create` (to interoperate
  with a C library that spawns its own threads, say) still cannot,
  for exactly §1's reason. `spawn` is this language's own primitive,
  not a wrapper around the libc one — the runtime backing it may well
  call `std::thread::spawn` (which itself calls `pthread_create`) in
  Rust, but that call is the compiler's to make, never the program's.

## 5. Verification plan

`function-values.md` §2's bar — two real askers — is not yet met by
counting, the way it was for a decision that already answered
`no`. The verification here has to be earned by building the
primitive and using it, not by counting existing programs against a
feature that does not exist yet for any of them to ask for.

1. **Function values, minimal slice. Built (#127).** `Type::Fn`,
   `Expr::FnValue`/`Expr::CallIndirect`, on both backends —
   `tests/accept/function_value.ls`.
2. **`spawn`/`join`, no shared data. Built.** `tests/accept/
   spawn_join.ls`: a thread that receives an owned `int`, computes on
   it with no capability, and returns an `int` through `join`.
   Verified by checking two things directly rather than trusting the
   type checker alone (`tests/accept/spawn_thread_ids.ls`,
   `tests/accept/spawn_parallel_sleep.ls`, checked on both backends in
   `crates/lex-sys/tests/conformance/backends.rs`): distinct OS thread
   IDs (`pthread_self`, declared the ordinary `extern fn` way — no
   `gettid` needed, since a thread ID here is only ever compared, never
   printed as the kernel's own number) and wall-clock evidence of real
   parallelism (four independent 200ms sleeps, joined, finish in about
   one sleep's time, asserted well under two, not four) — the same
   "measured, not argued" standard `reach.md` itself is held to. The
   same fixture doubles as §3's "shared reference crosses into more
   than one spawn, and the spawning side still reads it after every
   `join`" case: one borrowed `Ffi("libc")` capability is `payload` for
   two threads and is read a third time by `main` once both are
   joined. `tests/reject/spawn_handle_escapes_borrow.ls` checks §3's
   soundness argument itself: a `Thread[&r int, int]` handle returned
   out of the `borrow r` block that opened `r`, without `join`ing
   first, is refused by the existing `reference-escapes-region` check
   — no new rule, exactly as §3 predicted. `tests/reject/
   spawn_payload_type_not_supported.ls` checks this slice's own wall:
   a multi-field struct payload is refused by `Rule::ThreadPayloadType`
   before it ever reaches codegen.
3. **`spawn`/`join` with a moved capability. Built — and this step's own
   original text was wrong.** It assumed an owned `File`/`Io` is a
   multi-field struct under the hood needing the same trampoline the
   struct-payload case does; checked against `abi::leaves_into` on both
   backends before writing any code, `File` turns out to be exactly one
   leaf (the fd, `PRELUDE_FILE`'s own dedicated arm) and every
   zero-field capability (`Io`, `Ffi`, `Fs`, `Args`, `Heap`) is exactly
   zero — the same shapes this slice's codegen already handles for
   `int` and `()`, so the fix was a wider `crosses_to_a_thread`
   allowlist, not a trampoline. `tests/accept/spawn_owned_io.ls` moves
   an owned `Io` into a thread that writes to the real console (a
   zero-leaf capability, `worker`'s own row correctly `[]` since owning
   `Io` outright *discharges* `io_write`, `defs.rs`'s `discharged_by`);
   `tests/accept/spawn_owned_file.ls` moves an owned, already-`open_read`
   `File` into a thread that reads it and closes it there (one real
   leaf, the fd). Both checked on both backends
   (`the_two_backends_agree_on_spawn_owned_io`/`_file`,
   `crates/lex-sys/tests/conformance/backends.rs`). Left genuinely
   unbuilt: a capability shaped like a real multi-field struct — none
   of today's capabilities are, so this step's trampoline-free answer
   may not generalise past `File`/`Io`'s own accidentally-simple shape.
4. **`spawn`/`join` with a shared reference across the join.** The
   shared-reference half is built, folded into step 2 above. Its
   companion — a `tests/reject/` fixture proving a *unique* value
   cannot be read from the spawning side before `join` — is built as
   the owned-capability case's own mirror rather than a separate
   reference fixture: `tests/reject/spawn_owned_capability_reused.ls`
   moves an owned `Io` into `spawn` and then `release`s it again from
   the spawning function, refused by the pre-existing
   `linear-use-after-move` rule with no thread-specific rule added,
   confirming §3's "the existing rule, not a new one" claim by running
   it rather than only stating it.
5. **The `Rc` exclusion — measured, decided *not yet*, not left open.**
   Cannot be built today: `Rc[h] T` is not an implemented type in this
   compiler at all, so no fixture can construct the payload this step
   needs to refuse, and `crosses_to_a_thread` already excludes anything
   of that shape structurally with no code written for it specifically.
   The step this document actually owes an answer to is broader — does
   a multi-field spawn payload (`Rc`-shaped or otherwise) have a real
   asker anywhere today — and that was checked directly rather than
   left as a standing TODO: a repo-wide search across all four reachable
   repositories (`lex-sys`, `lex-os`, `lex-lang`, `lex-gpu`) found none.
   `lex-os-guest`'s reasoning loop (the concrete production target §2's
   own status note names) is single-threaded by design, with no comment
   or TODO anywhere in it wanting concurrency. `lex-lang`'s own,
   independently-designed `conc.spawn_thread` answers the same "how does
   a background task get its state" question with a captured, zero-argument
   closure rather than an explicit payload — evidence, not proof, that
   this problem shape does not actually want a fat payload even where a
   language's design is free to offer one. `AGENTS.md` §7's rule (a
   feature earns its way in when a program asks) applies as written:
   not built, and not because of missing infrastructure alone.

Steps 1 through 4 are built. Step 3's struct-shaped-capability half and
step 5 are **measured and decided not yet**, the same verdict
`self-hosting.md` reached the same way: nothing found is a hard
blocker (the trampoline is buildable, `Rc` is buildable), but nothing
asks for either today. Revisit both together — the trampoline is one
piece of infrastructure whether the payload that needs it is `Rc`-shaped
or any other multi-field struct — the day a concrete asker exists,
rather than on a schedule.
