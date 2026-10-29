# A `cancho` built by cancho: the first standalone release

Status: **design; nothing here is built.** Written before the code, as `CONTRIBUTING.md` asks. What was run to write it is in §1 and is the only
measurement in the document; everything after it is a **[choice]**, a **[measure]** (a later slice must measure it) or a **[read]** (taken from
the named file, not re-run).

Issue #485 moves the project from Rust to cancho; #486–#490 remove the Rust. This document is the part that is left between "the compiler is
written in cancho and is the default builder" (done: the compiler in cancho builds every program of the sibling repositories, #637/#639) and
"the `cancho` people run is a program built by the compiler in cancho". The maintainer's decision for the first release is to close the gaps of
the second **before** releasing, and to keep the Rust code in the repository as a backup.

## 1. What was run

On `main` at `c73f4ea0`, `x86_64-unknown-linux-gnu`. The command line in cancho was built as the tests build it, from the closure of
`examples/selfhost/cli.cho` (75 of the 94 files in the directory), with the compiler in cancho alone:

```sh
CANCHO_COMPILER=auto cancho build <the 75 files> --std -o cancho-cho      # 35 s; 2,348,376 bytes
```

and every form below was run against it and against the Rust binary, in an environment with no `cancho`, no Rust and no cache (`env -i`).

| form | the Rust binary | the one built by cancho |
|---|---|---|
| `--version` | `cancho 0.0.0 (rev c73f4ea0…, host x86_64-unknown-linux-gnu)`, status 0 | `usage: cancho (fmt …)`, status 2 |
| `build P --std` (a program that imports the library) | builds | status 2: "`--std` and `--target` are not available in this command line: `--std-dir DIR` adds the standard library from DIR" |
| `build P --std-dir std` | status 2, unknown option | builds |
| `check`, `authority`, `ids` with `--std` | work | status 2, as above |
| `build P --target wasm32-wasip1` | builds with a wasi-libc sysroot (`WASI_SYSROOT`), status 3 without | status 2, as above |
| `build P --backend cranelift` | builds | status 2: only `llvm` is available |
| `build P`, `build P --backend llvm`, `check P --output json`, `run P`, `layout`, `fmt --check`, `test` | work | the same status and the same words |
| `hello.cho` with no `--std` | builds and runs | builds and runs (`Hello, world!`) |

So the whole command surface exists and four things are missing: `--version`, `--std`, `--target` and the Cranelift backend. A fifth is a
consequence of the first: the **compiler pin** (`[package] cancho` in a project's `cancho.toml`) cannot be checked, because the program cannot say
which commit it was built from (`docs/cli-in-cancho.md`, the slice for `add` (§26): "Not done: the compiler pin").

How much of that the repositories that use cancho need, counted in files of the sibling repositories that mention it (`grep` over the `.yml`,
`.sh`, `.py`, `.md`, `Makefile` and `.toml` files of the 35 `cancho-*` repositories, 2026-10-28): `--backend cranelift` in 6 repositories
(44 files in `cancho-code`, where it is mostly the evaluation fixtures; 10 in `cancho-industrial`; 1 to 3 in each of the others); a WASI target in
one (`cancho-industrial`, 5 files); `--std` in all of them.

## 2. The decisions

**D1. The first standalone release builds for the host with the LLVM backend.** `--backend cranelift` and `--target wasm32-*` are refused by
the command line in cancho with a rule of their own (§4), not ignored, and say which binary to use. **[choice]** The reasons, each from this
repository: the LLVM backend has been the default since `docs/llvm-backend.md`; the compiler in cancho writes LLVM IR only, and the decision
that Cranelift stays Rust's is #523's (`docs/self-hosting.md`, stage 5q); the WASI target is a set of changes to the IR the Rust LLVM backend
writes (`docs/wasm.md`, W0.2–W2c: `size_t` at every call site, an inline checked multiply, the console without libc stdio) that the compiler in
cancho has not been given. Porting them is a later release (§6); the Rust binary stays in the repository, built by `cargo build --release -p
cancho`, and is what a project that needs either uses until then.

**D2. `--version` is stamped into the build, not read at run time.** **[choice]** A generated module `examples/selfhost/version.cho` holds the
commit and the host; the repository carries the unstamped form (`rev` is `unknown`, the word `crates/cancho/build.rs` uses when there is no git checkout and no `CANCHO_REV`) so that every test builds without a script, and the release
build writes the real one (`scripts/stamp-version.sh COMMIT HOST`) before it compiles. An unstamped binary reports `rev unknown`, and a project
that pins a compiler is refused by it unless `--ignore-compiler-rev` is given, as a Rust binary built outside a git checkout without
`CANCHO_REV` is **[measure]**: slice 1 reads how the Rust command line treats a stamp of `unknown` and does the same.

**D3. `--std` is the standard library carried in the binary.** **[choice]** The files of `std/` (46 files, 776,796 bytes of source, 2026-10-28)
become a generated module of string literals in the order `stdorder.cho` already fixes (`std_order_module_is_the_std_table_of_cancho` is the
precedent for a generated module that a test regenerates and compares). `--std-dir DIR` stays, for a library under development, and wins when
both are given. **[measured, slice 2]** A program of 808,893 bytes of literals (one per file) compiles in 0.5 s with the compiler in cancho and adds about 0.8 MB to the executable
(`docs/cli-in-cancho.md` §37). It is not too much, and the compressed blob is not needed.

**D4. A release is a commit, as now.** The tag is the full commit hash (`docs/package-system.md` §9, `.github/workflows/release.yml`), and `cancho
= "…"` in a project is that hash. A release adds a *kind* to the asset: `cancho-<commit>-<target>.tar.gz` stays the Rust-built binary until the
first standalone release, after which it is the one built by cancho; the Rust binary moves to `cancho-rust-<commit>-<target>.tar.gz`. **[choice]**

**D5. The first binary is built by the Rust one, the second by the first, and the third must equal the second.** A standalone `cancho` cannot be built from nothing. The release
build (`scripts/build-standalone.sh`) is: stage 0, the Rust `cancho` (`cargo build --release -p cancho`); stage 1, the 77 files of `examples/selfhost/cancho.files` (the import closure of
`cli.cho`, stamped) built by stage 0 with the compiler in cancho; stage 2, the same files built by stage 1; stage 3, built by stage 2. The build fails unless stage 2 and stage 3 are the same
bytes, and stage 2 is what is shipped. **[measured, slice 4]** (Linux x86-64, `env -i`, clang the only tool around): each stage takes 33 s and is 3.1 MB; **stage 1 differs from stage 2**
(3,136,648 against 3,138,248 bytes: stage 1 was linked by stage 0's driver, stage 2 by the command line in cancho's own builder) **and stage 2 equals stage 3** once both are built under the
same output name (the name of the object file is in the executable: built under two names, stages 2 and 3 differ in exactly one byte, `s2.ll` against `s3.ll`). So the fixed point is
reached at the second stage and not the first, and the comparison is 2 against 3. Stage 0 is the only place Rust is needed, and only in the build of a release or of the repository, never to
install or use one. **[choice]**

## 3. Slices

1. **`--version` and the pin.** `version.cho`, `stamp-version.sh`, `cli.cho --version`, the pin check in `check_compiler`. Gate: the Rust and
   the cancho command lines print the same line for the same stamp, and refuse and accept the same `[package] cancho` values (a matrix of
   missing, equal, different, `-dirty`, `--ignore-compiler-rev`). Mutants: a stamp that differs by one character is refused.
2. **`--std`.** The generated `stdsource.cho`, regenerated and compared by a test as `stdorder.cho` is; `--std` on `build`, `check`, `run`,
   `test`, `ids`, `authority`, `satisfy`, `vcs publish`. Gate: every program of `tests/accept` and the siblings' programs build both ways with
   the same results; the 780 KB cost of D3 is measured and stated.
3. **The refusals of D1.** `build --backend cranelift` and `build --target <wasm>` are refused with `unsupported-backend` and
   `unsupported-target-in-this-build`, with a fixture each, and the sentence names the Rust binary. Gate: the rule tags are in the catalogue and
   `tests/reject`.
4. **The fixed point.** `scripts/build-standalone.sh` (stages 0 to 3 of D5) and a test that fails unless stage 2 equals stage 3 and that the gate can fail; a CI job
   that runs it on each target the release builds follows in slice 5.
5. **The release.** `release.yml` builds and packages the standalone binary and the Rust one, smoke-tests `install.sh` on the packaged asset (it
   builds hello and a project with `cancho.toml`), and publishes on `workflow_dispatch`. `install.sh` gains a `latest` form (§5).
6. **The repositories.** Each repository's pin moves to the release commit, its CI installs the release instead of building the compiler from
   source, and its README says how to install (§5). One pull request per repository, drafts, merged on the maintainer's word.

Every slice is a pull request with the three gates of `CLAUDE.md`, a differential test against the Rust binary where the behaviour is shared,
and mutants where a gate is new.

## 4. Refusals added

**Corrected in slice 3.** This section said the two refusals of D1 are rules with a tag each (`unsupported-backend`, `unsupported-target-in-this-build`) and fixtures in `tests/reject`.
That was the wrong layer: a rule tag names a refusal of a *program* by the checker (`rules.rs`, `check --output json`), and `--backend cranelift` is a refusal of a *flag*, like every
flag this command line lacks, which is a usage error (status 2) with the sentence said and the usage printed. They are therefore sentences, not rules, and the test that pins them is a
conformance test of the command line: `selfhost::cli::the_command_line_in_cancho_says_what_this_build_does_not_carry`. The sentence is
"`FLAG VALUE` is not in this build of cancho, which writes code for the machine it runs on with the LLVM backend; the Rust build has it (`cargo build --release -p cancho`, or the
`cancho-rust` release asset; docs/standalone-cancho.md D1)". `--target` is accepted when it spells the host this build was stamped with (the Rust command takes the host's own triple
too) and refused otherwise; an unstamped build does not know its host, so it refuses every triple.

## 5. Installing

**`curl … | sh` works, and it is the way.** The installer already exists and already verifies what it fetches (`scripts/install.sh COMMIT [PREFIX]`:
the tarball's published sha256 is checked before it is unpacked, and the installed binary must report the commit that was asked for). What it
lacks for a quick start is a commit to ask for. **[choice]** `install.sh` with no commit resolves the latest release (the tag GitHub redirects
`releases/latest` to; release tags are hashes, so the redirect *is* the commit), then proceeds as now:

```sh
curl -fsSL https://raw.githubusercontent.com/alpibrusl/cancho/main/scripts/install.sh | sh
```

What this does not give, and the README says: both the script and the tarball come from GitHub, so the sha256 detects a damaged or mismatched
download and does not authenticate the publisher; a project that wants more pins the commit and checks the hash it was given out of band.

**`cancho-fetch` cannot be the first install and can be a later one.** `cancho-fetch` is a program built by cancho; installing it needs a
`cancho`. A program built by cancho can fetch a release (the TLS client is in the standard library), so a `cancho self-update` that downloads and
verifies the next release is possible once there is one binary, and is **not designed here**.

## 6. What stays Rust, and what is next

After slice 6 the Rust code in `crates/` is a backup: it still builds, it is still tested, and it is what a project needs for Cranelift or WASI.
Removing it (#486–#490) needs D1 reversed: the WASI changes ported to the compiler in cancho and a decision about Cranelift (port it, or retire the
backend with a release note). Neither is in this document, and neither is done by it.
