# io_utf8.sh -- the script of tests/commands/io.cho for tests/programs/io_utf8_probe.cho. $1 is the probe, $2 a scratch directory. Run from the root of the repository.
# `docs/utf8.md` section 3: the decoder agrees with an independent oracle over inputs nobody chose. tests/vectors/utf8_oracle.bin is 2,000 cases (a four-byte little-endian length and that many bytes) in four shapes:
# well-formed text at every width, raw noise, valid text with bytes corrupted, and valid text cut short; tests/vectors/utf8_oracle.expected is what Rust's own decoder says of each, recorded once (2026-11-09, from the
# generator the Rust test had: xorshift seed 0x5eed12349abcdef0): `String::from_utf8_lossy(c).chars().count()` (the maximal-subpart rule of section 3.2) and `str::from_utf8(c).is_ok()` (the strict validity of
# section 3.1), as `COUNT VALID` a line. The oracle is not GNU `wc -m`: it counts f5 80 80 80 as one character, and that sequence encodes a value above U+10FFFF.
probe=$1
dir=$2
"$probe" < tests/vectors/utf8_oracle.bin > "$dir/got"
echo "exit $?"
echo "cases answered: $(wc -l < "$dir/got" | tr -d ' ')"
if cmp -s "$dir/got" tests/vectors/utf8_oracle.expected; then echo "every case agrees with the recorded answers of Rust's decoder"; else echo "DISAGREES, first difference:"; diff "$dir/got" tests/vectors/utf8_oracle.expected | head -5; fi
