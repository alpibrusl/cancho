# Wycheproof test vectors

Files from Project Wycheproof's `testvectors_v1/`, copied unchanged
from <https://github.com/C2SP/wycheproof> on 2026-10-04, under the
Apache License 2.0 in `LICENSE` beside them.

| File | Cases | Run by |
|---|---|---|
| `chacha20_poly1305_test.json` | 325 | `conformance/aead.rs` (`docs/chacha20.md` §4) |
| `hmac_sha256_test.json` | 174 | `conformance/kdf.rs` (`docs/hkdf.md` §4) |
| `hmac_sha384_test.json` | 174 | `conformance/kdf.rs` |
| `hkdf_sha256_test.json` | 86 | `conformance/kdf.rs` |
| `hkdf_sha384_test.json` | 83 | `conformance/kdf.rs` |
