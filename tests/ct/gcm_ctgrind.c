/* `docs/gcm-wide.md` §5: does any branch or memory index in `std.gcm`'s hardware path depend on a secret?
 *
 * The ctgrind method of `ctgrind.c`: the key and the plaintext are marked undefined for Valgrind's Memcheck, which
 * reports every conditional jump, and every load or store whose address, depends on an undefined value. The cancho
 * functions are called directly from the object `cancho build --emit obj` makes of `tests/programs/gcm_driver.cho`
 * (a slice is a pointer and an `i64` length; capabilities and regions are no argument). `scripts/gcm_ctgrind.sh`
 * builds and runs it.
 *
 *     gcm_ctgrind <key bytes, 16 or 32> <length of the message>
 *     gcm_ctgrind sweep
 *
 * `sweep` is the other check Memcheck makes possible: every length from 0 to 300 and the neighbourhoods of 1, 4 and
 * 16 KiB, in buffers of exactly that size from `malloc`, sealed and opened with nothing marked undefined. Memcheck
 * reports a read or a write one byte outside a buffer (`Invalid read`, `Invalid write`), which no answer shows when the
 * byte is in the same allocation's neighbour: a builtin that reads a whole group of eight blocks of a message that
 * ends sooner, or writes past its output, is caught here and by nothing else (docs/gcm-wide.md §7).
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

static int sweep(void) {
    static const long extra[] = {1000, 1023, 1024, 1025, 4095, 4096, 4097, 16383, 16384, 16385, 16384 + 127, 16384 + 129};
    int bad = 0;
    for (int klen = 16; klen <= 32; klen += 16) {
        for (long k = 0; k < 301 + (long)(sizeof extra / sizeof extra[0]); k++) {
            long n = k < 301 ? k : extra[k - 301];
            unsigned char *key = malloc(klen), *nonce = malloc(12), *aad = malloc(13);
            for (int i = 0; i < klen; i++) key[i] = (unsigned char)(i * 37 + 11 + n);
            for (int i = 0; i < 12; i++) nonce[i] = (unsigned char)(i + n);
            for (int i = 0; i < 13; i++) aad[i] = (unsigned char)(i + 100);
            long cl = gcm_context_len(), hl = gcm_hw_len();
            long *ctx = calloc(cl, sizeof(long));
            unsigned char *hw = malloc(hl), *text = malloc(n), *sealed = malloc(n + 16), *back = malloc(n);
            memset(hw, 0, hl);
            for (long i = 0; i < n; i++) text[i] = (unsigned char)(i * 7 + 3);
            gcm_prepare(key, klen, ctx, cl, hw, hl);
            gcm_seal_with(ctx, cl, hw, hl, nonce, 12, aad, 13, text, n, sealed, n + 16);
            long o = gcm_open_with(ctx, cl, hw, hl, nonce, 12, aad, 13, sealed, n + 16, back, n);
            if (o != 0 || memcmp(back, text, n) != 0) bad++;
            free(key); free(nonce); free(aad); free(ctx); free(hw); free(text); free(sealed); free(back);
        }
    }
    printf("sweep %s\n", bad ? "BAD roundtrip" : "roundtrips ok");
    return bad != 0;
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "sweep") == 0) return sweep();
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
