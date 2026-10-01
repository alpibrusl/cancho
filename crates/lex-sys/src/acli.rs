//! ACLI integration: register every `lex-sys` subcommand with the
//! [acli](https://github.com/alpibrusl/acli) Rust SDK, the same way
//! `lex-lang`'s `crates/lex-cli/src/acli.rs` does, so the binary is
//! self-describing to any agent through `lex-sys introspect` / `lex-sys
//! skill` instead of a hand-maintained doc that can drift out of sync
//! with the dispatch table -- the same failure mode
//! `docs/benchmarks-game.md` and `docs/against-c-and-rust.md` were just
//! found to have, but for the CLI surface instead of a measurement.
//!
//! What ships:
//!
//! - `lex-sys introspect [--output json]` -- full command tree as JSON
//!   (ACLI spec §1.2); an agent reads this once and learns the surface.
//! - `lex-sys skill [--output json] [<out-file>]` -- markdown + YAML
//!   frontmatter per agentskills.io; `lex-sys skill > SKILL.md` is the
//!   generated replacement for a hand-written one.
//!
//! **Caveat, worth reading before trusting the raw output.** The SDK's
//! generated skill text carries a generic "Exit codes" / "Output
//! format" section -- acli 0.5's own fixed template, not derived from
//! anything registered below -- that does **not** describe this
//! binary: lex-sys's real exit codes (0/1/2/3, documented at the top
//! of `main.rs` and in `docs/agent-errors.md`) don't match ACLI's
//! generic 0/2/3/5/8/9 table, and not every command takes `--output
//! json` (only `check` and `authority` do). `agent-guidelines` stays
//! the authoritative contract for both; `introspect`/`skill` are for
//! command *discovery* -- the list of verbs and their arguments -- not
//! the exit-code contract. Shipping a generated doc whose boilerplate
//! looks authoritative but isn't is exactly the false-familiarity risk
//! `docs/agent-errors.md` exists to name, just arriving from a shared
//! SDK template rather than from a claim this repository wrote itself.
//!
//! `CommandInfo` is registered manually, same reason as lex-lang: this
//! binary's arg parsing is hand-rolled, not clap-derived, so the
//! `acli_args!` / `register::<C>()` macro path (which assumes a
//! clap-derived struct) doesn't apply.

use acli::AcliApp;
use acli::introspect::CommandInfo;
use serde_json::json;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn build_app() -> AcliApp {
    let mut app = AcliApp::new("lex-sys", VERSION);
    for cmd in commands() {
        app.register_command(cmd);
    }
    app
}

fn commands() -> Vec<CommandInfo> {
    vec![
        cmd_build(),
        cmd_check(),
        cmd_run(),
        cmd_test(),
        cmd_fmt(),
        cmd_ids(),
        cmd_authority(),
        cmd_layout(),
        cmd_print(),
        cmd_agent_guidelines(),
        cmd_vcs(),
    ]
}

fn cmd_build() -> CommandInfo {
    CommandInfo::new(
        "build",
        "compile one or more files to a linked executable or object file \
         (exit 1 = refused, 2 = bad command line, 3 = environment failure)",
    )
    .idempotent(true)
    .add_argument(
        "file",
        "string[]",
        "one or more .ls files -- a program is the set of files named on the command line",
        true,
    )
    // Single-dash flags (`-o`, `-l`, `-L`), registered as arguments rather
    // than options: `add_option` always renders with a `--` prefix
    // (skill.rs's fixed template), which would tell an agent to write
    // `--o`/`--l`/`--L` and get refused. Arguments render bare, so the
    // name carries the real, single-dash spelling instead of a wrong one.
    .add_argument("-o <path>", "string", "output path (default: the first input's stem)", false)
    .add_argument(
        "-l <name>",
        "string[]",
        "link `lib<name>` at the final link step, repeatable, passed to `cc` unexamined",
        false,
    )
    .add_argument(
        "-L <path>",
        "string[]",
        "linker search path for `-l`, repeatable, same treatment",
        false,
    )
    .add_option(
        "emit",
        "enum[exe|obj]",
        "emit a linked executable or a bare object file",
        Some(json!("exe")),
    )
    .add_option("std", "bool", "make the standard library's source available", None)
    .add_option(
        "backend",
        "enum[cranelift|llvm]",
        "which backend generates code",
        Some(json!("llvm")),
    )
    .with_examples(vec![
        ("Build an executable", "lex-sys build hello.ls"),
        ("Build with the standard library available", "lex-sys build --std app.ls"),
        (
            "Build against a library beyond libc",
            "lex-sys build -o client client.ls -l ssl -l crypto",
        ),
    ])
    .with_see_also(vec!["check", "run"])
}

fn cmd_check() -> CommandInfo {
    CommandInfo::new(
        "check",
        "type-check without producing an executable; --output json answers every \
         refusal as data (a stable rule tag, position, and sentence) instead of prose \
         (docs/agent-errors.md)",
    )
    .idempotent(true)
    .add_argument("file", "string[]", "one or more .ls files", true)
    .add_option("std", "bool", "make the standard library's source available", None)
    .add_option("output", "enum[json]", "report every independent refusal as structured data", None)
    .add_option(
        "backend",
        "enum[cranelift|llvm]",
        "which backend generates the code check discards",
        Some(json!("llvm")),
    )
    .with_examples(vec![
        ("Type-check a file", "lex-sys check hello.ls"),
        ("Every refusal as structured data", "lex-sys check --output json app.ls"),
    ])
    .with_see_also(vec!["build", "agent-guidelines"])
}

fn cmd_run() -> CommandInfo {
    CommandInfo::new(
        "run",
        "build to a temporary executable and run it; the process's own exit status \
         is the compiled program's, not a fixed lex-sys code",
    )
    .idempotent(false)
    .add_argument("file", "string[]", "one or more .ls files", true)
    // Single-dash flags, as arguments rather than options -- see the
    // same note on `cmd_build`.
    .add_argument("-l <name>", "string[]", "link `lib<name>` (repeatable)", false)
    .add_argument("-L <path>", "string[]", "linker search path for `-l` (repeatable)", false)
    .add_option("std", "bool", "make the standard library's source available", None)
    .add_option(
        "backend",
        "enum[cranelift|llvm]",
        "which backend generates code",
        Some(json!("llvm")),
    )
    .with_examples(vec![("Run a program", "lex-sys run hello.ls")])
    .with_see_also(vec!["build", "check"])
}

fn cmd_test() -> CommandInfo {
    CommandInfo::new(
        "test",
        "build once and run every `fn test_*` in the files named, one process each; \
         a test answers 0 to pass, and a trap fails it \
         (exit 0 = all passed, 4 = a test failed, 1 = refused, 2 = bad command line or no \
         tests found, 3 = environment failure; docs/testing.md)",
    )
    .idempotent(false)
    .add_argument("file", "string[]", "one or more .ls files, none declaring `main`", true)
    // Single-dash flags, as arguments rather than options -- see the
    // same note on `cmd_build`.
    .add_argument("-l <name>", "string[]", "link `lib<name>` (repeatable)", false)
    .add_argument("-L <path>", "string[]", "linker search path for `-l` (repeatable)", false)
    .add_option("std", "bool", "make the standard library's source available", None)
    .add_option(
        "backend",
        "enum[cranelift|llvm]",
        "which backend generates code",
        Some(json!("llvm")),
    )
    .with_examples(vec![("Run a file's tests", "lex-sys test --std tests.ls")])
    .with_see_also(vec!["run", "check"])
}

fn cmd_fmt() -> CommandInfo {
    CommandInfo::new(
        "fmt",
        "rewrite files in canonical layout, keeping comments, blank lines and literal \
         spellings; --check writes nothing and exits 1 if any file would change \
         (exit 1 also = a file it cannot format safely; docs/formatting.md)",
    )
    .idempotent(true)
    .add_argument(
        "file",
        "string[]",
        "one or more .ls files, or directories searched for them",
        true,
    )
    .add_option("check", "bool", "report what would change and write nothing", None)
    .with_examples(vec![
        ("Format a tree in place", "lex-sys fmt src/"),
        ("Fail if anything is not canonical (CI)", "lex-sys fmt --check src/ tests/"),
    ])
    .with_see_also(vec!["check", "print"])
}

fn cmd_ids() -> CommandInfo {
    CommandInfo::new(
        "ids",
        "print each declaration's content hash: a signature and a body for every \
         function, one identity for every type (docs/canonical-ast.md)",
    )
    .idempotent(true)
    .add_argument("file", "string[]", "one or more .ls files", true)
    .add_option("std", "bool", "make the standard library's source available", None)
    .with_examples(vec![("Print every declaration's hash", "lex-sys ids app.ls")])
    .with_see_also(vec!["print", "check"])
}

fn cmd_authority() -> CommandInfo {
    CommandInfo::new(
        "authority",
        "derive the least grant a program provably needs from its own effect rows \
         (docs/authority.md)",
    )
    .idempotent(true)
    .add_argument("file", "string[]", "one or more .ls files", true)
    .add_option("std", "bool", "make the standard library's source available", None)
    .add_option("output", "enum[json]", "the report as data rather than prose", None)
    .with_examples(vec![(
        "What authority does this program need?",
        "lex-sys authority --output json app.ls",
    )])
    .with_see_also(vec!["check", "layout"])
}

fn cmd_layout() -> CommandInfo {
    CommandInfo::new(
        "layout",
        "report struct field layout and what packing or reordering would cost or \
         save (docs/layout.md)",
    )
    .idempotent(true)
    .add_argument("file", "string[]", "one or more .ls files", true)
    .add_option("std", "bool", "make the standard library's source available", None)
    .with_examples(vec![("Report struct layout", "lex-sys layout app.ls")])
    .with_see_also(vec!["ids"])
}

fn cmd_print() -> CommandInfo {
    CommandInfo::new(
        "print",
        "render one parsed file in canonical form -- the AST-to-text direction of \
         the pipeline, not a formatter (comments never reach the AST)",
    )
    .idempotent(true)
    .add_argument("file", "string", "exactly one .ls file", true)
    .with_examples(vec![("Render a file canonically", "lex-sys print app.ls")])
    .with_see_also(vec!["ids"])
}

fn cmd_agent_guidelines() -> CommandInfo {
    CommandInfo::new(
        "agent-guidelines",
        "emit AGENTS.md: how to write lex-sys in one page rather than in 42 \
         documents. Every checked code block in it is run by the test suite, so a \
         guideline that stops being true is a red build",
    )
    .idempotent(true)
    .with_examples(vec![
        ("Read the rules", "lex-sys agent-guidelines"),
        ("Capture into a repo", "lex-sys agent-guidelines > AGENTS.md"),
    ])
    .with_see_also(vec!["skill", "introspect"])
}

fn cmd_vcs() -> CommandInfo {
    CommandInfo::new(
        "vcs",
        "the content-addressed package store: publish, log, resolve, lock, and \
         fetch declarations (docs/package-system.md)",
    )
    .idempotent(false)
    .add_argument("subcommand", "enum[publish|log|resolve|lock|fetch]", "what to do", true)
    .with_examples(vec![
        ("Publish every declaration in a file", "lex-sys vcs publish --store .lex-sys-vcs app.ls"),
        ("List what a store has published", "lex-sys vcs log --store .lex-sys-vcs"),
        ("Re-verify every pin a store has ever published", "lex-sys vcs resolve .lex-sys-vcs"),
        (
            "Pin names by hash into a lock file",
            "lex-sys vcs lock --store .lex-sys-vcs -o app.lock connect octets_of",
        ),
        (
            "Fetch verified sources for a lock",
            "lex-sys vcs fetch --lock app.lock --store .lex-sys-vcs -o vendor/",
        ),
    ])
    .with_see_also(vec!["check"])
}
