#!/bin/sh
# Diff a lex-sys front end written in lex-sys against the Rust one: the lexer
# (examples/selfhost/lexer.ls) or the parser (examples/selfhost/parser.ls).
# usage: diff.sh ORACLE_BIN PORT_BIN file...
#
# The oracles are examples of lex-sys-syntax, and the ports are built with lexcore.ls:
#
#   cargo build -p lex-sys-syntax --example dump_tokens --example dump_ast
#   lex-sys build examples/selfhost/lexer.ls examples/selfhost/lexcore.ls --std -o lexer
#   lex-sys build examples/selfhost/parser.ls examples/selfhost/lexcore.ls --std -o parser
#   diff.sh target/debug/examples/dump_tokens ./lexer $(find . -name '*.ls')
#   diff.sh target/debug/examples/dump_ast ./parser $(find . -name '*.ls')
#
# ORACLE_BIN and PORT_BIN read a source file on stdin and print the same bytes.
oracle=$1; port=$2; shift 2
pass=0; fail=0
for f in "$@"; do
  "$oracle" < "$f" > /tmp/selfhost.a 2>&1
  "$port" < "$f" > /tmp/selfhost.b 2>&1
  if cmp -s /tmp/selfhost.a /tmp/selfhost.b; then pass=$((pass+1)); else fail=$((fail+1)); echo "DIFF $f"; fi
done
echo "identical: $pass  different: $fail"
