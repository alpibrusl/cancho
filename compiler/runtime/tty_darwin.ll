
; Function Attrs: noinline nounwind
define void @cancho_tty_open(ptr %src, i64 %len, ptr %prefix, i64 %plen, ptr %out) {
entry:
  %src.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %prefix.addr = alloca ptr, align 8
  %plen.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %path = alloca ptr, align 8
  %fd = alloca i32, align 4
  %e = alloca i32, align 4
  store ptr %src, ptr %src.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store ptr %prefix, ptr %prefix.addr, align 8
  store i64 %plen, ptr %plen.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %src.addr, align 8
  %1 = load i64, ptr %len.addr, align 8
  %2 = load ptr, ptr %prefix.addr, align 8
  %3 = load i64, ptr %plen.addr, align 8
  %call = call ptr @cancho_checked_path(ptr %0, i64 %1, ptr %2, i64 %3)
  store ptr %call, ptr %path, align 8
  %4 = load ptr, ptr %path, align 8
  %call1 = call i32 (i32, ptr, i32, ...) @openat(i32 -2, ptr %4, i32 131078, i32 0)
  store i32 %call1, ptr %fd, align 4
  %call2 = call ptr @cancho_errno_location()
  %5 = load i32, ptr %call2, align 4
  store i32 %5, ptr %e, align 4
  %6 = load ptr, ptr %path, align 8
  call void @free(ptr %6)
  %7 = load i32, ptr %fd, align 4
  %cmp = icmp slt i32 %7, 0
  %conv = zext i1 %cmp to i32
  %conv3 = sext i32 %conv to i64
  %8 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %8, i64 0
  store i64 %conv3, ptr %arrayidx, align 8
  %9 = load i32, ptr %fd, align 4
  %conv4 = sext i32 %9 to i64
  %10 = load ptr, ptr %out.addr, align 8
  %arrayidx5 = getelementptr inbounds i64, ptr %10, i64 1
  store i64 %conv4, ptr %arrayidx5, align 8
  %11 = load i32, ptr %e, align 4
  %conv6 = sext i32 %11 to i64
  %12 = load ptr, ptr %out.addr, align 8
  %arrayidx7 = getelementptr inbounds i64, ptr %12, i64 2
  store i64 %conv6, ptr %arrayidx7, align 8
  ret void
}





; Function Attrs: noinline nounwind
define i64 @cancho_tty_configure(ptr %port, i64 %speed) {
entry:
  %retval = alloca i64, align 8
  %port.addr = alloca ptr, align 8
  %speed.addr = alloca i64, align 8
  %fd = alloca i32, align 4
  %bits = alloca i64, align 8
  %t = alloca [9 x i64], align 8
  %i = alloca i32, align 4
  %b = alloca ptr, align 8
  %cell = alloca i64, align 8
  store ptr %port, ptr %port.addr, align 8
  store i64 %speed, ptr %speed.addr, align 8
  %0 = load ptr, ptr %port.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  store i32 %conv, ptr %fd, align 4
  store i64 0, ptr %bits, align 8
  %2 = load i64, ptr %speed.addr, align 8
  %sub = sub nsw i64 %2, 1
  %cmp = icmp uge i64 %sub, 2147483646
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  store i64 22, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %entry
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %if.end
  %3 = load i32, ptr %i, align 4
  %cmp2 = icmp slt i32 %3, 9
  br i1 %cmp2, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %4 = load i32, ptr %i, align 4
  %idxprom = sext i32 %4 to i64
  %arrayidx = getelementptr inbounds [9 x i64], ptr %t, i64 0, i64 %idxprom
  store i64 0, ptr %arrayidx, align 8
  br label %for.inc

for.inc:                                          ; preds = %for.body
  %5 = load i32, ptr %i, align 4
  %inc = add nsw i32 %5, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %arraydecay = getelementptr inbounds [9 x i64], ptr %t, i64 0, i64 0
  store ptr %arraydecay, ptr %b, align 8
  %6 = load i64, ptr %bits, align 8
  %or = or i64 %6, 35584
  %7 = load ptr, ptr %b, align 8
  %add.ptr = getelementptr inbounds i8, ptr %7, i64 16
  store i64 %or, ptr %add.ptr, align 8
  %8 = load ptr, ptr %b, align 8
  %add.ptr4 = getelementptr inbounds i8, ptr %8, i64 56
  store i64 115200, ptr %add.ptr4, align 8
  %9 = load ptr, ptr %b, align 8
  %add.ptr5 = getelementptr inbounds i8, ptr %9, i64 64
  store i64 115200, ptr %add.ptr5, align 8
  %10 = load i32, ptr %fd, align 4
  %arraydecay6 = getelementptr inbounds [9 x i64], ptr %t, i64 0, i64 0
  %call = call i32 @tcsetattr(i32 %10, i32 0, ptr %arraydecay6)
  %cmp7 = icmp slt i32 %call, 0
  br i1 %cmp7, label %if.then9, label %if.end12

if.then9:                                         ; preds = %for.end
  %call10 = call ptr @cancho_errno_location()
  %11 = load i32, ptr %call10, align 4
  %conv11 = sext i32 %11 to i64
  store i64 %conv11, ptr %retval, align 8
  br label %return

if.end12:                                         ; preds = %for.end
  %12 = load i64, ptr %speed.addr, align 8
  store i64 %12, ptr %cell, align 8
  %13 = load i32, ptr %fd, align 4
  %call13 = call i32 (i32, i64, ...) @ioctl(i32 %13, i64 2148029442, ptr %cell)
  %cmp14 = icmp slt i32 %call13, 0
  br i1 %cmp14, label %if.then16, label %if.end19

if.then16:                                        ; preds = %if.end12
  %call17 = call ptr @cancho_errno_location()
  %14 = load i32, ptr %call17, align 4
  %conv18 = sext i32 %14 to i64
  store i64 %conv18, ptr %retval, align 8
  br label %return

if.end19:                                         ; preds = %if.end12
  store i64 0, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end19, %if.then16, %if.then9, %if.then
  %15 = load i64, ptr %retval, align 8
  ret i64 %15
}

declare i32 @tcsetattr(i32, i32, ptr)

declare i32 @ioctl(i32, i64, ...)

; Function Attrs: noinline nounwind
define i64 @cancho_tty_read(ptr %port, ptr %p, i64 %n) {
entry:
  %port.addr = alloca ptr, align 8
  %p.addr = alloca ptr, align 8
  %n.addr = alloca i64, align 8
  store ptr %port, ptr %port.addr, align 8
  store ptr %p, ptr %p.addr, align 8
  store i64 %n, ptr %n.addr, align 8
  %0 = load ptr, ptr %port.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %2 = load ptr, ptr %p.addr, align 8
  %3 = load i64, ptr %n.addr, align 8
  %call = call i64 @read(i32 %conv, ptr %2, i64 %3)
  ret i64 %call
}


; Function Attrs: noinline nounwind
define i64 @cancho_tty_write(ptr %port, ptr %p, i64 %n) {
entry:
  %port.addr = alloca ptr, align 8
  %p.addr = alloca ptr, align 8
  %n.addr = alloca i64, align 8
  store ptr %port, ptr %port.addr, align 8
  store ptr %p, ptr %p.addr, align 8
  store i64 %n, ptr %n.addr, align 8
  %0 = load ptr, ptr %port.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %2 = load ptr, ptr %p.addr, align 8
  %3 = load i64, ptr %n.addr, align 8
  %call = call i64 @write(i32 %conv, ptr %2, i64 %3)
  ret i64 %call
}


; Function Attrs: noinline nounwind
define i64 @cancho_tty_flush(ptr %port) {
entry:
  %port.addr = alloca ptr, align 8
  store ptr %port, ptr %port.addr, align 8
  %0 = load ptr, ptr %port.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %call = call i32 @tcflush(i32 %conv, i32 1)
  %conv1 = sext i32 %call to i64
  ret i64 %conv1
}

declare i32 @tcflush(i32, i32)



