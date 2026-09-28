//~ ERROR `spawn`'s payload has type `Point`, which cannot cross to a real thread yet
//~ RULE thread-payload-type

// `docs/threads.md` §1/§5 step 2: the single-leaf slice's own wall.
// `pthread_create`'s start routine is `void *(*)(void *)` -- one
// pointer-width leaf in, one out -- and nothing in this compiler yet
// synthesises a trampoline that could spill a multi-field struct like
// `Point` across that one argument. `spawn`'s payload (and `body`'s
// return type, checked the same way) is refused before it ever reaches
// codegen, not silently truncated or miscompiled.

edition 4;

struct Point { x: int, y: int }

fn worker(p: Point) -> [] int {
    return p.x;
}

fn main() -> [conc] int {
    let p = Point { x: 1, y: 2 };
    let w = worker;
    let h = spawn(p, w);
    return join(h);
}
