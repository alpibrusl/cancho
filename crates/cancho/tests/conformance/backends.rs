//! `--backend cranelift|llvm` (`docs/llvm-backend.md` §4): the CLI wiring
//! for the second backend's first slice, checked the way
//! `differential.rs` checks the constant folder -- two independent paths
//! agreeing on one answer is the point, not a detail of either.

use super::*;

fn build_with(tag: &str, relative: &str, backend: &str) -> std::process::Output {
    let dir = scratch(tag);
    let exe = dir.join("out");
    let build = Command::new(BIN)
        .args([
            "build".as_ref(),
            repo_root().join(relative).as_os_str(),
            "--std".as_ref(),
            "--backend".as_ref(),
            backend.as_ref(),
            "-o".as_ref(),
            exe.as_os_str(),
        ])
        .output()
        .expect("the compiler runs");
    if !build.status.success() {
        let _ = std::fs::remove_dir_all(&dir);
        return build;
    }
    // Neither fixture this module builds declares `//~ STDIN`, so a plain
    // run -- no piped input -- is exact, not merely convenient.
    let run = Command::new(&exe).output().expect("the compiled program runs");
    let _ = std::fs::remove_dir_all(&dir);
    run
}

fn assert_backends_agree(tag: &str, relative: &str, expected_stdout: &str) {
    let cranelift = build_with(&format!("{tag}-cranelift"), relative, "cranelift");
    let llvm = build_with(&format!("{tag}-llvm"), relative, "llvm");

    assert!(
        llvm.status.success(),
        "`--backend llvm` should build and run `{relative}`, but said:\n{}",
        String::from_utf8_lossy(&llvm.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&llvm.stdout),
        String::from_utf8_lossy(&cranelift.stdout),
        "the two backends printed different things for the same program"
    );
    assert_eq!(
        llvm.status.code(),
        cranelift.status.code(),
        "the two backends exited differently for the same program"
    );
    assert_eq!(String::from_utf8_lossy(&llvm.stdout), expected_stdout);
    assert_eq!(llvm.status.code(), Some(0));
}

/// The LLVM backend's first slice (`docs/llvm-backend.md` §5) builds
/// `tests/accept/llvm_smoke.cho` -- the fixture the doc's original bullet
/// list actually describes -- and the two backends agree with each other
/// and with the fixture's own `//~ STDOUT`/`//~ EXIT` directives.
#[test]
fn the_two_backends_agree_on_the_llvm_smoke_fixture() {
    assert_backends_agree("backends-smoke", "tests/accept/llvm_smoke.cho", "Hi!\n");
}

/// §5's second slice: checked arithmetic. `tests/accept/llvm_arith.cho`
/// exercises every trapping `BinOp` plus the bitwise operators, and the
/// two backends compute byte-for-byte the same answers.
#[test]
fn the_two_backends_agree_on_the_llvm_arith_fixture() {
    assert_backends_agree("backends-arith", "tests/accept/llvm_arith.cho", "Hi! OK$iK\n");
}

/// §5's third slice: control flow. `tests/accept/llvm_control.cho`
/// exercises `if`/`else`, `while`, every comparison and both
/// short-circuit operators, and the two backends compute byte-for-byte
/// the same answers.
#[test]
fn the_two_backends_agree_on_the_llvm_control_fixture() {
    assert_backends_agree(
        "backends-control",
        "tests/accept/llvm_control.cho",
        "01X34Z\n+-42\nF\n!T\nT\n!T\n",
    );
}

/// §5's fourth slice: slices and strings. `examples/hello.cho` -- the
/// program §5 originally (and wrongly) named as the first slice's own
/// target -- is what closes the loop: `ci.yml`'s own smoke test, built
/// and run through the second backend, byte for byte the same as the
/// first.
#[test]
fn the_two_backends_agree_on_hello_ls() {
    assert_backends_agree("backends-hello", "examples/hello.cho", "Hello, world!\n");
}

/// §5's fifth slice: structs and enums. `tests/accept/enums.cho`
/// exercises a struct literal, an enum with payloads (one of them
/// struct-typed), and `match` -- a chain of tag tests, a wildcard arm,
/// and field access on an owned matched binding -- and the two backends
/// compute byte-for-byte the same answers.
#[test]
fn the_two_backends_agree_on_the_enums_fixture() {
    assert_backends_agree("backends-enums", "tests/accept/enums.cho", "001220069901\n");
}

/// `docs/llvm-backend.md` §7: three of `benches/`' loop-heavy kernels
/// build on both backends already -- `sum_checked.cho` (tight checked
/// arithmetic, no memory traffic), `fib_checked.cho` (recursion, so the
/// cost is calls rather than arithmetic) and `benches/three/mandelbrot.cho`
/// (Q16.16 fixed-point compute, `docs/against-c-and-rust.md`'s own
/// kernel). Both communicate correctness through their exit code
/// (`result - expected`, zero when right) rather than `stdout`, except
/// `mandelbrot.cho`, which prints a checksum -- `scripts/backend_compare.py`
/// is where the timing comes from; this is only agreement.
#[test]
fn the_two_backends_agree_on_sum_checked() {
    assert_backends_agree("backends-sum", "benches/sum_checked.cho", "");
}

#[test]
fn the_two_backends_agree_on_fib_checked() {
    assert_backends_agree("backends-fib", "benches/fib_checked.cho", "");
}

#[test]
fn the_two_backends_agree_on_mandelbrot() {
    assert_backends_agree("backends-mandelbrot", "benches/three/mandelbrot.cho", "39690297\n");
}

/// `docs/llvm-backend.md` §7.3's first-named gap, closed: `wrapping_add`/
/// `sub`/`mul` are LLVM's own `add`/`sub`/`mul`, already two's-complement
/// wraparound with no `nsw`/`nuw` requested, so unlike `binop`'s checked
/// forms this needs no overflow check at all. Closing it makes every
/// checked-vs-wrapping pair in `benches/` buildable on `--backend llvm`
/// for the first time, and unblocks `benches/three/purity.cho` besides.
#[test]
fn the_two_backends_agree_on_sum_wrapping() {
    assert_backends_agree("backends-sum-wrapping", "benches/sum_wrapping.cho", "");
}

#[test]
fn the_two_backends_agree_on_fib_wrapping() {
    assert_backends_agree("backends-fib-wrapping", "benches/fib_wrapping.cho", "");
}

#[test]
fn the_two_backends_agree_on_purity() {
    assert_backends_agree("backends-purity", "benches/three/purity.cho", "-7463529374017724416\n");
}

/// `docs/llvm-backend.md` §7.5: `region`/`alloc_slice` closed -- one
/// `malloc` in, one `free` out, and a bump pointer kept in two `ptr`-typed
/// `alloca` cells rather than in an SSA `Variable`, the same arena
/// `cancho-codegen`'s own `body/memory.rs` already builds. `byte_of` and
/// `!` (`Expr::Not`) closed alongside it -- both were the only things
/// standing between this and `sieve`/`scan` actually building.
#[test]
fn the_two_backends_agree_on_sieve_checked() {
    assert_backends_agree("backends-sieve-checked", "benches/sieve_checked.cho", "");
}

#[test]
fn the_two_backends_agree_on_sieve_wrapping() {
    assert_backends_agree("backends-sieve-wrapping", "benches/sieve_wrapping.cho", "");
}

#[test]
fn the_two_backends_agree_on_scan_checked() {
    assert_backends_agree("backends-scan-checked", "benches/scan_checked.cho", "");
}

#[test]
fn the_two_backends_agree_on_scan_wrapping() {
    assert_backends_agree("backends-scan-wrapping", "benches/scan_wrapping.cho", "");
}

#[test]
fn the_two_backends_agree_on_the_three_language_sieve() {
    assert_backends_agree("backends-sieve-three", "benches/three/sieve.cho", "6057\n");
}

/// `docs/llvm-backend.md` §7.7: heap boxing (`box_slice`/`contents`/
/// `unbox_slice`) closed, plus multi-leaf returns -- `fill` in
/// `reduce_checked.cho` returns `Box[[int]]`, two leaves, which the LLVM
/// backend could not hand back out of a call at all until this slice.
#[test]
fn the_two_backends_agree_on_reduce_checked() {
    assert_backends_agree("backends-reduce-checked", "benches/reduce_checked.cho", "");
}

#[test]
fn the_two_backends_agree_on_reduce_wrapping() {
    assert_backends_agree("backends-reduce-wrapping", "benches/reduce_wrapping.cho", "");
}

#[test]
fn the_two_backends_agree_on_layout_aos() {
    assert_backends_agree("backends-layout-aos", "benches/layout/aos.cho", "32000000\n");
}

#[test]
fn the_two_backends_agree_on_layout_soa() {
    assert_backends_agree("backends-layout-soa", "benches/layout/soa.cho", "32000000\n");
}

#[test]
fn the_two_backends_agree_on_layout_ints() {
    assert_backends_agree("backends-layout-ints", "benches/layout/ints.cho", "192000000\n");
}

#[test]
fn the_two_backends_agree_on_layout_rgb() {
    assert_backends_agree("backends-layout-rgb", "benches/layout/rgb.cho", "192000000\n");
}

/// `docs/llvm-backend.md` §7.9: `getchar` closed (the mirror of
/// `putchar`, sign-extended the same way), plus `s[a..b]`
/// (`Expr::Subslice`) found sitting in front of `revcomp.cho` once it was
/// tried against it. `tests/accept/stdin_roundtrip.cho` is the first
/// fixture here needing piped input, so it gets its own comparison
/// rather than reusing `assert_backends_agree`, which never pipes one.
#[test]
fn the_two_backends_agree_on_stdin_roundtrip() {
    let stdin = "hello\nworld\n";
    let expected = "hello\nworld\n12 bytes\n";
    for backend in ["cranelift", "llvm"] {
        let dir = scratch(&format!("backends-stdin-{backend}"));
        let exe = dir.join("out");
        let build = Command::new(BIN)
            .args([
                "build".as_ref(),
                repo_root().join("tests/accept/stdin_roundtrip.cho").as_os_str(),
                "--std".as_ref(),
                "--backend".as_ref(),
                backend.as_ref(),
                "-o".as_ref(),
                exe.as_os_str(),
            ])
            .output()
            .expect("the compiler runs");
        assert!(
            build.status.success(),
            "`--backend {backend}` should build `stdin_roundtrip.cho`, but said:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );

        let mut child = Command::new(&exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the program runs");
        child
            .stdin
            .take()
            .expect("a piped stdin")
            .write_all(stdin.as_bytes())
            .expect("the program accepts its input");
        let output = child.wait_with_output().expect("the program finishes");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(output.status.code(), Some(0), "`--backend {backend}` should exit 0");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "`--backend {backend}` printed the wrong thing"
        );
    }
}

/// `docs/llvm-backend.md` §7.11: `Place::Field`/`Place::Deref` closed --
/// writing through a reference, whole-referent or one field, plus the
/// read-side siblings (`Expr::FieldRef`/`Expr::FieldAddr`/`Expr::Deref`)
/// nothing had built either. `tests/accept/deref_roundtrip.cho` already
/// existed as exactly this fixture: `*n`/`*n = e` (a shared and a unique
/// reference to a bare `int`), and `scale`'s `p.x`/`p.y` reading a field
/// *through* a reference before `*p = Point { .. }` replaces the whole
/// referent.
#[test]
fn the_two_backends_agree_on_deref_roundtrip() {
    assert_backends_agree(
        "backends-deref-roundtrip",
        "tests/accept/deref_roundtrip.cho",
        "41 42\n3 4 -> 30 40\n",
    );
}

/// §7.11 closed tuples alongside struct/reference writes: `Expr::Tuple`,
/// `Expr::TupleField` (an owner's `.0`/`.1`) and `Expr::TupleFieldRef`
/// (the same, through a reference -- `sum`'s own `pair.0 + pair.1`
/// where `pair: &p (int, int)`), none of them built before this slice
/// either. `tests/accept/tuple_roundtrip.cho` already existed as exactly
/// this fixture.
#[test]
fn the_two_backends_agree_on_tuple_roundtrip() {
    assert_backends_agree(
        "backends-tuple-roundtrip",
        "tests/accept/tuple_roundtrip.cho",
        "7 true\n42\n",
    );
}

/// §7.11 also closed `write_bytes`/`write_err` (`docs/bulk-io.md` §3),
/// found trying to build `revcomp.cho` once field/reference writes no
/// longer stood in front of it -- `fwrite` through `stdout`/`stderr`,
/// the platform-specific symbol `cancho-codegen`'s own `emit.rs`
/// already resolves. `revcomp.cho` is the real target #106 traced this
/// boundary to, checked against the Benchmarks Game's own reference
/// output the same way `benchmarks.rs`'s
/// `fasta_and_reverse_complement_print_the_published_answer` already
/// does for `--backend cranelift`.
#[test]
fn the_two_backends_agree_on_revcomp() {
    let root = repo_root().join("benches").join("game");
    let stdin = std::fs::read_to_string(root.join("fasta-1000.txt")).expect("fasta-1000.txt");
    let expected =
        std::fs::read_to_string(root.join("revcomp-1000.txt")).expect("revcomp-1000.txt");

    for backend in ["cranelift", "llvm"] {
        let dir = scratch(&format!("backends-revcomp-{backend}"));
        let exe = dir.join("out");
        let build = Command::new(BIN)
            .args([
                "build".as_ref(),
                root.join("revcomp.cho").as_os_str(),
                "--std".as_ref(),
                "--backend".as_ref(),
                backend.as_ref(),
                "-o".as_ref(),
                exe.as_os_str(),
            ])
            .output()
            .expect("the compiler runs");
        assert!(
            build.status.success(),
            "`--backend {backend}` should build `revcomp.cho`, but said:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );

        let mut child = Command::new(&exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the program runs");
        child
            .stdin
            .take()
            .expect("a piped stdin")
            .write_all(stdin.as_bytes())
            .expect("the program accepts its input");
        let output = child.wait_with_output().expect("the program finishes");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(output.status.code(), Some(0), "`--backend {backend}` should exit 0");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "`--backend {backend}` should print what the Benchmarks Game publishes"
        );
    }
}

/// `docs/llvm-backend.md` §7.13: `arg_count`/`arg` closed, `argc`/`argv`
/// stashed once into module-local storage by `@main` before the entry
/// function's own body runs (`docs/arguments.md` §3). No arguments are
/// passed here -- `assert_backends_agree` never does -- so this only
/// exercises the "one argument, its own name" path; `arg(a, 0)` and the
/// bounds trap are `arg_count_and_arg_build_and_run_the_arguments_fixture`'s
/// job in the crate's own suite, and real arguments are what
/// `the_two_backends_agree_on_fannkuch`, below, passes.
#[test]
fn the_two_backends_agree_on_arguments() {
    assert_backends_agree("backends-arguments", "tests/accept/arguments.cho", "1\nnamed: 1\n");
}

/// `fannkuch.cho` itself, past `arg_count`/`arg`: the boundary #106 traced
/// for `revcomp.cho` also applied here, and this is the first program in
/// this module actually given a real argument, exercising `arg`'s bounds
/// check and `strlen` read together with `arg_count`'s own count rather
/// than only the no-argument path `assert_backends_agree` covers.
/// `binarytrees.cho` -- `arg_count`/`arg`'s other named target -- refused
/// past this at the time (it also needed bare `Expr::Boxed`); §7.15
/// closed that too, and it gets its own test below.
#[test]
fn the_two_backends_agree_on_fannkuch() {
    for backend in ["cranelift", "llvm"] {
        let dir = scratch(&format!("backends-fannkuch-{backend}"));
        let exe = dir.join("out");
        let build = Command::new(BIN)
            .args([
                "build".as_ref(),
                repo_root().join("benches/game/fannkuch.cho").as_os_str(),
                "--std".as_ref(),
                "--backend".as_ref(),
                backend.as_ref(),
                "-o".as_ref(),
                exe.as_os_str(),
            ])
            .output()
            .expect("the compiler runs");
        assert!(
            build.status.success(),
            "`--backend {backend}` should build `fannkuch.cho`, but said:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );

        let output = Command::new(&exe).arg("8").output().expect("the program runs");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(output.status.code(), Some(0), "`--backend {backend}` should exit 0");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "1616\nPfannkuchen(8) = 22\n",
            "`--backend {backend}` printed the wrong thing"
        );
    }
}

/// `docs/llvm-backend.md` §7.15: bare `Expr::Alloc`/`Expr::Boxed`/
/// `Expr::Unboxed` closed -- single-value allocation, arena or heap,
/// the same `bump`/`malloc` this backend already opened for a slice's
/// many elements, minus the fill loop. `tests/accept/arena_roundtrip.cho`
/// and `tests/accept/box_roundtrip.cho` were the two ready-made fixtures
/// exercising `alloc`/`box`+`unbox` respectively.
#[test]
fn the_two_backends_agree_on_arena_roundtrip() {
    assert_backends_agree(
        "backends-arena-roundtrip",
        "tests/accept/arena_roundtrip.cho",
        "0 1 4 9 16 25 36 49 = 140\nnested: 7\n",
    );
}

#[test]
fn the_two_backends_agree_on_box_roundtrip() {
    assert_backends_agree("backends-box-roundtrip", "tests/accept/box_roundtrip.cho", "7\n10 4\n");
}

/// `binarytrees.cho` itself, past `arg_count`/`arg` and now past bare
/// `Expr::Boxed` too: `build`'s own `box[h](Tree::Node {..})` was the
/// gap §7.13 found and did not close. Given a real depth the same way
/// `the_two_backends_agree_on_fannkuch` is, exercising `alloc`/`box`/
/// `unbox` together with `arg_count`/`arg` rather than either alone.
#[test]
fn the_two_backends_agree_on_binarytrees() {
    for backend in ["cranelift", "llvm"] {
        let dir = scratch(&format!("backends-binarytrees-{backend}"));
        let exe = dir.join("out");
        let build = Command::new(BIN)
            .args([
                "build".as_ref(),
                repo_root().join("benches/game/binarytrees.cho").as_os_str(),
                "--std".as_ref(),
                "--backend".as_ref(),
                backend.as_ref(),
                "-o".as_ref(),
                exe.as_os_str(),
            ])
            .output()
            .expect("the compiler runs");
        assert!(
            build.status.success(),
            "`--backend {backend}` should build `binarytrees.cho`, but said:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );

        let output = Command::new(&exe).arg("8").output().expect("the program runs");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(output.status.code(), Some(0), "`--backend {backend}` should exit 0");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "stretch tree of depth 9\t check: 1023\n\
             256\t trees of depth 4\t check: 7936\n\
             64\t trees of depth 6\t check: 8128\n\
             16\t trees of depth 8\t check: 8176\n\
             long lived tree of depth 8\t check: 511\n",
            "`--backend {backend}` printed the wrong thing"
        );
    }
}

/// `docs/llvm-backend.md` §7.17: `Type::Float` closed -- literals,
/// arithmetic, comparison, both conversions, `bits_of`/`is_nan`, and
/// `sqrt`. `tests/accept/floating_point.cho` is the dedicated fixture,
/// checked here through the CLI the way the crate's own suite cannot
/// (it imports `std.io`, which `compile_object`'s bare `lower` has no
/// `--std` source injection to resolve).
#[test]
fn the_two_backends_agree_on_floating_point() {
    assert_backends_agree(
        "backends-floating-point",
        "tests/accept/floating_point.cho",
        "sum 35\nhalf 5\nneg -27\nexponent 2\nnan-is-nan 1\nnan-equals-itself 0\n\
         inf-beats-everything 1\ntoward-zero -2\nbits-of-one 4607182418800017408\n\
         sign-of-minus-zero 1\nminus-zero-equals-zero 1\n",
    );
}

/// `spectral.cho` itself: accumulation loops, `float_of`, and `sqrt`
/// together, `Type::Float`'s first real `benches/` target -- no
/// arguments needed, unlike `fasta.cho` below.
#[test]
fn the_two_backends_agree_on_spectral() {
    assert_backends_agree("backends-spectral", "benches/game/spectral.cho", "1.274219991\n");
}

/// `fasta.cho`, `Type::Float`'s other named target: many-digit decimal
/// literals not exactly representable in binary, and a real runtime
/// float comparison (`cumulative[idx] < r`) picking which base to
/// print, not just arithmetic. A small `n` keeps the expected output
/// short; `benches/game/fasta-1000.txt` is where the Benchmarks Game's
/// own published reference is checked, manually, against a much larger
/// run (`docs/llvm-backend.md` §7.17).
#[test]
fn the_two_backends_agree_on_fasta() {
    for backend in ["cranelift", "llvm"] {
        let dir = scratch(&format!("backends-fasta-{backend}"));
        let exe = dir.join("out");
        let build = Command::new(BIN)
            .args([
                "build".as_ref(),
                repo_root().join("benches/game/fasta.cho").as_os_str(),
                "--std".as_ref(),
                "--backend".as_ref(),
                backend.as_ref(),
                "-o".as_ref(),
                exe.as_os_str(),
            ])
            .output()
            .expect("the compiler runs");
        assert!(
            build.status.success(),
            "`--backend {backend}` should build `fasta.cho`, but said:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );

        let output = Command::new(&exe).arg("10").output().expect("the program runs");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(output.status.code(), Some(0), "`--backend {backend}` should exit 0");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            ">ONE Homo sapiens alu\n\
             GGCCGGGCGCGGTGGCTCAC\n\
             >TWO IUB ambiguity codes\n\
             cttBtatcatatgctaKggNcataaaSatg\n\
             >THREE Homo sapiens frequency\n\
             taaatcttgtgcttcgttagaagtctcgactacgtgtagcctagtgtttg\n",
            "`--backend {backend}` printed the wrong thing"
        );
    }
}

/// `docs/llvm-backend.md` §7.19: matching through a reference closed --
/// `tests/accept/match_a_reference.cho` reads the same list three times
/// through a shared reference (twice via `total`, once via `length`,
/// one of them discarding a bound position with `_`) before consuming
/// and freeing it once.
#[test]
fn the_two_backends_agree_on_match_a_reference() {
    assert_backends_agree(
        "backends-match-a-reference",
        "tests/accept/match_a_reference.cho",
        "10 3 10\nfreed 10\n",
    );
}

/// `examples/tree.cho`, the richer target §7.19 names: a three-field
/// variant (two `Box[Tree]`, one `int`), matched by reference three
/// separate ways -- all three bound and recursed (`contains`), the
/// first position discarded past two others (`deepest`), and all three
/// composed into a multi-leaf struct return (`tally`, folding
/// sum/count/depth). Not gated by any test until now, but named by
/// `docs/reading-references.md` §4 and `docs/llvm-backend.md` as this
/// feature's own motivating case.
#[test]
fn the_two_backends_agree_on_tree() {
    assert_backends_agree(
        "backends-tree",
        "examples/tree.cho",
        "1 3 4 5 7 8 9\nsum 37 count 7 depth 3\nhas 4: 1  has 6: 0  deepest 9\n",
    );
}

/// The boundary this slice draws is a located refusal, not a crash or a
/// silent wrong answer: `region`/`alloc_slice` moved out of this list
/// once §7.5 landed; bare `alloc[a]`/`box`/`unbox` moved out once §7.15
/// landed; `Type::Float` moved out once §7.17 landed; matching through
/// a reference moved out once §7.19 landed; a foreign call moved out
/// once §7.23 landed; `Fs` moved out once §7.24 landed; `Expr::Static`
/// and `Expr::BitNot` moved out once §7.25 landed. The last item on this
/// list, `examples/collect/collect.cho`'s own `extern fn socket`
/// colliding with this backend's internal `@socket` declaration
/// (`docs/ROADMAP.md` #92, once "already-documented, already-accepted"
/// because nothing built by default ever hit it), moved out too, once
/// `--backend llvm` becoming the default made it a live regression
/// rather than a theoretical one: every libc symbol a pre-`Net` program
/// might declare for itself (`socket`/`bind`/`listen`/`accept`/
/// `connect`/`getaddrinfo`/`freeaddrinfo`/`close`/`creat`/`open`, beside
/// `read`/`write`, already guarded) is now declared only when the
/// program's own `extern fn` does not already claim the symbol -- the
/// same rule `read`/`write` were guarded by first. This test now proves
/// the fix rather than pinning the collision: every socket-declaring
/// example in the corpus builds clean on `--backend llvm`.
#[test]
fn every_socket_declaring_example_builds_clean_on_llvm() {
    // Every program this test used to build bare now `import`s
    // `net.sockets` and/or `net.connect` (`docs/package-system.md` §6)
    // rather than declaring its own socket `extern fn`s, so each needs
    // its fetched package(s) alongside it rather than a bare
    // single-file build -- the same shape `examples/serve/`'s and
    // `examples/fetch/`'s own blocks below already use. The collision
    // this test guards against now lives in the *package's* `extern fn
    // socket`, not the program's own, but the guard is the same: the
    // first declaration on the command line wins over this backend's
    // internal one.
    fn assert_llvm_builds_with_deps(tag: &str, main_relative: &str, deps: &[PathBuf]) {
        let dir = scratch(tag);
        let exe = dir.join("out");
        let mut command = Command::new(BIN);
        command.arg("build");
        command.arg(repo_root().join(main_relative));
        for f in deps {
            command.arg(f);
        }
        command.args([
            "--std".as_ref(),
            "--backend".as_ref(),
            "llvm".as_ref(),
            "-o".as_ref(),
            exe.as_os_str(),
        ]);
        let build = command.output().expect("the compiler runs");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            build.status.success(),
            "`{main_relative}` should build on `--backend llvm` now that its own `extern fn` \
             wins over this backend's internal declaration, but the compiler said:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
    }

    assert_llvm_builds_with_deps(
        "backends-llvm-socket-collect",
        "examples/collect/collect.cho",
        &fetch_net_dependencies(
            "backends-llvm-socket-collect-fetch",
            &[("examples/collect/request.lock", "packages/http-request/.cancho-vcs")],
        ),
    );
    assert_llvm_builds_with_deps(
        "backends-llvm-socket-report",
        "examples/report/report.cho",
        &fetch_report_dependencies("backends-llvm-socket-report-fetch"),
    );
    assert_llvm_builds_with_deps(
        "backends-llvm-socket-vsock",
        "examples/vsock/vsock.cho",
        &fetch_vsock_dependencies("backends-llvm-socket-vsock-fetch"),
    );
    assert_llvm_builds_with_deps(
        "backends-llvm-socket-agent-guest",
        "examples/agent_guest/agent_guest.cho",
        &fetch_agent_guest_dependencies("backends-llvm-socket-agent-guest-fetch"),
    );
    assert_llvm_builds_with_deps(
        "backends-llvm-socket-agent-supervisor",
        "examples/agent_supervisor/agent_supervisor.cho",
        &fetch_net_dependencies(
            "backends-llvm-socket-agent-supervisor-fetch",
            &[("examples/agent_supervisor/request.lock", "packages/http-request/.cancho-vcs")],
        ),
    );

    // `examples/serve/serve.cho` no longer declares its own socket
    // `extern fn`s -- it `import`s `net.sockets` (`packages/net-sockets/`,
    // `docs/package-system.md` §6) -- so it needs the fetched package
    // alongside it rather than a bare single-file build like the rest of
    // this loop.
    let fetched = fetch_net_sockets("backends-llvm-socket-serve-fetch", "examples/serve/net.lock");
    let dir = scratch("backends-llvm-socket-serve");
    let exe = dir.join("out");
    let build = Command::new(BIN)
        .args([
            "build".as_ref(),
            repo_root().join("examples/serve/serve.cho").as_os_str(),
            fetched.as_os_str(),
            "--std".as_ref(),
            "--backend".as_ref(),
            "llvm".as_ref(),
            "-o".as_ref(),
            exe.as_os_str(),
        ])
        .output()
        .expect("the compiler runs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        build.status.success(),
        "`examples/serve/serve.cho` should build on `--backend llvm` with its fetched \
         `net.sockets` package, but the compiler said:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    // `examples/fetch/fetch.cho` needs two fetched packages at once
    // (`net.sockets` and `net.connect`, `docs/package-system.md` §6).
    let fetched = fetch_dependencies("backends-llvm-socket-fetch-fetch");
    let dir = scratch("backends-llvm-socket-fetch");
    let exe = dir.join("out");
    let mut command = Command::new(BIN);
    command.arg("build");
    command.arg(repo_root().join("examples/fetch/fetch.cho"));
    for f in &fetched {
        command.arg(f);
    }
    command.args([
        "--std".as_ref(),
        "--backend".as_ref(),
        "llvm".as_ref(),
        "-o".as_ref(),
        exe.as_os_str(),
    ]);
    let build = command.output().expect("the compiler runs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        build.status.success(),
        "`examples/fetch/fetch.cho` should build on `--backend llvm` with its fetched \
         `net.sockets`/`net.connect` packages, but the compiler said:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
}

/// §7.25: `Expr::Static`, closed. `tests/accept/static_data.cho` is the
/// whole feature in one program: a table built by a loop, a `[byte]`
/// table packed at the same one-byte stride every other byte slice
/// uses, a second table built by calling a pure function against the
/// first, and a pure function that reads a `static` directly --
/// checked against Cranelift byte for byte rather than only against "it
/// built".
#[test]
fn the_two_backends_agree_on_static_data() {
    assert_backends_agree(
        "backends-static-data",
        "tests/accept/static_data.cho",
        "squares 0 1 4 9 16 25 36 49 64 81\nshifted HELLO\ndoubled 0 2 8 18 32 50 72 98 128 162\n\
         pure-read 25\n",
    );
}

/// §7.25: `Expr::BitNot`, closed. `tests/accept/bitwise.cho` already ran
/// against Cranelift alone (`corpus.rs`'s `accepted_programs_build_and_
/// run`, which builds without `--backend` and so never touched this
/// backend); this is its first check against `--backend llvm`. Its own
/// `~0` folds to a literal before codegen ever sees a `BitNot` node, so
/// it alone would not have caught this backend's missing arm --
/// `bitnot_flips_every_bit_not_just_the_low_one`
/// (`crates/cancho-codegen-llvm/src/tests.rs`) is what forces a
/// genuinely unfoldable operand -- but every other operator on this
/// fixture's own page (`&`, `|`, `^`, `<<`, `>>`, plus the precedence
/// and shift-edge cases) is worth the same byte-for-byte check the rest
/// of this file gives every other closed gap.
#[test]
fn the_two_backends_agree_on_bitwise() {
    assert_backends_agree(
        "backends-bitwise",
        "tests/accept/bitwise.cho",
        "and 8\nor 15\nxor 6\nnot -1\nshl 16\nshr -4\nsign 1\nmask 13\ntight 1\n",
    );
}

/// §7.23: a foreign call, closed -- the gap the test above used to name.
/// `tests/accept/bytes_to_c.cho` is the `&r [byte]`-crossing case
/// (`write(fd, ptr, len)`, a literal and an arena slice both crossing as
/// pointer-and-length); `extern_fn_labs_computes_the_real_answer`
/// (`crates/cancho-codegen-llvm/src/tests.rs`) is the plain-`int` case.
#[test]
fn the_two_backends_agree_on_bytes_to_c() {
    assert_backends_agree(
        "backends-bytes-to-c",
        "tests/accept/bytes_to_c.cho",
        "written straight to fd 1\nand so was this\n",
    );
}

/// §7.24: `Fs`, closed -- the gap `a_program_outside_this_backend_is_
/// refused_through_the_cli` used to name. `tests/accept/file_handle.cho`
/// is the whole milestone in one program: `fs_write`, `open_read`,
/// `file_read` (twice, including the `End` case a second read proves
/// rather than assumes) and `file_close`.
#[test]
fn the_two_backends_agree_on_file_handle() {
    assert_backends_agree(
        "backends-file-handle",
        "tests/accept/file_handle.cho",
        "got 12\nend\nend again\nclosed\n",
    );
}

/// `docs/llvm-backend.md` §7.20: `listen`/`accept`, closed -- the first
/// crack in `Net` itself, not only in the foreign calls a program might
/// use to reach a socket. `tests/accept/listen_accept_bad_fd.cho` calls
/// both against a deliberately invalid fd, so the two backends agree
/// without either one needing a real socket.
#[test]
fn the_two_backends_agree_on_listen_and_accept_on_a_bad_fd() {
    assert_backends_agree(
        "backends-listen-accept-bad-fd",
        "tests/accept/listen_accept_bad_fd.cho",
        "",
    );
}

/// `docs/llvm-backend.md` §7.21: `bind`, closed -- the second of `Net`'s
/// four builtins, after `listen`/`accept` (§7.20). Unlike the bad-fd
/// fixture above, this reaches a *real* fd through `bind` itself, the
/// milestone this slice actually delivers: `--backend llvm` had no way to
/// obtain one before `bind` lowered (`Ffi`/`extern fn` and `connect` are
/// both still refused). `assert_backends_agree` cannot be reused here --
/// it runs a build to completion, and `accept` blocks until a peer
/// connects -- so each backend is built and spawned directly, with a real
/// `TcpStream` from this process supplying the connection, the same shape
/// `crates/cancho/tests/conformance/net.rs`'s own
/// `a_cancho_listener_accepts_a_real_connection` uses for Cranelift
/// alone. `read`/`write` on the accepted connection are left out --
/// that needs `extern fn`, still outside this backend -- so this checks
/// only that both backends bind the port and accept the connection.
#[test]
fn the_two_backends_bind_and_accept_a_real_connection() {
    use std::io::Write as _;
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    for backend in ["cranelift", "llvm"] {
        let port = free_port();
        let dir = scratch(&format!("backends-bind-accept-{backend}"));
        let source = dir.join("listener.cho");
        std::fs::write(
            &source,
            format!(
                "edition 2;\n\
                 fn main(world: World) -> [] int {{\n\
                     let Split {{ io, ffi, fs, heap, args, net }} = split(world);\n\
                     release(io); release(fs); release(heap); release(args); release(ffi);\n\
                     let bound = narrow(net, \"{port}\");\n\
                     var status = 1;\n\
                     borrow bound as &n in {{\n\
                         let listener = bind(n, {port});\n\
                         if listener >= 0 {{\n\
                             listen(listener, 1);\n\
                             let conn = accept(listener);\n\
                             if conn >= 0 {{\n\
                                 status = 0;\n\
                             }}\n\
                         }}\n\
                     }}\n\
                     release(bound);\n\
                     return status;\n\
                 }}\n",
            ),
        )
        .expect("a writable fixture");

        let exe = dir.join("listener");
        let build = Command::new(BIN)
            .args([
                "build".as_ref(),
                source.as_os_str(),
                "--std".as_ref(),
                "--backend".as_ref(),
                backend.as_ref(),
                "-o".as_ref(),
                exe.as_os_str(),
            ])
            .output()
            .expect("the compiler runs");
        assert!(
            build.status.success(),
            "`--backend {backend}` should build the listener, but said:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );

        let mut child = Command::new(&exe).spawn().expect("the listener runs");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(stream) => break stream,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    panic!("`--backend {backend}` could not connect within the deadline: {e}")
                }
            }
        };
        stream.write_all(b"ping").expect("the write succeeds");
        drop(stream);

        let run = child.wait().expect("the listener exits");
        assert_eq!(
            run.code(),
            Some(0),
            "`--backend {backend}` should accept the connection and report success"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `docs/llvm-backend.md` §7.22: `connect`, closed -- the last of `Net`'s
/// four builtins, and the last gap `docs/agent-tools.md`'s own "still
/// refused" list named alongside `Ffi`/`extern fn`. A plain
/// `std::net::TcpListener` stands in for the peer -- `connect` does not
/// care what accepted it, only that something did -- so this isolates
/// the half `connect` actually adds: host resolution
/// (`getaddrinfo`/`freeaddrinfo`) and the connect call itself, not
/// `bind`'s own machinery, already checked in
/// `the_two_backends_bind_and_accept_a_real_connection`.
#[test]
fn the_two_backends_connect_to_a_real_listener() {
    for backend in ["cranelift", "llvm"] {
        let port = free_port();
        let listener =
            std::net::TcpListener::bind(("127.0.0.1", port)).expect("a free loopback port");

        let dir = scratch(&format!("backends-connect-{backend}"));
        let source = dir.join("client.cho");
        std::fs::write(
            &source,
            format!(
                "edition 2;\n\
                 fn main(world: World) -> [] int {{\n\
                     let Split {{ io, ffi, fs, heap, args, net }} = split(world);\n\
                     release(io); release(fs); release(heap); release(args); release(ffi);\n\
                     let bound = narrow(net, \"127.0.0.1:{port}\");\n\
                     var fd = 0 - 1;\n\
                     borrow bound as &n in {{\n\
                         fd = connect(n, \"127.0.0.1\", {port});\n\
                     }}\n\
                     release(bound);\n\
                     if fd >= 0 {{\n\
                         return 0;\n\
                     }}\n\
                     return 1;\n\
                 }}\n",
            ),
        )
        .expect("a writable fixture");

        let exe = dir.join("client");
        let build = Command::new(BIN)
            .args([
                "build".as_ref(),
                source.as_os_str(),
                "--std".as_ref(),
                "--backend".as_ref(),
                backend.as_ref(),
                "-o".as_ref(),
                exe.as_os_str(),
            ])
            .output()
            .expect("the compiler runs");
        assert!(
            build.status.success(),
            "`--backend {backend}` should build the client, but said:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );

        // Accept on a thread: the client's `connect` and this process's
        // `accept` are two sides of the same handshake, and either one
        // blocking first for the other is a race, not a bug in either.
        let accepted = std::thread::spawn(move || listener.accept());

        let run = Command::new(&exe).output().expect("the client runs");
        assert_eq!(
            run.status.code(),
            Some(0),
            "`--backend {backend}`'s `connect` should reach the listener, but said:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
        accepted.join().expect("the accept thread does not panic").expect("the peer connects");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `docs/crypto.md`: `std.crypto`'s first slice, SHA-256, checked against
/// the system `sha256sum` on both backends -- the same reference this
/// module's own design doc computed its vectors against rather than
/// trusting either backend's arithmetic to agree with a memorised digest.
#[test]
fn the_two_backends_agree_on_sha256() {
    assert_backends_agree(
        "backends-sha256",
        "tests/accept/sha256.cho",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n\
         ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n\
         248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1\n\
         d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592\n",
    );
}

/// `docs/ed25519.md`: three real keypairs generated and signed by the
/// system `openssl`, checked on both backends -- signature bytes,
/// genuine verify, and a tampered signature's rejection, all three
/// times.
#[test]
fn the_two_backends_agree_on_ed25519() {
    assert_backends_agree(
        "backends-ed25519",
        "tests/accept/ed25519.cho",
        "228ada2141ab7425651a4ce8d9c5ed0f720319957a1035218354a81f950b480ee5ad13cad8eec58674f044f1e87e3795c91c303e2664272ab2c9b2595a67ee02\n\
         1\n\
         0\n\
         58cfcc803d68d21df7d5ecbc10a5dce09e549da84ef7dc664240799091ad2a5fca20f458498d91832e4012f339a4a0cb28934209b36b401a46d722ac9449db04\n\
         1\n\
         0\n\
         8760c2941c58fb6b2cdd57b3b7ede001de1e9bcd1955bf243a8a0a72a7363ec77fe0e0b1ae1b5b045506bb61f943c2791756ff14fefc8925db194069df30bb07\n\
         1\n\
         0\n",
    );
}

/// `docs/sha512.md`: `std.crypto`'s second slice, SHA-512, checked
/// against the system `sha512sum` on both backends -- the same
/// reference this module's own design doc computed its vectors
/// against. The 112-byte vector is the one that matters most: it is
/// the smallest input that forces the two-block path.
#[test]
fn the_two_backends_agree_on_sha512() {
    assert_backends_agree(
        "backends-sha512",
        "tests/accept/sha512.cho",
        "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e\n\
         ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f\n\
         c01d080efd492776a1c43bd23dd99d0a2e626d481e16782e75d54c2503b5dc32bd05f0f1ba33e568b88fd2d970929b719ecbb152f58f130a407c8830604b70ca\n\
         07e547d9586f6a73f73fbac0435ed76951218fb7d0c8d788a309d785436bbb642e93a252a954f23912547d1e8a3b5ed6e1bfd7097821233fa0538f3db854fee6\n",
    );
}

/// `docs/reach.md` §3.4: `c_int`, checked on both backends. Before this
/// slice, `access`'s real `-1` on a missing path read back as
/// `4294967295` on either backend -- the upper 32 bits of the return
/// register were never sign-extended, only ever read as though they
/// were.
#[test]
fn the_two_backends_agree_on_a_narrow_foreign_return() {
    assert_backends_agree(
        "backends-narrow-return",
        "tests/accept/foreign_narrow_return.cho",
        "ok\n-1\n",
    );
}

/// `docs/opaque-pointers.md` §3: `c_ptr`, checked on both backends.
/// Cranelift holds it as a pointer-width `int`
/// (`abi::leaves_into`); LLVM holds it as its own distinct `ptr` kind
/// (`emit::leaves_into`) -- two different representations of the same
/// checker-level type, so agreement here is the one place a divergence
/// between them would actually show.
#[test]
fn the_two_backends_agree_on_an_opaque_pointer() {
    assert_backends_agree(
        "backends-opaque-pointer",
        "tests/accept/opaque_pointer.cho",
        "stdin opened\nclosed 0\nbad fd is null\n",
    );
}

/// `docs/function-values.md` §4.2, checked on both backends: Cranelift
/// takes a function's address with `func_addr` and calls through it
/// with `call_indirect`; LLVM reads a global symbol directly as a
/// `ptr` value and spells the callee's signature explicitly at the
/// call (opaque pointers carry none of their own). Two different
/// mechanisms for the same value.
#[test]
fn the_two_backends_agree_on_a_function_value() {
    assert_backends_agree(
        "backends-function-value",
        "tests/accept/function_value.cho",
        "30\nhello from a function value\n",
    );
}

/// A call through a function value as an operand (`f(x) + 1`). LLVM's
/// `scalar_kind` had no `CallIndirect` arm, so only the form that bound
/// the result to a `let` first compiled there; Cranelift always did both.
#[test]
fn the_two_backends_agree_on_a_function_value_as_an_operand() {
    assert_backends_agree(
        "backends-function-value-operand",
        "tests/accept/function_value_operand.cho",
        "62\n",
    );
}

/// `docs/threads.md` §2/§5 step 2: `spawn`/`join`, the single-leaf slice,
/// checked on both backends. Cranelift passes `body`'s address and the
/// payload straight to a `pthread_create` it declares on demand
/// (`libc_fn`, the same mechanism `malloc`/`free` already use), widening
/// a narrower-than-pointer payload with `uextend` since `call` requires
/// an exact type match; LLVM converts the same payload with an explicit
/// `inttoptr` (a bare integer is not a legal `ptr`-typed call operand in
/// its textual IR) and reads `join`'s result back with a plain `load`
/// from an untyped `alloca` cell, no conversion needed on that side.
#[test]
fn the_two_backends_agree_on_spawn_join() {
    assert_backends_agree("backends-spawn-join", "tests/accept/spawn_join.cho", "42\n");
}

/// The other half of the same slice: `spawn` hands back a *real* OS
/// thread, not a disguised ordinary call, checked directly rather than
/// trusted from the type checker alone (`docs/threads.md` §5 step 2).
/// `tests/accept/spawn_thread_ids.cho` also exercises §3's "shared
/// reference crosses into two spawns, and the spawning side still reads
/// it after both are joined" case -- the same `Ffi("libc")` capability
/// is the payload for two threads and is read a third time by `main`.
#[test]
fn the_two_backends_agree_on_spawn_thread_ids() {
    assert_backends_agree(
        "backends-spawn-thread-ids",
        "tests/accept/spawn_thread_ids.cho",
        "main and t1 differ\nmain and t2 differ\nt1 and t2 differ\n",
    );
}

/// `docs/threads.md` §5 step 2's wall-clock evidence: four threads each
/// blocked in a real `usleep(200ms)`, joined. This is what makes
/// `tests/accept/spawn_thread_ids.cho`'s distinct-thread-IDs check airtight
/// rather than merely suggestive -- four *simultaneous* sleeps are only
/// possible with genuine OS-level parallelism, not with `spawn`
/// secretly running `body` on the calling thread before returning a
/// handle. A sequential implementation would take about four times a
/// single sleep; this asserts comfortably under twice one sleep,
/// leaving generous headroom for process startup and scheduler jitter
/// on a loaded CI host.
#[test]
fn spawn_and_join_run_concurrently_not_sequentially() {
    use std::time::{Duration, Instant};

    for backend in ["cranelift", "llvm"] {
        let dir = scratch(&format!("backends-spawn-parallel-{backend}"));
        let exe = dir.join("out");
        let build = Command::new(BIN)
            .args([
                "build".as_ref(),
                repo_root().join("tests/accept/spawn_parallel_sleep.cho").as_os_str(),
                "--std".as_ref(),
                "--backend".as_ref(),
                backend.as_ref(),
                "-o".as_ref(),
                exe.as_os_str(),
            ])
            .output()
            .expect("the compiler runs");
        assert!(
            build.status.success(),
            "`--backend {backend}` should build the parallel-sleep fixture, but said:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );

        let start = Instant::now();
        let run = Command::new(&exe).output().expect("the program runs");
        let elapsed = start.elapsed();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(
            run.status.code(),
            Some(0),
            "`--backend {backend}` should exit 0, but said:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(
            elapsed < Duration::from_millis(400),
            "`--backend {backend}`: four 200ms sleeps took {elapsed:?} -- that looks \
             sequential, not concurrent"
        );
    }
}

/// `docs/threads.md` §5 step 3: an owned capability payload, checked on
/// both backends. `Io` is a zero-field capability (`defs.rs`'s own
/// `prelude_types`), so it crosses to `pthread_create`'s `void *arg` as
/// no leaves at all -- the same path this backend already built for a
/// `()` payload, extended in `crosses_to_a_thread` from "`Unit`" to "any
/// zero-field capability." No codegen changed for this slice; only the
/// checker's allowlist did.
#[test]
fn the_two_backends_agree_on_spawn_owned_io() {
    assert_backends_agree(
        "backends-spawn-owned-io",
        "tests/accept/spawn_owned_io.cho",
        "hello from a thread\n",
    );
}

/// The other named case: `File`, one real `i64` leaf (the fd) rather than
/// zero, crossing exactly the way a plain `int` payload already does --
/// both backends' `Spawn`/`Join` codegen work off the value's actual
/// machine width, not its surface `Type`, so a capability whose ABI
/// shape happens to match `int`'s costs nothing new either.
#[test]
fn the_two_backends_agree_on_spawn_owned_file() {
    assert_backends_agree("backends-spawn-owned-file", "tests/accept/spawn_owned_file.cho", "30\n");
}

/// `docs/parallelism.md` T2: `Net` and `Clock` as thread payloads. Both are
/// declared with no fields (`leaf_free`), so each crosses the way `Io` does;
/// `crosses_to_a_thread` had left them off its list although its own comment
/// named `Net` as zero-field. The threads do real work -- a clock read, a
/// `connect` that is refused -- on both backends.
#[test]
fn the_two_backends_agree_on_spawn_owned_clock() {
    assert_backends_agree("backends-spawn-owned-clock", "tests/accept/spawn_owned_clock.cho", "");
}

#[test]
fn the_two_backends_agree_on_spawn_owned_net() {
    assert_backends_agree("backends-spawn-owned-net", "tests/accept/spawn_owned_net.cho", "");
}

/// `docs/parallelism.md` §8.1: a struct holding a `Heap` crosses to a
/// thread by `&!` reference and the thread allocates and frees through the
/// field -- the carrying half of a worker's own heap, on both backends.
#[test]
fn the_two_backends_agree_on_spawn_heap_in_struct() {
    assert_backends_agree(
        "backends-spawn-heap-in-struct",
        "tests/accept/spawn_heap_in_struct.cho",
        "",
    );
}

/// `docs/parallelism.md` §8: a heap forked from the parent's, moved into each of two workers, a million allocate-and-free
/// rounds in each thread, and a box the fork allocates freed by the parent -- on both backends.
#[test]
fn the_two_backends_agree_on_fork_heap_workers() {
    assert_backends_agree("backends-fork-heap-workers", "tests/accept/fork_heap_workers.cho", "");
}

/// `docs/parallelism.md` §9: a `Clock` forked from the parent's, owned by each of two threads, read there; on both backends.
#[test]
fn the_two_backends_agree_on_fork_clock_workers() {
    assert_backends_agree("backends-fork-clock-workers", "tests/accept/fork_clock_workers.cho", "");
}

/// `docs/memory-moves.md`: `copy_within`, overlapping in both directions, empty at either end, inside a sub-slice; on both backends.
#[test]
fn the_two_backends_agree_on_copy_within() {
    assert_backends_agree("backends-copy-within", "tests/accept/copy_within.cho", "");
}

/// `docs/bulk-copy.md`: `copy_into` between two slices and between overlapping views of one, in both directions, and
/// `std.buffer` growing through it; on both backends.
#[test]
fn the_two_backends_agree_on_copy_into() {
    assert_backends_agree("backends-copy-into", "tests/accept/copy_into.cho", "");
}

/// `docs/byte-search.md`: `index_of_byte` at every position, at both extreme byte values, inside sub-slices, and `std.bytes`'
/// `find` and `count_byte` built on it, each against a byte loop; on both backends.
#[test]
fn the_two_backends_agree_on_index_of_byte() {
    assert_backends_agree("backends-index-of-byte", "tests/accept/index_of_byte.cho", "");
}

/// `docs/byte-search.md` §2: a program that declares `memchr` itself still builds, on both backends.
#[test]
fn the_two_backends_agree_beside_a_program_s_own_memchr() {
    assert_backends_agree(
        "backends-own-memchr",
        "tests/accept/index_of_byte_beside_own_memchr.cho",
        "",
    );
}

/// `docs/word-scan.md`: the word-at-a-time primitives against loops written a byte and a bit at a time, on both backends.
#[test]
fn the_two_backends_agree_on_word_scan() {
    assert_backends_agree("backends-word-scan", "tests/accept/word_scan.cho", "");
}

/// `docs/word-scan.md` §6: an edition-7 program's own `popcount` and friends are untouched, on both backends.
#[test]
fn the_two_backends_agree_beside_a_program_s_own_word_scan_names() {
    assert_backends_agree(
        "backends-word-scan-own-names",
        "tests/accept/word_scan_beside_own_names.cho",
        "",
    );
}

/// `join(a) + join(b)`: a join as an arithmetic operand, which the LLVM backend could not type.
#[test]
fn the_two_backends_agree_on_a_join_as_an_operand() {
    assert_backends_agree(
        "backends-spawn-join-operands",
        "tests/accept/spawn_join_operands.cho",
        "",
    );
}

/// `docs/parallelism.md` §3.1: a unique reference to a *struct* is one
/// pointer leaf, so a worker's whole job (a range, an answer, a boxed
/// slice it owns) crosses in one payload with no trampoline -- the
/// multi-field case `threads.md` §5 had put behind one. Two workers, two
/// threads, a function value each, on both backends.
#[test]
fn the_two_backends_agree_on_spawn_struct_ref() {
    assert_backends_agree("backends-spawn-struct-ref", "tests/accept/spawn_struct_ref.cho", "");
}

/// The cancho epic issue's own "lex-os component ported/written in
/// cancho (first production use)": `examples/results_stub/
/// results_stub.cho`, the cancho twin of `lex-os/crates/results-stub` --
/// the single allowed-egress target the lex-os demo's manifest narrows
/// to (issue #10 there). Built and run for real on both backends, hit
/// with a real HTTP request over loopback the way the Rust original's
/// own client would, and checked against the same response shape and
/// request-log line it answers with.
#[test]
fn the_two_backends_answer_the_results_stub_port() {
    use std::io::{Read as _, Write as _};
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    for backend in ["cranelift", "llvm"] {
        let port = free_port();
        let fetched = fetch_net_sockets(
            &format!("results-stub-fetch-{backend}"),
            "examples/results_stub/net.lock",
        );
        let dir = scratch(&format!("backends-results-stub-{backend}"));
        let exe = dir.join("results_stub");
        let build = Command::new(BIN)
            .args([
                "build".as_ref(),
                repo_root().join("examples/results_stub/results_stub.cho").as_os_str(),
                fetched.as_os_str(),
                "--std".as_ref(),
                "--backend".as_ref(),
                backend.as_ref(),
                "-o".as_ref(),
                exe.as_os_str(),
            ])
            .output()
            .expect("the compiler runs");
        assert!(
            build.status.success(),
            "`--backend {backend}` should build `results_stub.cho`, but said:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );

        let mut child = Command::new(&exe)
            .arg("--listen")
            .arg(format!("127.0.0.1:{port}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the stub runs");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(s) => break s,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    let _ = child.kill();
                    panic!(
                        "`--backend {backend}`: could not connect to the stub within the \
                         deadline: {e}"
                    )
                }
            }
        };

        let body = b"{\"hello\":\"world\"}";
        let request = format!(
            "POST /report HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: \
             close\r\n\r\n",
            body.len()
        );
        stream.write_all(request.as_bytes()).expect("the request writes");
        stream.write_all(body).expect("the body writes");

        let mut response = Vec::new();
        stream.read_to_end(&mut response).expect("the response reads");
        let response = String::from_utf8_lossy(&response);

        assert!(
            response.starts_with("HTTP/1.1 200 OK\r\n"),
            "`--backend {backend}`: expected a 200, got:\n{response}"
        );
        assert!(
            response.contains("Content-Type: application/json"),
            "`--backend {backend}`: expected a JSON content type, got:\n{response}"
        );
        assert!(
            response.ends_with("{\"ok\":true,\"stub\":true}"),
            "`--backend {backend}`: expected the stub's own fixed body, got:\n{response}"
        );

        // Killed rather than waited on: the server never exits on its
        // own, the same as the real Rust binary it ports.
        child.kill().expect("the stub can be killed");
        let output = child.wait_with_output().expect("the stub's output reads after being killed");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("results-stub: listening"),
            "`--backend {backend}`: expected a startup line, got:\n{stdout}"
        );
        assert!(
            stdout.contains("req=\"POST /report HTTP/1.1\""),
            "`--backend {backend}`: expected the request line logged, got:\n{stdout}"
        );
        assert!(
            stdout.contains("body_len=17"),
            "`--backend {backend}`: expected the real body length logged, got:\n{stdout}"
        );
        assert!(
            stdout.contains("body_preview=\"{\"hello\":\"world\"}\""),
            "`--backend {backend}`: expected the body preview logged, got:\n{stdout}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
