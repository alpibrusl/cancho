# io_ordering.sh -- the script of tests/commands/io.cho for tests/programs/io_ordering_probe.cho. $1 is the probe, $2 a scratch directory.
# `docs/bulk-io.md` section 3: write_bytes goes through the same stream putchar does (stdio's), so the two kinds of output cannot interleave in the wrong order. The probe writes 2,000 rounds of 0123456789 through
# alternating primitives; any reordering shows up as an out-of-order digit.
probe=$1
dir=$2
awk 'BEGIN { for (i = 0; i < 2000; i++) printf "0123456789" }' > "$dir/want"
"$probe" > "$dir/got"
echo "exit $?"
if cmp -s "$dir/got" "$dir/want"; then echo "the two primitives interleave in program order"; else echo "OUT OF ORDER: the two primitives did not share one stream"; fi
