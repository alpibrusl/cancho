# compile_one.sh CANCHO DIR N PROBE -- one case of compile_programs.sh: DIR/N.cho built by CANCHO (`build --std`), run with no standard input, and the way it ended written to DIR/N.got as
# `exit CODE out HEX` (the program's status and its standard output in hex), `trap out HEX` (it was stopped by a signal, which is how a trap ends) or `refused TEXT` if it did not build.
# PROBE is tests/programs/process_probe.cho, built: it runs the program, copies its standard output and says last `== code N` or `== signal B` (a shell cannot tell a status of 137 from a
# signal, and some of these programs exit with 137). Not a test of its own; compile_programs.sh runs it four at a time. It is also the recording: with CANCHO the Rust compiler and
# COMPILER_FLAG="--compiler rust", DIR/N.got is the line the vector holds.
cancho=$1
dir=$2
n=$3
probe=$4
flag=${COMPILER_FLAG:-}
if ! "$cancho" build "$dir/$n.cho" --std $flag -o "$dir/$n.exe" > "$dir/$n.build" 2>&1; then
    echo "refused $(head -c 200 "$dir/$n.build" | tr '\n' ' ')" > "$dir/$n.got"
    exit 0
fi
"$probe" "$dir/$n.exe" - - - run > "$dir/$n.out" 2> /dev/null
how=$(tail -n 1 "$dir/$n.out" | grep -o '== \(code\|signal\) [0-9]*$')
size=$(wc -c < "$dir/$n.out" | tr -d ' ')
keep=$((size - ${#how} - 1))
if [ "$keep" -gt 0 ]; then head -c "$keep" "$dir/$n.out" > "$dir/$n.body"; else : > "$dir/$n.body"; fi
hex=$(od -An -tx1 -v "$dir/$n.body" | tr -d ' \n')
shown=out; [ -n "$hex" ] && shown="out $hex"
case $how in
    "== code "*) echo "exit ${how#== code } $shown" > "$dir/$n.got" ;;
    "== signal "*) echo "trap $shown" > "$dir/$n.got" ;;
    *) echo "refused the probe said '$how'" > "$dir/$n.got" ;;
esac
rm -f "$dir/$n.exe" "$dir/$n.out" "$dir/$n.build" "$dir/$n.body"
