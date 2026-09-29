# lex-sys

A **native systems language in which resource ownership and authority are
part of the program's type-level contract.** Linear ownership and
capability-typed effects are one system rather than two; behaviour is
fully defined, with no UB; and the AST is canonical and
content-addressable, designed in from day one rather than retrofitted.

> **Status.** M0–M3 complete, and post-milestone work ships one slice at a
> time. A bootstrap compiler takes a `.ls` file to a real native
> executable, green on **linux-x86_64 and darwin-aarch64**. It is
> **not a usable language yet** — [`docs/ROADMAP.md`](docs/ROADMAP.md)
> tracks what landed, what is next, and what each slice found.

## Learn more

**[`docs/README.md`](docs/README.md)** is where the language, the
ecosystem it sits in, what exists today, and the full design-document
index all live. Start there.

## Quick start

```sh
cargo run -p lex-sys -- run examples/tour.ls
```

[`examples/`](examples/README.md) is a guided index: what each program is,
what it prints, and which one to read for which idea. Every example declares
its own output in its header, and a test walks the directory and checks
them, so an example that stops matching the language fails CI rather than
quietly rotting.

### The compiler's whole surface

```sh
lex-sys build <file.ls>... [-o <output>] [--emit exe|obj] [--std] [--backend cranelift|llvm]
lex-sys check <file.ls>... [--std] [--output json] [--backend cranelift|llvm]   # refuse, or say nothing
lex-sys run   <file.ls>... [--std] [--backend cranelift|llvm]   # build, run, exit with the program's status
lex-sys ids   <file.ls>... [--std]    # each declaration's content hash
lex-sys authority <file.ls>... [--std] [--output json]  # what it can reach
lex-sys layout    <file.ls>... [--std]  # what every leaf costs, and what packing would save
lex-sys print <file.ls>               # the unit, rendered in canonical form
lex-sys agent-guidelines              # AGENTS.md, from inside the binary
lex-sys vcs publish [--store <dir>] <file.ls>  # log every declaration as an operation
lex-sys vcs log     [--store <dir>]            # what a store already has
```

This block is checked against `--help` in both directions by
`the_readme_commands_still_work`, so it cannot drift silently.

`authority` is the one worth trying on something you did not write:

```sh
$ lex-sys authority examples/tally.ls --std
performs
    io_read
    io_write
never touches
    the filesystem
    the heap
    the command line
    foreign code
```

The surface is the union of what everything `main` reaches performs, so
it is precise rather than conservative — rows are exact in both
directions. `--output json` gives the same report as data, and it
**fails closed**: its first field is `"bounded"`, `false` for any program
that reaches foreign code. [`docs/authority.md`](docs/authority.md),
[`docs/under-a-grant.md`](docs/under-a-grant.md).

A program is the **set of files named on the command line**, in any order.
Exit codes are semantic: `0` success, `1` refused with a located
diagnostic, `2` the command line was wrong, `3` the environment failed.
`--std` makes the standard library's source available; it is never a
prelude, so a program still writes `import std.io;` where it uses one.

### The gate

```sh
cargo test                                 # units, examples, and the conformance suite
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) — the gate, how a slice is done, the
file budget, and code conventions. [`AGENTS.md`](AGENTS.md) is the one
page for writing lex-sys *programs*, not the compiler: the rules, the six
things that cost this repository a compile each, and what the language
does not have. `lex-sys agent-guidelines` prints it, and every checked
code block in it is run by the test suite.

Design lands in `docs/` **before** the code that implements it, and a
claim that turns out wrong is corrected in place there rather than
quietly edited — [`docs/ROADMAP.md`](docs/ROADMAP.md) says which.

## Licence

[EUPL-1.2](LICENSE), matching the rest of the ecosystem. See `LICENSE` for
the notice and where to obtain the full text in any of the 23 EU languages.
