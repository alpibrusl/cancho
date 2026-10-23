# Same place twice: refusing one resource reached twice in one call

> **Status: proposed, not built.** No compiler change accompanies this
> document. It narrows `aliasing.md` §3 "route 1" to the cases where it
> is worth something, and says what it would cost, using a probe that was
> run on the compiler at `3fa3fbb` and then removed. Claims are marked
> **measured** (run on that revision), **read** (from the source) or
> **predicted** (not run).
>
> **Asked for by** `cancho-quantum` (`alpibrusl/cancho-quantum`,
> `docs/m2-results.md`, must-reject entry #4): a `cx(x, x)` on one
> borrowed qubit, which the checker accepts today.
>
> **One finding up front.** `aliasing.md` §2 counted *zero* places where
> two reference arguments are the same place and one is `&!`. Re-counted
> on today's corpus the answer is **at least 12 distinct call shapes in
> `std`**, all of them an output buffer that is also an input
> (`mul(t, t, z9)`). The earlier row is stale (§2 below), and the
> obvious version of route 1 would refuse the standard library.

---

## 1. The question

`aliasing.md` §4.1 called route 1 "free, and worth nothing": it refuses
the obvious spelling of a thing that routes 2 and 3 still permit, so it
would mislead a reader into thinking aliasing a `&!` is refused. That
argument is about `&!` *meaning unique*, and this document does not
dispute it. The question here is smaller and comes from a different
asker:

> A type declared `res` promises that exactly one thing holds the
> resource. When one call is handed **the same `res` referent twice**, and
> either reference is `&!`, the call breaks that promise in plain sight.
> Can the checker refuse that, and only that, without refusing programs
> that alias *data* on purpose?

`cancho-quantum`'s `Qubit` is such a type. `qubit.cx(ctl: &c Qubit,
tgt: &!t Qubit, ...)` called as `qubit.cx(x, x, st)` hands one qubit to a
two-qubit gate as both operands, which is not a state a quantum program
has.

## 2. What is accepted today (measured)

Nine probe programs (`cancho check --std`, `3fa3fbb`). Every one is
accepted, with zero refusals:

| Probe | Call | Today |
|---|---|---|
| a | `both(x, x)`, both params `&!` | accepted |
| b | `both(x, x)`, one param `&!`, one `&` | accepted |
| c | `let y = x; both(x, y)` | accepted |
| d | `both(x[0..2], x[2..4])`, both `&!` | accepted |
| e | `both(x, y)`, two distinct `&!` | accepted |
| f | `both(x, x)` on `&!` structs | accepted |
| g | `both(x, x)`, both params `&` | accepted |
| h | `both(head(x), x)`, `head` returns a `&!` in its parameter's region | accepted |
| i | `let y = head2(x); both(x, y)` | accepted |

And the three `cancho-quantum` fixtures
(`tests/accept/same_handle_cx.cho`, `copied_handle_cx.cho`,
`two_handles_one_index.cho`) are accepted. They trap at run time in
`gates.cx`.

### 2.1 A corpus pass: what a rule on this would meet

A temporary probe (not committed) was added at the direct-call lowering
site, `lower/expr.rs`, where the arguments are lowered, and run with
`cancho check --std` over **188 programs**: every `tests/accept/*.cho`,
`examples/*.cho`, each `examples/*/` directory, and each `benches/*/*.cho`.
For each call it looked at every pair of reference-typed **parameters**
whose arguments read the same reference slot, and classified the pair by:

* **U/S**: whether the *parameters* are unique (`&!`) or shared (`&`).
  `UU` is two `&!`, `US` is one of each, `SS` is two `&`;
* **bare / sub**: whether both arguments are the plain name (`t`), or at
  least one is a projection (`t[0..8]`, `p.field`);
* **RES / val**: whether the referent type is `res`.

Counts are of **distinct call shapes** (callee, call length, class),
deduplicated across programs because `std` is compiled into each one.
That is an approximation of distinct sites, not an exact count of them.

| Class | Shape | Referent | Distinct shapes | Example |
|---|---|---|---:|---|
| UU | bare | val | **1** | `both(s, s)`, `tests/accept/aliasing_same_place_twice.cho` |
| UU | sub | val | 6 | `std` curve and hash code passing two ranges of one buffer |
| US | bare | val | **12** | `field25519_51.mul(t, t, z9)`, `ed25519`'s `mul(cand, cand, x2, w)` |
| US | sub | val | 12 | `gcm_tag(..., buf[0..text], buf[text..text + 16])` |
| SS | any | val | 20 | read-only duplicates; irrelevant to this rule |
| SS | sub | RES | 1 | `__atomic_fetch_add_8`, `benches/atomics/libatomic_counter.cho` |
| UU or US | any | **RES** | **0** | none in the corpus |

The `cancho-quantum` fixture `same_handle_cx` is `US`, `bare`, `RES`
(measured by the same probe run over it).

Three things follow.

1. **The earlier row is stale.** `aliasing.md` §2's table says the count
   of same-place pairs involving a `&!` is **0**, with 0 in `std`. The
   `US bare` row above is 12 shapes, mostly in `std`. Why the two
   disagree was not established: `std` has grown since (the field
   arithmetic and AEAD code), and the probes count differently (the
   earlier one by argument place, this one by parameter mode), but
   neither explanation was checked. Per
   `CONTRIBUTING.md` the correction is made in `aliasing.md` itself; a
   note under the §2 table points here.
2. **Where the table said "cheap", the cheap rule was the narrow one.**
   `UU bare val` is 1 shape in 188 programs and it is the fixture whose
   header says "if a rule ever refuses it, move it to `tests/reject/` on
   purpose".
3. **`US bare` is the in-place idiom and must stay legal.** `mul(t, t,
   z9)` writes `t` from `t` and `z9`. It is defined behaviour
   (`defined-behaviour.md`: aliasing writes happen in program order),
   and 12 shapes in `std` depend on it. Any rule that refuses a unique
   and a shared reference to one place refuses them.

## 3. The proposal

Two rules, deliberately small. Both apply at a **call**, to **arguments**,
by the **parameter** mode written in the callee's signature.

### 3.1 Rule A: a unique referent, twice

> **A.** In one call, two arguments whose parameters are both `&!` may not
> be the same place, where "the same place" is the same bare reference
> (the name itself, not a projection of it).

Refusal tag: **`borrow-conflict`** (reused; see §5 for why a new tag is
not proposed). Corpus cost (measured): **1 fixture**, `both(s, s)`,
which moves from `tests/accept/aliasing_same_place_twice.cho` to
`tests/reject/` as that file already says it must.

### 3.2 Rule B: a `res` referent, twice

> **B.** In one call, two arguments whose parameters are references to the
> same `res` referent may not be the same place, if **either** parameter
> is `&!`. "Same place" is as in A. A `res` referent is any type whose
> mode is `res` (`linearity-and-effects.md` §3), so a `Qubit`, a `Buffer`,
> a capability.

This is rule A widened to `US`, but only for `res`. It is the case the
asker needs: `cx(x, x)` is `US bare RES`. Corpus cost (measured): **0**.
No `UU` or `US` pair on a `res` referent exists in the 188 programs.

Why `res` and not everything: for a `val` referent, aliasing is a
defined behaviour that 12 `std` shapes use deliberately. For a `res`
referent the type's own declaration says the thing is held once, so a
call that holds it twice contradicts the declaration, not an idiom.
The rule is a statement about `res`, and it should be documented as one.

### 3.3 What both rules deliberately leave open

* **Projections.** `f(x[0..2], x[2..4])`, `gcm_tag(..., buf[0..t],
  buf[t..t+16])` and `aes_ctr32(..., buf[0..t], buf[0..t])` (a range,
  twice, on purpose) are `sub`. The checker cannot decide whether two
  ranges overlap and the corpus has 18 such shapes in `std`, some
  disjoint and some deliberately identical. Refusing them is out of the
  question, so they stay accepted. This is the largest hole, and it is
  the same one `aliasing.md` §3 route 3 describes from the other side.
* **Copies (route 2) and returns (route 3).** See §4.
* **Two `Qubit` values with one index** (`two_handles_one_index`). Two
  distinct owned values holding the same integer. No reference rule
  touches that; it is the value-level gap in `qubit-types.md` §4.1.

### 3.4 Must-reject and accept examples

Every rule gets both.

Rule A, refused (`tests/reject/unique_twice.cho`, `borrow-conflict`):

```
fn both[&a, &b](p: &!a [int], q: &!b [int]) -> [] int { p[0] = 1; q[0] = 2; return p[0]; }
fn t[&s](x: &!s [int]) -> [] int { return both(x, x); }
```

Rule A, accepted: two different references, `both(x, y)`; a projection of
one `&!`, `both(x[0..2], x[2..4])`; two `&` params, `both(x, x)` with
both params `&`.

Rule B, refused (`tests/reject/res_referent_twice.cho`,
`borrow-conflict`; the program below is accepted today, measured):

```
res struct Ticket { fd: int }
fn pair[&c, &t](a: &c Ticket, b: &!t Ticket) -> [] int { return 0; }
fn t(k: Ticket) -> [] int {
    borrow mut k as &!x in { pair(x, x); }
    let Ticket { fd } = k;
    return fd;
}
```

Rule B, accepted: the in-place idiom on a `val` referent, `mul(t, t,
z9)` (`US bare val`); two *different* tickets, `pair(x, y)`.

## 4. What this does not close

| Way | Closed by A/B alone? | Notes |
|---|---|---|
| `f(s, s)`, both bare (route 1) | **yes** (A for `&!`/`&!`, B for a `res` referent) | the proposal |
| `let y = x; f(x, y)` (route 2) | **no** | two different slots; see §4.1 |
| `f(head(x), x)` (route 3) | **no** | provenance across a return; no fix short of lifetimes |
| `f(x[0..2], x[2..4])`, projections | **no** | cannot decide overlap; must stay legal |
| two owned `Qubit`s, one index | **no** | a value-level fact |

Against the three `cancho-quantum` fixtures:

* `same_handle_cx`: **closed by B** (predicted from the `US bare RES`
  classification above; the compile-time refusal itself was not run).
* `copied_handle_cx`: **not closed** by A or B as written.
* `two_handles_one_index`: **not closed**, by any reference rule.

### 4.1 Route 2 may be nearly free now (predicted)

The brief for this document expected route 2 to stay open. Reading
`crates/cancho-ir/src/linear.rs` says it might not. Since `aliasing.md`
§6.1 the replay already keeps, per `&!` binding, a set of **roots**:
`let y = x` emits `Event::Alias { slot: y, from: [x] }` and `roots_of(y)`
then answers `[x]` (`linear.rs`, `Alias`, `roots_of`, `unique_roots`).
That table was built so `spawn` can lend a `&!` and see through a copy.

If rules A and B compared **roots** instead of slots, `let y = x;
cx(x, y)` would be refused too: both arguments have root `x`. That would
close `copied_handle_cx` at no additional machinery, but it is a
prediction. It adds one assumption to check before it is promised:
roots are tracked only for bindings that are `&!` *by type*, and `x` in
`borrow mut q as &!x` is one; a `&` binding copied from a `&!` is not
tracked, so `US` through a shared copy may slip through.

If the prediction holds, §3's table gains a "yes" for route 2 and the
rule is better described as "roots", with "bare name" the special case
that needs no table. If it does not, the two rules above stand as
written. This is the first thing the implementation should measure.

## 5. Cost

**Read, not implemented.**

`linear.rs` is 701 lines. The pieces already exist:

* `roots_of` and `unique_roots` (the root table of §6.1);
* `Event::Alias`, emitted at `let` and assignment (`lower/stmt.rs`
  lines near 191 and 205);
* `Rule::BorrowConflict`, raised in the replay at eight places.

What is missing is the call:

1. **Lowering.** The direct-call site in `lower/expr.rs` (the loop over
   `args.iter().zip(params.iter())` near line 974) already walks the
   arguments and `self.reads` already records which slots each read. A new
   event, `Event::Args { groups: Vec<ArgGroup>, span }`, where an
   `ArgGroup` is `{ slots, param_unique, referent_is_res, bare }`, would
   be emitted after the loop. Two further sites have a similar loop (the
   indirect-call case near line 713, and `spawn`'s payload near line 499,
   which §6.1 already handles). Roughly 25 lines.
2. **Replay.** One arm in `linear.rs::run`: for each pair of groups,
   intersect `unique_roots` and raise `BorrowConflict` if the pair meets
   A or B. Roughly 30 lines, reusing the existing roots code.
3. **`res` referent test.** `mode_of` (`linear.rs`) already answers it
   given a type; the lowering has `self.defs` and `self.bounds`, as the
   probe used.

Total **well under 100 lines**, no change to the IR that code generation
reads, so **no content hash changes** (`identity.rs` is unaffected, as for
§6.1), and **no edition**: `editions.md` says a change that tightens
soundness should stay a refusal, and there is no old edition worth
preserving, since the one fixture it breaks says it should break.

**Why `borrow-conflict` and not a new tag.** Every refusal needs a rule
tag, and a new tag is a contract change (`agent-errors.md` lists them;
`every_rule_has_a_fixture` enforces it; the compiler in cancho must carry
the rule number). `borrow-conflict` already means "this value is being
reached in two conflicting ways", which is what this is, and §6.1's
`spawn` refusal already uses it for the same reason. If a distinct tag
is preferred so an agent can tell "lent to a thread" from "passed
twice", that is a one-line change and an open question (§7).

**The compiler in cancho.** `.github/pull_request_template.md` asks
whether a change alters what the compiler refuses. This one does, so
`examples/selfhost` would need to implement the rule or list the gap in
`reject_gaps.txt`. That is real work and is not counted in the 100 lines
above.

## 6. What this does and does not say

* **It does not say `&!` means unique.** `aliasing.md` §4.1 is right that
  a rule refusing one spelling of aliasing while routes 2 and 3 stay open
  tells readers something false. Rule A and B must be documented as what
  they are: *a call may not name the same bare reference twice when one
  of them is unique, and may not reach the same `res` thing twice*. The
  sentence in `README.md` and `AGENTS.md` about `&!` being a lock on the
  binding stays true and should say what this adds.
* **It does not give the optimiser anything.** Projections stay legal,
  so no `noalias` follows; `aliasing.md` §5's backend half is unchanged.
* **It is not a general aliasing rule.** `US bare val` is the in-place
  idiom and it stays.

## 7. Open questions

1. Reuse `borrow-conflict`, or add a tag that distinguishes "passed
   twice" from "lent to a thread"?
2. Should rule B cover `SS` on a `res` referent? Two `&` references to
   one `res` thing are harmless to a reader, so the proposal says no; the
   one corpus instance (`__atomic_fetch_add_8`) is a foreign shim.
3. Do roots (§4.1) go in the first slice, or after the slot-based rule
   has been through the corpus?
4. Is a `res` type the right line, or should a type opt in to "never
   reach me twice" explicitly? `res` is already the type's own promise,
   but it is an inference about intent that a declaration could state.
5. Method-call and builtin sites: the probe saw only direct calls. Calls
   through function values are refused by rule (`effect-polymorphism.md`
   §2), so there is nothing to cover there.

## 8. The suite this would add

| Fixture | Directory | Rule | § |
|---|---|---|---|
| `unique_twice` | `tests/reject/` | `borrow-conflict` | 3.1 |
| `res_referent_twice` | `tests/reject/` | `borrow-conflict` | 3.2 |
| `aliasing_same_place_twice` | **moved** to `tests/reject/` | `borrow-conflict` | 3.1 |
| `in_place_update_stays_legal` (`mul(t, t, z9)` shape) | `tests/accept/` | none | 3.2 |
| `two_ranges_of_one_buffer_stay_legal` | `tests/accept/` | none | 3.3 |
| `shared_twice_stays_legal` | `tests/accept/` | none | 3.1 |
| `copy_then_call` (only if §4.1 holds) | `tests/reject/` | `borrow-conflict` | 4.1 |

`aliasing_through_a_copy.cho` and `aliasing_through_a_return.cho` stay in
`tests/accept/` unless §4.1 closes the first, in which case it moves on
purpose.

## 9. Evidence not obtained

* The compile-time refusal itself: nothing was implemented, so every
  statement that a rule "refuses" is a statement about the classification
  of existing calls, not an observed refusal.
* An exact site count: shapes are deduplicated by callee name and call
  length, which can merge two different sites of one shape and can split
  one site into two when a call's text differs across programs.
* Method-call syntax and builtin call sites other than the direct-call
  loop.
