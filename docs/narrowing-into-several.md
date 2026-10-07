# Narrowing one capability into several

Status: **built (PR #364); the `covers` fix of section 6 was built first (PR #359).** Found by the PostgreSQL TLS work (alpibrusl/cancho-pg#14, its `docs/tls.md` section 5 and
section 11.3): a program that reads two unrelated files holds `Fs("")`, and its authority report says `fs_read("")`, which
is every file. Every claim below about the compiler before this change was checked against the code or a probe at `c0ad830`;
section 10 lists which, and what was not. Section 11 is what building it found, and corrects in place what it showed wrong.

## 1. The problem, as a program and its report

A TLS client needs two things from the filesystem: entropy (`/dev/urandom`) and a trust store (a CA bundle, for example
`/etc/ssl/certs/ca-certificates.crt`). Written today, the only way to read both is through one `Fs` that covers both
paths, and the only prefix that covers `/dev/urandom` and `/etc/...` is the root:

```
fn entropy[&f, &o](fs: &f Fs(""), out: &!o [byte]) -> [fs_read("")] int {
    return fs_read(fs, "/dev/urandom", out);
}

fn roots[&f, &o](fs: &f Fs(""), out: &!o [byte]) -> [fs_read("")] int {
    return fs_read(fs, "/etc/ssl/certs/ca-certificates.crt", out);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(heap); release(args);
    var got = 0;
    region r {
        let seed = alloc_slice[r](32, byte_of(0));
        let pem = alloc_slice[r](4096, byte_of(0));
        borrow fs as &f in {
            got = entropy(f, seed) + roots(f, pem);
        }
    }
    release(fs);
    return 0;
}
```

`cancho authority` on it (run, at `c0ad830`):

```
performs
    fs_read("")
never touches
    the console
    ...
```

and in JSON, `"labels": [ { "name": "fs_read", "argument": "", "bounded": true } ]`. The report is not wrong: the program
*can* read any file, because every function on the path holds the root. It is wider than the program needs, and a
supervisor that pins it (`docs/under-a-grant.md`: the filesystem is the one dimension a grant can already decide from the
report) has to grant the whole filesystem to a program that reads two files.

The toolbox's programs rely on the report being tight. cancho-pg's TLS design wanted the report to name the two files and
could not have it; its `tests/narrow_tls.cho` reads the bundle from standard input instead, to keep
`fs_read("/dev/urandom")`. That is a workaround that changes how the program is deployed in order to keep its report honest.

## 2. Why narrowing is once-only today

The rule is not about the effect row. It is linearity applied to `narrow`, and the reason is stated in the code.

* **`narrow` consumes its capability.** `FnLowering::narrow` (`crates/cancho-ir/src/lower/mod.rs`) lowers the capability
  argument as an ordinary owned use and returns the same value with a narrower type: `Fs` is a `res` value, so after
  `let rng = narrow(fs, "/dev/urandom");` the name `fs` is moved. A second `narrow(fs, ...)` is refused
  `linear-use-after-move` (probe: `` `fs` has already been consumed; a `res` value is used exactly once ``).
* **Narrowing goes one way.** The narrowed `Fs("/dev/urandom")` can only be narrowed to a path *inside* it:
  `extends_path(current, target)` (`crates/cancho-ir/src/defs.rs`) must hold, so `narrow(rng, "/etc/ssl/...")` is refused
  `capability-not-narrowable` ("a capability is attenuated, never widened").
* **A borrow cannot be narrowed.** `narrow` refuses a `Type::Ref` argument with `capability-not-narrowable`: "narrowing
  consumes what it attenuates; narrow the capability itself, before lending it". So there is no way to mint a narrowed
  child from a lent root and keep the root.
* **One `Fs` exists per program.** `split` hands out one `Fs("")`, and nothing else creates one (`linearity-and-effects.md`
  §8.1: a program may not construct a capability; `fork_heap` and `fork_clock` exist for `Heap` and `Clock` only,
  `parallelism.md` §8 and §9).

The reason narrowing consumes is `linearity-and-effects.md` §7.4: "after narrowing there is no way back to it, because
linearity says there is no second use." That is the property that makes `main`'s narrowing lines a declaration
(`authority.md` §1): once `main` has written `narrow(fs, "/tmp")`, nothing in the program can reach outside `/tmp`. The
consequence nobody needed until now is that **the lattice has one chain per program**: `Fs("")` to `Fs("/a")` to
`Fs("/a/b")`, never two siblings.

**The effect row is not the obstacle.** A row may already hold two `fs_read` labels with different arguments:
`Effects` is a sorted, deduplicated set of `Label { name, argument }` (`crates/cancho-ir/src/ir.rs`), compared by
equality in `Effects::missing_from`. Probe: a function taking `&a Fs("/dev/urandom")` and `&b Fs("/etc/ssl")`, reading
through both and declaring `[fs_read("/dev/urandom"), fs_read("/etc/ssl")]`, is accepted today. Nothing can *call* it with
real capabilities, because no program can hold both.

## 3. What the authority report computes, and what it promises

`print_authority` (`crates/cancho/src/main.rs`) unions `func.performs` over `program.funcs`, which pass 2 has already
pruned to what `main` reaches. `performs` is kept **before** ownership discharges anything
(the row check in `lower_function`, `crates/cancho-ir/src/function.rs`: `let performs = performed.clone();` precedes
`performed.discharge(&authority)`). So the report is a union of labels actually performed, each with the argument the
capability's type carried at the operation (`granted_prefix` in `lower/memory.rs` reads it from the `Fs` type at the call).
It promises (`authority.md` §2): a label present is performed on some path, and an absent label is a proof.

Two consequences matter for this design:

1. **The report needs no new format to name two paths.** If two capabilities `Fs("/dev/urandom")` and
   `Fs("/etc/pg/ca.crt")` can exist, reading through each performs a label with its own argument, and the report lists
   both: `"effects": ["fs_read"]` (the kinds are deduplicated) and two entries under `"labels"`.
2. **The report is about what is performed, not what is held.** A root that is held and only released never appears.
   This is why some options below (5 and 6) would make the report tight while leaving `main`'s text wide.

## 4. The options

| | what the program writes | rows | report | compiler cost | verdict |
|---|---|---|---|---|---|
| **1. Status quo** | read the CA bundle from stdin or an argument-named fd, or hold `Fs("")` | unchanged | `fs_read("")`, or tight at the cost of deployment | none | what pg does now; a workaround, not a fix |
| **2. Several children from one `narrow`** | `let (rng, ca) = narrow(fs, "/dev/urandom", "/etc/pg/ca.crt");` | two labels, one per capability | two `fs_read` labels, no format change | one branch in `narrow`, no backend change expected | **recommended** |
| **3. A set of paths in one capability** | `Fs("/dev/urandom", "/etc/pg/ca.crt")`-like, one capability | one label whose argument is a list | the argument becomes a list a consumer must parse | type-literal syntax, `Label::covers`, every backend's path check, a lending coercion | no |
| **4. A narrowing that yields a capability and the remainder** | `let (rng, rest) = carve(fs, "/dev/urandom");` | `rest` is `Fs("")` again | tight if `rest` is only released | the remainder cannot be expressed in the prefix lattice | no |
| **5. Narrow from a borrow (a `fork`)** | `fork_narrow(&!fs, "/dev/urandom")`, the root kept | as 2 | tight | the `fork_heap` exception extended to `Fs` | no |
| **6. Lend a root as a narrower `&Fs`** | `entropy(f, ...)` where `f: &Fs("")` and the parameter is `&Fs("/dev/urandom")` | as 2 | tight | `ffi_scope_attenuates`' coercion extended to `Fs` and the root | not for this; a possible follow-up from a narrowed parent |
| **7. Directory handles** | `open_dir` twice | `dir_read` plus the prefix `open_dir` spent | `fs_read("")` plus `dir_read` | none | does not apply |

### 4.1 Option 2: several children from one `narrow` (recommended)

`narrow` with more than one literal consumes the capability and answers a tuple of capabilities, one per literal, in the
order written:

```
let (rng, ca) = narrow(fs, "/dev/urandom", "/etc/pg/ca.crt");
// rng: Fs("/dev/urandom"), ca: Fs("/etc/pg/ca.crt"); `fs` is spent.
```

*Effect rows.* Each child is an ordinary `Fs(p)`. A helper that needs one borrows one and declares one label; a helper
that needs both borrows two and declares both, which section 2 shows the checker accepts today:

```
fn setup[&r, &c, &o](rng: &r Fs("/dev/urandom"), ca: &c Fs("/etc/pg/ca.crt"), out: &!o [byte])
    -> [fs_read("/dev/urandom"), fs_read("/etc/pg/ca.crt")] int
```

*The report.* Unchanged code, two labels: `fs_read("/dev/urandom")` and `fs_read("/etc/pg/ca.crt")`, and no
`fs_read("")`. The JSON shape, the text shape, `bounded` and `under-a-grant.md`'s reading of the filesystem dimension are
the same.

*Soundness.* Every child is strictly inside the parent (each literal is checked exactly as a single `narrow` checks it:
`extends_path`, and not equal to the parent), so the children together reach nothing the parent did not. The parent is
consumed, so `main`'s text still says everything: one `narrow` line names every path the program will ever touch, and
there is still no way back to the root. That keeps `linearity-and-effects.md` §7.4 and `authority.md` §1 as they are,
which no other option that solves the problem does.

*Code.* The capability is zero-sized: the backends emit nothing for `Builtin::Narrow` (`crates/cancho-codegen/src/body/expr.rs`
and `crates/cancho-codegen-llvm/src/body/expr.rs`, the `Split | Narrow | ForkHeap | ForkClock` arm returns no values).
A tuple of zero-sized capabilities has no leaves, and a tuple carrying an `Fs` already checks, builds and runs on both
backends (probe: a function `fn pass(fs: Fs("/dev/urandom")) -> [] (Fs("/dev/urandom"), int)` destructured with
`let (back, n) = pass(rng);`). *(Corrected in PR #364, section 11: the `Narrow` arm is never reached, because the checker answers an `Expr::Tuple` rather than a call, and neither backend
needed a change.)* The run-time path check is per operation and reads the prefix of the
capability that operation borrowed (`Expr::FileOp`'s `prefix`, `checked_path` in both backends), so it does not change:
each child is checked against its own literal. Probe at `c0ad830`: `fs_read` of `/etc/hosts` through `Fs("/dev/urandom")`
traps (SIGILL, exit 132).

*Cost to programs.* A helper that wants two files takes two parameters. No library can be generic over a program's path
(there is no prefix polymorphism: `effect-polymorphism.md`), so a library that reads a path the *program* chooses still has
to take the bytes, as cancho-pg's TLS library already does, or take `Fs("")`. This option fixes programs, not that.

### 4.2 Option 3: a set of paths in one capability

`Ffi` and `Signals` already narrow to a set (`foreign-authority.md` §5.2, `signals.md` §2.1), so this is the shape with a
precedent. It is the wrong one for paths:

* **A path has no separator to spare.** `Ffi`'s set is comma separated because a library name is
  `[A-Za-z0-9_.+-]` (`parse_scope`, `crates/cancho-ir/src/foreign.rs`). A path may contain a comma, a space, a newline;
  only NUL is excluded. The set needs an escape or a new literal form, in types, in `narrow` and in rows.
* **Every path check becomes a loop over prefixes.** `checked_path` compares the path with one prefix in one pass, in both
  backends (`crates/cancho-codegen/src/body/memory.rs`, `crates/cancho-codegen-llvm/src/body/fs.rs`) and for every user:
  `FileOp`, `OpenFile`, `PathOp`, `ExecSpawn` (`checked_path` is also called from `body/net.rs`, `body/files.rs` and
  `body/process.rs`). Each would take a set and trap only when no member matches.
* **`Label::covers` needs a set-of-path-prefixes rule**, and a coercion for lending a set where a member is wanted
  (`ffi_scope_attenuates` for `Fs`), so a helper can declare one path.
* **The report's argument stops being a path.** `fs_read("/dev/urandom,/etc/pg/ca.crt")` is a string a supervisor must
  parse by this design's escaping rule to know what was granted; under option 2 every argument stays one path.
* The self-hosted checker's declarations half would have to validate the set in a type, as `pass1.cho` does for `Ffi`.

What it buys over option 2 is one parameter instead of two for a helper that needs both paths. That is not worth a new
literal syntax and a change in three code generators.

### 4.3 Option 4: a capability plus the remainder

"Everything except `/dev/urandom`" is not a point in the prefix lattice. The remainder would have to be `Fs("")` again
(then this is option 5 with a different spelling) or a new type form for exclusions, with its own `covers`, its own run-time
check and its own report syntax. And the program does not need the remainder: it needs a second named path, which option 2
gives directly.

### 4.4 Option 5: narrow from a unique borrow

`fork_heap(&!Heap) -> Heap` is the precedent (`parallelism.md` §8): mint an owned child from a unique borrow, the parent
kept. For `Fs` it would be sound (the child is inside the parent) and the report would be tight, because the root would be
held and released but never performed (section 3). It costs the thing §7.4 defends: after it, `main` still owns the root,
and `release(fs)` at the end of `main` stops being "I will never touch the filesystem" beyond what was named.
`parallelism.md` §9 records that `fork_heap` and `fork_clock` already ended "one holder per capability" for two
capabilities that carry no value and so cannot be narrowed; extending that to the capability whose whole point is the
value it carries is a larger step for no gain over option 2. It is also awkward to write: the child is minted inside a
`borrow mut fs as &!f in { ... }` block and has to leave it through a `var` that cannot be initialised beforehand.

### 4.5 Option 6: lending a root as a narrower reference

`Ffi` has one coercion between capability types: a reference to a set may stand where a reference to a subset is wanted
(`FnLowering::ffi_scope_attenuates`, `lower/foreign.rs`, called from the reference arm of `expect_type` in `lower/mod.rs`).
The same for `Fs` would be sound in the same way, since the callee's operations are checked against the *callee's*
literal. Allowed from the root, it would make section 1's report tight without any change to `narrow`: `main` lends
`&Fs("")` to `entropy(fs: &Fs("/dev/urandom"))` and `roots(fs: &Fs("/etc/pg/ca.crt"))`, and only those labels are performed.

But `ffi_scope_attenuates` excludes the root on purpose ("a program that holds it must `narrow` before it calls out, which
is where its libraries are written down"), and the same reason holds for paths: with the root allowed, `main` reads
`Fs("")` and the paths live only in callees' signatures. From a *narrowed* parent (lend `&Fs("/etc/pg")` where
`&Fs("/etc/pg/ca.crt")` is wanted) it is a separate, smaller convenience and a possible follow-up; it does not solve this
problem, because `/dev/urandom` and `/etc/...` share no prefix but the root.

### 4.6 Option 7: directory handles

`open_dir(fs: &Fs(p), path)` spends `fs_read(p)` and hands back a `Dir` whose operations perform the path-free
`dir_read` (`directory-handles.md` §2). Opening two directories still needs one `Fs` covering both, so the report still
says `fs_read("")`, now with `dir_read` beside it, and `/dev/urandom` is a file, not a directory to be beneath. Directory
handles answer a different question (a link inside a prefix), and they compose with option 2: `narrow(fs, "/dev/urandom",
"/etc/pg")` and then `open_dir` beneath the second child.

## 5. The recommendation

**Option 2: `narrow(cap, "a", "b", ...)` consumes `cap` and answers a tuple of `cap`'s type narrowed to each literal, for
the two path capabilities, `Fs` and `Exec`.** Concretely:

1. **Arity.** Two arguments is today's `narrow`, unchanged, answering one capability. Three or more answer a tuple of
   `n - 1` capabilities, in the order written. No edition: a call with three or more arguments is refused
   `arity-mismatch` by every edition today, so no accepted program changes meaning, and `narrow` is not a new name that
   could shadow a program's own function (the reason `Builtin::since` gates new builtins).
2. **Each literal is checked as a single `narrow` checks it**: a literal (`capability-not-narrowable` otherwise),
   `extends_path(current, target)`, and not equal to `current`.
3. **The literals are pairwise unrelated**: no two equal, and none inside another by `extends_path`. Refused
   `capability-not-narrowable`, naming both. Section 9 asks whether nesting should be allowed.
4. **Only `Fs` and `Exec`.** Both are path prefixes with `extends_path` (`processes.md` §4.1 gave `Exec` `Fs`'s rule), and
   both have the same one-chain limit. `Ffi` already narrows to a set and lends narrower; `Signals` is a run-time claim
   (a second live claim of one signal is `EBUSY`, `signals.md` §3, which leaves disjoint children to a `fork_signals`
   nobody has asked for) and its set form already serves; `Net` is bounded by a textual prefix and its
   askers so far pass run-time hosts and ports (cancho-pg's pooler holds `Net("")` for that reason, not for this one).
   For those three, more than one literal is refused `capability-not-narrowable` with a message naming the set form or
   saying it is not supported.
5. **A borrowed capability still cannot be narrowed**, by either form.
6. **Fix `Label::covers` for path labels first** (section 6). It is a separate, small change and should land before or
   with this one.

The program from section 1, after:

```
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(heap); release(args);
    let (rng, ca) = narrow(fs, "/dev/urandom", "/etc/ssl/certs/ca-certificates.crt");
    ...
    release(rng); release(ca);
    return 0;
}
```

and its report: `fs_read("/dev/urandom")`, `fs_read("/etc/ssl/certs/ca-certificates.crt")`, nothing else under the
filesystem.

**What it does not fix.** A path chosen at run time (an operator's `--ca-file`) cannot be a literal, and `narrow` takes
only literals (§7.4). cancho-pg's `examples/psql_tls.cho` and cancho-hooks' `tlsx.setup` take the CA path at run time and
keep `fs_read("")` under every option above. The narrowest honest form for them is a fixed *directory* the operator puts the
bundle in: `narrow(fs, "/dev/urandom", "/etc/cancho-pg")` and a run-time file name beneath it, which reports
`fs_read("/etc/cancho-pg")`. Section 9 asks whether the toolbox wants that convention.

## 6. A latent row bug that option 2 makes reachable

> **Fixed in PR #359.** `Label::covers` now compares `fs_read`, `fs_write` and `exec` labels with `extends_path`, the rule
> `narrow` uses (`filesystem.md` §1.1). The probe below is refused `effect-not-declared` on `owner`'s row
> (`tests/reject/fs_owned_prefix_covers_sibling.cho`, and `exec_owned_prefix_covers_sibling.cho` for `Exec`), and
> `crates/cancho-ir/src/tests/label_covers.rs` pins the rule, including `covers` agreeing with `extends_path` on every
> pair of a path table. Every one of the 673 `.cho` files in the repository checks identically before and after, so no
> in-repo program relied on the byte prefix (the two new fixtures are the only difference); the claim "refuses nothing that is
> accepted today" below is now measured, not argued. `net_out`/`net_in` keep a text prefix, as `narrow` does for `Net`
> (section 5, item 4). The text below is as written when the bug was found.

`Label::covers` (`crates/cancho-ir/src/ir.rs`) decides discharge: owning a capability drops every label it covers from
the performed set before the row is compared (`Effects::discharge`). It has set rules for `signals` and `ffi`, and for
every other label it falls through to

```rust
(Some(mine), Some(theirs)) => theirs.starts_with(mine.as_str()),
```

a **byte** prefix. For `fs_read`, `fs_write` and `exec` that is the comparison `filesystem.md` §1.1 says is wrong: `/tmp`
is a byte prefix of `/tmpevil` and does not contain it. `narrow` uses `extends_path`; `covers` does not.

Probe, accepted at `c0ad830`:

```
fn evil[&e, &o](x: &e Fs("/tmpevil"), out: &!o [byte]) -> [fs_read("/tmpevil")] int {
    return fs_read(x, "/tmpevil/a", out);
}
fn owner[&e, &o](mine: Fs("/tmp"), x: &e Fs("/tmpevil"), out: &!o [byte]) -> [] int {
    release(mine);
    return evil(x, out);
}
```

`owner` performs `fs_read("/tmpevil")` and declares `[]`, because owning `Fs("/tmp")` "covers" it. The row is inexact,
which §7.3 forbids. The **report** is still right (it reads `performs`, before discharge, section 3), so this is a row
bug and not a report hole.

No program can reach it today: it needs one function to hold two `Fs` capabilities that are not on one chain, and section 2
is why no program can. Option 2 is exactly what makes them. So the fix belongs with it: `covers` uses `extends_path` for
`fs_read`, `fs_write` and `exec`. It changes no accepted program today, by the same argument (a function holds at most one
`Fs` and at most one `Exec`, all on one chain, and on one chain `extends_path` and the byte prefix agree because every
link was checked with `extends_path`). The test is the probe above as a `tests/reject/` fixture, refused
`effect-not-declared`.

## 7. What changes, and for whom

**The checker.** `FnLowering::narrow` gains a branch for more than two arguments, before the single-literal path: lower the
capability once, check each literal, check the literals against each other, and answer
`Type::Tuple` of `Type::Named(PRELUDE_FS or PRELUDE_EXEC, [Type::Lit(target)])`. `Label::covers` gains the path rule. No
new rule tag: every refusal is `capability-not-narrowable` or `arity-mismatch`, both existing.

**The backends.** Nothing is expected (section 4.1); to be confirmed by running the fixtures on both.

**The authority report and its checker.** No change to `print_authority`, the JSON or the text. What changes is what
programs can make it say. `under-a-grant.md`'s filesystem dimension gets a list of exact paths instead of `""`.

**Existing programs.** Source compatible: every accepted program checks to the same rows and the same report, and the
`covers` fix refuses nothing that is accepted today (section 6). cancho-pg, cancho-hooks and the planned cancho-mqtt and
cancho-gateway change only where they choose to: pg's narrow TLS pool can name its CA path instead of reading the bundle
from standard input.

**The self-hosted checker (epic #295).** The port has the declarations half (`examples/selfhost/pass1.cho`,
`foreign.cho`, `checker.cho`) and bodies for scalars, references and slices (stages 3c and 3d); a call to a builtin is
`SKIP` in `examples/selfhost/body.cho` (the call path answers `SKIP` when `pass1.builtin_name` matches). So:

* **Nothing to change now.** `tables.cho` carries the builtins' names and editions only (`builtin_starts`, `builtin_lens`,
  `builtin_sinces`, generated by `crates/cancho-ir/src/tests/selfhost_tables.rs`); option 2 adds no name and no edition,
  so the generated file and its drift test are unchanged. `discharged_by` (generated into `prelude_labels`) is unchanged:
  owning `Fs(p)` still discharges the same labels.
* **What stage 3e will need** when it ports `narrow` at all: the n-ary branch (the per-literal check, the pairwise check,
  the tuple answer) and the path rule in its `covers`. The pairwise check is quadratic in the number of literals, which is
  a handful, and needs only `extends_path`, which is byte comparison the port already does for `Ffi`'s scope.
* Option 3, by contrast, would have needed the declarations half changed now: a set in an `Fs("...")` type literal would be
  validated where `pass1.cho` validates `Ffi`'s scope.

## 8. What would prove it

The gate (`cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`) on both CI
targets, and:

* **The report names exactly the two paths.** In `crates/cancho/tests/conformance/authority.rs`, a program that narrows
  to `/dev/urandom` and to a CA file the test writes into a temporary directory (its path written into the program text as
  the literal), reads both, and checks `cancho authority --output json`: `"labels"` is exactly those two `fs_read` entries,
  no `fs_read("")`, `"bounded": true`. The same program with one of the reads removed lists one label: the report follows
  what is performed, not what is held.
* **It runs, and each child is confined**, on **both backends**: the program reads both files and the bytes are right;
  a read of a third path through either child traps; a read of the CA path through `rng` traps.
* **Accepts** (`tests/accept/`): three children; `Exec` with two program prefixes; children moved into a struct and
  released elsewhere; a helper declaring two `fs_read` labels.
* **Rejects** (`tests/reject/`, each with its rule): a literal outside the parent; a literal equal to the parent; two equal
  literals; one literal inside another; a non-literal; a borrowed capability; more than one literal on `Ffi`, `Net` and
  `Signals`; a child used after the tuple is consumed (`linear-use-after-move`); a child not released
  (`linear-value-unconsumed`); and section 6's `owner` (`effect-not-declared`).
* **Mutants**, each of which must turn a test red: skip the per-literal `extends_path`; skip the pairwise check; answer the
  parent's prefix for the second child; answer the children in the wrong order; revert `covers` to the byte prefix; accept
  the n-ary form on `Ffi`.
* **cancho-pg's** `tests/narrow_tls.cho`, rewritten to name a CA file, with its authority assertion changed to the two
  labels, as the downstream check that the toolbox gets what it asked for.

Nothing here needs a measurement of time or size: the change emits no code.

## 9. Open questions, each with a proposed answer

1. **One name or a new one?** `narrow(fs, "a", "b")` beside `narrow(ffi, "a,b")` means two capabilities in one case and one
   capability over a set in the other. *Proposed:* keep `narrow`, because it is the same operation (attenuate, consume)
   with more than one result, it needs no edition, and the n-ary form is refused on `Ffi`, `Net` and `Signals` with a
   message that names the set form, so the two never meet on one capability.
2. **May one literal be inside another** (`narrow(fs, "/var/lib/app", "/var/lib/app/cache")`, to keep the wide one and lend
   the narrow one)? *Proposed:* no, for now. It grants nothing the wider child does not, and the narrow-lending case is
   option 6 from a narrowed parent, which can be designed when a program asks for it.
3. **`Exec` in the first slice, or `Fs` only?** *Proposed:* both. They share `extends_path` and the one-chain limit, and
   the branch is the same code; leaving `Exec` out would be a second slice for one comparison.
4. **Is one asker enough to build it?** The bar is two (`CONTRIBUTING.md`, `standard-library.md`). Today there is one with a
   fixed path, cancho-pg's narrow TLS pool; cancho-mqtt and cancho-gateway will read `/dev/urandom` and a trust store and
   are not written yet. *Proposed:* land the `covers` fix and this design now; build option 2 when the second program
   reaches its TLS setup, or earlier if pg's maintainer wants `narrow_tls.cho` to read a file.
5. **Should the toolbox adopt a fixed trust-store directory** (for example `/etc/cancho/<tool>/`) so that programs whose CA
   file an operator chooses can still narrow? *Proposed:* yes, as a toolbox convention rather than a compiler feature: it
   is what turns `fs_read("")` into `fs_read("/etc/cancho/pg")` for every program that takes `--ca-file`, and it needs
   option 2 to sit beside `/dev/urandom`.
6. **Should the `covers` fix land on its own first?** *Proposed:* yes. It is a correctness fix to a rule
   (`filesystem.md` §1.1's) that `covers` does not follow, it changes no accepted program, and its fixture is the only test
   in this design that can be written today without option 2 (it needs no `main` that reaches it).

**Decided as proposed (2026-10-07):** all six questions above were decided as proposed. On question 4 the decision is: land
the `covers` fix and this design now (the fix is PR #359), and build the multi-literal `narrow` when cancho-mqtt and
cancho-gateway reach their TLS setup, which is the second asker.

## 10. What was verified, and how

Read in the code at `c0ad830`:

* `FnLowering::narrow` (`crates/cancho-ir/src/lower/mod.rs`): two arguments only (`arity-mismatch` otherwise); a literal;
  a borrowed capability refused; `Ffi` and `Signals` narrowed as sets (`narrow_ffi`, `narrow_signals`); `Fs` and `Exec` by
  `starts_with` and `extends_path`; equal refused; the value returned unchanged with the narrower type.
* `extends_path` (`crates/cancho-ir/src/defs.rs`), `discharged_by` for `Fs` (both directions, `file_*`, `dir_*`).
* `Label::covers`, `Effects::discharge`, `Effects::missing_from` (`crates/cancho-ir/src/ir.rs`).
* `lower_function`'s row check, `performs` kept before discharge (`crates/cancho-ir/src/function.rs`).
* `print_authority` (`crates/cancho/src/main.rs`): the union of `performs` over reachable functions.
* `granted_prefix`, `file_op`, `open_file` (`crates/cancho-ir/src/lower/memory.rs`); `checked_path` in both backends; the
  `Narrow` arm in both backends' call lowering.
* `ffi_scope_attenuates` (`crates/cancho-ir/src/lower/foreign.rs`) and its caller.
* The self-hosted port: `examples/selfhost/tables.cho` and its generator, `body.cho`'s handling of builtin calls.

Probed with `cancho` built from `c0ad830` (macOS, AArch64):

* section 1's program checks and reports `fs_read("")`;
* a second `narrow` of `fs` is `linear-use-after-move`; narrowing `Fs("/dev/urandom")` to `/etc/...` and narrowing a
  borrowed `Fs` are `capability-not-narrowable`;
* a row with two `fs_read` labels is accepted;
* section 6's `owner` is accepted;
* a tuple carrying an `Fs` checks, builds and runs on both backends;
* `fs_read` outside the narrowed prefix traps.

Not verified at that commit: that the backends need no change for an n-tuple answer from `narrow` (section 4.1; measured when it was built, section 11), and anything on Linux or
under WASI. cancho-pg and cancho-hooks were read, not built.

## 11. As built

> Built in PR #364, as section 5 recommends and section 9 decided (2026-10-07). Question 4 said to wait for the second asker; it was built ahead of
> cancho-mqtt and cancho-gateway on the maintainer's instruction, with `examples/tls_echo_fixed` (section 11.1) as the in-repository program that
> uses it.

**The rule as implemented** (`crates/cancho-ir/src/lower/narrow.rs`, called from `FnLowering::narrow`):

1. `narrow(cap, lit)` is today's form, byte for byte the same code path. `narrow(cap)` and `narrow()` are `arity-mismatch` ("takes a capability and one or more literals").
2. With two or more literals: every argument after the first must be a string literal (`capability-not-narrowable` otherwise, at the offending argument). A borrowed `cap`, a type
   that is not a capability, and one that carries no value (`Heap`, `Io`, ...) are refused with the single form's messages.
3. The capability must be `Fs` or `Exec`. On `Ffi` and `Signals` the refusal names the set form (`narrow(ffi, "libc,libm")`, `narrow(signals, "TERM,INT")`); on `Net` it says it is
   not supported. All `capability-not-narrowable`, all before any other check of the literals.
4. Each literal passes the single form's three checks in one shared function (`check_narrowing`): it starts with the capability's path, it extends it at a `/` (`extends_path`), and it
   is not equal to it.
5. Then the literals are compared pairwise, in the order written, and the first pair that is equal, or in which one `extends_path` the other, is refused
   `capability-not-narrowable` at the later literal, naming both. A literal ending in `/` therefore contains everything below it.
6. The answer is `Type::Tuple` of the capability's type narrowed to each literal, in the order written. `cap` is lowered once, as an owned use, so it is consumed.

Nothing else changed in the checker: no new rule tag, no new builtin, no new label, no new capability. `Label::covers` was already path-aware (PR #359).

**No edition.** `docs/editions.md` section 5 sorts changes into additive (a new name, field or label, which an older edition must not see), refining and tightening, and section 6.4 closes
the vocabulary of an edition against new labels, builtins and capabilities. This adds none of those. `narrow` is an existing name in every edition; the forms it now accepts
(three or more arguments) were refused `arity-mismatch` before, so no program that compiled changes meaning, no name a program declares can collide with it, and a file in any edition
reads exactly as before. The multi-literal `Exec` form is reachable only from edition 7, where `Exec` is nameable, by the same rule that already gates the single form.

**The backends needed no change.** The checker answers `Expr::Tuple { parts }` with the lowered capability as the first part and an empty tuple for each further child, so
the tuple has the right number of zero-sized components and no leaf. Both backends already lower `Expr::Tuple` by concatenating its parts' leaves, and a capability has none;
the `Builtin::Narrow` arm is not reached at all (the single form returns the capability expression itself and always did). Measured by running the fixtures on Cranelift and LLVM
(`conformance/narrow_many.rs`: two and three children, the order, the run-time confinement, the tuple passed whole and stored in a struct, `Exec`). The unmeasured part:
WASI (`docs/wasm.md`) was not run; a capability is zero-sized there too.

**What building found.**

* A capability inside a tuple or a struct *parameter* is not "owned" in the sense that discharges a label: `fn whole(p: (Fs("/a"), Fs("/b")), ...)` must declare `fs_read("/a")` and
  `fs_read("/b")` even though it releases both, where a function taking `Fs("/a")` directly declares `[]`. That is how rows treat any aggregate, not something this adds, and it is the
  conservative direction; section 4.1's helper (two borrowed parameters) is unaffected. `tests/accept/narrow_three_paths_forms.cho` writes it.
* `open_dir(fs, path)` takes its path at run time and traps outside the capability's prefix as every file operation does, so a program that narrows to a *directory* and opens that same
  literal is exactly confined (section 11.1, and `conformance/narrow_many.rs`'s TLS-setup case, which also checks `open_dir` of `/dev` through the directory's child traps).
* `/tmp` and `/tmpevil` are unrelated by `extends_path`, so `narrow(fs, "/tmp", "/tmpevil")` is accepted and each child is confined to its own tree; `narrow(fs, "/tmp", "/tmp/a")` is
  nested and refused.

**Mutants** (each turned a test red): skipping the per-literal check; skipping the pairwise check for nesting and, separately, for equality; answering the parent's path for the
children; answering the children in the wrong order. Allowing the multi-literal form on `Ffi` is killed by `tests/reject/narrow_many_on_ffi.cho` (see the PR for the run).
Reverting `Label::covers` to the byte prefix was already pinned by `label_covers.rs` (PR #359).

**The self-hosted checker (epic #295).** `examples/selfhost/` ports declarations and bodies for scalars, references and slices, and a call to a builtin is `SKIP` in `body.cho`. It does not
port `narrow`, so it does not need the multi-argument branch now and is unchanged; `tables.cho` is unchanged (no name or edition was added) and its drift test passes. Stage 3e, when it
ports `narrow`, will need the branch described in section 7: the per-literal check, the pairwise check (quadratic in a handful), the tuple answer.

### 11.1 The payoff: `examples/tls_echo_fixed`

`examples/tls_echo` reads its entropy from `/dev/urandom` and its certificates from a directory the operator names with `--dir`, so its `main` holds `Fs("")` and the pinned report says
`fs_read("")` (`conformance/tls_echo.rs`). `examples/tls_echo_fixed/tls_echo_fixed.cho` is the same server with the directory fixed at build time as a literal, `/etc/cancho/tls_echo`
(section 9, question 5's convention): `main` is `let (urandom, certs) = narrow(fs, "/dev/urandom", "/etc/cancho/tls_echo");` and the pinned report has `fs_read("/dev/urandom")` and
`fs_read("/etc/cancho/tls_echo")` where it had `fs_read("")`, and every other label the same (`conformance/tls_echo_fixed.rs`). The server body (`serve`) moved, unchanged, from
`tls_echo.cho` to `echo.cho` so that the two share it (the duplication test refuses a copy); `front.cho` gained `parse_flags`, `parse` without the `--dir` requirement, and `parse` is
`parse_flags` plus that requirement, so `tls_echo` and `https_hello` behave as before.

**What it costs.** The directory is part of the build: to serve certificates from elsewhere, edit the literal and rebuild (a deployment that wants `/etc/cancho/<name>` per instance builds
one binary per name). `--dir` is refused (usage error 2) because the program holds no more of the filesystem than the literal; `--identity <subdirectory>` still names a
subdirectory *beneath* it at run time, through the directory handle. Nothing else about the server changes.
