/* `docs/gcm-wide.md` §5: does any branch or memory index in `std.gcm`'s hardware path depend on a secret?
 *
 * The ctgrind method of `ctgrind.c`: the key and the plaintext are marked undefined for Valgrind's Memcheck, which
 * reports every conditional jump, and every load or store whose address, depends on an undefined value. The cancho
 * functions are called directly from the object `cancho build --emit obj` makes of `tests/programs/gcm_driver.cho`
 * (a slice is a pointer and an `i64` length; capabilities and regions are no argument). `scripts/gcm_ctgrind.sh`
 * builds and runs it.
 *
 *     gcm_ctgrind <key bytes, 16 or 32> <length of the message>
 *
 * Opening ends in one branch on a secret by design: whether the tag matched, which the peer learns whatever happens.
 * Memcheck reports it, once, in `open_hardware`, so the script expects exactly that one report from the open and none
 * from the seal or the preparation.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <valgrind/memcheck.h>

long gcm_prepare(const unsigned char *, long, long *, long, unsigned char *, long) __asm__("lexs_std.gcm.prepare");
long gcm_seal_with(const long *, long, const unsigned char *, long, const unsigned char *, long, const unsigned char *,
                   long, const unsigned char *, long, unsigned char *, long) __asm__("lexs_std.gcm.seal_with");
long gcm_open_with(const long *, long, const unsigned char *, long, const unsigned char *, long, const unsigned char *,
                   long, const unsigned char *, long, unsigned char *, long) __asm__("lexs_std.gcm.open_with");
long gcm_context_len(void) __asm__("lexs_std.gcm.context_len");
long gcm_hw_len(void) __asm__("lexs_std.gcm.hw_len");

int main(int argc, char **argv) {
    if (argc < 3) return 2;
    long klen = atol(argv[1]), n = atol(argv[2]);
    unsigned char key[32], nonce[12], aad[13];
    for (int i = 0; i < 32; i++) key[i] = (unsigned char)(i * 37 + 11);
    for (int i = 0; i < 12; i++) nonce[i] = (unsigned char)(i + 1);
    for (int i = 0; i < 13; i++) aad[i] = (unsigned char)(i + 100);
    long cl = gcm_context_len(), hl = gcm_hw_len();
    long *ctx = calloc(cl, sizeof(long));
    unsigned char *hw = calloc(hl, 1), *text = malloc(n + 1), *sealed = malloc(n + 17), *back = malloc(n + 1);
    for (long i = 0; i < n; i++) text[i] = (unsigned char)(i * 7 + 3);

    /* The key and the message are secret. */
    VALGRIND_MAKE_MEM_UNDEFINED(key, klen);
    VALGRIND_MAKE_MEM_UNDEFINED(text, n);
    long r = gcm_prepare(key, klen, ctx, cl, hw, hl);
    VALGRIND_MAKE_MEM_DEFINED(&r, sizeof r);
    long s = gcm_seal_with(ctx, cl, hw, hl, nonce, 12, aad, 13, text, n, sealed, n + 16);
    VALGRIND_MAKE_MEM_DEFINED(&s, sizeof s);
    /* The ciphertext and tag are public from here on. */
    VALGRIND_MAKE_MEM_DEFINED(sealed, n + 16);
    fprintf(stderr, "== seal done\n");
    long o = gcm_open_with(ctx, cl, hw, hl, nonce, 12, aad, 13, sealed, n + 16, back, n);
    VALGRIND_MAKE_MEM_DEFINED(&o, sizeof o);
    /* Declassified only to print the answer. */
    VALGRIND_MAKE_MEM_DEFINED(back, n);
    VALGRIND_MAKE_MEM_DEFINED(text, n);
    VALGRIND_MAKE_MEM_DEFINED(hw, hl);
    printf("prepare %ld seal %ld open %ld roundtrip %s hardware-key-bytes %s\n", r, s, o,
           memcmp(back, text, n) == 0 ? "ok" : "BAD", hw[240] | hw[241] ? "present" : "absent");
    return 0;
}
