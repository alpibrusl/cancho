# `lex-sys-vcs`: the first publish, before transport

> **Status: design, not yet built.** Written on contact with the actual
> code, which found the premise this document was going to start from —
> "the local store works, so the next slice is HTTP push/pull" — false.
> There is no `head`, no way to enumerate what is already logged, and no
> way to turn real `.ls` source into `Operation`s at all: `crates/lex-sys-vcs`
> is a library with no caller anywhere in this repository outside its own
> tests. Transport has nothing to move until this slice exists. This
> document scopes that slice, and only that slice.

## 1. What this is answering

`docs/ROADMAP.md`'s corrected row (#132) says what §8 says is unbuilt:
whole-function merge, merge sessions, typed issues, predicate branches,
the op log's own history index. Reading past that list rather than
stopping at it finds something none of those five items name, because
all five assume the thing this document is about already works.

`crates/lex-sys-vcs` has no CLI command anywhere in `crates/lex-sys/src/main.rs`
— the file's only top-level match arms are `check`, `agent-guidelines`,
`docsync`, `repo-stats`, `print`, `ids`, `authority`, `layout`, `build`,
`run`. Every one of the crate's own capabilities — `Operation::new`,
`gate::check_candidate`, `OpLog::put`/`get`, `Chain::append` — is exercised
today only by `crates/lex-sys-vcs/tests/`, hand-constructing an
`OperationKind::AddFunction { sig_id, stage_id, effects, in_file }` literal
per test case. Nobody, human or agent, can run a `.ls` file through this
crate from a terminal. That is a smaller gap than "no transport"; it is
"no first user."

## 2. What exists, read from the source rather than assumed

| Capability | Exists? | Where |
|---|---|---|
| `Operation`/`OperationKind`/`OpId`, content-addressed | Yes | `operation.rs` |
| A candidate program type-checks before its op is accepted | Yes | `gate::check_candidate(files: &[(&str, &str)])` |
| Durable storage, one JSON file per `OpId` | Yes | `OpLog::open`/`put`/`get`/`contains` |
| A hash-chained, Ed25519-sealable attestation log | Yes | `attestation::Chain<E>` |
| **Turning real source into an `Operation`** | **No** | Nothing calls `Operation::new` outside a test. `vcs.md` §4 named this as real, ~1,500-line native work (`compute_diff`+`diff_to_ops`+`body_merge`, rewritten against `lex-sys-ir` rather than `lex_ast`) and it has not been started |
| **A "what does the log currently contain" read** | **No** | `OpLog` has `get(op_id)` and `contains(op_id)` — both need the `OpId` already in hand. There is no directory listing, no iteration, no way to ask "what functions does this store know about" without already knowing every hash to ask for |
| **A head, a branch, a manifest — any notion of "current state"** | **No** | Grepped `vcs.md` for `head`/`branch`: the one hit is `lex-vcs`'s own generic table, describing what it has, not what `lex-sys-vcs` does. Nothing in this repository has ever named this gap, because nothing has tried to publish a second revision yet |
| A CLI command | No | Confirmed above |

The gate, op log and attestation log are real and correctly built for
what they do. What they do is smaller than "a working store" — they are
the *accept* and *record* halves of a pipeline with no *propose* half.

## 3. The smallest real first slice: publish with an empty log

`vcs.md` §4's hard, deferred work — `compute_diff`/`diff_to_ops`,
detecting that *this* declaration changed since *that* revision — is
only needed to publish a **second** revision. A **first** publish, against
an empty log, needs no diff at all: every declaration in the source is new,
by definition, so every one of them is an `AddFunction`. This is exactly
the asymmetry `lex-lang`'s own `lex publish` does not have (a `Store`
always already has a previous head to diff against) and `lex-sys-vcs`
gets for free from not having built branches yet.

Concretely, reusing what `lex-sys ids` already computes rather than adding
a second hashing path:

```rust
let identities = lex_sys_id::identify(&ast); // already built, main.rs:1054
for func in &identities.functions {
    let op = Operation::new(
        OperationKind::AddFunction {
            sig_id: func.sig.clone(),        // already lex-sys-id's own hash
            stage_id: func.body.clone(),     // already lex-sys-id's own hash
            effects: effects_of(&decl),      // straight off the FnDecl's own row —
                                              // `authority.md`'s own source, not new
            in_file: module_ref_of(&decl),   // many-files.md's existing ModuleRef
        },
        edition,   // Ast::edition_of, same as #124
        [],        // no parents: nothing preceded this in an empty log
    );
    gate::check_candidate(&assembled_files)?;  // already built
    op_log.put(&OperationRecord::new(op))?;    // already built
}
```

No new hashing, no new gate logic, no new storage format. The only new
code is the walk from `Identities`/`Ast` to `OperationKind::AddFunction`
values and the wiring to call it — genuinely small, and it is the piece
every other numbered gap in this document depends on existing first.

## 4. Why a head cannot be deferred even for slice 1

A first publish only needs to *write*. The moment anyone runs `publish`
a second time — including by accident, including a CI job re-running the
same command — something has to answer "is this `AddFunction` already
logged," or the store double-writes the identical, harmless-looking op
forever, silently, because `OpLog::put` has no idempotence check of its
own beyond `OperationRecord`'s own content hash (two writes of the *same*
op collide safely; two writes of what is logically the same declaration,
republished, do not — nothing today makes that comparison at all).

The minimal answer is not `lex-vcs`'s full branch-and-merge model — that
is exactly the "largely unmodified, not started" list `vcs.md` §8 already
tracks, and building it now would be building ahead of an asker the same
way `AGENTS.md` §7 already argues against. The minimal answer is a
**manifest**: `SigId -> StageId` for every declaration the last publish
from this working copy logged, written once per publish, read at the
start of the next one. It is not a head in `lex-vcs`'s sense (no branches,
no merge, no advancing a shared pointer two agents could race on) — it is
the one fact a single working copy's own next publish needs to not repeat
itself, and it is the seam a real head would attach to later without
this slice's own format needing to change: a manifest with a documented,
migratable shape at the same discipline `editions.md` already applies to
`Operation` itself.

## 5. Deliberately not in this slice

- **Incremental diffing** — detecting that an *existing* declaration's
  body changed (`ModifyBody`) or that one was removed (`RemoveFunction`)
  needs comparing two revisions structurally, which is `vcs.md` §4's own
  ~600-line `compute_diff`+`diff_to_ops` rewrite against `lex-sys-ir`'s
  `Expr`/`FnDecl` (not `lex_ast::CExpr`). Real work, correctly deferred:
  it has no asker until slice 1 makes a *second* publish possible at all,
  and testing it needs two real revisions of a real program to diff, which
  do not exist as fixtures yet either.
- **HTTP transport (push/pull)** — nothing to move until slice 1 writes a
  local op log worth moving. When it is built, the shape to mirror is
  `lex-hub`'s own — a thin gateway, JWT or none depending on whether a
  second machine or a second person is the actual asker — but that is a
  question this document declines to answer ahead of slice 1 existing,
  the same restraint `vcs.md` §5 already applied to a shared `lex-vcs-core`
  crate: worth deciding when the duplication (or, here, the lack of a
  second party) actually costs something, not on the strength of "it
  would be nice to have."
- **Whole-function merge, merge sessions, typed issues, predicate
  branches, the op log's own history index** — `vcs.md` §8's own list,
  unchanged by this document, and further downstream than any of the
  above.

## 6. What "built" would look like

- `lex-sys vcs publish [--store <dir>] [--std] <inputs...>` — new top-level match
  arm in `crates/lex-sys/src/main.rs`, next to `build`/`run`/`ids`; parses
  and lowers exactly as `ids`/`check` already do, then runs §3's walk.
  Refuses (not panics) on: a gate rejection (reports the rejected
  declaration's rule tag, not a bare `422`-shaped opaque failure); a
  `--store` directory that exists but is not a `lex-sys-vcs` store.
- `lex-sys vcs log [--store <dir>]` — reads the manifest and lists what is
  known, by name and `SigId`, the read-side counterpart with no analogue
  in the crate today (§2's second row).
- A fixture pair: a real multi-function `.ls` file published against an
  empty store, checked against the store's own on-disk `OperationRecord`
  files afterward — the same "checked against real `lex-sys ids` output,
  not invented strings" discipline #124 already set for this crate's
  golden tests.

## 7. Sequencing

Slice 1 (this document) → incremental diffing, once slice 1 has a second
real revision to test it against → transport, once there is a second
machine or person actually asking to sync with one. Each gated on the
next one existing to ask for it, the same order `vcs.md` §8 already
committed to and this document is not changing, only extending one slice
earlier than "gate, op log, attestation" turned out to reach.
