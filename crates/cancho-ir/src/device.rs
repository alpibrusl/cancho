//! The V4L2 and ALSA numbers, in one place so the two backends cannot disagree (`docs/camera.md` §2 and
//! `docs/audio.md` §2, every value measured on Linux x86_64 with clang against `<linux/videodev2.h>` and
//! `<sound/asound.h>`).
//!
//! A program writing `extern fn` carries these in source; under `Camera`, `Mic` and `Speaker` they are the backend's,
//! written once in Rust. They are the x86_64 Linux numbers. The aarch64 Linux numbers (the robot's Raspberry Pi 5)
//! and macOS's (AVFoundation and CoreAudio are not V4L2 or ALSA at all) are **not measured**, so the backends open
//! nothing on those targets: `camera_open`, `mic_open` and `speaker_open` answer `Failed(ENODEV)`.

/// `ENODEV`: what an open answers on a target whose numbers are not measured.
pub const DEVICE_ENODEV: i64 = 19;
/// `EINVAL`: a size or a rate the device does not offer, answered before streaming starts.
pub const DEVICE_EINVAL: i64 = 22;
/// `EAGAIN`: nothing ready on a nonblocking descriptor.
pub const DEVICE_EAGAIN: i64 = 11;
/// `EPIPE`: an ALSA overrun or underrun.
pub const DEVICE_EPIPE: i64 = 32;

/// `O_RDWR | O_NONBLOCK` (Linux): how a camera, a capture stream and a playback stream are opened. The nonblocking
/// bit is why the verbs never wait and the poller is the waiting story.
pub const DEVICE_OPEN_FLAGS: i64 = 2 | 0o4000;

/// The V4L2 half (`docs/camera.md` §2).
pub mod v4l2 {
    pub const VIDIOC_S_FMT: i64 = 0xc0d0_5605;
    pub const VIDIOC_REQBUFS: i64 = 0xc014_5608;
    pub const VIDIOC_QUERYBUF: i64 = 0xc058_5609;
    pub const VIDIOC_QBUF: i64 = 0xc058_560f;
    pub const VIDIOC_DQBUF: i64 = 0xc058_5611;
    pub const VIDIOC_STREAMON: i64 = 0x4004_5612;
    pub const VIDIOC_STREAMOFF: i64 = 0x4004_5613;
    /// `V4L2_BUF_TYPE_VIDEO_CAPTURE`.
    pub const BUF_TYPE_CAPTURE: i64 = 1;
    /// `V4L2_MEMORY_MMAP`.
    pub const MEMORY_MMAP: i64 = 1;
    /// `V4L2_FIELD_NONE`.
    pub const FIELD_NONE: i64 = 1;
    /// `V4L2_PIX_FMT_YUYV`, the four characters `YUYV` little-endian.
    pub const PIX_FMT_YUYV: i64 = 0x5659_5559;
    /// The number of buffers requested, and the most the state block holds.
    pub const BUFFERS: usize = 4;
    /// `sizeof(struct v4l2_format)`, `v4l2_requestbuffers` and `v4l2_buffer`.
    pub const FORMAT_SIZE: usize = 208;
    pub const REQBUFS_SIZE: usize = 20;
    pub const BUFFER_SIZE: usize = 88;
    /// `struct v4l2_format` offsets: `type`, then `fmt.pix` `width`, `height`, `pixelformat`, `field`, `sizeimage`.
    pub const FORMAT_TYPE_AT: i32 = 0;
    pub const FORMAT_WIDTH_AT: i32 = 8;
    pub const FORMAT_HEIGHT_AT: i32 = 12;
    pub const FORMAT_PIXELFORMAT_AT: i32 = 16;
    pub const FORMAT_FIELD_AT: i32 = 20;
    /// `struct v4l2_requestbuffers` offsets.
    pub const REQBUFS_COUNT_AT: i32 = 0;
    pub const REQBUFS_TYPE_AT: i32 = 4;
    pub const REQBUFS_MEMORY_AT: i32 = 8;
    /// `struct v4l2_buffer` offsets: `index`, `type`, `bytesused`, `memory`, `m.offset`, `length`.
    pub const BUFFER_INDEX_AT: i32 = 0;
    pub const BUFFER_TYPE_AT: i32 = 4;
    pub const BUFFER_BYTESUSED_AT: i32 = 8;
    pub const BUFFER_MEMORY_AT: i32 = 60;
    pub const BUFFER_OFFSET_AT: i32 = 64;
    pub const BUFFER_LENGTH_AT: i32 = 72;
    /// The state block a `Cam` handle points to (allocated zeroed at `camera_open`, freed by `camera_close`), as
    /// 8-byte cells: the descriptor, the width, the height, the buffer count, then each buffer's mapped address
    /// and its length.
    pub const STATE_SIZE: usize = 128;
    pub const STATE_FD_AT: i32 = 0;
    pub const STATE_WIDTH_AT: i32 = 8;
    pub const STATE_HEIGHT_AT: i32 = 16;
    pub const STATE_COUNT_AT: i32 = 24;
    pub const STATE_ADDR_AT: i32 = 32;
    pub const STATE_LEN_AT: i32 = 64;
}

/// The ALSA half (`docs/audio.md` §2).
pub mod alsa {
    pub const PCM_IOCTL_HW_PARAMS: i64 = 0xc260_4111;
    pub const PCM_IOCTL_PREPARE: i64 = 0x4140;
    pub const PCM_IOCTL_WRITEI_FRAMES: i64 = 0x4018_4150;
    pub const PCM_IOCTL_READI_FRAMES: i64 = 0x8018_4151;
    pub const PCM_IOCTL_DROP: i64 = 0x4143;
    /// `sizeof(struct snd_pcm_hw_params)`, and its offsets: `masks` (3 masks of 32 bytes, 4 bytes in),
    /// `intervals` (12 of 12 bytes, 260 bytes in), `rmask`.
    pub const HW_PARAMS_SIZE: usize = 608;
    pub const HW_PARAMS_FLAGS_AT: i32 = 0;
    pub const HW_PARAMS_MASKS_AT: i32 = 4;
    pub const HW_PARAMS_INTERVALS_AT: i32 = 260;
    pub const HW_PARAMS_RMASK_AT: i32 = 512;
    pub const MASK_SIZE: usize = 32;
    pub const INTERVAL_SIZE: usize = 12;
    /// `SNDRV_PCM_HW_PARAM_ACCESS`, `FORMAT` and `SUBFORMAT` index the masks; `SAMPLE_BITS`, `FRAME_BITS`,
    /// `CHANNELS`, `RATE`, ... index the intervals from `FIRST_INTERVAL` (8).
    pub const PARAM_ACCESS: usize = 0;
    pub const PARAM_FORMAT: usize = 1;
    pub const PARAM_SUBFORMAT: usize = 2;
    pub const PARAM_CHANNELS: usize = 10;
    pub const PARAM_RATE: usize = 11;
    pub const FIRST_INTERVAL: usize = 8;
    /// `SNDRV_PCM_ACCESS_RW_INTERLEAVED`, `SNDRV_PCM_FORMAT_S16_LE`, `SNDRV_PCM_SUBFORMAT_STD`: the bit each
    /// sets in its mask.
    pub const ACCESS_RW_INTERLEAVED: u32 = 3;
    pub const FORMAT_S16_LE: u32 = 2;
    pub const SUBFORMAT_STD: u32 = 0;
    /// `struct snd_interval` is `min`, `max` and a flags word (`openmin`, `openmax`, `integer`, `empty`).
    pub const INTERVAL_MIN_AT: i32 = 0;
    pub const INTERVAL_MAX_AT: i32 = 4;
    pub const INTERVAL_FLAGS_AT: i32 = 8;
    /// `struct snd_xferi`: `result`, `buf`, `frames`.
    pub const XFERI_SIZE: usize = 24;
    pub const XFERI_RESULT_AT: i32 = 0;
    pub const XFERI_BUF_AT: i32 = 8;
    pub const XFERI_FRAMES_AT: i32 = 16;
    /// The stream is opened stereo at the device's rate (`docs/audio.md` §2: no mono mode exists) and the
    /// backend converts, one frame being two 16-bit samples.
    pub const CHANNELS: i64 = 2;
    pub const FRAME_BYTES: i64 = 4;
    /// Frames converted per `mic_read`/`speaker_write` call, bounding the stack buffer (4 KiB).
    pub const CHUNK_FRAMES: i64 = 1024;
}
