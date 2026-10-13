; os_linux.ll -- what differs between systems, for Linux: the address of errno, how a spawn is set up, a child's word.

define ptr @cancho_errno_location() {
entry:
  %p = call ptr @__errno_location()
  ret ptr %p
}
declare ptr @__errno_location()

define void @cancho_spawn_setup(ptr %actions, ptr %attr) {
entry:
  call i32 @posix_spawn_file_actions_addclosefrom_np(ptr %actions, i32 3)
  call i32 @posix_spawnattr_init(ptr %attr)
  %empty = alloca [128 x i8]
  call i32 @sigemptyset(ptr %empty)
  call i32 @posix_spawnattr_setsigmask(ptr %attr, ptr %empty)
  %every = alloca [128 x i8]
  call i32 @sigfillset(ptr %every)
  call i32 @posix_spawnattr_setsigdefault(ptr %attr, ptr %every)
  call i32 @posix_spawnattr_setflags(ptr %attr, i16 12)
  ret void
}
declare i32 @posix_spawnattr_init(ptr)
declare i32 @sigemptyset(ptr)
declare i32 @sigfillset(ptr)
declare i32 @posix_spawnattr_setsigmask(ptr, ptr)
declare i32 @posix_spawnattr_setsigdefault(ptr, ptr)
declare i32 @posix_spawnattr_setflags(ptr, i16)
declare i32 @posix_spawn_file_actions_addclosefrom_np(ptr, i32)

; A child's word: the pid, and in the high half a pidfd for it (or minus the errno) to watch its exit with.
define i64 @cancho_child_word(i32 %pid) {
entry:
  %low = zext i32 %pid to i64
  %pid64 = sext i32 %pid to i64
  %opened = call i64 (i64, ...) @syscall(i64 434, i64 %pid64, i64 0)
  %reason = call i64 @cancho_errno()
  %refused = icmp slt i64 %opened, 0
  %negated = sub i64 0, %reason
  %chosen = select i1 %refused, i64 %negated, i64 %opened
  %narrow = trunc i64 %chosen to i32
  %wide = zext i32 %narrow to i64
  %high = shl i64 %wide, 32
  %word = or i64 %low, %high
  ret i64 %word
}
declare i64 @syscall(i64, ...)

