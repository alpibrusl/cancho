/* `docs/x25519.md` §3.1: does any branch or memory index depend on a secret?
 *
 * The ctgrind method: the secret bytes are marked *undefined* for Valgrind's Memcheck, and Memcheck reports every
 * conditional jump, and every load or store whose address, depends on an undefined value. Nothing is reported for
 * arithmetic on them, which is the point: constant-time code may compute with secrets, never branch on them or index
 * by them.
 *
 * The lex-sys functions are called directly from the object `lex-sys build --emit obj` makes (a slice is a pointer and
 * an `i64` length, an `int` an `i64`; capabilities and regions are no argument at all: crates/lex-sys-codegen-llvm/src/
 * emit.rs, `leaves_into`). `scripts/curve25519_ctgrind.sh` builds and runs it.
 *
 *     ctgrind x25519 | ed25519-sign
 */
#include <stdio.h>
#include <string.h>
#include <valgrind/memcheck.h>

long x25519_scalarmult(const unsigned char *, long, const unsigned char *, long, unsigned char *, long)
    __asm__("lexs_std.x25519.scalarmult");
long ed25519_sign(const unsigned char *, long, const unsigned char *, long, unsigned char *, long)
    __asm__("lexs_std.ed25519.sign");

int main(int argc, char **argv) {
    unsigned char secret[32], u[32], out[64], msg[64];
    for (int i = 0; i < 32; i++) secret[i] = (unsigned char)(i * 37 + 11);
    memset(u, 0, sizeof u);
    u[0] = 9;
    memset(msg, 1, sizeof msg);
    if (argc < 2) return 2;
    VALGRIND_MAKE_MEM_UNDEFINED(secret, sizeof secret);
    long r;
    if (strcmp(argv[1], "x25519") == 0) {
        r = x25519_scalarmult(secret, 32, u, 32, out, 32);
        VALGRIND_MAKE_MEM_DEFINED(out, 32);
    } else {
        r = ed25519_sign(secret, 32, msg, 64, out, 64);
        VALGRIND_MAKE_MEM_DEFINED(out, 64);
    }
    VALGRIND_MAKE_MEM_DEFINED(&r, sizeof r);
    printf("%ld %02x%02x\n", r, out[0], out[1]);
    return 0;
}
