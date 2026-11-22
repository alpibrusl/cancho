# Where the project stands

> **Status: moved from `README.md` (2026-11-22).** The front page had grown
> to 280 lines, most of them this section and the long form of the
> surface listing, and the sibling repositories' pages are a screen long.
> The text is here unchanged; the figures were measured on 2026-11-22 on
> `main` and are not re-measured by anything, so they age. A claim that
> turns out false is corrected here, in place.

## 1. Where it stands

Everything below was measured on 2026-11-22, on `main`, unless a document
is cited.

- **The compiler** is [`compiler/`](../compiler/): 143 files, about 70,900
  lines of cancho (its own tests included), and about 16,500 lines of LLVM
  IR text in [`compiler/runtime/`](../compiler/runtime). The standard library
  is [`std/`](../std/), 47 files, about 23,100 lines, also cancho.
- **Optional comparisons.** The Rust halves of two benchmarks and the
  `rustls` peers of the TLS interop scripts skip when `rustc` or `cargo`
  is missing; nothing else needs them.
- **It reproduces itself.** `clang` builds the committed seed
  ([`seed/`](../seed/), [`docs/seed.md`](seed.md)); the seed builds the
  compiler; that compiler builds itself twice more and the last two must be
  identical. The release workflow does this on all three targets.
- **The gate** is [`scripts/gate.sh`](../scripts/gate.sh), written in cancho
  and shell: formatting, a file-size budget, a link check, every fixture of
  [`tests/accept`](../tests/accept) built and run, about 400 fixtures of
  [`tests/reject`](../tests/reject) each refused by the rule it names, and the
  compiler's own tests. Its design is [`docs/gate.md`](gate.md).
- **Packages**: [`packages/`](../packages/) holds eleven, among them a TLS 1.3
  client and server (written in cancho, with no C library, and **not
  independently reviewed**: [`docs/tls-server.md`](tls-server.md)),
  X.509, secp256k1, an HTTP client and a server, and the sockets they stand
  on. Dependencies resolve end to end by hash, a
  dependency's own dependencies included
  ([`docs/package-system.md`](package-system.md)).
- **History still reads.** Replaying every historical revision of `std/`,
  `examples/` and `compiler/` through today's compiler: 983 revisions, 964
  read, 16 do not, 3 cannot be judged by it. Each failure is listed with a
  reason in [`docs/stability-exemptions.md`](stability-exemptions.md),
  and the CI job fails if a rule starts failing more revisions than its
  recorded count ([`docs/hash-stability.md`](hash-stability.md) §2).
  Features arrive by *edition*, additively, so a file that does not opt in
  cannot be broken by one.

And honestly, **pre-1.0**: the language is still growing (the roadmap says
what landed, what is next and what each slice found out:
[`docs/ROADMAP.md`](https://alpibrusl.github.io/cancho/ROADMAP.html)), there
is no human-readable version string for a dependency — a hash is the only
thing actually depended on — and the compiler writes code for two operating
systems and two architectures, not more. Release `0.1.0` is the one this page
describes.


---

## 2. What the commands are for

A program that must be shown to do only what it is allowed to do, by
something other than its author's say-so:

- **`cancho authority`** says what a program can reach: the union of what
  everything `main` reaches performs, from the same reachability that
  decides what goes in the binary, so it is precise rather than
  conservative. `--output json` gives the report as data and **fails
  closed**: a program that reaches foreign code is `"bounded": false` and
  names the symbols ([`authority.md`](authority.md),
  [`foreign-authority.md`](foreign-authority.md)).
- **`cancho ids`** gives every declaration a content hash that does not
  move when it is renamed or reformatted, and `cancho vcs` keeps a
  content-addressed log of them; a dependency is a hash
  ([`canonical-ast.md`](canonical-ast.md),
  [`package-system.md`](package-system.md)).
- **`cancho check --output json`** answers every refusal as data: a stable
  rule tag, a position, a sentence ([`agent-errors.md`](agent-errors.md)).
- **`cancho satisfy`** is the spec-to-implementation loop: a contract is a
  file whose `fn test_*` call a candidate, and the verdict names the
  candidate's identity ([`satisfy.md`](satisfy.md)).

---

## 3. Building the compiler

```sh
git clone https://github.com/alpibrusl/cancho
cd cancho
scripts/gate.sh   # builds the compiler from the seed, then every check
```

The `cancho` of a release is built by `scripts/build-standalone.sh`: `clang`
builds the committed seed, which compiles the command line written in
cancho ([`../compiler/cli.cho`](../compiler/cli.cho)), which compiles itself
twice more and must reproduce itself byte for byte
([`standalone-cancho.md`](standalone-cancho.md)). A project that pins a
compiler (`cancho.toml`) installs the prebuilt one instead
([`package-system.md`](package-system.md) §9).
