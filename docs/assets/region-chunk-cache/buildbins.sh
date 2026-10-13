#!/bin/sh
# usage: buildbins.sh <src tree> <cancho binary> <out dir>
set -e
src=$1; cancho=$2; out=$3
mkdir -p "$out"
cd "$src"
TLS="packages/tls/tls.cho packages/tls/record.cho packages/tls/message.cho packages/tls/slot.cho packages/tls/cident.cho packages/tls/client12.cho packages/tls/client.cho packages/tls/hello.cho packages/tls/identity.cho packages/tls/clientauth.cho packages/tls/ticket.cho packages/tls/server.cho packages/x509/verify.cho packages/x509/names.cho packages/x509/x509.cho packages/x509/key.cho"
"$cancho" build --std examples/tls_echo/tls_echo.cho examples/tls_echo/echo.cho examples/tls_echo/front.cho examples/tls_echo/identity.cho $TLS -o "$out/tls_echo"
"$cancho" build --std tests/programs/tls_many.cho $TLS -o "$out/tls_many"
[ -x "$out/tconn" ] || gcc -O2 -o "$out/tconn" benches/server/tconn.c -lssl -lcrypto
[ -x "$out/oserv" ] || gcc -O2 -o "$out/oserv" benches/server/oserv.c -lssl -lcrypto
echo built "$out"
