# Wycheproof test vectors

Files from Project Wycheproof's `testvectors_v1/`, copied unchanged
from <https://github.com/C2SP/wycheproof> on 2026-10-04, under the
Apache License 2.0 in `LICENSE` beside them.

| File | Cases | Run by |
|---|---|---|
| `chacha20_poly1305_test.json` | 325 | `conformance/aead.rs` (`docs/chacha20.md` §4) |
| `aes_gcm_test.json` | 316 | `conformance/gcm.rs` (`docs/tls-parity.md` §3.1.1) |
| `hmac_sha256_test.json` | 174 | `conformance/kdf.rs` (`docs/hkdf.md` §4) |
| `hmac_sha384_test.json` | 174 | `conformance/kdf.rs` |
| `hkdf_sha256_test.json` | 86 | `conformance/kdf.rs` |
| `hkdf_sha384_test.json` | 83 | `conformance/kdf.rs` |
| `x25519_test.json` | 518 | `conformance/x25519.rs` (`docs/x25519.md` §4.2) |
| `ed25519_test.json` | 151 | `conformance/x25519.rs` |
| `rsa_pss_2048_sha256_mgf1_0_test.json` | 103 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_pss_2048_sha256_mgf1_32_test.json` | 108 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_pss_2048_sha384_mgf1_48_test.json` | 141 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_pss_3072_sha256_mgf1_32_test.json` | 108 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_pss_4096_sha256_mgf1_32_test.json` | 108 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_pss_4096_sha384_mgf1_48_test.json` | 141 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_pss_4096_sha512_mgf1_64_test.json` | 179 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_pss_misc_test.json` | 150 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_signature_2048_sha256_test.json` | 259 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_signature_2048_sha384_test.json` | 258 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_signature_2048_sha512_test.json` | 259 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_signature_3072_sha256_test.json` | 259 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_signature_3072_sha384_test.json` | 259 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_signature_3072_sha512_test.json` | 260 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_signature_4096_sha256_test.json` | 258 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_signature_4096_sha384_test.json` | 259 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `rsa_signature_4096_sha512_test.json` | 259 | `conformance/rsa.rs` (`docs/rsa.md` §5.1) |
| `ecdsa_secp256r1_sha256_test.json` | 484 | `conformance/ecdsa.rs` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp256r1_sha256_p1363_test.json` | 262 | `conformance/ecdsa.rs` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp256r1_sha512_test.json` | 554 | `conformance/ecdsa.rs` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp384r1_sha384_test.json` | 504 | `conformance/ecdsa.rs` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp384r1_sha384_p1363_test.json` | 280 | `conformance/ecdsa.rs` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp384r1_sha256_test.json` | 472 | `conformance/ecdsa.rs` (`docs/ecdsa.md` §5.1) |
| `ecdsa_secp384r1_sha512_test.json` | 542 | `conformance/ecdsa.rs` (`docs/ecdsa.md` §5.1) |
