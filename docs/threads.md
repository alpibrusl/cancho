# Threads: why `pthread_create` cannot be the primitive, and what can be

> **Status: proposed, not started.** `docs/reach.md` §3.3 and
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
> exists. Nothing here is built yet — this is `function-values.md`'s
> own discipline applied to a bigger asker: settle the shape before
> anything forces it.

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

- **`Rc[h] T` must not cross.** `linearity-and-effects.md` §9 names
  it "non-atomic reference count" as a deliberate, single-threaded
  simplicity trade. Two threads each holding an `Rc` to the same
  value could race its increment/decrement, corrupting the count —
  the exact hazard the "non-atomic" word warns about, latent until
  something lets an `Rc` reach a second thread. `spawn`'s payload
  type must exclude it explicitly (a new, narrow check, not derived
  from anything above) until an atomic variant exists, the same way
  `Gen[T]`'s own handle would need auditing before it could cross —
  neither is a hatch this document opens.
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

1. **Function values, minimal slice.** `function-values.md` §4.2 as
   written: a new `Type::Fn(Vec<Type>, Box<Effects>, Box<Type>)`-shaped
   type (or equivalent), a captureless, non-generic, `val` value
   naming a top-level function's `DefId`, and a call through it
   checked against the type's own row. `tests/accept/` gets a
   fixture with no threading at all — passing `write_bytes` as a
   value to a hand-written higher-order function — to prove this
   slice alone before `spawn` is built on it.
2. **`spawn`/`join`, no shared data.** A thread that receives an
   owned `int`, computes on it with no capability, and returns an
   `int` through `join`. Verified by running many of them and
   checking two things directly rather than trusting the type
   checker alone: distinct OS thread IDs (`gettid` or equivalent,
   declared the ordinary `extern fn` way — a thread ID is a plain
   `int`, no different from a process ID `fork` already returns) and
   wall-clock evidence of real parallelism (N independent
   busy-loops joined take roughly one loop's time on a multi-core
   host, not N times it) — the same "measured, not argued" standard
   `reach.md` itself is held to.
3. **`spawn`/`join` with a moved capability.** A thread that receives
   an owned `File` (or `Io`) and performs real I/O, joined, with the
   spawning function checked to no longer hold that capability
   (a `tests/reject/` fixture: using it again after `spawn` is
   `use-after-move`, the existing rule, not a new one).
4. **`spawn`/`join` with a shared reference across the join.** A
   thread that reads a `&r [byte]` the spawning function also reads
   after `join` returns — proving §3's argument holds in practice,
   not only on paper — and a companion `tests/reject/` fixture
   proving a *unique* reference cannot be read from the spawning side
   before `join` (still held by the thread, still moved).
5. **The `Rc` exclusion, checked, not assumed.** A `tests/reject/`
   fixture spawning a thread with an `Rc[h] T` in its payload,
   refused with a rule naming §4's reason, so the exclusion is a
   compiler fact from the day `spawn` lands rather than a documented
   intention nobody enforces.

Given the size of step 1 alone — a real type-system feature this
project deliberately deferred twice (`ROADMAP.md` #82,
`function-values.md` itself) — and that step 3's soundness argument
in §3, while derived from rules that already exist, has not been
checked by anyone but this document's own reasoning, this is written
up for review before any of it is built, the same gate `c_ptr`
(`opaque-pointers.md`) went through before its own implementation,
and a materially larger one to walk through given what it touches.
