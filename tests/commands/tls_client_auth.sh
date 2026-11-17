# tls_client_auth.sh -- the script of tests/commands/tls_scripts.cho for `conformance/tls_client_auth.rs` (deleted): packages/tls's server with client certificates and no network (docs/tls-server.md section 13). The connections of
# scripts/tls_liar_client_auth.py replayed byte for byte through tests/programs/tls_server_driver.cho, each ending with its tag. Every rule of 13.2 to 13.4 and every tag of 13.7 is reached; every honest case asks the
# server for the client's identity (`P`) and the recording holds the answer, which the script compared with what pyca reads from the same certificate. The honest connections again with the client's bytes fed one byte a line.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM): the Cranelift leg of the Rust tests went with D2.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/tls_lib.sh

R=tests/vectors/tls/liar_client_auth.txt

begin "every client certificate case replays with its own tag (81)"
if tls_build driver tests/programs/tls_server_driver.cho $tls_package; then
    count=$(tls_cases "$R" "$work/cases" tagged)
    same "$count" 81 "the number of cases"
    i=1
    while [ "$i" -le "$count" ]; do
        c=$work/cases/$i; cname=$(cat "$c.name"); tag=$(cat "$c.tag")
        same "$(tls_lines "$c.asked")" "$(tls_lines "$c.answered")" "$cname: questions and answers"
        if tls_run "$work/driver" "$c" "$cname"; then
            last_op=$(tail -n 1 "$c.asked" | cut -c1)
            case $last_op in
                F | Z) [ "$tag" = ok ] || tls_ends "$c.got" "$tag" 5 "$cname" ;;
                *)
                    if [ "$tag" != ok ]; then
                        [ "$(awk -F'[ ]' -v t="$tag" '$2 == t' "$c.got" | wc -l | tr -d ' ')" -gt 0 ] || fail "$cname: $tag on some line"
                    fi
                    ;;
            esac
        fi
        i=$((i + 1))
    done
fi
end

begin "the recording reaches every refusal of the client's flight (22 tags)"
n=0
for want in ok tls-server-client-cert-required tls-server-client-sigalg tls-server-client-store tls-server-client-auth-config tls-server-client-store-replaced tls-unexpected-message tls-server-illegal-parameter tls-unsupported-extension tls-decode-error tls-bad-certificate-verify tls-server-finished x509-decode x509-chain-too-large x509-path-too-long x509-unsupported-algorithm x509-unknown-issuer x509-not-ca x509-expired x509-not-yet-valid x509-key-usage x509-name-constraint; do
    n=$((n + 1))
    grep -q "^## $want " "$R" || fail "no case ends with $want"
done
same "$n" 22 "the number of tags"
end

begin "the client's flight in any split gives the same connection (the honest connections, one byte a line)"
if [ -x "$work/driver" ]; then
    count=$(tls_cases "$R" "$work/cases" tagged)
    honest=0
    i=1
    while [ "$i" -le "$count" ]; do
        c=$work/cases/$i; cname=$(cat "$c.name"); tag=$(cat "$c.tag")
        case $cname in "honest: required"*) named=1 ;; *) named=0 ;; esac
        if [ "$tag" = ok ] && [ "$named" = 1 ] && grep -qx 'P' "$c.asked"; then
            honest=$((honest + 1))
            tls_bytewise "$c.asked" "$work/split.asked"
            if tls_ask "$work/driver" "$work/split.asked" "$cname"; then
                same "$(tls_total "$work/split.asked" "$work/ask.got" 4 VFWQZ)" "$(tls_total "$c.asked" "$c.answered" 4 VFWQZ)" "$cname: sent"
                same "$(tls_total "$work/split.asked" "$work/ask.got" 5 VFWQZ)" "$(tls_total "$c.asked" "$c.answered" 5 VFWQZ)" "$cname: received"
                # The answer to the first `P` question: the client's identity.
                same "$(paste -d'|' "$work/split.asked" "$work/ask.got" | awk -F'|' '$1 == "P" { print $2; exit }')" "$(paste -d'|' "$c.asked" "$c.answered" | awk -F'|' '$1 == "P" { print $2; exit }')" "$cname: the identity"
            fi
        fi
        i=$((i + 1))
    done
    [ "$honest" -ge 10 ] || fail "$honest honest connections, wanted at least 10"
    same "$honest" 14 "the number of honest connections replayed bytewise"
fi
end
