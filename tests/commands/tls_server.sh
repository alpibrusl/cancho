# tls_server.sh -- the script of tests/commands/tls_scripts.cho for `conformance/tls_server.rs` (deleted): packages/tls's server with no network (docs/tls-server.md section 8, step 2). The connections of
# scripts/tls_liar_client.py's lying client replayed byte for byte through tests/programs/tls_server_driver.cho, each ending with its tag; and the honest connections again with the client's bytes fed one byte a line.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM): the Cranelift leg of the Rust tests went with D2.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/tls_lib.sh

V=tests/vectors/tls

begin "every lying client is refused with its own tag (190)"
if tls_build driver tests/programs/tls_server_driver.cho $tls_package; then
    count=$(tls_cases "$V/liar_client.txt" "$work/liar" tagged)
    same "$count" 190 "the number of cases"
    i=1
    while [ "$i" -le "$count" ]; do
        c=$work/liar/$i; cname=$(cat "$c.name"); tag=$(cat "$c.tag")
        same "$(tls_lines "$c.asked")" "$(tls_lines "$c.answered")" "$cname: questions and answers"
        if tls_run "$work/driver" "$c" "$cname"; then
            last_op=$(tail -n 1 "$c.asked" | cut -c1)
            case $last_op in
                F | Z) tls_ends "$c.got" "$tag" 5 "$cname" ;;
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

begin "the client's bytes in any split give the same connection (the honest connections, one byte a line)"
if [ -x "$work/driver" ]; then
    count=$(tls_cases "$V/liar_client.txt" "$work/liar" tagged)
    honest=0
    i=1
    while [ "$i" -le "$count" ]; do
        c=$work/liar/$i; cname=$(cat "$c.name"); tag=$(cat "$c.tag")
        case $cname in honest*) named=1 ;; *) named=0 ;; esac
        if [ "$tag" = ok ] && [ "$named" = 1 ] && grep -q '^Q' "$c.asked"; then
            honest=$((honest + 1))
            tls_bytewise "$c.asked" "$work/split.asked"
            if tls_ask "$work/driver" "$work/split.asked" "$cname"; then
                same "$(tls_total "$work/split.asked" "$work/ask.got" 4 VFWQZ)" "$(tls_total "$c.asked" "$c.answered" 4 VFWQZ)" "$cname: what the server sent"
                same "$(tls_total "$work/split.asked" "$work/ask.got" 5 VFWQZ)" "$(tls_total "$c.asked" "$c.answered" 5 VFWQZ)" "$cname: what it received"
            fi
        fi
        i=$((i + 1))
    done
    [ "$honest" -ge 20 ] || fail "$honest honest connections, wanted at least 20"
    # The number is pinned: a recording that loses honest connections is a narrower test.
    same "$honest" 26 "the number of honest connections replayed bytewise"
fi
end
