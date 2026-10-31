//! The microphone and speaker verbs (`docs/audio.md` §3), LLVM: the walk is `docs/device-backends.md` §6-§9, every
//! number `cancho_ir::device`'s. Linux x86_64 only; elsewhere every verb is the constant `-1`.

use crate::*;
use cancho_ir::device::{DEVICE_EAGAIN, DEVICE_EINVAL, DEVICE_EPIPE, alsa};

/// Cell 8 of the audio state block: the rate; cell 16: 1 once configured.
const RATE_AT: i32 = 8;
const CONFIGURED_AT: i32 = 16;

impl FuncEmitter<'_> {
    pub(crate) fn mic_configure(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        self.audio_configure(args)
    }
    pub(crate) fn mic_read(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        self.audio_transfer(args, true)
    }
    pub(crate) fn mic_close(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        self.audio_close(args)
    }
    pub(crate) fn speaker_configure(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        self.audio_configure(args)
    }
    pub(crate) fn speaker_write(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        self.audio_transfer(args, false)
    }
    pub(crate) fn speaker_close(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        self.audio_close(args)
    }

    /// `mic_configure` and `speaker_configure` (`docs/device-backends.md` §6): `hw_params` as libasound's `any`
    /// builds it, then restricted to stereo S16_LE interleaved at the rate, then `PREPARE`.
    fn audio_configure(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if !self.device_supported() {
            return Ok(vec![LValue::Reg("-1".to_string())]);
        }
        let block = self.dev_block_of_ref(&args[0]);
        let fd = self.dev_fd(&block);
        let rate = operand(&args[1]);
        let out = self.dev_cell("0");
        let n = self.dev_n();
        let done = format!("audio.conf.done.{n}");

        // a rate with no meaning (or one that does not fit the interval's 32 bits)
        let below = self.dev_bin("add", "i64", &rate, "-1");
        let bad = self.dev_icmp("uge", "i64", &below, "4294967295");
        self.dev_answer_if(&bad, DEVICE_EINVAL, &out, &done);
        let rate32 = self.fresh();
        self.out.push_str(&format!("  {rate32} = trunc i64 {rate} to i32\n"));

        let params = self.dev_zeroed(alsa::HW_PARAMS_SIZE);
        self.dev_store("i32", "-1", &params, alsa::HW_PARAMS_RMASK_AT);
        // every mask all ones, every interval [0, 0xffffffff]
        self.dev_fill(&params, alsa::HW_PARAMS_MASKS_AT, -1, 3 * alsa::MASK_SIZE);
        for i in 0..12 {
            let at = alsa::HW_PARAMS_INTERVALS_AT + i * alsa::INTERVAL_SIZE as i32;
            self.dev_store("i32", "0", &params, at + alsa::INTERVAL_MIN_AT);
            self.dev_store("i32", "-1", &params, at + alsa::INTERVAL_MAX_AT);
            self.dev_store("i32", "0", &params, at + alsa::INTERVAL_FLAGS_AT);
        }
        // then the single bit of each mask
        for (index, bit) in [
            (alsa::PARAM_ACCESS, alsa::ACCESS_RW_INTERLEAVED),
            (alsa::PARAM_FORMAT, alsa::FORMAT_S16_LE),
            (alsa::PARAM_SUBFORMAT, alsa::SUBFORMAT_STD),
        ] {
            let mask = alsa::HW_PARAMS_MASKS_AT + (index * alsa::MASK_SIZE) as i32;
            self.dev_fill(&params, mask, 0, alsa::MASK_SIZE);
            let word = mask + 4 * (bit / 32) as i32;
            self.dev_store("i32", &(1u32 << (bit % 32)).to_string(), &params, word);
        }
        // the channels and the rate
        for (index, value) in
            [(alsa::PARAM_CHANNELS, alsa::CHANNELS.to_string()), (alsa::PARAM_RATE, rate32)]
        {
            let at = alsa::HW_PARAMS_INTERVALS_AT
                + ((index - alsa::FIRST_INTERVAL) * alsa::INTERVAL_SIZE) as i32;
            self.dev_store("i32", &value, &params, at + alsa::INTERVAL_MIN_AT);
            self.dev_store("i32", &value, &params, at + alsa::INTERVAL_MAX_AT);
        }
        let rc = self.dev_ioctl(&fd, alsa::PCM_IOCTL_HW_PARAMS, &params);
        self.dev_fail_on_neg(&rc, &out, &done);
        let rc = self.dev_ioctl(&fd, alsa::PCM_IOCTL_PREPARE, "null");
        self.dev_fail_on_neg(&rc, &out, &done);
        self.dev_store("i64", &rate, &block, RATE_AT);
        self.dev_store("i64", "1", &block, CONFIGURED_AT);
        self.dev_set(&out, "0");
        self.dev_br(&done);
        self.dev_label(&done);
        Ok(vec![LValue::Reg(self.dev_get(&out))])
    }

    /// `mic_read` (`capture`) and `speaker_write` (`docs/device-backends.md` §7, §8): at most `CHUNK_FRAMES`
    /// frames through a 4 KiB stack buffer, mono 16-bit on the program's side and stereo on the device's.
    fn audio_transfer(&mut self, args: &[LValue], capture: bool) -> Result<Vec<LValue>, String> {
        if !self.device_supported() {
            return Ok(vec![LValue::Reg("-1".to_string())]);
        }
        let block = self.dev_block_of_ref(&args[0]);
        let fd = self.dev_fd(&block);
        let (mono, len) = (operand(&args[1]), operand(&args[2]));
        let out = self.dev_cell("-1");
        let n = self.dev_n();
        let done = format!("audio.xfer.done.{n}");
        let configured = self.dev_load("i64", &block, CONFIGURED_AT);
        let unset = self.dev_icmp("eq", "i64", &configured, "0");
        self.dev_answer_if(&unset, -1, &out, &done);
        let half = self.dev_bin("lshr", "i64", &len, "1");
        let frames = self.dev_umin(&half, &alsa::CHUNK_FRAMES.to_string());
        let nothing = self.dev_icmp("eq", "i64", &frames, "0");
        self.dev_answer_if(&nothing, 0, &out, &done);

        let buf = self.fresh();
        self.hoist(format!(
            "  {buf} = alloca i8, i64 {}, align 4\n",
            alsa::CHUNK_FRAMES * alsa::FRAME_BYTES
        ));
        if !capture {
            // expand each mono sample to two identical ones
            let (index, head, exit, k) = self.dev_loop_begin(&frames);
            let src_at = self.dev_bin("mul", "i64", &k, "2");
            let src = self.dev_gep(&mono, &src_at);
            let sample = self.fresh();
            self.out.push_str(&format!("  {sample} = load i16, ptr {src}, align 1\n"));
            let dst_at = self.dev_bin("mul", "i64", &k, &alsa::FRAME_BYTES.to_string());
            let dst = self.dev_gep(&buf, &dst_at);
            self.out.push_str(&format!("  store i16 {sample}, ptr {dst}, align 2\n"));
            let dst2 = self.dev_gep(&dst, "2");
            self.out.push_str(&format!("  store i16 {sample}, ptr {dst2}, align 2\n"));
            self.dev_loop_end(&index, &head, &exit, &k);
        }
        let xfer = self.dev_zeroed(alsa::XFERI_SIZE);
        let buf_address = self.fresh();
        self.out.push_str(&format!("  {buf_address} = ptrtoint ptr {buf} to i64\n"));
        self.dev_store("i64", &buf_address, &xfer, alsa::XFERI_BUF_AT);
        self.dev_store("i64", &frames, &xfer, alsa::XFERI_FRAMES_AT);
        let request =
            if capture { alsa::PCM_IOCTL_READI_FRAMES } else { alsa::PCM_IOCTL_WRITEI_FRAMES };
        let rc = self.dev_ioctl(&fd, request, &xfer);
        let (fail, moved) = (format!("audio.fail.{n}"), format!("audio.moved.{n}"));
        let (zero, pipe, other) =
            (format!("audio.zero.{n}"), format!("audio.pipe.{n}"), format!("audio.other.{n}"));
        let neg = self.dev_icmp("slt", "i32", &rc, "0");
        self.dev_cond(&neg, &fail, &moved);
        // EAGAIN: nothing moved; EPIPE: an overrun or underrun, so prepare again; anything else is -1
        self.dev_label(&fail);
        let reason = self.errno();
        let reason = operand(&reason);
        let again = self.dev_icmp("eq", "i64", &reason, &DEVICE_EAGAIN.to_string());
        let (test_pipe, _) = (format!("audio.testpipe.{n}"), ());
        self.dev_cond(&again, &zero, &test_pipe);
        self.dev_label(&test_pipe);
        let broken = self.dev_icmp("eq", "i64", &reason, &DEVICE_EPIPE.to_string());
        self.dev_cond(&broken, &pipe, &other);
        self.dev_label(&pipe);
        self.dev_ioctl(&fd, alsa::PCM_IOCTL_PREPARE, "null");
        self.dev_br(&zero);
        self.dev_label(&zero);
        self.dev_set(&out, "0");
        self.dev_br(&done);
        self.dev_label(&other);
        self.dev_set(&out, "-1");
        self.dev_br(&done);

        self.dev_label(&moved);
        let result = self.dev_load("i64", &xfer, alsa::XFERI_RESULT_AT);
        let result = self.dev_umin(&result, &frames);
        if capture {
            // the left sample of each frame, as one mono sample
            let (index, head, exit, k) = self.dev_loop_begin(&result);
            let src_at = self.dev_bin("mul", "i64", &k, &alsa::FRAME_BYTES.to_string());
            let src = self.dev_gep(&buf, &src_at);
            let sample = self.fresh();
            self.out.push_str(&format!("  {sample} = load i16, ptr {src}, align 2\n"));
            let dst_at = self.dev_bin("mul", "i64", &k, "2");
            let dst = self.dev_gep(&mono, &dst_at);
            self.out.push_str(&format!("  store i16 {sample}, ptr {dst}, align 1\n"));
            self.dev_loop_end(&index, &head, &exit, &k);
        }
        let bytes = self.dev_bin("mul", "i64", &result, "2");
        self.dev_set(&out, &bytes);
        self.dev_br(&done);
        self.dev_label(&done);
        Ok(vec![LValue::Reg(self.dev_get(&out))])
    }

    /// `mic_close` and `speaker_close` (`docs/device-backends.md` §9): the handle is the block's address.
    fn audio_close(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if !self.device_supported() {
            return Ok(vec![LValue::Reg("-1".to_string())]);
        }
        let block = self.dev_block(&operand(&args[0]));
        let fd = self.dev_fd(&block);
        let configured = self.dev_load("i64", &block, CONFIGURED_AT);
        let on = self.dev_icmp("ne", "i64", &configured, "0");
        let n = self.dev_n();
        let (drop, release) = (format!("audio.drop.{n}"), format!("audio.release.{n}"));
        self.dev_cond(&on, &drop, &release);
        self.dev_label(&drop);
        self.dev_ioctl(&fd, alsa::PCM_IOCTL_DROP, "null");
        self.dev_br(&release);
        self.dev_label(&release);
        Ok(vec![self.dev_close_and_free(&fd, &block)])
    }
}
