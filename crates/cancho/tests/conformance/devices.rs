//! `docs/camera.md` §3, §5 and `docs/audio.md` §3, §5: the camera, microphone and speaker verbs, on both backends.
//! Real capture and playback are hardware and a person's room, so what is checked here without either is what needs
//! none: the answers a refusal gives (`Failed(errno)`, `EINVAL` before the device is touched, `ENOTTY` from a node
//! that is not a device) and the run-time trap for a path outside the bound. The hardware halves run only when
//! `CANCHO_DEVICE_HW=1` is set, because they switch a camera light on and open a microphone: they are the manual
//! check of `docs/audio.md` §7, never a CI one.

use super::sockets::{BACKENDS, build};
use super::*;

const ENOTTY: i32 = 25;
const EINVAL: i32 = 22;
const NO_DEVICE: i32 = 99;
const TRAPPED: Option<i32> = None;

/// One of the three devices, with the words its program is built from.
struct Kind {
    open: &'static str,
    opened: &'static str,
    field: &'static str,
    close: &'static str,
}

const CAMERA: Kind =
    Kind { open: "camera_open", opened: "CameraOpened", field: "camera", close: "camera_close" };
const MIC: Kind = Kind { open: "mic_open", opened: "MicOpened", field: "mic", close: "mic_close" };
const SPEAKER: Kind = Kind {
    open: "speaker_open",
    opened: "SpeakerOpened",
    field: "speaker",
    close: "speaker_close",
};

/// Open `path` under `/dev` and exit with `configure`'s answer -- or [`NO_DEVICE`] when the open is refused.
/// `configure` is the verb's call written out, for example `camera_configure(h, 320, 240)`.
fn configure_program(kind: &Kind, path: &str, configure: &str) -> String {
    let Kind { open, opened, field, close, .. } = kind;
    format!(
        "edition 12;\n\
         fn main(world: World) -> [{open}(\"/dev\")] int {{\n\
             let Split {{ io, ffi, fs, heap, args, out, ports, clock, signals, exec, tty, camera, mic, speaker }} = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args);\n\
             release(out); release(ports); release(clock); release(signals); release(exec); release(tty);\n\
             {others}\n\
             let device = narrow({field}, \"/dev\");\n\
             var code = 0;\n\
             borrow device as &d in {{\n\
                 match {open}(d, \"{path}\") {{\n\
                     {opened}::Ok(handle) => {{\n\
                         borrow mut handle as &!h in {{ code = {configure}; }}\n\
                         {close}(handle);\n\
                     }}\n\
                     {opened}::Failed(errno) => {{ code = {NO_DEVICE}; }}\n\
                 }}\n\
             }}\n\
             release(device);\n\
             return code;\n\
         }}\n",
        others = ["camera", "mic", "speaker"]
            .iter()
            .filter(|f| *f != field)
            .map(|f| format!("release({f});"))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn answers(tag: &str, source: &str) -> Vec<(String, Option<i32>)> {
    BACKENDS
        .iter()
        .map(|backend| {
            let dir = scratch(&format!("dev-{tag}-{backend}"));
            let exe = build(&dir, tag, source, backend);
            let code = Command::new(&exe).status().expect("the program runs").code();
            let _ = std::fs::remove_dir_all(&dir);
            (backend.to_string(), code)
        })
        .collect()
}

fn expect(tag: &str, source: &str, want: Option<i32>) {
    if !cfg!(target_os = "linux") {
        return; // the V4L2 and ALSA numbers are Linux's (`cancho_ir::device`); other targets open nothing
    }
    for (backend, code) in answers(tag, source) {
        assert_eq!(code, want, "{backend}/{tag}");
    }
}

#[test]
fn a_missing_device_is_a_value_not_a_crash() {
    for (kind, configure) in [
        (&CAMERA, "camera_configure(h, 320, 240)"),
        (&MIC, "mic_configure(h, 48000)"),
        (&SPEAKER, "speaker_configure(h, 48000)"),
    ] {
        let tag = format!("missing-{}", kind.field);
        expect(
            &tag,
            &configure_program(kind, "/dev/cancho-no-such-device", configure),
            Some(NO_DEVICE),
        );
    }
}

#[test]
fn a_node_that_is_not_a_device_is_refused_when_configured() {
    // /dev/null opens read-write, so the open is `Ok`; the first ioctl is what answers `ENOTTY`
    for (kind, configure) in [
        (&CAMERA, "camera_configure(h, 320, 240)"),
        (&MIC, "mic_configure(h, 48000)"),
        (&SPEAKER, "speaker_configure(h, 48000)"),
    ] {
        let tag = format!("null-{}", kind.field);
        expect(&tag, &configure_program(kind, "/dev/null", configure), Some(ENOTTY));
    }
}

#[test]
fn a_size_or_rate_with_no_meaning_is_refused_before_the_device_is_touched() {
    // /dev/null would answer `ENOTTY` if the device were touched, so `EINVAL` shows it was not
    for (kind, configure) in [
        (&CAMERA, "camera_configure(h, 0, 240)"),
        (&CAMERA, "camera_configure(h, 320, 0)"),
        (&CAMERA, "camera_configure(h, -320, 240)"),
        (&MIC, "mic_configure(h, 0)"),
        (&MIC, "mic_configure(h, -48000)"),
        (&SPEAKER, "speaker_configure(h, 0)"),
        (&SPEAKER, "speaker_configure(h, -1)"),
    ] {
        let tag = format!("einval-{}-{}", kind.field, configure.len());
        expect(&tag, &configure_program(kind, "/dev/null", configure), Some(EINVAL));
    }
}

#[test]
fn a_path_outside_the_bound_traps() {
    // the capability is narrowed to /dev; /etc/passwd is not under it (`path_escapes`, as `tty_open`'s is)
    for (kind, configure) in [
        (&CAMERA, "camera_configure(h, 320, 240)"),
        (&MIC, "mic_configure(h, 48000)"),
        (&SPEAKER, "speaker_configure(h, 48000)"),
    ] {
        let tag = format!("escape-{}", kind.field);
        expect(&tag, &configure_program(kind, "/etc/passwd", configure), TRAPPED);
    }
}

#[test]
fn reading_a_closed_or_missing_stream_is_not_reached_without_a_handle() {
    // a stream exists only inside the `Ok` arm, so `camera_next` on a closed camera cannot be written: the
    // linear handle is spent by `camera_close`
    let source = configure_program(&CAMERA, "/dev/null", "camera_configure(h, 320, 240)").replace(
        "camera_close(handle);",
        "camera_close(handle); borrow mut handle as &!h in { code = camera_configure(h, 1, 1); }",
    );
    let dir = scratch("dev-spent");
    let file = dir.join("spent.cho");
    std::fs::write(&file, source).unwrap();
    let out = Command::new(BIN)
        .args(["check".as_ref(), file.as_os_str(), "--std".as_ref()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "using a closed camera must be refused at compile time");
    let _ = std::fs::remove_dir_all(&dir);
}

fn hardware() -> bool {
    cfg!(target_os = "linux") && std::env::var("CANCHO_DEVICE_HW").as_deref() == Ok("1")
}

#[test]
fn a_real_camera_gives_frames_of_the_size_it_was_asked_for() {
    if !hardware() || !Path::new("/dev/video0").exists() {
        return;
    }
    // 320x240 is offered by the UVC camera measured in `docs/camera.md` §2; a frame is 76,800 grey bytes
    let source = "edition 12;\n\
        fn main(world: World) -> [camera_open(\"/dev\"), camera_next] int {\n\
            let Split { io, ffi, fs, heap, args, out, ports, clock, signals, exec, tty, camera, mic, speaker } = split(world);\n\
            release(io); release(ffi); release(fs); release(args);\n\
            release(out); release(ports); release(clock); release(signals); release(exec); release(tty);\n\
            release(mic); release(speaker);\n\
            let device = narrow(camera, \"/dev\");\n\
            var code = 1;\n\
            borrow mut heap as &!hp in {\n\
                borrow device as &d in {\n\
                    match camera_open(d, \"/dev/video0\") {\n\
                        CameraOpened::Ok(cam) => {\n\
                            borrow mut cam as &!c in {\n\
                                if camera_configure(c, 320, 240) == 0 {\n\
                                    let frame = box_slice(hp, 76800, byte_of(0));\n\
                                    borrow mut frame as &!fw in {\n\
                                        var tries = 0;\n\
                                        while tries < 20000000 {\n\
                                            let n = camera_next(c, contents(fw));\n\
                                            if n == 76800 { code = 0; tries = 20000000; }\n\
                                            if n < 0 { code = 2; tries = 20000000; }\n\
                                            tries = tries + 1;\n\
                                        }\n\
                                    }\n\
                                    unbox_slice(hp, frame);\n\
                                }\n\
                            }\n\
                            camera_close(cam);\n\
                        }\n\
                        CameraOpened::Failed(errno) => { code = 3; }\n\
                    }\n\
                }\n\
            }\n\
            release(device); release(heap);\n\
            return code;\n\
        }\n"
    .to_string();
    for (backend, code) in answers("camera-frame", &source) {
        assert_eq!(code, Some(0), "{backend}: a 320x240 frame is 76,800 bytes");
    }
}
