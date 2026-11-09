# filesystem.sh -- the script of tests/commands/filesystem.cho. $1 is the probe (tests/programs/filesystem_probe.cho, built), $2 a scratch directory.
# The probe takes a case in argv[1] (see its head); a trap is a fault signal, which a shell reports as a status over 128.
probe=$1
dir=$2

killed() {
  "$probe" "$@" > /dev/null 2>&1
  s=$?
  if [ "$s" -gt 128 ]; then echo "$1: killed by a signal"; else echo "$1: exit $s (it should have been killed)"; fi
}

echo "== a path outside the granted prefix, one with a .., and a sibling of the granted directory: traps, not missing files"
killed outside
killed dotdot
killed sibling

echo "== a written file is readable by its owner (open is variadic: a mode that travelled in the wrong place left it unreadable)"
target=$dir/written.txt
"$probe" write "$target"
echo "write: exit $?"
echo "content: $(cat "$target")"
echo "owner bits: $(ls -l "$target" | cut -c2-3)"

echo "== a missing file is -1, not a trap"
"$probe" missing
echo "missing: exit $?"

echo "== a short path under a narrowed prefix is read (the check's loop unrolled; LLVM once folded the read to poison)"
printf abc > /tmp/lxfsp
"$probe" short
echo "short: exit $?"
rm -f /tmp/lxfsp

echo "== a file op is an operand of a comparison, of && and of ||"
rm -f "$dir/operand"
"$probe" operand "$dir/operand"
echo "operand: exit $?"
