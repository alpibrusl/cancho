

; Function Attrs: noinline nounwind
define void @cancho_dir_open(ptr %src, i64 %len, ptr %prefix, i64 %plen, ptr %out) {
entry:
  %src.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %prefix.addr = alloca ptr, align 8
  %plen.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %path = alloca ptr, align 8
  %fd = alloca i32, align 4
  %why = alloca i64, align 8
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
  %4 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 5), align 8
  %conv = trunc i64 %4 to i32
  %5 = load ptr, ptr %path, align 8
  %6 = load i64, ptr @cancho_plat, align 8
  %conv1 = trunc i64 %6 to i32
  %call2 = call i32 (i32, ptr, i32, ...) @openat(i32 %conv, ptr %5, i32 %conv1, i32 0)
  store i32 %call2, ptr %fd, align 4
  %call3 = call ptr @cancho_errno_location()
  %7 = load i32, ptr %call3, align 4
  %conv4 = sext i32 %7 to i64
  store i64 %conv4, ptr %why, align 8
  %8 = load ptr, ptr %path, align 8
  call void @free(ptr %8)
  %9 = load ptr, ptr %out.addr, align 8
  %10 = load i32, ptr %fd, align 4
  %conv5 = sext i32 %10 to i64
  %11 = load i64, ptr %why, align 8
  call void @dirrt_triple(ptr %9, i64 %conv5, i64 %11)
  ret void
}





; Function Attrs: noinline nounwind
define internal void @dirrt_triple(ptr %out, i64 %result, i64 %why) {
entry:
  %out.addr = alloca ptr, align 8
  %result.addr = alloca i64, align 8
  %why.addr = alloca i64, align 8
  store ptr %out, ptr %out.addr, align 8
  store i64 %result, ptr %result.addr, align 8
  store i64 %why, ptr %why.addr, align 8
  %0 = load i64, ptr %result.addr, align 8
  %cmp = icmp slt i64 %0, 0
  %conv = zext i1 %cmp to i32
  %conv1 = sext i32 %conv to i64
  %1 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %1, i64 0
  store i64 %conv1, ptr %arrayidx, align 8
  %2 = load i64, ptr %result.addr, align 8
  %3 = load ptr, ptr %out.addr, align 8
  %arrayidx2 = getelementptr inbounds i64, ptr %3, i64 1
  store i64 %2, ptr %arrayidx2, align 8
  %4 = load i64, ptr %why.addr, align 8
  %5 = load ptr, ptr %out.addr, align 8
  %arrayidx3 = getelementptr inbounds i64, ptr %5, i64 2
  store i64 %4, ptr %arrayidx3, align 8
  ret void
}

; Function Attrs: noinline nounwind
define void @cancho_dir_step(ptr %dir, ptr %name, i64 %len, i64 %which, ptr %out) {
entry:
  %dir.addr = alloca ptr, align 8
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %which.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %copy = alloca [256 x i8], align 1
  %flags = alloca i32, align 4
  %fd = alloca i32, align 4
  store ptr %dir, ptr %dir.addr, align 8
  store ptr %name, ptr %name.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store i64 %which, ptr %which.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %name.addr, align 8
  %1 = load i64, ptr %len.addr, align 8
  %arraydecay = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %call = call i32 @dirrt_component(ptr %0, i64 %1, ptr %arraydecay)
  %tobool = icmp ne i32 %call, 0
  br i1 %tobool, label %if.end, label %if.then

if.then:                                          ; preds = %entry
  %2 = load ptr, ptr %out.addr, align 8
  call void @dirrt_refused(ptr %2)
  br label %return

if.end:                                           ; preds = %entry
  %3 = load i64, ptr %which.addr, align 8
  %cmp = icmp eq i64 %3, 0
  br i1 %cmp, label %cond.true, label %cond.false

cond.true:                                        ; preds = %if.end
  br label %cond.end

cond.false:                                       ; preds = %if.end
  %4 = load i64, ptr %which.addr, align 8
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond = phi i64 [ 0, %cond.true ], [ %4, %cond.false ]
  %add = add nsw i64 1, %cond
  %arrayidx = getelementptr inbounds [0 x i64], ptr @cancho_plat, i64 0, i64 %add
  %5 = load i64, ptr %arrayidx, align 8
  %conv = trunc i64 %5 to i32
  store i32 %conv, ptr %flags, align 4
  %6 = load ptr, ptr %dir.addr, align 8
  %7 = load i64, ptr %6, align 8
  %conv1 = trunc i64 %7 to i32
  %arraydecay2 = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %8 = load i32, ptr %flags, align 4
  %9 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 20), align 8
  %conv3 = trunc i64 %9 to i32
  %call4 = call i32 (i32, ptr, i32, ...) @openat(i32 %conv1, ptr %arraydecay2, i32 %8, i32 %conv3)
  store i32 %call4, ptr %fd, align 4
  %10 = load ptr, ptr %out.addr, align 8
  %11 = load i32, ptr %fd, align 4
  %conv5 = sext i32 %11 to i64
  %call6 = call ptr @cancho_errno_location()
  %12 = load i32, ptr %call6, align 4
  %conv7 = sext i32 %12 to i64
  call void @dirrt_triple(ptr %10, i64 %conv5, i64 %conv7)
  br label %return

return:                                           ; preds = %cond.end, %if.then
  ret void
}

; Function Attrs: noinline nounwind
define internal i32 @dirrt_component(ptr %name, i64 %len, ptr %copy) {
entry:
  %retval = alloca i32, align 4
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %copy.addr = alloca ptr, align 8
  store ptr %name, ptr %name.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store ptr %copy, ptr %copy.addr, align 8
  %0 = load i64, ptr %len.addr, align 8
  %cmp = icmp eq i64 %0, 0
  br i1 %cmp, label %if.then, label %lor.lhs.false

lor.lhs.false:                                    ; preds = %entry
  %1 = load i64, ptr %len.addr, align 8
  %cmp1 = icmp sgt i64 %1, 255
  br i1 %cmp1, label %if.then, label %if.end

if.then:                                          ; preds = %lor.lhs.false, %entry
  store i32 0, ptr %retval, align 4
  br label %return

if.end:                                           ; preds = %lor.lhs.false
  %2 = load ptr, ptr %name.addr, align 8
  %3 = load i64, ptr %len.addr, align 8
  %call = call ptr @memchr(ptr %2, i32 47, i64 %3)
  %tobool = icmp ne ptr %call, null
  br i1 %tobool, label %if.then5, label %lor.lhs.false2

lor.lhs.false2:                                   ; preds = %if.end
  %4 = load ptr, ptr %name.addr, align 8
  %5 = load i64, ptr %len.addr, align 8
  %call3 = call ptr @memchr(ptr %4, i32 0, i64 %5)
  %tobool4 = icmp ne ptr %call3, null
  br i1 %tobool4, label %if.then5, label %if.end6

if.then5:                                         ; preds = %lor.lhs.false2, %if.end
  store i32 0, ptr %retval, align 4
  br label %return

if.end6:                                          ; preds = %lor.lhs.false2
  %6 = load ptr, ptr %name.addr, align 8
  %arrayidx = getelementptr inbounds i8, ptr %6, i64 0
  %7 = load i8, ptr %arrayidx, align 1
  %conv = zext i8 %7 to i32
  %cmp7 = icmp eq i32 %conv, 46
  br i1 %cmp7, label %land.lhs.true, label %if.end20

land.lhs.true:                                    ; preds = %if.end6
  %8 = load i64, ptr %len.addr, align 8
  %cmp9 = icmp eq i64 %8, 1
  br i1 %cmp9, label %if.then19, label %lor.lhs.false11

lor.lhs.false11:                                  ; preds = %land.lhs.true
  %9 = load i64, ptr %len.addr, align 8
  %cmp12 = icmp eq i64 %9, 2
  br i1 %cmp12, label %land.lhs.true14, label %if.end20

land.lhs.true14:                                  ; preds = %lor.lhs.false11
  %10 = load ptr, ptr %name.addr, align 8
  %arrayidx15 = getelementptr inbounds i8, ptr %10, i64 1
  %11 = load i8, ptr %arrayidx15, align 1
  %conv16 = zext i8 %11 to i32
  %cmp17 = icmp eq i32 %conv16, 46
  br i1 %cmp17, label %if.then19, label %if.end20

if.then19:                                        ; preds = %land.lhs.true14, %land.lhs.true
  store i32 0, ptr %retval, align 4
  br label %return

if.end20:                                         ; preds = %land.lhs.true14, %lor.lhs.false11, %if.end6
  %12 = load ptr, ptr %copy.addr, align 8
  %13 = load ptr, ptr %name.addr, align 8
  %14 = load i64, ptr %len.addr, align 8
  %call21 = call ptr @memmove(ptr %12, ptr %13, i64 %14)
  %15 = load ptr, ptr %copy.addr, align 8
  %16 = load i64, ptr %len.addr, align 8
  %arrayidx22 = getelementptr inbounds i8, ptr %15, i64 %16
  store i8 0, ptr %arrayidx22, align 1
  store i32 1, ptr %retval, align 4
  br label %return

return:                                           ; preds = %if.end20, %if.then19, %if.then5, %if.then
  %17 = load i32, ptr %retval, align 4
  ret i32 %17
}

; Function Attrs: noinline nounwind
define internal void @dirrt_refused(ptr %out) {
entry:
  %out.addr = alloca ptr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %out.addr, align 8
  call void @dirrt_triple(ptr %0, i64 -1, i64 22)
  ret void
}

; Function Attrs: noinline nounwind
define void @cancho_dir_rename(ptr %dir, ptr %from, i64 %flen, ptr %to, i64 %tlen, i64 %fresh, ptr %out) {
entry:
  %dir.addr = alloca ptr, align 8
  %from.addr = alloca ptr, align 8
  %flen.addr = alloca i64, align 8
  %to.addr = alloca ptr, align 8
  %tlen.addr = alloca i64, align 8
  %fresh.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %a = alloca [256 x i8], align 1
  %b = alloca [256 x i8], align 1
  %fd = alloca i32, align 4
  %r = alloca i32, align 4
  %why = alloca i64, align 8
  store ptr %dir, ptr %dir.addr, align 8
  store ptr %from, ptr %from.addr, align 8
  store i64 %flen, ptr %flen.addr, align 8
  store ptr %to, ptr %to.addr, align 8
  store i64 %tlen, ptr %tlen.addr, align 8
  store i64 %fresh, ptr %fresh.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %from.addr, align 8
  %1 = load i64, ptr %flen.addr, align 8
  %arraydecay = getelementptr inbounds [256 x i8], ptr %a, i64 0, i64 0
  %call = call i32 @dirrt_component(ptr %0, i64 %1, ptr %arraydecay)
  %tobool = icmp ne i32 %call, 0
  br i1 %tobool, label %lor.lhs.false, label %if.then

lor.lhs.false:                                    ; preds = %entry
  %2 = load ptr, ptr %to.addr, align 8
  %3 = load i64, ptr %tlen.addr, align 8
  %arraydecay1 = getelementptr inbounds [256 x i8], ptr %b, i64 0, i64 0
  %call2 = call i32 @dirrt_component(ptr %2, i64 %3, ptr %arraydecay1)
  %tobool3 = icmp ne i32 %call2, 0
  br i1 %tobool3, label %if.end, label %if.then

if.then:                                          ; preds = %lor.lhs.false, %entry
  %4 = load ptr, ptr %out.addr, align 8
  call void @dirrt_refused(ptr %4)
  br label %return

if.end:                                           ; preds = %lor.lhs.false
  %5 = load ptr, ptr %dir.addr, align 8
  %6 = load i64, ptr %5, align 8
  %conv = trunc i64 %6 to i32
  store i32 %conv, ptr %fd, align 4
  %7 = load i64, ptr %fresh.addr, align 8
  %tobool4 = icmp ne i64 %7, 0
  br i1 %tobool4, label %if.else, label %if.then5

if.then5:                                         ; preds = %if.end
  %8 = load i32, ptr %fd, align 4
  %arraydecay6 = getelementptr inbounds [256 x i8], ptr %a, i64 0, i64 0
  %9 = load i32, ptr %fd, align 4
  %arraydecay7 = getelementptr inbounds [256 x i8], ptr %b, i64 0, i64 0
  %call8 = call i32 @renameat(i32 %8, ptr %arraydecay6, i32 %9, ptr %arraydecay7)
  store i32 %call8, ptr %r, align 4
  br label %if.end13

if.else:                                          ; preds = %if.end
  %10 = load i32, ptr %fd, align 4
  %arraydecay9 = getelementptr inbounds [256 x i8], ptr %a, i64 0, i64 0
  %11 = load i32, ptr %fd, align 4
  %arraydecay10 = getelementptr inbounds [256 x i8], ptr %b, i64 0, i64 0
  %12 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 6), align 8
  %conv11 = trunc i64 %12 to i32
  %call12 = call i32 @renameat2(i32 %10, ptr %arraydecay9, i32 %11, ptr %arraydecay10, i32 %conv11)
  store i32 %call12, ptr %r, align 4
  br label %if.end13

if.end13:                                         ; preds = %if.else, %if.then5
  %call14 = call ptr @cancho_errno_location()
  %13 = load i32, ptr %call14, align 4
  %conv15 = sext i32 %13 to i64
  store i64 %conv15, ptr %why, align 8
  %14 = load i64, ptr %fresh.addr, align 8
  %tobool16 = icmp ne i64 %14, 0
  br i1 %tobool16, label %land.lhs.true, label %if.end22

land.lhs.true:                                    ; preds = %if.end13
  %15 = load i32, ptr %r, align 4
  %cmp = icmp slt i32 %15, 0
  br i1 %cmp, label %land.lhs.true18, label %if.end22

land.lhs.true18:                                  ; preds = %land.lhs.true
  %16 = load i64, ptr %why, align 8
  %cmp19 = icmp eq i64 %16, 22
  br i1 %cmp19, label %if.then21, label %if.end22

if.then21:                                        ; preds = %land.lhs.true18
  %17 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 7), align 8
  store i64 %17, ptr %why, align 8
  br label %if.end22

if.end22:                                         ; preds = %if.then21, %land.lhs.true18, %land.lhs.true, %if.end13
  %18 = load ptr, ptr %out.addr, align 8
  %19 = load i32, ptr %r, align 4
  %conv23 = sext i32 %19 to i64
  %20 = load i64, ptr %why, align 8
  call void @dirrt_triple(ptr %18, i64 %conv23, i64 %20)
  br label %return

return:                                           ; preds = %if.end22, %if.then
  ret void
}

declare i32 @renameat(i32, ptr, i32, ptr)

declare i32 @renameat2(i32, ptr, i32, ptr, i32)

; Function Attrs: noinline nounwind
define void @cancho_dir_remove(ptr %dir, ptr %name, i64 %len, ptr %out) {
entry:
  %dir.addr = alloca ptr, align 8
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %copy = alloca [256 x i8], align 1
  %r = alloca i32, align 4
  store ptr %dir, ptr %dir.addr, align 8
  store ptr %name, ptr %name.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %name.addr, align 8
  %1 = load i64, ptr %len.addr, align 8
  %arraydecay = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %call = call i32 @dirrt_component(ptr %0, i64 %1, ptr %arraydecay)
  %tobool = icmp ne i32 %call, 0
  br i1 %tobool, label %if.end, label %if.then

if.then:                                          ; preds = %entry
  %2 = load ptr, ptr %out.addr, align 8
  call void @dirrt_refused(ptr %2)
  br label %return

if.end:                                           ; preds = %entry
  %3 = load ptr, ptr %dir.addr, align 8
  %4 = load i64, ptr %3, align 8
  %conv = trunc i64 %4 to i32
  %arraydecay1 = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %call2 = call i32 @unlinkat(i32 %conv, ptr %arraydecay1, i32 0)
  store i32 %call2, ptr %r, align 4
  %5 = load ptr, ptr %out.addr, align 8
  %6 = load i32, ptr %r, align 4
  %conv3 = sext i32 %6 to i64
  %call4 = call ptr @cancho_errno_location()
  %7 = load i32, ptr %call4, align 4
  %conv5 = sext i32 %7 to i64
  call void @dirrt_triple(ptr %5, i64 %conv3, i64 %conv5)
  br label %return

return:                                           ; preds = %if.end, %if.then
  ret void
}

declare i32 @unlinkat(i32, ptr, i32)

; Function Attrs: noinline nounwind
define void @cancho_dir_create(ptr %dir, ptr %name, i64 %len, i64 %mode, ptr %out) {
entry:
  %dir.addr = alloca ptr, align 8
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %mode.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %copy = alloca [256 x i8], align 1
  %fd = alloca i32, align 4
  store ptr %dir, ptr %dir.addr, align 8
  store ptr %name, ptr %name.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store i64 %mode, ptr %mode.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %name.addr, align 8
  %1 = load i64, ptr %len.addr, align 8
  %arraydecay = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %call = call i32 @dirrt_component(ptr %0, i64 %1, ptr %arraydecay)
  %tobool = icmp ne i32 %call, 0
  br i1 %tobool, label %lor.lhs.false, label %if.then

lor.lhs.false:                                    ; preds = %entry
  %2 = load i64, ptr %mode.addr, align 8
  %cmp = icmp ugt i64 %2, 511
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %lor.lhs.false, %entry
  %3 = load ptr, ptr %out.addr, align 8
  call void @dirrt_refused(ptr %3)
  br label %return

if.end:                                           ; preds = %lor.lhs.false
  %4 = load ptr, ptr %dir.addr, align 8
  %5 = load i64, ptr %4, align 8
  %conv = trunc i64 %5 to i32
  %arraydecay1 = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %6 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 3), align 8
  %conv2 = trunc i64 %6 to i32
  %7 = load i64, ptr %mode.addr, align 8
  %conv3 = trunc i64 %7 to i32
  %call4 = call i32 (i32, ptr, i32, ...) @openat(i32 %conv, ptr %arraydecay1, i32 %conv2, i32 %conv3)
  store i32 %call4, ptr %fd, align 4
  %8 = load ptr, ptr %out.addr, align 8
  %9 = load i32, ptr %fd, align 4
  %conv5 = sext i32 %9 to i64
  %call6 = call ptr @cancho_errno_location()
  %10 = load i32, ptr %call6, align 4
  %conv7 = sext i32 %10 to i64
  call void @dirrt_triple(ptr %8, i64 %conv5, i64 %conv7)
  br label %return

return:                                           ; preds = %if.end, %if.then
  ret void
}

; Function Attrs: noinline nounwind
define void @cancho_dir_mkdir(ptr %dir, ptr %name, i64 %len, i64 %mode, ptr %out) {
entry:
  %dir.addr = alloca ptr, align 8
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %mode.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %copy = alloca [256 x i8], align 1
  %r = alloca i32, align 4
  store ptr %dir, ptr %dir.addr, align 8
  store ptr %name, ptr %name.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store i64 %mode, ptr %mode.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %name.addr, align 8
  %1 = load i64, ptr %len.addr, align 8
  %arraydecay = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %call = call i32 @dirrt_component(ptr %0, i64 %1, ptr %arraydecay)
  %tobool = icmp ne i32 %call, 0
  br i1 %tobool, label %lor.lhs.false, label %if.then

lor.lhs.false:                                    ; preds = %entry
  %2 = load i64, ptr %mode.addr, align 8
  %cmp = icmp ugt i64 %2, 511
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %lor.lhs.false, %entry
  %3 = load ptr, ptr %out.addr, align 8
  call void @dirrt_refused(ptr %3)
  br label %return

if.end:                                           ; preds = %lor.lhs.false
  %4 = load ptr, ptr %dir.addr, align 8
  %5 = load i64, ptr %4, align 8
  %conv = trunc i64 %5 to i32
  %arraydecay1 = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %6 = load i64, ptr %mode.addr, align 8
  %conv2 = trunc i64 %6 to i32
  %call3 = call i32 @mkdirat(i32 %conv, ptr %arraydecay1, i32 %conv2)
  store i32 %call3, ptr %r, align 4
  %7 = load ptr, ptr %out.addr, align 8
  %8 = load i32, ptr %r, align 4
  %conv4 = sext i32 %8 to i64
  %call5 = call ptr @cancho_errno_location()
  %9 = load i32, ptr %call5, align 4
  %conv6 = sext i32 %9 to i64
  call void @dirrt_triple(ptr %7, i64 %conv4, i64 %conv6)
  br label %return

return:                                           ; preds = %if.end, %if.then
  ret void
}

declare i32 @mkdirat(i32, ptr, i32)

; Function Attrs: noinline nounwind
define void @cancho_dir_sync(ptr %dir, ptr %out) {
entry:
  %dir.addr = alloca ptr, align 8
  %out.addr = alloca ptr, align 8
  %r = alloca i32, align 4
  store ptr %dir, ptr %dir.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %dir.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %call = call i32 @fsync(i32 %conv)
  store i32 %call, ptr %r, align 4
  %2 = load ptr, ptr %out.addr, align 8
  %3 = load i32, ptr %r, align 4
  %conv1 = sext i32 %3 to i64
  %call2 = call ptr @cancho_errno_location()
  %4 = load i32, ptr %call2, align 4
  %conv3 = sext i32 %4 to i64
  call void @dirrt_triple(ptr %2, i64 %conv1, i64 %conv3)
  ret void
}


; Function Attrs: noinline nounwind
define void @cancho_dir_list(ptr %dir, ptr %out) {
entry:
  %dir.addr = alloca ptr, align 8
  %out.addr = alloca ptr, align 8
  %dot = alloca [2 x i8], align 1
  %own = alloca i32, align 4
  %why = alloca i64, align 8
  %stream = alloca ptr, align 8
  store ptr %dir, ptr %dir.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds [2 x i8], ptr %dot, i64 0, i64 0
  store i8 46, ptr %arrayidx, align 1
  %arrayidx1 = getelementptr inbounds [2 x i8], ptr %dot, i64 0, i64 1
  store i8 0, ptr %arrayidx1, align 1
  %0 = load ptr, ptr %dir.addr, align 8
  %1 = load i64, ptr %0, align 8
  %conv = trunc i64 %1 to i32
  %arraydecay = getelementptr inbounds [2 x i8], ptr %dot, i64 0, i64 0
  %2 = load i64, ptr @cancho_plat, align 8
  %conv2 = trunc i64 %2 to i32
  %call = call i32 (i32, ptr, i32, ...) @openat(i32 %conv, ptr %arraydecay, i32 %conv2, i32 0)
  store i32 %call, ptr %own, align 4
  %call3 = call ptr @cancho_errno_location()
  %3 = load i32, ptr %call3, align 4
  %conv4 = sext i32 %3 to i64
  store i64 %conv4, ptr %why, align 8
  %4 = load ptr, ptr %out.addr, align 8
  %arrayidx5 = getelementptr inbounds i64, ptr %4, i64 0
  store i64 1, ptr %arrayidx5, align 8
  %5 = load ptr, ptr %out.addr, align 8
  %arrayidx6 = getelementptr inbounds i64, ptr %5, i64 1
  store i64 0, ptr %arrayidx6, align 8
  %6 = load i64, ptr %why, align 8
  %7 = load ptr, ptr %out.addr, align 8
  %arrayidx7 = getelementptr inbounds i64, ptr %7, i64 2
  store i64 %6, ptr %arrayidx7, align 8
  %8 = load i32, ptr %own, align 4
  %cmp = icmp slt i32 %8, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  br label %return

if.end:                                           ; preds = %entry
  %9 = load i32, ptr %own, align 4
  %call9 = call ptr @fdopendir(i32 %9)
  store ptr %call9, ptr %stream, align 8
  %call10 = call ptr @cancho_errno_location()
  %10 = load i32, ptr %call10, align 4
  %conv11 = sext i32 %10 to i64
  store i64 %conv11, ptr %why, align 8
  %11 = load ptr, ptr %stream, align 8
  %cmp12 = icmp eq ptr %11, null
  br i1 %cmp12, label %if.then14, label %if.end17

if.then14:                                        ; preds = %if.end
  %12 = load i32, ptr %own, align 4
  %call15 = call i32 @close(i32 %12)
  %13 = load i64, ptr %why, align 8
  %14 = load ptr, ptr %out.addr, align 8
  %arrayidx16 = getelementptr inbounds i64, ptr %14, i64 2
  store i64 %13, ptr %arrayidx16, align 8
  br label %return

if.end17:                                         ; preds = %if.end
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx18 = getelementptr inbounds i64, ptr %15, i64 0
  store i64 0, ptr %arrayidx18, align 8
  %16 = load ptr, ptr %stream, align 8
  %17 = ptrtoint ptr %16 to i64
  %18 = load ptr, ptr %out.addr, align 8
  %arrayidx19 = getelementptr inbounds i64, ptr %18, i64 1
  store i64 %17, ptr %arrayidx19, align 8
  %19 = load ptr, ptr %out.addr, align 8
  %arrayidx20 = getelementptr inbounds i64, ptr %19, i64 2
  store i64 0, ptr %arrayidx20, align 8
  br label %return

return:                                           ; preds = %if.end17, %if.then14, %if.then
  ret void
}

declare ptr @fdopendir(i32)


; Function Attrs: noinline nounwind
define void @cancho_dir_next(ptr %list, ptr %buf, i64 %room, ptr %out) {
entry:
  %list.addr = alloca ptr, align 8
  %buf.addr = alloca ptr, align 8
  %room.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %stream = alloca ptr, align 8
  %entry1 = alloca ptr, align 8
  %why = alloca i64, align 8
  %name = alloca ptr, align 8
  %n = alloca i64, align 8
  store ptr %list, ptr %list.addr, align 8
  store ptr %buf, ptr %buf.addr, align 8
  store i64 %room, ptr %room.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %list.addr, align 8
  %1 = load i64, ptr %0, align 8
  %2 = inttoptr i64 %1 to ptr
  store ptr %2, ptr %stream, align 8
  br label %for.cond

for.cond:                                         ; preds = %if.then24, %entry
  %call = call ptr @cancho_errno_location()
  store i32 0, ptr %call, align 4
  %3 = load ptr, ptr %stream, align 8
  %call2 = call ptr @readdir(ptr %3)
  store ptr %call2, ptr %entry1, align 8
  %4 = load ptr, ptr %entry1, align 8
  %cmp = icmp eq ptr %4, null
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %for.cond
  %call3 = call ptr @cancho_errno_location()
  %5 = load i32, ptr %call3, align 4
  %conv = sext i32 %5 to i64
  store i64 %conv, ptr %why, align 8
  %6 = load i64, ptr %why, align 8
  %cmp4 = icmp eq i64 %6, 0
  %7 = zext i1 %cmp4 to i64
  %cond = select i1 %cmp4, i32 1, i32 2
  %conv6 = sext i32 %cond to i64
  %8 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %8, i64 0
  store i64 %conv6, ptr %arrayidx, align 8
  %9 = load ptr, ptr %out.addr, align 8
  %arrayidx7 = getelementptr inbounds i64, ptr %9, i64 1
  store i64 0, ptr %arrayidx7, align 8
  %10 = load ptr, ptr %out.addr, align 8
  %arrayidx8 = getelementptr inbounds i64, ptr %10, i64 2
  store i64 0, ptr %arrayidx8, align 8
  %11 = load i64, ptr %why, align 8
  %12 = load ptr, ptr %out.addr, align 8
  %arrayidx9 = getelementptr inbounds i64, ptr %12, i64 3
  store i64 %11, ptr %arrayidx9, align 8
  br label %return

if.end:                                           ; preds = %for.cond
  %13 = load ptr, ptr %entry1, align 8
  %14 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 10), align 8
  %add.ptr = getelementptr inbounds i8, ptr %13, i64 %14
  store ptr %add.ptr, ptr %name, align 8
  %15 = load ptr, ptr %name, align 8
  %call10 = call i64 @strlen(ptr %15)
  store i64 %call10, ptr %n, align 8
  %16 = load ptr, ptr %name, align 8
  %arrayidx11 = getelementptr inbounds i8, ptr %16, i64 0
  %17 = load i8, ptr %arrayidx11, align 1
  %conv12 = zext i8 %17 to i32
  %cmp13 = icmp eq i32 %conv12, 46
  br i1 %cmp13, label %land.lhs.true, label %if.end25

land.lhs.true:                                    ; preds = %if.end
  %18 = load i64, ptr %n, align 8
  %cmp15 = icmp eq i64 %18, 1
  br i1 %cmp15, label %if.then24, label %lor.lhs.false

lor.lhs.false:                                    ; preds = %land.lhs.true
  %19 = load i64, ptr %n, align 8
  %cmp17 = icmp eq i64 %19, 2
  br i1 %cmp17, label %land.lhs.true19, label %if.end25

land.lhs.true19:                                  ; preds = %lor.lhs.false
  %20 = load ptr, ptr %name, align 8
  %arrayidx20 = getelementptr inbounds i8, ptr %20, i64 1
  %21 = load i8, ptr %arrayidx20, align 1
  %conv21 = zext i8 %21 to i32
  %cmp22 = icmp eq i32 %conv21, 46
  br i1 %cmp22, label %if.then24, label %if.end25

if.then24:                                        ; preds = %land.lhs.true19, %land.lhs.true
  br label %for.cond

if.end25:                                         ; preds = %land.lhs.true19, %lor.lhs.false, %if.end
  %22 = load i64, ptr %n, align 8
  %23 = load i64, ptr %room.addr, align 8
  %cmp26 = icmp sgt i64 %22, %23
  br i1 %cmp26, label %if.then28, label %if.end33

if.then28:                                        ; preds = %if.end25
  %24 = load ptr, ptr %out.addr, align 8
  %arrayidx29 = getelementptr inbounds i64, ptr %24, i64 0
  store i64 2, ptr %arrayidx29, align 8
  %25 = load ptr, ptr %out.addr, align 8
  %arrayidx30 = getelementptr inbounds i64, ptr %25, i64 1
  store i64 0, ptr %arrayidx30, align 8
  %26 = load ptr, ptr %out.addr, align 8
  %arrayidx31 = getelementptr inbounds i64, ptr %26, i64 2
  store i64 0, ptr %arrayidx31, align 8
  %27 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 8), align 8
  %28 = load ptr, ptr %out.addr, align 8
  %arrayidx32 = getelementptr inbounds i64, ptr %28, i64 3
  store i64 %27, ptr %arrayidx32, align 8
  br label %return

if.end33:                                         ; preds = %if.end25
  %29 = load ptr, ptr %buf.addr, align 8
  %30 = load ptr, ptr %name, align 8
  %31 = load i64, ptr %n, align 8
  %call34 = call ptr @memmove(ptr %29, ptr %30, i64 %31)
  %32 = load ptr, ptr %out.addr, align 8
  %arrayidx35 = getelementptr inbounds i64, ptr %32, i64 0
  store i64 0, ptr %arrayidx35, align 8
  %33 = load i64, ptr %n, align 8
  %34 = load ptr, ptr %out.addr, align 8
  %arrayidx36 = getelementptr inbounds i64, ptr %34, i64 1
  store i64 %33, ptr %arrayidx36, align 8
  %35 = load ptr, ptr %entry1, align 8
  %36 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 9), align 8
  %arrayidx37 = getelementptr inbounds i8, ptr %35, i64 %36
  %37 = load i8, ptr %arrayidx37, align 1
  %conv38 = zext i8 %37 to i64
  %call39 = call i64 @dirrt_kind(i64 %conv38)
  %38 = load ptr, ptr %out.addr, align 8
  %arrayidx40 = getelementptr inbounds i64, ptr %38, i64 2
  store i64 %call39, ptr %arrayidx40, align 8
  %39 = load ptr, ptr %out.addr, align 8
  %arrayidx41 = getelementptr inbounds i64, ptr %39, i64 3
  store i64 0, ptr %arrayidx41, align 8
  br label %return

return:                                           ; preds = %if.end33, %if.then28, %if.then
  ret void
}

declare ptr @readdir(ptr)



; Function Attrs: noinline nounwind
define internal i64 @dirrt_kind(i64 %dt) {
entry:
  %retval = alloca i64, align 8
  %dt.addr = alloca i64, align 8
  store i64 %dt, ptr %dt.addr, align 8
  %0 = load i64, ptr %dt.addr, align 8
  %1 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 11), align 8
  %cmp = icmp eq i64 %0, %1
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  store i64 0, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %entry
  %2 = load i64, ptr %dt.addr, align 8
  %3 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 12), align 8
  %cmp1 = icmp eq i64 %2, %3
  br i1 %cmp1, label %if.then2, label %if.end3

if.then2:                                         ; preds = %if.end
  store i64 3, ptr %retval, align 8
  br label %return

if.end3:                                          ; preds = %if.end
  %4 = load i64, ptr %dt.addr, align 8
  %5 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 13), align 8
  %cmp4 = icmp eq i64 %4, %5
  br i1 %cmp4, label %if.then5, label %if.end6

if.then5:                                         ; preds = %if.end3
  store i64 2, ptr %retval, align 8
  br label %return

if.end6:                                          ; preds = %if.end3
  %6 = load i64, ptr %dt.addr, align 8
  %7 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 14), align 8
  %cmp7 = icmp eq i64 %6, %7
  br i1 %cmp7, label %if.then8, label %if.end9

if.then8:                                         ; preds = %if.end6
  store i64 1, ptr %retval, align 8
  br label %return

if.end9:                                          ; preds = %if.end6
  store i64 4, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end9, %if.then8, %if.then5, %if.then2, %if.then
  %8 = load i64, ptr %retval, align 8
  ret i64 %8
}

; Function Attrs: noinline nounwind
define i64 @cancho_dir_list_close(i64 %stream) {
entry:
  %stream.addr = alloca i64, align 8
  store i64 %stream, ptr %stream.addr, align 8
  %0 = load i64, ptr %stream.addr, align 8
  %1 = inttoptr i64 %0 to ptr
  %call = call i32 @closedir(ptr %1)
  %conv = sext i32 %call to i64
  ret i64 %conv
}

declare i32 @closedir(ptr)

; Function Attrs: noinline nounwind
define void @cancho_dir_stat(ptr %dir, ptr %name, i64 %len, ptr %out) {
entry:
  %dir.addr = alloca ptr, align 8
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %copy = alloca [256 x i8], align 1
  %st = alloca [160 x i8], align 1
  %i = alloca i32, align 4
  %r = alloca i32, align 4
  %why = alloca i64, align 8
  %type = alloca i64, align 8
  store ptr %dir, ptr %dir.addr, align 8
  store ptr %name, ptr %name.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %0 = load i32, ptr %i, align 4
  %cmp = icmp slt i32 %0, 5
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %1 = load ptr, ptr %out.addr, align 8
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
  %4 = load ptr, ptr %name.addr, align 8
  %5 = load i64, ptr %len.addr, align 8
  %arraydecay = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %call = call i32 @dirrt_component(ptr %4, i64 %5, ptr %arraydecay)
  %tobool = icmp ne i32 %call, 0
  br i1 %tobool, label %if.end, label %if.then

if.then:                                          ; preds = %for.end
  %6 = load ptr, ptr %out.addr, align 8
  %arrayidx1 = getelementptr inbounds i64, ptr %6, i64 0
  store i64 1, ptr %arrayidx1, align 8
  %7 = load ptr, ptr %out.addr, align 8
  %arrayidx2 = getelementptr inbounds i64, ptr %7, i64 4
  store i64 22, ptr %arrayidx2, align 8
  br label %return

if.end:                                           ; preds = %for.end
  %8 = load ptr, ptr %dir.addr, align 8
  %9 = load i64, ptr %8, align 8
  %conv = trunc i64 %9 to i32
  %arraydecay3 = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %arraydecay4 = getelementptr inbounds [160 x i8], ptr %st, i64 0, i64 0
  %10 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 19), align 8
  %conv5 = trunc i64 %10 to i32
  %call6 = call i32 @fstatat(i32 %conv, ptr %arraydecay3, ptr %arraydecay4, i32 %conv5)
  store i32 %call6, ptr %r, align 4
  %call7 = call ptr @cancho_errno_location()
  %11 = load i32, ptr %call7, align 4
  %conv8 = sext i32 %11 to i64
  store i64 %conv8, ptr %why, align 8
  %12 = load i32, ptr %r, align 4
  %cmp9 = icmp slt i32 %12, 0
  br i1 %cmp9, label %if.then11, label %if.end14

if.then11:                                        ; preds = %if.end
  %13 = load ptr, ptr %out.addr, align 8
  %arrayidx12 = getelementptr inbounds i64, ptr %13, i64 0
  store i64 1, ptr %arrayidx12, align 8
  %14 = load i64, ptr %why, align 8
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx13 = getelementptr inbounds i64, ptr %15, i64 4
  store i64 %14, ptr %arrayidx13, align 8
  br label %return

if.end14:                                         ; preds = %if.end
  %arraydecay15 = getelementptr inbounds [160 x i8], ptr %st, i64 0, i64 0
  %16 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 15), align 8
  %17 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 16), align 8
  %call16 = call i64 @dirrt_field(ptr %arraydecay15, i64 %16, i64 %17)
  %and = and i64 %call16, 61440
  store i64 %and, ptr %type, align 8
  %18 = load i64, ptr %type, align 8
  %cmp17 = icmp eq i64 %18, 40960
  br i1 %cmp17, label %cond.true, label %cond.false

cond.true:                                        ; preds = %if.end14
  br label %cond.end26

cond.false:                                       ; preds = %if.end14
  %19 = load i64, ptr %type, align 8
  %cmp19 = icmp eq i64 %19, 16384
  br i1 %cmp19, label %cond.true21, label %cond.false22

cond.true21:                                      ; preds = %cond.false
  br label %cond.end

cond.false22:                                     ; preds = %cond.false
  %20 = load i64, ptr %type, align 8
  %cmp23 = icmp eq i64 %20, 32768
  %21 = zext i1 %cmp23 to i64
  %cond = select i1 %cmp23, i32 1, i32 4
  br label %cond.end

cond.end:                                         ; preds = %cond.false22, %cond.true21
  %cond25 = phi i32 [ 2, %cond.true21 ], [ %cond, %cond.false22 ]
  br label %cond.end26

cond.end26:                                       ; preds = %cond.end, %cond.true
  %cond27 = phi i32 [ 3, %cond.true ], [ %cond25, %cond.end ]
  %conv28 = sext i32 %cond27 to i64
  %22 = load ptr, ptr %out.addr, align 8
  %arrayidx29 = getelementptr inbounds i64, ptr %22, i64 1
  store i64 %conv28, ptr %arrayidx29, align 8
  %arraydecay30 = getelementptr inbounds [160 x i8], ptr %st, i64 0, i64 0
  %23 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 17), align 8
  %call31 = call i64 @dirrt_field(ptr %arraydecay30, i64 %23, i64 64)
  %24 = load ptr, ptr %out.addr, align 8
  %arrayidx32 = getelementptr inbounds i64, ptr %24, i64 2
  store i64 %call31, ptr %arrayidx32, align 8
  %arraydecay33 = getelementptr inbounds [160 x i8], ptr %st, i64 0, i64 0
  %25 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 18), align 8
  %call34 = call i64 @dirrt_field(ptr %arraydecay33, i64 %25, i64 64)
  %26 = load ptr, ptr %out.addr, align 8
  %arrayidx35 = getelementptr inbounds i64, ptr %26, i64 3
  store i64 %call34, ptr %arrayidx35, align 8
  br label %return

return:                                           ; preds = %cond.end26, %if.then11, %if.then
  ret void
}

declare i32 @fstatat(i32, ptr, ptr, i32)

; Function Attrs: noinline nounwind
define internal i64 @dirrt_field(ptr %st, i64 %at, i64 %bits) {
entry:
  %retval = alloca i64, align 8
  %st.addr = alloca ptr, align 8
  %at.addr = alloca i64, align 8
  %bits.addr = alloca i64, align 8
  store ptr %st, ptr %st.addr, align 8
  store i64 %at, ptr %at.addr, align 8
  store i64 %bits, ptr %bits.addr, align 8
  %0 = load i64, ptr %bits.addr, align 8
  %cmp = icmp eq i64 %0, 64
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %1 = load ptr, ptr %st.addr, align 8
  %2 = load i64, ptr %at.addr, align 8
  %add.ptr = getelementptr inbounds i8, ptr %1, i64 %2
  %3 = load i64, ptr %add.ptr, align 8
  store i64 %3, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %entry
  %4 = load i64, ptr %bits.addr, align 8
  %cmp1 = icmp eq i64 %4, 32
  br i1 %cmp1, label %if.then2, label %if.end4

if.then2:                                         ; preds = %if.end
  %5 = load ptr, ptr %st.addr, align 8
  %6 = load i64, ptr %at.addr, align 8
  %add.ptr3 = getelementptr inbounds i8, ptr %5, i64 %6
  %7 = load i32, ptr %add.ptr3, align 4
  %conv = zext i32 %7 to i64
  store i64 %conv, ptr %retval, align 8
  br label %return

if.end4:                                          ; preds = %if.end
  %8 = load ptr, ptr %st.addr, align 8
  %9 = load i64, ptr %at.addr, align 8
  %add.ptr5 = getelementptr inbounds i8, ptr %8, i64 %9
  %10 = load i16, ptr %add.ptr5, align 2
  %conv6 = zext i16 %10 to i64
  store i64 %conv6, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end4, %if.then2, %if.then
  %11 = load i64, ptr %retval, align 8
  ret i64 %11
}

; Function Attrs: noinline nounwind
define void @cancho_dir_mode(ptr %dir, ptr %name, i64 %len, i64 %own, ptr %out) {
entry:
  %dir.addr = alloca ptr, align 8
  %name.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %own.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %copy = alloca [256 x i8], align 1
  %st = alloca [160 x i8], align 1
  %r = alloca i32, align 4
  %why = alloca i64, align 8
  store ptr %dir, ptr %dir.addr, align 8
  store ptr %name, ptr %name.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store i64 %own, ptr %own.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load i64, ptr %own.addr, align 8
  %tobool = icmp ne i64 %0, 0
  br i1 %tobool, label %if.then, label %if.else

if.then:                                          ; preds = %entry
  %1 = load ptr, ptr %dir.addr, align 8
  %2 = load i64, ptr %1, align 8
  %conv = trunc i64 %2 to i32
  %arraydecay = getelementptr inbounds [160 x i8], ptr %st, i64 0, i64 0
  %call = call i32 @fstat(i32 %conv, ptr %arraydecay)
  store i32 %call, ptr %r, align 4
  br label %if.end10

if.else:                                          ; preds = %entry
  %3 = load ptr, ptr %name.addr, align 8
  %4 = load i64, ptr %len.addr, align 8
  %arraydecay1 = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %call2 = call i32 @dirrt_component(ptr %3, i64 %4, ptr %arraydecay1)
  %tobool3 = icmp ne i32 %call2, 0
  br i1 %tobool3, label %if.end, label %if.then4

if.then4:                                         ; preds = %if.else
  %5 = load ptr, ptr %out.addr, align 8
  call void @dirrt_refused(ptr %5)
  br label %return

if.end:                                           ; preds = %if.else
  %6 = load ptr, ptr %dir.addr, align 8
  %7 = load i64, ptr %6, align 8
  %conv5 = trunc i64 %7 to i32
  %arraydecay6 = getelementptr inbounds [256 x i8], ptr %copy, i64 0, i64 0
  %arraydecay7 = getelementptr inbounds [160 x i8], ptr %st, i64 0, i64 0
  %8 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 19), align 8
  %conv8 = trunc i64 %8 to i32
  %call9 = call i32 @fstatat(i32 %conv5, ptr %arraydecay6, ptr %arraydecay7, i32 %conv8)
  store i32 %call9, ptr %r, align 4
  br label %if.end10

if.end10:                                         ; preds = %if.end, %if.then
  %call11 = call ptr @cancho_errno_location()
  %9 = load i32, ptr %call11, align 4
  %conv12 = sext i32 %9 to i64
  store i64 %conv12, ptr %why, align 8
  %10 = load i32, ptr %r, align 4
  %cmp = icmp slt i32 %10, 0
  %conv13 = zext i1 %cmp to i32
  %conv14 = sext i32 %conv13 to i64
  %11 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %11, i64 0
  store i64 %conv14, ptr %arrayidx, align 8
  %12 = load i32, ptr %r, align 4
  %cmp15 = icmp slt i32 %12, 0
  br i1 %cmp15, label %cond.true, label %cond.false

cond.true:                                        ; preds = %if.end10
  br label %cond.end

cond.false:                                       ; preds = %if.end10
  %arraydecay17 = getelementptr inbounds [160 x i8], ptr %st, i64 0, i64 0
  %13 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 15), align 8
  %14 = load i64, ptr getelementptr inbounds ([0 x i64], ptr @cancho_plat, i64 0, i64 16), align 8
  %call18 = call i64 @dirrt_field(ptr %arraydecay17, i64 %13, i64 %14)
  %and = and i64 %call18, 4095
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond = phi i64 [ 0, %cond.true ], [ %and, %cond.false ]
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx19 = getelementptr inbounds i64, ptr %15, i64 1
  store i64 %cond, ptr %arrayidx19, align 8
  %16 = load i64, ptr %why, align 8
  %17 = load ptr, ptr %out.addr, align 8
  %arrayidx20 = getelementptr inbounds i64, ptr %17, i64 2
  store i64 %16, ptr %arrayidx20, align 8
  br label %return

return:                                           ; preds = %cond.end, %if.then4
  ret void
}

declare i32 @fstat(i32, ptr)




