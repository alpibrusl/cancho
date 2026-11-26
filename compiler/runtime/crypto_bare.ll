; crypto_bare.ll -- the hardware AES, GHASH and SHA-256 functions for a machine with no system under it (`--target aarch64-none-elf`).
; Written by hand (docs/freestanding.md section 8). Nothing here asks the processor: the two questions answer no, so `std.crypto` and
; `std.gcm` take their software paths, and a builtin reached anyway traps, which is what docs/sha256-builtin.md section 3 and
; docs/crypto-builtins.md section 5 say of a target without the instructions.
define internal i8 @cancho_hw_aes_gcm() {
entry:
  ret i8 0
}
define internal void @cancho_aes_encrypt_block(ptr %rk, i64 %rounds, ptr %blk, ptr %out) {
entry:
  call void @cancho_trap()
  unreachable
}
define internal void @cancho_aes_ctr32(ptr %rk, i64 %rounds, ptr %nonce, i64 %counter, ptr %in, ptr %out, i64 %len) {
entry:
  call void @cancho_trap()
  unreachable
}
define internal void @cancho_ghash_powers(ptr %rk, i64 %rounds, ptr %table) {
entry:
  call void @cancho_trap()
  unreachable
}
define internal void @cancho_ghash_update(ptr %h, ptr %y, ptr %data, i64 %n) {
entry:
  call void @cancho_trap()
  unreachable
}
define internal void @cancho_gcm_tag(ptr %rk, i64 %rounds, ptr %tab, ptr %nonce, ptr %aad, i64 %aadlen, ptr %text, i64 %textlen, ptr %out) {
entry:
  call void @cancho_trap()
  unreachable
}
define internal i64 @cancho_gcm_tag_diff(ptr %rk, i64 %rounds, ptr %tab, ptr %nonce, ptr %aad, i64 %aadlen, ptr %text, i64 %textlen, ptr %expected) {
entry:
  call void @cancho_trap()
  unreachable
}
define internal i8 @cancho_hw_sha256() {
entry:
  ret i8 0
}
define internal void @cancho_sha256_blocks(ptr %state, ptr %data, i64 %n) {
entry:
  call void @cancho_trap()
  unreachable
}
define internal void @cancho_sha256_block_words(ptr %state, ptr %block) {
entry:
  call void @cancho_trap()
  unreachable
}
