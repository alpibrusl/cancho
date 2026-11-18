# cli_vcs_publish.sh -- the script of tests/commands/script_tests.cho that was `vcs_publish_in_cancho_logs_a_file_like_cancho_vcs_publish` of crates/cancho/tests/conformance/selfhost/vcs.rs (docs/gate.md G4, wave 4):
# `vcs publish` of four stores of packages/ again from their sources into a store of the case, against the stores they require, without `--std` and then with it; then the source of `net-sockets` changed in
# the ways a publish tells apart (a comment, a body, a new function, a new effect row, a second module, a file that does not parse, one that is refused, one that imports `std`), and the command lines the Rust
# command refuses. What is compared is the status, the output, the first line of the error (not for a program that is refused) and every file of the store. NAME.expected is what the Rust command did, recorded on
# 2026-11-17 (see tests/commands/vcs_diff_lib.sh). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_diff_lib.sh

# blob REL: the first source of the store packages/REL.
blob() { printf '%s/%s' "$packages/$1/sources" "$(ls "$packages/$1/sources" | LC_ALL=C sort | head -n 1)"; }
# requires_of REL: the `--requires LOCK:STORE` arguments the store packages/REL was published with: each requirement it records, its lock written to $work/locks and the store it names found (a path
# relative to the store, made absolute and lexical: `..` made out, no link followed).
requires_of() {
    _store=$packages/$1
    [ -d "$_store/requires" ] || return 0
    mkdir -p "$work/locks"
    _i=0
    for _n in $(ls "$_store/requires" | sed 's/\.json$//' | sort -n); do
        _f=$_store/requires/$_n.json
        _lock=$work/locks/$(printf '%s' "$1" | tr / _)-$_i.lock
        awk 'f { lines[++n] = $0; next } /"lock":/ { f = 1; sub(/^.*"lock":[ \t]*/, ""); lines[++n] = $0 }
             END { for (i = 1; i < n; i++) printf "%s%s", lines[i], (i < n - 1 ? "\n" : "") }' "$_f" > "$_lock"
        _given=$(sed -n 's/.*"store": "\([^"]*\)".*/\1/p' "$_f" | head -n 1)
        if [ -z "$_given" ]; then
            printf ' --requires %s' "$_lock"
        else
            _found=$(printf '%s/%s' "$_store" "$_given" | awk -F/ '{ n = 0; for (i = 1; i <= NF; i++) { if ($i == "" || $i == ".") continue; if ($i == "..") { if (n > 0) n--; continue } p[++n] = $i } s = ""; for (i = 1; i <= n; i++) s = s "/" p[i]; print s }')
            printf ' --requires %s:%s' "$_lock" "$_found"
        fi
        _i=$((_i + 1))
    done
}

orig=$(blob net-sockets/.cancho-vcs)
# putmore PATH TEXT: the source of net-sockets and a blank line, TEXT and a newline.
putmore() { mkdir -p "$(dirname "$d/$1")"; { cat "$orig"; printf '\n%s\n' "$2"; } > "$d/$1"; }
pub() { vstep publish --store st "$1"; }

begin vcs_publish_in_cancho_logs_a_file_like_cancho_vcs_publish
# The stores of packages/, from their sources.
for rel in net-sockets/.cancho-vcs http-response/.cancho-vcs x509/.cancho-vcs/x509_verify tls/.cancho-vcs/tls_hello; do
    dcase "$rel"
    src=$(blob "$rel")
    reqs=$(requires_of "$rel")
    # shellcheck disable=SC2086
    vstep publish --store "$d/st" $reqs "$src"
    # shellcheck disable=SC2086
    vstep publish --std --store "$d/st" $reqs "$src"
    dend
done
# The source of net-sockets changed.
dcase "again, and a comment"; putf a.cho "$orig"; pub a.cho; pub a.cho; putmore a.cho '// a comment, nothing else'; pub a.cho; dend
dcase "a body changed"; putf a.cho "$orig"; pub a.cho
sed 's/return at + len(src);/return at + len(src) + 0;/' "$orig" > "$d/a.cho"
cmp -s "$orig" "$d/a.cho" && fail "the body of net-sockets has no 'return at + len(src);' to change"
pub a.cho; dend
dcase "a new function"; putf a.cho "$orig"; pub a.cho; putmore a.cho "$(printf 'pub fn twice(n: int) -> [] int {\n    return n + n;\n}')"; pub a.cho; pub a.cho; dend
dcase "a new effect row"; putmore a.cho "$(printf 'pub fn kept[&h](heap: &!h Heap) -> [heap] int {\n    let area = box_slice(heap, 4, 0);\n    unbox_slice(heap, area);\n    return 0;\n}')"; pub a.cho; dend
dcase "a second module in the store"; putf a.cho "$orig"; pub a.cho; putp b.cho 'module lib.other;\n\npub fn same(n: int) -> [] int {\n    return n;\n}\n'; pub b.cho; dend
dcase "a file that does not parse"; putmore a.cho 'fn broken('; pub a.cho; dend
dcase "a file that is refused"; putmore a.cho "$(printf 'pub fn bad() -> [] int {\n    return missing_name;\n}')"; pub a.cho; dend
dcase "a file that imports std"
putp a.cho 'module lib.s;\n\nimport std.bytes;\n\npub fn same[&a, &b](x: &a [byte], y: &b [byte]) -> [] bool {\n    return bytes.equal(x, y);\n}\n'
pub a.cho; vstep publish --std --store st a.cho; vstep publish --std --store st a.cho; dend
dcase "the default store"; putf a.cho "$orig"; vstep publish a.cho; vstep publish a.cho; dend
dcase "a store that is a file"; put st "not a directory"; putf a.cho "$orig"; pub a.cho; dend
dcase "a manifest that is not JSON"; putf a.cho "$orig"; put st/manifest.json '{'; pub a.cho; dend
dcase "no file"; putf a.cho "$orig"; vstep publish; dend
dcase "a file that is not there"; putf a.cho "$orig"; pub absent.cho; dend
dcase "two files"; putf a.cho "$orig"; vstep publish a.cho b.cho; dend
dcase "--dir with a file"; putf a.cho "$orig"; vstep publish --dir d a.cho; dend
dcase "--dir with --requires"; putf a.cho "$orig"; vstep publish --dir d --requires x; dend
dcase "a lock that is not there, no store"; putf a.cho "$orig"; vstep publish --requires absent.lock a.cho; dend
dcase "a lock that is not there, a store"; putf a.cho "$orig"; vstep publish --requires absent.lock:st2 a.cho; dend
dcase "an option"; putf a.cho "$orig"; vstep publish --bogus; dend
dcase "--store last"; putf a.cho "$orig"; vstep publish --store; dend
dcase "--dir last"; putf a.cho "$orig"; vstep publish --dir; dend
dcase "--requires last"; putf a.cho "$orig"; vstep publish --requires; dend
# A store that required another, published again with no requirement: the old ones go. And a program whose own row has two labels.
net_store=$packages/net-sockets/.cancho-vcs
mklock net.lock --store "$net_store" --all
dcase "requirements dropped"
putp a.cho 'module lib.t;\n\npub fn one(n: int) -> [] int {\n    return n;\n}\n'
putf n.lock "$work/net.lock"
vstep publish --store st --requires "n.lock:$net_store" a.cho
pub a.cho
dend
dcase "a row of two labels"
{ cat "$orig"; printf '\npub fn two[&h, &i](heap: &!h Heap, io: &!i Io) -> [io_write, heap] int {\n    let area = box_slice(heap, 4, 0);\n    unbox_slice(heap, area);\n    putchar(io, 65);\n    return 0;\n}\n'; } > "$d/a.cho"
pub a.cho
dend
end
