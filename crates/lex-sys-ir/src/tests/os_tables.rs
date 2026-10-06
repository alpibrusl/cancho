//! The constants a backend writes into a program for the file and directory
//! builtins, per operating system (`docs/wasm.md`).
//!
//! WASI's are read off wasi-libc's own headers (`__header_fcntl.h`,
//! `__struct_stat.h`, `__struct_dirent.h`, `__header_dirent.h`,
//! `__errno_values.h`) -- not guessed -- and two of them contradict what this
//! file used to say about every target: `O_RDONLY` is not zero, and `d_type`'s
//! values are not shared.

use crate::{
    DirentTypes, Os, dirent_layout_for, dirent_types, enametoolong_for, open_flags, open_flags_for,
    stat_layout, stat_layout_for,
};

#[test]
fn wasi_open_flags_are_wasi_libcs() {
    let f = open_flags_for(Os::Wasi, false);
    assert_eq!(
        f.read_only, 0x0400_0000,
        "O_RDONLY is a real flag on WASI; zero asks for no rights"
    );
    assert_eq!(f.write_only, 0x1000_0000);
    assert_eq!((f.create, f.directory, f.exclusive, f.truncate), (0x1000, 0x2000, 0x4000, 0x8000));
    assert_eq!((f.append, f.nofollow), (0x1, 0x0100_0000));
    assert_eq!(f.cloexec, 0, "a WASI module cannot exec, so O_CLOEXEC is 0");
    assert_eq!(f.at_fdcwd, -2);
}

#[test]
fn a_read_only_open_is_a_no_op_off_wasi() {
    for (os, aarch64) in [(Os::Linux, false), (Os::Linux, true), (Os::Darwin, false)] {
        assert_eq!(open_flags_for(os, aarch64).read_only, 0, "{os:?}");
    }
}

#[test]
fn the_bool_helpers_still_answer_the_hosts_they_always_did() {
    // Cranelift calls these; nothing about it moved.
    assert_eq!(open_flags(false, false), open_flags_for(Os::Linux, false));
    assert_eq!(open_flags(true, true), open_flags_for(Os::Darwin, true));
    assert_eq!(stat_layout(false, true), stat_layout_for(Os::Linux, true));
    assert_eq!(stat_layout(true, false), stat_layout_for(Os::Darwin, false));
}

#[test]
fn wasi_stat_has_linux_x86_64s_offsets_and_its_own_nofollow() {
    let wasi = stat_layout_for(Os::Wasi, false);
    let linux = stat_layout_for(Os::Linux, false);
    assert_eq!(
        (wasi.size, wasi.mode, wasi.mode_bits, wasi.st_size, wasi.mtime),
        (linux.size, linux.mode, linux.mode_bits, linux.st_size, linux.mtime)
    );
    assert_eq!(wasi.no_follow, 0x1, "AT_SYMLINK_NOFOLLOW, not Linux's 0x100");
}

#[test]
fn wasi_dirent_and_its_type_numbers() {
    let layout = dirent_layout_for(Os::Wasi);
    assert_eq!(
        (layout.d_type, layout.d_name),
        (8, 9),
        "{{ ino_t d_ino; u8 d_type; char d_name[] }}"
    );
    assert_eq!(
        dirent_types(Os::Wasi),
        DirentTypes { unknown: 0, link: 7, dir: 3, reg: 4 },
        "wasi-libc's DT_LNK, DT_DIR and DT_REG"
    );
    assert_eq!(dirent_types(Os::Linux), dirent_types(Os::Darwin));
    assert_eq!(dirent_types(Os::Linux), DirentTypes { unknown: 0, link: 10, dir: 4, reg: 8 });
    assert_eq!(enametoolong_for(Os::Wasi), 37);
}
