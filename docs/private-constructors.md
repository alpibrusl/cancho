# Private constructors: a `pub` type its own module alone can build

> **Status: proposed, not built.** No compiler change accompanies this
> document. Claims are marked **measured** (run on the compiler at
> `3fa3fbb`), **read** (from the source or a document) or **predicted**
> (not run).
>
> **Asked for by** `cancho-quantum` (`alpibrusl/cancho-quantum`,
> `docs/m2-results.md`, "R2 is confirmed" and the `forged_qubit`
> fixture): a linear `Qubit` whose only intended source is the
> simulator's `fresh`, which any module can bypass by writing
> `qubit.Qubit { idx: 1, n: 2 }`.
>
> **One finding up front.** Hiding construction alone is not enough: the
> same hole is open for taking a value apart and for reading and writing
> its fields (§2). And making fields private *by default* would break 12
> of the repository's 56 `pub` types and 225 call sites (§3.3), so the
> rule has to be opt-in.

---

## 1. The question

`modules.md` §5 gives `pub` two meanings at once: the **name** is
reachable from other modules, and so are the **fields**, which is the
ability to build a value, take one apart and reach into it. A type that
wants the first without the second has no way to say so. For a plain
record that is fine. For a `res` type whose existence *means* something,
it is not:

> `Qubit` is a resource. If any module can write `Qubit { idx: 1, n: 2 }`,
> then "this qubit came from the simulator" is a convention, and the
> linearity the checker enforces is the linearity of a number that
> anyone can mint.

`modules.md` §6 is clear that a module "is not a trust boundary" and
`pub` "means *reachable*, never *safe*". This proposal does not change
that for functions or authority. It adds one narrow, opt-in refusal:
**a type can say its fields stay inside its module.** It is a statement
about names, in the same sense `pub` is.

## 2. What is accepted today (measured)

A module `t.mod` declares `pub res struct Q { idx: int }` and `pub fn
mint`/`kill`. A second file imports it. All of the following are
accepted by `cancho check --std`, with zero refusals:

| From another module | Accepted today |
|---|---|
| Construct: `mod.Q { idx: 5 }` | **yes** |
| Destructure: `let mod.Q { idx } = q;` | **yes** |
| Read a field through a reference: `fn peek(q: &r mod.Q) { q.idx }` | **yes** |
| Write a field through a unique reference: `q.idx = 99` (and it runs: the program exits 99) | **yes** |
| Read a field of an *owned* `res` value: `q.idx` | refused, `linear-value-taken-apart` (a linearity rule, not a visibility one) |
| A non-`pub` type in a `pub` signature | refused, `not-public` |

So there are **four** holes, not one: construct, destructure, field read,
field write. The last row is the only one that stops today, and it stops
for the wrong reason.

## 3. The proposal

### 3.1 Syntax: a marker on the type

```
pub opaque struct Qubit { idx: int, n: int }
pub opaque res struct Qubit { idx: int, n: int }
```

`opaque` follows `pub` and precedes the mode keyword. Applies to
structs. (Enums: §6.)

### 3.2 The rule

> **O.** Outside the module that declares an `opaque` type `T` —
> that is, in any module whose `module` declaration differs, across any
> number of files — the following are refused:
>
> 1. a struct literal `T { ... }` (**construct**);
> 2. a pattern `T { ... }` in a `let`, a `match` arm, or a parameter
>    (**destructure**);
> 3. a field access `v.f` or `v.f = e` where `v : T`, `&T` or `&!T`
>    (**field read, field write**).
>
> The type may still be **named**, **passed**, **returned**, **borrowed**
> and **stored** (as a field of another type, as an argument).

Inside the declaring module nothing changes: all four operations remain
legal, including across two files that declare the same module
(`modules.md` §3.2), because the module is the privacy boundary, not the
file.

Refusal tag: **`not-public`** (reused). The rule is "this field is not
`pub`", which is what `not-public` already says for a function, a struct
or a type in a signature (`modules.md` §5). Reuse adds no entry to
`agent-errors.md`'s contract and no rule number to the compiler in cancho,
and an agent that learned `not-public` already knows the repair: go
through a function the module exports. If a distinct tag is preferred
(§6), the checks below do not change.

### 3.3 Why opt-in, not a new default (measured)

Counted by script over `std/`, `examples/`, `packages/`, `tests/accept/`
and `benches/` (the `.cancho-vcs` snapshot copies excluded):

* **56** `pub` struct and enum declarations in **36** modules
  (28 `res` structs, 11 inferred structs, 1 `val` struct, 16 enums).
* **12** of those types are constructed or destructured (`Type {`) from
  another module, **225** places in all.
* The heaviest is `std.buffer.Buffer`: **153** places in **31** programs,
  because a `res` value is consumed by destructuring it, and `Buffer`
  programs do so. `std.conns.Table` (3) and `std.route.Router` (2) are the
  other `std` types; the rest are in `examples/selfhost`.
* **28** `pub` structs are **not** constructed or destructured from outside
  their module at all.

So making every `pub` struct opaque would break 12 types and 225 sites,
including every program that takes a `Buffer` apart. Making only the types
that ask for it opaque breaks **0**: nothing today says `opaque`.

(The counts use a regular expression for `qualifier.Type {` in a file
that imports the declaring module. They miss a use through an `as`
alias that differs from the last segment and count a mention in a
comment. They are a lower bound and an approximation.)

### 3.4 What `res` destructuring does here

A `res` value ends by being destructured (`linearity-and-effects.md`
§4; there is no `drop`). Under O, a module outside cannot destructure an
opaque `res` value, so **the only way to end one is a function the
declaring module exports**. That is the property `cancho-quantum` wants:
`measure` is the sole consumer of a `Qubit`. It also means an `opaque
res` type without an exported consumer is a type nobody outside can
finish with, and every program that obtains one fails
`linear-value-unconsumed`. That is correct and the repair is local
(export the consumer), but the diagnostic reads as a linearity error
rather than "this opaque type has no way out". Whether the checker
should say so at the declaration is open (§6).

## 4. Interactions

### 4.1 The content hash (predicted)

`modules.md` §2: a module name is not part of any hash. `pub` is the
precedent: `cancho-id` (`crates/cancho-id/src/lib.rs`) contains no
reference to `public` (measured by `grep`). A declared `res` changes the hash because it "is a fact about the type
that every caller can observe" (`linearity-and-effects.md` §3, citing
`canonical-ast.md` §3); `opaque`
is a fact about *which module may name the fields*, not about the
type's values, so it should be treated as `pub` is and stay out of the
hash. This is **predicted, not tested**; the test it needs is the one
`modules.md` §8 already has the shape of:
`adding_opaque_changes_no_hash`. If it turns out the hash must include
it, the cost is a one-time re-hash of every type that uses `opaque`,
which today is none.

### 4.2 Editions

`editions.md`'s rule of thumb: a change that **adds** something can be
absent from an older edition; one that **refines** needs a migration; one
that **tightens soundness** should stay a refusal. This adds a keyword
and a refusal that applies **only to types that use the keyword**, so no
existing file changes meaning. The only edition question is the keyword
itself: a program that uses `opaque` as an identifier would stop parsing.

* **Measured:** across the repository's `.cho` files, with strings and
  comments stripped, `opaque` appears as an identifier **0** times.
  (`hidden` appears once and `sealed` dozens of times in `std/gcm.cho`
  and elsewhere, which is why neither is proposed.)
* The highest edition in use is **12** (one file). The cheap safe course
  is to recognise `opaque` only in a file that declares the new edition,
  as earlier additive keywords were, so a file written before it cannot
  be affected. `editions.md` §6 describes the per-item edition plumbing
  (`item_editions`) this would use.

### 4.3 Generics and monomorphisation (read)

Mode for generics (`linearity-and-effects.md` §3.1): a generic body is
checked once with its parameters rigid, then again per instantiation. A
privacy check must run on the **written** code with the module of the
*written* declaration, not on the instantiated copy, or `Wrapper[T]`
instantiated in module B from module A's generic would be refused for
code B never wrote. Where the module of a monomorphised copy is recorded
is not something this document established; it is the first thing the
implementation must check.

### 4.4 `std`

Nothing in `std` changes: no `std` type is declared `opaque` by this
proposal. The 28 `pub` structs that nothing outside constructs are
candidates for a later, separate change, one type at a time. `Buffer`
is not one of them (153 sites).

### 4.5 Where the checks would go (read)

Three places in `crates/cancho-ir/src/lower/`: struct literals
(`lower/expr.rs`, `AstExpr::StructLit`, near line 150), field access
(`AstExpr::Field`, near line 340) and field assignment (`lower/stmt.rs`
near line 615), plus struct patterns in `let`, `match` and parameters
(`lower/stmt.rs`, `ast::Pattern`). The visibility test already exists
for functions: `lower/mod.rs` near line 254 and `function.rs` near line
873 compare `target != self.module && !public`. A `TypeDef` already
carries `public` (`defs.rs`); it would gain an `opaque` beside it. An
estimate of **under 100 lines** plus parser support for one keyword, but
this was read, not built.

## 5. What it buys `cancho-quantum`, and what it does not

* **Closes `forged_qubit`** (the asker): `qubit.Qubit { idx: 1, n: 2 }`
  from outside `qc.qubit` is refused, as are destructuring it and reading
  `q.idx`.
* **Does not close `two_handles_one_index`**: `fresh(2, 0)` called twice
  gives two valid, distinct `Qubit` values with one index. `opaque` stops
  *forging*; it does not make *minting* generative. Making D1
  (`qubit-types.md`) enforceable needs both: an `opaque` `Qubit` **and** a
  minting function that takes the simulator's own allocation state and
  advances it, so no two qubits from one simulator share an index. That
  second part is ordinary cancho and needs nothing from the compiler.
* **Does not close `same_handle_cx`** or `copied_handle_cx`: those are
  aliasing, the subject of `same-place-twice.md`.

So the full set of fixes for `qubit-types.md`'s R2 and D1 is **this
document plus a minting function that counts**. Neither alone is enough,
and this document should not be read as promising D1.

## 6. Open questions

1. **Tag.** Reuse `not-public` (the proposal), or a distinct
   `opaque-field`, so a refusal reads differently from "not `pub`"?
2. **Enums.** 16 `pub` enums exist. Hiding variant construction and
   matching is a different rule (and `match` exhaustiveness needs the
   variants). Leave enums out of the first slice?
3. **Per-field visibility.** `pub opaque` is a whole-type switch. A
   field-level `pub` (some fields reachable, some not) is more general
   and three times the surface. Is the type-level switch enough?
4. **Diagnostic for an `opaque res` type with no consumer** (§3.4): warn
   at the declaration, or leave it to `linear-value-unconsumed` at each
   use?
5. **Where does a monomorphised copy's module come from** (§4.3)?
6. **Edition number.** Gate the keyword behind a new edition, or accept
   it in all editions given 0 identifier uses in the repository?
7. **`derive`-like helpers.** Cancho has none, so there is nothing to
   forget to update; flagged only because a future one would need to be
   inside the module.

## 7. The suite this would add

Every rule gets an accept and a must-reject, per `CONTRIBUTING.md`.

| Fixture | Directory | Result | § |
|---|---|---|---|
| `opaque_construct` | `tests/reject/` (two files) | `not-public` | 3.2 (1) |
| `opaque_destructure` | `tests/reject/` | `not-public` | 3.2 (2) |
| `opaque_field_read_through_ref` | `tests/reject/` | `not-public` | 3.2 (3) |
| `opaque_field_write_through_unique_ref` | `tests/reject/` | `not-public` | 3.2 (3) |
| `opaque_inside_its_module` (two files, one module) | `tests/accept/` | accepted | 3.2 |
| `opaque_named_passed_borrowed_returned` | `tests/accept/` | accepted | 3.2 |
| `opaque_res_consumed_by_exported_function` | `tests/accept/` | accepted | 3.4 |
| `opaque_res_without_consumer` | `tests/reject/` | `linear-value-unconsumed` | 3.4 |
| `opaque_generic_instantiated_elsewhere` | `tests/accept/` | accepted | 4.3 |
| `adding_opaque_changes_no_hash` | conformance | passes | 4.1 |
| `opaque_is_an_identifier_before_the_edition` | conformance | passes | 4.2 |

Since the rule needs two files, the reject fixtures live in the
conformance harness beside `the_module_rules_are_enforced_across_files`
(`modules.md` §8), not in `tests/reject/`, which compiles one file at a
time.

The `cancho-quantum` fixture `forged_qubit` would move to its
`tests/reject/` on purpose, as its own header says.

## 8. What this does not say

* **Not that a module is a trust boundary.** `modules.md` §6 stands:
  `opaque` hides *field names*, it grants and removes no authority, and
  an `opaque` type's `pub` functions still run with whatever the caller
  holds.
* **Not that `Qubit` would be unforgeable.** Code **inside** the module
  can still build one, and the module's own `fresh` can still be called
  with any index. §5.
* **Not that the cost is measured.** The 100-line estimate and the hash
  claim are reading and prediction. Everything in §2 and §3.3 is
  measured; nothing in §4 is.

## 9. Evidence not obtained

* No implementation was written, so no refusal here was observed.
* The hash prediction (§4.1) and the monomorphisation question (§4.3)
  were not tested.
* The 225-site count is a regular-expression lower bound.
* The behaviour of a use through an `import ... as` alias, through
  `match` payload patterns, and in parameter position was not probed.
