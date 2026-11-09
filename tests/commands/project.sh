# project.sh -- the script of tests/commands/script_tests.cho for `conformance/project.rs` (deleted): docs/package-system.md section 8, the project file, `add`, `install`, `build` and `test` with no files.
# The dependency is a local git repository, so nothing touches a network; each test has a cache directory of its own. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# Every check prints `ok NAME` when it holds and `FAILS NAME` when it does not, so the transcript is the same on every machine.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
root=$(pwd)
work=$2
cd "$work" || exit 1
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1

git() { command git -c user.email=t@example.com -c user.name=t -c commit.gpgsign=false "$@"; }
yes() { if "$@"; then echo "ok $*"; else echo "FAILS $*"; fi; }
has() { if grep -qF -- "$2" "$1"; then echo "ok $3"; else echo "FAILS $3: $(head -c 300 "$1")"; fi; }
hasnt() { if grep -qF -- "$2" "$1"; then echo "FAILS $3"; else echo "ok $3"; fi; }
# `cancho ARGS` in $DIR with the cache $CACHE: stdout in out.txt, stderr in err.txt, the status in $status.
c() {
    (cd "$DIR" && CANCHO_CACHE=$CACHE "$cancho" "$@" > "$work/out.txt" 2> "$work/err.txt")
    status=$?
}
# `c`, which must succeed.
cok() {
    c "$@"
    [ $status = 0 ] || { echo "FAILS cancho $* exited $status: $(cat "$work/err.txt")"; }
}
# `c`, which must fail, saying $WHY on standard error.
cfails() { # WHY ARGS...
    why=$1
    shift
    c "$@"
    if [ $status = 0 ]; then echo "FAILS cancho $* should refuse"; else has "$work/err.txt" "$why" "refuses: $why"; fi
}

BASE='module libx.base;

static table: [int] {
    let t = alloc_slice[static](4, 0);
    t[0] = 10;
    t[1] = 20;
    t[2] = 30;
    t[3] = 40;
    return t;
}

pub fn pick(i: int) -> [] int {
    return table[i];
}
'
TOP='module libx.top;

import libx.base;

pub fn twice(i: int) -> [] int {
    return base.pick(i) * 2;
}
'
APP='import libx.top;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    return top.twice(2);
}
'
PROJECT='[package]
name = "app"

[[bin]]
name = "app"
sources = ["src"]
'
# The project file with the package name $1 and the extra line $2 (empty for none).
project_with() { printf '[package]\nname = "%s"\n%s\n[[bin]]\nname = "app"\nsources = ["src"]\n' "$1" "$2"; }
publish() { # CACHE REPO: publish repo/src into repo/.cancho-vcs
    CANCHO_CACHE=$1 "$cancho" vcs publish --dir "$2/src" --store "$2/.cancho-vcs" > /dev/null || echo "FAILS publish"
}
commit() { git -C "$1" add -A && git -C "$1" commit --quiet -m "$2" && git -C "$1" rev-parse HEAD; }

# A project directory beside a library repository: `src/app.cho` imports `libx.top`. Sets REPO, CACHE, DIR, REV.
project() { # TAG
    base=$work/$1
    REPO=$base/libx
    CACHE=$base/cache
    DIR=$base/app
    mkdir -p "$REPO/src" "$DIR/src"
    printf '%s' "$BASE" > "$REPO/src/base.cho"
    printf '%s' "$TOP" > "$REPO/src/top.cho"
    git -C "$REPO" init --quiet
    publish "$CACHE" "$REPO"
    REV=$(commit "$REPO" libx)
    printf '%s' "$APP" > "$DIR/src/app.cho"
    printf '%s' "$PROJECT" > "$DIR/cancho.toml"
}
add_libx() { cok add libx "$REPO" --rev "$REV" --path .cancho-vcs/libx.top; }
files_in() { ls "$1" 2> /dev/null | wc -l | tr -d ' '; }
code_of() { "$@" > /dev/null 2>&1; echo "program exit $?"; }

echo "== add then build makes a program that uses the dependency"
project e2e
add_libx
has out.txt "added libx" "add says it added"
has out.txt "installed libx" "add says it installed"
has "$DIR/cancho.toml" "[dependencies.libx]" "the dependency is in the file"
has "$DIR/cancho.toml" "$REV" "the pin is in the file"
printf '%s' "$PROJECT" > project.toml
head -c "$(wc -c < project.toml)" "$DIR/cancho.toml" > head.toml
cmp -s project.toml head.toml && echo "ok the file is appended to, what was there is untouched" || echo "FAILS the file was changed"
echo "dependency files: $(files_in "$DIR/build/deps")"
rm -rf "$DIR/build/deps"
cok build
echo "dependency files after build: $(files_in "$DIR/build/deps")"
code_of "$DIR/build/app"

echo "== a ref is resolved once and the hash is what is written"
project ref
branch=$(git -C "$REPO" rev-parse --abbrev-ref HEAD)
cok add libx "$REPO" --ref "$branch" --path .cancho-vcs/libx.top
has "$DIR/cancho.toml" "rev = \"$REV\"" "the hash is written"
hasnt "$DIR/cancho.toml" "\"$branch\"" "the name of the ref is not written"

echo "== add refuses what it should and leaves the file as it was"
project refusals
cp "$DIR/cancho.toml" before.toml
cfails "full commit hash" add libx "$REPO" --rev master
cfails ".cancho-vcs/libx.top" add libx "$REPO" --rev "$REV"
has err.txt "not a store" "a store directory holds stores, not one"
cfails "is not a dependency name" add "a b" "$REPO" --rev "$REV" --path .cancho-vcs/libx.top
c add libx "$REPO" --rev "$REV" --path nope
[ $status != 0 ] && echo "ok a path that is not there is refused" || echo "FAILS a missing path was added"
c add libx "$REPO" --rev "$REV" --ref main
[ $status != 0 ] && echo "ok both --rev and --ref are refused" || echo "FAILS both --rev and --ref were taken"
cmp -s before.toml "$DIR/cancho.toml" && echo "ok the file is as it was" || echo "FAILS the file changed"
add_libx
cp "$DIR/cancho.toml" once.toml
cfails "already has a dependency" add libx "$REPO" --rev "$REV" --path .cancho-vcs/libx.top
cmp -s once.toml "$DIR/cancho.toml" && echo "ok the same name twice changes nothing" || echo "FAILS the file changed"

echo "== an add whose install fails puts the file back and names the dependency"
project rollback
# A store whose source blob was changed after it was published: it is there, the arguments are fine, and the fetch fails -- after the file was written.
blob=$(ls "$REPO"/.cancho-vcs/libx.top/sources/*.cho | head -1)
printf '// changed\n' >> "$blob"
bad=$(commit "$REPO" corrupt)
cp "$DIR/cancho.toml" before.toml
cfails 'dependency `libx`' add libx "$REPO" --rev "$bad" --path .cancho-vcs/libx.top
cmp -s before.toml "$DIR/cancho.toml" && echo "ok the file is put back" || echo "FAILS the file was not put back"
# A commit the repository does not have fails earlier still, and also changes nothing.
c add libx "$REPO" --rev 0123456789abcdef0123456789abcdef01234567 --path .cancho-vcs/libx.top
[ $status != 0 ] && echo "ok a commit the repository lacks is refused" || echo "FAILS an unknown commit was added"
cmp -s before.toml "$DIR/cancho.toml" && echo "ok the file is as it was" || echo "FAILS the file changed"

echo "== a store with nothing in it is not a dependency"
project empty
printf 'module empty;\n' > "$REPO/src/empty.cho"
publish "$CACHE" "$REPO"
empty=$(commit "$REPO" empty)
cp "$DIR/cancho.toml" before.toml
cfails "nothing is published" add nothing "$REPO" --rev "$empty" --path .cancho-vcs/empty
cmp -s before.toml "$DIR/cancho.toml" && echo "ok the file is as it was" || echo "FAILS the file changed"

echo "== a project file that is not one is refused"
project badfiles
refused() { # WHAT TEXT WHY
    printf '%s' "$2" > "$DIR/cancho.toml"
    for command in install build; do
        c "$command"
        if [ $status = 0 ]; then echo "FAILS $1: $command should refuse"; else has err.txt "$3" "$1: $command says $3"; fi
    done
}
refused "an unknown key in a program" "${PROJECT}lex = \"0.1\"
" 'unknown field `lex`'
refused "an unknown section" "${PROJECT}
[extra]
x = 1
" 'unknown field `extra`'
refused "an unknown key in a package" "$(project_with app 'version = "1"')
" 'unknown field `version`'
refused "a dependency rev that is a name" "${PROJECT}[dependencies.x]
git = \"https://example.com/r\"
rev = \"main\"
" '[dependencies.x]'
refused "a dependency path that leaves the repository" "${PROJECT}[dependencies.x]
git = \"https://example.com/r\"
rev = \"$REV\"
path = \"../x\"
" '[dependencies.x]'
refused "a cancho that is not a hash" "$(project_with app 'cancho = "f804ce7"')
" "full commit hash"
refused "two programs of one name" "${PROJECT}[[bin]]
name = \"app\"
sources = [\"src\"]
" "must be unique"
refused "a program with no sources" "$(printf '%s' "$PROJECT" | sed 's/sources = \["src"\]/sources = []/')
" 'has no `sources`'
refused "a bad package name" "$(project_with 'a b' '')
" "package name"

echo "== a compiler other than the one named is refused"
project compiler
other=0000000000000000000000000000000000000000
project_with app "cancho = \"$other\"" > "$DIR/cancho.toml"
for command in install build; do
    c "$command"
    if [ $status = 0 ]; then echo "FAILS $command must refuse a compiler that is not $other"; else
        has err.txt "$other" "$command names the compiler it wants"
        has err.txt "this compiler is" "$command names this compiler"
    fi
done
c install --ignore-compiler-rev
[ $status = 0 ] && echo "ok the escape hatch works for install" || echo "FAILS the escape hatch: $(cat err.txt)"
c build --ignore-compiler-rev
has err.txt "note: this project was written for cancho" "the compiler is only noted"
# The program imports a library this project does not depend on, so the build fails: but on that, with the compiler only noted. (The compiler in cancho says the sentence of the rule, not the Rust compiler's `no module`.)
[ $status != 0 ] && echo "ok and the build fails on the missing library" || echo "FAILS the build of a program that imports a missing library succeeded"
# The compiler's own revision, when it is a clean commit: accepted.
rev=$("$cancho" --version | sed -n 's/.*(rev \([0-9a-f]\{40\}\),.*/\1/p')
if [ -n "$rev" ]; then
    project_with app "cancho = \"$rev\"" > "$DIR/cancho.toml"
    cok install
fi

echo "== install replaces what an older pin fetched"
project restale
add_libx
ls "$DIR/build/deps" > first.txt
# The library moves on; the project moves its pin.
printf '%s' "$BASE" | sed 's/t\[2\] = 30;/t[2] = 31;/' > "$REPO/src/base.cho"
publish "$CACHE" "$REPO"
moved=$(commit "$REPO" moved)
sed "s/$REV/$moved/" "$DIR/cancho.toml" > moved.toml && cp moved.toml "$DIR/cancho.toml"
cok install
echo "dependency files: $(files_in "$DIR/build/deps")"
ls "$DIR/build/deps" > second.txt
[ "$(comm -13 first.txt second.txt | wc -l | tr -d ' ')" = 1 ] && echo "ok the base changed, so one file is new" || echo "FAILS the files did not change as one"
cok build
code_of "$DIR/build/app"

echo "== a project is found from a directory below it and files still build as files"
project walkup
add_libx
(DIR=$DIR/src; cok build)
[ -f "$work/walkup/app/build/app" ] && echo "ok built at the project's root, not the directory it was run from" || echo "FAILS built elsewhere"
printf 'fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(io); release(ffi); release(fs); release(heap); release(args);\n    return 3;\n}\n' > "$DIR/tool.cho"
cok build "$DIR/tool.cho" -o "$DIR/tool"
code_of "$DIR/tool"

echo "== --bin picks one program and an unknown one is refused"
project bin
add_libx
mkdir "$DIR/other"
printf 'fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(io); release(ffi); release(fs); release(heap); release(args);\n    return 7;\n}\n' > "$DIR/other/other.cho"
printf '\n[[bin]]\nname = "other"\nsources = ["other/other.cho"]\nout = "out/other"\n' >> "$DIR/cancho.toml"
cok build --bin other
[ -f "$DIR/out/other" ] && echo "ok the named program is built" || echo "FAILS out/other is missing"
[ -e "$DIR/build/app" ] && echo "FAILS the other program was built too" || echo "ok only the named program is built"
cfails "no [[bin]] named" build --bin nope

echo "== without a project file there is nothing to install"
mkdir none
(DIR=$work/none; CACHE=$work/none/cache; cfails "cancho.toml" install)

# A project with a dependency and a test file that uses it; $1 is what the second test answers.
TEST_FILE='module checks;

import libx.top;

pub fn test_twice_reads_the_table() -> [] int {
    if top.twice(2) == 60 {
        return 0;
    }
    return 1;
}

pub fn test_always_fails_when_asked() -> [] int {
    return FAIL_WITH;
}
'
project_with_tests() { # TAG FAIL
    project "$1"
    add_libx
    mkdir -p "$DIR/tests"
    printf '%s' "$TEST_FILE" | sed "s/FAIL_WITH/$2/" > "$DIR/tests/checks.cho"
    printf '\n[[test]]\nname = "checks"\nsources = ["tests/checks.cho"]\n\n[[test]]\nname = "second"\nsources = ["tests/checks.cho"]\n' >> "$DIR/cancho.toml"
}

echo "== test in a project runs every set against the dependencies"
project_with_tests testpass 0
printf 'import std.math;\n\npub fn test_abs() -> [] int {\n    if math.abs(0 - 3) == 3 {\n        return 0;\n    }\n    return 1;\n}\n' > "$DIR/tests/uses_std.cho"
printf '\n[[test]]\nname = "std"\nsources = ["tests/uses_std.cho"]\nstd = true\n' >> "$DIR/cancho.toml"
rm -rf "$DIR/build/deps"
cok test
has out.txt "== test std" "the std set ran"
has out.txt "== test checks" "the checks set ran"
has out.txt "== test second" "the second set ran"
echo "sets that passed both tests: $(grep -c 'test result: ok. 2 passed' out.txt)"

echo "== a failing set does not stop the others and fails the run"
project_with_tests testfail 1
c test
echo "exit $status"
has out.txt "== test second" "the second set still ran"
has out.txt "test checks.test_always_fails_when_asked ... FAILED" "the failure is named"

echo "== test picks one set and refuses an unknown one or none at all"
project_with_tests testpick 0
cok test --test second
has out.txt "== test second" "the named set ran"
hasnt out.txt "== test checks" "and only that one"
cfails "no [[test]] named" test --test nope
project testnone
cfails "has no [[test]] to run" test

echo "== test with files is still the test runner and a bad test set is refused"
project_with_tests testfiles 0
mkdir "$DIR/elsewhere"
printf 'pub fn test_one() -> [] int {\n    return 0;\n}\n' > "$DIR/elsewhere/alone.cho"
cok test "$DIR/elsewhere/alone.cho"
has out.txt "running 1 test" "the old command ran"
hasnt out.txt "== test" "and not the project's sets"
cp "$DIR/cancho.toml" good.toml
bad_set() { # WHAT TEXT WHY
    { cat good.toml; printf '%b' "$2"; } > "$DIR/cancho.toml"
    cfails "$3" test
}
bad_set "duplicate set names" '\n[[test]]\nname = "checks"\nsources = ["tests"]\n' "must be unique"
bad_set "a set with no sources" '\n[[test]]\nname = "empty"\nsources = []\n' 'has no `sources`'
bad_set "an unknown key" '\n[[test]]\nname = "x"\nsources = ["tests"]\nlink = ["m"]\n' 'unknown field `link`'

# Every package of packages/ names the compiler its sources were checked against (docs/package-system.md section 8.9): one full commit, the same for all of them, and a project file the compiler reads.
echo "== every package names the compiler it was checked against"
pins=
n=0
for dir in "$root"/packages/*/; do
    [ -f "$dir/cancho.toml" ] || continue
    n=$((n + 1))
    pin=$(sed -n 's/^cancho = "\(.*\)"$/\1/p' "$dir/cancho.toml" | head -1)
    case $pin in
        [0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]) ;;
        *) echo "FAILS $dir: '$pin' is not a full commit hash" ;;
    esac
    pins="$pins$pin
"
    (cd "$dir" && CANCHO_CACHE=$work/pkgcache "$cancho" install --ignore-compiler-rev > /dev/null 2> "$work/err.txt") || echo "FAILS install in $dir: $(cat "$work/err.txt")"
done
[ $n -ge 9 ] && echo "ok at least nine packages" || echo "FAILS only $n packages"
echo "distinct pins: $(printf '%s' "$pins" | sort -u | wc -l | tr -d ' ')"
exit 0
