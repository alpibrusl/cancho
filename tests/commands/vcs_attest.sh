# vcs_attest.sh -- the script of tests/commands/script_tests.cho for the attestations of `cancho vcs` (docs/vcs-cli.md section 11; compiler/vcsattest.cho, compiler/clivcsmore.cho): `attest`, `attestations`,
# `keys new|list|trust|require|sign`, `record --key`, and what `verify` says of a chain: every document pinned (ids, signatures and the compiler's name made `<id>`, `<signature>` and `<compiler>`, since
# a record names the compiler that wrote it), every refusal tag reached (`chain-broken` four ways, `unsigned-change`, `key-not-found`, `key-exists`, `gate-failed`, `bad-argument`).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
root=$(pwd)
. tests/commands/script_lib.sh

AUTHOR='Ada <ada@example.com>'
# RFC 8032 section 7.1, test 1: the secret key, and the public key it must give.
SEED=9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60
PUBLIC=d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a

# `rv ARGS...`: `cancho vcs ARGS --output json` in the current directory: the document in $work/o.json, the exit status in $code.
rv() {
    case $1 in
        record | revert) "$cancho" vcs "$@" --author "$AUTHOR" --output json > "$work/o.json" 2> "$work/e.txt" ;;
        *) "$cancho" vcs "$@" --output json > "$work/o.json" 2> "$work/e.txt" ;;
    esac
    code=$?
}
# jf KEY: the value of the first `"KEY": value` of the document, without quotes.
jf() { sed -n "s/^ *\"$1\": \"\{0,1\}\([^\",]*\)\"\{0,1\},\{0,1\}\$/\1/p" "$work/o.json" | head -1; }
# shown: the document with what differs by build or by run taken out, each line after a `|`.
shown() {
    sed -e 's/"compiler": "[^"]*"/"compiler": "<compiler>"/' -e 's/[0-9a-f]\{128\}/<signature>/g' -e "s/$PUBLIC/<the key of RFC 8032 test 1>/g" -e 's/[0-9a-f]\{64\}/<id>/g' -e 's/^/| /' "$work/o.json"
}
# said TAG WHAT: the command was refused (exit 1) with TAG.
said() {
    [ "$code" = 1 ] || fail "$2: exit $code, wanted 1: $(head -c 200 "$work/o.json")"
    [ "$(jf tag)" = "$1" ] || fail "$2: the tag is '$(jf tag)', wanted '$1'"
}
ok() { [ "$code" = 0 ] || fail "$1: exit $code: $(head -c 300 "$work/o.json")"; }
# A repository with one recorded change of one `.cho` file: its id in $c1.
repo() {
    rm -rf "$work/$1"; mkdir -p "$work/$1"; cd "$work/$1" || exit 1
    rv init; ok init
    printf 'module a;\npub fn twice(n: int) -> [] int { return n + n; }\n' > a.cho
    rv record --all --time 100 --intent-prompt 'the first change'; ok record
    c1=$(jf change)
}
# object KIND ID: the file of an object of the store here.
object() { echo ".cancho/vcs/objects/$1/$(printf '%s' "$2" | cut -c1-2)/$(printf '%s' "$2" | cut -c3-64)"; }

begin a_change_is_attested_by_the_built_in_gate
repo one
rv attest --time 200; ok attest; shown
a1=$(jf attestation)
[ -f "$(object attestation "$a1")" ] || fail "the attestation is not an object of the store"
same "$(cat .cancho/vcs/refs/attest/"$c1")" "$a1" "the chain's ref holds the attestation"
rv attestations; ok attestations; shown
rv verify; ok verify; shown
rv attest HEAD --time 200; ok "attest again"
[ "$(jf attestation)" != "$a1" ] || fail "a second attestation is the first one again: it must name the first as the one before it"
end

begin another_gate_is_recorded_as_the_caller_says
repo two
printf 'ok 12 tests\n' > "$work/gate.out"
rv attest --gate tests --status 0 --command 'sh scripts/gate.sh --skip seed' --output-file "$work/gate.out" --time 300; ok "attest --gate tests"; shown
rv attest --gate authority --status 3 --time 301; ok "attest of a gate that failed"
rv attestations; ok attestations; shown
[ "$(grep -c '"gate"' "$work/o.json")" = 2 ] || fail "two links were expected"
rv verify; ok verify
[ "$(jf attestation)" = 2 ] || fail "verify counts $(jf attestation) attestations, wanted 2"
end

begin a_key_signs_and_the_signature_is_checked
repo three
printf '%s\n' "$SEED" > "$work/seed.txt"
rv keys new agent --seed-file "$work/seed.txt"; ok "keys new"; shown
same "$(jf key)" "$PUBLIC" "the public key of the seed of RFC 8032 test 1"
same "$(ls -l .cancho/vcs/keys/agent.key | cut -c1-10)" "-rw-------" "the seed's file is its owner's only"
rv keys new agent --seed-file "$work/seed.txt"; said key-exists "a second key of the same name"
rv keys new 'bad name' --seed-file "$work/seed.txt"; said bad-argument "a name with a space"
rv keys new fresh; ok "keys new from the system's randomness"
[ "$(wc -c < .cancho/vcs/keys/fresh.key | tr -d ' ')" = 65 ] || fail "a fresh seed is not 64 digits and a newline"
[ "$(cat .cancho/vcs/keys/fresh.key)" != "$SEED" ] || fail "a fresh seed is the test's"
rv attest --key agent --time 400; ok "attest --key"; shown
rv attest --key nobody --time 400; said key-not-found "a key that is not there"
rv keys sign --key agent --time 401; ok "keys sign"; shown
rv keys sign --time 401; said bad-argument "keys sign with no key"
rv attestations; ok attestations; shown
rv keys list; ok "keys list"
grep -q "\"key\": \"$PUBLIC\"" "$work/o.json" || fail "keys list does not show the key as trusted"
rv verify; ok verify
end

begin a_broken_chain_is_named_by_its_first_broken_link
repo four
printf '%s\n' "$SEED" > "$work/seed.txt"
rv keys new agent --seed-file "$work/seed.txt"; ok "keys new"
rv attest --key agent --time 500; ok attest; a1=$(jf attestation)
rv attest --gate tests --status 0 --time 501; ok "attest tests"; a2=$(jf attestation)
# 1. a record whose bytes are not what its name says
cp "$(object attestation "$a1")" "$work/a1.json"
chmod u+w "$(object attestation "$a1")"; sed 's/"status":0/"status":1/' "$work/a1.json" > "$(object attestation "$a1")"
rv verify; said chain-broken "an attestation that was edited"; shown
rv attestations; said chain-broken "attestations of the edited chain"
cp "$work/a1.json" "$(object attestation "$a1")"
rv verify; ok "verify once the record is put back"
# 2. a record that is whole and hashes to its name, with a signature that is not over its bytes
sed 's/"gate":"check"/"gate":"tests"/' "$work/a1.json" > "$work/forged.json"
{ echo attestation; cat "$work/forged.json"; } > "$work/forged.in"
"$cancho" build "$root/compiler/vcsid.cho" "$root/compiler/vcsobj.cho" "$root/compiler/blake3.cho" "$root/compiler/lexcore.cho" --std -o "$work/vcsid" > "$work/vcsid.txt" 2>&1 || fail "the id program did not build"
forged=$("$work/vcsid" < "$work/forged.in" | tr -d '\n')
[ ${#forged} = 64 ] || fail "the forged record got no id: '$forged'"
mkdir -p "$(dirname "$(object attestation "$forged")")"; cp "$work/forged.json" "$(object attestation "$forged")"
cp ".cancho/vcs/refs/attest/$c1" "$work/head.txt"; echo "$forged" > ".cancho/vcs/refs/attest/$c1"
rv verify; said chain-broken "a record whose signature is not its key's"; shown
cp "$work/head.txt" ".cancho/vcs/refs/attest/$c1"; rm -f "$(object attestation "$forged")"
# 3. a link that is not in the store
mv "$(object attestation "$a1")" "$work/gone"
rv verify; said chain-broken "a link that is missing"; shown
mv "$work/gone" "$(object attestation "$a1")"
# 4. a chain that holds the attestation of another change
printf 'module a;\npub fn twice(n: int) -> [] int { return n * 2; }\n' > a.cho
rv record --all --time 510 --intent-prompt 'the second change'; ok "record 2"; c2=$(jf change)
cp ".cancho/vcs/refs/attest/$c1" ".cancho/vcs/refs/attest/$c2"
rv verify; said chain-broken "a chain of another change"; shown
rm ".cancho/vcs/refs/attest/$c2"
rv verify; ok "verify at the end"
end

begin a_branch_that_asks_for_a_signature_moves_only_with_one
repo five
printf '%s\n' "$SEED" > "$work/seed.txt"
rv keys new agent --seed-file "$work/seed.txt"; ok "keys new"
rv keys require main; ok "keys require"; shown
rv keys require 'not a branch'; said bad-ref-name "a branch name with a space"
rv verify; said unsigned-change "verify of a required branch at an unsigned change"; shown
printf 'module a;\npub fn twice(n: int) -> [] int { return n * 2; }\n' > a.cho
rv record --all --time 600 --intent-prompt 'unsigned'; said unsigned-change "a record with no key on a required branch"; shown
same "$(cat .cancho/vcs/refs/heads/main)" "$c1" "the branch did not move"
rv record --all --dry-run --time 600; said unsigned-change "a dry run says the same"
rv record --all --time 600 --intent-prompt 'signed' --key agent; ok "record --key"; c2=$(jf change)
same "$(cat .cancho/vcs/refs/heads/main)" "$c2" "the branch moved to the signed change"
rv attestations; ok "attestations of the signed change"; shown
rv verify; ok "verify of a required branch at a signed change"
rv revert "$c2" --time 601; said unsigned-change "a revert with no key"
rv revert "$c2" --time 601 --key agent; ok "revert --key"
rv verify; ok "verify after the signed revert"
: > .cancho/vcs/keys/trusted
rv verify; said unsigned-change "verify when the key is no longer trusted"
rv keys trust "$PUBLIC"; ok "keys trust"; shown
rv keys trust nothex; said bad-argument "a key that is not digits"
rv verify; ok "verify when the key is trusted again"
end

begin what_attest_refuses
repo six
rv attest --gate tests; said bad-argument "another gate with no status"
rv attest --gate 'two words' --status 0; said bad-argument "a gate's name with a space"
rv attest --status 0; said bad-argument "the built-in gate with a status"
rv attest --time soon; said bad-argument "a time that is not a number"
rv attest nosuchchange; said bad-revision "a change that is not there"
rv attest --gate tests --status 0 --output-file "$work/not-there.txt"; [ "$code" = 3 ] || fail "an output file that cannot be read: exit $code, wanted 3"
rv keys; said bad-argument "keys with no verb"
rv keys destroy all; said bad-argument "keys with a verb it does not have"
rv attestations HEAD HEAD; said bad-argument "attestations of two changes"
printf 'module a;\npub fn twice(n: int) -> [] int { return nope; }\n' > a.cho
rv record --all --no-gate --time 700; ok "record --no-gate"
rv attest --time 701; said gate-failed "the built-in gate on a change that does not check"; shown
rv attestations; ok "attestations of a change with none"; shown
end
