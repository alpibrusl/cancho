import duckdb, time
con = duckdb.connect(":memory:")
con.execute("create table t as select ((i * 2654435761) >> 7) & 1023 as x from range(50000000) r(i)")
print("rows", con.execute("select count(*) from t").fetchone()[0], " type", con.execute("select typeof(x) from t limit 1").fetchone()[0])
def best(q, threads, reps=5):
    con.execute(f"pragma threads={threads}")
    r = None; b = 1e9
    for _ in range(reps):
        t = time.perf_counter(); r = con.execute(q).fetchone()[0]; b = min(b, (time.perf_counter() - t) * 1000)
    return b, r
for threads in (1, 2, 4):
    for name, q in (("sum(x)", "select sum(x) from t"), ("count where x>500", "select count(*) from t where x > 500")):
        ms, r = best(q, threads)
        print(f"duckdb {threads} thread(s): {name:20s} {ms:7.1f} ms  answer {r}")
