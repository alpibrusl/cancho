# A function passed as a value is held to the type it is passed for

> **Status: built.** Found on 2026-11-25 while closing [`capability-literals.md`](capability-literals.md), whose section 6 recorded it:
> the checker written in cancho accepted any named function where a `fn(...)` type was wanted. This closes it, with the rule the Rust
> front end had, and two things that front end's rule implies and the port also lacked (`spawn`'s payload, section 4) or that neither
> checked (`where` clauses, section 2.5). Every number below was produced by the command next to it, on darwin-aarch64, with two
> compilers built by `scripts/build-standalone.sh`: **before** is `capability-literals.md`'s change (`3fd8482d`), **after** is this one.

---

## 1. The hole

```
fn takes_bool(b: bool) -> [] int { ... }

fn apply(f: fn(int) -> [] int, x: int) -> [] int {
    return f(x);
}

fn main(world: World) -> [] int {
    release(world);
    return apply(takes_bool, 5);
}
```

Before this change the program was accepted and ran: `takes_bool` read the `int` 5 as a `bool`. `types.unify` had one arm for a
function type and it said, of a named function met where one is wanted, "the code is written for programs the Rust compiler has
accepted, so the signatures are not compared". That was true while the Rust front end checked every program first. It was deleted
on 2026-11-19 (#802), and since then the gap is the language's.

It is the hole of `capability-literals.md` again, by another door. A capability is nothing in the code, and the bound an operation
is checked against is written from the type at the call: a function written for `Fs("/tmp/x/a")`, handed as a value to one that
calls it with an `Fs("/tmp/x/a/b")`, opens files under `/tmp/x/a`.

Measured, each a program of fifteen to forty lines, `cancho check` and then `cancho run`:

| the program | before | after |
|---|---|---|
| `takes_bool` passed for `fn(int) -> [] int` (the one above) | accepted; runs, answers 1 | `type-mismatch` |
| the same through `let f: fn(int) -> [] int = takes_bool;` | accepted; runs | `type-mismatch` |
| a `fn(int) -> [] bool` passed for `fn(int) -> [] int` | accepted; runs | `type-mismatch` |
| a function of two parameters passed for `fn(int) -> [] int` | accepted; runs | `type-mismatch` |
| `wide[&m](fs: &m Fs("/tmp/x/a")) -> [fs_read("/tmp/x/a")] int` passed for `fn(&m Fs("/tmp/x/a/b")) -> [fs_read("/tmp/x/a/b")] int` | accepted; with the file present it opens `/tmp/x/a/other.txt`, outside the bound | `type-mismatch` |
| a function with row `[io_write]` passed for a type with row `[]` | accepted; writes, under a row that says nothing is written | `type-mismatch` |
| a function with row `[]` passed for a type with row `[io_write]` | accepted | `type-mismatch` |
| a parameter `f: fn(&!i Io, int) -> [io_write] int` passed on for a `fn(&!i Io, int) -> [] int` (two function types; their rows were not compared either) | accepted | `type-mismatch` |
| a function over `&r [byte]` passed for `fn(&!r [byte]) -> [] int` | accepted | `type-mismatch` |
| a function with `where b <= a`, passed where the regions it is given do not keep it (section 2.5) | accepted; the direct call is `reference-escapes-region` | `reference-escapes-region` |
| `spawn(5, takes_bool)` | accepted; runs | `type-mismatch` |
| `spawn(5, add)`, `add` of two parameters | accepted; runs | `arity-mismatch` |
| `tests/accept/function_value.cho`, and a function passed for the type its signature writes | accepted | accepted |

## 2. The rule

**A function named as a value has the type its signature writes**: `fn(P1, ..., Pn) -> [row] R`, with the parameters, the row and
the result of its declaration. Where that value meets a function type (an argument, an annotation, a field, a result) the two are
compared as two function types are, and a difference is `type-mismatch`.

### 2.1 What the Rust front end did

This is not designed here; it is recovered. The Rust front end is in the history of the repository (the parent of `14d586eb`, #802),
and its record in the tree is [`function-values.md`](function-values.md) section 4.2 and the header of
`tests/accept/function_value.cho`. Three places:

- **The value** (`crates/cancho-ir/src/lower/expr.rs`): a name that is a function with no type parameters is
  `Type::Fn(params, labels, ret)`, the parameters and the result with each region parameter replaced by a **fresh region variable**,
  "instantiated exactly like a call's region parameters, solved by whatever the value's own type is checked against next", and the
  labels copied from the signature, name and argument.
- **The comparison** (`crates/cancho-types/src/lib.rs`, `unify`): two `Fn` types unify when they have the same number of parameters
  **and equal rows**; then the parameters and the result are unified. "The row is written and exact, never inferred across this
  boundary, so it is compared as plain data rather than unified -- two `Fn` types with different rows are simply two different
  types." Anything else falls to `Mismatch`, which is `type-mismatch`: a different count of parameters is a mismatch of types and
  not `arity-mismatch`.
- **`spawn`** (`crates/cancho-ir/src/lower/conc.rs`): the body must take exactly one parameter (`arity-mismatch`), and the payload
  is held to it by `expect_type` (section 4).

The recorded answers (`tests/vectors/`) hold nothing against this: of the 3,402 edge cases of `tests/vectors/frontend/cases`, 12
write a function type, and none passes a function for a type it is not. That is how the gap went unseen when the oracle was
replaced by its recordings.

### 2.2 The parameters and the result: unified, no coercion

Each parameter and the result are unified (`types.unify`), which is equality: at a call a `&!` passes for a `&`, and a reference
passes for one of a shorter region, because the argument is a value that is used once there. Inside a function type neither holds.
A function that takes `&r [byte]` is not a `fn(&!r [byte])`: the two are different types as the referents of two references are
(`types.expect_type`: "the referent is invariant"). Passing a function for a type whose parameters it would accept by coercion is
sound, and it is not what the Rust front end did; it would be a subtyping rule for functions with its own variance, and no program
asks (section 5).

A capability in a parameter is compared as `capability-literals.md` says, so its literal is part of the comparison. That is what
closes the fifth row of the table.

### 2.3 The row: the same set of labels

Two rows are the same when each label of one is a label of the other (`pass1.same_row`), whether one of them is a named function's
or both are written in function types: the same name and, for a label that
carries an argument, the same argument, spelled the same. The order they are written in does not matter, which is what the Rust
front end's sorted list gave.

- **Exact in both directions.** A function that performs more than the type says is the unsound one: the caller's row is checked
  against the type (`effects.perform_pointer`), so the extra label would be performed under a row that does not declare it. A
  function that performs *less* is sound and refused too: a function's own row is exact in both directions
  (`linearity-and-effects.md` section 7.3), the authority report of the caller would say it performs what it cannot, and
  `function-values.md` section 4.3 settled that a fixed-row function type accepts one row. Accepting a subset is row subtyping, and
  [`effect-polymorphism.md`](effect-polymorphism.md) answered that no.
- **A label's argument is compared by spelling**, for every label: `fs_read("/tmp/x/a")` is not `fs_read("/tmp/x/a/b")`, and
  `ffi("libc,libm")` is not `ffi("libm,libc")`. The capability types beside them are compared as sets for `Ffi` and `Signals`
  (`capability-literals.md` section 3), so a function whose row writes a set in another order than the type's is refused here though
  its parameters agree. That errs on the side of refusing, it is what the Rust front end's string comparison did, and no program of
  the repository passes such a function (section 5).

The row of a function type is kept in the type as the token of its `[` (`types.fnptr_row`), and `types.unify` now takes the text of
the program to read it. `capability-literals.md` section 3 gave unify tokens to compare because it had no text; the text was always
at hand in its one caller (`types.expect_type`), and is passed down.

### 2.4 Region parameters: variables, made where the value is taken

A function with region parameters can be a value (`function-values.md` section 4.2; one with type parameters cannot,
`no-function-values`). Each of its region parameters becomes a region variable **at the expression that names the function**
(`atoms.value_type`), and the first type the value is compared with solves them. So:

```
fn both[&a, &b](s: &a [byte], t: &b [byte]) -> [] int {
    return apply(s, first) + apply(t, first);      // accepted: two values, one for each region
}

fn one[&a, &b](s: &a [byte], t: &b [byte]) -> [] int {
    let f = first;
    return apply(s, f) + apply(t, f);              // region-mismatch: `f` is a function over `&a` after the first call
}
```

This is the Rust front end's rule, and its consequence for a value kept in a name was the same there. A value in a name that is
*called* (`f(s)`) is still checked as a call of the function it names, with fresh variables at each call, as before.

### 2.5 `where` clauses: shown where the value is given its type

`fn longer[&a, &b where b <= a](x: &a [byte], y: &b [byte]) -> [] &b [byte]` may answer `x` as a `&b` because every caller shows
that `a` outlives `b` (`reference-escapes-region` at the call otherwise). A function type has no `where`. So a function that calls
`longer` through an `f: fn(&p [byte], &q [byte]) -> [] &q [byte]` shows nothing, and after sections 2.2 to 2.4 alone the program
that passes `longer` for that type would still be accepted, with `p` and `q` unrelated: a reference returned for longer than what
it points at lives. Measured before the check below was added: accepted.

The Rust front end did not check this either (nothing in the three places of section 2.1 reads `where_clauses`); it is closed here
because it is the same evasion. **Where a function value is compared with a function type, its `where` clauses must hold of the
regions that comparison solved** (`types.where_holds`), by the rule of a call and with its refusal, `reference-escapes-region`.
The one passing the function is the one who knows both regions, as the caller is at a call.

One limit, on the side of refusing: a region nothing has solved yet keeps nothing. Arguments are checked left to right, so if the
function comes before the arguments that fix the regions of its type (`choose(longer, x, y)`), the clause is refused though it
would hold. Two functions of the repository outside `tests/reject` declare a `where` at all (`examples/tour.cho`,
`tests/accept/nested_regions.cho`), and neither is used as a value, so nothing meets this limit today.

## 3. What is not compared, and why it need not be

- **The region parameters themselves, by name or count.** They are solved, not compared: a function over two regions passes for a
  type that names one region twice, as it could be called with one region twice.
- **Type parameters.** A function with any is not a value (`no-function-values`), so no function value has them.
- **Mode.** A function value is `val` and captures nothing; a `res` parameter is part of the parameter's type and compared there.
- **Two function values with each other.** `types.unify` still has no arm for a named function met where a *named function* is
  expected (two arms of a `match` that answer two functions, a type parameter solved by one function and then given another): that
  stays `type-mismatch`, as before, including for the same function twice. It refuses more than the Rust front end did and admits
  nothing.

## 4. `spawn`

`spawn(payload, f)` runs `f` on the payload in a thread. The Rust front end held `f` to one parameter and the payload to its type
(section 2.1). The port read the result of `f` from its declaration and compared nothing else: `spawn(5, takes_bool)` was accepted
and ran. Now (`exprs.thread_call`): a function of any other number of parameters than one is `arity-mismatch`; the payload is held
to the parameter by `expect_type`, so the coercions of a call apply, as they did in Rust; and the `where` clauses are held as in
section 2.5.

## 5. What stops checking

Every `.cho` file git tracks outside `tests/reject` was checked with both compilers, alone where it is a program of one file
(`cancho check FILE --std`; a script over `git ls-files '*.cho'`, the method of `capability-literals.md` section 5), and the larger
programs by the gate, which builds and runs them.

| | programs | stop checking |
|---|---:|---:|
| one file, checked alone: `tests/accept` 132, `tests/programs` 117, `examples` 31, `benches` 29, `tests/probes` 3, `scripts` 2 (the other 440 of the 754 are modules of larger programs and are refused alone by both compilers, with the same status each) | 314 | **0** |
| gate stage `walk`: `tests/accept` and the examples with an expected output, built and run | all | **0** |
| gate stage `tests`: the `cancho test` runs and the scripts of `tests/commands`, which build `std`, `packages`, the examples and the compiler | all | **0** |
| the compiler itself (`scripts/build-standalone.sh`, three stages, stage 2 equal to stage 3) | 1 | **0** |

No program had to be changed. Why none: almost nothing passes a function for a type.

| tree | function types written (`fn(` in a type position) | calls of `spawn` |
|---|---:|---:|
| `compiler`, `std`, `packages` | 0 | 0 |
| `examples` | 0 | 9 |
| `benches` | 0 | 10 |
| `tests/accept` | 2 (both in `function_value.cho`) | 26 |
| `tests/programs` | 0 | 6 |
| `tests/commands` | 1 (in a program `compile_cli.sh` writes to be refused) | 1 |

`tests/accept/function_value.cho` is the one program of the repository that passes a function for a function type, twice, each for
the type its signature writes; the Rust front end held it to that when it was written. The 52 calls of `spawn` each give the
function a payload of its parameter's type, for the same reason. Outside the repository,
[cancho-os](https://github.com/alpibrusl/cancho-os) writes no function type and calls no `spawn` (the same search over its sources).

## 6. What it costs

A function value now carries a type made where it is named (its parameters, its result, and the function type of them), and the
comparison reads two rows token by token. `types.unify` takes one more argument everywhere.

**The compiler compiling itself**, which writes no function type, so this is the cost of the extra argument alone (the 114 files
of `compiler/cancho.files` with `--std`, 71,262 lines; user time in seconds, five runs each, alternating):

| | run 1 | 2 | 3 | 4 | 5 | median |
|---|---:|---:|---:|---:|---:|---:|
| `cancho check`, before | 3.23 | 3.16 | 3.17 | 3.24 | 3.17 | 3.17 |
| `cancho check`, after | 3.20 | 3.21 | 3.20 | 3.22 | 3.26 | 3.21 |
| `cancho build` (the check, the code and `clang`), before | 13.29 | 13.19 | 12.90 | 12.79 | 12.81 | 12.90 |
| `cancho build`, after | 13.39 | 12.88 | 12.83 | 12.88 | 12.83 | 12.88 |

The medians of the check differ by 0.04 s (1.3%), inside the spread of each row (0.08 s and 0.06 s); the build does not differ. Both
compilers read the same sources, those of this change.

## 7. What this does not close

- **A row is not held to its argument** except for `mmio` (`capability-literals.md` section 6): a function may declare
  `fs_read("/a")` while holding `Fs("/b")`. Unchanged here; what is new is that a function type's row and a function's row must be
  the same label, whatever it says.
- **A function value that is not a function of the module it is named in** is still not a value the port checks (`SKIP`, which a
  build refuses): `atoms.name_expr` looks in the module being checked.
- **A function type of more than three parameters** is still not one the port writes code for (`SKIP`), so a function of four or
  more can be a value only for a name that is called.
- **A clause that would hold, refused for the order of the arguments** (section 2.5).

## 8. Where it is

| | |
|---|---|
| the type of a function value | `atoms.value_type`; `types.make_fnval`, `types.fnval_signature` |
| the row of a function type | `types.make_fnptr`, `types.fnptr_row`; written by `scope.fn_type` |
| the comparison | `types.unify` (the arm of function types), `pass1.same_row` |
| `where` clauses | `types.where_holds`; status 4 of `types.unify`, reported as `reference-escapes-region` |
| `spawn` | `exprs.thread_call` |
| tests | fourteen fixtures of `tests/reject`: `function_value_other_parameter`, `function_value_other_result`, `function_value_other_count`, `function_value_annotated`, `function_value_row_wider`, `function_value_row_narrower`, `function_value_other_label_argument`, `function_value_passed_on_other_row`, `function_value_wider_capability`, `function_value_shared_for_unique`, `function_value_two_regions`, `function_value_where_not_kept`, `spawn_payload_other_type`, `spawn_function_two_parameters`; `tests/accept/function_value_signature.cho` and `tests/accept/function_value.cho` for what is still accepted |

Each of the fourteen is accepted by the compiler before this change (measured: all fourteen, `cancho check --output json` answers
`"refused": []`), so each could have failed. Their lines in `tests/reject.golden` were not recorded from the Rust front end, which
no longer exists to ask: the rule and the position are the compiler's in cancho, and the message is written in the Rust front
end's wording (`Unifier::display` of a function type, read from the history).
