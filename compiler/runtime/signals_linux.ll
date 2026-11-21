
@sigrt_bit = internal constant [8 x i32] [i32 64, i32 1, i32 2, i32 4, i32 8, i32 16, i32 32, i32 128], align 4
@sigrt_number = internal constant [8 x i32] [i32 14, i32 1, i32 2, i32 3, i32 15, i32 10, i32 12, i32 28], align 4

; Function Attrs: noinline nounwind
define void @cancho_signals_watch(i64 %mask, ptr %out) {
entry:
  %mask.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %native = alloca i64, align 8
  %claimed = alloca i64, align 8
  %fd = alloca i32, align 4
  %set = alloca [16 x i64], align 8
  %e = alloca i32, align 4
  store i64 %mask, ptr %mask.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load i64, ptr %mask.addr, align 8
  %call = call i64 @sigrt_native(i64 %0)
  store i64 %call, ptr %native, align 8
  %1 = load i64, ptr @cancho_signal_state, align 8
  store i64 %1, ptr %claimed, align 8
  %2 = load i64, ptr %claimed, align 8
  %3 = load i64, ptr %native, align 8
  %and = and i64 %2, %3
  %cmp = icmp ne i64 %and, 0
  br i1 %cmp, label %if.then, label %lor.lhs.false

lor.lhs.false:                                    ; preds = %entry
  %4 = load i64, ptr getelementptr inbounds ([2 x i64], ptr @cancho_signal_state, i64 0, i64 1), align 8
  %cmp1 = icmp ne i64 %4, 0
  br i1 %cmp1, label %if.then, label %if.end

if.then:                                          ; preds = %lor.lhs.false, %entry
  %5 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %5, i64 0
  store i64 1, ptr %arrayidx, align 8
  %6 = load ptr, ptr %out.addr, align 8
  %arrayidx2 = getelementptr inbounds i64, ptr %6, i64 1
  store i64 0, ptr %arrayidx2, align 8
  %7 = load ptr, ptr %out.addr, align 8
  %arrayidx3 = getelementptr inbounds i64, ptr %7, i64 2
  store i64 16, ptr %arrayidx3, align 8
  br label %return

if.end:                                           ; preds = %lor.lhs.false
  %arraydecay = getelementptr inbounds [16 x i64], ptr %set, i64 0, i64 0
  %8 = load i64, ptr %native, align 8
  call void @sigrt_set(ptr %arraydecay, i64 %8)
  %arraydecay4 = getelementptr inbounds [16 x i64], ptr %set, i64 0, i64 0
  %call5 = call i32 @pthread_sigmask(i32 0, ptr %arraydecay4, ptr null)
  %arraydecay6 = getelementptr inbounds [16 x i64], ptr %set, i64 0, i64 0
  %call7 = call i32 @signalfd(i32 -1, ptr %arraydecay6, i32 526336)
  store i32 %call7, ptr %fd, align 4
  %call8 = call ptr @cancho_errno_location()
  %9 = load i32, ptr %call8, align 4
  store i32 %9, ptr %e, align 4
  %10 = load i32, ptr %fd, align 4
  %cmp9 = icmp slt i32 %10, 0
  br i1 %cmp9, label %if.then10, label %if.end16

if.then10:                                        ; preds = %if.end
  %arraydecay11 = getelementptr inbounds [16 x i64], ptr %set, i64 0, i64 0
  %call12 = call i32 @pthread_sigmask(i32 1, ptr %arraydecay11, ptr null)
  %11 = load ptr, ptr %out.addr, align 8
  %arrayidx13 = getelementptr inbounds i64, ptr %11, i64 0
  store i64 1, ptr %arrayidx13, align 8
  %12 = load ptr, ptr %out.addr, align 8
  %arrayidx14 = getelementptr inbounds i64, ptr %12, i64 1
  store i64 0, ptr %arrayidx14, align 8
  %13 = load i32, ptr %e, align 4
  %conv = sext i32 %13 to i64
  %14 = load ptr, ptr %out.addr, align 8
  %arrayidx15 = getelementptr inbounds i64, ptr %14, i64 2
  store i64 %conv, ptr %arrayidx15, align 8
  br label %return

if.end16:                                         ; preds = %if.end
  %15 = load i64, ptr %claimed, align 8
  %16 = load i64, ptr %native, align 8
  %or = or i64 %15, %16
  store i64 %or, ptr @cancho_signal_state, align 8
  %17 = load ptr, ptr %out.addr, align 8
  %arrayidx17 = getelementptr inbounds i64, ptr %17, i64 0
  store i64 0, ptr %arrayidx17, align 8
  %18 = load i64, ptr %native, align 8
  %shl = shl i64 %18, 32
  %19 = load i32, ptr %fd, align 4
  %conv18 = zext i32 %19 to i64
  %or19 = or i64 %shl, %conv18
  %20 = load ptr, ptr %out.addr, align 8
  %arrayidx20 = getelementptr inbounds i64, ptr %20, i64 1
  store i64 %or19, ptr %arrayidx20, align 8
  %21 = load ptr, ptr %out.addr, align 8
  %arrayidx21 = getelementptr inbounds i64, ptr %21, i64 2
  store i64 0, ptr %arrayidx21, align 8
  br label %return

return:                                           ; preds = %if.end16, %if.then10, %if.then
  ret void
}

; Function Attrs: noinline nounwind
define internal i64 @sigrt_native(i64 %mask) {
entry:
  %mask.addr = alloca i64, align 8
  %native = alloca i64, align 8
  %i = alloca i32, align 4
  store i64 %mask, ptr %mask.addr, align 8
  store i64 0, ptr %native, align 8
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %0 = load i32, ptr %i, align 4
  %cmp = icmp slt i32 %0, 8
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %1 = load i64, ptr %mask.addr, align 8
  %2 = load i32, ptr %i, align 4
  %idxprom = sext i32 %2 to i64
  %arrayidx = getelementptr inbounds [8 x i32], ptr @sigrt_bit, i64 0, i64 %idxprom
  %3 = load i32, ptr %arrayidx, align 4
  %conv = sext i32 %3 to i64
  %and = and i64 %1, %conv
  %tobool = icmp ne i64 %and, 0
  br i1 %tobool, label %if.then, label %if.end

if.then:                                          ; preds = %for.body
  %4 = load i32, ptr %i, align 4
  %idxprom1 = sext i32 %4 to i64
  %arrayidx2 = getelementptr inbounds [8 x i32], ptr @sigrt_number, i64 0, i64 %idxprom1
  %5 = load i32, ptr %arrayidx2, align 4
  %sub = sub nsw i32 %5, 1
  %sh_prom = zext i32 %sub to i64
  %shl = shl i64 1, %sh_prom
  %6 = load i64, ptr %native, align 8
  %or = or i64 %6, %shl
  store i64 %or, ptr %native, align 8
  br label %if.end

if.end:                                           ; preds = %if.then, %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end
  %7 = load i32, ptr %i, align 4
  %inc = add nsw i32 %7, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %8 = load i64, ptr %native, align 8
  ret i64 %8
}

; Function Attrs: noinline nounwind
define internal void @sigrt_set(ptr %set, i64 %native) {
entry:
  %set.addr = alloca ptr, align 8
  %native.addr = alloca i64, align 8
  %i = alloca i32, align 4
  store ptr %set, ptr %set.addr, align 8
  store i64 %native, ptr %native.addr, align 8
  store i32 1, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %0 = load i32, ptr %i, align 4
  %cmp = icmp slt i32 %0, 16
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %1 = load ptr, ptr %set.addr, align 8
  %2 = load i32, ptr %i, align 4
  %idxprom = sext i32 %2 to i64
  %arrayidx = getelementptr inbounds i64, ptr %1, i64 %idxprom
  store i64 0, ptr %arrayidx, align 8
  br label %for.inc

for.inc:                                          ; preds = %for.body
  %3 = load i32, ptr %i, align 4
  %inc = add nsw i32 %3, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %4 = load i64, ptr %native.addr, align 8
  %5 = load ptr, ptr %set.addr, align 8
  %arrayidx1 = getelementptr inbounds i64, ptr %5, i64 0
  store i64 %4, ptr %arrayidx1, align 8
  ret void
}

declare i32 @pthread_sigmask(i32, ptr, ptr)

declare i32 @signalfd(i32, ptr, i32)


; Function Attrs: noinline nounwind
define i64 @cancho_signals_pending(ptr %watch) {
entry:
  %watch.addr = alloca ptr, align 8
  store ptr %watch, ptr %watch.addr, align 8
  %0 = load ptr, ptr %watch.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %call = call i64 @sigrt_drain(i32 %conv)
  %call1 = call i64 @sigrt_to_bits(i64 %call)
  ret i64 %call1
}

; Function Attrs: noinline nounwind
define internal i64 @sigrt_to_bits(i64 %native) {
entry:
  %native.addr = alloca i64, align 8
  %out = alloca i64, align 8
  %i = alloca i32, align 4
  store i64 %native, ptr %native.addr, align 8
  store i64 0, ptr %out, align 8
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %0 = load i32, ptr %i, align 4
  %cmp = icmp slt i32 %0, 8
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %1 = load i64, ptr %native.addr, align 8
  %2 = load i32, ptr %i, align 4
  %idxprom = sext i32 %2 to i64
  %arrayidx = getelementptr inbounds [8 x i32], ptr @sigrt_number, i64 0, i64 %idxprom
  %3 = load i32, ptr %arrayidx, align 4
  %sub = sub nsw i32 %3, 1
  %sh_prom = zext i32 %sub to i64
  %shl = shl i64 1, %sh_prom
  %and = and i64 %1, %shl
  %tobool = icmp ne i64 %and, 0
  br i1 %tobool, label %if.then, label %if.end

if.then:                                          ; preds = %for.body
  %4 = load i32, ptr %i, align 4
  %idxprom1 = sext i32 %4 to i64
  %arrayidx2 = getelementptr inbounds [8 x i32], ptr @sigrt_bit, i64 0, i64 %idxprom1
  %5 = load i32, ptr %arrayidx2, align 4
  %conv = sext i32 %5 to i64
  %6 = load i64, ptr %out, align 8
  %or = or i64 %6, %conv
  store i64 %or, ptr %out, align 8
  br label %if.end

if.end:                                           ; preds = %if.then, %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end
  %7 = load i32, ptr %i, align 4
  %inc = add nsw i32 %7, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %8 = load i64, ptr %out, align 8
  ret i64 %8
}

; Function Attrs: noinline nounwind
define internal i64 @sigrt_drain(i32 %fd) {
entry:
  %fd.addr = alloca i32, align 4
  %seen = alloca i64, align 8
  %count = alloca i64, align 8
  %buf = alloca [8 x [32 x i32]], align 4
  %got = alloca i64, align 8
  %i = alloca i64, align 8
  %n = alloca i64, align 8
  store i32 %fd, ptr %fd.addr, align 4
  store i64 0, ptr %seen, align 8
  %0 = load i32, ptr %fd.addr, align 4
  %arraydecay = getelementptr inbounds [8 x [32 x i32]], ptr %buf, i64 0, i64 0
  %call = call i64 @read(i32 %0, ptr %arraydecay, i64 1024)
  store i64 %call, ptr %got, align 8
  %1 = load i64, ptr %got, align 8
  %div = sdiv i64 %1, 128
  store i64 %div, ptr %count, align 8
  store i64 0, ptr %i, align 8
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %2 = load i64, ptr %i, align 8
  %3 = load i64, ptr %count, align 8
  %cmp = icmp slt i64 %2, %3
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %4 = load i64, ptr %i, align 8
  %arrayidx = getelementptr inbounds [8 x [32 x i32]], ptr %buf, i64 0, i64 %4
  %arrayidx1 = getelementptr inbounds [32 x i32], ptr %arrayidx, i64 0, i64 0
  %5 = load i32, ptr %arrayidx1, align 4
  %conv = zext i32 %5 to i64
  store i64 %conv, ptr %n, align 8
  %6 = load i64, ptr %n, align 8
  %sub = sub i64 %6, 1
  %cmp2 = icmp ult i64 %sub, 31
  br i1 %cmp2, label %if.then, label %if.end

if.then:                                          ; preds = %for.body
  %7 = load i64, ptr %n, align 8
  %sub4 = sub i64 %7, 1
  %shl = shl i64 1, %sub4
  %8 = load i64, ptr %seen, align 8
  %or = or i64 %8, %shl
  store i64 %or, ptr %seen, align 8
  br label %if.end

if.end:                                           ; preds = %if.then, %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end
  %9 = load i64, ptr %i, align 8
  %inc = add nsw i64 %9, 1
  store i64 %inc, ptr %i, align 8
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %10 = load i64, ptr %seen, align 8
  ret i64 %10
}

; Function Attrs: noinline nounwind
define i64 @cancho_signals_close(i64 %handle) {
entry:
  %handle.addr = alloca i64, align 8
  %fd = alloca i32, align 4
  %native = alloca i64, align 8
  %queued = alloca i64, align 8
  %set = alloca [16 x i64], align 8
  %answer = alloca i32, align 4
  store i64 %handle, ptr %handle.addr, align 8
  %0 = load i64, ptr %handle.addr, align 8
  %conv = trunc i64 %0 to i32
  store i32 %conv, ptr %fd, align 4
  %1 = load i64, ptr %handle.addr, align 8
  %shr = lshr i64 %1, 32
  store i64 %shr, ptr %native, align 8
  store i64 0, ptr %queued, align 8
  %arraydecay = getelementptr inbounds [16 x i64], ptr %set, i64 0, i64 0
  %2 = load i64, ptr %native, align 8
  call void @sigrt_set(ptr %arraydecay, i64 %2)
  %arraydecay1 = getelementptr inbounds [16 x i64], ptr %set, i64 0, i64 0
  %call = call i32 @pthread_sigmask(i32 1, ptr %arraydecay1, ptr null)
  %3 = load i32, ptr %fd, align 4
  %call2 = call i32 @close(i32 %3)
  store i32 %call2, ptr %answer, align 4
  %4 = load i64, ptr @cancho_signal_state, align 8
  %5 = load i64, ptr %native, align 8
  %not = xor i64 %5, -1
  %and = and i64 %4, %not
  store i64 %and, ptr @cancho_signal_state, align 8
  %6 = load i32, ptr %answer, align 4
  %conv3 = sext i32 %6 to i64
  ret i64 %conv3
}





