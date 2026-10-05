#!/bin/sh
# Diff the lex-sys lexer (examples/selfhost/lexer.ls) against the Rust one.
# usage: diff.sh ORACLE_BIN LEXER_BIN file...
# ORACLE_BIN reads stdin and prints "Kind start end" lines (or "ERR rule start end");
# LEXER_BIN is the compiled lexer.ls.
oracle=$1; lexer=$2; shift 2
pass=0; fail=0
for f in "$@"; do
  "$oracle" < "$f" > /tmp/selfhost.a 2>&1
  "$lexer" < "$f" > /tmp/selfhost.b 2>&1
  if cmp -s /tmp/selfhost.a /tmp/selfhost.b; then pass=$((pass+1)); else fail=$((fail+1)); echo "DIFF $f"; fi
done
echo "identical: $pass  different: $fail"
