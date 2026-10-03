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
> published. **The lock-driven, recursive, multi-package resolution
> §4.2's first paragraph describes is built too, as of §4.6**: a store
> may carry its own `requires/*.json`, written by `vcs publish
> --requires <lock>:<dep-store>`, and `vcs resolve`/`vcs fetch` walk it
> however deep, detecting a cycle and catching a diamond conflict rather
> than guessing. **§4.7's first fetch step is built too**: `lex-sys vcs fetch
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
>
> **A second real package, `net.connect` (`packages/net-connect/
> connect.ls`), and the first program with two real dependencies at
> once.** Six more examples turned out to duplicate `net.sockets`'
> declarations independently -- `examples/fetch/`, `examples/report/`,
> `examples/collect/`, `examples/vsock/`, `examples/agent_guest/`,
> `examples/agent_supervisor/` -- and they split cleanly on
> `net.md` §1's own line, "the two directions do not meet": the inbound
> ones (`collect`/`agent_supervisor`, joining `serve`/`results_stub`)
> want `net.sockets` alone, the outbound ones want `net.sockets` plus
> `connect`, which is now its own package rather than joining
> `net.sockets`, for the same reason. `examples/fetch/fetch.ls` migrated
> onto both, proving what §6 flagged as unbuilt is not the same question
> as "does more than one dependency work at all": **it already does**,
> with no new tooling -- two independent `vcs lock`/`vcs fetch` pairs,
> composed at the same `build` command line, each re-verified
> separately. What §6 still means by "not built" is narrower: a program
> that depends on a package which itself has dependencies (a true
> closure), and conflict detection when two dependencies disagree about
> a third. Composing N *direct*, independent dependencies was never the
> open question; it just had not been tried against a real program
> before now.
>
> **Found and fixed a real, separate compiler gap, not a package-system
> one**: `lex-sys authority`'s `foreign_symbols` field was not
> reachability-pruned the way effects and `Program::funcs` are -- it
> listed every `extern fn` declared in the compiled unit, called or not,
> so importing a package that declares more than a program calls made
> the report over-name what the program reaches. Fixed in `lex-sys-ir`
> (`fold::collect_extern_refs`/`reachable_externs`), read-only over
> `Program::externs` so `Callee::Extern`'s index and codegen are
> untouched. `docs/authority.md` §3 and `docs/under-a-grant.md` §6 have
> the detail.
>
> **The other five of those six duplicating examples migrated too.**
> `examples/report/`, `examples/vsock/`, and `examples/agent_guest/` now
> `import net.sockets`/`net.connect`, the same shape as `fetch.ls`;
> `examples/collect/` and `examples/agent_supervisor/` now `import
> net.sockets` alone, the same shape as `serve.ls`/`results_stub.ls`.
> Nothing in this section's own "not built yet" gap moved: this was nine
> already-real files converging on the two already-real packages, not a
> new question about composition.
>
> **A third package, `agent.wire` (`packages/agent-wire/wire.ls`), with
> no `extern fn` in it.** `examples/vsock/vsock.ls` and
> `examples/agent_guest/agent_guest.ls` duplicated the same five pure
> functions -- an `AgentViewMsg` decoder, byte-for-byte, the same "two
> real askers" bar `net.sockets` cleared first. Confirms a package is
> just a `lex-sys-vcs` store, indifferent to whether what it publishes
> declares against libc or is ordinary Lex with a body. §6 has the
> detail, including the one real publish-time constraint it found
> (`--std` was not available to `vcs publish` -- corrected: §4.8
> built it) and why the JSON
> escaper on the encoding side of the same wire protocol stays
> unextracted.
>
> **Closure resolution, built (§4.6): a package can depend on a
> package.** Found from a real fourth duplication — `examples/collect/
> collect.ls` and `examples/agent_supervisor/agent_supervisor.ls`
> duplicate `content_length_of`/`read_request` byte-for-byte, but the
> package that would hold them needs `net.sockets` itself, and `vcs
> publish` refused that outright until now. `vcs publish --requires
> <lock>:<dep-store>` (repeatable) writes a store-carried
> `requires/*.json`; `vcs resolve`/`vcs fetch` walk it recursively,
> refusing a cycle or a diamond conflict (two paths pinning the same
> store's same name to two different heads) rather than guessing.
> **Found one real thing wrong with this section's own first draft**:
> `lex-sys-id::identify()` is not resolution-independent the way it
> claimed — a call's contribution to its own caller's body hash is the
> *resolved* callee's signature when the callee resolves, so computing
> identity without the dependency present recorded the wrong hash,
> refusing on the very first `vcs resolve` with a body that had not
> moved at all. §4.6 has the correction and the fix. Proven against
> synthetic packages built for exactly that, not yet against
> `http.request` itself, which is the next slice.
>
> **`packages/http-request/` built too, the real case §4.6 was found
> from.** `content_length_of`/`read_request` move into `request.ls`,
> published with `--requires <net.lock>:packages/net-sockets/
> .lex-sys-vcs`; `collect.ls` and `agent_supervisor.ls` both `import
> http.request` instead, and drop their separate `net.sockets` lock
> entirely — one fetch of `http.request` transitively materializes the
> whole `net-sockets.ls` file too, since a store is always exactly one
> file. **Found a second real thing wrong with this section's own first
> draft**: `Requirement.store`, read back as a bare path, was silently
> interpreted against *whoever runs `vcs resolve`/`vcs fetch` next*'s own
> working directory rather than the one it was recorded relative to —
> invisible against the synthetic tests above (their stores are always
> absolute paths under one scratch directory), but real against
> `http.request`+`net.sockets`, fetched from a working directory other
> than the repo root. Fixed by recording it relative to the depending
> store's own directory instead, and joining rather than reading it bare
> on the way back. §4.6 has the correction and the fix.
>
> **`net.connect` extended, and a fifth package, `http.response`.**
> Hashing every function body across `examples/`, `packages/`, `std/`
> and `benches/` and grouping the ones that collide -- rather than
> rereading files by eye -- found a fifth duplicated cluster: `fetch.ls`,
> `report.ls` and `agent_guest.ls` each declare the same six functions.
> `octets_of`/`port_of`/`address`/`connect_to` are generic to any
> outbound program and joined `net.connect` itself (growing an
> already-published store is the same act as its first publish, just a
> second `vcs publish` against the same `--store`); `send_all`/
> `status_of` are HTTP-specific and became `http.response`, the
> client-side mirror of `http.request`. Both now `import net.sockets`
> too -- its third and fourth real consumer, closure resolution doing
> the same work again with no changes needed. **Found one thing the
> design had not foreseen**: two *sibling* packages transitively
> requiring the *same* dependency means fetching each into its own
> output directory -- every earlier multi-package example's own shape --
> now fetches that shared dependency twice, under two different paths,
> which `build`/`check` correctly refuse as a duplicate declaration. Not
> a resolver gap (`vcs resolve` on either store alone is silent), only in
> how a consumer composes two fetches: one shared output directory is
> enough, since `vcs fetch` writes each file as `<source_hash>.ls` and a
> second fetch of the same content overwrites the same path rather than
> writing a new one.

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

### 4.6 Closure resolution: a package that depends on a package

The real motivating case, found rather than invented: `examples/collect/
collect.ls` and `examples/agent_supervisor/agent_supervisor.ls` — both
already `import net.sockets` — duplicate a second pair of functions
byte-for-byte, `content_length_of` and `read_request` (63 lines
together), an HTTP request reader built on top of `sockets.read`. That
is the same "two real askers" bar every package here has cleared, but
extracting it hits a wall none of the first three packages did: the new
package's own source would need `import net.sockets` too, and confirmed
directly (`vcs publish` on a file importing `net.sockets` today) —

```
error: no module `net.sockets` in this program; a module exists where a
file declares it
```

— because `vcs publish` only ever sees the one file it is publishing
(§4.1), and `vcs resolve --lock`/`vcs fetch --lock`'s own re-check
(`verify_selected` in `crates/lex-sys/src/vcs_cli.rs`) type-checks each
pinned source blob **alone**, the same restriction. Both are real, not
incidental: a package with no dependency of its own (all three built so
far) never needed either to change.

**One prediction this section made turned out wrong, and building the
fix found it rather than assuming it.** The first draft here claimed
`lex-sys-id::identify()` is purely structural — a hash of a function's
own written signature and body, never of what an imported callee's
declaration actually says — and that only the soundness gate below
needed a dependency's source in scope. Wiring `vcs publish --requires`
against a real dependency and then re-verifying it with `vcs resolve`
found that false on the very first try: identity computed with the
dependency absent and identity computed with it present disagreed,
`vcs resolve` refusing a body that had, in truth, not moved at all.
`lex-sys-id::qualified_name` (`crates/lex-sys-id/src/lib.rs`) is why —
a call's contribution to its caller's body hash is the *resolved*
callee's own signature hash (`tag::FREE`) when the callee resolves, and
only the bare written name (`tag::NONE`) when it does not; a callee's
signature changing is meant to count as a body change for its caller,
which is sound, but it means identity is not resolution-independent
after all. Fixed by computing `identify()` from the same merged,
dependency-resolved `Ast` the soundness gate below builds, keeping only
the names a first pass over the primary file *alone* declared (reliable
regardless of resolution, since it is asking "does this function
exist," never "what does its call hash to") to filter a dependency's
own declarations back out before publishing.

**What does not need to change**, once identity itself is computed
correctly: resolving `import net.sockets` is `lex-sys-ir`'s job
(`defs.rs`'s module-scope pass, run during `lower_all`), not a second
identity pass — the "no module" refusal above comes from there, and the
soundness gate — confirming a pinned declaration still type-checks
under today's compiler, the actual point of re-verifying rather than
trusting a hash (`hash-stability.md`'s whole finding) — is a `lower_all`
call already made from an `Ast` built by merging named texts
(`parse_program`'s own loop, `crates/lex-sys/src/main.rs`); nothing
about multi-file lowering itself is new, `net.connect`+`net.sockets`
compose that way at `build` time already. What is new is doing it from
already-verified **text in memory** rather than files a caller named on
a command line — `vcs publish`/`verify_selected` hold dependency source
as trusted strings, not paths, so the shared helper this needs
(`parse_texts`, a text-only version of `parse_program`'s inner loop) has
to accept `(name, text)` pairs directly.

**A second prediction this section made turned out wrong too, and again
building the real package (`packages/http-request/`, not the synthetic
stores the mechanism was first proven against) found it, not a review of
the design.** The first draft's `Requirement.store` bullet, below, said a
repo-relative path recorded at publish time "stays valid for every
consumer... the same way every existing example's own command already
assumes running from the repo root" — true of every hand-typed `cargo
run` command in this document, but not of `cargo test`'s own integration
binaries, which run with their crate directory as their working
directory, not the repo root. Reading `req.store` as a bare path and
letting the OS interpret it against *whoever calls resolve/fetch next*'s
own working directory means the exact same closure walk succeeds from
one working directory and fails from another — reproduced directly:
`vcs fetch --store packages/http-request/.lex-sys-vcs ...` from the repo
root fetches `net.sockets` transitively without complaint; the identical
command, with both `--lock`/`--store` made absolute first, run from
`crates/lex-sys/` instead, refuses with `put`/`read ... is no longer
published at packages/net-sockets/.lex-sys-vcs` — the *store's own*
`--store` argument was absolute and correct, but the *string inside
`requires/0.json`* was still read bare and resolved against the wrong
base. Fixed the same way `import` itself is never working-directory-
sensitive: `Requirement.store` is now recorded **relative to the
depending store's own directory**, computed lexically at publish time
(`relative_from`, comparing path components, never touching the
filesystem, since the store being published to may not exist yet), and
resolved by **joining it against that store's own already-resolved
path** (`store.join(&req.store)`) rather than reading it as a stray path
of its own — so the walk composes correctly however deep, and however
its own top-level `--store` argument was spelled, with no dependence on
any process's working directory at all.

**The shape, concretely:**

- **A store may carry `requires/*.json`** alongside `manifest.json`/
  `ops/`/`sources/` — each file a `Requirement { store: String, lock:
  Lock }`: `store` is a relative path from *this store's own directory*
  to the dependency's, computed once at publish time and joined against
  the depending store's own path whenever it is read back — never
  interpreted against a process's own working directory, the correction
  above. `lock` is an ordinary `Lock` (§4.5's format,
  unchanged) — the *package's own* pin into its dependency, chosen once
  at publish time exactly the way a program's own `net.lock` is chosen
  today. One file per distinct dependency store, the same "one lock per
  store" shape a program with two direct dependencies already uses
  (`examples/fetch/net.lock` + `connect.lock`).
- **`vcs publish` grows `--requires <lock-file>:<dep-store>`
  (repeatable).** For each one: resolve and verify it exactly the way
  `vcs resolve --lock` already does (reusing `select_locked`/
  `verify_selected` unchanged) to get real, trusted dependency source;
  feed that alongside the primary file into the new soundness gate; on
  success, publish exactly as today (`identify()` on the primary file
  *alone*, unaffected by any of this) and write `requires/` from the
  `--requires` pairs given. Requirements are whole-store, last-publish-
  wins metadata, not diffed or versioned per declaration — consistent
  with every package here so far having exactly one publish, ever
  (`vcs-publish.md` §5: incremental publish is not built yet either).
- **`vcs resolve`/`vcs fetch` walk `requires/` recursively before
  verifying.** A new `resolve_requirements` gathers every transitively
  required source, bottoming out at a leaf store (no `requires/` at
  all, today's exact behaviour, unchanged) and threading two things
  through the whole walk rather than resetting per level, because
  either violation can appear between siblings as easily as between a
  level and its own ancestor:
  - **a cycle** — the same store's canonicalized path already on the
    current path — refused, naming the chain;
  - **a diamond conflict** — the same `(store, name)` pair pinned to
    two different `sig_id`s by two different paths through the graph —
    refused, naming both pins. Two paths agreeing on the same head is
    not a conflict and is not refused; only disagreement is.
  `verify_selected` gains an `extra_context: &BTreeMap<source_hash,
  text>` parameter — the closure's gathered text, merged into every
  group's `lower_all` call so an `import` in the group's own blob
  resolves — everything else about it, including the per-entry
  `SigId`/`StageId` check, is unchanged. `vcs fetch` writes the whole
  closure's text to `-o`, not just the directly-locked store's own, so
  one fetch of `http.request` hands back `net.sockets`' source too — a
  consumer of a package that has a dependency never has to know that,
  let alone fetch it themselves.

What this still does not solve, on purpose: **conflict resolution
beyond refusal.** Two disagreeing pins are refused, never reconciled
(no "pick the newer one," no semver range) — the same reason §5 defers
semantic versioning: a hash is the only thing actually depended on, and
there is no rule here for choosing between two different hashes a human
did not choose between explicitly.

### 4.7 Distribution: no hub required to start

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

### 4.8 A package that imports `std`

**Built.** Until this, `vcs publish` parsed its input alone, so a
package could not `import std.math`: the import resolved against
nothing and the publish was refused. Every package so far worked around
it by re-declaring the few functions it needed (`net.sockets`'s
`put`/`put_nat`), which is the duplication `std` exists to end.

- **`vcs publish --std`** merges the bundled library (the same bytes
  `build --std` uses) into the publish's own parse, lowering, identity
  computation and soundness gate. It is a flag, not automatic: a
  publish that imports `std` without it, directly *or through a
  `--requires` package*, is refused and the refusal names the flag.
  `--std` is the same consent `build` asks for, and a package quietly
  widening its own dependencies would be one a consumer cannot read off
  its command line.
- **Attribution is by `(module, name)`.** `FunctionId` gained a dotted
  `module` (not part of either hash -- a hash never mentions a module).
  With `std` merged in, a package's `abs` and `std.math`'s `abs` are two
  functions; attributing by name alone published the library's under the
  package's store (measured by mutation: the own-functions test fails
  with the name-only rule). Effects are looked up the same way.
- **`resolve`, `lock` and `fetch` need nothing from the user.**
  `verify_selected` brings the library in whenever the blob it is
  checking, or anything in its `requires/` closure, imports `std`. That
  is read off the parsed imports, never off `ManifestEntry.uses_std`:
  the flag is informational (it marks `vcs log` rows and is
  `#[serde(default)]`, so older manifests still load), and a test
  forges it to `false` and shows `resolve` still verifies against the
  library.
- **`fetch` does not write `std` into the output directory.** The
  library belongs to the compiler, not to the package; `fetch` prints a
  note that the consumer must build with `--std`, and a consumer built
  without it is refused rather than compiled against a library that is
  not there.

**The first real consumer is `http.server`** (`packages/http-server/`, `http-server.md`): 730 lines that import `std.http`, `std.json`, `std.buffer` and `std.conns`, published with `--std`, locked and fetched by `examples/api` like any other package.

**No pin on `std` itself.** The store records no hash of the library it
was published against. A package function's body hash includes the
*signature* hash of each callee it resolved (`identify`), so a std
function whose signature changed moves the dependent's body hash and
`resolve` reports "body moved" -- loudly, as it does for any drift. A
std function whose body changed but whose signature did not leaves the
package's identity alone, which is the right answer: the consumer gets
the library its own compiler carries. That follows from how `identify`
treats callees; it is not separately tested here, because `std` cannot
be varied under a test. If the library ever needs to be pinned
(reproducibility across compiler versions), the place is a `std`
identity in the store's `requires/` metadata, not a manifest flag.

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
  building; §4.7 argues it is not the first thing needed.
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

**Done too: `net.connect` (`packages/net-connect/connect.ls`) and a real
program with two dependencies at once.** `examples/fetch/fetch.ls`
locks and fetches `net.sockets` and `net.connect` independently — two
`vcs lock`/`vcs fetch` pairs, composed at the same `build` command
line — and builds, runs, and answers a real request exactly as before
either package existed. This answers a question this section's previous
revision had conflated with the one below: composing *N direct,
independent* dependencies needed no new tooling at all, only a second
real package to try it against. (It also found a real compiler gap,
unrelated to packages: `authority.md` §3.)

**Done too: every other duplicated consumer moved onto both packages.**
`examples/report/`, `examples/vsock/`, and `examples/agent_guest/` --
named above as candidates for the `net.connect` move and left there --
now `import net.sockets` and `net.connect` the same way `fetch.ls` does;
`examples/collect/` and `examples/agent_supervisor/` now `import
net.sockets` alone, the same way `serve.ls`/`results_stub.ls` do. Every
one of the nine files that used to declare its own copy of these
`extern fn`s now imports the package instead, verified the same way
each earlier one was: locked, fetched fresh, and built. `docs/net.md`
§5 has the recount.

**Done too: `agent.wire` (`packages/agent-wire/wire.ls`), the third real
package and the first with no `extern fn` in it at all.**
`examples/vsock/vsock.ls` and `examples/agent_guest/agent_guest.ls`
duplicated the same five pure functions -- `find_after`/`end_of_quoted`/
`goal_start_of`/`goal_end_of`/`step_of`, a decoder for one field of a
real `lex-os-proto` `AgentViewMsg` line -- byte-for-byte, exactly the
"two real askers" bar `net.sockets` itself cleared first. Everything
about publishing and fetching a package works the same whether its
declarations are `extern fn` against libc or ordinary `fn` with a body:
`vcs publish` type-checks and hashes either kind identically, and the
one real constraint this slice found -- `vcs publish` never makes
`--std` available (`cmd_publish` parsed with `with_std: false`
unconditionally, unlike `build`/`check`; **no longer true** -- §4.8) --
was already true for
`net.sockets`'s own `put`/`put_nat`, which is why neither package
imports anything from `std`. The escaper on the other side of this same
wire protocol (`append_json_escaped` in `vsock.ls`, `put_escaped` in
`agent_supervisor.ls`) stays unextracted: same escaping rule, but two
different signatures (one writes into a `std.buffer.Buffer`, the other
into a fixed slice with a cursor, because `agent_supervisor.ls` has no
`Heap` to spend), so bundling them would be guessing at a shape neither
file asked for rather than naming one that is already duplicated.

**Done: closure resolution (§4.6), against a real motivating case
rather than the hypothetical one this section used to point at.**
`examples/collect/collect.ls` and `examples/agent_supervisor/
agent_supervisor.ls` duplicate a fourth byte-for-byte pair,
`content_length_of`/`read_request`, and extracting it needs a package
that itself needs `net.sockets` — a true closure of stores, not just
several direct ones, the gap this section used to say was not built.
`vcs publish` grows `--requires <lock-file>:<dep-store>` (repeatable),
writing a store-carried `requires/*.json`; `vcs resolve`/`vcs fetch`
walk it recursively, refusing a cycle (a store's own canonicalized path
already on the current walk) or a diamond conflict (the same
`(store, name)` pinned to two different heads by two different paths)
rather than guessing. Building it found `identify()` is *not*
resolution-independent, contrary to this section's own first draft —
§4.6 has the correction and the fix. `packages/http-request/` itself —
the package this gap was found *from* — is the next slice, not this
one: this PR proves the mechanism against synthetic packages built
for exactly that, before trusting it with a real one.

**Done too: `packages/http-request/` (`request.ls`), the real motivating
case §4.6 was found from, and the first package that itself depends on
another.** `content_length_of`/`read_request` move out of `collect.ls`
and `agent_supervisor.ls` into one file, published with `--requires
<net.lock>:packages/net-sockets/.lex-sys-vcs`; both examples now `import
http.request` instead of duplicating the pair, calling
`request.content_length_of`/`request.read_request`. A store is always
exactly one file (`vcs publish` refuses more than one input), so
fetching any of `http.request`'s own declarations transitively
materializes the *whole* `net-sockets.ls` file too — both examples drop
their own separate direct `net.sockets` lock entirely, since the one
`http.request` fetch already brings in every `sockets.*` declaration
they call directly. Publishing it also found the second wrong prediction
§4.6 now carries its own correction for — `Requirement.store` read as a
bare, working-directory-relative path, rather than resolved against the
depending store's own directory — surfaced only once a real,
non-synthetic dependency pair was fetched from a working directory other
than the repo root.

**Done too: `net.connect` extended, and a fifth package,
`packages/http-response/` (`response.ls`).** `examples/fetch/fetch.ls`,
`examples/report/report.ls` and `examples/agent_guest/agent_guest.ls`
turned out to duplicate a fifth cluster, six functions byte-for-byte
this time — `octets_of`/`port_of`/`address`/`connect_to` and
`send_all`/`status_of` — three real askers each, found by hashing every
function body across `examples/`, `packages/`, `std/` and `benches/`
and grouping the ones that collide, rather than by rereading files by
eye the way every earlier slice here found its own duplication. The
first four are generic to any outbound program, not HTTP-specific, and
joined `net.connect` itself — growing an already-published store with
more declarations turns out to be the same act as its first publish,
just a second `vcs publish` against the same `--store`: unchanged
entries (`connect` itself, here) are skipped, only the new ones log.
`send_all`/`status_of` are HTTP-specific and became `http.response`, a
new, fifth package and the client-side mirror of `http.request`: that
one reads a request head server-side, this one writes a request fully
and reads a response's status line client-side. `connect_to` needs
`socket`/`close` and `send_all` needs `write`, so both now `import
net.sockets` too — the third and fourth real consumers of that package
alongside `http.request`, `docs/package-system.md` §4.6's own closure
resolution doing the same work a third and fourth time with no changes
needed. One thing the design doc had not foreseen: two *sibling*
packages (`net.connect`, `http.response`) transitively requiring the
*same* dependency (`net.sockets`) means fetching each into its own
output directory — the pattern every earlier multi-package example
used, back when no two packages shared a transitive dependency —
now fetches `net.sockets` twice, under two different paths, which
`build`/`check` correctly refuse as a duplicate declaration. Not a gap
in the resolver itself (`vcs resolve` on either store alone is silent),
only in how a *consumer* composes two fetches: fetching both locks into
one shared output directory is enough, since `vcs fetch` writes each
file as `<source_hash>.ls` and two fetches of the same content
overwrite the same path rather than writing two different ones.
`examples/README.md` and the conformance test harness
(`fetch_net_dependencies`) both moved to that shape.

## 7. Packages across repositories

> **Status: steps 1 and 2 are built, and §7.7 records what building them
> showed. Everything from §7.5 on is named and not built.**

### 7.1 What asked for it

Sections 4 to 6 built a package system that works inside one checkout:
`vcs fetch` takes `--store <dir>`, a directory on the same disk, and the
only example of a package used from another program is `examples/api`
consuming `packages/http-server` from the *same repository*. The first
programs written outside this repository did not use it. `lexsys-hooks`
(a webhook service, four sibling repositories) builds like this:

```sh
git clone lex-sys; git clone lexsys-log; git clone lexsys-hooks
(cd lex-sys && git checkout $LEX_SYS_REV && cargo build --release)    # the compiler
(cd lexsys-log && git checkout $LOG_REV)                              # a library
scripts/build.sh   # names every file of http-server and of the log by relative path
```

Two SHAs in CI, three clones, a file list written by hand, and no check
that what was cloned is what the author tested except the SHA. That is
exactly the ritual a lock file exists to remove, and `lexsys-web`,
`lexsys-pg` and `lexsys-cache` each repeat part of it (`lexsys-web` needs
`lexsys-schema` and `http-server`). The two-asker bar of `CONTRIBUTING.md`
is cleared by a count, not an argument.

Trying to use the existing machinery for `lexsys-log` found three things,
each reproduced rather than assumed:

1. **A lock says what, not where.** `Lock` holds `name -> (sig_id,
   stage_id, source_hash)`; `vcs fetch` takes a local directory. The only
   way to get the store is to clone its repository by hand.
2. **`lexsys-log` cannot be published.** `vcs publish crc.ls` fails with
   ``internal: `crc_table` has an identity but no lowered function or
   extern``. `lex-sys-id::identify` gives a `static` two identities (its
   type and its body, `compile-time-data.md` §2) and `cmd_publish` reads a
   declaration's effects from a lowered function or an extern, which a
   `static` is neither. This is the same shape as the `extern fn` gap §6
   records for `net.sockets`, one declaration kind further along.
3. **One file per store.** `lexsys-log` is four modules (`crc`, `record`,
   `segment`, `log`), each importing the one before. Published one file at
   a time that is four `vcs publish` runs in dependency order, each
   `--requires` taking a lock file for the one before; correct, and a ritual
   nobody will perform by hand on every commit.

### 7.2 What `lex-lang` already learned about this

`lex pkg` is a working package manager that fetches from git and from a
hosted registry, and it has paid for lessons this design would otherwise
repeat. Read from `crates/lex-syntax/src/workspace.rs`,
`crates/lex-syntax/src/lock.rs` and `crates/lex-cli/src/pkg.rs`:

| `lex-lang` | What happened | What this design does with it |
|---|---|---|
| A git dependency is `{ git, rev \| tag \| branch }` in `lex.toml`; the default is the branch head ("not reproducible — pin for releases") | A *moving* ref was cached under the package's name, so a checkout made once was read for months: a rename upstream was invisible and `lex check` reported `unknown_variant` against an interface that no longer existed (#1005) | A lock records a full commit hash and never a ref. A ref is resolved once, by the command that writes the lock, and the cache is keyed by the hash, so staleness cannot happen rather than being detected |
| Resolving a moving ref ran `git ls-remote` once per import site | 13 `ls-remote`s of one repository for one small file, about two minutes (#1015); then a per-process answer cache, because CI running `lex check` per file went from 55 s to about 10 minutes | Nothing on the build path asks the network a question. `fetch` asks for a hash it already has, which is a directory lookup, and only goes to the network when the directory is missing |
| `rev` could not be shallow-cloned: a full clone, then `checkout` | Cost grows with the dependency's history | `git fetch --depth 1 <url> <hash>` into a fresh repository (hosts that serve a commit by hash, which GitHub does), falling back to a full fetch only if that is refused |
| Git dependencies carry **no lock entry**; "hosted verification resolves dependencies only through `lex.lock` pins into registry stores (it never fetches git)" (#944) | Two tiers: pinned-and-checkable (registry) and convenient-and-unpinned (git) | One tier. A pin is always a hash of source (`Lock`'s `source_hash`), so *the transport carries no trust* and git can be the transport: whatever git delivers is re-parsed, re-typechecked and re-hashed by `vcs fetch` exactly as a local store is |
| `lex = "0.10.15"` as a toolchain floor in `[package]`, which nothing read until #803 | A dependency moved onto a newer stdlib, installed without complaint, and the mismatch surfaced as `unknown_field` errors naming a function nobody in the consuming repo had written; now `lex pkg install` refuses (`--ignore-lex-floor` to override) | **Not in this slice, and the reason it is the next one**: `hash-stability.md` is the same failure at a larger scale. Section 7.5 |
| Prebuilt release tarballs with a `.sha256`, installed by `curl \| tar` in docs and CI | CI installs a pinned `lex` in seconds | Section 7.5; today a lex-sys consumer runs `cargo build --release` of the compiler in CI |
| A hosted registry (archive download, immutable releases, signed contracts, `--trusted-keys`) | Real infrastructure, built after git | Out of scope, as §4.7 already argued: a registry would be a name-to-pin index in front of the same stores |
| `import "pkg/module"` resolved by walking up to the nearest `lex.toml` | The compiler reads the manifest | **Deliberately not copied**: `modules.md` §4.2 and §6 above found that `import` needs no search path, and the compiler stays ignorant of where files came from |

### 7.3 Step 1: an origin on the lock, and a fetch that can go and get it

`Lock` gains one optional field, because a lock already addresses exactly
one store (`vcs fetch --lock` takes one `--store`):

```json
{ "origin": { "git": "https://github.com/alpibrusl/lexsys-log",
              "rev": "c0c3852541e927c3e893331207f1a0aaf184bc2e",
              "path": ".lex-sys-vcs/log" },
  "entries": { ... unchanged ... } }
```

* `rev` is a full commit hash (40 hex digits, or 64 for a SHA-256
  repository) and nothing else. A branch or tag name is refused at the
  point of writing and at the point of reading.
* `path` is the store's directory inside the repository (default
  `.lex-sys-vcs`).
* An older lock without `origin` loads as before.

**Commands.**

* `vcs lock --git <url> --rev <hash> [--path <dir>] -o <file> (--all |
  <name>...)` makes the store available (below), reads its manifest, and
  writes the pins and the origin. `--ref <name>` in place of `--rev`
  resolves the name once, through `git ls-remote`, prints the hash it
  found, and writes the hash. `--all` pins every declaration of the store;
  a library locked a name at a time is a list nobody will keep.
* `vcs fetch --lock <file> -o <dir>` and `vcs resolve --lock <file>`: when
  the lock has an origin and no `--store` is given, the store is the cached
  checkout's `<path>`. `--store` still wins when given, for working on a
  dependency from a local checkout.
* **Closure.** A `Requirement` (§4.6) carries a `Lock`, so a package that
  requires a package in *another repository* records that lock's origin,
  and `resolve_own_requirements` uses the origin when there is one and the
  relative `store` path otherwise. Same-repository requirements, which is
  all of `packages/` and all of what step 2 publishes, keep working inside
  the checkout unchanged.

**The cache.** `$LEX_SYS_CACHE`, else `$XDG_CACHE_HOME/lex-sys`, else
`$HOME/.cache/lex-sys`; a checkout lives at `git/<rev>/`. (The first draft of this section keyed it by the repository's location too, `git/<blake3(url)[..16]>/<rev>/`; building it found that wrong in the useful direction: a commit hash *is* its content, so two mirrors of one commit should share a directory, and where it came from decides nothing. Corrected in place.)
It is created once, in a temporary directory beside its destination
(`git init`; `git fetch --depth 1 <url> <rev>`; `git checkout --detach
FETCH_HEAD`; `git rev-parse HEAD` must equal `rev`), and renamed into place,
so a concurrent or interrupted fetch leaves nothing half-made and a
directory that exists is complete. It is never updated: a different `rev`
is a different directory. Nothing evicts it; `rm -r` does.

**Why this is not "running the dependency's code"** (§4.3). `git` is run
with `core.hooksPath` pointed at nothing, `GIT_TERMINAL_PROMPT=0`, only the
`https`, `ssh`, `git` and `file` transports allowed (`ext::` and the other
remote helpers run programs, and `http` is cleartext), and no submodule is
initialised; the checkout is read, never built or executed.
Whatever the transport delivers is then held to the same recheck as a
local store: re-parsed, re-typechecked, and every identity recomputed
against the manifest and the lock. A hostile mirror can refuse to serve;
it cannot make `fetch` accept different source, because the pin is a hash
of the source and the commit hash only decides *which directory to look
in*.

**What stays as it is.** No project file, no version string, no
resolution of anything the lock does not name.

### 7.4 Step 2: publishing a library

1. **`static` is publishable.** A `static` has no effects (it is evaluated
   at compile time and cannot call out), so `cmd_publish` gives it the
   empty row instead of the internal error, and it is published and pinned
   like any declaration. Private statics (`crc_table`) are published too,
   as private functions already are; a consumer only ever names a public
   one.
2. **`vcs publish --dir <dir> --store <root>`** publishes every `.ls` file
   of a directory, each into its own store `<root>/<module>`, in dependency
   order. The order and the `--requires` are *derived*, not typed: each
   file's module name and imports come from its parse, an import of another
   file of the directory becomes a requirement on that file's store (a lock
   of all its declarations, with a relative `store` path inside `<root>`),
   an import of `std` needs `--std` as before, and any other import is
   refused naming the file and the module. A cycle is refused.
3. **Stores are regenerated, not appended to.** Publishing a changed
   declaration into a store that already has it is refused today
   (`vcs-publish.md` §5: incremental publish is not built), which would
   make `--dir` unusable after the first edit. A directory publish removes
   and rebuilds each module's store. This loses nothing a consumer holds:
   its lock pins a commit of the repository, and the store *at that commit*
   is what it fetches; the git history is the version history, which is
   also what §4.1 said a version is (a label over hashes).
4. **Deterministic.** Operations, manifests and blobs are content-addressed
   and carry no times, so publishing the same directory twice writes the
   same bytes; committing the store to the library's repository produces no
   churn. (This is a claim and is a gate item below.)

The store is committed to the library's repository, as `packages/*/
.lex-sys-vcs` already is here. A CI check that the committed store matches
`vcs publish --dir` of the source is natural and not part of this step.

### 7.5 Not in this step, in the order I would take them

3. **A project file** (`lex-sys.toml`: entry files, dependencies, the
   compiler it was written for) read by `build`, `check` and `test`, so a
   consumer's `scripts/build.sh` and its file list disappear, and a
   module-level `lock` derived from the `import` lines.
4. **A compiler pin that is checked.** `lex-lang`'s #803 is the argument:
   a floor nobody read produced errors that named the wrong thing. For
   lex-sys the needed fact is stronger than a floor, because
   `hash-stability.md` measured that most of this repository's history does
   not type-check under today's compiler: the project file records the
   compiler's source revision, `lex-sys --version` reports its own, and
   `build` refuses a mismatch with a message that says so. This needs
   `lex-sys` to *know* its revision, which it does not today.
5. **Prebuilt compiler releases**, so CI installs a pinned compiler in
   seconds instead of running `cargo build --release` each time.
6. **`update` shows the authority diff** (§4.4): what a bumped dependency
   can newly do, from `lex-sys authority --output json` of both sides.

### 7.6 The gate, fixed before the build

Steps 1 and 2 are done when all of these hold, measured, on the
`lexsys-hooks` / `lexsys-log` / `lex-sys` triple, and a gate that fails is
reported as failed, not loosened:

| | Claim | How it is checked |
|---|---|---|
| G1 | **`lexsys-hooks` builds from its own repository plus the compiler**: no `../lexsys-log`, no `../lex-sys` source tree for the packages | a clean directory containing a clone of `lexsys-hooks` and nothing else; `lex-sys vcs lock --git … --all`, `vcs fetch`, `build`; then `idempotency_test` and `chaos` pass on that binary |
| G2 | **A tampered checkout is refused** | one byte changed in a cached source blob (and, separately, in the manifest) before `fetch`: nonzero exit, nothing written to `-o` |
| G3 | **A cache hit needs no network and no `git`** | the second `fetch` run with `PATH` stripped of `git` succeeds |
| G4 | **A pin is a hash** | `--rev main` is refused; a lock edited to hold a branch name is refused at load; moving a branch of the origin changes nothing |
| G5 | **`lexsys-log` publishes, resolves and is deterministic** | `vcs publish --dir src` on all four modules succeeds including `static`; `vcs resolve` of the closure passes; publishing twice gives byte-identical trees; publishing after editing a function succeeds |
| G6 | **Mutation survivors are classified**, as for hooks: remove each check (hash length, `rev-parse`, tamper recheck, atomic rename, origin-wins in closure) and the test that dies is named | a table in the section that records the result |
| G7 | **Cost is reported, not gated**: cold fetch of `lexsys-log` and of `http-server`, cache-hit fetch, `publish --dir` time | numbers in this section |

### 7.7 What building steps 1 and 2 showed

**The gate (§7.6), as measured.**

| | Result |
|---|---|
| G1 | **Met.** In a directory holding a clone of `lexsys-hooks` and the compiler binary, and nothing else: `vcs lock --git … --all` for `lexsys-log` (commit `f4bde04`) and for `http-server` (a commit of this repository), `vcs fetch` of both, `build`. `idempotency_test` (including the 65,536-key stage) and `chaos` (2,000 events, power cuts) pass on that binary. The binary is **not byte-identical** to the one `scripts/build.sh` makes from sibling checkouts; the fetched files are named by hash and so sorted differently, and I did not chase the difference further than that, so "same behaviour under these suites" is the claim, not "same bytes" |
| G2 | **Met.** One byte changed in a cached source blob: `fetch` exits nonzero and writes nothing (`a_cached_checkout_that_was_changed_is_refused_and_nothing_is_written`) |
| G3 | **Met.** The repository deleted and `git` taken off `PATH`, the second `fetch` succeeds from the cache (`a_second_fetch_is_served_from_the_cache_without_git`) |
| G4 | **Met.** `--rev master` refused at the command and, in a hand-edited lock, at load; `--ref` resolved once and the hash written; a branch that moves afterwards changes nothing a lock pins |
| G5 | **Met.** All four modules of `lexsys-log` publish, `crc`'s `static` included; `vcs resolve` passes for each; publishing twice gives a byte-identical tree, and publishing after editing a function succeeds |
| G6 | **Met, with one unverified check**, below |
| G7 | Reported: cold fetch of `lexsys-log` 0.55 s and of this repository's `http-server` 1.34 s (a depth-1 fetch by hash from GitHub through this environment's proxy, 8.3 MB of cache for both); the second fetch of each, from the cache, 0.06 s for both together; `vcs publish --dir` of the four modules 0.15 s; the whole closure of `log` verified in well under a second |

**Mutants of the new code: twenty-one, twenty killed.** Killed (the test that died is in `vcs_remote.rs` unless noted): a `rev` that is not a full hash accepted (`lock.rs` unit test and `a_lock_holds_a_commit_and_never_a_name`); a `path` that leaves the repository accepted; an origin not validated on load; the checkout's `.git` kept; the checkout built in place instead of beside its destination (`a_fetch_that_fails_leaves_no_half_made_checkout`); the cache always refilled; an origin that beats an explicit `--store`; a requirement that ignores its lock's origin; a machine-specific path recorded in a requirement; modules published in alphabetical rather than dependency order; stores not cleared before a republish; a `static` refused; a program in the directory refused instead of skipped; no cycle detection; an unplaceable import accepted; `--ref` writing the name instead of the hash; a lock extended with a different store; every transport allowed; git hooks not disabled; `ext::` not refused by `validate`. Three of these **survived the first set of tests and each told something**: the machine-path check only looked for the cache's absolute path and the requirement held a relative path to it (the test now asserts the recorded store is empty); nothing exercised `vcs lock` on a lock that already pins another commit; and the hook and transport settings are invisible to any test that does not provide a hook or a cleartext server (a template directory with a `post-checkout` hook now proves the first; a listener that hangs up on connection proves the second). **Unverified: the `git rev-parse HEAD` comparison after the checkout.** It is a second line behind git's own object hashing and the full-length `rev` that `validate` demands, and I could not construct a repository in which the first fails and the second passes. It stays as defence in depth with no test.

**Found along the way.**

* **`vcs publish` could not publish a `static`.** §7.1 predicted this and it was the first thing to fail. A `static` is evaluated at compile time and cannot call out, so its effect row is empty; the fix is a third case beside the function and the extern the publish path already knew.
* **A module's qualifier is its last name segment.** For `module libx.base;` the call is `base.pick(i)`, not `libx.base.pick(i)`. Not a bug, but the tests' first draft assumed the other, and a directory of modules whose last segments collide would have to be told apart by the author.
* **A test hung instead of failing.** The cleartext-transport test first listened and checked afterwards; with the transport check removed, git connected, waited for an answer that never came, and the mutation run stalled for twenty minutes. The listener now hangs up at once.
* **The cache holds trees, not repositories.** The `.git` directory is deleted once the commit is verified: nothing later can run `git` in it by accident, and it is smaller. The cost is that a checkout cannot be updated in place, which is the design.
* **`--dir` skips a file with no `module` declaration** (a program such as `logtool.ls`) and says so. A library module that forgot its `module` line is skipped the same way, visibly.

**Not built, as §7.5 says:** the project file, the compiler pin, prebuilt releases, and the authority diff on update. Also not built and noticed: a CI check in a library's repository that its committed `.lex-sys-vcs` matches `vcs publish --dir` of its source (needs this change in a pinned compiler first); a `git` that is not installed is an environment error with git's own message and no suggestion; an origin whose server serves only branches and tags falls back to a full fetch of them, which was written and is not tested.

## 8. The project file

> **Status: design, written before the code.** §7.5 steps 3 and 4. What was built is recorded in §8.7 when it is.

### 8.1 What asked for it

§7 let a program depend on a library in another repository, and `lexsys-hooks` is the proof; it also shows what is still done by hand. Its `scripts/build.sh` lists the source files, runs `vcs fetch` once per lock, clears the output directory so an old lock's files do not become second declarations, and passes `build/deps/*.ls` to `build`. Its CI repeats the file lists for the unit tests. Its two locks are moved by a second script that takes two full commit hashes. And nothing says which compiler the sources were written for: CI pins a commit of `lex-sys` in a YAML file, which `lex-sys` itself cannot read, and `hash-stability.md` measured what that costs when it is wrong (71% of this repository's own history stops type-checking under today's compiler). `lex-lang` found the same thing from the other side: a toolchain floor was written in `lex.toml` for a long time and nothing read it, until a dependency that had moved to a newer standard library installed without complaint and failed with errors that named a function nobody in the consuming repository had written (#803).

### 8.2 The file

`lex-sys.toml`, at the root of a project:

```toml
[package]
name = "hooks"
lex-sys = "f804ce7e6fcf5717ea52442bf648a1fe81090f98"   # the compiler these sources were written for

[dependencies.log]
git = "https://github.com/alpibrusl/lexsys-log"
rev = "6b4f46fd045f9e6c1a3f4bda22ee9d850daf63d1"       # a full commit hash, never a name
path = ".lex-sys-vcs/log"                              # the store inside the repository

[[bin]]
name = "hooks"
sources = ["src"]                                      # files, or a directory: every .ls directly in it
std = true
out = "build/hooks"                                    # default: build/<name>
```

* **Unknown keys are refused**, not ignored: serde silently dropped `lex = "..."` in `lex.toml` for as long as nothing read it, and a misspelt key here would be the same trap.
* **`rev` is a full commit hash** (`Origin::validate`, §7.3). There is no separate lock file: a manifest of exact commits has no ranges to resolve, and the declaration pins that `vcs lock --all` would write are derived from the store at that commit and re-checked on every install. A lock appears when a dependency can name something that moves.
* **A dependency's own dependencies** (a package in another repository that a package requires) are not listed: they are in the store's `requires/` with their own origins (§7.3), and fetched with it.
* **Paths are relative to the directory holding `lex-sys.toml`**, found by looking from the current directory upwards.

### 8.3 The commands

* `lex-sys install` reads the file, checks the compiler (below), and for each dependency fetches its commit into the cache, re-parses, re-typechecks and re-hashes every declaration, and writes the sources to `build/deps/<hash>.ls`, clearing that directory first. It is the `vcs fetch` of §7 for every dependency of the project at once.
* `lex-sys add <name> <git-url> [--rev <hash> | --ref <name>] [--path <dir>]` adds a dependency: `--ref` (default: the repository's head) is resolved once and **the hash is what is written**; the store is fetched and checked before the file is touched; then the table is appended to `lex-sys.toml` (appended, so comments in the file survive) and the project is installed. A name that is already there is refused.
* `lex-sys build` with no file arguments builds every `[[bin]]` (`--bin <name>` one of them): it installs first, which costs a directory lookup and a recheck when everything is cached (0.06 s for the two libraries of `lexsys-hooks`), so there is no staleness to get wrong. With files it is `build` as before.

### 8.4 The compiler

`lex-sys --version` reports the commit it was built from: `build.rs` reads it from git (suffixed `-dirty` if the working tree has uncommitted changes), or from `LEX_SYS_REV` for a build outside a git checkout, and says `unknown` otherwise. `install` and `build` compare it with `[package] lex-sys` and **refuse a difference**, naming both; `--ignore-compiler-rev` is the escape hatch for someone who knows. A binary that does not know its own revision cannot be checked and is refused the same way: a floor that cannot be read is how #803 happened. The pin is on a *commit* because that is what the repository can say exactly; when compilers are released as binaries (§7.5 step 5) the release will embed the commit it was built from.

### 8.5 What this does not do

No version ranges, no registry, no feature flags, no dev-dependencies, no workspaces, no `remove` (delete the table), no `test` (the unit-test commands of `lexsys-hooks` are still written out), no linking options in `[[bin]]`. Each is a decision that waits for a program that asks, by the bar of `CONTRIBUTING.md`.

### 8.6 Distributing a program

Asked alongside this: does `lex-sys build` make a binary that a package can be shipped as, and should `lex-sys` make Docker images? Measured on `lexsys-hooks`: `build` makes a native executable (an ELF PIE, 169 KB), linked by `cc` (the `CC` environment variable names another linker), and the only shared library it needs is libc (`ldd`: `libc.so.6` and the loader). A static one builds with a linker that adds `-static` (`CC=./static-cc`, a three-line script): 1.2 MB, `not a dynamic executable`, with the glibc warning that `getaddrinfo` still wants the shared libraries at run time, which the service avoids by dialling IP literals (§16 of its design). So a program is distributable as a file today, and a static one runs in an image with nothing else in it. **No Docker daemon was available in the environment this was written in, so no image was built here**; that is a claim about the binary, not a test of an image. The recommendation, not built: `lex-sys` does not generate images (an image is a deployment decision, not a compiler's), a release is the binary, its checksum and `lex-sys authority --output json` of it (what it can do, in the form `lex-os-capsule` already signs), and an image of the *compiler* for CI is a thin layer over a release.

### 8.7 What building the project file showed

Built as designed in §8.2 to §8.4: `lex-sys.toml` (`[package]`, `[dependencies.<name>]`, `[[bin]]`), `lex-sys install`, `lex-sys add`, `lex-sys build` with no files and `--bin`, and the compiler's revision in `--version` (`build.rs`) checked against `[package] lex-sys`. About 380 lines in `crates/lex-sys/src/project.rs`, one of them shared with `vcs fetch`: `fetch_verified` was cut out of `cmd_fetch`, and both call it.

**Checked.** `lexsys-hooks` runs on it: `lex-sys.toml` replaces its two lock files, its `lock.sh`, the file lists in `build.sh`, and the second copy of the compiler pin in its CI (the workflow reads the commit out of `lex-sys.toml`, so there is one place to change). `lex-sys build` there takes 5.7 s from nothing, of which the installs are the same two libraries as in §7.7 and the rest is compiling. 11 conformance tests (`project.rs`), on local git repositories: add, install and build end to end with the program's exit code; a `--ref` is resolved once and the hash written; every refusal of `add` leaves the file as it was, and an install that fails *after* the file was written puts it back (a store whose blob was changed after publishing); a project file with an unknown key, an unknown section, a name for a `rev`, a path that leaves the repository, a `lex-sys` that is not a hash, duplicate programs, a program with no sources or a bad name is refused, each with its reason; the wrong compiler is refused naming both, and `--ignore-compiler-rev` goes on with a note; a moved pin replaces what the old one fetched; the project is found from a directory below it; `build <files>` is still the compiler it was; `--bin` picks one; an empty store is not a dependency.

**Mutants of the new code: twenty-three, twenty-two killed, one unverified.** Killed: the compiler check always passing; `--ignore-compiler-rev` ignored; the fetched directory not cleared; unknown keys allowed (at the top, and in a program); a dependency's origin not validated at load; `lex-sys` not validated; `add` without its rollback, allowing a duplicate, writing the ref's name instead of its hash, accepting any name, or skipping the store check; a project build that did not install first; `wants_project` always true or the search not going upwards; `--bin` ignored or an unknown one accepted; an empty store accepted; an install failure that does not name the dependency; duplicate or source-less programs; a `--version` without the revision; a stamp that is always `unknown`. **Eight of these survived the first tests, and each was a test that checked less than it said:** the "unknown key" case appended its key to the last table, which the program's own check caught, so nothing proved the top level refused; the origin and `lex-sys` cases were refused later by other checks, so the load-time ones were untested until the tests asserted the reason; a build that skipped its install passed because every test had installed before; and a bad dependency name was refused by a later check, not the name's. **Unverified: the branch of `check_compiler` for a compiler that does not know its revision** (`unknown`, a build outside a git checkout without `LEX_SYS_REV`). It needs a second build of the compiler to reach, and is a plain refusal; I read it, I did not run it.

**Found along the way.**

* **A hash in the file is checked against a build that may be dirty.** The compiler says `-dirty` when tracked files differ from the commit, and such a revision can never equal the 40 hex digits the file holds. That is the right answer (a compiler with local edits is not the commit), and it means the test of the *accepted* case only runs when the tree is clean, as in CI; locally it is skipped and says nothing.
* **`name = "app"` appears twice in a small project file**, and a string replace in a test put the compiler pin into the program too. `deny_unknown_fields` caught it, which is the case for having it.
* **The commit is not known before it exists.** `lexsys-hooks` has to name a compiler that contains the project file, which this change is; the file in its pull request holds the commit of a build of this branch and is moved to the merge commit once there is one, the same dance as the `lexsys-log` pin in §7.

**Not built, as §8.5 says.** And one thing noticed: `lex-sys test` has no project mode, so `lexsys-hooks` still spells out its three unit-test commands, with `build/deps/*.ls` as the way to name the libraries.
