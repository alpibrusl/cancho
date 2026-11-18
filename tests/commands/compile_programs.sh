# compile_programs.sh -- the script of tests/commands/script_tests.cho for `the_compiler_in_cancho_builds_programs_that_do_what_the_rust_ones_do` of conformance/selfhost/compile.rs
# (deleted; docs/gate.md, G4 wave 4): programs built by the compiler in cancho end as the Rust compiler's builds of them did. $1 is the compiler, $2 a scratch directory; run from the root of the
# repository.
#
# tests/vectors/compile_programs.txt is the programs the Rust test built (whole numbers and booleans, every operator, the ones that trap, generics, floats, structs, enums, defers, words,
# crypto, threads, sockets, pipes, files, directories, function values, the poller and standard input) and how the Rust compiler's build of each ended: `##@@ N exit CODE out HEX` or
# `##@@ N trap out HEX`, then the program. The recording is `COMPILER_FLAG="--compiler rust"` with the Rust `cancho` in the place of the compiler (compile_one.sh writes the same line);
# a program marked `subst` has `NAME` in the names of the files it makes in /tmp and `PORT` in the port it listens on, which this script gives names and a port of its own.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
vector=$(pwd)/tests/vectors/compile_programs.txt
one=$(pwd)/tests/commands/compile_one.sh

d=$work/cases
mkdir -p "$d"
# One file a program, N.cho, and the answer of the Rust compiler, N.want.
awk -v dir="$d" '
    /^##@@ / {
        if (out != "") close(out)
        n = $2
        out = dir "/" n ".cho"
        last = NF
        if ($NF == "subst") { last = NF - 1; print n >> (dir "/subst") }
        want = $3
        for (i = 4; i <= last; i++) want = want " " $i
        print want > (dir "/" n ".want")
        close(dir "/" n ".want")
        next
    }
    { print > out }
' "$vector"
if [ -f "$d/subst" ]; then
    while read -r n; do
        sed -e "s/NAME/native$$/g" -e "s/PORT/29135/g" "$d/$n.cho" > "$d/$n.tmp" && mv "$d/$n.tmp" "$d/$n.cho"
    done < "$d/subst"
fi
count=$(grep -c '^##@@ ' "$vector")
ls "$d" | sed -n 's/\.cho$//p' | sort -n > "$d/list"

begin the_programs_were_all_split_out
same "$(wc -l < "$d/list" | tr -d ' ')" "$count" "programs split out of the vector"
end

build tests/programs/process_probe.cho process_probe
# Four at a time: each is a build (clang runs) and a run.
xargs -P 4 -I{} sh "$one" "$cancho" "$d" {} "$work/process_probe" < "$d/list"

begin the_compiler_in_cancho_builds_programs_that_do_what_the_rust_ones_do
different=0
trapped=0
while read -r n; do
    got=$(cat "$d/$n.got" 2> /dev/null)
    want=$(cat "$d/$n.want")
    case $want in trap*) trapped=$((trapped + 1)) ;; esac
    if [ "$got" != "$want" ]; then
        different=$((different + 1))
        [ "$different" -le 5 ] && fail "program $n: the Rust compiler's build ended '$want', the one in cancho's '$(printf '%s' "$got" | cut -c1-120)' ($(head -c 120 "$d/$n.cho" | tr '\n' ' '))"
    fi
done < "$d/list"
same "$different" 0 "programs that end differently"
[ "$trapped" -ge 40 ] || fail "$trapped of the programs trap; the test is about them too"
end
echo "programs built both ways: $count, $trapped of them trap"
rm -rf "$d"
