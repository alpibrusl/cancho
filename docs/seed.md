# The seed: building the compiler in cancho with no Rust

Status: **design, written before any `seed/` directory exists.** Phase 1a of [`removing-rust.md`](removing-rust.md) (epic
[#491](https://github.com/alpibrusl/cancho/issues/491), issue #488). Numbers marked *measured* come from one run on macOS aarch64 on
2026-11-05 (the tree of #697, stage 0 a debug Rust build); everything else is a decision proposed here. A claim that turns out false is
corrected here, in place.

**Measured so far:** macOS aarch64 (2026-11-05) Linux aarch64 (2026-11-06, a Debian 13 container on the same Mac, clang 19.1.7, `main` at `56a1ad84`) and Linux x86-64 (2026-11-06, the `seed-ir` workflow on `ubuntu-latest`, clang 18.1.3, `scripts/seed-ir-fixed-point.sh`, [#704](https://github.com/alpibrusl/cancho/pull/704)).
**Not measured:** the IR fixed point on macOS (stage 1 against stage 2 IR).

## 1. The problem

`scripts/build-standalone.sh` starts from a Rust `cancho` (stage 0, `--cancho0`) that builds stage 1 with the compiler in cancho; stage 1
builds stage 2 and stage 2 builds stage 3, and stage 2 must equal stage 3. Delete the Rust and there is no stage 0. A compiler written in
itself needs a first compiler that is not built from the sources it is about to compile: the **seed**.

## 2. What the seed is

**The LLVM IR text that the compiler writes for its own command line, one file per target, committed.** `clang` turns it into a working
`cancho` (the *seed compiler*), which then runs the existing chain:

```
seed/<target>.ll.gz --clang--> cancho0 --builds--> stage 1 --> stage 2 --> stage 3        (stage 2 = stage 3, as now)
                                          from cancho.files + std, all of it text in the repository
```

Why IR and not a binary: it is text a person or a tool can read and diff; it needs only `clang`, which the build needs anyway (#489); and
the one trusted thing is a file format (LLVM IR), not an executable of unknown origin. Why not a pinned release binary: it needs a network
and trusts whoever built it. Why not the whole compiler's source in another language: that is the Rust we are removing.

*Measured* (the 77-file command line, `--std`):

| | IR bytes | gzip -9 | xz -9 | `clang -O2` from the IR | stage 2 from stage 1 |
|---|---|---|---|---|---|
| macOS aarch64 (tree of #697) | 22,140,821 (288,887 lines, 2,126 functions) | 1.89 MB | 1.07 MB | 8 s | 10 s |
| Linux aarch64 (`main` at `56a1ad84`) | 22,540,267 | 1.91 MB | not run | not timed | not timed |
| Linux x86-64 (tree of #704) | 22,594,598 | 1.92 MB (1,919,664) | not run | not timed | not timed |

**The fixed point on IR holds on Linux aarch64 and on Linux x86-64.** The IR written by stage 1 (built by the Rust compiler) and the IR written by stage 2 (built by
stage 1) are the same file, SHA-256 `3cd382d9c0f57d5e78d410d7f1b51f3ed1482bf2745623ef12688610027bc106`, 22,540,267 bytes; stage 2 equals stage 3
byte for byte; and **`clang -O2` on that IR alone, in another directory, gives an executable byte-identical to stage 3**. So on Linux the seed
needs no special comparison. On x86-64 the same holds: stage 1 and stage 2 write the same IR, SHA-256
`a42d0222a48cd71724dbc73151c75ec755096c26809656fdf2a999dcaecc3ac6`, 22,594,598 bytes; stage 2 equals stage 3 byte for byte; and `clang -O2` on that
IR alone gives an executable byte-identical to stage 3. (The two Linux runs used different clang versions, 19.1.7 and 18.1.3, and both hold.) (On macOS the same experiment differs by 48 bytes, §3 S4.) This is what check 2 of §5 depends on.

## 3. Decisions

- **S1. Targets.** Three files, `seed/aarch64-apple-darwin.ll.gz`, `seed/x86_64-unknown-linux-gnu.ll.gz`, `seed/aarch64-unknown-linux-gnu.ll.gz`
  (the matrix of `removing-rust.md` §4; Windows is out, D4). The IR differs per target (the `TARGET` line picks runtime files and
  constants). Linux aarch64 and Linux x86-64 are measured (above). Each has to be written on its own Linux: the standalone build refuses a
  `--target` that is not its own host (`cli-in-cancho.md` §38).
- **S2. Compression: gzip.** `gzip` is on every machine that has `clang`; xz saves 0.8 MB a target and adds a dependency. 3 targets: about
  5.7 MB a reseed, against about 3.2 MB with xz. **[open]** if the repository's size is the worse cost.
- **S3. The recorded hash** is SHA-256 of the **uncompressed** IR, in `seed/<target>.sha256` (not BLAKE3: `shasum` is on both platforms and
  `b3sum` is not; the plan said BLAKE3, corrected here). The scripts check it before `clang` reads the file.
- **S4. What "the same" means.** On Linux the executables from the same IR are byte-identical across directories (the existing stage 2 =
  stage 3 check). On macOS they are not: *measured*, 48 bytes differ at the Mach-O header between two output directories, while `otool -tv`,
  `nm -a` and `strings -a` are identical, which is the comparison `build-standalone.sh` already makes for a stage 3 built under another name.
  The seed check uses the same: bytes on Linux, code plus symbols plus strings on macOS.
- **S5. Stage 0 disappears from the release.** `release.yml` runs `seed/<target>` → `cancho0` → stages 1 to 3 and publishes stage 2 (phase 4).
  `--cancho0` stays as an option for developers who have a `cancho` already.

## 4. The scripts

- `scripts/seed-build.sh [--out DIR]`: checks the hash, `gunzip`, `clang -O2 -Wno-override-module`, writes `DIR/cancho0`. No Rust, no `cancho`.
- `scripts/reseed.sh [--cancho0 PATH]`: builds the chain from the committed seed (or from `--cancho0`), then writes the IR that the stage-2
  compiler produces for the same files, with `--clang` pointed at a wrapper that copies the `.ll` before `clang` reads it (*measured*: this is how
  the 22 MB above were captured; no new flag in the compiler), compresses it, and writes the hash. Run on each of the three targets; the
  result is one pull request, "reseed", that changes only `seed/`.
- `scripts/build-standalone.sh` gains `--seed` (default when no `--cancho0` is given) and drops its Rust default.

## 5. When the seed is refreshed, and what CI checks

A seed that is rebuilt on **every** change to the compiler would put about 5.7 MB into the history each time. So the seed is allowed to **lag**,
with one rule and three checks:

- **The rule (the bootstrap rule).** The compiler's sources may use a language feature, a builtin or a standard-library function only when the
  committed seed already compiles it. A change that needs a newer compiler than the seed is two pull requests: the compiler change, then the
  reseed, and only then the code that uses it.
- **Check 1, every pull request that touches `examples/selfhost/`, `std/` or `cancho.files`:** the chain from the committed seed builds stage
  1, 2 and 3, and stage 2 equals stage 3 (S4). This is what catches a use of a feature the seed lacks.
- **Check 2, every pull request that touches `seed/`:** the hash matches, and **the IR written by the stage-2 compiler of the same sources
  equals the IR in `seed/`** (a seed that is a fixed point of its own sources, not only of itself). Without this a seed could be anything.
- **Check 3, scheduled (weekly) and before each release:** the same as check 2 on the head of `main`. It fails when the seed is so old that
  `main`'s IR differs from it, which is the signal to reseed; it does not block a pull request.

The fixed point on IR (stage 1 against stage 2) is **measured true on Linux aarch64 and Linux x86-64**; on macOS it is not yet measured. If it
were false the reseed would have to use stage 2's output, which is what S5 assumes.

## 6. Risks

- **The seed is 22 MB of IR no one reads.** The assurance is the checks above and the reject goldens, not review. A bug in the seed's compiler that
  also lives in the sources passes the fixed point (`removing-rust.md` §7); `tests/reject.golden` and the conformance goldens are the
  defence, and they are kept, not regenerated from the new compiler.
- **Repository growth.** One reseed is about 5.7 MB; history keeps every one. If reseeds are monthly, about 70 MB a year. **[open]** a limit, and
  whether old seeds are dropped from a release branch only.
- **`clang` is trusted.** The seed IR is trusted as text, `clang` as a program (#489 says the compiler still needs it).
- **A new target** needs an IR file written on that target, by a compiler that already runs there: a chicken and an egg that is solved once, by the
  cross build of the Rust compiler, before the crates are deleted; after that, never. This is a reason to write all three seeds **before** phase 5.

## 7. What this does not do

It commits no seed, writes no seed script and changes no workflow beyond the measuring job of #704. Phase 1a is: (1) the macOS measurement of the IR fixed point (both Linux targets are done), (2) the two
scripts and `seed/` for one target, (3) the other two, (4) the checks in CI, each its own pull request. Phase 4 uses it.
