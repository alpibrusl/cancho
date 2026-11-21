/* camera.c -- the camera verbs (`camera_open`, `camera_configure`, `camera_next`, `camera_close`), for the code of the
 * compiler written in cancho: the walk of `docs/device-backends.md` §2-§5 and §10, as the two Rust backends write it.
 *
 * SOURCE of `camera_linux.ll` and `camera_darwin.ll` (see `net.c`). Every number is `cancho_ir::device`'s (`v4l2`),
 * measured on Linux x86_64 and on aarch64 (the robot's Raspberry Pi 5): the structs are byte offsets, not the kernel's
 * headers, because this file is compiled once for both systems. A camera is the address of a 128-byte block (`STATE_*`);
 * the verbs that take one are given the address of the cell that holds it, `camera_close` the address itself. On macOS
 * there is no V4L2: the open answers `Failed(ENODEV)` and no call is made.
 */
typedef long i64;
typedef unsigned char u8;

extern int openat(int, const char *, int, ...);
extern int close(int);
extern void free(void *);
extern void *calloc(unsigned long, unsigned long);
extern void *cancho_checked_path(const u8 *, i64, const u8 *, i64);
extern int *cancho_errno_location(void);
#define ERRNO (*cancho_errno_location())

#define ENOMEM 12
#define ENODEV 19
#define EINVAL 22
#define EAGAIN 11

#if DARWIN
void cancho_camera_open(const u8 *src, i64 len, const u8 *prefix, i64 plen, i64 *out) {
  out[0] = 1;
  out[1] = -1;
  out[2] = ENODEV;
}

i64 cancho_camera_configure(i64 *cam, i64 width, i64 height) { return -1; }

i64 cancho_camera_next(i64 *cam, u8 *into, i64 n) { return -1; }

i64 cancho_camera_close(i64 handle) { return -1; }
#else
extern int ioctl(int, unsigned long, ...);
extern void *mmap(void *, unsigned long, int, int, int, i64);
extern int munmap(void *, unsigned long);

#define AT_FDCWD (-100)
#define OPEN_FLAGS (2 | 2048)
#define S_FMT 0xc0d05605ul
#define REQBUFS 0xc0145608ul
#define QUERYBUF 0xc0585609ul
#define QBUF 0xc058560ful
#define DQBUF 0xc0585611ul
#define STREAMON 0x40045612ul
#define STREAMOFF 0x40045613ul
#define BUFFERS 4

#define FORMAT_WORDS 26
#define BUFFER_WORDS 11

static void cancho_camera_zero(i64 *p, int words) {
  for (int i = 0; i < words; i++) p[i] = 0;
}

/* An `i32` at a byte offset. */
#define AT32(base, off) (*(int *)((u8 *)(base) + (off)))
#define AT64(base, off) (*(i64 *)((u8 *)(base) + (off)))

void cancho_camera_open(const u8 *src, i64 len, const u8 *prefix, i64 plen, i64 *out) {
  void *path = cancho_checked_path(src, len, prefix, plen);
  int fd = openat(AT_FDCWD, (const char *)path, OPEN_FLAGS, 0);
  int e = ERRNO;
  free(path);
  if (fd < 0) {
    out[0] = 1;
    out[1] = fd;
    out[2] = e;
    return;
  }
  void *block = calloc(1, 128);
  if (block == 0) {
    close(fd);
    out[0] = 1;
    out[1] = -1;
    out[2] = ENOMEM;
    return;
  }
  AT32(block, 0) = fd;
  out[0] = 0;
  out[1] = (i64)block;
  out[2] = 0;
}

i64 cancho_camera_configure(i64 *cam, i64 width, i64 height) {
  u8 *s = (u8 *)*cam;
  if (width <= 0 || height <= 0 || width > 2147483647 || height > 2147483647) return EINVAL;
  if (AT64(s, 8) != 0) return EINVAL;
  int fd = AT32(s, 0);
  i64 fmt[FORMAT_WORDS];
  cancho_camera_zero(fmt, FORMAT_WORDS);
  AT32(fmt, 0) = 1;
  AT32(fmt, 8) = (int)width;
  AT32(fmt, 12) = (int)height;
  AT32(fmt, 16) = 0x56595559;
  AT32(fmt, 20) = 1;
  if (ioctl(fd, S_FMT, fmt) < 0) return ERRNO;
  if (AT32(fmt, 8) != (int)width || AT32(fmt, 12) != (int)height) return EINVAL;
  AT64(s, 8) = width;
  AT64(s, 16) = height;
  i64 req[3];
  cancho_camera_zero(req, 3);
  AT32(req, 0) = BUFFERS;
  AT32(req, 4) = 1;
  AT32(req, 8) = 1;
  if (ioctl(fd, REQBUFS, req) < 0) return ERRNO;
  int granted = AT32(req, 0);
  if (granted <= 0) return EINVAL;
  if (granted > BUFFERS) granted = BUFFERS;
  AT64(s, 24) = granted;
  for (int i = 0; i < granted; i++) {
    i64 buf[BUFFER_WORDS];
    cancho_camera_zero(buf, BUFFER_WORDS);
    AT32(buf, 0) = i;
    AT32(buf, 4) = 1;
    AT32(buf, 60) = 1;
    if (ioctl(fd, QUERYBUF, buf) < 0) return ERRNO;
    unsigned length = (unsigned)AT32(buf, 72);
    unsigned offset = (unsigned)AT32(buf, 64);
    void *at = mmap(0, length, 3, 1, fd, offset);
    if (at == (void *)-1) return ERRNO;
    AT64(s, 32 + 8 * i) = (i64)at;
    AT64(s, 64 + 8 * i) = length;
    if (ioctl(fd, QBUF, buf) < 0) return ERRNO;
  }
  int type = 1;
  if (ioctl(fd, STREAMON, &type) < 0) return ERRNO;
  return 0;
}

i64 cancho_camera_next(i64 *cam, u8 *into, i64 n) {
  u8 *s = (u8 *)*cam;
  if (AT64(s, 8) == 0) return -1;
  int fd = AT32(s, 0);
  int have = -1;
  int err = 0;
  for (;;) {
    i64 buf[BUFFER_WORDS];
    cancho_camera_zero(buf, BUFFER_WORDS);
    AT32(buf, 4) = 1;
    AT32(buf, 60) = 1;
    if (ioctl(fd, DQBUF, buf) < 0) {
      err = ERRNO;
      break;
    }
    int index = AT32(buf, 0);
    if (have >= 0) {
      i64 older[BUFFER_WORDS];
      cancho_camera_zero(older, BUFFER_WORDS);
      AT32(older, 0) = have;
      AT32(older, 4) = 1;
      AT32(older, 60) = 1;
      ioctl(fd, QBUF, older);
    }
    have = index;
  }
  if (have < 0) return err == EAGAIN ? 0 : -1;
  if (have >= BUFFERS) return -1;
  i64 total = AT64(s, 8) * AT64(s, 16);
  i64 count = n < total ? n : total;
  const u8 *frame = (const u8 *)AT64(s, 32 + 8 * have);
  for (i64 k = 0; k < count; k++) into[k] = frame[2 * k];
  i64 back[BUFFER_WORDS];
  cancho_camera_zero(back, BUFFER_WORDS);
  AT32(back, 0) = have;
  AT32(back, 4) = 1;
  AT32(back, 60) = 1;
  ioctl(fd, QBUF, back);
  return count;
}

i64 cancho_camera_close(i64 handle) {
  u8 *s = (u8 *)handle;
  int fd = AT32(s, 0);
  if (AT64(s, 8) != 0) {
    int type = 1;
    ioctl(fd, STREAMOFF, &type);
    for (int i = 0; i < BUFFERS; i++) {
      i64 at = AT64(s, 32 + 8 * i);
      if (at != 0) munmap((void *)at, (unsigned long)AT64(s, 64 + 8 * i));
    }
  }
  int r = close(fd);
  free(s);
  return r;
}
#endif
