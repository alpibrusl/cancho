# Audio: capabilities for sound capture and playback

> **Status: design (T1); nothing here is built.** Written 2026-10-31 for issue #660, before any code, as `CONTRIBUTING.md` asks. What was
> measured is in §2 and is the only measurement in the document; every other section is a **[choice]** or an open question. The models are
> `docs/tty.md` and `docs/camera.md`, which this design follows on purpose. **No audio was recorded or played to take these
> measurements**: they are hardware parameter queries (`arecord --dump-hw-params`) and a C program that prints `<sound/asound.h>`
> numbers, which open a device and read what it offers without starting a stream.

## 1. Why

A cancho program cannot record from a microphone or play to a speaker without a foreign call. For cancho-robot the question is the one
`docs/tty.md` §1 answers for serial ports and `docs/camera.md` §1 for cameras: `cancho authority` should say *which sound device* a program
can reach, not a list of libc and ALSA symbol names beside "never touches the filesystem" while it opens a device node. Listening is also
the capability a user most wants to see named: a program that can play a tone and a program that can hear the room are not the same
authority, so they are two capabilities (§3).

## 2. What was measured (Linux x86_64, card `sof-hda-dsp`, a laptop with a digital microphone array)

| Fact | Measurement |
|---|---|
| Devices (`/proc/asound/cards`, `arecord -l`, `aplay -l`) | one card, `sofhdadsp`. Capture: device 0 "HDA Analog", device 6 "DMIC", device 7 "DMIC16kHz". Playback: device 0 "HDA Analog", 3 to 5 HDMI, 31 "HDA Analog Deep Buffer" |
| Device nodes | `/dev/snd/pcmC0D0c` (capture), `pcmC0D0p` (playback), mode `crw-rw----+`, group `audio` |
| Access modes offered | `MMAP_INTERLEAVED` and `RW_INTERLEAVED` on every device listed |
| Sample formats | `hw:0,0` capture: `S16_LE`, `S24_LE`; `hw:0,6` and `hw:0,7` capture: `S16_LE`, `S24_LE`, `S32_LE`; `hw:0,0` playback: `S16_LE`, `S24_LE`, `S32_LE`. `S16_LE` is on every device |
| Channels | exactly **2** on every device tried; no mono mode |
| Rate | exactly **48000** on `hw:0,0` and `hw:0,6`; exactly **16000** on `hw:0,7`; no other rate |
| Period / buffer | period 24 to 4096 frames, 2 to 16 periods, buffer 48 to 16384 frames |
| `sizeof` (clang, `<sound/asound.h>`) | `snd_pcm_hw_params` 608, `snd_pcm_sw_params` 136, `snd_xferi` 24, `snd_mask` 32, `snd_interval` 12, `snd_pcm_status` 152 |
| `snd_pcm_hw_params` offsets | `masks` 4, `intervals` 260, `rmask` 512 |
| `snd_xferi` offsets | `result` 0, `buf` 8, `frames` 16 |
| Request numbers | `PVERSION 0x80044100`, `INFO 0x81204101`, `HW_REFINE 0xc2604110`, `HW_PARAMS 0xc2604111`, `SW_PARAMS 0xc0884113`, `PREPARE 0x4140`, `START 0x4142`, `DROP 0x4143`, `DRAIN 0x4144`, `WRITEI_FRAMES 0x40184150`, `READI_FRAMES 0x80184151` |
| Parameter indices | masks: `ACCESS` 0, `FORMAT` 1; intervals (first = 8): `CHANNELS` 10, `RATE` 11, `PERIOD_SIZE` 13, `BUFFER_SIZE` 17; `RW_INTERLEAVED` = 3, `S16_LE` = 2 |

Consequences: the raw `hw:` devices on this machine are **stereo-only at one fixed rate**, so "mono at 16 kHz" is not something the kernel
device will give; a backend that offers mono has to convert (take one channel, or average two). A desktop sound server (PipeWire is installed
here) sits above these nodes and does its own conversion, but it is a user-space daemon with its own protocol, not a syscall surface, and a
capability that talks to it would be a different design. Not measured: whether the raw device can be opened for a *stream* while the sound
server holds it (the parameter queries opened the nodes without error, which says nothing about a started stream); aarch64's numbers (the
robot's Raspberry Pi 5); macOS (CoreAudio is not ALSA at all, so a second backend, not a variation); latency and xrun behaviour under load.

## 3. The surface **[choice]**

Two capabilities, because recording and playing are different authority and the report should be able to say "can play, cannot hear":

```cho
res Mic              -- the capture capability, prefix-narrowed: Mic("/dev/snd/pcmC0D0c")
res Speaker          -- the playback capability, prefix-narrowed: Speaker("/dev/snd/pcmC0D0p")
res Capture          -- an open capture stream, linear
res Playback         -- an open playback stream, linear

mic_open(m: &Mic(bound), path: &[byte]) -> MicOpened                 // Ok(Capture) | Failed(errno)
mic_configure(c: &!Capture, rate: int) -> int                        // mono, signed 16-bit; 0, or the errno
mic_read(c: &!Capture, into: &![byte]) -> int                        // bytes that fit; never blocks; 0 when none is ready; -1 on error
mic_close(c: Capture) -> int

speaker_open(s: &Speaker(bound), path: &[byte]) -> SpeakerOpened    // Ok(Playback) | Failed(errno)
speaker_configure(p: &!Playback, rate: int) -> int                   // mono, signed 16-bit; 0, or the errno
speaker_write(p: &!Playback, bytes: &[byte]) -> int                  // bytes accepted, whole or short; never blocks; -1 on error
speaker_close(p: Playback) -> int

poller_add_mic(q: &!Poller, c: &Capture, token: int, events: int) -> int
poller_add_speaker(q: &!Poller, p: &Playback, token: int, events: int) -> int
```

One call configures, as `tty_configure` and `camera_configure` do: the program names a rate; the backend picks everything else. **[choice]**
The sample the program sees is **signed 16-bit little-endian, one channel**, whatever the device speaks, because that is the format every
device in §2 offers (`S16_LE`) and a format vocabulary is the surface `Tty` exists to avoid. The device is opened stereo, because §2 measured
nothing else; a capture takes the left channel, a playback writes the sample to both. A rate the device does not offer is `EINVAL` before the
stream starts (on this machine, 48000 for `hw:0,0` and 16000 for `hw:0,7`, and nothing else). The backend owns the ALSA structs, the period
and buffer sizes, the `hw_params` mask arithmetic and the `START`; the program sees none of them. A second call for stereo or a second format
is added when something asks for it.

Nonblocking is the same story as `Tty` and `Camera`: the stream is opened nonblocking, `mic_read` and `speaker_write` never wait, and the
`Poller` is the waiting story. **[choice]** An overrun (capture) or underrun (playback) is recovered by the backend with `PREPARE` and the
call answers what it can; the program is not told how many frames were lost. A caller who needs to know can compare byte counts against a
clock, and the design does not add a counter until something asks.

## 4. Authority **[choice]**

`mic_open` and `speaker_open` read a path under the capability's bound, narrowed as `Tty` is (prefix-shaped, non-empty); the stream verbs are
path-free, the prefix having been spent at open:

```
performs
    mic_open("/dev/snd/pcmC0D0c")
    mic_read
    speaker_open("/dev/snd/pcmC0D0p")
    speaker_write
```

and **no `foreign` lines**, which is the acceptance of §7. A program with a `Speaker` and no `Mic` is shown by its report to be unable to
listen.

## 5. Refusals

As in `docs/tty.md` §5: a path outside the bound **traps at run time** (`path_escapes`), exactly as `tty_open`'s does, because the path is a run-time value: there are no `mic-path` or `speaker-path` rule tags. *(Corrected before any code, with `docs/camera.md` §5: `docs/tty.md` §5 promises a compile-time `tty-path` refusal, but the compiler has no such rule and `tty_open` checks at run time.)* a node
that is not a PCM device, or will not open, is `Failed(errno)`; a rate the device does not offer is `EINVAL` before streaming starts; read or
write on a closed stream is `-1`. Nothing here may reach a panic other than that trap.

## 6. Edition and slices **[choice]**

`Split` grows **three** fields, `camera` (`docs/camera.md` §6), `mic` and `speaker`, and that is **edition 12** (issue #661): edition 12 is
edition 11 plus those three, fourteen fields in all, and an older file's `split()` still answers the fields it did. Slices, each with its
acceptance written before it is built: T1 this document; T2 the verbs and the refusals on both backends, with the aarch64 numbers measured
on the robot; T3 `poller_add_mic` and `poller_add_speaker`; T4 an example (`examples/audio_hello.cho`: open the speaker, write a tone, close
it; open the mic, take some samples, print their peak level); T5 the report check.

## 7. Acceptance of the whole

A cancho program with no `extern fn` plays a one-second tone on `Speaker` and takes a second of samples from `Mic`, `cancho authority` reports
`speaker_open("/dev/snd/pcmC0D0p")` and `mic_open("/dev/snd/pcmC0D0c")` and no foreign line, and a program given only a `Speaker` does not
type-check a call to `mic_read`. The recording half of the acceptance runs only where the person running it has agreed to the microphone
being opened; it is a manual check, not a CI one.

## 8. Not decided, not claimed

macOS (CoreAudio is a second backend); a sound-server (PipeWire/PulseAudio) backend; several cards or devices at once; stereo and other
sample formats; latency and xrun counts (not measured); MMAP access (offered by the hardware, not needed for the first user); device
enumeration (the program names a node, as `Tty` does); echo and loopback; whether `Mic` and `Speaker` belong in the standard library or
beside it. Nothing here claims any of them.
