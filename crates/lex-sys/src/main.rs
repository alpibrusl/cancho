//! The lex-sys bootstrap compiler CLI.
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
//! program's, so `lex-sys run p.ls` and `lex-sys build p.ls && ./p` agree.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use target_lexicon::Triple;

use foreign_report::{Attribution, ForeignReach};
use lex_sys_ir::TypeInfo;
use lex_sys_syntax::{Ast, Rule, SourceFile, SourceMap};
use lex_sys_types::{DefId, Type};

mod acli;
mod fmt_cli;
mod foreign_report;
mod project;
mod test_cli;
mod vcs_cli;
mod vcs_dir;
mod vcs_origin;

const USAGE: &str = "\
lex-sys — the bootstrap compiler for the lex-sys systems dialect

usage:
    lex-sys build <file.ls>... [-o <output>] [--emit exe|obj] [--std] [--backend cranelift|llvm] [--target <triple>] [-l <name>]... [-L <path>]...
    lex-sys check <file.ls>... [--std] [--output json] [--backend cranelift|llvm] [--target <triple>]
    lex-sys run   <file.ls>... [--std] [--backend cranelift|llvm] [--target <triple>] [-l <name>]... [-L <path>]...
    lex-sys test  <file.ls>... [--std] [--backend cranelift|llvm] [-l <name>]... [-L <path>]...
    lex-sys fmt   <file.ls|dir>... [--check]
    lex-sys ids   <file.ls>... [--std]
    lex-sys authority <file.ls>... [--std] [--output json] [--target <triple>]
    lex-sys layout    <file.ls>... [--std]
    lex-sys print <file.ls>
    lex-sys agent-guidelines
    lex-sys introspect [--output json]
    lex-sys skill [--output json] [<out-file>]
    lex-sys install [--ignore-compiler-rev]
    lex-sys add <name> <git-url> [--rev <hash> | --ref <name>] [--path <dir>] [--ignore-compiler-rev]
    lex-sys build [--bin <name>] [--ignore-compiler-rev]
    lex-sys test  [--test <name>] [--ignore-compiler-rev]
    lex-sys vcs publish [--store <dir>] [--std] [--requires <lock>[:<dep-store>]]... <file.ls>
    lex-sys vcs publish [--store <root>] [--std] --dir <dir>
    lex-sys vcs log     [--store <dir>]
    lex-sys vcs resolve [--lock <file>] [<store-dir>]
    lex-sys vcs lock (--store <dir> | --git <url> (--rev <hash> | --ref <name>) [--path <dir>]) -o <file> (--all | <name>...)
    lex-sys vcs fetch --lock <file> [--store <dir>] -o <dir>
    lex-sys --version

options:
    -o <output>     where to write the result (default: the first input's stem)
    --emit exe|obj  emit a linked executable (default) or a bare object file
    --std           make the standard library's source available
    --output json   `check` and `authority` as data rather than prose
    --backend cranelift|llvm
                    which backend generates code (default: llvm, since #127).
                    `llvm` is `lex-sys-codegen-llvm` (docs/llvm-backend.md):
                    every program this repository tracks builds on it, and
                    anything outside that is refused with an ordinary located
                    error, not a crash. `cranelift` is unaffected either way.
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

`check` also generates the code and discards it, so a program it
accepts is one `build` can compile. If the compiler itself fails, that
is a refusal with rule `internal`, at the function it failed on. See
docs/internal-errors.md.

`test` runs every `fn test_*` in the files named, one process each, and
exits 4 if any failed. A test answers 0 to pass; `std.test`'s `assert`
traps otherwise. The files must not declare `main`: the runner writes
one. See docs/testing.md.

`fmt` rewrites each file (or each `.ls` under a directory) in canonical
layout, keeping its comments, its blank lines and the way it spelled each
literal. `--check` writes nothing and exits 1 if any file would change.
A file it cannot format without moving or losing a comment is left alone
and reported, also exit 1. See docs/formatting.md.

`agent-guidelines` prints AGENTS.md, which is how to write lex-sys in
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
`.lex-sys-vcs`), gated by the same checker `check` runs. A second publish
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
distinct verified source to `-o` as `<source_hash>.ls` -- nothing is
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

/// What `build` generates code with and for: the backend, and the target
/// triple when it is not the host's (`docs/wasm.md`).
#[derive(Clone, Copy)]
struct Codegen<'a> {
    backend: Backend,
    target: Option<&'a Triple>,
}

/// Which backend generates code (`docs/llvm-backend.md` §4: purely
/// additive, so `Cranelift` is every existing invocation's behaviour
/// unchanged).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Backend {
    Cranelift,
    Llvm,
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
                "lex-sys {} (rev {}, host {})",
                env!("CARGO_PKG_VERSION"),
                project::COMPILER_REV,
                lex_sys_codegen::host_triple()
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
            let Invocation { inputs, with_std, json, backend, target, .. } =
                parse_args(&args[1..], false, false)?;
            check_program(&inputs, with_std, json, backend, target.as_ref())
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
        // `docs/vcs-publish.md`: a real `.ls` file through
        // `crates/lex-sys-vcs` for the first time.
        "vcs" => vcs_cli::cmd_vcs(&args[1..]),
        // `docs/many-files.md` §5: printing is about text, and text is
        // what a file is -- so this renders exactly one.
        "print" => {
            let Invocation { inputs, .. } = parse_args(&args[1..], false, false)?;
            let [input] = &inputs[..] else {
                return Err(usage("`print` renders one file at a time"));
            };
            let text = std::fs::read_to_string(input)
                .map_err(|e| environment(format!("cannot read `{}`: {e}", input.display())))?;
            let file = SourceFile::new(input.display().to_string(), text);
            let ast = lex_sys_syntax::parse(&file.text).map_err(|d| refused(d.render(&file)))?;
            print!("{}", lex_sys_syntax::print(&ast));
            Ok(ExitCode::SUCCESS)
        }
        // `docs/formatting.md`.
        "fmt" => fmt_cli::cmd_fmt(&args[1..]),
        // `docs/testing.md` §3.
        "test" if project::wants_project_test(&args[1..]) => project::cmd_test(&args[1..]),
        "test" => test_cli::cmd_test(&args[1..]),
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
        "layout" => {
            let Invocation { inputs, with_std, .. } = parse_args(&args[1..], false, false)?;
            print_layout(&inputs, with_std)?;
            Ok(ExitCode::SUCCESS)
        }
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
                backend,
                target,
                link_libs,
                link_paths,
                ..
            } = parse_args(&args[1..], true, true)?;
            let output = output.unwrap_or_else(|| default_output(&inputs[0], emit));
            build(
                &inputs,
                &output,
                emit,
                with_std,
                Codegen { backend, target: target.as_ref() },
                &link_libs,
                &link_paths,
            )?;
            Ok(ExitCode::SUCCESS)
        }
        "run" => {
            let Invocation { inputs, with_std, backend, target, link_libs, link_paths, .. } =
                parse_args(&args[1..], false, true)?;
            let dir = std::env::temp_dir().join(format!("lex-sys-run-{}", std::process::id()));
            std::fs::create_dir_all(&dir)
                .map_err(|e| environment(format!("cannot create `{}`: {e}", dir.display())))?;
            let exe =
                dir.join(default_output(&inputs[0], Emit::Exe).file_name().unwrap_or_default());
            let result = build(
                &inputs,
                &exe,
                Emit::Exe,
                with_std,
                Codegen { backend, target: target.as_ref() },
                &link_libs,
                &link_paths,
            )
            .and_then(|()| match target.as_ref() {
                Some(triple) if is_wasm(triple) => {
                    let runtime =
                        std::env::var("WASMTIME").unwrap_or_else(|_| "wasmtime".to_owned());
                    // A WASI module gets no directory unless it is handed one,
                    // which is the point; `WASMTIME_FLAGS` (e.g. `--dir=.`) is
                    // how a run grants one, so the grant is written where it
                    // is made.
                    let flags = std::env::var("WASMTIME_FLAGS").unwrap_or_default();
                    Command::new(&runtime)
                        .arg("run")
                        .args(flags.split_whitespace())
                        .arg(&exe)
                        .status()
                        .map_err(|e| environment(format!("cannot run `{runtime}`: {e}")))
                }
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
/// downstream repo copies `AGENT_GUIDELINES.md`, and lex-sys is a
/// different language, so it needs its own. Carried in the binary for
/// the same reason `--std` is — a reader with the compiler needs
/// nothing else, and a file on disk is a file that can be absent.
const AGENT_GUIDELINES: &str = include_str!("../../../AGENTS.md");

const STD: &[(&str, &str)] = &[
    ("<std>/bytes.ls", include_str!("../../../std/bytes.ls")),
    ("<std>/math.ls", include_str!("../../../std/math.ls")),
    ("<std>/io.ls", include_str!("../../../std/io.ls")),
    ("<std>/buffer.ls", include_str!("../../../std/buffer.ls")),
    ("<std>/option.ls", include_str!("../../../std/option.ls")),
    ("<std>/result.ls", include_str!("../../../std/result.ls")),
    ("<std>/list.ls", include_str!("../../../std/list.ls")),
    ("<std>/vec.ls", include_str!("../../../std/vec.ls")),
    ("<std>/bignum.ls", include_str!("../../../std/bignum.ls")),
    ("<std>/fmt.ls", include_str!("../../../std/fmt.ls")),
    ("<std>/fmt32.ls", include_str!("../../../std/fmt32.ls")),
    ("<std>/utf8.ls", include_str!("../../../std/utf8.ls")),
    ("<std>/flags.ls", include_str!("../../../std/flags.ls")),
    ("<std>/crypto.ls", include_str!("../../../std/crypto.ls")),
    ("<std>/field25519.ls", include_str!("../../../std/field25519.ls")),
    ("<std>/ed25519.ls", include_str!("../../../std/ed25519.ls")),
    ("<std>/x25519.ls", include_str!("../../../std/x25519.ls")),
    ("<std>/chacha20.ls", include_str!("../../../std/chacha20.ls")),
    ("<std>/aes.ls", include_str!("../../../std/aes.ls")),
    ("<std>/gcm.ls", include_str!("../../../std/gcm.ls")),
    ("<std>/hmac.ls", include_str!("../../../std/hmac.ls")),
    ("<std>/hkdf.ls", include_str!("../../../std/hkdf.ls")),
    ("<std>/bigmod.ls", include_str!("../../../std/bigmod.ls")),
    ("<std>/rsa.ls", include_str!("../../../std/rsa.ls")),
    ("<std>/ecdsa.ls", include_str!("../../../std/ecdsa.ls")),
    ("<std>/ecdh.ls", include_str!("../../../std/ecdh.ls")),
    ("<std>/test.ls", include_str!("../../../std/test.ls")),
    ("<std>/json.ls", include_str!("../../../std/json.ls")),
    ("<std>/map.ls", include_str!("../../../std/map.ls")),
    ("<std>/http.ls", include_str!("../../../std/http.ls")),
    ("<std>/route.ls", include_str!("../../../std/route.ls")),
    ("<std>/conns.ls", include_str!("../../../std/conns.ls")),
    ("<std>/signals.ls", include_str!("../../../std/signals.ls")),
    ("<std>/dirs.ls", include_str!("../../../std/dirs.ls")),
    ("<std>/process.ls", include_str!("../../../std/process.ls")),
];

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
    /// `--backend` (`docs/llvm-backend.md` §4).
    backend: Backend,
    /// `--target` (`docs/wasm.md`): the triple to generate code for, if not the host's.
    target: Option<Triple>,
    /// `-l <name>` (`docs/foreign-linking.md` §3): libraries to link
    /// beyond libc, in the order given, passed to `cc` unexamined.
    link_libs: Vec<String>,
    /// `-L <path>` (`docs/foreign-linking.md` §3): search paths for the
    /// above, same order, same treatment.
    link_paths: Vec<String>,
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
    let mut backend = Backend::Llvm;
    let mut target: Option<Triple> = None;
    let mut link_libs = Vec::new();
    let mut link_paths = Vec::new();
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
            "--backend" => {
                backend = match it.next().map(String::as_str) {
                    Some("cranelift") => Backend::Cranelift,
                    Some("llvm") => Backend::Llvm,
                    Some(other) => return Err(usage(format!("unknown backend `{other}`"))),
                    None => return Err(usage("`--backend` needs a name")),
                };
            }
            "--target" => {
                let value = it.next().ok_or_else(|| usage("`--target` needs a triple"))?;
                target = Some(
                    value
                        .parse::<Triple>()
                        .map_err(|e| usage(format!("unknown target `{value}`: {e}")))?,
                );
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
    if target.is_some() && backend == Backend::Cranelift {
        return Err(usage("`--target` needs `--backend llvm`: Cranelift emits for the host only"));
    }
    Ok(Invocation { inputs, output, emit, with_std, json, backend, target, link_libs, link_paths })
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
            diagnostic: lex_sys_syntax::Diagnostic::new(
                Rule::ProgramShape,
                "unreadable input",
                lex_sys_syntax::Span::new(0, 0),
            ),
            map: None,
        })?;
        let base = map.add(input.display().to_string(), text.clone());
        sources.push((text, base));
    }
    // The library is more files of the same program (`many-files.md` §2)
    // -- it arrives from `include_str!` rather than from the disk, and
    // nothing downstream of here can tell the difference. Named
    // `<std>/io.ls` in the map, so a diagnostic inside the library says
    // so rather than naming a path that does not exist.
    if with_std {
        for (name, text) in STD {
            let base = map.add((*name).to_owned(), (*text).to_owned());
            sources.push(((*text).to_owned(), base));
        }
    }
    for (text, base) in &sources {
        if let Err(d) = lex_sys_syntax::parse_into(&mut ast, text, *base) {
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
    diagnostic: lex_sys_syntax::Diagnostic,
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
fn compile_to_ir(inputs: &[PathBuf], with_std: bool) -> Result<lex_sys_ir::Program, Failure> {
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
/// reaches that the target cannot do (`lex_sys_ir::unsupported_on_target`).
/// Empty for the host and for anything but WASI, whose gaps are the only ones
/// this knows (`docs/wasm.md`).
fn target_refusals(program: &lex_sys_ir::Program, target: Option<&Triple>) -> Vec<Refusal> {
    let Some(triple) = target else { return Vec::new() };
    if !is_wasi(triple) {
        return Vec::new();
    }
    lex_sys_ir::unsupported_on_target(program, lex_sys_ir::Os::Wasi, &triple.to_string())
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
    backend_kind: Backend,
    target: Option<&Triple>,
) -> Result<ExitCode, Failure> {
    // `docs/internal-errors.md` §3: the backend runs here too, and its
    // object is thrown away, so a program `check` accepts is a program
    // `build` can generate code for. Before, `check` stopped after
    // lowering and answered an empty list for a program `build` refused.
    let refusals = match compile_reporting(inputs, with_std) {
        // What the target cannot do is answered first and without a code
        // generator, so it needs no wasm toolchain and says where it is.
        Ok((program, _)) if !target_refusals(&program, target).is_empty() => {
            target_refusals(&program, target)
        }
        Ok((program, _)) => match backend(&program, inputs, backend_kind, target) {
            Ok(_) => Vec::new(),
            Err(BackendFailure::Refusals(refusals)) => refusals,
            Err(BackendFailure::Environment(message)) => return Err(environment(message)),
        },
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
    span: Option<lex_sys_syntax::Span>,
}

impl Refusal {
    /// The sentence a person reads, byte for byte what it was before the
    /// rule was attached to it (§6).
    fn render(&self, map: &lex_sys_syntax::SourceMap) -> String {
        match self.span {
            Some(span) => {
                lex_sys_syntax::Diagnostic::new(self.rule, &self.message, span).render_in(map)
            }
            None => self.message.clone(),
        }
    }
}

/// Read, parse and lower, answering **every** refusal (§4).
fn compile_reporting(
    inputs: &[PathBuf],
    with_std: bool,
) -> Result<(lex_sys_ir::Program, lex_sys_syntax::SourceMap), Vec<Refusal>> {
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
    let program = match lex_sys_ir::lower_all(&ast) {
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
        if entry.ret != lex_sys_types::Type::Int {
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

/// Generate the object code, or say why the compiler could not.
///
/// `docs/internal-errors.md`. Every failure here is the compiler's: the
/// program was already accepted. So it is reported as a refusal with
/// rule `internal`, at the declaration of the function whose code
/// failed, with the backend's own words kept as the cause. A panic in
/// the backend is caught per function by `lex-sys-codegen`; the default
/// hook, which would print Rust's "thread 'main' panicked at", is
/// silenced for the duration and put back.
///
/// `docs/llvm-backend.md` §4: `--backend` is purely additive, so
/// `Backend::Cranelift` is this function's old, only behaviour, unchanged.
/// `Backend::Llvm` never panics by design (`lex-sys-codegen-llvm` returns
/// `CodegenError` for everything its first slice does not lower), but the
/// same catch applies to it too: a bug there should read as `internal`,
/// not as a crash, exactly like a bug in `lex-sys-codegen` does.
///
/// A bug is not the only way this backend fails now that it is the
/// default: `Backend::Llvm` shells out to a real `clang`, so a host
/// missing one is not lex-sys's bug, it is the environment's
/// (`CodegenError::environment`). `BackendFailure` keeps that apart from
/// an ordinary refusal so a caller reports exit 3, not 1.
fn backend(
    program: &lex_sys_ir::Program,
    inputs: &[PathBuf],
    backend_kind: Backend,
    target: Option<&Triple>,
) -> Result<Vec<u8>, BackendFailure> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = match backend_kind {
        Backend::Cranelift => lex_sys_codegen::compile_object(program, "main"),
        Backend::Llvm => match target {
            Some(triple) => {
                lex_sys_codegen_llvm::compile_object_for(program, "main", triple.clone())
            }
            None => lex_sys_codegen_llvm::compile_object(program, "main"),
        },
    };
    std::panic::set_hook(hook);
    result.map_err(|e| {
        if e.environment {
            BackendFailure::Environment(e.message.clone())
        } else {
            BackendFailure::Refusals(vec![internal_refusal(program, &e, inputs)])
        }
    })
}

enum BackendFailure {
    /// The compiler's own bug, reported as it always was: rule
    /// `internal`, exit 1, located at the function that failed.
    Refusals(Vec<Refusal>),
    /// The host's, not lex-sys's: `--backend llvm` could not reach a
    /// real `clang`. Exit 3 (`docs/llvm-backend.md`).
    Environment(String),
}

fn internal_refusal(
    program: &lex_sys_ir::Program,
    e: &lex_sys_codegen::CodegenError,
    inputs: &[PathBuf],
) -> Refusal {
    match e.function.and_then(|index| program.funcs.get(index)) {
        Some(func) => Refusal {
            rule: Rule::Internal,
            message: format!(
                "the compiler failed to generate code for `{}`; this is a bug in lex-sys, not in \
                 the program ({})",
                func.name, e.message
            ),
            span: Some(func.span),
        },
        // Nothing to point at, and nothing is invented (§2).
        None => Refusal {
            rule: Rule::Internal,
            message: format!(
                "{}: error: the compiler failed to generate code; this is a bug in lex-sys, not \
                 in the program ({})",
                inputs.first().map(|p| p.display().to_string()).unwrap_or_default(),
                e.message
            ),
            span: None,
        },
    }
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

/// `lex-sys authority <file>` — what a program can do
/// (`docs/authority.md`).
///
/// The surface is the union of the declared rows of everything `main`
/// reaches, and that needs no new analysis: pass 2 already emits exactly
/// what `main` reaches (`standard-library.md` §5.2), so `Program::funcs`
/// *is* the reachable set and every `Func` carries its row. The same
/// reachability that decides what goes in the binary decides what the
/// binary can do, which is the reason these are one number rather than
/// two.
///
/// Rows are exact in both directions (§7.3), so this is precise rather
/// than conservative: a label here is an effect the program performs on
/// some path, not one it might.
/// `lex-sys layout` — what each declared type costs in memory
/// (`docs/layout.md` §4).
///
/// A report rather than a flag, because there is nothing to turn on: §2
/// of that document measured the layout and deliberately did not change
/// it. What the report is *for* is the trigger — when `size` and
/// `packed` differ on a program someone cares about, the deferral stops
/// being right, and this is how that gets noticed rather than
/// remembered.
fn print_layout(inputs: &[PathBuf], with_std: bool) -> Result<(), Failure> {
    let program = compile_to_ir(inputs, with_std)?;
    let triple = lex_sys_codegen::host_triple();

    // The prelude's eight -- `World`, the five capabilities, `Box` and
    // `Split` -- are not types the program declared, and a report about
    // a program's memory should not open with eight rows of zero.
    const PRELUDE: usize = lex_sys_ir::PRELUDE_SPLIT + 1;

    let mut rows: Vec<(String, lex_sys_codegen::Layout)> = Vec::new();
    for (index, info) in program.types.iter().enumerate().skip(PRELUDE) {
        let (name, members): (&String, Vec<&Type>) = match info {
            TypeInfo::Struct { name, fields } => (name, fields.iter().map(|(_, t)| t).collect()),
            TypeInfo::Enum { name, variants } => {
                (name, variants.iter().flat_map(|(_, p)| p.iter()).collect())
            }
        };
        // A generic declaration has no one layout -- `Pair[int, bool]`
        // and `Pair[bool, int]` are two -- so it is skipped rather than
        // measured at a substitution nobody wrote. Reporting each
        // instantiation instead would mean reporting names the source
        // does not contain, which is a worse answer than saying nothing.
        if members.iter().any(|t| mentions_parameter(t)) {
            continue;
        }
        let ty = Type::Named(DefId(index as u32), Vec::new());
        rows.push((name.clone(), lex_sys_codegen::layout_of(&ty, &program, &triple)));
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let written = (|| -> io::Result<()> {
        writeln!(
            out,
            "{:<24} {:>6} {:>7} {:>8} {:>8}",
            "type", "leaves", "size", "packed", "stride"
        )?;
        for (name, layout) in &rows {
            writeln!(
                out,
                "{:<24} {:>6} {:>7} {:>8} {:>8}",
                name, layout.leaves, layout.size, layout.packed, layout.stride
            )?;
        }
        // The one line that is the point of the report.
        let savings: u32 = rows.iter().map(|(_, l)| l.size.saturating_sub(l.packed)).sum();
        if savings > 0 {
            writeln!(
                out,
                "\npacking would save {savings} bytes per copy across these types (`docs/layout.md` §2)"
            )?;
        }
        out.flush()
    })();

    match written {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(environment(format!("cannot write to stdout: {e}"))),
    }
}

/// Does this type mention a generic parameter, at any depth?
fn mentions_parameter(ty: &Type) -> bool {
    match ty {
        Type::Param(_) => true,
        Type::Named(_, args) => args.iter().any(mentions_parameter),
        Type::Slice(inner) => mentions_parameter(inner),
        Type::Ref { inner, .. } => mentions_parameter(inner),
        Type::Tuple(parts) => parts.iter().any(mentions_parameter),
        _ => false,
    }
}

/// Whether a label's name bounds what it authorises.
///
/// Every label but one names the domain it grants: `fs_read("/tmp")` is a
/// directory, `io_write` is one stream, `heap` reaches nothing else. The
/// exception is `ffi`, whose argument names a *library* -- and a library
/// is not an authority domain: `Ffi("libc")` grants sockets, processes and
/// `unlink` in the same breath as `abs` (`docs/under-a-grant.md` §4).
///
/// So the report **fails closed**. A supervisor that reads nothing but the
/// top-level `bounded` field refuses any program that reaches foreign
/// code, which is the safe default for a report that cannot say what that
/// code does. Reading further is how a supervisor decides to trust one
/// anyway -- a decision about the program, which the report must not make
/// on its behalf.
fn bounds_its_domain(label: &str) -> bool {
    label != "ffi"
}

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

    let labels_bounded = performed.iter().all(|(name, _)| bounds_its_domain(name));

    // `docs/authority.md` §3: `Program::externs` lists every `extern fn`
    // declared in the compiled unit whether anything calls it or not,
    // unlike `Program::funcs`, which pass 2 already pruned to what `main`
    // reaches -- so the report is filtered to the same reachable set,
    // rather than trusting `program.externs`'s own length.
    let reachable = lex_sys_ir::reachable_externs(&program);
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
    let Attribution { bounded: foreign_bounded, unbounded_by } =
        foreign_report::attribute(ffi_performed, &reach);
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
        .map(|triple| (triple.to_string(), lex_sys_ir::wasi_imports(kinds.iter().copied())));

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
                let argument = match argument {
                    Some(value) => format!("\"{}\"", escaped(value)),
                    None => "null".to_owned(),
                };
                writeln!(
                    out,
                    "    {{ \"name\": \"{}\", \"argument\": {argument}, \"bounded\": {} }}{comma}",
                    escaped(name),
                    bounds_its_domain(name)
                )?;
            }
            writeln!(out, "  ],")?;
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
        if !bounded {
            writeln!(out, "UNBOUNDED: this program calls foreign code. The labels below bound it")?;
            writeln!(
                out,
                "everywhere except through the symbols under \"unbounded by\", and what"
            )?;
            writeln!(out, "a symbol does is the linked library's, not the language's.")?;
            writeln!(out, "See docs/foreign-authority.md.")?;
        }
        if labels.is_empty() {
            writeln!(out, "performs nothing")?;
        } else {
            writeln!(out, "performs")?;
            for (label, (name, _)) in labels.iter().zip(&performed) {
                if bounds_its_domain(name) {
                    writeln!(out, "    {label}")?;
                } else {
                    writeln!(out, "    {label}    <- unbounded")?;
                }
            }
        }
        // The negative half, which is the one a capability language is for:
        // a reader wants to know what a program *cannot* do, and an absent
        // label is exactly that.
        let untouched: Vec<&str> = [
            // All three streams, because a program whose entire
            // output is a diagnostic touches the console —
            // `docs/standard-error.md` §4 is the lie this row prevents.
            ("the console", ["io_read", "io_write", "err_write"].as_slice()),
            ("the filesystem", ["fs_read", "fs_write", "file_read", "file_write"].as_slice()),
            (
                "the network",
                ["net_out", "net_in", "conn_accept", "conn_read", "conn_write"].as_slice(),
            ),
            ("the heap", ["heap"].as_slice()),
            ("the command line", ["args"].as_slice()),
            ("signals", ["signals", "signals_read"].as_slice()),
            // `docs/processes.md` §2: starting a program, and what a parent
            // does with the children it started.
            ("other programs", ["exec", "child_signal", "pipe_read", "pipe_write"].as_slice()),
            ("foreign code", ["ffi"].as_slice()),
        ]
        .into_iter()
        .filter(|(_, names)| {
            !names.iter().any(|name| {
                labels.iter().any(|label| label == name || label.starts_with(&format!("{name}(")))
            })
        })
        .map(|(what, _)| what)
        .collect();
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
    lex_sys_ir::lower(&ast).map_err(|d| refused(d.render_in(&map)))?;

    let identities = lex_sys_id::identify(&ast);

    // Written through a locked handle rather than `println!`, which panics on
    // a closed pipe: `lex-sys ids big.ls | head` is an ordinary thing to do,
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

fn build(
    inputs: &[PathBuf],
    output: &Path,
    emit: Emit,
    with_std: bool,
    codegen: Codegen<'_>,
    link_libs: &[String],
    link_paths: &[String],
) -> Result<(), Failure> {
    let Codegen { backend: backend_kind, target } = codegen;
    let program = compile_to_ir(inputs, with_std)?;
    let gaps = target_refusals(&program, target);
    if !gaps.is_empty() {
        let text: Vec<String> = match parse_program(inputs, with_std) {
            Ok((_, map)) => gaps.iter().map(|r| r.render(&map)).collect(),
            Err(_) => gaps.iter().map(|r| r.message.clone()).collect(),
        };
        return Err(refused(text.join("\n\n")));
    }
    // A compiler bug in the backend is a refusal with rule `internal`,
    // exit 1, located at the function (`docs/internal-errors.md` §2).
    // Since `--backend llvm` became the default, a *host* missing
    // `clang` is not that -- it is exit 3, the environment, same as any
    // other missing tool.
    let object =
        backend(&program, inputs, backend_kind, target).map_err(|failure| match failure {
            BackendFailure::Refusals(refusals) => {
                let text: Vec<String> = match parse_program(inputs, with_std) {
                    Ok((_, map)) => refusals.iter().map(|r| r.render(&map)).collect(),
                    Err(_) => refusals.iter().map(|r| r.message.clone()).collect(),
                };
                refused(text.join("\n\n"))
            }
            BackendFailure::Environment(message) => environment(message),
        })?;

    match emit {
        Emit::Obj => std::fs::write(output, &object)
            .map_err(|e| environment(format!("cannot write `{}`: {e}", output.display()))),
        Emit::Exe => {
            let object_path = output.with_extension("o");
            std::fs::write(&object_path, &object).map_err(|e| {
                environment(format!("cannot write `{}`: {e}", object_path.display()))
            })?;
            let result = match target {
                Some(triple) if is_wasm(triple) => link_wasm(&object_path, output, triple),
                _ => link(&object_path, output, link_libs, link_paths),
            };
            let _ = std::fs::remove_file(&object_path);
            result
        }
    }
}

/// Is this triple one of WebAssembly's (`docs/wasm.md`)?
fn is_wasm(triple: &Triple) -> bool {
    matches!(
        triple.architecture,
        target_lexicon::Architecture::Wasm32 | target_lexicon::Architecture::Wasm64
    )
}

/// Link a WebAssembly command with `wasm-ld` against a wasi-libc sysroot.
///
/// No C compiler driver in the way: the object and `-lc`. **Not `crt1-command.o`**: the
/// module defines `_start` itself (`wasi_entry`), so that the command line is fetched only
/// by a program that reads it (`docs/wasm.md`, W2c).
/// `WASI_SYSROOT` names the sysroot (the one that holds `lib/<triple>/`),
/// `WASM_LD` the linker. A missing one is the environment's fault, exit 3,
/// and the message says which variable to set.
fn link_wasm(object: &Path, output: &Path, triple: &Triple) -> Result<(), Failure> {
    let sysroot = std::env::var("WASI_SYSROOT").map_err(|_| {
        environment(format!(
            "`--target {triple}` needs `WASI_SYSROOT`, a wasi-libc sysroot (the directory that \
             holds `lib/{triple}/libc.a`)"
        ))
    })?;
    let lib = Path::new(&sysroot).join("lib").join(triple.to_string());
    let ld = std::env::var("WASM_LD").unwrap_or_else(|_| "wasm-ld".to_owned());
    let status = Command::new(&ld)
        // A call whose type disagrees with the definition is, to `wasm-ld`,
        // a warning that swaps in a trap: the program links and dies at run
        // time. Fatal, so it is a build failure here instead.
        .arg("--fatal-warnings")
        .arg(object)
        .arg(format!("-L{}", lib.display()))
        .arg("-lc")
        .arg("-o")
        .arg(output)
        .status()
        .map_err(|e| environment(format!("cannot run the linker `{ld}`: {e}")))?;
    if !status.success() {
        return Err(environment(format!("the linker `{ld}` failed with {status}")));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A checked program whose function `seven` is then broken in the IR,
    /// the way `docs/internal-errors.md` §5 describes: the body changes and
    /// the signature does not, so the failure is `seven`'s own.
    fn broken(body: lex_sys_ir::Stmt) -> (lex_sys_ir::Program, lex_sys_syntax::Span) {
        let source = "fn seven() -> [] int { return 7; }\n\
                      fn main(world: World) -> [] int { release(world); return seven() - 7; }\n";
        let ast = lex_sys_syntax::parse(source).expect("should parse");
        let mut program = lex_sys_ir::lower(&ast).expect("should lower");
        let seven = program.funcs.iter_mut().find(|f| f.name == "seven").expect("`seven`");
        seven.body = vec![body];
        let span = seven.span;
        (program, span)
    }

    fn refusal_for(body: lex_sys_ir::Stmt) -> (Refusal, lex_sys_syntax::Span) {
        let (program, span) = broken(body);
        let failure = backend(&program, &[PathBuf::from("seven.ls")], Backend::Cranelift, None)
            .expect_err("the backend should refuse");
        let BackendFailure::Refusals(mut refusals) = failure else {
            panic!("Cranelift has no environment failure mode; expected a refusal");
        };
        assert_eq!(refusals.len(), 1, "code generation stops at the first failure");
        (refusals.remove(0), span)
    }

    /// §2: a verifier failure is an `internal` refusal, located at the
    /// declaration of the function whose code failed, saying whose bug it
    /// is and keeping Cranelift's own words.
    #[test]
    fn a_backend_failure_is_a_located_internal_refusal() {
        let (refusal, span) = refusal_for(lex_sys_ir::Stmt::Return(lex_sys_ir::Expr::Bool(true)));
        assert_eq!(refusal.rule, Rule::Internal);
        assert_eq!(refusal.span, Some(span), "it points at `seven`'s declaration");
        assert!(refusal.message.contains("`seven`"), "{}", refusal.message);
        assert!(
            refusal.message.contains("a bug in lex-sys, not in the program"),
            "{}",
            refusal.message
        );
        assert!(refusal.message.contains("Verifier"), "{}", refusal.message);
    }

    /// §4: a panic in the backend is the same refusal, carrying the
    /// panic's message, and the compiler does not unwind out of `backend`.
    #[test]
    fn a_backend_panic_is_a_located_internal_refusal() {
        let (refusal, span) = refusal_for(lex_sys_ir::Stmt::Return(lex_sys_ir::Expr::Call {
            callee: lex_sys_ir::Callee::Builtin(lex_sys_ir::Builtin::Len),
            args: vec![lex_sys_ir::Expr::Int(0)],
        }));
        assert_eq!(refusal.rule, Rule::Internal);
        assert_eq!(refusal.span, Some(span));
        assert!(refusal.message.contains("`len` is lowered as `Expr::Len`"), "{}", refusal.message);
    }

    /// The rendered form is an ordinary located diagnostic, like every
    /// other refusal: file, line and column, then the source line.
    #[test]
    fn an_internal_refusal_renders_where_the_function_is() {
        let source = "fn seven() -> [] int { return 7; }\n\
                      fn main(world: World) -> [] int { release(world); return seven() - 7; }\n";
        let mut map = lex_sys_syntax::SourceMap::new();
        assert_eq!(map.add("seven.ls", source), 0, "one file, based at zero like the parse");
        let (refusal, _) = refusal_for(lex_sys_ir::Stmt::Return(lex_sys_ir::Expr::Bool(true)));
        let text = refusal.render(&map);
        assert!(text.starts_with("seven.ls:1:1: error: the compiler failed"), "{text}");
    }
}
