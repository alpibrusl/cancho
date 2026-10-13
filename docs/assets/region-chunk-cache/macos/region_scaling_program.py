import sys
T = int(sys.argv[1]); N = int(sys.argv[2])
spawns = "".join(f"    let h{t} = spawn({N}, w);\n" for t in range(T))
joins = "".join(f" + join(h{t})" for t in range(T))
print(f"""edition 4;
fn sum[&r](s: &r [int]) -> [] int {{
    var t = 0;
    var i = 0;
    while i < len(s) {{ t = t + s[i]; i = i + 1; }}
    return t;
}}
fn work(n: int) -> [] int {{
    var bad = 0;
    var i = 0;
    while i < n {{
        region a {{
            let s = alloc_slice[a](80, i % 7);
            if sum(s) != 80 * (i % 7) {{ bad = bad + 1; }}
        }}
        i = i + 1;
    }}
    return bad;
}}
fn worker(n: int) -> [] int {{ return work(n); }}
fn main(world: World) -> [conc] int {{
    let Split {{ io, ffi, fs, heap, args, net }} = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net);
    let w = worker;
{spawns}    return 0{joins};
}}""")
