# Pinning the compiler by version

Status: **design, not built.** Nothing in this document is measured yet except
what §1 reads from the source; each claim about behaviour after the change is a
prediction, to be replaced by a measurement when the slices below land.

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
bumping; the release workflow should refuse that (a tag for a version that
already has one). This part is not built, and whether the workflow already
refuses it has not been checked.

## 4. What is not decided

1. **Finding a release from a version.** Release assets are named by commit
   (`cancho-<hash>-<target>.tar.gz`, `scripts/install.sh <commit>`). A project's
   CI that reads `cancho = "0.1.1"` needs the commit. Candidates: a second tag
   `v0.1.1` on the release, which `install.sh` accepts in place of a commit; or
   the release notes. Choose after looking at what `install.sh` can resolve
   without a new network dependency.
2. **Whether the hash form should warn** when it equals a released commit. Not
   needed to ship; noted.

## 5. Slices

1. This document.
2. `main` moves to `0.1.1-dev`; `projfile.cho` accepts `N.N.N`;
   `check_compiler` compares it; refusals keep a rule tag; tests: accepted,
   refused for another version, refused for a `-dev` build, hash form
   unchanged, `unknown` build says so. Renew `tests/commands/*.expected` and
   `docs/cli-in-cancho.md`.
3. `0.1.1` released (needs the owner's go-ahead; it is outward-facing).
4. Each `cancho-*` repository that has a pin moves to `cancho = "0.1.1"` in a
   PR of its own, after building and testing with that compiler; those that have
   no `cancho.toml` are listed and decided one by one, since adding a pin is a
   change to how they build.
