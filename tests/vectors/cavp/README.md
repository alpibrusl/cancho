# NIST CAVP SHA-2 test vectors

The response files of NIST's Cryptographic Algorithm Validation Program
for SHA-256, SHA-384 and SHA-512 (FIPS 180-4), byte-oriented: short
messages, long messages (SHA-256 and SHA-384) and the Monte Carlo test.
Copied unchanged from pyca/cryptography's `cryptography_vectors` 49.0.0
(`hashes/SHA2/`), which carries NIST's `shabytetestvectors` files.
Works of the US government, not subject to copyright.

`crates/lex-sys/tests/conformance/kdf.rs` runs every case through
`std.crypto` (`docs/hkdf.md` §4). SHA-512's long-message file (1.7 MB)
is left out: SHA-384's runs the same compression and the same streaming
code, and `scripts/kdf_differential.py` covers SHA-512 at long lengths.

## RSA signature verification

`SigVer15_186-3.rsp` and `SigVerPSS_186-3.rsp` are NIST's FIPS 186-4
RSA SigVer response files (RSASSA-PKCS1-v1_5, and RSASSA-PSS with a
10-byte salt), from pyca/cryptography's `cryptography_vectors` 50.0.2
(`asymmetric/RSA/FIPS_186-2/`). The SHA-1 and SHA-224 cases were removed
(`std.rsa` has neither hash) and runs of blank lines left by that were
collapsed; every SHA-256, SHA-384 and SHA-512 case is unchanged, 270 a
file. `crates/lex-sys/tests/conformance/rsa.rs` runs them all
(`docs/rsa.md` §5.2).
