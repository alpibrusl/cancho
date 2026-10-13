#!/bin/bash
# usage: sysc.sh <bindir> <srctree> [shim]   -- brk/mmap/munmap a handshake: server full, server resumed, client full (strace -c)
b=$1; src=$2; shim=$3
V=$src/tests/vectors/tls/echo
w=$(mktemp -d); cp $V/first/{chain.pem,key.pem,names} $w/; cd $w
big="--connections 256 --handshakes 256 --rate 100000 --per-address 1024 --per-address-rate 100000"
pre=""; [ -n "$shim" ] && pre="env LD_PRELOAD=$shim CHUNKCOUNT_OUT=$w/chunks"
for mode in full resumed; do
  port=$((20000 + RANDOM % 20000))
  extra=""; [ $mode = resumed ] && extra="--tickets 2"
  taskset -c 0 strace -f -c -e trace=brk,mmap,munmap,madvise -o $w/st.$mode $pre $b/tls_echo --port $port --dir $w $big $extra >/dev/null 2>&1 &
  pid=$!; sleep 1.5
  if [ $mode = full ]; then
    n=$(taskset -c 1 openssl s_time -new -time 5 -CAfile $V/ca.pem -connect 127.0.0.1:$port 2>/dev/null | grep -o '^[0-9]* connections in' | head -1 | cut -d' ' -f1)
  else
    n=$(taskset -c 1 $b/tconn $port 5 $V/ca.pem echo.lex-sys.test resume | awk '{print $1; exit}')
  fi
  kill -TERM $(pgrep -P $pid) 2>/dev/null; kill -TERM $pid 2>/dev/null; wait $pid 2>/dev/null; sleep 0.5
  echo "== server $mode: $n handshakes"; awk -v n=$n '$NF ~ /^(brk|mmap|munmap|madvise)$/ {printf "  %-8s %8d calls  %.2f a handshake\n", $NF, $4, $4/n}' $w/st.$mode
  [ -n "$shim" ] && cat $w/chunks 2>/dev/null
done
port=$((20000 + RANDOM % 20000))
taskset -c 1 openssl s_server -accept $port -tls1_3 -groups X25519 -ciphersuites TLS_AES_128_GCM_SHA256 -cert chain.pem -key key.pem -num_tickets 0 -quiet -HTTP >/dev/null 2>&1 &
sp=$!; sleep 1
for c in 200 1; do
  taskset -c 0 strace -f -c -e trace=brk,mmap,munmap,madvise -o $w/cl.$c $pre $b/tls_many 127.0.0.1 $port echo.lex-sys.test $c 65536 < $V/ca.pem > $w/out.$c 2>&1
  echo "== client tls_many $c: $(grep -o 'failed=[0-9]*' $w/out.$c | head -1)"; awk '$NF ~ /^(brk|mmap|munmap|madvise)$/ {printf "  %-8s %8d calls\n", $NF, $4}' $w/cl.$c
  [ -n "$shim" ] && cat $w/chunks 2>/dev/null
done
kill $sp; rm -rf $w
