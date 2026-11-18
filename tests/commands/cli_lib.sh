# cli_lib.sh -- what the scripts that hold the command line to answers recorded from the Rust `cancho` share (cli_fmt.sh, cli_print.sh, cli_check.sh, cli_build.sh, ...). Not a test of its own: a script
# sources it after tests/commands/script_lib.sh (which gives `begin`, `fail`, `end`, `same`) with   cancho=$1  work=$2  and the current directory the root of the repository:
#     . tests/commands/script_lib.sh; . tests/commands/cli_lib.sh
#
# These scripts replace the tests of `crates/cancho/tests/conformance/selfhost/cli.rs`, each of which ran the command line in cancho and the Rust command on the same programs and compared
# what they said. The Rust command is gone, so its answers were recorded once (2026-11-17, `docs/gate.md` G4 wave 4, batch "the command line"): a corpus test has a vector file in
# tests/vectors/cli/ with one line a case, `CASE|CKSUM|KIND|ANSWER`, where CASE is a program of the repository or one of the edge programs (tests/vectors/cli/*.txt, which `unframe` writes out as
# files), CKSUM is the `cksum` of that program as it was when the answer was recorded, KIND says what was done to it, and ANSWER is what the Rust command did (the exit status and the checksums of
# what it wrote). A program of the repository that has changed since (its CKSUM differs) is left out of the comparison, as it has no recorded answer any more; `cli_floor` says how many may be left out.
# The scripts take no more than one thing from the environment: CLI_RECORD=DIR makes a script write its vector files into DIR (and not compare them) and is meant to be given the Rust binary; it is
# how the files were made, and how one is renewed with the compiler in cancho when a program changes and the two agreed on it before.

export CANCHO_NO_DEPRECATION_WARNING=1
root=$(pwd)
vec=$root/tests/vectors/cli
skipped=0

# ck FILE: the checksum of a file (POSIX `cksum`).
ck() { cksum < "$1" | awk '{ print $1 }'; }
# cks: the checksum of the standard input.
cks() { cksum | awk '{ print $1 }'; }

# unframe FRAMEFILE DIR EXT: the cases of a framed file (`#### NAME [nonl]`, then the text, a line each; `nonl` is a text with no newline at its end) are written to DIR/NAME.EXT, and their names to DIR/list.
unframe() {
    mkdir -p "$2"
    awk -v dir="$2" -v ext="$3" '
        function flush() { if (out != "") { if (have) printf "%s%s", pend, (nonl ? "" : "\n") > out; close(out) } }
        /^#### / { flush(); name = $2; nonl = ($3 == "nonl"); out = dir "/" name "." ext; printf "" > out; print name >> (dir "/list"); have = 0; next }
        { if (have) printf "%s\n", pend > out; pend = $0; have = 1 }
        END { flush() }' "$1"
}

# corpus: the programs of the repository, one path a line, in the order of the bytes of the names.
corpus() { LC_ALL=C find std examples packages tests/accept tests/reject tests/programs -name '*.cho' -not -path '*/.cancho-vcs/*' | LC_ALL=C sort; }

# edge_files: the edge programs written out under $work/edge (the names are `edge/NAME.cho`, relative to $work).
edge_files() {
    rm -rf "$work/edge"
    for set in edge format_edge print_edge json_edge; do unframe "$vec/$set.txt" "$work/edge" cho; done
    unframe "$vec/ingest_edge.txt" "$work/edge" json
    unframe "$vec/several.txt" "$work/several" cho
}

# real PATH: where a case's file is (a path of the repository, or one of the edge files under $work).
real() { case $1 in edge/*) echo "$work/$1" ;; *) echo "$root/$1" ;; esac; }
# case_files CASE: the files of a case, one a line: its file, or for `several/gNN` the files gNNf0, gNNf1, ... of a program of several files.
case_files() {
    case $1 in
        several/*) ls "$work/several/${1#several/}"f*.cho 2> /dev/null | LC_ALL=C sort ;;
        *) real "$1" ;;
    esac
}
# case_sum CASE: the checksum of what the case is made of (empty if a file is not there).
case_sum() {
    _list=$(case_files "$1")
    [ -n "$_list" ] || return 0
    for _f in $_list; do [ -f "$_f" ] || return 0; cat "$_f"; done | cks
}

# spoil MODE IN OUT
spoil() { LC_ALL=C awk -v mode="$1" -f "$root/tests/commands/cli_spoil.awk" "$2" > "$3"; }

# have_vector NAME: the recorded file; a script that records writes its lines to $CLI_RECORD/NAME.txt instead.
recording() { [ -n "${CLI_RECORD:-}" ]; }
# cases NAME: the cases to run, `CASE|CKSUM|KIND` a line. When comparing, the lines of the vector whose file has not changed (the others are counted in $skipped); when recording, none (the caller makes them).
vector_cases() {
    skipped=0
    : > "$work/cases.txt"
    while IFS='|' read -r case sum kind rest; do
        if [ "$(case_sum "$case")" = "$sum" ]; then echo "$case|$sum|$kind" >> "$work/cases.txt"; else skipped=$((skipped + 1)); fi
    done < "$vec/$1.txt"
}

# settle NAME GOT: GOT, the lines made now (`CASE|CKSUM|KIND|ANSWER`), is written as the vector when recording; otherwise the check fails for each line that differs from the vector's line for that case.
# Returns the count of lines compared in $compared and of those that differ in $differ.
settle() {
    if recording; then
        mkdir -p "$CLI_RECORD"
        cp "$2" "$CLI_RECORD/$1.txt"
        compared=$(wc -l < "$2" | tr -d ' ')
        differ=0
        return 0
    fi
    # Only the recorded lines whose file is unchanged are compared.
    awk -F'|' -v skip="$work/cases.txt" 'BEGIN { while ((getline l < skip) > 0) { split(l, f, "|"); want[f[1] "|" f[2] "|" f[3]] = 1 } }
        { if (($1 "|" $2 "|" $3) in want) print }' "$vec/$1.txt" > "$work/want.txt"
    compared=$(wc -l < "$work/want.txt" | tr -d ' ')
    differ=$(diff "$work/want.txt" "$2" | grep -c '^>')
    if [ "$differ" != 0 ]; then
        fail "$1: $differ of $compared recorded answers differ, the first three: $(diff "$work/want.txt" "$2" | grep '^[<>]' | head -6 | cut -c1-200 | tr '\n' '~')"
    fi
}

# floor NAME COMPARED MIN: at least MIN cases were compared (the rest changed since they were recorded: renew the vector, CLI_RECORD).
floor() {
    recording && return 0
    [ "$2" -ge "$3" ] || fail "$1: only $2 recorded cases are still current ($skipped changed since the vector was recorded; at least $3 are needed)"
}
