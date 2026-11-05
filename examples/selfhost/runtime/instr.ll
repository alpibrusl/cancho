; cancho_i: the support code of an instrumented build (`docs/agent-debug.md` section 11). Sizes are in @cancho_i_dims: statements, functions, mode (0 trace, 1 profile), 0.
declare i32 @dprintf(i32, ptr, ...)
declare ptr @getenv(ptr)
declare i32 @creat(ptr, i32)
declare ptr @signal(i32, ptr)
declare i32 @atexit(ptr)
declare void @_exit(i32) noreturn

@cancho_i_depth = thread_local global i64 0
@cancho_i_fid = thread_local global [4097 x i32] zeroinitializer
@cancho_i_sid = thread_local global [4097 x i32] zeroinitializer
@cancho_i_cur = thread_local global i32 0
@cancho_i_kind = thread_local global i64 0
@cancho_i_out = global i32 -1
@cancho_i_hits = global i64 0
@cancho_i_env = private constant [17 x i8] c"CANCHO_TRACE_OUT\00"
@cancho_i_w_breakpoint = private constant [11 x i8] c"breakpoint\00"
@cancho_i_w_trap = private constant [5 x i8] c"trap\00"
@cancho_i_f_stop = private constant [28 x i8] c"stop\09%s\0Asignal\09%d\0Akind\09%ld\0A\00"
@cancho_i_f_hits = private constant [10 x i8] c"hits\09%ld\0A\00"
@cancho_i_f_frame = private constant [16 x i8] c"frame\09%s\09%s\09%d\0A\00"
@cancho_i_f_fn = private constant [25 x i8] c"fn\09%s\09%s\09%d\09%ld\09%ld\09%ld\0A\00"

define void @cancho_note(i64 %k) {
entry:
  store i64 %k, ptr @cancho_i_kind
  ret void
}

define void @cancho_i_enter(i32 %f) {
entry:
  %d0 = load i64, ptr @cancho_i_depth
  %d = add i64 %d0, 1
  store i64 %d, ptr @cancho_i_depth
  %ok = icmp ult i64 %d, 4096
  br i1 %ok, label %set, label %out
set:
  %pf = getelementptr i32, ptr @cancho_i_fid, i64 %d
  store i32 %f, ptr %pf
  %ps = getelementptr i32, ptr @cancho_i_sid, i64 %d
  store i32 0, ptr %ps
  br label %out
out:
  ret void
}

define void @cancho_i_leave() {
entry:
  %d0 = load i64, ptr @cancho_i_depth
  %pos = icmp sgt i64 %d0, 0
  br i1 %pos, label %dec, label %out
dec:
  %d = sub i64 %d0, 1
  store i64 %d, ptr @cancho_i_depth
  br label %out
out:
  ret void
}

define void @cancho_i_alloc_note() {
entry:
  %cur = load i32, ptr @cancho_i_cur
  %some = icmp ne i32 %cur, 0
  br i1 %some, label %add, label %out
add:
  %i = sub i32 %cur, 1
  %i64 = zext i32 %i to i64
  %p = getelementptr i64, ptr @cancho_i_alloc, i64 %i64
  %v = load i64, ptr %p
  %v1 = add i64 %v, 1
  store i64 %v1, ptr %p
  br label %out
out:
  ret void
}

define void @cancho_i_report(i32 %why, i32 %sig) {
entry:
  %fd = load i32, ptr @cancho_i_out
  %open = icmp sge i32 %fd, 0
  br i1 %open, label %go, label %done
go:
  %isbp = icmp eq i32 %why, 0
  %word = select i1 %isbp, ptr @cancho_i_w_breakpoint, ptr @cancho_i_w_trap
  %kind = load i64, ptr @cancho_i_kind
  %r1 = call i32 (i32, ptr, ...) @dprintf(i32 %fd, ptr @cancho_i_f_stop, ptr %word, i32 %sig, i64 %kind)
  %hits = load i64, ptr @cancho_i_hits
  %r2 = call i32 (i32, ptr, ...) @dprintf(i32 %fd, ptr @cancho_i_f_hits, i64 %hits)
  %depth0 = load i64, ptr @cancho_i_depth
  %big = icmp sgt i64 %depth0, 4095
  %depth = select i1 %big, i64 4095, i64 %depth0
  br label %loop
loop:
  %d = phi i64 [ %depth, %go ], [ %dn, %next ]
  %more = icmp sge i64 %d, 1
  br i1 %more, label %frame, label %done
frame:
  %pf = getelementptr i32, ptr @cancho_i_fid, i64 %d
  %f = load i32, ptr %pf
  %f64 = zext i32 %f to i64
  %pn = getelementptr ptr, ptr @cancho_i_f_name, i64 %f64
  %name = load ptr, ptr %pn
  %pfi = getelementptr ptr, ptr @cancho_i_f_file, i64 %f64
  %file = load ptr, ptr %pfi
  %ps = getelementptr i32, ptr @cancho_i_sid, i64 %d
  %s1 = load i32, ptr %ps
  %none = icmp eq i32 %s1, 0
  br i1 %none, label %fnline, label %stline
fnline:
  %pl = getelementptr i32, ptr @cancho_i_f_line, i64 %f64
  %l1 = load i32, ptr %pl
  br label %print
stline:
  %s = sub i32 %s1, 1
  %s64 = zext i32 %s to i64
  %psl = getelementptr i32, ptr @cancho_i_s_line, i64 %s64
  %l2 = load i32, ptr %psl
  br label %print
print:
  %line = phi i32 [ %l1, %fnline ], [ %l2, %stline ]
  %r3 = call i32 (i32, ptr, ...) @dprintf(i32 %fd, ptr @cancho_i_f_frame, ptr %name, ptr %file, i32 %line)
  br label %next
next:
  %dn = sub i64 %d, 1
  br label %loop
done:
  ret void
}

define void @cancho_i_hit(i32 %s) {
entry:
  %d = load i64, ptr @cancho_i_depth
  %inb = icmp ult i64 %d, 4096
  br i1 %inb, label %rec, label %cnt
rec:
  %p = getelementptr i32, ptr @cancho_i_sid, i64 %d
  %s1 = add i32 %s, 1
  store i32 %s1, ptr %p
  br label %cnt
cnt:
  %s1b = add i32 %s, 1
  store i32 %s1b, ptr @cancho_i_cur
  %s64 = zext i32 %s to i64
  %pc = getelementptr i64, ptr @cancho_i_count, i64 %s64
  %c = load i64, ptr %pc
  %c1 = add i64 %c, 1
  store i64 %c1, ptr %pc
  %mode = load i64, ptr getelementptr (i64, ptr @cancho_i_dims, i64 2)
  %tracing = icmp eq i64 %mode, 0
  br i1 %tracing, label %test, label %out
test:
  %pfl = getelementptr i32, ptr @cancho_i_s_file, i64 %s64
  %fl = load i32, ptr %pfl
  %pln = getelementptr i32, ptr @cancho_i_s_line, i64 %s64
  %ln = load i32, ptr %pln
  %fl64 = zext i32 %fl to i64
  %ln64 = zext i32 %ln to i64
  %bf = load i64, ptr @cancho_i_bp
  %bl = load i64, ptr getelementptr (i64, ptr @cancho_i_bp, i64 1)
  %bn = load i64, ptr getelementptr (i64, ptr @cancho_i_bp, i64 2)
  %armed = icmp sgt i64 %bn, 0
  %samef = icmp eq i64 %fl64, %bf
  %samel = icmp eq i64 %ln64, %bl
  %m1 = and i1 %samef, %samel
  %m = and i1 %m1, %armed
  br i1 %m, label %match, label %out
match:
  %h = load i64, ptr @cancho_i_hits
  %h1 = add i64 %h, 1
  store i64 %h1, ptr @cancho_i_hits
  %reached = icmp eq i64 %h1, %bn
  br i1 %reached, label %stop, label %out
stop:
  call void @cancho_i_report(i32 0, i32 0)
  call void @_exit(i32 0)
  unreachable
out:
  ret void
}

define void @cancho_i_on_signal(i32 %sig) {
entry:
  call void @cancho_i_report(i32 1, i32 %sig)
  call void @_exit(i32 1)
  unreachable
}

define void @cancho_i_dump() {
entry:
  %fd = load i32, ptr @cancho_i_out
  %open = icmp sge i32 %fd, 0
  br i1 %open, label %sum, label %done
sum:
  %ns = load i64, ptr @cancho_i_dims
  br label %sloop
sloop:
  %s = phi i64 [ 0, %sum ], [ %sn, %sadd ]
  %smore = icmp slt i64 %s, %ns
  br i1 %smore, label %sadd, label %fstart
sadd:
  %pfn = getelementptr i32, ptr @cancho_i_s_fn, i64 %s
  %fn32 = load i32, ptr %pfn
  %fn = zext i32 %fn32 to i64
  %pc = getelementptr i64, ptr @cancho_i_count, i64 %s
  %c = load i64, ptr %pc
  %pw = getelementptr i32, ptr @cancho_i_s_w, i64 %s
  %w32 = load i32, ptr %pw
  %w = zext i32 %w32 to i64
  %cw = mul i64 %c, %w
  %pfs = getelementptr i64, ptr @cancho_i_fs, i64 %fn
  %vs = load i64, ptr %pfs
  %vs1 = add i64 %vs, %c
  store i64 %vs1, ptr %pfs
  %pfi = getelementptr i64, ptr @cancho_i_fi, i64 %fn
  %vi = load i64, ptr %pfi
  %vi1 = add i64 %vi, %cw
  store i64 %vi1, ptr %pfi
  %pa = getelementptr i64, ptr @cancho_i_alloc, i64 %s
  %a = load i64, ptr %pa
  %pfa = getelementptr i64, ptr @cancho_i_fa, i64 %fn
  %va = load i64, ptr %pfa
  %va1 = add i64 %va, %a
  store i64 %va1, ptr %pfa
  %sn = add i64 %s, 1
  br label %sloop
fstart:
  %nf = load i64, ptr getelementptr (i64, ptr @cancho_i_dims, i64 1)
  br label %floop
floop:
  %f = phi i64 [ 0, %fstart ], [ %fnx, %fnext ]
  %fmore = icmp slt i64 %f, %nf
  br i1 %fmore, label %fbody, label %done
fbody:
  %pst = getelementptr i64, ptr @cancho_i_fs, i64 %f
  %st = load i64, ptr %pst
  %pal = getelementptr i64, ptr @cancho_i_fa, i64 %f
  %al = load i64, ptr %pal
  %any = or i64 %st, %al
  %nz = icmp ne i64 %any, 0
  br i1 %nz, label %print, label %fnext
print:
  %pin = getelementptr ptr, ptr @cancho_i_f_name, i64 %f
  %name = load ptr, ptr %pin
  %pfl = getelementptr ptr, ptr @cancho_i_f_file, i64 %f
  %file = load ptr, ptr %pfl
  %pln = getelementptr i32, ptr @cancho_i_f_line, i64 %f
  %line = load i32, ptr %pln
  %pir = getelementptr i64, ptr @cancho_i_fi, i64 %f
  %ir = load i64, ptr %pir
  %r = call i32 (i32, ptr, ...) @dprintf(i32 %fd, ptr @cancho_i_f_fn, ptr %name, ptr %file, i32 %line, i64 %ir, i64 %st, i64 %al)
  br label %fnext
fnext:
  %fnx = add i64 %f, 1
  br label %floop
done:
  ret void
}

define internal void @cancho_i_init() {
entry:
  %path = call ptr @getenv(ptr @cancho_i_env)
  %none = icmp eq ptr %path, null
  br i1 %none, label %out, label %open
open:
  %fd = call i32 @creat(ptr %path, i32 384)
  store i32 %fd, ptr @cancho_i_out
  %a = call ptr @signal(i32 4, ptr @cancho_i_on_signal)
  %b = call ptr @signal(i32 5, ptr @cancho_i_on_signal)
  %c = call ptr @signal(i32 8, ptr @cancho_i_on_signal)
  %d = call ptr @signal(i32 11, ptr @cancho_i_on_signal)
  %mode = load i64, ptr getelementptr (i64, ptr @cancho_i_dims, i64 2)
  %profiling = icmp eq i64 %mode, 1
  br i1 %profiling, label %reg, label %out
reg:
  %e = call i32 @atexit(ptr @cancho_i_dump)
  br label %out
out:
  ret void
}

@llvm.global_ctors = appending global [1 x { i32, ptr, ptr }] [{ i32, ptr, ptr } { i32 65535, ptr @cancho_i_init, ptr null }]
