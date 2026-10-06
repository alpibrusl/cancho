#!/bin/sh
# Diff a cancho front end written in cancho against the Rust one: the lexer
# (examples/selfhost/lexer.cho) or the parser (examples/selfhost/parser.cho).
# usage: diff.sh ORACLE_BIN PORT_BIN file...
#
# The oracles are examples of cancho-syntax, and the ports are built with lexcore.cho (the parser and the checker with the modules they share):
#
#   cargo build -p cancho-syntax --example dump_tokens --example dump_ast
#   cancho build examples/selfhost/lexer.cho examples/selfhost/lexcore.cho --std -o lexer
#   MODS='driver listing pass1 ast kinds lexcore tables'   # examples/selfhost/<name>.cho
#   cancho build examples/selfhost/parser.cho $(for m in $MODS; do echo examples/selfhost/$m.cho; done) --std -o parser
#   diff.sh target/debug/examples/dump_tokens ./lexer $(find . -name '*.cho')
#   diff.sh target/debug/examples/dump_ast ./parser $(find . -name '*.cho')
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
