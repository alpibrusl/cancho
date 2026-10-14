#!/bin/sh
# Rebuilds net_linux.ll and net_darwin.ll from net.c, and dirs.ll from dirs.c: no target, no attributes, nothing that is not portable.
cd "$(dirname "$0")" || exit 1
for os in linux:0 darwin:1; do
  name=${os%%:*}; flag=${os##*:}
  clang -S -emit-llvm -O0 -Xclang -disable-O0-optnone -DDARWIN=$flag -fno-unwind-tables -fno-asynchronous-unwind-tables -fno-stack-protector -fno-discard-value-names -target arm64-apple-macos net.c -o - |
    grep -v '^target \|^source_filename\|^!\|^attributes #\|^; ModuleID\|^declare i32 @close(\|^declare i32 @fcntl(\|^declare i32 @kqueue(\|^declare i32 @kevent(\|^declare ptr @cancho_errno_location\|^declare void @llvm.trap' |
    sed -e 's/ #[0-9]* {/ {/' -e 's/ #[0-9]*$//' -e 's/, !tbaa ![0-9]*//' -e 's/, !llvm.loop ![0-9]*//' -e 's/, !range ![0-9]*//' \
        -e 's/ local_unnamed_addr//g' -e 's/ noundef//g' -e 's/ nocapture//g' -e 's/ readonly//g' -e 's/ writeonly//g' -e 's/ nonnull//g' \
        -e 's/ noalias//g' -e 's/ nsw//g' -e 's/ nuw//g' -e 's/ dso_local//g' -e 's/ signext//g' -e 's/ zeroext//g' -e 's/ captures([a-z, ]*)//g' \
        -e 's/ memory([a-z: ,]*)//g' -e 's/ nofree//g' -e 's/ willreturn//g' -e 's/ initializes([^)]*)//g' > net_$name.ll
done

# The directory support differs by system only in the call that renames without replacing; the rest of what
# differs (flags, offsets, errno values) is the array the compiler writes for the target.
for os in linux:0 darwin:1; do
  name=${os%%:*}; flag=${os##*:}
  clang -S -emit-llvm -O0 -Xclang -disable-O0-optnone -DDARWIN=$flag -Wno-incompatible-library-redeclaration -fno-unwind-tables -fno-asynchronous-unwind-tables -fno-stack-protector -fno-discard-value-names -target arm64-apple-macos dirs.c -o - |
    grep -v '^target \|^source_filename\|^!\|^attributes #\|^; ModuleID\|^declare i32 @close(\|^declare i32 @fsync(\|^declare i32 @openat(\|^declare ptr @cancho_errno_location\|^declare void @llvm.trap\|^@cancho_plat\|^declare ptr @cancho_checked_path\|^declare void @free(\|^declare ptr @memchr(\|^declare ptr @memmove(\|^declare i64 @strlen(' |
    sed -e 's/ #[0-9]* {/ {/' -e 's/ #[0-9]*$//' -e 's/, !llvm.loop ![0-9]*//' -e 's/ noundef//g' -e 's/ dso_local//g' -e 's/ signext//g' -e 's/ zeroext//g' > dirs_$name.ll
done

# The signal claim: the same, for the system's own calls. `kqueue` and `kevent` are declared by `runtime.ll`.
for os in linux:0 darwin:1; do
  name=${os%%:*}; flag=${os##*:}
  clang -S -emit-llvm -O0 -Xclang -disable-O0-optnone -DDARWIN=$flag -fno-unwind-tables -fno-asynchronous-unwind-tables -fno-stack-protector -fno-discard-value-names -target arm64-apple-macos signals.c -o - |
    grep -v '^target \|^source_filename\|^!\|^attributes #\|^; ModuleID\|^declare i32 @close(\|^declare i32 @fcntl(\|^declare i64 @read(\|^declare i32 @kqueue(\|^declare i32 @kevent(\|^declare ptr @cancho_errno_location\|^@cancho_signal_state\|^declare void @llvm.' |
    sed -e 's/ #[0-9]* {/ {/' -e 's/ #[0-9]*$//' -e 's/, !tbaa ![0-9]*//' -e 's/, !llvm.loop ![0-9]*//' -e 's/ noundef//g' -e 's/ dso_local//g' -e 's/ signext//g' -e 's/ zeroext//g' -e 's/ local_unnamed_addr//g' > signals_$name.ll
done

# The serial-port verbs.
for os in linux:0 darwin:1; do
  name=${os%%:*}; flag=${os##*:}
  clang -S -emit-llvm -O0 -Xclang -disable-O0-optnone -DDARWIN=$flag -fno-unwind-tables -fno-asynchronous-unwind-tables -fno-stack-protector -fno-discard-value-names -target arm64-apple-macos tty.c -o - |
    grep -v '^target \|^source_filename\|^!\|^attributes #\|^; ModuleID\|^declare i32 @openat(\|^declare i64 @read(\|^declare i64 @write(\|^declare void @free(\|^declare ptr @cancho_checked_path\|^declare ptr @cancho_errno_location\|^declare void @llvm.' |
    sed -e 's/ #[0-9]* {/ {/' -e 's/ #[0-9]*$//' -e 's/, !tbaa ![0-9]*//' -e 's/, !llvm.loop ![0-9]*//' -e 's/ noundef//g' -e 's/ dso_local//g' -e 's/ signext//g' -e 's/ zeroext//g' -e 's/ local_unnamed_addr//g' > tty_$name.ll
done
rm -f dirs.ll
