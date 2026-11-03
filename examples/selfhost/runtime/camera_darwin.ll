
; Function Attrs: noinline nounwind
define void @cancho_camera_open(ptr %src, i64 %len, ptr %prefix, i64 %plen, ptr %out) {
entry:
  %src.addr = alloca ptr, align 8
  %len.addr = alloca i64, align 8
  %prefix.addr = alloca ptr, align 8
  %plen.addr = alloca i64, align 8
  %out.addr = alloca ptr, align 8
  store ptr %src, ptr %src.addr, align 8
  store i64 %len, ptr %len.addr, align 8
  store ptr %prefix, ptr %prefix.addr, align 8
  store i64 %plen, ptr %plen.addr, align 8
  store ptr %out, ptr %out.addr, align 8
  %0 = load ptr, ptr %out.addr, align 8
  %arrayidx = getelementptr inbounds i64, ptr %0, i64 0
  store i64 1, ptr %arrayidx, align 8
  %1 = load ptr, ptr %out.addr, align 8
  %arrayidx1 = getelementptr inbounds i64, ptr %1, i64 1
  store i64 -1, ptr %arrayidx1, align 8
  %2 = load ptr, ptr %out.addr, align 8
  %arrayidx2 = getelementptr inbounds i64, ptr %2, i64 2
  store i64 19, ptr %arrayidx2, align 8
  ret void
}

; Function Attrs: noinline nounwind
define i64 @cancho_camera_configure(ptr %cam, i64 %width, i64 %height) {
entry:
  %cam.addr = alloca ptr, align 8
  %width.addr = alloca i64, align 8
  %height.addr = alloca i64, align 8
  store ptr %cam, ptr %cam.addr, align 8
  store i64 %width, ptr %width.addr, align 8
  store i64 %height, ptr %height.addr, align 8
  ret i64 -1
}

; Function Attrs: noinline nounwind
define i64 @cancho_camera_next(ptr %cam, ptr %into, i64 %n) {
entry:
  %cam.addr = alloca ptr, align 8
  %into.addr = alloca ptr, align 8
  %n.addr = alloca i64, align 8
  store ptr %cam, ptr %cam.addr, align 8
  store ptr %into, ptr %into.addr, align 8
  store i64 %n, ptr %n.addr, align 8
  ret i64 -1
}

; Function Attrs: noinline nounwind
define i64 @cancho_camera_close(i64 %handle) {
entry:
  %handle.addr = alloca i64, align 8
  store i64 %handle, ptr %handle.addr, align 8
  ret i64 -1
}



