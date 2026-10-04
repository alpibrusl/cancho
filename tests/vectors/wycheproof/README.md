# Wycheproof test vectors

`chacha20_poly1305_test.json` is Project Wycheproof's
`testvectors_v1/chacha20_poly1305_test.json`, copied unchanged from
<https://github.com/C2SP/wycheproof> on 2026-10-04 (325 cases), under
the Apache License 2.0 in `LICENSE` beside it.

`crates/lex-sys/tests/conformance/aead.rs` runs every case through
`std.chacha20` (`docs/chacha20.md` §4).
