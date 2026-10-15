//! `docs/tty.md` §3 and §5: the serial-port verbs, on both backends. A real
//! serial line is hardware; what is checked here is what needs none -- the
//! answers a refusal gives (`Failed(errno)`, `EINVAL` before the port is
//! touched, `ENOTTY` from a device that is not a terminal) and, where the
//! host has a pseudo-terminal master, a configuration that applies. The
//! termios numbers themselves are `cancho_ir::tty`'s unit tests and the
//! robot's measurement (`docs/tty.md` §2).

use super::sockets::{BACKENDS, build};
use super::*;

const ENOTTY: i32 = 25;
/// What macOS answers for `tcsetattr` on a device that is not a terminal
/// (`/dev/null`): "Operation not supported by device", where Linux says ENOTTY.
const ENODEV: i32 = 19;
const EINVAL: i32 = 22;
const NO_PORT: i32 = 99;

/// Open `path` under `/dev`, configure it at `speed`, and exit with
/// `tty_configure`'s answer -- or [`NO_PORT`] when the open is refused.
fn configure_program(path: &str, speed: i64) -> String {
    format!(
        "edition 8;\n\
         fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args, net, clock, signals, exec, tty }} = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args);\n\
             release(net); release(clock); release(signals); release(exec);\n\
             let device = narrow(tty, \"/dev\");\n\
             var code = 0;\n\
             borrow device as &d in {{\n\
                 match tty_open(d, \"{path}\") {{\n\
                     TtyOpened::Ok(port) => {{\n\
                         borrow mut port as &!p in {{ code = tty_configure(p, {speed}); }}\n\
                         tty_close(port);\n\
                     }}\n\
                     TtyOpened::Failed(errno) => {{ code = {NO_PORT}; }}\n\
                 }}\n\
             }}\n\
             release(device);\n\
             return code;\n\
         }}\n"
    )
}

fn answers(tag: &str, path: &str, speed: i64) -> Vec<(String, Option<i32>)> {
    BACKENDS
        .iter()
        .map(|backend| {
            let dir = scratch(&format!("tty-{tag}-{backend}"));
            let exe = build(&dir, tag, &configure_program(path, speed), backend);
            let code = Command::new(&exe).status().expect("the program runs").code();
            let _ = std::fs::remove_dir_all(&dir);
            (backend.to_string(), code)
        })
        .collect()
}

fn expect(tag: &str, path: &str, speed: i64, want: i32) {
    for (backend, code) in answers(tag, path, speed) {
        assert_eq!(code, Some(want), "{backend}/{tag}: {path} at {speed}");
    }
}

#[test]
fn a_missing_device_is_a_value_not_a_crash() {
    expect("missing", "/dev/cancho-no-such-port", 115_200, NO_PORT);
}

#[test]
fn a_device_that_is_not_a_terminal_is_refused_by_tcsetattr() {
    let refusal = if cfg!(target_os = "macos") { ENODEV } else { ENOTTY };
    expect("not-a-tty", "/dev/null", 115_200, refusal);
}

#[test]
fn a_speed_with_no_rate_is_refused_before_the_port_is_touched() {
    // zero is `B0` (hang up) and a negative is no rate on either target;
    // neither reaches `tcsetattr`, so /dev/null's `ENOTTY` is not the answer
    expect("zero", "/dev/null", 0, EINVAL);
    expect("negative", "/dev/null", -5, EINVAL);
    if cfg!(target_os = "linux") {
        // Linux has a table of standard rates, and 12345 is not in it
        expect("odd", "/dev/null", 12_345, EINVAL);
    }
}

#[test]
fn a_pseudo_terminal_takes_a_standard_rate() {
    // /dev/ptmx opens a pty master, whose termios `tcsetattr` sets without
    // hardware. Linux only: on macOS the second step is `IOSSIOSPEED`, and
    // whether a pty master takes it is not something this suite has
    // measured -- that path runs on the robot's hardware (`docs/tty.md` §9).
    if !cfg!(target_os = "linux") || !Path::new("/dev/ptmx").exists() {
        return;
    }
    expect("pty-9600", "/dev/ptmx", 9_600, 0);
    expect("pty-115200", "/dev/ptmx", 115_200, 0);
    // the robot's bus: a standard rate on Linux, in `c_cflag` alone
    expect("pty-1m", "/dev/ptmx", 1_000_000, 0);
}

/// A pseudo-terminal held open by a Python process for as long as `Pty` lives,
/// so a program can configure its slave and `stty` can read the result back --
/// the one check that the speed and the raw flags *landed*, not merely that
/// the call answered 0.
struct Pty {
    child: std::process::Child,
    slave: String,
}

impl Pty {
    fn open() -> Option<Pty> {
        use std::io::BufRead as _;
        let mut child = Command::new("python3")
            .args([
                "-I",
                "-c",
                "import os,pty,sys\nm,s=pty.openpty()\nprint(os.ttyname(s),flush=True)\nsys.stdin.read()",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .ok()?;
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
        let slave = line.trim().to_string();
        if slave.starts_with("/dev/") { Some(Pty { child, slave }) } else { None }
    }

    /// The line's speed, read from `stty -a` (`speed 9600 baud;`): GNU `stty` also takes `speed`, but
    /// uutils' (Ubuntu 26.04's `coreutils`) does not.
    fn speed(&self) -> String {
        let all = self.stty(&["-a"]);
        all.split("speed ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or("")
            .to_owned()
    }

    fn stty(&self, args: &[&str]) -> String {
        let flag = if cfg!(target_os = "macos") { "-f" } else { "-F" };
        let out = Command::new("stty").arg(flag).arg(&self.slave).args(args).output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}

#[test]
fn the_speed_and_the_raw_flags_land_on_the_line() {
    // Linux only: macOS's `stty` reads the same fields, but its 1,000,000
    // goes by `IOSSIOSPEED`, which a pty does not take, and this is not a
    // check anyone can run there without the hardware (`docs/tty.md` §9).
    if !cfg!(target_os = "linux") {
        return;
    }
    let Some(pty) = Pty::open() else { return };
    for (speed, wanted) in [(9_600, "9600"), (115_200, "115200"), (1_000_000, "1000000")] {
        for (backend, code) in answers(&format!("line-{speed}"), &pty.slave, speed) {
            assert_eq!(code, Some(0), "{backend}: configure at {speed}");
            assert_eq!(pty.speed(), wanted, "{backend}: the speed at {speed}");
            let all = pty.stty(&["-a"]);
            for flag in ["cs8", "-parenb", "-icanon", "-echo", "min = 0", "time = 0"] {
                assert!(all.contains(flag), "{backend}/{speed}: expected `{flag}` in\n{all}");
            }
        }
    }
}
