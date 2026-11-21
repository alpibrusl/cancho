; os_linux.ll -- what differs between systems, for Linux: the address of errno, how a spawn is set up, a child's word.

define ptr @cancho_errno_location() {
entry:
  %p = call ptr @__errno_location()
  ret ptr %p
}
declare ptr @__errno_location()

define void @cancho_spawn_setup(ptr %actions, ptr %attr, i64 %detached) {
entry:
  %session = icmp ne i64 %detached, 0
  call i32 @posix_spawn_file_actions_addclosefrom_np(ptr %actions, i32 3)
  call i32 @posix_spawnattr_init(ptr %attr)
  %empty = alloca [128 x i8]
  call i32 @sigemptyset(ptr %empty)
  call i32 @posix_spawnattr_setsigmask(ptr %attr, ptr %empty)
  %every = alloca [128 x i8]
  call i32 @sigfillset(ptr %every)
  call i32 @posix_spawnattr_setsigdefault(ptr %attr, ptr %every)
  %flags = select i1 %session, i16 140, i16 12
  call i32 @posix_spawnattr_setflags(ptr %attr, i16 %flags)
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

; The system's number for a signal of `child_kill`: by its bit (std.signals', and 256 for KILL); 0 for another.
define i64 @cancho_native_signal(i64 %bit) {
entry:
  %is0 = icmp eq i64 %bit, 64
  %n0 = select i1 %is0, i64 14, i64 0
  %is1 = icmp eq i64 %bit, 1
  %n1 = select i1 %is1, i64 1, i64 %n0
  %is2 = icmp eq i64 %bit, 2
  %n2 = select i1 %is2, i64 2, i64 %n1
  %is3 = icmp eq i64 %bit, 4
  %n3 = select i1 %is3, i64 3, i64 %n2
  %is4 = icmp eq i64 %bit, 8
  %n4 = select i1 %is4, i64 15, i64 %n3
  %is5 = icmp eq i64 %bit, 16
  %n5 = select i1 %is5, i64 10, i64 %n4
  %is6 = icmp eq i64 %bit, 32
  %n6 = select i1 %is6, i64 12, i64 %n5
  %is7 = icmp eq i64 %bit, 128
  %n7 = select i1 %is7, i64 28, i64 %n6
  %is8 = icmp eq i64 %bit, 256
  %n8 = select i1 %is8, i64 9, i64 %n7
  ret i64 %n8
}

; The bit (std.signals', 256 for KILL) of the system's number of a signal that ended a child; 0 for another.
define i64 @cancho_signal_bit(i64 %native) {
entry:
  %is0 = icmp eq i64 %native, 14
  %b0 = select i1 %is0, i64 64, i64 0
  %is1 = icmp eq i64 %native, 1
  %b1 = select i1 %is1, i64 1, i64 %b0
  %is2 = icmp eq i64 %native, 2
  %b2 = select i1 %is2, i64 2, i64 %b1
  %is3 = icmp eq i64 %native, 3
  %b3 = select i1 %is3, i64 4, i64 %b2
  %is4 = icmp eq i64 %native, 15
  %b4 = select i1 %is4, i64 8, i64 %b3
  %is5 = icmp eq i64 %native, 10
  %b5 = select i1 %is5, i64 16, i64 %b4
  %is6 = icmp eq i64 %native, 12
  %b6 = select i1 %is6, i64 32, i64 %b5
  %is7 = icmp eq i64 %native, 28
  %b7 = select i1 %is7, i64 128, i64 %b6
  %is8 = icmp eq i64 %native, 9
  %b8 = select i1 %is8, i64 256, i64 %b7
  ret i64 %b8
}

; Once a child is reaped, its pidfd (the high half of its word) has done its work.
define void @cancho_child_release(i64 %word) {
entry:
  %high = lshr i64 %word, 32
  %pidfd = trunc i64 %high to i32
  %ignored = call i32 @close(i32 %pidfd)
  ret void
}
