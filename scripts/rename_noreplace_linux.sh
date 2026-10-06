# Linux run of slice 4's conformance test, on the filesystems that matter:
# the container's own (overlayfs), then an NFS v3 mount and an ntfs-3g mount,
# which have no RENAME_NOREPLACE. Needs --privileged and --device /dev/fuse.
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null 2>&1; apt-get install -y -qq nfs-kernel-server nfs-common ntfs-3g >/tmp/apt.log 2>&1
mkdir -p /exp /mnt/nfs3 /mnt/ntfs
truncate -s 100M /n.img; mkntfs -F -q /n.img >/dev/null 2>&1; ntfs-3g -o loop /n.img /mnt/ntfs
mount -t tmpfs tmpfs /exp; mount -t nfsd nfsd /proc/fs/nfsd
echo '/exp *(rw,no_root_squash,fsid=0,insecure,no_subtree_check)' > /etc/exports
rpcbind; exportfs -ra; rpc.nfsd; rpc.mountd; sleep 2
mount -t nfs -o vers=3,nolock 127.0.0.1:/exp /mnt/nfs3
cd /w
for dir in "" /mnt/nfs3 /mnt/ntfs; do
  echo "=== LEX_SYS_RENAME_UNSUPPORTED_DIR=$dir"
  LEX_SYS_RENAME_UNSUPPORTED_DIR=$dir cargo test -p lex-sys --test conformance directory_rename_new -- --nocapture "$@" 2>&1 | grep -E "^test |won by|not set|panicked|trial|left:|right:|result"
done
