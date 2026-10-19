; runtime.ll -- the support the code of a few builtins calls: paths checked against the prefix a capability was
; narrowed to, files, processes, arguments.
;
; SOURCE of `runtime.cho` (`crates/cancho-ir/src/tests/selfhost_tables.rs` checks that file is this one as a string;
; `UPDATE_SELFHOST_TABLES=1 cargo test -p cancho-ir selfhost_tables` rewrites it). `driver.cho` prints it in the
; module it writes, after a `cancho_errno_location` of the target's own.

declare i64 @fwrite(ptr, i64, i64, ptr)
declare ptr @fopen(ptr, ptr)
declare i32 @fileno(ptr)
declare i32 @fclose(ptr)
declare i32 @dup(i32)
declare i32 @close(i32)
declare i32 @fcntl(i32, i32, ...)
declare i64 @read(i32, ptr, i64)
declare i64 @strlen(ptr)
declare i32 @kill(i32, i32)
declare i32 @openat(i32, ptr, i32, ...)
declare i32 @kqueue()
declare i32 @kevent(i32, ptr, i32, ptr, i32, ptr)
declare i64 @pread(i32, ptr, i64, i64)
declare i64 @pwrite(i32, ptr, i64, i64)
declare i32 @fsync(i32)
declare i32 @ftruncate(i32, i64)
declare i64 @lseek(i32, i64, i32)
declare i32 @flock(i32, i32)
declare i32 @unlink(ptr)
declare i32 @rename(ptr, ptr)
declare i32 @waitpid(i32, ptr, i32)
declare i32 @posix_spawn(ptr, ptr, ptr, ptr, ptr, ptr)
declare i32 @posix_spawn_file_actions_init(ptr)
declare i32 @posix_spawn_file_actions_destroy(ptr)
declare i32 @posix_spawn_file_actions_addopen(ptr, i32, ptr, i32, i32)
declare i32 @posix_spawn_file_actions_adddup2(ptr, i32, i32)
declare i32 @posix_spawn_file_actions_addfchdir_np(ptr, i32)
declare i32 @posix_spawnattr_destroy(ptr)

@cancho_signal_state = global [2 x i64] zeroinitializer
@cancho_argc = global i64 0
@cancho_argv = global ptr null
@cancho_dev_null = private constant [10 x i8] c"/dev/null\00"
@cancho_mode_rb = private constant [3 x i8] c"rb\00"
@cancho_mode_ab = private constant [3 x i8] c"ab\00"
@cancho_mode_wb = private constant [3 x i8] c"wb\00"
@cancho_mode_wbx = private constant [4 x i8] c"wbx\00"
@cancho_mode_rpb = private constant [4 x i8] c"r+b\00"
@cancho_ld = private constant [3 x i8] c"LD_"
@cancho_dyld = private constant [5 x i8] c"DYLD_"

define i64 @cancho_errno() {
entry:
  %p = call ptr @cancho_errno_location()
  %v = load i32, ptr %p
  %w = sext i32 %v to i64
  ret i64 %w
}

; Is the path (%src, %len) inside the prefix a capability was narrowed to? The one rule (docs/filesystem.md 1.1 and 4,
; docs/processes.md 4.13): not when it is 4096 bytes or more, shorter than the prefix, has ".." in it, does not begin with
; the prefix, or (when the prefix is not empty and does not end in "/") goes on from it at a byte other than "/" (a NUL
; there ends the C string at the prefix, so it is the prefix). `cancho_checked_path` traps on the answer, and
; `exec_covers`/`fs_covers` return it, so what a program is told is what the runtime enforces.
define i1 @cancho_path_covers(ptr %src, i64 %len, ptr %prefix, i64 %plen) {
entry:
  %toolong = icmp uge i64 %len, 4096
  br i1 %toolong, label %no, label %c1
c1:
  %short = icmp ult i64 %len, %plen
  br i1 %short, label %no, label %loop
loop:
  %i = phi i64 [ 0, %c1 ], [ %next, %cont ]
  %more = icmp ult i64 %i, %len
  br i1 %more, label %body, label %done
body:
  %a = getelementptr i8, ptr %src, i64 %i
  %byte = load i8, ptr %a
  %isdot = icmp eq i8 %byte, 46
  %hasprev = icmp ugt i64 %i, 0
  %pi = sub i64 %i, 1
  %pidx = select i1 %hasprev, i64 %pi, i64 0
  %pa = getelementptr i8, ptr %src, i64 %pidx
  %pbyte = load i8, ptr %pa
  %pdot = icmp eq i8 %pbyte, 46
  %both0 = and i1 %isdot, %pdot
  %both = and i1 %both0, %hasprev
  br i1 %both, label %no, label %inpre
inpre:
  %insidep = icmp ult i64 %i, %plen
  br i1 %insidep, label %cmp, label %cont
cmp:
  %ea = getelementptr i8, ptr %prefix, i64 %i
  %ebyte = load i8, ptr %ea
  %diff = icmp ne i8 %ebyte, %byte
  br i1 %diff, label %no, label %cont
cont:
  %next = add i64 %i, 1
  br label %loop
done:
  %nonempty = icmp ugt i64 %plen, 0
  br i1 %nonempty, label %b1, label %yes
b1:
  %li = sub i64 %plen, 1
  %la = getelementptr i8, ptr %prefix, i64 %li
  %last = load i8, ptr %la
  %slash = icmp eq i8 %last, 47
  br i1 %slash, label %yes, label %b2
b2:
  %longer = icmp ugt i64 %len, %plen
  br i1 %longer, label %b3, label %yes
b3:
  %ba = getelementptr i8, ptr %src, i64 %plen
  %bbyte = load i8, ptr %ba
  %bslash = icmp eq i8 %bbyte, 47
  %bnul = icmp eq i8 %bbyte, 0
  %bok = or i1 %bslash, %bnul
  ret i1 %bok
yes:
  ret i1 true
no:
  ret i1 false
}

; A path under the prefix a capability was narrowed to, as a NUL-terminated copy; anything else traps
; (`cancho_path_covers` says which).
define ptr @cancho_checked_path(ptr %src, i64 %len, ptr %prefix, i64 %plen) {
entry:
  %inside = call i1 @cancho_path_covers(ptr %src, i64 %len, ptr %prefix, i64 %plen)
  br i1 %inside, label %c2, label %trap
c2:
  %size = add i64 %len, 1
  %buf = call ptr @malloc(i64 %size)
  %none = icmp eq ptr %buf, null
  br i1 %none, label %trap, label %loop
loop:
  %i = phi i64 [ 0, %c2 ], [ %next, %body ]
  %more = icmp ult i64 %i, %len
  br i1 %more, label %body, label %done
body:
  %a = getelementptr i8, ptr %src, i64 %i
  %byte = load i8, ptr %a
  %d = getelementptr i8, ptr %buf, i64 %i
  store i8 %byte, ptr %d
  %next = add i64 %i, 1
  br label %loop
done:
  %end = getelementptr i8, ptr %buf, i64 %len
  store i8 0, ptr %end
  ret ptr %buf
trap:
  call void @cancho_trap()
  unreachable
}

; open_read (0), open_write (1), open_append (2), open_new (3), open_rw (4): a descriptor, or minus the errno.
define i64 @cancho_open(ptr %src, i64 %len, ptr %prefix, i64 %plen, i64 %mode) {
entry:
  %path = call ptr @cancho_checked_path(ptr %src, i64 %len, ptr %prefix, i64 %plen)
  switch i64 %mode, label %read [ i64 1, label %write
                                  i64 2, label %append
                                  i64 3, label %new
                                  i64 4, label %rw ]
read:
  br label %open
write:
  br label %open
append:
  br label %open
new:
  br label %open
rw:
  br label %open
open:
  %text = phi ptr [ @cancho_mode_rb, %read ], [ @cancho_mode_wb, %write ], [ @cancho_mode_ab, %append ], [ @cancho_mode_wbx, %new ], [ @cancho_mode_rpb, %rw ]
  %fp = call ptr @fopen(ptr %path, ptr %text)
  %bad = icmp eq ptr %fp, null
  br i1 %bad, label %fail, label %good
fail:
  %e = call i64 @cancho_errno()
  call void @free(ptr %path)
  %neg = sub i64 0, %e
  ret i64 %neg
good:
  %fd = call i32 @fileno(ptr %fp)
  %copy = call i32 @dup(i32 %fd)
  %derr = call i64 @cancho_errno()
  %dupok = icmp sge i32 %copy, 0
  br i1 %dupok, label %set, label %dupfail
set:
  call i32 (i32, i32, ...) @fcntl(i32 %copy, i32 2, i32 1)
  call i32 @fclose(ptr %fp)
  call void @free(ptr %path)
  %r = sext i32 %copy to i64
  ret i64 %r
dupfail:
  call i32 @fclose(ptr %fp)
  call void @free(ptr %path)
  %n2 = sub i64 0, %derr
  ret i64 %n2
}

; fs_read (0) and fs_write (1) in one: the path checked, opened, one read or write, closed; the count, or -1.
define i64 @cancho_fs_once(ptr %src, i64 %len, ptr %prefix, i64 %plen, ptr %p, i64 %n, i64 %writing) {
entry:
  %mode = icmp eq i64 %writing, 1
  %kind = select i1 %mode, i64 1, i64 0
  %fd = call i64 @cancho_open(ptr %src, i64 %len, ptr %prefix, i64 %plen, i64 %kind)
  %bad = icmp slt i64 %fd, 0
  br i1 %bad, label %fail, label %go
fail:
  ret i64 -1
go:
  %f32 = trunc i64 %fd to i32
  br i1 %mode, label %w, label %r
w:
  %wn = call i64 @write(i32 %f32, ptr %p, i64 %n)
  br label %done
r:
  %rn = call i64 @read(i32 %f32, ptr %p, i64 %n)
  br label %done
done:
  %moved = phi i64 [ %wn, %w ], [ %rn, %r ]
  call i32 @close(i32 %f32)
  ret i64 %moved
}

; write(2) and read(2) on a descriptor: the count, or minus the errno.
define i64 @cancho_fd_write(i64 %fd, ptr %p, i64 %n) {
entry:
  %f = trunc i64 %fd to i32
  %r = call i64 @write(i32 %f, ptr %p, i64 %n)
  %bad = icmp slt i64 %r, 0
  br i1 %bad, label %fail, label %ok
fail:
  %e = call i64 @cancho_errno()
  %neg = sub i64 0, %e
  ret i64 %neg
ok:
  ret i64 %r
}

define i64 @cancho_fd_read(i64 %fd, ptr %p, i64 %n) {
entry:
  %f = trunc i64 %fd to i32
  %r = call i64 @read(i32 %f, ptr %p, i64 %n)
  %bad = icmp slt i64 %r, 0
  br i1 %bad, label %fail, label %ok
fail:
  %e = call i64 @cancho_errno()
  %neg = sub i64 0, %e
  ret i64 %neg
ok:
  ret i64 %r
}

define i64 @cancho_fd_close(i64 %fd) {
entry:
  %f = trunc i64 %fd to i32
  %r = call i32 @close(i32 %f)
  %w = sext i32 %r to i64
  ret i64 %w
}

; file_pwrite and file_pread: one pwrite(2) or pread(2) at an offset; the count, or minus the errno.
define i64 @cancho_fd_pwrite(i64 %fd, ptr %p, i64 %n, i64 %at) {
entry:
  %f = trunc i64 %fd to i32
  %r = call i64 @pwrite(i32 %f, ptr %p, i64 %n, i64 %at)
  %bad = icmp slt i64 %r, 0
  br i1 %bad, label %fail, label %ok
fail:
  %e = call i64 @cancho_errno()
  %neg = sub i64 0, %e
  ret i64 %neg
ok:
  ret i64 %r
}

define i64 @cancho_fd_pread(i64 %fd, ptr %p, i64 %n, i64 %at) {
entry:
  %f = trunc i64 %fd to i32
  %r = call i64 @pread(i32 %f, ptr %p, i64 %n, i64 %at)
  %bad = icmp slt i64 %r, 0
  br i1 %bad, label %fail, label %ok
fail:
  %e = call i64 @cancho_errno()
  %neg = sub i64 0, %e
  ret i64 %neg
ok:
  ret i64 %r
}

; file_sync, file_truncate, file_lock: 0, or minus the errno.
define i64 @cancho_fd_sync(i64 %fd) {
entry:
  %f = trunc i64 %fd to i32
  %r = call i32 @fsync(i32 %f)
  %bad = icmp slt i32 %r, 0
  br i1 %bad, label %fail, label %ok
fail:
  %e = call i64 @cancho_errno()
  %neg = sub i64 0, %e
  ret i64 %neg
ok:
  ret i64 0
}

define i64 @cancho_fd_truncate(i64 %fd, i64 %len) {
entry:
  %f = trunc i64 %fd to i32
  %r = call i32 @ftruncate(i32 %f, i64 %len)
  %bad = icmp slt i32 %r, 0
  br i1 %bad, label %fail, label %ok
fail:
  %e = call i64 @cancho_errno()
  %neg = sub i64 0, %e
  ret i64 %neg
ok:
  ret i64 0
}

define i64 @cancho_fd_lock(i64 %fd) {
entry:
  %f = trunc i64 %fd to i32
  %r = call i32 @flock(i32 %f, i32 6)
  %bad = icmp slt i32 %r, 0
  br i1 %bad, label %fail, label %ok
fail:
  %e = call i64 @cancho_errno()
  %neg = sub i64 0, %e
  ret i64 %neg
ok:
  ret i64 0
}

; file_size: the end by lseek, the cursor put back; the size, or minus the errno.
define i64 @cancho_fd_size(i64 %fd) {
entry:
  %f = trunc i64 %fd to i32
  %here = call i64 @lseek(i32 %f, i64 0, i32 1)
  %hbad = icmp slt i64 %here, 0
  br i1 %hbad, label %fail, label %end
end:
  %size = call i64 @lseek(i32 %f, i64 0, i32 2)
  %e = call i64 @cancho_errno()
  %back = call i64 @lseek(i32 %f, i64 %here, i32 0)
  %sbad = icmp slt i64 %size, 0
  br i1 %sbad, label %fail2, label %ok
ok:
  ret i64 %size
fail:
  %he = call i64 @cancho_errno()
  %hneg = sub i64 0, %he
  ret i64 %hneg
fail2:
  %neg = sub i64 0, %e
  ret i64 %neg
}

; fs_remove and fs_rename: the paths checked against the prefix, then one unlink(2) or rename(2); 0, or minus the errno.
define i64 @cancho_fs_unlink(ptr %src, i64 %len, ptr %prefix, i64 %plen) {
entry:
  %path = call ptr @cancho_checked_path(ptr %src, i64 %len, ptr %prefix, i64 %plen)
  %r = call i32 @unlink(ptr %path)
  %e = call i64 @cancho_errno()
  call void @free(ptr %path)
  %bad = icmp slt i32 %r, 0
  %neg = sub i64 0, %e
  %out = select i1 %bad, i64 %neg, i64 0
  ret i64 %out
}

define i64 @cancho_fs_rename(ptr %src, i64 %len, ptr %src2, i64 %len2, ptr %prefix, i64 %plen) {
entry:
  %path = call ptr @cancho_checked_path(ptr %src, i64 %len, ptr %prefix, i64 %plen)
  %path2 = call ptr @cancho_checked_path(ptr %src2, i64 %len2, ptr %prefix, i64 %plen)
  %r = call i32 @rename(ptr %path, ptr %path2)
  %e = call i64 @cancho_errno()
  call void @free(ptr %path)
  call void @free(ptr %path2)
  %bad = icmp slt i32 %r, 0
  %neg = sub i64 0, %e
  %out = select i1 %bad, i64 %neg, i64 0
  ret i64 %out
}

; Does the NUL-terminated string at %s begin with the %n bytes at %lit?
define i1 @cancho_begins(ptr %s, ptr %lit, i64 %n) {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %step ]
  %more = icmp ult i64 %i, %n
  br i1 %more, label %body, label %yes
body:
  %a = getelementptr i8, ptr %s, i64 %i
  %x = load i8, ptr %a
  %b = getelementptr i8, ptr %lit, i64 %i
  %y = load i8, ptr %b
  %same = icmp eq i8 %x, %y
  br i1 %same, label %step, label %no
step:
  %next = add i64 %i, 1
  br label %loop
yes:
  ret i1 true
no:
  ret i1 false
}

; A list of NUL-terminated entries (each ends in NUL, so a non-empty list must) as the array of pointers to
; them that `posix_spawn` takes: %first (if not null) in front, a null at the end. An entry of the environment
; (%env) that begins with LD_ or DYLD_ traps.
define ptr @cancho_pointer_list(ptr %list, i64 %len, ptr %first, i1 %env) {
entry:
  %nonempty = icmp ugt i64 %len, 0
  br i1 %nonempty, label %chk, label %count
chk:
  %li = sub i64 %len, 1
  %la = getelementptr i8, ptr %list, i64 %li
  %lb = load i8, ptr %la
  %okend = icmp eq i8 %lb, 0
  br i1 %okend, label %count, label %trap
count:
  br label %cl
cl:
  %j = phi i64 [ 0, %count ], [ %jn, %cs ]
  %n = phi i64 [ 0, %count ], [ %nn, %cs ]
  %cmore = icmp ult i64 %j, %len
  br i1 %cmore, label %cb, label %alloc
cb:
  %ca = getelementptr i8, ptr %list, i64 %j
  %cx = load i8, ptr %ca
  %iszero = icmp eq i8 %cx, 0
  %inc = zext i1 %iszero to i64
  %nn = add i64 %n, %inc
  br label %cs
cs:
  %jn = add i64 %j, 1
  br label %cl
alloc:
  %hasfirst = icmp ne ptr %first, null
  %extra = select i1 %hasfirst, i64 2, i64 1
  %slots = add i64 %n, %extra
  %bytes = mul i64 %slots, 8
  %array = call ptr @malloc(i64 %bytes)
  %nulled = icmp eq ptr %array, null
  br i1 %nulled, label %trap, label %fillfirst
fillfirst:
  br i1 %hasfirst, label %putfirst, label %fill
putfirst:
  store ptr %first, ptr %array
  br label %fill
fill:
  %base = select i1 %hasfirst, i64 1, i64 0
  br label %fl
fl:
  %k = phi i64 [ 0, %fill ], [ %kn, %fnext ]
  %slot = phi i64 [ %base, %fill ], [ %slotn, %fnext ]
  %fmore = icmp ult i64 %k, %len
  br i1 %fmore, label %fb, label %finish
fb:
  ; an entry starts at k when k is 0 or the byte before is NUL
  %kz = icmp eq i64 %k, 0
  %pk = sub i64 %k, 1
  %pkidx = select i1 %kz, i64 0, i64 %pk
  %pa = getelementptr i8, ptr %list, i64 %pkidx
  %pb = load i8, ptr %pa
  %pzero = icmp eq i8 %pb, 0
  %starts = or i1 %kz, %pzero
  br i1 %starts, label %entry_here, label %fnext
entry_here:
  %at = getelementptr i8, ptr %list, i64 %k
  %where = getelementptr ptr, ptr %array, i64 %slot
  store ptr %at, ptr %where
  br i1 %env, label %envchk, label %advance
envchk:
  %isld = call i1 @cancho_begins(ptr %at, ptr @cancho_ld, i64 3)
  %isdyld = call i1 @cancho_begins(ptr %at, ptr @cancho_dyld, i64 5)
  %loader = or i1 %isld, %isdyld
  br i1 %loader, label %trap, label %advance
advance:
  %slotp = add i64 %slot, 1
  br label %fnext
fnext:
  %slotn = phi i64 [ %slot, %fb ], [ %slotp, %advance ]
  %kn = add i64 %k, 1
  br label %fl
finish:
  %endat = getelementptr ptr, ptr %array, i64 %slot
  store ptr null, ptr %endat
  ret ptr %array
trap:
  call void @cancho_trap()
  unreachable
}

; Where one of the child's streams comes from: /dev/null (tag 0), the child's end of a channel (1) or a file (2).
define void @cancho_stream(ptr %actions, i32 %target, i64 %tag, i64 %end, i64 %file) {
entry:
  %isnull = icmp eq i64 %tag, 0
  br i1 %isnull, label %null, label %dup
null:
  call i32 @posix_spawn_file_actions_addopen(ptr %actions, i32 %target, ptr @cancho_dev_null, i32 2, i32 0)
  ret void
dup:
  %ispipe = icmp eq i64 %tag, 1
  %fd64 = select i1 %ispipe, i64 %end, i64 %file
  %fd = trunc i64 %fd64 to i32
  call i32 @posix_spawn_file_actions_adddup2(ptr %actions, i32 %fd, i32 %target)
  ret void
}

; What was handed to the child is the parent's no longer.
define void @cancho_stream_close(i64 %tag, i64 %end, i64 %file) {
entry:
  %given = icmp ne i64 %tag, 0
  br i1 %given, label %close, label %done
close:
  %ispipe = icmp eq i64 %tag, 1
  %fd64 = select i1 %ispipe, i64 %end, i64 %file
  %fd = trunc i64 %fd64 to i32
  call i32 @close(i32 %fd)
  br label %done
done:
  ret void
}

; exec_spawn: the pid, or minus the errno. The streams are (tag, child's end, file) each; a child of `exec_spawn_detached` (%detached is 1) leads a session of its own.
define i64 @cancho_spawn(ptr %src, i64 %len, ptr %prefix, i64 %plen, ptr %args, i64 %alen, ptr %env, i64 %elen, i64 %t0, i64 %e0, i64 %f0, i64 %t1, i64 %e1, i64 %f1, i64 %t2, i64 %e2, i64 %f2, ptr %dir, i64 %detached) {
entry:
  %path = call ptr @cancho_checked_path(ptr %src, i64 %len, ptr %prefix, i64 %plen)
  %argv = call ptr @cancho_pointer_list(ptr %args, i64 %alen, ptr %path, i1 false)
  %envp = call ptr @cancho_pointer_list(ptr %env, i64 %elen, ptr null, i1 true)
  %actions = alloca [512 x i8], align 16
  %pid = alloca i32
  store i32 0, ptr %pid
  call i32 @posix_spawn_file_actions_init(ptr %actions)
  call void @cancho_stream(ptr %actions, i32 0, i64 %t0, i64 %e0, i64 %f0)
  call void @cancho_stream(ptr %actions, i32 1, i64 %t1, i64 %e1, i64 %f1)
  call void @cancho_stream(ptr %actions, i32 2, i64 %t2, i64 %e2, i64 %f2)
  %hasdir = icmp ne ptr %dir, null
  br i1 %hasdir, label %chdir, label %attrs
chdir:
  %dirfd64 = load i64, ptr %dir
  %dirfd = trunc i64 %dirfd64 to i32
  %refused = call i32 @posix_spawn_file_actions_addfchdir_np(ptr %actions, i32 %dirfd)
  %refusal = icmp ne i32 %refused, 0
  br i1 %refusal, label %trap, label %attrs
trap:
  call void @cancho_trap()
  unreachable
attrs:
  %attr = alloca [512 x i8], align 16
  call void @cancho_spawn_setup(ptr %actions, ptr %attr, i64 %detached)
  %err = call i32 @posix_spawn(ptr %pid, ptr %path, ptr %actions, ptr %attr, ptr %argv, ptr %envp)
  call i32 @posix_spawn_file_actions_destroy(ptr %actions)
  call i32 @posix_spawnattr_destroy(ptr %attr)
  call void @free(ptr %argv)
  call void @free(ptr %envp)
  call void @free(ptr %path)
  call void @cancho_stream_close(i64 %t0, i64 %e0, i64 %f0)
  call void @cancho_stream_close(i64 %t1, i64 %e1, i64 %f1)
  call void @cancho_stream_close(i64 %t2, i64 %e2, i64 %f2)
  %failed = icmp ne i32 %err, 0
  br i1 %failed, label %fail, label %ok
fail:
  %e64 = sext i32 %err to i64
  %neg = sub i64 0, %e64
  ret i64 %neg
ok:
  %p = load i32, ptr %pid
  %word = call i64 @cancho_child_word(i32 %p)
  ret i64 %word
}

; child_wait: the exit code (0 to 255), 256 and the signal's bit (0 for a signal that has none) if it was killed, or minus the errno.
define i64 @cancho_wait(i64 %pid) {
entry:
  %cell = alloca i32
  %p = trunc i64 %pid to i32
  br label %loop
loop:
  %r = call i32 @waitpid(i32 %p, ptr %cell, i32 0)
  %bad = icmp slt i32 %r, 0
  br i1 %bad, label %err, label %done
err:
  %e = call i64 @cancho_errno()
  %eintr = icmp eq i64 %e, 4
  br i1 %eintr, label %loop, label %fail
fail:
  %neg = sub i64 0, %e
  ret i64 %neg
done:
  call void @cancho_child_release(i64 %pid)
  %status = load i32, ptr %cell
  %low = and i32 %status, 127
  %exited = icmp eq i32 %low, 0
  br i1 %exited, label %code, label %sig
code:
  %sh = lshr i32 %status, 8
  %c = and i32 %sh, 255
  %cz = zext i32 %c to i64
  ret i64 %cz
sig:
  %sz = zext i32 %low to i64
  %bit = call i64 @cancho_signal_bit(i64 %sz)
  %v = add i64 %bit, 256
  ret i64 %v
}

; child_kill: one of the signals by its bit, to the child whose word is in the slot; 0, or the errno (EINVAL, with no call, for a bit that is none).
define i64 @cancho_child_kill(ptr %child, i64 %bit) {
entry:
  %word = load i64, ptr %child
  %pid = trunc i64 %word to i32
  %native = call i64 @cancho_native_signal(i64 %bit)
  %known = icmp ne i64 %native, 0
  br i1 %known, label %send, label %bad
bad:
  ret i64 22
send:
  %sig = trunc i64 %native to i32
  %r = call i32 @kill(i32 %pid, i32 %sig)
  %e = call i64 @cancho_errno()
  %failed = icmp slt i32 %r, 0
  %v = select i1 %failed, i64 %e, i64 0
  ret i64 %v
}

; child_kill_group: `child_kill` to the process group the child leads (`kill(-pid)`, docs/processes.md 4.12). A pid of 1 or less would be
; kill(-1) or kill(0), which are never sent: EINVAL, as for a bit that is none. A child that leads no group is ESRCH, and nothing is sent.
define i64 @cancho_child_kill_group(ptr %child, i64 %bit) {
entry:
  %word = load i64, ptr %child
  %pid = trunc i64 %word to i32
  %native = call i64 @cancho_native_signal(i64 %bit)
  %known = icmp ne i64 %native, 0
  %sane = icmp sgt i32 %pid, 1
  %ok = and i1 %known, %sane
  br i1 %ok, label %send, label %bad
bad:
  ret i64 22
send:
  %sig = trunc i64 %native to i32
  %group = sub i32 0, %pid
  %r = call i32 @kill(i32 %group, i32 %sig)
  %e = call i64 @cancho_errno()
  %failed = icmp slt i32 %r, 0
  %v = select i1 %failed, i64 %e, i64 0
  ret i64 %v
}

; child_pid: the pid, the low 32 bits of the word in the slot a reference to the child points at.
define i64 @cancho_child_pid(ptr %child) {
entry:
  %word = load i64, ptr %child
  %low = trunc i64 %word to i32
  %pid = zext i32 %low to i64
  ret i64 %pid
}

; arg: argument n of the command line as a pointer; an index outside 0 to argc traps.
define ptr @cancho_arg(i64 %n) {
entry:
  %argc = load i64, ptr @cancho_argc
  %past = icmp uge i64 %n, %argc
  br i1 %past, label %trap, label %ok
ok:
  %argv = load ptr, ptr @cancho_argv
  %slot = getelementptr ptr, ptr %argv, i64 %n
  %p = load ptr, ptr %slot
  ret ptr %p
trap:
  call void @cancho_trap()
  unreachable
}
