# parity.sh -- the script of tests/commands/script_tests.cho for three tests of `conformance/selfhost/parity.rs` (deleted): what the Rust crates had that the compiler in cancho
# (compiler) does not know about yet cannot go unnoticed. The catalogues are tests/rules.golden (the tag of every refusal rule) and tests/builtins.golden (the name of every builtin),
# recorded from `cancho_syntax::Rule::ALL` and `cancho_ir::Builtin::ALL` on 2026-11-17 (the test that held the two files equal to the crates, `rules_and_builtins_are_the_goldens`, went with
# them: the two files are the record now). A rule or a builtin that no source of compiler names fails here until the compiler in cancho has it, or the name is listed in
# tests/unported_rules.txt or tests/unported_builtins.txt with the reason; and a listed name that has been ported fails until its line is taken out, so the lists only shrink.
# $1 is the compiler (unused), $2 a scratch directory; run from the root of the repository.
work=$2
. tests/commands/script_lib.sh

D=compiler

# listed FILE: the names a list holds: a line is a name, then perhaps a `#` and why.
listed() { sed 's/#.*//' "$1" | awk 'NF { print $1 }' | sort -u; }

# parity KIND GOLDEN LIST SOURCES...: the names of GOLDEN that no source holds as a string literal and LIST does not have are missing; a name of LIST that a source holds, or that GOLDEN lacks, is stale.
parity() {
    _kind=$1
    _golden=$2
    _list=$3
    shift 3
    sort -u "$_golden" > "$work/wanted"
    : > "$work/known"
    while read -r _name; do
        # shellcheck disable=SC2068
        if cat $@ | grep -F -q "\"$_name\""; then echo "$_name" >> "$work/known"; fi
    done < "$work/wanted"
    sort -u "$work/known" -o "$work/known"
    listed "$_list" > "$work/listed"
    _missing=$(comm -23 "$work/wanted" "$work/known" | comm -23 - "$work/listed" | tr '\n' ' ')
    [ -z "$_missing" ] || fail "these ${_kind}s of the Rust crates are not known to the compiler in cancho ($D) and are not listed in $_list: $_missing"
    _stale=$( (comm -12 "$work/listed" "$work/known"; comm -23 "$work/listed" "$work/wanted") | sort -u | tr '\n' ' ')
    [ -z "$_stale" ] || fail "$_list lists ${_kind}s that are ported now, or that Rust no longer has; take them out: $_stale"
}

begin every_rule_of_the_rust_front_end_is_known_to_the_compiler_in_cancho
same "$(wc -l < tests/rules.golden | tr -d ' ')" 64 "tests/rules.golden has 64 rules"
parity rule tests/rules.golden tests/unported_rules.txt $D/*.cho
# And the other way: a name in rules.cho that Rust does not have is a stale one.
sort -u tests/rules.golden > "$work/wanted"
grep '^ *return "' $D/rules.cho | sed 's/^ *return "//; s/";.*//' | sort -u > "$work/ruled"
_alien=$(comm -23 "$work/ruled" "$work/wanted" | grep -v -x -e '' -e SKIP | tr '\n' ' ')
[ -z "$_alien" ] || fail "rules.cho names $_alien, which the Rust crates do not have"
end

begin every_builtin_of_the_rust_front_end_is_named_by_the_compiler_in_cancho
# The tables are generated from the Rust ones, so naming a builtin there says nothing; the code is elsewhere.
# 187 are the builtins of the Rust crates; a builtin that only the compiler in cancho has is appended after them (docs/builtin-tables.md): dir_nlink is the 188th, and the six `mmio_*` of docs/mmio.md and the three `cpu_*` of docs/cpu.md follow.
same "$(wc -l < tests/builtins.golden | tr -d ' ')" 197 "tests/builtins.golden has 197 builtins"
_sources=
for _f in $D/*.cho; do
    case $_f in $D/tables.cho | $D/builtins.cho | $D/runtime.cho) ;; *) _sources="$_sources $_f" ;; esac
done
# shellcheck disable=SC2086
parity builtin tests/builtins.golden tests/unported_builtins.txt $_sources
end

# A module the compiler imports but the list of the compiler's files leaves out makes it refuse its own source with `unknown-name`: this says which list, before a long test does.
begin every_module_of_the_compiler_is_in_the_lists_the_tests_keep_of_them
: > "$work/closure"
queue=compiler
while [ -n "$queue" ]; do
    queue=${queue# }
    cur=${queue%% *}
    case $queue in *' '*) queue=${queue#* } ;; *) queue= ;; esac
    for m in $(sed -n 's/^ *import selfhost\.\([A-Za-z0-9_]*\).*/\1/p' "$D/$cur.cho"); do
        if [ -f "$D/$m.cho" ] && ! grep -q -x "$m" "$work/closure"; then
            echo "$m" >> "$work/closure"
            queue="$queue $m"
        fi
    done
done
[ "$(wc -l < "$work/closure" | tr -d ' ')" -gt 50 ] || fail "the import closure of compiler.cho has fewer than 51 modules"
for m in $(cat "$work/closure"); do
    grep -q -x "$m.cho" "$D/cancho.files" || fail "the module $m of $D is not in $D/cancho.files, which a front-end program is built from"
done
end
