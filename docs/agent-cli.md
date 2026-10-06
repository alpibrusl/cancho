# The CLI as data, not a doc that can drift

> **Status: built.** `cancho introspect` and `cancho skill` register
> every subcommand once, in `src/acli.rs`, via the
> [acli](https://github.com/alpibrusl/acli) SDK `lex-lang`'s
> `crates/lex-cli/src/acli.rs` already uses — so an agent learns the
> command surface from the binary itself rather than from a hand-written
> doc, which is exactly the failure mode `docs/benchmarks-game.md`'s
> sqrt paragraph and Open row were just found to have (stale for four
> months, because nothing forced them to move when the code did).
>
> **One real bug found building this, the same shape as the reason for
> building it.** The SDK's generated text always renders an `option` with
> a `--` prefix. Three of this binary's real flags — `-o`, `-l`, `-L` —
> are single-dash, so registering them as options would have told an
> agent to write `--o`/`--l`/`--L` and get refused: a tool meant to
> prevent an agent from learning the wrong thing would have taught it
> one. Fixed by registering those three as *arguments* instead (which
> render bare, no assumed prefix), named literally `-o <path>` — checked
> below in §2.
>
> **A second thing found, not fixed, because it isn't this repository's
> to fix.** `cancho skill`'s generated text also carries a fixed
> "Exit codes" / "Output format" section — acli 0.5's own template,
> the same one `lex-lang` ships — that does not describe this binary:
> cancho's real codes are 0/1/2/3 (and 4, for `cancho test` only:
> a built program with a failing `test_*`; *corrected, the original
> sentence left it out*), documented at the top of `main.rs`
> and in `docs/agent-errors.md`, not ACLI's generic 0/2/3/5/8/9; and not
> every command takes `--output json` (only `check` and `authority` do).
> §3 is the mitigation taken — call it out in `AGENTS.md` and this
> document rather than silently ship boilerplate that looks
> authoritative and isn't.

---

## 1. What ships

```sh
cancho introspect [--output json]        # full command tree, ACLI spec §1.2
cancho skill [--output json] [<out-file>]  # markdown + YAML, agentskills.io
```

Both are generated from one `Vec<CommandInfo>` in `src/acli.rs` — the
same nine top-level commands `--help` lists (`vcs`'s five subcommands
count as one entry, an enum-typed argument, matching how `lex-lang`
registers `pkg`/`issue`). Nothing here is clap-derived: this binary's
arg parsing is hand-rolled (`docs/many-files.md`-era, predating any
SDK), so `CommandInfo` is built by hand rather than through the SDK's
`acli_args!`/`register::<C>()` macro path, exactly as `lex-lang`'s own
comment on that decision explains.

## 2. The false-familiarity bug this found

`crates/cancho/src/acli.rs`'s `CommandInfo::add_option` always renders
as `--{name}` in the generated skill text (`acli`'s own `skill.rs`
template has no other form). `build` and `run` take `-o`, `-l`, and
`-L` — single dash, checked directly in `parse_args`
(`crates/cancho/src/main.rs`) — so the first draft of this registered
them as options and generated:

```
- `--o` (string) — output path (default: the first input's stem)
```

An agent reading only the generated surface, with no other prior on
this binary, would write `cancho build --o out app.cho` and be refused
with `unknown option --o`— the exact shape of harm §0 of this whole
line of work (`docs/first-page.md`, the `false-familiarity` discussion
that motivated it) was about: a surface that *looks* authoritative
teaching the wrong thing with full confidence.

Fixed by registering `-o <path>`, `-l <name>`, `-L <path>` as
*arguments* instead of options: `CommandInfo::add_argument` renders the
name bare, with no assumed prefix, so the literal dash in the name is
what an agent sees. Checked directly — `cargo run -p cancho -- skill`
is grepped for a bare `` `-o <path>` `` line in
`crates/cancho/tests/conformance/agent_cli.rs`'s
`introspect_names_every_registered_command`, though the sharper check
is just reading the output once, by hand, which is how this was found
in the first place.

## 3. What was not fixed, and why

`generate_skill_with` (acli 0.5's `skill.rs`) hard-codes an "Exit
codes" table (0/2/3/5/8/9, ACLI spec's own generic set) and an "Output
format" paragraph claiming every command takes `--output
json|text|table`. Neither is a function of anything registered in
`src/acli.rs` — there is no `SkillOptions` hook for it in this SDK
version — so it cannot be made accurate from this repository without
forking the dependency, which is out of scope for a project that only
works on cancho.

The mitigation is documentation, not code: `AGENTS.md` §0 and this
document both say, in the same breath as recommending `skill`, that its
exit-code/output-format sections are the SDK's template and the real
contract lives at the top of `main.rs` and in `docs/agent-errors.md`.
An agent that reads `agent-guidelines` first (as `AGENTS.md` itself
tells it to) sees the real contract before it would see the generic
one.

## 4. What `.cli/` is, and why it isn't committed

`introspect` and `version` (not `skill`) also refresh a `.cli/`
folder — `commands.json`, a `README.md`, and one example script per
command with examples — as a side effect, per ACLI spec §1.3.
`lex-lang` commits this folder; cancho does not (`.gitignore`): unlike
`lex-lang`'s ~50-command surface, cancho's nine commands are cheap
enough to regenerate on demand, and a checked-in copy is one more place
the same staleness this document opened with could hide. `introspect`/
`skill` themselves are the up-to-date surface.

---

## 5. Open

| Question | Why it waits |
|---|---|
| Route `--version` through `acli::build_app().handle_version()` | `lex-lang` does; cancho's `--version` predates this slice and already has its own tested text (`cancho {version} (host {triple})`). Rewiring it for consistency alone would change committed-test-visible output for no behavior gain — left as is |
| A golden snapshot of `cancho skill` (`lex-lang`'s `cli_skill_is_in_sync`) | `every_dispatched_command_is_documented` already catches a command going undocumented; a byte-for-byte snapshot catches wording drift too, at the cost of a fixture to update on every `CommandInfo` edit. Not yet worth it at nine commands |

---

## 6. The suite

| Test | Shows |
|---|---|
| `agent_cli::every_dispatched_command_is_documented` | every command `main.rs` dispatches is in both the `usage:` block and `cancho skill`'s command list — sourced from the dispatch table itself, not a curated second list |
| `agent_cli::introspect_names_every_registered_command` | `cancho introspect --output json` parses as JSON and names every top-level command |
