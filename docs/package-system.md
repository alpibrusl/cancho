# A package system for lex-sys

> **Status: design, not yet built** — except §4.2's own recheck step,
> which is: **`lex-sys vcs resolve <store-dir>`** re-parses and
> re-typechecks the source behind every pin in a real second
> `.lex-sys-vcs` store under today's compiler, and refuses if a pin no
> longer matches its own source, its source no longer type-checks, or a
> source blob is missing. This is §6's own proposed smallest slice, done,
> and it needed a real prerequisite this document had not named: neither
> `OpLog` nor `Manifest` store a declaration's actual source, only its
> hash — `Blobs` (`crates/lex-sys-vcs/src/blobs.rs`), a small
> content-addressed store for exactly that, is the piece that was
> missing. **§4.5's naming half is built too**: `lex-sys vcs lock --store
> <dep-store> -o <file> <name>...` looks a name up in a dependency's
> manifest and pins it by hash into a lock file, refusing if the name is
> unpublished or ambiguous; `vcs resolve --lock <file> <store-dir>` scopes
> resolution to just those pins instead of everything the store has ever
> published. What is **not** built yet: the lock-driven, recursive,
> multi-package resolution §4.2's first paragraph describes — a lock pins
> one store's own names, not a *program's* dependency graph, and nothing
> here walks a closure of stores, detects a cycle, or catches a diamond
> conflict. **§4.6's first fetch step is built too**: `lex-sys vcs fetch
> --lock <file> --store <dep-store> -o <dir>` re-verifies a lock the same
> way `vcs resolve --lock` does, then writes each distinct verified
> source to `-o` as `<source_hash>.ls`. It needed no compiler change at
> all — `modules.md` §4.2 already means a program is the set of files
> named on the command line and `import` already resolves a name against
> whichever of them declares it, so a fetched file is `import`-able the
> moment it exists on disk; see §6. `modules.md`
> §7 named "a package and version story" as open and explicitly out of
> scope for naming ("distribution, not naming"); `standard-library.md`
> §2.1 named it "the decision to revisit first when a package story
> exists." Both waited on two things that did not exist yet and now do:
> `lex-sys-vcs` (`vcs-publish.md`, #123–#134 — a real content-addressed
> store with a gate) and editions (`editions.md`, #85–#92 — a way for a
> file to stay compatible with an older compiler without a version
> number). This document is otherwise still the first slice: not a
> manifest format, a full CLI, or code for the rest of it — what the
> pieces already built here mean for the two or three decisions
> a package system cannot avoid, and which of them this project has
> effectively already made.
>
> **A first real package now exists: `net.sockets`
> (`packages/net-sockets/sockets.ls`, #141).** Not a fixture built to
> exercise the pipeline — `examples/serve/serve.ls` and
> `examples/results_stub/results_stub.ls` had independently declared the
> same eight `extern fn`s against libc and the same two byte-writing
> helpers since #129, an organic duplication this project's own rule
> (`standard-library.md`: "a feature earns its way in when a program
> asks") already justified extracting. Publishing it found a real gap
> `vcs publish` had never hit: every store published here before now held
> only ordinary functions with bodies, and `cmd_publish` looked up a
> lowered IR function to read its effects, which an `extern fn`
> declaration — a signature and nothing else, per `lex-sys-id`'s own
> `identify()` — does not have. Fixed by reading effects from
> `Program::externs` instead when no lowered function matches, the row
> the declaration itself carries, not one a lowering pass computed. Both
> consumer programs now lock `net.sockets` and `import` it rather than
> duplicating it, verified end to end: built together, run, and hit with
> a real HTTP request on both backends, the same as before the
> extraction.

## 1. What asked for it

The direct trigger is `README.md`'s own "not a usable language yet"
paragraph. `docs/foreign-linking.md` closed one of its three reasons;
this document is the design half of the second (the effect-vocabulary
instability `hash-stability.md` measures is the third, and it is not a
feature to build — see §5). "No package system" has sat in that
sentence since the README was rewritten with nothing behind it beyond
the two pointers above.

## 2. What this should not reinvent

Everything here already has a real answer sitting in this ecosystem,
built for a reason that had nothing to do with packages. Reading the
actual code rather than assuming a greenfield design is the point of
this section.

| Piece | Where | What it already gives a package system |
|---|---|---|
| Content-addressed declarations | `lex-sys-id::identify()`, `SigId`/`BodyId` | The identity a dependency pin should be made of — already exists, per declaration, today |
| **"A call encodes the callee's hash, not its spelling"** | `modules.md` §2 | The single fact this whole design rests on (§4.5) |
| A content-addressed store with a gate | `crates/lex-sys-vcs` — `Operation`/`OpId`/`StageId`/`OpLog`, `gate::check_candidate` | "A package is a repository of hashed declarations that refuses what does not type-check" is already built, for one repository |
| A publish/log CLI | `lex-sys vcs publish`/`vcs log` (`vcs-publish.md`) | The shape a `lex-sys pkg publish` would take, reusing the same crate |
| Per-file, additive compatibility | `editions.md` | The answer to "does an old dependency still compile," already solved without a version number |
| A fail-closed authority report | `lex-sys authority --output json` (`authority.md`, `reach.md` §5) | What a dependency's public surface is *allowed* to be checked against, already machine-readable |
| Deriving/diffing the least authority a program needs | `lex-os-authority` (`lex-os` repo) | The mechanical "did this upgrade widen what I'm exposed to" answer — built for one program's own revisions, the same shape a dependency bump needs |
| A signed capability contract, consumer's grant as the ceiling | `lex-os-capsule` (`lex-os` repo) | "Refuse, don't downgrade": installing something never silently grants more than was already allowed |
| A real, working package manager for the sibling language | `lex-lang`'s `lex pkg` — `crates/lex-store` (`Store`, content-addressed "stages" under `<root>/stages/<SigId>/`), `crates/lex-store/src/deps.rs` (lock-driven recursive resolution), `crates/lex-syntax/src/registry.rs` (a hosted index over stores) | Proof this design is not speculative — `lex pkg`'s `Store` **is** the same idea `lex-sys-vcs` already ported once (`vcs.md` §7, "shares the idea and no code") |
| Machine-readable refusals | `agent-errors.md` — rule tags, `check --output json` | The vocabulary a resolution failure should speak, not a bespoke error format |

The load-bearing row is the fourth: `lex-lang` already has a real,
working, hosted package manager, and its core data structure is a
content-addressed store keyed by declaration hash, resolved by pinning
a `(store, head)` — which is not a coincidence, it is `lex-sys-vcs`'s
own architecture, described from the other side. `lex-sys-vcs` was
built by porting `lex-vcs`'s *idea* with no shared code
(`vcs.md` §7); this document proposes doing the same thing one layer
up — porting `lex pkg`'s idea, not its code — because the two
languages' packages should relate exactly the way their VCS crates
already do.

## 3. The philosophy this has to fit

Four of this project's standing commitments bear directly on a package
system, and each one rules something out before any format gets
designed:

- **No implicit anything.** `CLAUDE.md`/`AGENTS.md`: no implicit
  conversions, no ambient authority. A package resolver that runs
  arbitrary code to fetch, build, or "prepare" a dependency — `npm`'s
  `postinstall`, Cargo's `build.rs` — is exactly the ambient authority
  this project refuses everywhere else. §4.3 makes this a stated
  design commitment, not an oversight to fix later.
- **No macros, because they break stable identity.** The same reason
  applies one level up: a dependency has to be *data* (source files and
  hashes), never a program that runs during resolution and could
  produce different bytes on two machines.
- **Refuse, don't downgrade.** `lex-os`'s repo-wide rule, and
  `lex-os-capsule`'s own phrasing of it for distribution: "the
  consumer's grant — not the publisher's declaration — is the
  ceiling." A lex-sys package has no runtime grant to check against
  (lex-sys itself has no sandbox — that is `lex-os`'s job), so the
  translation is a build-time one: what a dependency's authority report
  already says is allowed to be **shown and re-checked**, never
  silently exceeded by an upgrade. §4.4.
- **Design before code, and the two-asker bar.** `CONTRIBUTING.md`
  rules 1 and 3. This document is step one. Nothing here is scoped as
  a slice to build yet — §6 says what the first askable slice probably
  is.

## 4. The shape

### 4.1 A package is a `lex-sys-vcs` store at a pinned head

No new identity system. `crates/lex-sys-vcs`'s `Operation`/`OpId`/
`SigId`/`StageId`/`OpLog` are already, word for word, what `lex-store`'s
own module doc calls itself: *"a content-addressed code repository."*
A package is nothing more than an `OpLog` someone else published, and a
version string, if one exists at all, is a human label pointing at one
state of it — a tag over hashes, the way a git tag names a commit
without being the commit's identity.

### 4.2 Resolution is lock-driven, recursive, and never trusted without a recheck

Borrow `lex-store/src/deps.rs`'s actual algorithm, not just its idea:
each dependency's own dependencies resolve against *that dependency's*
committed lock at its own pinned head, never the root's — "a package is
checked against exactly the pins it was published with." A `(store,
head)` visited set turns a cycle into a diagnostic instead of unbounded
recursion. The same package pinned to two different heads anywhere in
the closure is a conflict, refused with a rule tag, never silently
resolved by picking one.

Where this has to diverge from `lex pkg`, on purpose: `lex-sys-vcs`'s
own gate (`gate::check_candidate`) re-parses and re-typechecks a
candidate before accepting it into a log at all (`vcs-publish.md` §3) —
this project already refuses to trust a hash's claimed shape without
recomputing it once. A resolver should hold a dependency to the same
standard the store holds a publish to: fetch the *source* behind a pin,
not only its hash, and run it through the consumer's own `lex-sys
check` before it is usable. A lock file's `SigId` is not proof the code
still type-checks under today's compiler — `hash-stability.md` already
measured that **71% of this repository's own history** stops
type-checking under today's build. A dependency pinned a year ago is
exactly this repository a year ago, and a lock file does not get to
assert on its behalf that it still compiles.

### 4.3 No code runs to resolve a dependency

Stated as a design commitment, not left implicit: fetching, unpacking,
and locking a dependency is pure data movement — read files, hash them,
compare to a pin — and never executes anything the dependency contains.
This project's own "no textual or proc macros" rule already rules out
the *mechanism* (a build script) that gives `npm`/Cargo/`pip` their
worst agent-safety failure mode; this section says the resolver itself
inherits the same rule rather than merely benefiting from it by
accident. An agent wiring up a dependency graph unattended should never
need to sandbox `lex-sys pkg` the way it has to sandbox `npm install`.

### 4.4 What a pin buys an agent, mechanically, before it is trusted

Every package publishes its own `lex-sys authority --output json`
report for its public (`pub fn`) surface, computed by the publisher's
compiler — but per §3's "refuse, don't downgrade," this is
**transparency, not trust**. `lex-os-capsule` already built the rule
this needs, for a different kind of artifact: *"the consumer's grant —
not the publisher's declaration — is the ceiling."* For lex-sys, with
no runtime grant to check against, the ceiling is the *consumer's own
rebuild*:

- Adding a dependency for the first time computes the union of what the
  whole closure's public surface performs (`lex-sys authority`,
  already fails closed: `"bounded": false` for anything reaching
  foreign code) and shows it before a lock entry is written — an agent
  decides once, against real data, not a publisher's promise.
- Bumping a pin **diffs** the old and new authority reports — the exact
  shape `lex-os-authority` already computes for one program's own
  revisions (widening / narrowing / unchanged) — and a widening refuses
  the bump until re-approved the same way the first install was;
  narrowing or unchanged applies without asking again. This is a
  mechanical, non-promise-based answer to "is this upgrade safe," and
  it needs no new machinery: `lex-os-authority`'s diff already exists,
  one repository over, built for exactly this shape of question.

### 4.5 Naming: a label is chosen once, resolved by hash forever after

`modules.md` §2's own fact — *"a call already encodes the callee's hash
rather than its spelling"* — extends past one program into a dependency
graph for free. Once `import lex-nt;` is written and its lock entry
pinned to a `StageId`, every later build resolves that name through the
**lock**, never by asking anything "what is `lex-nt` today." A name is
chosen exactly once, by whoever ran the add command; nothing downstream
can silently substitute a different package under the same name later
— the mutable name-resolves-to-latest model that is dependency
confusion and typosquatting's entire attack surface in a registry that
works that way.

**Built as `lex-sys vcs lock`, and one prediction corrected on
contact.** This paragraph originally said re-locking an existing name to
a different hash should be a refusal; building it found that too strict.
The attack this section is about is *automatic* substitution — a build
silently re-resolving a name against a live registry, with nothing in
the loop to notice. Running `vcs lock` a second time, by hand or by an
agent that decided to, is not that: it is the same deliberate act as
`cargo update` or `git tag -f`, and there is nothing to protect a
consumer *from* in their own explicit choice to re-pin. So a second
`vcs lock <name>` overwrites the lock entry, and what actually
implements this section's guarantee is narrower and correct as stated:
nothing *other* than that explicit command ever changes what a name
resolves to, and `vcs resolve --lock` only ever reads the lock, never
re-derives a name from anything live.

### 4.6 Distribution: no hub required to start

`lex pkg`'s hub (`crates/lex-syntax/src/registry.rs`, hosted at
`vcs.lexlang.org`) is real, useful infrastructure, but nothing here
needs it *first*. A `DepLocator`-shaped abstraction — `lex-store/src/
deps.rs`'s own phrase, "what a pin points at is context-specific... [is]
abstracted" — can resolve a dependency from a local path or a plain
`git clone` of someone else's `.lex-sys-vcs` store before any hosted
service exists, the same way this repository's own `--std` shipped by
embedding the library's source rather than waiting on a package host at
all (`standard-library.md` §2). A hub, when one is worth building, is
*only* a name-to-pin index and an archive cache in front of the same
content-addressed stores — never a second source of truth, and never a
place authority is decided (that stays local, per §4.4, on every
machine that resolves, agent or human).

**The first fetch step is built as `lex-sys vcs fetch`.** It re-verifies
a lock exactly the way `vcs resolve --lock` does, then materializes each
distinct verified source as `<out-dir>/<source_hash>.ls` — nothing is
written unless every pin verifies. This is the `DepLocator`-shaped
"local path" case above, made concrete: a plain directory of files is
already a valid resolution target, because §6 found that `import` needs
no new mechanism to consume one.

## 5. What this does not solve

- **Editions across a dependency boundary.** `editions.md` solved "a
  file predates a feature" for one program's own files. Whether a
  *dependency* pinned at an older edition composes with a consumer on a
  newer one — does the newer compiler simply read the pinned file at
  its own declared edition, the way it already does within one program
  — needs its own reading of `editions.md` §7's open question, not
  assumed here.
- **Semantic versioning, or a human-readable version string at all.**
  Deliberately deferred: §4.5 makes a hash the only thing actually
  depended on, and a `"1.2.3"` string, if it exists, is a tag a
  publisher attaches for a human reader, never consulted by the
  resolver itself.
- **A hosted hub, a search index, anything like `lex pkg search`.**
  `lex-cli/src/pkg_search.rs` is real prior art for when this is worth
  building; §4.6 argues it is not the first thing needed.
- **Signing, or trust-of-signer.** `lex-os-capsule`'s own stated gap —
  *"verifies a signature is valid for a given key, not that the key is
  trusted"* — is inherited here unresolved, not solved by anything
  above.
- **The effect-vocabulary plateau itself.** A package system makes the
  71% figure `hash-stability.md` measures *visible and mechanically
  checked* (§4.2, §4.4) rather than silently assumed away. It does not
  make the vocabulary stop moving, and should not try to — that is a
  question about the language, not about distributing it.

## 6. What would make this real

**Done: `lex-sys vcs resolve`** — the minimum resolver in §4.1–§4.2
against a real, genuinely separate `.lex-sys-vcs` store, exactly as
proposed here. The answer to whether the recheck step is as cheap in
practice as this document assumed: yes, for a store this size — a
re-parse and a re-typecheck per distinct source file, deduplicated
across the declarations that share one, is the same cost `lex-sys
check` already pays for an ordinary program. What it found that this
document had not named: source itself had nowhere to live (`Blobs`,
now built).

**Done too: `lex-sys vcs lock` / `vcs resolve --lock`** — §4.5's naming
half now has a lock file format (`crates/lex-sys-vcs/src/lock.rs`,
name-keyed rather than `Manifest`'s `SigId`-keyed, because a consumer
looks a dependency up by the name it wrote down, not by a hash it does
not have yet) and a command that writes one, plus `resolve`'s own
`--lock` flag to check only what was pinned rather than a whole store.
One prediction this section made turned out wrong and is corrected at
§4.5 itself: re-locking a name is an overwrite, not a refusal, because
re-running the command by hand is a deliberate choice, not the
automatic substitution the guarantee is actually about.

**Done too: `lex-sys vcs fetch`** — a lock's pins, re-verified and
materialized on disk as `<source_hash>.ls` files, one per distinct
source. This was scoped, in the previous revision of this section, as
needing `import` to learn to read a lock at compile time. Building it
found that premise wrong: `modules.md` §4.2 already means a program is
the set of files named on the command line and `import` already resolves
a name against whichever of them declares it — there is no search path,
no manifest lookup, nothing in the compiler that only knows about
locally-written files. A fetched file is exactly such a file the moment
it lands on disk. So the real next slice needed zero compiler changes;
it needed only a place to fetch *to*, which is what `vcs fetch` is. This
is confirmed end to end by a test that publishes a dependency, locks it,
fetches it, and then builds and runs a second, separate source file that
`import`s it — the fetched file, unmodified by anything package-specific
in the compiler.

**Done too, and the first time any of this ran on a real program rather
than a fixture: `net.sockets`** (`packages/net-sockets/sockets.ls`) —
`examples/serve/serve.ls`'s and `examples/results_stub/results_stub.ls`'s
own duplicated `extern fn`s and byte helpers, published once, locked by
each consumer, and `import`ed rather than copy-pasted. This is what found
`vcs publish`'s only real gap so far: it had never been asked to publish
a foreign declaration, which has a signature and no lowered body, and the
publish path read effects off a lowered body unconditionally. Fixed by
reading `Program::externs` when no lowered function matches (§4's own
status header has the detail). Two real, already-tested example programs
still build, still run, and still answer identically on both backends
after the extraction — the strongest evidence yet that a fetched package
composes with the rest of the toolchain exactly the way an ordinary file
does, because it *is* one.

What is not real yet, and is the actual next slice: `resolve --lock`
and `vcs fetch` still check one store against one lock, not a
*program's* dependency graph. There is no recursion across a closure of
stores (§4.2's "each dependency's own dependencies resolve against that
dependency's committed lock"), no cycle detection, no diamond-conflict
refusal — those all need more than one dependency in the loop at once,
and nothing here has tried that yet. `standard-library.md` §2.1's
"versioning the library separately from the compiler" is still the
likely candidate for what exercises that next, and `--std` is still
the wrong first dependency to force it with — nothing here has a
reason to make it optional.
