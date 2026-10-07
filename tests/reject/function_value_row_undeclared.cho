//~ ERROR performs `io_write`, which its row [] does not declare
//~ RULE effect-not-declared

// `docs/function-values.md` §4.2: the row travels with the value's
// type, and calling through it performs exactly that row -- checked
// against the *caller's* row the same way a named call's is. A
// higher-order function that forgets to declare what its own callback
// parameter performs is refused here, not silently trusted because
// the call is indirect.

fn shout[&i, &r](out: &!i Io, s: &r [byte]) -> [io_write] int {
    return write_bytes(out, s);
}

fn call_it[&i, &r](out: &!i Io, s: &r [byte], f: fn(&!i Io, &r [byte]) -> [io_write] int) -> [] int {
    return f(out, s);
}

fn main() -> [] int {
    return 0;
}
