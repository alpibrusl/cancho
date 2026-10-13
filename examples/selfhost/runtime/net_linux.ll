
; Function Attrs: noinline nounwind
define i64 @cancho_net_bind(i64 %port, ptr %bound, i64 %bound_len) {
entry:
  %retval = alloca i64, align 8
  %port.addr = alloca i64, align 8
  %bound.addr = alloca ptr, align 8
  %bound_len.addr = alloca i64, align 8
  %a = alloca [16 x i8], align 1
  %fd = alloca i32, align 4
  store i64 %port, ptr %port.addr, align 8
  store ptr %bound, ptr %bound.addr, align 8
  store i64 %bound_len, ptr %bound_len.addr, align 8
  %0 = load i64, ptr %port.addr, align 8
  %1 = load ptr, ptr %bound.addr, align 8
  %2 = load i64, ptr %bound_len.addr, align 8
  %call = call i64 @netrt_port_of(ptr %1, i64 0, i64 %2)
  call void @netrt_check_port(i64 %0, i64 %call)
  %arraydecay = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 0
  %3 = load i64, ptr %port.addr, align 8
  call void @netrt_sockaddr_any(ptr %arraydecay, i64 %3)
  %call1 = call i32 @netrt_tcp_socket(i32 1)
  store i32 %call1, ptr %fd, align 4
  %4 = load i32, ptr %fd, align 4
  %cmp = icmp slt i32 %4, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  store i64 -1, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %entry
  %5 = load i32, ptr %fd, align 4
  call void @netrt_int_option(i32 %5, i32 1, i32 2)
  %6 = load i32, ptr %fd, align 4
  %arraydecay2 = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 0
  %call3 = call i32 @bind(i32 %6, ptr %arraydecay2, i32 16)
  %cmp4 = icmp ne i32 %call3, 0
  br i1 %cmp4, label %if.then5, label %if.end7

if.then5:                                         ; preds = %if.end
  %7 = load i32, ptr %fd, align 4
  %call6 = call i32 @close(i32 %7)
  store i64 -1, ptr %retval, align 8
  br label %return

if.end7:                                          ; preds = %if.end
  %8 = load i32, ptr %fd, align 4
  %conv = sext i32 %8 to i64
  store i64 %conv, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end7, %if.then5, %if.then
  %9 = load i64, ptr %retval, align 8
  ret i64 %9
}

; Function Attrs: noinline nounwind
define internal void @netrt_check_port(i64 %port, i64 %bound) {
entry:
  %port.addr = alloca i64, align 8
  %bound.addr = alloca i64, align 8
  store i64 %port, ptr %port.addr, align 8
  store i64 %bound, ptr %bound.addr, align 8
  %0 = load i64, ptr %bound.addr, align 8
  %cmp = icmp ne i64 %0, -2
  br i1 %cmp, label %land.lhs.true, label %if.end

land.lhs.true:                                    ; preds = %entry
  %1 = load i64, ptr %port.addr, align 8
  %2 = load i64, ptr %bound.addr, align 8
  %cmp1 = icmp ne i64 %1, %2
  br i1 %cmp1, label %if.then, label %if.end

if.then:                                          ; preds = %land.lhs.true
  call void @llvm.trap()
  br label %if.end

if.end:                                           ; preds = %if.then, %land.lhs.true, %entry
  ret void
}

; Function Attrs: noinline nounwind
define internal i64 @netrt_port_of(ptr %text, i64 %from, i64 %to) {
entry:
  %retval = alloca i64, align 8
  %text.addr = alloca ptr, align 8
  %from.addr = alloca i64, align 8
  %to.addr = alloca i64, align 8
  %v = alloca i64, align 8
  %i = alloca i64, align 8
  store ptr %text, ptr %text.addr, align 8
  store i64 %from, ptr %from.addr, align 8
  store i64 %to, ptr %to.addr, align 8
  %0 = load i64, ptr %to.addr, align 8
  %1 = load i64, ptr %from.addr, align 8
  %cmp = icmp sle i64 %0, %1
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  store i64 -2, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %entry
  store i64 0, ptr %v, align 8
  %2 = load i64, ptr %from.addr, align 8
  store i64 %2, ptr %i, align 8
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %if.end
  %3 = load i64, ptr %i, align 8
  %4 = load i64, ptr %to.addr, align 8
  %cmp1 = icmp slt i64 %3, %4
  br i1 %cmp1, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %5 = load ptr, ptr %text.addr, align 8
  %6 = load i64, ptr %i, align 8
  %arrayidx = getelementptr inbounds i8, ptr %5, i64 %6
  %7 = load i8, ptr %arrayidx, align 1
  %conv = zext i8 %7 to i32
  %cmp2 = icmp slt i32 %conv, 48
  br i1 %cmp2, label %if.then8, label %lor.lhs.false

lor.lhs.false:                                    ; preds = %for.body
  %8 = load ptr, ptr %text.addr, align 8
  %9 = load i64, ptr %i, align 8
  %arrayidx4 = getelementptr inbounds i8, ptr %8, i64 %9
  %10 = load i8, ptr %arrayidx4, align 1
  %conv5 = zext i8 %10 to i32
  %cmp6 = icmp sgt i32 %conv5, 57
  br i1 %cmp6, label %if.then8, label %if.end9

if.then8:                                         ; preds = %lor.lhs.false, %for.body
  store i64 -1, ptr %retval, align 8
  br label %return

if.end9:                                          ; preds = %lor.lhs.false
  %11 = load i64, ptr %v, align 8
  %mul = mul i64 %11, 10
  %12 = load ptr, ptr %text.addr, align 8
  %13 = load i64, ptr %i, align 8
  %arrayidx10 = getelementptr inbounds i8, ptr %12, i64 %13
  %14 = load i8, ptr %arrayidx10, align 1
  %conv11 = zext i8 %14 to i32
  %sub = sub i32 %conv11, 48
  %conv12 = sext i32 %sub to i64
  %add = add i64 %mul, %conv12
  store i64 %add, ptr %v, align 8
  br label %for.inc

for.inc:                                          ; preds = %if.end9
  %15 = load i64, ptr %i, align 8
  %inc = add i64 %15, 1
  store i64 %inc, ptr %i, align 8
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %16 = load i64, ptr %v, align 8
  store i64 %16, ptr %retval, align 8
  br label %return

return:                                           ; preds = %for.end, %if.then8, %if.then
  %17 = load i64, ptr %retval, align 8
  ret i64 %17
}

; Function Attrs: noinline nounwind
define internal void @netrt_sockaddr_any(ptr %a, i64 %port) {
entry:
  %a.addr = alloca ptr, align 8
  %port.addr = alloca i64, align 8
  %i = alloca i32, align 4
  store ptr %a, ptr %a.addr, align 8
  store i64 %port, ptr %port.addr, align 8
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %0 = load i32, ptr %i, align 4
  %cmp = icmp slt i32 %0, 16
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %1 = load ptr, ptr %a.addr, align 8
  %2 = load i32, ptr %i, align 4
  %idxprom = sext i32 %2 to i64
  %arrayidx = getelementptr inbounds i8, ptr %1, i64 %idxprom
  store i8 0, ptr %arrayidx, align 1
  br label %for.inc

for.inc:                                          ; preds = %for.body
  %3 = load i32, ptr %i, align 4
  %inc = add i32 %3, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %4 = load ptr, ptr %a.addr, align 8
  %arrayidx1 = getelementptr inbounds i8, ptr %4, i64 0
  store i8 2, ptr %arrayidx1, align 1
  %5 = load i64, ptr %port.addr, align 8
  %shr = ashr i64 %5, 8
  %conv = trunc i64 %shr to i8
  %6 = load ptr, ptr %a.addr, align 8
  %arrayidx2 = getelementptr inbounds i8, ptr %6, i64 2
  store i8 %conv, ptr %arrayidx2, align 1
  %7 = load i64, ptr %port.addr, align 8
  %conv3 = trunc i64 %7 to i8
  %8 = load ptr, ptr %a.addr, align 8
  %arrayidx4 = getelementptr inbounds i8, ptr %8, i64 3
  store i8 %conv3, ptr %arrayidx4, align 1
  ret void
}

; Function Attrs: noinline nounwind
define internal i32 @netrt_tcp_socket(i32 %kind) {
entry:
  %kind.addr = alloca i32, align 4
  %fd = alloca i32, align 4
  store i32 %kind, ptr %kind.addr, align 4
  %0 = load i32, ptr %kind.addr, align 4
  %or = or i32 %0, 524288
  %call = call i32 @socket(i32 2, i32 %or, i32 0)
  store i32 %call, ptr %fd, align 4
  %1 = load i32, ptr %fd, align 4
  ret i32 %1
}

; Function Attrs: noinline nounwind
define internal void @netrt_int_option(i32 %fd, i32 %level, i32 %name) {
entry:
  %fd.addr = alloca i32, align 4
  %level.addr = alloca i32, align 4
  %name.addr = alloca i32, align 4
  %one = alloca i32, align 4
  store i32 %fd, ptr %fd.addr, align 4
  store i32 %level, ptr %level.addr, align 4
  store i32 %name, ptr %name.addr, align 4
  store i32 1, ptr %one, align 4
  %0 = load i32, ptr %fd.addr, align 4
  %1 = load i32, ptr %level.addr, align 4
  %2 = load i32, ptr %name.addr, align 4
  %call = call i32 @setsockopt(i32 %0, i32 %1, i32 %2, ptr %one, i32 4)
  ret void
}

declare i32 @bind(i32, ptr, i32)


; Function Attrs: noinline nounwind
define i64 @cancho_net_listen(i64 %fd, i64 %backlog) {
entry:
  %fd.addr = alloca i64, align 8
  %backlog.addr = alloca i64, align 8
  store i64 %fd, ptr %fd.addr, align 8
  store i64 %backlog, ptr %backlog.addr, align 8
  %0 = load i64, ptr %fd.addr, align 8
  %conv = trunc i64 %0 to i32
  %1 = load i64, ptr %backlog.addr, align 8
  %conv1 = trunc i64 %1 to i32
  %call = call i32 @listen(i32 %conv, i32 %conv1)
  %conv2 = sext i32 %call to i64
  ret i64 %conv2
}

declare i32 @listen(i32, i32)

; Function Attrs: noinline nounwind
define i64 @cancho_net_accept(i64 %fd) {
entry:
  %fd.addr = alloca i64, align 8
  store i64 %fd, ptr %fd.addr, align 8
  %0 = load i64, ptr %fd.addr, align 8
  %conv = trunc i64 %0 to i32
  %call = call i32 @netrt_accept_cloexec(i32 %conv)
  %conv1 = sext i32 %call to i64
  ret i64 %conv1
}

; Function Attrs: noinline nounwind
define internal i32 @netrt_accept_cloexec(i32 %fd) {
entry:
  %fd.addr = alloca i32, align 4
  store i32 %fd, ptr %fd.addr, align 4
  %0 = load i32, ptr %fd.addr, align 4
  %call = call i32 @accept4(i32 %0, ptr null, ptr null, i32 524288)
  ret i32 %call
}

; Function Attrs: noinline nounwind
define void @cancho_net_tcp_listen(i64 %port, i64 %backlog, i64 %flags, ptr %bound, i64 %bound_len, ptr %out) {
entry:
  %port.addr = alloca i64, align 8
  %backlog.addr = alloca i64, align 8
  %flags.addr = alloca i64, align 8
  %bound.addr = alloca ptr, align 8
  %bound_len.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %a = alloca [16 x i8], align 1
  %fd = alloca i32, align 4
  store i64 %port, ptr %port.addr, align 8
  store i64 %backlog, ptr %backlog.addr, align 8
  store i64 %flags, ptr %flags.addr, align 8
  store ptr %bound, ptr %bound.addr, align 8
  store i64 %bound_len, ptr %bound_len.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load i64, ptr %port.addr, align 8
  %1 = load ptr, ptr %bound.addr, align 8
  %2 = load i64, ptr %bound_len.addr, align 8
  %call = call i64 @netrt_port_of(ptr %1, i64 0, i64 %2)
  call void @netrt_check_port(i64 %0, i64 %call)
  %arraydecay = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 0
  %3 = load i64, ptr %port.addr, align 8
  call void @netrt_sockaddr_any(ptr %arraydecay, i64 %3)
  %4 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %4, i64 0
  store i64 1, ptr %arrayidx, align 8
  %5 = load ptr, ptr %out.addr, align 8
  %arrayidx1 = getelementptr inbounds i64, ptr %5, i64 1
  store i64 -1, ptr %arrayidx1, align 8
  %6 = load ptr, ptr %out.addr, align 8
  %arrayidx2 = getelementptr inbounds i64, ptr %6, i64 2
  store i64 0, ptr %arrayidx2, align 8
  %call3 = call i32 @netrt_tcp_socket(i32 1)
  store i32 %call3, ptr %fd, align 4
  %7 = load i32, ptr %fd, align 4
  %cmp = icmp slt i32 %7, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %call4 = call ptr @cancho_errno_location()
  %8 = load i64, ptr %call4, align 8
  %9 = load ptr, ptr %out.addr, align 8
  %arrayidx5 = getelementptr inbounds i64, ptr %9, i64 2
  store i64 %8, ptr %arrayidx5, align 8
  br label %return

if.end:                                           ; preds = %entry
  %10 = load i32, ptr %fd, align 4
  call void @netrt_int_option(i32 %10, i32 1, i32 2)
  %11 = load i64, ptr %flags.addr, align 8
  %and = and i64 %11, 1
  %tobool = icmp ne i64 %and, 0
  br i1 %tobool, label %if.then6, label %if.end7

if.then6:                                         ; preds = %if.end
  %12 = load i32, ptr %fd, align 4
  call void @netrt_int_option(i32 %12, i32 1, i32 15)
  br label %if.end7

if.end7:                                          ; preds = %if.then6, %if.end
  %13 = load i32, ptr %fd, align 4
  %arraydecay8 = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 0
  %call9 = call i32 @bind(i32 %13, ptr %arraydecay8, i32 16)
  %cmp10 = icmp ne i32 %call9, 0
  br i1 %cmp10, label %if.then11, label %if.end15

if.then11:                                        ; preds = %if.end7
  %call12 = call ptr @cancho_errno_location()
  %14 = load i64, ptr %call12, align 8
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx13 = getelementptr inbounds i64, ptr %15, i64 2
  store i64 %14, ptr %arrayidx13, align 8
  %16 = load i32, ptr %fd, align 4
  %call14 = call i32 @close(i32 %16)
  br label %return

if.end15:                                         ; preds = %if.end7
  %17 = load i32, ptr %fd, align 4
  %18 = load i64, ptr %backlog.addr, align 8
  %conv = trunc i64 %18 to i32
  %call16 = call i32 @listen(i32 %17, i32 %conv)
  %cmp17 = icmp ne i32 %call16, 0
  br i1 %cmp17, label %if.then19, label %if.end23

if.then19:                                        ; preds = %if.end15
  %call20 = call ptr @cancho_errno_location()
  %19 = load i64, ptr %call20, align 8
  %20 = load ptr, ptr %out.addr, align 8
  %arrayidx21 = getelementptr inbounds i64, ptr %20, i64 2
  store i64 %19, ptr %arrayidx21, align 8
  %21 = load i32, ptr %fd, align 4
  %call22 = call i32 @close(i32 %21)
  br label %return

if.end23:                                         ; preds = %if.end15
  %22 = load ptr, ptr %out.addr, align 8
  %arrayidx24 = getelementptr inbounds i64, ptr %22, i64 0
  store i64 0, ptr %arrayidx24, align 8
  %23 = load i32, ptr %fd, align 4
  %conv25 = sext i32 %23 to i64
  %24 = load ptr, ptr %out.addr, align 8
  %arrayidx26 = getelementptr inbounds i64, ptr %24, i64 1
  store i64 %conv25, ptr %arrayidx26, align 8
  br label %return

return:                                           ; preds = %if.end23, %if.then19, %if.then11, %if.then
  ret void
}


; Function Attrs: noinline nounwind
define void @cancho_net_tcp_accept(ptr %listener, ptr %out) {
entry:
  %listener.addr = alloca ptr, align 8
  %out.addr = alloca ptr, align 8
  %c = alloca i32, align 4
  %why = alloca i64, align 8
  store ptr %listener, ptr %listener.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %listener.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %call = call i32 @netrt_accept_cloexec(i32 %conv)
  store i32 %call, ptr %c, align 4
  %call1 = call ptr @cancho_errno_location()
  %2 = load i64, ptr %call1, align 8
  store i64 %2, ptr %why, align 8
  %3 = load i32, ptr %c, align 4
  %cmp = icmp sge i32 %3, 0
  br i1 %cmp, label %if.then, label %if.else

if.then:                                          ; preds = %entry
  %4 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %4, i64 0
  store i64 0, ptr %arrayidx, align 8
  br label %if.end

if.else:                                          ; preds = %entry
  %5 = load i64, ptr %why, align 8
  %cmp3 = icmp eq i64 %5, 11
  %6 = zext i1 %cmp3 to i64
  %cond = select i1 %cmp3, i32 1, i32 2
  %conv5 = sext i32 %cond to i64
  %7 = load ptr, ptr %out.addr, align 8
  %arrayidx6 = getelementptr inbounds i64, ptr %7, i64 0
  store i64 %conv5, ptr %arrayidx6, align 8
  br label %if.end

if.end:                                           ; preds = %if.else, %if.then
  %8 = load i32, ptr %c, align 4
  %conv7 = sext i32 %8 to i64
  %9 = load ptr, ptr %out.addr, align 8
  %arrayidx8 = getelementptr inbounds i64, ptr %9, i64 1
  store i64 %conv7, ptr %arrayidx8, align 8
  %10 = load i64, ptr %why, align 8
  %11 = load ptr, ptr %out.addr, align 8
  %arrayidx9 = getelementptr inbounds i64, ptr %11, i64 2
  store i64 %10, ptr %arrayidx9, align 8
  ret void
}

; Function Attrs: noinline nounwind
define void @cancho_net_conn_read(ptr %conn, ptr %buf, i64 %len, ptr %out) {
entry:
  %conn.addr = alloca ptr, align 8
  %buf.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %got = alloca i64, align 8
  %why = alloca i64, align 8
  store ptr %conn, ptr %conn.addr, align 8
  store ptr %buf, ptr %buf.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load i64, ptr %len.addr, align 8
  %cmp = icmp eq i64 %0, 0
  br i1 %cmp, label %if.then, label %if.else

if.then:                                          ; preds = %entry
  store i64 -1, ptr %got, align 8
  store i64 22, ptr %why, align 8
  br label %if.end

if.else:                                          ; preds = %entry
  %1 = load ptr, ptr %conn.addr, align 8
  %2 = load i64, ptr %1, align 8
  %conv = trunc i64 %2 to i32
  %3 = load ptr, ptr %buf.addr, align 8
  %4 = load i64, ptr %len.addr, align 8
  %call = call i64 @recv(i32 %conv, ptr %3, i64 %4, i32 0)
  store i64 %call, ptr %got, align 8
  %call1 = call ptr @cancho_errno_location()
  %5 = load i64, ptr %call1, align 8
  store i64 %5, ptr %why, align 8
  br label %if.end

if.end:                                           ; preds = %if.else, %if.then
  %6 = load i64, ptr %got, align 8
  %cmp2 = icmp slt i64 %6, 0
  br i1 %cmp2, label %cond.true, label %cond.false

cond.true:                                        ; preds = %if.end
  %7 = load i64, ptr %why, align 8
  %cmp4 = icmp eq i64 %7, 11
  %8 = zext i1 %cmp4 to i64
  %cond = select i1 %cmp4, i32 2, i32 3
  br label %cond.end

cond.false:                                       ; preds = %if.end
  %9 = load i64, ptr %got, align 8
  %cmp6 = icmp eq i64 %9, 0
  %10 = zext i1 %cmp6 to i64
  %cond8 = select i1 %cmp6, i32 1, i32 0
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond9 = phi i32 [ %cond, %cond.true ], [ %cond8, %cond.false ]
  %conv10 = sext i32 %cond9 to i64
  %11 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %11, i64 0
  store i64 %conv10, ptr %arrayidx, align 8
  %12 = load i64, ptr %got, align 8
  %13 = load ptr, ptr %out.addr, align 8
  %arrayidx11 = getelementptr inbounds i64, ptr %13, i64 1
  store i64 %12, ptr %arrayidx11, align 8
  %14 = load i64, ptr %why, align 8
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx12 = getelementptr inbounds i64, ptr %15, i64 2
  store i64 %14, ptr %arrayidx12, align 8
  ret void
}

declare i64 @recv(i32, ptr, i64, i32)

; Function Attrs: noinline nounwind
define void @cancho_net_conn_write(ptr %conn, ptr %buf, i64 %len, ptr %out) {
entry:
  %conn.addr = alloca ptr, align 8
  %buf.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %moved = alloca i64, align 8
  %why = alloca i64, align 8
  store ptr %conn, ptr %conn.addr, align 8
  store ptr %buf, ptr %buf.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %conn.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %2 = load ptr, ptr %buf.addr, align 8
  %3 = load i64, ptr %len.addr, align 8
  %call = call i64 @send(i32 %conv, ptr %2, i64 %3, i32 16384)
  store i64 %call, ptr %moved, align 8
  %call1 = call ptr @cancho_errno_location()
  %4 = load i64, ptr %call1, align 8
  store i64 %4, ptr %why, align 8
  %5 = load i64, ptr %moved, align 8
  %cmp = icmp slt i64 %5, 0
  br i1 %cmp, label %cond.true, label %cond.false

cond.true:                                        ; preds = %entry
  %6 = load i64, ptr %why, align 8
  %cmp3 = icmp eq i64 %6, 11
  %7 = zext i1 %cmp3 to i64
  %cond = select i1 %cmp3, i32 1, i32 2
  br label %cond.end

cond.false:                                       ; preds = %entry
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond5 = phi i32 [ %cond, %cond.true ], [ 0, %cond.false ]
  %conv6 = sext i32 %cond5 to i64
  %8 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %8, i64 0
  store i64 %conv6, ptr %arrayidx, align 8
  %9 = load i64, ptr %moved, align 8
  %10 = load ptr, ptr %out.addr, align 8
  %arrayidx7 = getelementptr inbounds i64, ptr %10, i64 1
  store i64 %9, ptr %arrayidx7, align 8
  %11 = load i64, ptr %why, align 8
  %12 = load ptr, ptr %out.addr, align 8
  %arrayidx8 = getelementptr inbounds i64, ptr %12, i64 2
  store i64 %11, ptr %arrayidx8, align 8
  ret void
}

declare i64 @send(i32, ptr, i64, i32)

; Function Attrs: noinline nounwind
define i64 @cancho_net_nonblocking(ptr %handle) {
entry:
  %handle.addr = alloca ptr, align 8
  %fd = alloca i32, align 4
  %flags = alloca i32, align 4
  %r = alloca i32, align 4
  store ptr %handle, ptr %handle.addr, align 8
  %0 = load ptr, ptr %handle.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  store i32 %conv, ptr %fd, align 4
  %2 = load i32, ptr %fd, align 4
  %call = call i32 (i32, i32, ...) @fcntl(i32 %2, i32 3)
  store i32 %call, ptr %flags, align 4
  %3 = load i32, ptr %fd, align 4
  %4 = load i32, ptr %flags, align 4
  %or = or i32 %4, 2048
  %call1 = call i32 (i32, i32, ...) @fcntl(i32 %3, i32 4, i32 %or)
  store i32 %call1, ptr %r, align 4
  %5 = load i32, ptr %r, align 4
  %cmp = icmp slt i32 %5, 0
  br i1 %cmp, label %cond.true, label %cond.false

cond.true:                                        ; preds = %entry
  %call3 = call ptr @cancho_errno_location()
  %6 = load i64, ptr %call3, align 8
  br label %cond.end

cond.false:                                       ; preds = %entry
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond = phi i64 [ %6, %cond.true ], [ 0, %cond.false ]
  ret i64 %cond
}


; Function Attrs: noinline nounwind
define i64 @cancho_net_nodelay(ptr %handle) {
entry:
  %handle.addr = alloca ptr, align 8
  %one = alloca i32, align 4
  %r = alloca i32, align 4
  store ptr %handle, ptr %handle.addr, align 8
  store i32 1, ptr %one, align 4
  %0 = load ptr, ptr %handle.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %call = call i32 @setsockopt(i32 %conv, i32 6, i32 1, ptr %one, i32 4)
  store i32 %call, ptr %r, align 4
  %2 = load i32, ptr %r, align 4
  %cmp = icmp slt i32 %2, 0
  br i1 %cmp, label %cond.true, label %cond.false

cond.true:                                        ; preds = %entry
  %call2 = call ptr @cancho_errno_location()
  %3 = load i64, ptr %call2, align 8
  br label %cond.end

cond.false:                                       ; preds = %entry
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond = phi i64 [ %3, %cond.true ], [ 0, %cond.false ]
  ret i64 %cond
}

declare i32 @setsockopt(i32, i32, i32, ptr, i32)

; Function Attrs: noinline nounwind
define void @cancho_net_pipe_open(ptr %out) {
entry:
  %out.addr = alloca ptr, align 8
  %fds = alloca [2 x i32], align 4
  %r = alloca i32, align 4
  %why = alloca i64, align 8
  store ptr %out, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 0
  store i32 -1, ptr %arrayidx, align 4
  %arrayidx1 = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 1
  store i32 -1, ptr %arrayidx1, align 4
  %arraydecay = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 0
  %call = call i32 @socketpair(i32 1, i32 524289, i32 0, ptr %arraydecay)
  store i32 %call, ptr %r, align 4
  %call2 = call ptr @cancho_errno_location()
  %0 = load i64, ptr %call2, align 8
  store i64 %0, ptr %why, align 8
  %1 = load i32, ptr %r, align 4
  %cmp = icmp sge i32 %1, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  br label %if.end

if.end:                                           ; preds = %if.then, %entry
  %2 = load i32, ptr %r, align 4
  %cmp3 = icmp slt i32 %2, 0
  %conv = zext i1 %cmp3 to i32
  %conv4 = sext i32 %conv to i64
  %3 = load ptr, ptr %out.addr, align 8
  %arrayidx5 = getelementptr inbounds i64, ptr %3, i64 0
  store i64 %conv4, ptr %arrayidx5, align 8
  %arrayidx6 = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 0
  %4 = load i32, ptr %arrayidx6, align 4
  %conv7 = sext i32 %4 to i64
  %5 = load ptr, ptr %out.addr, align 8
  %arrayidx8 = getelementptr inbounds i64, ptr %5, i64 1
  store i64 %conv7, ptr %arrayidx8, align 8
  %arrayidx9 = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 1
  %6 = load i32, ptr %arrayidx9, align 4
  %conv10 = sext i32 %6 to i64
  %7 = load ptr, ptr %out.addr, align 8
  %arrayidx11 = getelementptr inbounds i64, ptr %7, i64 2
  store i64 %conv10, ptr %arrayidx11, align 8
  %8 = load i64, ptr %why, align 8
  %9 = load ptr, ptr %out.addr, align 8
  %arrayidx12 = getelementptr inbounds i64, ptr %9, i64 3
  store i64 %8, ptr %arrayidx12, align 8
  ret void
}

declare i32 @socketpair(i32, i32, i32, ptr)

; Function Attrs: noinline nounwind
define i64 @cancho_net_close(i64 %fd) {
entry:
  %fd.addr = alloca i64, align 8
  store i64 %fd, ptr %fd.addr, align 8
  %0 = load i64, ptr %fd.addr, align 8
  %conv = trunc i64 %0 to i32
  %call = call i32 @close(i32 %conv)
  %conv1 = sext i32 %call to i64
  ret i64 %conv1
}

; Function Attrs: noinline nounwind
define i64 @cancho_net_connect(ptr %name, i64 %len, i64 %port, ptr %bound, i64 %bound_len) {
entry:
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %port.addr = alloca i64, align 8
  %bound.addr = alloca ptr, align 8
  %bound_len.addr = alloca i64, align 8
  %why = alloca i64, align 8
  store ptr %name, ptr %name.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store i64 %port, ptr %port.addr, align 8
  store ptr %bound, ptr %bound.addr, align 8
  store i64 %bound_len, ptr %bound_len.addr, align 8
  %0 = load ptr, ptr %name.addr, align 8
  %1 = load i64, ptr %len.addr, align 8
  %2 = load i64, ptr %port.addr, align 8
  %3 = load ptr, ptr %bound.addr, align 8
  %4 = load i64, ptr %bound_len.addr, align 8
  %call = call i64 @netrt_dial(ptr %0, i64 %1, i64 %2, ptr %3, i64 %4, i64 0, i64 0, ptr %why)
  ret i64 %call
}

; Function Attrs: noinline nounwind
define internal i64 @netrt_dial(ptr %name, i64 %len, i64 %port, ptr %bound, i64 %bound_len, i64 %conn, i64 %start, ptr %why) {
entry:
  %retval = alloca i64, align 8
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %port.addr = alloca i64, align 8
  %bound.addr = alloca ptr, align 8
  %bound_len.addr = alloca i64, align 8
  %conn.addr = alloca i64, align 8
  %start.addr = alloca i64, align 8
  %why.addr = alloca ptr, align 8
  %host = alloca [256 x i8], align 1
  %hints = alloca [48 x i8], align 1
  %i = alloca i32, align 4
  %res = alloca ptr, align 8
  %addr = alloca ptr, align 8
  %addrlen = alloca i32, align 4
  %fd = alloca i32, align 4
  %flags = alloca i32, align 4
  %r = alloca i32, align 4
  %reason = alloca i64, align 8
  %ok = alloca i32, align 4
  store ptr %name, ptr %name.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store i64 %port, ptr %port.addr, align 8
  store ptr %bound, ptr %bound.addr, align 8
  store i64 %bound_len, ptr %bound_len.addr, align 8
  store i64 %conn, ptr %conn.addr, align 8
  store i64 %start, ptr %start.addr, align 8
  store ptr %why, ptr %why.addr, align 8
  %0 = load i64, ptr %port.addr, align 8
  %1 = load ptr, ptr %bound.addr, align 8
  %2 = load i64, ptr %bound_len.addr, align 8
  %call = call i64 @netrt_dial_port(ptr %1, i64 %2)
  call void @netrt_check_port(i64 %0, i64 %call)
  %3 = load ptr, ptr %name.addr, align 8
  %4 = load i64, ptr %len.addr, align 8
  %5 = load ptr, ptr %bound.addr, align 8
  %6 = load ptr, ptr %bound.addr, align 8
  %7 = load i64, ptr %bound_len.addr, align 8
  %call1 = call i64 @netrt_host_length(ptr %6, i64 %7)
  %arraydecay = getelementptr inbounds [256 x i8], ptr %host, i64 0, i64 0
  call void @netrt_checked_host(ptr %3, i64 %4, ptr %5, i64 %call1, ptr %arraydecay)
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %8 = load i32, ptr %i, align 4
  %cmp = icmp slt i32 %8, 48
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %9 = load i32, ptr %i, align 4
  %idxprom = sext i32 %9 to i64
  %arrayidx = getelementptr inbounds [48 x i8], ptr %hints, i64 0, i64 %idxprom
  store i8 0, ptr %arrayidx, align 1
  br label %for.inc

for.inc:                                          ; preds = %for.body
  %10 = load i32, ptr %i, align 4
  %inc = add i32 %10, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %arraydecay2 = getelementptr inbounds [48 x i8], ptr %hints, i64 0, i64 0
  %add.ptr = getelementptr inbounds i8, ptr %arraydecay2, i64 4
  store i32 2, ptr %add.ptr, align 4
  %arraydecay3 = getelementptr inbounds [48 x i8], ptr %hints, i64 0, i64 0
  %add.ptr4 = getelementptr inbounds i8, ptr %arraydecay3, i64 8
  store i32 1, ptr %add.ptr4, align 4
  store ptr null, ptr %res, align 8
  %11 = load ptr, ptr %why.addr, align 8
  store i64 0, ptr %11, align 8
  %arraydecay5 = getelementptr inbounds [256 x i8], ptr %host, i64 0, i64 0
  %arraydecay6 = getelementptr inbounds [48 x i8], ptr %hints, i64 0, i64 0
  %call7 = call i32 @getaddrinfo(ptr %arraydecay5, ptr null, ptr %arraydecay6, ptr %res)
  %cmp8 = icmp ne i32 %call7, 0
  br i1 %cmp8, label %if.then, label %if.end

if.then:                                          ; preds = %for.end
  %12 = load ptr, ptr %why.addr, align 8
  store i64 -1, ptr %12, align 8
  store i64 -1, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %for.end
  %13 = load ptr, ptr %res, align 8
  %add.ptr9 = getelementptr inbounds i8, ptr %13, i64 24
  %14 = load ptr, ptr %add.ptr9, align 8
  store ptr %14, ptr %addr, align 8
  %15 = load ptr, ptr %res, align 8
  %add.ptr10 = getelementptr inbounds i8, ptr %15, i64 16
  %16 = load i32, ptr %add.ptr10, align 4
  store i32 %16, ptr %addrlen, align 4
  %17 = load i64, ptr %port.addr, align 8
  %shr = ashr i64 %17, 8
  %conv = trunc i64 %shr to i8
  %18 = load ptr, ptr %addr, align 8
  %arrayidx11 = getelementptr inbounds i8, ptr %18, i64 2
  store i8 %conv, ptr %arrayidx11, align 1
  %19 = load i64, ptr %port.addr, align 8
  %conv12 = trunc i64 %19 to i8
  %20 = load ptr, ptr %addr, align 8
  %arrayidx13 = getelementptr inbounds i8, ptr %20, i64 3
  store i8 %conv12, ptr %arrayidx13, align 1
  %call14 = call i32 @netrt_tcp_socket(i32 1)
  store i32 %call14, ptr %fd, align 4
  %21 = load i32, ptr %fd, align 4
  %cmp15 = icmp slt i32 %21, 0
  br i1 %cmp15, label %if.then17, label %if.end19

if.then17:                                        ; preds = %if.end
  %call18 = call ptr @cancho_errno_location()
  %22 = load i64, ptr %call18, align 8
  %23 = load ptr, ptr %why.addr, align 8
  store i64 %22, ptr %23, align 8
  %24 = load ptr, ptr %res, align 8
  call void @freeaddrinfo(ptr %24)
  store i64 -1, ptr %retval, align 8
  br label %return

if.end19:                                         ; preds = %if.end
  %25 = load i64, ptr %start.addr, align 8
  %tobool = icmp ne i64 %25, 0
  br i1 %tobool, label %if.then20, label %if.end23

if.then20:                                        ; preds = %if.end19
  %26 = load i32, ptr %fd, align 4
  %call21 = call i32 (i32, i32, ...) @fcntl(i32 %26, i32 3)
  store i32 %call21, ptr %flags, align 4
  %27 = load i32, ptr %fd, align 4
  %28 = load i32, ptr %flags, align 4
  %or = or i32 %28, 2048
  %call22 = call i32 (i32, i32, ...) @fcntl(i32 %27, i32 4, i32 %or)
  br label %if.end23

if.end23:                                         ; preds = %if.then20, %if.end19
  %29 = load i32, ptr %fd, align 4
  %30 = load ptr, ptr %addr, align 8
  %31 = load i32, ptr %addrlen, align 4
  %call24 = call i32 @connect(i32 %29, ptr %30, i32 %31)
  store i32 %call24, ptr %r, align 4
  %call25 = call ptr @cancho_errno_location()
  %32 = load i64, ptr %call25, align 8
  store i64 %32, ptr %reason, align 8
  %33 = load i32, ptr %r, align 4
  %cmp26 = icmp eq i32 %33, 0
  br i1 %cmp26, label %lor.end, label %lor.rhs

lor.rhs:                                          ; preds = %if.end23
  %34 = load i64, ptr %start.addr, align 8
  %tobool28 = icmp ne i64 %34, 0
  br i1 %tobool28, label %land.lhs.true, label %land.end

land.lhs.true:                                    ; preds = %lor.rhs
  %35 = load i32, ptr %r, align 4
  %cmp29 = icmp slt i32 %35, 0
  br i1 %cmp29, label %land.rhs, label %land.end

land.rhs:                                         ; preds = %land.lhs.true
  %36 = load i64, ptr %reason, align 8
  %cmp31 = icmp eq i64 %36, 115
  br label %land.end

land.end:                                         ; preds = %land.rhs, %land.lhs.true, %lor.rhs
  %37 = phi i1 [ false, %land.lhs.true ], [ false, %lor.rhs ], [ %cmp31, %land.rhs ]
  br label %lor.end

lor.end:                                          ; preds = %land.end, %if.end23
  %38 = phi i1 [ true, %if.end23 ], [ %37, %land.end ]
  %lor.ext = zext i1 %38 to i32
  store i32 %lor.ext, ptr %ok, align 4
  %39 = load ptr, ptr %res, align 8
  call void @freeaddrinfo(ptr %39)
  %40 = load i32, ptr %ok, align 4
  %tobool33 = icmp ne i32 %40, 0
  br i1 %tobool33, label %if.then34, label %if.end36

if.then34:                                        ; preds = %lor.end
  %41 = load i32, ptr %fd, align 4
  %conv35 = sext i32 %41 to i64
  store i64 %conv35, ptr %retval, align 8
  br label %return

if.end36:                                         ; preds = %lor.end
  %42 = load i64, ptr %reason, align 8
  %43 = load ptr, ptr %why.addr, align 8
  store i64 %42, ptr %43, align 8
  %44 = load i32, ptr %fd, align 4
  %call37 = call i32 @close(i32 %44)
  store i64 -1, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end36, %if.then34, %if.then17, %if.then
  %45 = load i64, ptr %retval, align 8
  ret i64 %45
}

; Function Attrs: noinline nounwind
define void @cancho_net_tcp_connect(ptr %name, i64 %len, i64 %port, ptr %bound, i64 %bound_len, ptr %out) {
entry:
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %port.addr = alloca i64, align 8
  %bound.addr = alloca ptr, align 8
  %bound_len.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %why = alloca i64, align 8
  %fd = alloca i64, align 8
  store ptr %name, ptr %name.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store i64 %port, ptr %port.addr, align 8
  store ptr %bound, ptr %bound.addr, align 8
  store i64 %bound_len, ptr %bound_len.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %name.addr, align 8
  %1 = load i64, ptr %len.addr, align 8
  %2 = load i64, ptr %port.addr, align 8
  %3 = load ptr, ptr %bound.addr, align 8
  %4 = load i64, ptr %bound_len.addr, align 8
  %call = call i64 @netrt_dial(ptr %0, i64 %1, i64 %2, ptr %3, i64 %4, i64 1, i64 0, ptr %why)
  store i64 %call, ptr %fd, align 8
  %5 = load i64, ptr %fd, align 8
  %cmp = icmp slt i64 %5, 0
  %6 = zext i1 %cmp to i64
  %cond = select i1 %cmp, i32 1, i32 0
  %conv = sext i32 %cond to i64
  %7 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %7, i64 0
  store i64 %conv, ptr %arrayidx, align 8
  %8 = load i64, ptr %fd, align 8
  %9 = load ptr, ptr %out.addr, align 8
  %arrayidx1 = getelementptr inbounds i64, ptr %9, i64 1
  store i64 %8, ptr %arrayidx1, align 8
  %10 = load i64, ptr %why, align 8
  %11 = load ptr, ptr %out.addr, align 8
  %arrayidx2 = getelementptr inbounds i64, ptr %11, i64 2
  store i64 %10, ptr %arrayidx2, align 8
  ret void
}

; Function Attrs: noinline nounwind
define i64 @cancho_clock_ms(i64 %wall) {
entry:
  %wall.addr = alloca i64, align 8
  %ts = alloca [2 x i64], align 8
  store i64 %wall, ptr %wall.addr, align 8
  %0 = load i64, ptr %wall.addr, align 8
  %tobool = icmp ne i64 %0, 0
  %1 = zext i1 %tobool to i64
  %cond = select i1 %tobool, i32 0, i32 1
  %arraydecay = getelementptr inbounds [2 x i64], ptr %ts, i64 0, i64 0
  %call = call i32 @clock_gettime(i32 %cond, ptr %arraydecay)
  %arrayidx = getelementptr inbounds [2 x i64], ptr %ts, i64 0, i64 0
  %2 = load i64, ptr %arrayidx, align 8
  %mul = mul i64 %2, 1000
  %arrayidx1 = getelementptr inbounds [2 x i64], ptr %ts, i64 0, i64 1
  %3 = load i64, ptr %arrayidx1, align 8
  %div = udiv i64 %3, 1000000
  %add = add i64 %mul, %div
  ret i64 %add
}

declare i32 @clock_gettime(i32, ptr)

; Function Attrs: cold noreturn nounwind

declare i32 @socket(i32, i32, i32)

declare i32 @accept4(i32, ptr, ptr, i32)

; Function Attrs: noinline nounwind
define internal i64 @netrt_dial_port(ptr %bound, i64 %len) {
entry:
  %retval = alloca i64, align 8
  %bound.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %i = alloca i64, align 8
  store ptr %bound, ptr %bound.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  %0 = load i64, ptr %len.addr, align 8
  store i64 %0, ptr %i, align 8
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %1 = load i64, ptr %i, align 8
  %cmp = icmp sgt i64 %1, 0
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %2 = load ptr, ptr %bound.addr, align 8
  %3 = load i64, ptr %i, align 8
  %sub = sub i64 %3, 1
  %arrayidx = getelementptr inbounds i8, ptr %2, i64 %sub
  %4 = load i8, ptr %arrayidx, align 1
  %conv = zext i8 %4 to i32
  %cmp1 = icmp eq i32 %conv, 58
  br i1 %cmp1, label %if.then, label %if.end

if.then:                                          ; preds = %for.body
  %5 = load ptr, ptr %bound.addr, align 8
  %6 = load i64, ptr %i, align 8
  %7 = load i64, ptr %len.addr, align 8
  %call = call i64 @netrt_port_of(ptr %5, i64 %6, i64 %7)
  store i64 %call, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end
  %8 = load i64, ptr %i, align 8
  %dec = add i64 %8, -1
  store i64 %dec, ptr %i, align 8
  br label %for.cond

for.end:                                          ; preds = %for.cond
  store i64 -2, ptr %retval, align 8
  br label %return

return:                                           ; preds = %for.end, %if.then
  %9 = load i64, ptr %retval, align 8
  ret i64 %9
}

; Function Attrs: noinline nounwind
define internal void @netrt_checked_host(ptr %name, i64 %len, ptr %bound, i64 %host_len, ptr %buffer) {
entry:
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %bound.addr = alloca ptr, align 8
  %host_len.addr = alloca i64, align 8
  %buffer.addr = alloca ptr, align 8
  %i = alloca i64, align 8
  store ptr %name, ptr %name.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store ptr %bound, ptr %bound.addr, align 8
  store i64 %host_len, ptr %host_len.addr, align 8
  store ptr %buffer, ptr %buffer.addr, align 8
  %0 = load i64, ptr %len.addr, align 8
  %cmp = icmp sge i64 %0, 256
  br i1 %cmp, label %if.then, label %lor.lhs.false

lor.lhs.false:                                    ; preds = %entry
  %1 = load i64, ptr %len.addr, align 8
  %2 = load i64, ptr %host_len.addr, align 8
  %cmp1 = icmp slt i64 %1, %2
  br i1 %cmp1, label %if.then, label %if.end

if.then:                                          ; preds = %lor.lhs.false, %entry
  call void @llvm.trap()
  br label %if.end

if.end:                                           ; preds = %if.then, %lor.lhs.false
  store i64 0, ptr %i, align 8
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %if.end
  %3 = load i64, ptr %i, align 8
  %4 = load i64, ptr %len.addr, align 8
  %cmp2 = icmp slt i64 %3, %4
  br i1 %cmp2, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %5 = load ptr, ptr %name.addr, align 8
  %6 = load i64, ptr %i, align 8
  %arrayidx = getelementptr inbounds i8, ptr %5, i64 %6
  %7 = load i8, ptr %arrayidx, align 1
  %8 = load ptr, ptr %buffer.addr, align 8
  %9 = load i64, ptr %i, align 8
  %arrayidx3 = getelementptr inbounds i8, ptr %8, i64 %9
  store i8 %7, ptr %arrayidx3, align 1
  %10 = load i64, ptr %i, align 8
  %11 = load i64, ptr %host_len.addr, align 8
  %cmp4 = icmp slt i64 %10, %11
  br i1 %cmp4, label %land.lhs.true, label %if.end11

land.lhs.true:                                    ; preds = %for.body
  %12 = load ptr, ptr %name.addr, align 8
  %13 = load i64, ptr %i, align 8
  %arrayidx5 = getelementptr inbounds i8, ptr %12, i64 %13
  %14 = load i8, ptr %arrayidx5, align 1
  %conv = zext i8 %14 to i32
  %15 = load ptr, ptr %bound.addr, align 8
  %16 = load i64, ptr %i, align 8
  %arrayidx6 = getelementptr inbounds i8, ptr %15, i64 %16
  %17 = load i8, ptr %arrayidx6, align 1
  %conv7 = zext i8 %17 to i32
  %cmp8 = icmp ne i32 %conv, %conv7
  br i1 %cmp8, label %if.then10, label %if.end11

if.then10:                                        ; preds = %land.lhs.true
  call void @llvm.trap()
  br label %if.end11

if.end11:                                         ; preds = %if.then10, %land.lhs.true, %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end11
  %18 = load i64, ptr %i, align 8
  %inc = add i64 %18, 1
  store i64 %inc, ptr %i, align 8
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %19 = load ptr, ptr %buffer.addr, align 8
  %20 = load i64, ptr %len.addr, align 8
  %arrayidx12 = getelementptr inbounds i8, ptr %19, i64 %20
  store i8 0, ptr %arrayidx12, align 1
  ret void
}

; Function Attrs: noinline nounwind
define internal i64 @netrt_host_length(ptr %bound, i64 %len) {
entry:
  %retval = alloca i64, align 8
  %bound.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %i = alloca i64, align 8
  store ptr %bound, ptr %bound.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  %0 = load i64, ptr %len.addr, align 8
  store i64 %0, ptr %i, align 8
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %1 = load i64, ptr %i, align 8
  %cmp = icmp sgt i64 %1, 0
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %2 = load ptr, ptr %bound.addr, align 8
  %3 = load i64, ptr %i, align 8
  %sub = sub i64 %3, 1
  %arrayidx = getelementptr inbounds i8, ptr %2, i64 %sub
  %4 = load i8, ptr %arrayidx, align 1
  %conv = zext i8 %4 to i32
  %cmp1 = icmp eq i32 %conv, 58
  br i1 %cmp1, label %if.then, label %if.end

if.then:                                          ; preds = %for.body
  %5 = load i64, ptr %i, align 8
  %sub3 = sub i64 %5, 1
  store i64 %sub3, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end
  %6 = load i64, ptr %i, align 8
  %dec = add i64 %6, -1
  store i64 %dec, ptr %i, align 8
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %7 = load i64, ptr %len.addr, align 8
  store i64 %7, ptr %retval, align 8
  br label %return

return:                                           ; preds = %for.end, %if.then
  %8 = load i64, ptr %retval, align 8
  ret i64 %8
}

declare i32 @getaddrinfo(ptr, ptr, ptr, ptr)

declare void @freeaddrinfo(ptr)

declare i32 @connect(i32, ptr, i32)



