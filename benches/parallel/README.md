# `benches/parallel/`

The programs behind `docs/parallelism.md`'s measurements. Nothing here is a benchmark *pair* (`benches/README.md`):
these are one-off experiments, kept so that every number in that document can be reproduced. Times are the minimum
of several runs, the program pinned to one core with `taskset -c 2` (the thread experiment to `0-3`).

| file | what it measures |
|---|---|
| `reduce_limbs.ls` | a checked sum that vectorises: low and high 32 bits summed separately, one checked step per block |
| `reduce_proved_abs.ls`, `reduce_proved.ls` | a checked sum that proves a block cannot overflow (largest absolute value; then max and min by comparisons) and sums it unchecked |
| `proved.c` | the same idea in C, to see what a better target does: `clang -O2` and `clang -O2 -mavx2` |
| `colscan.ls`, `colscan_duckdb.py` | a sum and a filtered count over 50 million `int64`, in lex-sys and in DuckDB, on the same values |
| `gen_threads.py` | writes the 1-, 2- and 4-thread programs of the thread-scaling experiment |
| `copies_users.sh` | the T3 baseline: one and two processes sharing a port, on a stateless workload of `lexsys-web`'s `users` service |
| `threads_users.sh` | T3: the same workload, two processes against two threads (`examples/users_threads`), interleaved in rounds |

The `reduce_*` programs are `benches/reduce_checked.ls` with the `run` function replaced; build them with
`lex-sys build --std --backend llvm`, and for the cache-resident variant (128 KB, the same total work):

```sh
sed -e 's/fill(h, 1000000)/fill(h, 16384)/; s/run(contents(b), 200)/run(contents(b), 12207)/; s/total - (0 - 200)/total - (0 - 12207)/' \
    benches/reduce_checked.ls > /tmp/small_reduce_checked.ls
```

`lex-sys build --std --backend llvm` compiles with `clang -c -O2 -target <triple>`: no `-march`, so the baseline of
the architecture (SSE2 on x86-64). SIMD instructions are counted with `objdump -d <exe> | grep -cE 'xmm|ymm'`.
