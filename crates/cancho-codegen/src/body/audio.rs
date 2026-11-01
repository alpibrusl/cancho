//! The microphone and speaker verbs (`docs/audio.md` §3), Cranelift, walking `docs/device-backends.md` §6-§9. The
//! handle is the address of a `calloc`'d 128-byte block: cell 0 the descriptor, cell 8 the rate, cell 16 one once
//! configured. The helpers (`ioctl`, zeroed slots, counted loops) live in `camera.rs`.

use crate::body::BodyEmitter;
use crate::*;
use cancho_ir::device::alsa as a;
use cancho_ir::{DEVICE_EAGAIN, DEVICE_EINVAL, DEVICE_EPIPE};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{Block, MemFlags, Value};

const RATE_AT: i32 = 8;
const CONFIGURED_AT: i32 = 16;

impl<'a, 'f> BodyEmitter<'a, 'f> {
    /// `snd_pcm_hw_params` as libasound's `any` builds it, restricted to stereo S16_LE interleaved at `rate`.
    fn pcm_params(&mut self, rate: Value) -> Value {
        let params = self.zeroed_slot(a::HW_PARAMS_SIZE);
        self.store32(params, a::HW_PARAMS_RMASK_AT, 0xffff_ffff);
        // every mask all ones, then the three restricted to their one bit
        for word in 0..(3 * a::MASK_SIZE / 4) {
            self.store32(params, a::HW_PARAMS_MASKS_AT + 4 * word as i32, 0xffff_ffff);
        }
        for (index, bit) in [
            (a::PARAM_ACCESS, a::ACCESS_RW_INTERLEAVED),
            (a::PARAM_FORMAT, a::FORMAT_S16_LE),
            (a::PARAM_SUBFORMAT, a::SUBFORMAT_STD),
        ] {
            let mask = a::HW_PARAMS_MASKS_AT + (index * a::MASK_SIZE) as i32;
            for word in 0..(a::MASK_SIZE / 4) as u32 {
                let bits = if word == bit / 32 { 1i64 << (bit % 32) } else { 0 };
                self.store32(params, mask + 4 * word as i32, bits);
            }
        }
        // every interval open, then channels and rate pinned
        for interval in 0..12 {
            let at = a::HW_PARAMS_INTERVALS_AT + (interval * a::INTERVAL_SIZE) as i32;
            self.store32(params, at + a::INTERVAL_MIN_AT, 0);
            self.store32(params, at + a::INTERVAL_MAX_AT, 0xffff_ffff);
            self.store32(params, at + a::INTERVAL_FLAGS_AT, 0);
        }
        let rate = self.builder.ins().ireduce(types::I32, rate);
        for (index, value) in [(a::PARAM_CHANNELS, None), (a::PARAM_RATE, Some(rate))] {
            let at =
                a::HW_PARAMS_INTERVALS_AT + ((index - a::FIRST_INTERVAL) * a::INTERVAL_SIZE) as i32;
            let value = match value {
                Some(rate) => rate,
                None => self.builder.ins().iconst(types::I32, a::CHANNELS),
            };
            self.builder.ins().store(MemFlags::trusted(), value, params, at + a::INTERVAL_MIN_AT);
            self.builder.ins().store(MemFlags::trusted(), value, params, at + a::INTERVAL_MAX_AT);
        }
        params
    }

    /// `mic_configure` and `speaker_configure`: `docs/device-backends.md` §6.
    fn pcm_configure(&mut self, args: &[Value]) -> Vec<Value> {
        if !self.device_supported() {
            return vec![self.builder.ins().iconst(types::I64, -1)];
        }
        let pointer = self.pointer;
        let rate = args[1];
        let merge = self.merge_block();
        let nothing = self.builder.ins().icmp_imm(IntCC::SignedLessThanOrEqual, rate, 0);
        self.answer_if(nothing, merge, DEVICE_EINVAL);

        let state = self.state_of(args[0]);
        let fd = self.state_fd(state);
        let params = self.pcm_params(rate);
        let refined = self.ioctl(fd, a::PCM_IOCTL_HW_PARAMS, params);
        self.bail_on_error(refined, merge);
        let none = self.builder.ins().iconst(pointer, 0);
        let prepared = self.ioctl(fd, a::PCM_IOCTL_PREPARE, none);
        self.bail_on_error(prepared, merge);
        self.builder.ins().store(MemFlags::trusted(), rate, state, RATE_AT);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().store(MemFlags::trusted(), one, state, CONFIGURED_AT);
        let ok = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge, &[ok.into()]);
        self.finish_at(merge)
    }

    /// The frames a call moves: `min(len / 2, CHUNK_FRAMES)`; an unconfigured stream answers `-1` and zero frames
    /// answer `0`. Leaves the builder in the block where there is work to do.
    fn pcm_frames(&mut self, state: Value, length: Value, merge: Block) -> Value {
        let configured =
            self.builder.ins().load(types::I64, MemFlags::trusted(), state, CONFIGURED_AT);
        let not = self.builder.ins().icmp_imm(IntCC::Equal, configured, 0);
        self.answer_if(not, merge, -1);
        let samples = self.builder.ins().ushr_imm(length, 1);
        let cap = self.builder.ins().iconst(types::I64, a::CHUNK_FRAMES);
        let frames = self.builder.ins().umin(samples, cap);
        let idle = self.builder.ins().icmp_imm(IntCC::Equal, frames, 0);
        self.answer_if(idle, merge, 0);
        frames
    }

    /// One `snd_xferi` transfer of `frames` frames at `buffer`; `request` is `READI` or `WRITEI`. A failure
    /// answers `merge` (`EAGAIN` -> 0, `EPIPE` -> prepare and 0, else -1); on success the builder continues in a
    /// block and the frames actually moved (clamped to `frames`) are returned.
    fn pcm_transfer(
        &mut self,
        fd: Value,
        request: i64,
        buffer: Value,
        frames: Value,
        merge: Block,
    ) -> Value {
        let pointer = self.pointer;
        let xfer = self.zeroed_slot(a::XFERI_SIZE);
        self.builder.ins().store(MemFlags::trusted(), buffer, xfer, a::XFERI_BUF_AT);
        self.builder.ins().store(MemFlags::trusted(), frames, xfer, a::XFERI_FRAMES_AT);
        let moved = self.ioctl(fd, request, xfer);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, moved, 0);
        let refuse = self.builder.create_block();
        let go_on = self.builder.create_block();
        self.builder.ins().brif(failed, refuse, &[], go_on, &[]);

        self.builder.switch_to_block(refuse);
        self.builder.seal_block(refuse);
        let reason = self.errno();
        let busy = self.builder.create_block();
        let not_busy = self.builder.create_block();
        let is_busy = self.builder.ins().icmp_imm(IntCC::Equal, reason, DEVICE_EAGAIN);
        self.builder.ins().brif(is_busy, busy, &[], not_busy, &[]);
        self.builder.switch_to_block(busy);
        self.builder.seal_block(busy);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge, &[zero.into()]);
        self.builder.switch_to_block(not_busy);
        self.builder.seal_block(not_busy);
        let broken = self.builder.create_block();
        let wrong = self.builder.create_block();
        let is_broken = self.builder.ins().icmp_imm(IntCC::Equal, reason, DEVICE_EPIPE);
        self.builder.ins().brif(is_broken, broken, &[], wrong, &[]);
        self.builder.switch_to_block(broken);
        self.builder.seal_block(broken);
        let none = self.builder.ins().iconst(pointer, 0);
        self.ioctl(fd, a::PCM_IOCTL_PREPARE, none);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge, &[zero.into()]);
        self.builder.switch_to_block(wrong);
        self.builder.seal_block(wrong);
        let minus = self.builder.ins().iconst(types::I64, -1);
        self.builder.ins().jump(merge, &[minus.into()]);

        self.builder.switch_to_block(go_on);
        self.builder.seal_block(go_on);
        let result =
            self.builder.ins().load(types::I64, MemFlags::trusted(), xfer, a::XFERI_RESULT_AT);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let positive = self.builder.ins().smax(result, zero);
        self.builder.ins().umin(positive, frames)
    }

    /// A 4 KiB stack buffer for the stereo frames of one chunk.
    fn pcm_buffer(&mut self) -> Value {
        let pointer = self.pointer;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            (a::CHUNK_FRAMES * a::FRAME_BYTES) as u32,
            3,
        ));
        self.builder.ins().stack_addr(pointer, slot, 0)
    }

    pub(crate) fn mic_configure(&mut self, args: &[Value]) -> Vec<Value> {
        self.pcm_configure(args)
    }

    /// `mic_read(&!Capture, into)`: `docs/device-backends.md` §7.
    pub(crate) fn mic_read(&mut self, args: &[Value]) -> Vec<Value> {
        if !self.device_supported() {
            return vec![self.builder.ins().iconst(types::I64, -1)];
        }
        let (into, length) = (args[1], args[2]);
        let merge = self.merge_block();
        let state = self.state_of(args[0]);
        let fd = self.state_fd(state);
        let frames = self.pcm_frames(state, length, merge);
        let buffer = self.pcm_buffer();
        let moved = self.pcm_transfer(fd, a::PCM_IOCTL_READI_FRAMES, buffer, frames, merge);
        self.counted_loop(moved, |this, k| {
            let from = this.builder.ins().ishl_imm(k, 2);
            let from = this.builder.ins().iadd(buffer, from);
            let left = this.builder.ins().load(types::I16, MemFlags::trusted(), from, 0);
            let to = this.builder.ins().ishl_imm(k, 1);
            let to = this.builder.ins().iadd(into, to);
            this.builder.ins().store(MemFlags::trusted(), left, to, 0);
        });
        let bytes = self.builder.ins().ishl_imm(moved, 1);
        self.builder.ins().jump(merge, &[bytes.into()]);
        self.finish_at(merge)
    }

    /// `mic_close(Capture)` and `speaker_close(Playback)`: `docs/device-backends.md` §9.
    fn pcm_close(&mut self, state: Value) -> Vec<Value> {
        if !self.device_supported() {
            return vec![self.builder.ins().iconst(types::I64, -1)];
        }
        let pointer = self.pointer;
        let fd = self.state_fd(state);
        let configured =
            self.builder.ins().load(types::I64, MemFlags::trusted(), state, CONFIGURED_AT);
        let drop = self.builder.create_block();
        let release = self.builder.create_block();
        self.builder.ins().brif(configured, drop, &[], release, &[]);
        self.builder.switch_to_block(drop);
        self.builder.seal_block(drop);
        let none = self.builder.ins().iconst(pointer, 0);
        self.ioctl(fd, a::PCM_IOCTL_DROP, none);
        self.builder.ins().jump(release, &[]);
        self.builder.switch_to_block(release);
        self.builder.seal_block(release);
        vec![self.close_and_free(state)]
    }

    pub(crate) fn mic_close(&mut self, args: &[Value]) -> Vec<Value> {
        self.pcm_close(args[0])
    }

    pub(crate) fn speaker_configure(&mut self, args: &[Value]) -> Vec<Value> {
        self.pcm_configure(args)
    }

    /// `speaker_write(&!Playback, bytes)`: `docs/device-backends.md` §8.
    pub(crate) fn speaker_write(&mut self, args: &[Value]) -> Vec<Value> {
        if !self.device_supported() {
            return vec![self.builder.ins().iconst(types::I64, -1)];
        }
        let (bytes, length) = (args[1], args[2]);
        let merge = self.merge_block();
        let state = self.state_of(args[0]);
        let fd = self.state_fd(state);
        let frames = self.pcm_frames(state, length, merge);
        let buffer = self.pcm_buffer();
        self.counted_loop(frames, |this, k| {
            let from = this.builder.ins().ishl_imm(k, 1);
            let from = this.builder.ins().iadd(bytes, from);
            let sample = this.builder.ins().load(types::I16, MemFlags::trusted(), from, 0);
            let to = this.builder.ins().ishl_imm(k, 2);
            let to = this.builder.ins().iadd(buffer, to);
            this.builder.ins().store(MemFlags::trusted(), sample, to, 0);
            this.builder.ins().store(MemFlags::trusted(), sample, to, 2);
        });
        let moved = self.pcm_transfer(fd, a::PCM_IOCTL_WRITEI_FRAMES, buffer, frames, merge);
        let written = self.builder.ins().ishl_imm(moved, 1);
        self.builder.ins().jump(merge, &[written.into()]);
        self.finish_at(merge)
    }

    pub(crate) fn speaker_close(&mut self, args: &[Value]) -> Vec<Value> {
        self.pcm_close(args[0])
    }
}
