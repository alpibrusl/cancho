# Pinning the compiler by version

Status: **slice 2 built** (the pin form, `main` at `0.1.1-dev`) and the tooling of
slice 3 (`install.sh <version>`, the release workflow); the release itself and slice 4
are not. §3 is measured by `tests/commands/cli_project.sh`: a `-dev` build
refuses `cancho = "0.1.0"` (and says which version it is), accepts it with
`--ignore-compiler-rev`, a release build accepts the pin that names it and
refuses another, and a project file whose pin is `N.N`, `N.N.N.N`, `vN.N.N`,
`N.N.N-dev` or empty is refused at load. §4.1 is decided; `install.sh <version>` is measured by `tests/commands/release.sh`, the
workflow's two refusals only when it publishes (a pull request cannot run them).

## 1. What there is

`[package] cancho` in `cancho.toml` names the compiler a project was written
for. `projfile.cho` accepts only a full commit hash there (40 or 64 lowercase
hex digits), and `check_compiler` in `compiler/cliproject.cho` compares it with
the commit that `--version` reports (`version.rev()`), refusing a difference
unless `--ignore-compiler-rev` is given. A compiler built outside a git checkout
reports `unknown` and cannot satisfy any pin.

Every project in the sibling repositories that has a pin writes a hash. A hash
is exact, and unreadable: a reader cannot tell whether
`162633a79a45a14e5eeecb54cb33c31a39bfaed9` is the 0.1.0 compiler or something
older, and moving a project to a new release means finding the commit by hand.

## 2. What is asked

A project should be able to say `cancho = "0.1.0"`, a release, instead of a
hash. The hash form stays, because a pin to an unreleased commit is still
needed (a project written against a fix that has not been released).

## 3. The rule

A pin that is `N.N.N` (digits and two dots) is compared with
`version.crate_version()`; any other accepted form is the hash, compared as now.
Nothing is resolved and nothing is fetched: the compiler already knows its own
version.

This is weaker than a hash in one respect, and the design has to close it. Every
build between two releases reports the version of the last one, so
`cancho = "0.1.0"` would accept any `main` after the 0.1.0 release. The rule
that closes it:

- **`version.cho` holds a release's version only at the commit that is
  released.** Right after a release, `main` moves to the next version with a
  `-dev` suffix (`0.1.1-dev`), and the release commit is the one that drops the
  suffix.
- The pin form accepts `N.N.N` only, never `N.N.N-dev`: a `-dev` build satisfies
  a hash pin and no version pin, and a version pin names a release.

Two commits can still both report `0.1.0` only if someone releases without
bumping; the release workflow refuses that (a tag for a version that already
has one), see §4.1.

## 4. What was not decided

1. **Finding a release from a version** (decided). The release workflow, when it publishes, also
   makes the release `v<version>` with one asset, `commit`, holding the full hash, and
   `scripts/install.sh 0.1.1` reads it (`CANCHO_VERSIONS` names another place) and then does
   what it does for a hash: the published sha256, the commit the binary reports, and now the
   version it reports. Chosen over the release notes (they are prose to parse) and over a
   second tag alone (a tag says a commit, but `install.sh` reaches releases by asset URL, and
   a tag URL is not one). No new host: the same `releases/download` the assets are on. The
   workflow refuses to publish a commit whose `version.cho` holds a `-dev` version, and a
   version that already has a release (the tag `v<version>` exists), which is the check
   §3 asks for. A release that was made before this (0.1.0) has no `v0.1.0` and is found by
   its hash.
2. **Whether the hash form should warn** when it equals a released commit. Not
   needed to ship; noted.

## 5. Slices

1. This document.
2. `main` moves to `0.1.1-dev`; `projfile.cho` accepts `N.N.N`;
   `check_compiler` compares it; refusals keep a rule tag; tests: accepted,
   refused for another version, refused for a `-dev` build, hash form
   unchanged, `unknown` build says so. Renew `tests/commands/*.expected` and
   `docs/cli-in-cancho.md`.
3. `0.1.1` released (the owner gave the go-ahead 2026-11-23; it is outward-facing).
4. Each `cancho-*` repository that has a pin moves to `cancho = "0.1.1"` in a
   PR of its own, after building and testing with that compiler; those that have
   no `cancho.toml` are listed and decided one by one, since adding a pin is a
   change to how they build.
