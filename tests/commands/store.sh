# store.sh -- the script of tests/commands/script_tests.cho for the four tests of conformance/selfhost/store.rs (deleted; docs/gate.md, G4 wave 4): the package stores fetched, refused, installed and
# added by the compiler in cancho (`cancho install`, `cancho add`: examples/selfhost/clistore.cho, cliorigin.cho) as the Rust `cancho install` and `cancho add` did. $1 is the compiler, $2 a scratch
# directory; run from the root of the repository.
#
# Each case builds a git repository (`file://`, one commit, the date and the author fixed) that holds a copy of `packages/` or of a changed copy of `packages/net-sockets`, a project that depends on
# a store in it, and runs the command. tests/vectors/store.txt is what the Rust `cancho` did with the same cases, a line a fact: `CASE KEY VALUE` (`status`, `ok`, `out`, `err1` the first line
# of the standard error, `toml` the project file afterwards, `files` the count and sums of what is in `build/deps`, `built`), the work directory, the project directory and every commit hash
# written as WORK, DIR and REV, a newline as `|`. The recording is this script with the Rust `cancho`:
#     ORACLE=rust RECORD_TO=tests/vectors/store.txt sh tests/commands/store.sh target/release/cancho DIR
# It pins `packages/` as it was when recorded: the stores of `net-sockets`, `http-response`, `x509` and `tls` (their sources are named by their hashes), and the sources of `net-sockets`.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
root=$(pwd)
oracle=${ORACLE:-}
vector=$root/tests/vectors/store.txt
got=$work/got.txt
mkdir -p "$work"
: > "$got"
: > "$work/norm.sed"
project=$work/project
# The directories as the compiler under test reports them: the physical path (a scratch directory under /tmp is /private/tmp on macOS).
physical=$(cd "$work" && pwd -P)
cache=$work/cache

g() { git -c user.email=a@b -c user.name=n -c commit.gpgsign=false "$@"; }
export GIT_AUTHOR_DATE=2020-01-01T00:00:00Z GIT_COMMITTER_DATE=2020-01-01T00:00:00Z

# repository NAME TREE: $work/NAME is a repository with one commit that holds the directory TREE as `packages`; leaves $url and $rev.
repository() {
    _repo=$work/$1
    rm -rf "$_repo"
    mkdir -p "$_repo/packages"
    cp -R "$2/." "$_repo/packages/"
    (cd "$_repo" && g init --quiet . && g add -A && g commit --quiet -m stores) > /dev/null 2>&1
    rev=$(cd "$_repo" && git rev-parse HEAD)
    url=file://$_repo
    echo "s|$rev|REV|g" >> "$work/norm.sed"
}

# norm: the standard input with the names of this run taken out, and the lines that are the compiler's own noise.
norm() {
    grep -v -e '^building the compiler in cancho' -e '^warning: building with the Rust compiler' -e '^ld: warning:' | sed -e "s|$physical/project|DIR|g" -e "s|$project|DIR|g" -e "s|$physical|WORK|g" -e "s|$work|WORK|g" -f "$work/norm.sed" | tr '\n' '|'
    echo
}

# fresh_project: an empty $project and an empty cache.
fresh_project() { rm -rf "$project" "$cache"; mkdir -p "$project"; }

# tool ARGS...: the command in $project; leaves $code, $stdout, $stderr (files of $work) and the first line of the error in $err1.
tool() {
    if [ -n "$oracle" ]; then
        (cd "$project" && CANCHO_COMPILER=rust CANCHO_CACHE=$cache "$cancho" "$@" --ignore-compiler-rev < /dev/null > "$work/tool.out" 2> "$work/tool.err")
    else
        (cd "$project" && "$cancho" "$@" --cache "$cache" < /dev/null > "$work/tool.out" 2> "$work/tool.err")
    fi
    code=$?
    err1=$(grep -v -e '^building the compiler in cancho' -e '^warning: building with the Rust compiler' -e '^ld: warning:' "$work/tool.err" | head -1 | norm)
}

# emit CASE KEY VALUE: a fact of this run.
emit() { printf '%s\t%s\t=%s\n' "$1" "$2" "$3" >> "$got"; }
# deps: what `build/deps` holds, as `COUNT SUM-OF-THE-NAMES SUM-OF-THE-BYTES`.
deps() {
    if [ -d "$project/build/deps" ] && [ -n "$(ls -A "$project/build/deps")" ]; then
        (cd "$project/build/deps" && echo "$(ls | wc -l | tr -d ' ') $(ls | cksum | tr ' ' -) $(cat $(ls) | cksum | tr ' ' -)")
    else
        echo "0"
    fi
}
# facts CASE: the facts of the last `tool`.
facts() {
    emit "$1" status "$code"
    emit "$1" out "$(norm < "$work/tool.out")"
    emit "$1" err1 "$err1"
    emit "$1" toml "$(cat "$project/cancho.toml" 2> /dev/null | norm)"
    emit "$1" files "$(deps)"
    emit "$1" built "$([ -f "$project/build/a" ] && echo yes || echo no)"
}

# A project that depends on the store at PATH of the repository ($url, $rev).
fetch_project() {
    fresh_project
    printf '[package]\nname = "p"\n\n[dependencies.dep]\ngit = "%s"\nrev = "%s"\npath = "%s"\n' "$url" "$rev" "$1" > "$project/cancho.toml"
}
# A project with a program and a test that import `net.sockets`, and the dependency `socks` at PATH.
program_project() {
    fresh_project
    mkdir -p "$project/src" "$project/tests"
    printf '[package]\nname = "p"\n\n[dependencies.socks]\ngit = "%s"\nrev = "%s"\npath = "%s"\n\n[[bin]]\nname = "a"\nsources = ["src"]\n\n[[test]]\nname = "t"\nsources = ["tests"]\n' "$url" "$rev" "$1" > "$project/cancho.toml"
    cat > "$project/src/main.cho" <<'EOF'
edition 7;
import net.sockets;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi);
    release(net);
    release(signals);
    release(clock);
    release(exec);
    release(fs);
    release(args);
    release(heap);
    release(io);
    return 0;
}
EOF
    printf 'edition 7;\nimport net.sockets;\nfn test_one() -> [] int { return 0; }\n' > "$project/tests/t.cho"
}

# want CASE KEY: what the vector says; same_as_rust CASE KEY...: the facts of the run are the vector's.
want() { awk -F'\t' -v c="$1" -v k="$2" '$1 == c && $2 == k { print $3; exit }' "$vector"; }
have() { awk -F'\t' -v c="$1" -v k="$2" '$1 == c && $2 == k { print $3; exit }' "$got"; }
same_as_rust() {
    _case=$1
    shift
    for _key in "$@"; do
        [ -n "$(want "$_case" "$_key")" ] || { fail "$_case: the vector has no $_key"; continue; }
        same "$(have "$_case" "$_key")" "$(want "$_case" "$_key")" "$_case: $_key"
    done
}
# ok_as_rust CASE: both fetched, or both refused, and the same files where it worked.
ok_as_rust() {
    _case=$1
    emit "$_case" ok "$([ "$code" = 0 ] && echo 1 || echo 0)"
    emit "$_case" files "$(deps)"
    same "$(have "$_case" ok)" "$(want "$_case" ok)" "$_case: whether the store holds"
    [ "$(want "$_case" ok)" = "=0" ] || same "$(have "$_case" files)" "$(want "$_case" files)" "$_case: the files"
}

packages=$root/packages
all=$work/alltree
rm -rf "$all"
cp -R "$packages" "$all"

# 1. The stores of packages/ fetched: no dependency, one dependency, a dependency chain that imports `std`, and a store of four sources.
begin the_stores_of_packages_are_fetched_in_cancho_as_cancho_install_fetches_them
repository all "$all"
for path in net-sockets/.cancho-vcs http-response/.cancho-vcs x509/.cancho-vcs/x509_verify tls/.cancho-vcs/tls_hello; do
    fetch_project "packages/$path"
    tool install
    ok_as_rust "fetch $path"
    [ "$code" = 0 ] || fail "$path: install said $err1"
    [ "$(ls "$project/build/deps" 2> /dev/null | wc -l | tr -d ' ')" -gt 0 ] || fail "$path: nothing was fetched"
done
end

# 2. A store that does not hold: a copy of packages/net-sockets (no dependency, one source) changed in one way each, and fetched.
base=$work/base
rm -rf "$base"
mkdir -p "$base"
cp -R "$packages/net-sockets" "$base/net-sockets"
store=net-sockets/.cancho-vcs
blob=$(ls "$base/$store/sources" | head -1)
first_name=$(awk -F'"' '/"name": "/ { print $4; exit }' "$base/$store/manifest.json")
first_sig=$(awk -F'"' '/^    "[0-9a-f]+": \{$/ { print $2; exit }' "$base/$store/manifest.json")
zeros=0000000000000000000000000000000000000000000000000000000000000000
effs=ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
# change NAME: the store changed as the case says.
change() {
    _s=$work/tree/$store
    case $1 in
        "as it is") ;;
        "a blob that is not what its name says") printf '\n// changed\n' >> "$_s/sources/$blob" ;;
        "a blob that is not there") rm -f "$_s/sources/$blob" ;;
        "a body that is not the one the manifest says")
            awk -v name="$first_name" -v new="$zeros" '
                !armed && index($0, "\"name\": \"" name "\"") { armed = 1 }
                armed && !done && match($0, "\"stage_id\": \"[0-9a-f]+\"") { $0 = substr($0, 1, RSTART - 1) "\"stage_id\": \"" new "\"" substr($0, RSTART + RLENGTH); done = 1 }
                { print }' "$_s/manifest.json" > "$_s/manifest.new" && mv "$_s/manifest.new" "$_s/manifest.json" ;;
        "a signature that its source does not produce")
            awk -v sig="$first_sig" -v new="$effs" '
                !done && index($0, "\"" sig "\": {") { sub(sig, new); done = 1 }
                { print }' "$_s/manifest.json" > "$_s/manifest.new" && mv "$_s/manifest.new" "$_s/manifest.json" ;;
        "a name published twice, the signature that sorts last being no longer produced")
            awk -v sig="$first_sig" -v new="$effs" '
                index($0, "\"" sig "\": {") && !copying { copying = 1 }
                copying { block = block $0 "\n" }
                { print }
                copying && /^    \},$/ { copying = 0; sub(sig, new, block); printf "%s", block; block = "" }' "$_s/manifest.json" > "$_s/manifest.new" && mv "$_s/manifest.new" "$_s/manifest.json" ;;
        "a manifest that is not JSON") printf '{' > "$_s/manifest.json" ;;
        "a manifest with nothing in it") printf '{"entries": {}}' > "$_s/manifest.json" ;;
        "a store that requires itself")
            _stage=$(awk -F'"' -v name="$first_name" 'index($0, "\"name\": \"" name "\"") { armed = 1 } armed && /"stage_id"/ { print $4; exit }' "$_s/manifest.json")
            _src=$(awk -F'"' -v name="$first_name" 'index($0, "\"name\": \"" name "\"") { armed = 1 } armed && /"source_hash"/ { print $4; exit }' "$_s/manifest.json")
            mkdir -p "$_s/requires"
            printf '{"store":".","lock":{"entries":{"%s":{"sig_id":"%s","stage_id":"%s","source_hash":"%s"}}}}' "$first_name" "$first_sig" "$_stage" "$_src" > "$_s/requires/0.json" ;;
    esac
}
begin a_store_that_does_not_hold_is_refused_in_cancho_as_cancho_install_refuses_it
fetched=0
for what in "as it is" "a blob that is not what its name says" "a blob that is not there" "a body that is not the one the manifest says" "a signature that its source does not produce" "a name published twice, the signature that sorts last being no longer produced" "a manifest that is not JSON" "a manifest with nothing in it" "a store that requires itself"; do
    rm -rf "$work/tree"
    mkdir -p "$work/tree"
    cp -R "$base/net-sockets" "$work/tree/net-sockets"
    change "$what"
    repository repository "$work/tree"
    fetch_project packages/net-sockets/.cancho-vcs
    tool install
    ok_as_rust "bad store: $what"
    [ "$code" != 0 ] || fetched=$((fetched + 1))
done
same "$fetched" 1 "only the unchanged store is fetched"
end

# 3. install, build and test of a project with a dependency.
begin the_command_line_in_cancho_installs_dependencies_like_cancho
rm -rf "$work/tree"
mkdir -p "$work/tree"
cp -R "$packages/net-sockets" "$work/tree/net-sockets"
repository good "$work/tree"
good_url=$url
good_rev=$rev
rm -rf "$work/empty-tree"
cp -R "$work/tree" "$work/empty-tree"
printf '{"entries": {}}' > "$work/empty-tree/net-sockets/.cancho-vcs/manifest.json"
repository empty "$work/empty-tree"
empty_url=$url
empty_rev=$rev
path=packages/net-sockets/.cancho-vcs
# install CASE URL REV PATH COMMAND [older]
dependency_case() {
    url=$2; rev=$3
    program_project "$4"
    if [ -n "$6" ]; then
        mkdir -p "$project/build/deps"
        echo "// an old pin" > "$project/build/deps/old.cho"
    fi
    tool "$5"
    facts "dependency: $1"
    same_as_rust "dependency: $1" status out err1 files built
    if [ -z "$oracle" ] && [ "$code" = 0 ] && [ "$5" = install ]; then
        [ -d "$cache/git" ] || fail "$1: --cache is where the checkout went"
    fi
}
dependency_case "install" "$good_url" "$good_rev" "$path" install ""
dependency_case "install over a file an older install left" "$good_url" "$good_rev" "$path" install older
dependency_case "build" "$good_url" "$good_rev" "$path" build ""
dependency_case "test" "$good_url" "$good_rev" "$path" test ""
dependency_case "a directory that is not in the commit" "$good_url" "$good_rev" packages/nowhere install ""
dependency_case "a store with nothing in it" "$empty_url" "$empty_rev" "$path" install ""
end

# 4. add: each case in a project of its own with a cancho.toml that has a comment in it.
begin the_command_line_in_cancho_adds_dependencies_like_cancho
rm -rf "$work/tree"
mkdir -p "$work/tree"
cp -R "$packages/net-sockets" "$work/tree/net-sockets"
# Two stores one level down, for the message that says where they are.
mkdir -p "$work/tree/stores"
cp -R "$work/tree/net-sockets/.cancho-vcs" "$work/tree/stores/a"
cp -R "$work/tree/net-sockets/.cancho-vcs" "$work/tree/stores/b"
repository good "$work/tree"
(cd "$work/good" && g tag v1)
good_url=$url
good_rev=$rev
rm -rf "$work/empty-tree"
cp -R "$work/tree" "$work/empty-tree"
printf '{"entries": {}}' > "$work/empty-tree/net-sockets/.cancho-vcs/manifest.json"
repository empty "$work/empty-tree"
empty_url=$url
empty_rev=$rev
# add_case NAME HAS_PROJECT ADD_FIRST WORDS...
add_case() {
    _name=$1
    _has=$2
    _first=$3
    shift 3
    fresh_project
    [ "$_has" = yes ] && printf '# a comment\n[package]\nname = "p"' > "$project/cancho.toml"
    if [ "$_first" = yes ]; then
        tool add "$@"
        [ "$code" = 0 ] || fail "$_name: the first add said $err1"
    fi
    tool add "$@"
    facts "add: $_name"
    same_as_rust "add: $_name" status out err1 toml files
}
add_case "a revision" yes no socks "$good_url" --rev "$good_rev" --path "$path"
add_case "a tag" yes no socks "$good_url" --ref v1 --path "$path"
add_case "the head" yes no socks "$good_url" --path "$path"
add_case "a name taken" yes yes socks "$good_url" --rev "$good_rev" --path "$path"
add_case "a directory that is not in the commit" yes no socks "$good_url" --path nowhere
add_case "a directory that is not a store" yes no socks "$good_url" --path packages/stores
add_case "a revision that is not a hash" yes no socks "$good_url" --rev abc
add_case "a ref that is not there" yes no socks "$good_url" --ref nope --path "$path"
add_case "a store with nothing in it" yes no socks "$empty_url" --rev "$empty_rev" --path "$path"
add_case "a url with a quote in it" yes no socks 'file:///x"y' --rev "$good_rev"
add_case "a path with a backslash in it" yes no socks "$good_url" --rev "$good_rev" --path 'a\b'
add_case "--rev and --ref" yes no socks "$good_url" --rev "$good_rev" --ref v1
add_case "a name that is not one" yes no "a b" "$good_url"
add_case "no url" yes no socks
add_case "an option that is not one" yes no socks "$good_url" --nope
add_case "no project" no no socks "$good_url" --rev "$good_rev" --path "$path"
end

if [ -n "$oracle" ]; then cp "$got" "${RECORD_TO:-/dev/null}"; fi
rm -rf "$all" "$work/tree" "$work/empty-tree" "$work/base"
