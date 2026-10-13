; os_darwin.ll -- what differs between systems, for Darwin: the address of errno, how a spawn is set up, a child's word.

define ptr @cancho_errno_location() {
entry:
  %p = call ptr @__error()
  ret ptr %p
}
declare ptr @__error()

define void @cancho_spawn_setup(ptr %actions, ptr %attr) {
entry:
  call i32 @posix_spawnattr_init(ptr %attr)
  %empty = alloca [128 x i8]
  call i32 @sigemptyset(ptr %empty)
  call i32 @posix_spawnattr_setsigmask(ptr %attr, ptr %empty)
  %every = alloca [128 x i8]
  call i32 @sigfillset(ptr %every)
  call i32 @posix_spawnattr_setsigdefault(ptr %attr, ptr %every)
  call i32 @posix_spawnattr_setflags(ptr %attr, i16 16396)
  ret void
}
declare i32 @posix_spawnattr_init(ptr)
declare i32 @sigemptyset(ptr)
declare i32 @sigfillset(ptr)
declare i32 @posix_spawnattr_setsigmask(ptr, ptr)
declare i32 @posix_spawnattr_setsigdefault(ptr, ptr)
declare i32 @posix_spawnattr_setflags(ptr, i16)

; A child's word: the pid alone.
define i64 @cancho_child_word(i32 %pid) {
entry:
  %low = zext i32 %pid to i64
  ret i64 %low
}

