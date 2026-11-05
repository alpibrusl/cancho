# `cancho ci`: the gate as one command, and the authority baseline

> **Status: slices 1 to 3 built (§10 to §12); project mode and the baseline of several programs are not.** Written before the code, as
> `CONTRIBUTING.md` asks. Every number below is either marked *measured
> (this document)* with how, or is a **gate**: a figure the built thing
> must meet, not a result. A claim that turns out false is corrected here,
> in place.
>
> **Written in cancho.** `cancho ci` is a slice of the command line in
> cancho (`examples/selfhost/cli*.cho`, `docs/cli-in-cancho.md`), not of
> `crates/cancho/src/`. The Rust binary gets no new subcommand and no new
> line for this. §7 says what that costs.

## 1. The question

A project written by an agent has no default CI. The pieces exist and have
stable exit codes (`fmt --check`, `check --output json`, `test`,
`authority --output json`), but nothing composes them, so every project
re-derives the composition, and a person and an agent do not even run the
same one. An agent's loop is *edit, run the gate, read the verdict, repair*.
The gate should be one command whose verdict is data.

The second question is the one only this language can ask. A conventional CI
cannot say *"this change made the program able to reach the network"*.
Here it can, because `cancho authority` derives the least grant a program
provably needs (`authority.md`). A reviewer can miss that in a diff. A loop
that reads a verdict cannot, if the verdict contains it.

## 2. The command

```sh
cancho ci [FILE... | (project)] [--std-dir DIR] [--output json] [--strict]
cancho ci --accept-authority --reason "<why>"
cancho ci --init
```

With no files, in a project, it takes the `[[bin]]` and `[[test]]` sets of
`cancho.toml` (`package-system.md` §8). Four steps, in this order:

| step | is | exit it maps from |
|---|---|---|
| `fmt` | `fmt --check` over the sources | 1 |
| `check` | `check --output json` over every program | 1 |
| `test` | `test`, one process per `fn test_*` | 4 |
| `authority` | each `[[bin]]`'s report against the baseline (§3) | n/a, new |

Every step runs unless one it depends on failed: `test` and `authority` need
a program that checks, so they are `skipped` with `because: "check"`, not
failed twice. Independent failures are all reported, as `check` reports
every independent refusal.

**Exit codes are the existing ones**, not new ones: 0 pass, 1 any finding,
2 the command line was wrong, 3 the environment failed (no `clang`, no
`cancho.toml` when none was needed), 4 only when `test` failed and nothing
else did. The first failing step's code wins when several do, as
`cancho test` in a project already does (`package-system.md` §8.8).

## 3. The authority baseline: `cancho.authority.json`

A **committed file at the project root**. This is decided (the user,
2026-11-05), over a `cancho.toml` section, for one reason: a review shows a
changed line in a file whose only content is authority, and a widening
cannot hide inside an unrelated `cancho.toml` edit.

It holds, per `[[bin]]`, the part of `authority --output json` that means
something and is stable:

```json
{
  "format": 1,
  "programs": {
    "fetch": {
      "bounded": true,
      "unbounded_by": [],
      "effects": ["io_write", "net_out"],
      "labels": [{ "name": "net_out", "argument": "api.example.test:443", "bounded": true }],
      "never_touches": ["the filesystem", "the heap", "other programs", "foreign code"],
      "foreign_symbols": [],
      "accepted": [{ "label": "net_out", "reason": "fetch the release index" }]
    }
  }
}
```

**What is left out, and why.** `pure`, `folded_operators`, `folded_calls`
and `functions` are in the report (`print_authority`, `main.rs`) and are not
in the baseline: they move on every refactor and on every constant-folding
change, and a baseline that fails on those is a baseline an agent learns to
overwrite. The kept fields are the ones a consumer reading only data is told
to read (`under-a-grant.md` §5.1, `foreign-authority.md` §5).

**One line per fact.** The file is written the way the report is, one label
and one symbol per line, so a widening is one added line in a diff
(`foreign-authority.md` §5 did the same for `unbounded_by`).

### 3.1 The comparison

Computed on the *set* of `(name, argument)` labels and `never_touches`
domains, not on text:

| verdict | when | tag | exit |
|---|---|---|---|
| widened | a label, a foreign symbol, or `bounded` going `true` to `false`, or a domain leaving `never_touches` | `ci-authority-widened` | 1 |
| unbounded | `bounded` is `false` and the baseline's was `true` | `ci-authority-unbounded` | 1 |
| narrowed | the reverse of any of the above | `ci-authority-narrowed` | 0, or 1 with `--strict` |
| absent | a `[[bin]]` the baseline lacks | `ci-authority-baseline-missing` | 1 |
| stale | the baseline names a program that no longer exists | `ci-authority-baseline-stale` | 1 |
| unreadable | not JSON, or `format` unknown | `ci-baseline-unreadable` | 1 |

Narrowed passes by default because a program that can do less is not a
regression, and `--strict` exists for a project that wants the baseline to
stay exact. A narrowing is reported so the file can be re-recorded, as
`tls-performance.md` §1.3 requires of a primitive that got cheaper.

### 3.2 Accepting a widening is a recorded act

```sh
cancho ci --accept-authority --reason "fetch the release index"
```

rewrites the baseline from the current reports **and appends the reason
under `accepted`** for each label added. `--reason` is required: without it
the command is refused (`ci-accept-needs-reason`, exit 2). This does not
stop an agent from lying. It makes the act appear as a reviewable line next
to the label it admitted, which a bare `--update` flag would not.

## 4. The report

```json
{
  "ok": false,
  "steps": [
    { "name": "fmt", "status": "pass" },
    { "name": "check", "status": "fail", "findings": 1 },
    { "name": "test", "status": "skipped", "because": "check" },
    { "name": "authority", "status": "skipped", "because": "check" }
  ],
  "findings": [
    { "tag": "effect-not-declared", "step": "check",
      "message": "...", "position": { "file": "src/a.cho", "line": 11, "column": 1 } }
  ]
}
```

An **envelope with a list**, as `check`'s, for the reason `agent-errors.md`
§5 gives. A finding from `check` keeps its compiler **rule** as `tag`
unchanged; the findings only `ci` raises have the `ci-` prefix. The first
field a consumer reading one thing should read is `ok`.

The `ci-` tags are a **second namespace**. They are not among the 58
compiler rules that `AGENTS.md` counts, and adding them does not change
that count. They are listed in this document and registered with the
command, and `introspect` carries them.

## 5. `--init`

Writes `.github/workflows/cancho.yml` and nothing else. The file does one
thing: install the compiler **at the commit `cancho.toml` pins**
(`package-system.md` §9, `scripts/install.sh`, checksum and reported commit
both checked), then run `cancho ci --output json`. All the logic is in the
binary, so a workflow file has nothing in it to drift and nothing to test
beyond "it calls the command".

* An existing file is never overwritten: `ci-init-exists`, exit 1.
* No `cancho.toml` pin means no workflow: `ci-init-unpinned`, exit 1. An
  unpinned workflow would be a CI that changes when the compiler does
  (`hash-stability.md` measured that 71% of this repository's history stops
  type-checking under today's compiler).

## 6. Authority of the tool itself

`cancho ci` is a cancho program and its own report is derivable. The design
asks for the smallest grant that does the work, and the build is gated on
`cancho authority` of it saying so:

* `Fs` narrowed to the project directory (read, plus write of
  `cancho.authority.json` and `.github/workflows/` only under the two
  accepting flags, which are separate programs' worth of rows if the row
  model forces it);
* `Exec` narrowed to the compiler's own path and nothing else: the steps are
  the existing commands, run as children;
* no `Net`.

*Not measured.* Whether the row model can express "write only these two
paths" is exactly what `authority.md` §2.3 shows only for a literal. If it
cannot, the write of the baseline goes in a second, separately granted
program, and this section is corrected here.

## 7. What this design does not claim

* **It runs only what the cancho command line runs.** `cli-in-cancho.md` §14
  measured 80 of 82 programs buildable and the rest a status 3, and a
  program the compiler in cancho has no code for is *not* handed to the Rust
  compiler. `cancho ci` inherits that: a project the cancho `check` skips is
  an environment failure (3) and not a pass. This is stated so that a green
  CI is never a skipped one.
* **Not a replacement for the repository's own CI.** `.github/workflows/ci.yml`
  gates the compiler, and `ci-speed.md` is about that. This is for a project
  written *in* cancho.
* **No cache, no matrix, no deploy.** One verdict.

## 8. Gates (to be measured, not claimed)

1. **Parity.** On the programs of the repository, `cancho ci` over a single
   file gives the verdict that `fmt --check && check && test` give, step by
   step, with a differential test as `cli-in-cancho.md` §5 does for every
   slice.
2. **The baseline catches a widening and only that.** For each of the 16
   programs of `examples/` with a `main` (the set `cli-in-cancho.md` §34
   measured): record, add one `net_out` label, expect exactly
   `ci-authority-widened`; remove it, expect a pass. Plus a corpus of
   refactors that change `pure`/`functions` and must pass.
3. **No input reaches a trap.** A fuzzer over `cancho.authority.json`
   (truncated, duplicated keys, wrong types, a 64 MiB file) must produce
   only `ci-baseline-unreadable`, never a crash. The same bar as every
   parser here (`fuzz_afl.py`).
4. **Mutants.** The comparison is small enough to mutate exhaustively; the
   target is the 10 of 10 that `package-system.md` §8.8 reached.
5. **Cost.** A gate: `cancho ci` on `examples/tally.cho` adds under 50 ms to
   the steps it runs. Not measured; the steps themselves dominate.

## 9. Open

| Question | Why it waits |
|---|---|
| Should `ci` also run `satisfy` for a contract file | no asker; `satisfy.md` is a loop, not a gate |
| A `cancho ci --since REV` that checks only what changed | needs a dependency graph per program; `ids` has the hashes, nothing consumes them |
| Per-label baseline expiry | a widening accepted "until" a date; no asker |

## 10. Slice 1, as built (`examples/selfhost/clici.cho`)

```sh
cancho ci [--output json] [--std-dir DIR] [--test FILE]... FILE...
```

`fmt`, `check` and `test` are built; the `authority` step, `--strict`,
`--accept-authority` and `--init` are not. The code is `clici.cho` (about 340
lines) and a three-line dispatch in `cli.cho`; **no line of Rust in
`crates/cancho/src/`**. The test is `selfhost/ci.rs`.

**What building it changed in this document** (corrected in place, as the
header promises):

* **A finding's key is `rule`, not `tag`**, for `ci`'s own findings and
  `check`'s alike, so a consumer reads one key (§4's example is amended
  below). `check`'s findings are spliced in unchanged, with their
  `explanation` and `position`.
* **Findings sit inside their step**, as `steps[i].findings`, not in one
  top-level list. A step with no findings has no `findings` key.
* **There are `--test` files.** `cancho test` takes the files of one run in
  full (the library and the tests), so `ci` takes them the same way:
  `--test lib.cho --test t.cho`. Without a `--test` the step is `skipped`
  with `because: "no-tests"`.
* **`check` needs a `main`.** With only `--test` files it is `skipped`
  with `because: "no-program"` and the test step still runs (before, the
  design had it fail as `program-shape`; measured on a library with tests).
* **The steps are runs of this command line**, found by the name it was run
  by (`arg 0`). A name with `..` in it **trapped** (`Illegal instruction`,
  exit 132) before this was found, under `Exec`, outside a debugger: with
  `../cli` the parent died after opening its pipes, and the same command
  under `gdb` ran, because `gdb` runs it by an absolute path. It is now the
  finding `ci-self-path`, exit 3, on standard error. The trap in `Exec` for
  such a name is a defect of its own and is not fixed here.

```json
{ "ok": false, "steps": [
  { "name": "fmt", "status": "pass" },
  { "name": "check", "status": "fail", "findings": [
    { "rule": "effect-not-declared", "message": "...", "explanation": "...",
      "position": { "file": "bad.cho", "line": 11, "column": 1 } } ] },
  { "name": "test", "status": "skipped", "because": "check" } ] }
```

**Rules slice 1 can raise:** `ci-not-formatted`, `ci-fmt-refused`,
`ci-test-failed`, `ci-test-refused`, `ci-no-tests`, `ci-step-unrunnable`,
`ci-check-unreadable`, and on standard error `ci-self-path`.

**Measured (this document, Linux x86-64).** One test builds the command line
and runs `ci` on a passing program, a badly formatted one, a refused one, a
library with a passing test, a library with a failing one, and the two
combined: the status of each step, the rules, the exit statuses (0, 1, 4, 1 —
the first failing step wins) and that every answer is accepted by
`serde_json`. `ci_in_cancho_says_what_fmt_check_and_test_say`: passes, 46 s
(almost all of it building the command line).

**Not done, and said so:**

* **Corrected: `ci` is in `introspect` and `skill`** (decided, 2026-11-05; the first version of this section said it was not and asked). The
  command tree is registered once, in `crates/cancho/src/acli.rs`, and `examples/selfhost/commandtree.cho` is generated from the Rust binary's
  output; the Rust binary is held by `agent_cli.rs` to advertising only what it runs. So `ci`, `profile` and `trace` are registered there as data
  (`extra_commands()`, no logic) and **advertised only when `CANCHO_ACLI_EXTRAS` is set**, which the generator and the parity test set. The
  command line in cancho shows them; the Rust binary does not. They are **not in `--help`**: its text is the `USAGE` constant of `main.rs`,
  which the Rust binary prints for itself. `ci_in_cancho_says_what_fmt_check_and_test_say` checks both (`introspect` lists the three, the Rust
  binary lists none, `skill` carries their options).
* Not run on a project (`cancho.toml`): files are named.
* Not measured: the cost of a `ci` over the cost of its three steps (gate 5).
* Gates 2 to 4 of §8 are the authority baseline's and wait for it.

## 11. Slice 2, as built: the authority baseline (`clici.cho`, `cibase.cho`)

```sh
cancho ci [--strict] FILE...
cancho ci --accept-authority --reason "<why>" FILE...
```

A fourth step, `authority`, after `test`: `authority --output json` of the
program, compared with `cancho.authority.json` in the current directory.
It runs when there are FILEs and `check` passed; otherwise it is `skipped`
(`no-program`, `check`).

**What differs from §3, because of what was built:**

* **One program per baseline file.** The file has `"program": "<stem of the
  first FILE>"` at its top level, not a `programs` map. The map needs
  `[[bin]]` names, which project mode will give; until then a baseline
  that names another program is `ci-authority-baseline-stale`. The format
  is `1` now and a map would be format `2`.
* **`effects` is not stored**; the labels say the same and a second copy
  could disagree. Kept: `bounded`, `unbounded_by`, `labels`,
  `never_touches`, `foreign_symbols`, and `accepted`.
* **The comparison is on lines.** `authority --output json` prints one
  label, one symbol and one `unbounded_by` entry to a line, and the baseline
  is written in the same lines (the kept lines of the run, verbatim), so
  both are read by one scanner into *facts* and a change is a fact on one
  side only. `never_touches` is read the other way round (a domain that is
  gone is a widening). Nothing needs a JSON value to be built; the file is
  checked to be JSON by the reader in cancho (`jsonread.cho`) and for the
  `format` and `program` lines.
* **A finding names the fact**, e.g. `authority widened: label:{ "name":
  "io_read", ... }`.

**Verdicts, as built** (the tag is the `rule`):

| when | rule | status |
|---|---|---|
| a label, symbol or `unbounded_by` entry not in the baseline, or a `never_touches` domain gone | `ci-authority-widened` | fail, exit 1 |
| `bounded` true to false | `ci-authority-unbounded` | fail |
| the reverse of either | `ci-authority-narrowed` | pass, fail with `--strict` |
| no baseline | `ci-authority-baseline-missing` | fail |
| another program's | `ci-authority-baseline-stale` | fail |
| not JSON, no `"format": 1`, empty | `ci-baseline-unreadable` | fail |
| `--accept-authority` wrote it | `ci-authority-accepted` | pass |
| the write failed | `ci-authority-unwritable` | fail, exit 3 |
| `--accept-authority` without `--reason` | `ci-accept-needs-reason` | exit 2, standard error |

**Accepting.** `--accept-authority --reason WHY` writes the baseline from
the run. Every fact that widened is appended to `accepted` as `{ "fact":
..., "reason": WHY }`, and the entries already there are kept. A baseline
that is unreadable or another program's is **not** overwritten by an
acceptance (the test reads it back byte for byte): an agent that has been
told "fix the baseline" cannot make a broken one disappear. The first record
has nothing to compare with and so has an empty `accepted`. A failing run
never touches the file.

**Measured (this document, Linux x86-64).** In the same test
(`ci_in_cancho_says_what_fmt_check_and_test_say`, 46 s): record, pass,
add `io_read` (exactly one finding, `ci-authority-widened`, naming it;
baseline unchanged), accept with a reason (one `accepted` entry carrying the
reason and the fact; passes), remove it again (`ci-authority-narrowed`, pass;
exit 1 under `--strict`), re-record (the earlier entry is kept), another
program's baseline, and four files that are not baselines. The hand run of the
widening gave the same answer.

**Not done:** the baseline of several programs and project mode;
`--init`; gates 3 (a fuzzer over the baseline: only the four hand-made bad
files so far) and 4 (mutants) of §8; the cost of §8.5. Two limits worth
knowing: a `foreign_symbols` or `never_touches` string containing a quote is
cut at the quote (the report does not produce one today), and the baseline's
program name is the stem, so `a/prog.cho` and `b/prog.cho` share one.

## 12. Slice 3, as built: `--init` (`ciinit.cho`)

```sh
cancho ci --init [--test FILE]... FILE...
```

Writes `.github/workflows/cancho.yml` and nothing else (`mkdir -p` of the
directory by `/bin/mkdir`, as `cliproject.cho` does). The file installs the
compiler `cancho.toml` pins with `scripts/install.sh` at that commit, then
runs `$HOME/.local/bin/cancho ci --output json` **with the arguments given
to `--init`**: the workflow runs exactly the command that was run to make it.

**Differences from §5, found by building it:**

* **`--init` takes the files**, because `ci` needs them and a workflow has
  nowhere else to learn them.
* **The command is run by its path**, not by `cancho`. `ci` runs this command
  line again by the name it was run by, and a bare name is looked for in the
  *current directory* (`cliprog.run_capture_in` prefixes `./`), so
  `cancho ci` from a `PATH` would look for `./cancho`. This is a limit of
  `ci` itself and is not fixed here: **`ci` cannot be run from `PATH` by a
  bare name** (as it cannot by a name with `..`, §10).
* **Only safe words go into the file.** A file name outside letters, digits
  and `_ . / -` (or one that starts with `-`), `--std-dir` (a directory of
  this machine), and any other option are `ci-init-unsafe-argument`. The
  file is YAML and the line is a shell line; the commit is checked to be
  40 or 64 lower-case hexadecimal digits before it is put in.
* **The pin is read by a line scan** for `cancho = "..."`, not by the project
  parser (`projfile.cho`), which `init` has no need of. `cancho = "main"`,
  a short hash and upper-case digits are not pins.

Rules (standard error, as `RULE: sentence`): `ci-init-unpinned` (1),
`ci-init-exists` (1; also when the file cannot be told not to exist, so
an unreadable one is never overwritten), `ci-init-unsafe-argument` (1),
`ci-init-unwritable` (3).

**Measured (this document).** The same test: no `cancho.toml` (nothing
written); three pins that are not pins; five unsafe command lines; no
files (exit 2); then a pinned project, where the file contains the install
line with the pin twice and the run line with the arguments; a second
`--init` leaves a hand-written file byte for byte. The file was also read by
PyYAML once (`steps[2]` is the `Gate` step); that is not in the test, which
has no YAML reader.

**Not measured:** that the workflow *runs on GitHub*. It cannot be, from here;
`release.yml` is the nearest evidence that `install.sh` works on the runner it
names, and the first real run of this file is the test of it.
