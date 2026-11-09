# agent_tools.sh -- the script of tests/commands/script_tests.cho for `conformance/agent_tools.rs` (deleted): `examples/seek/` (docs/agent-tools.md), a bounded authority report, correctness on
# ordinary input and no crash on the adversarial input an agent cannot vouch for. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
root=$(pwd)
work=$2
cd "$work" || exit 1

# docs/agent-tools.md section 2: the whole claim, pinned. Seven labels, bounded, and no foreign symbols.
echo "== authority"
"$cancho" authority "$root/examples/seek/seek.cho" --std --output json > report.json || echo "authority exit $?"
grep -o '"effects": \[[^]]*\]' report.json
grep -o '"foreign_symbols": \[[^]]*\]' report.json
head -2 report.json | tail -1

"$cancho" build "$root/examples/seek/seek.cho" --std -o seek || { echo "seek did not build"; exit 1; }

# Run seek in $work; print its exit status after the lines it said.
seek() {
    "$@" 2> err.txt
    echo "exit $?"
}

printf 'hello world\nfoo bar\nhello again\nbaz\n' > a.txt
printf 'hello once more\n' > b.txt
printf 'hello world\nhello again\n' > c.txt

# A literal match, with the file name prefixed, and exit 0.
echo "== match"
seek ./seek hello a.txt

# No match anywhere is exit 1, GNU grep's own vocabulary.
echo "== no match"
seek ./seek xyz a.txt

# -n prefixes the 1-based line number; -c prints a count.
echo "== -n"
seek ./seek -n hello a.txt
echo "== -c"
seek ./seek -c hello a.txt

# -m caps matches across every file named, not per file.
echo "== -m"
seek ./seek -m 1 hello c.txt b.txt

# A file that cannot be read is reported and moved past, and the overall exit is 2.
echo "== missing file"
seek ./seek hello missing.txt b.txt
grep -c missing.txt err.txt

# No pattern at all is a usage error, exit 2, said on standard error.
echo "== usage"
seek ./seek
[ -s err.txt ] && echo "usage said something"

# No file named reads standard input.
echo "== stdin"
printf 'nothing here\nhello stdin\n' | ./seek hello
echo "exit $?"

# Binary safe: a NUL and a lone 0xFF either side of the pattern; the line comes back whole, byte for byte.
echo "== binary safe"
printf 'binary\000stuff\377hello\n' > bin.dat
./seek hello bin.dat > got.dat
echo "exit $?"
{ printf 'bin.dat:'; cat bin.dat; } > want.dat
cmp got.dat want.dat && echo "the line came back whole"
