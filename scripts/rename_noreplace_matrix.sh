export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null 2>&1; apt-get install -y -qq gcc libc6-dev e2fsprogs xfsprogs btrfs-progs dosfstools exfatprogs f2fs-tools ntfs-3g >/tmp/apt.log 2>&1 || apt-get install -y -qq gcc libc6-dev e2fsprogs xfsprogs dosfstools >>/tmp/apt.log 2>&1
gcc -O1 -o /p /s/rename_noreplace_probe.c || exit 1
uname -mr; ldd --version | head -1
t(){ printf "%-34s " "$1"; /p "$2"; }
mkdir -p /m/tmpfs && mount -t tmpfs tmpfs /m/tmpfs && t tmpfs /m/tmpfs
mkdir -p /root/o && t "container rootfs ($(stat -f -c %T /root/o))" /root/o
for fs in ext4 xfs btrfs vfat exfat f2fs; do
  command -v mkfs.$fs >/dev/null || { echo "$fs: no mkfs"; continue; }
  truncate -s 300M /img.$fs; case $fs in ext4|xfs|btrfs|f2fs) mkfs.$fs -q /img.$fs >/dev/null 2>&1;; vfat) mkfs.vfat /img.$fs >/dev/null;; exfat) mkfs.exfat /img.$fs >/dev/null;; esac
  mkdir -p /m/$fs; if mount -o loop /img.$fs /m/$fs 2>/dev/null; then t $fs /m/$fs; else echo "$fs: mount failed"; fi
done
mkdir -p /m/ov/{l,u,w,m} && mount -t overlay overlay -o lowerdir=/m/ov/l,upperdir=/m/ov/u,workdir=/m/ov/w /m/ov/m && t overlayfs /m/ov/m
t "host bind mount ($(stat -f -c %T /s))" /s
