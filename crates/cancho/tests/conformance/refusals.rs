//! Refusals the Rust compiler makes and the compiler written in cancho does not yet (`docs/gate.md`, G4 wave 2); the rest moved to
//! `tests/commands/refusals.sh`.

use super::*;

/// `open_new` and `file_write` (`docs/file-writes.md`) are names an edition-4 file may already declare against libc, so to that file they are
/// not builtins at all. This test came from `file_writes.rs` when that moved to `tests/commands/file_writes.cho`: the compiler written in cancho
/// does not gate builtin names by edition (it accepts this file), so it cannot move until it does (`docs/gate.md`, G4 wave 2).
#[test]
fn an_earlier_edition_does_not_see_the_new_names() {
    let dir = scratch("file-writes-edition");
    let file = dir.join("old.cho");
    std::fs::write(
        &file,
        "edition 4;\n\
         fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args, net } = split(world);\n\
             release(args); release(heap); release(ffi); release(io); release(net);\n\
             let d = narrow(fs, \"/tmp\");\n\
             borrow d as &f in { open_new(f, \"/tmp/x\"); }\n\
             release(d);\n\
             return 0;\n\
         }\n",
    )
    .unwrap();
    let out = Command::new(BIN).args(["check".as_ref(), file.as_os_str()]).output().unwrap();
    assert!(!out.status.success(), "an edition-4 file should not resolve `open_new`");
    let _ = std::fs::remove_dir_all(&dir);
}
