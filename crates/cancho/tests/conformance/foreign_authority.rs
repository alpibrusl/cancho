//! `docs/foreign-authority.md`: what a program that calls foreign code can
//! reach, said symbol by symbol. Real programs, built and run on **both
//! backends**, and the authority report read as text and as JSON.

use super::*;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// One libc symbol, called through one helper.
const ONE_SYMBOL: &str = "\
extern fn getpid[&f](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] c_int;

fn alive[&f](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] bool {
    return getpid(ffi) > 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(fs); release(heap); release(args);
    let libc = narrow(ffi, \"libc\");
    var status = 1;
    borrow libc as &f in {
        if alive(f) { status = 0; }
    }
    release(libc);
    return status;
}
";

/// Two libraries held by one capability: `labs` is libc's, `pthread_self` is
/// declared under `libpthread` (which glibc folded into libc in 2.34, so the
/// scope is the *claim* and the symbol is the fact).
const TWO_LIBRARIES: &str = "\
edition 5;
extern fn labs[&f](ffi: &f Ffi(\"libc\"), n: int) -> [ffi(\"libc\")] int;
extern fn pthread_self[&f](ffi: &f Ffi(\"libpthread\")) -> [ffi(\"libpthread\")] int;

fn magnitude[&f](ffi: &f Ffi(\"libc,libpthread\"), n: int) -> [ffi(\"libc\")] int {
    return labs(ffi, n);
}

fn thread[&f](ffi: &f Ffi(\"libc,libpthread\")) -> [ffi(\"libpthread\")] int {
    return pthread_self(ffi);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io); release(fs); release(heap); release(args); release(net); release(clock);
    let native = narrow(ffi, \"libpthread,libc\");
    var status = 0;
    borrow native as &f in {
        status = magnitude(f, 0 - 7);
        if thread(f) == 0 { status = 1; }
    }
    release(native);
    return status;
}
";

/// Declares a symbol that exists nowhere and a second that does, and calls
/// neither: the report says nothing, and the program links and runs.
const DECLARED_NEVER_CALLED: &str = "\
extern fn getpid[&f](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] c_int;
extern fn no_such_symbol_anywhere[&f](ffi: &f Ffi(\"libnowhere\")) -> [ffi(\"libnowhere\")] c_int;

fn unreached[&f](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int {
    return getpid(ffi);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    return 0;
}
";

fn write(dir: &Path, name: &str, source: &str) -> PathBuf {
    let file = dir.join(name);
    std::fs::write(&file, source).expect("a writable fixture");
    file
}

fn authority(file: &Path, json: bool) -> String {
    let mut command = Command::new(BIN);
    command.args(["authority".as_ref(), file.as_os_str(), "--std".as_ref()]);
    if json {
        command.args(["--output", "json"]);
    }
    let out = command.output().expect("the compiler runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("the report is utf-8")
}

/// The exit status of the program built by `backend`, or the refusal.
fn run(file: &Path, backend: &str) -> i32 {
    let out = Command::new(BIN)
        .args(["run".as_ref(), file.as_os_str(), "--std".as_ref(), "--backend".as_ref()])
        .arg(backend)
        .output()
        .expect("the compiler runs");
    out.status.code().unwrap_or_else(|| {
        panic!("killed on `{backend}`: {}", String::from_utf8_lossy(&out.stderr))
    })
}

/// The rule of the first refusal, from `check --output json`.
fn refusal(source: &str, tag: &str) -> (String, String) {
    let dir = scratch(tag);
    let file = write(&dir, "refused.cho", source);
    let out = Command::new(BIN)
        .args(["check".as_ref(), file.as_os_str(), "--std".as_ref()])
        .args(["--output", "json"])
        .output()
        .expect("the compiler runs");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let _ = std::fs::remove_dir_all(&dir);
    // A JSON string up to its closing quote, the escapes left as written.
    let field = |key: &str| -> String {
        let at =
            text.find(&format!("\"{key}\": \"")).unwrap_or_else(|| panic!("no {key}:\n{text}"));
        let mut value = String::new();
        let mut chars = text[at + key.len() + 5..].chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    value.push(chars.next().unwrap_or_default());
                }
                '"' => break,
                other => value.push(other),
            }
        }
        value
    };
    (field("rule"), field("message"))
}

/// A `main` that narrows `scope` and does nothing, around `declarations`.
fn narrowing_to(scope: &str, declarations: &str) -> String {
    format!(
        "{declarations}\nfn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
             release(io); release(fs); release(heap); release(args);\n\
             let held = narrow(ffi, \"{scope}\");\n\
             release(held);\n\
             return 0;\n\
         }}\n"
    )
}

// ---- the report -----------------------------------------------------------

/// No foreign code: the report is what it was, with the one new key empty.
#[test]
fn a_program_with_no_foreign_code_is_bounded_and_unbounded_by_nothing() {
    let cut = repo_root().join("examples/cut/cut.cho");
    let json = authority(&cut, true);
    assert!(json.starts_with("{\n  \"bounded\": true,\n  \"unbounded_by\": [],\n"), "{json}");
    assert!(json.contains("\"foreign_symbols\": [],"), "{json}");
    let text = authority(&cut, false);
    assert!(!text.contains("UNBOUNDED"), "{text}");
    assert!(!text.contains("unbounded by"), "{text}");
    assert!(text.contains("foreign code"), "and it says it never touches foreign code:\n{text}");
}

/// One libc symbol: the report names it, exactly, in both forms.
#[test]
fn one_libc_symbol_is_named_exactly() {
    let dir = scratch("foreign-one");
    let file = write(&dir, "one.cho", ONE_SYMBOL);

    assert_eq!(
        authority(&file, true),
        "{\n\
         \x20 \"bounded\": false,\n\
         \x20 \"unbounded_by\": [\n\
         \x20   \"libc:getpid\"\n\
         \x20 ],\n\
         \x20 \"effects\": [\"ffi\"],\n\
         \x20 \"labels\": [\n\
         \x20   { \"name\": \"ffi\", \"argument\": \"libc\", \"bounded\": false }\n\
         \x20 ],\n\
         \x20 \"foreign_symbols\": [\"getpid\"],\n\
         \x20 \"pure\": [],\n\
         \x20 \"folded_operators\": 0,\n\
         \x20 \"folded_calls\": 0,\n\
         \x20 \"functions\": 2\n\
         }\n"
    );
    assert_eq!(
        authority(&file, false),
        "UNBOUNDED: this program calls foreign code. The labels below bound it\n\
         everywhere except through the symbols under \"unbounded by\", and what\n\
         a symbol does is the linked library's, not the language's.\n\
         See docs/foreign-authority.md.\n\
         performs\n\
         \x20   ffi(\"libc\")    <- unbounded\n\
         never touches\n\
         \x20   the console\n\
         \x20   the filesystem\n\
         \x20   the network\n\
         \x20   the heap\n\
         \x20   the command line\n\
         \x20   signals\n\
         \x20   other programs\n\
         unbounded by\n\
         \x20   libc:getpid\n"
    );
    for backend in BACKENDS {
        assert_eq!(run(&file, backend), 0, "`getpid` is positive on {backend}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two libraries in one program, held by one capability: each symbol is
/// attributed to the scope its declaration claims, sorted.
#[test]
fn two_libraries_are_attributed_symbol_by_symbol() {
    let dir = scratch("foreign-two");
    let file = write(&dir, "two.cho", TWO_LIBRARIES);

    let json = authority(&file, true);
    assert!(
        json.starts_with(
            "{\n  \"bounded\": false,\n  \"unbounded_by\": [\n    \"libc:labs\",\n    \"libpthread:pthread_self\"\n  ],\n"
        ),
        "{json}"
    );
    assert!(
        json.contains("{ \"name\": \"ffi\", \"argument\": \"libc\", \"bounded\": false },"),
        "{json}"
    );
    assert!(
        json.contains("{ \"name\": \"ffi\", \"argument\": \"libpthread\", \"bounded\": false }\n"),
        "{json}"
    );
    assert!(json.contains("\"foreign_symbols\": [\"labs\", \"pthread_self\"],"), "{json}");
    let text = authority(&file, false);
    assert!(text.contains("unbounded by\n    libc:labs\n    libpthread:pthread_self\n"), "{text}");

    for backend in BACKENDS {
        assert_eq!(run(&file, backend), 7, "{backend}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A declaration alone is not reach. Decided and documented
/// (`docs/foreign-authority.md` section 6): the symbol is reported when a
/// reachable call site names it, the same standard the labels are held to
/// (`docs/authority.md` section 3), and the program does not even need the
/// library to exist.
#[test]
fn a_declaration_nobody_calls_is_not_reach() {
    let dir = scratch("foreign-declared");
    let file = write(&dir, "declared.cho", DECLARED_NEVER_CALLED);

    let json = authority(&file, true);
    assert!(json.starts_with("{\n  \"bounded\": true,\n  \"unbounded_by\": [],\n"), "{json}");
    assert!(json.contains("\"foreign_symbols\": [],"), "{json}");
    for backend in BACKENDS {
        // Linked and run: an unused import is not an undefined reference.
        assert_eq!(run(&file, backend), 0, "{backend}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A CI pin of the report shows a diff when a symbol is added, and only
/// that: it is what `cancho-hooks` will commit.
#[test]
fn adding_a_symbol_changes_the_pin_by_exactly_that_symbol() {
    let dir = scratch("foreign-pin");
    let before = write(&dir, "before.cho", ONE_SYMBOL);
    let after = write(
        &dir,
        "after.cho",
        &ONE_SYMBOL
            .replace(
                "fn alive",
                "extern fn getuid[&f](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] c_int;\n\nfn alive",
            )
            .replace("return getpid(ffi) > 0;", "return getpid(ffi) > 0 && getuid(ffi) >= 0;"),
    );
    let (a, b) = (authority(&before, true), authority(&after, true));
    // The pin is the `unbounded_by` array, one pair a line: adding a symbol
    // adds exactly one line to it (and the legacy flat list changes too).
    let pins = |report: &str| -> Vec<String> {
        report
            .lines()
            .skip_while(|l| !l.starts_with("  \"unbounded_by\""))
            .take_while(|l| !l.starts_with("  \"effects\""))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(pins(&a), ["  \"unbounded_by\": [", "    \"libc:getpid\"", "  ],"], "{a}");
    assert_eq!(
        pins(&b),
        ["  \"unbounded_by\": [", "    \"libc:getpid\",", "    \"libc:getuid\"", "  ],"],
        "{b}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The scope is a claim and the symbol is the fact: a libc symbol declared
/// under another library's scope is reported under that scope, and it is
/// the symbol a supervisor must read. Pinned so a future check against the
/// linker turns it red (`docs/foreign-authority.md` section 3).
#[test]
fn the_scope_is_a_claim_and_the_symbol_is_the_fact() {
    let dir = scratch("foreign-claim");
    let file = write(
        &dir,
        "claim.cho",
        "extern fn system[&f, &c](ffi: &f Ffi(\"openssl\"), command: &c [byte]) -> [ffi(\"openssl\")] c_int;\n\
         fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args } = split(world);\n\
             release(io); release(fs); release(heap); release(args);\n\
             let ssl = narrow(ffi, \"openssl\");\n\
             var status = 1;\n\
             borrow ssl as &f in { status = system(f, \"exit 0\\0\"); }\n\
             release(ssl);\n\
             return status;\n\
         }\n",
    );
    let json = authority(&file, true);
    assert!(json.contains("\"unbounded_by\": [\n    \"openssl:system\"\n  ],"), "{json}");
    for backend in BACKENDS {
        assert_eq!(run(&file, backend), 0, "{backend}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the hole this slice closed -------------------------------------------

/// Measured before the fix: `system` declared with no capability and the row
/// `[]` ran a shell while the report said `bounded: true`, `performs
/// nothing` and *never touches foreign code*. A foreign function is reached
/// through exactly one `Ffi`.
#[test]
fn a_foreign_function_without_a_capability_is_refused() {
    let main = "fn main(world: World) -> [] int {\n\
                    let Split { io, ffi, fs, heap, args } = split(world);\n\
                    release(io); release(ffi); release(fs); release(heap); release(args);\n\
                    return 0;\n\
                }\n";
    let cases = [
        ("extern fn system[&c](command: &c [byte]) -> [] c_int;\n", "borrows 0 `Ffi` capabilities"),
        ("extern fn getpid() -> [] c_int;\n", "borrows 0 `Ffi` capabilities"),
        (
            "extern fn tick[&i](io: &!i Io) -> [err_write, io_read, io_write] c_int;\n",
            "borrows 0 `Ffi` capabilities",
        ),
        (
            "extern fn f[&a, &b](x: &a Ffi(\"libc\"), y: &b Ffi(\"libm\")) -> [ffi(\"libc\"), ffi(\"libm\")] int;\n",
            "borrows 2 `Ffi` capabilities",
        ),
        (
            "extern fn f[&a](x: &a Ffi(\"libc,libm\")) -> [ffi(\"libc,libm\")] int;\n",
            "names several libraries",
        ),
    ];
    for (declaration, why) in cases {
        let (rule, message) = refusal(&format!("{declaration}{main}"), "foreign-no-capability");
        assert_eq!(rule, "foreign-declaration", "{declaration}");
        assert!(message.contains(why), "{declaration}\n{message}");
    }
}

// ---- narrowing refusals, each with its rule --------------------------------

#[test]
fn a_scope_is_narrowed_as_a_set_and_every_refusal_has_its_rule() {
    let cases: &[(&str, &str, &str)] = &[
        // malformed
        ("libc,", "foreign-scope", "empty library name"),
        ("libc,,libm", "foreign-scope", "empty library name"),
        ("libc,libc", "foreign-scope", "named twice"),
        ("libc libm", "foreign-scope", "not a library name"),
        ("libc:statx", "foreign-scope", "not a library name"),
        ("lib/c", "foreign-scope", "not a library name"),
        // the empty scope is the root
        ("", "capability-not-narrowable", "to itself"),
    ];
    for (scope, rule, why) in cases {
        let (found, message) = refusal(&narrowing_to(scope, ""), "foreign-narrow");
        assert_eq!((found.as_str(), message.contains(why)), (*rule, true), "`{scope}`: {message}");
    }

    // widening, in a program that holds a set and asks for more, for a
    // prefix of what it holds, and for the root.
    let held = |first: &str, then: &str| {
        format!(
            "fn main(world: World) -> [] int {{\n\
                 let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
                 release(io); release(fs); release(heap); release(args);\n\
                 let one = narrow(ffi, \"{first}\");\n\
                 let two = narrow(one, \"{then}\");\n\
                 release(two);\n\
                 return 0;\n\
             }}\n"
        )
    };
    for (first, then, why) in [
        ("libc", "libc,libm", "never widened"),
        ("libc,libm", "libssl", "never widened"),
        // Text prefixes: the old rule let each of these through.
        ("libc", "libcrypto", "never widened"),
        ("libcrypto", "libc", "never widened"),
        ("libc", "", "never widened"),
        ("libc,libm", "libm,libc", "to itself"),
    ] {
        let (rule, message) = refusal(&held(first, then), "foreign-widen");
        assert_eq!(rule, "capability-not-narrowable", "{first} -> {then}: {message}");
        assert!(message.contains(why), "{first} -> {then}: {message}");
    }

    // a scope written in a type is checked where it is written
    let (rule, _) = refusal(
        &narrowing_to("libc", "fn f[&a](x: &a Ffi(\"libc,,libm\")) -> [] int { return 0; }\n"),
        "foreign-type",
    );
    assert_eq!(rule, "foreign-scope");
}

/// Lending a capability narrower is accepted; lending what it lacks, or the
/// unnarrowed root, is a type error.
#[test]
fn a_capability_is_lent_narrower_and_never_wider() {
    let program = |lend: &str, wants: &str| {
        format!(
            "extern fn labs[&f](ffi: &f Ffi(\"libc\"), n: int) -> [ffi(\"libc\")] int;\n\
             fn helper[&f](ffi: &f Ffi(\"{wants}\")) -> [] int {{ return 0; }}\n\
             fn main(world: World) -> [] int {{\n\
                 let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
                 release(io); release(fs); release(heap); release(args);\n\
                 {lend}\n\
                 return 0;\n\
             }}\n"
        )
    };
    let narrowed = |held: &str| {
        format!("let c = narrow(ffi, \"{held}\"); borrow c as &f in {{ helper(f); }} release(c);")
    };
    let dir = scratch("foreign-lend");
    for (held, wants, accepted) in [
        ("libc,libm", "libc", true),
        // Written in another order, it is the same set and the same type.
        ("libm,libc", "libc,libm", true),
        ("libc", "libm", false),
        ("libc", "libc,libm", false),
        ("libc,libm", "libssl", false),
        ("libc,libm,libssl", "libm,libssl", true),
    ] {
        let file = write(&dir, "lend.cho", &program(&narrowed(held), wants));
        let out = Command::new(BIN)
            .args(["check".as_ref(), file.as_os_str(), "--std".as_ref()])
            .output()
            .expect("the compiler runs");
        assert_eq!(out.status.success(), accepted, "{held} lent to {wants}");
    }

    // The root is lent only after it is narrowed.
    let root = program("borrow ffi as &f in { helper(f); }", "libc");
    let file = write(&dir, "root.cho", &root);
    let out = Command::new(BIN)
        .args([
            "check".as_ref(),
            file.as_os_str(),
            "--std".as_ref(),
            "--output".as_ref(),
            "json".as_ref(),
        ])
        .output()
        .expect("the compiler runs");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"rule\": \"type-mismatch\""));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Owning a set discharges each of its libraries, and a row that names a
/// library the function does not call is still refused.
#[test]
fn rows_stay_exact_per_library() {
    let dir = scratch("foreign-rows");
    // `magnitude` declares a library it does not call.
    let wrong = TWO_LIBRARIES.replace(
        "-> [ffi(\"libc\")] int {\n    return labs",
        "-> [ffi(\"libc\"), ffi(\"libpthread\")] int {\n    return labs",
    );
    assert_ne!(wrong, TWO_LIBRARIES);
    let (rule, message) = refusal(&wrong, "foreign-rows");
    assert_eq!(rule, "effect-declared-not-performed", "{message}");
    // `thread` calls into libpthread and declares libc.
    let swapped = TWO_LIBRARIES.replace(
        "-> [ffi(\"libpthread\")] int {\n    return pthread_self",
        "-> [ffi(\"libc\")] int {\n    return pthread_self",
    );
    let (rule, message) = refusal(&swapped, "foreign-rows");
    assert_eq!(rule, "effect-not-declared", "{message}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- follow-ups, pinned (docs/foreign-authority.md section 8) --------------

/// A slice crosses as a pointer **and** a length, so `statx(AT_FDCWD, path,
/// flags, mask, buf)` cannot be declared as C has it: the length lands in
/// `flags`, `flags` in `mask`, `mask` in `buf`. Measured with `strace` as
/// `statx(AT_FDCWD, "/etc/passwd", AT_STATX_SYNC_AS_STAT|0xc, 0, 0x4) =
/// -1 EINVAL`. When a bare-pointer parameter exists this turns red.
///
/// Linux only: `statx` is a Linux call and macOS's libc has no such symbol
/// (the program does not build there), so the reproducer cannot run on it.
#[cfg(target_os = "linux")]
#[test]
fn a_path_cannot_be_passed_to_statx_as_c_has_it() {
    let dir = scratch("foreign-statx");
    let file = write(
        &dir,
        "statx.cho",
        "extern fn statx[&f, &p, &b](ffi: &f Ffi(\"libc\"), dirfd: int, path: &p [byte], flags: int, mask: int, buf: &b [byte]) -> [ffi(\"libc\")] c_int;\n\
         fn mode[&f](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int {\n\
             region a {\n\
                 let buf = alloc_slice[a](256, byte_of(0));\n\
                 return statx(ffi, 0 - 100, \"/etc/passwd\\0\", 0, 4, buf);\n\
             }\n\
         }\n\
         fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args } = split(world);\n\
             release(io); release(fs); release(heap); release(args);\n\
             let libc = narrow(ffi, \"libc\");\n\
             var r = 0;\n\
             borrow libc as &f in { r = mode(f); }\n\
             release(libc);\n\
             if r == 0 { return 0; }\n\
             return 1;\n\
         }\n",
    );
    for backend in BACKENDS {
        assert_eq!(run(&file, backend), 1, "the call fails with EINVAL on {backend}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A string literal has six escapes and no `\x`, so a byte such as `0xFF`
/// cannot be written in one.
#[test]
fn a_string_literal_has_no_hex_escape() {
    let (rule, message) = refusal(
        "fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args } = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args);\n\
             let text = \"a\\x41\";\n\
             return len(text);\n\
         }\n",
        "foreign-hex",
    );
    assert_eq!(rule, "unknown-escape", "{message}");
}
