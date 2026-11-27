<p align="center"><img src="docs/assets/cancho-logo-256.png" alt="cancho" width="200"></p>

# cancho

A native systems language in which ownership and authority are one system: a value is consumed exactly once, and reaching the outside world (the console, a file, a socket, a foreign library) takes a capability the program was handed, checked at compile time. The compiler is written in cancho, and `cancho authority` says what a program can reach before it runs.

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

```
$ cancho run greet.cho --std
Hello, cancho
$ cancho authority greet.cho --std
performs
    io_write
never touches
    the filesystem
    the network
    the heap
    the command line
    signals
    other programs
    foreign code
```

`split(world)` is the only place a program gets authority, and it gets all of it, once. `greet` says `[io_write]` because it was handed a borrowed `&!i Io`; a function nothing was given cannot print. More in [`examples/tour.cho`](examples/tour.cho), and `cancho agent-guidelines` prints [`AGENTS.md`](AGENTS.md), the one page for writing cancho.

**Status: 0.1.0, pre-1.0.** Built from a committed seed with `clang` alone, and it reproduces itself byte for byte. Linux (x86-64, aarch64) and macOS (Apple silicon), LLVM only. Not a usable language yet: it is still growing, and a dependency is a hash. What exists and how each part is checked is in [`docs/status.md`](docs/status.md); the roadmap says what is next.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/alpibrusl/cancho/main/scripts/install.sh | sh
cancho --version
```

One file of about 1.2 MB in `~/.local/bin`, checked against its published sha256 and the commit it reports (that detects a damaged download; it does not authenticate who published it). Building a program needs `clang` 15 or newer and `cc`; name another with `cancho build ... --clang /path/to/clang`. To work on the compiler: `scripts/gate.sh` ([`docs/status.md`](docs/status.md) section 3).

### On a Raspberry Pi

Two different things, and only the first is known to work.

- **The compiler, on a Pi running Linux.** A Pi 4 or Pi 5 with the 64-bit Raspberry Pi OS is Linux on aarch64, which the release is built for. `sudo apt install clang`, then the install line above. The 32-bit OS has no build.
- **A program with no Linux under it.** `cancho build --target aarch64-none-elf --emit obj` writes an object with no libc in it ([`docs/freestanding.md`](docs/freestanding.md)), and hardware is reached through two capabilities, [`Mmio`](docs/mmio.md) and [`Cpu`](docs/cpu.md). [cancho-os](https://github.com/alpibrusl/cancho-os) is a kernel built this way; its README has the steps to put it on a Pi 4's SD card. It has booted one real Pi 4 (one board, one card, one day of sessions; its README says what was seen there) and boots in QEMU as a Pi 3 and as QEMU's `virt` machine on every change. The target is on `main` and is not in release 0.1.1: build the compiler from a checkout with `sh scripts/build-standalone.sh --out DIR --rev $(git rev-parse HEAD)`, which needs only `clang`.

## Deliberately not here

No garbage collector, no destructors, no exceptions. No borrow checker (borrowing is lexical). No implicit conversions and no ambient authority. No undefined behaviour ([`docs/defined-behaviour.md`](docs/defined-behaviour.md)). No macros, which would break stable content-addressed identity.

### The compiler's whole surface

```sh
cancho build <file.cho>... [-o <output>] [--emit exe|obj|ll] [--std] [--target aarch64-none-elf] [-l <name>]... [-L <path>]... [--clang <path>]
cancho check <file.cho>... [--std] [--output json]   # refuse, or say nothing
cancho run   <file.cho>... [--std] [-l <name>]... [-L <path>]...   # build, run, exit with the program's status
cancho test  <file.cho>... [--std] [-l <name>]... [-L <path>]...   # run every `fn test_*`, one process each; exit 4 if one failed
cancho satisfy <contract.cho> <candidate.cho> [--std]   # a contract's tests call the candidate; the verdict names the SigId
cancho ids   <file.cho>... [--std]    # each declaration's content hash
cancho ids   <file.cho>... [--std] --edges   # as JSON: what each declaration refers to, its file, lines and signature
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

Exit codes: `0` success, `1` refused with a located diagnostic, `2` the command line was wrong, `3` the environment failed, `4` (`test`) a test failed. `--output json` makes every refusal data: a stable rule tag, a position, a sentence. This block is checked against `--help` in both directions (`tests/commands/docs.sh`); where they differ on a flag, the block says what the compiler does today.

## Docs

[alpibrusl.github.io/cancho](https://alpibrusl.github.io/cancho/): the language, the ecosystem and every design document, one per page; the same files read on GitHub from [`docs/README.md`](docs/README.md). [`CONTRIBUTING.md`](CONTRIBUTING.md) is the gate and how a slice is done. Design lands in `docs/` before the code, and a claim that turns out wrong is corrected in place.

Licence EUPL-1.2, copyright Alpibru S.L. ([`LICENSE`](LICENSE)); every package under `packages/` carries its own copy.
