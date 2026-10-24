# A capability with a numeric budget

> **Status: proposed, not built.** Nothing in the compiler changes in this
> document's PR. Answers alpibrusl/cancho#612, asked for by
> [cancho-quantum](https://github.com/alpibrusl/cancho-quantum) (`docs/qubit-types.md`
> §5 item 6 and milestone M4): a program's text says which quantum backend it can
> reach and how many shots it may spend.
>
> **Measured** (compiler `dd0f38f`, `cancho authority` and `check` on the probes
> in §1 and §3): what the report prints for a bound and for a loop; what
> narrowing refuses today for paths and ports; what the unnarrowed network says.
> **Predicted, not run**: every `qpu_*` line below, because no `Qpu` exists. They are marked.
>
> **The answer in one sentence.** The *bound* is decidable from the text and the
> report can print it; the *consumption* is not, and the honest claim is **a
> declared upper bound, enforced by the capability's own counter and compared by
> a supervisor with its grant. It is not a proof that a program's total is at most
> n.** The compiler can refuse the cases that are literal (§4), and nothing more.

---

## 1. Question 1, measured: what does `authority` print for a count?

Every bound in `authority`'s vocabulary today is a **string taken from the
capability's type** (`granted_prefix`, `narrowing-into-several.md` §3), checked at run
time on each operation. The report never counts operations. Two probes, one loop each:

```
fn store[&f](fs: &f Fs("/tmp/budget-out.txt")) -> [fs_write("/tmp/budget-out.txt")] int {
    var i = 0; var last = 0;
    while i < 3 {                       // loop_literal.cho
        last = fs_write(fs, "/tmp/budget-out.txt", "x");
        i = i + 1;
    }
    return last;
}
```

`loop_runtime.cho` is the same function with `while i < n`, `n` read from standard
input (`printf 7 | cancho run`: seven writes). Both print:

```
$ cancho authority loop_literal.cho --std
performs
    fs_write("/tmp/budget-out.txt")
$ cancho authority loop_runtime.cho --std --output json     # labels only
  "labels": [
    { "name": "fs_write", "argument": "/tmp/budget-out.txt", "bounded": true },
    { "name": "io_read", "argument": null, "bounded": true }
```

**Three writes and seven writes are the same report: one label, no count.** So today
"how many" is not something the report can say; a numeric budget needs a *new field*,
not a new reading of an old one.

Where a bound comes from, and what happens when the program's own value strays from it:

| Probe | `check` | `authority` | run |
|---|---|---|---|
| `narrow(fs, "/tmp")`, then `fs_read(fs, "/etc/hostname", buf)` with the path passed in | accepted | `fs_read("/tmp")` | **exit 3**: the program is stopped; `filesystem.md` §1 says a read outside the capability's prefix traps |
| `Out("")` (unnarrowed), then `tcp_connect` | accepted | `"bounded": false`, `unbounded_by: ["net_out(\"\")"]`, and `UNBOUNDED: this program holds the network without narrowing it.` | |
| `narrow(ports, "5353")`, then `narrow(.., "80")` | **refused `capability-not-narrowable`**: *`5353` cannot be narrowed to `80`: a `Net` bound's port is exact … `80` admits what `5353` does not, which would widen it* | | |
| `narrow(fs, "/tmp/a")`, then `narrow(.., "/tmp")` | **refused `capability-not-narrowable`**: *a capability is attenuated, never widened* | | |
| `narrow(.., "/tmp/a")` of `Fs("/tmp/a")` | **refused `capability-not-narrowable`**: *narrows `/tmp/a` to itself, which grants nothing new* | | |
| `narrow(fs, "/tmp", "/tmp/a")` | **refused**: *`narrow` into several capabilities needs unrelated paths, and `/tmp` and `/tmp/a` are nested* | | |
| `narrow(ports, "5353")` then `narrow(ports, "80")` | **refused `linear-use-after-move`** | | |

Three things carry over. The bound is **attenuation-only** and checked where it is
written, as a literal. An unnarrowed root reports `bounded: false` with a reason
(`net-bounds.md` §2.4). And a value that strays from the bound at run time is not a
check-time matter: the program is stopped.

The closest thing to a number in the corpus is `net_in("5353")`: a port, but compared
by equality, so it has no order. **There is no existing bound that is an integer
which goes down.** That is the gap this design fills.

### 1.1 How `budget.md` answers part of this, and where this differs

`budget.md` says no to `[budget]` in the type system. Its reasons, against a `Qpu`:

| `budget.md` | Applies to a shot count? |
|---|---|
| §2: wall-clock, cents, commands, API calls are not properties of source | **Mostly not.** A shot count is an *argument the program writes* to one operation (`qpu_run(q, c, 1000)`). It is a count of that operation, not a cost |
| §3: dynamic fuel taxes every operation, in every program | **No.** The check is inside one builtin and a program that holds no `Qpu` never runs it |
| §3: a static sum needs a cost model | **Yes, and §4 does not do it.** The compiler refuses only a literal over the bound; it does not sum |
| §4.1: the number must come from the embedder, not the program | **Agreed for the root; not for attenuation.** The root capability is unlimited as `Fs("")` is; a program may only *narrow* it, with a literal, which is the declaration (`authority.md` §1). The supervisor's grant is what limits the root (§5) |

So this is a narrow reversal of `budget.md` §4: a counted capability, gating one
mediated operation, narrowed by literals. `budget.md` §6's "Open" table should point
here if this is accepted.

---

## 2. The design in one page

A new capability `Qpu(backend, n)`, handed out by `split` (an edition's addition; its
number is not fixed here, as in `net-bounds.md` §2.1). `n` is **an upper bound on the
shots this capability may ever submit**, and the unnarrowed root has none.

```cho
// PROPOSED. None of this compiles today.
fn run[&q, &c](qpu: &!q Qpu("sim", 100), circuit: &c [byte]) -> [qpu_shots("sim", 100)] int {
    match qpu_run(qpu, circuit, 40) {          // 40 shots
        Ran::Ok(counts) => { return counts; }
        Ran::Exhausted(left) => { return 0 - left; }
    }
}
```

* **Row label:** `qpu_shots("sim", 100)`. The backend is the string argument (as
  `fs_read`'s path is); the limit is a second, numeric part of the label.
* **The operation:** `qpu_run(qpu, circuit, shots)` takes `&!` because the capability
  carries a counter. No existing capability does, as far as I could find: every check
  is against the type. This is the largest language change and §6 asks whether it is
  worth it.
* **At run time** each call subtracts `shots` from what is left. If `shots` is more
  than is left, **nothing is submitted** and the call answers `Ran::Exhausted(left)`.
  This differs on purpose from `Fs` (§1: out of bound stops the program, exit 3):
  spending a budget to zero is the *expected* end of a loop, not an attack, so it is
  a value the program matches on, as `Dialed::Failed` is.
* **What the bound means:** through this capability, the sum of accepted `shots` is
  at most `n`, always, by construction. It does *not* mean the program uses `n`.

### 2.1 Why `shots` and not Net's call count

`Net` could carry a count of `tcp_connect`s, reusing the existing capability. It would
not help: a cloud QPU API takes the shot count **inside a request body**, which
`tcp_connect` cannot see, and `net-bounds.md` §2.3 compares hosts as written without
reading traffic. The unit that must be bounded is an argument of a domain operation,
so the operation has to be the gate. A generic `Quota(name, n)` capability that the
program *debits itself* was also considered: it is unforgeable but **advisory**, because
nothing ties the debit to the submission, so it bounds nothing. Rejected for that reason.

The price of the chosen design is a domain builtin in a language that tries to have few.
§6 lists a cheaper variant (backend as a user-written library behind `Ffi`) and what it loses.

---

## 3. Narrowing a number

Today's `narrow` takes string literals and compares them with `extends_path` (paths) or
equality (ports, exact hosts). A count needs an **order**:

> **Rule N1.** `narrow(q, k)` of a `Qpu` bounded by `n` is accepted iff `k` is an
> integer literal and `0 < k < n`. The result is `Qpu(b, k)` and `q` is spent.

| | |
|---|---|
| accept | `let small = narrow(qpu, 100);` of the root, then `narrow(small, 30)` |
| reject, `k >= n` | `narrow(small, 100)` of a capability already at 50. **`capability-not-narrowable`**: *a budget is attenuated, never raised*. The measured analogues (§1 table): `5353` to `80`, `/tmp/a` to `/tmp` |
| reject, `k == n` | `narrow(small, 50)` of one at 50: **`capability-not-narrowable`**: *narrows to itself, which grants nothing new* (measured for a path) |
| reject, not a literal | `narrow(qpu, count)`: **`budget-not-literal`** (new). A bound checked where it is written must be written. Matches `narrow`'s literal-only rule |

So the question in #612: `narrow(qpu, 100)` of a capability already narrowed to 50
**means a request to raise the budget, and is refused**, exactly as `narrow(.., "/tmp")`
of `Fs("/tmp/a")` is.

### 3.1 Narrowing split across callees

`narrowing-into-several.md` §2: `narrow` consumes its capability, so **a program has one
chain of capabilities**, never two siblings, and `narrow` of a *borrow* is refused. That
rule holds unchanged: a callee that wants part of a budget cannot take it from a lent
`Qpu`. What it can do is be *handed* a child that `main` made. The built answer for
paths is several children from one `narrow` (`narrowing-into-several.md` §4.1), and it
requires the children to be **unrelated** (measured above: nested paths are refused).

For numbers that rule is *wrong*: two children of 60 from a parent of 100 are unrelated
in the way paths are, and together allow 120. The budget version needs a **sum**:

> **Rule N2.** `narrow(q, k1, k2, …)` is accepted iff every `ki` is a positive literal
> and `k1 + k2 + … <= n`. The result is a tuple `(Qpu(b, k1), Qpu(b, k2), …)`.
> Any remainder `n - sum` is not recoverable: the parent is spent.

| | |
|---|---|
| accept | `let (a, b) = narrow(qpu, 60, 30);` of a 100 (10 are lost; say so in a comment) |
| reject | `let (a, b) = narrow(qpu, 60, 60);` **`budget-split-exceeds`** (new): *60 + 60 is more than the 100 it splits* |

This keeps the one claim that matters: **the total of the leaves is at most the root's
bound**, so the report's number (§5) is a bound on the whole program. The lost
remainder is deliberate. Giving it back as a third child (`narrowing-into-several.md`
option 4, "carve") was rejected there because the remainder is not expressible in a
prefix lattice; for numbers it *is* (`n - sum`), so this is a convenience left for a
follow-up (§6), not a soundness gap.

### 3.2 Lending, and what the callee's row says

A function that takes `&!q Qpu("sim", 100)` should be callable with a `Qpu("sim", 60)`:
its bound is an upper bound on what it may spend, and the real remainder is smaller.
There is a precedent, `ffi_scope_attenuates`, a coercion that lends a narrower capability
where a wider is declared (`narrowing-into-several.md` §4, option 6). The cost:
`authority` reads the label's bound from the type *at the operation*
(`granted_prefix`), so a shared callee reports its **declared** 100, not the caller's 60.
The report **over-approximates**. That is sound for a bound and wasteful for a reader.

---

## 4. What the compiler can refuse, and what it cannot

Only what is local and literal:

> **Rule C1, `shots-exceed-bound`.** A `qpu_run` whose `shots` is an integer literal
> greater than the bound in the capability's type is refused.

```cho
// PROPOSED. Reject, `shots-exceed-bound`: 200 shots through a capability of 100.
fn bad[&q, &c](qpu: &!q Qpu("sim", 100), circuit: &c [byte]) -> [qpu_shots("sim", 100)] int {
    match qpu_run(qpu, circuit, 200) { Ran::Ok(n) => { return n; } Ran::Exhausted(l) => { return l; } }
}
```

Accept: the same with `40`. A literal within the bound is accepted even when three of them would exceed it: summing is §4.1.

### 4.1 Which programs can be bounded statically, exactly

| Program | Static total | By what |
|---|---|---|
| one `qpu_run` with a literal count | **exact** (that count) | C1 compares it with the bound |
| straight-line code, every count a literal | **summable** (the sum) | *not done in v1*; no analysis exists |
| a loop with a literal trip count (`while i < 3`) | **summable** in principle (3 × count) | *not done*: the compiler has no loop-bound analysis. §1 shows it prints the same report for 3 trips and 7 |
| a loop whose bound is read at run time (`printf 7`) | **not decidable** | the counter, at run time |
| a count that is a variable, a parameter, or a data-dependent value | **not decidable** | the counter |
| recursion | **not decidable** | the counter |

So the static part is **C1 and nothing else**. Everything else is the counter, and the
report's number is the *declared* bound, not a sum of what the code does. The static sum for
literal-only code would be a lint (`shots-sum-exceeds-bound`) and is left to §6.

---

## 5. What `authority` prints, and who enforces what

```
// PROPOSED output, for a program that narrows the root to 100 and runs 40.
$ cancho authority qrun.cho --std
performs
    qpu_shots("sim", 100)
never touches
    the network
    ...
```

```json
{
  "bounded": true,
  "unbounded_by": [],
  "effects": ["qpu_shots"],
  "labels": [
    { "name": "qpu_shots", "argument": "sim", "limit": 100, "bounded": true }
  ]
}
```

`limit` is a new optional integer on a label, absent on every existing one, so no
consumer of today's JSON breaks. The report is a *union of labels* (`authority.md` §2),
so a program with two children (`qpu_shots("sim", 60)` and `qpu_shots("sim", 30)`)
prints both and a consumer **sums per backend**: 90. A new top-level field would
repeat the sum, so one is not proposed (§6).

An unnarrowed root, like the unnarrowed network (measured, §1), is reported as unbounded
and says why:

```
// PROPOSED, by analogy with the measured `net_out("")` output.
UNBOUNDED: this program holds a Qpu without narrowing it. A qpu_shots("sim") can submit any number of shots; narrow it.
"bounded": false, "unbounded_by": ["qpu_shots(\"sim\")"]
```

### 5.1 Three enforcers, three jobs

| Layer | Question | Mechanism | Evidence |
|---|---|---|---|
| **Compiler** | Is this literal within the declared bound? Is every narrowing a real attenuation, and does a split fit? | `shots-exceed-bound`, `capability-not-narrowable`, `budget-split-exceeds`, `budget-not-literal` | §3, §4 |
| **The capability's counter** (the program's own process) | Does the program stay inside what it declared? | `qpu_run` refuses with `Ran::Exhausted`; *no counter, no submission* | predicted |
| **A supervisor** (`under-a-grant.md`) | Is the declared bound within what the grant allows? Is the real backend charged? | compares the report's per-backend sum with the manifest *before* running; charges per mediated command (`budget.md` §1's third layer) | the filesystem dimension is already decidable this way (`under-a-grant.md` §2) |

The program's own counter is **not** a defence against a hostile program: the counter is in
the program's process, behind a capability it cannot forge, which is as strong as the rest of the
capability model and no stronger. A grant that must hold against a program that *breaks* the
runtime is the supervisor's job (`budget.md` §4.1: "the charge happens before the effect runs").
So the claim, plainly:

> **A declared upper bound, enforced by the capability and compared by a supervisor with
> its grant. Not a type-level proof that the total of a program is at most `n`.** The compiler
> proves a literal is in bounds, that no narrowing raised the bound, and that a split fits. It
> cannot prove, for a loop whose count is read at run time, anything about how many shots
> are spent; the counter does.

---

## 6. Open questions, and what would falsify this

1. **Is a stateful capability acceptable?** `qpu_run` mutates a counter through `&!`.
   Every capability today is checked against its *type*. If a counter in a capability
   breaks an invariant of the checker (copying a `&!` is allowed: `aliasing.md` §1, so two
   copies of one pointer share the counter, which is what we want; a copy of the
   *value* would not), a probe of `&!`-copies of a capability is needed first.
2. **Is a domain builtin worth it?** A lighter option: the backend is a library over `Ffi`
   or `Net`, and `Qpu` is *only* a counted capability that library must debit. That is
   the advisory design of §2.1 and bounds nothing. The cheaper honest variant is to bound
   `Net` (calls) and leave shots to the supervisor, which then is the *only* shot
   enforcement. cancho-quantum's M4 gate ("exceeding granted shots is refused at check time")
   **would then not be met**; it is only met for literals.
3. **Where does the unit live?** A backend that charges per shot *and* per job needs two
   limits. This design has one. A second numeric part generalises badly; if two are common,
   `Qpu` should be a record, not a label.
4. **Smaller conveniences.** Giving back the remainder of a split (§3.1); a top-level
   `limit_total` (§5) if `lex-os`'s `CheckReport` grows a numeric field; static sums for
   literal-only code (§4.1), which needs a loop-trip analysis the compiler lacks and
   should wait for an asker whose program it would catch.
5. **Edition and the compiler in cancho.** A new capability is an edition and a `split`
   field (`editions.md` §7), and `examples/selfhost` must follow (`CONTRIBUTING.md`).

**What would falsify it.** (a) A probe where two `&!` copies of a `Qpu` each hold a *private*
counter, so the bound is the sum of copies: then the counter cannot live in the value.
(b) A shared callee (§3.2) whose over-approximated limit makes every report useless to a
supervisor in practice: then lending must carry the caller's bound, which needs
bound-polymorphism (`effect-polymorphism.md` found none, and none is proposed). (c) A
consumer that needs the *actual* shots spent from the report: that is a trace, not a
declaration, and `authority` cannot be it.

---

## 7. The asker, and what exists

`cancho-quantum` simulates states and noise on one CPU and has no backend to meter.
`cancho authority src/*.cho --std` on it (measured at `255c3e1`):

```
performs
    heap
    io_read
    io_write
never touches
    the filesystem
    the network
    the command line
    signals
    other programs
    foreign code
```

That is the property M4 wants, **for free, because the simulator is pure computation**; it
says nothing about a *bound*, because nothing in it can spend a budget. M4's own plan (a mock
backend behind the capability, then a refusal at check time of a literal over the grant, and
`authority --output json` showing the bound) is C1 plus §5's `limit`, and nothing else in this
document. Real hardware (M5) is out of scope here: its `Net` bounds are design-only
(`net-bounds.md`), so its report would be coarser.

---

## 8. If built: the suite

Accept: narrowing to 30 and splitting 60 + 30; a loop of ten 40-shot runs against 100 (three
accepted, then `Ran::Exhausted(20)`, nothing submitted). Reject, one fixture each:
`capability-not-narrowable` (raise and equal), `budget-not-literal`, `budget-split-exceeds`,
`shots-exceed-bound`. Conformance: §5's text and JSON, and `bounded: false` for the root.
