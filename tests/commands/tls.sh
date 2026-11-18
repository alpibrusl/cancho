# tls.sh -- the script of tests/commands/tls_scripts.cho for `conformance/tls.rs` (deleted): packages/tls with no network (docs/tls-core.md section 6.1). Recorded handshakes replayed byte for byte (tlslite-ng, `openssl s_server`), the TLS 1.2 PRF,
# the same bytes in any split, a wrong root, a crafted ServerHello for each rule of RFC 8446 section 4.1.3, the 84 connections of a lying server, the engine's rules for offering a ticket, 64 connections on one thread against recorded streams,
# one RFC 8448 record, and the record layer's opening in place. $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM): the Cranelift leg of the Rust tests went with D2.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/tls_lib.sh

V=tests/vectors/tls
TRACES="tlslite_rsa tlslite_ecdsa tlslite_aes256_x25519 tlslite_aes128_p256 tlslite_aes256_p384 openssl12_ecdhe_ecdsa_aes128_gcm_sha256 openssl12_ecdhe_ecdsa_aes256_gcm_sha384 openssl12_ecdhe_ecdsa_chacha20_poly1305 openssl12_ecdhe_rsa_aes128_gcm_sha256 openssl12_ecdhe_rsa_aes256_gcm_sha384 openssl12_ecdhe_rsa_chacha20_poly1305 tlslite_mutual_ecdsa openssl12_mutual_ecdhe_ecdsa_aes128_gcm_sha256 openssl13_mutual_ecdsa"

# rep TEXT N: TEXT N times.
rep() { _o=; _i=0; while [ "$_i" -lt "$2" ]; do _o="$_o$1"; _i=$((_i + 1)); done; printf '%s' "$_o"; }

begin "the driver builds"
tls_build driver tests/programs/tls_driver.cho $tls_client_package
end

if [ -x "$work/driver" ]; then

begin "every recorded handshake replays byte for byte (14)"
n=0
for t in $TRACES; do
    tls_cases "$V/$t.txt" "$work/t" trace > /dev/null
    tls_run "$work/driver" "$work/t/1" "$t" && same "$(tls_field "$work/t/1.answered" 3)" 4 "$t: closed"
    n=$((n + 1))
done
same "$n" 14 "the number of traces"
end

begin "every lying server is refused with its own tag (84)"
count=$(tls_cases "$V/liar.txt" "$work/liar" tagged)
same "$count" 84 "the number of cases"
i=1
while [ "$i" -le "$count" ]; do
    c=$work/liar/$i; cname=$(cat "$c.name"); tag=$(cat "$c.tag")
    same "$(tls_lines "$c.asked")" "$(tls_lines "$c.answered")" "$cname: questions and answers"
    if tls_run "$work/driver" "$c" "$cname"; then
        if [ "$tag" = ok ]; then want=4; else want=5; fi
        tls_ends "$c.got" "$tag" "$want" "$cname"
    fi
    i=$((i + 1))
done
end

begin "the same bytes in any split give the same connection (one byte a line, and the flight in one line)"
n=0
for t in $TRACES; do
    tls_cases "$V/$t.txt" "$work/t" trace > /dev/null
    c=$work/t/1
    want_out=$(tls_total "$c.asked" "$c.answered" 4 "")
    want_in=$(tls_total "$c.asked" "$c.answered" 5 "")
    tls_bytewise "$c.asked" "$work/split.asked"
    if tls_ask "$work/driver" "$work/split.asked" "$t: one byte a line"; then
        same "$(tls_total "$work/split.asked" "$work/ask.got" 4 "")" "$want_out" "$t: one byte a line, what the client sent"
        same "$(tls_total "$work/split.asked" "$work/ask.got" 5 "")" "$want_in" "$t: one byte a line, what the client received"
        same "$(tls_field "$work/ask.got" 3)" 4 "$t: closed"
    fi
    # Every F before the request in one line; the mutual traces begin with an `I` line, the client's identity, before `C`.
    awk '
        { line[NR] = $0 }
        END {
            for (i = 1; i <= NR; i++) { if (line[i] ~ /^C / && !start) start = i; if (line[i] ~ /^W / && !w) w = i }
            for (i = 1; i <= start; i++) print line[i]
            flight = ""; for (i = start + 1; i < w; i++) flight = flight substr(line[i], 3)
            print "F " flight
            for (i = w; i <= NR; i++) print line[i]
        }' "$c.asked" > "$work/coalesced.asked"
    if tls_ask "$work/driver" "$work/coalesced.asked" "$t: coalesced"; then
        same "$(tls_total "$work/coalesced.asked" "$work/ask.got" 4 "")" "$want_out" "$t: coalesced, what the client sent"
        same "$(tls_total "$work/coalesced.asked" "$work/ask.got" 5 "")" "$want_in" "$t: coalesced, what the client received"
    fi
    n=$((n + 1))
done
same "$n" 14 "the number of traces"
end

begin "a certificate from another CA is refused"
tls_cases "$V/tlslite_rsa.txt" "$work/rsa" trace > /dev/null
tls_cases "$V/tlslite_ecdsa.txt" "$work/ecdsa" trace > /dev/null
other_root=$(head -n 1 "$work/ecdsa/1.asked" | awk -F'[ ]' '{ print $4 }')
{
    head -n 1 "$work/rsa/1.asked" | awk -F'[ ]' -v root="$other_root" '{ print "C " $2 " " $3 " " root " " $5 }'
    sed -n 2,3p "$work/rsa/1.asked"
} > "$work/pin.asked"
if tls_ask "$work/driver" "$work/pin.asked" "pin"; then
    same "$(tls_field "$work/ask.got" 2)" x509-unknown-issuer "the tag"
    same "$(tls_field "$work/ask.got" 3)" 5 "failed"
fi
end

begin "every ServerHello rule is refused with its own tag (26), and a bit flipped in the encrypted flight is a bad record MAC"
start=$(head -n 1 "$work/ecdsa/1.asked")
# A ServerHello record answering the recorded ClientHello; TRAILER is bytes after the extensions, inside the message.
hello_with() { # VERSION RANDOM SID SUITE EXTS TRAILER
    _body="$1$2$(printf '%02x' $((${#3} / 2)))$3$4$(printf '00%04x' $((${#5} / 2)))$5$6"
    _msg="02$(printf '%06x' $((${#_body} / 2)))$_body"
    printf 'F 16030300%02x%s' $((${#_msg} / 2)) "$_msg"
}
hello() { hello_with "$1" "$2" "$3" "$4" "$5" ""; }
sh_cases=0
# sh_case WHAT TAG LINE: the ClientHello, then LINE; the connection fails with TAG (a fatal alert goes out) or, for `ok`, carries on.
sh_case() {
    sh_cases=$((sh_cases + 1))
    printf '%s\n%s\n' "$start" "$3" > "$work/hello.asked"
    tls_ask "$work/driver" "$work/hello.asked" "$1" || return 0
    same "$(tls_field "$work/ask.got" 2)" "$2" "$1"
    if [ "$2" != ok ]; then
        same "$(tls_field "$work/ask.got" 3)" 5 "$1: failed"
        case $(tls_field "$work/ask.got" 4) in 15030300020*) ;; *) fail "$1: a fatal alert is sent" ;; esac
    fi
}
sid=$(i=32; while [ $i -lt 64 ]; do printf '%02x' $i; i=$((i + 1)); done)
share=09$(rep 00 31)
random=$(rep 11 32)
versions=002b00020304
key_share=00330024001d0020$share
good=$versions$key_share
hrr=cf21ad74e59a6111be1d8c021e65b891c2a211167abb8c5e079e09e2c8a8339c
downgrade=$(rep 11 24)444f574e47524401
aa=$(rep aa 32)
sh_case "a valid one" ok "$(hello 0303 "$random" "$sid" 1303 "$good")"
sh_case "a HelloRetryRequest for P-256" ok "$(hello 0303 $hrr "$sid" 1301 "${versions}003300020017")"
sh_case "a HelloRetryRequest with a share in it" tls-decode-error "$(hello 0303 $hrr "$sid" 1303 "$good")"
sh_case "a HelloRetryRequest for X25519, already sent" tls-key-share "$(hello 0303 $hrr "$sid" 1303 "${versions}00330002001d")"
sh_case "a HelloRetryRequest that changes nothing" tls-hello-retry "$(hello 0303 $hrr "$sid" 1303 "$versions")"
sh_case "the downgrade sentinel in a TLS 1.3 ServerHello: random bytes (RFC 8446 4.1.3)" ok "$(hello 0303 "$downgrade" "$sid" 1303 "$good")"
sh_case "server_name acknowledged, empty, in a TLS 1.2 ServerHello (RFC 6066 3; nginx sends it)" ok "$(hello 0303 "$random" "$aa" c02f ff010001000017000000000000)"
sh_case "server_name in a TLS 1.3 ServerHello: its place is EncryptedExtensions" tls-unsupported-extension "$(hello 0303 "$random" "$sid" 1303 "${good}00000000")"
sh_case "server_name twice in a TLS 1.2 ServerHello" tls-decode-error "$(hello 0303 "$random" "$aa" c02f ff0100010000170000000000000000000000)"
sh_case "the downgrade sentinel in a TLS 1.2 ServerHello" tls-protocol-version "$(hello 0303 "$downgrade" "$aa" c02f ff0100010000170000)"
sh_case "no supported_versions (TLS 1.2), with a TLS 1.3 suite" tls-no-shared-cipher "$(hello 0303 "$random" "$sid" 1303 "$key_share")"
sh_case "TLS 1.2 with no extended master secret" tls-extended-master-secret "$(hello 0303 "$random" "$aa" c02f ff01000100)"
sh_case "TLS 1.2, valid" ok "$(hello 0303 "$random" "$aa" c02f ff0100010000170000)"
sh_case "supported_versions 1.2" tls-protocol-version "$(hello 0303 "$random" "$sid" 1303 "002b00020303$key_share")"
sh_case "AES-128-GCM chosen" ok "$(hello 0303 "$random" "$sid" 1301 "$good")"
sh_case "AES-256-GCM chosen" ok "$(hello 0303 "$random" "$sid" 1302 "$good")"
sh_case "TLS_AES_128_CCM_SHA256" tls-no-shared-cipher "$(hello 0303 "$random" "$sid" 1304 "$good")"
sh_case "ALPN, never offered" tls-unsupported-extension "$(hello 0303 "$random" "$sid" 1303 "${good}001000050003026832")"
sh_case "a duplicate supported_versions" tls-decode-error "$(hello 0303 "$random" "$sid" 1303 "$versions$good")"
sh_case "another session id" tls-decode-error "$(hello 0303 "$random" "$(rep 00 32)" 1303 "$good")"
sh_case "a P-256 share, never sent" tls-key-share "$(hello 0303 "$random" "$sid" 1303 "${versions}003300450017004104$(rep 11 64)")"
sh_case "an all-zero share" tls-key-share "$(hello 0303 "$random" "$sid" 1303 "${versions}00330024001d0020$(rep 00 32)")"
sh_case "a byte after the extensions" tls-decode-error "$(hello_with 0303 "$random" "$sid" 1303 "$good" 00)"
sh_case "a record over 2^14 + 256" tls-record-overflow "F 1603034201"
sh_case "an unknown content type" tls-unexpected-message "F 1803030001"
sh_case "application data before the handshake" tls-unexpected-message "F 1703030010$(rep 00 16)"
same "$sh_cases" 26 "the number of ServerHello cases"
# The record layer's other half: a bit flipped in the server's encrypted flight is a bad record MAC.
flight=$(sed -n 3p "$work/rsa/1.asked" | cut -c3-)
flipped=$(tls_flip "$flight" 40 1)
{ sed -n 1,2p "$work/rsa/1.asked"; echo "F $flipped"; } > "$work/flip.asked"
if tls_ask "$work/driver" "$work/flip.asked" "a flipped bit"; then
    same "$(tail -n 1 "$work/ask.got" | awk -F'[ ]' '{ print $2 }')" tls-bad-record-mac "a flipped bit"
fi
end

begin "RFC 8448's server flight opens under AES-128-GCM and is refused with a bit flipped (3 questions)"
value() { sed -n "s/^$1 //p" "$V/rfc8448_record.txt"; }
key=$(value key); iv=$(value iv); seq=$(value seq); record=$(value record); inner=$(value inner)
same "$((${#record} / 2))" "$((5 + 0x2a2))" "the record's length"
case $inner in *16) ;; *) fail "a handshake record" ;; esac
content=${inner%??}
flipped=$(tls_flip "$record" 100 4)
{ echo "O 1301 $key $iv $seq $record"; echo "O 1301 $key $iv $seq $flipped"; echo "O 1303 $key$key $iv $seq $record"; } > "$work/rfc.asked"
if tls_ask "$work/driver" "$work/rfc.asked" "RFC 8448"; then
    same "$(sed -n 1p "$work/ask.got")" "0 ok 22 $content" "the flight"
    same "$(sed -n 2p "$work/ask.got" | awk -F'[ ]' '{ print $2 }')" tls-bad-record-mac "a bit flipped"
    same "$(sed -n 3p "$work/ask.got" | awk -F'[ ]' '{ print $2 }')" tls-bad-record-mac "the other AEAD"
fi
end

begin "the TLS 1.2 PRF matches OpenSSL's (40 rows)"
grep -v '^#' "$V/prf12.txt" | awk -F' [|] ' -v w="$work" '{ print $1 > (w "/prf.asked"); print "0 ok " $2 > (w "/prf.answered") }'
same "$(tls_lines "$work/prf.asked")" 40 "the number of rows"
if tls_ask "$work/driver" "$work/prf.asked" "prf"; then
    cmp -s "$work/ask.got" "$work/prf.answered" || fail "the PRF differs from the recorded rows (first at line $(cmp "$work/ask.got" "$work/prf.answered" 2>&1 | sed 's/.*line //'))"
fi
end

fi

begin "64 connections on one thread, fed one byte and in bulk, each answered from a recorded stream; and the same server cut short, every connection fails tls-peer-closed (3 rounds)"
if tls_build many tests/programs/tls_many.cho $tls_package && tls_build replay tests/programs/tls_replay_server.cho; then
    seed=$(sed -n 's/.*seed \([0-9a-f]\{64\}\).*/\1/p' "$V/streams.txt" | head -n 1)
    same "${#seed}" 64 "the seed"
    same "$(grep -vc '^#' "$V/streams.txt")" 64 "the number of streams"
    grep -v '^#' "$V/streams.txt" | awk '{ print $4 " " $5 }' | sort > "$work/many.want"
    for round in "1 full" "65536 full" "65536 cut"; do
        set -- $round
        chunk=$1; mode=$2
        extra=; [ "$mode" = cut ] && extra=cut
        if tls_start "$work/replay.out" ready stdin "$V/streams.txt" -- "$work/replay" @PORT@ 64 $extra; then
            "$work/many" 127.0.0.1 "$port" liar.lex-sys.test 64 "$chunk" "$seed" < "$V/streams.pem" > "$work/many.out" 2> "$work/many.err"
            tls_end
            same "$status" 0 "chunk $chunk $mode: the server's status"
            same "$(tail -n 1 "$work/replay.out")" "served 64" "chunk $chunk $mode: the server answered every connection"
            if [ "$mode" = cut ]; then
                same "$(tail -n 1 "$work/many.out")" "done ok=0 failed=64" "chunk $chunk $mode: the clients"
                same "$(head -n 64 "$work/many.out" | awk '$3 == "tls-peer-closed"' | wc -l | tr -d ' ')" 64 "chunk $chunk $mode: every one tls-peer-closed"
            else
                same "$(tail -n 1 "$work/many.out")" "done ok=64 failed=0" "chunk $chunk $mode: the clients"
                head -n 64 "$work/many.out" | awk '{ print $4 " " $5 }' | sort > "$work/many.got"
                cmp -s "$work/many.got" "$work/many.want" || fail "chunk $chunk $mode: the responses are not each stream's own, once"
            fi
            tls_kill
        fi
    done
fi
end

begin "a record opens into the received data and leaves no plaintext (18 checks)"
if tls_build record_open tests/programs/record_open.cho $tls_client_package; then
    "$work/record_open" > "$work/record_open.out" 2> /dev/null
    same "$?" 0 "the program's status"
    same "$(tls_lines "$work/record_open.out")" 18 "the number of lines"
    same "$(grep -c '^ok ' "$work/record_open.out")" 18 "the number of ok lines"
fi
end

begin "the engine offers a ticket only where the rules allow (20 cases)"
if tls_build ticket_driver tests/programs/tls_tickets.cho $tls_package; then
    for spec in "tickets 20"; do
        set -- $spec
        count=$(tls_cases "$V/$1.txt" "$work/$1" named)
        same "$count" "$2" "the number of $1 cases"
        i=1
        while [ "$i" -le "$count" ]; do
            c=$work/$1/$i
            tls_run "$work/ticket_driver" "$c" "$1: $(cat "$c.name")"
            i=$((i + 1))
        done
    done
fi
end
