//! The camera verbs (`docs/camera.md` §3), Cranelift, walking `docs/device-backends.md` §2-§5 and §10 on Linux
//! x86_64 (the only target whose V4L2 numbers are measured). The handle is the address of a `calloc`'d state block
//! (`cancho_ir::device::v4l2::STATE_*`). The helpers at the top (`ioctl`, zeroed slots, counted loops, the state
//! block) are shared with `audio.rs`.

use crate::body::BodyEmitter;
use crate::*;
use cancho_ir::device::v4l2 as v;
use cancho_ir::{DEVICE_EAGAIN, DEVICE_EINVAL, DEVICE_ENODEV, Device};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{Block, MemFlags, Value};

/// `ENOMEM`: the answer when the state block cannot be allocated.
const ENOMEM: i64 = 12;

impl<'a, 'f> BodyEmitter<'a, 'f> {
    /// Whether the V4L2 and ALSA numbers are measured for the target being compiled for (Linux x86_64).
    pub(crate) fn device_supported(&self) -> bool {
        matches!(
            self.module.isa().triple().operating_system,
            target_lexicon::OperatingSystem::Linux
        ) && matches!(self.module.isa().triple().architecture, target_lexicon::Architecture::X86_64)
    }

    fn const64(&mut self, value: i64) -> Value {
        self.builder.ins().iconst(types::I64, value)
    }

    /// A zeroed, 8-aligned stack slot of `size` bytes (a multiple of 4), its address.
    pub(crate) fn zeroed_slot(&mut self, size: usize) -> Value {
        let pointer = self.pointer;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size.div_ceil(8) as u32 * 8,
            3,
        ));
        let at = self.builder.ins().stack_addr(pointer, slot, 0);
        let zero = self.const64(0);
        let mut offset = 0;
        while offset + 8 <= size {
            self.builder.ins().store(MemFlags::trusted(), zero, at, offset as i32);
            offset += 8;
        }
        if offset < size {
            let zero32 = self.builder.ins().iconst(types::I32, 0);
            self.builder.ins().store(MemFlags::trusted(), zero32, at, offset as i32);
        }
        at
    }

    /// Store the constant `value` as a 32-bit word at `base + at`.
    pub(crate) fn store32(&mut self, base: Value, at: i32, value: i64) {
        let value = self.builder.ins().iconst(types::I32, value as u32 as i32 as i64);
        self.builder.ins().store(MemFlags::trusted(), value, base, at);
    }

    /// `ioctl(fd, request, argument)` with the request zero-extended to 64 bits (Linux), the result widened.
    pub(crate) fn ioctl(&mut self, fd: Value, request: i64, argument: Value) -> Value {
        let pointer = self.pointer;
        let request = self.const64(request);
        let result = self.libc_call(
            "ioctl",
            &[types::I32, types::I64, pointer],
            &[types::I32],
            &[fd, request, argument],
        );
        self.builder.ins().sextend(types::I64, result)
    }

    /// If `result` is negative, jump to `merge` with the `errno`; otherwise carry on in a fresh block.
    pub(crate) fn bail_on_error(&mut self, result: Value, merge: Block) {
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        let refuse = self.builder.create_block();
        let go_on = self.builder.create_block();
        self.builder.ins().brif(failed, refuse, &[], go_on, &[]);
        self.builder.switch_to_block(refuse);
        self.builder.seal_block(refuse);
        let reason = self.errno();
        self.builder.ins().jump(merge, &[reason.into()]);
        self.builder.switch_to_block(go_on);
        self.builder.seal_block(go_on);
    }

    /// Jump to `merge` with the constant `value` when `condition` holds; otherwise carry on in a fresh block.
    pub(crate) fn answer_if(&mut self, condition: Value, merge: Block, value: i64) {
        let answer = self.builder.create_block();
        let go_on = self.builder.create_block();
        self.builder.ins().brif(condition, answer, &[], go_on, &[]);
        self.builder.switch_to_block(answer);
        self.builder.seal_block(answer);
        let value = self.const64(value);
        self.builder.ins().jump(merge, &[value.into()]);
        self.builder.switch_to_block(go_on);
        self.builder.seal_block(go_on);
    }

    /// A block taking one `I64` parameter, the merge point of a verb.
    pub(crate) fn merge_block(&mut self) -> Block {
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        merge
    }

    /// Switch to `merge` and answer its parameter.
    pub(crate) fn finish_at(&mut self, merge: Block) -> Vec<Value> {
        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        vec![self.builder.block_params(merge)[0]]
    }

    /// `for i in 0..count` (unsigned): `body` emits one pass and leaves the builder in an unterminated block.
    pub(crate) fn counted_loop(&mut self, count: Value, body: impl FnOnce(&mut Self, Value)) {
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        let pass = self.builder.create_block();
        let exit = self.builder.create_block();
        let zero = self.const64(0);
        self.builder.ins().jump(header, &[zero.into()]);
        self.builder.switch_to_block(header);
        let i = self.builder.block_params(header)[0];
        let done = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, i, count);
        self.builder.ins().brif(done, exit, &[], pass, &[]);
        self.builder.switch_to_block(pass);
        self.builder.seal_block(pass);
        body(self, i);
        let next = self.builder.ins().iadd_imm(i, 1);
        self.builder.ins().jump(header, &[next.into()]);
        self.builder.seal_block(header);
        self.builder.switch_to_block(exit);
        self.builder.seal_block(exit);
    }

    /// The descriptor in a state block (cell 0).
    pub(crate) fn state_fd(&mut self, state: Value) -> Value {
        let fd = self.builder.ins().load(types::I64, MemFlags::trusted(), state, v::STATE_FD_AT);
        self.builder.ins().ireduce(types::I32, fd)
    }

    /// The block a handle reference points to.
    pub(crate) fn state_of(&mut self, reference: Value) -> Value {
        self.builder.ins().load(types::I64, MemFlags::trusted(), reference, 0)
    }

    /// `close(fd)`, `free(block)`; `close`'s result widened.
    pub(crate) fn close_and_free(&mut self, state: Value) -> Value {
        let fd = self.state_fd(state);
        let closed = self.libc_call("close", &[types::I32], &[types::I32], &[fd]);
        self.free(state);
        self.builder.ins().sextend(types::I64, closed)
    }

    /// `camera_open`, `mic_open` and `speaker_open` (`Expr::DeviceOpen`): answers the three leaves of the
    /// `*Opened` enum -- tag (`Ok` 0, `Failed` 1), the handle, and the `errno`.
    pub(crate) fn device_open(
        &mut self,
        _device: Device,
        prefix: &str,
        args: &[cancho_ir::Expr],
    ) -> Vec<Value> {
        if !self.device_supported() {
            let tag = self.const64(1);
            let handle = self.const64(-1);
            let reason = self.const64(DEVICE_ENODEV);
            return vec![tag, handle, reason];
        }
        let pointer = self.pointer;
        let path = self.expr(&args[1]);
        let path = self.checked_path(prefix, &path);
        let cwd = self.builder.ins().iconst(types::I32, -100);
        let fd = self.openat(cwd, path, cancho_ir::DEVICE_OPEN_FLAGS, 0);
        let fd = self.builder.ins().sextend(types::I64, fd);

        let merge = self.builder.create_block();
        for _ in 0..3 {
            self.builder.append_block_param(merge, types::I64);
        }
        let refused = self.builder.create_block();
        let opened = self.builder.create_block();
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, fd, 0);
        self.builder.ins().brif(failed, refused, &[], opened, &[]);

        self.builder.switch_to_block(refused);
        self.builder.seal_block(refused);
        let reason = self.errno();
        let one = self.const64(1);
        self.builder.ins().jump(merge, &[one.into(), fd.into(), reason.into()]);

        self.builder.switch_to_block(opened);
        self.builder.seal_block(opened);
        let count = self.builder.ins().iconst(pointer, 1);
        let size = self.builder.ins().iconst(pointer, v::STATE_SIZE as i64);
        let state = self.libc_call("calloc", &[pointer, pointer], &[pointer], &[count, size]);
        let no_memory = self.builder.create_block();
        let allocated = self.builder.create_block();
        self.builder.ins().brif(state, allocated, &[], no_memory, &[]);

        self.builder.switch_to_block(no_memory);
        self.builder.seal_block(no_memory);
        let fd32 = self.builder.ins().ireduce(types::I32, fd);
        self.libc_call("close", &[types::I32], &[types::I32], &[fd32]);
        let one = self.const64(1);
        let none = self.const64(-1);
        let reason = self.const64(ENOMEM);
        self.builder.ins().jump(merge, &[one.into(), none.into(), reason.into()]);

        self.builder.switch_to_block(allocated);
        self.builder.seal_block(allocated);
        self.builder.ins().store(MemFlags::trusted(), fd, state, v::STATE_FD_AT);
        let zero = self.const64(0);
        self.builder.ins().jump(merge, &[zero.into(), state.into(), zero.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        self.builder.block_params(merge).to_vec()
    }

    /// A `v4l2_buffer` (zeroed) with `type = capture`, `memory = mmap` and `index`.
    fn v4l2_buffer(&mut self, index: Value) -> Value {
        let buffer = self.zeroed_slot(v::BUFFER_SIZE);
        let index = self.builder.ins().ireduce(types::I32, index);
        self.builder.ins().store(MemFlags::trusted(), index, buffer, v::BUFFER_INDEX_AT);
        self.store32(buffer, v::BUFFER_TYPE_AT, v::BUF_TYPE_CAPTURE);
        self.store32(buffer, v::BUFFER_MEMORY_AT, v::MEMORY_MMAP);
        buffer
    }

    /// A 4-byte cell holding the buffer type, for `STREAMON` and `STREAMOFF`.
    fn type_cell(&mut self) -> Value {
        let cell = self.zeroed_slot(8);
        self.store32(cell, 0, v::BUF_TYPE_CAPTURE);
        cell
    }

    /// `camera_configure(&!Cam, width, height)` -- `docs/device-backends.md` §3.
    pub(crate) fn camera_configure(&mut self, args: &[Value]) -> Vec<Value> {
        if !self.device_supported() {
            return vec![self.const64(-1)];
        }
        let pointer = self.pointer;
        let (width, height) = (args[1], args[2]);
        let merge = self.merge_block();

        let smaller = self.builder.ins().smin(width, height);
        let nothing = self.builder.ins().icmp_imm(IntCC::SignedLessThanOrEqual, smaller, 0);
        self.answer_if(nothing, merge, DEVICE_EINVAL);

        let state = self.state_of(args[0]);
        let fd = self.state_fd(state);
        let known =
            self.builder.ins().load(types::I64, MemFlags::trusted(), state, v::STATE_WIDTH_AT);
        let twice = self.builder.ins().icmp_imm(IntCC::NotEqual, known, 0);
        self.answer_if(twice, merge, DEVICE_EINVAL);

        // the format: YUYV at the size asked
        let format = self.zeroed_slot(v::FORMAT_SIZE);
        let (w32, h32) = (
            self.builder.ins().ireduce(types::I32, width),
            self.builder.ins().ireduce(types::I32, height),
        );
        self.store32(format, v::FORMAT_TYPE_AT, v::BUF_TYPE_CAPTURE);
        self.builder.ins().store(MemFlags::trusted(), w32, format, v::FORMAT_WIDTH_AT);
        self.builder.ins().store(MemFlags::trusted(), h32, format, v::FORMAT_HEIGHT_AT);
        self.store32(format, v::FORMAT_PIXELFORMAT_AT, v::PIX_FMT_YUYV);
        self.store32(format, v::FORMAT_FIELD_AT, v::FIELD_NONE);
        let set = self.ioctl(fd, v::VIDIOC_S_FMT, format);
        self.bail_on_error(set, merge);

        // the device must have kept the size
        let got_w =
            self.builder.ins().load(types::I32, MemFlags::trusted(), format, v::FORMAT_WIDTH_AT);
        let got_h =
            self.builder.ins().load(types::I32, MemFlags::trusted(), format, v::FORMAT_HEIGHT_AT);
        let same_w = self.builder.ins().icmp(IntCC::Equal, got_w, w32);
        let same_h = self.builder.ins().icmp(IntCC::Equal, got_h, h32);
        let same = self.builder.ins().band(same_w, same_h);
        let differs = self.builder.ins().icmp_imm(IntCC::Equal, same, 0);
        self.answer_if(differs, merge, DEVICE_EINVAL);
        self.builder.ins().store(MemFlags::trusted(), width, state, v::STATE_WIDTH_AT);
        self.builder.ins().store(MemFlags::trusted(), height, state, v::STATE_HEIGHT_AT);

        // the buffers: ask for four, use what was granted (at most four)
        let request = self.zeroed_slot(v::REQBUFS_SIZE);
        self.store32(request, v::REQBUFS_COUNT_AT, v::BUFFERS as i64);
        self.store32(request, v::REQBUFS_TYPE_AT, v::BUF_TYPE_CAPTURE);
        self.store32(request, v::REQBUFS_MEMORY_AT, v::MEMORY_MMAP);
        let asked = self.ioctl(fd, v::VIDIOC_REQBUFS, request);
        self.bail_on_error(asked, merge);
        let granted =
            self.builder.ins().load(types::I32, MemFlags::trusted(), request, v::REQBUFS_COUNT_AT);
        let granted = self.builder.ins().uextend(types::I64, granted);
        let none = self.builder.ins().icmp_imm(IntCC::Equal, granted, 0);
        self.answer_if(none, merge, DEVICE_EINVAL);
        let four = self.const64(v::BUFFERS as i64);
        let count = self.builder.ins().umin(granted, four);
        self.builder.ins().store(MemFlags::trusted(), count, state, v::STATE_COUNT_AT);

        // each buffer: query, map, queue
        self.counted_loop(count, |this, i| {
            let buffer = this.v4l2_buffer(i);
            let queried = this.ioctl(fd, v::VIDIOC_QUERYBUF, buffer);
            this.bail_on_error(queried, merge);
            let length = this.builder.ins().load(
                types::I32,
                MemFlags::trusted(),
                buffer,
                v::BUFFER_LENGTH_AT,
            );
            let length = this.builder.ins().uextend(types::I64, length);
            let offset = this.builder.ins().load(
                types::I32,
                MemFlags::trusted(),
                buffer,
                v::BUFFER_OFFSET_AT,
            );
            let offset = this.builder.ins().uextend(types::I64, offset);
            let null = this.builder.ins().iconst(pointer, 0);
            let protection = this.builder.ins().iconst(types::I32, 3);
            let shared = this.builder.ins().iconst(types::I32, 1);
            let mapped = this.libc_call(
                "mmap",
                &[pointer, types::I64, types::I32, types::I32, types::I32, types::I64],
                &[pointer],
                &[null, length, protection, shared, fd, offset],
            );
            let failed = this.builder.ins().icmp_imm(IntCC::Equal, mapped, -1);
            let refuse = this.builder.create_block();
            let go_on = this.builder.create_block();
            this.builder.ins().brif(failed, refuse, &[], go_on, &[]);
            this.builder.switch_to_block(refuse);
            this.builder.seal_block(refuse);
            let reason = this.errno();
            this.builder.ins().jump(merge, &[reason.into()]);
            this.builder.switch_to_block(go_on);
            this.builder.seal_block(go_on);
            let cell = this.builder.ins().ishl_imm(i, 3);
            let cell = this.builder.ins().iadd(state, cell);
            this.builder.ins().store(MemFlags::trusted(), mapped, cell, v::STATE_ADDR_AT);
            this.builder.ins().store(MemFlags::trusted(), length, cell, v::STATE_LEN_AT);
            let queued = this.ioctl(fd, v::VIDIOC_QBUF, buffer);
            this.bail_on_error(queued, merge);
        });

        let on = self.type_cell();
        let started = self.ioctl(fd, v::VIDIOC_STREAMON, on);
        self.bail_on_error(started, merge);
        let ok = self.const64(0);
        self.builder.ins().jump(merge, &[ok.into()]);
        self.finish_at(merge)
    }

    /// `camera_next(&!Cam, into)` -- `docs/device-backends.md` §4: the newest ready frame as grey bytes.
    pub(crate) fn camera_next(&mut self, args: &[Value]) -> Vec<Value> {
        if !self.device_supported() {
            return vec![self.const64(-1)];
        }
        let (into, length) = (args[1], args[2]);
        let merge = self.merge_block();
        let state = self.state_of(args[0]);
        let fd = self.state_fd(state);
        let width =
            self.builder.ins().load(types::I64, MemFlags::trusted(), state, v::STATE_WIDTH_AT);
        let height =
            self.builder.ins().load(types::I64, MemFlags::trusted(), state, v::STATE_HEIGHT_AT);
        let unset = self.builder.ins().icmp_imm(IntCC::Equal, width, 0);
        self.answer_if(unset, merge, -1);

        // drain the queue, keeping the newest buffer
        let drain = self.builder.create_block();
        self.builder.append_block_param(drain, types::I64); // have
        let after = self.builder.create_block();
        self.builder.append_block_param(after, types::I64); // have
        self.builder.append_block_param(after, types::I64); // errno of the failed dequeue
        let first = self.const64(-1);
        self.builder.ins().jump(drain, &[first.into()]);

        self.builder.switch_to_block(drain);
        let have = self.builder.block_params(drain)[0];
        let zero = self.const64(0);
        let buffer = self.v4l2_buffer(zero);
        let dequeued = self.ioctl(fd, v::VIDIOC_DQBUF, buffer);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, dequeued, 0);
        let empty = self.builder.create_block();
        let got = self.builder.create_block();
        self.builder.ins().brif(failed, empty, &[], got, &[]);

        self.builder.switch_to_block(empty);
        self.builder.seal_block(empty);
        let reason = self.errno();
        self.builder.ins().jump(after, &[have.into(), reason.into()]);

        self.builder.switch_to_block(got);
        self.builder.seal_block(got);
        let index =
            self.builder.ins().load(types::I32, MemFlags::trusted(), buffer, v::BUFFER_INDEX_AT);
        let index = self.builder.ins().uextend(types::I64, index);
        let older = self.builder.ins().icmp_imm(IntCC::SignedGreaterThanOrEqual, have, 0);
        let give_back = self.builder.create_block();
        let again = self.builder.create_block();
        self.builder.ins().brif(older, give_back, &[], again, &[]);
        self.builder.switch_to_block(give_back);
        self.builder.seal_block(give_back);
        let returned = self.v4l2_buffer(have);
        self.ioctl(fd, v::VIDIOC_QBUF, returned);
        self.builder.ins().jump(again, &[]);
        self.builder.switch_to_block(again);
        self.builder.seal_block(again);
        self.builder.ins().jump(drain, &[index.into()]);
        self.builder.seal_block(drain);

        // nothing ready, or a frame to convert
        self.builder.switch_to_block(after);
        self.builder.seal_block(after);
        let have = self.builder.block_params(after)[0];
        let reason = self.builder.block_params(after)[1];
        let nothing = self.builder.ins().icmp_imm(IntCC::SignedLessThan, have, 0);
        let idle = self.builder.create_block();
        let frame = self.builder.create_block();
        self.builder.ins().brif(nothing, idle, &[], frame, &[]);
        self.builder.switch_to_block(idle);
        self.builder.seal_block(idle);
        let busy = self.builder.ins().icmp_imm(IntCC::Equal, reason, DEVICE_EAGAIN);
        let zero = self.const64(0);
        let wrong = self.const64(-1);
        let answer = self.builder.ins().select(busy, zero, wrong);
        self.builder.ins().jump(merge, &[answer.into()]);

        self.builder.switch_to_block(frame);
        self.builder.seal_block(frame);
        let pixels = self.builder.ins().imul(width, height);
        let n = self.builder.ins().umin(length, pixels);
        let cell = self.builder.ins().ishl_imm(have, 3);
        let cell = self.builder.ins().iadd(state, cell);
        let mapped =
            self.builder.ins().load(self.pointer, MemFlags::trusted(), cell, v::STATE_ADDR_AT);
        self.counted_loop(n, |this, k| {
            let at = this.builder.ins().ishl_imm(k, 1);
            let at = this.builder.ins().iadd(mapped, at);
            let grey = this.builder.ins().load(types::I8, MemFlags::trusted(), at, 0);
            let to = this.builder.ins().iadd(into, k);
            this.builder.ins().store(MemFlags::trusted(), grey, to, 0);
        });
        let returned = self.v4l2_buffer(have);
        self.ioctl(fd, v::VIDIOC_QBUF, returned);
        self.builder.ins().jump(merge, &[n.into()]);
        self.finish_at(merge)
    }

    /// `camera_close(Cam)` -- `docs/device-backends.md` §5. The argument is the block address.
    pub(crate) fn camera_close(&mut self, args: &[Value]) -> Vec<Value> {
        if !self.device_supported() {
            return vec![self.const64(-1)];
        }
        let pointer = self.pointer;
        let state = args[0];
        let fd = self.state_fd(state);
        let width =
            self.builder.ins().load(types::I64, MemFlags::trusted(), state, v::STATE_WIDTH_AT);
        let configured = self.builder.create_block();
        let unmap = self.builder.create_block();
        self.builder.ins().brif(width, configured, &[], unmap, &[]);
        self.builder.switch_to_block(configured);
        self.builder.seal_block(configured);
        let off = self.type_cell();
        self.ioctl(fd, v::VIDIOC_STREAMOFF, off);
        self.builder.ins().jump(unmap, &[]);
        self.builder.switch_to_block(unmap);
        self.builder.seal_block(unmap);

        let four = self.const64(v::BUFFERS as i64);
        self.counted_loop(four, |this, i| {
            let cell = this.builder.ins().ishl_imm(i, 3);
            let cell = this.builder.ins().iadd(state, cell);
            let mapped =
                this.builder.ins().load(pointer, MemFlags::trusted(), cell, v::STATE_ADDR_AT);
            let length =
                this.builder.ins().load(types::I64, MemFlags::trusted(), cell, v::STATE_LEN_AT);
            let present = this.builder.create_block();
            let next = this.builder.create_block();
            this.builder.ins().brif(mapped, present, &[], next, &[]);
            this.builder.switch_to_block(present);
            this.builder.seal_block(present);
            this.libc_call("munmap", &[pointer, types::I64], &[types::I32], &[mapped, length]);
            this.builder.ins().jump(next, &[]);
            this.builder.switch_to_block(next);
            this.builder.seal_block(next);
        });
        vec![self.close_and_free(state)]
    }
}
