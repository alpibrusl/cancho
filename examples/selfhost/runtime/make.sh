#!/bin/sh
# Rebuilds net_linux.ll and net_darwin.ll from net.c: no target, no attributes, nothing that is not portable.
cd "$(dirname "$0")" || exit 1
for os in linux:0 darwin:1; do
  name=${os%%:*}; flag=${os##*:}
  clang -S -emit-llvm -O0 -Xclang -disable-O0-optnone -DDARWIN=$flag -fno-unwind-tables -fno-asynchronous-unwind-tables -fno-stack-protector -fno-discard-value-names -target arm64-apple-macos net.c -o - |
    grep -v '^target \|^source_filename\|^!\|^attributes #\|^; ModuleID\|^declare i32 @close(\|^declare i32 @fcntl(\|^declare ptr @cancho_errno_location\|^declare void @llvm.trap' |
    sed -e 's/ #[0-9]* {/ {/' -e 's/ #[0-9]*$//' -e 's/, !tbaa ![0-9]*//' -e 's/, !llvm.loop ![0-9]*//' -e 's/, !range ![0-9]*//' \
        -e 's/ local_unnamed_addr//g' -e 's/ noundef//g' -e 's/ nocapture//g' -e 's/ readonly//g' -e 's/ writeonly//g' -e 's/ nonnull//g' \
        -e 's/ noalias//g' -e 's/ nsw//g' -e 's/ nuw//g' -e 's/ dso_local//g' -e 's/ signext//g' -e 's/ zeroext//g' -e 's/ captures([a-z, ]*)//g' \
        -e 's/ memory([a-z: ,]*)//g' -e 's/ nofree//g' -e 's/ willreturn//g' -e 's/ initializes([^)]*)//g' > net_$name.ll
done
