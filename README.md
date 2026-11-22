<p align="center"><img src="docs/assets/cancho-logo-256.png" alt="cancho" width="200"></p>

# cancho

A **native systems language in which resource ownership and authority are
part of the program's type-level contract.** Ownership and capability-typed
effects are one system rather than two: a value is consumed exactly once,
and reaching the outside world — the console, a file, a socket, a foreign
library — requires holding a capability for it, checked at compile time.

**Version 0.1.0.** The compiler is written in cancho. It is built from a
committed seed of LLVM IR with `clang` alone, reproduces itself byte for
byte. It writes
code for Linux (x86-64 and aarch64) and macOS (Apple silicon).

## Install

A release is a `cancho` built by cancho, one file of about 1.2 MB, checked
against its published sha256 and against the commit it reports:

```sh
curl -fsSL https://raw.githubusercontent.com/alpibrusl/cancho/main/scripts/install.sh | sh
cancho --version
```

That installs the latest release into `~/.local/bin`. `install.sh <commit>
[prefix]` installs the one a project pins. Building a program needs `clang`
15 or newer and `cc`; on a machine whose `clang` is older or not the first
on `PATH`, name one with `cancho build … --clang /absolute/path/to/clang`
(this build does not read `$CLANG`). The sha256 detects a damaged or
mismatched download; it does not authenticate who published it, since the
script and the archive both come from GitHub.

What this build does not have: Cranelift (`--backend cranelift`) and
WebAssembly (`--target wasm32-…`); it writes code with LLVM, for the
machine it runs on ([`docs/standalone-cancho.md`](docs/standalone-cancho.md)).

## An example

```
import std.io;

fn greet[&i, &r](io: &!i Io, name: &r [byte]) -> [io_write] int {
    io.write_all(io, "Hello, ");
    return io.write_all(io, name);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi); release(fs); release(heap); release(args);

    borrow mut io as &!i in {
        greet(i, "cancho");
    }
    release(io);
    return 0;
}
```

`split(world)` is the only place a program gets authority from, and it
gets all of it, once. `release` is not cleanup — an unused capability is
still a linear value, so forgetting to release one does not compile.
`greet`'s signature says `[io_write]` because it was handed a borrowed
`&!i Io`; a function nothing was given cannot print however much it
wants to, and `main`'s own row is `[]` even though it prints, because it
*owns* the capability rather than borrowing one. Save it as `greet.cho` and
run it:

```sh
cancho run greet.cho --std      # Hello, cancho
```

There is a longer tour in [`examples/tour.cho`](examples/tour.cho), and
`cancho agent-guidelines` prints [`AGENTS.md`](AGENTS.md), the one page for
writing cancho in a hurry.

## What it is for

A program that must be shown to do only what it is allowed to do, by
something other than its author's say-so:

- **`cancho authority`** says what a program can reach — the union of what
  everything `main` reaches performs, from the same reachability that
  decides what goes in the binary, so it is precise rather than
  conservative. `--output json` gives the report as data and **fails
  closed**: a program that reaches foreign code is `"bounded": false` and
  names the symbols
  ([`docs/authority.md`](docs/authority.md),
  [`docs/foreign-authority.md`](docs/foreign-authority.md)).
- **`cancho ids`** gives every declaration a content hash that does not
  move when it is renamed or reformatted, and `cancho vcs` keeps a
  content-addressed log of them; a dependency is a hash
  ([`docs/canonical-ast.md`](docs/canonical-ast.md),
  [`docs/package-system.md`](docs/package-system.md)).
- **`cancho check --output json`** answers every refusal as data: a stable
  rule tag, a position, a sentence. A refusal is a result an agent can act
  on, not prose to scrape ([`docs/agent-errors.md`](docs/agent-errors.md)).
- **`cancho satisfy`** is the spec-to-implementation loop: a contract is a
  file whose `fn test_*` call a candidate, and the verdict names the
  candidate's identity ([`docs/satisfy.md`](docs/satisfy.md)).

## What's deliberately not here

- **No garbage collector, no destructors, no exceptions.** A resource is
  destroyed by naming the function that knows how; a `res` value must be
  consumed exactly once on every path, checked statically.
- **No borrow checker.** Borrowing is lexical — a region is a block, so a
  reference's validity is read off the syntax rather than inferred. No
  non-lexical lifetimes, no variance.
- **No implicit anything.** No implicit conversions, no operator
  overloading resolved by inference, no ambient authority: `Io {}` does
  not compile, and the only capability a program ever has is the one
  `main` was handed.
- **No undefined behaviour.** Checked arithmetic that traps, left-to-right
  evaluation, deterministic layout: every place C leaves behaviour open is
  named and closed ([`docs/defined-behaviour.md`](docs/defined-behaviour.md)).
- **No textual or proc macros.** They break stable, content-addressable
  identity — a design commitment kept from the day this project started,
  not added on later.

## Where it stands

Everything below was measured on 2026-11-22, on `main`, unless a document
is cited.

- **The compiler** is [`compiler/`](compiler/): 143 files, about 70,900
  lines of cancho (its own tests included), and about 16,500 lines of LLVM
  IR text in [`compiler/runtime/`](compiler/runtime). The standard library
  is [`std/`](std/), 47 files, about 23,100 lines, also cancho.
- **Optional comparisons.** The Rust halves of two benchmarks and the
  `rustls` peers of the TLS interop scripts skip when `rustc` or `cargo`
  is missing; nothing else needs them.
- **It reproduces itself.** `clang` builds the committed seed
  ([`seed/`](seed/), [`docs/seed.md`](docs/seed.md)); the seed builds the
  compiler; that compiler builds itself twice more and the last two must be
  identical. The release workflow does this on all three targets.
- **The gate** is [`scripts/gate.sh`](scripts/gate.sh), written in cancho
  and shell: formatting, a file-size budget, a link check, every fixture of
  [`tests/accept`](tests/accept) built and run, about 400 fixtures of
  [`tests/reject`](tests/reject) each refused by the rule it names, and the
  compiler's own tests. Its design is [`docs/gate.md`](docs/gate.md).
- **Packages**: [`packages/`](packages/) holds eleven, among them a TLS 1.3
  client and server (written in cancho, with no C library, and **not
  independently reviewed**: [`docs/tls-server.md`](docs/tls-server.md)),
  X.509, secp256k1, an HTTP client and a server, and the sockets they stand
  on. Dependencies resolve end to end by hash, a
  dependency's own dependencies included
  ([`docs/package-system.md`](docs/package-system.md)).
- **History still reads.** Replaying every historical revision of `std/`,
  `examples/` and `compiler/` through today's compiler: 983 revisions, 964
  read, 16 do not, 3 cannot be judged by it. Each failure is listed with a
  reason in [`docs/stability-exemptions.md`](docs/stability-exemptions.md),
  and the CI job fails if a rule starts failing more revisions than its
  recorded count ([`docs/hash-stability.md`](docs/hash-stability.md) §2).
  Features arrive by *edition*, additively, so a file that does not opt in
  cannot be broken by one.

And honestly, **pre-1.0**: the language is still growing (the roadmap says
what landed, what is next and what each slice found out:
[`docs/ROADMAP.md`](https://alpibrusl.github.io/cancho/ROADMAP.html)), there
is no human-readable version string for a dependency — a hash is the only
thing actually depended on — and the compiler writes code for two operating
systems and two architectures, not more. Release `0.1.0` is the one this page
describes.

## Building

Using cancho does not need this section (see Install). To work on the
compiler you need `clang` 15 or newer and nothing else:

```sh
git clone https://github.com/alpibrusl/cancho
cd cancho
scripts/gate.sh   # the whole gate: builds the compiler from the seed, then every check
```

The `cancho` of a release is built by `scripts/build-standalone.sh`: `clang`
builds the committed seed, which compiles the command line written in
cancho ([`compiler/cli.cho`](compiler/cli.cho)), which compiles itself twice
more and must reproduce itself byte for byte
([`docs/standalone-cancho.md`](docs/standalone-cancho.md)).

A project that pins a compiler (`cancho.toml`) installs the prebuilt one for
that commit instead of building it:

```sh
scripts/install.sh <commit> [prefix]   # sha256 and the reported commit are both checked
```

It still needs `clang` and `cc` at build time
([`docs/package-system.md`](docs/package-system.md) §9).

### The compiler's whole surface

```sh
cancho build <file.cho>... [-o <output>] [--emit exe|obj] [--std] [-l <name>]... [-L <path>]... [--clang <path>]
cancho check <file.cho>... [--std] [--output json]   # refuse, or say nothing
cancho run   <file.cho>... [--std] [-l <name>]... [-L <path>]...   # build, run, exit with the program's status
cancho test  <file.cho>... [--std] [-l <name>]... [-L <path>]...   # run every `fn test_*`, one process each; exit 4 if one failed
cancho satisfy <contract.cho> <candidate.cho> [--std]   # a contract's tests call the candidate; the verdict names the SigId
cancho ids   <file.cho>... [--std]    # each declaration's content hash
cancho authority <file.cho>... [--std] [--output json]  # what it can reach
cancho layout    <file.cho>... [--std]  # what every leaf costs, and what packing would save
cancho fmt   <file.cho|dir>... [--check]   # canonical layout, comments kept; --check exits 1 if anything would change
cancho print <file.cho> [--output json]  # the unit, as canonical text or as the JSON an agent reads
cancho ingest <file.json> [--output json]  # that JSON back as canonical text; refusals as data
cancho agent-guidelines              # AGENTS.md, from inside the binary
cancho introspect [--output json]    # the full command tree, as data (docs/agent-cli.md)
cancho skill [--output json] [<out-file>]  # a generated SKILL.md, agentskills.io
cancho install [--ignore-compiler-rev]  # fetch and check the dependencies named in cancho.toml
cancho add <name> <git-url> [--rev <hash> | --ref <name>] [--path <dir>] [--ignore-compiler-rev]  # add one, pinned to a commit
cancho build [--bin <name>] [--ignore-compiler-rev]  # with no files: install, then build the programs of cancho.toml
cancho test  [--test <name>] [--ignore-compiler-rev]  # with no files: install, then run the [[test]] sets of cancho.toml
cancho vcs publish [--store <dir>] [--std] [--requires <lock>[:<dep-store>]]... <file.cho>  # log every declaration as an operation
cancho vcs publish [--store <root>] [--std] --dir <dir>  # a library of several files, one store each, in dependency order
cancho vcs sync [--dir <dir>] [--std] [--check]  # write a directory's sorted identity index (.cancho/ids.txt); --check writes nothing and exits 1 on a semantic change
cancho vcs log     [--store <dir>]            # what a store already has
cancho vcs resolve [--lock <file>] [<store-dir>]  # re-check every pin under today's compiler
cancho vcs lock (--store <dir> | --git <url> (--rev <hash> | --ref <name>) [--path <dir>]) -o <file> (--all | <name>...)  # pin names to a dependency's hash
cancho vcs fetch --lock <file> [--store <dir>] -o <dir>  # verify a lock, write its sources to disk
cancho --version
```

This block is checked against `--help` in both directions by a test
(`tests/commands/docs.sh`), so a command cannot be added or removed
without it. The flags are not checked that way: where `--help` and this
block differ on a flag, the block says what the compiler does today.

`authority` is the one worth trying on something you did not write:

```sh
$ cancho authority examples/tally.cho --std
performs
    io_read
    io_write
never touches
    the filesystem
    the network
    the heap
    the command line
    signals
    other programs
    foreign code
provably pure (2 of 6)
    is_blank
    width
```

`--output json` gives the same report as data; its first field is
`"bounded"` and its second `"unbounded_by"`, exactly which foreign symbols,
as `scope:symbol`. Exit codes are semantic throughout: `0` success, `1`
refused with a located diagnostic, `2` the command line was wrong, `3` the
environment failed, and (`test` only) `4` a test failed.

## Docs

**[alpibrusl.github.io/cancho](https://alpibrusl.github.io/cancho/)** —
the language, the ecosystem it sits in, and every design document, one
per page, in order. Built from [`docs/`](docs/), which is just as
readable straight on GitHub if you'd rather not leave the repo — start
at [`docs/README.md`](docs/README.md).

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) — the gate, how a slice is done,
the file budget, and code conventions. [`AGENTS.md`](AGENTS.md) is the
one page for writing cancho *programs*, not the compiler.

Design lands in `docs/` **before** the code that implements it, and a
claim that turns out wrong is corrected in place there rather than
quietly edited — the roadmap says which.

## Licence

[EUPL-1.2](LICENSE), matching the rest of the ecosystem. See `LICENSE`
for the notice and where to obtain the full text in any of the 23 EU
languages.

Every package under `packages/` carries its own copy of the same `LICENSE`, so a
package published or extracted on its own is still EUPL-1.2. The copy is a
file beside the source, not a line in it: a comment in a `.cho` file would change
its `source_hash` and so every lock that pins it.
