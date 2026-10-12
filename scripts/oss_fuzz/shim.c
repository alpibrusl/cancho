/* The libFuzzer entry point for a cancho fuzzing harness
 * (scripts/oss_fuzz/project.yaml explains the integration; build.sh
 * renames the harness's `main` to CANCHO_FUZZ_MAIN and links this
 * beside it).
 *
 * A cancho program reads its input from standard input -- the shape
 * every `tests/programs/fuzz_*.cho` harness has, the same interface
 * `scripts/fuzz_afl.py` drives through a fork server. libFuzzer instead
 * hands the input to `LLVMFuzzerTestOneInput` in memory, so this shim
 * writes the input to a memfd, dup2's it onto standard input, and calls
 * the renamed `main`. fd 0 is saved and restored, so every iteration
 * starts from a clean stdin and the harness's world (its heap, its
 * allocator) is fresh -- a cancho `main` allocates and frees its own.
 *
 * The return value is libFuzzer's: 0 to continue, non-zero to flag the
 * input as a crash. A trap in the cancho code (an out-of-bounds index,
 * a failed checked operation) aborts the process, which libFuzzer and
 * the sanitizers catch as they would any abort.
 */
#define _GNU_SOURCE
#include <sys/syscall.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>

int cancho_fuzz_main(void) __asm__(XSTR(CANCHO_FUZZ_MAIN));
#ifndef CANCHO_FUZZ_MAIN
#define CANCHO_FUZZ_MAIN cancho_fuzz_main
#endif
#define STR(x) #x
#define XSTR(x) STR(x)

int cancho_fuzz_main(void);

int LLVMFuzzerTestOneInput(const unsigned char *data, size_t size) {
    int in = syscall(SYS_memfd_create, "cancho-fuzz-input", 0);
    if (in < 0)
        return 0;
    size_t written = 0;
    while (written < size) {
        ssize_t n = write(in, data + written, size - written);
        if (n <= 0) {
            close(in);
            return 0;
        }
        written += (size_t)n;
    }
    lseek(in, 0, SEEK_SET);

    int saved = dup(0);
    if (saved < 0) {
        close(in);
        return 0;
    }
    dup2(in, 0);
    close(in);

    cancho_fuzz_main();

    fflush(stdout);
    dup2(saved, 0);
    close(saved);
    return 0;
}
