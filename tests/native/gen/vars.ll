@fmt = private constant [6 x i8] c"%ld \0A\00"
@cell = global i64 41
declare i32 @printf(ptr, ...)
define void @show(i64 %v) {
entry:
  %r = call i32 (ptr, ...) @printf(ptr @fmt, i64 %v)
  ret void
}
define void @poke(ptr %p, i64 %v) {
entry:
  store i64 %v, ptr %p
  ret void
}
define i64 @fib(i64 %n) {
entry:
  %a = alloca i64
  %b = alloca i64
  %t = alloca i64
  %i = alloca i64
  store i64 0, ptr %a
  store i64 1, ptr %b
  store i64 0, ptr %i
  br label %head
head:
  %iv = load i64, ptr %i
  %c = icmp slt i64 %iv, %n
  br i1 %c, label %body, label %done
body:
  %av = load i64, ptr %a
  store i64 %av, ptr %t
  %bv = load i64, ptr %b
  store i64 %bv, ptr %a
  %tv = load i64, ptr %t
  %s = add i64 %tv, %bv
  store i64 %s, ptr %b
  %i2 = add i64 %iv, 1
  store i64 %i2, ptr %i
  br label %head
done:
  %ra = load i64, ptr %a
  %rb = load i64, ptr %b
  %r1 = mul i64 %ra, 1000
  %r = add i64 %r1, %rb
  ret i64 %r
}
define i64 @diamond(i1 %c, i64 %x) {
entry:
  %r = alloca i64
  br i1 %c, label %then, label %else
then:
  %d = mul i64 %x, 2
  store i64 %d, ptr %r
  br label %join
else:
  %e = add i64 %x, 100
  store i64 %e, ptr %r
  br label %join
join:
  %v = load i64, ptr %r
  ret i64 %v
}
define i64 @nested(i64 %n) {
entry:
  %s = alloca i64
  %i = alloca i64
  %j = alloca i64
  store i64 0, ptr %s
  store i64 0, ptr %i
  br label %outer
outer:
  %iv = load i64, ptr %i
  %c = icmp slt i64 %iv, %n
  br i1 %c, label %init, label %done
init:
  store i64 0, ptr %j
  br label %inner
inner:
  %jv = load i64, ptr %j
  %d = icmp slt i64 %jv, %iv
  br i1 %d, label %body, label %next
body:
  %sv = load i64, ptr %s
  %p = mul i64 %iv, %jv
  %s2 = add i64 %sv, %p
  store i64 %s2, ptr %s
  %j2 = add i64 %jv, 1
  store i64 %j2, ptr %j
  br label %inner
next:
  %i2 = add i64 %iv, 1
  store i64 %i2, ptr %i
  br label %outer
done:
  %r = load i64, ptr %s
  ret i64 %r
}
define i64 @floats(i64 %n) {
entry:
  %acc = alloca double
  %f = alloca float
  %k = alloca i64
  store double 0x3FF8000000000000, ptr %acc
  store float 0x3FF0000000000000, ptr %f
  store i64 0, ptr %k
  br label %head
head:
  %kv = load i64, ptr %k
  %c = icmp slt i64 %kv, %n
  br i1 %c, label %body, label %done
body:
  %av = load double, ptr %acc
  %a2 = fadd double %av, 0x3FE0000000000000
  store double %a2, ptr %acc
  %fv = load float, ptr %f
  %f2 = fmul float %fv, 0x4000000000000000
  store float %f2, ptr %f
  %k2 = add i64 %kv, 1
  store i64 %k2, ptr %k
  br label %head
done:
  %ra = load double, ptr %acc
  %rf = load float, ptr %f
  %rd = fpext float %rf to double
  %sum = fadd double %ra, %rd
  %r = fptosi double %sum to i64
  ret i64 %r
}
define i64 @narrow(i64 %k) {
entry:
  %b = alloca i8
  %flag = alloca i1
  %h = alloca i16
  store i8 -1, ptr %b
  store i1 false, ptr %flag
  store i16 1000, ptr %h
  switch i64 %k, label %other [
    i64 1, label %one
    i64 2, label %two
  ]
one:
  store i8 7, ptr %b
  store i1 true, ptr %flag
  br label %join
two:
  %hv = load i16, ptr %h
  %h2 = mul i16 %hv, 70
  store i16 %h2, ptr %h
  br label %join
other:
  br label %join
join:
  %bv = load i8, ptr %b
  %fv = load i1, ptr %flag
  %hw = load i16, ptr %h
  %b64 = zext i8 %bv to i64
  %f64 = zext i1 %fv to i64
  %h64 = zext i16 %hw to i64
  %x = mul i64 %f64, 100000
  %y = add i64 %b64, %x
  %z = mul i64 %h64, 1000
  %r = add i64 %y, %z
  ret i64 %r
}
define i64 @escape(i64 %n) {
entry:
  %e = alloca i64
  %v = alloca i64
  %q = alloca ptr
  store i64 %n, ptr %v
  store ptr @cell, ptr %q
  call void @poke(ptr %e, i64 5)
  %ev = load i64, ptr %e
  %vv = load i64, ptr %v
  %qv = load ptr, ptr %q
  %g = load i64, ptr %qv
  %s = add i64 %ev, %vv
  %r = add i64 %s, %g
  ret i64 %r
}
define i64 @mixed(i64 %n) {
entry:
  %s = alloca i64
  %last = alloca i64
  store i64 0, ptr %s
  store i64 -1, ptr %last
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %i2, %step ]
  %odd = and i64 %i, 1
  %isodd = icmp eq i64 %odd, 1
  br i1 %isodd, label %keep, label %step
keep:
  store i64 %i, ptr %last
  %sv = load i64, ptr %s
  %s2 = add i64 %sv, %i
  store i64 %s2, ptr %s
  br label %step
step:
  %i2 = add i64 %i, 1
  %c = icmp slt i64 %i2, %n
  br i1 %c, label %loop, label %done
dead:
  store i64 99, ptr %s
  br label %done
done:
  %rs = load i64, ptr %s
  %rl = load i64, ptr %last
  %m = mul i64 %rl, 10000
  %r = add i64 %m, %rs
  ret i64 %r
}
define i32 @main(i32 %argc, ptr %argv) {
entry:
  %f0 = call i64 @fib(i64 0)
  call void @show(i64 %f0)
  %f10 = call i64 @fib(i64 10)
  call void @show(i64 %f10)
  %d1 = call i64 @diamond(i1 true, i64 21)
  call void @show(i64 %d1)
  %d2 = call i64 @diamond(i1 false, i64 21)
  call void @show(i64 %d2)
  %n5 = call i64 @nested(i64 5)
  call void @show(i64 %n5)
  %n0 = call i64 @nested(i64 0)
  call void @show(i64 %n0)
  %fl = call i64 @floats(i64 10)
  call void @show(i64 %fl)
  %w1 = call i64 @narrow(i64 1)
  call void @show(i64 %w1)
  %w2 = call i64 @narrow(i64 2)
  call void @show(i64 %w2)
  %w3 = call i64 @narrow(i64 3)
  call void @show(i64 %w3)
  %e = call i64 @escape(i64 1000)
  call void @show(i64 %e)
  %m7 = call i64 @mixed(i64 7)
  call void @show(i64 %m7)
  %m1 = call i64 @mixed(i64 1)
  call void @show(i64 %m1)
  ret i32 0
}
