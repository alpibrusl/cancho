
; bare.ll -- the runtime of a program with no system under it (`--target aarch64-none-elf`, docs/freestanding.md).
; Written by hand. No libc and no threads: the chunks of the regions come from the memory between two symbols the
; link provides, `__cancho_heap_start` and `__cancho_heap_end`, and the four functions LLVM itself calls for a copy
; or a fill are defined here.
@__cancho_heap_start = external global i8
@__cancho_heap_end = external global i8
@cancho_bare_next = internal global i64 0
@cancho_bare_free = internal global ptr null
declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)

; A chunk of 64 KiB: one given back earlier, or the next of the memory; none left is a trap.
define internal ptr @cancho_region_take() {
entry:
  %f = load ptr, ptr @cancho_bare_free
  %none = icmp eq ptr %f, null
  br i1 %none, label %fresh, label %pop
pop:
  %link = load ptr, ptr %f
  store ptr %link, ptr @cancho_bare_free
  store ptr null, ptr %f
  ret ptr %f
fresh:
  %n0 = load i64, ptr @cancho_bare_next
  %unset = icmp eq i64 %n0, 0
  %first = ptrtoint ptr @__cancho_heap_start to i64
  %up = add i64 %first, 15
  %aligned = and i64 %up, -16
  %n = select i1 %unset, i64 %aligned, i64 %n0
  %e = add i64 %n, 65536
  %lim = ptrtoint ptr @__cancho_heap_end to i64
  %wrapped = icmp ult i64 %e, %n
  %over = icmp ugt i64 %e, %lim
  %bad = or i1 %wrapped, %over
  br i1 %bad, label %fail, label %ok
fail:
  call void @cancho_trap()
  unreachable
ok:
  store i64 %e, ptr @cancho_bare_next
  %p = inttoptr i64 %n to ptr
  ret ptr %p
}

; The chunk is cleared as far as it was used, and kept for the next region.
define internal void @cancho_region_give(ptr %base, ptr %end) {
entry:
  %b = ptrtoint ptr %base to i64
  %e = ptrtoint ptr %end to i64
  %used = sub i64 %e, %b
  call void @llvm.memset.p0.i64(ptr %base, i8 0, i64 %used, i1 false)
  %head = load ptr, ptr @cancho_bare_free
  store ptr %head, ptr %base
  store ptr %base, ptr @cancho_bare_free
  ret void
}

; One thread, so there is no key to make.
define internal void @cancho_region_init() {
entry:
  ret void
}

; What LLVM calls for a fill or a copy it does not write in line. "no-builtins" keeps each loop a loop:
; without it the loop of `memset` is recognised as a fill and becomes a call of `memset`.
define ptr @memset(ptr %d, i32 %c, i64 %n) noinline "no-builtins" {
entry:
  %v = trunc i32 %c to i8
  br label %head
head:
  %i = phi i64 [ 0, %entry ], [ %next, %body ]
  %more = icmp ult i64 %i, %n
  br i1 %more, label %body, label %done
body:
  %at = getelementptr i8, ptr %d, i64 %i
  store i8 %v, ptr %at
  %next = add i64 %i, 1
  br label %head
done:
  ret ptr %d
}

define ptr @memcpy(ptr %d, ptr %s, i64 %n) noinline "no-builtins" {
entry:
  br label %head
head:
  %i = phi i64 [ 0, %entry ], [ %next, %body ]
  %more = icmp ult i64 %i, %n
  br i1 %more, label %body, label %done
body:
  %from = getelementptr i8, ptr %s, i64 %i
  %b = load i8, ptr %from
  %to = getelementptr i8, ptr %d, i64 %i
  store i8 %b, ptr %to
  %next = add i64 %i, 1
  br label %head
done:
  ret ptr %d
}

; Forwards when the destination is below the source, backwards otherwise, so an overlap is copied right.
define ptr @memmove(ptr %d, ptr %s, i64 %n) noinline "no-builtins" {
entry:
  %da = ptrtoint ptr %d to i64
  %sa = ptrtoint ptr %s to i64
  %below = icmp ult i64 %da, %sa
  br i1 %below, label %fhead, label %bhead
fhead:
  %i = phi i64 [ 0, %entry ], [ %inext, %fbody ]
  %fmore = icmp ult i64 %i, %n
  br i1 %fmore, label %fbody, label %done
fbody:
  %ffrom = getelementptr i8, ptr %s, i64 %i
  %fb = load i8, ptr %ffrom
  %fto = getelementptr i8, ptr %d, i64 %i
  store i8 %fb, ptr %fto
  %inext = add i64 %i, 1
  br label %fhead
bhead:
  %j = phi i64 [ %n, %entry ], [ %jnext, %bbody ]
  %bmore = icmp ne i64 %j, 0
  br i1 %bmore, label %bbody, label %done
bbody:
  %jnext = sub i64 %j, 1
  %bfrom = getelementptr i8, ptr %s, i64 %jnext
  %bb = load i8, ptr %bfrom
  %bto = getelementptr i8, ptr %d, i64 %jnext
  store i8 %bb, ptr %bto
  br label %bhead
done:
  ret ptr %d
}

define ptr @memchr(ptr %s, i32 %c, i64 %n) noinline "no-builtins" {
entry:
  %v = trunc i32 %c to i8
  br label %head
head:
  %i = phi i64 [ 0, %entry ], [ %next, %step ]
  %more = icmp ult i64 %i, %n
  br i1 %more, label %body, label %none
body:
  %at = getelementptr i8, ptr %s, i64 %i
  %b = load i8, ptr %at
  %hit = icmp eq i8 %b, %v
  br i1 %hit, label %found, label %step
step:
  %next = add i64 %i, 1
  br label %head
found:
  ret ptr %at
none:
  ret ptr null
}
