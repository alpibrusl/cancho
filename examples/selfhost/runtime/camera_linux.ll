
; Function Attrs: noinline nounwind
define void @cancho_camera_open(ptr %src, i64 %len, ptr %prefix, i64 %plen, ptr %out) {
entry:
  %src.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %prefix.addr = alloca ptr, align 8
  %plen.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  %path = alloca ptr, align 8
  %fd = alloca i32, align 4
  %e = alloca i32, align 4
  %block = alloca ptr, align 8
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
  %call1 = call i32 (i32, ptr, i32, ...) @openat(i32 -100, ptr %4, i32 2050, i32 0)
  store i32 %call1, ptr %fd, align 4
  %call2 = call ptr @cancho_errno_location()
  %5 = load i32, ptr %call2, align 4
  store i32 %5, ptr %e, align 4
  %6 = load ptr, ptr %path, align 8
  call void @free(ptr %6)
  %7 = load i32, ptr %fd, align 4
  %cmp = icmp slt i32 %7, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  %8 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %8, i64 0
  store i64 1, ptr %arrayidx, align 8
  %9 = load i32, ptr %fd, align 4
  %conv = sext i32 %9 to i64
  %10 = load ptr, ptr %out.addr, align 8
  %arrayidx3 = getelementptr inbounds i64, ptr %10, i64 1
  store i64 %conv, ptr %arrayidx3, align 8
  %11 = load i32, ptr %e, align 4
  %conv4 = sext i32 %11 to i64
  %12 = load ptr, ptr %out.addr, align 8
  %arrayidx5 = getelementptr inbounds i64, ptr %12, i64 2
  store i64 %conv4, ptr %arrayidx5, align 8
  br label %return

if.end:                                           ; preds = %entry
  %call6 = call ptr @calloc(i64 1, i64 128)
  store ptr %call6, ptr %block, align 8
  %13 = load ptr, ptr %block, align 8
  %cmp7 = icmp eq ptr %13, null
  br i1 %cmp7, label %if.then9, label %if.end14

if.then9:                                         ; preds = %if.end
  %14 = load i32, ptr %fd, align 4
  %call10 = call i32 @close(i32 %14)
  %15 = load ptr, ptr %out.addr, align 8
  %arrayidx11 = getelementptr inbounds i64, ptr %15, i64 0
  store i64 1, ptr %arrayidx11, align 8
  %16 = load ptr, ptr %out.addr, align 8
  %arrayidx12 = getelementptr inbounds i64, ptr %16, i64 1
  store i64 -1, ptr %arrayidx12, align 8
  %17 = load ptr, ptr %out.addr, align 8
  %arrayidx13 = getelementptr inbounds i64, ptr %17, i64 2
  store i64 12, ptr %arrayidx13, align 8
  br label %return

if.end14:                                         ; preds = %if.end
  %18 = load i32, ptr %fd, align 4
  %19 = load ptr, ptr %block, align 8
  %add.ptr = getelementptr inbounds i8, ptr %19, i64 0
  store i32 %18, ptr %add.ptr, align 4
  %20 = load ptr, ptr %out.addr, align 8
  %arrayidx15 = getelementptr inbounds i64, ptr %20, i64 0
  store i64 0, ptr %arrayidx15, align 8
  %21 = load ptr, ptr %block, align 8
  %22 = ptrtoint ptr %21 to i64
  %23 = load ptr, ptr %out.addr, align 8
  %arrayidx16 = getelementptr inbounds i64, ptr %23, i64 1
  store i64 %22, ptr %arrayidx16, align 8
  %24 = load ptr, ptr %out.addr, align 8
  %arrayidx17 = getelementptr inbounds i64, ptr %24, i64 2
  store i64 0, ptr %arrayidx17, align 8
  br label %return

return:                                           ; preds = %if.end14, %if.then9, %if.then
  ret void
}





; Function Attrs: allocsize(0,1)


; Function Attrs: noinline nounwind
define i64 @cancho_camera_configure(ptr %cam, i64 %width, i64 %height) {
entry:
  %retval = alloca i64, align 8
  %cam.addr = alloca ptr, align 8
  %width.addr = alloca i64, align 8
  %height.addr = alloca i64, align 8
  %s = alloca ptr, align 8
  %fd = alloca i32, align 4
  %fmt = alloca [26 x i64], align 8
  %req = alloca [3 x i64], align 8
  %granted = alloca i32, align 4
  %i = alloca i32, align 4
  %buf = alloca [11 x i64], align 8
  %length = alloca i32, align 4
  %offset = alloca i32, align 4
  %at = alloca ptr, align 8
  %type = alloca i32, align 4
  store ptr %cam, ptr %cam.addr, align 8
  store i64 %width, ptr %width.addr, align 8
  store i64 %height, ptr %height.addr, align 8
  %0 = load ptr, ptr %cam.addr, align 8
  %1 = load i64, ptr %0, align 8
  %2 = inttoptr i64 %1 to ptr
  store ptr %2, ptr %s, align 8
  %3 = load i64, ptr %width.addr, align 8
  %cmp = icmp sle i64 %3, 0
  br i1 %cmp, label %if.then, label %lor.lhs.false

lor.lhs.false:                                    ; preds = %entry
  %4 = load i64, ptr %height.addr, align 8
  %cmp1 = icmp sle i64 %4, 0
  br i1 %cmp1, label %if.then, label %lor.lhs.false2

lor.lhs.false2:                                   ; preds = %lor.lhs.false
  %5 = load i64, ptr %width.addr, align 8
  %cmp3 = icmp sgt i64 %5, 2147483647
  br i1 %cmp3, label %if.then, label %lor.lhs.false4

lor.lhs.false4:                                   ; preds = %lor.lhs.false2
  %6 = load i64, ptr %height.addr, align 8
  %cmp5 = icmp sgt i64 %6, 2147483647
  br i1 %cmp5, label %if.then, label %if.end

if.then:                                          ; preds = %lor.lhs.false4, %lor.lhs.false2, %lor.lhs.false, %entry
  store i64 22, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %lor.lhs.false4
  %7 = load ptr, ptr %s, align 8
  %add.ptr = getelementptr inbounds i8, ptr %7, i64 8
  %8 = load i64, ptr %add.ptr, align 8
  %cmp6 = icmp ne i64 %8, 0
  br i1 %cmp6, label %if.then7, label %if.end8

if.then7:                                         ; preds = %if.end
  store i64 22, ptr %retval, align 8
  br label %return

if.end8:                                          ; preds = %if.end
  %9 = load ptr, ptr %s, align 8
  %add.ptr9 = getelementptr inbounds i8, ptr %9, i64 0
  %10 = load i32, ptr %add.ptr9, align 4
  store i32 %10, ptr %fd, align 4
  %arraydecay = getelementptr inbounds [26 x i64], ptr %fmt, i64 0, i64 0
  call void @cancho_camera_zero(ptr %arraydecay, i32 26)
  %arraydecay10 = getelementptr inbounds [26 x i64], ptr %fmt, i64 0, i64 0
  %add.ptr11 = getelementptr inbounds i8, ptr %arraydecay10, i64 0
  store i32 1, ptr %add.ptr11, align 4
  %11 = load i64, ptr %width.addr, align 8
  %conv = trunc i64 %11 to i32
  %arraydecay12 = getelementptr inbounds [26 x i64], ptr %fmt, i64 0, i64 0
  %add.ptr13 = getelementptr inbounds i8, ptr %arraydecay12, i64 8
  store i32 %conv, ptr %add.ptr13, align 4
  %12 = load i64, ptr %height.addr, align 8
  %conv14 = trunc i64 %12 to i32
  %arraydecay15 = getelementptr inbounds [26 x i64], ptr %fmt, i64 0, i64 0
  %add.ptr16 = getelementptr inbounds i8, ptr %arraydecay15, i64 12
  store i32 %conv14, ptr %add.ptr16, align 4
  %arraydecay17 = getelementptr inbounds [26 x i64], ptr %fmt, i64 0, i64 0
  %add.ptr18 = getelementptr inbounds i8, ptr %arraydecay17, i64 16
  store i32 1448695129, ptr %add.ptr18, align 4
  %arraydecay19 = getelementptr inbounds [26 x i64], ptr %fmt, i64 0, i64 0
  %add.ptr20 = getelementptr inbounds i8, ptr %arraydecay19, i64 20
  store i32 1, ptr %add.ptr20, align 4
  %13 = load i32, ptr %fd, align 4
  %arraydecay21 = getelementptr inbounds [26 x i64], ptr %fmt, i64 0, i64 0
  %call = call i32 (i32, i64, ...) @ioctl(i32 %13, i64 3234878981, ptr %arraydecay21)
  %cmp22 = icmp slt i32 %call, 0
  br i1 %cmp22, label %if.then24, label %if.end27

if.then24:                                        ; preds = %if.end8
  %call25 = call ptr @cancho_errno_location()
  %14 = load i32, ptr %call25, align 4
  %conv26 = sext i32 %14 to i64
  store i64 %conv26, ptr %retval, align 8
  br label %return

if.end27:                                         ; preds = %if.end8
  %arraydecay28 = getelementptr inbounds [26 x i64], ptr %fmt, i64 0, i64 0
  %add.ptr29 = getelementptr inbounds i8, ptr %arraydecay28, i64 8
  %15 = load i32, ptr %add.ptr29, align 4
  %16 = load i64, ptr %width.addr, align 8
  %conv30 = trunc i64 %16 to i32
  %cmp31 = icmp ne i32 %15, %conv30
  br i1 %cmp31, label %if.then39, label %lor.lhs.false33

lor.lhs.false33:                                  ; preds = %if.end27
  %arraydecay34 = getelementptr inbounds [26 x i64], ptr %fmt, i64 0, i64 0
  %add.ptr35 = getelementptr inbounds i8, ptr %arraydecay34, i64 12
  %17 = load i32, ptr %add.ptr35, align 4
  %18 = load i64, ptr %height.addr, align 8
  %conv36 = trunc i64 %18 to i32
  %cmp37 = icmp ne i32 %17, %conv36
  br i1 %cmp37, label %if.then39, label %if.end40

if.then39:                                        ; preds = %lor.lhs.false33, %if.end27
  store i64 22, ptr %retval, align 8
  br label %return

if.end40:                                         ; preds = %lor.lhs.false33
  %19 = load i64, ptr %width.addr, align 8
  %20 = load ptr, ptr %s, align 8
  %add.ptr41 = getelementptr inbounds i8, ptr %20, i64 8
  store i64 %19, ptr %add.ptr41, align 8
  %21 = load i64, ptr %height.addr, align 8
  %22 = load ptr, ptr %s, align 8
  %add.ptr42 = getelementptr inbounds i8, ptr %22, i64 16
  store i64 %21, ptr %add.ptr42, align 8
  %arraydecay43 = getelementptr inbounds [3 x i64], ptr %req, i64 0, i64 0
  call void @cancho_camera_zero(ptr %arraydecay43, i32 3)
  %arraydecay44 = getelementptr inbounds [3 x i64], ptr %req, i64 0, i64 0
  %add.ptr45 = getelementptr inbounds i8, ptr %arraydecay44, i64 0
  store i32 4, ptr %add.ptr45, align 4
  %arraydecay46 = getelementptr inbounds [3 x i64], ptr %req, i64 0, i64 0
  %add.ptr47 = getelementptr inbounds i8, ptr %arraydecay46, i64 4
  store i32 1, ptr %add.ptr47, align 4
  %arraydecay48 = getelementptr inbounds [3 x i64], ptr %req, i64 0, i64 0
  %add.ptr49 = getelementptr inbounds i8, ptr %arraydecay48, i64 8
  store i32 1, ptr %add.ptr49, align 4
  %23 = load i32, ptr %fd, align 4
  %arraydecay50 = getelementptr inbounds [3 x i64], ptr %req, i64 0, i64 0
  %call51 = call i32 (i32, i64, ...) @ioctl(i32 %23, i64 3222558216, ptr %arraydecay50)
  %cmp52 = icmp slt i32 %call51, 0
  br i1 %cmp52, label %if.then54, label %if.end57

if.then54:                                        ; preds = %if.end40
  %call55 = call ptr @cancho_errno_location()
  %24 = load i32, ptr %call55, align 4
  %conv56 = sext i32 %24 to i64
  store i64 %conv56, ptr %retval, align 8
  br label %return

if.end57:                                         ; preds = %if.end40
  %arraydecay58 = getelementptr inbounds [3 x i64], ptr %req, i64 0, i64 0
  %add.ptr59 = getelementptr inbounds i8, ptr %arraydecay58, i64 0
  %25 = load i32, ptr %add.ptr59, align 4
  store i32 %25, ptr %granted, align 4
  %26 = load i32, ptr %granted, align 4
  %cmp60 = icmp sle i32 %26, 0
  br i1 %cmp60, label %if.then62, label %if.end63

if.then62:                                        ; preds = %if.end57
  store i64 22, ptr %retval, align 8
  br label %return

if.end63:                                         ; preds = %if.end57
  %27 = load i32, ptr %granted, align 4
  %cmp64 = icmp sgt i32 %27, 4
  br i1 %cmp64, label %if.then66, label %if.end67

if.then66:                                        ; preds = %if.end63
  store i32 4, ptr %granted, align 4
  br label %if.end67

if.end67:                                         ; preds = %if.then66, %if.end63
  %28 = load i32, ptr %granted, align 4
  %conv68 = sext i32 %28 to i64
  %29 = load ptr, ptr %s, align 8
  %add.ptr69 = getelementptr inbounds i8, ptr %29, i64 24
  store i64 %conv68, ptr %add.ptr69, align 8
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %if.end67
  %30 = load i32, ptr %i, align 4
  %31 = load i32, ptr %granted, align 4
  %cmp70 = icmp slt i32 %30, %31
  br i1 %cmp70, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %arraydecay72 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  call void @cancho_camera_zero(ptr %arraydecay72, i32 11)
  %32 = load i32, ptr %i, align 4
  %arraydecay73 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  %add.ptr74 = getelementptr inbounds i8, ptr %arraydecay73, i64 0
  store i32 %32, ptr %add.ptr74, align 4
  %arraydecay75 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  %add.ptr76 = getelementptr inbounds i8, ptr %arraydecay75, i64 4
  store i32 1, ptr %add.ptr76, align 4
  %arraydecay77 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  %add.ptr78 = getelementptr inbounds i8, ptr %arraydecay77, i64 60
  store i32 1, ptr %add.ptr78, align 4
  %33 = load i32, ptr %fd, align 4
  %arraydecay79 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  %call80 = call i32 (i32, i64, ...) @ioctl(i32 %33, i64 3227014665, ptr %arraydecay79)
  %cmp81 = icmp slt i32 %call80, 0
  br i1 %cmp81, label %if.then83, label %if.end86

if.then83:                                        ; preds = %for.body
  %call84 = call ptr @cancho_errno_location()
  %34 = load i32, ptr %call84, align 4
  %conv85 = sext i32 %34 to i64
  store i64 %conv85, ptr %retval, align 8
  br label %return

if.end86:                                         ; preds = %for.body
  %arraydecay87 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  %add.ptr88 = getelementptr inbounds i8, ptr %arraydecay87, i64 72
  %35 = load i32, ptr %add.ptr88, align 4
  store i32 %35, ptr %length, align 4
  %arraydecay89 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  %add.ptr90 = getelementptr inbounds i8, ptr %arraydecay89, i64 64
  %36 = load i32, ptr %add.ptr90, align 4
  store i32 %36, ptr %offset, align 4
  %37 = load i32, ptr %length, align 4
  %conv91 = zext i32 %37 to i64
  %38 = load i32, ptr %fd, align 4
  %39 = load i32, ptr %offset, align 4
  %conv92 = zext i32 %39 to i64
  %call93 = call ptr @mmap(ptr null, i64 %conv91, i32 3, i32 1, i32 %38, i64 %conv92)
  store ptr %call93, ptr %at, align 8
  %40 = load ptr, ptr %at, align 8
  %cmp94 = icmp eq ptr %40, inttoptr (i64 -1 to ptr)
  br i1 %cmp94, label %if.then96, label %if.end99

if.then96:                                        ; preds = %if.end86
  %call97 = call ptr @cancho_errno_location()
  %41 = load i32, ptr %call97, align 4
  %conv98 = sext i32 %41 to i64
  store i64 %conv98, ptr %retval, align 8
  br label %return

if.end99:                                         ; preds = %if.end86
  %42 = load ptr, ptr %at, align 8
  %43 = ptrtoint ptr %42 to i64
  %44 = load ptr, ptr %s, align 8
  %45 = load i32, ptr %i, align 4
  %mul = mul nsw i32 8, %45
  %add = add nsw i32 32, %mul
  %idx.ext = sext i32 %add to i64
  %add.ptr100 = getelementptr inbounds i8, ptr %44, i64 %idx.ext
  store i64 %43, ptr %add.ptr100, align 8
  %46 = load i32, ptr %length, align 4
  %conv101 = zext i32 %46 to i64
  %47 = load ptr, ptr %s, align 8
  %48 = load i32, ptr %i, align 4
  %mul102 = mul nsw i32 8, %48
  %add103 = add nsw i32 64, %mul102
  %idx.ext104 = sext i32 %add103 to i64
  %add.ptr105 = getelementptr inbounds i8, ptr %47, i64 %idx.ext104
  store i64 %conv101, ptr %add.ptr105, align 8
  %49 = load i32, ptr %fd, align 4
  %arraydecay106 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  %call107 = call i32 (i32, i64, ...) @ioctl(i32 %49, i64 3227014671, ptr %arraydecay106)
  %cmp108 = icmp slt i32 %call107, 0
  br i1 %cmp108, label %if.then110, label %if.end113

if.then110:                                       ; preds = %if.end99
  %call111 = call ptr @cancho_errno_location()
  %50 = load i32, ptr %call111, align 4
  %conv112 = sext i32 %50 to i64
  store i64 %conv112, ptr %retval, align 8
  br label %return

if.end113:                                        ; preds = %if.end99
  br label %for.inc

for.inc:                                          ; preds = %if.end113
  %51 = load i32, ptr %i, align 4
  %inc = add nsw i32 %51, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  store i32 1, ptr %type, align 4
  %52 = load i32, ptr %fd, align 4
  %call114 = call i32 (i32, i64, ...) @ioctl(i32 %52, i64 1074026002, ptr %type)
  %cmp115 = icmp slt i32 %call114, 0
  br i1 %cmp115, label %if.then117, label %if.end120

if.then117:                                       ; preds = %for.end
  %call118 = call ptr @cancho_errno_location()
  %53 = load i32, ptr %call118, align 4
  %conv119 = sext i32 %53 to i64
  store i64 %conv119, ptr %retval, align 8
  br label %return

if.end120:                                        ; preds = %for.end
  store i64 0, ptr %retval, align 8
  br label %return

return:                                           ; preds = %if.end120, %if.then117, %if.then110, %if.then96, %if.then83, %if.then62, %if.then54, %if.then39, %if.then24, %if.then7, %if.then
  %54 = load i64, ptr %retval, align 8
  ret i64 %54
}

; Function Attrs: noinline nounwind
define internal void @cancho_camera_zero(ptr %p, i32 %words) {
entry:
  %p.addr = alloca ptr, align 8
  %words.addr = alloca i32, align 4
  %i = alloca i32, align 4
  store ptr %p, ptr %p.addr, align 8
  store i32 %words, ptr %words.addr, align 4
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %entry
  %0 = load i32, ptr %i, align 4
  %1 = load i32, ptr %words.addr, align 4
  %cmp = icmp slt i32 %0, %1
  br i1 %cmp, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %2 = load ptr, ptr %p.addr, align 8
  %3 = load i32, ptr %i, align 4
  %idxprom = sext i32 %3 to i64
  %arrayidx = getelementptr inbounds i64, ptr %2, i64 %idxprom
  store i64 0, ptr %arrayidx, align 8
  br label %for.inc

for.inc:                                          ; preds = %for.body
  %4 = load i32, ptr %i, align 4
  %inc = add nsw i32 %4, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  ret void
}

declare i32 @ioctl(i32, i64, ...)

declare ptr @mmap(ptr, i64, i32, i32, i32, i64)

; Function Attrs: noinline nounwind
define i64 @cancho_camera_next(ptr %cam, ptr %into, i64 %n) {
entry:
  %retval = alloca i64, align 8
  %cam.addr = alloca ptr, align 8
  %into.addr = alloca ptr, align 8
  %n.addr = alloca i64, align 8
  %s = alloca ptr, align 8
  %fd = alloca i32, align 4
  %have = alloca i32, align 4
  %err = alloca i32, align 4
  %buf = alloca [11 x i64], align 8
  %index = alloca i32, align 4
  %older = alloca [11 x i64], align 8
  %total = alloca i64, align 8
  %count = alloca i64, align 8
  %frame = alloca ptr, align 8
  %k = alloca i64, align 8
  %back = alloca [11 x i64], align 8
  store ptr %cam, ptr %cam.addr, align 8
  store ptr %into, ptr %into.addr, align 8
  store i64 %n, ptr %n.addr, align 8
  %0 = load ptr, ptr %cam.addr, align 8
  %1 = load i64, ptr %0, align 8
  %2 = inttoptr i64 %1 to ptr
  store ptr %2, ptr %s, align 8
  %3 = load ptr, ptr %s, align 8
  %add.ptr = getelementptr inbounds i8, ptr %3, i64 8
  %4 = load i64, ptr %add.ptr, align 8
  %cmp = icmp eq i64 %4, 0
  br i1 %cmp, label %if.then, label %if.end

if.then:                                          ; preds = %entry
  store i64 -1, ptr %retval, align 8
  br label %return

if.end:                                           ; preds = %entry
  %5 = load ptr, ptr %s, align 8
  %add.ptr1 = getelementptr inbounds i8, ptr %5, i64 0
  %6 = load i32, ptr %add.ptr1, align 4
  store i32 %6, ptr %fd, align 4
  store i32 -1, ptr %have, align 4
  store i32 0, ptr %err, align 4
  br label %for.cond

for.cond:                                         ; preds = %if.end24, %if.end
  %arraydecay = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  call void @cancho_camera_zero(ptr %arraydecay, i32 11)
  %arraydecay2 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  %add.ptr3 = getelementptr inbounds i8, ptr %arraydecay2, i64 4
  store i32 1, ptr %add.ptr3, align 4
  %arraydecay4 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  %add.ptr5 = getelementptr inbounds i8, ptr %arraydecay4, i64 60
  store i32 1, ptr %add.ptr5, align 4
  %7 = load i32, ptr %fd, align 4
  %arraydecay6 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  %call = call i32 (i32, i64, ...) @ioctl(i32 %7, i64 3227014673, ptr %arraydecay6)
  %cmp7 = icmp slt i32 %call, 0
  br i1 %cmp7, label %if.then8, label %if.end10

if.then8:                                         ; preds = %for.cond
  %call9 = call ptr @cancho_errno_location()
  %8 = load i32, ptr %call9, align 4
  store i32 %8, ptr %err, align 4
  br label %for.end

if.end10:                                         ; preds = %for.cond
  %arraydecay11 = getelementptr inbounds [11 x i64], ptr %buf, i64 0, i64 0
  %add.ptr12 = getelementptr inbounds i8, ptr %arraydecay11, i64 0
  %9 = load i32, ptr %add.ptr12, align 4
  store i32 %9, ptr %index, align 4
  %10 = load i32, ptr %have, align 4
  %cmp13 = icmp sge i32 %10, 0
  br i1 %cmp13, label %if.then14, label %if.end24

if.then14:                                        ; preds = %if.end10
  %arraydecay15 = getelementptr inbounds [11 x i64], ptr %older, i64 0, i64 0
  call void @cancho_camera_zero(ptr %arraydecay15, i32 11)
  %11 = load i32, ptr %have, align 4
  %arraydecay16 = getelementptr inbounds [11 x i64], ptr %older, i64 0, i64 0
  %add.ptr17 = getelementptr inbounds i8, ptr %arraydecay16, i64 0
  store i32 %11, ptr %add.ptr17, align 4
  %arraydecay18 = getelementptr inbounds [11 x i64], ptr %older, i64 0, i64 0
  %add.ptr19 = getelementptr inbounds i8, ptr %arraydecay18, i64 4
  store i32 1, ptr %add.ptr19, align 4
  %arraydecay20 = getelementptr inbounds [11 x i64], ptr %older, i64 0, i64 0
  %add.ptr21 = getelementptr inbounds i8, ptr %arraydecay20, i64 60
  store i32 1, ptr %add.ptr21, align 4
  %12 = load i32, ptr %fd, align 4
  %arraydecay22 = getelementptr inbounds [11 x i64], ptr %older, i64 0, i64 0
  %call23 = call i32 (i32, i64, ...) @ioctl(i32 %12, i64 3227014671, ptr %arraydecay22)
  br label %if.end24

if.end24:                                         ; preds = %if.then14, %if.end10
  %13 = load i32, ptr %index, align 4
  store i32 %13, ptr %have, align 4
  br label %for.cond

for.end:                                          ; preds = %if.then8
  %14 = load i32, ptr %have, align 4
  %cmp25 = icmp slt i32 %14, 0
  br i1 %cmp25, label %if.then26, label %if.end28

if.then26:                                        ; preds = %for.end
  %15 = load i32, ptr %err, align 4
  %cmp27 = icmp eq i32 %15, 11
  %16 = zext i1 %cmp27 to i64
  %cond = select i1 %cmp27, i32 0, i32 -1
  %conv = sext i32 %cond to i64
  store i64 %conv, ptr %retval, align 8
  br label %return

if.end28:                                         ; preds = %for.end
  %17 = load i32, ptr %have, align 4
  %cmp29 = icmp sge i32 %17, 4
  br i1 %cmp29, label %if.then31, label %if.end32

if.then31:                                        ; preds = %if.end28
  store i64 -1, ptr %retval, align 8
  br label %return

if.end32:                                         ; preds = %if.end28
  %18 = load ptr, ptr %s, align 8
  %add.ptr33 = getelementptr inbounds i8, ptr %18, i64 8
  %19 = load i64, ptr %add.ptr33, align 8
  %20 = load ptr, ptr %s, align 8
  %add.ptr34 = getelementptr inbounds i8, ptr %20, i64 16
  %21 = load i64, ptr %add.ptr34, align 8
  %mul = mul nsw i64 %19, %21
  store i64 %mul, ptr %total, align 8
  %22 = load i64, ptr %n.addr, align 8
  %23 = load i64, ptr %total, align 8
  %cmp35 = icmp slt i64 %22, %23
  br i1 %cmp35, label %cond.true, label %cond.false

cond.true:                                        ; preds = %if.end32
  %24 = load i64, ptr %n.addr, align 8
  br label %cond.end

cond.false:                                       ; preds = %if.end32
  %25 = load i64, ptr %total, align 8
  br label %cond.end

cond.end:                                         ; preds = %cond.false, %cond.true
  %cond37 = phi i64 [ %24, %cond.true ], [ %25, %cond.false ]
  store i64 %cond37, ptr %count, align 8
  %26 = load ptr, ptr %s, align 8
  %27 = load i32, ptr %have, align 4
  %mul38 = mul nsw i32 8, %27
  %add = add nsw i32 32, %mul38
  %idx.ext = sext i32 %add to i64
  %add.ptr39 = getelementptr inbounds i8, ptr %26, i64 %idx.ext
  %28 = load i64, ptr %add.ptr39, align 8
  %29 = inttoptr i64 %28 to ptr
  store ptr %29, ptr %frame, align 8
  store i64 0, ptr %k, align 8
  br label %for.cond40

for.cond40:                                       ; preds = %for.inc, %cond.end
  %30 = load i64, ptr %k, align 8
  %31 = load i64, ptr %count, align 8
  %cmp41 = icmp slt i64 %30, %31
  br i1 %cmp41, label %for.body, label %for.end45

for.body:                                         ; preds = %for.cond40
  %32 = load ptr, ptr %frame, align 8
  %33 = load i64, ptr %k, align 8
  %mul43 = mul nsw i64 2, %33
  %arrayidx = getelementptr inbounds i8, ptr %32, i64 %mul43
  %34 = load i8, ptr %arrayidx, align 1
  %35 = load ptr, ptr %into.addr, align 8
  %36 = load i64, ptr %k, align 8
  %arrayidx44 = getelementptr inbounds i8, ptr %35, i64 %36
  store i8 %34, ptr %arrayidx44, align 1
  br label %for.inc

for.inc:                                          ; preds = %for.body
  %37 = load i64, ptr %k, align 8
  %inc = add nsw i64 %37, 1
  store i64 %inc, ptr %k, align 8
  br label %for.cond40

for.end45:                                        ; preds = %for.cond40
  %arraydecay46 = getelementptr inbounds [11 x i64], ptr %back, i64 0, i64 0
  call void @cancho_camera_zero(ptr %arraydecay46, i32 11)
  %38 = load i32, ptr %have, align 4
  %arraydecay47 = getelementptr inbounds [11 x i64], ptr %back, i64 0, i64 0
  %add.ptr48 = getelementptr inbounds i8, ptr %arraydecay47, i64 0
  store i32 %38, ptr %add.ptr48, align 4
  %arraydecay49 = getelementptr inbounds [11 x i64], ptr %back, i64 0, i64 0
  %add.ptr50 = getelementptr inbounds i8, ptr %arraydecay49, i64 4
  store i32 1, ptr %add.ptr50, align 4
  %arraydecay51 = getelementptr inbounds [11 x i64], ptr %back, i64 0, i64 0
  %add.ptr52 = getelementptr inbounds i8, ptr %arraydecay51, i64 60
  store i32 1, ptr %add.ptr52, align 4
  %39 = load i32, ptr %fd, align 4
  %arraydecay53 = getelementptr inbounds [11 x i64], ptr %back, i64 0, i64 0
  %call54 = call i32 (i32, i64, ...) @ioctl(i32 %39, i64 3227014671, ptr %arraydecay53)
  %40 = load i64, ptr %count, align 8
  store i64 %40, ptr %retval, align 8
  br label %return

return:                                           ; preds = %for.end45, %if.then31, %if.then26, %if.then
  %41 = load i64, ptr %retval, align 8
  ret i64 %41
}

; Function Attrs: noinline nounwind
define i64 @cancho_camera_close(i64 %handle) {
entry:
  %handle.addr = alloca i64, align 8
  %s = alloca ptr, align 8
  %fd = alloca i32, align 4
  %type = alloca i32, align 4
  %i = alloca i32, align 4
  %at = alloca i64, align 8
  %r = alloca i32, align 4
  store i64 %handle, ptr %handle.addr, align 8
  %0 = load i64, ptr %handle.addr, align 8
  %1 = inttoptr i64 %0 to ptr
  store ptr %1, ptr %s, align 8
  %2 = load ptr, ptr %s, align 8
  %add.ptr = getelementptr inbounds i8, ptr %2, i64 0
  %3 = load i32, ptr %add.ptr, align 4
  store i32 %3, ptr %fd, align 4
  %4 = load ptr, ptr %s, align 8
  %add.ptr1 = getelementptr inbounds i8, ptr %4, i64 8
  %5 = load i64, ptr %add.ptr1, align 8
  %cmp = icmp ne i64 %5, 0
  br i1 %cmp, label %if.then, label %if.end11

if.then:                                          ; preds = %entry
  store i32 1, ptr %type, align 4
  %6 = load i32, ptr %fd, align 4
  %call = call i32 (i32, i64, ...) @ioctl(i32 %6, i64 1074026003, ptr %type)
  store i32 0, ptr %i, align 4
  br label %for.cond

for.cond:                                         ; preds = %for.inc, %if.then
  %7 = load i32, ptr %i, align 4
  %cmp2 = icmp slt i32 %7, 4
  br i1 %cmp2, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %8 = load ptr, ptr %s, align 8
  %9 = load i32, ptr %i, align 4
  %mul = mul nsw i32 8, %9
  %add = add nsw i32 32, %mul
  %idx.ext = sext i32 %add to i64
  %add.ptr3 = getelementptr inbounds i8, ptr %8, i64 %idx.ext
  %10 = load i64, ptr %add.ptr3, align 8
  store i64 %10, ptr %at, align 8
  %11 = load i64, ptr %at, align 8
  %cmp4 = icmp ne i64 %11, 0
  br i1 %cmp4, label %if.then5, label %if.end

if.then5:                                         ; preds = %for.body
  %12 = load i64, ptr %at, align 8
  %13 = inttoptr i64 %12 to ptr
  %14 = load ptr, ptr %s, align 8
  %15 = load i32, ptr %i, align 4
  %mul6 = mul nsw i32 8, %15
  %add7 = add nsw i32 64, %mul6
  %idx.ext8 = sext i32 %add7 to i64
  %add.ptr9 = getelementptr inbounds i8, ptr %14, i64 %idx.ext8
  %16 = load i64, ptr %add.ptr9, align 8
  %call10 = call i32 @munmap(ptr %13, i64 %16)
  br label %if.end

if.end:                                           ; preds = %if.then5, %for.body
  br label %for.inc

for.inc:                                          ; preds = %if.end
  %17 = load i32, ptr %i, align 4
  %inc = add nsw i32 %17, 1
  store i32 %inc, ptr %i, align 4
  br label %for.cond

for.end:                                          ; preds = %for.cond
  br label %if.end11

if.end11:                                         ; preds = %for.end, %entry
  %18 = load i32, ptr %fd, align 4
  %call12 = call i32 @close(i32 %18)
  store i32 %call12, ptr %r, align 4
  %19 = load ptr, ptr %s, align 8
  call void @free(ptr %19)
  %20 = load i32, ptr %r, align 4
  %conv = sext i32 %20 to i64
  ret i64 %conv
}

declare i32 @munmap(ptr, i64)



