#!/bin/bash
# Throughput of an HTTP server under closed-loop keep-alive load (docs/server.md §5).
#
#   benches/server/bench.sh <label> <load-cores> <server command>...
#
# Starts each server command (give each one `$PORT` for its port), waits for the
# port, runs three 5-second rounds of `kload` -- 2 threads x 16 connections, one
# request outstanding per connection, `GET /users/42` -- pinned to <load-cores>,
# prints the three requests-a-second figures, and stops the servers. Pin the
# servers yourself (`taskset -c 0 ...`), on cores the load does not use.
#
#   gcc -O2 -o kload benches/server/kload.c -lpthread   (a sixth argument, `lat`, adds latency percentiles)
#   gcc -O2 -o cpoll benches/server/cpoll.c
#   PORT=19001 benches/server/bench.sh "cancho" 2,3 "taskset -c 0 ./api 19001"
#
# `./api` is `examples/api/api.cho` plus the fetched `http.server` package, built as
# `examples/README.md` shows. The
# FastAPI baseline is `benches/server/app.py` under
# `python3 -m uvicorn app:app --port $PORT` (add `--loop uvloop --http httptools`
# for the faster setup).
label=$1; lcores=$2; shift 2
PORT=${PORT:-19000}
pids=()
for cmd in "$@"; do
  eval "$cmd" >/dev/null 2>&1 &
  pids+=($!)
done
for _ in $(seq 1 100); do (echo > /dev/tcp/127.0.0.1/"$PORT") 2>/dev/null && break; sleep 0.1; done
sleep 0.5
res=()
for _ in 1 2 3; do res+=("$(taskset -c "$lcores" ./kload "$PORT" 2 16 5 /users/42)"); done
echo "$label: ${res[*]}"
for p in "${pids[@]}"; do kill "$p" 2>/dev/null; done
pkill -f "uvicorn app:app --port $PORT" 2>/dev/null
sleep 0.5
