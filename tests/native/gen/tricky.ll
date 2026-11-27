@fmt = private constant [6 x i8] c"%ld \0A\00"
@fmtd = private constant [5 x i8] c"%f \0A\00"
@cell = global i64 5
@word = global i32 7
declare i32 @printf(ptr, ...)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i64, i1 } @llvm.ssub.with.overflow.i64(i64, i64)
declare { i64, i1 } @llvm.smul.with.overflow.i64(i64, i64)
declare { i64, i1 } @llvm.uadd.with.overflow.i64(i64, i64)
declare { i64, i1 } @llvm.usub.with.overflow.i64(i64, i64)
declare { i64, i1 } @llvm.umul.with.overflow.i64(i64, i64)
declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)
declare i64 @llvm.ctpop.i64(i64)
declare i64 @llvm.ctlz.i64(i64, i1)
declare i64 @llvm.cttz.i64(i64, i1)
declare double @llvm.sqrt.f64(double)
declare float @llvm.sqrt.f32(float)
define void @show(i64 %v) {
entry:
  %r = call i32 (ptr, ...) @printf(ptr @fmt, i64 %v)
  ret void
}
define void @showd(double %v) {
entry:
  %r = call i32 (ptr, ...) @printf(ptr @fmtd, double %v)
  ret void
}
define i64 @swap(i64 %n) {
entry:
  br label %loop
loop:
  %a = phi i64 [ 1, %entry ], [ %b, %loop ]
  %b = phi i64 [ 2, %entry ], [ %a, %loop ]
  %i = phi i64 [ 0, %entry ], [ %j, %loop ]
  %j = add i64 %i, 1
  %d = icmp eq i64 %j, %n
  br i1 %d, label %done, label %loop
done:
  %x = mul i64 %a, 10
  %y = add i64 %x, %b
  ret i64 %y
}
define { i64, i64, i64 } @three(i64 %a, { ptr, i64 } %s, i64 %b) {
entry:
  %len = extractvalue { ptr, i64 } %s, 1
  %r0 = insertvalue { i64, i64, i64 } undef, i64 %a, 0
  %r1 = insertvalue { i64, i64, i64 } %r0, i64 %len, 1
  %r2 = insertvalue { i64, i64, i64 } %r1, i64 %b, 2
  ret { i64, i64, i64 } %r2
}
define { i64, i64, i64, i64, i64, i64, i64, i64, i64, i8 } @ten(i64 %a) {
entry:
  %b = add i64 %a, 1
  %r0 = insertvalue { i64, i64, i64, i64, i64, i64, i64, i64, i64, i8 } zeroinitializer, i64 %a, 0
  %r1 = insertvalue { i64, i64, i64, i64, i64, i64, i64, i64, i64, i8 } %r0, i64 %b, 8
  %r2 = insertvalue { i64, i64, i64, i64, i64, i64, i64, i64, i64, i8 } %r1, i8 9, 9
  ret { i64, i64, i64, i64, i64, i64, i64, i64, i64, i8 } %r2
}
define i64 @many(i64 %a, i64 %b, i64 %c, i64 %d, i64 %e, i64 %f, i64 %g, i64 %h, i64 %i, { i64, i64 } %p, i8 %q, double %x, i1 %t) {
entry:
  %p0 = extractvalue { i64, i64 } %p, 0
  %p1 = extractvalue { i64, i64 } %p, 1
  %s1 = add i64 %a, %h
  %s2 = add i64 %s1, %i
  %s3 = add i64 %s2, %p0
  %s4 = add i64 %s3, %p1
  %qq = zext i8 %q to i64
  %s5 = add i64 %s4, %qq
  %xx = fptosi double %x to i64
  %s6 = add i64 %s5, %xx
  %tt = zext i1 %t to i64
  %s7 = add i64 %s6, %tt
  ret i64 %s7
}
define i64 @big() {
entry:
  %small = alloca [5000 x i8]
  %huge = alloca [300000 x i64]
  call void @llvm.memset.p0.i64(ptr %small, i8 3, i64 5000, i1 false)
  %last = getelementptr [300000 x i64], ptr %huge, i64 0, i64 299999
  store i64 41, ptr %last
  %first = getelementptr i64, ptr %last, i64 -299999
  store i64 1, ptr %first
  %a = load i64, ptr %huge
  %b = load i64, ptr %last
  %p = getelementptr i8, ptr %small, i64 4999
  %c8 = load i8, ptr %p
  %c = zext i8 %c8 to i64
  %s = add i64 %a, %b
  %r = add i64 %s, %c
  ret i64 %r
}
define i64 @over(i64 %a, i64 %b) {
entry:
  %r1 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %a, i64 %b)
  %r2 = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %a, i64 %b)
  %r3 = call { i64, i1 } @llvm.smul.with.overflow.i64(i64 %a, i64 %b)
  %r4 = call { i64, i1 } @llvm.uadd.with.overflow.i64(i64 %a, i64 %b)
  %r5 = call { i64, i1 } @llvm.usub.with.overflow.i64(i64 %a, i64 %b)
  %r6 = call { i64, i1 } @llvm.umul.with.overflow.i64(i64 %a, i64 %b)
  %o1 = extractvalue { i64, i1 } %r1, 1
  %o2 = extractvalue { i64, i1 } %r2, 1
  %o3 = extractvalue { i64, i1 } %r3, 1
  %o4 = extractvalue { i64, i1 } %r4, 1
  %o5 = extractvalue { i64, i1 } %r5, 1
  %o6 = extractvalue { i64, i1 } %r6, 1
  %z1 = zext i1 %o1 to i64
  %z2 = zext i1 %o2 to i64
  %z3 = zext i1 %o3 to i64
  %z4 = zext i1 %o4 to i64
  %z5 = zext i1 %o5 to i64
  %z6 = zext i1 %o6 to i64
  %a2 = shl i64 %z2, 1
  %a3 = shl i64 %z3, 2
  %a4 = shl i64 %z4, 3
  %a5 = shl i64 %z5, 4
  %a6 = shl i64 %z6, 5
  %b1 = or i64 %z1, %a2
  %b2 = or i64 %b1, %a3
  %b3 = or i64 %b2, %a4
  %b4 = or i64 %b3, %a5
  %b5 = or i64 %b4, %a6
  %v3 = extractvalue { i64, i1 } %r3, 0
  %v5 = extractvalue { i64, i1 } %r5, 0
  %x = xor i64 %v3, %v5
  %m = shl i64 %x, 8
  %res = or i64 %m, %b5
  ret i64 %res
}
define i64 @narrow(i8 %a, i32 %b, i16 %c) {
entry:
  %sa = sext i8 %a to i64
  %sb = sext i32 %b to i64
  %sc = sext i16 %c to i64
  %d = sdiv i32 %b, 3
  %r = srem i32 %b, 7
  %sh = ashr i32 %b, 2
  %lt = icmp slt i8 %a, 0
  %ult = icmp ult i8 %a, 200
  %sel = select i1 %lt, i32 %d, i32 %r
  %sel2 = select i1 %ult, i64 %sa, i64 %sb
  %se = sext i32 %sel to i64
  %sh64 = sext i32 %sh to i64
  %t1 = add i64 %se, %sel2
  %t2 = add i64 %t1, %sh64
  %t3 = add i64 %t2, %sc
  %bit = trunc i64 %t3 to i1
  %sbit = sext i1 %bit to i64
  %t4 = add i64 %t3, %sbit
  %m = mul i8 %a, 3
  %zm = zext i8 %m to i64
  %t5 = add i64 %t4, %zm
  ret i64 %t5
}
define i64 @wide(i64 %a, i64 %b) {
entry:
  %wa = zext i64 %a to i128
  %wb = zext i64 %b to i128
  %p = mul i128 %wa, %wb
  %lo = trunc i128 %p to i64
  %top = lshr i128 %p, 64
  %hi = trunc i128 %top to i64
  %x = xor i64 %lo, %hi
  ret i64 %x
}
define i64 @atoms() {
entry:
  %old = atomicrmw add ptr @cell, i64 10 seq_cst
  %r = cmpxchg ptr @cell, i64 15, i64 100 seq_cst seq_cst
  %ok = extractvalue { i64, i1 } %r, 1
  %r2 = cmpxchg ptr @word, i32 8, i32 9 seq_cst seq_cst
  %ok2 = extractvalue { i32, i1 } %r2, 1
  %was = extractvalue { i32, i1 } %r2, 0
  %now = load atomic i64, ptr @cell acquire, align 8
  store atomic i32 3, ptr @word release, align 4
  %w = load i32, ptr @word
  %z1 = zext i1 %ok to i64
  %z2 = zext i1 %ok2 to i64
  %zw = zext i32 %was to i64
  %zz = zext i32 %w to i64
  %s1 = add i64 %old, %now
  %s2 = mul i64 %s1, 10
  %s3 = add i64 %s2, %z1
  %s4 = mul i64 %s3, 10
  %s5 = add i64 %s4, %z2
  %s6 = mul i64 %s5, 10
  %s7 = add i64 %s6, %zw
  %s8 = mul i64 %s7, 10
  %s9 = add i64 %s8, %zz
  ret i64 %s9
}
define double @floats(double %x, i64 %n) {
entry:
  %f = sitofp i64 %n to double
  %g = fmul double %x, 0x3FE0000000000000
  %h = fadd double %g, %f
  %s = call double @llvm.sqrt.f64(double %h)
  %c = fcmp olt double %s, %x
  %u = fcmp uno double %s, %x
  %one = fcmp one double %s, %x
  %ueq = fcmp ueq double %s, %s
  %k = select i1 %c, double %s, double %h
  %small = fptrunc double %k to float
  %twice = fadd float %small, %small
  %wide = fpext float %twice to double
  %neg = fneg double %wide
  %bits = bitcast double %neg to i64
  %back = bitcast i64 %bits to double
  %d = fdiv double %back, 0x4008000000000000
  %e = fsub double %d, 0x3FF0000000000000
  %i1 = zext i1 %u to i64
  %i2 = zext i1 %one to i64
  %i3 = zext i1 %ueq to i64
  %sum = add i64 %i1, %i2
  %sum2 = add i64 %sum, %i3
  %fs = sitofp i64 %sum2 to double
  %res = fadd double %e, %fs
  ret double %res
}
define i64 @pick(i64 %k) {
entry:
  switch i64 %k, label %other [
    i64 0, label %zero
    i64 7, label %seven
    i64 -1, label %minus
  ]
zero:
  br label %end
seven:
  br label %end
minus:
  br label %end
other:
  br label %end
end:
  %v = phi i64 [ 100, %zero ], [ 700, %seven ], [ 900, %minus ], [ 1, %other ]
  %s = phi { i64, i64 } [ { i64 1, i64 2 }, %zero ], [ zeroinitializer, %seven ], [ { i64 3, i64 4 }, %minus ], [ { i64 5, i64 6 }, %other ]
  %s1 = extractvalue { i64, i64 } %s, 1
  %c = icmp ugt i64 %v, 500
  %t = select i1 %c, { i64, i64 } %s, { i64, i64 } { i64 50, i64 60 }
  %t0 = extractvalue { i64, i64 } %t, 0
  %r = add i64 %v, %s1
  %r2 = add i64 %r, %t0
  ret i64 %r2
}
define i64 @rest(i64 %a, i64 %b) {
entry:
  %m = and i64 %a, 255
  %d = sub i64 %b, %m
  %q = udiv i64 -1, %a
  %l = lshr i64 %q, 40
  %p = inttoptr i64 %d to ptr
  %g = getelementptr i8, ptr %p, i64 3
  %i = ptrtoint ptr %g to i64
  %f = sitofp i64 %l to float
  %r = call float @llvm.sqrt.f32(float %f)
  %x = fptosi float %r to i64
  %s = add i64 %i, %x
  %v = load volatile i64, ptr @cell, align 8
  store volatile i32 1, ptr @word, align 4
  %t = add i64 %s, %v
  %z = icmp eq i64 %a, 0
  br i1 %z, label %never, label %out
never:
  unreachable
out:
  ret i64 %t
}
define i32 @main(i32 %argc, ptr %argv) {
entry:
  %a = call i64 @swap(i64 5)
  call void @show(i64 %a)
  %b = call i64 @swap(i64 6)
  call void @show(i64 %b)
  %t = call { i64, i64, i64 } @three(i64 11, { ptr, i64 } { ptr @fmt, i64 22 }, i64 33)
  %t0 = extractvalue { i64, i64, i64 } %t, 0
  %t1 = extractvalue { i64, i64, i64 } %t, 1
  %t2 = extractvalue { i64, i64, i64 } %t, 2
  call void @show(i64 %t0)
  call void @show(i64 %t1)
  call void @show(i64 %t2)
  %u = call { i64, i64, i64, i64, i64, i64, i64, i64, i64, i8 } @ten(i64 70)
  %u0 = extractvalue { i64, i64, i64, i64, i64, i64, i64, i64, i64, i8 } %u, 0
  %u8 = extractvalue { i64, i64, i64, i64, i64, i64, i64, i64, i64, i8 } %u, 8
  %u9 = extractvalue { i64, i64, i64, i64, i64, i64, i64, i64, i64, i8 } %u, 9
  %u9z = zext i8 %u9 to i64
  call void @show(i64 %u0)
  call void @show(i64 %u8)
  call void @show(i64 %u9z)
  %m = call i64 @many(i64 1, i64 2, i64 3, i64 4, i64 5, i64 6, i64 7, i64 8, i64 90, { i64, i64 } { i64 100, i64 2000 }, i8 255, double 0x4045000000000000, i1 true)
  call void @show(i64 %m)
  %g = call i64 @big()
  call void @show(i64 %g)
  %o1 = call i64 @over(i64 9223372036854775807, i64 1)
  call void @show(i64 %o1)
  %o2 = call i64 @over(i64 -9223372036854775808, i64 -1)
  call void @show(i64 %o2)
  %o3 = call i64 @over(i64 3, i64 5)
  call void @show(i64 %o3)
  %o4 = call i64 @over(i64 4294967296, i64 4294967296)
  call void @show(i64 %o4)
  %n1 = call i64 @narrow(i8 -5, i32 -100, i16 -3)
  call void @show(i64 %n1)
  %n2 = call i64 @narrow(i8 100, i32 2147483647, i16 77)
  call void @show(i64 %n2)
  %w = call i64 @wide(i64 -1, i64 -1)
  call void @show(i64 %w)
  %w2 = call i64 @wide(i64 123456789012345, i64 987654321098765)
  call void @show(i64 %w2)
  %at = call i64 @atoms()
  call void @show(i64 %at)
  %fl = call double @floats(double 0x4059000000000000, i64 3)
  call void @showd(double %fl)
  %p0 = call i64 @pick(i64 0)
  call void @show(i64 %p0)
  %p7 = call i64 @pick(i64 7)
  call void @show(i64 %p7)
  %pm = call i64 @pick(i64 -1)
  call void @show(i64 %pm)
  %px = call i64 @pick(i64 12)
  call void @show(i64 %px)
  %rs = call i64 @rest(i64 1000, i64 77)
  call void @show(i64 %rs)
  %c1 = call i64 @llvm.ctpop.i64(i64 -81985529216486896)
  call void @show(i64 %c1)
  %c2 = call i64 @llvm.ctlz.i64(i64 65536, i1 false)
  call void @show(i64 %c2)
  %c3 = call i64 @llvm.cttz.i64(i64 65536, i1 false)
  call void @show(i64 %c3)
  ret i32 0
}
