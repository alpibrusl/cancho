
@lexs_udp_peers = common global [2097160 x i8] zeroinitializer, align 1
@lexs_fd_epoch = common global [65536 x i32] zeroinitializer, align 4
@__stdoutp = external global ptr, align 8
@__stdinp = external global ptr, align 8

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
  call void @netrt_int_option(i32 %5, i32 65535, i32 4)
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
  %or = or i32 %0, 0
  %call = call i32 @socket(i32 2, i32 %or, i32 0)
  store i32 %call, ptr %fd, align 4
  %1 = load i32, ptr %fd, align 4
  %cmp = icmp sge i32 %1, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %2 = load i32, ptr %fd, align 4
  %call1 = call i32 (i32, i32, ...) @fcntl(i32 %2, i32 2, i32 1)
  br label %if.end

if.end:                                           ; preds = %if.then, %entry
  %3 = load i32, ptr %fd, align 4
  ret i32 %3
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
  %c = alloca i32, align 4
  store i32 %fd, ptr %fd.addr, align 4
  %0 = load i32, ptr %fd.addr, align 4
  %call = call i32 @accept(i32 %0, ptr null, ptr null)
  store i32 %call, ptr %c, align 4
  %1 = load i32, ptr %c, align 4
  %cmp = icmp sge i32 %1, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %2 = load i32, ptr %c, align 4
  %call1 = call i32 (i32, i32, ...) @fcntl(i32 %2, i32 2, i32 1)
  br label %if.end

if.end:                                           ; preds = %if.then, %entry
  %3 = load i32, ptr %c, align 4
  ret i32 %3
}

; Function Attrs: noinline nounwind
define void @cancho_net_tcp_listen(i64 %port, i64 %backlog, i64 %flags, ptr %bound, i64 %bound_len, i64 %host, ptr %out) {
entry:
  %port.addr = alloca i64, align 8
  %backlog.addr = alloca i64, align 8
  %flags.addr = alloca i64, align 8
  %bound.addr = alloca ptr, align 8
  %bound_len.addr = alloca i64, align 8
  %host.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %a = alloca [16 x i8], align 1
  %fd = alloca i32, align 4
  store i64 %port, ptr %port.addr, align 8
  store i64 %backlog, ptr %backlog.addr, align 8
  store i64 %flags, ptr %flags.addr, align 8
  store ptr %bound, ptr %bound.addr, align 8
  store i64 %bound_len, ptr %bound_len.addr, align 8
  store i64 %host, ptr %host.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load i64, ptr %port.addr, align 8
  %1 = load ptr, ptr %bound.addr, align 8
  %2 = load i64, ptr %bound_len.addr, align 8
  %call = call i64 @netrt_listen_port(ptr %1, i64 %2)
  call void @netrt_check_port(i64 %0, i64 %call)
  %arraydecay = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 0
  %3 = load i64, ptr %port.addr, align 8
  call void @netrt_sockaddr_any(ptr %arraydecay, i64 %3)
  %4 = load i64, ptr %host.addr, align 8
  %shr = ashr i64 %4, 24
  %conv = trunc i64 %shr to i8
  %arrayidx = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 4
  store i8 %conv, ptr %arrayidx, align 1
  %5 = load i64, ptr %host.addr, align 8
  %shr1 = ashr i64 %5, 16
  %conv2 = trunc i64 %shr1 to i8
  %arrayidx3 = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 5
  store i8 %conv2, ptr %arrayidx3, align 1
  %6 = load i64, ptr %host.addr, align 8
  %shr4 = ashr i64 %6, 8
  %conv5 = trunc i64 %shr4 to i8
  %arrayidx6 = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 6
  store i8 %conv5, ptr %arrayidx6, align 1
  %7 = load i64, ptr %host.addr, align 8
  %conv7 = trunc i64 %7 to i8
  %arrayidx8 = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 7
  store i8 %conv7, ptr %arrayidx8, align 1
  %8 = load ptr, ptr %out.addr, align 8
  %arrayidx9 = getelementptr inbounds i64, ptr %8, i64 0
  store i64 1, ptr %arrayidx9, align 8
  %9 = load ptr, ptr %out.addr, align 8
  %arrayidx10 = getelementptr inbounds i64, ptr %9, i64 1
  store i64 -1, ptr %arrayidx10, align 8
  %10 = load ptr, ptr %out.addr, align 8
  %arrayidx11 = getelementptr inbounds i64, ptr %10, i64 2
  store i64 0, ptr %arrayidx11, align 8
  %call12 = call i32 @netrt_tcp_socket(i32 1)
  store i32 %call12, ptr %fd, align 4
  %11 = load i32, ptr %fd, align 4
  %cmp = icmp slt i32 %11, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %call14 = call ptr @cancho_errno_location()
  %12 = load i32, ptr %call14, align 4
  %conv15 = sext i32 %12 to i64
  %13 = load ptr, ptr %out.addr, align 8
  %arrayidx16 = getelementptr inbounds i64, ptr %13, i64 2
  store i64 %conv15, ptr %arrayidx16, align 8
  br label %return

if.end:                                           ; preds = %entry
  %14 = load i32, ptr %fd, align 4
  call void @netrt_int_option(i32 %14, i32 65535, i32 4)
  %15 = load i64, ptr %flags.addr, align 8
  %and = and i64 %15, 1
  %tobool = icmp ne i64 %and, 0
  br i1 %tobool, label %if.then17, label %if.end18

if.then17:                                        ; preds = %if.end
  %16 = load i32, ptr %fd, align 4
  call void @netrt_int_option(i32 %16, i32 65535, i32 512)
  br label %if.end18

if.end18:                                         ; preds = %if.then17, %if.end
  %17 = load i32, ptr %fd, align 4
  %arraydecay19 = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 0
  %call20 = call i32 @bind(i32 %17, ptr %arraydecay19, i32 16)
  %cmp21 = icmp ne i32 %call20, 0
  br i1 %cmp21, label %if.then23, label %if.end28

if.then23:                                        ; preds = %if.end18
  %call24 = call ptr @cancho_errno_location()
  %18 = load i32, ptr %call24, align 4
  %conv25 = sext i32 %18 to i64
  %19 = load ptr, ptr %out.addr, align 8
  %arrayidx26 = getelementptr inbounds i64, ptr %19, i64 2
  store i64 %conv25, ptr %arrayidx26, align 8
  %20 = load i32, ptr %fd, align 4
  %call27 = call i32 @close(i32 %20)
  br label %return

if.end28:                                         ; preds = %if.end18
  %21 = load i32, ptr %fd, align 4
  %22 = load i64, ptr %backlog.addr, align 8
  %conv29 = trunc i64 %22 to i32
  %call30 = call i32 @listen(i32 %21, i32 %conv29)
  %cmp31 = icmp ne i32 %call30, 0
  br i1 %cmp31, label %if.then33, label %if.end38

if.then33:                                        ; preds = %if.end28
  %call34 = call ptr @cancho_errno_location()
  %23 = load i32, ptr %call34, align 4
  %conv35 = sext i32 %23 to i64
  %24 = load ptr, ptr %out.addr, align 8
  %arrayidx36 = getelementptr inbounds i64, ptr %24, i64 2
  store i64 %conv35, ptr %arrayidx36, align 8
  %25 = load i32, ptr %fd, align 4
  %call37 = call i32 @close(i32 %25)
  br label %return

if.end38:                                         ; preds = %if.end28
  %26 = load ptr, ptr %out.addr, align 8
  %arrayidx39 = getelementptr inbounds i64, ptr %26, i64 0
  store i64 0, ptr %arrayidx39, align 8
  %27 = load i32, ptr %fd, align 4
  %conv40 = sext i32 %27 to i64
  %28 = load ptr, ptr %out.addr, align 8
  %arrayidx41 = getelementptr inbounds i64, ptr %28, i64 1
  store i64 %conv40, ptr %arrayidx41, align 8
  br label %return

return:                                           ; preds = %if.end38, %if.then33, %if.then23, %if.then
  ret void
}

; Function Attrs: noinline nounwind
define internal i64 @netrt_listen_port(ptr %bound, i64 %len) {
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
  %9 = load ptr, ptr %bound.addr, align 8
  %10 = load i64, ptr %len.addr, align 8
  %call3 = call i64 @netrt_port_of(ptr %9, i64 0, i64 %10)
  store i64 %call3, ptr %retval, align 8
  br label %return

return:                                           ; preds = %for.end, %if.then
  %11 = load i64, ptr %retval, align 8
  ret i64 %11
}


; Function Attrs: noinline nounwind
define void @cancho_net_tcp_accept(ptr %listener, ptr %out) {
entry:
  %listener.addr = alloca ptr, align 8
  %out.addr = alloca ptr, align 8
  %c = alloca i32, align 4
  %why = alloca i64, align 8
  %flags = alloca i32, align 4
  store ptr %listener, ptr %listener.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %listener.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %call = call i32 @netrt_accept_cloexec(i32 %conv)
  store i32 %call, ptr %c, align 4
  %call1 = call ptr @cancho_errno_location()
  %2 = load i32, ptr %call1, align 4
  %conv2 = sext i32 %2 to i64
  store i64 %conv2, ptr %why, align 8
  %3 = load i32, ptr %c, align 4
  %cmp = icmp sge i32 %3, 0
  br i1 %cmp, label %if.then, label %if.else

if.then:                                          ; preds = %entry
  %4 = load i32, ptr %c, align 4
  call void @netrt_int_option(i32 %4, i32 65535, i32 4130)
  %5 = load i32, ptr %c, align 4
  %call4 = call i32 (i32, i32, ...) @fcntl(i32 %5, i32 3)
  store i32 %call4, ptr %flags, align 4
  %6 = load i32, ptr %c, align 4
  %7 = load i32, ptr %flags, align 4
  %and = and i32 %7, -5
  %call5 = call i32 (i32, i32, ...) @fcntl(i32 %6, i32 4, i32 %and)
  %8 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %8, i64 0
  store i64 0, ptr %arrayidx, align 8
  br label %if.end

if.else:                                          ; preds = %entry
  %9 = load i64, ptr %why, align 8
  %cmp6 = icmp eq i64 %9, 35
  %10 = zext i1 %cmp6 to i64
  %cond = select i1 %cmp6, i32 1, i32 2
  %conv8 = sext i32 %cond to i64
  %11 = load ptr, ptr %out.addr, align 8
  %arrayidx9 = getelementptr inbounds i64, ptr %11, i64 0
  store i64 %conv8, ptr %arrayidx9, align 8
  br label %if.end

if.end:                                           ; preds = %if.else, %if.then
  %12 = load i32, ptr %c, align 4
  %conv10 = sext i32 %12 to i64
  %13 = load ptr, ptr %out.addr, align 8
  %arrayidx11 = getelementptr inbounds i64, ptr %13, i64 1
  store i64 %conv10, ptr %arrayidx11, align 8
  %14 = load i64, ptr %why, align 8
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx12 = getelementptr inbounds i64, ptr %15, i64 2
  store i64 %14, ptr %arrayidx12, align 8
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
  %5 = load i32, ptr %call1, align 4
  %conv2 = sext i32 %5 to i64
  store i64 %conv2, ptr %why, align 8
  br label %if.end

if.end:                                           ; preds = %if.else, %if.then
  %6 = load i64, ptr %got, align 8
  %cmp3 = icmp slt i64 %6, 0
  br i1 %cmp3, label %cond.true, label %cond.false

cond.true:                                        ; preds = %if.end
  %7 = load i64, ptr %why, align 8
  %cmp5 = icmp eq i64 %7, 35
  %8 = zext i1 %cmp5 to i64
  %cond = select i1 %cmp5, i32 2, i32 3
  br label %cond.end

cond.false:                                       ; preds = %if.end
  %9 = load i64, ptr %got, align 8
  %cmp7 = icmp eq i64 %9, 0
  %10 = zext i1 %cmp7 to i64
  %cond9 = select i1 %cmp7, i32 1, i32 0
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond10 = phi i32 [ %cond, %cond.true ], [ %cond9, %cond.false ]
  %conv11 = sext i32 %cond10 to i64
  %11 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %11, i64 0
  store i64 %conv11, ptr %arrayidx, align 8
  %12 = load i64, ptr %got, align 8
  %13 = load ptr, ptr %out.addr, align 8
  %arrayidx12 = getelementptr inbounds i64, ptr %13, i64 1
  store i64 %12, ptr %arrayidx12, align 8
  %14 = load i64, ptr %why, align 8
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx13 = getelementptr inbounds i64, ptr %15, i64 2
  store i64 %14, ptr %arrayidx13, align 8
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
  %call = call i64 @send(i32 %conv, ptr %2, i64 %3, i32 0)
  store i64 %call, ptr %moved, align 8
  %call1 = call ptr @cancho_errno_location()
  %4 = load i32, ptr %call1, align 4
  %conv2 = sext i32 %4 to i64
  store i64 %conv2, ptr %why, align 8
  %5 = load i64, ptr %moved, align 8
  %cmp = icmp slt i64 %5, 0
  br i1 %cmp, label %cond.true, label %cond.false

cond.true:                                        ; preds = %entry
  %6 = load i64, ptr %why, align 8
  %cmp4 = icmp eq i64 %6, 35
  %7 = zext i1 %cmp4 to i64
  %cond = select i1 %cmp4, i32 1, i32 2
  br label %cond.end

cond.false:                                       ; preds = %entry
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond6 = phi i32 [ %cond, %cond.true ], [ 0, %cond.false ]
  %conv7 = sext i32 %cond6 to i64
  %8 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %8, i64 0
  store i64 %conv7, ptr %arrayidx, align 8
  %9 = load i64, ptr %moved, align 8
  %10 = load ptr, ptr %out.addr, align 8
  %arrayidx8 = getelementptr inbounds i64, ptr %10, i64 1
  store i64 %9, ptr %arrayidx8, align 8
  %11 = load i64, ptr %why, align 8
  %12 = load ptr, ptr %out.addr, align 8
  %arrayidx9 = getelementptr inbounds i64, ptr %12, i64 2
  store i64 %11, ptr %arrayidx9, align 8
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
  %or = or i32 %4, 4
  %call1 = call i32 (i32, i32, ...) @fcntl(i32 %3, i32 4, i32 %or)
  store i32 %call1, ptr %r, align 4
  %5 = load i32, ptr %r, align 4
  %cmp = icmp slt i32 %5, 0
  br i1 %cmp, label %cond.true, label %cond.false

cond.true:                                        ; preds = %entry
  %call3 = call ptr @cancho_errno_location()
  %6 = load i32, ptr %call3, align 4
  br label %cond.end

cond.false:                                       ; preds = %entry
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond = phi i32 [ %6, %cond.true ], [ 0, %cond.false ]
  %conv4 = sext i32 %cond to i64
  ret i64 %conv4
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
  %3 = load i32, ptr %call2, align 4
  br label %cond.end

cond.false:                                       ; preds = %entry
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond = phi i32 [ %3, %cond.true ], [ 0, %cond.false ]
  %conv3 = sext i32 %cond to i64
  ret i64 %conv3
}

declare i32 @setsockopt(i32, i32, i32, ptr, i32)

; Function Attrs: noinline nounwind
define void @cancho_net_pipe_open(ptr %out) {
entry:
  %out.addr = alloca ptr, align 8
  %fds = alloca [2 x i32], align 4
  %r = alloca i32, align 4
  %why = alloca i64, align 8
  %i = alloca i32, align 4
  store ptr %out, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 0
  store i32 -1, ptr %arrayidx, align 4
  %arrayidx1 = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 1
  store i32 -1, ptr %arrayidx1, align 4
  %arraydecay = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 0
  %call = call i32 @socketpair(i32 1, i32 1, i32 0, ptr %arraydecay)
  store i32 %call, ptr %r, align 4
  %call2 = call ptr @cancho_errno_location()
  %0 = load i32, ptr %call2, align 4
  %conv = sext i32 %0 to i64
  store i64 %conv, ptr %why, align 8
  %1 = load i32, ptr %r, align 4
  %cmp = icmp sge i32 %1, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %if.then
  %2 = load i32, ptr %i, align 4
  %cmp4 = icmp slt i32 %2, 2
  br i1 %cmp4, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %3 = load i32, ptr %i, align 4
  %idxprom = sext i32 %3 to i64
  %arrayidx6 = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 %idxprom
  %4 = load i32, ptr %arrayidx6, align 4
  %call7 = call i32 (i32, i32, ...) @fcntl(i32 %4, i32 2, i32 1)
  %5 = load i32, ptr %i, align 4
  %idxprom8 = sext i32 %5 to i64
  %arrayidx9 = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 %idxprom8
  %6 = load i32, ptr %arrayidx9, align 4
  call void @netrt_int_option(i32 %6, i32 65535, i32 4130)
  br label %for.inc

for.inc:                                          ; preds = %for.body
  %7 = load i32, ptr %i, align 4
  %inc = add i32 %7, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  br label %if.end

if.end:                                           ; preds = %for.end, %entry
  %8 = load i32, ptr %r, align 4
  %cmp10 = icmp slt i32 %8, 0
  %conv11 = zext i1 %cmp10 to i32
  %conv12 = sext i32 %conv11 to i64
  %9 = load ptr, ptr %out.addr, align 8
  %arrayidx13 = getelementptr inbounds i64, ptr %9, i64 0
  store i64 %conv12, ptr %arrayidx13, align 8
  %arrayidx14 = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 0
  %10 = load i32, ptr %arrayidx14, align 4
  %conv15 = sext i32 %10 to i64
  %11 = load ptr, ptr %out.addr, align 8
  %arrayidx16 = getelementptr inbounds i64, ptr %11, i64 1
  store i64 %conv15, ptr %arrayidx16, align 8
  %arrayidx17 = getelementptr inbounds [2 x i32], ptr %fds, i64 0, i64 1
  %12 = load i32, ptr %arrayidx17, align 4
  %conv18 = sext i32 %12 to i64
  %13 = load ptr, ptr %out.addr, align 8
  %arrayidx19 = getelementptr inbounds i64, ptr %13, i64 2
  store i64 %conv18, ptr %arrayidx19, align 8
  %14 = load i64, ptr %why, align 8
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx20 = getelementptr inbounds i64, ptr %15, i64 3
  store i64 %14, ptr %arrayidx20, align 8
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
  %call = call i64 @netrt_dial(ptr %0, i64 %1, i64 %2, ptr %3, i64 %4, i64 0, i64 0, i32 1, ptr %why)
  ret i64 %call
}

; Function Attrs: noinline nounwind
define internal i64 @netrt_dial(ptr %name, i64 %len, i64 %port, ptr %bound, i64 %bound_len, i64 %conn, i64 %start, i32 %kind, ptr %why) {
entry:
  %retval = alloca i64, align 8
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %port.addr = alloca i64, align 8
  %bound.addr = alloca ptr, align 8
  %bound_len.addr = alloca i64, align 8
  %conn.addr = alloca i64, align 8
  %start.addr = alloca i64, align 8
  %kind.addr = alloca i32, align 4
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
  store i32 %kind, ptr %kind.addr, align 4
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
  %11 = load i32, ptr %kind.addr, align 4
  %arraydecay3 = getelementptr inbounds [48 x i8], ptr %hints, i64 0, i64 0
  %add.ptr4 = getelementptr inbounds i8, ptr %arraydecay3, i64 8
  store i32 %11, ptr %add.ptr4, align 4
  store ptr null, ptr %res, align 8
  %12 = load ptr, ptr %why.addr, align 8
  store i64 0, ptr %12, align 8
  %arraydecay5 = getelementptr inbounds [256 x i8], ptr %host, i64 0, i64 0
  %arraydecay6 = getelementptr inbounds [48 x i8], ptr %hints, i64 0, i64 0
  %call7 = call i32 @getaddrinfo(ptr %arraydecay5, ptr null, ptr %arraydecay6, ptr %res)
  %cmp8 = icmp ne i32 %call7, 0
  br i1 %cmp8, label %if.then, label %if.end

if.then:                                          ; preds = %for.end
  %13 = load ptr, ptr %why.addr, align 8
  store i64 -1, ptr %13, align 8
  store i64 -1, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %for.end
  %14 = load ptr, ptr %res, align 8
  %add.ptr9 = getelementptr inbounds i8, ptr %14, i64 32
  %15 = load ptr, ptr %add.ptr9, align 8
  store ptr %15, ptr %addr, align 8
  %16 = load ptr, ptr %res, align 8
  %add.ptr10 = getelementptr inbounds i8, ptr %16, i64 16
  %17 = load i32, ptr %add.ptr10, align 4
  store i32 %17, ptr %addrlen, align 4
  %18 = load i64, ptr %port.addr, align 8
  %shr = ashr i64 %18, 8
  %conv = trunc i64 %shr to i8
  %19 = load ptr, ptr %addr, align 8
  %arrayidx11 = getelementptr inbounds i8, ptr %19, i64 2
  store i8 %conv, ptr %arrayidx11, align 1
  %20 = load i64, ptr %port.addr, align 8
  %conv12 = trunc i64 %20 to i8
  %21 = load ptr, ptr %addr, align 8
  %arrayidx13 = getelementptr inbounds i8, ptr %21, i64 3
  store i8 %conv12, ptr %arrayidx13, align 1
  %22 = load i32, ptr %kind.addr, align 4
  %call14 = call i32 @netrt_tcp_socket(i32 %22)
  store i32 %call14, ptr %fd, align 4
  %23 = load i32, ptr %fd, align 4
  %cmp15 = icmp slt i32 %23, 0
  br i1 %cmp15, label %if.then17, label %if.end20

if.then17:                                        ; preds = %if.end
  %call18 = call ptr @cancho_errno_location()
  %24 = load i32, ptr %call18, align 4
  %conv19 = sext i32 %24 to i64
  %25 = load ptr, ptr %why.addr, align 8
  store i64 %conv19, ptr %25, align 8
  %26 = load ptr, ptr %res, align 8
  call void @freeaddrinfo(ptr %26)
  store i64 -1, ptr %retval, align 8
  br label %return

if.end20:                                         ; preds = %if.end
  %27 = load i64, ptr %start.addr, align 8
  %tobool = icmp ne i64 %27, 0
  br i1 %tobool, label %if.then21, label %if.end24

if.then21:                                        ; preds = %if.end20
  %28 = load i32, ptr %fd, align 4
  %call22 = call i32 (i32, i32, ...) @fcntl(i32 %28, i32 3)
  store i32 %call22, ptr %flags, align 4
  %29 = load i32, ptr %fd, align 4
  %30 = load i32, ptr %flags, align 4
  %or = or i32 %30, 4
  %call23 = call i32 (i32, i32, ...) @fcntl(i32 %29, i32 4, i32 %or)
  br label %if.end24

if.end24:                                         ; preds = %if.then21, %if.end20
  %31 = load i32, ptr %fd, align 4
  %32 = load ptr, ptr %addr, align 8
  %33 = load i32, ptr %addrlen, align 4
  %call25 = call i32 @connect(i32 %31, ptr %32, i32 %33)
  store i32 %call25, ptr %r, align 4
  %call26 = call ptr @cancho_errno_location()
  %34 = load i32, ptr %call26, align 4
  %conv27 = sext i32 %34 to i64
  store i64 %conv27, ptr %reason, align 8
  %35 = load i32, ptr %r, align 4
  %cmp28 = icmp eq i32 %35, 0
  br i1 %cmp28, label %lor.end, label %lor.rhs

lor.rhs:                                          ; preds = %if.end24
  %36 = load i64, ptr %start.addr, align 8
  %tobool30 = icmp ne i64 %36, 0
  br i1 %tobool30, label %land.lhs.true, label %land.end

land.lhs.true:                                    ; preds = %lor.rhs
  %37 = load i32, ptr %r, align 4
  %cmp31 = icmp slt i32 %37, 0
  br i1 %cmp31, label %land.rhs, label %land.end

land.rhs:                                         ; preds = %land.lhs.true
  %38 = load i64, ptr %reason, align 8
  %cmp33 = icmp eq i64 %38, 36
  br label %land.end

land.end:                                         ; preds = %land.rhs, %land.lhs.true, %lor.rhs
  %39 = phi i1 [ false, %land.lhs.true ], [ false, %lor.rhs ], [ %cmp33, %land.rhs ]
  br label %lor.end

lor.end:                                          ; preds = %land.end, %if.end24
  %40 = phi i1 [ true, %if.end24 ], [ %39, %land.end ]
  %lor.ext = zext i1 %40 to i32
  store i32 %lor.ext, ptr %ok, align 4
  %41 = load ptr, ptr %res, align 8
  call void @freeaddrinfo(ptr %41)
  %42 = load i32, ptr %ok, align 4
  %tobool35 = icmp ne i32 %42, 0
  br i1 %tobool35, label %if.then36, label %if.end41

if.then36:                                        ; preds = %lor.end
  %43 = load i64, ptr %conn.addr, align 8
  %tobool37 = icmp ne i64 %43, 0
  br i1 %tobool37, label %if.then38, label %if.end39

if.then38:                                        ; preds = %if.then36
  %44 = load i32, ptr %fd, align 4
  call void @netrt_int_option(i32 %44, i32 65535, i32 4130)
  br label %if.end39

if.end39:                                         ; preds = %if.then38, %if.then36
  %45 = load i32, ptr %fd, align 4
  %conv40 = sext i32 %45 to i64
  store i64 %conv40, ptr %retval, align 8
  br label %return

if.end41:                                         ; preds = %lor.end
  %46 = load i64, ptr %reason, align 8
  %47 = load ptr, ptr %why.addr, align 8
  store i64 %46, ptr %47, align 8
  %48 = load i32, ptr %fd, align 4
  %call42 = call i32 @close(i32 %48)
  store i64 -1, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end41, %if.end39, %if.then17, %if.then
  %49 = load i64, ptr %retval, align 8
  ret i64 %49
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
  %call = call i64 @netrt_dial(ptr %0, i64 %1, i64 %2, ptr %3, i64 %4, i64 1, i64 0, i32 1, ptr %why)
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
define void @cancho_net_udp_connect(ptr %name, i64 %len, i64 %port, ptr %bound, i64 %bound_len, ptr %out) {
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
  %call = call i64 @netrt_dial(ptr %0, i64 %1, i64 %2, ptr %3, i64 %4, i64 0, i64 0, i32 2, ptr %why)
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
define void @cancho_net_udp_bind(i64 %port, i64 %flags, ptr %bound, i64 %bound_len, ptr %out) {
entry:
  %port.addr = alloca i64, align 8
  %flags.addr = alloca i64, align 8
  %bound.addr = alloca ptr, align 8
  %bound_len.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %a = alloca [16 x i8], align 1
  %fd = alloca i32, align 4
  store i64 %port, ptr %port.addr, align 8
  store i64 %flags, ptr %flags.addr, align 8
  store ptr %bound, ptr %bound.addr, align 8
  store i64 %bound_len, ptr %bound_len.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load i64, ptr %port.addr, align 8
  %1 = load ptr, ptr %bound.addr, align 8
  %2 = load i64, ptr %bound_len.addr, align 8
  %call = call i64 @netrt_listen_port(ptr %1, i64 %2)
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
  %call3 = call i32 @netrt_tcp_socket(i32 2)
  store i32 %call3, ptr %fd, align 4
  %7 = load i32, ptr %fd, align 4
  %cmp = icmp slt i32 %7, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %call4 = call ptr @cancho_errno_location()
  %8 = load i32, ptr %call4, align 4
  %conv = sext i32 %8 to i64
  %9 = load ptr, ptr %out.addr, align 8
  %arrayidx5 = getelementptr inbounds i64, ptr %9, i64 2
  store i64 %conv, ptr %arrayidx5, align 8
  br label %return

if.end:                                           ; preds = %entry
  %10 = load i32, ptr %fd, align 4
  call void @netrt_int_option(i32 %10, i32 65535, i32 4)
  %11 = load i64, ptr %flags.addr, align 8
  %and = and i64 %11, 1
  %tobool = icmp ne i64 %and, 0
  br i1 %tobool, label %if.then6, label %if.end7

if.then6:                                         ; preds = %if.end
  %12 = load i32, ptr %fd, align 4
  call void @netrt_int_option(i32 %12, i32 65535, i32 512)
  br label %if.end7

if.end7:                                          ; preds = %if.then6, %if.end
  %13 = load i32, ptr %fd, align 4
  %arraydecay8 = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 0
  %call9 = call i32 @bind(i32 %13, ptr %arraydecay8, i32 16)
  %cmp10 = icmp ne i32 %call9, 0
  br i1 %cmp10, label %if.then12, label %if.end17

if.then12:                                        ; preds = %if.end7
  %call13 = call ptr @cancho_errno_location()
  %14 = load i32, ptr %call13, align 4
  %conv14 = sext i32 %14 to i64
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx15 = getelementptr inbounds i64, ptr %15, i64 2
  store i64 %conv14, ptr %arrayidx15, align 8
  %16 = load i32, ptr %fd, align 4
  %call16 = call i32 @close(i32 %16)
  br label %return

if.end17:                                         ; preds = %if.end7
  %17 = load ptr, ptr %out.addr, align 8
  %arrayidx18 = getelementptr inbounds i64, ptr %17, i64 0
  store i64 0, ptr %arrayidx18, align 8
  %18 = load i32, ptr %fd, align 4
  %conv19 = sext i32 %18 to i64
  %19 = load ptr, ptr %out.addr, align 8
  %arrayidx20 = getelementptr inbounds i64, ptr %19, i64 1
  store i64 %conv19, ptr %arrayidx20, align 8
  br label %return

return:                                           ; preds = %if.end17, %if.then12, %if.then
  ret void
}

; Function Attrs: noinline nounwind
define void @cancho_udp_recv(ptr %udp, ptr %p, i64 %room, ptr %out) {
entry:
  %udp.addr = alloca ptr, align 8
  %p.addr = alloca ptr, align 8
  %room.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %moved = alloca i64, align 8
  %reason = alloca i64, align 8
  store ptr %udp, ptr %udp.addr, align 8
  store ptr %p, ptr %p.addr, align 8
  store i64 %room, ptr %room.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  store i64 -1, ptr %moved, align 8
  store i64 22, ptr %reason, align 8
  %0 = load i64, ptr %room.addr, align 8
  %cmp = icmp ne i64 %0, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %1 = load ptr, ptr %udp.addr, align 8
  %2 = load i64, ptr %1, align 8
  %conv = trunc i64 %2 to i32
  %3 = load ptr, ptr %p.addr, align 8
  %4 = load i64, ptr %room.addr, align 8
  %call = call i64 @recv(i32 %conv, ptr %3, i64 %4, i32 0)
  store i64 %call, ptr %moved, align 8
  %call1 = call ptr @cancho_errno_location()
  %5 = load i32, ptr %call1, align 4
  %conv2 = sext i32 %5 to i64
  store i64 %conv2, ptr %reason, align 8
  br label %if.end

if.end:                                           ; preds = %if.then, %entry
  %6 = load i64, ptr %moved, align 8
  %7 = load i64, ptr %reason, align 8
  %8 = load i64, ptr %room.addr, align 8
  %9 = load ptr, ptr %out.addr, align 8
  call void @netrt_datagram(i64 %6, i64 %7, i64 %8, ptr %9)
  ret void
}

; Function Attrs: noinline nounwind
define internal void @netrt_datagram(i64 %moved, i64 %reason, i64 %room, ptr %out) {
entry:
  %moved.addr = alloca i64, align 8
  %reason.addr = alloca i64, align 8
  %room.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %tag = alloca i64, align 8
  store i64 %moved, ptr %moved.addr, align 8
  store i64 %reason, ptr %reason.addr, align 8
  store i64 %room, ptr %room.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load i64, ptr %moved.addr, align 8
  %cmp = icmp slt i64 %0, 0
  br i1 %cmp, label %if.then, label %if.else

if.then:                                          ; preds = %entry
  %1 = load i64, ptr %reason.addr, align 8
  %cmp1 = icmp eq i64 %1, 35
  %2 = zext i1 %cmp1 to i64
  %cond = select i1 %cmp1, i32 2, i32 3
  %conv = sext i32 %cond to i64
  store i64 %conv, ptr %tag, align 8
  br label %if.end

if.else:                                          ; preds = %entry
  %3 = load i64, ptr %moved.addr, align 8
  %4 = load i64, ptr %room.addr, align 8
  %cmp2 = icmp sge i64 %3, %4
  %5 = zext i1 %cmp2 to i64
  %cond4 = select i1 %cmp2, i32 1, i32 0
  %conv5 = sext i32 %cond4 to i64
  store i64 %conv5, ptr %tag, align 8
  br label %if.end

if.end:                                           ; preds = %if.else, %if.then
  %6 = load i64, ptr %tag, align 8
  %7 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %7, i64 0
  store i64 %6, ptr %arrayidx, align 8
  %8 = load i64, ptr %moved.addr, align 8
  %9 = load ptr, ptr %out.addr, align 8
  %arrayidx6 = getelementptr inbounds i64, ptr %9, i64 1
  store i64 %8, ptr %arrayidx6, align 8
  %10 = load i64, ptr %moved.addr, align 8
  %11 = load ptr, ptr %out.addr, align 8
  %arrayidx7 = getelementptr inbounds i64, ptr %11, i64 2
  store i64 %10, ptr %arrayidx7, align 8
  %12 = load i64, ptr %reason.addr, align 8
  %13 = load ptr, ptr %out.addr, align 8
  %arrayidx8 = getelementptr inbounds i64, ptr %13, i64 3
  store i64 %12, ptr %arrayidx8, align 8
  ret void
}

; Function Attrs: noinline nounwind
define void @cancho_udp_recv_from(ptr %udp, ptr %p, i64 %room, ptr %cells, i64 %ncells, ptr %out) {
entry:
  %udp.addr = alloca ptr, align 8
  %p.addr = alloca ptr, align 8
  %room.addr = alloca i64, align 8
  %cells.addr = alloca ptr, align 8
  %ncells.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %fd = alloca i32, align 4
  %ticket = alloca i64, align 8
  %entry1 = alloca ptr, align 8
  %moved = alloca i64, align 8
  %reason = alloca i64, align 8
  %len = alloca i32, align 4
  store ptr %udp, ptr %udp.addr, align 8
  store ptr %p, ptr %p.addr, align 8
  store i64 %room, ptr %room.addr, align 8
  store ptr %cells, ptr %cells.addr, align 8
  store i64 %ncells, ptr %ncells.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %udp.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  store i32 %conv, ptr %fd, align 4
  %2 = load i64, ptr getelementptr inbounds (i8, ptr @lexs_udp_peers, i64 2097152), align 8
  %add = add i64 %2, 1
  store i64 %add, ptr %ticket, align 8
  %3 = load i64, ptr %ticket, align 8
  %and = and i64 %3, 65535
  %mul = mul i64 %and, 32
  %add.ptr = getelementptr inbounds i8, ptr @lexs_udp_peers, i64 %mul
  store ptr %add.ptr, ptr %entry1, align 8
  store i64 -1, ptr %moved, align 8
  store i64 22, ptr %reason, align 8
  %4 = load i64, ptr %room.addr, align 8
  %cmp = icmp ne i64 %4, 0
  br i1 %cmp, label %land.lhs.true, label %if.end

land.lhs.true:                                    ; preds = %entry
  %5 = load i64, ptr %ncells.addr, align 8
  %cmp3 = icmp ne i64 %5, 0
  br i1 %cmp3, label %if.then, label %if.end

if.then:                                          ; preds = %land.lhs.true
  store i32 16, ptr %len, align 4
  %6 = load i32, ptr %fd, align 4
  %7 = load ptr, ptr %p.addr, align 8
  %8 = load i64, ptr %room.addr, align 8
  %9 = load ptr, ptr %entry1, align 8
  %call = call i64 @recvfrom(i32 %6, ptr %7, i64 %8, i32 0, ptr %9, ptr %len)
  store i64 %call, ptr %moved, align 8
  %call5 = call ptr @cancho_errno_location()
  %10 = load i32, ptr %call5, align 4
  %conv6 = sext i32 %10 to i64
  store i64 %conv6, ptr %reason, align 8
  br label %if.end

if.end:                                           ; preds = %if.then, %land.lhs.true, %entry
  %11 = load i64, ptr %moved, align 8
  %cmp7 = icmp sge i64 %11, 0
  br i1 %cmp7, label %if.then9, label %if.end12

if.then9:                                         ; preds = %if.end
  %12 = load i64, ptr %ticket, align 8
  %13 = load ptr, ptr %entry1, align 8
  %add.ptr10 = getelementptr inbounds i8, ptr %13, i64 16
  store i64 %12, ptr %add.ptr10, align 8
  %14 = load i32, ptr %fd, align 4
  %15 = load ptr, ptr %entry1, align 8
  %add.ptr11 = getelementptr inbounds i8, ptr %15, i64 24
  store i32 %14, ptr %add.ptr11, align 4
  %16 = load i64, ptr %ticket, align 8
  store i64 %16, ptr getelementptr inbounds (i8, ptr @lexs_udp_peers, i64 2097152), align 8
  %17 = load i64, ptr %ticket, align 8
  %18 = load ptr, ptr %cells.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %18, i64 0
  store i64 %17, ptr %arrayidx, align 8
  br label %if.end12

if.end12:                                         ; preds = %if.then9, %if.end
  %19 = load i64, ptr %moved, align 8
  %20 = load i64, ptr %reason, align 8
  %21 = load i64, ptr %room.addr, align 8
  %22 = load ptr, ptr %out.addr, align 8
  call void @netrt_datagram(i64 %19, i64 %20, i64 %21, ptr %22)
  ret void
}

declare i64 @recvfrom(i32, ptr, i64, i32, ptr, ptr)

; Function Attrs: noinline nounwind
define void @cancho_udp_send_to(ptr %udp, ptr %p, i64 %n, i64 %ticket, ptr %out) {
entry:
  %udp.addr = alloca ptr, align 8
  %p.addr = alloca ptr, align 8
  %n.addr = alloca i64, align 8
  %ticket.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %fd = alloca i32, align 4
  %entry1 = alloca ptr, align 8
  %moved = alloca i64, align 8
  %reason = alloca i64, align 8
  store ptr %udp, ptr %udp.addr, align 8
  store ptr %p, ptr %p.addr, align 8
  store i64 %n, ptr %n.addr, align 8
  store i64 %ticket, ptr %ticket.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %udp.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  store i32 %conv, ptr %fd, align 4
  %2 = load i64, ptr %ticket.addr, align 8
  %and = and i64 %2, 65535
  %mul = mul i64 %and, 32
  %add.ptr = getelementptr inbounds i8, ptr @lexs_udp_peers, i64 %mul
  store ptr %add.ptr, ptr %entry1, align 8
  store i64 -1, ptr %moved, align 8
  store i64 9, ptr %reason, align 8
  %3 = load i32, ptr %fd, align 4
  %4 = load i64, ptr %ticket.addr, align 8
  %5 = load ptr, ptr %entry1, align 8
  %call = call i32 @netrt_ticket_ok(i32 %3, i64 %4, ptr %5)
  %tobool = icmp ne i32 %call, 0
  br i1 %tobool, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %6 = load i32, ptr %fd, align 4
  %7 = load ptr, ptr %p.addr, align 8
  %8 = load i64, ptr %n.addr, align 8
  %9 = load ptr, ptr %entry1, align 8
  %call2 = call i64 @sendto(i32 %6, ptr %7, i64 %8, i32 0, ptr %9, i32 16)
  store i64 %call2, ptr %moved, align 8
  %call3 = call ptr @cancho_errno_location()
  %10 = load i32, ptr %call3, align 4
  %conv4 = sext i32 %10 to i64
  store i64 %conv4, ptr %reason, align 8
  br label %if.end

if.end:                                           ; preds = %if.then, %entry
  %11 = load i64, ptr %moved, align 8
  %cmp = icmp slt i64 %11, 0
  br i1 %cmp, label %cond.true, label %cond.false

cond.true:                                        ; preds = %if.end
  %12 = load i64, ptr %reason, align 8
  %cmp6 = icmp eq i64 %12, 35
  %13 = zext i1 %cmp6 to i64
  %cond = select i1 %cmp6, i32 1, i32 2
  br label %cond.end

cond.false:                                       ; preds = %if.end
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond8 = phi i32 [ %cond, %cond.true ], [ 0, %cond.false ]
  %conv9 = sext i32 %cond8 to i64
  %14 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %14, i64 0
  store i64 %conv9, ptr %arrayidx, align 8
  %15 = load i64, ptr %moved, align 8
  %16 = load ptr, ptr %out.addr, align 8
  %arrayidx10 = getelementptr inbounds i64, ptr %16, i64 1
  store i64 %15, ptr %arrayidx10, align 8
  %17 = load i64, ptr %reason, align 8
  %18 = load ptr, ptr %out.addr, align 8
  %arrayidx11 = getelementptr inbounds i64, ptr %18, i64 2
  store i64 %17, ptr %arrayidx11, align 8
  ret void
}

; Function Attrs: noinline nounwind
define internal i32 @netrt_ticket_ok(i32 %fd, i64 %ticket, ptr %entry1) {
entry:
  %fd.addr = alloca i32, align 4
  %ticket.addr = alloca i64, align 8
  %entry.addr = alloca ptr, align 8
  store i32 %fd, ptr %fd.addr, align 4
  store i64 %ticket, ptr %ticket.addr, align 8
  store ptr %entry1, ptr %entry.addr, align 8
  %0 = load i64, ptr %ticket.addr, align 8
  %cmp = icmp sgt i64 %0, 0
  br i1 %cmp, label %land.lhs.true, label %land.end

land.lhs.true:                                    ; preds = %entry
  %1 = load ptr, ptr %entry.addr, align 8
  %add.ptr = getelementptr inbounds i8, ptr %1, i64 16
  %2 = load i64, ptr %add.ptr, align 8
  %3 = load i64, ptr %ticket.addr, align 8
  %cmp2 = icmp eq i64 %2, %3
  br i1 %cmp2, label %land.rhs, label %land.end

land.rhs:                                         ; preds = %land.lhs.true
  %4 = load ptr, ptr %entry.addr, align 8
  %add.ptr3 = getelementptr inbounds i8, ptr %4, i64 24
  %5 = load i32, ptr %add.ptr3, align 4
  %6 = load i32, ptr %fd.addr, align 4
  %cmp4 = icmp eq i32 %5, %6
  br label %land.end

land.end:                                         ; preds = %land.rhs, %land.lhs.true, %entry
  %7 = phi i1 [ false, %land.lhs.true ], [ false, %entry ], [ %cmp4, %land.rhs ]
  %land.ext = zext i1 %7 to i32
  ret i32 %land.ext
}

declare i64 @sendto(i32, ptr, i64, i32, ptr, i32)

; Function Attrs: noinline nounwind
define i64 @cancho_udp_peer(ptr %udp, i64 %ticket, ptr %out, i64 %room) {
entry:
  %retval = alloca i64, align 8
  %udp.addr = alloca ptr, align 8
  %ticket.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %room.addr = alloca i64, align 8
  %entry1 = alloca ptr, align 8
  %i = alloca i32, align 4
  %i11 = alloca i32, align 4
  store ptr %udp, ptr %udp.addr, align 8
  store i64 %ticket, ptr %ticket.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  store i64 %room, ptr %room.addr, align 8
  %0 = load i64, ptr %ticket.addr, align 8
  %and = and i64 %0, 65535
  %mul = mul i64 %and, 32
  %add.ptr = getelementptr inbounds i8, ptr @lexs_udp_peers, i64 %mul
  store ptr %add.ptr, ptr %entry1, align 8
  %1 = load ptr, ptr %udp.addr, align 8
  %2 = load i64, ptr %1, align 8
  %conv = trunc i64 %2 to i32
  %3 = load i64, ptr %ticket.addr, align 8
  %4 = load ptr, ptr %entry1, align 8
  %call = call i32 @netrt_ticket_ok(i32 %conv, i64 %3, ptr %4)
  %tobool = icmp ne i32 %call, 0
  br i1 %tobool, label %if.end, label %if.then

if.then:                                          ; preds = %entry
  store i64 9, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %entry
  %5 = load i64, ptr %room.addr, align 8
  %cmp = icmp slt i64 %5, 19
  br i1 %cmp, label %if.then3, label %if.end4

if.then3:                                         ; preds = %if.end
  store i64 22, ptr %retval, align 8
  br label %return

if.end4:                                          ; preds = %if.end
  %6 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i8, ptr %6, i64 0
  store i8 4, ptr %arrayidx, align 1
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %if.end4
  %7 = load i32, ptr %i, align 4
  %cmp5 = icmp slt i32 %7, 4
  br i1 %cmp5, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %8 = load ptr, ptr %entry1, align 8
  %9 = load i32, ptr %i, align 4
  %add = add i32 4, %9
  %idxprom = sext i32 %add to i64
  %arrayidx7 = getelementptr inbounds i8, ptr %8, i64 %idxprom
  %10 = load i8, ptr %arrayidx7, align 1
  %11 = load ptr, ptr %out.addr, align 8
  %12 = load i32, ptr %i, align 4
  %add8 = add i32 1, %12
  %idxprom9 = sext i32 %add8 to i64
  %arrayidx10 = getelementptr inbounds i8, ptr %11, i64 %idxprom9
  store i8 %10, ptr %arrayidx10, align 1
  br label %for.inc

for.inc:                                          ; preds = %for.body
  %13 = load i32, ptr %i, align 4
  %inc = add i32 %13, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  store i32 5, ptr %i11, align 4
  br label %for.cond12

for.cond12:                                       ; preds = %for.inc18, %for.end
  %14 = load i32, ptr %i11, align 4
  %cmp13 = icmp slt i32 %14, 17
  br i1 %cmp13, label %for.body15, label %for.end20

for.body15:                                       ; preds = %for.cond12
  %15 = load ptr, ptr %out.addr, align 8
  %16 = load i32, ptr %i11, align 4
  %idxprom16 = sext i32 %16 to i64
  %arrayidx17 = getelementptr inbounds i8, ptr %15, i64 %idxprom16
  store i8 0, ptr %arrayidx17, align 1
  br label %for.inc18

for.inc18:                                        ; preds = %for.body15
  %17 = load i32, ptr %i11, align 4
  %inc19 = add i32 %17, 1
  store i32 %inc19, ptr %i11, align 4
  br label %for.cond12

for.end20:                                        ; preds = %for.cond12
  %18 = load ptr, ptr %entry1, align 8
  %arrayidx21 = getelementptr inbounds i8, ptr %18, i64 2
  %19 = load i8, ptr %arrayidx21, align 1
  %20 = load ptr, ptr %out.addr, align 8
  %arrayidx22 = getelementptr inbounds i8, ptr %20, i64 17
  store i8 %19, ptr %arrayidx22, align 1
  %21 = load ptr, ptr %entry1, align 8
  %arrayidx23 = getelementptr inbounds i8, ptr %21, i64 3
  %22 = load i8, ptr %arrayidx23, align 1
  %23 = load ptr, ptr %out.addr, align 8
  %arrayidx24 = getelementptr inbounds i8, ptr %23, i64 18
  store i8 %22, ptr %arrayidx24, align 1
  store i64 0, ptr %retval, align 8
  br label %return

return:                                           ; preds = %for.end20, %if.then3, %if.then
  %24 = load i64, ptr %retval, align 8
  ret i64 %24
}

; Function Attrs: noinline nounwind
define i64 @cancho_udp_local_port(ptr %udp) {
entry:
  %retval = alloca i64, align 8
  %udp.addr = alloca ptr, align 8
  %a = alloca [16 x i8], align 1
  %len = alloca i32, align 4
  %r = alloca i32, align 4
  %why = alloca i64, align 8
  store ptr %udp, ptr %udp.addr, align 8
  store i32 16, ptr %len, align 4
  %0 = load ptr, ptr %udp.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %arraydecay = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 0
  %call = call i32 @getsockname(i32 %conv, ptr %arraydecay, ptr %len)
  store i32 %call, ptr %r, align 4
  %call1 = call ptr @cancho_errno_location()
  %2 = load i32, ptr %call1, align 4
  %conv2 = sext i32 %2 to i64
  store i64 %conv2, ptr %why, align 8
  %3 = load i32, ptr %r, align 4
  %cmp = icmp slt i32 %3, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %4 = load i64, ptr %why, align 8
  %sub = sub i64 0, %4
  store i64 %sub, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %entry
  %arrayidx = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 2
  %5 = load i8, ptr %arrayidx, align 1
  %conv4 = zext i8 %5 to i64
  %shl = shl i64 %conv4, 8
  %arrayidx5 = getelementptr inbounds [16 x i8], ptr %a, i64 0, i64 3
  %6 = load i8, ptr %arrayidx5, align 1
  %conv6 = zext i8 %6 to i64
  %or = or i64 %shl, %conv6
  store i64 %or, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end, %if.then
  %7 = load i64, ptr %retval, align 8
  ret i64 %7
}

declare i32 @getsockname(i32, ptr, ptr)

; Function Attrs: noinline nounwind
define void @cancho_poller_new(ptr %out) {
entry:
  %out.addr = alloca ptr, align 8
  %fd = alloca i32, align 4
  %why = alloca i64, align 8
  store ptr %out, ptr %out.addr, align 8
  %call = call i32 @kqueue()
  store i32 %call, ptr %fd, align 4
  %call1 = call ptr @cancho_errno_location()
  %0 = load i32, ptr %call1, align 4
  %conv = sext i32 %0 to i64
  store i64 %conv, ptr %why, align 8
  %1 = load i32, ptr %fd, align 4
  %cmp = icmp sge i32 %1, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %2 = load i32, ptr %fd, align 4
  %call3 = call i32 (i32, i32, ...) @fcntl(i32 %2, i32 2, i32 1)
  br label %if.end

if.end:                                           ; preds = %if.then, %entry
  %3 = load i32, ptr %fd, align 4
  %cmp4 = icmp slt i32 %3, 0
  %conv5 = zext i1 %cmp4 to i32
  %conv6 = sext i32 %conv5 to i64
  %4 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %4, i64 0
  store i64 %conv6, ptr %arrayidx, align 8
  %5 = load i32, ptr %fd, align 4
  %conv7 = sext i32 %5 to i64
  %6 = load ptr, ptr %out.addr, align 8
  %arrayidx8 = getelementptr inbounds i64, ptr %6, i64 1
  store i64 %conv7, ptr %arrayidx8, align 8
  %7 = load i64, ptr %why, align 8
  %8 = load ptr, ptr %out.addr, align 8
  %arrayidx9 = getelementptr inbounds i64, ptr %8, i64 2
  store i64 %7, ptr %arrayidx9, align 8
  ret void
}


; Function Attrs: noinline nounwind
define i64 @cancho_poller_ctl(ptr %poller, ptr %handle, i64 %token, i64 %events, i64 %modify, i64 %packed) {
entry:
  %retval = alloca i64, align 8
  %poller.addr = alloca ptr, align 8
  %handle.addr = alloca ptr, align 8
  %token.addr = alloca i64, align 8
  %events.addr = alloca i64, align 8
  %modify.addr = alloca i64, align 8
  %packed.addr = alloca i64, align 8
  %p = alloca i32, align 4
  %fd = alloca i32, align 4
  %wants_read = alloca i32, align 4
  %wants_write = alloca i32, align 4
  %r = alloca i32, align 4
  %read_why = alloca i64, align 8
  %w = alloca i32, align 4
  %write_why = alloca i64, align 8
  store ptr %poller, ptr %poller.addr, align 8
  store ptr %handle, ptr %handle.addr, align 8
  store i64 %token, ptr %token.addr, align 8
  store i64 %events, ptr %events.addr, align 8
  store i64 %modify, ptr %modify.addr, align 8
  store i64 %packed, ptr %packed.addr, align 8
  %0 = load ptr, ptr %poller.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  store i32 %conv, ptr %p, align 4
  %2 = load ptr, ptr %handle.addr, align 8
  %3 = load i64, ptr %2, align 8
  %conv1 = trunc i64 %3 to i32
  store i32 %conv1, ptr %fd, align 4
  %4 = load i64, ptr %events.addr, align 8
  %and = and i64 %4, 1
  %cmp = icmp ne i64 %and, 0
  %conv2 = zext i1 %cmp to i32
  store i32 %conv2, ptr %wants_read, align 4
  %5 = load i64, ptr %events.addr, align 8
  %and3 = and i64 %5, 2
  %cmp4 = icmp ne i64 %and3, 0
  %conv5 = zext i1 %cmp4 to i32
  store i32 %conv5, ptr %wants_write, align 4
  %6 = load i32, ptr %p, align 4
  %7 = load i32, ptr %fd, align 4
  %8 = load i32, ptr %wants_read, align 4
  %9 = load i64, ptr %token.addr, align 8
  %call = call i32 @netrt_kevent_change(i32 %6, i32 %7, i64 -1, i32 %8, i64 %9)
  store i32 %call, ptr %r, align 4
  %call6 = call ptr @cancho_errno_location()
  %10 = load i32, ptr %call6, align 4
  %conv7 = sext i32 %10 to i64
  store i64 %conv7, ptr %read_why, align 8
  %11 = load i32, ptr %p, align 4
  %12 = load i32, ptr %fd, align 4
  %13 = load i32, ptr %wants_write, align 4
  %14 = load i64, ptr %token.addr, align 8
  %call8 = call i32 @netrt_kevent_change(i32 %11, i32 %12, i64 -2, i32 %13, i64 %14)
  store i32 %call8, ptr %w, align 4
  %call9 = call ptr @cancho_errno_location()
  %15 = load i32, ptr %call9, align 4
  %conv10 = sext i32 %15 to i64
  store i64 %conv10, ptr %write_why, align 8
  %16 = load i32, ptr %r, align 4
  %cmp11 = icmp slt i32 %16, 0
  br i1 %cmp11, label %land.lhs.true, label %if.end

land.lhs.true:                                    ; preds = %entry
  %17 = load i32, ptr %wants_read, align 4
  %tobool = icmp ne i32 %17, 0
  br i1 %tobool, label %if.then, label %if.end

if.then:                                          ; preds = %land.lhs.true
  %18 = load i64, ptr %read_why, align 8
  store i64 %18, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %land.lhs.true, %entry
  %19 = load i32, ptr %w, align 4
  %cmp13 = icmp slt i32 %19, 0
  br i1 %cmp13, label %land.lhs.true15, label %if.end18

land.lhs.true15:                                  ; preds = %if.end
  %20 = load i32, ptr %wants_write, align 4
  %tobool16 = icmp ne i32 %20, 0
  br i1 %tobool16, label %if.then17, label %if.end18

if.then17:                                        ; preds = %land.lhs.true15
  %21 = load i64, ptr %write_why, align 8
  store i64 %21, ptr %retval, align 8
  br label %return

if.end18:                                         ; preds = %land.lhs.true15, %if.end
  store i64 0, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end18, %if.then17, %if.then
  %22 = load i64, ptr %retval, align 8
  ret i64 %22
}

; Function Attrs: noinline nounwind
define internal i32 @netrt_kevent_change(i32 %kq, i32 %fd, i64 %filter, i32 %add, i64 %token) {
entry:
  %kq.addr = alloca i32, align 4
  %fd.addr = alloca i32, align 4
  %filter.addr = alloca i64, align 8
  %add.addr = alloca i32, align 4
  %token.addr = alloca i64, align 8
  %kev = alloca [32 x i8], align 1
  %i = alloca i32, align 4
  store i32 %kq, ptr %kq.addr, align 4
  store i32 %fd, ptr %fd.addr, align 4
  store i64 %filter, ptr %filter.addr, align 8
  store i32 %add, ptr %add.addr, align 4
  store i64 %token, ptr %token.addr, align 8
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %0 = load i32, ptr %i, align 4
  %cmp = icmp slt i32 %0, 32
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %1 = load i32, ptr %i, align 4
  %idxprom = sext i32 %1 to i64
  %arrayidx = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 %idxprom
  store i8 0, ptr %arrayidx, align 1
  br label %for.inc

for.inc:                                          ; preds = %for.body
  %2 = load i32, ptr %i, align 4
  %inc = add i32 %2, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %3 = load i32, ptr %fd.addr, align 4
  %conv = sext i32 %3 to i64
  %arraydecay = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  store i64 %conv, ptr %arraydecay, align 1
  %4 = load i64, ptr %filter.addr, align 8
  %conv1 = trunc i64 %4 to i16
  %arraydecay2 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %add.ptr = getelementptr inbounds i8, ptr %arraydecay2, i64 8
  store i16 %conv1, ptr %add.ptr, align 2
  %5 = load i32, ptr %add.addr, align 4
  %tobool = icmp ne i32 %5, 0
  %6 = zext i1 %tobool to i64
  %cond = select i1 %tobool, i32 1, i32 2
  %conv3 = trunc i32 %cond to i16
  %arraydecay4 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %add.ptr5 = getelementptr inbounds i8, ptr %arraydecay4, i64 10
  store i16 %conv3, ptr %add.ptr5, align 2
  %7 = load i64, ptr %token.addr, align 8
  %arraydecay6 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %add.ptr7 = getelementptr inbounds i8, ptr %arraydecay6, i64 24
  store i64 %7, ptr %add.ptr7, align 8
  %8 = load i32, ptr %kq.addr, align 4
  %arraydecay8 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %call = call i32 @kevent(i32 %8, ptr %arraydecay8, i32 1, ptr null, i32 0, ptr null)
  ret i32 %call
}

; Function Attrs: noinline nounwind
define i64 @cancho_poller_add_child(ptr %poller, ptr %child, i64 %token, i64 %packed) {
entry:
  %poller.addr = alloca ptr, align 8
  %child.addr = alloca ptr, align 8
  %token.addr = alloca i64, align 8
  %packed.addr = alloca i64, align 8
  %p = alloca i32, align 4
  %word = alloca i64, align 8
  %pid = alloca i32, align 4
  %kev = alloca [32 x i8], align 1
  %i = alloca i32, align 4
  %r = alloca i32, align 4
  %answer = alloca i64, align 8
  store ptr %poller, ptr %poller.addr, align 8
  store ptr %child, ptr %child.addr, align 8
  store i64 %token, ptr %token.addr, align 8
  store i64 %packed, ptr %packed.addr, align 8
  %0 = load ptr, ptr %poller.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  store i32 %conv, ptr %p, align 4
  %2 = load ptr, ptr %child.addr, align 8
  %3 = load i64, ptr %2, align 8
  store i64 %3, ptr %word, align 8
  %4 = load i64, ptr %word, align 8
  %conv1 = trunc i64 %4 to i32
  store i32 %conv1, ptr %pid, align 4
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %5 = load i32, ptr %i, align 4
  %cmp = icmp slt i32 %5, 32
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %6 = load i32, ptr %i, align 4
  %idxprom = sext i32 %6 to i64
  %arrayidx = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 %idxprom
  store i8 0, ptr %arrayidx, align 1
  br label %for.inc

for.inc:                                          ; preds = %for.body
  %7 = load i32, ptr %i, align 4
  %inc = add i32 %7, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %8 = load i32, ptr %pid, align 4
  %conv3 = sext i32 %8 to i64
  %arraydecay = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  store i64 %conv3, ptr %arraydecay, align 1
  %arraydecay4 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %add.ptr = getelementptr inbounds i8, ptr %arraydecay4, i64 8
  store i16 -5, ptr %add.ptr, align 2
  %arraydecay5 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %add.ptr6 = getelementptr inbounds i8, ptr %arraydecay5, i64 10
  store i16 17, ptr %add.ptr6, align 2
  %arraydecay7 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %add.ptr8 = getelementptr inbounds i8, ptr %arraydecay7, i64 12
  store i32 -2147483648, ptr %add.ptr8, align 4
  %9 = load i64, ptr %token.addr, align 8
  %arraydecay9 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %add.ptr10 = getelementptr inbounds i8, ptr %arraydecay9, i64 24
  store i64 %9, ptr %add.ptr10, align 8
  %10 = load i32, ptr %p, align 4
  %arraydecay11 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %call = call i32 @kevent(i32 %10, ptr %arraydecay11, i32 1, ptr null, i32 0, ptr null)
  store i32 %call, ptr %r, align 4
  %11 = load i32, ptr %r, align 4
  %cmp12 = icmp slt i32 %11, 0
  br i1 %cmp12, label %cond.true, label %cond.false

cond.true:                                        ; preds = %for.end
  %call14 = call ptr @cancho_errno_location()
  %12 = load i32, ptr %call14, align 4
  br label %cond.end

cond.false:                                       ; preds = %for.end
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond = phi i32 [ %12, %cond.true ], [ 0, %cond.false ]
  %conv15 = sext i32 %cond to i64
  store i64 %conv15, ptr %answer, align 8
  %13 = load i64, ptr %answer, align 8
  %cmp16 = icmp eq i64 %13, 3
  br i1 %cmp16, label %if.then, label %if.end

if.then:                                          ; preds = %cond.end
  %arraydecay18 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %add.ptr19 = getelementptr inbounds i8, ptr %arraydecay18, i64 8
  store i16 -10, ptr %add.ptr19, align 2
  %arraydecay20 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %add.ptr21 = getelementptr inbounds i8, ptr %arraydecay20, i64 12
  store i32 16777216, ptr %add.ptr21, align 4
  %14 = load i32, ptr %p, align 4
  %arraydecay22 = getelementptr inbounds [32 x i8], ptr %kev, i64 0, i64 0
  %call23 = call i32 @kevent(i32 %14, ptr %arraydecay22, i32 1, ptr null, i32 0, ptr null)
  store i32 %call23, ptr %r, align 4
  %15 = load i32, ptr %r, align 4
  %cmp24 = icmp slt i32 %15, 0
  br i1 %cmp24, label %cond.true26, label %cond.false28

cond.true26:                                      ; preds = %if.then
  %call27 = call ptr @cancho_errno_location()
  %16 = load i32, ptr %call27, align 4
  br label %cond.end29

cond.false28:                                     ; preds = %if.then
  br label %cond.end29

cond.end29:                                       ; preds = %cond.false28, %cond.true26
  %cond30 = phi i32 [ %16, %cond.true26 ], [ 0, %cond.false28 ]
  %conv31 = sext i32 %cond30 to i64
  store i64 %conv31, ptr %answer, align 8
  br label %if.end

if.end:                                           ; preds = %cond.end29, %cond.end
  %17 = load i64, ptr %answer, align 8
  ret i64 %17
}


; Function Attrs: noinline nounwind
define i64 @cancho_poller_remove(ptr %poller, ptr %handle, i64 %packed) {
entry:
  %poller.addr = alloca ptr, align 8
  %handle.addr = alloca ptr, align 8
  %packed.addr = alloca i64, align 8
  %p = alloca i32, align 4
  %fd = alloca i32, align 4
  store ptr %poller, ptr %poller.addr, align 8
  store ptr %handle, ptr %handle.addr, align 8
  store i64 %packed, ptr %packed.addr, align 8
  %0 = load ptr, ptr %poller.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  store i32 %conv, ptr %p, align 4
  %2 = load ptr, ptr %handle.addr, align 8
  %3 = load i64, ptr %2, align 8
  %conv1 = trunc i64 %3 to i32
  store i32 %conv1, ptr %fd, align 4
  %4 = load i32, ptr %p, align 4
  %5 = load i32, ptr %fd, align 4
  %call = call i32 @netrt_kevent_change(i32 %4, i32 %5, i64 -1, i32 0, i64 0)
  %6 = load i32, ptr %p, align 4
  %7 = load i32, ptr %fd, align 4
  %call2 = call i32 @netrt_kevent_change(i32 %6, i32 %7, i64 -2, i32 0, i64 0)
  ret i64 0
}

; Function Attrs: noinline nounwind
define i64 @cancho_poller_wait(ptr %poller, ptr %pairs, i64 %length, i64 %timeout, i64 %packed) {
entry:
  %retval = alloca i64, align 8
  %poller.addr = alloca ptr, align 8
  %pairs.addr = alloca ptr, align 8
  %length.addr = alloca i64, align 8
  %timeout.addr = alloca i64, align 8
  %packed.addr = alloca i64, align 8
  %p = alloca i32, align 4
  %max = alloca i64, align 8
  %buf = alloca [2048 x i8], align 1
  %ts = alloca [2 x i64], align 8
  %got = alloca i32, align 4
  %i = alloca i32, align 4
  %token = alloca i64, align 8
  %events = alloca i64, align 8
  %e = alloca ptr, align 8
  %filter = alloca i16, align 2
  %flags = alloca i16, align 2
  %reads = alloca i32, align 4
  store ptr %poller, ptr %poller.addr, align 8
  store ptr %pairs, ptr %pairs.addr, align 8
  store i64 %length, ptr %length.addr, align 8
  store i64 %timeout, ptr %timeout.addr, align 8
  store i64 %packed, ptr %packed.addr, align 8
  %0 = load ptr, ptr %poller.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  store i32 %conv, ptr %p, align 4
  %2 = load i64, ptr %length.addr, align 8
  %div = sdiv i64 %2, 2
  store i64 %div, ptr %max, align 8
  %3 = load i64, ptr %max, align 8
  %cmp = icmp sgt i64 %3, 64
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  store i64 64, ptr %max, align 8
  br label %if.end

if.end:                                           ; preds = %if.then, %entry
  %4 = load i64, ptr %max, align 8
  %cmp2 = icmp eq i64 %4, 0
  br i1 %cmp2, label %if.then4, label %if.end5

if.then4:                                         ; preds = %if.end
  store i64 -22, ptr %retval, align 8
  br label %return

if.end5:                                          ; preds = %if.end
  %5 = load i64, ptr %timeout.addr, align 8
  %div6 = sdiv i64 %5, 1000
  %arrayidx = getelementptr inbounds [2 x i64], ptr %ts, i64 0, i64 0
  store i64 %div6, ptr %arrayidx, align 8
  %6 = load i64, ptr %timeout.addr, align 8
  %rem = srem i64 %6, 1000
  %mul = mul i64 %rem, 1000000
  %arrayidx7 = getelementptr inbounds [2 x i64], ptr %ts, i64 0, i64 1
  store i64 %mul, ptr %arrayidx7, align 8
  %7 = load i32, ptr %p, align 4
  %arraydecay = getelementptr inbounds [2048 x i8], ptr %buf, i64 0, i64 0
  %8 = load i64, ptr %max, align 8
  %conv8 = trunc i64 %8 to i32
  %9 = load i64, ptr %timeout.addr, align 8
  %cmp9 = icmp slt i64 %9, 0
  br i1 %cmp9, label %cond.true, label %cond.false

cond.true:                                        ; preds = %if.end5
  br label %cond.end

cond.false:                                       ; preds = %if.end5
  %arraydecay11 = getelementptr inbounds [2 x i64], ptr %ts, i64 0, i64 0
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond = phi ptr [ null, %cond.true ], [ %arraydecay11, %cond.false ]
  %call = call i32 @kevent(i32 %7, ptr null, i32 0, ptr %arraydecay, i32 %conv8, ptr %cond)
  store i32 %call, ptr %got, align 4
  %10 = load i32, ptr %got, align 4
  %cmp12 = icmp slt i32 %10, 0
  br i1 %cmp12, label %if.then14, label %if.end17

if.then14:                                        ; preds = %cond.end
  %call15 = call ptr @cancho_errno_location()
  %11 = load i32, ptr %call15, align 4
  %conv16 = sext i32 %11 to i64
  %sub = sub i64 0, %conv16
  store i64 %sub, ptr %retval, align 8
  br label %return

if.end17:                                         ; preds = %cond.end
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %if.end17
  %12 = load i32, ptr %i, align 4
  %13 = load i32, ptr %got, align 4
  %cmp18 = icmp slt i32 %12, %13
  br i1 %cmp18, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %arraydecay20 = getelementptr inbounds [2048 x i8], ptr %buf, i64 0, i64 0
  %14 = load i32, ptr %i, align 4
  %mul21 = mul i32 32, %14
  %idx.ext = sext i32 %mul21 to i64
  %add.ptr = getelementptr inbounds i8, ptr %arraydecay20, i64 %idx.ext
  store ptr %add.ptr, ptr %e, align 8
  %15 = load ptr, ptr %e, align 8
  %add.ptr22 = getelementptr inbounds i8, ptr %15, i64 8
  %16 = load i16, ptr %add.ptr22, align 2
  store i16 %16, ptr %filter, align 2
  %17 = load ptr, ptr %e, align 8
  %add.ptr23 = getelementptr inbounds i8, ptr %17, i64 10
  %18 = load i16, ptr %add.ptr23, align 2
  store i16 %18, ptr %flags, align 2
  %19 = load ptr, ptr %e, align 8
  %add.ptr24 = getelementptr inbounds i8, ptr %19, i64 24
  %20 = load i64, ptr %add.ptr24, align 8
  store i64 %20, ptr %token, align 8
  %21 = load i16, ptr %filter, align 2
  %conv25 = sext i16 %21 to i32
  %cmp26 = icmp eq i32 %conv25, -1
  br i1 %cmp26, label %lor.end, label %lor.lhs.false

lor.lhs.false:                                    ; preds = %for.body
  %22 = load i16, ptr %filter, align 2
  %conv28 = sext i16 %22 to i32
  %cmp29 = icmp eq i32 %conv28, -5
  br i1 %cmp29, label %lor.end, label %lor.lhs.false31

lor.lhs.false31:                                  ; preds = %lor.lhs.false
  %23 = load i16, ptr %filter, align 2
  %conv32 = sext i16 %23 to i32
  %cmp33 = icmp eq i32 %conv32, -10
  br i1 %cmp33, label %lor.end, label %lor.rhs

lor.rhs:                                          ; preds = %lor.lhs.false31
  %24 = load i16, ptr %flags, align 2
  %conv35 = zext i16 %24 to i32
  %and = and i32 %conv35, 16384
  %cmp36 = icmp ne i32 %and, 0
  br label %lor.end

lor.end:                                          ; preds = %lor.rhs, %lor.lhs.false31, %lor.lhs.false, %for.body
  %25 = phi i1 [ true, %lor.lhs.false31 ], [ true, %lor.lhs.false ], [ true, %for.body ], [ %cmp36, %lor.rhs ]
  %lor.ext = zext i1 %25 to i32
  store i32 %lor.ext, ptr %reads, align 4
  %26 = load i32, ptr %reads, align 4
  %tobool = icmp ne i32 %26, 0
  %27 = zext i1 %tobool to i64
  %cond38 = select i1 %tobool, i32 1, i32 0
  %28 = load i16, ptr %filter, align 2
  %conv39 = sext i16 %28 to i32
  %cmp40 = icmp eq i32 %conv39, -2
  %29 = zext i1 %cmp40 to i64
  %cond42 = select i1 %cmp40, i32 2, i32 0
  %or = or i32 %cond38, %cond42
  %conv43 = sext i32 %or to i64
  store i64 %conv43, ptr %events, align 8
  %30 = load i64, ptr %token, align 8
  %31 = load ptr, ptr %pairs.addr, align 8
  %32 = load i32, ptr %i, align 4
  %mul44 = mul i32 2, %32
  %idxprom = sext i32 %mul44 to i64
  %arrayidx45 = getelementptr inbounds i64, ptr %31, i64 %idxprom
  store i64 %30, ptr %arrayidx45, align 8
  %33 = load i64, ptr %events, align 8
  %34 = load ptr, ptr %pairs.addr, align 8
  %35 = load i32, ptr %i, align 4
  %mul46 = mul i32 2, %35
  %add = add i32 %mul46, 1
  %idxprom47 = sext i32 %add to i64
  %arrayidx48 = getelementptr inbounds i64, ptr %34, i64 %idxprom47
  store i64 %33, ptr %arrayidx48, align 8
  br label %for.inc

for.inc:                                          ; preds = %lor.end
  %36 = load i32, ptr %i, align 4
  %inc = add i32 %36, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  %37 = load i32, ptr %got, align 4
  %conv49 = sext i32 %37 to i64
  store i64 %conv49, ptr %retval, align 8
  br label %return

return:                                           ; preds = %for.end, %if.then14, %if.then4
  %38 = load i64, ptr %retval, align 8
  ret i64 %38
}

; Function Attrs: noinline nounwind
define i64 @cancho_conn_detach(i64 %fd, i64 %udp) {
entry:
  %retval = alloca i64, align 8
  %fd.addr = alloca i64, align 8
  %udp.addr = alloca i64, align 8
  %next = alloca i32, align 4
  store i64 %fd, ptr %fd.addr, align 8
  store i64 %udp, ptr %udp.addr, align 8
  %0 = load i64, ptr %fd.addr, align 8
  %cmp = icmp uge i64 %0, 65536
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %1 = load i64, ptr %fd.addr, align 8
  %conv = trunc i64 %1 to i32
  %call = call i32 @close(i32 %conv)
  store i64 -1, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %entry
  %2 = load i64, ptr %fd.addr, align 8
  %arrayidx = getelementptr inbounds [65536 x i32], ptr @lexs_fd_epoch, i64 0, i64 %2
  %3 = load i32, ptr %arrayidx, align 4
  %add = add i32 %3, 1
  store i32 %add, ptr %next, align 4
  %4 = load i32, ptr %next, align 4
  %5 = load i64, ptr %fd.addr, align 8
  %arrayidx1 = getelementptr inbounds [65536 x i32], ptr @lexs_fd_epoch, i64 0, i64 %5
  store i32 %4, ptr %arrayidx1, align 4
  %6 = load i32, ptr %next, align 4
  %and = and i32 %6, 2147483647
  %conv2 = sext i32 %and to i64
  %shl = shl i64 %conv2, 32
  %7 = load i64, ptr %fd.addr, align 8
  %or = or i64 %shl, %7
  %8 = load i64, ptr %udp.addr, align 8
  %tobool = icmp ne i64 %8, 0
  %9 = zext i1 %tobool to i64
  %cond = select i1 %tobool, i64 2147483648, i64 0
  %or3 = or i64 %or, %cond
  store i64 %or3, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end, %if.then
  %10 = load i64, ptr %retval, align 8
  ret i64 %10
}

; Function Attrs: noinline nounwind
define void @cancho_conn_attach(i64 %ticket, i64 %udp, ptr %out) {
entry:
  %ticket.addr = alloca i64, align 8
  %udp.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %fd = alloca i64, align 8
  %epoch = alloca i64, align 8
  %in_range = alloca i32, align 4
  %index = alloca i64, align 8
  %current = alloca i32, align 4
  %valid = alloca i32, align 4
  store i64 %ticket, ptr %ticket.addr, align 8
  store i64 %udp, ptr %udp.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load i64, ptr %ticket.addr, align 8
  %1 = load i64, ptr %udp.addr, align 8
  %tobool = icmp ne i64 %1, 0
  %2 = zext i1 %tobool to i64
  %cond = select i1 %tobool, i64 2147483647, i64 4294967295
  %and = and i64 %0, %cond
  store i64 %and, ptr %fd, align 8
  %3 = load i64, ptr %ticket.addr, align 8
  %shr = lshr i64 %3, 32
  store i64 %shr, ptr %epoch, align 8
  %4 = load i64, ptr %fd, align 8
  %cmp = icmp ult i64 %4, 65536
  %conv = zext i1 %cmp to i32
  store i32 %conv, ptr %in_range, align 4
  %5 = load i32, ptr %in_range, align 4
  %tobool1 = icmp ne i32 %5, 0
  br i1 %tobool1, label %cond.true, label %cond.false

cond.true:                                        ; preds = %entry
  %6 = load i64, ptr %fd, align 8
  br label %cond.end

cond.false:                                       ; preds = %entry
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond2 = phi i64 [ %6, %cond.true ], [ 0, %cond.false ]
  store i64 %cond2, ptr %index, align 8
  %7 = load i64, ptr %index, align 8
  %arrayidx = getelementptr inbounds [65536 x i32], ptr @lexs_fd_epoch, i64 0, i64 %7
  %8 = load i32, ptr %arrayidx, align 4
  store i32 %8, ptr %current, align 4
  %9 = load i32, ptr %in_range, align 4
  %tobool3 = icmp ne i32 %9, 0
  br i1 %tobool3, label %land.lhs.true, label %land.end

land.lhs.true:                                    ; preds = %cond.end
  %10 = load i32, ptr %current, align 4
  %and4 = and i32 %10, 2147483647
  %conv5 = sext i32 %and4 to i64
  %11 = load i64, ptr %epoch, align 8
  %cmp6 = icmp eq i64 %conv5, %11
  br i1 %cmp6, label %land.lhs.true8, label %land.end

land.lhs.true8:                                   ; preds = %land.lhs.true
  %12 = load i64, ptr %epoch, align 8
  %and9 = and i64 %12, 1
  %tobool10 = icmp ne i64 %and9, 0
  br i1 %tobool10, label %land.rhs, label %land.end

land.rhs:                                         ; preds = %land.lhs.true8
  %13 = load i64, ptr %ticket.addr, align 8
  %cmp11 = icmp sge i64 %13, 0
  br label %land.end

land.end:                                         ; preds = %land.rhs, %land.lhs.true8, %land.lhs.true, %cond.end
  %14 = phi i1 [ false, %land.lhs.true8 ], [ false, %land.lhs.true ], [ false, %cond.end ], [ %cmp11, %land.rhs ]
  %land.ext = zext i1 %14 to i32
  store i32 %land.ext, ptr %valid, align 4
  %15 = load i64, ptr %udp.addr, align 8
  %tobool13 = icmp ne i64 %15, 0
  br i1 %tobool13, label %if.then, label %if.end

if.then:                                          ; preds = %land.end
  %16 = load i32, ptr %valid, align 4
  %tobool14 = icmp ne i32 %16, 0
  br i1 %tobool14, label %land.rhs15, label %land.end19

land.rhs15:                                       ; preds = %if.then
  %17 = load i64, ptr %ticket.addr, align 8
  %shr16 = lshr i64 %17, 31
  %and17 = and i64 %shr16, 1
  %tobool18 = icmp ne i64 %and17, 0
  br label %land.end19

land.end19:                                       ; preds = %land.rhs15, %if.then
  %18 = phi i1 [ false, %if.then ], [ %tobool18, %land.rhs15 ]
  %land.ext20 = zext i1 %18 to i32
  store i32 %land.ext20, ptr %valid, align 4
  br label %if.end

if.end:                                           ; preds = %land.end19, %land.end
  %19 = load i32, ptr %valid, align 4
  %tobool21 = icmp ne i32 %19, 0
  br i1 %tobool21, label %if.then22, label %if.end24

if.then22:                                        ; preds = %if.end
  %20 = load i32, ptr %current, align 4
  %add = add i32 %20, 1
  %21 = load i64, ptr %index, align 8
  %arrayidx23 = getelementptr inbounds [65536 x i32], ptr @lexs_fd_epoch, i64 0, i64 %21
  store i32 %add, ptr %arrayidx23, align 4
  br label %if.end24

if.end24:                                         ; preds = %if.then22, %if.end
  %22 = load i32, ptr %valid, align 4
  %tobool25 = icmp ne i32 %22, 0
  %23 = zext i1 %tobool25 to i64
  %cond26 = select i1 %tobool25, i32 0, i32 1
  %conv27 = sext i32 %cond26 to i64
  %24 = load ptr, ptr %out.addr, align 8
  %arrayidx28 = getelementptr inbounds i64, ptr %24, i64 0
  store i64 %conv27, ptr %arrayidx28, align 8
  %25 = load i64, ptr %fd, align 8
  %26 = load ptr, ptr %out.addr, align 8
  %arrayidx29 = getelementptr inbounds i64, ptr %26, i64 1
  store i64 %25, ptr %arrayidx29, align 8
  %27 = load ptr, ptr %out.addr, align 8
  %arrayidx30 = getelementptr inbounds i64, ptr %27, i64 2
  store i64 9, ptr %arrayidx30, align 8
  ret void
}

; Function Attrs: noinline nounwind
define void @cancho_net_tcp_connect_start(ptr %name, i64 %len, i64 %port, ptr %bound, i64 %bound_len, ptr %out) {
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
  %call = call i64 @netrt_dial(ptr %0, i64 %1, i64 %2, ptr %3, i64 %4, i64 1, i64 1, i32 1, ptr %why)
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
define i64 @cancho_conn_connect_status(ptr %conn) {
entry:
  %conn.addr = alloca ptr, align 8
  %value = alloca i32, align 4
  %length = alloca i32, align 4
  %r = alloca i32, align 4
  %why = alloca i64, align 8
  store ptr %conn, ptr %conn.addr, align 8
  store i32 0, ptr %value, align 4
  store i32 4, ptr %length, align 4
  %0 = load ptr, ptr %conn.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %call = call i32 @getsockopt(i32 %conv, i32 65535, i32 4103, ptr %value, ptr %length)
  store i32 %call, ptr %r, align 4
  %call1 = call ptr @cancho_errno_location()
  %2 = load i32, ptr %call1, align 4
  %conv2 = sext i32 %2 to i64
  store i64 %conv2, ptr %why, align 8
  %3 = load i32, ptr %r, align 4
  %cmp = icmp slt i32 %3, 0
  br i1 %cmp, label %cond.true, label %cond.false

cond.true:                                        ; preds = %entry
  %4 = load i64, ptr %why, align 8
  br label %cond.end

cond.false:                                       ; preds = %entry
  %5 = load i32, ptr %value, align 4
  %conv4 = sext i32 %5 to i64
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond = phi i64 [ %4, %cond.true ], [ %conv4, %cond.false ]
  ret i64 %cond
}

declare i32 @getsockopt(i32, i32, i32, ptr, ptr)

; Function Attrs: noinline nounwind
define i64 @cancho_conn_peer(ptr %conn, ptr %out, i64 %room) {
entry:
  %retval = alloca i64, align 8
  %conn.addr = alloca ptr, align 8
  %out.addr = alloca ptr, align 8
  %room.addr = alloca i64, align 8
  %sa = alloca [32 x i8], align 1
  %length = alloca i32, align 4
  %r = alloca i32, align 4
  %family = alloca i8, align 1
  %i = alloca i32, align 4
  %i27 = alloca i32, align 4
  store ptr %conn, ptr %conn.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  store i64 %room, ptr %room.addr, align 8
  %0 = load i64, ptr %room.addr, align 8
  %cmp = icmp slt i64 %0, 19
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  store i64 22, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %entry
  store i32 32, ptr %length, align 4
  %1 = load ptr, ptr %conn.addr, align 8
  %2 = load i64, ptr %1, align 8
  %conv = trunc i64 %2 to i32
  %arraydecay = getelementptr inbounds [32 x i8], ptr %sa, i64 0, i64 0
  %call = call i32 @getpeername(i32 %conv, ptr %arraydecay, ptr %length)
  store i32 %call, ptr %r, align 4
  %3 = load i32, ptr %r, align 4
  %cmp1 = icmp slt i32 %3, 0
  br i1 %cmp1, label %if.then3, label %if.end6

if.then3:                                         ; preds = %if.end
  %call4 = call ptr @cancho_errno_location()
  %4 = load i32, ptr %call4, align 4
  %conv5 = sext i32 %4 to i64
  store i64 %conv5, ptr %retval, align 8
  br label %return

if.end6:                                          ; preds = %if.end
  %arrayidx = getelementptr inbounds [32 x i8], ptr %sa, i64 0, i64 1
  %5 = load i8, ptr %arrayidx, align 1
  store i8 %5, ptr %family, align 1
  %6 = load i8, ptr %family, align 1
  %conv7 = zext i8 %6 to i32
  %cmp8 = icmp eq i32 %conv7, 2
  br i1 %cmp8, label %if.then10, label %if.else

if.then10:                                        ; preds = %if.end6
  %7 = load ptr, ptr %out.addr, align 8
  %arrayidx11 = getelementptr inbounds i8, ptr %7, i64 0
  store i8 4, ptr %arrayidx11, align 1
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %if.then10
  %8 = load i32, ptr %i, align 4
  %cmp12 = icmp slt i32 %8, 16
  br i1 %cmp12, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %9 = load i32, ptr %i, align 4
  %cmp14 = icmp slt i32 %9, 4
  br i1 %cmp14, label %cond.true, label %cond.false

cond.true:                                        ; preds = %for.body
  %10 = load i32, ptr %i, align 4
  %add = add i32 4, %10
  %idxprom = sext i32 %add to i64
  %arrayidx16 = getelementptr inbounds [32 x i8], ptr %sa, i64 0, i64 %idxprom
  %11 = load i8, ptr %arrayidx16, align 1
  %conv17 = zext i8 %11 to i32
  br label %cond.end

cond.false:                                       ; preds = %for.body
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond = phi i32 [ %conv17, %cond.true ], [ 0, %cond.false ]
  %conv18 = trunc i32 %cond to i8
  %12 = load ptr, ptr %out.addr, align 8
  %13 = load i32, ptr %i, align 4
  %add19 = add i32 1, %13
  %idxprom20 = sext i32 %add19 to i64
  %arrayidx21 = getelementptr inbounds i8, ptr %12, i64 %idxprom20
  store i8 %conv18, ptr %arrayidx21, align 1
  br label %for.inc

for.inc:                                          ; preds = %cond.end
  %14 = load i32, ptr %i, align 4
  %inc = add i32 %14, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  br label %if.end43

if.else:                                          ; preds = %if.end6
  %15 = load i8, ptr %family, align 1
  %conv22 = zext i8 %15 to i32
  %cmp23 = icmp eq i32 %conv22, 30
  br i1 %cmp23, label %if.then25, label %if.else41

if.then25:                                        ; preds = %if.else
  %16 = load ptr, ptr %out.addr, align 8
  %arrayidx26 = getelementptr inbounds i8, ptr %16, i64 0
  store i8 6, ptr %arrayidx26, align 1
  store i32 0, ptr %i27, align 4
  br label %for.cond28

for.cond28:                                       ; preds = %for.inc38, %if.then25
  %17 = load i32, ptr %i27, align 4
  %cmp29 = icmp slt i32 %17, 16
  br i1 %cmp29, label %for.body31, label %for.end40

for.body31:                                       ; preds = %for.cond28
  %18 = load i32, ptr %i27, align 4
  %add32 = add i32 8, %18
  %idxprom33 = sext i32 %add32 to i64
  %arrayidx34 = getelementptr inbounds [32 x i8], ptr %sa, i64 0, i64 %idxprom33
  %19 = load i8, ptr %arrayidx34, align 1
  %20 = load ptr, ptr %out.addr, align 8
  %21 = load i32, ptr %i27, align 4
  %add35 = add i32 1, %21
  %idxprom36 = sext i32 %add35 to i64
  %arrayidx37 = getelementptr inbounds i8, ptr %20, i64 %idxprom36
  store i8 %19, ptr %arrayidx37, align 1
  br label %for.inc38

for.inc38:                                        ; preds = %for.body31
  %22 = load i32, ptr %i27, align 4
  %inc39 = add i32 %22, 1
  store i32 %inc39, ptr %i27, align 4
  br label %for.cond28

for.end40:                                        ; preds = %for.cond28
  br label %if.end42

if.else41:                                        ; preds = %if.else
  store i64 22, ptr %retval, align 8
  br label %return

if.end42:                                         ; preds = %for.end40
  br label %if.end43

if.end43:                                         ; preds = %if.end42, %for.end
  %arrayidx44 = getelementptr inbounds [32 x i8], ptr %sa, i64 0, i64 2
  %23 = load i8, ptr %arrayidx44, align 1
  %24 = load ptr, ptr %out.addr, align 8
  %arrayidx45 = getelementptr inbounds i8, ptr %24, i64 17
  store i8 %23, ptr %arrayidx45, align 1
  %arrayidx46 = getelementptr inbounds [32 x i8], ptr %sa, i64 0, i64 3
  %25 = load i8, ptr %arrayidx46, align 1
  %26 = load ptr, ptr %out.addr, align 8
  %arrayidx47 = getelementptr inbounds i8, ptr %26, i64 18
  store i8 %25, ptr %arrayidx47, align 1
  store i64 0, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end43, %if.else41, %if.then3, %if.then
  %27 = load i64, ptr %retval, align 8
  ret i64 %27
}

declare i32 @getpeername(i32, ptr, ptr)

; Function Attrs: noinline nounwind
define void @cancho_flush_out(ptr %out) {
entry:
  %out.addr = alloca ptr, align 8
  %flushed = alloca i32, align 4
  %why = alloca i64, align 8
  %indicator = alloca i32, align 4
  %failed = alloca i32, align 4
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr @__stdoutp, align 8
  %call = call i32 @fflush(ptr %0)
  store i32 %call, ptr %flushed, align 4
  %call1 = call ptr @cancho_errno_location()
  %1 = load i32, ptr %call1, align 4
  %conv = sext i32 %1 to i64
  store i64 %conv, ptr %why, align 8
  %2 = load ptr, ptr @__stdoutp, align 8
  %call2 = call i32 @ferror(ptr %2)
  store i32 %call2, ptr %indicator, align 4
  %3 = load i32, ptr %flushed, align 4
  %cmp = icmp ne i32 %3, 0
  br i1 %cmp, label %lor.end, label %lor.rhs

lor.rhs:                                          ; preds = %entry
  %4 = load i32, ptr %indicator, align 4
  %cmp4 = icmp ne i32 %4, 0
  br label %lor.end

lor.end:                                          ; preds = %lor.rhs, %entry
  %5 = phi i1 [ true, %entry ], [ %cmp4, %lor.rhs ]
  %lor.ext = zext i1 %5 to i32
  store i32 %lor.ext, ptr %failed, align 4
  %6 = load i32, ptr %failed, align 4
  %conv6 = sext i32 %6 to i64
  %7 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %7, i64 0
  store i64 %conv6, ptr %arrayidx, align 8
  %8 = load ptr, ptr %out.addr, align 8
  %arrayidx7 = getelementptr inbounds i64, ptr %8, i64 1
  store i64 0, ptr %arrayidx7, align 8
  %9 = load i32, ptr %flushed, align 4
  %cmp8 = icmp ne i32 %9, 0
  br i1 %cmp8, label %cond.true, label %cond.false

cond.true:                                        ; preds = %lor.end
  %10 = load i64, ptr %why, align 8
  br label %cond.end

cond.false:                                       ; preds = %lor.end
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond = phi i64 [ %10, %cond.true ], [ 5, %cond.false ]
  %11 = load ptr, ptr %out.addr, align 8
  %arrayidx10 = getelementptr inbounds i64, ptr %11, i64 2
  store i64 %cond, ptr %arrayidx10, align 8
  ret void
}

declare i32 @fflush(ptr)

declare i32 @ferror(ptr)

; Function Attrs: noinline nounwind
define void @cancho_read_stdin(ptr %buf, i64 %length, ptr %out) {
entry:
  %buf.addr = alloca ptr, align 8
  %length.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %moved = alloca i64, align 8
  %why = alloca i64, align 8
  %failed = alloca i32, align 4
  %ended = alloca i64, align 8
  store ptr %buf, ptr %buf.addr, align 8
  store i64 %length, ptr %length.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %buf.addr, align 8
  %1 = load i64, ptr %length.addr, align 8
  %2 = load ptr, ptr @__stdinp, align 8
  %call = call i64 @fread(ptr %0, i64 1, i64 %1, ptr %2)
  store i64 %call, ptr %moved, align 8
  %call1 = call ptr @cancho_errno_location()
  %3 = load i32, ptr %call1, align 4
  %conv = sext i32 %3 to i64
  store i64 %conv, ptr %why, align 8
  %4 = load ptr, ptr @__stdinp, align 8
  %call2 = call i32 @ferror(ptr %4)
  %cmp = icmp ne i32 %call2, 0
  %conv3 = zext i1 %cmp to i32
  store i32 %conv3, ptr %failed, align 4
  %5 = load ptr, ptr @__stdinp, align 8
  call void @clearerr(ptr %5)
  %6 = load i64, ptr %length.addr, align 8
  %cmp4 = icmp ne i64 %6, 0
  %7 = zext i1 %cmp4 to i64
  %cond = select i1 %cmp4, i32 1, i32 0
  %conv6 = sext i32 %cond to i64
  store i64 %conv6, ptr %ended, align 8
  %8 = load i64, ptr %moved, align 8
  %cmp7 = icmp ne i64 %8, 0
  br i1 %cmp7, label %cond.true, label %cond.false

cond.true:                                        ; preds = %entry
  br label %cond.end12

cond.false:                                       ; preds = %entry
  %9 = load i32, ptr %failed, align 4
  %tobool = icmp ne i32 %9, 0
  br i1 %tobool, label %cond.true9, label %cond.false10

cond.true9:                                       ; preds = %cond.false
  br label %cond.end

cond.false10:                                     ; preds = %cond.false
  %10 = load i64, ptr %ended, align 8
  br label %cond.end

cond.end:                                         ; preds = %cond.false10, %cond.true9
  %cond11 = phi i64 [ 2, %cond.true9 ], [ %10, %cond.false10 ]
  br label %cond.end12

cond.end12:                                       ; preds = %cond.end, %cond.true
  %cond13 = phi i64 [ 0, %cond.true ], [ %cond11, %cond.end ]
  %11 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %11, i64 0
  store i64 %cond13, ptr %arrayidx, align 8
  %12 = load i64, ptr %moved, align 8
  %13 = load ptr, ptr %out.addr, align 8
  %arrayidx14 = getelementptr inbounds i64, ptr %13, i64 1
  store i64 %12, ptr %arrayidx14, align 8
  %14 = load i64, ptr %why, align 8
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx15 = getelementptr inbounds i64, ptr %15, i64 2
  store i64 %14, ptr %arrayidx15, align 8
  ret void
}

declare i64 @fread(ptr, i64, i64, ptr)

declare void @clearerr(ptr)

; Function Attrs: noinline nounwind
define i64 @cancho_clock_ms(i64 %wall) {
entry:
  %wall.addr = alloca i64, align 8
  %ts = alloca [2 x i64], align 8
  store i64 %wall, ptr %wall.addr, align 8
  %0 = load i64, ptr %wall.addr, align 8
  %tobool = icmp ne i64 %0, 0
  %1 = zext i1 %tobool to i64
  %cond = select i1 %tobool, i32 0, i32 6
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

declare i32 @accept(i32, ptr, ptr)

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



