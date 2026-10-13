// LD_PRELOAD shim: counts malloc/free of 65,536-byte chunks (a region's), the most live at once, and writes the
// counts to $CHUNKCOUNT_OUT every 4096 chunk mallocs and at exit. Linux/glibc.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>
static void *(*real_malloc)(size_t); static void (*real_free)(void *);
static __thread void *live[256]; static __thread int nlive;
static long total, frees, maxlive, other;
static pthread_mutex_t mu = PTHREAD_MUTEX_INITIALIZER;
static void dump(void) {
  const char *p = getenv("CHUNKCOUNT_OUT"); if (!p) return;
  FILE *f = fopen(p, "w"); if (!f) return;
  fprintf(f, "chunk_mallocs %ld chunk_frees %ld max_live_per_thread %ld\n", total, frees, maxlive); fclose(f);
}
__attribute__((destructor)) static void fini(void) { dump(); }
void *malloc(size_t n) {
  if (!real_malloc) real_malloc = dlsym(RTLD_NEXT, "malloc");
  void *p = real_malloc(n);
  if (n == 65536 && p) {
    pthread_mutex_lock(&mu); total++; 
    if (nlive < 256) live[nlive++] = p;
    if (nlive > maxlive) maxlive = nlive;
    int d = (total % 4096) == 0; pthread_mutex_unlock(&mu);
    if (d) dump();
  }
  return p;
}
void free(void *p) {
  if (!real_free) real_free = dlsym(RTLD_NEXT, "free");
  for (int i = nlive - 1; i >= 0; i--) if (live[i] == p) { live[i] = live[--nlive]; pthread_mutex_lock(&mu); frees++; pthread_mutex_unlock(&mu); break; }
  real_free(p);
}
