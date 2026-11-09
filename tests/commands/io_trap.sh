# io_trap.sh -- the script of tests/commands/io.cho for tests/programs/io_trap_probe.cho. $1 is the probe, $2 a scratch directory.
# `docs/standard-error.md` section 1.2: standard output is fully buffered when it is not a terminal and a trap does not flush it, so a program that says what is wrong and then dies says nothing at all (measured at zero
# bytes, into a file and through a pipe). C guarantees stderr is not fully buffered, so the same message on the other stream has already left. A trap is a fault signal, a status over 128 to a shell.
probe=$1
dir=$2
"$probe" > "$dir/out" 2> "$dir/err"
s=$?
if [ "$s" -gt 128 ]; then echo "the program trapped (a fault signal)"; else echo "exit $s: the program was supposed to trap"; fi
echo "standard error: $(cat "$dir/err")"
echo "standard output: $(wc -c < "$dir/out" | tr -d ' ') bytes"
