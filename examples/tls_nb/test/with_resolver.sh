#!/bin/sh
# Run a command with /etc/resolv.conf replaced, in a private mount namespace (the host's file is not touched):
#
#     unshare -m ./with_resolver.sh <resolv.conf> <command> [args...]
#
# libc's resolver (getaddrinfo, res_query) reads only that file for its name servers and has no port to configure, so the test
# name server (dns_stub.py --port 53) is reached by pointing the command's own view of the file at 127.0.0.1.
set -eu
conf=$1
shift
mount --bind "$conf" /etc/resolv.conf
exec "$@"
