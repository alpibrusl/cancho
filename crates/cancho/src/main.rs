//! The Rust remainder of the cancho CLI (`docs/removing-rust.md`, Phase 5).
//!
//! It lexes, parses, checks and writes no code any more: every command that read a program is answered by the compiler
//! written in cancho (`scripts/build-standalone.sh`), and so is `vcs` (its crate is deleted). What is left is `agent-guidelines`,
//! `introspect`, `skill` and `--version`, until this crate goes too.
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
use std::process::ExitCode;
use target_lexicon::Triple;

mod acli;
mod vcs_cli;

const USAGE: &str = "\
cancho — the Rust remainder of the cancho CLI

usage:
    cancho agent-guidelines
    cancho introspect [--output json]
    cancho skill [--output json] [<out-file>]
    cancho --version

Every command that reads a program (`build`, `run`, `test`, `check`, `fmt`, `print`, `ingest`, `ids`, `authority`,
`satisfy`, `layout`, `install`, `add`, `vcs`) is answered by the compiler written in cancho (`scripts/build-standalone.sh`);
this binary refuses them (exit 2). See docs/removing-rust.md.

`agent-guidelines` prints AGENTS.md. `introspect` prints the command tree as JSON (ACLI spec §1.2) and `skill` a
generated agentskills.io SKILL.md. See docs/agent-cli.md.
";

const EXIT_USAGE: u8 = 2;
const EXIT_ENVIRONMENT: u8 = 3;

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

fn usage(message: impl Into<String>) -> Failure {
    Failure { message: format!("{}\n\n{USAGE}", message.into()), code: EXIT_USAGE }
}

fn environment(message: impl Into<String>) -> Failure {
    Failure { message: message.into(), code: EXIT_ENVIRONMENT }
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
                COMPILER_REV,
                Triple::host()
            );
            Ok(ExitCode::SUCCESS)
        }
        "--help" | "-h" | "help" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        // ACLI built-ins (docs/agent-cli.md): the command tree and a generated skill file.
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
        // `docs/vcs-publish.md`.
        "vcs" => vcs_cli::cmd_vcs(&args[1..]),
        "build" | "run" | "test" | "check" | "authority" | "ids" | "satisfy" | "fmt" | "print"
        | "ingest" | "layout" | "install" | "add" => Err(retired(command)),
        other => Err(usage(format!("unknown command `{other}`"))),
    }
}

/// `docs/agent-errors.md` §7: AGENTS.md, compiled into the binary.
const AGENT_GUIDELINES: &str = include_str!("../../../AGENTS.md");

/// The commands that read a program. Their front end (`cancho-syntax`, `cancho-ir`) and code generators are deleted
/// (`docs/removing-rust.md`, Phase 5); the compiler in cancho answers all of them. This binary says so rather than answer
/// from a front end it no longer has. Not a refusal of a program, so no rule tag.
fn retired(command: &str) -> Failure {
    Failure {
        message: format!(
            "`cancho {command}` is no longer answered by the Rust binary: it has no front end and no repository store (`docs/removing-rust.md`, Phase 5); use the compiler in cancho (`scripts/build-standalone.sh`)"
        ),
        code: EXIT_USAGE,
    }
}

/// The commit this compiler was built from (`build.rs`).
const COMPILER_REV: &str = env!("CANCHO_REV_STAMP");
