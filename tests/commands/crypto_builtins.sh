# crypto_builtins.sh -- the script of tests/commands/script_tests.cho for `conformance/crypto_builtins.rs` (deleted): the hardware AES and GHASH builtins (docs/crypto-builtins.md section 7,
# docs/gcm-wide.md section 6) through tests/programs/crypto_builtins_driver.cho: FIPS 197's examples, NIST's GCM test case 2 from the builtins, and a differential against plain reference
# implementations (FIPS 197 and SP 800-38D as written) over random keys, blocks and every length, now recorded: tests/vectors/crypto_builtins.txt (2,080 cases, and the 1,226 tag comparisons made from them) and .expected.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/vector_lib.sh

build tests/programs/crypto_builtins_driver.cho driver

# z N: N zero bytes as hex, or `-` for none (the driver's field for no bytes).
z() {
    if [ "$1" = 0 ]; then echo -; else head -c "$1" /dev/zero | od -An -v -tx1 | tr -d ' \n'; echo; fi
}

begin "the hardware builtins agree with FIPS, NIST and the references"
# Line 1 asks `hw_aes_gcm()`, which must be true on this suite's hosts (x86-64 Linux, aarch64 macOS), so a silent fall back to software cannot hide. Then FIPS 197 C.1 and C.3, NIST GCM test
# case 2 (A, C and T), 600 random AES blocks (all three key sizes), counter mode over every length 0 to 300 and 200 counters about to wrap, 300 `ghash_update` calls, 60 tables of the powers
# of H, and 613 tag cases (text lengths to 300 with aad to 70 and back, and eleven long ones to 16 KiB), each also with its tag compared equal and with one bit changed (made below).
same "$(lines tests/vectors/crypto_builtins.txt)" 2080 "the cases recorded (a count, so the file cannot shrink silently)"
same "$(head -1 tests/vectors/crypto_builtins.expected)" 1 "the first answer recorded is hw_aes_gcm() = 1"
agrees "$work/driver" tests/vectors/crypto_builtins.txt tests/vectors/crypto_builtins.expected "the hardware builtins"
# The tag comparisons (`D`) are not recorded: each is its tag case (`T`, after the first, NIST's) with the tag appended, once as it is (answer 0: equal) and once with one bit of byte
# (text length + aad length) mod 16 flipped, bit (text length) mod 8 (answer 1: different), as the Rust test made them; 613 cases, so 1,226 comparisons.
paste -d'|' tests/vectors/crypto_builtins.txt tests/vectors/crypto_builtins.expected | awk -F'|' -v good_in="$work/d.good.in" -v good_want="$work/d.good.want" -v bad_in="$work/d.bad.in" -v bad_want="$work/d.bad.want" '
    function hex(c) { return index("0123456789abcdef", c) - 1 }
    function bytelen(f) { return f == "-" ? 0 : length(f) / 2 }
    substr($1, 1, 2) == "T " {
        seen++
        if (seen == 1) next
        split($1, f, " ")
        tag = $2
        print "D " f[2] " " f[3] " " f[4] " " f[5] " " f[6] " " tag > good_in
        print 0 > good_want
        tl = bytelen(f[6]); al = bytelen(f[5])
        at = (tl + al) % 16
        mask = 2 ^ (tl % 8)
        byte = hex(substr(tag, 2 * at + 1, 1)) * 16 + hex(substr(tag, 2 * at + 2, 1))
        flipped = byte + mask - 2 * (int(byte / mask) % 2) * mask
        bad = substr(tag, 1, 2 * at) sprintf("%02x", flipped) substr(tag, 2 * at + 3)
        print "D " f[2] " " f[3] " " f[4] " " f[5] " " f[6] " " bad > bad_in
        print 1 > bad_want
    }'
same "$(lines "$work/d.good.in")" 613 "the tag comparisons of a tag as it is (a count)"
agrees "$work/driver" "$work/d.good.in" "$work/d.good.want" "the tag compared equal"
agrees "$work/driver" "$work/d.bad.in" "$work/d.bad.want" "the tag with one bit changed"
end

begin "a wrong length, counter or round count traps"
# As an index out of bounds does, before any instruction runs (docs/crypto-builtins.md section 3, docs/gcm-wide.md section 4).
z192=$(z 192); z176=$(z 176); z160=$(z 160); z16=$(z 16); z17=$(z 17); z15=$(z 15); z12=$(z 12); z13=$(z 13); z11=$(z 11); z1=$(z 1)
n=0
while IFS= read -r case_line; do
    n=$((n + 1))
    traps "$work/driver" "$case_line" "a wrong length, counter or round count"
done << EOF
A 11 $z192 $z16
A 10 $z160 $z16
A 10 $z176 $z15
C 11 $z192 $z12 0 $z16
C 10 $z160 $z12 0 $z16
C 10 $z176 $z11 0 $z16
C 10 $z176 $z13 0 $z16
C 10 $z176 $z12 4294967296 $z16
c 10 $z176 $z12 0 $z16
d 10 $z176 $z12 0 $z16
d 10 $z176 $z12 0 $z1
G $z15 $z16 $z16
G $z16 $z15 $z16
G $z16 $z16 $z17
P 11 $z192
P 10 $z160
P 14 $z176
T 10 $z176 $z11 - $z16
T 10 $z160 $z12 - $z16
T 12 $z176 $z12 - $z16
D 10 $z176 $z12 - $z16 $z15
D 10 $z176 $z12 - $z16 $z17
X 10 $z176 $z12 - $z16 $z16
EOF
same "$n" 23 "the cases that trap (a count)"
end
