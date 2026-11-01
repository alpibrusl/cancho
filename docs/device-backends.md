# The device verbs' backend walk

> **Status: the walk both backends implement** for `docs/camera.md` §3 and `docs/audio.md` §3. It exists so Cranelift
> (`crates/cancho-codegen/src/body/camera.rs`, `audio.rs`) and LLVM (`crates/cancho-codegen-llvm/src/body/camera.rs`,
> `audio.rs`) cannot disagree, as `cancho_ir::device` does for the numbers. Every number is in `cancho_ir::device`
> (`v4l2` and `alsa` modules), measured with clang on Linux x86_64. Targets other than Linux are **not measured**: on them
> the open answers `Failed(ENODEV)` and no ioctl is emitted.

## 1. The handle is a pointer to a state block

A `Cam`, `Capture` or `Playback` is one 64-bit leaf, like `Port`. Here it is the address of a block the open obtained
from `calloc` and `camera_close`, `mic_close` or `speaker_close` returns with `free`. A `&!Cam` is a pointer to the cell
that holds that address (`handle_fd` in `body/sockets.rs` loads the cell). The block carries what the verbs need and a
64-bit leaf cannot: the descriptor, the negotiated size, and (for the camera) the mapped buffers. Its layout is
`cancho_ir::device::v4l2::STATE_*` for the camera; audio uses cell 0 = descriptor, cell 8 = rate, cell 16 = 1 once
configured. 128 bytes, zeroed.

## 2. `*_open` (`Expr::DeviceOpen`)

`path` is checked against `prefix` with `checked_path` (a path outside the bound traps, `path_escapes`). Then
`openat(AT_FDCWD, path, DEVICE_OPEN_FLAGS)`. Answer three leaves: tag (`Ok` 0, `Failed` 1), the handle, the `errno`
(read before any other call). On `Failed` no block stays allocated (open first, `calloc` only when the descriptor is
good, or `free` it on the failing path) and the handle leaf is the (negative) descriptor.

## 3. `camera_configure(&!Cam, width, height) -> int`

1. `width <= 0` or `height <= 0` -> `DEVICE_EINVAL`, before the device is touched.
2. `v4l2_format` (208 bytes, zeroed): `type = 1`, `pix.width`, `pix.height`, `pix.pixelformat = YUYV`,
   `pix.field = NONE`. `ioctl(fd, VIDIOC_S_FMT, &fmt)`; `< 0` -> that `errno` (`/dev/null` answers `ENOTTY` here).
3. Read `pix.width` and `pix.height` back. If either differs from what was asked, the device does not offer the size:
   `DEVICE_EINVAL` (nothing is streaming yet). Store both in the block.
4. `v4l2_requestbuffers` (20 bytes, zeroed): `count = 4`, `type = 1`, `memory = MMAP`. `VIDIOC_REQBUFS`. `< 0` -> errno.
   The driver may grant fewer: read `count` back; zero granted -> `DEVICE_EINVAL`. Store `min(count, 4)` in cell
   `STATE_COUNT_AT` and use that below, so the cell agrees with what is mapped (both backends do).
5. For each buffer `i` in `0..count` (a counted loop, or four unrolled guarded copies): `v4l2_buffer` (88 bytes,
   zeroed): `index = i`, `type = 1`, `memory = MMAP`; `VIDIOC_QUERYBUF`; read `length` and `m.offset`;
   `mmap(NULL, length, PROT_READ|PROT_WRITE (3), MAP_SHARED (1), fd, offset)`; `MAP_FAILED` (-1) -> errno; store
   address and length in the block; `VIDIOC_QBUF` with the same buffer struct.
6. `VIDIOC_STREAMON` with a 4-byte cell holding `type = 1`. Answer `0`, or the `errno` of the call that failed.
   A failure after some buffers are mapped leaves them mapped: `camera_close` unmaps whatever the block records.

Configuring twice is not supported (the second call after a first success is `DEVICE_EINVAL` when the block already
records a size). The width and height are stored after the read-back of step 3 and before `REQBUFS`, so a failure later
in the walk also makes a second configure `DEVICE_EINVAL`; `camera_close` then issues a harmless `STREAMOFF`.
A width or height above `i32::MAX`, and an audio rate above `u32::MAX`, are `DEVICE_EINVAL` too (the LLVM backend
says so; the interval fields are 32 bits and would otherwise truncate silently).

## 4. `camera_next(&!Cam, into: &![byte]) -> int`

`into` arrives as pointer and length. Not configured (block width is zero) -> `-1`. Otherwise:

1. Keep `have = -1`. Loop: `v4l2_buffer` zeroed with `type = 1`, `memory = MMAP`; `VIDIOC_DQBUF`. If it
   fails (the descriptor is nonblocking, so `EAGAIN` means no frame is ready) leave the loop. If it succeeded and
   `have >= 0` already, give the older buffer back with `VIDIOC_QBUF` (index `have`); then `have = index`. Repeat. (`bytesused` is not used: the
   length is `width * height`. A driver that returns an index of 4 or more is not guarded.) The newest frame wins and the older are dropped (`docs/camera.md` §3).
2. `have < 0`: nothing was ready. Answer `0` if the failing `errno` was `EAGAIN` (11), `-1` otherwise.
3. Convert: YUYV is `Y0 U Y1 V` per two pixels, so the grey byte of pixel `k` is byte `2k` of the buffer. Write
   `n = min(len(into), width * height)` grey bytes: `into[k] = mapped[have][2k]`. The loop is counted, `n` iterations.
4. Return the buffer to the driver (`VIDIOC_QBUF`, index `have`). Answer `n`.

## 5. `camera_close(Cam) -> int`

The argument is the handle value (the block address), not a reference. If the block records a size:
`VIDIOC_STREAMOFF` (ignore its result); for each recorded buffer `munmap(addr, len)`. Then `close(fd)`, `free(block)`,
and answer `close`'s result widened to 64 bits.

## 6. Audio: `mic_configure` and `speaker_configure`

Both open the PCM node stereo, 16-bit signed little-endian, interleaved read/write, at the rate asked
(`docs/audio.md` §3; `alsa::*`).

1. `rate <= 0` -> `DEVICE_EINVAL`, before the device is touched.
2. `snd_pcm_hw_params` (608 bytes) is built as libasound's `any` would: zero the struct; set `rmask` (offset 512) to
   `0xffffffff`; every mask (3 of 32 bytes at offset 4) all ones; every interval (12 of 12 bytes at offset 260, the
   first at parameter index 8) `min = 0`, `max = 0xffffffff`, flags 0. Then restrict: the ACCESS mask (index 0) to the
   single bit `ACCESS_RW_INTERLEAVED`; the FORMAT mask (index 1) to the single bit `FORMAT_S16_LE`; the SUBFORMAT mask
   (index 2) to the single bit `SUBFORMAT_STD`; the CHANNELS interval (index 10, i.e. interval number 2) to
   `min = max = 2` and the RATE interval (index 11, interval number 3) to `min = max = rate`. A mask is eight 32-bit
   words; bit `b` is word `b / 32`, bit `b % 32`.
3. `ioctl(fd, SNDRV_PCM_IOCTL_HW_PARAMS, &params)`. The kernel refines and chooses period and buffer sizes itself.
   `< 0` -> the `errno` (`/dev/null` answers `ENOTTY`; a rate the device does not offer answers `EINVAL` here, the
   kernel's, which is the same number as `DEVICE_EINVAL`). Then `ioctl(fd, SNDRV_PCM_IOCTL_PREPARE, 0)`; `< 0` ->
   `errno`. Do **not** call `SNDRV_PCM_IOCTL_SW_PARAMS`: the kernel's defaults at `HW_PARAMS` start a playback on the
   first write and a capture on the first read. Record the rate and set the configured cell. Answer `0`.

## 7. `mic_read(&!Capture, into: &![byte]) -> int`

Not configured -> `-1`. `frames = min(len(into) / 2, CHUNK_FRAMES)`; `0` frames -> answer `0`. Read into a 4 KiB stack
buffer: `snd_xferi { result, buf, frames }`, `ioctl(fd, SNDRV_PCM_IOCTL_READI_FRAMES, &xfer)`.
`ioctl` failing with `EAGAIN` -> answer `0`; failing with `EPIPE` (an overrun) -> `SNDRV_PCM_IOCTL_PREPARE`, answer `0`;
any other failure -> `-1`. On success read `result` (frames actually moved); for each frame copy the **left** sample (the
first two bytes of the four) to `into` as one mono 16-bit sample. Answer `2 * result`.

## 8. `speaker_write(&!Playback, bytes: &[byte]) -> int`

Not configured -> `-1`. `frames = min(len(bytes) / 2, CHUNK_FRAMES)`; `0` frames -> answer `0`. Expand each mono sample
to two identical 16-bit samples in a 4 KiB stack buffer, `WRITEI_FRAMES` as above. `EAGAIN` -> `0`; `EPIPE` (an
underrun) -> `PREPARE`, answer `0`; any other failure -> `-1`. Answer `2 * result`.

## 9. `mic_close(Capture)`, `speaker_close(Playback)`

Take the handle value (the block address). `ioctl(fd, SNDRV_PCM_IOCTL_DROP, 0)` if configured (ignore the result),
`close(fd)`, `free(block)`, answer `close`'s result widened to 64 bits.

## 10. Not on Linux x86_64

`Os` is not Linux, or the target is not x86_64 (`self.is_darwin()` or the aarch64 check the backend already has):
`DeviceOpen` answers tag 1, handle -1, `DEVICE_ENODEV` and emits no ioctl. The other verbs are then unreachable at
run time (no handle exists) but must still be emitted, so they may emit the same code or a constant `-1`.

## 11. What a program must know

* The capability's bound is **directory-shaped**: `narrow(camera, "/dev/video")` does not admit `/dev/video0` (the
  path check treats it as outside and traps). Narrow to `/dev` or `/dev/`.
* A camera frame of 76,800 bytes does not fit a region (one 65,536-byte chunk): the buffer for it comes from the heap
  with `box_slice`.
* Only the open carries a prefix on its label; `camera_next`, `mic_read` and `speaker_write` are plain labels and must
  be in the row of any function that calls them.
* The LLVM backend calls libc `memset` and declares `mmap` and `munmap` itself; `llvm.memset` is declared by
  `region_cache.rs` and a second declaration is an invalid redefinition.

## 12. What the conformance suite holds both backends to

`crates/cancho/tests/conformance/devices.rs`: a missing node is `Failed`; `/dev/null` opens and the first configure
answers `ENOTTY`; a non-positive size or rate is `EINVAL` before the device is touched (so `/dev/null` does **not**
answer `ENOTTY`); a path outside the bound traps; and, with `CANCHO_DEVICE_HW=1` and `/dev/video0` present, a 320x240
camera delivers 76,800 bytes.
