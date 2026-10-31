# Camera: a capability for video capture devices

> **Status: design (T1); nothing here is built.** Written 2026-10-29 for issue #647, before any code, as `CONTRIBUTING.md` asks. What was
> measured is in §2 and is the only measurement in the document; every other section is a **[choice]** or an open question. The model is
> `docs/tty.md`, which this design follows on purpose.

## 1. Why

A cancho program cannot take a frame from a camera without a foreign call. A real use exists: cancho-hdl's `camfilter.cho` streams
greyscale frames from standard input through an FPGA image filter and writes the result (4 real frames x 5 modes, 27,648 pixels per mode,
byte for byte equal to an independent implementation); the frames come from `ffmpeg` because there is no other way to reach `/dev/video0`.
For cancho-robot the question is the one `docs/tty.md` §1 answers for serial ports: `cancho authority` should say *which camera* a program can
reach, not eleven libc symbol names and "never touches the filesystem" while it opens a device node.

## 2. What was measured (Linux x86_64, UVC webcam "LGE FHD Camera", `/dev/video0`)

| Fact | Measurement |
|---|---|
| Capabilities (`VIDIOC_QUERYCAP`) | `0x84a00001`: video capture and streaming I/O set; **read/write I/O (`0x01000000`) not set** |
| Formats | raw YUYV 4:2:2 and MJPEG; YUYV at 320x180, 320x240, 424x240, 640x360, 640x480, 960x540, 1280x720, 1920x1080 (`ffmpeg -list_formats`) |
| `sizeof` (clang, `<linux/videodev2.h>`) | `v4l2_capability` 104, `v4l2_format` 208, `v4l2_requestbuffers` 20, `v4l2_buffer` 88 |
| Request numbers | `QUERYCAP 0x80685600`, `S_FMT 0xc0d05605`, `REQBUFS 0xc0145608`, `QUERYBUF 0xc0585609`, `QBUF 0xc058560f`, `DQBUF 0xc0585611`, `STREAMON 0x40045612` |
| `v4l2_buffer` offsets | `bytesused` 8, `m` 64, `length` 72 |

Consequence: capture on this driver is the streaming path, `REQBUFS` + `QUERYBUF` + `mmap` + `QBUF` + `STREAMON` + `DQBUF`. The numbers above
are x86_64 Linux's; the aarch64 numbers (the robot's Raspberry Pi 5) and macOS's (AVFoundation, no V4L2 at all) are **not measured**.
Not measured either: whether a frame in an `mmap`'d buffer can be copied into a cancho slice without a further foreign call. In a program
today that copy is the hard part, and it is what the backend would own.

## 3. The surface **[choice]**

```cho
res Camera           -- the capability, prefix-narrowed: Camera("/dev/video0")
res Cam              -- an open camera, linear: one closer (Port's shape beside Tty)

camera_open(c: &Camera(bound), path: &[byte]) -> CameraOpened   // Ok(Cam) | Failed(errno)
camera_configure(k: &!Cam, width: int, height: int) -> int      // YUYV or grey, a size; 0, or the errno
camera_next(k: &!Cam, into: &![byte]) -> int                    // the newest frame's bytes that fit, never blocks; 0 when none is ready; -1 on error
camera_close(k: Cam) -> int
poller_add_camera(q: &!Poller, k: &Cam, token: int, events: int) -> int
```

One call configures, as `tty_configure` does: the program names a size; the backend picks the pixel format. **[choice]** The backend
delivers 8-bit greyscale (the luma of YUYV), `width * height` bytes, because that is what the first user needs and because a pixel-format
vocabulary is the surface `Tty` exists to avoid. A second call for colour is added when something asks for it. The backend owns the V4L2
numbers, the buffer count and the `mmap`; the program sees none of them. `camera_next` returns the **newest** frame and drops older ones,
because a robot wants the present, not a queue; this is a choice a later user may want to change.

## 4. Authority **[choice]**

`camera_open` reads a path under the capability's bound, narrowed as `Tty` is (prefix-shaped, non-empty); `camera_next` is path-free.

```
performs
    camera_open("/dev/video0")
    camera_next
```

and **no `foreign` lines**, which is the acceptance of §7. A program with a `Camera` and a `Tty` and nothing else can be shown, by its report, to reach one
camera and one serial device.

## 5. Refusals

As in `docs/tty.md` §5: a path outside the bound is a located compile-time refusal (a new rule tag, `camera-path`); a device that is not a
capture device, or will not open, is `Failed(errno)`; a size the device does not offer is `EINVAL` before streaming starts; read on a closed
camera is `-1`.

## 6. Edition and slices **[choice]**

`Split` grows a twelfth field, `camera`, and that opens **edition 12**. *(Corrected before any code: the first version of this section put the
field behind `Tty`'s edition 8 as "an additive field". It is not additive to a program: edition 8's `Split` has exactly ten fields, every
edition-8 file destructures all ten, and `docs/editions.md` records the same reasoning for editions 7 and 11 (an older file's pattern is
`arity-mismatch` against a longer `Split`). Edition 11, the latest, has eleven fields; edition 12 is edition 11 plus `camera`, and an older
file's `split()` still answers the fields it did.)* If a microphone and speaker capability is to join the same `Split`, it should share this
edition rather than open a thirteenth. Slices, each with its acceptance written before it is built: T1 this document; T2 the verbs and the
refusals on both backends, with the aarch64 numbers measured on the robot; T3 `poller_add_camera`; T4 the example (`examples/camera_hello.cho`:
open, configure 320x240, take 10 frames, print their mean grey level) and the report check; T5 cancho-hdl's `camfilter` drops `ffmpeg`.

## 7. Acceptance of the whole

A cancho program with no `extern fn` captures 100 frames from `/dev/video0`, `cancho authority` reports `camera_open("/dev/video0")` and no
foreign line, and cancho-hdl's camera pipeline runs without `ffmpeg`.

## 8. Not decided, not claimed

macOS (AVFoundation is not V4L2: a second backend, not a variation); colour; exposure and focus controls (the same "one setting" argument as
`tty_configure`, or a second call); several cameras at once; MJPEG (needs a decoder, which is not a capability question); whether `Camera`
belongs in the standard library or beside it. Nothing here claims any of them.
