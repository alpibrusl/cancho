// Measures renameat2(RENAME_NOREPLACE) (Linux) or renameatx_np(RENAME_EXCL) (macOS) on the filesystem holding argv[1]:
// docs/directory-handles.md slice 4. Prints one line: the answer for a free
// destination, for an existing destination (and whether its bytes survived),
// and for plain renameat over the same, for contrast.
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <sys/stat.h>
#ifndef __APPLE__
#include <sys/syscall.h>
#endif

#ifdef __APPLE__
// macOS: renameatx_np(RENAME_EXCL), the directory-descriptor form of renamex_np.
int renameatx_np(int, const char *, int, const char *, unsigned int);
#define RENAME_NOREPLACE 0x4
#else
#ifndef RENAME_NOREPLACE
#define RENAME_NOREPLACE (1 << 0)
#endif
#endif

static void put(int dir, const char *name, const char *bytes) {
    int fd = openat(dir, name, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0 || write(fd, bytes, strlen(bytes)) < 0) { perror("put"); _exit(2); }
    close(fd);
}

static int slurp(int dir, const char *name, char *out, int n) {
    int fd = openat(dir, name, O_RDONLY);
    if (fd < 0) return -1;
    int k = read(fd, out, n - 1);
    close(fd);
    out[k < 0 ? 0 : k] = 0;
    return k;
}

static int noreplace(int dir, const char *from, const char *to) {
#ifdef __APPLE__
    return renameatx_np(dir, from, dir, to, RENAME_NOREPLACE) < 0 ? errno : 0;
#else
    // The raw syscall, so the probe does not depend on glibc >= 2.28.
    long r = syscall(SYS_renameat2, dir, from, dir, to, RENAME_NOREPLACE);
    return r < 0 ? errno : 0;
#endif
}

int main(int argc, char **argv) {
    int dir = open(argv[1], O_RDONLY | O_DIRECTORY);
    if (dir < 0) { perror("open dir"); return 2; }
    char buf[64];
    unlinkat(dir, "a", 0); unlinkat(dir, "b", 0); unlinkat(dir, "c", 0);

    put(dir, "a", "AAAA");
    int free_dest = noreplace(dir, "a", "c");          // c is free
    int moved = slurp(dir, "c", buf, sizeof buf) == 4 && !strcmp(buf, "AAAA");

    put(dir, "a", "NEW!");
    put(dir, "b", "KEEP");
    int taken = noreplace(dir, "a", "b");               // b exists
    int kept = slurp(dir, "b", buf, sizeof buf) == 4 && !strcmp(buf, "KEEP");
    int src_left = faccessat(dir, "a", F_OK, 0) == 0;

    int plain = renameat(dir, "a", dir, "b") == 0 ? 0 : errno;  // contrast
    int replaced = slurp(dir, "b", buf, sizeof buf) == 4 && !strcmp(buf, "NEW!");

    printf("free=%d(%s) taken=%d(%s) dest_kept=%d source_left=%d | plain_renameat=%d replaced=%d\n",
           free_dest, strerror(free_dest), taken, strerror(taken), kept, src_left, plain, replaced);
    unlinkat(dir, "a", 0); unlinkat(dir, "b", 0); unlinkat(dir, "c", 0);
    return 0;
}
