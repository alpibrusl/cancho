// probe2.c -- what probe.c did not ask (docs/directory-links.md §2, docs/vcs-worktree.md §5): renaming over an existing file and over a link, and the longest name that can
// have a `NAME.cancho-tmp-0` (13 bytes) beside it. Built with `cc -o probe2 probe2.c && ./probe2`; it makes and removes its own directory under $TMPDIR (or /tmp).
#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <errno.h>
#include <string.h>
#include <stdlib.h>
#include <sys/stat.h>
static void r(const char *what, long v) { printf("%-52s %ld errno=%d (%s)\n", what, v, v < 0 ? errno : 0, v < 0 ? strerror(errno) : "-"); }
static void put(int d, const char *name, const char *text) { int f = openat(d, name, O_CREAT | O_WRONLY | O_TRUNC, 0644); if (f >= 0) { write(f, text, strlen(text)); close(f); } }
static void show(int d, const char *name) {
  struct stat s; char b[64]; ssize_t n;
  if (fstatat(d, name, &s, AT_SYMLINK_NOFOLLOW) < 0) { printf("    %s: absent\n", name); return; }
  if (S_ISLNK(s.st_mode)) { n = readlinkat(d, name, b, sizeof b - 1); b[n < 0 ? 0 : n] = 0; printf("    %s: link -> %s\n", name, b); }
  else if (S_ISDIR(s.st_mode)) printf("    %s: directory\n", name);
  else { int f = openat(d, name, O_RDONLY); n = read(f, b, sizeof b - 1); close(f); b[n < 0 ? 0 : n] = 0; printf("    %s: file \"%s\"\n", name, b); }
}
int main(void) {
  const char *tmp = getenv("TMPDIR"); char t[512]; snprintf(t, sizeof t, "%s/probe2.XXXXXX", tmp ? tmp : "/tmp");
  if (!mkdtemp(t)) return 1;
  int d = open(t, O_RDONLY | O_DIRECTORY);
  put(d, "new", "NEW"); put(d, "old", "OLD");
  r("rename file over an existing file", renameat(d, "new", d, "old")); show(d, "old"); show(d, "new");
  put(d, "new", "NEW2"); symlinkat("elsewhere", d, "lnk"); put(d, "elsewhere", "TARGET");
  r("rename file over a link (link to a file)", renameat(d, "new", d, "lnk")); show(d, "lnk"); show(d, "elsewhere");
  mkdirat(d, "dir", 0755); symlinkat("dir", d, "lnkdir"); put(d, "new", "NEW3");
  r("rename file over a link to a directory", renameat(d, "new", d, "lnkdir")); show(d, "lnkdir"); show(d, "dir");
  symlinkat("dangling-target", d, "dang"); put(d, "new", "NEW4");
  r("rename file over a dangling link", renameat(d, "new", d, "dang")); show(d, "dang");
  symlinkat("tgt", d, "src_link"); put(d, "victim", "VICTIM");
  r("rename a link over a file", renameat(d, "src_link", d, "victim")); show(d, "victim");
  put(d, "f", "F"); mkdirat(d, "somedir", 0755);
  r("rename a file over a directory", renameat(d, "f", d, "somedir"));
  r("rename a directory over a file", renameat(d, "somedir", d, "old"));
  mkdirat(d, "d2", 0755); mkdirat(d, "d3", 0755); put(d, "d3/x", "x");
  r("rename a directory over an empty directory", renameat(d, "d2", d, "dir"));
  r("rename a directory over a non-empty directory", renameat(d, "dir", d, "d3"));
  r("rename a missing name", renameat(d, "nope", d, "zz"));
  // the longest name that can have NAME.cancho-tmp-0 beside it: make NAME of n bytes, then NAME + the 13-byte suffix
  char name[400]; int last_ok = -1, first_bad = -1;
  for (int n = 230; n <= 262; n++) {
    memset(name, 'n', n); name[n] = 0;
    int a = openat(d, name, O_CREAT | O_WRONLY, 0644); int ea = errno;
    char tmpn[420]; snprintf(tmpn, sizeof tmpn, "%s.cancho-tmp-0", name);
    int b = openat(d, tmpn, O_CREAT | O_WRONLY, 0644); int eb = errno;
    if (a >= 0 && b >= 0) last_ok = n; else if (first_bad < 0) { first_bad = n; printf("first n that fails: %d (name: %s, name+13: %s)\n", n, a >= 0 ? "made" : strerror(ea), b >= 0 ? "made" : strerror(eb)); }
    if (a >= 0) { close(a); unlinkat(d, name, 0); } if (b >= 0) { close(b); unlinkat(d, tmpn, 0); }
  }
  printf("longest name with a NAME.cancho-tmp-0 beside it: %d bytes\n", last_ok);
  for (int n = 250; n <= 258; n++) { memset(name, 'n', n); name[n] = 0; int a = openat(d, name, O_CREAT | O_WRONLY, 0644); printf("  a name of %d bytes: %s\n", n, a >= 0 ? "made" : strerror(errno)); if (a >= 0) { close(a); unlinkat(d, name, 0); } }
  { // the longest symlink target the kernel accepts, and whether readlinkat gives it back whole
    char big[5000]; int ok = 0;
    for (int n = 1; n <= 4100; n++) { memset(big, 'a', n); big[n] = 0; char ln[32]; snprintf(ln, sizeof ln, "L%d", n); if (symlinkat(big, d, ln) == 0) { ok = n; unlinkat(d, ln, 0); } else { printf("first target length refused: %d (%s); longest made: %d\n", n, strerror(errno), ok); break; } }
    memset(big, 'a', ok); big[ok] = 0; symlinkat(big, d, "Lmax"); char back[5000]; ssize_t got = readlinkat(d, "Lmax", back, sizeof back);
    printf("readlinkat of the longest target (%d bytes) into 5000: %zd\n", ok, got);
  }
  printf("ENOTEMPTY=%d ENOTDIR=%d EISDIR=%d EEXIST=%d ENOENT=%d ENAMETOOLONG=%d ELOOP=%d EINVAL=%d AT_REMOVEDIR=%#x\n", ENOTEMPTY, ENOTDIR, EISDIR, EEXIST, ENOENT, ENAMETOOLONG, ELOOP, EINVAL, AT_REMOVEDIR);
  char cmd[600]; snprintf(cmd, sizeof cmd, "rm -rf '%s'", t); system(cmd);
  return 0;
}
