#define _GNU_SOURCE
#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <errno.h>
#include <string.h>
#include <sys/stat.h>
#include <stdlib.h>
static void r(const char *what, long v) { printf("%-46s %ld errno=%d (%s)\n", what, v, v < 0 ? errno : 0, v < 0 ? strerror(errno) : "-"); }
int main(void) {
  char t[] = "/tmp/claude-0/s/p/XXXXXX"; if (!mkdtemp(t)) return 1;
  int d = open(t, O_RDONLY | O_DIRECTORY);
  mkdirat(d, "empty", 0755); mkdirat(d, "full", 0755);
  int f = openat(d, "full/x", O_CREAT | O_WRONLY, 0644); close(f);
  symlinkat("empty", d, "lnk_to_dir"); symlinkat("nowhere", d, "dangling");
  r("rmdir empty", unlinkat(d, "empty", AT_REMOVEDIR));
  mkdirat(d, "empty", 0755);
  r("rmdir non-empty", unlinkat(d, "full", AT_REMOVEDIR));
  r("rmdir missing", unlinkat(d, "nope", AT_REMOVEDIR));
  r("rmdir a symlink to a directory", unlinkat(d, "lnk_to_dir", AT_REMOVEDIR));
  r("rmdir a dangling symlink", unlinkat(d, "dangling", AT_REMOVEDIR));
  r("rmdir a regular file", unlinkat(d, "full/x", AT_REMOVEDIR) );
  r("rmdir 'full/x' (two components, not checked)", unlinkat(d, "full/x", AT_REMOVEDIR));
  r("symlinkat empty target", symlinkat("", d, "e"));
  r("symlinkat existing name", symlinkat("a", d, "empty"));
  r("symlinkat onto dangling link", symlinkat("a", d, "dangling"));
  char big[5000]; memset(big, 'a', 4999); big[4999] = 0;
  r("symlinkat 4095-byte target", symlinkat(memset(big, 'a', 4095) ? (big[4095]=0, big) : big, d, "l4095"));
  memset(big, 'a', 4999); big[4096] = 0;
  r("symlinkat 4096-byte target", symlinkat(big, d, "l4096"));
  char buf[8];
  r("readlinkat 'nowhere' into 8 bytes", readlinkat(d, "dangling", buf, 8));
  r("readlinkat 'nowhere' into 7 bytes (exact)", readlinkat(d, "dangling", buf, 7));
  r("readlinkat 'nowhere' into 4 bytes (cut)", readlinkat(d, "dangling", buf, 4));
  r("readlinkat a regular file", readlinkat(d, "full/x", buf, 8));
  r("readlinkat a directory", readlinkat(d, "full", buf, 8));
  r("readlinkat missing", readlinkat(d, "nope", buf, 8));
  r("readlinkat into 0 bytes", readlinkat(d, "dangling", buf, 0));
  r("readlinkat 4096 target into 4096", readlinkat(d, "l4096", big, 4096));
  r("openat O_NOFOLLOW on a link", openat(d, "lnk_to_dir", O_RDONLY | O_NOFOLLOW));
  printf("ENOTEMPTY=%d ENOTDIR=%d EEXIST=%d ENOENT=%d ENAMETOOLONG=%d\n", ENOTEMPTY, ENOTDIR, EEXIST, ENOENT, ENAMETOOLONG);
  printf("AT_REMOVEDIR=%#x NAME_MAX path limit PATH_MAX=%d\n", AT_REMOVEDIR, 4096);
  char cmd[200]; snprintf(cmd, sizeof cmd, "rm -rf %s", t); system(cmd);
  return 0;
}
