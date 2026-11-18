# cli_check.sh -- the script of tests/commands/script_tests.cho for three tests of `conformance/selfhost/cli.rs` (deleted): `the_command_line_in_cancho_checks_like_cancho_check`,
# `..._writes_the_identities_like_cancho_ids` and `..._reports_layout_like_cancho_layout`. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# Their oracle was the Rust `cancho check`, `ids` and `layout` over the programs of the repository, the edge programs and 40 programs of several files; the answers of the Rust binary are recorded
# in tests/vectors/cli/{check,ids,layout}.txt (tests/commands/cli_lib.sh). What is compared is what the Rust tests compared: for `check` the status and the first refusal (its rule, its file, and
# where it is; the sentence is the rule's and not the Rust command's per-site words, `docs/cli-in-cancho.md` section 12); for `ids` and `layout` the whole standard output, byte for byte (a tenth of the
# programs, chosen by a checksum of their names, with `--std`). A program the compiler in cancho cannot judge is a status 3 and is counted, not a difference; a refusal it does not make is.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/cli_lib.sh
cd "$root" || exit 1
edge_files
mkdir -p "$work/c"

# put CASE: the files of a case copied to $work/c as x.cho, or as f0.cho, f1.cho, ... for a program of several files; the names are in $names.
put() {
    rm -f "$work"/c/*.cho
    names=
    case $1 in
        several/*)
            n=0
            for f in $(case_files "$1"); do cp "$f" "$work/c/f$n.cho"; names="$names f$n.cho"; n=$((n + 1)); done ;;
        *) cp "$(real "$1")" "$work/c/x.cho"; names=x.cho ;;
    esac
}
# case_list KINDS...: when recording, the corpus, the edge programs of the Rust tests and the programs of several files, each with the kinds; otherwise the vector's.
all_cases() {
    { corpus; for f in "$work"/edge/edge*.cho; do echo "edge/$(basename "$f")"; done; for g in $(sed 's/f[0-9]*$//' "$work/several/list" | sort -u); do echo "several/$g"; done; } | while read -r case; do
        _s=$(case_sum "$case")
        for k in "$@"; do echo "$case|$_s|$k"; done
    done
}
std_pick() { [ $(($(printf '%s' "$1" | cks) % 10)) = 0 ]; }

# --- check
begin the_command_line_in_cancho_checks_like_cancho_check
check_answer() {
    put "$1"
    ( cd "$work/c" && "$cancho" check --output json $names > o.txt 2> /dev/null; s=$?
      rule=$(sed -n 's/^ *"rule": "\([^"]*\)".*/\1/p' o.txt | head -1)
      pos=$(sed -n -e 's/.*"position": { "file": "", .*/- 0 0/p' -e 's/.*"position": { "file": "\([^"][^"]*\)", "line": \([0-9]*\), "column": \([0-9]*\).*/\1 \2 \3/p' o.txt | head -1)
      if [ $s = 0 ]; then echo "0 - - 0 0 $(ck o.txt)"; else
          "$cancho" check $names > /dev/null 2> e.txt
          # the place, then the line of the source and the caret under it, which are the same whatever the sentence is.
          t=$(awk 'BEGIN { RS = "" } NR == 1 { n = split($0, l, "\n"); h = l[1]; sub(/: error: .*/, "", h); print h; print l[n - 1]; print l[n] }' e.txt | cks)
          echo "$s ${rule:--} ${pos:--  0 0} $t"
      fi )
}
if recording; then all_cases c > "$work/cases.txt"; else vector_cases check; fi
: > "$work/got.txt"
while IFS='|' read -r case sum kind; do
    echo "$case|$sum|$kind|$(check_answer "$case")" >> "$work/got.txt"
done < "$work/cases.txt"
if recording; then settle check "$work/got.txt"; else
    # `want` is the recorded line of each case that is current; `got` what the compiler in cancho says. The answer is `STATUS RULE FILE LINE COLUMN TEXT`.
    awk -F'|' 'NR == FNR { keep[$1 "|" $2 "|" $3] = 1; next } (($1 "|" $2 "|" $3) in keep)' "$work/cases.txt" "$vec/check.txt" > "$work/want.txt"
    awk -F'|' -v out="$work/check-report.txt" '
        NR == FNR { want[$1] = $4; next }
        {
            split(want[$1], w, " "); split($4, g, " ")
            if (g[1] == 3) { cannot++; next }
            if (w[1] == 0) {
                if (g[1] == 0 && g[6] == w[6]) accepted++; else wrong[++nw] = $1 ": accepted by Rust, " $4
                next
            }
            if (g[1] == 0) { missed[++nm] = $1; next }
            if (g[1] == 1 && w[2] == g[2] && w[3] == g[3]) {
                if (w[4] == g[4] && w[5] == g[5]) agreed++; else column++
            } else if (g[1] == 1 && w[3] == "-" && w[2] == g[2]) agreed++
            else wrong[++nw] = $1 ": Rust " want[$1] " against " $4
        }
        END {
            for (i = 1; i <= nw; i++) print "wrong: " wrong[i] > out
            for (i = 1; i <= nm; i++) print "missed: " missed[i] > out
            print "accepted " accepted + 0 > out; print "agreed " agreed + 0 > out; print "column " column + 0 > out; print "cannot " cannot + 0 > out
        }' "$work/want.txt" "$work/got.txt"
    ncases=$(wc -l < "$work/got.txt" | tr -d ' ')
    bad=$(grep -c '^wrong\|^missed' "$work/check-report.txt")
    [ "$bad" = 0 ] || fail "check: the compiler in cancho and the Rust command disagree about $bad programs: $(grep '^wrong\|^missed' "$work/check-report.txt" | head -3 | cut -c1-240 | tr '\n' '~')"
    accepted=$(sed -n 's/^accepted //p' "$work/check-report.txt"); agreed=$(sed -n 's/^agreed //p' "$work/check-report.txt"); column=$(sed -n 's/^column //p' "$work/check-report.txt")
    [ "$accepted" -gt 60 ] && [ "$agreed" -gt 600 ] || fail "check: the comparison covered $accepted accepted and $agreed refused programs"
    [ $((column * 20)) -lt "$ncases" ] || fail "check: the place of the first refusal differs in $column of $ncases programs"
    echo "check: $(tr '\n' ' ' < "$work/check-report.txt")" >&2
fi
end

# --- ids and layout: the whole standard output, byte for byte; `--std` for a tenth.
both() {
    # both NAME TESTNAME FLOOR STDFLOOR
    begin "$2"
    out_answer() {
        put "$1"
        _std=; [ "$2" = std ] && _std=--std
        ( cd "$work/c" && "$cancho" "$3" $_std $names > o.txt 2> /dev/null; echo "$? $(ck o.txt)" )
    }
    if recording; then
        all_cases plain > "$work/cases-all.txt"
        while IFS='|' read -r case sum kind; do
            echo "$case|$sum|$kind"
            if std_pick "$case"; then echo "$case|$sum|std"; fi
        done < "$work/cases-all.txt" > "$work/cases.txt"
    else
        vector_cases "$1"
    fi
    : > "$work/got.txt"
    while IFS='|' read -r case sum kind; do
        echo "$case|$sum|$kind|$(out_answer "$case" "$kind" "$1")" >> "$work/got.txt"
    done < "$work/cases.txt"
    if recording; then settle "$1" "$work/got.txt"; else
        awk -F'|' 'NR == FNR { keep[$1 "|" $2 "|" $3] = 1; next } (($1 "|" $2 "|" $3) in keep)' "$work/cases.txt" "$vec/$1.txt" > "$work/want.txt"
        awk -F'|' '
            NR == FNR { want[$1 "|" $3] = $4; next }
            {
                split(want[$1 "|" $3], w, " "); split($4, g, " ")
                if (w[1] == 1 && g[1] == 0) missed[++nm] = $1
                else if (g[1] == 3 && w[1] != 3) cannot++
                else if (w[1] == g[1] && w[2] == g[2]) { if (w[1] == 0) { same++; if ($3 == "std") std++ } else refused++ }
                else different[++nd] = $1 " " $3 ": " want[$1 "|" $3] " against " $4
            }
            END {
                for (i = 1; i <= nd; i++) print "different: " different[i]
                for (i = 1; i <= nm; i++) print "missed: " missed[i]
                print "same " same + 0; print "std " std + 0; print "cannot " cannot + 0
            }' "$work/want.txt" "$work/got.txt" > "$work/$1-report.txt"
        _bad=$(grep -c '^different\|^missed' "$work/$1-report.txt")
        _same=$(sed -n 's/^same //p' "$work/$1-report.txt"); _std=$(sed -n 's/^std //p' "$work/$1-report.txt")
        _missed=$(grep '^missed' "$work/$1-report.txt" | sed 's/^missed: //' | tr '\n' ' ')
        if [ "$1" = ids ]; then
            # The refusals the checker in cancho does not make (it lets through a program of several files whose body it has no judgement of): one, as in the Rust test.
            [ "$_missed" = "several/g39 " ] || fail "$1: the refusals the checker in cancho does not make: '$_missed'"
            [ "$(grep -c '^different' "$work/$1-report.txt")" = 0 ] || fail "$1: $(grep '^different' "$work/$1-report.txt" | head -3 | cut -c1-200 | tr '\n' '~')"
        else
            [ "$_bad" = 0 ] || fail "$1: $(grep '^different\|^missed' "$work/$1-report.txt" | head -3 | cut -c1-200 | tr '\n' '~')"
        fi
        [ "$_same" -gt "$3" ] && [ "$_std" -gt "$4" ] || fail "$1: only $_same runs gave an answer, $_std with the library"
        echo "$1: $(tr '\n' ' ' < "$work/$1-report.txt")" >&2
    fi
    end
}
both ids the_command_line_in_cancho_writes_the_identities_like_cancho_ids 100 20
both layout the_command_line_in_cancho_reports_layout_like_cancho_layout 100 10
