# cancho

A **native systems language in which resource ownership and authority are
part of the program's type-level contract.** Ownership and capability-typed
effects are one system rather than two: a value is consumed exactly once,
and reaching the outside world — the console, a file, a foreign library —
requires holding a capability for it, checked at compile time.

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
*owns* the capability rather than borrowing one. Run it:

```sh
cargo run -p cancho -- run examples/tour.cho --std
```

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
- **No textual or proc macros.** They break stable, content-addressable
  identity — a design commitment kept from the day this project started,
  not added on later.

And honestly, **not a usable language yet**: real dependencies now
resolve end to end (`cancho vcs publish`/`lock`/`fetch`,
`docs/package-system.md`) — `packages/net-sockets/`,
`packages/net-connect/`, `packages/agent-wire/`, `packages/
http-request/`, and `packages/http-response/` are five real published
packages (`packages/` also holds `http-server`, `tls` and `x509`), and `examples/fetch/fetch.cho` depends on two of them at once,
composed with no new tooling. **A dependency's own dependencies resolve
too now**
(`docs/package-system.md` §4.6): `packages/http-request/` itself needs
`net.sockets`, and `vcs publish --requires`/`vcs resolve`/`vcs fetch`
walk that closure recursively, refusing a cycle or a diamond conflict
rather than guessing — a claim this paragraph made and got wrong once
already, corrected here the way `ROADMAP.md` requires. There is still
no manifest, and no human-readable version string at all: a hash is the
only thing actually depended on. The effect vocabulary keeps growing, and
every addition since editions.md landed has been edition-gated and
additive by construction, so a file that does not opt into the edition
that adds a feature cannot be broken by it: 39% of this repository's own
historical revisions do not type-check under today's build
(`docs/hash-stability.md` §2, re-measured), most of it a closed debt from
one pre-editions rename — but that figure moved since the last time this
paragraph was corrected (37%, then thought to have stopped climbing), and
the honest reading now is that it grew again and not everything in that
growth has been individually explained yet. `c_ptr` lets `SSL_CTX *` and
similar opaque handles
type-check, and `-l`/`-L` (`docs/foreign-linking.md`) let `build` link
a library beyond libc, so a real TLS handshake compiles and runs
today (`examples/tls_client/`).
[The roadmap](https://alpibrusl.github.io/cancho/ROADMAP.html)
tracks what landed, what's next, and what each slice found out.

## Building

```sh
git clone https://github.com/alpibrusl/cancho
cd cancho
cargo test --workspace   # the whole gate: fmt, clippy and every test
```

Green on **linux-x86_64** and **darwin-aarch64**. `--backend llvm` is the
default codegen path (LLVM via `clang`, no new build dependency);
`--backend cranelift` is faster to iterate on and used the same way.

A project that pins a compiler (`cancho.toml`) can install the prebuilt one
instead of building it, once a release exists for that commit:

```sh
scripts/install.sh <commit> [prefix]   # sha256 and the reported commit are both checked
```

It still needs `clang` and `cc` at build time (`docs/package-system.md` §9).

### The compiler's whole surface

```sh
cancho build <file.cho>... [-o <output>] [--emit exe|obj] [--std] [--backend cranelift|llvm] [-l <name>]... [-L <path>]...
cancho check <file.cho>... [--std] [--output json] [--backend cranelift|llvm]   # refuse, or say nothing
cancho run   <file.cho>... [--std] [--backend cranelift|llvm] [-l <name>]... [-L <path>]...   # build, run, exit with the program's status
cancho test  <file.cho>... [--std] [--backend cranelift|llvm]   # run every `fn test_*`, one process each; exit 4 if one failed
cancho ids   <file.cho>... [--std]    # each declaration's content hash
cancho authority <file.cho>... [--std] [--output json] [--target <triple>]  # what it can reach
cancho layout    <file.cho>... [--std]  # what every leaf costs, and what packing would save
cancho fmt   <file.cho|dir>... [--check]   # canonical layout, comments kept; --check exits 1 if anything would change
cancho print <file.cho>               # the unit, rendered in canonical form
cancho agent-guidelines              # AGENTS.md, from inside the binary
cancho introspect [--output json]    # the full command tree, as data (docs/agent-cli.md)
cancho skill [--output json] [<out-file>]  # a generated SKILL.md, agentskills.io
cancho install [--ignore-compiler-rev]  # fetch and check the dependencies named in cancho.toml
cancho add <name> <git-url> [--rev <hash> | --ref <name>] [--path <dir>] [--ignore-compiler-rev]  # add one, pinned to a commit
cancho build [--bin <name>] [--ignore-compiler-rev]  # with no files: install, then build the programs of cancho.toml
cancho test  [--test <name>] [--ignore-compiler-rev]  # with no files: install, then run the [[test]] sets of cancho.toml
cancho vcs publish [--store <dir>] [--std] [--requires <lock>[:<dep-store>]]... <file.cho>  # log every declaration as an operation
cancho vcs publish [--store <root>] [--std] --dir <dir>  # a library of several files, one store each, in dependency order
cancho vcs log     [--store <dir>]            # what a store already has
cancho vcs resolve [--lock <file>] [<store-dir>]  # re-check every pin under today's compiler
cancho vcs lock (--store <dir> | --git <url> (--rev <hash> | --ref <name>) [--path <dir>]) -o <file> (--all | <name>...)  # pin names to a dependency's hash
cancho vcs fetch --lock <file> [--store <dir>] -o <dir>  # verify a lock, write its sources to disk
```

This block is checked against `--help` in both directions by a test
(`the_readme_commands_still_work`), so it cannot drift silently.

`authority` is the one worth trying on something you did not write —
it's the union of what everything `main` reaches performs, computed
from the same reachability that decides what goes in the binary, so
it's precise rather than conservative:

```sh
$ cancho authority examples/tally.cho --std
performs
    io_read
    io_write
never touches
    the filesystem
    the heap
    the command line
    foreign code
```

`--output json` gives the same report as data and **fails closed**: its
first field is `"bounded"`, `false` for any program that reaches foreign
code, and the second is `"unbounded_by"`: exactly which foreign symbols,
as `scope:symbol` (`docs/foreign-authority.md`). Exit codes are semantic throughout: `0` success, `1` refused with
a located diagnostic, `2` the command line was wrong, `3` the
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
