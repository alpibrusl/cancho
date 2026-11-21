# front_end_syntax.sh -- the script of tests/commands/script_tests.cho for five tests of `conformance/selfhost.rs` (deleted): the lexer, the parser, the printer, the JSON printer and the
# formatter of compiler against the Rust front end (cancho-syntax). The oracle ran once and its answers are in tests/vectors/frontend/ (tests/commands/frontend_lib.sh says how):
# each port, built by the compiler under test, is fed every program of the corpus (std/, examples/, packages/, tests/accept, tests/reject, tests/programs, and the edge cases of the Rust
# tests, now files of tests/vectors/frontend/cases/) and must answer what the oracle did, by digest. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/frontend_lib.sh

# unchanged VEC: the lines of the formatter's vector whose answer is the input itself.
unchanged() { awk '$4 == "ok" && $2 == $3 { n++ } END { print n + 0 }' "$V/$1.vec"; }

begin the_lexer_in_cancho_agrees_with_the_rust_lexer
port lexer lexer lexer.cho lexcore.cho && {
    same "$(lines_of $V/lexer.vec)" 1050 "the lexer's vector has 1050 programs"
    same "$(tags lexer '^ERR')" 14 "of which the lexer refuses"
    replay lexer "$work/lexer"
}
end

begin the_parser_in_cancho_agrees_with_the_rust_parser
port parser parser && {
    same "$(lines_of $V/parser.vec)" 1050 "the parser's vector has 1050 programs"
    same "$(tags parser '^ERR')" 56 "of which the parser refuses"
    replay parser "$work/parser"
}
end

begin the_printer_in_cancho_agrees_with_the_rust_printer
port printer printer && {
    same "$(lines_of $V/printer.vec)" 2111 "the printer's vector has 2111 programs"
    replay printer "$work/printer"
}
end

begin the_json_printer_in_cancho_agrees_with_the_rust_one
port jsonprinter jsonprinter && {
    same "$(lines_of $V/jsonprinter.vec)" 1113 "the JSON printer's vector has 1113 programs"
    replay jsonprinter "$work/jsonprinter"
    # A literal of more than 120 significant digits has bits this port does not work out: it says so, and writes no JSON.
    {
        printf 'fn f() -> [] float { return 1'
        i=0
        while [ $i -lt 130 ]; do
            printf 0
            i=$((i + 1))
        done
        printf '.0; }\n'
    } > "$work/long.cho"
    said=$("$work/jsonprinter" < "$work/long.cho" | head -c 7)
    same "$said" "CANNOT " "a literal of 130 digits is not written"
}
end

begin the_formatter_in_cancho_agrees_with_the_rust_formatter
port formatter formatter && port printer printer && {
    same "$(lines_of $V/formatter.vec)" 1963 "the formatter's vector has 1963 programs"
    # The comparison is only worth something if it covers files that change, and files that are refused.
    [ "$(tags formatter '^CANNOT')" -ge 10 ] || fail "fewer than 10 programs the formatter refuses"
    [ "$(unchanged formatter)" -gt 300 ] || fail "fewer than 300 programs already formatted"
    [ $(($(tags formatter '^ok') - $(unchanged formatter))) -gt 400 ] || fail "fewer than 400 programs the formatter changes"
    replay formatter "$work/formatter"
}
end
