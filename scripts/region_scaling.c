// `docs/threads.md` §7: `malloc(n); free()` in a loop on T threads, the cost of a `region` per unit of work.
//   cc -O2 -o region_scaling scripts/region_scaling.c -lpthread
//   ./region_scaling THREADS MODE [BYTES [ITERATIONS]]     MODE 0: none, 1: touch 512 bytes, 2: touch all, 3: touch 1
// Linux (glibc) scales; macOS (libmalloc) does not above 32 KiB.
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#include <sys/resource.h>
#include <stdint.h>
static long N = 1000000; static int mode; static size_t sz = 65536;
static void *run(void *a) {
  volatile char sink = 0;
  for (long i = 0; i < N; i++) {
    char *p = malloc(sz);
    if (!p) abort();
    if (mode == 1) memset(p, 0, 512);           // a few limbs: one page touched
    else if (mode == 2) memset(p, 0, sz);       // all of it
    else if (mode == 3) { p[0] = 1; }           // one byte
    sink += p[0];
    free(p);
  }
  return 0;
}
static double tv(struct timeval t){return t.tv_sec+t.tv_usec/1e6;}
int main(int c, char **v) {
  int T = atoi(v[1]); mode = atoi(v[2]); if (c > 3) sz = atol(v[3]); if (c > 4) N = atol(v[4]);
  pthread_t th[64]; struct timeval a, b; gettimeofday(&a, 0);
  for (int i = 0; i < T; i++) pthread_create(&th[i], 0, run, 0);
  for (int i = 0; i < T; i++) pthread_join(th[i], 0);
  gettimeofday(&b, 0); struct rusage ru; getrusage(RUSAGE_SELF, &ru);
  printf("threads %2d mode %d size %6zu  each %ld iters: wall %.2fs user %.2fs sys %.2fs\n", T, mode, sz, N, tv(b)-tv(a), tv(ru.ru_utime), tv(ru.ru_stime));
}
