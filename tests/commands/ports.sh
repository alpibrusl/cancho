# ports.sh -- the GNU ports, `base64`, `sort` and `cut` (examples/base64, examples/sort, examples/cut; docs/porting.md, docs/flags.md), held to what the originals print.
# Moved from crates/cancho/tests/conformance/ports.rs (8 tests, deleted; docs/gate.md, G4 wave 4). Where GNU coreutils is at /usr/bin the port is also compared with it
# (the original was: BSD's tools are different programs); where it is not, everything that needs no reference still runs. The checks are `begin NAME` ... `end` of
# tests/commands/script_lib.sh. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/support/net.sh

build_exe base64 examples/base64/base64.cho || exit 1
build_exe sort examples/sort/sort.cho || exit 1
build_exe cut examples/cut/cut.cho || exit 1
build_exe bytes_gen tests/programs/bytes_gen.cho || exit 1
b64=$work/base64
srt=$work/sort
cut=$work/cut
gen=$work/bytes_gen
have_base64=0; gnu base64 && have_base64=1
have_sort=0; gnu sort && have_sort=1
have_cut=0; gnu cut && have_cut=1
echo "GNU coreutils at /usr/bin: base64 $have_base64, sort $have_sort, cut $have_cut" > "$work/references.txt"

# run PROGRAM ARGS... : standard input from $work/in, standard output in $work/out, standard error in $work/err, the exit status in $status.
run() {
    LC_ALL=C "$@" < "$work/in" > "$work/out" 2> "$work/err"
    status=$?
}

begin base64_agrees_with_coreutils
# Twelve input sizes, the boundaries around the 57 input bytes that fill a 76-column line, both directions; a megabyte at the end proves it streams.
for size in 0 1 2 3 4 5 17 56 57 58 100 1000; do
    "$gen" $size 37 5 > "$work/in"
    run "$b64"
    same "$status" 0 "encoding $size bytes"
    cp "$work/out" "$work/encoded"
    if [ $have_base64 = 1 ]; then
        /usr/bin/base64 < "$work/in" > "$work/theirs"
        same_file "$work/encoded" "$work/theirs" "encoding $size bytes differs from GNU coreutils"
    fi
    cp "$work/in" "$work/original"
    cp "$work/encoded" "$work/in"
    run "$b64" -d
    same "$status" 0 "decoding $size bytes"
    same_file "$work/out" "$work/original" "the round trip lost bytes of $size"
done
# Malformed input, where the exit status is the whole behaviour.
for input in 'abc$def' 'QQ=A' 'Q'; do
    printf '%s' "$input" > "$work/in"
    run "$b64" -d
    same "$status" 1 "'$input' should be refused"
    if [ $have_base64 = 1 ]; then
        /usr/bin/base64 -d < "$work/in" > /dev/null 2>&1
        same "$?" "$status" "GNU coreutils disagrees about '$input'"
    fi
done
"$gen" 1048576 31 7 > "$work/in"
cp "$work/in" "$work/original"
run "$b64"
same "$status" 0 "a megabyte should encode"
cp "$work/out" "$work/in"
run "$b64" -d
same "$status" 0 "a megabyte should decode"
same_file "$work/out" "$work/original" "the round trip changed a megabyte"
end

begin sort_agrees_with_gnu_sort
# The shapes that decide a line sort: nothing, no trailing newline, blank lines, one line a prefix of another, bytes outside the letters, enough lines to be a merge.
i=0
for input in '' 'zebra\nant' '\n\nb\n\na\n' 'ab\na\nabc\nb\n' '~\n!\nA\na\n0\n' 'same\nsame\nsame\n' BULK; do
    i=$((i + 1))
    if [ "$input" = BULK ]; then
        awk 'BEGIN { s = "xyzab"; for (i = 0; i < 5000; i++) printf "%s%d\n", substr(s, i % 5 + 1, 1), (i * 7919) % 10007 }' > "$work/in"
    else
        printf "$input" > "$work/in"
    fi
    run "$srt"
    same "$status" 0 "sorting input $i"
    # A permutation of the input's lines, in non-descending order: both hold with or without a reference.
    LC_ALL=C sort "$work/in" > "$work/in.sorted"
    LC_ALL=C sort "$work/out" > "$work/out.sorted"
    same_file "$work/out.sorted" "$work/in.sorted" "the output of input $i is not a permutation of its lines"
    LC_ALL=C sort -c "$work/out" 2> /dev/null || fail "the output of input $i is not in byte order"
    if [ $have_sort = 1 ]; then
        LC_ALL=C /usr/bin/sort < "$work/in" > "$work/theirs"
        same_file "$work/out" "$work/theirs" "input $i differs from GNU sort"
    fi
done
# Named files, several at once, and one that is not there.
printf 'pear\napple\n' > "$work/one.txt"
printf 'fig\nbanana\n' > "$work/two.txt"
: > "$work/in"
LC_ALL=C "$srt" "$work/one.txt" "$work/two.txt" < /dev/null > "$work/out" 2> /dev/null
same "$?" 0 "sorting two files"
printf 'apple\nbanana\nfig\npear\n' > "$work/want"
same_file "$work/out" "$work/want" "two files should sort together"
LC_ALL=C "$srt" "$work/not-here.txt" < /dev/null > /dev/null 2> /dev/null
same "$?" 2 "a missing file should exit 2, as GNU does"
# Past the 64 KiB first read, so the doubling in read_file runs.
awk 'BEGIN { for (i = 0; i < 40000; i++) printf "line %06d\n", (i * 31) % 40000 }' > "$work/big.txt"
LC_ALL=C "$srt" "$work/big.txt" < /dev/null > "$work/out" 2> /dev/null
same "$?" 0 "a file past the first read"
same "$(wc -l < "$work/out" | tr -d ' ')" 40000 "lines kept while growing"
LC_ALL=C sort -c "$work/out" 2> /dev/null || fail "a file past the first read came out unsorted"
if [ $have_sort = 1 ]; then
    LC_ALL=C /usr/bin/sort "$work/big.txt" > "$work/theirs"
    same_file "$work/out" "$work/theirs" "a file past the first read differs from GNU sort"
fi
# Past the old 8 MiB ceiling of the growth loop: 300,000 lines of 30 bytes (9 MB), 130 reads through the handle.
awk 'BEGIN { for (i = 0; i < 300000; i++) printf "%029d\n", (i * 7919) % 300000 }' > "$work/huge.txt"
[ "$(wc -c < "$work/huge.txt" | tr -d ' ')" -gt 8388608 ] || fail "the fixture has to clear the old 8 MiB bound"
LC_ALL=C "$srt" "$work/huge.txt" < /dev/null > "$work/out" 2> /dev/null
same "$?" 0 "a file past the old 8 MiB ceiling"
same "$(wc -l < "$work/out" | tr -d ' ')" 300000 "lines kept past the old ceiling"
LC_ALL=C sort -c "$work/out" 2> /dev/null || fail "a file past the old ceiling came out unsorted"
end

begin cut_agrees_with_gnu_cut
printf 'alpha,beta,gamma,delta,epsilon\none,two,three\na,b,c,d,e,f,g\n,leading,empty\ntrailing,empty,\nnodelimiterhere\nx,y\n' > "$work/in"
for spec in -f1 -f2 -f2,4 -f1-3 -f3- -f2-4 -f1,3-5 -f9; do
    run "$cut" -d, "$spec"
    # Shape check, with or without a reference: every input line produces exactly one output line.
    same "$(wc -l < "$work/out" | tr -d ' ')" 7 "cut -d, $spec should answer one line per input line"
    if [ $have_cut = 1 ]; then
        run /usr/bin/cut -d, "$spec"
        cp "$work/out" "$work/theirs"
        run "$cut" -d, "$spec"
        same_file "$work/out" "$work/theirs" "cut -d, $spec differs from GNU cut"
    fi
done
end

# The first line of a diagnostic after the program's own name.
complaint() { head -1 "$1" | sed -n 's/^[^:]*: //p'; }

begin cut_reports_a_bad_field_list_like_gnu_cut
: > "$work/in"
run "$cut" -d, -fzzz
same "$status" 1 "a malformed -f list should exit 1"
[ ! -s "$work/out" ] || fail "a refused -f list should print no data"
printf "cut: invalid field value 'zzz'\n" > "$work/want"
same_file "$work/err" "$work/want" "a refused -f list should say so on standard error"
if [ $have_cut = 1 ]; then
    cp "$work/err" "$work/ours.err"
    run /usr/bin/cut -d, -fzzz
    same "$status" 1 "GNU cut exits differently"
    # GNU adds a second line pointing at --help and calls itself by the path it was run by, so only what follows the name on the first line is compared.
    same "$(complaint "$work/err")" "$(complaint "$work/ours.err")" "GNU cut words it differently"
fi
# The other refusal: an argument this program does not understand. GNU names the option and this does not, so only the status and the stream compare.
run "$cut" --nope
same "$status" 1 "an unrecognised argument should exit 1"
[ ! -s "$work/out" ] || fail "an unrecognised argument should print no data"
printf 'cut: usage: cut -d<c> -f<list>\n' > "$work/want"
same_file "$work/err" "$work/want" "an unrecognised argument should say so on standard error"
end

begin both_ports_match_gnu_on_every_spelling
# program|arguments|standard output|exit status|whether GNU agrees. The `no` rows are the divergences docs/flags.md section 4 keeps on purpose: they are asserted to still
# disagree, so one that quietly starts agreeing gets moved rather than forgotten.
cases=$(cat <<'EOF'
cut|-d, -f2|b\n|0|yes
cut|-d , -f2|b\n|0|yes
cut|--delimiter=, --fields=2|b\n|0|yes
cut|--delimiter , --fields 2|b\n|0|yes
cut|-d, -f 2|b\n|0|yes
cut|-f2 -d,|b\n|0|yes
cut|-d, -f1,3|a,c\n|0|yes
cut|-d, -f2 --|b\n|0|yes
cut|-x||1|yes
cut|-d,, -f2||1|yes
cut|--delim=, -f2||1|no
base64|-d|hi|0|yes
base64|--decode|hi|0|yes
base64||YUdrPQo=\n|0|yes
base64|--|YUdrPQo=\n|0|yes
base64|-q||1|yes
base64|-di||1|no
base64|-w0||1|no
EOF
)
rows=0
oldifs=$IFS
IFS='
'
for row in $cases; do
    IFS=$oldifs
    rows=$((rows + 1))
    program=$(printf '%s' "$row" | cut -d'|' -f1)
    args=$(printf '%s' "$row" | cut -d'|' -f2)
    stdout=$(printf '%s' "$row" | cut -d'|' -f3)
    exit_want=$(printf '%s' "$row" | cut -d'|' -f4)
    agrees=$(printf '%s' "$row" | cut -d'|' -f5)
    case $program in
        cut) exe=$cut; printf 'a,b,c\n' > "$work/in" ;;
        *) exe=$b64; printf 'aGk=\n' > "$work/in" ;;
    esac
    printf "$stdout" > "$work/want"
    # shellcheck disable=SC2086
    run "$exe" $args
    cmp -s "$work/out" "$work/want" && [ "$status" = "$exit_want" ] || fail "$program $args should answer what docs/flags.md section 1 says it does (exit $status)"
    ref_have=0
    case $program in cut) ref_have=$have_cut ;; *) ref_have=$have_base64 ;; esac
    if [ $ref_have = 1 ]; then
        cp "$work/out" "$work/ours.out"; ours_status=$status
        # shellcheck disable=SC2086
        run "/usr/bin/$program" $args
        if [ "$agrees" = yes ]; then
            [ "$status" = "$ours_status" ] && cmp -s "$work/out" "$work/ours.out" || fail "GNU $program $args should answer this too"
        else
            [ "$status" != "$ours_status" ] || ! cmp -s "$work/out" "$work/ours.out" || fail "$program $args is a documented divergence; if it now agrees, move the row"
        fi
    fi
    IFS='
'
done
IFS=$oldifs
same "$rows" 18 "the rows of the table"
end

begin sort_names_the_file_it_cannot_read
: > "$work/in"
missing=$work/no-such-file.txt
run "$srt" "$missing"
same "$status" 2 "a file that is not there should exit 2"
[ ! -s "$work/out" ] || fail "a failed read should print no data"
printf 'sort: cannot read: %s\n' "$missing" > "$work/want"
same_file "$work/err" "$work/want" "a failed read should name the file on standard error"
if [ $have_sort = 1 ]; then
    cp "$work/err" "$work/ours.err"
    run /usr/bin/sort "$missing"
    same "$status" 2 "GNU sort exits differently"
    # GNU appends the errno string, which fs_read does not carry: everything before it is the same claim.
    case $(complaint "$work/err") in "$(complaint "$work/ours.err")"*) ;; *) fail "GNU sort words it differently: $(complaint "$work/err")" ;; esac
fi
end

begin base64_reports_invalid_input_like_gnu_base64
printf 'a@b\n' > "$work/in"
run "$b64" -d
same "$status" 1 "a byte outside the alphabet should exit 1"
printf 'base64: invalid input\n' > "$work/want"
same_file "$work/err" "$work/want" "a refused decode should say so on standard error"
# And a valid decode stays silent, which is the half a new stream makes it possible to get wrong.
printf 'aGVsbG8K\n' > "$work/in"
run "$b64" -d
same "$status" 0 "a valid decode"
printf 'hello\n' > "$work/want"
same_file "$work/out" "$work/want" "the decoded text"
[ ! -s "$work/err" ] || fail "a successful decode should say nothing"
if [ $have_base64 = 1 ]; then
    printf 'a@b\n' > "$work/in"
    run "$b64" -d
    cp "$work/err" "$work/ours.err"
    run /usr/bin/base64 -d
    same "$status" 1 "GNU base64 exits differently"
    same "$(complaint "$work/err")" "$(complaint "$work/ours.err")" "GNU base64 words it differently"
fi
end

begin cut_reports_a_long_line_like_gnu_cut
# Around the old 60,000-byte arena slice, and well past it.
for first in 59998 59999 60000 70000 200000; do
    { head -c "$first" /dev/zero | tr '\0' a; printf ',second\n'; } > "$work/in"
    run "$cut" -d, -f2
    same "$status" 0 "a $first-byte first field is not an error"
    printf 'second\n' > "$work/want"
    same_file "$work/out" "$work/want" "a $first-byte first field should not change which field comes back"
    if [ $have_cut = 1 ]; then
        cp "$work/out" "$work/ours.out"
        run /usr/bin/cut -d, -f2
        same_file "$work/out" "$work/ours.out" "GNU cut answers differently on a $first-byte field"
    fi
done
# And the field itself, when it is the long one: the whole of it.
{ printf 'first,'; head -c 100000 /dev/zero | tr '\0' b; printf '\n'; } > "$work/in"
run "$cut" -d, -f2
same "$(wc -c < "$work/out" | tr -d ' ')" 100001 "a long field should come back whole (with its newline)"
end
