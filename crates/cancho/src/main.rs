//! The cancho bootstrap compiler CLI.
//!
//! The v0 compiler is hosted in Rust (an open decision in #1, settled here and
//! recorded in `docs/bootstrap.md`). It compiles one file at a time: M0 has no
//! module system, and a unit is a file.
//!
//! Exit codes are semantic, as they are across the rest of the ecosystem:
//!
//! | code | meaning |
//! |---|---|
//! | 0 | success |
//! | 1 | the program was refused (a located diagnostic was printed) -- including by the compiler's own failure, rule `internal` (`docs/internal-errors.md`) |
//! | 2 | the command line was wrong |
//! | 3 | the environment failed: no linker, unwritable output, unsupported host |
//! | 4 | `test` only: the program built, and at least one `test_*` function failed |
//!
//! `run` is the exception: it replaces its own status with the compiled
//! program's, so `cancho run p.cho` and `cancho build p.cho && ./p` agree.
#![forbid(unsafe_code)]

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use target_lexicon::Triple;

use cancho_syntax::{Ast, Rule, SourceFile, SourceMap};
use foreign_report::{Attribution, ForeignReach};

mod acli;
mod fmt_cli;
mod foreign_report;
mod project;
mod satisfy_cli;
mod scratch;
mod selfhost_build;
mod test_cli;
mod vcs_args;
mod vcs_cli;
mod vcs_decl;
mod vcs_dir;
mod vcs_git;
mod vcs_origin;
mod vcs_record;
mod vcs_repo;
mod vcs_sync;

const USAGE: &str = "\
cancho — the bootstrap compiler for the cancho systems dialect

usage:
    cancho build <file.cho>... [-o <output>] [--emit exe|obj] [--std] [--backend llvm] [--target <triple>] [-l <name>]... [-L <path>]... [--compiler <exe>]
    cancho check <file.cho>... [--std] [--output json] [--backend llvm] [--target <triple>]
    cancho run   <file.cho>... [--std] [--backend llvm] [--target <triple>] [-l <name>]... [-L <path>]...
    cancho test  <file.cho>... [--std] [--backend llvm] [-l <name>]... [-L <path>]...
    cancho satisfy <contract.cho> <candidate.cho> [--std]
    cancho fmt   <file.cho|dir>... [--check]
    cancho ids   <file.cho>... [--std]
    cancho authority <file.cho>... [--std] [--output json] [--target <triple>]
    cancho layout    <file.cho>... [--std]
    cancho print <file.cho> [--output json]
    cancho ingest <file.json> [--output json]
    cancho agent-guidelines
    cancho introspect [--output json]
    cancho skill [--output json] [<out-file>]
    cancho install [--ignore-compiler-rev]
    cancho add <name> <git-url> [--rev <hash> | --ref <name>] [--path <dir>] [--ignore-compiler-rev]
    cancho build [--bin <name>] [--ignore-compiler-rev]
    cancho test  [--test <name>] [--ignore-compiler-rev]
    cancho vcs publish [--store <dir>] [--std] [--requires <lock>[:<dep-store>]]... <file.cho>
    cancho vcs publish [--store <root>] [--std] --dir <dir>
    cancho vcs sync [--dir <dir>] [--std] [--check]
    cancho vcs log     [--store <dir>]
    cancho vcs resolve [--lock <file>] [<store-dir>]
    cancho vcs lock (--store <dir> | --git <url> (--rev <hash> | --ref <name>) [--path <dir>]) -o <file> (--all | <name>...)
    cancho vcs fetch --lock <file> [--store <dir>] -o <dir>
    cancho --version

options:
    -o <output>     where to write the result (default: the first input's stem)
    --emit exe|obj  emit a linked executable (default) or a bare object file
    --std           make the standard library's source available
    --compiler <exe>  (build) which compiler builds the program: the path of a `cancho` written in cancho
                      (`scripts/build-standalone.sh`), or `CANCHO_COMPILER`. This binary writes no code itself any more
                      (`docs/removing-rust.md`, Phase 5), so naming none is refused, and so are `rust` and `auto`
    --output json   `check` and `authority` as data rather than prose
    --backend llvm
                    which backend generates code (default: llvm, since #127).
                    `llvm` is the only one, written by the compiler in cancho
                    (docs/llvm-backend.md); accepted, and ignored here.
    --target <triple>
                    generate code for another target (`--backend llvm` only;
                    docs/wasm.md). Only `wasm32-wasip1` is built so far: it
                    needs `CLANG` (a clang with the wasm32 target), `WASM_LD`,
                    and `WASI_SYSROOT` (a wasi-libc sysroot); `run` uses
                    `WASMTIME` and `WASMTIME_FLAGS` (e.g. `--dir=.`). A builtin
                    with no WASI meaning is refused.
    -l <name>       link `lib<name>` at the final link step (`build`/`run`
                    only); repeatable, passed to `cc` unexamined
    -L <path>       add `<path>` to the linker's search path for the above;
                    repeatable, same treatment

A program is the set of files named on the command line, in any order.
Each file is in a module -- the root, unless it says `module a.b;` -- and
reaches another module's names through `import`. See docs/many-files.md
and docs/modules.md.

`--std` adds the standard library's source, which is compiled into this
binary rather than looked up on disk: no search path, no manifest. It is
not a prelude -- a program still writes `import std.io;` where it uses
one -- and a declaration nothing calls emits nothing. See
docs/standard-library.md.

`check --output json` answers every refusal as data: a stable `rule`
tag, the same sentence, what the rule enforces, and a position. The exit
status is unchanged -- 1 for a refused program -- and `check` reports
every independent refusal rather than the first. See
docs/agent-errors.md.

`check` stops after the front end: it no longer generates the code and
discards it (the Rust backend that did is gone, docs/removing-rust.md),
so a program it accepts is not yet known to build.

`test` runs every `fn test_*` in the files named, one process each, and
exits 4 if any failed. A test answers 0 to pass; `std.test`'s `assert`
traps otherwise. The files must not declare `main`: the runner writes
one. See docs/testing.md.

`fmt` rewrites each file (or each `.cho` under a directory) in canonical
layout, keeping its comments, its blank lines and the way it spelled each
literal. `--check` writes nothing and exits 1 if any file would change.
A file it cannot format without moving or losing a comment is left alone
and reported, also exit 1. See docs/formatting.md.

`agent-guidelines` prints AGENTS.md, which is how to write cancho in
one page rather than in 42 documents. Every checked code block in it is
run by the test suite, so a guideline that stops being true is a red
build. See docs/agent-errors.md for what a refusal says as data.

`introspect` prints the full command tree as JSON (ACLI spec §1.2) and
`skill` prints a generated agentskills.io SKILL.md -- both sourced from
one registration in src/acli.rs, so the command list an agent reads
cannot drift from the dispatch table the way a hand-written doc could.
Neither is the exit-code contract: `skill`'s own \"Exit codes\" section
is the ACLI SDK's generic template, not this binary's. `--version`,
`build`, `check`, `run` keep the codes at the top of this file, and
`check --output json` keeps its own `rule` tag as the machine-readable
form. See docs/agent-cli.md.

`ids` prints each declaration's content hash: a signature and a body for
every function, one identity for every type. A unit hashes its content,
not the file it sits in. See docs/canonical-ast.md.

`print` renders one parsed file in canonical form. It is the AST-to-text
direction of that same pipeline, not a formatter: comments never reach the
AST, so they are not in the output. `fmt` is the formatter.

`vcs publish` logs every declaration in one file as an `AddFunction`
operation in a content-addressed store at `--store` (default
`.cancho-vcs`), gated by the same checker `check` runs. A second publish
of an unchanged declaration is a no-op; a second publish of a *changed*
one is refused -- incremental publish is design-stage, not built. `vcs
log` lists what a store already has. `--requires <lock>:<dep-store>`
(repeatable) is for a package that itself needs another package: the
identity published is still this file's own (structural, unaffected by
any dependency), but the file must still type-check with that
dependency's source in scope, and the pin is recorded so a later `vcs
resolve`/`vcs fetch` finds it without the consumer needing to know it
exists. See docs/vcs-publish.md and docs/package-system.md §4.6.
`--std` lets the package import the bundled library; a publish that
imports `std` (itself or through a `--requires` package) without it is
refused. `resolve` and `fetch` need no flag, and `fetch` notes when the
consumer must `build --std` (docs/package-system.md §4.8).

`vcs resolve <store-dir>` never trusts that store's own manifest: it
re-parses and re-typechecks the source behind every pin under today's
compiler, then re-derives each declaration's identity and refuses, with
every mismatch reported at once, if a pin no longer matches its own
source, its source no longer type-checks, or a source blob is missing.
Exit 0 only if every pin still resolves exactly as published. `--lock
<file>` scopes this to just the names `vcs lock` pinned, instead of
everything the store has ever published. If the store itself has a
dependency (`--requires` at publish time), its own closure is walked
and verified too, refusing on a cycle or a diamond conflict rather than
guessing. See docs/package-system.md §4.2 and §4.6.

`vcs lock --store <dir> -o <file> <name>...` looks each name up in a
dependency store's manifest and pins it -- by hash, not by name -- into
a lock file at `-o`, refusing if a name is unpublished or ambiguous.
A name is chosen once, when it is first locked; nothing later can
silently substitute a different declaration under it. See
docs/package-system.md §4.5.

`vcs fetch --lock <file> --store <dir> -o <dir>` re-verifies every pin
in a lock the same way `vcs resolve --lock` does, then writes each
distinct verified source to `-o` as `<source_hash>.cho` -- nothing is
written unless everything verifies. This needs no compiler support: a
program is already the set of files named on the command line, and
`import` already resolves a name against whatever module is among
them (docs/modules.md §4.2), so a fetched file is `import`-able the
moment it exists on disk. A dependency's own dependency (`--requires`)
is fetched too, into the same `-o`, so a consumer of a package that has
one never has to fetch it separately. See docs/package-system.md §4.6
and §4.7.

`-l`/`-L` are `cc`'s own flags, passed through unexamined: this project
invents no manifest and no dependency resolution, only the ability to
ask the linker for a library beyond libc at all, which nothing here
could do before. `c_ptr` (docs/opaque-pointers.md) lets a foreign
signature name a handle from a library like OpenSSL; this is what
makes that library reachable at link time. See docs/foreign-linking.md.
";

const EXIT_REFUSED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_ENVIRONMENT: u8 = 3;
const EXIT_TEST_FAILED: u8 = 4;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(Failure { message, code }) => {
            eprintln!("{message}");
            ExitCode::from(code)
        }
    }
}

struct Failure {
    message: String,
    code: u8,
}

fn refused(message: impl Into<String>) -> Failure {
    Failure { message: message.into(), code: EXIT_REFUSED }
}

fn usage(message: impl Into<String>) -> Failure {
    Failure { message: format!("{}\n\n{USAGE}", message.into()), code: EXIT_USAGE }
}

fn environment(message: impl Into<String>) -> Failure {
    Failure { message: message.into(), code: EXIT_ENVIRONMENT }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Emit {
    Exe,
    Obj,
}

/// What `build` generates code for: the target triple when it is not the host's (`docs/wasm.md`). The backend is no
/// longer a choice: the Rust code generators are gone (`docs/removing-rust.md`, Phase 5).
#[derive(Clone, Copy)]
struct Codegen<'a> {
    target: Option<&'a Triple>,
}

/// Strip a leading `--output FORMAT` (or `--output=FORMAT`) from `args`,
/// for the two ACLI-native commands (`introspect`, `skill`). Kept
/// separate from `parse_args`'s own `--output json` (a bool flag on
/// `check`/`authority`, unrelated to this): those two commands are not
/// `Invocation`-shaped at all, and this accepts `text`/`json`/`table`
/// rather than just `json`, per the acli crate's own `OutputFormat`.
fn parse_agent_output(args: &[String]) -> Result<(::acli::OutputFormat, Vec<String>), Failure> {
    use std::str::FromStr;
    let mut out: Vec<String> = Vec::with_capacity(args.len());
    let mut format = ::acli::OutputFormat::Text;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--output" {
            let value = it.next().ok_or_else(|| usage("`--output` needs a form"))?;
            format = ::acli::OutputFormat::from_str(value).map_err(usage)?;
        } else if let Some(v) = arg.strip_prefix("--output=") {
            format = ::acli::OutputFormat::from_str(v).map_err(usage)?;
        } else {
            out.push(arg.clone());
        }
    }
    Ok((format, out))
}

fn run(args: &[String]) -> Result<ExitCode, Failure> {
    let Some(command) = args.first() else {
        return Err(usage("no command given"));
    };

    match command.as_str() {
        "--version" | "-V" => {
            println!(
                "cancho {} (rev {}, host {})",
                env!("CARGO_PKG_VERSION"),
                project::COMPILER_REV,
                Triple::host()
            );
            Ok(ExitCode::SUCCESS)
        }
        "--help" | "-h" | "help" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        // ACLI built-ins (docs/agent-cli.md): the command tree and a
        // generated skill file, so an agent learns the surface from the
        // binary itself rather than from a doc that can drift.
        "introspect" => {
            let (fmt, _) = parse_agent_output(&args[1..])?;
            acli::build_app().handle_introspect(&fmt);
            Ok(ExitCode::SUCCESS)
        }
        "skill" => {
            let (fmt, rest) = parse_agent_output(&args[1..])?;
            let out_path = rest.first().map(String::as_str);
            acli::build_app().handle_skill(out_path, &fmt);
            Ok(ExitCode::SUCCESS)
        }
        "check" => {
            let Invocation { inputs, with_std, json, target, .. } =
                parse_args(&args[1..], false, false)?;
            check_program(&inputs, with_std, json, target.as_ref())
        }
        // The one command that reads no program: it is a contract with
        // whoever is about to write one.
        "agent-guidelines" => {
            if args.len() > 1 {
                return Err(usage("`agent-guidelines` takes no arguments"));
            }
            let stdout = io::stdout();
            let mut out = stdout.lock();
            match out.write_all(AGENT_GUIDELINES.as_bytes()).and_then(|()| out.flush()) {
                Ok(()) => Ok(ExitCode::SUCCESS),
                // The reader stopped listening, which is their business.
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(ExitCode::SUCCESS),
                Err(e) => Err(environment(format!("cannot write to stdout: {e}"))),
            }
        }
        // `docs/vcs-publish.md`: a real `.cho` file through
        // `crates/cancho-vcs` for the first time.
        "vcs" => vcs_cli::cmd_vcs(&args[1..]),
        // `docs/many-files.md` §5: printing is about text, and text is
        // what a file is -- so this renders exactly one.
        "print" => {
            let Invocation { inputs, json, .. } = parse_args(&args[1..], false, false)?;
            let [input] = &inputs[..] else {
                return Err(usage("`print` renders one file at a time"));
            };
            let text = std::fs::read_to_string(input)
                .map_err(|e| environment(format!("cannot read `{}`: {e}", input.display())))?;
            let file = SourceFile::new(input.display().to_string(), text);
            let ast = cancho_syntax::parse(&file.text).map_err(|d| refused(d.render(&file)))?;
            // `--output json` is the data form of the same tree
            // (`docs/structured-ingest.md` §2): the AST as JSON rather than
            // as canonical text, for an agent that reads the one and writes
            // the other.
            if json {
                println!("{}", cancho_syntax::write_json(&cancho_syntax::ast_to_json(&ast)));
                return Ok(ExitCode::SUCCESS);
            }
            print!("{}", cancho_syntax::print(&ast));
            Ok(ExitCode::SUCCESS)
        }
        // `docs/structured-ingest.md`: the other direction of the same
        // pipeline — JSON in, canonical `.cho` text out, with the tree
        // checked against the same vocabulary the encoder writes and a
        // refusal carrying a rule like any other.
        "ingest" => {
            let Invocation { inputs, json: as_data, .. } = parse_args(&args[1..], false, false)?;
            let [input] = &inputs[..] else {
                return Err(usage("`ingest` reads one file at a time"));
            };
            let text = std::fs::read_to_string(input)
                .map_err(|e| environment(format!("cannot read `{}`: {e}", input.display())))?;
            let file = SourceFile::new(input.display().to_string(), text);
            let build = cancho_syntax::read_json(&file.text)
                .and_then(|json| cancho_syntax::json_to_ast(&json));
            let Ok(ast) = build else {
                // A refusal is data here too (`docs/structured-ingest.md`
                // §3): an agent's first contact with this path is a
                // rejected file, and `--output json` answers the rule, the
                // sentence and the position the way `check` does — the
                // same shape, the same exit code, no new vocabulary.
                let d = build.expect_err("checked above");
                if as_data {
                    let line = 1 + file.text[..d.span.start as usize]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count();
                    let column = d.span.start + 1;
                    let body = format!(
                        "{{\n  \"refused\": [\n    {{\n      \"rule\": \"{}\",\n      \"message\": \"{}\",\n      \"explanation\": \"{}\",\n      \"position\": {{ \"file\": \"{}\", \"line\": {line}, \"column\": {column} }}\n    }}\n  ]\n}}\n",
                        d.rule.tag(),
                        escaped(&d.message),
                        escaped(d.rule.explanation()),
                        escaped(&file.path),
                    );
                    let stdout = io::stdout();
                    let mut out = stdout.lock();
                    let _ = out.write_all(body.as_bytes()).and_then(|()| out.flush());
                    return Ok(ExitCode::from(EXIT_REFUSED));
                }
                return Err(refused(d.render(&file)));
            };
            print!("{}", cancho_syntax::print(&ast));
            Ok(ExitCode::SUCCESS)
        }
        // `docs/formatting.md`.
        "fmt" => fmt_cli::cmd_fmt(&args[1..]),
        // `docs/testing.md` §3.
        "test" if project::wants_project_test(&args[1..]) => project::cmd_test(&args[1..]),
        "test" => test_cli::cmd_test(&args[1..]),
        "satisfy" => satisfy_cli::cmd_satisfy(&args[1..]),
        "ids" => {
            let Invocation { inputs, with_std, .. } = parse_args(&args[1..], false, false)?;
            print_ids(&inputs, with_std)?;
            Ok(ExitCode::SUCCESS)
        }
        "authority" => {
            let Invocation { inputs, with_std, json, target, .. } =
                parse_args(&args[1..], false, false)?;
            print_authority(&inputs, with_std, json, target.as_ref())?;
            Ok(ExitCode::SUCCESS)
        }
        // `cancho layout` is the compiler in cancho's (`layoutrep.cho`); the Rust one measured with the
        // Cranelift leaf counts, which went with `cancho-codegen` (`docs/removing-rust.md`, Phase 5).
        "layout" => Err(refused(
            "`cancho layout` is no longer written by the Rust compiler (the Cranelift backend it measured with is retired, `docs/removing-rust.md` D2); use the compiler in cancho",
        )),
        // `docs/package-system.md` §8: the project file.
        "install" => project::cmd_install(&args[1..]),
        "add" => project::cmd_add(&args[1..]),
        "build" if project::wants_project(&args[1..]) => project::cmd_build(&args[1..]),
        "build" => {
            let Invocation {
                inputs,
                output,
                emit,
                with_std,
                target,
                link_libs,
                link_paths,
                compiler,
                ..
            } = parse_args(&args[1..], true, true)?;
            let output = output.unwrap_or_else(|| default_output(&inputs[0], emit));
            selfhost_build::set_flag(compiler);
            build(
                &inputs,
                &output,
                emit,
                with_std,
                Codegen { target: target.as_ref() },
                &link_libs,
                &link_paths,
            )?;
            Ok(ExitCode::SUCCESS)
        }
        "run" => {
            let Invocation { inputs, with_std, target, link_libs, link_paths, .. } =
                parse_args(&args[1..], false, true)?;
            let dir = scratch::private_temp_dir("cancho-run")
                .map_err(|e| environment(format!("cannot create a temporary directory: {e}")))?;
            let exe =
                dir.join(default_output(&inputs[0], Emit::Exe).file_name().unwrap_or_default());
            let result = build(
                &inputs,
                &exe,
                Emit::Exe,
                with_std,
                Codegen { target: target.as_ref() },
                &link_libs,
                &link_paths,
            )
            .and_then(|()| match target.as_ref() {
                Some(triple) => Err(environment(format!(
                    "`run` cannot execute a `{triple}` program on this host; use `build`"
                ))),
                None => Command::new(&exe)
                    .status()
                    .map_err(|e| environment(format!("cannot run `{}`: {e}", exe.display()))),
            });
            let _ = std::fs::remove_dir_all(&dir);
            let status = result?;
            Ok(ExitCode::from(status.code().unwrap_or(EXIT_ENVIRONMENT as i32) as u8))
        }
        other => Err(usage(format!("unknown command `{other}`"))),
    }
}

/// The standard library's source, compiled into this binary
/// (`docs/standard-library.md` §2).
///
/// Embedded rather than looked up, so `--std` adds **no search path**,
/// no manifest and no build step -- `modules.md` §6 promised none of
/// those, and this keeps the promise by never going near the disk.
///
/// The cost is that the library's version is the compiler's version.
/// For a language at this stage that is the right trade -- one artifact,
/// one thing to install, nothing to resolve -- and §2.1 records it as
/// the first thing to revisit when a package story exists.
/// `AGENTS.md`, compiled into the binary the way the library is.
///
/// `docs/agent-errors.md` §7 named the gap: lex-lang's contract says a
/// downstream repo copies `AGENT_GUIDELINES.md`, and cancho is a
/// different language, so it needs its own. Carried in the binary for
/// the same reason `--std` is — a reader with the compiler needs
/// nothing else, and a file on disk is a file that can be absent.
const AGENT_GUIDELINES: &str = include_str!("../../../AGENTS.md");

// The standard library, one `(name, text)` per file of `std/`, in the order of `std/std.files` (the order `--std` adds them): generated by
// `build.rs`, so a new module of the library is one line of that file and nothing here.
include!(concat!(env!("OUT_DIR"), "/std_table.rs"));

/// What a command line asked for.
///
/// A struct rather than a tuple because the fifth field is where a tuple
/// stops being readable at the call site -- the same reason
/// `resolve_type_at` became a `Resolving`.
struct Invocation {
    inputs: Vec<PathBuf>,
    output: Option<PathBuf>,
    emit: Emit,
    with_std: bool,
    /// `--output json` (`docs/authority.md` §3): the report as data rather
    /// than as prose, for a consumer that checks it against a grant.
    json: bool,
    /// `--target` (`docs/wasm.md`): the triple to generate code for, if not the host's.
    target: Option<Triple>,
    /// `-l <name>` (`docs/foreign-linking.md` §3): libraries to link
    /// beyond libc, in the order given, passed to `cc` unexamined.
    link_libs: Vec<String>,
    /// `-L <path>` (`docs/foreign-linking.md` §3): search paths for the
    /// above, same order, same treatment.
    link_paths: Vec<String>,
    /// `--compiler <exe>` (`docs/self-hosting.md`): build with the compiler written in cancho.
    compiler: Option<PathBuf>,
}

fn parse_args(
    args: &[String],
    allow_output: bool,
    allow_link: bool,
) -> Result<Invocation, Failure> {
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut output = None;
    let mut emit = Emit::Exe;
    let mut with_std = false;
    let mut json = false;
    let mut target: Option<Triple> = None;
    let mut link_libs = Vec::new();
    let mut link_paths = Vec::new();
    let mut compiler: Option<PathBuf> = None;
    let mut it = args.iter();

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-o" if allow_output => {
                let value = it.next().ok_or_else(|| usage("`-o` needs a path"))?;
                output = Some(PathBuf::from(value));
            }
            "--emit" if allow_output => {
                emit = match it.next().map(String::as_str) {
                    Some("exe") => Emit::Exe,
                    Some("obj") => Emit::Obj,
                    Some(other) => return Err(usage(format!("unknown emit kind `{other}`"))),
                    None => return Err(usage("`--emit` needs a kind")),
                };
            }
            // `docs/standard-library.md` §2. Opt-in, and never an
            // implicit prelude: this decides whether the library's
            // *source* is present, never whether a name is in scope. A
            // program still writes `import std.io;` where it uses it.
            "--std" => with_std = true,
            "--output" => match it.next().map(String::as_str) {
                Some("json") => json = true,
                Some(other) => return Err(usage(format!("unknown output form `{other}`"))),
                None => return Err(usage("`--output` needs a form")),
            },
            "--backend" => match it.next().map(String::as_str) {
                Some("cranelift") => {
                    return Err(usage(
                        "`--backend cranelift` is retired with the Rust compiler's Cranelift backend (`docs/removing-rust.md` D2); the only backend is `llvm`",
                    ));
                }
                Some("llvm") => {}
                Some(other) => return Err(usage(format!("unknown backend `{other}`"))),
                None => return Err(usage("`--backend` needs a name")),
            },
            "--target" => {
                let value = it.next().ok_or_else(|| usage("`--target` needs a triple"))?;
                target = Some(
                    value
                        .parse::<Triple>()
                        .map_err(|e| usage(format!("unknown target `{value}`: {e}")))?,
                );
            }
            "--compiler" if allow_output => {
                let value = it.next().ok_or_else(|| usage("`--compiler` needs a path"))?;
                compiler = Some(PathBuf::from(value));
            }
            "-l" if allow_link => {
                let value = it.next().ok_or_else(|| usage("`-l` needs a library name"))?;
                link_libs.push(value.clone());
            }
            "-L" if allow_link => {
                let value = it.next().ok_or_else(|| usage("`-L` needs a path"))?;
                link_paths.push(value.clone());
            }
            other if other.starts_with('-') => {
                return Err(usage(format!("unknown option `{other}`")));
            }
            // `docs/many-files.md` §2: a program is a set of files, named
            // on the command line in any order.
            other => inputs.push(PathBuf::from(other)),
        }
    }

    if inputs.is_empty() {
        return Err(usage("no input file given"));
    }
    Ok(Invocation { inputs, output, emit, with_std, json, target, link_libs, link_paths, compiler })
}

fn default_output(input: &Path, emit: Emit) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default();
    match emit {
        Emit::Exe => PathBuf::from(stem),
        Emit::Obj => PathBuf::from(format!("{}.o", stem.to_string_lossy())),
    }
}

/// Read and parse every file of a program into one AST
/// (`docs/many-files.md` §2).
///
/// The `SourceMap` hands out a base offset per file and resolves any
/// diagnostic's span back to the file it came from, so nothing downstream
/// of here learns that a program can have more than one (§4).
fn parse_program(inputs: &[PathBuf], with_std: bool) -> Result<(Ast, SourceMap), ParseFailure> {
    let mut map = SourceMap::new();
    let mut ast = Ast::new();
    let mut sources = Vec::new();
    for input in inputs {
        let text = std::fs::read_to_string(input).map_err(|e| ParseFailure {
            rendered: environment(format!("cannot read `{}`: {e}", input.display())),
            // Nothing was parsed, so there is no span and no rule about
            // the program: `docs/agent-errors.md` §7 keeps the
            // environment's failures out of the refusal vocabulary.
            diagnostic: cancho_syntax::Diagnostic::new(
                Rule::ProgramShape,
                "unreadable input",
                cancho_syntax::Span::new(0, 0),
            ),
            map: None,
        })?;
        let base = map.add(input.display().to_string(), text.clone());
        sources.push((text, base));
    }
    // The library is more files of the same program (`many-files.md` §2)
    // -- it arrives from `include_str!` rather than from the disk, and
    // nothing downstream of here can tell the difference. Named
    // `<std>/io.cho` in the map, so a diagnostic inside the library says
    // so rather than naming a path that does not exist.
    if with_std {
        for (name, text) in STD {
            let base = map.add((*name).to_owned(), (*text).to_owned());
            sources.push(((*text).to_owned(), base));
        }
    }
    for (text, base) in &sources {
        if let Err(d) = cancho_syntax::parse_into(&mut ast, text, *base) {
            // The rendered sentence for the caller that wants prose, and
            // the diagnostic itself for the one that wants the rule
            // (`docs/agent-errors.md` §5): a parse error has a rule like
            // any other refusal, and rendering it away was how
            // `unknown-escape` came to report as `type-mismatch`.
            let rendered = refused(d.render_in(&map));
            return Err(ParseFailure { rendered, diagnostic: d, map: Some(map) });
        }
    }
    Ok((ast, map))
}

/// A parse refusal, before it has been reduced to a sentence.
struct ParseFailure {
    rendered: Failure,
    diagnostic: cancho_syntax::Diagnostic,
    /// The files read so far, when the failure is a parse error rather
    /// than an unreadable input: what `--output json` resolves the
    /// diagnostic's span against.
    map: Option<SourceMap>,
}

impl From<ParseFailure> for Failure {
    fn from(failure: ParseFailure) -> Failure {
        failure.rendered
    }
}

/// Read, parse and lower a program, reporting any refusal with its source line.
fn compile_to_ir(inputs: &[PathBuf], with_std: bool) -> Result<cancho_ir::Program, Failure> {
    match compile_reporting(inputs, with_std) {
        Ok((program, _)) => Ok(program),
        Err(refusals) => {
            let (_, map) = parse_program(inputs, with_std)?;
            let text: Vec<String> = refusals.iter().map(|r| r.render(&map)).collect();
            Err(refused(text.join("\n\n")))
        }
    }
}

/// Is this one of WASI's operating systems (`wasm32-wasip1`, `-wasip2`, `-wasi`)?
fn is_wasi(triple: &Triple) -> bool {
    matches!(
        triple.operating_system,
        target_lexicon::OperatingSystem::Wasi
            | target_lexicon::OperatingSystem::WasiP1
            | target_lexicon::OperatingSystem::WasiP2
    )
}

/// The refusals a target adds before any code is generated: what the program
/// reaches that the target cannot do (`cancho_ir::unsupported_on_target`).
/// Empty for the host and for anything but WASI, whose gaps are the only ones
/// this knows (`docs/wasm.md`).
fn target_refusals(program: &cancho_ir::Program, target: Option<&Triple>) -> Vec<Refusal> {
    let Some(triple) = target else { return Vec::new() };
    if !is_wasi(triple) {
        return Vec::new();
    }
    cancho_ir::unsupported_on_target(program, cancho_ir::Os::Wasi, &triple.to_string())
        .into_iter()
        .map(|d| Refusal { rule: d.rule, message: d.message, span: Some(d.span) })
        .collect()
}

/// `check`: type-check a program and say what is wrong with it.
///
/// The prose is what it always was — `docs/agent-errors.md` §6 keeps
/// every message byte for byte — and `--output json` adds the form a
/// program can read without a regular expression over English (§5).
///
/// Either way the exit status is the same: **1** for a refused program.
/// A machine-readable body does not change what happened, and this
/// repository already has semantic exit codes.
fn check_program(
    inputs: &[PathBuf],
    with_std: bool,
    json: bool,
    target: Option<&Triple>,
) -> Result<ExitCode, Failure> {
    // The front end only: the Rust backend that used to generate code here and throw it away is gone (`docs/removing-rust.md`,
    // Phase 5), so `check` no longer says whether `build` can generate code for the program; the compiler in cancho does.
    let refusals = match compile_reporting(inputs, with_std) {
        // What the target cannot do is answered first and without a code
        // generator, so it needs no wasm toolchain and says where it is.
        Ok((program, _)) if !target_refusals(&program, target).is_empty() => {
            target_refusals(&program, target)
        }
        Ok(_) => Vec::new(),
        Err(refusals) => refusals,
    };
    if !json {
        if refusals.is_empty() {
            return Ok(ExitCode::SUCCESS);
        }
        // Re-parsing to render is cheap beside the checking that just
        // happened, and it keeps `compile_reporting` from having to hand
        // back a map it could not build on a parse error.
        let (_, map) = parse_program(inputs, with_std)?;
        let text: Vec<String> = refusals.iter().map(|r| r.render(&map)).collect();
        return Err(refused(text.join("\n\n")));
    }

    let map = match parse_program(inputs, with_std) {
        Ok((_, map)) => Some(map),
        Err(failure) => failure.map,
    };
    if refusals.is_empty() {
        // An empty list rather than an empty-looking one: a consumer
        // checks the length, and `[]` is the shape it expects.
        let stdout = io::stdout();
        let mut out = stdout.lock();
        let _ = out.write_all(b"{\n  \"refused\": []\n}\n").and_then(|()| out.flush());
        return Ok(ExitCode::SUCCESS);
    }
    let mut body = String::from("{\n  \"refused\": [\n");
    for (index, refusal) in refusals.iter().enumerate() {
        let position = match (&map, refusal.span) {
            (Some(map), Some(span)) => match map.position_of(span) {
                Some((file, line, column)) => format!(
                    "{{ \"file\": \"{}\", \"line\": {line}, \"column\": {column} }}",
                    escaped(file)
                ),
                // §1.1: a refusal about the program rather than about a
                // span in it has nowhere to point, and says so.
                None => "null".to_owned(),
            },
            _ => "null".to_owned(),
        };
        let comma = if index + 1 == refusals.len() { "" } else { "," };
        body.push_str(&format!(
            "    {{\n      \"rule\": \"{}\",\n      \"message\": \"{}\",\n      \"explanation\": \"{}\",\n      \"position\": {position}\n    }}{comma}\n",
            refusal.rule.tag(),
            escaped(&refusal.message),
            escaped(refusal.rule.explanation()),
        ));
    }
    body.push_str("  ]\n}\n");

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match out.write_all(body.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {}
        Err(e) => return Err(environment(format!("cannot write to stdout: {e}"))),
    }
    if refusals.is_empty() { Ok(ExitCode::SUCCESS) } else { Ok(ExitCode::from(EXIT_REFUSED)) }
}

/// One refusal, with the rule it enforces beside the sentence it wrote.
///
/// `docs/agent-errors.md` §5. `position` is absent for the refusals about
/// a program rather than a span in one (§1.1), which is an answer rather
/// than a gap.
struct Refusal {
    rule: Rule,
    message: String,
    span: Option<cancho_syntax::Span>,
}

impl Refusal {
    /// The sentence a person reads, byte for byte what it was before the
    /// rule was attached to it (§6).
    fn render(&self, map: &cancho_syntax::SourceMap) -> String {
        match self.span {
            Some(span) => {
                cancho_syntax::Diagnostic::new(self.rule, &self.message, span).render_in(map)
            }
            None => self.message.clone(),
        }
    }
}

/// Read, parse and lower, answering **every** refusal (§4).
fn compile_reporting(
    inputs: &[PathBuf],
    with_std: bool,
) -> Result<(cancho_ir::Program, cancho_syntax::SourceMap), Vec<Refusal>> {
    let (ast, map) = match parse_program(inputs, with_std) {
        Ok(pair) => pair,
        // A parse error ends the file: a program that did not parse has no
        // reliable second error, and inventing one teaches a reader to
        // chase phantoms (§4).
        // A parse error is a sentence at a span, like any other refusal,
        // so `--output json` gets both (`docs/agent-errors.md` §5). The
        // prose callers never see this: they parse again, fail the same
        // way, and print the rendered failure.
        Err(ParseFailure { diagnostic, map: Some(_), .. }) => {
            return Err(vec![Refusal {
                rule: diagnostic.rule,
                message: diagnostic.message,
                span: Some(diagnostic.span),
            }]);
        }
        // An unreadable input has no span to point at.
        Err(failure) => {
            return Err(vec![Refusal {
                rule: failure.diagnostic.rule,
                message: failure.rendered.message,
                span: None,
            }]);
        }
    };
    let program = match cancho_ir::lower_all(&ast) {
        Ok(program) => program,
        Err(all) => {
            return Err(all
                .into_iter()
                .map(|d| Refusal { rule: d.rule, message: d.message, span: Some(d.span) })
                .collect());
        }
    };
    let where_ = inputs[0].display().to_string();

    // `main` becomes the process entry point, so its shape is part of the
    // contract with the C runtime rather than a matter of taste.
    if let Some(entry) = program.find("main") {
        let entry = program.func(entry);
        // §8.2: the runtime hands over exactly one `World`, and it is the
        // only place authority enters a program. `World` is zero-sized, so
        // this parameter costs nothing at the machine level -- the C entry
        // point still calls `main` with no arguments.
        let world = program.world();
        if entry.n_params != 1 || entry.slots.first() != Some(&world) {
            return Err(vec![Refusal {
                rule: Rule::ProgramShape,
                message: format!(
                    "{where_}: error: `main` takes one argument, the `World` the runtime hands it"
                ),
                span: None,
            }]);
        }
        if entry.ret != cancho_types::Type::Int {
            return Err(vec![Refusal {
                rule: Rule::ProgramShape,
                message: format!("{where_}: error: `main` returns `int`, the process exit status"),
                span: None,
            }]);
        }
    } else {
        return Err(vec![Refusal {
            rule: Rule::ProgramShape,
            message: format!("{where_}: error: no `main` function"),
            span: None,
        }]);
    }

    Ok((program, map))
}

/// Print every unit's content hash.
///
/// The program is checked first: hashing something that does not compile would
/// hand out an identity for a thing that is not a program.
/// A JSON string array, with each element escaped.
fn quoted(values: &[&str]) -> String {
    let each: Vec<String> = values.iter().map(|v| format!("\"{}\"", escaped(v))).collect();
    each.join(", ")
}

/// The two characters a JSON string may not carry raw. An effect label is
/// an identifier and a narrowing is a path or a library name, so neither
/// can contain a control character -- but escaping here rather than
/// trusting that is what keeps the output parseable if either widens.
/// A JSON string body. Quotes and backslashes are not enough: a control
/// character inside a string is invalid JSON, and a parse error's message
/// once carried the whole rendered excerpt, newlines and all, which no
/// JSON parser would read (`docs/agent-errors.md` §5).
fn escaped(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Whether a label bounds what it authorises.
///
/// Two kinds of label do not. `ffi` names a *library*, and a library is not
/// an authority domain: `Ffi("libc")` grants sockets, processes and `unlink`
/// in the same breath as `abs` (`docs/under-a-grant.md` §4). And `net_out`
/// or `net_in` with an empty argument is the *unnarrowed root* `split` hands
/// out -- every host and port, or every port -- which names no domain at all
/// (`docs/net-bounds.md` §2.4: the same state `narrow(fs, "")` is refused
/// for). Every other label names the domain it grants: `fs_read("/tmp")` is a
/// directory, `io_write` is one stream, `heap` reaches nothing else.
///
/// So the report **fails closed**. A supervisor that reads nothing but the
/// top-level `bounded` field refuses any program that reaches foreign code
/// or holds the whole network, which is the safe default for a report that
/// cannot say what that code does or where those packets go. Reading further
/// is how a supervisor decides to trust one anyway -- a decision about the
/// program, which the report must not make on its behalf.
fn bounds_its_domain(label: &str, argument: Option<&str>) -> bool {
    match label {
        "ffi" => false,
        "net_out" | "net_in" => argument.is_some_and(|bound| !bound.is_empty()),
        _ => true,
    }
}

/// The domains of the negative half (`docs/authority.md` §2.2): what a
/// capability language is *for* is the question "what can this not do",
/// and an absent label is a proof rather than an absence of evidence.
/// Both outputs — the prose report's "never touches" and the JSON's
/// `never_touches` — answer it from this one table, so the two cannot
/// drift apart.
fn untouched_domains(labels: &[String]) -> Vec<&'static str> {
    [
        // All three streams, because a program whose entire
        // output is a diagnostic touches the console —
        // `docs/standard-error.md` §4 is the lie this row prevents.
        ("the console", ["io_read", "io_write", "err_write"].as_slice()),
        ("the filesystem", ["fs_read", "fs_write", "file_read", "file_write"].as_slice()),
        ("the network", ["net_out", "net_in", "conn_accept", "conn_read", "conn_write"].as_slice()),
        ("the heap", ["heap"].as_slice()),
        ("the command line", ["args"].as_slice()),
        ("signals", ["signals", "signals_read"].as_slice()),
        // `docs/processes.md` §2: starting a program, and what a parent
        // does with the children it started.
        (
            "other programs",
            ["exec", "exec_detached", "exec_under", "child_signal", "pipe_read", "pipe_write"]
                .as_slice(),
        ),
        ("foreign code", ["ffi"].as_slice()),
    ]
    .into_iter()
    .filter(|(_, names)| {
        !names.iter().any(|name| {
            labels.iter().any(|label| label == name || label.starts_with(&format!("{name}(")))
        })
    })
    .map(|(what, _)| what)
    .collect()
}

/// `cancho authority <file>` — what a program can do
/// (`docs/authority.md`): the union of the declared rows of everything `main`
/// reaches. `Program::funcs` *is* the reachable set (`standard-library.md` §5.2),
/// and every `Func` carries its row, so this is exact in both directions (§7.3).
fn print_authority(
    inputs: &[PathBuf],
    with_std: bool,
    json: bool,
    target: Option<&Triple>,
) -> Result<(), Failure> {
    let program = compile_to_ir(inputs, with_std)?;

    // Every distinct label the reachable set performs, with the value it
    // was narrowed to. Sorted so the report is a function of the program
    // rather than of declaration order.
    let mut performed: Vec<(String, Option<String>)> = Vec::new();
    for func in &program.funcs {
        for label in func.performs.labels() {
            let entry = (label.name.clone(), label.argument.clone());
            if !performed.contains(&entry) {
                performed.push(entry);
            }
        }
    }
    performed.sort();

    let labels: Vec<String> = performed
        .iter()
        .map(|(name, argument)| match argument {
            Some(value) => format!("{name}(\"{value}\")"),
            None => name.clone(),
        })
        .collect();

    // The distinct *kinds*, which is the coarse question a grant is
    // written against -- `lex-os-check`'s `CheckReport` draws the same
    // line, keeping the arguments separately for the precise one.
    let mut kinds: Vec<&str> = performed.iter().map(|(name, _)| name.as_str()).collect();
    kinds.dedup();

    let labels_bounded =
        performed.iter().all(|(name, argument)| bounds_its_domain(name, argument.as_deref()));

    // `docs/authority.md` §3: `Program::externs` lists every `extern fn`
    // declared in the compiled unit whether anything calls it or not,
    // unlike `Program::funcs`, which pass 2 already pruned to what `main`
    // reaches -- so the report is filtered to the same reachable set,
    // rather than trusting `program.externs`'s own length.
    let reachable = cancho_ir::reachable_externs(&program);
    let mut symbols: Vec<&str> = program
        .externs
        .iter()
        .enumerate()
        .filter(|(index, _)| reachable.contains(&(*index as u32)))
        .map(|(_, declared)| declared.symbol.as_str())
        .collect();
    symbols.sort_unstable();
    symbols.dedup();

    // `docs/foreign-authority.md` section 5: the same reachable externs with
    // the scope each declaration claims -- the library in the one `ffi` label
    // of its row, which the checker requires it to have -- so the report
    // can say exactly what its unboundedness is made of.
    let reach: Vec<ForeignReach> = program
        .externs
        .iter()
        .enumerate()
        .filter(|(index, _)| reachable.contains(&(*index as u32)))
        .map(|(_, declared)| ForeignReach {
            scope: declared
                .effects
                .labels()
                .iter()
                .find(|label| label.name == "ffi")
                .and_then(|label| label.argument.clone())
                .unwrap_or_default(),
            symbol: declared.symbol.clone(),
        })
        .collect();
    let ffi_performed = performed.iter().any(|(name, _)| name == "ffi");
    let Attribution { bounded: foreign_bounded, mut unbounded_by } =
        foreign_report::attribute(ffi_performed, &reach);
    // The unnarrowed network, named in the same list as the foreign symbols so
    // `bounded: false` is always explained there (`docs/net-bounds.md` §2.4).
    let open_network: Vec<String> = performed
        .iter()
        .filter(|(name, argument)| {
            matches!(name.as_str(), "net_out" | "net_in")
                && !bounds_its_domain(name, argument.as_deref())
        })
        .map(|(name, _)| format!("{name}(\"\")"))
        .collect();
    unbounded_by.extend(open_network.iter().cloned());
    let bounded = labels_bounded && foreign_bounded;

    // The functions the checker can prove are pure (`docs/purity.md` §2).
    // Reported because it is a fact about the program that no other
    // language here can state: C's `__attribute__((const))` is an
    // unchecked promise and Rust has no way to say it at all. What it is
    // *for* is a backend that can use it, which Cranelift cannot (§4).
    let mut pure: Vec<&str> =
        program.funcs.iter().filter(|f| f.is_pure()).map(|f| f.name.as_str()).collect();
    pure.sort_unstable();
    let total = program.funcs.len();

    // What compile-time evaluation removed (`docs/compile-time.md` §9).
    // A pass that rewrites a program's own arithmetic should be able to
    // say how much of it it rewrote; reporting it is also the only
    // portable way to *test* that it happened, since reading the
    // instructions back needs a disassembler and CI has two platforms.
    let folded_operators: usize =
        program.funcs.iter().map(|f| f.folded).sum::<usize>() + program.folded_late;
    let folded_calls = program.folded_calls;

    // `--target wasm32-wasip1` (`docs/wasm.md`, W2b): what the row licenses a WASI
    // module built from this program to import. The host's report is unchanged.
    let wasi = target
        .filter(|triple| is_wasi(triple))
        .map(|triple| (triple.to_string(), cancho_ir::wasi_imports(kinds.iter().copied())));

    let stdout = io::stdout();
    let mut out = stdout.lock();
    if json {
        let written = (|| -> io::Result<()> {
            writeln!(out, "{{")?;
            // First on purpose: the one field a consumer that reads nothing
            // else should read (`docs/under-a-grant.md` §5.1).
            writeln!(out, "  \"bounded\": {bounded},")?;
            // Second: what `bounded: false` is made of, exactly
            // (`docs/foreign-authority.md` section 5). One pair a line, so
            // a CI pin of the report shows an added symbol as one added line.
            if unbounded_by.is_empty() {
                writeln!(out, "  \"unbounded_by\": [],")?;
            } else {
                writeln!(out, "  \"unbounded_by\": [")?;
                for (i, pair) in unbounded_by.iter().enumerate() {
                    let comma = if i + 1 == unbounded_by.len() { "" } else { "," };
                    writeln!(out, "    \"{}\"{comma}", escaped(pair))?;
                }
                writeln!(out, "  ],")?;
            }
            writeln!(out, "  \"effects\": [{}],", quoted(&kinds))?;
            writeln!(out, "  \"labels\": [")?;
            for (i, (name, argument)) in performed.iter().enumerate() {
                let comma = if i + 1 == performed.len() { "" } else { "," };
                let bounds = bounds_its_domain(name, argument.as_deref());
                let argument = match argument {
                    Some(value) => format!("\"{}\"", escaped(value)),
                    None => "null".to_owned(),
                };
                // `fs_read(p)` and `fs_write(p)` are a text prefix that a link inside leads out of (#762).
                let confinement = if name == "fs_read" || name == "fs_write" {
                    ", \"confinement\": \"lexical\""
                } else {
                    ""
                };
                writeln!(
                    out,
                    "    {{ \"name\": \"{}\", \"argument\": {argument}, \"bounded\": {}{confinement} }}{comma}",
                    escaped(name),
                    bounds
                )?;
            }
            writeln!(out, "  ],")?;
            // The negative half, beside the positive one so a consumer
            // never reads silence as absence (`docs/authority.md` §2.2):
            // the same domains the prose report's "never touches" lists,
            // from the one table both share. A domain absent here is
            // performed; a domain present is proven out of reach.
            let untouched = untouched_domains(&labels);
            writeln!(out, "  \"never_touches\": [{}],", quoted(&untouched))?;
            writeln!(out, "  \"foreign_symbols\": [{}],", quoted(&symbols))?;
            writeln!(out, "  \"pure\": [{}],", quoted(&pure))?;
            writeln!(out, "  \"folded_operators\": {folded_operators},")?;
            writeln!(out, "  \"folded_calls\": {folded_calls},")?;
            match &wasi {
                None => writeln!(out, "  \"functions\": {total}")?,
                Some((triple, imports)) => {
                    writeln!(out, "  \"functions\": {total},")?;
                    writeln!(out, "  \"wasi\": {{")?;
                    writeln!(out, "    \"target\": \"{}\",", escaped(triple))?;
                    writeln!(out, "    \"required\": [{}],", quoted(&imports.required))?;
                    writeln!(out, "    \"allowed\": [{}],", quoted(&imports.allowed))?;
                    let refused: Vec<&str> = imports.refused.iter().map(String::as_str).collect();
                    let unbounded: Vec<&str> =
                        imports.unbounded.iter().map(String::as_str).collect();
                    let unknown: Vec<&str> = imports.unknown.iter().map(String::as_str).collect();
                    writeln!(out, "    \"refused\": [{}],", quoted(&refused))?;
                    writeln!(out, "    \"unbounded\": [{}],", quoted(&unbounded))?;
                    writeln!(out, "    \"unknown\": [{}]", quoted(&unknown))?;
                    writeln!(out, "  }}")?;
                }
            }
            writeln!(out, "}}")?;
            out.flush()
        })();
        return match written {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            Err(e) => Err(environment(format!("cannot write to stdout: {e}"))),
        };
    }
    let written = (|| -> io::Result<()> {
        if !foreign_bounded {
            writeln!(out, "UNBOUNDED: this program calls foreign code. The labels below bound it")?;
            writeln!(
                out,
                "everywhere except through the symbols under \"unbounded by\", and what"
            )?;
            writeln!(out, "a symbol does is the linked library's, not the language's.")?;
            writeln!(out, "See docs/foreign-authority.md.")?;
        }
        if !open_network.is_empty() {
            writeln!(out, "UNBOUNDED: this program holds the network without narrowing it. A")?;
            writeln!(out, "net_out(\"\") reaches every host and port, a net_in(\"\") every port;")?;
            writeln!(out, "narrow the capability to name the ones it may use.")?;
            writeln!(out, "See docs/net-bounds.md.")?;
        }
        if labels.is_empty() {
            writeln!(out, "performs nothing")?;
        } else {
            writeln!(out, "performs")?;
            for (label, (name, argument)) in labels.iter().zip(&performed) {
                if bounds_its_domain(name, argument.as_deref()) {
                    writeln!(out, "    {label}")?;
                } else {
                    writeln!(out, "    {label}    <- unbounded")?;
                }
            }
        }
        // The negative half, which is the one a capability language is for:
        // a reader wants to know what a program *cannot* do, and an absent
        // label is exactly that.
        let untouched: Vec<&str> = untouched_domains(&labels);
        if !untouched.is_empty() {
            writeln!(out, "never touches")?;
            for what in untouched {
                writeln!(out, "    {what}")?;
            }
        }
        if !unbounded_by.is_empty() {
            writeln!(out, "unbounded by")?;
            for pair in &unbounded_by {
                writeln!(out, "    {pair}")?;
            }
        }
        if !pure.is_empty() {
            writeln!(out, "provably pure ({} of {total})", pure.len())?;
            for name in &pure {
                writeln!(out, "    {name}")?;
            }
        }
        if folded_operators > 0 || folded_calls > 0 {
            writeln!(out, "evaluated at compile time")?;
            if folded_operators > 0 {
                writeln!(out, "    {folded_operators} operators")?;
            }
            if folded_calls > 0 {
                writeln!(out, "    {folded_calls} calls")?;
            }
        }
        if let Some((triple, imports)) = &wasi {
            writeln!(out, "a {triple} module built from this program")?;
            let line = |names: &[&str]| {
                if names.is_empty() { "nothing".to_owned() } else { names.join(" ") }
            };
            writeln!(out, "    must import {}", line(&imports.required))?;
            writeln!(out, "    may import  {}", line(&imports.allowed))?;
            if !imports.refused.is_empty() {
                writeln!(
                    out,
                    "    REFUSED on this target: {} (`check --target` refuses the program)",
                    imports.refused.join(" ")
                )?;
            }
            if !imports.unbounded.is_empty() {
                writeln!(
                    out,
                    "    UNBOUNDED by {}: what foreign code imports is the library's, not the row's",
                    imports.unbounded.join(" ")
                )?;
            }
            if !imports.unknown.is_empty() {
                writeln!(out, "    not in the table: {}", imports.unknown.join(" "))?;
            }
        }
        out.flush()
    })();

    match written {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(environment(format!("cannot write to stdout: {e}"))),
    }
}

fn print_ids(inputs: &[PathBuf], with_std: bool) -> Result<(), Failure> {
    let (ast, map) = parse_program(inputs, with_std)?;
    cancho_ir::lower(&ast).map_err(|d| refused(d.render_in(&map)))?;

    let identities = cancho_id::identify(&ast);

    // Written through a locked handle rather than `println!`, which panics on
    // a closed pipe: `cancho ids big.cho | head` is an ordinary thing to do,
    // and a backtrace is the wrong answer to it.
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let written = (|| -> io::Result<()> {
        for decl in &identities.types {
            writeln!(out, "type {:<24} {}", decl.name, decl.id)?;
        }
        for func in &identities.functions {
            writeln!(out, "sig  {:<24} {}", func.name, func.sig)?;
            writeln!(out, "body {:<24} {}", func.name, func.body)?;
        }
        out.flush()
    })();

    match written {
        Ok(()) => Ok(()),
        // The reader stopped listening. That is their business, not an error.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(environment(format!("cannot write to stdout: {e}"))),
    }
}

/// Build a program with the compiler this run names (`selfhost_build::choose`): the one written in cancho, as an executable
/// (`--compiler <exe>`, `CANCHO_COMPILER`). The Rust code generators are gone (`docs/removing-rust.md`, Phase 5), so nothing
/// else can write code: a build that names no compiler, or `rust`, or a target the compiler in cancho does not write code
/// for, is refused here with a message that says what to name.
fn build(
    inputs: &[PathBuf],
    output: &Path,
    emit: Emit,
    with_std: bool,
    codegen: Codegen<'_>,
    link_libs: &[String],
    link_paths: &[String],
) -> Result<(), Failure> {
    let compiler = selfhost_build::choose(codegen.target)?;
    let built = selfhost_build::build(selfhost_build::Job {
        compiler: &compiler,
        inputs,
        output,
        emit,
        with_std,
        target: codegen.target,
        link_libs,
        link_paths,
    })?;
    match built {
        selfhost_build::Built::Done => Ok(()),
        // The compiler in cancho says what it cannot do; there is no other compiler to hand the build to.
        selfhost_build::Built::NoCode(failure) | selfhost_build::Built::Broke(failure) => {
            Err(failure)
        }
    }
}

/// Link with the platform C toolchain.
///
/// M0 shells out to `cc` rather than driving a linker itself: the C runtime
/// provides `_start` and `putchar`, and "no C" is a much later goal than "no
/// Rust" (#1).
///
/// `link_libs`/`link_paths` are `-l`/`-L`, passed to `cc` unexamined, in the
/// order given, between the object and `-o` (`docs/foreign-linking.md` §3.1:
/// a library named before the object whose symbols it resolves is a library
/// the linker never looks at, measured against this project's own `ld`).
fn link(
    object: &Path,
    output: &Path,
    link_libs: &[String],
    link_paths: &[String],
) -> Result<(), Failure> {
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_owned());
    let status = Command::new(&cc)
        .arg(object)
        .args(link_paths.iter().map(|path| format!("-L{path}")))
        .args(link_libs.iter().map(|lib| format!("-l{lib}")))
        .arg("-o")
        .arg(output)
        .status()
        .map_err(|e| environment(format!("cannot run the linker `{cc}`: {e}")))?;
    if !status.success() {
        return Err(environment(format!("the linker `{cc}` failed with {status}")));
    }
    Ok(())
}
