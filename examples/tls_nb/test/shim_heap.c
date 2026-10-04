/* LD_PRELOAD shim for the leak test: prints how many bytes malloc still has handed out when the process exits.
 *
 *   cc -O2 -shared -fPIC -o shim_heap.so shim_heap.c
 *   LD_PRELOAD=./shim_heap.so tls_nb ...        # prints `heap_in_use=<bytes>` on standard error as the process ends
 *
 * Peak resident size is a poor leak detector (it moves with fragmentation and with what the program itself keeps); the bytes the
 * allocator has outstanding after the program has freed everything it knows about are not: a constant for any number of connections
 * unless something was not freed. `mallinfo2` is glibc's.
 */
#include <malloc.h>
#include <stdio.h>

__attribute__((destructor)) static void report(void) {
    struct mallinfo2 m = mallinfo2();
    fprintf(stderr, "heap_in_use=%zu\n", m.uordblks);
}
