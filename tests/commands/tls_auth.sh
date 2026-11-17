# tls_auth.sh -- the script of tests/commands/tls_scripts.cho for `conformance/tls_auth.rs` (deleted): packages/tls's client certificates and ALPN with no network (docs/tls-parity.md section 6). The connections of
# scripts/tls_liar_auth.py's lying server replayed byte for byte, each ending with its tag, and the engine's rules for client identities and ALPN (scripts/tls_tickets_auth.py) replayed too. The recording's server checked, with
# pyca/cryptography, the client's Certificate, its CertificateVerify and the Finished that covers both, so a replay that matches is a signature that verified when it was recorded.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM): the Cranelift leg of the Rust tests went with D2.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/tls_lib.sh

V=tests/vectors/tls

begin "every client certificate and ALPN case replays and ends with its tag (88)"
if tls_build driver tests/programs/tls_driver.cho $tls_client_package; then
    count=$(tls_cases "$V/liar_auth.txt" "$work/auth" tagged)
    same "$count" 88 "the number of cases"
    i=1
    while [ "$i" -le "$count" ]; do
        c=$work/auth/$i; cname=$(cat "$c.name"); tag=$(cat "$c.tag")
        same "$(tls_lines "$c.asked")" "$(tls_lines "$c.answered")" "$cname: questions and answers"
        if tls_run "$work/driver" "$c" "$cname"; then
            # A connection ends closed (4) or failed (5); the identity's and the offer's own refusals are single lines, answered at event 0.
            case $cname in identity:* | *"refused when set") configuration=1 ;; *) configuration=0 ;; esac
            if [ "$tag" = ok ]; then want=4; elif [ "$configuration" = 1 ]; then want=0; else want=5; fi
            tls_ends "$c.got" "$tag" "$want" "$cname"
        fi
        i=$((i + 1))
    done
fi
end

begin "the engine keeps a ticket back when the identity changes and chooses it by host (14)"
if tls_build ticket_driver tests/programs/tls_tickets.cho $tls_package; then
    count=$(tls_cases "$V/tickets_auth.txt" "$work/tickets_auth" named)
    same "$count" 14 "the number of cases"
    i=1
    while [ "$i" -le "$count" ]; do
        c=$work/tickets_auth/$i
        tls_run "$work/ticket_driver" "$c" "$(cat "$c.name")"
        i=$((i + 1))
    done
fi
end
