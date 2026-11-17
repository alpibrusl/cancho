# front_end_checks.sh -- the script of tests/commands/script_tests.cho for four tests of `conformance/selfhost.rs` (deleted): the declarations checker (`check.cho`, against
# `check_declarations` of cancho-ir), the body checker (`bodies.cho`, against `check_bodies`, function by function), and the three ports that read several files (`parser_files.cho`,
# `check_files.cho`, `bodies_files.cho`). The oracle ran once; its answers are in tests/vectors/frontend/ (check.vec, bodies.sections, parser_files.vec, check_files.vec,
# bodies_files.sections; tests/commands/frontend_lib.sh says how they are read). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/frontend_lib.sh

# The checker, twice: each program alone, which is how it stops at its first `import std...` (1,219), and with the library, which is what `cancho check --std` does and what makes the
# program's own declarations meet the library's (432, a sample, because each parses half a megabyte).
begin the_checker_in_cancho_agrees_with_the_rust_declarations_check
port check check && port check_files check_files && {
    same "$(lines_of $V/check.vec)" 1651 "the vector of the checker has 1651 programs"
    same "$(awk '{ n[$1]++ } END { print n["s"] " " n["l"] }' $V/check.vec)" "1219 432" "the modes of the vector"
    [ "$(tags check '^ERR')" -gt 200 ] || fail "fewer than 201 programs refused"
    replay check "$work/check" s
    replay check "$work/check_files" l
}
end

# The body checker: a function the port answers `SKIP` for is not compared, and enough must be that the test cannot pass by skipping. Alone (2,540 programs), and with the library after a
# sample (274, and the library's own functions with each, moved on by the program's length: tests/vectors/frontend/bodies_library.txt).
begin the_body_checker_in_cancho_agrees_with_the_rust_one_per_function
port bodies bodies && port bodies_files bodies_files && {
    same "$(grep -c '^## ' $V/bodies.sections)" 2814 "the vector of the body checker has 2814 programs"
    same "$(grep -c '^## s ' $V/bodies.sections)" 2540 "of which 2540 alone"
    replay_sections bodies "$work/bodies" s
    _c=$compared
    replay_sections bodies "$work/bodies_files" l
    _c=$((_c + compared))
    [ "$_c" -gt 20000 ] || fail "compared $_c lines, skipped $skipped"
}
end

begin the_ports_read_several_files_as_the_compiler_does
port parser_files parser_files && port check_files check_files && port bodies_files bodies_files && {
    same "$(lines_of $V/parser_files.vec)" 42 "the vector of the parser has 42 programs"
    same "$(lines_of $V/check_files.vec)" 42 "the vector of the checker has 42 programs"
    same "$(grep -c '^## ' $V/bodies_files.sections)" 42 "the vector of the body checker has 42 programs"
    replay parser_files "$work/parser_files"
    replay check_files "$work/check_files"
    replay_sections bodies_files "$work/bodies_files"
}
end
