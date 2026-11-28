
; bare.ll -- the runtime of a program with no system under it (`--target aarch64-none-elf`, docs/freestanding.md).
; Written by hand. No libc and no threads: the chunks of the regions come from the memory between two symbols the
; link provides, `__cancho_heap_start` and `__cancho_heap_end`; so do the blocks of the heap (`malloc`, `free`); and
; the four functions LLVM itself calls for a copy or a fill are defined here.
@__cancho_heap_start = external global i8
@__cancho_heap_end = external global i8
declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)

; What the allocator keeps is one record of 52 words: 0 where the next fresh memory begins (0: not yet begun), 1 the
; list of chunks given back, 2 to 49 the lists of the heap's blocks by size, 50 and 51 the two ends of the memory it
; hands out (0: the two symbols the link provides). It is reached through `@cancho_bare_arena`, which is null for the
; module's own record, `@cancho_bare_own`, so that a lent system can point it at the record of a program's block
; (cancho-os docs/programs.md, section 6) and nothing of one is ever in the other's lists.
@cancho_bare_own = internal global [52 x i64] zeroinitializer

; What a lent system and a block that runs keep (docs/bare-system.md, 11e), here since the trap and every function read them:
; whether something is lent (0 no, 1 yes, 2 the kernel is answering a program), the function that ends a program that traps
; (null when none runs), where the stack a function runs on ends (0 for the kernel's: nothing is below it), and why the last trap
; was taken (1 a check of the language, 2 no memory left, 3 no stack left).
@cancho_sys_on = global i64 0
@cancho_runs_hook = global ptr null
@cancho_stack_end = internal global i64 0
@cancho_sys_why = global i64 0

; The check every function makes first, after its frame is taken: the stack pointer (here, the address of a cell of this
; function's own frame) below the end is a trap, why 3. One call, so that a function LLVM writes into another does not bring a
; copy of the comparison.
define void @cancho_stack_check() noinline {
entry:
  %cell = alloca i64
  %sp = ptrtoint ptr %cell to i64
  %end = load i64, ptr @cancho_stack_end
  %deep = icmp ult i64 %sp, %end
  br i1 %deep, label %over, label %room
over:
  store i64 3, ptr @cancho_sys_why
  call void @cancho_trap()
  unreachable
room:
  ret void
}
@cancho_bare_arena = internal global ptr null

define internal ptr @cancho_bare_state() {
entry:
  %a = load ptr, ptr @cancho_bare_arena
  %own = icmp eq ptr %a, null
  %s = select i1 %own, ptr @cancho_bare_own, ptr %a
  ret ptr %s
}

; The next `bytes` (a multiple of 16) of the memory the link names; past its end is a trap, why 2.
define internal ptr @cancho_bare_bump(i64 %bytes) {
entry:
  %st = call ptr @cancho_bare_state()
  %n0 = load i64, ptr %st
  %unset = icmp eq i64 %n0, 0
  %startp = getelementptr i64, ptr %st, i64 50
  %start = load i64, ptr %startp
  %linked = ptrtoint ptr @__cancho_heap_start to i64
  %nostart = icmp eq i64 %start, 0
  %first = select i1 %nostart, i64 %linked, i64 %start
  %up = add i64 %first, 15
  %aligned = and i64 %up, -16
  %n = select i1 %unset, i64 %aligned, i64 %n0
  %e = add i64 %n, %bytes
  %endp = getelementptr i64, ptr %st, i64 51
  %end = load i64, ptr %endp
  %linkend = ptrtoint ptr @__cancho_heap_end to i64
  %noend = icmp eq i64 %end, 0
  %lim = select i1 %noend, i64 %linkend, i64 %end
  %wrapped = icmp ult i64 %e, %n
  %over = icmp ugt i64 %e, %lim
  %bad = or i1 %wrapped, %over
  br i1 %bad, label %fail, label %ok
fail:
  store i64 2, ptr @cancho_sys_why
  call void @cancho_trap()
  unreachable
ok:
  store i64 %e, ptr %st
  %p = inttoptr i64 %n to ptr
  ret ptr %p
}

; A chunk of 64 KiB: one given back earlier, or the next of the memory.
define internal ptr @cancho_region_take() {
entry:
  %st = call ptr @cancho_bare_state()
  %freep = getelementptr i64, ptr %st, i64 1
  %f = load ptr, ptr %freep
  %none = icmp eq ptr %f, null
  br i1 %none, label %fresh, label %pop
pop:
  %link = load ptr, ptr %f
  store ptr %link, ptr %freep
  store ptr null, ptr %f
  ret ptr %f
fresh:
  %p = call ptr @cancho_bare_bump(i64 65536)
  ret ptr %p
}

; ---- the heap (`box`, `box_slice`, and so `std.buffer` and what is made of it) --------------------------------
; A block is 32 bytes times a power of two: 16 of them are its header, which holds which power, and the rest is
; what the caller asked for. A block that is given back is kept in the list of its size and is the next one of that
; size to be handed out; it is never split and never joined to another. So at most half of a block is not what was
; asked for, and memory that has been one size stays that size. The lists are threaded through the blocks themselves.
declare i64 @llvm.ctlz.i64(i64, i1)

; `size` bytes, aligned to 16. No memory left is a trap, as it is for a region: there is nobody to tell.
define ptr @malloc(i64 %size) {
entry:
  %total = add i64 %size, 16
  %wrapped = icmp ult i64 %total, %size
  %huge = icmp ugt i64 %total, 1099511627776
  %bad = or i1 %wrapped, %huge
  br i1 %bad, label %fail, label %ok
fail:
  store i64 2, ptr @cancho_sys_why
  call void @cancho_trap()
  unreachable
ok:
  ; the least k with 32 << k >= total
  %m1 = sub i64 %total, 1
  %lz = call i64 @llvm.ctlz.i64(i64 %m1, i1 false)
  %bits = sub i64 64, %lz
  %small = icmp ult i64 %bits, 5
  %k0 = sub i64 %bits, 5
  %k = select i1 %small, i64 0, i64 %k0
  %st = call ptr @cancho_bare_state()
  %bin = add i64 %k, 2
  %slot = getelementptr i64, ptr %st, i64 %bin
  %head = load ptr, ptr %slot
  %none = icmp eq ptr %head, null
  br i1 %none, label %fresh, label %pop
pop:
  %user = getelementptr i8, ptr %head, i64 16
  %link = load ptr, ptr %user
  store ptr %link, ptr %slot
  ret ptr %user
fresh:
  %bytes = shl i64 32, %k
  %p = call ptr @cancho_bare_bump(i64 %bytes)
  store i64 %k, ptr %p
  %u = getelementptr i8, ptr %p, i64 16
  ret ptr %u
}

; The block goes to the list of its size. A header that names no size is a trap: the pointer was not `malloc`'s.
define void @free(ptr %user) {
entry:
  %null = icmp eq ptr %user, null
  br i1 %null, label %done, label %some
some:
  %base = getelementptr i8, ptr %user, i64 -16
  %k = load i64, ptr %base
  %bad = icmp ugt i64 %k, 47
  br i1 %bad, label %fail, label %ok
fail:
  call void @cancho_trap()
  unreachable
ok:
  %st = call ptr @cancho_bare_state()
  %bin = add i64 %k, 2
  %slot = getelementptr i64, ptr %st, i64 %bin
  %head = load ptr, ptr %slot
  store ptr %head, ptr %user
  store ptr %base, ptr %slot
  br label %done
done:
  ret void
}

; Zeroed memory: what a `static` is made in, once, before `main`, and what a `box_slice` filled with zeros is.
define ptr @calloc(i64 %count, i64 %size) {
entry:
  %m = call {i64, i1} @llvm.umul.with.overflow.i64(i64 %count, i64 %size)
  %bytes = extractvalue {i64, i1} %m, 0
  %carry = extractvalue {i64, i1} %m, 1
  br i1 %carry, label %fail, label %ok
fail:
  store i64 2, ptr @cancho_sys_why
  call void @cancho_trap()
  unreachable
ok:
  %p = call ptr @malloc(i64 %bytes)
  call void @llvm.memset.p0.i64(ptr %p, i8 0, i64 %bytes, i1 false)
  ret ptr %p
}
declare {i64, i1} @llvm.umul.with.overflow.i64(i64, i64)

; The chunk is cleared as far as it was used, and kept for the next region.
define internal void @cancho_region_give(ptr %base, ptr %end) {
entry:
  %b = ptrtoint ptr %base to i64
  %e = ptrtoint ptr %end to i64
  %used = sub i64 %e, %b
  call void @llvm.memset.p0.i64(ptr %base, i8 0, i64 %used, i1 false)
  %st = call ptr @cancho_bare_state()
  %freep = getelementptr i64, ptr %st, i64 1
  %head = load ptr, ptr %freep
  store ptr %head, ptr %base
  store ptr %base, ptr %freep
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

; What `arg` measures an argument with (`docs/bare-system.md`): the bytes before the first zero.
define i64 @strlen(ptr %s) noinline "no-builtins" {
entry:
  br label %head
head:
  %i = phi i64 [ 0, %entry ], [ %next, %step ]
  %at = getelementptr i8, ptr %s, i64 %i
  %b = load i8, ptr %at
  %end = icmp eq i8 %b, 0
  br i1 %end, label %done, label %step
step:
  %next = add i64 %i, 1
  br label %head
done:
  ret i64 %i
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

; Does the path lie under the prefix of the capability? The same text as runtime.ll's, for a lent system's files.
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

; Is `name[0..len]` one path component? Not empty, not longer than 255, not `.` or `..`, with no `/` and no zero in it
; (what `dirrt_component` of dirs.c holds a name to, for a system that is lent: docs/bare-system.md).
define i1 @cancho_sys_component(ptr %name, i64 %len) {
entry:
  %empty = icmp eq i64 %len, 0
  %long = icmp ugt i64 %len, 255
  %out = or i1 %empty, %long
  br i1 %out, label %no, label %first
first:
  %b0 = load i8, ptr %name
  %dot0 = icmp eq i8 %b0, 46
  %is1 = icmp eq i64 %len, 1
  %single = and i1 %dot0, %is1
  br i1 %single, label %no, label %second
second:
  %is2 = icmp eq i64 %len, 2
  %both = and i1 %dot0, %is2
  br i1 %both, label %two, label %loop
two:
  %a1 = getelementptr i8, ptr %name, i64 1
  %b1 = load i8, ptr %a1
  %dot1 = icmp eq i8 %b1, 46
  br i1 %dot1, label %no, label %loop
loop:
  %i = phi i64 [ 0, %second ], [ 0, %two ], [ %next, %cont ]
  %more = icmp ult i64 %i, %len
  br i1 %more, label %body, label %yes
body:
  %a = getelementptr i8, ptr %name, i64 %i
  %c = load i8, ptr %a
  %slash = icmp eq i8 %c, 47
  %zero = icmp eq i8 %c, 0
  %badc = or i1 %slash, %zero
  br i1 %badc, label %no, label %cont
cont:
  %next = add i64 %i, 1
  br label %loop
yes:
  ret i1 true
no:
  ret i1 false
}

; Is `name[0..len]` a host, and `port` a port, that the bound `bound[0..blen]` of a `Net` reaches? A trap if not, as the
; hosted runtime traps (`netrt_dial` and `netrt_checked_host` of net.c), for a system that is lent (docs/bare-system.md):
; a byte 1 in front of the bound says the host is matched exactly (edition 10), and `*.` there stands for one label;
; otherwise the host is a prefix of the name. What follows the bound's last `:` is its port, if it is not empty.
define void @cancho_sys_dial_check(ptr %name, i64 %len, i64 %port, ptr %b0, i64 %bl0) {
entry:
  %long = icmp uge i64 %len, 256
  br i1 %long, label %bad, label %lead
lead:
  %has = icmp sgt i64 %bl0, 0
  br i1 %has, label %peek, label %plain
peek:
  %c0 = load i8, ptr %b0
  %isone = icmp eq i8 %c0, 1
  br label %plain
plain:
  %exact = phi i1 [ false, %lead ], [ %isone, %peek ]
  %b1 = getelementptr i8, ptr %b0, i64 1
  %bound = select i1 %exact, ptr %b1, ptr %b0
  %bl1 = sub i64 %bl0, 1
  %blen = select i1 %exact, i64 %bl1, i64 %bl0
  br label %colon
colon:
  %ci = phi i64 [ %blen, %plain ], [ %cprev, %cnext ]
  %cany = icmp sgt i64 %ci, 0
  br i1 %cany, label %cbody, label %nocolon
cbody:
  %cprev = sub i64 %ci, 1
  %cp = getelementptr i8, ptr %bound, i64 %cprev
  %cc = load i8, ptr %cp
  %isc = icmp eq i8 %cc, 58
  br i1 %isc, label %pdig, label %cnext
cnext:
  br label %colon
pdig:
  %pi = phi i64 [ %ci, %cbody ], [ %pi1, %pstep ]
  %pv = phi i64 [ 0, %cbody ], [ %pv1, %pstep ]
  %pmore = icmp slt i64 %pi, %blen
  br i1 %pmore, label %pbody, label %pdone
pbody:
  %pp = getelementptr i8, ptr %bound, i64 %pi
  %pc = load i8, ptr %pp
  %lo = icmp ult i8 %pc, 48
  %hi = icmp ugt i8 %pc, 57
  %nd = or i1 %lo, %hi
  br i1 %nd, label %bad, label %pstep
pstep:
  %pcz = zext i8 %pc to i64
  %pd = sub i64 %pcz, 48
  %pv10 = mul i64 %pv, 10
  %pv1 = add i64 %pv10, %pd
  %pi1 = add i64 %pi, 1
  br label %pdig
pdone:
  %empty = icmp sge i64 %ci, %blen
  %same = icmp eq i64 %pv, %port
  %portok = or i1 %empty, %same
  br i1 %portok, label %host, label %bad
nocolon:
  br label %host
host:
  %hl = phi i64 [ %cprev, %pdone ], [ %blen, %nocolon ]
  %hasl = icmp sgt i64 %hl, 0
  %ex = and i1 %exact, %hasl
  br i1 %ex, label %exactly, label %prefix
prefix:
  %short = icmp slt i64 %len, %hl
  br i1 %short, label %bad, label %ploop
exactly:
  %two = icmp sge i64 %hl, 2
  br i1 %two, label %star, label %whole
star:
  %s0 = load i8, ptr %bound
  %s1p = getelementptr i8, ptr %bound, i64 1
  %s1 = load i8, ptr %s1p
  %isstar = icmp eq i8 %s0, 42
  %isdot = icmp eq i8 %s1, 46
  %wild = and i1 %isstar, %isdot
  br i1 %wild, label %wildcard, label %whole
whole:
  %samelen = icmp eq i64 %len, %hl
  br i1 %samelen, label %ploop, label %bad
ploop:
  %i = phi i64 [ 0, %prefix ], [ 0, %whole ], [ %i1, %pok ]
  %im = icmp slt i64 %i, %hl
  br i1 %im, label %pcmp, label %ok
pcmp:
  %na = getelementptr i8, ptr %name, i64 %i
  %nb = load i8, ptr %na
  %ba = getelementptr i8, ptr %bound, i64 %i
  %bb = load i8, ptr %ba
  %eq = icmp eq i8 %nb, %bb
  br i1 %eq, label %pok, label %bad
pok:
  %i1 = add i64 %i, 1
  br label %ploop
wildcard:
  %tl = sub i64 %hl, 1
  %fits = icmp sgt i64 %len, %tl
  br i1 %fits, label %wstart, label %bad
wstart:
  %le = sub i64 %len, %tl
  br label %wloop
wloop:
  %j = phi i64 [ 0, %wstart ], [ %j1, %wok ]
  %jm = icmp slt i64 %j, %len
  br i1 %jm, label %wbody, label %ok
wbody:
  %wa = getelementptr i8, ptr %name, i64 %j
  %wc = load i8, ptr %wa
  %inlabel = icmp slt i64 %j, %le
  br i1 %inlabel, label %wlabel, label %wtail
wlabel:
  %wdot = icmp eq i8 %wc, 46
  br i1 %wdot, label %bad, label %wok
wtail:
  %ti = sub i64 %j, %le
  %ti1 = add i64 %ti, 1
  %ta = getelementptr i8, ptr %bound, i64 %ti1
  %tc = load i8, ptr %ta
  %teq = icmp eq i8 %wc, %tc
  br i1 %teq, label %wok, label %bad
wok:
  %j1 = add i64 %j, 1
  br label %wloop
ok:
  ret void
bad:
  call void @cancho_trap()
  unreachable
}
