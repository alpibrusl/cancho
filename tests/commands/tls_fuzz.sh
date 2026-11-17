# tls_fuzz.sh -- the script of tests/commands/tls_scripts.cho for `conformance/tls_fuzz.rs` (deleted): the fuzzing corpus as a regression test (docs/tls-assurance.md section 3.5). Every input committed under
# tests/vectors/fuzz/HARNESS/ (AFL++'s minimised queue from the campaign section 3.4 reports) is run through its harness, tests/programs/fuzz_HARNESS.cho, and must exit 0: a trap ends the process with a signal, which is the
# failure the fuzzer looked for (`hello` and `server` are the TLS server's, docs/tls-server.md section 7). One backend (LLVM, the one AFL++ instrumented): the Cranelift run of the Rust test went with D2.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. The number of inputs of each corpus is pinned, so a corpus that loses inputs is a red check.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/tls_lib.sh

client_files="packages/tls/record.cho packages/tls/message.cho packages/tls/slot.cho packages/tls/cident.cho packages/tls/client12.cho packages/tls/client.cho packages/x509/verify.cho packages/x509/names.cho packages/x509/key.cho packages/x509/x509.cho"

# replay HARNESS INPUTS FILES...: the harness built with FILES and run on every input of its corpus.
replay() {
    h=$1; want=$2; shift 2
    begin "the $h corpus never traps ($want inputs)"
    dir=tests/vectors/fuzz/$h
    same "$(ls "$dir" | wc -l | tr -d ' ')" "$want" "the number of inputs"
    if tls_build "fuzz_$h" "tests/programs/fuzz_$h.cho" "$@"; then
        ran=0
        for input in "$dir"/*; do
            "$work/fuzz_$h" < "$input" > /dev/null 2>&1
            st=$?
            [ "$st" = 0 ] || fail "$input ends with status $st"
            ran=$((ran + 1))
        done
        same "$ran" "$want" "the number of inputs run"
    fi
    end
}

replay der 236 tests/programs/fuzz_common.cho tests/programs/fuzz_fixture.cho $client_files
replay chain 237 tests/programs/fuzz_common.cho tests/programs/fuzz_fixture.cho $client_files
replay messages 129 tests/programs/fuzz_common.cho tests/programs/fuzz_fixture.cho $client_files
replay client 398 tests/programs/fuzz_common.cho tests/programs/fuzz_fixture.cho $client_files
replay flight 410 tests/programs/fuzz_common.cho tests/programs/fuzz_fixture.cho $client_files
# The server's ClientHello parser (docs/tls-server.md section 7), from real ClientHellos and what AFL++ made of them.
replay hello 93 packages/tls/record.cho packages/tls/message.cho packages/tls/hello.cho packages/x509/x509.cho
# The whole server from `serve`, fed a client's bytes.
replay server 150 tests/programs/fuzz_server_fixture.cho tests/programs/fuzz_clientauth_fixture.cho $tls_package
# The client's Certificate and CertificateVerify (docs/tls-server.md section 13.11), from the good ones of each key type and what AFL++ made of them.
replay clientauth 272 tests/programs/fuzz_clientauth_fixture.cho $tls_package
