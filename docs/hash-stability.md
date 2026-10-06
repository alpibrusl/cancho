# How often does a hash actually move? Two rates, and they disagree

> **Status: measured, and the instrument turns out never to have been
> tested.**
>
> `canonical-ast.md` §8 keeps three things off the contract list and says
> why the list might never empty:
>
> > *"The point is the **rate**: the question of whether this section can
> > ever be emptied is a question about how often these actually move,
> > and nothing was measuring that."*
>
> `ROADMAP.md`'s `lex-vcs` row is gated on the same number — *"the design
> doc is worth writing when that number exists"*. This is the number, and
> there are two of them.
>
> **The encoder has moved 20 times and the golden fixtures have observed
> none of them**, because they landed after the movement stopped.
> **The language has moved enough that 71% of this repository's own `.ls`
> history no longer type-checks** — and the single largest cause is one
> effect label being split in two.
>
> A content-addressed VCS keyed on these hashes inherits the second rate,
> not the first.
>
> **Current reading, as of #163: 39%, not 71%**, up from 37% at #142
> (§2's "Re-measured (#163)" below, appended rather than correcting the
> "growth has stopped" sentence two rounds up — that sentence was true
> when written and is not true now, and the record keeps both). The
> `io`-split debt itself has not grown — `--migrate`/`--alias` still
> recover exactly 23/0, unchanged across all three measurements — but
> the corpus gained 28 revisions since #142 and 15 joined the unreadable
> pile, not 3. One cause is confirmed (a harness gap: `scripts/history.py`
> never learned to replay a file that imports a `vcs`-fetched package);
> the rest is measured, not yet explained.

---

## 1. The encoder rate: zero, out of zero observations

`crates/lex-sys-id/tests/golden.rs` pins 35 fixtures, one per node
family. §8 introduced them for exactly this purpose: *"It is not a freeze
and a failure is not a bug report — it asks which of two things
happened."*

| | |
|---|---:|
| commits touching `crates/lex-sys-id/src/` | **20** |
| …of those, since the goldens landed | **0** |
| commits since the goldens landed | 14 |
| golden hashes that moved in them | **0** |

So the fixtures have been green for fourteen commits, and that is worth
less than it looks: **none of those commits changed the encoder.** The
instrument was installed after the thing it measures stopped happening.

One commit since did touch `crates/lex-sys-syntax/src/ast.rs` — file
handles (#65), which added three prelude type names. It added no *node
kind*, so no tag moved, and the goldens were right to stay still. That
is one observation of the right kind and it passed.

**The honest reading: the encoder rate is unmeasured, not low.** It will
become measurable the first time a milestone adds a node, and §8's
sentence about "every milestone from M2 on adds nodes" is the reason to
expect that to happen rather than not.

> **A second observation, same kind (#70).**
> [`character-literals.md`](character-literals.md) adds a *spelling* —
> `'a'` is the integer 97 — and rewrote 119 sites across `std/`,
> `examples/` and `tests/accept/` onto it. No golden hash moved, and
> `a_character_literal_hashes_as_its_integer` checks the two spellings
> against each other. Two observations, two passes, and the encoder rate
> is still unmeasured: a change designed to move no tag cannot show that
> a moved tag would be caught.

> **A third observation, and the first that moved a hash (static
> signatures).** A `static`'s `sig` hashed only its referent type, so two
> `static`s of one type shared an identity and `vcs publish`, which keys its
> manifest by `sig`, refused the second. The fix puts the **name** in the
> `sig` (behind a new tag, `STATIC_DECL`, appended as `0x75`), as a
> function's already was. Every `static`'s `sig` moved; no `body`, no
> function, type or extern hash did, because a body refers to a `static`
> by name and not by `sig`. What pinned them: one golden row
> (`static-item`), and the committed `packages/x509` store, the only
> `static` in `packages/` (`oid_table`), plus the `requires` locks of the
> five dependent modules (`x509_names`, `x509_verify`, `tls_message`,
> `tls_slot`, `tls_client12`); `scripts/publish_packages.py` regenerated
> them and `--check` passes. Nothing else quoted a static's hash.
> **Why not a narrower fix** (the publish layer keying a static by
> `(name, sig)` and leaving identities alone): the manifest is keyed by
> `sig` on purpose, a lock records `sig_id` as *the* identity a consumer
> pins, and a `sig` that two declarations share would still be wrong for
> every other reader of it (`resolve`, `fetch`, a future diff). The price of
> the wide fix is one regenerated package store, paid once, in a
> language that moves faster than this (§2); the price of the narrow one is
> a non-identifying identity. Also closed: an unused `static` (one nothing
> reachable reads, so `lex-sys-ir` drops it) failed to publish as
> "internal: ... no lowered function, extern or static"; the publish layer
> now asks the source.

---

## 2. The language rate: 71% of its own past is unreadable

The other rate needs no new instrument, because git has it. Every
distinct revision of every `.ls` file under `std/` and `examples/` across
all **67** commits, compiled by **today's** binary — one compiler over
every revision, so anything that fails is the language having moved and
never the encoder:

| | |
|---|---:|
| distinct file revisions | **117** |
| today's compiler still reads | **34** (29%) |
| no longer parses or checks | **83** (71%) |

And the causes are not spread out. Classified by first error:

| revisions | why |
|---:|---|
| **35** | `performs io_write, which its row [io] does not declare` |
| 8 | no effect row at all — written before rows existed |
| 8 | a type or function declared twice (files later split apart) |
| 5 | needs a sibling file, or a name that moved |
| 4 | an unknown type |
| 3 | a builtin's arity changed |

> **Re-measured (#85).** [`editions.md`](editions.md) §2 replays the
> same history with a harness that is in the repository
> (`scripts/history.py`) and gives each file the program it belonged to:
> **58 of 141** revisions (41%) no longer check, and the `io` split is
> still the largest class, at 45. §2 there also shows that an alias
> cannot absorb it, because rows are exact.

> **Re-measured (#142): 37%, and the growth almost stopped adding to
> it.** `scripts/history.py` again, against 24 more commits' worth of
> history: **165** revisions, **104** (63%) read, **61** (37%) do not.
> The `io` split is still the whole story — `--alias` still recovers
> **zero** (a row is exact, so aliasing just moves which half is
> missing, exactly as `editions.md` §3 found), and `--migrate`'s two
> mechanical steps still recover exactly the same **23**. What changed
> is the *rate*: the corpus grew by 24 revisions since #85 and only
> **3** joined the unreadable pile. Not because the vocabulary stopped
> growing — §3 below found real growth in the same window — but because
> everything added since (`null_ptr`, `spawn`/`join`, `Thread`) is
> edition-gated and additive by construction, the property
> `editions.md` exists to guarantee. The 61 unreadable revisions are a
> closed debt from one pre-editions rename, not a target still moving.

> **Re-measured (#163): 39%, and the growth resumed.** `scripts/history.py`
> again, against 8 more commits touching `std`/`examples` (20 total since
> #143's measurement commit): **193** revisions, **117** (61%) read,
> **76** (39%) do not. `--migrate` still recovers exactly **23** and
> `--alias` still **0** — the `io`-split debt itself has not grown by one
> revision — but the corpus gained 28 revisions this time and **15**
> joined the unreadable pile, not 3. The "growth has stopped adding to
> it" sentence two entries up is **false as of this measurement** and is
> kept above anyway, uncorrected in place, so the record shows what was
> believed and when.
>
> One new cause is confirmed, not guessed: `examples/tls_client/socket.ls`
> (`docs/next-phase.md` §4.1's own migration onto `net.connect`) is
> unreadable by this harness for a reason that has nothing to do with the
> language moving. `scripts/history.py`'s own `lay_out` passes `--std` or
> a file's same-folder siblings and nothing else — it has never known how
> to hand a replayed file the `packages/` source a `vcs`-fetched import
> needs. Every other package-importing example (`fetch.ls`, `report.ls`,
> `serve.ls`, `collect.ls`, `vsock.ls`, `agent_guest.ls`,
> `agent_supervisor.ls`, `results_stub.ls`) was already unreadable by this
> same gap before this measurement, already inside the 61 — confirmed
> directly, `lex-sys check <file> --std` on each reproduces the identical
> `no module 'net.sockets'` refusal `socket.ls` now also gets. `socket.ls`
> joining that list is the harness missing an argument, not a program
> whose identity moved.
>
> The other roughly 14 of the 15 newly unreadable revisions are **not**
> individually diagnosed here. Attributing each one would mean replaying
> every new revision by hand and reading its refusal, which this entry
> has not done — and `next-phase.md` §4.1's own lesson applies just as
> much to a number as to a duplicate: a check that measures correctly can
> still be wrong to explain without doing the reading. What is measured
> and certain: the full current corpus builds and passes `cargo test
> --workspace` at 100% (that is a different, stronger guarantee than this
> historical replay — it says today's code is right, not that yesterday's
> stays readable), the `io`-split debt is unchanged, and the harness gap
> above accounts for exactly one of the fifteen.

**One label rename accounts for 42% of the unreadable past.** When
`standard-input.md` §2 split `io` into `io_read` and `io_write` —
correctly, and for reasons that document argues well — it invalidated
thirty-five file revisions' worth of identity in a single commit.

Nothing about that is a bug. It is what a language doing its growing in
public looks like, and every one of those changes was the right call at
the time. The point is only that it is the rate that a hash-keyed tool
would actually experience.

---

## 3. What this means for `lex-vcs`

`ROADMAP.md`'s row says lex-lang's `crates/lex-vcs` is *"81%
language-agnostic already"* and that the gate is *"this side: §8"*. The
two rates say which side of §8 matters.

* **The tag values** (§8's first bullet) are stable in practice and
  unmeasured in principle. Freezing them is a table nobody has had to
  write yet.
* **What moves is the vocabulary a body is written in.** A body's hash is
  a function of its AST, and its AST mentions effect labels, builtin
  names and arities. Those changed 46 times in 67 commits.

So an op DAG built on these hashes would not mostly record edits a
programmer made. It would record the language changing underneath
programs nobody touched — thirty-five of them, at once, for one rename.

That is not an argument against the design. It is an argument about
**when**: the row's instinct to wait was right, and the thing to wait for
is not a number but a *plateau* — a stretch of commits in which the
effect vocabulary and the builtin surface do not move. At the time this
was written, this repository had not had one yet: the last four slices
before it added `err_write`, `file_read`, three prelude types and three
builtins.

> **Corrected (`ROADMAP.md`'s `lex-vcs` row, current as of #119).** That
> was true when written and is not any more. `builtin.rs` and `defs.rs`
> — every builtin, every effect label, every prelude type's own row —
> last changed at #92, the third `Net` slice. The 27 commits since
> (#93 through #119, an entire second backend designed and built start
> to finish) touch none of `crates/lex-sys-ir/`, `crates/lex-sys-types/`
> or `crates/lex-sys-syntax/` at all — `git diff 52daddb..2999a7d --
> crates/lex-sys-ir/ crates/lex-sys-types/ crates/lex-sys-syntax/` is
> empty. The plateau this section said had not happened has now
> happened, measured the same way §1 and §2 above were: by reading the
> commit range rather than assuming it. Whether 27 commits is *long*
> enough — long enough that a `lex-vcs` design written against it would
> not immediately need revising — is a judgement call this section
> leaves open rather than answers for it.

> **Corrected again (current as of #142): the plateau did not hold, and
> that turns out to be the wrong thing to have measured.** `null_ptr`
> (#137, [`opaque-pointers.md`](opaque-pointers.md) §3) and `spawn`/
> `join`/`Thread` ([`threads.md`](threads.md)) all landed after #120,
> touching `builtin.rs`/`defs.rs`/`crates/lex-sys-types` exactly the way
> this section's own reasoning predicted growth would. But every one of
> them is edition-gated and additive by its own stated design
> (`opaque-pointers.md` §4, `threads.md` §4): an edition-1 or -2 file
> cannot be broken by a builtin or type that only edition 3 or 4 grants.
> That is the actual property a hash-keyed tool needs, and it is not "no
> growth" — it is "no growth that reaches backward." §2's fresh
> measurement above is the evidence, not an assumption: the corpus grew
> by 24 revisions in this same window and only 3 joined the unreadable
> pile. The vocabulary moved. The past it could break did not.

---

## 4. What this does not say

* **Not that the hashes are unstable.** Within one build they are exact,
  and 61 relational tests plus 35 golden fixtures say so. The instability
  measured here is *across language versions*, which §8's third bullet
  already refuses to make a claim about — this puts a number on the
  refusal.
* **Not that 71% is a defect rate.** Every one of those revisions
  compiled when it was written. The figure measures how far the language
  has travelled, and a young language travelling is the intended
  behaviour.
* **Not a measurement of the encoder.** §1 is explicit that the encoder
  rate has zero observations. Anyone reading §1 as "the encoder is
  stable" has read it backwards.
