export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null 2>&1; apt-get install -y -qq gcc libc6-dev nfs-kernel-server nfs-common ntfs-3g >/tmp/apt.log 2>&1
gcc -O1 -o /p /s/rename_noreplace_probe.c || exit 1
t(){ printf "%-34s " "$1"; /p "$2"; }
mkdir -p /exp /mnt/nfs3 /mnt/nfs4 /mnt/ntfs
echo "== ntfs-3g (FUSE)"; truncate -s 100M /n.img; mkntfs -F -q /n.img >/dev/null 2>&1; ntfs-3g -o loop /n.img /mnt/ntfs 2>&1 | tail -1; t "ntfs-3g (FUSE)" /mnt/ntfs
echo "== nfs"; mount -t tmpfs tmpfs /exp
modprobe nfsd 2>&1 | tail -1; mount -t nfsd nfsd /proc/fs/nfsd 2>&1 | tail -1
echo '/exp *(rw,no_root_squash,fsid=0,insecure,no_subtree_check)' > /etc/exports
rpcbind 2>&1 | tail -1; exportfs -ra 2>&1 | tail -1; rpc.nfsd 2>&1 | tail -1; rpc.mountd 2>&1 | tail -1; sleep 2
mount -t nfs -o vers=3,nolock 127.0.0.1:/exp /mnt/nfs3 2>&1 | tail -1 && t "nfs v3" /mnt/nfs3
mount -t nfs -o vers=4.2 127.0.0.1:/ /mnt/nfs4 2>&1 | tail -1 && t "nfs v4.2" /mnt/nfs4
grep -E " (nfs|fuse)" /proc/mounts
