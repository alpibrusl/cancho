# agent_cli.sh -- the script of tests/commands/script_tests.cho for `conformance/agent_cli.rs` (partly moved): the ACLI surface stays honest about what the command line says. $1 is the compiler,
# $2 a scratch directory (the commands are run in it: the ACLI side effect, writing `.cli/`, never touches the source tree); run from the root of the repository.
# What moved: `introspect_names_every_registered_command`, and `every_dispatched_command_is_documented` with its subject changed: the Rust test read the dispatch table out of `crates/cancho/src/main.rs`; the
# command line of the compiler in cancho has no such table to read, so the commands are those of its own `--help` usage block, which must each be in what `cancho skill` advertises.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
cd "$work" || exit 1

has() { if grep -qF -- "$2" "$1"; then echo "ok $3"; else echo "FAILS $3"; fi; }

echo "== every command of the usage block is documented by skill"
"$cancho" --help > help.txt
"$cancho" skill > skill.txt
# The words after `cancho ` at the start of a usage line; `help`, `introspect` and `skill` are the surfaces being checked against.
sed -n '/^usage:/,/^options:/p' help.txt | sed 's/^[ \t]*//' | awk '/^cancho / { if ($2 !~ /^-/) print $2 }' | sort -u | grep -vx -e help -e introspect -e skill > usage.txt
sed -n 's/^- `cancho \([A-Za-z0-9-]*\).*/\1/p' skill.txt | sort -u > advertised.txt
echo "usage commands: $(wc -l < usage.txt | tr -d ' ')"
while read -r command; do
    grep -qx "$command" advertised.txt || echo "FAILS 'cancho skill' omits $command -- add a CommandInfo for it"
done < usage.txt
echo "ok skill advertises every command of the usage block"

echo "== introspect names every registered command"
"$cancho" introspect --output json > introspect.json
has introspect.json '"name": "cancho"' "the tree is the tree of cancho"
for expected in build check run ids authority layout print vcs; do
    has introspect.json "\"name\": \"$expected\"" "introspect has $expected"
done
exit 0
