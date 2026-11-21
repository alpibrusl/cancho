; process.ll -- what a program that lets a child go (`child_release`, docs/processes.md section 4.11) has: the function a thread runs to
; reap the child, and the call that starts that thread. The same on every system; the pidfd is closed by `@cancho_child_release`.

; The thread: it waits for that one child (its pid is the argument), never for -1, and ends. It installs no handler and takes no signal
; of its own, so nothing interrupts its wait.
define ptr @cancho_child_reaper(ptr %pid) {
entry:
  %status = alloca i64, align 8
  %n = ptrtoint ptr %pid to i64
  %p = trunc i64 %n to i32
  %r = call i32 @waitpid(i32 %p, ptr %status, i32 0)
  ret ptr null
}

; child_release: close the child's pidfd, then give its pid to a detached thread with 64 KiB of stack. 0, or the error number of
; `pthread_create`, in which case the child still runs and is a zombie when it ends until the program does.
define i64 @cancho_child_let_go(i64 %word) {
entry:
  call void @cancho_child_release(i64 %word)
  %low = trunc i64 %word to i32
  %pid = zext i32 %low to i64
  %arg = inttoptr i64 %pid to ptr
  %attr = alloca [128 x i8], align 16
  %thread = alloca i64, align 8
  store i64 0, ptr %thread
  call i32 @pthread_attr_init(ptr %attr)
  call i32 @pthread_attr_setstacksize(ptr %attr, i64 65536)
  %status = call i32 @pthread_create(ptr %thread, ptr %attr, ptr @cancho_child_reaper, ptr %arg)
  call i32 @pthread_attr_destroy(ptr %attr)
  %failed = icmp ne i32 %status, 0
  br i1 %failed, label %done, label %detach
detach:
  %id = load ptr, ptr %thread
  call i32 @pthread_detach(ptr %id)
  br label %done
done:
  %answer = sext i32 %status to i64
  ret i64 %answer
}
declare i32 @pthread_attr_init(ptr)
declare i32 @pthread_attr_setstacksize(ptr, i64)
declare i32 @pthread_attr_destroy(ptr)
declare i32 @pthread_detach(ptr)
