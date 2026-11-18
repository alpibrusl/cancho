# tls_echo_fixed.sh -- the script of tests/commands/tls_scripts.cho for `conformance/tls_echo_fixed.rs` (deleted): examples/tls_echo_fixed (docs/narrowing-into-several.md section 11.1), `tls_echo` with the certificate directory fixed at
# build time, so `main` narrows the filesystem to two literals and the authority report has no `fs_read("")`. It shares `serve`, the loop and the options with `tls_echo` (examples/tls_echo/echo.cho, front.cho), which the
# duplication test keeps true. $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM).
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/tls_lib.sh

E=tests/vectors/tls/echo
FIXED=/etc/cancho/tls_echo
main=examples/tls_echo_fixed/tls_echo_fixed.cho
others="examples/tls_echo/echo.cho examples/tls_echo/front.cho examples/tls_echo/identity.cho $tls_package"

# The report, pinned as `tls_echo`'s is, with the one difference the example exists for: the two `fs_read` labels name /dev/urandom and the certificate directory, and none names "". Every other label is `tls_echo`'s.
begin "the report names the two paths and no fs_read of the root (16 labels, pinned)"
tls_authority tests/commands/tls_echo_fixed.labels $main $others
grep -q '"fs_read", "argument": ""' "$work/authority.json" && fail "an fs_read of the root"
end

# The shipped example with FIXED replaced by DIR, built as $work/NAME (a test cannot create /etc/cancho; the literal is the only difference).
fixed_build() { # DIR NAME
    grep -q "narrow(fs, \"/dev/urandom\", \"$FIXED\")" "$main" || { fail "main narrows to the two literals"; return 1; }
    sed "s|$FIXED|$1|g" "$main" > "$work/$2.cho"
    tls_build "$2" "$work/$2.cho" $others
}

certs=$work/certs
mkdir -p "$certs"
for f in chain.pem key.pem names; do cp "$E/first/$f" "$certs/$f"; done

# It serves from the fixed directory with no `--dir`, echoes to a client that verifies the chain, refuses `--dir` (a usage error, status 2), exits 5 when the fixed directory is absent, and stops cleanly on SIGTERM.
begin "it serves from the fixed directory and refuses --dir (8 connections; status 2, 0 and 5)"
if fixed_build "$certs" fixed && tls_build many tests/programs/tls_many.cho $tls_package; then
    "$work/fixed" --port 1 --dir "$certs" > /dev/null 2>&1
    same "$?" 2 "--dir is not an option here"
    if tls_start "$work/fixed.out" listening -- "$work/fixed" --port @PORT@ --idle 500 --handshakes 4 --connections 16; then
        tls_clients 8 16384
        tls_wait "$work/fixed.out" " established suite=" 8
        kill -TERM "$spid"
        tls_end
        same "$status" 0 "the server's status after SIGTERM"
        tls_kill
    fi
    # The directory the literal names is missing: status 5, nothing listening.
    if fixed_build "$work/absent" absent; then
        "$work/absent" --port 40123 > /dev/null 2>&1
        same "$?" 5 "the directory is missing"
    fi
fi
end
