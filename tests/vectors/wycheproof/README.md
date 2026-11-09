# Wycheproof test vectors

Files from Project Wycheproof's `testvectors_v1/`, copied unchanged
from <https://github.com/C2SP/wycheproof> on 2026-10-04, under the
Apache License 2.0 in `LICENSE` beside them.

| File | Cases | Run by |
|---|---|---|
| `chacha20_poly1305_test.json` | 325 | `tests/commands/aead.cho` (`docs/chacha20.md` §4) |
| `aes_gcm_test.json` | 316 | `tests/commands/gcm.cho` (`docs/tls-parity.md` §3.1.1) |
| `hmac_sha256_test.json` | 174 | `tests/commands/kdf.cho` (`docs/hkdf.md` §4) |
| `hmac_sha384_test.json` | 174 | `tests/commands/kdf.cho` |
| `hkdf_sha256_test.json` | 86 | `tests/commands/kdf.cho` |
| `hkdf_sha384_test.json` | 83 | `tests/commands/kdf.cho` |
| `x25519_test.json` | 518 | `tests/commands/x25519.cho` (`docs/x25519.md` §4.2) |
| `ed25519_test.json` | 151 | `tests/commands/x25519.cho` |
| `rsa_pss_2048_sha256_mgf1_0_test.json` | 103 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_pss_2048_sha256_mgf1_32_test.json` | 108 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_pss_2048_sha384_mgf1_48_test.json` | 141 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_pss_3072_sha256_mgf1_32_test.json` | 108 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_pss_4096_sha256_mgf1_32_test.json` | 108 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_pss_4096_sha384_mgf1_48_test.json` | 141 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_pss_4096_sha512_mgf1_64_test.json` | 179 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_pss_misc_test.json` | 150 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_signature_2048_sha256_test.json` | 259 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_signature_2048_sha384_test.json` | 258 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_signature_2048_sha512_test.json` | 259 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_signature_3072_sha256_test.json` | 259 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_signature_3072_sha384_test.json` | 259 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_signature_3072_sha512_test.json` | 260 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_signature_4096_sha256_test.json` | 258 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_signature_4096_sha384_test.json` | 259 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `rsa_signature_4096_sha512_test.json` | 259 | `tests/commands/rsa.cho` (`docs/rsa.md` §5.1) |
| `ecdsa_secp256r1_sha256_test.json` | 484 | `tests/commands/ecdsa.cho` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp256r1_sha256_p1363_test.json` | 262 | `tests/commands/ecdsa.cho` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp256r1_sha512_test.json` | 554 | `tests/commands/ecdsa.cho` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp384r1_sha384_test.json` | 504 | `tests/commands/ecdsa.cho` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp384r1_sha384_p1363_test.json` | 280 | `tests/commands/ecdsa.cho` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp384r1_sha256_test.json` | 472 | `tests/commands/ecdsa.cho` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp384r1_sha512_test.json` | 542 | `tests/commands/ecdsa.cho` (`docs/ecdsa.md` §5.1) |
