// `std.vec` shrinking: `pop`/`remove`/`insert`, next to the growing half
// `tests/accept/collections.ls` already exercises.
//
// `docs/next-phase.md` §4's own duplication check is what surfaces a
// stdlib gap like this one now: nothing hashed or hunted here, an agent
// just asked what the toolchain was missing for real use, and a `Vec`
// that only grows was the answer. `pop`/`remove` are by reference, like
// `get`/`set`/`swap` — nothing they do reallocates, so there is no new
// `Vec[T]` to hand back. `insert` is by value, like `push`, because
// `reserve` may replace the box. All three trap on an out-of-range
// index exactly the way `get`/`set` already do, by the same bounds
// check on the same slice index, not a check written here.
//~ STDOUT 10 15 20 30 40
//~ STDOUT removed 15
//~ STDOUT 10 20 30 40
//~ STDOUT popped 40 30 20 10
//~ STDOUT size 0
//~ EXIT 0

import std.vec;
import std.io as console;

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_write] int {
    var v = vec.empty(heap, 4, 0);
    v = vec.push(heap, v, 10);
    v = vec.push(heap, v, 20);
    v = vec.push(heap, v, 30);

    // Insert in the middle, then at the end (the same as `push`).
    v = vec.insert(heap, v, 1, 15);
    v = vec.insert(heap, v, 4, 40);
    borrow v as &r in {
        console.print_int(io, vec.get(r, 0));
        putchar(io, 32);
        console.print_int(io, vec.get(r, 1));
        putchar(io, 32);
        console.print_int(io, vec.get(r, 2));
        putchar(io, 32);
        console.print_int(io, vec.get(r, 3));
        putchar(io, 32);
        console.print_int(io, vec.get(r, 4));
        console.newline(io);
    }

    // Remove from the middle: what comes back, and what shifted.
    var removed = 0;
    borrow mut v as &!r in {
        removed = vec.remove(r, 1);
    }
    console.write_all(io, "removed ");
    console.print_int(io, removed);
    console.newline(io);
    borrow v as &r in {
        console.print_int(io, vec.get(r, 0));
        putchar(io, 32);
        console.print_int(io, vec.get(r, 1));
        putchar(io, 32);
        console.print_int(io, vec.get(r, 2));
        putchar(io, 32);
        console.print_int(io, vec.get(r, 3));
        console.newline(io);
    }

    // Pop everything, in reverse order.
    var a = 0;
    var b = 0;
    var c = 0;
    var d = 0;
    borrow mut v as &!r in {
        a = vec.pop(r);
        b = vec.pop(r);
        c = vec.pop(r);
        d = vec.pop(r);
    }
    console.write_all(io, "popped ");
    console.print_int(io, a);
    putchar(io, 32);
    console.print_int(io, b);
    putchar(io, 32);
    console.print_int(io, c);
    putchar(io, 32);
    console.print_int(io, d);
    console.newline(io);

    var count = 0;
    borrow v as &r in {
        count = vec.size(r);
    }
    console.write_all(io, "size ");
    console.print_int(io, count);
    console.newline(io);

    return vec.drop(heap, v);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(ffi);
    release(fs);

    var status = 0;
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            status = run(h, i);
        }
    }
    release(heap);
    release(io);
    return status;
}
