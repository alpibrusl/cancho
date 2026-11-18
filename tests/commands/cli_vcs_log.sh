# cli_vcs_log.sh -- the script of tests/commands/script_tests.cho that was `vcs_log_in_cancho_lists_a_store_like_cancho_vcs_log` of crates/cancho/tests/conformance/selfhost/vcs.rs (docs/gate.md G4, wave 4): `vcs log` on every
# store of packages/ (the 26, and the stores inside the ones that hold several), and on stores made for what they do not have: a name given twice, one longer than the 24 columns, one of two bytes to a character,
# a `uses_std` that is false and one that is left out, no entries, no manifest, a manifest that is not JSON and one whose fields are not text. NAME.expected is what the Rust command did, recorded on 2026-11-17
# (see tests/commands/vcs_diff_lib.sh). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_diff_lib.sh

begin vcs_log_in_cancho_lists_a_store_like_cancho_vcs_log
rows=0; std_rows=0; stores=0
for rel in $(stores_of_packages); do
    stores=$((stores + 1))
    dcase "store $rel"
    vstep log --store "$packages/$rel"
    rows=$((rows + $(wc -l < "$work/s.out")))
    std_rows=$((std_rows + $(grep -c '  std$' "$work/s.out")))
done
[ "$stores" -ge 26 ] || fail "only $stores stores in packages/"
{ [ "$rows" -gt 900 ] && [ "$std_rows" -gt 100 ]; } || fail "$rows rows, $std_rows with std"

dcase "the stores made for the case"
mkdir -p "$d/made" "$d/x" "$d/made/text" "$d/made/wrong" "$d/made/empty"
{ ent f zeta true; ent 1 same ""; ent 2 same true; ent 3 a_name_that_is_longer_than_twenty_four_bytes false; ent 0 Alpha ""; ent e é ""; ent d ü_and_more true; } | store_with "$d/made/names"
store_with "$d/made/none" < /dev/null
printf '{\n' > "$d/made/text/manifest.json"
printf '{"entries":{"a":{"name":1}}}' > "$d/made/wrong/manifest.json"
for c in "log --store made/names" "log --store made/none" "log --store made/empty" "log --store made/absent" "log" "log --std --store made/names" \
    "log --dir x --store made/names" "log --requires a:b --store made/names" "log --store" "log --dir" "log --requires" "log x.cho" "log --bogus"; do
    echo "== $c"
    # shellcheck disable=SC2086
    vstep $c
done
for badstore in made/text made/wrong; do
    echo "== $badstore"
    vstep log --store "$badstore"
    [ "$code" = 3 ] || fail "$badstore: exit $code, wanted 3"
    case $(head -n 1 "$work/s.err") in
        "cannot read manifest at \`$badstore\`: malformed manifest"*) ;;
        *) fail "$badstore: the error is '$(head -n 1 "$work/s.err")'" ;;
    esac
done
end
