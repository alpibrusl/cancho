# cli_vcs_sync.sh -- the script of tests/commands/script_tests.cho that was `vcs_sync_in_cancho_indexes_a_tree_like_cancho_vcs_sync` of crates/cancho/tests/conformance/selfhost/vcs.rs (docs/gate.md G4, wave 4): `vcs sync` on trees that
# are empty, new, unchanged and changed (a body, a signature, a declaration gone, one added), the directories that are not walked and the files that are not read, a declaration made twice (alike, and with two
# identities), a file that does not parse, the library (`--std`), an index with lines that are not entries, a directory that is not there, every command line the Rust command refuses, and every source of the 26
# stores of packages/ synced, changed and synced again. The status, the output (a warning about a file that does not parse is only compared as a warning), and every file left (the index and the operations) are
# compared. NAME.expected is what the Rust command did, recorded on 2026-11-17 (see tests/commands/vcs_diff_lib.sh). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_diff_lib.sh

# The sources the cases are made of.
k=$work/k
mkdir -p "$k"
printf 'module lib.one;\n\npub fn same(n: int) -> [] int {\n    return n;\n}\n\npub fn twice(n: int) -> [] int {\n    return n + n;\n}\n' > "$k/one.cho"
printf 'module lib.two;\n\npub fn plus(a: int, b: int) -> [] int {\n    return a + b;\n}\n' > "$k/two.cho"
printf 'module lib.two;\n\npub fn minus(a: int, b: int) -> [] int {\n    return a - b;\n}\n' > "$k/other_two.cho"
sed 's/return n + n;/return n * 2;/' "$k/one.cho" > "$k/changed.cho"
sed 's/twice(n: int)/twice(n: int, m: int)/' "$k/one.cho" > "$k/signed.cho"
sed 's/lib\.one/lib.three/' "$k/one.cho" > "$k/three.cho"
printf 'module lib.hid;\n\npub fn h(n: int) -> [] int {\n    return n;\n}\n' > "$k/hid.cho"
printf 'module lib.dot;\n\npub fn d(n: int) -> [] int {\n    return n;\n}\n' > "$k/dot.cho"
printf 'module lib.z;\n\npub fn z(n: int) -> [] int {\n    return n;\n}\n' > "$k/z.cho"
printf 'module lib.y;\n\npub fn y(n: int) -> [] int {\n    return n;\n}\n' > "$k/y.cho"
printf 'fn helper(n: int) -> [] int {\n    return n;\n}\n' > "$k/helper1.cho"
printf 'fn helper(n: int) -> [] int {\n    return n + 1;\n}\n' > "$k/helper2.cho"
printf 'fn helper(n: int) -> [] int {\n    return n;\n}\n\nfn main() -> [] int {\n    return 0;\n}\n' > "$k/root.cho"
printf 'module lib.s;\n\nimport std.bytes;\n\npub fn same[&a, &b](x: &a [byte], y: &b [byte]) -> [] bool {\n    return bytes.equal(x, y);\n}\n' > "$k/lib_s.cho"
for f in changed signed; do cmp -s "$k/one.cho" "$k/$f.cho" && fail "$f.cho is the same as one.cho"; done
# tree: the tree most cases start with.
tree() { putf a.cho "$k/one.cho"; putf sub/b.cho "$k/two.cho"; }
sy() { vstep sync "$@"; }

begin vcs_sync_in_cancho_indexes_a_tree_like_cancho_vcs_sync
dcase "an empty directory"; sy; sy; sy --check; dend
dcase "new, again, checked"; tree; sy; sy; sy --check; dend
dcase "checked before the first"; tree; sy --check; dend
dcase "a body changed"; tree; sy; putf a.cho "$k/changed.cho"; sy --check; sy; sy; dend
dcase "a signature changed"; tree; sy; putf a.cho "$k/signed.cho"; sy --check; sy; sy; dend
dcase "a declaration gone and one added"; tree; sy; putf sub/b.cho "$k/other_two.cho"; sy; sy --check; dend
dcase "everything changed at once"; tree; sy; putf a.cho "$k/changed.cho"; putf sub/b.cho "$k/other_two.cho"; putf c.cho "$k/three.cho"; sy; dend
dcase "what is walked"
putf a.cho "$k/one.cho"; putf .git/g.cho "$k/two.cho"; putf target/t.cho "$k/two.cho"; putf .cancho/c.cho "$k/two.cho"; putf .hid/h.cho "$k/hid.cho"; putf .h.cho "$k/dot.cho"
putf .cho "$k/two.cho"; putf x.CHO "$k/two.cho"; putf notes.txt "$k/two.cho"; putf a/z.cho "$k/z.cho"; putf a.d/y.cho "$k/y.cho"
sy; dend
dcase "links are not followed"; putf a.cho "$k/one.cho"; putf real/b.cho "$k/two.cho"; link file.cho a.cho; link dir real; link gone.cho nowhere.cho; sy; dend
dcase "the root made twice, and not alike"; putf r.cho "$k/helper1.cho"; putf s.cho "$k/helper2.cho"; sy; dend
dcase "no module, the root"; putf r.cho "$k/root.cho"; sy; sy --check; dend
dcase "a declaration made twice, alike"; putf a.cho "$k/one.cho"; putf b.cho "$k/one.cho"; sy; sy --check; dend
dcase "a declaration made twice, and not alike"; putf a.cho "$k/one.cho"; putf b.cho "$k/changed.cho"; sy; sy --check; dend
dcase "a file that does not parse"; putf a.cho "$k/one.cho"; put b.cho 'fn broken('; putf c.cho "$k/two.cho"; sy; sy --check; dend
dcase "a collision and a file that does not parse"; putf a.cho "$k/one.cho"; put b.cho 'fn broken('; putf c.cho "$k/changed.cho"; putf d.cho "$k/signed.cho"; sy; dend
dcase "the library"; putf a.cho "$k/lib_s.cho"; sy --std; sy --std; sy; sy --std --check; dend
dcase "an index with lines that are not entries"
putf a.cho "$k/one.cho"
a=$(rep a 64); b=$(rep b 64)
printf 'garbage\nfn lib.one same @A@ @B@\r\nfn lib.one same @B@ @A@\nfn  root @A@ @B@\nfn lib.one short @A@\nfn lib.x  @A@ @B@\nfn lib.x name @A@ @B@ extra\nfn lib.old gone @A@ @B@\nfn \n\n' | sed -e "s/@A@/$a/g" -e "s/@B@/$b/g" > "$work/index-garbage.txt"
putf .cancho/ids.txt "$work/index-garbage.txt"
sy --check; sy; sy --check; dend
dcase "--dir"; tree; sy --dir sub; sy --dir sub/; sy --dir sub --check; dend
dcase "a directory that is not there"; tree; sy --dir nowhere; dend
dcase "a directory that is a file"; tree; sy --dir a.cho; dend
dcase "an option"; tree; sy --bogus; dend
dcase "a word"; tree; sy a.cho; dend
dcase "--dir last"; tree; sy --dir; dend

# An index made by the command for the first source, with its last line repeated after a wrong one, and with every line ending in `\r\n`.
dcase "the index the command makes"; putf a.cho "$k/one.cho"; vrun sync; [ "$code" = 0 ] || fail "sync of the first source exited $code"
cp "$d/.cancho/ids.txt" "$work/index-real.txt"
awk -v dd="$(rep d 64)" '{ last = $0 } END { n = split(last, p, " "); printf "%s %s %s %s %s\n", p[1], p[2], p[3], p[4], dd }' "$work/index-real.txt" > "$work/index-repeated.txt"
cat "$work/index-real.txt" >> "$work/index-repeated.txt"
awk '{ printf "%s\r\n", $0 }' "$work/index-real.txt" > "$work/index-carriage.txt"
dcase "a key twice in the index, the last right"; putf a.cho "$k/one.cho"; putf .cancho/ids.txt "$work/index-repeated.txt"; sy --check; dend
dcase "an index with lines ending in \\r\\n"; putf a.cho "$k/one.cho"; putf .cancho/ids.txt "$work/index-carriage.txt"; sy --check; dend

# Every source of the 26 stores of packages/, synced, changed and synced again.
dcase "all of packages"
n=0; first=
for rel in $(stores_of_packages); do
    for blob in $(ls "$packages/$rel/sources" | LC_ALL=C sort); do
        nn=$(printf '%02d' "$n")
        putf "src/$nn/$blob" "$packages/$rel/sources/$blob"
        [ -n "$first" ] || first=$nn/$blob
    done
    n=$((n + 1))
done
[ -n "$first" ] || fail "no source in packages/"
sy --dir src
sy --dir src --check
{ cat "$d/src/$first"; printf '\npub fn extra_for_sync(n: int) -> [] int {\n    return n;\n}\n'; } > "$work/edited.cho"
putf "src/$first" "$work/edited.cho"
sy --dir src
sy --dir src --check
dend 'src/[0-9][0-9]' src/.cancho/ops
end
