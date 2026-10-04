/* LD_PRELOAD shim for the partial-I/O test: makes `send` and `recv` behave the way a busy kernel does.
 *
 *   cc -O2 -shared -fPIC -o shim_io.so shim_io.c -ldl
 *   SHIM_SEND_MAX=700 SHIM_RECV_MAX=300 SHIM_EAGAIN_EVERY=3 LD_PRELOAD=./shim_io.so tls_nb ...
 *
 * `send` takes at most SHIM_SEND_MAX bytes of what it is given, `recv` returns at most SHIM_RECV_MAX, and every SHIM_EAGAIN_EVERY-th
 * call of either answers -1 with EAGAIN without touching the socket. Real sockets on loopback accept 60 KB in one call, so the branches
 * of `tls.ls` that handle a partial write, a write that waits, a record that arrives in pieces and a read that has nothing yet are
 * not reached by an ordinary run (`design.md` section 16 of lexsys-hooks says the same of its own partial-write branch).
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <sys/types.h>

static long env(const char *k, long d) { const char *v = getenv(k); return v ? atol(v) : d; }
static long calls_send, calls_recv;

ssize_t send(int fd, const void *buf, size_t len, int flags) {
    static ssize_t (*real)(int, const void *, size_t, int);
    if (!real) real = dlsym(RTLD_NEXT, "send");
    long every = env("SHIM_EAGAIN_EVERY", 0), max = env("SHIM_SEND_MAX", 0);
    if (every && ++calls_send % every == 0) { errno = EAGAIN; return -1; }
    if (max && len > (size_t)max) len = max;
    return real(fd, buf, len, flags);
}

ssize_t recv(int fd, void *buf, size_t len, int flags) {
    static ssize_t (*real)(int, void *, size_t, int);
    if (!real) real = dlsym(RTLD_NEXT, "recv");
    long every = env("SHIM_EAGAIN_EVERY", 0), max = env("SHIM_RECV_MAX", 0);
    if (every && ++calls_recv % every == 0) { errno = EAGAIN; return -1; }
    if (max && len > (size_t)max) len = max;
    return real(fd, buf, len, flags);
}
