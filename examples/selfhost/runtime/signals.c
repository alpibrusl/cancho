/* signals.c -- the signal claim (`signals_watch`, `signals_pending`, `signals_close`), for the code of the compiler
 * written in cancho.
 *
 * SOURCE of `signals_linux.ll` and `signals_darwin.ll` (see `net.c` for how they are made and used). It does what
 * `cancho-codegen-llvm`'s `body/signals.rs` does: on Linux `pthread_sigmask` blocks the set and a `signalfd` reads it;
 * on Darwin each signal is ignored and a `kqueue` watches it. The handle is the kernel's mask in the high 32 bits
 * over the descriptor in the low 32. `cancho_signal_state` is two words the module defines: the mask of every signal
 * a live claim holds, and the number of threads spawned and not yet joined.
 */
typedef long i64;
typedef unsigned long u64;

extern int pthread_sigmask(int, const void *, void *);
extern int sigaction(int, const void *, void *);
extern int raise(int);
extern int close(int);
extern int fcntl(int, int, ...);
extern i64 read(int, void *, i64);
extern int *cancho_errno_location(void);
#define ERRNO (*cancho_errno_location())
#if DARWIN
extern int kqueue(void);
extern int kevent(int, const void *, int, void *, int, const void *);
#else
extern int signalfd(int, const void *, int);
#endif
extern i64 cancho_signal_state[2];

#define EBUSY 16
#define SIG_BLOCK 0
#define SIG_UNBLOCK 1
#define STATE_CLAIMED 0
#define STATE_THREADS 1

/* The eight signals a claim can name: the bit a program writes, and the number the system gives it. */
#if DARWIN
static const int sigrt_number[8] = {14, 1, 2, 3, 15, 30, 31, 28};
#else
static const int sigrt_number[8] = {14, 1, 2, 3, 15, 10, 12, 28};
#endif
static const int sigrt_bit[8] = {64, 1, 2, 4, 8, 16, 32, 128};

static i64 sigrt_native(i64 mask) {
  i64 native = 0;
  for (int i = 0; i < 8; i++)
    if (mask & sigrt_bit[i]) native |= 1L << (sigrt_number[i] - 1);
  return native;
}

static i64 sigrt_to_bits(i64 native) {
  i64 out = 0;
  for (int i = 0; i < 8; i++)
    if (native & (1L << (sigrt_number[i] - 1))) out |= sigrt_bit[i];
  return out;
}

#if DARWIN
struct sigrt_action { i64 handler; unsigned mask; int flags; };
struct sigrt_event { u64 ident; short filter; unsigned short flags; unsigned fflags; i64 data; void *udata; };
struct sigrt_time { i64 sec; i64 nsec; };

static void sigrt_disposition(i64 native, i64 handler) {
  for (int n = 1; n <= 31; n++)
    if ((native >> (n - 1)) & 1) {
      struct sigrt_action act = {handler, 0, 0};
      sigaction(n, &act, 0);
    }
}
#else
static void sigrt_set(u64 *set, i64 native) {
  for (int i = 1; i < 16; i++) set[i] = 0;
  set[0] = (u64)native;
}
#endif

void cancho_signals_watch(i64 mask, i64 *out) {
  i64 native = sigrt_native(mask);
  i64 claimed = cancho_signal_state[STATE_CLAIMED];
  if ((claimed & native) != 0 || cancho_signal_state[STATE_THREADS] != 0) {
    out[0] = 1;
    out[1] = 0;
    out[2] = EBUSY;
    return;
  }
  int fd;
#if DARWIN
  sigrt_disposition(native, 1);
  fd = kqueue();
  int e = ERRNO;
  if (fd >= 0) fcntl(fd, 2, 1);
  if (fd < 0) {
    sigrt_disposition(native, 0);
    out[0] = 1;
    out[1] = 0;
    out[2] = e;
    return;
  }
  for (int n = 1; n <= 31; n++)
    if ((native >> (n - 1)) & 1) {
      struct sigrt_event ev = {(u64)n, -6, 0x21, 0, 0, 0};
      kevent(fd, &ev, 1, 0, 0, 0);
    }
#else
  u64 set[16];
  sigrt_set(set, native);
  pthread_sigmask(SIG_BLOCK, set, 0);
  fd = signalfd(-1, set, 04000 | 02000000);
  int e = ERRNO;
  if (fd < 0) {
    pthread_sigmask(SIG_UNBLOCK, set, 0);
    out[0] = 1;
    out[1] = 0;
    out[2] = e;
    return;
  }
#endif
  cancho_signal_state[STATE_CLAIMED] = claimed | native;
  out[0] = 0;
  out[1] = (i64)(((u64)native << 32) | (u64)(unsigned)fd);
  out[2] = 0;
}

/* What the kernel has queued on the descriptor, as a kernel mask. Never waits. */
static i64 sigrt_drain(int fd) {
  i64 seen = 0;
  i64 count;
#if DARWIN
  struct sigrt_event buf[8];
  struct sigrt_time none = {0, 0};
  count = kevent(fd, 0, 0, buf, 8, &none);
  for (i64 i = 0; i < count; i++) {
    u64 n = buf[i].ident;
    if (n - 1 < 31) seen |= 1L << (n - 1);
  }
#else
  unsigned buf[8][32];
  i64 got = read(fd, buf, 1024);
  count = got / 128;
  for (i64 i = 0; i < count; i++) {
    u64 n = buf[i][0];
    if (n - 1 < 31) seen |= 1L << (n - 1);
  }
#endif
  return seen;
}

i64 cancho_signals_pending(const i64 *watch) {
  return sigrt_to_bits(sigrt_drain((int)*watch));
}

i64 cancho_signals_close(i64 handle) {
  int fd = (int)handle;
  i64 native = (i64)((u64)handle >> 32);
  i64 queued = 0;
#if DARWIN
  queued = sigrt_drain(fd);
  sigrt_disposition(native, 0);
#else
  u64 set[16];
  sigrt_set(set, native);
  pthread_sigmask(SIG_UNBLOCK, set, 0);
#endif
  int answer = close(fd);
#if DARWIN
  for (int n = 1; n <= 31; n++)
    if ((queued >> (n - 1)) & 1) raise(n);
#endif
  cancho_signal_state[STATE_CLAIMED] = cancho_signal_state[STATE_CLAIMED] & ~native;
  return (i64)answer;
}
