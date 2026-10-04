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
