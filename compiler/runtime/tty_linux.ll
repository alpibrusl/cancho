
@ttyrt_rates = internal constant [30 x [2 x i64]] [[2 x i64] [i64 50, i64 1], [2 x i64] [i64 75, i64 2], [2 x i64] [i64 110, i64 3], [2 x i64] [i64 134, i64 4], [2 x i64] [i64 150, i64 5], [2 x i64] [i64 200, i64 6], [2 x i64] [i64 300, i64 7], [2 x i64] [i64 600, i64 8], [2 x i64] [i64 1200, i64 9], [2 x i64] [i64 1800, i64 10], [2 x i64] [i64 2400, i64 11], [2 x i64] [i64 4800, i64 12], [2 x i64] [i64 9600, i64 13], [2 x i64] [i64 19200, i64 14], [2 x i64] [i64 38400, i64 15], [2 x i64] [i64 57600, i64 4097], [2 x i64] [i64 115200, i64 4098], [2 x i64] [i64 230400, i64 4099], [2 x i64] [i64 460800, i64 4100], [2 x i64] [i64 500000, i64 4101], [2 x i64] [i64 576000, i64 4102], [2 x i64] [i64 921600, i64 4103], [2 x i64] [i64 1000000, i64 4104], [2 x i64] [i64 1152000, i64 4105], [2 x i64] [i64 1500000, i64 4106], [2 x i64] [i64 2000000, i64 4107], [2 x i64] [i64 2500000, i64 4108], [2 x i64] [i64 3000000, i64 4109], [2 x i64] [i64 3500000, i64 4110], [2 x i64] [i64 4000000, i64 4111]], align 8

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
  %call1 = call i32 (i32, ptr, i32, ...) @openat(i32 -100, ptr %4, i32 2306, i32 0)
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
  %i = alloca i32, align 4
  %t = alloca [15 x i32], align 4
  %i12 = alloca i32, align 4
  %b = alloca ptr, align 8
  store ptr %port, ptr %port.addr, align 8
  store i64 %speed, ptr %speed.addr, align 8
  %0 = load ptr, ptr %port.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  store i32 %conv, ptr %fd, align 4
  store i64 0, ptr %bits, align 8
  store i64 -1, ptr %bits, align 8
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %2 = load i32, ptr %i, align 4
  %cmp = icmp slt i32 %2, 30
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %3 = load i32, ptr %i, align 4
  %idxprom = sext i32 %3 to i64
  %arrayidx = getelementptr inbounds [30 x [2 x i64]], ptr @ttyrt_rates, i64 0, i64 %idxprom
  %arrayidx2 = getelementptr inbounds [2 x i64], ptr %arrayidx, i64 0, i64 0
  %4 = load i64, ptr %arrayidx2, align 8
  %5 = load i64, ptr %speed.addr, align 8
  %cmp3 = icmp eq i64 %4, %5
  br i1 %cmp3, label %if.then, label %if.end

if.then:                                          ; preds = %for.body
  %6 = load i32, ptr %i, align 4
  %idxprom5 = sext i32 %6 to i64
  %arrayidx6 = getelementptr inbounds [30 x [2 x i64]], ptr @ttyrt_rates, i64 0, i64 %idxprom5
  %arrayidx7 = getelementptr inbounds [2 x i64], ptr %arrayidx6, i64 0, i64 1
  %7 = load i64, ptr %arrayidx7, align 8
  store i64 %7, ptr %bits, align 8
  br label %if.end

if.end:                                           ; preds = %if.then, %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end
  %8 = load i32, ptr %i, align 4
  %inc = add nsw i32 %8, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %9 = load i64, ptr %bits, align 8
  %cmp8 = icmp slt i64 %9, 0
  br i1 %cmp8, label %if.then10, label %if.end11

if.then10:                                        ; preds = %for.end
  store i64 22, ptr %retval, align 8
  br label %return

if.end11:                                         ; preds = %for.end
  store i32 0, ptr %i12, align 4
  br label %for.cond13

for.cond13:                                       ; preds = %for.inc19, %if.end11
  %10 = load i32, ptr %i12, align 4
  %cmp14 = icmp slt i32 %10, 15
  br i1 %cmp14, label %for.body16, label %for.end21

for.body16:                                       ; preds = %for.cond13
  %11 = load i32, ptr %i12, align 4
  %idxprom17 = sext i32 %11 to i64
  %arrayidx18 = getelementptr inbounds [15 x i32], ptr %t, i64 0, i64 %idxprom17
  store i32 0, ptr %arrayidx18, align 4
  br label %for.inc19

for.inc19:                                        ; preds = %for.body16
  %12 = load i32, ptr %i12, align 4
  %inc20 = add nsw i32 %12, 1
  store i32 %inc20, ptr %i12, align 4
  br label %for.cond13

for.end21:                                        ; preds = %for.cond13
  %arraydecay = getelementptr inbounds [15 x i32], ptr %t, i64 0, i64 0
  store ptr %arraydecay, ptr %b, align 8
  %13 = load i64, ptr %bits, align 8
  %or = or i64 %13, 2224
  %conv22 = trunc i64 %or to i32
  %14 = load ptr, ptr %b, align 8
  %add.ptr = getelementptr inbounds i8, ptr %14, i64 8
  store i32 %conv22, ptr %add.ptr, align 4
  %15 = load i32, ptr %fd, align 4
  %arraydecay23 = getelementptr inbounds [15 x i32], ptr %t, i64 0, i64 0
  %call = call i32 @tcsetattr(i32 %15, i32 0, ptr %arraydecay23)
  %cmp24 = icmp slt i32 %call, 0
  br i1 %cmp24, label %if.then26, label %if.end29

if.then26:                                        ; preds = %for.end21
  %call27 = call ptr @cancho_errno_location()
  %16 = load i32, ptr %call27, align 4
  %conv28 = sext i32 %16 to i64
  store i64 %conv28, ptr %retval, align 8
  br label %return

if.end29:                                         ; preds = %for.end21
  store i64 0, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end29, %if.then26, %if.then10
  %17 = load i64, ptr %retval, align 8
  ret i64 %17
}

declare i32 @tcsetattr(i32, i32, ptr)

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
  %call = call i32 @tcflush(i32 %conv, i32 0)
  %conv1 = sext i32 %call to i64
  ret i64 %conv1
}

declare i32 @tcflush(i32, i32)



