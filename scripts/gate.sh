#!/bin/sh
# The gate without Rust (docs/gate.md section 2, step G2): the checks a change must pass, cheapest first, stopping at the first that fails and
# saying which stage it was.
#
#   scripts/gate.sh [--cancho PATH] [--only STAGE] [--skip STAGE]...
#
#   --cancho PATH  the `cancho` to run the checks with (also $CANCHO); with none, the one `scripts/build-standalone.sh --seed` builds from the committed seed
#   --only STAGE   run one stage: fmt, files, seed, walk or reject
#   --skip STAGE   leave a stage out (more than once is allowed); the seed stage rebuilds the compiler three times and is the dear one
#
# Stages: fmt (`cancho fmt --check` over the corpus of conformance/formatting.rs), files (scripts/files-budget.cho: no source file over 2,000
# lines), seed (scripts/seed-build.sh against the committed seed: the hash matches and clang builds it), walk (tests/run.cho: every fixture of
# tests/accept and every example built and run, every fixture of tests/reject refused by its rule; `--selfhost`, since the compiler is the one in cancho), reject
# (scripts/reject_golden.py check against tests/reject.golden). The *_test.cho files join `walk` as they move (G4).
# Run from anywhere; it works from the root of the repository. Exit status: 0 if every stage ran and passed, 1 if one failed, 2 for a bad call.
set -u
here=$(cd "$(dirname "$0")/.." && pwd)
cd "$here" || exit 2
cancho=${CANCHO:-}
only=
skip=" "
while [ $# -gt 0 ]; do
  case $1 in
    --cancho) cancho=$2; shift 2 ;;
    --only) only=$2; shift 2 ;;
    --skip) skip="$skip$2 "; shift 2 ;;
    *) echo "usage: gate.sh [--cancho PATH] [--only fmt|files|seed|walk|reject] [--skip STAGE]..." >&2; exit 2 ;;
  esac
done
for s in $only $skip; do
  case $s in fmt|files|seed|walk|reject) ;; *) echo "gate: no stage called $s (fmt, files, seed, walk, reject)" >&2; exit 2 ;; esac
done

wanted() { [ -z "$only" ] && case "$skip" in *" $1 "*) return 1 ;; esac; [ -z "$only" ] || [ "$only" = "$1" ]; }

# The stage that is running, said when one fails.
fail() { echo "gate: FAILED at stage $1" >&2; exit 1; }
stage() { echo "gate: $1" >&2; }

if [ -z "$cancho" ]; then
  # Nothing given: build the compiler from the seed, once, into a directory that is the same each time so a second run is cheap to read.
  work=${GATE_WORK:-$here/target/gate}
  stage "no --cancho: building one from the seed (scripts/build-standalone.sh --seed)"
  sh scripts/build-standalone.sh --seed --out "$work" --rev "$(git rev-parse HEAD 2>/dev/null || echo unknown)" >/dev/null || { echo "gate: the seed did not build a compiler" >&2; exit 1; }
  cancho=$work/cancho
fi
[ -x "$cancho" ] || { echo "gate: $cancho is not an executable" >&2; exit 2; }
case $cancho in /*) ;; *) cancho=$(pwd)/$cancho ;; esac

if wanted fmt; then
  stage "fmt: cancho fmt --check"
  "$cancho" fmt --check std examples packages tests/accept tests/programs tests/run.cho scripts benches || fail fmt
fi

if wanted files; then
  stage "files: no source file over the budget"
  "$cancho" run scripts/files-budget.cho --std || fail files
fi

if wanted seed; then
  stage "seed: the committed seed matches its hash and builds"
  out=$(mktemp -d "${TMPDIR:-/tmp}/gate-seed.XXXXXX") || exit 2
  sh scripts/seed-build.sh --out "$out" >/dev/null || { rm -rf "$out"; fail seed; }
  rm -rf "$out"
fi

if wanted walk; then
  stage "walk: tests/run.cho over tests/accept, tests/reject and examples"
  scratch=$(mktemp -d "${TMPDIR:-/tmp}/gate-walk.XXXXXX") || exit 2
  "$cancho" build tests/run.cho --std -o "$scratch/fixtures-run" || { rm -rf "$scratch"; fail walk; }
  for kind in reject accept examples; do
    "$scratch/fixtures-run" "$cancho" "$kind" --selfhost --cache "$scratch/cache" || { rm -rf "$scratch"; fail walk; }
  done
  rm -rf "$scratch"
fi

if wanted reject; then
  stage "reject: the rules each program of tests/reject is refused by"
  python3 scripts/reject_golden.py check "$cancho" || fail reject
fi

stage "ok"
