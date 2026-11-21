/* tty.c -- the serial-port verbs (`tty_open`, `tty_configure`, `tty_read`, `tty_write`, `tty_flush_input`), for the code
 * of the compiler written in cancho.
 *
 * SOURCE of `tty_linux.ll` and `tty_darwin.ll` (see `net.c`). The numbers are `cancho_ir::tty`'s, measured on each
 * system: the layout of `struct termios`, the flags of `open`, the Linux rates and the macOS two-step (`tcsetattr` at a
 * standard rate, then `ioctl(IOSSIOSPEED)`). A port is a descriptor; the verbs that take one are given the address of
 * its slot.
 */
typedef long i64;
typedef unsigned long u64;
typedef unsigned char u8;

extern int openat(int, const char *, int, ...);
extern int tcsetattr(int, int, const void *);
extern int tcflush(int, int);
extern i64 read(int, void *, i64);
extern i64 write(int, const void *, i64);
extern void free(void *);
extern void *cancho_checked_path(const u8 *, i64, const u8 *, i64);
extern int *cancho_errno_location(void);
#define ERRNO (*cancho_errno_location())
#if DARWIN
extern int ioctl(int, unsigned long, ...);
#endif

#define EINVAL 22
#if DARWIN
#define AT_FDCWD (-2)
#define OPEN_FLAGS (2 | 131072 | 4)
#define TCIFLUSH 1
#define TERMIOS_WORDS 9
#define CFLAG_AT 16
#define CFLAG (0x300 | 0x8000 | 0x800)
#define ISPEED_AT 56
#define OSPEED_AT 64
typedef u64 word;
#else
#define AT_FDCWD (-100)
#define OPEN_FLAGS (2 | 256 | 2048)
#define TCIFLUSH 0
#define TERMIOS_WORDS 15
#define CFLAG_AT 8
#define CFLAG (0x30 | 0x800 | 0x80)
typedef unsigned int word;
#endif

void cancho_tty_open(const u8 *src, i64 len, const u8 *prefix, i64 plen, i64 *out) {
  void *path = cancho_checked_path(src, len, prefix, plen);
  int fd = openat(AT_FDCWD, (const char *)path, OPEN_FLAGS, 0);
  int e = ERRNO;
  free(path);
  out[0] = fd < 0;
  out[1] = fd;
  out[2] = e;
}

#if !DARWIN
static const i64 ttyrt_rates[30][2] = {
    {50, 01}, {75, 02}, {110, 03}, {134, 04}, {150, 05}, {200, 06}, {300, 07}, {600, 010}, {1200, 011}, {1800, 012},
    {2400, 013}, {4800, 014}, {9600, 015}, {19200, 016}, {38400, 017}, {57600, 010001}, {115200, 010002},
    {230400, 010003}, {460800, 010004}, {500000, 010005}, {576000, 010006}, {921600, 010007}, {1000000, 010010},
    {1152000, 010011}, {1500000, 010012}, {2000000, 010013}, {2500000, 010014}, {3000000, 010015}, {3500000, 010016},
    {4000000, 010017}};
#endif

/* Raw mode, 8N1 and the speed: 0, or the errno; EINVAL for a speed the system has no rate for. */
i64 cancho_tty_configure(const i64 *port, i64 speed) {
  int fd = (int)*port;
  i64 bits = 0;
#if DARWIN
  if ((u64)(speed - 1) >= 2147483646UL) return EINVAL;
#else
  bits = -1;
  for (int i = 0; i < 30; i++)
    if (ttyrt_rates[i][0] == speed) bits = ttyrt_rates[i][1];
  if (bits < 0) return EINVAL;
#endif
  word t[TERMIOS_WORDS];
  for (int i = 0; i < TERMIOS_WORDS; i++) t[i] = 0;
  u8 *b = (u8 *)t;
  *(word *)(b + CFLAG_AT) = (word)(bits | CFLAG);
#if DARWIN
  *(word *)(b + ISPEED_AT) = 115200;
  *(word *)(b + OSPEED_AT) = 115200;
#endif
  if (tcsetattr(fd, 0, t) < 0) return ERRNO;
#if DARWIN
  i64 cell = speed;
  if (ioctl(fd, 0x80085402UL, &cell) < 0) return ERRNO;
#endif
  return 0;
}

i64 cancho_tty_read(const i64 *port, u8 *p, i64 n) { return read((int)*port, p, n); }

i64 cancho_tty_write(const i64 *port, const u8 *p, i64 n) { return write((int)*port, p, n); }

i64 cancho_tty_flush(const i64 *port) { return (i64)tcflush((int)*port, TCIFLUSH); }
