# vcs_diff_lib.sh -- what the six scripts that were crates/cancho/tests/conformance/selfhost/vcs.rs share (cli_vcs_log.sh, cli_vcs_lock.sh, cli_vcs_fetch.sh, cli_vcs_publish.sh, cli_vcs_sync.sh,
# cli_vcs_dir.sh; docs/gate.md G4, wave 4). Not a test of its own: a script sources it with cancho=$1 (an absolute path) and work=$2 set, from the root of the repository:
#     . tests/commands/vcs_diff_lib.sh
#
# Those seven tests held the commands `cancho vcs log|lock|fetch|resolve|publish|sync` of the compiler in cancho to the Rust command, case by case: the same files in two directories, the same
# command line, and the status, the standard output, the first line of the standard error and every file left had to be equal. The Rust command is gone, so a case here is run once and
# printed, and the transcript (NAME.expected) is what the Rust command did, recorded once on 2026-11-17 by running the same script with the Rust binary: `CANCHO_ORACLE=rust sh
# tests/commands/NAME.sh RUST_CANCHO SCRATCH`. With CANCHO_ORACLE set, the command is run as the Rust command takes its arguments (a cache in CANCHO_CACHE, no --cache and no --std-dir); without it, as the
# compiler in cancho takes them. A transcript line says what a case did: `step N: vcs ARGS`, its `exit`, its output (inline up to 400 bytes, else its length and `cksum`), the first line of its error,
# and after the last step every file left (`file PATH CKSUM LENGTH`, or `link PATH -> TARGET`).
#
# What the old tests did not compare, and this does not: the words after the path of a file that could not be read (the JSON reader's or the operating system's: the first line of an error
# that starts `cannot ` ends at its second backtick), a warning about a file that does not parse and the parser's rendering of it, and the words of the compiler that refused a program or did not
# hold a pin (the error line of a `fetch`, `resolve`, `publish` or `sync` that exits 1 is dropped, unless it is about `--std` or, with `--dir`, one of the commands' own refusals).

oracle=${CANCHO_ORACLE:-}
root=$(pwd -P)
packages=$root/packages
stddir=$root/std
mkdir -p "$work"
work=$(cd "$work" && pwd -P)
cache=$work/cache
nf=$work/norm.sed
: > "$nf"
# addnorm FROM TO: from now on FROM is written TO in every transcript.
addnorm() { printf 's#%s#%s#g\n' "$1" "$2" >> "$nf"; }
addnorm "$work" '<work>'
addnorm "$root" '<root>'
# A store a publish requires is recorded by its path from the store, so how many `../` it takes depends on where the scratch directory is.
printf 's#\\(\\.\\./\\)\\{3,\\}#<up>/#g\n' >> "$nf"
norm() { sed -f "$nf"; }
d=$work/case
steps=0

# sig C: a 64-digit identity of the one digit C. rep TEXT N: TEXT N times.
sig() { printf '%064d' 0 | tr 0 "$1"; }
rep() { awk -v c="$1" -v n="$2" 'BEGIN { for (i = 0; i < n; i++) printf "%s", c }'; }

# dcase NAME: a clean directory $d for one case.
dcase() { echo "== $1"; rm -rf "$d"; mkdir -p "$d"; steps=0; }
# put PATH TEXT (written as it is): a file of the case. putp PATH FORMAT [ARGS]: the text is a printf format. putf PATH FILE: a copy. link PATH TARGET: a symbolic link.
put() { mkdir -p "$(dirname "$d/$1")"; printf '%s' "$2" > "$d/$1"; }
putp() { _p=$1; shift; mkdir -p "$(dirname "$d/$_p")"; printf "$@" > "$d/$_p"; }
putf() { mkdir -p "$(dirname "$d/$1")"; cp "$2" "$d/$1"; }
link() { mkdir -p "$(dirname "$d/$1")"; ln -s "$2" "$d/$1"; }
copytree() { mkdir -p "$(dirname "$2")"; rm -rf "$2"; cp -R "$1" "$2"; }

# show LABEL FILE: a text, inline up to 400 bytes.
show() {
    _n=$(wc -c < "$2" | tr -d ' ')
    if [ "$_n" = 0 ]; then echo "  $1: (nothing)"
    elif [ "$_n" -le 400 ]; then sed "s/^/  $1| /" "$2"
    else echo "  $1: $(wc -l < "$2" | tr -d ' ') lines, $(cksum < "$2" | awk '{print $1 " " $2}')"; fi
}

# vstep COMMAND ARGS...: `cancho vcs COMMAND ARGS` in $d; the status is left in $code.
vstep() {
    steps=$((steps + 1))
    _cmd=$1
    _shown="$*"
    _shown=${_shown#"$_cmd"}; _shown=${_shown# }
    _dir=0; for _a in "$@"; do [ "$_a" = --dir ] && _dir=1; done
    vrun "$@"
    norm < "$work/s.out" > "$work/s.nout"
    vreport
}
# vrun COMMAND ARGS...: the command run in $d with its output in $work/s.out and $work/s.err, and its status in $code.
vrun() {
    _cmd=$1; shift
    if [ -n "$oracle" ]; then
        (cd "$d" && CANCHO_CACHE=$cache "$cancho" vcs "$_cmd" "$@") > "$work/s.out" 2> "$work/s.err"
        code=$?
    else
        case $_cmd in
            lock)
                _git=0; _last=; for _a in "$@"; do [ "$_a" = --git ] && _git=1; _last=$_a; done
                if [ $_git = 1 ] && [ "$_last" != --git ]; then set -- "$@" --cache "$cache"; fi ;;
            log) ;;
            sync) set -- --std-dir "$stddir" "$@" ;;
            *) set -- --cache "$cache" --std-dir "$stddir" "$@" ;;
        esac
        (cd "$d" && "$cancho" vcs "$_cmd" "$@") > "$work/s.out" 2> "$work/s.err"
        code=$?
    fi
}
# vreport: what the last vstep did, in the transcript.
vreport() {
    if [ "$_cmd" = sync ]; then
        awk '/^    / { next } { t = $0; sub(/^ +/, "", t); sub(/ +$/, "", t); if (t == "^") next }
             /^warning: / && !/^warning: collision:/ { print "warning:"; next } { print }' "$work/s.nout" > "$work/s.nout2"
        mv "$work/s.nout2" "$work/s.nout"
    fi
    head -n 1 "$work/s.err" | norm | awk '{ if ($0 ~ /^cannot /) { first = index($0, "`"); if (first) { rest = substr($0, first + 1); second = index(rest, "`"); if (second) $0 = substr($0, 1, first + second) } } print }' > "$work/s.nerr"
    if [ "$_cmd" != lock ] && [ "$code" = 1 ]; then
        _first=$(cat "$work/s.nerr")
        case $_first in
            *'was not given `--std`'*) ;;
            *)
                _own=0
                if [ $_dir = 1 ]; then
                    case $_first in
                        *'modules import each other'*|*'module `'*|*'no file of'*|*'no `.cho` files'*|*'which is neither `std`'*) _own=1 ;;
                    esac
                fi
                [ $_own = 1 ] || : > "$work/s.nerr" ;;
        esac
    fi
    echo "step $steps: vcs $_cmd $(printf '%s' "$_shown" | norm)"
    echo "  exit $code"
    show out "$work/s.nout"
    show err "$work/s.nerr"
}

# dend [DIR...]: every file the case left, by path; the files beneath each DIR given are one line, their number and the checksum of their paths and checksums.
dend() {
    rm -f "$work"/agg.*
    (cd "$d" && find . \( -type f -o -type l \) | LC_ALL=C sort) | while IFS= read -r _p; do
        if [ -L "$d/$_p" ]; then
            _line="link $_p -> $(readlink "$d/$_p" | norm)"
        else
            _line="file $_p $(norm < "$d/$_p" | cksum | awk '{print $1 " " $2}')"
        fi
        _agg=
        _k=0
        for _pre in "$@"; do
            _k=$((_k + 1))
            case $_p in ./$_pre/*) _agg=$_k ;; esac
        done
        if [ -n "$_agg" ]; then echo "$_line" >> "$work/agg.$_agg"; else echo "  $_line"; fi
    done
    _k=0
    for _pre in "$@"; do
        _k=$((_k + 1))
        if [ -f "$work/agg.$_k" ]; then echo "  files beneath $_pre: $(wc -l < "$work/agg.$_k" | tr -d ' '), $(cksum < "$work/agg.$_k" | awk '{print $1 " " $2}')"; else echo "  files beneath $_pre: 0"; fi
    done
}

# store_with DIR < ENTRIES: a store with a manifest of the entries, one `SIG TAB NAME TAB STD` a line (STD is true, false or nothing: nothing leaves `uses_std` out).
ent() { printf '%s\t%s\t%s\n' "$(sig "$1")" "$2" "$3"; }
store_with() {
    mkdir -p "$1"
    awk -F'\t' -v b="$(sig b)" -v c="$(sig c)" '
        BEGIN { printf "{\n  \"entries\": {" }
        { printf "%s", (NR == 1 ? "\n" : ",\n"); flag = ($3 == "") ? "" : ",\n      \"uses_std\": " $3
          printf "    \"%s\": {\n      \"name\": \"%s\",\n      \"stage_id\": \"%s\",\n      \"source_hash\": \"%s\"%s\n    }", $1, $2, b, c, flag }
        END { if (NR == 0) printf "}\n}"; else printf "\n  }\n}" }' > "$1/manifest.json"
}
# manifest_lit FILE < ENTRIES: a manifest of `SIG TAB NAME-AS-A-JSON-LITERAL` lines.
manifest_lit() {
    mkdir -p "$(dirname "$1")"
    awk -F'\t' -v b="$(sig b)" -v c="$(sig c)" '
        BEGIN { printf "{\n  \"entries\": {" }
        { printf "%s", (NR == 1 ? "\n" : ",\n")
          printf "    \"%s\": {\n      \"name\": %s,\n      \"stage_id\": \"%s\",\n      \"source_hash\": \"%s\"\n    }", $1, $2, b, c }
        END { if (NR == 0) printf "}\n}"; else printf "\n  }\n}" }' > "$1"
}

# stores_of_packages: the directories under packages/ that hold a manifest of the store (the 26, with the stores inside the ones that hold several), sorted, as paths relative to packages/.
stores_of_packages() { (cd "$packages" && find . -path '*/.cancho-vcs/*' -name manifest.json | sed -e 's#^\./##' -e 's#/manifest.json$##' | LC_ALL=C sort); }

# A git repository of a copy of packages/ (`repo`), and one of a single store at its root (`plainrepo`): `repo_url` and `repo_rev` say where and what. The rev and the URL are not
# in the transcript as they are, so that the transcript does not depend on where it ran.
gitq() { _g=$1; shift; GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$_g" -c user.email=t@example.com -c user.name=t -c commit.gpgsign=false "$@"; }
make_repo() {
    # make_repo NAME SOURCE-DIR SUBDIR-OR-EMPTY: leaves repo_url and repo_rev
    _r=$work/$1
    rm -rf "$_r"; mkdir -p "$_r"
    gitq "$_r" init --quiet .
    if [ -n "$3" ]; then cp -R "$2" "$_r/$3"; else cp -R "$2/." "$_r/"; fi
    gitq "$_r" add -A
    gitq "$_r" commit --quiet -m stores
    repo_url=file://$_r
    repo_rev=$(gitq "$_r" rev-parse HEAD)
    addnorm "$repo_url" '<url>'
    addnorm "$_r" '<repo>'
    addnorm "$repo_rev" "<rev:$1>"
}

# mklock OUT ARGS...: a lock made by `vcs lock -o OUT ARGS` in a directory of its own, left in the file $work/OUT. (The locks that a command is given are made by the command under test: cli_vcs_lock.sh
# holds `vcs lock` to what the Rust command wrote.)
mklock() {
    _o=$1; shift
    _save=$d; d=$work/make-lock; rm -rf "$d"; mkdir -p "$d"
    vrun lock -o made.lock "$@"
    if [ "$code" != 0 ]; then fail "the lock of $* was not made (exit $code): $(head -c 200 "$work/s.err")"; fi
    cp "$d/made.lock" "$work/$_o"
    d=$_save
}
