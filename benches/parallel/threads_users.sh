#!/bin/bash
# T3 of docs/parallelism.md: two processes sharing a port against two threads in one process, on the stateless workload
# of `copies_users.sh`, interleaved in rounds so the machine's drift hits both. Server threads/processes on cores 0 and 1,
# the load generator on cores 2 and 3.
#
#   benches/parallel/threads_users.sh /path/to/users_reuse /path/to/users_threads /path/to/kload [rounds]
#
# users_reuse   `examples/users/users.ls` built with `tcp_listen(nn, port, 1024, 1)` (see copies_users.sh)
# users_threads `examples/users_threads/users_threads.ls`, built with lexsys-web's scripts/build.sh
procs=${1:?users_reuse binary}; threads=${2:?users_threads binary}; kload=${3:-/tmp/kload}; rounds=${4:-3}
load() { for r in 1 2 3 4 5; do KLOAD_EXPECT=422 taskset -c 2,3 "$kload" "$1" 2 16 4 /users - POST '{"name":""}'; done | sort -n | tr '\n' ' '; }
for round in $(seq 1 "$rounds"); do
  port=$((19970 + round)); pids=()
  for i in 0 1; do taskset -c $i "$procs" $port > /dev/null 2>&1 & pids+=($!); sleep 0.3; done
  sleep 0.5; printf 'round %s  2 processes: %s\n' "$round" "$(load $port)"
  for p in "${pids[@]}"; do kill "$p" 2> /dev/null; wait "$p" 2> /dev/null; done
  port=$((19980 + round)); taskset -c 0,1 "$threads" $port > /dev/null 2>&1 & pid=$!
  sleep 1; printf 'round %s  2 threads:   %s\n' "$round" "$(load $port)"
  kill "$pid" 2> /dev/null; wait "$pid" 2> /dev/null
done
