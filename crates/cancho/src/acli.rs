//! ACLI integration: register every `cancho` subcommand with the
//! [acli](https://github.com/alpibrusl/acli) Rust SDK, the same way
//! `lex-lang`'s `crates/lex-cli/src/acli.rs` does, so the binary is
//! self-describing to any agent through `cancho introspect` / `cancho
//! skill` instead of a hand-maintained doc that can drift out of sync
//! with the dispatch table -- the same failure mode
//! `docs/benchmarks-game.md` and `docs/against-c-and-rust.md` were just
//! found to have, but for the CLI surface instead of a measurement.
//!
//! What ships:
//!
//! - `cancho introspect [--output json]` -- full command tree as JSON
//!   (ACLI spec §1.2); an agent reads this once and learns the surface.
//! - `cancho skill [--output json] [<out-file>]` -- markdown + YAML
//!   frontmatter per agentskills.io; `cancho skill > SKILL.md` is the
//!   generated replacement for a hand-written one.
//!
//! **Caveat, worth reading before trusting the raw output.** The SDK's
//! generated skill text carries a generic "Exit codes" / "Output
//! format" section -- acli 0.5's own fixed template, not derived from
//! anything registered below -- that does **not** describe this
//! binary: cancho's real exit codes (0/1/2/3, documented at the top
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
    let mut app = AcliApp::new("cancho", VERSION);
    for cmd in commands() {
        app.register_command(cmd);
    }
    // The commands that only the command line written in cancho runs (`examples/selfhost`, `docs/agent-ci.md`) are advertised when this is
    // asked for: this binary does not dispatch them, and `tests/conformance/agent_cli.rs` holds it to advertising only what it runs. The test
    // that writes `examples/selfhost/commandtree.cho` and the one that compares that command line with this binary ask.
    if std::env::var_os("CANCHO_ACLI_EXTRAS").is_some() {
        for cmd in extra_commands() {
            app.register_command(cmd);
        }
    }
    app
}

/// The commands of the command line in cancho that this binary does not have: `ci`, `profile` and `trace`.
fn extra_commands() -> Vec<CommandInfo> {
    vec![cmd_ci(), cmd_profile(), cmd_trace()]
}

fn cmd_ci() -> CommandInfo {
    CommandInfo::new(
        "ci",
        "the gate as one command: fmt --check, check, test and the authority baseline, answered as one JSON verdict \
         (exit 0 = every step passed, 1 = a finding, 2 = bad command line, 3 = a step could not be run, 4 = only `test` failed; \
         only in the command line written in cancho; docs/agent-ci.md)",
    )
    .idempotent(false)
    .add_argument("file", "string[]", "the .cho files of the program; checked, formatted and compared with cancho.authority.json", false)
    .add_argument("--test <file>", "string[]", "a file of the test run, named in full as `cancho test` takes them (repeatable)", false)
    .add_option("std-dir", "string", "the directory of the standard library's files, passed to each step", None)
    .add_option("output", "enum[json]", "the verdict as data rather than one line a step", None)
    .add_option("strict", "bool", "a narrowing of the authority is a failure too", None)
    .add_option("accept-authority", "bool", "write cancho.authority.json from this run, recording what widened (needs --reason)", None)
    .add_option("reason", "string", "why the authority widened, written beside what it admits", None)
    .add_option("init", "bool", "write .github/workflows/cancho.yml for the pinned compiler, running this command", None)
    .with_examples(vec![
        ("Run the gate", "cancho ci --output json src/main.cho src/lib.cho --test src/lib.cho --test tests/lib_test.cho"),
        ("Admit a widening of the authority", "cancho ci --accept-authority --reason \"fetch the index\" src/main.cho"),
        ("Write the GitHub workflow", "cancho ci --init src/main.cho"),
    ])
    .with_see_also(vec!["check", "test", "authority", "fmt"])
}

fn cmd_profile() -> CommandInfo {
    CommandInfo::new(
        "profile",
        "where the instructions of a program go, counted by an instrumented build of it (no other tool) and answered as JSON; with --baseline, which function \
         got dearer (--machine counts machine instructions under valgrind instead) \
         (exit 0 = a profile, 1 = the program did not end with 0 or a function regressed, 2 = bad command line, 3 = it could not be run; \
         only in the command line written in cancho; docs/agent-profile.md)",
    )
    .idempotent(false)
    .add_argument("file", "string[]", "the .cho files of the program; program arguments follow `--`", true)
    .add_option("std", "bool", "make the standard library's source available", None)
    .add_option("std-dir", "string", "the directory of the standard library's files", None)
    .add_option("advise", "bool", "report the facts for deciding what to optimise (shares, the speedup bound, calls, statements and allocations a call, the hottest lines with their text); no effort or advice is given", None)
    .add_option("machine", "bool", "count machine instructions under valgrind (needs valgrind; line attribution is lost where the optimiser removes it)", None)
    .add_option("unoptimized", "bool", "build with -O0 (with --machine, more of the run has a line)", None)
    .add_option("top", "string", "how many functions to list (default 20)", None)
    .add_option("record", "string", "write a baseline from this run", None)
    .add_option("baseline", "string", "compare with a baseline and fail on a regression", None)
    .add_option("factor", "string", "how much dearer is a regression, in hundredths (default 1.3)", None)
    .add_option("strict", "bool", "an improvement is a failure too (re-record the baseline)", None)
    .with_examples(vec![
        ("Profile a program", "cancho profile app.cho"),
        ("Gate on a baseline", "cancho profile --baseline perf.json app.cho"),
        ("Where to look for a speedup", "cancho profile --advise app.cho"),
    ])
    .with_see_also(vec!["trace", "build"])
}

fn cmd_trace() -> CommandInfo {
    CommandInfo::new(
        "trace",
        "one run of an instrumented build of a program (no debugger) and one answer: why it stopped (a line asked for, or a trap and its cause), where, \
         and the stack of its own functions \
         (exit 0 = an answer, a trap included, 1 = the place was not reached, is not in the program or the time ran out, 2 = bad command line, \
         3 = it could not be run; only in the command line written in cancho; docs/agent-debug.md)",
    )
    .idempotent(false)
    .add_argument("file", "string[]", "the .cho files of the program; program arguments follow `--`", true)
    .add_option("std", "bool", "make the standard library's source available", None)
    .add_option("std-dir", "string", "the directory of the standard library's files", None)
    .add_option("at", "string", "stop at FILE:LINE, the file as named on the command line (a line with no statement stops at the next)", None)
    .add_option("hit", "string", "with --at, stop at the Nth arrival (default 1)", None)
    .add_option("on-trap", "bool", "stop at the first trap (the default)", None)
    .add_option("unoptimized", "bool", "build with -O0", None)
    .add_option("timeout", "string", "seconds before the run is stopped (default 30)", None)
    .add_option("max-frames", "string", "how many frames of the stack to give (default 32)", None)
    .with_examples(vec![
        ("Where does it trap, and why?", "cancho trace app.cho"),
        ("Stop at a line", "cancho trace --at src/app.cho:42 src/app.cho"),
    ])
    .with_see_also(vec!["profile", "run"])
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
        cmd_satisfy(),
        cmd_layout(),
        cmd_print(),
        cmd_ingest(),
        cmd_agent_guidelines(),
        cmd_vcs(),
        cmd_install(),
        cmd_add(),
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
        "one or more .cho files -- a program is the set of files named on the command line; with none, \
         in a project, build the [[bin]] programs of cancho.toml (--bin <name> one of them)",
        false,
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
    .add_option(
        "target",
        "string",
        "generate code for this target triple (`--backend llvm` only; `wasm32-wasip1`, docs/wasm.md)",
        None,
    )
    .with_examples(vec![
        ("Build an executable", "cancho build hello.cho"),
        ("Build with the standard library available", "cancho build --std app.cho"),
        (
            "Build against a library beyond libc",
            "cancho build -o client client.cho -l ssl -l crypto",
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
    .add_argument("file", "string[]", "one or more .cho files", true)
    .add_option("std", "bool", "make the standard library's source available", None)
    .add_option("output", "enum[json]", "report every independent refusal as structured data", None)
    .add_option(
        "backend",
        "enum[cranelift|llvm]",
        "which backend generates the code check discards",
        Some(json!("llvm")),
    )
    .add_option(
        "target",
        "string",
        "generate code for this target triple (`--backend llvm` only; `wasm32-wasip1`, docs/wasm.md)",
        None,
    )
    .with_examples(vec![
        ("Type-check a file", "cancho check hello.cho"),
        ("Every refusal as structured data", "cancho check --output json app.cho"),
    ])
    .with_see_also(vec!["build", "agent-guidelines"])
}

fn cmd_run() -> CommandInfo {
    CommandInfo::new(
        "run",
        "build to a temporary executable and run it; the process's own exit status \
         is the compiled program's, not a fixed cancho code",
    )
    .idempotent(false)
    .add_argument("file", "string[]", "one or more .cho files", true)
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
    .add_option(
        "target",
        "string",
        "generate code for this target triple (`--backend llvm` only; `wasm32-wasip1`, docs/wasm.md)",
        None,
    )
    .with_examples(vec![("Run a program", "cancho run hello.cho")])
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
    .add_argument(
        "file",
        "string[]",
        "one or more .cho files, none declaring `main`; with none, in a project, run the [[test]] sets of \
         cancho.toml (--test <name> one of them)",
        false,
    )
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
    .with_examples(vec![("Run a file's tests", "cancho test --std tests.cho")])
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
        "one or more .cho files, or directories searched for them",
        true,
    )
    .add_option("check", "bool", "report what would change and write nothing", None)
    .with_examples(vec![
        ("Format a tree in place", "cancho fmt src/"),
        ("Fail if anything is not canonical (CI)", "cancho fmt --check src/ tests/"),
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
    .add_argument("file", "string[]", "one or more .cho files", true)
    .add_option("std", "bool", "make the standard library's source available", None)
    .with_examples(vec![("Print every declaration's hash", "cancho ids app.cho")])
    .with_see_also(vec!["print", "check"])
}

fn cmd_authority() -> CommandInfo {
    CommandInfo::new(
        "authority",
        "derive the least grant a program provably needs from its own effect rows \
         (docs/authority.md)",
    )
    .idempotent(true)
    .add_argument("file", "string[]", "one or more .cho files", true)
    .add_option("std", "bool", "make the standard library's source available", None)
    .add_option("output", "enum[json]", "the report as data rather than prose", None)
    .add_option(
        "target",
        "string",
        "also report the WASI imports a module built for this target triple may make \
         (`wasm32-wasip1`, docs/wasm.md)",
        None,
    )
    .with_examples(vec![
        ("What authority does this program need?", "cancho authority --output json app.cho"),
        (
            "And what may a WebAssembly build of it import?",
            "cancho authority --target wasm32-wasip1 --output json app.cho",
        ),
    ])
    .with_see_also(vec!["check", "layout"])
}

fn cmd_satisfy() -> CommandInfo {
    CommandInfo::new(
        "satisfy",
        "the spec-to-implementation loop (docs/satisfy.md): a contract is a .cho file whose \
         `fn test_*` call the candidate, and satisfaction is the checker and the test runner \
         composed against one program, the verdict named with the candidate's recomputed \
         SigId (exit 0 = satisfied, 1 = refused by the checker, 4 = a test failed, \
         2 = bad command line or a contract with no tests)",
    )
    .idempotent(false)
    .add_argument("contract", "string", "the .cho file whose `fn test_*` call the candidate", false)
    .add_argument(
        "candidate",
        "string",
        "the .cho file whose declarations the contract's tests call",
        false,
    )
    .add_option("std", "bool", "make the standard library's source available", None)
    .with_examples(vec![(
        "Ask a contract of an implementation",
        "cancho satisfy --std tests/satisfy/max.contract.cho tests/satisfy/max.candidate.cho",
    )])
    .with_see_also(vec!["test", "check", "ids"])
}

fn cmd_layout() -> CommandInfo {
    CommandInfo::new(
        "layout",
        "report struct field layout and what packing or reordering would cost or \
         save (docs/layout.md)",
    )
    .idempotent(true)
    .add_argument("file", "string[]", "one or more .cho files", true)
    .add_option("std", "bool", "make the standard library's source available", None)
    .with_examples(vec![("Report struct layout", "cancho layout app.cho")])
    .with_see_also(vec!["ids"])
}

fn cmd_print() -> CommandInfo {
    CommandInfo::new(
        "print",
        "render one parsed file in canonical form -- the AST-to-text direction of \
         the pipeline, not a formatter (comments never reach the AST)",
    )
    .idempotent(true)
    .add_argument("file", "string", "exactly one .cho file", true)
    .add_option(
        "output",
        "enum[json]",
        "the tree as JSON rather than canonical text (docs/structured-ingest.md)",
        None,
    )
    .with_examples(vec![
        ("Render a file canonically", "cancho print app.cho"),
        ("The same tree as data, for an agent to read", "cancho print --output json app.cho"),
    ])
    .with_see_also(vec!["ids", "ingest"])
}

fn cmd_ingest() -> CommandInfo {
    CommandInfo::new(
        "ingest",
        "the missing half of `print`: a JSON unit in, canonical .cho text out (docs/structured-ingest.md) -- an agent writes the data form and the compiler checks it against the same vocabulary the encoder writes, refusing with a rule like any other (ingest-json, ingest-node, ingest-arity)",
    )
    .idempotent(true)
    .add_argument("file", "string", "exactly one .json file", true)
    .add_option("output", "enum[json]", "a refusal as data: rule, message, position (docs/structured-ingest.md)", None)
    .with_examples(vec![
        ("Render a JSON unit as canonical text", "cancho ingest unit.json"),
        (
            "The loop an agent closes with it",
            "cancho print --output json app.cho > unit.json && cancho ingest unit.json",
        ),
    ])
    .with_see_also(vec!["print", "check"])
}

fn cmd_agent_guidelines() -> CommandInfo {
    CommandInfo::new(
        "agent-guidelines",
        "emit AGENTS.md: how to write cancho in one page rather than in 42 \
         documents. Every checked code block in it is run by the test suite, so a \
         guideline that stops being true is a red build",
    )
    .idempotent(true)
    .with_examples(vec![
        ("Read the rules", "cancho agent-guidelines"),
        ("Capture into a repo", "cancho agent-guidelines > AGENTS.md"),
    ])
    .with_see_also(vec!["skill", "introspect"])
}

fn cmd_install() -> CommandInfo {
    CommandInfo::new(
        "install",
        "fetch and check the dependencies named in cancho.toml into build/deps, after checking the \
         compiler is the one the project was written for (docs/package-system.md section 8)",
    )
    .idempotent(true)
    .add_option(
        "ignore-compiler-rev",
        "bool",
        "go on although this compiler is not the commit the project names",
        None,
    )
    .with_examples(vec![("Fetch the project's dependencies", "cancho install")])
    .with_see_also(vec!["add", "build"])
}

fn cmd_add() -> CommandInfo {
    CommandInfo::new(
        "add",
        "add a dependency to cancho.toml, pinned to a full commit hash (a --ref is resolved once and \
         the hash written), then install it; the file is put back if the install fails",
    )
    .idempotent(false)
    .add_argument("name", "string", "what the dependency is called in the project", true)
    .add_argument("git-url", "string", "the repository", true)
    .add_option("rev", "string", "a full commit hash", None)
    .add_option("ref", "string", "a branch or tag, resolved once (default: the repository's head)", None)
    .add_option("path", "string", "the store inside the repository (default .cancho-vcs)", None)
    .with_examples(vec![(
        "Depend on a library at its current head",
        "cancho add log https://github.com/alpibrusl/cancho-log --path .cancho-vcs/log",
    )])
    .with_see_also(vec!["install", "vcs"])
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
        ("Publish every declaration in a file", "cancho vcs publish --store .cancho-vcs app.cho"),
        ("List what a store has published", "cancho vcs log --store .cancho-vcs"),
        ("Re-verify every pin a store has ever published", "cancho vcs resolve .cancho-vcs"),
        (
            "Pin names by hash into a lock file",
            "cancho vcs lock --store .cancho-vcs -o app.lock connect octets_of",
        ),
        (
            "Fetch verified sources for a lock",
            "cancho vcs fetch --lock app.lock --store .cancho-vcs -o vendor/",
        ),
    ])
    .with_see_also(vec!["check"])
}
