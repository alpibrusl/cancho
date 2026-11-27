#!/bin/sh
# The gate without Rust (docs/gate.md section 2, step G2): the checks a change must pass, cheapest first, stopping at the first that fails and
# saying which stage it was.
#
#   scripts/gate.sh [--cancho PATH] [--only STAGE] [--skip STAGE]...
#
#   --cancho PATH  the `cancho` to run the checks with (also $CANCHO); with none, the one `scripts/build-standalone.sh --seed` builds from the committed seed
#   --only STAGE   run one stage: fmt, files, links, seed, walk, tests, reject or native
#   --skip STAGE   leave a stage out (more than once is allowed); the seed stage rebuilds the compiler three times and is the dear one
#
# Stages: fmt (`cancho fmt --check`), files (scripts/files-budget.cho: no source file over 2,000
# lines), links (scripts/links-check.cho: every relative link and every `examples/` or `compiler/` path the documents name exists), seed (scripts/seed-build.sh against the committed seed: the hash matches and clang builds it), walk (tests/run.cho: every fixture of
# tests/accept and every example built and run, every fixture of tests/reject refused by its rule; `--selfhost`, since the compiler is the one in cancho), reject
# (scripts/reject-golden.cho check against tests/reject.golden), tests (every `cancho test` run listed in tests/cancho-tests.txt, with the number of tests it must pass; the scripts of
# tests/commands run through tests/commands/script_tests.cho, which are what the Rust conformance suite was, G4).
# Stage native (docs/native-backend.md sections 13 and 14): the programs of stage walk, the `cancho test` runs and the command tests of stage tests again, built by the native backend
# (`--backend native`: no clang and no LLVM between the program and its object), each held to what its fixture says it prints. On a machine the native backend
# writes no code for (it has Linux on AArch64) the stage says so and passes.
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
    *) echo "usage: gate.sh [--cancho PATH] [--only fmt|files|links|seed|walk|tests|reject|native] [--skip STAGE]..." >&2; exit 2 ;;
  esac
done
for s in $only $skip; do
  case $s in fmt|files|links|seed|walk|tests|reject|native) ;; *) echo "gate: no stage called $s (fmt, files, links, seed, walk, tests, reject, native)" >&2; exit 2 ;; esac
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
  "$cancho" fmt --check std examples compiler packages tests/accept tests/programs tests/run.cho tests/support tests/commands scripts benches || fail fmt
fi

if wanted files; then
  stage "files: no source file over the budget"
  "$cancho" run scripts/files-budget.cho --std || fail files
fi

if wanted links; then
  stage "links: every link and repository path the documents name exists"
  "$cancho" run scripts/links-check.cho --std || fail links
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
  "$cancho" build tests/run.cho tests/support/harness.cho --std -o "$scratch/fixtures-run" || { rm -rf "$scratch"; fail walk; }
  for kind in reject accept examples; do
    "$scratch/fixtures-run" "$cancho" "$kind" --selfhost --cache "$scratch/cache" || { rm -rf "$scratch"; fail walk; }
  done
  rm -rf "$scratch"
fi

if wanted tests; then
  stage "tests: the cancho test runs of tests/cancho-tests.txt"
  list="${TMPDIR:-/tmp}/gate-tests.$$"
  grep -v '^#' tests/cancho-tests.txt | grep . > "$list" || true
  while IFS='|' read -r name passed arguments; do
    name=$(echo $name); passed=$(echo $passed)
    # shellcheck disable=SC2086
    result=$("$cancho" test $arguments 2>&1) || { echo "$result" >&2; echo "gate: $name failed" >&2; rm -f "$list"; fail tests; }
    echo "$result" | grep -q "test result: ok. $passed passed; 0 failed" || { echo "$result" >&2; echo "gate: $name: expected $passed passed" >&2; rm -f "$list"; fail tests; }
    stage "  $name: $passed passed"
  done < "$list"
  rm -f "$list"
  # The command tests: each program of tests/commands is built with tests/support/harness.cho and run with the compiler under test (docs/gate.md G4).
  scratch=$(mktemp -d "${TMPDIR:-/tmp}/gate-commands.XXXXXX") || exit 2
  count=0
  for source in tests/commands/*.cho; do
    [ -e "$source" ] || continue
    name=$(basename "$source" .cho)
    "$cancho" build "$source" tests/support/harness.cho --std -o "$scratch/$name" || { rm -rf "$scratch"; fail tests; }
    "$scratch/$name" "$cancho" --cache "$scratch/cache" || { rm -rf "$scratch"; fail tests; }
    count=$((count + 1))
  done
  rm -rf "$scratch"
  stage "  $count command tests"
fi

if wanted reject; then
  stage "reject: the rules each program of tests/reject is refused by"
  scratch=$(mktemp -d "${TMPDIR:-/tmp}/gate-reject.XXXXXX") || exit 2
  "$cancho" build scripts/reject-golden.cho tests/support/harness.cho --std -o "$scratch/reject-golden" || { rm -rf "$scratch"; fail reject; }
  "$scratch/reject-golden" check "$cancho" || { rm -rf "$scratch"; fail reject; }
  rm -rf "$scratch"
fi

if wanted native; then
  if [ "$(uname -sm)" = "Linux aarch64" ]; then
    stage "native: the fixtures and the cancho test runs, built by the native backend"
    scratch=$(mktemp -d "${TMPDIR:-/tmp}/gate-native.XXXXXX") || exit 2
    # The compiler under test, asked for the native backend whenever it builds: the walker and the list know nothing of backends.
    printf '#!/bin/sh\ncase "$1" in build|run|test) exec "%s" "$@" --backend native ;; *) exec "%s" "$@" ;; esac\n' "$cancho" "$cancho" > "$scratch/cancho"
    chmod +x "$scratch/cancho"
    "$cancho" build tests/run.cho tests/support/harness.cho --std -o "$scratch/fixtures-run" || { rm -rf "$scratch"; fail native; }
    for kind in accept examples; do
      "$scratch/fixtures-run" "$scratch/cancho" "$kind" --selfhost --cache "$scratch/cache" || { rm -rf "$scratch"; fail native; }
    done
    grep -v '^#' tests/cancho-tests.txt | grep . > "$scratch/list" || true
    while IFS='|' read -r name passed arguments; do
      name=$(echo $name); passed=$(echo $passed)
      # shellcheck disable=SC2086
      result=$("$scratch/cancho" test $arguments 2>&1) || { echo "$result" >&2; echo "gate: $name failed, built by the native backend" >&2; rm -rf "$scratch"; fail native; }
      echo "$result" | grep -q "test result: ok. $passed passed; 0 failed" || { echo "$result" >&2; echo "gate: $name: expected $passed passed" >&2; rm -rf "$scratch"; fail native; }
      stage "  $name: $passed passed"
    done < "$scratch/list"
    # The command tests, which are what uses the runtime hardest (files, directories, sockets, processes, terminals, TLS): each test's own program is built as
    # always, and is given the wrapper, so every program it builds and runs is the native backend's. `script_tests` is left out: its scripts are transcripts of
    # the command line, and they name the backend they ask for themselves (docs/native-backend.md section 14).
    count=0
    for source in tests/commands/*.cho; do
      [ -e "$source" ] || continue
      name=$(basename "$source" .cho)
      [ "$name" = script_tests ] && continue
      "$cancho" build "$source" tests/support/harness.cho --std -o "$scratch/$name" || { rm -rf "$scratch"; fail native; }
      "$scratch/$name" "$scratch/cancho" --cache "$scratch/command-cache" || { echo "gate: $name failed, its programs built by the native backend" >&2; rm -rf "$scratch"; fail native; }
      count=$((count + 1))
    done
    stage "  $count command tests"
    rm -rf "$scratch"
  else
    stage "native: the native backend writes no code for $(uname -sm): not run here (it runs on Linux on AArch64)"
  fi
fi

stage "ok"
