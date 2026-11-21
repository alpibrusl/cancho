# frontend_lib.sh -- what the scripts that replay the Rust front end's recorded answers through the ports of compiler share (front_end_syntax.sh, front_end_names.sh,
# front_end_checks.sh). Not a test of its own: a script sources it after tests/commands/script_lib.sh (`begin`, `end`, `fail`, `same`):   . tests/commands/frontend_lib.sh
#
# The oracle was the Rust crates (cancho-syntax, cancho-ir, cancho-id, the blake3 crate). It ran once, on 2026-11-17, over the corpus the Rust tests used, and what it said is in
# tests/vectors/frontend/ (docs/gate.md, G4 wave 5): a `NAME.vec` is a line a case,
#
#     MODE INPUT-DIGEST ANSWER-DIGEST TAG PATH
#
# the first 16 hex digits of the SHA-256 of the file at PATH (the input) and of what the oracle answered, and the tag the oracle's answer has (`ok`, `ERR:rule`, `CANNOT`) so that a vector
# cannot lose its refusals unnoticed. MODE says how the input is fed to the port: `s` the file as it is, `w` the file as a one-file stream (`FILE n` and then the bytes, what the ports that
# read several files expect), `f` the file is a stream already, `l` the file as a stream followed by the library, `p` the formatter's pair (the file and the text the printer makes of it),
# `j` the tree of the file as JSON (the JSON printer's answer fed to the ingester). The library is tests/vectors/frontend/std.stream, the standard library of that day, frozen: an answer
# that depends on it must not change when `std/` does. A file whose digest is no longer the recorded one (the repository's own programs change) is left out; the check fails if fewer
# than 85 in 100 of the lines were run. NAME.sections (the body checker, whose port may answer `SKIP`) holds the oracle's answers themselves: `## MODE DIGEST TAG PATH` and then the lines.

V=tests/vectors/frontend

# sha FILES...: `digest  name` lines (SHA-256).
if command -v sha256sum > /dev/null 2>&1; then shacmd=sha256sum; else shacmd="shasum -a 256"; fi
sha() { $shacmd "$@"; }

# size FILE: the bytes of FILE.
size() { wc -c < "$1" | tr -d ' '; }
lines_of() { wc -l < "$1" | tr -d ' '; }

# port NAME ROOT [FILE...]: the port ROOT.cho of compiler built by the compiler under test as $work/NAME, with the FILEs given or else the files of
# compiler/cancho.files less cli.cho (what a front-end program is).
port() {
    _name=$1
    _root=$2
    shift 2
    _files=
    if [ $# -gt 0 ]; then
        for _f in "$@"; do _files="$_files compiler/$_f"; done
    else
        _files="compiler/$_root.cho"
        for _f in $(cat compiler/cancho.files); do
            [ "$_f" = cli.cho ] || [ "$_f" = "$_root.cho" ] || _files="$_files compiler/$_f"
        done
    fi
    # shellcheck disable=SC2086
    if ! "$cancho" build $_files --std -o "$work/$_name" > "$work/build-$_name.txt" 2>&1; then
        fail "building $_root said: $(head -c 300 "$work/build-$_name.txt")"
        return 1
    fi
}

# feed MODE FILE: the input the mode makes of FILE, on standard output.
feed() {
    case $1 in
        s | f) cat "$2" ;;
        w)
            printf 'FILE %s\n' "$(size "$2")"
            cat "$2"
            ;;
        l)
            printf 'FILE %s\n' "$(size "$2")"
            cat "$2"
            cat "$V/std.stream"
            ;;
    esac
}

# fresh VEC MODES: leaves in $work/sel the lines of VEC whose mode is one of MODES (a regular expression) and in $work/fresh those among them whose input is still the recorded one.
fresh() {
    awk -v re="^($2)\$" '$1 ~ re' "$1" > "$work/sel"
    cut -d' ' -f5 "$work/sel" | sort -u | while read -r _p; do [ -f "$_p" ] && echo "$_p"; done > "$work/paths"
    : > "$work/fresh"
    if [ -s "$work/paths" ]; then
        xargs $shacmd < "$work/paths" > "$work/inshas"
        awk 'FILENAME == ARGV[1] { have[substr($0, 67)] = substr($0, 1, 16); next }
             { if (have[$5] == $2) print }' "$work/inshas" "$work/sel" > "$work/fresh"
    fi
}

# enough NAME: at least 85 in 100 of the selected lines were run ($ran of them).
enough() {
    _total=$(lines_of "$work/sel")
    if [ $((ran * 100)) -lt $((_total * 85)) ]; then fail "$1: only $ran of the $_total recorded programs are still the ones recorded"; fi
}

# feed_line MODE FILE EXE OUT: runs EXE on the input MODE makes of FILE; its standard output is OUT.
feed_line() {
    case $1 in
        p)
            "$work/printer" < "$2" > "$work/printed" 2> /dev/null
            {
                printf 'FILE %s\n' "$(size "$2")"
                cat "$2"
                printf 'FILE %s\n' "$(size "$work/printed")"
                cat "$work/printed"
            } | "$3" > "$4" 2> /dev/null
            ;;
        j) "$work/jsonprinter" < "$2" 2> /dev/null | "$3" > "$4" 2> /dev/null ;;
        *) feed "$1" "$2" | "$3" > "$4" 2> /dev/null ;;
    esac
}

# replay NAME EXE [MODES]: every fresh line of tests/vectors/frontend/NAME.vec (of the modes given, all by default) fed to EXE (mode p needs $work/printer, mode j $work/jsonprinter) answers what
# the oracle answered, by digest: the first three that do not are reported as the failure of the check. Leaves the number of lines run in $ran.
replay() {
    fresh "$V/$1.vec" "${3:-[a-z]}"
    rm -rf "$work/out"
    mkdir "$work/out"
    _i=0
    while read -r _mode _in _ans _tag _path; do
        _i=$((_i + 1))
        feed_line "$_mode" "$_path" "$2" "$work/out/$_i"
    done < "$work/fresh"
    ran=$_i
    : > "$work/outshas"
    if [ "$ran" -gt 0 ]; then (cd "$work/out" && ls | xargs $shacmd) > "$work/outshas"; fi
    # the answers' digests, in the order of the lines run
    awk 'FILENAME == ARGV[1] { got[substr($0, 67)] = substr($0, 1, 16); next }
         { n++; if (got[n] != $3) { bad++; if (bad <= 3) printf "%s ", $5 } }
         END { if (bad) printf "(%d differ)\n", bad }' "$work/outshas" "$work/fresh" > "$work/differ"
    if [ -s "$work/differ" ]; then fail "$1: the answers differ from the oracle's for $(cat "$work/differ")"; fi
    enough "$1"
}

# bodies_agree WANT OUT MODE LEN: do the port's lines (OUT) agree with the oracle's (WANT), line by line, where the port may answer `SKIP` for a function whose body it does not check yet?
# For mode l the oracle's file holds only the program's part (or the one refusal): the library's part is tests/vectors/frontend/bodies_library.txt, the answers for the library after an empty
# program, every offset moved on by LEN, the bytes of the program. Prints `OK compared skipped` or `DIFF why`.
bodies_agree() {
    awk -v wantf="$1" -v outf="$2" -v mode="$3" -v len="$4" -v libf="$V/bodies_library.txt" '
        function shifted(line,   w, n) {
            n = split(line, w, " ")
            if (n == 4) return "fn " (w[2] + len) " " (w[3] + len) " " w[4]
            return "fn " (w[2] + len) " " (w[3] + len) " ERR " w[5] " " (w[6] + len) " " (w[7] + len)
        }
        BEGIN {
            while ((getline l < wantf) > 0) want[++nw] = l
            while ((getline l < outf) > 0) out[++no] = l
            if (mode == "l" && want[1] !~ /^ERR/) { while ((getline l < libf) > 0) want[++nw] = shifted(l) }
            if (no != nw) { print "DIFF " no " lines, the oracle " nw; exit }
            for (i = 1; i <= nw; i++) {
                if (out[i] ~ / SKIP$/) skipped++
                else if (out[i] == want[i]) compared++
                else { print "DIFF line " i ": " out[i] ", the oracle " want[i]; exit }
            }
            print "OK " compared + 0 " " skipped + 0
        }'
}

# replay_sections NAME EXE [MODES]: as replay, for NAME.sections (the oracle's answers themselves) and the body checker's way of agreeing (bodies_agree). Leaves $ran, $compared and $skipped.
replay_sections() {
    rm -rf "$work/want" "$work/out"
    mkdir "$work/want" "$work/out"
    awk -v dir="$work/want" '/^## / { n++; if (f != "") close(f); f = dir "/" n; printf "" > f
                                       printf "%s %s %d %s %s\n", $2, $3, n, $4, $5; next }
                              { print > f }' "$V/$1.sections" > "$work/meta"
    fresh "$work/meta" "${3:-[a-z]}"
    ran=0
    compared=0
    skipped=0
    _bad=
    while read -r _mode _in _n _tag _path; do
        ran=$((ran + 1))
        feed_line "$_mode" "$_path" "$2" "$work/out/answer"
        _r=$(bodies_agree "$work/want/$_n" "$work/out/answer" "$_mode" "$(size "$_path")")
        case $_r in
            OK*)
                compared=$((compared + $(echo "$_r" | cut -d' ' -f2)))
                skipped=$((skipped + $(echo "$_r" | cut -d' ' -f3)))
                ;;
            *) _bad="$_bad $_path: $_r;" ;;
        esac
    done < "$work/fresh"
    if [ -n "$_bad" ]; then fail "$1: the port and the oracle disagree about$(echo "$_bad" | cut -c1-500)"; fi
    enough "$1"
}

# tags NAME REGEX: how many lines of NAME.vec have a tag that matches REGEX.
tags() { awk -v re="$2" '$4 ~ re { n++ } END { print n + 0 }' "$V/$1.vec"; }
