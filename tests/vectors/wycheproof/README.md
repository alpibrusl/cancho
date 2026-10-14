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

## RSA PKCS#1 v1.5 signature generation (`std.rsa_sign`)

`rsa_pkcs1_{1024,1536,2048,3072,4096}_sig_gen_test.json` are Wycheproof's
`RsassaPkcs1Generate` files (schema `rsassa_pkcs1_generate_schema_v1.json`):
a private key as PKCS#1 and PKCS#8 and RSASSA-PKCS1-v1_5 signatures to
reproduce. Copied unchanged from <https://github.com/C2SP/wycheproof>
`testvectors_v1/` at commit `7a325932231bd7a724c791c8b0d65ea4d6d2f435`
(2026-10-12), retrieved 2026-10-14, under the same Apache License 2.0 in
`LICENSE` (byte-identical to the one fetched at that commit). They are test
keys published by Wycheproof. Run by `conformance/rsa_sign.rs`
(`docs/rsa-sign.md` §11 G1); `THIRD_PARTY.md` has each file's SHA-256.
