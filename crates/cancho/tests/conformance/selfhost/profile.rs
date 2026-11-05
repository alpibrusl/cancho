//! `cli.cho profile` (`examples/selfhost/cliprofile.cho`, `docs/agent-profile.md`): instruction counts under callgrind, attributed to the functions of the
//! program, and a baseline that says which one got more expensive.

use super::*;

const SPIN: &str = "fn inner(n: int) -> [] int {\n    var total = 0;\n    var i = 0;\n    while i < n {\n        total = total + i % 7;\n        i = i + 1;\n    }\n    return total;\n}\n\nfn outer(n: int) -> [] int {\n    var sum = 0;\n    var k = 0;\n    while k < 200 {\n        sum = sum + inner(n);\n        k = k + 1;\n    }\n    return sum;\n}\n\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(args);\n    release(heap);\n    release(fs);\n    release(ffi);\n    release(io);\n    let r = outer(5000);\n    if r > 0 {\n        return 0;\n    }\n    return 1;\n}\n";

const GROW: &str = "import std.buffer;\n\nfn fill[&h](heap: &!h Heap, n: int) -> [heap] int {\n    var b = buffer.empty(heap, 1);\n    var i = 0;\n    while i < n {\n        b = buffer.push(heap, b, byte_of(i % 200));\n        i = i + 1;\n    }\n    var size = 0;\n    borrow b as &bb in {\n        size = buffer.size(bb);\n    }\n    buffer.drop(heap, b);\n    return size;\n}\n\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(args);\n    release(fs);\n    release(ffi);\n    release(io);\n    var total = 0;\n    borrow mut heap as &!h in {\n        total = fill(h, 100000);\n    }\n    release(heap);\n    if total == 100000 {\n        return 0;\n    }\n    return 1;\n}\n";

fn have_valgrind() -> bool {
    Command::new("valgrind").arg("--version").output().is_ok()
}

/// What `profile` said: its exit status, standard output as JSON (if it is), and standard error.
fn profile(exe: &Path, dir: &Path, arguments: &[&str]) -> (i32, Option<serde_json::Value>, String) {
    let done =
        Command::new(exe).current_dir(dir).arg("profile").args(arguments).output().expect("runs");
    let out = String::from_utf8_lossy(&done.stdout).into_owned();
    (
        done.status.code().unwrap_or(-1),
        serde_json::from_str(&out).ok(),
        String::from_utf8_lossy(&done.stderr).into_owned(),
    )
}

fn rules(answer: &serde_json::Value) -> Vec<String> {
    answer["findings"]
        .as_array()
        .map(|f| f.iter().filter_map(|x| x["rule"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

/// The instructions of a run, in the parts of the answer: they add up to the total.
fn accounted(answer: &serde_json::Value) -> u64 {
    let listed: u64 = answer["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .map(|f| f["self"].as_u64().expect("self"))
        .sum();
    listed
        + answer["other"]["self"].as_u64().unwrap_or(0)
        + answer["unattributed"]["self"].as_u64().unwrap_or(0)
        + answer["external"]["self"].as_u64().unwrap_or(0)
}

#[test]
fn profile_in_cancho_counts_instructions_by_function_and_gates_on_a_baseline() {
    if !have_valgrind() {
        eprintln!("skipped: needs valgrind");
        return;
    }
    let exe = build("cli-profile", &with_front_end("cli.cho"));
    let dir = exe.parent().expect("a scratch directory").join("p");
    std::fs::create_dir_all(&dir).expect("the directory is made");
    // `profile` runs this command line again by the name it was run by, which `Exec` refuses with `..` in it.
    let exe = exe.canonicalize().expect("a path");
    let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).expect("a file");
    write("spin.cho", SPIN);
    write("grow.cho", GROW);

    // The same program counts the same: valgrind's instruction count does not depend on the clock or the load.
    let (code, first, said) = profile(&exe, &dir, &["--unoptimized", "spin.cho"]);
    assert_eq!(code, 0, "{said}");
    let first = first.expect("JSON");
    let (_, second, _) = profile(&exe, &dir, &["--unoptimized", "spin.cho"]);
    assert_eq!(Some(&first), second.as_ref(), "two runs of one program differ");
    assert_eq!(first["metric"], "instructions");
    let total = first["total"].as_u64().expect("total");
    assert_eq!(accounted(&first), total, "the parts do not add up to the total: {first}");
    let top = &first["functions"][0];
    assert_eq!((top["name"].as_str(), top["file"].as_str()), (Some("inner"), Some("spin.cho")));
    assert_eq!(top["line"], 1);
    // 200 times 5000 iterations of a loop of 38 instructions at -O0: most of the run, and none of it in another function.
    assert!(top["self"].as_u64().expect("self") * 100 > total * 80, "{first}");
    // The shipped binary: counts are fewer, the parts still add up, and what the optimiser lost is said and not hidden.
    let (code, optimised, said) = profile(&exe, &dir, &["spin.cho"]);
    assert_eq!(code, 0, "{said}");
    let optimised = optimised.expect("JSON");
    assert_eq!(accounted(&optimised), optimised["total"].as_u64().expect("total"), "{optimised}");
    assert!(optimised["total"].as_u64() < first["total"].as_u64());

    // A baseline: the same run passes; a run that does the work twice is a regression in `inner` and in the total; half the work is an improvement.
    let (code, _, said) =
        profile(&exe, &dir, &["--unoptimized", "--record", "base.json", "spin.cho"]);
    assert_eq!(code, 0, "{said}");
    let recorded = std::fs::read_to_string(dir.join("base.json")).expect("a baseline");
    let parsed: serde_json::Value = serde_json::from_str(&recorded).expect("a baseline is JSON");
    assert_eq!(parsed["functions"]["inner@spin.cho"].as_u64(), top["self"].as_u64());
    let (code, same, _) =
        profile(&exe, &dir, &["--unoptimized", "--baseline", "base.json", "spin.cho"]);
    assert_eq!(code, 0);
    assert_eq!(rules(&same.expect("JSON")), Vec::<String>::new());
    write("spin.cho", &SPIN.replace("k < 200", "k < 400"));
    let (code, slower, _) =
        profile(&exe, &dir, &["--unoptimized", "--baseline", "base.json", "spin.cho"]);
    assert_eq!(code, 1);
    let slower = slower.expect("JSON");
    assert_eq!(rules(&slower), ["profile-regressed", "profile-regressed"], "{slower}");
    assert!(slower["findings"].to_string().contains("inner@spin.cho"), "{slower}");
    write("spin.cho", &SPIN.replace("k < 200", "k < 100"));
    let (code, faster, _) =
        profile(&exe, &dir, &["--unoptimized", "--baseline", "base.json", "spin.cho"]);
    assert_eq!(code, 0);
    assert_eq!(rules(&faster.expect("JSON")), ["profile-improved", "profile-improved"]);
    assert_eq!(
        profile(&exe, &dir, &["--unoptimized", "--strict", "--baseline", "base.json", "spin.cho"])
            .0,
        1
    );
    // A factor that allows it.
    write("spin.cho", &SPIN.replace("k < 200", "k < 400"));
    assert_eq!(
        profile(
            &exe,
            &dir,
            &["--unoptimized", "--factor", "3", "--baseline", "base.json", "spin.cho"]
        )
        .0,
        0
    );
    // A baseline that is not there, and ones that are not baselines: findings, never a crash.
    let (code, missing, _) =
        profile(&exe, &dir, &["--unoptimized", "--baseline", "nope.json", "spin.cho"]);
    assert_eq!(
        (code, rules(&missing.expect("JSON"))),
        (1, vec!["profile-baseline-missing".to_owned()])
    );
    for bad in [
        "not json",
        "{}",
        "",
        "{\n  \"format\": 1\n}",
        "{\n  \"format\": 1,\n  \"metric\": \"seconds\",\n  \"total\": 1\n}",
    ] {
        write("bad.json", bad);
        let (code, answer, _) =
            profile(&exe, &dir, &["--unoptimized", "--baseline", "bad.json", "spin.cho"]);
        assert_eq!(code, 1, "{bad:?}");
        assert_eq!(rules(&answer.expect("JSON")), ["profile-baseline-unreadable"], "{bad:?}");
    }

    // The allocator is counted, and found at the function that asked: `reserve` of `std.buffer` grows the buffer by doubling.
    let (code, grown, said) = profile(&exe, &dir, &["--std", "--unoptimized", "grow.cho"]);
    assert_eq!(code, 0, "{said}");
    let grown = grown.expect("JSON");
    assert_eq!(accounted(&grown), grown["total"].as_u64().expect("total"), "{grown}");
    let calls = grown["allocations"]["calls"].as_u64().expect("calls");
    assert!((10..=40).contains(&calls), "{grown}");
    let by = grown["allocations"]["by"].as_array().expect("by");
    assert!(
        by.iter().any(|f| f["name"] == "reserve" && f["file"] == "<std>/buffer.cho"),
        "{grown}"
    );

    // A program that does not end with 0 has no profile, a refused one is refused as `check` refuses it, and nothing is left beside the program.
    write("fails.cho", &SPIN.replace("if r > 0", "if r < 0"));
    let (code, answer, said) = profile(&exe, &dir, &["fails.cho"]);
    assert_eq!((code, answer.is_none()), (1, true));
    assert!(said.contains("profile-failed"), "{said}");
    write("refused.cho", "fn main(world: World) -> [] int {\n    return undefined_name;\n}\n");
    let (code, _, said) = profile(&exe, &dir, &["refused.cho"]);
    assert_eq!(code, 1, "{said}");
    assert_eq!(profile(&exe, &dir, &[]).0, 2);
    assert_eq!(profile(&exe, &dir, &["--top", "0", "spin.cho"]).0, 2);
    assert_eq!(profile(&exe, &dir, &["--factor", "0.5", "spin.cho"]).0, 2);
    let leftovers: Vec<String> = std::fs::read_dir(&dir)
        .expect("a directory")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|n| n.contains(".cancho-profile"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}
