/* dirs.c -- directory handles and listings, for the code of the compiler written in cancho.
 *
 * SOURCE of `dirs_linux.ll` and `dirs_darwin.ll` (`make.sh`), which `driver.cho` prints into a module that opens a
 * directory. The same text for every architecture: what differs between them (open flags, `AT_FDCWD`, `struct dirent` and `struct stat`
 * offsets, errno values) is the array `cancho_plat`, which the compiler writes for the target it builds for
 * (`platform.cho`). A function that answers several values writes them to `out`.
 */
typedef long i64;
typedef unsigned char u8;

extern const i64 cancho_plat[];
#define P(i) (cancho_plat[i])
enum {
  OPEN_DIR, OPEN_ENTER, OPEN_READ, OPEN_NEW, OPEN_APPEND, FDCWD, NOREPLACE, UNSUPPORTED, ENAMETOOLONG,
  D_TYPE, D_NAME, DT_UNKNOWN, DT_LNK, DT_DIR, DT_REG, ST_MODE, ST_MODE_BITS, ST_SIZE, ST_MTIME, AT_NOFOLLOW,
  CREATE_MODE
};
#define EINVAL 22
#define NAME_MAX 255

extern int openat(int, const char *, int, ...);
extern int renameat(int, const char *, int, const char *);
#if DARWIN
extern int renameatx_np(int, const char *, int, const char *, unsigned);
#else
extern int renameat2(int, const char *, int, const char *, unsigned);
#endif
extern int unlinkat(int, const char *, int);
extern int fsync(int);
extern int fstatat(int, const char *, void *, int);
extern int fstat(int, void *);
extern int close(int);
extern void *fdopendir(int);
extern void *readdir(void *);
extern int closedir(void *);
extern i64 strlen(const char *);
extern void *memchr(const void *, int, i64);
extern void *memmove(void *, const void *, i64);
extern u8 *cancho_checked_path(const u8 *, i64, const u8 *, i64);
extern void free(void *);
extern int *cancho_errno_location(void);
#define ERRNO (*cancho_errno_location())

/* One path component: not empty, not longer than NAME_MAX, not `.` or `..`, no `/` and no NUL. */
static int dirrt_component(const u8 *name, i64 len, u8 *copy) {
  if (len == 0 || len > NAME_MAX) return 0;
  if (memchr(name, 47, len) || memchr(name, 0, len)) return 0;
  if (name[0] == 46 && (len == 1 || (len == 2 && name[1] == 46))) return 0;
  memmove(copy, name, len);
  copy[len] = 0;
  return 1;
}

/* The three numbers of `Opened`, `DirOpened` and `Done`: the tag, the result, why. */
static void dirrt_triple(i64 *out, i64 result, i64 why) {
  out[0] = result < 0;
  out[1] = result;
  out[2] = why;
}

static void dirrt_refused(i64 *out) { dirrt_triple(out, -1, EINVAL); }

/* `open_dir(fs, path)`. */
void cancho_dir_open(const u8 *src, i64 len, const u8 *prefix, i64 plen, i64 *out) {
  u8 *path = cancho_checked_path(src, len, prefix, plen);
  int fd = openat((int)P(FDCWD), (const char *)path, (int)P(OPEN_DIR), 0);
  i64 why = ERRNO;
  free(path);
  dirrt_triple(out, fd, why);
}

/* `dir_enter` (0), `dir_open_read` (1), `dir_open_new` (2), `dir_open_append` (3). */
void cancho_dir_step(const i64 *dir, const u8 *name, i64 len, i64 which, i64 *out) {
  u8 copy[NAME_MAX + 1];
  if (!dirrt_component(name, len, copy)) return dirrt_refused(out);
  int flags = (int)P(OPEN_ENTER + (which == 0 ? 0 : which));
  int fd = openat((int)*dir, (const char *)copy, flags, (int)P(CREATE_MODE));
  dirrt_triple(out, fd, ERRNO);
}

/* `dir_rename` and, with `fresh`, `dir_rename_new`. */
void cancho_dir_rename(const i64 *dir, const u8 *from, i64 flen, const u8 *to, i64 tlen, i64 fresh, i64 *out) {
  u8 a[NAME_MAX + 1], b[NAME_MAX + 1];
  if (!dirrt_component(from, flen, a) || !dirrt_component(to, tlen, b)) return dirrt_refused(out);
  int fd = (int)*dir, r;
  if (!fresh) {
    r = renameat(fd, (const char *)a, fd, (const char *)b);
  } else {
#if DARWIN
    r = renameatx_np(fd, (const char *)a, fd, (const char *)b, (unsigned)P(NOREPLACE));
#else
    r = renameat2(fd, (const char *)a, fd, (const char *)b, (unsigned)P(NOREPLACE));
#endif
  }
  i64 why = ERRNO;
  if (fresh && r < 0 && why == EINVAL) why = P(UNSUPPORTED);
  dirrt_triple(out, r, why);
}

void cancho_dir_remove(const i64 *dir, const u8 *name, i64 len, i64 *out) {
  u8 copy[NAME_MAX + 1];
  if (!dirrt_component(name, len, copy)) return dirrt_refused(out);
  int r = unlinkat((int)*dir, (const char *)copy, 0);
  dirrt_triple(out, r, ERRNO);
}

void cancho_dir_sync(const i64 *dir, i64 *out) {
  int r = fsync((int)*dir);
  dirrt_triple(out, r, ERRNO);
}

/* `dir_list(dir)`: a stream on a descriptor of its own. out = tag, the stream, why. */
void cancho_dir_list(const i64 *dir, i64 *out) {
  u8 dot[2];
  dot[0] = 46;
  dot[1] = 0;
  int own = openat((int)*dir, (const char *)dot, (int)P(OPEN_DIR), 0);
  i64 why = ERRNO;
  out[0] = 1;
  out[1] = 0;
  out[2] = why;
  if (own < 0) return;
  void *stream = fdopendir(own);
  why = ERRNO;
  if (stream == 0) {
    close(own);
    out[2] = why;
    return;
  }
  out[0] = 0;
  out[1] = (i64)stream;
  out[2] = 0;
}

static i64 dirrt_kind(i64 dt) {
  if (dt == P(DT_UNKNOWN)) return 0;
  if (dt == P(DT_LNK)) return 3;
  if (dt == P(DT_DIR)) return 2;
  if (dt == P(DT_REG)) return 1;
  return 4;
}

/* `dir_next(list, name)`: out = tag (0 Name, 1 End, 2 Failed), the length, the kind, why. */
void cancho_dir_next(const i64 *list, u8 *buf, i64 room, i64 *out) {
  void *stream = (void *)*list;
  for (;;) {
    ERRNO = 0;
    u8 *entry = readdir(stream);
    if (entry == 0) {
      i64 why = ERRNO;
      out[0] = why == 0 ? 1 : 2;
      out[1] = 0;
      out[2] = 0;
      out[3] = why;
      return;
    }
    const u8 *name = entry + P(D_NAME);
    i64 n = strlen((const char *)name);
    if (name[0] == 46 && (n == 1 || (n == 2 && name[1] == 46))) continue;
    if (n > room) {
      out[0] = 2;
      out[1] = 0;
      out[2] = 0;
      out[3] = P(ENAMETOOLONG);
      return;
    }
    memmove(buf, name, n);
    out[0] = 0;
    out[1] = n;
    out[2] = dirrt_kind(entry[P(D_TYPE)]);
    out[3] = 0;
    return;
  }
}

i64 cancho_dir_list_close(i64 stream) { return closedir((void *)stream); }

static i64 dirrt_field(const u8 *st, i64 at, i64 bits) {
  if (bits == 64) return *(const i64 *)(st + at);
  if (bits == 32) return *(const unsigned *)(st + at);
  return *(const unsigned short *)(st + at);
}

/* `dir_stat(dir, name)`: out = tag, the kind, the size, the modification time, why. */
void cancho_dir_stat(const i64 *dir, const u8 *name, i64 len, i64 *out) {
  u8 copy[NAME_MAX + 1];
  u8 st[160];
  for (int i = 0; i < 5; i++) out[i] = 0;
  if (!dirrt_component(name, len, copy)) {
    out[0] = 1;
    out[4] = EINVAL;
    return;
  }
  int r = fstatat((int)*dir, (const char *)copy, st, (int)P(AT_NOFOLLOW));
  i64 why = ERRNO;
  if (r < 0) {
    out[0] = 1;
    out[4] = why;
    return;
  }
  i64 type = dirrt_field(st, P(ST_MODE), P(ST_MODE_BITS)) & 0170000;
  out[1] = type == 0120000 ? 3 : type == 0040000 ? 2 : type == 0100000 ? 1 : 4;
  out[2] = dirrt_field(st, P(ST_SIZE), 64);
  out[3] = dirrt_field(st, P(ST_MTIME), 64);
}

/* `dir_mode(dir, name)` and, with no name (`own`), `dir_own_mode(dir)`: the permission bits as a `Done`. */
void cancho_dir_mode(const i64 *dir, const u8 *name, i64 len, i64 own, i64 *out) {
  u8 copy[NAME_MAX + 1];
  u8 st[160];
  int r;
  if (own) {
    r = fstat((int)*dir, st);
  } else {
    if (!dirrt_component(name, len, copy)) return dirrt_refused(out);
    r = fstatat((int)*dir, (const char *)copy, st, (int)P(AT_NOFOLLOW));
  }
  i64 why = ERRNO;
  out[0] = r < 0;
  out[1] = r < 0 ? 0 : (dirrt_field(st, P(ST_MODE), P(ST_MODE_BITS)) & 07777);
  out[2] = why;
}
