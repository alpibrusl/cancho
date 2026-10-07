# Foreign authority: what a program that calls C can reach, said symbol by symbol

Status: **built** (written before the code; sections 6 and 10 are what building it corrected and what it could not check).

## 1. Why

`cancho authority` prints `UNBOUNDED` for any program that makes a foreign call, and says one thing about it: *a library is not an authority domain* (`docs/under-a-grant.md`). That is
true, and it is the same sentence for a program that calls one harmless function and one that can call anything. Two real programs are affected:

* **`cancho-hooks` today** reads a file's mode with one call to libc's `statx` (cancho#243). Its report went from bounded to `UNBOUNDED` for that call.
* **`cancho-hooks` with `https` delivery** will call OpenSSL in-process, about 25 functions from `libssl` and `libcrypto` (`docs/tls-nonblocking.md` section 6; the spike's `examples/tls_nb/`
  reaches **37** symbols, one of them libc's `signal`). The report should say *which* 25 and *where they live*, because that list is the thing a reviewer reads and a CI pin diffs.

The product's claim is that the signature says what the program can do (`AGENTS.md` section 3). For a foreign call the signature can say exactly one fact, which symbol, and it did not.
It said `ffi("libc")`, a label naming a library the program had *claimed*.

## 2. What the code did, measured

Everything below marked *confirmed* was run on the unmodified `main` (`32bad9c`) with `cancho check` / `authority` / `run`, both backends where it matters; the rest was read from the checker's source.

| | measured |
|---|---|
| An `extern fn` is `extern fn getuid[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] c_int;`. Its Lex name **is** the linked symbol (`docs/foreign-linking.md`, `gaps/g11`), so the set of foreign symbols a program can reach is a **closed set in its text**: the `Callee::Extern` call sites of the reachable functions (`docs/authority.md` section 3 pruned it to that). | yes: `foreign_symbols` already lists it |
| The scope in `Ffi("libc")` is a **label**: libc's `system` declared under `Ffi("openssl")` checks, builds and runs (`examples/tls_nb/gaps/a2_scope_is_nominal.cho`). | confirmed |
| `narrow` consumes the one `Ffi` that `split` hands out, so **a program has one scope**: a function taking `Ffi("libc")` and `Ffi("tls")` can never be called (`gaps/g12`). `net.sockets`/`net.connect` hard-code `"libc"`, so OpenSSL and the socket packages could not share a program, and the TLS spike named its scope by *purpose* (`"tls"`) and put libc's `signal` under it. | confirmed (gap 10) |
| A **program with two libraries** therefore could not be written, so the report could never say "per library". | follows |
| **A foreign function needs no capability at all.** `extern fn system[&c](command: &c [byte]) -> [] c_int;` with no `Ffi` parameter and the row `[]` was accepted. Called, it ran a shell on both backends, and the report said `"bounded": true`, `performs nothing`, *never touches foreign code*. | **a hole in the report's central claim**, found while measuring this; closed in section 5.1 |
| `Label::covers` for `ffi` was a text prefix, and so was `narrow`: `Ffi("libc")` could be narrowed to `Ffi("libcrypto")` and `ffi("libc")` covered `ffi("libcrypto")`. | confirmed by reading `ir.rs` and `lower/mod.rs`; the same bug `docs/signals.md` section 2.1 avoided for signals |
| 108 `extern fn` declarations in 36 `.cho` files; 218 mentions of `Ffi("libc")`, 81 of `Ffi("tls")`, none with a comma. | `grep` |

## 3. What a report can know: a fact, a claim, and an open question

Three different things were being called "the library":

| | what it is | who establishes it | in the report as |
|---|---|---|---|
| **the symbol** | the name the linker binds. A program that does not declare `statx` cannot call it. | the program text, exactly, by reachability | a **fact** |
| **the scope** | the library the declaration says the symbol lives in. Nothing checks it (`a2`). | the declaration's author | a **claim**, printed beside the fact |
| **what the symbol does** | `statx` reads a mode; `syscall` does anything; `dlsym` and `system` and `execve` do the rest. | the linked library, and a reader | **open**, and cannot be closed by the compiler |

The last row is why a deny-list is theatre (`under-a-grant.md` section 6: refusing `syscall` only moves the program to a wrapper), and why an allow-list belongs to whoever reads the report,
not to the compiler. What the compiler can do is make the first row as precise as it is and put the second beside it, honestly named.

So the word **`bounded` stays exactly what it was**: *the labels bound what the program can reach*. It is `false` whenever a foreign symbol is reachable, because nothing in the language bounds
what that symbol does. What changes is that `false` is no longer a verdict without a cause. The report now says what it is made of, and everything it does not list is bounded.

## 4. The options

| | what the report/type says | cost | verdict |
|---|---|---|---|
| **A. Report only** | `unbounded_by: ["libc:statx"]`; no language change | ~60 lines in the CLI | necessary, **not sufficient**: a program still has one scope, so "per library" is the author's convention, and the hole of section 2 stays |
| **B. A scope is a set of libraries** | `Ffi("libc,libssl")`, canonical like `Signals("INT,TERM")`; each `extern fn` names *one* library; the capability is lent narrower to a function that needs less | checker only: one new module, `narrow`, `covers`, one coercion; no backend | **built.** Makes "two libraries in one program" writable, so the pairs are meaningful |
| **C. Symbol-level rows** | `ffi("libc:statx")` in every function's row | every function on the chain names every symbol under it: the TLS chain's top function would declare up to 37 labels (the report would not need them: it computes the union); `Label::covers` would need a second separator rule; every existing `[ffi("libc")]` row changes | **not worth it.** The union is already computed exactly; per-function symbol rows add a signature a reader cannot take in and give the supervisor nothing the report lacks |
| **D. Symbol-level capability** | `Ffi("libc:statx")`, a capability that can call exactly one symbol | needs `narrow` to **split** (two disjoint capabilities from one), which no capability has (`signals.md` section 3: "nothing can make two disjoint capabilities yet"); a program holding 25 would write them in every signature | **no**, until a `fork`-style narrowing exists and a program asks |
| **E. `--allow-foreign FILE`** | the tool compares `unbounded_by` against an allow-list and exits non-zero | small, but it is a policy format and a second source of truth next to the grant | **not now.** `unbounded_by` is the interface: a supervisor holds the set it accepts and checks `unbounded_by ⊆ allowed` |

**Recommendation (taken):** A and B together, plus the hole fix, which is not optional. They are one slice because A without B cannot show two libraries, and B without A has nothing to show.

### 4.1 Against the `signals` precedent

`signals("INT,TERM")` put a **set in the capability's type** and made the row exact, because eight short names exist and a program claims two. The same shape is right for libraries
(a program links two or three) and wrong for symbols (a program calls tens), which is why B follows it and C and D do not. The part that carries over unchanged: the set is canonical
(alphabetical, no spaces), `narrow` accepts any order and answers the canonical one, `covers` is set containment and not a text prefix, and a refusal has a rule tag.

### 4.2 Against `under-a-grant.md` and a lex-os grant

A grant says `filesystem`, `network`, `exec`, and an egress list. `exec` and `network` are exactly the dimensions a foreign symbol reaches (`system`, `socket`). The report cannot say which
domain a symbol belongs to, and could not honestly: that is the heuristic `under-a-grant.md` section 3 refuted (`syscall`). What the report **can** give a supervisor is the exact set of
pairs, and a supervisor that has decided "`libc:statx` and `libc:getpid` are fine" holds a grant over *symbols*, which is finer than the one lex-os has and is decidable. The check is
`unbounded_by ⊆ granted`; `bounded: true` programs need no such grant. A symbol the supervisor does not recognise is refused. A symbol it recognises is trusted for what the library does, which is the supervisor's
judgement and not the report's.

## 5. The design

### 5.1 A foreign function is reached through exactly one `Ffi` (the hole)

`extern fn` must borrow **exactly one** `Ffi` capability, and that capability must name **one** library. Refused under the existing rule `foreign-declaration`:

```cancho-refused
//~ RULE foreign-declaration

extern fn system[&c](command: &c [byte]) -> [] c_int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    return system("true\0");
}
```

The three ways a declaration breaks it, each with its message: *borrows 0 `Ffi` capabilities* (including borrowing only an `Io`, which has a row of its own), *borrows 2* (a symbol lives in one library,
so a second capability would make the pair say less than the declaration did), and *names several libraries* (`Ffi("libc,libm")` on an extern). A declaration that names an effect without
the capability, or a capability without the effect, still gets the older `effect-not-declared` it always did.

### 5.2 A scope is a set of libraries

`Ffi("libc,libssl")`. Library names are letters, digits, `_`, `.`, `+`, `-`; comma separated; each once; any order when written and alphabetical when answered, in `narrow` and in a type alike
(`Ffi("libssl,libc")` in a parameter is `Ffi("libc,libssl")`). The empty string is still the root `split` hands out. Malformed scopes are the new rule **`foreign-scope`**.

| | rule |
|---|---|
| `narrow(ffi, "libc,")`, `"libc,libc"`, `"libc libm"`, `"libc:statx"`, `"lib/c"`; the same in a type | `foreign-scope` |
| `narrow` to a library the capability does not hold, to the root, or (new) to a text-prefix neighbour such as `"libc"` to `"libcrypto"` | `capability-not-narrowable` |
| `narrow` to the same set | `capability-not-narrowable` |
| an `extern` borrowing a set, no `Ffi`, or two | `foreign-declaration` |
| lending a library the capability lacks, or the unnarrowed root, to a function that wants one | `type-mismatch` |

`covers` for `ffi` is set containment: `ffi("libc,libssl")` covers `ffi("libssl")`, the root covers all, and `ffi("libc")` no longer covers `ffi("libcrypto")`. **That is a behaviour change**:
`narrow` from `"libc"` to `"libcrypto"` was accepted before and is refused now. Nothing in this repository did it (the only fixture for the direction, `tests/reject/effect_widened.cho`, is the refused
one), and it was a bug of the same kind `signals.md` section 2.1 describes. No edition gates it: every scope in this repository is a single plain name, so it spells the same set before and after.

**Lending.** A reference to a capability over several libraries is accepted where a reference to a subset is wanted, in any context a reference is expected. That is the only coercion between
capability types. It is attenuation: the callee can reach fewer libraries than the caller holds. Not from the root (a program says which libraries it calls by narrowing), and not for an owned
`Ffi` (that is `narrow`, which consumes). It is what lets `net.sockets` keep its `Ffi("libc")` while a program also holds `libssl`.

```cancho
edition 5;

extern fn labs[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int;
extern fn pthread_self[&f](ffi: &f Ffi("libpthread")) -> [ffi("libpthread")] int;

// One capability over both libraries, lent down to what each helper needs.
fn magnitude[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int {
    return labs(ffi, n);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io); release(fs); release(heap); release(args); release(net); release(clock);
    let native = narrow(ffi, "libpthread,libc");
    var status = 1;
    borrow native as &f in {
        if magnitude(f, 0 - 7) == 7 && pthread_self(f) != 0 { status = 0; }
    }
    release(native);
    return status;
}
```

Rows stay exact in both directions, per library: `magnitude` performs `ffi("libc")` and declares it; one that declared `ffi("libpthread")` too would be
`effect-declared-not-performed`. Owning the set discharges every library in it (`main` declares `[]`).

### 5.3 The report

`bounded` is still the first key. The second is new:

```json
{
  "bounded": false,
  "unbounded_by": [
    "libc:statx"
  ],
  "effects": ["ffi"],
  "labels": [
    { "name": "ffi", "argument": "libc", "bounded": false }
  ],
  "foreign_symbols": ["statx"],
  "pure": [],
  "folded_operators": 0,
  "folded_calls": 0,
  "functions": 2
}
```

* `unbounded_by`: every reachable foreign symbol as `scope:symbol`, sorted, each once, **one per line**, so a CI pin of the report shows an added symbol as one added line. `[]` when `bounded` is `true`.
  The scope is the one the declaration claims (section 3).
* The entry `"*"` is *foreign reach the symbol list does not account for*: the program performs an `ffi` label and no reachable call site names a symbol. No program the checker accepts produces it
  (every foreign transfer of control is a call to a declared symbol; there is no call through a pointer), and it exists so that if that stops being true the report fails closed rather than calling an unaccounted-for
  program bounded. It is the only remaining meaning of the old unqualified `UNBOUNDED`, and it is unit-tested because no program reaches it.
* `foreign_symbols` and `labels` are unchanged, so every consumer of the old shape still reads it. A pin should use `unbounded_by`: it is the only field that carries the scope.
* `bounded` stays `false` for any foreign symbol, `true` otherwise. It is not `true` "given an allow-list": the allow-list is the reader's.

The text form:

```
UNBOUNDED: this program calls foreign code. The labels below bound it
everywhere except through the symbols under "unbounded by", and what
a symbol does is the linked library's, not the language's.
See docs/foreign-authority.md.
performs
    ffi("libc")    <- unbounded
never touches
    ...
unbounded by
    libc:statx
```

## 6. Decisions on the edges

* **A declaration nobody calls is not reach.** The report lists a symbol when a reachable call site names it, the standard the labels were already held to (`docs/authority.md` section 3).
  Measured: a program declaring `getpid` and a symbol that exists nowhere, calling neither, reports `bounded: true`, `unbounded_by: []`, and *builds and runs on both backends*: an unused declaration is not an
  undefined reference. A function that calls a symbol but is itself unreachable is the same. The cost is that deleting a call from `main` changes the pin without touching the declaration, which is the point.
* **The same symbol under two scopes** is two entries (`a:f`, `b:f`), because they are two claims. Within one program the checker already refuses two declarations binding one symbol (`foreign-declaration`), so this
  needs two libraries to claim one name through different Lex names, which cannot happen; the case is a property of `attribute`, tested there.
* **The scope is a claim.** `system` declared under `Ffi("openssl")` is reported as `openssl:system`. The report never says "this is OpenSSL"; it says "the program says this is OpenSSL", next to the symbol that decides.
  Checking a symbol against the library that defines it would need the linker's answer (`-l`), and `check` and `authority` do not link. Not built.
* **`-l`/`-L` are not in the report.** They are the build's, not the program's; a different link line can bind a symbol to different code. The report is about the program text, and says so (section 10).
* **Zero symbols, `bounded: true`.** A program that holds an `Ffi` and calls nothing is bounded: the capability was released, and a released capability reaches nothing (`authority.md` section 2.2).

## 7. What it is checked by

`crates/cancho/tests/conformance/foreign_authority.rs`, real programs built and run on **both backends**:

| claim | test |
|---|---|
| no foreign code: `bounded: true`, `unbounded_by: []`, never touches foreign code | `a_program_with_no_foreign_code_is_bounded_and_unbounded_by_nothing` |
| one libc symbol: the **exact** JSON and the **exact** text, exits 0 on both backends | `one_libc_symbol_is_named_exactly` |
| two libraries in one program, attributed symbol by symbol, run on both backends | `two_libraries_are_attributed_symbol_by_symbol` |
| a declaration nobody calls: bounded, no symbols, links and runs | `a_declaration_nobody_calls_is_not_reach` |
| the pin: adding a symbol adds exactly one line to `unbounded_by` | `adding_a_symbol_changes_the_pin_by_exactly_that_symbol` |
| the scope is a claim: `system` under `openssl` is `openssl:system` | `the_scope_is_a_claim_and_the_symbol_is_the_fact` |
| the hole: no capability, two, an `Io`, or a set on an extern is refused | `a_foreign_function_without_a_capability_is_refused` |
| every `narrow` refusal with its rule: malformed, widening, text-prefix neighbours, the root, itself; and in a type | `a_scope_is_narrowed_as_a_set_and_every_refusal_has_its_rule` |
| lending: narrower accepted, a library the capability lacks and the root refused | `a_capability_is_lent_narrower_and_never_wider` |
| rows exact per library, both directions | `rows_stay_exact_per_library` |
| the follow-ups of section 8 are pinned, so a fix turns them red | `a_path_cannot_be_passed_to_statx_as_c_has_it`, `a_string_literal_has_no_hex_escape` |

Beside them: the unit tests of `scope` parsing and covering (`cancho-ir/src/tests/foreign_scope.rs`), of the checker (`tests/foreign.rs`), of the attribution (`cancho/src/tests/foreign_report.rs`, including the `"*"`
branch no program reaches), `tests/accept/foreign_two_libraries.cho`, `tests/reject/foreign_scope.cho` (the new rule's fixture), `tests/reject/foreign_without_capability.cho`, and the checked blocks of this document and `AGENTS.md` section 3.3.

**Mutants.** 26 deliberate breakages of the new compiler code, each checked to **compile** and then run against the unit tests, and if they passed, the conformance suite; each **killed**
(19 by the unit tests of `cancho-ir`/`cancho-syntax`, 7 only by the conformance suite or the report's unit tests). In the scope parser: an empty name allowed, a duplicate allowed, any character
allowed, no canonical sort. In the cover test: the root covering nothing, the root covered by anything, `any` where `all` belongs, and `Label::covers` falling back to a text prefix. In `narrow`:
widening allowed, narrowing to itself allowed, a malformed scope under the wrong rule. In lending: the root lent, anything lent, nothing lent. In the extern checks: no capability allowed, two allowed, a set
allowed. In the type: the scope neither checked nor canonicalised. The rule's tag renamed. In the report: the `"*"` branch removed, pairs unsorted, not deduplicated, `bounded` always true, the scope read
from the wrong label, unreachable externs listed, scope and symbol swapped. **Two lines are deliberately not mutated** because no program reaches the difference: `bounded = labels_bounded && foreign_bounded`
and `ffi_performed` in `print_authority` are the fail-closed wiring around the `"*"` branch, whose logic is unit-tested in `attribute` and which no checked program can drive.

## 8. Follow-ups found, not fixed

Each has a reproducer pinned in the conformance suite, so fixing it turns a test red and is the prompt to move the row.

1. **A slice crosses as a pointer and a length, and there is no bare pointer.** `statx(AT_FDCWD, path, flags, mask, buf)` cannot be declared as C has it:
   `extern fn statx[&f, &p, &b](ffi: &f Ffi("libc"), dirfd: int, path: &p [byte], flags: int, mask: int, buf: &b [byte]) -> [ffi("libc")] c_int;`
   passes seven values, the length of `path` lands in `flags`, `flags` in `mask`, and `mask` in `buf`. Measured with `strace -e statx` on both backends:
   `statx(AT_FDCWD, "/etc/passwd", AT_STATX_SYNC_AS_STAT|0xc, 0, 0x4) = -1 EINVAL`. `cancho-hooks` declares `statx` *shifted* to compensate. Same family as `tls-nonblocking.md` gap 4 (`strcmp`,
   `SSL_CTX_load_verify_locations`, `BIO_new_bio_pair`). Not trivial: it is a new parameter kind (a `NUL`-terminated slice passed as a pointer only, or an out-parameter form) in both backends and the
   foreign-boundary rules. Reproducer: `a_path_cannot_be_passed_to_statx_as_c_has_it`.
2. **A different arity than the declaration shape.** The same defect from the other side: the number of C arguments is the number of declared parameters plus one per slice, so a C function with *n* arguments
   and a slice anywhere but last has no declaration. Fixing 1 fixes this.
3. **A string literal has no `\x` escape.** A byte such as `0xFF` or a binary `statx` mask cannot be written in a literal; `"a\x41"` is `unknown-escape`. `AGENTS.md` section 6 says so, and `strings.md` section 8
   decided it for source text that is UTF-8; the C boundary is where it hurts. Trivial in the lexer, deliberately **not** done here: it is a change to the language's literal syntax (a byte that is not UTF-8 in a
   `[byte]` literal) with its own design questions (is `"\xFF"` a `str`?). Reproducer: `a_string_literal_has_no_hex_escape`.
4. **`c_int` only in return position.** A C `int` *parameter* is declared `int` and relies on C reading the low 32 bits; `dirfd: c_int` is `unknown type`. Harmless today (the callee ignores the upper half), recorded because
   `reach.md` section 3.4 argued the return side only.
5. **Scope against the link line.** A scope is not checked against the libraries `-l` names, so `libssl` can be claimed with no `-lssl`. A check at `build` (every claimed scope other than `libc` has a matching `-l`) is cheap
   and would give the claim one real constraint. Not built: `authority` does not link.
6. **`foreign_symbols` is redundant now** (`unbounded_by` is the same list with the scope). It stays for the consumers of the old shape; a future shape change can drop it.
7. **`Split` still hands out one `Ffi`.** B makes one capability cover several libraries; two *disjoint* capabilities (D) still need a splitting `narrow`. Nothing has asked.

## 9. What `cancho-hooks` changes to adopt it

1. Nothing to *keep working*: a program with `Ffi("libc")` compiles as before and reports the same labels. Its report gains `unbounded_by`.
2. **`statx`.** Keep `Ffi("libc")`. The report becomes `bounded: false`, `unbounded_by: ["libc:statx"]` (once the shifted declaration is the only foreign symbol; today it also lists whatever else `ops.cho` declares:
   the four signal functions go with `docs/signals.md` section 7). Commit the JSON as the pin; CI diffs it.
3. **`https`.** `narrow(ffi, "libc,libcrypto,libssl")` once in `main`; declare each OpenSSL function under its **own library** (`SSL_*`, `TLS_*` under `Ffi("libssl")`; `BIO_*`, `ERR_*`, `X509_*` under
   `Ffi("libcrypto")`; `statx` under `Ffi("libc")`); lend each chain function only the part it uses; rows then name at most three libraries instead of one purpose-word, and the pin lists the 25 pairs grouped by library.
   `tls.cho` of the spike would change its 37 externs' scope from `"tls"` and move its one libc symbol (`signal`) out. Build with `-l ssl -l crypto` as before.
4. `cancho-hooks`' README says "No `Ffi`, no `unsafe`". It must say: no `Ffi` **beyond the pinned list**, and link to the pin.
5. Nothing else in `cancho-hooks` is touched by this change; the shifted `statx` declaration and the missing `\x` are follow-ups 1 and 3.

## 10. What is not verified, and what building it found

* **Not run on `cancho-hooks` or on a real `https` build.** `tls_nb` was run for its report (37 pairs, all under the spike's `"tls"`); it was not converted to per-library scopes, so the claim that its rows would name at most three
  libraries is by counting its prefixes (`SSL_`/`TLS_`: libssl; `BIO_`/`ERR_`/`X509_`: libcrypto; `signal`: libc), not by compiling it.
* **The scope is unchecked.** Section 6. A person reading `libc:system` and `openssl:system` sees the same symbol; the first is what it is.
* **The link line is outside the report.** A different `-l` order or `LD_PRELOAD` binds a name to other code. So does a `constructor` in a linked library, which runs before `main` and appears nowhere.
* **macOS** is not run here. The checker change is target-independent; the two programs in the suite call `getpid`, `labs` and `pthread_self`, which every libc has.
* **`libpthread` is a claim.** glibc has folded it into libc since 2.34, so `pthread_self` under `Ffi("libpthread")` is exactly the unchecked claim of section 3 and is used on purpose: it is the one integer-only function
  here that a second library nominally owns.
* **Corrected in place while building it:** the first draft of section 5.1 refused an `extern` with no `Ffi` *before* the older row checks, which changed the message of `tests/reject/ffi_without_capability.cho`'s refusal
  and of a unit test; the new check runs after them, so an older refusal keeps its words.
