#!/bin/bash
# The T3 baseline of docs/parallelism.md: one process and two processes sharing a port (SO_REUSEPORT) of lexsys-web's
# in-memory `users` service, on a workload that keeps no state (POST /users with an invalid body: the whole JSON parse,
# validation and error rendering, no store). Copy i is pinned to core i, the load generator to cores 2 and 3, so two
# copies is the most this 4-core machine can run without the load generator sharing a core.
#
#   benches/parallel/copies_users.sh /path/to/users-built-with-listen-flags-1 /path/to/kload
#
# `users` listens with flags 0; build the measurement variant with
#     sed 's/tcp_listen(nn, port, 1024, 0)/tcp_listen(nn, port, 1024, 1)/' examples/users/users.ls > users_reuse.ls
# and the packages fetched by lexsys-web's scripts/build.sh.
bin=${1:?users binary}; kload=${2:-/tmp/kload}
for n in 1 2; do
  port=$((19900 + n)); pids=()
  for i in $(seq 0 $((n - 1))); do taskset -c $i "$bin" $port > /dev/null 2>&1 & pids+=($!); sleep 0.3; done
  sleep 0.5
  runs=()
  for r in 1 2 3 4 5; do
    runs+=("$(KLOAD_EXPECT=422 taskset -c 2,3 "$kload" $port 2 16 4 /users - POST '{"name":""}')")
  done
  for p in "${pids[@]}"; do kill "$p" 2> /dev/null; wait "$p" 2> /dev/null; done
  printf '%s process(es): %s\n' $n "$(printf '%s\n' "${runs[@]}" | sort -n | tr '\n' ' ')"
done
