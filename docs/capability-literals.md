# A capability's literal is part of its type

> **Status: built.** Found on 2026-11-25 while adding `Mmio` ([`mmio.md`](mmio.md) section 3, which recorded it as a hole and closed it
> for `Mmio` alone). This closes it for every other capability that carries a literal: `Fs`, `Net`, `Out`, `Ports`, `Exec`, `Tty`,
> `Camera`, `Mic`, `Speaker`, `Signals` and `Ffi`. Every number below was produced by the command next to it, on darwin-aarch64, with
> two compilers built by `scripts/build-standalone.sh`: **before** is the commit that added `Mmio`, **after** is this change.

---

## 1. The hole

```
edition 5;

fn wide[&m, &i](fs: &m Fs("/tmp/x/a"), io: &!i Io) -> [fs_read("/tmp/x/a"), io_write] int {
    match open_read(fs, "/tmp/x/a/other.txt") { ... }
}

fn main(world: World) -> [] int {
    ...
    let n = narrow(fs, "/tmp/x/a/b");
    borrow n as &r in { borrow mut o as &!w in { status = wide(r, w); } }
    ...
}
```

`main` cuts its `Fs` down to `/tmp/x/a/b` and lends that. `wide` is declared to take `/tmp/x/a`. Before this change the program was
accepted, and with the file present `wide` opened `/tmp/x/a/other.txt` and read it: a file outside what `main` lent.

Two facts of the compiler made it so:

- `types.unify` compared two prelude types by their index (`Fs` is `Fs`) and not by the literal kept beside it;
- a capability is nothing in the code, so the bound an operation is checked against is written from the **type at the call**
  (`atoms.push_bound`): inside `wide` that is `/tmp/x/a`, whatever was passed.

It is a gap of the checker written in cancho and not of the language. The Rust front end, which did the checking until it was
deleted, compared the literals: [`foreign-authority.md`](foreign-authority.md) section 5.2 calls lending an `Ffi` narrower "the only
coercion between capability types", [`narrowing-into-several.md`](narrowing-into-several.md) section 4.5 says the same, and
`tests/commands/foreign_authority.expected` recorded three lines of `GAP: accepted, the Rust checker said refused` for it.

Measured before, each a program of about twenty lines run with `cancho run`:

| the program | before | after |
|---|---|---|
| the one above (`Fs("/tmp/x/a/b")` lent as `Fs("/tmp/x/a")`) | accepted; opens the file | `type-mismatch` |
| `Fs("/tmp/x/a/b")` lent as `Fs("")` | accepted; opens the file | `type-mismatch` |
| `Fs("/tmp/x/a")` lent as `Fs("/tmp/x/a/b")` (wider for narrower) | accepted; the open traps, since the check is the callee's | `type-mismatch` |
| `Ffi("libpthread,libc")` lent as `Ffi("libm")`, which then calls a function of `libc` | accepted; calls it | `type-mismatch` |
| `Fs("/tmp/x/a")` lent as `Fs("/tmp/x/a")` | accepted | accepted |
| `Ffi("libc,libm")` lent as `Ffi("libm,libc")` | accepted | accepted |
| a struct field `fs: Fs("/tmp/x/a")` given an `Fs("/tmp/x/a/b")` | accepted | `type-mismatch` |

## 2. The rule

**Two capabilities of one kind are the same type when their literals say the same thing**, wherever a type is compared: an argument, a
field, a result, an assignment. A difference is `type-mismatch`.

### 2.1 What `split` gives is `Fs("")`

The capability `split` gives has no literal, and a signature cannot write one without (`Fs` alone is `arity-mismatch`); what it writes
for "every file" is `Fs("")`. These are one type: a type with no literal and a type whose literal is two quotes compare equal
(`types.bound_key`). The code already treated them alike (`atoms.push_under`: "a literal of two quotes is the bound that bounds
nothing, as no literal at all is").

This was measured, because the other choice was open: of the 256 capability types the compiler's own sources write, 249 are `Fs("")` or
`Exec("")`, all of them lent the capability of `split`. Were the two different types, the compiler would not check itself.

### 2.2 The same literal, in both directions

A narrower capability passing for a wider parameter is the hole. The other direction, a wider one passing for a narrower parameter, is
refused too, and that was a decision:

- **It is not unsound.** Measured in section 1, third row: the callee's operations are checked against the callee's literal, so nothing
  outside it is reached, and the report names the narrower path. `narrowing-into-several.md` section 4.5 said so when it considered
  this as a coercion and put it off.
- **It needs a `covers` for each capability inside the type checker**: a path prefix for `Fs`, `Exec` and the devices, the rule of
  `host:port` for `Net`, `Out` and `Ports` (which changed at edition 10), a set for `Signals`. Each is a place to be wrong about what
  is inside what, and being wrong there is this hole again.
- **Nothing asks for it.** No program of the repository lends a wider `Fs`, `Net`, `Exec` or device for a narrower one (section 5: none
  stops checking). The bar is two askers.
- **A program says what it lends where it narrows.** `main` writes `narrow(fs, "/tmp/x/a/b")` and that literal is the one the function
  it lends to must name. With the coercion the paths would live in the signatures of the callees alone.

So a function that holds `/tmp/x` and wants to lend `/tmp/x/a` narrows to it. A function that only borrows `/tmp/x` cannot (`narrow`
consumes), and lends what it has: its callee is written for `/tmp/x`. Lending a narrower view of a borrowed path remains the follow-up
`narrowing-into-several.md` names.

### 2.3 The one coercion: an `Ffi` over several libraries

A reference to an `Ffi` over several libraries is accepted where a reference to one over **some of them** is wanted
(`types.lent_narrower`, in the reference arm of `types.expect_type`, where the Rust front end had it). Never from the capability of
`split`, never for an owned `Ffi`, and never the other way.

This is not new, it is the rule of `foreign-authority.md` section 5.2 with its reason: an `Ffi` narrows to a set in **one**
capability (`narrow(ffi, "libc,libm")`; `narrow` into several refuses `Ffi`), and a foreign function borrows exactly one library, so
without lending a program over two libraries could call neither. `tests/accept/foreign_two_libraries.cho` is the program that asks:
with exact equality alone it is the one fixture of the repository that stops checking.

An `Ffi` literal is a set written in any order, so `Ffi("libm,libc")` and `Ffi("libc,libm")` are one type, and so it is for
`Signals` (section 3).

### 2.4 A builtin takes any literal

`tty_open` and the six of `Mmio` are the builtins whose signature in `compiler/signatures.cho` names a capability that carries a
literal (measured: `scripts/builtin_tables.py show` over every name of `tests/builtins.golden`; none takes one by value and none
answers one). The capability such a builtin borrows is a wildcard (`prims.decode`, -1 in the literal slot), and it is checked against
the bound of whatever it was given. The other builtins that take a capability (`open_read`, `connect`, `exec_spawn`, ...) are checked
by hand and already took any literal. `release` and `narrow` are not in the table.

A wildcard exists only behind the reference of a builtin's parameter. A field of `Split` is the capability with no literal, as before.

## 3. How two literals are compared

`types.unify` has the state of the checker and not the text of the program, so it compares tokens and not bytes. Every literal of a
capability is therefore rewritten, where it is resolved, to **the first string token of the program that says the same**
(`pass1.first_same`):

- where a type is written (`pass1.resolve_name`), for every prelude type with a literal;
- the literals of `narrow` (`narrowing.refused`), which become the type of what it makes.

"Says the same" is the same spelling (`pass1.first_spelled`, which `Mmio` introduced), except for `Ffi` and `Signals`, where it is the
same set of names in any order. Then two types are the same bound exactly when they keep the same token.

One limit, on the side of refusing, and one thing measured:

- **A literal with an escape is another spelling.** `Fs("/tmp/\x61")` and `Fs("/tmp/a")` are different types. No program of the
  repository writes an escape in one.
- **A set is compared as a set only for `Ffi` and `Signals`**, the two whose literal is one. `tests/commands/signals.sh` holds
  `Signals("INT,TERM")` to being the type of `narrow(signals, "TERM,INT")`, and it was the script that said so when the first
  version of this change compared `Signals` by spelling.

## 4. What it costs

`first_same` reads every earlier token of the program for each literal it places, so the cost grows with the number of capability
literals times the size of the program.

**The compiler compiling itself** (`cancho check` of the 114 files of `compiler/cancho.files` with `--std`: 71,123 lines, 256 capability
types and 3 calls of `narrow`), user time in seconds, five runs each, alternating:

| | run 1 | 2 | 3 | 4 | 5 | median |
|---|---:|---:|---:|---:|---:|---:|
| before | 3.14 | 3.08 | 3.06 | 3.05 | 3.08 | 3.08 |
| after | 3.23 | 3.10 | 3.08 | 3.05 | 3.06 | 3.08 |

No difference that five runs can show.

**The worst case, made on purpose**: a file of N functions, each taking an `Fs` of its own literal, so every scan runs to the end
(`cancho check`, user time in seconds):

| N (functions, each one literal) | before | after |
|---:|---:|---:|
| 1,000 | 0.01 | 0.02 |
| 4,000 | 0.17 | 0.26 |
| 16,000 | 2.74 | 4.39 |

The scan is quadratic and it shows only where the checker already is: at 16,000 functions the checker without it takes 2.7 seconds,
and the scan adds 1.7. The largest count in the repository is the compiler's 256. A table of spellings would make
it linear; nothing asks for it yet.

## 5. What stops checking

Every `.cho` file git tracks outside `tests/reject` was checked with both compilers, alone where it is a program of one file
(`cancho check FILE --std`: 314 of 754 files are accepted so by the compiler before, the rest are modules of larger programs), and the
larger programs by the gate, which builds and runs them.

| | programs | stop checking with exact equality alone | with the rule of section 2 |
|---|---:|---:|---:|
| one file, checked alone: `tests/accept` 132, `tests/programs` 117, `examples` 31, `benches` 29, `tests/probes` 3, `scripts` 2 | 314 | 1 | **0** |
| gate stage `walk`: `tests/accept` (132) and the examples with an expected output (16), built and run | 148 | 1 (in `tests/accept`; the walk stops at the tree that fails, so the examples were run only with the rule) | **0** |
| gate stage `tests`: the `cancho test` runs and the 56 scripts of `tests/commands`, which build `std`, `packages`, the examples and the compiler | all | 1 (run with the `Ffi` coercion already in, and `Signals` compared by spelling) | **0** |
| the compiler itself (`scripts/build-standalone.sh`, three stages) | 1 | 0 | **0** |

The one is `tests/accept/foreign_two_libraries.cho`, which lends `Ffi("libpthread,libc")` to a function that wants `Ffi("libc")`: the
coercion of section 2.3, which is why it is in the rule. Stage `tests` was first run with that coercion in and
`Signals` compared by spelling, and one script differed: the program of `tests/commands/signals.sh` that narrows to `"TERM,INT"` and
lends it as `Signals("INT,TERM")` (section 3). With the rule of section 2 both are accepted. No program had to be changed.

Outside the repository, [cancho-os](https://github.com/alpibrusl/cancho-os) at `5455544` (the kernel that asked for `Mmio`, and the two
programs of its simulator) checks with both compilers.

Why so few: the literals of the repository are counted below (a regular expression over the tracked files). Almost every one is the
capability of `split` written as `""`, lent to a parameter written the same; and a program that narrows passes what it narrowed to a
function written for that literal, because the Rust front end held it to that when the program was written.

| tree | capability types with a literal | of them `("")` | calls of `narrow` |
|---|---:|---:|---:|
| `compiler` | 256 | 249 | 3 |
| `tests/commands` | 487 | 487 | 12 |
| `examples` | 177 | 18 | 37 |
| `tests/programs` | 92 | 39 | 37 |
| `tests/accept` | 56 | 7 | 32 |
| `tests/support`, `tests/run.cho` | 53 | 53 | 0 |
| `packages` | 24 | 0 | 0 |
| `scripts` | 15 | 15 | 0 |
| `benches`, `tests/probes` | 9 | 1 | 3 |
| `std` | 3 | 1 | 0 |

**What changed in the recorded answers.** `tests/commands/foreign_authority.expected` loses four lines of `GAP` (an `Ffi` lent a
library it lacks, three ways, and the root lent): the compiler in cancho now says what the Rust checker said. The counts of fixtures in
`cli_messages.expected` and `refusals.expected` go from 400 to 411.

## 6. What this does not close

- **A function passed as a value is held to the type it is passed for**, and this hole is no longer reachable through one
  ([`function-signatures.md`](function-signatures.md)). *(Corrected in place: this item said `types.unify` accepts any named function
  where a `fn(...)` type is wanted, so that a function over `Fs("/tmp/x/a")` could be handed to one that calls it with an
  `Fs("/tmp/x/a/b")`, and that the same compiler accepts a `fn(bool) -> int` where a `fn(int) -> int` is wanted. Both were measured
  true after this change and are `type-mismatch` since that one.)*
- **A row is not held to its argument**, except for `mmio` ([`mmio.md`](mmio.md) section 5). A function may still declare
  `fs_read("/a")` while holding `Fs("/b")`.
- **Lending a narrower view of a borrowed path** (section 2.2).

## 7. Where it is

| | |
|---|---|
| the comparison, the literal of two quotes, the `Ffi` coercion | `types.unify`, `types.bound_key`, `types.lent_narrower` |
| the one token of a literal | `pass1.first_same`, `pass1.first_spelled`, `pass1.set_within`; applied in `pass1.resolve_name` and `narrowing.refused` |
| a builtin's parameter | `prims.decode` |
| tests | eleven fixtures of `tests/reject`: `fs_wider_parameter`, `fs_narrowed_for_root`, `fs_root_for_narrowed`, `exec_wider_parameter`, `net_other_bound_parameter`, `out_other_bound_parameter`, `ports_other_bound_parameter`, `tty_wider_parameter`, `signals_other_set_parameter`, `ffi_lent_a_library_it_lacks`, `ffi_root_lent`; `tests/accept/foreign_two_libraries.cho` and `tests/commands/foreign_authority.sh` for what is still accepted |

Each fixture is accepted by the compiler before this change (measured: all eleven), so each could have failed.
