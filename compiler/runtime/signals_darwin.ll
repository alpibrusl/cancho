
%struct.sigrt_event = type { i64, i16, i16, i32, i64, ptr }
%struct.sigrt_action = type { i64, i32, i32 }
%struct.sigrt_time = type { i64, i64 }

@sigrt_bit = internal constant [8 x i32] [i32 64, i32 1, i32 2, i32 4, i32 8, i32 16, i32 32, i32 128], align 4
@sigrt_number = internal constant [8 x i32] [i32 14, i32 1, i32 2, i32 3, i32 15, i32 30, i32 31, i32 28], align 4

; Function Attrs: noinline nounwind
define void @cancho_signals_watch(i64 %mask, ptr %out) {
entry:
  %mask.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %native = alloca i64, align 8
  %claimed = alloca i64, align 8
  %fd = alloca i32, align 4
  %e = alloca i32, align 4
  %n = alloca i32, align 4
  %ev = alloca %struct.sigrt_event, align 8
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
  %8 = load i64, ptr %native, align 8
  call void @sigrt_disposition(i64 %8, i64 1)
  %call4 = call i32 @kqueue()
  store i32 %call4, ptr %fd, align 4
  %call5 = call ptr @cancho_errno_location()
  %9 = load i32, ptr %call5, align 4
  store i32 %9, ptr %e, align 4
  %10 = load i32, ptr %fd, align 4
  %cmp6 = icmp sge i32 %10, 0
  br i1 %cmp6, label %if.then7, label %if.end9

if.then7:                                         ; preds = %if.end
  %11 = load i32, ptr %fd, align 4
  %call8 = call i32 (i32, i32, ...) @fcntl(i32 %11, i32 2, i32 1)
  br label %if.end9

if.end9:                                          ; preds = %if.then7, %if.end
  %12 = load i32, ptr %fd, align 4
  %cmp10 = icmp slt i32 %12, 0
  br i1 %cmp10, label %if.then11, label %if.end15

if.then11:                                        ; preds = %if.end9
  %13 = load i64, ptr %native, align 8
  call void @sigrt_disposition(i64 %13, i64 0)
  %14 = load ptr, ptr %out.addr, align 8
  %arrayidx12 = getelementptr inbounds i64, ptr %14, i64 0
  store i64 1, ptr %arrayidx12, align 8
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx13 = getelementptr inbounds i64, ptr %15, i64 1
  store i64 0, ptr %arrayidx13, align 8
  %16 = load i32, ptr %e, align 4
  %conv = sext i32 %16 to i64
  %17 = load ptr, ptr %out.addr, align 8
  %arrayidx14 = getelementptr inbounds i64, ptr %17, i64 2
  store i64 %conv, ptr %arrayidx14, align 8
  br label %return

if.end15:                                         ; preds = %if.end9
  store i32 1, ptr %n, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %if.end15
  %18 = load i32, ptr %n, align 4
  %cmp16 = icmp sle i32 %18, 31
  br i1 %cmp16, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %19 = load i64, ptr %native, align 8
  %20 = load i32, ptr %n, align 4
  %sub = sub nsw i32 %20, 1
  %sh_prom = zext i32 %sub to i64
  %shr = ashr i64 %19, %sh_prom
  %and18 = and i64 %shr, 1
  %tobool = icmp ne i64 %and18, 0
  br i1 %tobool, label %if.then19, label %if.end22

if.then19:                                        ; preds = %for.body
  %ident = getelementptr inbounds nuw %struct.sigrt_event, ptr %ev, i32 0, i32 0
  %21 = load i32, ptr %n, align 4
  %conv20 = sext i32 %21 to i64
  store i64 %conv20, ptr %ident, align 8
  %filter = getelementptr inbounds nuw %struct.sigrt_event, ptr %ev, i32 0, i32 1
  store i16 -6, ptr %filter, align 8
  %flags = getelementptr inbounds nuw %struct.sigrt_event, ptr %ev, i32 0, i32 2
  store i16 33, ptr %flags, align 2
  %fflags = getelementptr inbounds nuw %struct.sigrt_event, ptr %ev, i32 0, i32 3
  store i32 0, ptr %fflags, align 4
  %data = getelementptr inbounds nuw %struct.sigrt_event, ptr %ev, i32 0, i32 4
  store i64 0, ptr %data, align 8
  %udata = getelementptr inbounds nuw %struct.sigrt_event, ptr %ev, i32 0, i32 5
  store ptr null, ptr %udata, align 8
  %22 = load i32, ptr %fd, align 4
  %call21 = call i32 @kevent(i32 %22, ptr %ev, i32 1, ptr null, i32 0, ptr null)
  br label %if.end22

if.end22:                                         ; preds = %if.then19, %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end22
  %23 = load i32, ptr %n, align 4
  %inc = add nsw i32 %23, 1
  store i32 %inc, ptr %n, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %24 = load i64, ptr %claimed, align 8
  %25 = load i64, ptr %native, align 8
  %or = or i64 %24, %25
  store i64 %or, ptr @cancho_signal_state, align 8
  %26 = load ptr, ptr %out.addr, align 8
  %arrayidx23 = getelementptr inbounds i64, ptr %26, i64 0
  store i64 0, ptr %arrayidx23, align 8
  %27 = load i64, ptr %native, align 8
  %shl = shl i64 %27, 32
  %28 = load i32, ptr %fd, align 4
  %conv24 = zext i32 %28 to i64
  %or25 = or i64 %shl, %conv24
  %29 = load ptr, ptr %out.addr, align 8
  %arrayidx26 = getelementptr inbounds i64, ptr %29, i64 1
  store i64 %or25, ptr %arrayidx26, align 8
  %30 = load ptr, ptr %out.addr, align 8
  %arrayidx27 = getelementptr inbounds i64, ptr %30, i64 2
  store i64 0, ptr %arrayidx27, align 8
  br label %return

return:                                           ; preds = %for.end, %if.then11, %if.then
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
define internal void @sigrt_disposition(i64 %native, i64 %handler) {
entry:
  %native.addr = alloca i64, align 8
  %handler.addr = alloca i64, align 8
  %n = alloca i32, align 4
  %act = alloca %struct.sigrt_action, align 8
  store i64 %native, ptr %native.addr, align 8
  store i64 %handler, ptr %handler.addr, align 8
  store i32 1, ptr %n, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %0 = load i32, ptr %n, align 4
  %cmp = icmp sle i32 %0, 31
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %1 = load i64, ptr %native.addr, align 8
  %2 = load i32, ptr %n, align 4
  %sub = sub nsw i32 %2, 1
  %sh_prom = zext i32 %sub to i64
  %shr = ashr i64 %1, %sh_prom
  %and = and i64 %shr, 1
  %tobool = icmp ne i64 %and, 0
  br i1 %tobool, label %if.then, label %if.end

if.then:                                          ; preds = %for.body
  %handler1 = getelementptr inbounds nuw %struct.sigrt_action, ptr %act, i32 0, i32 0
  %3 = load i64, ptr %handler.addr, align 8
  store i64 %3, ptr %handler1, align 8
  %mask = getelementptr inbounds nuw %struct.sigrt_action, ptr %act, i32 0, i32 1
  store i32 0, ptr %mask, align 8
  %flags = getelementptr inbounds nuw %struct.sigrt_action, ptr %act, i32 0, i32 2
  store i32 0, ptr %flags, align 4
  %4 = load i32, ptr %n, align 4
  %call = call i32 @sigaction(i32 %4, ptr %act, ptr null)
  br label %if.end

if.end:                                           ; preds = %if.then, %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end
  %5 = load i32, ptr %n, align 4
  %inc = add nsw i32 %5, 1
  store i32 %inc, ptr %n, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  ret void
}





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
  %buf = alloca [8 x %struct.sigrt_event], align 8
  %none = alloca %struct.sigrt_time, align 8
  %i = alloca i64, align 8
  %n = alloca i64, align 8
  store i32 %fd, ptr %fd.addr, align 4
  store i64 0, ptr %seen, align 8
  call void @llvm.memset.p0.i64(ptr align 8 %none, i8 0, i64 16, i1 false)
  %0 = load i32, ptr %fd.addr, align 4
  %arraydecay = getelementptr inbounds [8 x %struct.sigrt_event], ptr %buf, i64 0, i64 0
  %call = call i32 @kevent(i32 %0, ptr null, i32 0, ptr %arraydecay, i32 8, ptr %none)
  %conv = sext i32 %call to i64
  store i64 %conv, ptr %count, align 8
  store i64 0, ptr %i, align 8
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %1 = load i64, ptr %i, align 8
  %2 = load i64, ptr %count, align 8
  %cmp = icmp slt i64 %1, %2
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %3 = load i64, ptr %i, align 8
  %arrayidx = getelementptr inbounds [8 x %struct.sigrt_event], ptr %buf, i64 0, i64 %3
  %ident = getelementptr inbounds nuw %struct.sigrt_event, ptr %arrayidx, i32 0, i32 0
  %4 = load i64, ptr %ident, align 8
  store i64 %4, ptr %n, align 8
  %5 = load i64, ptr %n, align 8
  %sub = sub i64 %5, 1
  %cmp2 = icmp ult i64 %sub, 31
  br i1 %cmp2, label %if.then, label %if.end

if.then:                                          ; preds = %for.body
  %6 = load i64, ptr %n, align 8
  %sub4 = sub i64 %6, 1
  %shl = shl i64 1, %sub4
  %7 = load i64, ptr %seen, align 8
  %or = or i64 %7, %shl
  store i64 %or, ptr %seen, align 8
  br label %if.end

if.end:                                           ; preds = %if.then, %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end
  %8 = load i64, ptr %i, align 8
  %inc = add nsw i64 %8, 1
  store i64 %inc, ptr %i, align 8
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %9 = load i64, ptr %seen, align 8
  ret i64 %9
}

; Function Attrs: noinline nounwind
define i64 @cancho_signals_close(i64 %handle) {
entry:
  %handle.addr = alloca i64, align 8
  %fd = alloca i32, align 4
  %native = alloca i64, align 8
  %queued = alloca i64, align 8
  %answer = alloca i32, align 4
  %n = alloca i32, align 4
  store i64 %handle, ptr %handle.addr, align 8
  %0 = load i64, ptr %handle.addr, align 8
  %conv = trunc i64 %0 to i32
  store i32 %conv, ptr %fd, align 4
  %1 = load i64, ptr %handle.addr, align 8
  %shr = lshr i64 %1, 32
  store i64 %shr, ptr %native, align 8
  store i64 0, ptr %queued, align 8
  %2 = load i32, ptr %fd, align 4
  %call = call i64 @sigrt_drain(i32 %2)
  store i64 %call, ptr %queued, align 8
  %3 = load i64, ptr %native, align 8
  call void @sigrt_disposition(i64 %3, i64 0)
  %4 = load i32, ptr %fd, align 4
  %call1 = call i32 @close(i32 %4)
  store i32 %call1, ptr %answer, align 4
  store i32 1, ptr %n, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %5 = load i32, ptr %n, align 4
  %cmp = icmp sle i32 %5, 31
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %6 = load i64, ptr %queued, align 8
  %7 = load i32, ptr %n, align 4
  %sub = sub nsw i32 %7, 1
  %sh_prom = zext i32 %sub to i64
  %shr3 = ashr i64 %6, %sh_prom
  %and = and i64 %shr3, 1
  %tobool = icmp ne i64 %and, 0
  br i1 %tobool, label %if.then, label %if.end

if.then:                                          ; preds = %for.body
  %8 = load i32, ptr %n, align 4
  %call4 = call i32 @raise(i32 %8)
  br label %if.end

if.end:                                           ; preds = %if.then, %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end
  %9 = load i32, ptr %n, align 4
  %inc = add nsw i32 %9, 1
  store i32 %inc, ptr %n, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %10 = load i64, ptr @cancho_signal_state, align 8
  %11 = load i64, ptr %native, align 8
  %not = xor i64 %11, -1
  %and5 = and i64 %10, %not
  store i64 %and5, ptr @cancho_signal_state, align 8
  %12 = load i32, ptr %answer, align 4
  %conv6 = sext i32 %12 to i64
  ret i64 %conv6
}


declare i32 @raise(i32)

declare i32 @sigaction(i32, ptr, ptr)

; Function Attrs: nocallback nofree nounwind willreturn memory(argmem: write)



