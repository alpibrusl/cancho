# Three gaps found by building the sibling tools with the compiler written in cancho

**Status: design, written before any code.** Found on 2026-11-13 while moving cancho-table, cancho-ml, cancho-stats, cancho-flow, cancho-solve and cancho-opt to `51524d2` and
building them with the compiler written in cancho (`docs/seed.md`), with no Rust. Each sibling got a workaround in its own repository to stay green. Each workaround is a cost of
something this compiler does, and each can be removed by a change here. Claims are **[measured]** (with the command or the file), **[read]**, a **[choice]** or **[open]**; a claim that
turns out false is corrected here, in place.

## 1. A function, and a `static`, is named by its node number in the whole program

**[measured]** cancho-ml's `ml`, built with the three server stores in its package set and without them (and without the `ml-serve` bin), on Linux x86-64: the files are both 2,252,128 bytes
and are not equal. `.text` (1,777,376 bytes), `.rodata`, `.eh_frame`, the dynamic sections, the data and the program headers are byte for byte the same. `.symtab` and `.strtab` differ (12,086 and
25,106 bytes) and nothing else does: both have 2,371 symbols, and the names that differ are 2,010 numbered functions `lexs_<n>` (the first four numbers, 5, 11, 17 and 21, agree and later ones shift), 66 static
initialisers `sinit<n>` and the six `sd<n>` globals of statics, which the three modules added. I did not check whether the order of the symbols also differs. `readelf -s`, `nm` and a comparison of the sections by name show it.

**[read]** `examples/selfhost/emit.cho:1800`: a function is `define <ret> @lexs_<n>(`, "`n` the function's node, which no other function shares" (and `i<n>` for an instance of a generic one); `prune.cho` keys
on that spelling; a `static` is `@sd<n>` with an initialiser `@sinit<n>`, `n` "the item's node" (`emit.cho:1218`). A node number counts nodes over the whole program, so adding modules renames the functions and statics that come after them. The Rust emitter has the same
spelling (`crates/cancho-codegen-llvm/src/emit.rs`).

**Consequence.** The same program is not the same executable when the set of packages around it changes; a gate that wants "adding these dependencies changes nothing" (cancho-ml's W2) cannot ask the files.
The siblings compare everything but the symbol and string tables, which needs a parser of ELF and of Mach-O in a Python script (`scripts/check_web_pin.py`, cancho-ml). It is also part of why a build is not
bit-reproducible (`docs/ci.md` of cancho-ml, section 10).

**Proposal [choice, open on the identity].** Name a function, a static and its initialiser by an identity that does not depend on other modules: its module path, its item name and its instance, or the per-unit identity of
`docs/canonical-ast.md`. The prune key (`@lexs_<item>i<instance>`) and the Rust emitter change with it. **[open]** which identity: a path is readable in a backtrace and can collide with a
generic instance; the canonical id is stable and unreadable.

**Gates, written first:** (a) build one program twice, alone and with an unrelated module added to its package set: the executables are byte for byte equal, on both stages; (b) the stage-2 and stage-3 compilers still
equal (`scripts/build-standalone.sh`); (c) the reject and accept goldens are unchanged. The seed may lag (`docs/seed.md` section 5): a reseed is its own pull request. **Removes:** the ELF and Mach-O comparison in
cancho-ml `check_web_pin.py`, and its note in `docs/ci.md`.

## 2. A listener whose port is a run-time value cannot be bounded

**[measured, 2026-11-13]** cancho-ml's `ml-serve` reports `bounded: false`, `unbounded_by: ['net_in("")']` at `51524d2`. **[read]** `docs/net-bounds.md` section 2.4, built: an unnarrowed `net_in("")` is unbounded.
`ml-serve` takes `--port` when it starts, and `narrow` takes literal bounds (`docs/net-bounds.md` sections 2.2 to 2.3), so there is no literal to give it. Before section 2.4 the row read `bounded: true`.

**Consequence.** cancho-ml lists `net_in("")` as an exception allowed to stay unbounded, with a reason, in `tools.toml`, and `scripts/manifest.py` accepts an unbounded row only for a listed label. That is a person's
sign-off standing in for a bound the language cannot state. The compiler's own examples that name their ports at run time (`http_fetch_nb`, `tls_echo`, `https_hello`) do the same.

**Options [open, a language decision].**
* **(a) A bound that is a range or a set of literals**, for example `narrow(ports, "1024-65535")` for "unprivileged ports": the report says `net_in(1024-65535)` and is bounded; the check at `listen` compares the run-time port to the range, as it
  compares a literal bound today. `ml-serve` would narrow to the unprivileged range and keep its `--port`. This states a true and useful thing and needs the grammar of a bound, the check, the report and an edition.
* **(b) A bound taken from a run-time value** (`narrow(ports, port_from_args)`): the report cannot print it, so it would be `net_in(<run-time>)`, which says no more than `""` does. Not recommended.
* **(c) Leave it**: unbounded with a listed reason, as now.

**Recommendation:** (a). **Gates, written first:** the report of a program that narrows to a range lists the range and is bounded; a `listen` outside it traps on both backends; a range that contains another, or equals it, is
refused as the literal bounds are (`docs/net-bounds.md` section 2.2). **Removes:** the exception in cancho-ml's `tools.toml`, the `unbounded` rule in its `scripts/manifest.py`, and a test.

## 3. The compiler in cancho accepts a `res` value overwritten through `&!`

**[measured, 2026-11-13, `51524d2`]** `scripts/spikes/join/s8/n1_table_into_job.cho` of cancho-table: a worker lent a job by `&!` builds a table (`res`) and assigns it to a field of the job. The Rust compiler refuses it
(`the value overwritten here is res (JTab), so it cannot be discarded`, rule `linear-value-unconsumed`, `crates/cancho-ir/src/linear.rs`, the `Discard` event). The compiler written in cancho **accepts it, and builds a
program** (exit 0). Its checker has the rule (`tables.cho` lists `linear-value-unconsumed`) and does not raise it for this assignment. The second program of the pair, `n2_table_as_result`, is refused by both.

**Consequence.** The Rust compiler is the only oracle for this refusal, and `docs/removing-rust.md` removes it. cancho-table's premise test (`Premise`) asked it with `CANCHO_COMPILER=rust`; with no Rust it has a strong form that needs a
Rust-built `cancho` and a tripwire that pins the gap. Until it is closed, a program that leaks a `res` value compiles.

**Proposal.** The checker in cancho raises `linear-value-unconsumed` where an assignment overwrites a live `res` value, as `Event::Discard` does. **Gates, written first:** `tests/reject/` gets a fixture of the n1 shape (a `res` field
overwritten through `&!`) refused by both compilers with the same rule; the differential gate (`docs/differential.md`) covers it. **Removes:** the tripwire and the `CANCHO_RUST` branch of cancho-table's `Premise`.

## What each fix removes in the siblings

| Gap | Workaround today | Where | Removed by |
|---|---|---|---|
| 1. node-number names | W2 compares sections, not files (ELF and Mach-O parser) | cancho-ml `scripts/check_web_pin.py`, `docs/ci.md` | stable names |
| 2. run-time port | `net_in("")` listed as allowed to be unbounded | cancho-ml `tools.toml`, `scripts/manifest.py` | range bounds (a) |
| 3. `res` overwrite accepted | tripwire on a known gap, strong form behind `CANCHO_RUST` | cancho-table `tests/conformance/test_join_j8.py` | the check in the compiler in cancho |

## Order

1. **Gap 3**: contained, a rule that exists and is not raised in one case; it restores a real check for cancho-table and closes a parity gap that the removal of Rust would otherwise make permanent.
2. **Gap 1**: mechanical but touches the emitter, the prune key and the Rust emitter; the gates are cheap to write.
3. **Gap 2**: needs the language decision above before any code.

Each is its own pull request with a row in `docs/ROADMAP.md`.
