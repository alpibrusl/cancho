# NIST CAVP SHA-2 test vectors

The response files of NIST's Cryptographic Algorithm Validation Program
for SHA-256, SHA-384 and SHA-512 (FIPS 180-4), byte-oriented: short
messages, long messages (SHA-256 and SHA-384) and the Monte Carlo test.
Copied unchanged from pyca/cryptography's `cryptography_vectors` 49.0.0
(`hashes/SHA2/`), which carries NIST's `shabytetestvectors` files.
Works of the US government, not subject to copyright.

`crates/cancho/tests/conformance/kdf.rs` runs every case through
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
file. `crates/cancho/tests/conformance/rsa.rs` runs them all
(`docs/rsa.md` §5.2).

## ECDSA signature verification

`ECDSA_SigVer_186-3.rsp` is NIST's FIPS 186-3 ECDSA SigVer response file
from pyca/cryptography's `cryptography_vectors` 50.0.2
(`asymmetric/ECDSA/FIPS_186-3/SigVer.rsp`), cut to the six sections
`std.ecdsa` supports: P-256 and P-384, each with SHA-256, SHA-384 and
SHA-512, 15 cases a section. Those sections are unchanged; the header
comment is NIST's, other curves and hashes included.
`crates/cancho/tests/conformance/ecdsa.rs` runs all 90 (`docs/ecdsa.md`
§5.2).

## AES-GCM

`gcmEncryptExtIV128.rsp`, `gcmEncryptExtIV256.rsp`, `gcmDecrypt128.rsp`
and `gcmDecrypt256.rsp` are NIST's GCM response files (CAVS 14.0, SP
800-38D) from pyca/cryptography's `cryptography_vectors`
(`ciphers/AES/GCM/`), cut to the sections with a 96-bit IV and a 128-bit
tag, the only ones TLS uses: 25 sections of 15 cases a file. Those
sections are unchanged, apart from CRLF line endings made LF; the header
comment is NIST's. `crates/cancho/tests/conformance/gcm.rs` runs all
1,500, 387 of them decryptions marked `FAIL` (`docs/tls-parity.md`
§3.1.1).

## RSA signature generation (`std.rsa_sign`)

`SigGen15_186-3.txt` is NIST's FIPS 186-4 RSASSA-PKCS1-v1_5 signature
*generation* example file (CAVS 11.4; "with the additional value d added to
the file for testing purposes": two test keys, 2,048 and 3,072 bits, and ten
messages for each of SHA-224, SHA-256, SHA-384 and SHA-512). Copied unchanged
from the `SigGen15_186-3.txt` member of `186-3rsatestvectors.zip`, retrieved
2026-10-14 from
<https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Algorithm-Validation-Program/documents/dss/186-3rsatestvectors.zip>.
Works of the US government, not subject to copyright. `std.rsa_sign` has no
SHA-224, so those 20 cases are counted and not run.
`scripts/rsa_sign_vectors.py` derives `tests/vectors/rsa_sign/nist_siggen15.txt`
from it (p and q recovered from n, e and d) and
`crates/cancho/tests/conformance/rsa_sign.rs` runs all 60 (`docs/rsa-sign.md`
§11 G1). `THIRD_PARTY.md` has the SHA-256 of the archive and of the file.
