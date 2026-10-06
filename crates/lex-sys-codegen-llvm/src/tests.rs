//! `docs/llvm-backend.md` §5: the doorway, checked end to end against a
//! real `clang` on the host running these tests.

use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Command;

use lex_sys_syntax::parse;

use super::*;

/// A `main` that returns `expr`, where `expr` may use `x` -- a value the
/// checker cannot fold, because it comes back from `probe`, an impure call
/// (`putchar` performs `io_write`).
///
/// A trap-signal test needs this: `docs/compile-time.md` §3 folds an
/// operator whose *both* operands are literals at compile time, and turns
/// one that would overflow into a refused program (`Rule::ConstantTraps`)
/// rather than a running one -- correct for a `static`, wrong for what
/// this backend's own checked-arithmetic codegen is supposed to be tested
/// against. `x` is always `0` at run time (`putchar` echoes back the byte
/// it wrote), so every expression below reads as the constant it would be
/// if it were foldable -- it is only kept unfoldable on purpose.
fn program_returning(expr: &str) -> String {
    format!(
        "fn probe[&i](io: &!i Io) -> [io_write] int {{\n\
             return putchar(io, 0);\n\
         }}\n\
         fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
             release(args); release(heap); release(fs); release(ffi);\n\
             var x = 0;\n\
             borrow mut io as &!i in {{\n\
                 x = probe(i);\n\
             }}\n\
             release(io);\n\
             return {expr};\n\
         }}\n"
    )
}

/// `docs/llvm-backend.md` §3.2's own finding, applied to this backend's
/// own suite rather than left as a gap in someone else's: a runtime trap
/// is checked by its **signal**, `SIGILL` (4), not only by
/// `status.code() == None` -- which is true of every signal alike and
/// would not have caught §3.2's `SIGTRAP` bug either.
fn assert_traps_with_sigill(expr: &str, tag: &str) {
    let source = program_returning(expr);
    let object = compiled(&source, "main");
    let output = run(&object, tag);
    assert_eq!(output.status.code(), None, "`{expr}` should be killed by a signal, not exit");
    assert_eq!(
        output.status.signal(),
        Some(4),
        "`{expr}` should trap with SIGILL, matching Cranelift's own signal for a checked-\
         arithmetic trap (docs/llvm-backend.md §3.2)"
    );
}

fn compiled(source: &str, entry: &str) -> Vec<u8> {
    let ast = parse(source).expect("the fixture parses");
    let program = lex_sys_ir::lower(&ast).expect("the fixture type-checks");
    compile_object(&program, entry).expect("the LLVM backend should lower this fixture")
}

fn run(object: &[u8], tag: &str) -> std::process::Output {
    let dir =
        std::env::temp_dir().join(format!("lex-sys-codegen-llvm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a writable temporary directory");
    let obj = dir.join("out.o");
    let exe = dir.join("out");
    std::fs::write(&obj, object).expect("a writable object file");

    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_owned());
    let link = Command::new(&cc).arg(&obj).arg("-o").arg(&exe).status().expect("the linker runs");
    assert!(link.success(), "linking the LLVM-emitted object failed");

    let output = Command::new(&exe).output().expect("the linked program runs");
    let _ = std::fs::remove_dir_all(&dir);
    output
}

/// The fixture `tests/accept/llvm_smoke.ls` builds and runs identically
/// through `lex-sys build --backend llvm`, checked here directly against
/// the backend's own doorway rather than through the CLI.
#[test]
fn the_first_slice_builds_and_runs_the_smoke_fixture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("accept")
        .join("llvm_smoke.ls");
    let source = std::fs::read_to_string(&path).expect("the fixture exists");
    let object = compiled(&source, "main");
    let output = run(&object, "smoke");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Hi!\n",
        "the LLVM backend printed the wrong thing"
    );
    assert_eq!(output.status.code(), Some(0), "the LLVM backend exited wrongly");
}

/// `docs/internal-errors.md`'s promise applies to this backend too, even
/// though its gaps are "not implemented yet" rather than "the checker
/// should have refused this": a program outside what this backend lowers
/// is refused with a located `CodegenError`, never a panic. `region`/
/// `alloc_slice` moved out of this list once §7.5 landed; bare
/// `alloc[a]`/`box`/`unbox` moved out once §7.15 landed; `Type::Float`
/// moved out once §7.17 landed; matching through a reference moved out
/// once §7.19 landed; a foreign call moved out once §7.23 landed; `Fs`
/// moved out once §7.24 landed; `Expr::Static` and `Expr::BitNot` moved
/// out once §7.25 landed -- and with them, every `Expr` variant.
/// `body/expr.rs`'s own `expr` match has no wildcard arm any more, so
/// the compiler itself now enforces that this test's title stays true
/// for `Expr`: a variant this backend does not lower is a build failure
/// here, not a runtime refusal to go looking for.
///
/// The one item this list used to keep -- a program's own `extern fn
/// socket` colliding with this backend's internal declaration for the
/// same libc symbol (§7.23, `docs/ROADMAP.md` #92) -- moved out too, once
/// `--backend llvm` becoming the default made the "already-accepted"
/// framing false: `socket`/`bind`/`listen`/`accept`/`connect`/
/// `getaddrinfo`/`freeaddrinfo`/`close`/`creat`/`open` are now declared
/// only when the program's own `extern fn` does not already claim the
/// symbol, the same guard `read`/`write` already had. So there is no
/// longer a real fixture that reaches an unbuilt `Expr`/`Builtin` or an
/// avoidable collision for this test to name -- checked directly, by
/// building every fixture in `tests/accept/` and `examples/` against
/// `--backend llvm`. This test now proves the fix rather than pinning
/// the collision: the program that used to be refused here builds and
/// links clean.
#[test]
fn a_colliding_extern_fn_no_longer_collides() {
    let source = "\
extern fn socket[&f](ffi: &f Ffi(\"libc\"), domain: int, kind: int, protocol: int) \
    -> [ffi(\"libc\")] int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(io); release(ffi);
    return 0;
}
";
    let ast = parse(source).expect("parses");
    let program = lex_sys_ir::lower(&ast).expect("type-checks");
    compile_object(&program, "main").expect(
        "a program's own `extern fn socket` should build clean now that this backend defers \
         to it instead of insisting on its own declaration",
    );
}

/// §7.25: `Expr::Static`, closed -- checked directly rather than only
/// through the CLI-level `static_data.ls` fixture (`backends.rs`), the
/// same way `extern_fn_labs_computes_the_real_answer` checks `Ffi`
/// directly rather than relying only on `bytes_to_c.ls`. A loop-built
/// table, matching `docs/compile-time-data.md` §2's own `decode_table`
/// shape, read back and summed in `main` with no helper function in
/// between -- if the module-level global `emit_module` writes and the
/// per-occurrence reference `Expr::Static` emits ever disagreed on the
/// symbol name or the byte layout, this would link wrong or read wrong
/// silently rather than refuse, which is exactly what a numeric check
/// and not just "it built" is for.
#[test]
fn a_static_table_is_built_at_compile_time_and_reads_back_correctly() {
    let source = "\
static squares: [int] {
    let table = alloc_slice[static](4, 0);
    var i = 0;
    while i < 4 {
        table[i] = i * i;
        i = i + 1;
    }
    return table;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(ffi); release(io);
    return squares[0] + squares[1] + squares[2] + squares[3] - 14;
}
";
    let object = compiled(source, "main");
    let output = run(&object, "static-table");
    assert_eq!(output.status.code(), Some(0), "0 + 1 + 4 + 9 - 14 should be 0");
}

/// §7.25: `Expr::BitNot`, closed -- found while surveying this backend's
/// `Expr` match for the exhaustiveness `a_program_outside_this_backend_
/// is_refused_not_panicked`'s doc comment now claims, the same way `Fs`
/// was found while building `extern fn` (§7.24). `tests/accept/
/// bitwise.ls` already exercises `~0`, but every operand there is a
/// literal the checker folds away before codegen ever sees a `BitNot`
/// node -- this backend's own `expr` match had no arm for it at all,
/// silently unreachable rather than silently wrong, and nothing caught
/// it because nothing in this document's suite applied `~` to a value
/// the checker could not fold. `x` here is `putchar`'s own echo, real at
/// run time and not a literal. `~5` is `-6`, checked against `Not`'s own
/// "flip the low bit" shape (which would give `4`, not `-6`) rather than
/// only against "it did not crash".
#[test]
fn bitnot_flips_every_bit_not_just_the_low_one() {
    let source = "\
fn probe[&i](io: &!i Io) -> [io_write] int {
    return putchar(io, 5);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(ffi);
    var x = 0;
    borrow mut io as &!i in {
        x = probe(i);
    }
    release(io);
    return (~x) - (0 - 6);
}
";
    let object = compiled(source, "main");
    let output = run(&object, "bitnot");
    assert_eq!(output.status.code(), Some(0), "~5 - (-6) should be 0, not 4 - (-6) = 10");
}

/// §7.23: a foreign call, closed -- the gap the test above used to name.
/// `labs(-5)` through `extern fn`/`Ffi("libc")`, checked against the real
/// answer rather than only against "it built": this is the smallest
/// `int`-in, `int`-out crossing, and `tests/accept/bytes_to_c.ls`
/// (checked in `backends.rs`) is the `&r [byte]`-crossing counterpart.
#[test]
fn extern_fn_labs_computes_the_real_answer() {
    let source = "\
extern fn labs[&f](ffi: &f Ffi(\"libc\"), n: int) -> [ffi(\"libc\")] int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(fs);
    release(io);
    release(heap);
    var result = 0;
    let libc = narrow(ffi, \"libc\");
    borrow libc as &f in {
        result = labs(f, 0 - 5);
    }
    release(libc);
    return result;
}
";
    let object = compiled(source, "main");
    let output = run(&object, "extern-fn-labs");
    assert_eq!(output.status.code(), Some(5), "`labs(-5)` should be `5`");
}

/// §7.17: `Type::Float` closed. `tests/accept/floating_point.ls` itself
/// imports `std.io` for its printing, which `compiled`'s bare
/// `lex_sys_ir::lower` cannot resolve (no `--std` source injection at
/// this level, unlike every other crate-level fixture here) -- so this
/// is a self-contained program instead, covering the same ground:
/// arithmetic, `truncate`, `sqrt`, `bits_of`, `is_nan`, threaded
/// through `x`, an unfoldable runtime value the same way
/// `program_returning` keeps one, so the checker cannot fold this into
/// a `static` and skip codegen entirely.
#[test]
fn floating_point_arithmetic_conversions_and_sqrt_build_and_run() {
    let source = "\
fn probe[&i](io: &!i Io) -> [io_write] int {
    return putchar(io, 0);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(ffi);
    var x = 0;
    borrow mut io as &!i in {
        x = probe(i);
    }
    release(io);

    let n = float_of(x) + 4.0;
    let sum = n + 1.5;
    let product = sum * 2.0;
    let back = truncate(product);
    let root = sqrt(product * product);
    let nan = (n - n) / (n - n);

    var status = 0;
    if back == 11 {
        status = status + 1;
    }
    if root == product {
        status = status + 1;
    }
    if bits_of(n) == bits_of(4.0) {
        status = status + 1;
    }
    if is_nan(nan) {
        status = status + 1;
    }
    return status - 4;
}
";
    let object = compiled(source, "main");
    let output = run(&object, "float-arithmetic");
    assert_eq!(
        output.status.code(),
        Some(0),
        "one of float_of/truncate/sqrt/bits_of/is_nan computed the wrong value"
    );
}

/// §7.19: matching through a reference, closed. `tests/accept/
/// match_a_reference.ls` reads the same list three times through a
/// shared reference (twice via `total`, once via `length`, one of
/// them discarding a bound position with `_`) before consuming and
/// freeing it once -- exactly the "read three times, freed once"
/// claim the fixture's own header makes.
#[test]
fn matching_through_a_reference_builds_and_runs_the_match_a_reference_fixture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("accept")
        .join("match_a_reference.ls");
    let source = std::fs::read_to_string(&path).expect("the fixture exists");
    let object = compiled(&source, "main");
    let output = run(&object, "match-a-reference");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "10 3 10\nfreed 10\n",
        "the LLVM backend computed the wrong values"
    );
    assert_eq!(output.status.code(), Some(0), "the LLVM backend exited wrongly");
}

/// `docs/llvm-backend.md` §5's second slice: `tests/accept/llvm_arith.ls`
/// exercises every trapping `BinOp` plus the three bitwise operators, each
/// once, and the exact bytes prove the values are right, not only that
/// `clang` accepted the module.
#[test]
fn checked_arithmetic_builds_and_runs_the_arith_fixture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("accept")
        .join("llvm_arith.ls");
    let source = std::fs::read_to_string(&path).expect("the fixture exists");
    let object = compiled(&source, "main");
    let output = run(&object, "arith");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Hi! OK$iK\n",
        "the LLVM backend computed the wrong values"
    );
    assert_eq!(output.status.code(), Some(0), "the LLVM backend exited wrongly");
}

/// `docs/llvm-backend.md` §5's third slice: `tests/accept/llvm_control.ls`
/// exercises `if`/`else`, `while`, all six comparisons and both
/// short-circuit operators. The exact bytes prove the loop ran the right
/// number of times and that `&&`/`||` skipped `shout` exactly when they
/// should have, not only that the module compiled.
#[test]
fn control_flow_builds_and_runs_the_control_fixture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("accept")
        .join("llvm_control.ls");
    let source = std::fs::read_to_string(&path).expect("the fixture exists");
    let object = compiled(&source, "main");
    let output = run(&object, "control");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "01X34Z\n+-42\nF\n!T\nT\n!T\n",
        "the LLVM backend computed the wrong values"
    );
    assert_eq!(output.status.code(), Some(0), "the LLVM backend exited wrongly");
}

/// `Add`/`Sub`/`Mul` overflow: LLVM's own `with.overflow` intrinsics,
/// checked against the exact boundary Cranelift's `sadd_overflow` traps
/// on.
#[test]
fn checked_add_traps_on_overflow() {
    assert_traps_with_sigill("x + 9223372036854775807 + 1", "add-overflow");
}

#[test]
fn checked_sub_traps_on_overflow() {
    assert_traps_with_sigill("(-9223372036854775808 + x) - 1", "sub-overflow");
}

#[test]
fn checked_mul_traps_on_overflow() {
    assert_traps_with_sigill("(4611686018427387904 + x) * 2", "mul-overflow");
}

/// `Div`/`Rem`: LLVM's `sdiv`/`srem` are undefined, not trapping, on
/// these two inputs (`docs/llvm-backend.md` §5's own finding) -- these
/// four tests are what proves the manual checks ahead of the instruction
/// actually run, on a real `clang`, rather than merely compile.
#[test]
fn checked_div_traps_on_division_by_zero() {
    assert_traps_with_sigill("10 / x", "div-zero");
}

#[test]
fn checked_div_traps_on_int_min_over_negative_one() {
    assert_traps_with_sigill("(-9223372036854775808 + x) / (0 - 1)", "div-intmin");
}

#[test]
fn checked_rem_traps_on_division_by_zero() {
    assert_traps_with_sigill("10 % x", "rem-zero");
}

/// `Shl`/`Shr`: an amount outside `0..64` traps rather than being masked.
#[test]
fn checked_shl_traps_on_an_out_of_range_amount() {
    assert_traps_with_sigill("1 << (64 + x)", "shl-range");
}

#[test]
fn checked_shr_traps_on_a_negative_amount() {
    assert_traps_with_sigill("1 >> (x - 1)", "shr-range");
}

/// `docs/llvm-backend.md` §5's fourth slice: `examples/hello.ls` -- the
/// program §5 originally (and wrongly) named as the first slice's own
/// target -- builds and runs, closing the loop `ci.yml`'s smoke test
/// opened.
#[test]
fn hello_ls_builds_and_runs_end_to_end() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
        .join("hello.ls");
    let source = std::fs::read_to_string(&path).expect("the example exists");
    let object = compiled(&source, "main");
    let output = run(&object, "hello");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Hello, world!\n",
        "the LLVM backend printed the wrong thing"
    );
    assert_eq!(output.status.code(), Some(0), "the LLVM backend exited wrongly");
}

/// `docs/llvm-backend.md` §5's fifth slice: `tests/accept/enums.ls`
/// exercises a struct literal, an enum with payloads (including a
/// struct-typed payload), and `match` -- a chain of tag tests, a
/// wildcard arm, and a matched binding read back through plain
/// `Expr::Field`, since a by-value match binds an owned struct, not a
/// reference to one.
#[test]
fn structs_and_enums_build_and_run_the_enums_fixture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("accept")
        .join("enums.ls");
    let source = std::fs::read_to_string(&path).expect("the fixture exists");
    let object = compiled(&source, "main");
    let output = run(&object, "enums");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "001220069901\n",
        "the LLVM backend computed the wrong values"
    );
    assert_eq!(output.status.code(), Some(0), "the LLVM backend exited wrongly");
}

/// `docs/llvm-backend.md` §7.11: `Place::Field`/`Place::Deref`, plus the
/// read-side siblings (`Expr::FieldRef`/`Expr::FieldAddr`/`Expr::Deref`)
/// -- `tests/accept/deref_roundtrip.ls` exercises a shared and a unique
/// reference to a bare `int` (`*n`, `*n = e`), a field read *through* a
/// reference (`scale`'s own `p.x`/`p.y`), and the whole referent
/// replaced (`*p = Point { .. }`), all in one fixture.
#[test]
fn field_and_deref_writes_build_and_run_the_deref_roundtrip_fixture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("accept")
        .join("deref_roundtrip.ls");
    let source = std::fs::read_to_string(&path).expect("the fixture exists");
    let object = compiled(&source, "main");
    let output = run(&object, "deref-roundtrip");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "41 42\n3 4 -> 30 40\n",
        "the LLVM backend computed the wrong values"
    );
    assert_eq!(output.status.code(), Some(0), "the LLVM backend exited wrongly");
}

/// `tests/accept/arguments.ls`, run with no arguments (`run` passes none):
/// `argc` is still `1`, its own name, exactly as C hands it over
/// (`docs/arguments.md` §3). The conformance suite's differential test
/// passes real arguments; this fixture's own contract only covers the
/// no-argument case.
#[test]
fn arg_count_and_arg_build_and_run_the_arguments_fixture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("accept")
        .join("arguments.ls");
    let source = std::fs::read_to_string(&path).expect("the fixture exists");
    let object = compiled(&source, "main");
    let output = run(&object, "arguments");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "1\nnamed: 1\n",
        "the LLVM backend computed the wrong argc/argv"
    );
    assert_eq!(output.status.code(), Some(0), "the LLVM backend exited wrongly");
}

/// `tests/accept/arena_roundtrip.ls`: `alloc[a](value)`, single-value
/// bump allocation -- the same `bump` helper `alloc_slice` already
/// opened (§7.5), minus its fill loop.
#[test]
fn alloc_builds_and_runs_the_arena_roundtrip_fixture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("accept")
        .join("arena_roundtrip.ls");
    let source = std::fs::read_to_string(&path).expect("the fixture exists");
    let object = compiled(&source, "main");
    let output = run(&object, "arena-roundtrip");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "0 1 4 9 16 25 36 49 = 140\nnested: 7\n",
        "the LLVM backend computed the wrong values"
    );
    assert_eq!(output.status.code(), Some(0), "the LLVM backend exited wrongly");
}

/// `tests/accept/box_roundtrip.ls`: `box(h, value)`/`unbox(h, b)`, the
/// heap-shaped twin of `alloc` above -- one `malloc`, trapping on
/// exhaustion exactly as `boxed_slice` already does, and one `free` on
/// the way out, the load happening first.
#[test]
fn box_and_unbox_build_and_run_the_box_roundtrip_fixture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("accept")
        .join("box_roundtrip.ls");
    let source = std::fs::read_to_string(&path).expect("the fixture exists");
    let object = compiled(&source, "main");
    let output = run(&object, "box-roundtrip");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "7\n10 4\n",
        "the LLVM backend computed the wrong values"
    );
    assert_eq!(output.status.code(), Some(0), "the LLVM backend exited wrongly");
}

/// `s[i]` is bounds-checked (`docs/defined-behaviour.md` §1); `uge`
/// catches both ends with one comparison, so a negative index and one
/// past the end are the same check on real hardware, not only on paper.
fn assert_indexing_traps(index: &str, tag: &str) {
    let source = format!(
        "fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
             release(args); release(heap); release(fs); release(ffi); release(io);\n\
             let s = \"abc\";\n\
             return int_of(s[{index}]);\n\
         }}\n"
    );
    let object = compiled(&source, "main");
    let output = run(&object, tag);
    assert_eq!(output.status.code(), None, "`s[{index}]` should be killed by a signal, not exit");
    assert_eq!(
        output.status.signal(),
        Some(4),
        "`s[{index}]` should trap with SIGILL, matching Cranelift's own signal for a bounds \
         check (docs/llvm-backend.md §3.2)"
    );
}

#[test]
fn indexing_past_a_slice_traps_with_sigill() {
    assert_indexing_traps("5", "index-past");
}

#[test]
fn indexing_before_a_slice_traps_with_sigill() {
    assert_indexing_traps("0 - 1", "index-before");
}

/// `docs/llvm-backend.md` §7.5: `region`/`alloc_slice` -- exhausting the
/// arena's chunk traps rather than handing back a slice past the end,
/// matching `lex-sys-codegen`'s own `bump`. 9000 `int`s is 72000 bytes,
/// past `ARENA_CHUNK`'s 65536.
#[test]
fn allocating_past_an_arenas_chunk_traps_with_sigill() {
    let source = "\
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(fs); release(ffi); release(io); release(heap);
    region a {
        let s = alloc_slice[a](9000, 0);
        return s[0];
    }
}
";
    let object = compiled(source, "main");
    let output = run(&object, "arena-exhausted");
    assert_eq!(output.status.code(), None, "an exhausted arena should be killed by a signal");
    assert_eq!(
        output.status.signal(),
        Some(4),
        "an exhausted arena should trap with SIGILL, matching Cranelift's own signal \
         (docs/llvm-backend.md §3.2)"
    );
}

/// `docs/slicing.md`, `docs/llvm-backend.md` §7.9: `s[a..b]` traps
/// rather than yielding a silently wrong answer, on either of its two
/// bad shapes -- past the slice's own length, or inverted (`start >
/// end`) -- matching `lex-sys-codegen`'s own `subslice`.
fn assert_subslicing_traps(range: &str, tag: &str) {
    let source = format!(
        "fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
             release(args); release(heap); release(fs); release(ffi); release(io);\n\
             let s = \"abc\";\n\
             let t = s[{range}];\n\
             return len(t);\n\
         }}\n"
    );
    let object = compiled(&source, "main");
    let output = run(&object, tag);
    assert_eq!(output.status.code(), None, "`s[{range}]` should be killed by a signal, not exit");
    assert_eq!(
        output.status.signal(),
        Some(4),
        "`s[{range}]` should trap with SIGILL, matching Cranelift's own signal for a bad \
         subslice (docs/llvm-backend.md §3.2)"
    );
}

#[test]
fn subslicing_past_the_end_traps_with_sigill() {
    assert_subslicing_traps("0..5", "subslice-past");
}

#[test]
fn an_inverted_subslice_traps_with_sigill() {
    assert_subslicing_traps("2..0", "subslice-inverted");
}

/// `docs/llvm-backend.md` §7.20: `listen`/`accept`, closed. Neither
/// builtin takes a capability -- the fd's authority was already proved
/// at `bind`, which this backend still refuses (`Ffi`/`Net`'s other
/// builtins are still outside this slice, so there is no way to get a
/// *real* bound fd out of a `--backend llvm` program yet) -- so both
/// are ordinary fixed-signature `libc` calls, checked here the same way
/// `lex-sys-codegen`'s own arm is: a deliberately invalid fd (`999`,
/// never opened) makes both calls fail the same way on any host,
/// `EBADF`, without needing a real socket or a live connection.
#[test]
fn listen_and_accept_on_a_bad_fd_both_fail_matching_cranelift() {
    let source = "\
edition 2;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(args); release(heap); release(fs); release(ffi); release(io); release(net);

    let l = listen(999, 16);
    let a = accept(999);
    if l < 0 {
        if a < 0 {
            return 0;
        }
    }
    return 1;
}
";
    let object = compiled(source, "main");
    let output = run(&object, "listen-accept-bad-fd");
    assert_eq!(
        output.status.code(),
        Some(0),
        "listen/accept on an invalid fd should both report failure, matching Cranelift"
    );
}

/// `docs/llvm-backend.md` §7.21: `bind`, closed -- the second of `Net`'s
/// four builtins, after `listen`/`accept` (§7.20). §6.1's own rule:
/// `bind`'s port is checked against the capability's bound *before* any
/// syscall runs, the same as `lex-sys-codegen`'s own `bind` and confirmed
/// here the same way `assert_traps_with_sigill`/`assert_subslicing_traps`
/// already check a trap -- by signal, not only by `status.code() ==
/// None`, since every signal alike would satisfy that.
#[test]
fn binding_the_wrong_port_traps_with_sigill() {
    let source = "\
edition 2;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(args); release(heap); release(fs); release(ffi); release(io);

    var fd = 0;
    let bound = narrow(net, \"1\");
    borrow bound as &n in {
        fd = bind(n, 2);
    }
    release(bound);
    return fd;
}
";
    let object = compiled(source, "main");
    let output = run(&object, "bind-wrong-port");
    assert_eq!(output.status.code(), None, "a port outside the bound should not exit normally");
    assert_eq!(
        output.status.signal(),
        Some(4),
        "binding a port outside the capability's bound should trap with SIGILL, matching \
         Cranelift's own signal (docs/llvm-backend.md §3.2)"
    );
}

/// `docs/llvm-backend.md` §7.22: `connect`, closed -- the last of `Net`'s
/// four builtins. `docs/connect.md` §10.1: the *host* half of the bound
/// is checked before `getaddrinfo` ever runs, the outbound mirror of
/// `bind`'s own port check above.
#[test]
fn connecting_outside_the_granted_host_traps_with_sigill() {
    let source = "\
edition 2;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(args); release(heap); release(ffi); release(fs); release(io);

    var fd = 0;
    let bound = narrow(net, \"127.0.0.1:1\");
    borrow bound as &n in {
        fd = connect(n, \"10.0.0.1\", 1);
    }
    release(bound);
    return fd;
}
";
    let object = compiled(source, "main");
    let output = run(&object, "connect-outside-host");
    assert_eq!(output.status.code(), None, "a host outside the bound should not exit normally");
    assert_eq!(
        output.status.signal(),
        Some(4),
        "connecting to a host outside the capability's bound should trap with SIGILL, matching \
         Cranelift's own signal (docs/llvm-backend.md §3.2)"
    );
}

/// §10.1's other half: the bound's port is checked exactly, not as a
/// prefix, the outbound mirror of `binding_the_wrong_port_traps_with_
/// sigill` above.
#[test]
fn connecting_to_the_wrong_port_traps_with_sigill() {
    let source = "\
edition 2;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(args); release(heap); release(ffi); release(fs); release(io);

    var fd = 0;
    let bound = narrow(net, \"127.0.0.1:1\");
    borrow bound as &n in {
        fd = connect(n, \"127.0.0.1\", 2);
    }
    release(bound);
    return fd;
}
";
    let object = compiled(source, "main");
    let output = run(&object, "connect-wrong-port");
    assert_eq!(output.status.code(), None, "a port outside the bound should not exit normally");
    assert_eq!(
        output.status.signal(),
        Some(4),
        "connecting to a port outside the capability's bound should trap with SIGILL, matching \
         Cranelift's own signal (docs/llvm-backend.md §3.2)"
    );
}

/// `docs/llvm-backend.md` §7.24: `compare` hardcoded `icmp {cc} i64`
/// regardless of what its operands actually are -- silently correct for
/// `int` (already `i64`), silently **ill-typed** for `byte`/`bool`
/// (`i8`), and nothing had caught it: every comparison in every fixture
/// and `benches/` program this document tracks compares `int`s. `std.
/// bytes.find`'s own `text[at + i] != needle[i]` compares raw bytes, and
/// building `examples/cut/`/`examples/seek/` against this slice -- both
/// of which call it through `std.flags` -- is what found `clang`
/// refusing the emitted module rather than a wrong answer, once `Fs`
/// stopped being the first thing either program hit. `x`, echoed
/// through `putchar`, keeps `byte_of(x)`/`byte_of(x + 1)` from folding
/// to a compile-time constant the checker would take a different path
/// for (`docs/compile-time.md` §3), the same device `program_returning`
/// uses for `int`.
#[test]
fn byte_comparison_uses_the_right_width() {
    let source = "\
fn probe[&i](io: &!i Io) -> [io_write] int {
    return putchar(io, 0);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(ffi);
    var x = 0;
    borrow mut io as &!i in {
        x = probe(i);
    }
    release(io);

    let a = byte_of(x);
    let b = byte_of(x + 1);
    var status = 0;
    if a != b {
        status = status + 1;
    }
    if a == a {
        status = status + 1;
    }
    if b != a {
        status = status + 1;
    }
    return status - 3;
}
";
    let object = compiled(source, "main");
    let output = run(&object, "byte-comparison");
    assert_eq!(
        output.status.code(),
        Some(0),
        "a `byte` comparison should compare at `i8`, not be silently widened to `i64`"
    );
}

/// `docs/signals.md` section 5: a signal claim builds for both binary
/// formats on both architectures that run macOS, through a real `clang`, and
/// each kernel's module calls its own facility. Darwin is built and not run
/// here: it is the only check that path gets on a Linux host.
#[test]
fn a_signal_claim_builds_for_every_target_with_each_kernels_calls() {
    const CLAIM: &str = "edition 6;\n\
         fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args); release(net);\n\
             release(clock);\n\
             let claim = narrow(signals, \"TERM,USR1\");\n\
             var status = 1;\n\
             borrow claim as &s in {\n\
                 match signals_watch(s) {\n\
                     Watching::Ok(w) => {\n\
                         var watch = w;\n\
                         borrow mut watch as &!wh in { signals_pending(wh); }\n\
                         status = signals_close(watch);\n\
                     }\n\
                     Watching::Failed(e) => { status = 2; }\n\
                 }\n\
             }\n\
             release(claim);\n\
             return status;\n\
         }\n";
    let ast = parse(CLAIM).expect("should parse");
    let program = lex_sys_ir::lower(&ast).expect("should lower");
    for triple in [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ] {
        let triple: Triple = triple.parse().expect("a valid triple");
        let text = emit::emit_module(&program, "main", &triple)
            .unwrap_or_else(|(_, message)| panic!("{triple}: {message}"));
        let calls = |name: &str| text.contains(&format!("call i32 @{name}("));
        if triple.to_string().contains("darwin") {
            for call in ["sigaction", "kqueue", "kevent", "raise", "close"] {
                assert!(calls(call), "{triple} should call `{call}`");
            }
            assert!(!calls("signalfd") && !calls("pthread_sigmask"), "{triple}");
        } else {
            for call in ["pthread_sigmask", "signalfd", "close"] {
                assert!(calls(call), "{triple} should call `{call}`");
            }
            assert!(!calls("raise") && !calls("sigaction"), "{triple}");
        }
        compile_object_for(&program, "main", triple.clone()).unwrap_or_else(|e| {
            panic!("`clang` should accept the module for {triple}: {}", e.message)
        });
    }
}

/// `docs/processes.md` §4.5 and §4.8, from any host: starting a program and
/// watching it ask Linux for `closefrom` and a `pidfd`, and Darwin for
/// neither -- `POSIX_SPAWN_CLOEXEC_DEFAULT` and `kevent` instead.
#[test]
fn a_child_is_started_and_watched_the_way_each_platform_does() {
    const WATCH: &str = r#"edition 7;
fn go[&x](exec: &x Exec("/bin")) -> [exec("/bin"), poll] int {
    match poller_new() {
        Polling::Ok(p) => {
            var poller = p;
            match pipe_open() {
                Piped::Ok(mine, theirs) => {
                    var m = mine;
                    match exec_spawn(exec, "/bin/true", "", "", Stdio::Null, Stdio::Pipe(theirs), Stdio::Null) {
                        Spawned::Ok(c) => {
                            var child = c;
                            borrow mut poller as &!ph in {
                                borrow mut m as &!pp in {
                                    borrow child as &ch in {
                                        poller_add_pipe(ph, pp, 1, 1);
                                        poller_add_child(ph, ch, 2);
                                    }
                                }
                            }
                            match child_wait(child) {
                                Exited::Code(n) => { }
                                Exited::Signaled(s) => { }
                                Exited::Failed(e) => { }
                            }
                        }
                        Spawned::Failed(e) => { }
                    }
                    pipe_close(m);
                }
                Piped::Failed(e) => { }
            }
            poller_close(poller);
        }
        Polling::Failed(e) => { }
    }
    return 0;
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    let bin = narrow(exec, "/bin");
    var status = 0;
    borrow bin as &x in { status = go(x); }
    release(bin);
    return status;
}
"#;
    let ast = parse(WATCH).expect("should parse");
    let program = lex_sys_ir::lower(&ast).expect("should lower");
    for triple in [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ] {
        let triple: Triple = triple.parse().expect("a valid triple");
        let text = emit::emit_module(&program, "main", &triple)
            .unwrap_or_else(|(_, message)| panic!("{triple}: {message}"));
        // A `declare` is emitted for every libc function whatever the target
        // uses; what a platform *does* is what it calls.
        let calls = |name: &str| {
            text.lines().any(|l| l.contains("call ") && l.contains(&format!("@{name}(")))
        };
        for call in ["posix_spawn", "waitpid", "socketpair", "close"] {
            assert!(calls(call), "{triple} should call `{call}`");
        }
        if triple.to_string().contains("darwin") {
            for call in ["kqueue", "kevent"] {
                assert!(calls(call), "{triple} should call `{call}`");
            }
            assert!(
                !calls("syscall")
                    && !calls("epoll_ctl")
                    && !calls("posix_spawn_file_actions_addclosefrom_np"),
                "{triple}"
            );
        } else {
            for call in ["syscall", "epoll_ctl", "posix_spawn_file_actions_addclosefrom_np"] {
                assert!(calls(call), "{triple} should call `{call}`");
            }
            assert!(!calls("kevent") && !calls("kqueue"), "{triple}");
        }
        compile_object_for(&program, "main", triple.clone()).unwrap_or_else(|e| {
            panic!("`clang` should accept the module for {triple}: {}", e.message)
        });
    }
}

/// lex-sys#252: a `region` left by `return` gives its chunk back, as one
/// left by falling out of its last statement does. This backend used to
/// emit the `ret` with no `free`, so a function that returned from inside a
/// region kept one 64 KiB chunk per call for good (lexsys-hooks lost about
/// 1 GB in 100,000 deliveries). `lex-sys-codegen`'s `emit_return` frees
/// every open arena; this checks the LLVM backend does the same, in the
/// IR, and that the value is read before its arena is freed.
const RETURN_FROM_REGION: &str = "\
fn copy[&o, &v](out: &!o [byte], at: int, value: &v [byte]) -> [] int {
    var i = 0;
    while i < len(value) {
        out[at + i] = value[i];
        i = i + 1;
    }
    return at + len(value);
}

fn left_by_return[&o](out: &!o [byte], n: int) -> [] int {
    region a {
        let value = alloc_slice[a](40, byte_of(0));
        value[0] = byte_of(n & 127);
        return copy(out, 0, value) + int_of(value[0]);
    }
}

fn nested(n: int) -> [] int {
    region a {
        let x = alloc_slice[a](4, 0);
        x[0] = n;
        region b {
            let y = alloc_slice[b](4, 0);
            y[0] = x[0] * 2;
            if y[0] > 10 {
                return y[0];
            }
        }
        return x[0];
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(fs); release(ffi); release(io); release(heap);
    var total = 0;
    region outer {
        let buf = alloc_slice[outer](64, byte_of(0));
        var i = 0;
        while i < 100000 {
            total = total + left_by_return(buf, i) + nested(i & 15);
            i = i + 1;
        }
    }
    return total & 127;
}
";

/// The body of the function whose symbol contains `name`, from its
/// `define` line to its closing brace.
fn function_text<'t>(module: &'t str, name: &str) -> &'t str {
    let mut at = 0;
    let start = module
        .lines()
        .find_map(|line| {
            let here = at;
            at += line.len() + 1;
            (line.starts_with("define ") && line.contains(&format!("{name}("))).then_some(here)
        })
        .unwrap_or_else(|| panic!("no function `{name}` in the module"));
    let end = module[start..].find("\n}\n").expect("the function ends") + start;
    &module[start..end]
}

#[test]
fn a_region_left_by_return_frees_its_chunk_before_the_ret() {
    let ast = parse(RETURN_FROM_REGION).expect("should parse");
    let program = lex_sys_ir::lower(&ast).expect("should lower");
    let triple: Triple = "x86_64-unknown-linux-gnu".parse().expect("a valid triple");
    let module = emit::emit_module(&program, "main", &triple)
        .unwrap_or_else(|(_, message)| panic!("{message}"));
    let count = |text: &str, what: &str| text.lines().filter(|l| l.contains(what)).count();
    // Every `ret` in a function that returns from inside a region comes
    // straight after a `free` of an arena's chunk.
    for name in ["left_by_return", "nested"] {
        let text = function_text(&module, name);
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        for (at, line) in lines.iter().enumerate() {
            if line.starts_with("ret ") {
                assert!(
                    lines[..at].last().is_some_and(|l| l.starts_with("call void @free(")),
                    "`{name}`: a `ret` with no `free` just before it:\n{text}"
                );
            }
        }
    }
    // `left_by_return`: one chunk, one `return`, one `free`.
    let one = function_text(&module, "left_by_return");
    assert_eq!(count(one, "call ptr @malloc("), 1, "{one}");
    assert_eq!(count(one, "call void @free("), 1, "{one}");
    // `nested`: the inner `return` frees both chunks, the inner region's
    // fall-out frees its own, the outer `return` the outer one: four.
    let two = function_text(&module, "nested");
    assert_eq!(count(two, "call ptr @malloc("), 2, "{two}");
    assert_eq!(count(two, "call void @free("), 4, "{two}");
}

#[test]
fn a_value_returned_from_a_region_is_read_before_the_region_is_freed() {
    let object = compiled(RETURN_FROM_REGION, "main");
    let output = run(&object, "return-from-region");
    // left_by_return(i) = 40 + (i & 127); nested(k) = 2k if 2k > 10 else k.
    let mut total: i64 = 0;
    for i in 0..100_000_i64 {
        let k = i & 15;
        total += 40 + (i & 127) + if 2 * k > 10 { 2 * k } else { k };
    }
    assert_eq!(output.status.code(), Some((total & 127) as i32), "{output:?}");
}

/// `docs/wasm.md`: what wasi-libc needs from the module the backend writes,
/// checked on the text so it runs without a wasm toolchain.
///
/// Three things went wrong on the first `wasm32-wasip1` run, each silently:
/// the entry was called `main` where wasi-libc calls `__main_argc_argv`; `malloc`
/// was declared with an `i64` size where `size_t` is 4 bytes, which `wasm-ld`
/// only warns about before swapping in a trap; and the trap was `ud2`.
#[test]
fn a_wasm32_module_has_the_shape_wasi_libc_needs() {
    const SOURCE: &str = "fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args } = split(world);\n\
             release(args); release(heap); release(fs); release(ffi); release(io);\n\
             var n = 0;\n\
             region a {\n\
                 let s = alloc_slice[a](8, byte_of(1));\n\
                 n = n + len(s);\n\
             }\n\
             return n;\n\
         }\n";
    let ast = parse(SOURCE).expect("should parse");
    let program = lex_sys_ir::lower(&ast).expect("should lower");

    let wasm: Triple = "wasm32-wasip1".parse().expect("a valid triple");
    let text = emit::emit_module(&program, "main", &wasm).expect("wasm32 should emit");
    assert!(text.contains("define i32 @__main_argc_argv("), "the entry symbol wasi-libc calls");
    assert!(!text.contains("define i32 @main("), "no plain `main` on wasm");
    assert!(text.contains("declare ptr @malloc(i32)"), "`malloc` with a 32-bit `size_t`");
    assert!(!text.contains("declare ptr @malloc(i64)"), "no 64-bit `malloc`");
    assert!(text.contains("call ptr @malloc(i32 "), "calls pass a 32-bit size");
    assert!(
        text.contains("icmp ugt i64") && text.contains(", 4294967295"),
        "a huge size is clamped to the target's maximum, not truncated"
    );

    // The host's own module is untouched by any of this.
    let host: Triple = "x86_64-unknown-linux-gnu".parse().expect("a valid triple");
    let native = emit::emit_module(&program, "main", &host).expect("native should emit");
    assert!(native.contains("define i32 @main("), "{native}");
    assert!(native.contains("declare ptr @malloc(i64)"));
    assert!(!native.contains("4294967295"), "no clamp on a native module");
}

#[test]
fn wasm32_traps_with_unreachable() {
    let ast = parse(&program_returning("x + 9223372036854775807")).expect("should parse");
    let program = lex_sys_ir::lower(&ast).expect("should lower");
    let wasm: Triple = "wasm32-wasip1".parse().expect("a valid triple");
    let text = emit::emit_module(&program, "main", &wasm).expect("wasm32 should emit");
    assert!(text.contains("asm sideeffect \"unreachable\""), "{text}");
    assert!(!text.contains("ud2"), "no x86 trap in a wasm module");
}

/// The constant tables know Linux, Darwin and WASI. A fourth operating system
/// used to take the Linux numbers without a word; now it is refused where the
/// module is emitted.
#[test]
fn an_operating_system_without_tables_is_refused_not_given_linuxs() {
    let ast = parse(&program_returning("x")).expect("should parse");
    let program = lex_sys_ir::lower(&ast).expect("should lower");
    let freebsd: Triple = "x86_64-unknown-freebsd".parse().expect("a valid triple");
    let (_, message) = emit::emit_module(&program, "main", &freebsd).expect_err("no tables");
    assert!(message.contains("freebsd"), "{message}");
    for known in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "wasm32-wasip1"] {
        let triple: Triple = known.parse().expect("a valid triple");
        emit::emit_module(&program, "main", &triple)
            .unwrap_or_else(|(_, m)| panic!("{known}: {m}"));
    }
}

/// `docs/wasm.md`: a failure's `errno` is translated on WASI and only there.
#[test]
fn errno_is_translated_on_wasi_and_only_there() {
    const SOURCE: &str = "edition 6;\n\
         fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);\n\
             release(io); release(ffi); release(heap); release(args);\n\
             release(net); release(clock); release(signals);\n\
             let root = narrow(fs, \"/\");\n\
             var status = 0;\n\
             borrow root as &f in {\n\
                 match open_dir(f, \"/nonexistent\") {\n\
                     DirOpened::Ok(d) => { dir_close(d); }\n\
                     DirOpened::Failed(e) => { status = e; }\n\
                 }\n\
             }\n\
             release(root);\n\
             return status;\n\
         }\n";
    let ast = parse(SOURCE).expect("should parse");
    let program = lex_sys_ir::lower(&ast).expect("should lower");
    let wasm: Triple = "wasm32-wasip1".parse().expect("a valid triple");
    let text = emit::emit_module(&program, "main", &wasm).expect("wasm32 should emit");
    assert!(text.contains("define internal i32 @lexsys_wasi_errno("), "{text}");
    assert!(text.contains("i32 44, label %to2"), "ENOENT is 44 on WASI, 2 in the language");
    assert!(text.contains("call i32 @lexsys_wasi_errno("), "errno is read through it");
    let host: Triple = "x86_64-unknown-linux-gnu".parse().expect("a valid triple");
    let native = emit::emit_module(&program, "main", &host).expect("native should emit");
    assert!(!native.contains("lexsys_wasi_errno"), "no translation off WASI");
}

/// A module that runs [`emit::smul_overflow_definition`] against LLVM's own
/// `smul.with.overflow` on the host and answers whether they ever disagreed:
/// the 400 pairs of twenty edge values (both extremes, the square root of
/// `i64::MAX` either side, powers of two, 2^32 either side) and then random
/// pairs whose magnitudes are themselves random, so every bit-width of
/// operand is reached. They must agree on the overflow flag always, and on the
/// value whenever there was no overflow (what the intrinsic leaves unspecified
/// otherwise).
fn smul_differential(definition: &str, tag: &str) -> std::process::Output {
    let host = lex_sys_codegen::host_triple();
    let module = format!(
        r#"target triple = "{host}"

{definition}
declare {{i64, i1}} @llvm.smul.with.overflow.i64(i64, i64)

@edges = internal constant [20 x i64] [i64 0, i64 1, i64 -1, i64 2, i64 -2, i64 3,
  i64 9223372036854775807, i64 -9223372036854775808, i64 9223372036854775806,
  i64 -9223372036854775807, i64 2147483648, i64 4294967296, i64 -4294967296,
  i64 4611686018427387904, i64 -4611686018427387904, i64 4294967295,
  i64 3037000499, i64 3037000500, i64 -3037000500, i64 -2147483648]

define i32 @main(i32 %argc, ptr %argv) {{
entry:
  br label %loop
loop:
  %i = phi i64 [0, %entry], [%i1, %loop]
  %sa = phi i64 [88172645463325252, %entry], [%sa2, %loop]
  %sb = phi i64 [1181783497276652981, %entry], [%sb2, %loop]
  %bad = phi i64 [0, %entry], [%bad1, %loop]
  %a1 = shl i64 %sa, 13
  %a2 = xor i64 %sa, %a1
  %a3 = lshr i64 %a2, 7
  %a4 = xor i64 %a2, %a3
  %a5 = shl i64 %a4, 17
  %sa2 = xor i64 %a4, %a5
  %b1 = shl i64 %sb, 13
  %b2 = xor i64 %sb, %b1
  %b3 = lshr i64 %b2, 7
  %b4 = xor i64 %b2, %b3
  %b5 = shl i64 %b4, 17
  %sb2 = xor i64 %b4, %b5
  %sha = lshr i64 %sb2, 58
  %shb = lshr i64 %sa2, 58
  %ra = ashr i64 %sa2, %sha
  %rb = ashr i64 %sb2, %shb
  %ia0 = udiv i64 %i, 20
  %ia = urem i64 %ia0, 20
  %ib = urem i64 %i, 20
  %pa = getelementptr i64, ptr @edges, i64 %ia
  %pb = getelementptr i64, ptr @edges, i64 %ib
  %ea = load i64, ptr %pa
  %eb = load i64, ptr %pb
  %edge = icmp ult i64 %i, 400
  %a = select i1 %edge, i64 %ea, i64 %ra
  %b = select i1 %edge, i64 %eb, i64 %rb
  %want = call {{i64, i1}} @llvm.smul.with.overflow.i64(i64 %a, i64 %b)
  %got = call {{i64, i1}} @lexsys_smul_overflow(i64 %a, i64 %b)
  %wo = extractvalue {{i64, i1}} %want, 1
  %go = extractvalue {{i64, i1}} %got, 1
  %wv = extractvalue {{i64, i1}} %want, 0
  %gv = extractvalue {{i64, i1}} %got, 0
  %flag_diff = xor i1 %wo, %go
  %val_diff0 = icmp ne i64 %wv, %gv
  %nov = xor i1 %wo, true
  %val_diff = and i1 %val_diff0, %nov
  %diff = or i1 %flag_diff, %val_diff
  %d64 = zext i1 %diff to i64
  %bad1 = add i64 %bad, %d64
  %i1 = add i64 %i, 1
  %more = icmp ult i64 %i1, 2000000
  br i1 %more, label %loop, label %done
done:
  %any = icmp ne i64 %bad1, 0
  %r = zext i1 %any to i32
  ret i32 %r
}}
"#
    );
    let object = run_clang(&module, &host).expect("clang should accept the differential module");
    run(&object, tag)
}

/// W0.3: the multiply `wasm32` uses instead of `__multi3` is LLVM's own, bit
/// for bit, where it is defined.
#[test]
fn the_inline_multiply_agrees_with_llvms_intrinsic() {
    let out = smul_differential(&emit::smul_overflow_definition(), "smul-agrees");
    assert_eq!(out.status.code(), Some(0), "the inline multiply disagreed with LLVM's: {out:?}");
}

/// The test above would pass if it could not fail, so break the algorithm and
/// require that it does: the signed limit for a negative product is 2^63, not
/// 2^63-1, and `INT_MIN * 1` is the pair that says so.
#[test]
fn the_differential_test_can_fail() {
    let broken = emit::smul_overflow_definition().replace(
        "select i1 %neg, i64 9223372036854775808, i64 9223372036854775807",
        "select i1 %neg, i64 9223372036854775807, i64 9223372036854775807",
    );
    assert_ne!(broken, emit::smul_overflow_definition(), "the replacement did not apply");
    let out = smul_differential(&broken, "smul-broken");
    assert_eq!(out.status.code(), Some(1), "a wrong multiply went unnoticed: {out:?}");
}

/// A `wasm32` module uses it, and a native one never does.
#[test]
fn only_wasm32_swaps_the_multiply() {
    let ast = parse(&program_returning("(4611686018427387904 + x) * 2")).expect("should parse");
    let program = lex_sys_ir::lower(&ast).expect("should lower");
    let wasm: Triple = "wasm32-wasip1".parse().expect("a valid triple");
    let text = emit::emit_module(&program, "main", &wasm).expect("wasm32 should emit");
    assert!(text.contains("call {i64, i1} @lexsys_smul_overflow("), "{text}");
    assert!(text.contains("define internal {i64, i1} @lexsys_smul_overflow("));
    assert!(!text.contains("call {i64, i1} @llvm.smul.with.overflow"), "no `__multi3` path left");
    // Add and subtract are single instructions on wasm and keep the intrinsic.
    let host: Triple = "x86_64-unknown-linux-gnu".parse().expect("a valid triple");
    let native = emit::emit_module(&program, "main", &host).expect("native should emit");
    assert!(native.contains("call {i64, i1} @llvm.smul.with.overflow"), "{native}");
    assert!(!native.contains("lexsys_smul_overflow"), "no shim off wasm32");
}

/// `docs/wasm.md`, W0.4: on wasm32 every libc function whose C signature has a
/// `size_t` is declared with a 32-bit one *and called with one*, at every call
/// site the backend has, not through a wrapper added afterwards.
///
/// `wasm-ld` only warns about a call whose type disagrees with its definition
/// and swaps in a trap, so a missed site links and dies at run time (the CLI links
/// with `--fatal-warnings`, which makes it a build failure instead). This is the
/// same check one step earlier and with no toolchain: for each of these
/// fixtures, no `@malloc`, `@memmove`, `@read`, ... line mentions an `i64` -- save
/// `pread` and `pwrite`, whose offset is a 64-bit `off_t` on WASI and is the only
/// one -- and the same fixtures on the host still pass 64-bit sizes.
#[test]
fn every_sized_libc_call_on_wasm32_passes_a_32_bit_size() {
    const SIZED: [&str; 11] = [
        "malloc", "calloc", "memchr", "memmove", "strlen", "strncmp", "fwrite", "read", "write",
        "pread", "pwrite",
    ];
    let wasm: Triple = "wasm32-wasip1".parse().expect("a valid triple");
    let host: Triple = "x86_64-unknown-linux-gnu".parse().expect("a valid triple");
    let mut seen = std::collections::BTreeSet::new();
    // `write_bytes` is `fwrite`; the fixtures below reach the rest. None imports
    // `std`, which a bare `lower` has no copy of.
    const BULK_WRITE: &str = "edition 5;\n\
         fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n\
             release(ffi); release(fs); release(heap); release(args); release(net);\n\
             release(clock);\n\
             borrow mut io as &!i in { write_bytes(i, \"hi\"); }\n\
             release(io);\n\
             return 0;\n\
         }\n";
    for (name, source) in [
        ("copy_within", include_str!("../../../tests/accept/copy_within.ls")),
        ("arguments", include_str!("../../../tests/accept/arguments.ls")),
        ("file_roundtrip", include_str!("../../../tests/accept/file_roundtrip.ls")),
        ("write_bytes", BULK_WRITE),
    ] {
        let ast = parse(source).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let program = lex_sys_ir::lower(&ast).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let text = emit::emit_module(&program, "main", &wasm).expect("wasm32 should emit");
        for line in text.lines() {
            for func in SIZED {
                if !line.contains(&format!("@{func}(")) {
                    continue;
                }
                let allowed = if func == "pread" || func == "pwrite" { 1 } else { 0 };
                assert_eq!(
                    line.matches("i64").count(),
                    allowed,
                    "{name}: `{func}` on wasm32 must pass a 32-bit `size_t`: {line}"
                );
                seen.insert(func);
            }
        }
        // The host is not touched: its sizes are still 64-bit.
        let native = emit::emit_module(&program, "main", &host).expect("native should emit");
        assert!(native.contains("declare ptr @malloc(i64)"), "{name}");
    }
    for func in ["malloc", "memmove", "fwrite", "strlen", "read", "write"] {
        assert!(seen.contains(func), "the fixtures never reached `{func}`: {seen:?}");
    }
}
