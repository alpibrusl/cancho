//! The camera verbs (`docs/camera.md` §3), LLVM: the walk is `docs/device-backends.md` §2-§5, every number
//! `cancho_ir::device`'s. Linux x86_64 only; elsewhere the open answers `Failed(ENODEV)` and the rest are constants.

use crate::*;
use cancho_ir::Device;
use cancho_ir::device::{DEVICE_EAGAIN, DEVICE_EINVAL, DEVICE_ENODEV, DEVICE_OPEN_FLAGS, v4l2};

impl FuncEmitter<'_> {
    /// `camera_open`, `mic_open` and `speaker_open` (`Expr::DeviceOpen`): answers the three leaves of the
    /// `*Opened` enum -- tag (`Ok` 0, `Failed` 1), the handle, and the `errno`. The handle is the address of a
    /// zeroed state block, obtained only once the descriptor is good.
    pub(crate) fn device_open(
        &mut self,
        _device: Device,
        prefix: &str,
        args: &[Expr],
    ) -> Result<Vec<LValue>, String> {
        if !self.device_supported() {
            return Ok(vec![
                LValue::Reg("1".to_string()),
                LValue::Reg("-1".to_string()),
                LValue::Reg(DEVICE_ENODEV.to_string()),
            ]);
        }
        let path = self.expr(&args[1])?;
        let path = self.checked_path(prefix, &path)?;
        let fd32 = self.open_at_cwd(&path, DEVICE_OPEN_FLAGS, 0);
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = sext i32 {fd32} to i64\n"));
        // the errno before any other call
        let reason = self.errno();
        let handle = self.dev_cell(&fd);
        let failed = self.dev_icmp("slt", "i64", &fd, "0");
        let n = self.dev_n();
        let (good, done) = (format!("dev.open.good.{n}"), format!("dev.open.done.{n}"));
        self.dev_cond(&failed, &done, &good);
        self.dev_label(&good);
        let block = self.fresh();
        self.out
            .push_str(&format!("  {block} = call ptr @calloc(i64 1, i64 {})\n", v4l2::STATE_SIZE));
        self.dev_store("i64", &fd, &block, v4l2::STATE_FD_AT);
        let address = self.fresh();
        self.out.push_str(&format!("  {address} = ptrtoint ptr {block} to i64\n"));
        self.dev_set(&handle, &address);
        self.dev_br(&done);
        self.dev_label(&done);
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        let handle = self.dev_get(&handle);
        Ok(vec![LValue::Reg(tag), LValue::Reg(handle), reason])
    }

    /// `camera_configure(&!Cam, width, height) -> int` (`docs/device-backends.md` §3).
    pub(crate) fn camera_configure(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if !self.device_supported() {
            return Ok(vec![LValue::Reg("-1".to_string())]);
        }
        let block = self.dev_block_of_ref(&args[0]);
        let fd = self.dev_fd(&block);
        let (width, height) = (operand(&args[1]), operand(&args[2]));
        let out = self.dev_cell("0");
        let n = self.dev_n();
        let done = format!("cam.conf.done.{n}");

        // a size with no meaning (or one that does not fit the driver's 32 bits), or a second configure
        let w_less = self.dev_bin("add", "i64", &width, "-1");
        let w_bad = self.dev_icmp("uge", "i64", &w_less, "2147483647");
        let h_less = self.dev_bin("add", "i64", &height, "-1");
        let h_bad = self.dev_icmp("uge", "i64", &h_less, "2147483647");
        let seen = self.dev_load("i64", &block, v4l2::STATE_WIDTH_AT);
        let again = self.dev_icmp("ne", "i64", &seen, "0");
        let bad = self.dev_bin("or", "i1", &w_bad, &h_bad);
        let bad = self.dev_bin("or", "i1", &bad, &again);
        self.dev_answer_if(&bad, DEVICE_EINVAL, &out, &done);

        // S_FMT
        let w32 = self.fresh();
        self.out.push_str(&format!("  {w32} = trunc i64 {width} to i32\n"));
        let h32 = self.fresh();
        self.out.push_str(&format!("  {h32} = trunc i64 {height} to i32\n"));
        let fmt = self.dev_zeroed(v4l2::FORMAT_SIZE);
        self.dev_store("i32", &v4l2::BUF_TYPE_CAPTURE.to_string(), &fmt, v4l2::FORMAT_TYPE_AT);
        self.dev_store("i32", &w32, &fmt, v4l2::FORMAT_WIDTH_AT);
        self.dev_store("i32", &h32, &fmt, v4l2::FORMAT_HEIGHT_AT);
        self.dev_store(
            "i32",
            &(v4l2::PIX_FMT_YUYV as i32).to_string(),
            &fmt,
            v4l2::FORMAT_PIXELFORMAT_AT,
        );
        self.dev_store("i32", &v4l2::FIELD_NONE.to_string(), &fmt, v4l2::FORMAT_FIELD_AT);
        let rc = self.dev_ioctl(&fd, v4l2::VIDIOC_S_FMT, &fmt);
        self.dev_fail_on_neg(&rc, &out, &done);

        // the driver may offer another size
        let got_w = self.dev_load("i32", &fmt, v4l2::FORMAT_WIDTH_AT);
        let got_h = self.dev_load("i32", &fmt, v4l2::FORMAT_HEIGHT_AT);
        let w_ne = self.dev_icmp("ne", "i32", &got_w, &w32);
        let h_ne = self.dev_icmp("ne", "i32", &got_h, &h32);
        let differs = self.dev_bin("or", "i1", &w_ne, &h_ne);
        self.dev_answer_if(&differs, DEVICE_EINVAL, &out, &done);
        self.dev_store("i64", &width, &block, v4l2::STATE_WIDTH_AT);
        self.dev_store("i64", &height, &block, v4l2::STATE_HEIGHT_AT);

        // REQBUFS
        let req = self.dev_zeroed(v4l2::REQBUFS_SIZE);
        self.dev_store("i32", &v4l2::BUFFERS.to_string(), &req, v4l2::REQBUFS_COUNT_AT);
        self.dev_store("i32", &v4l2::BUF_TYPE_CAPTURE.to_string(), &req, v4l2::REQBUFS_TYPE_AT);
        self.dev_store("i32", &v4l2::MEMORY_MMAP.to_string(), &req, v4l2::REQBUFS_MEMORY_AT);
        let rc = self.dev_ioctl(&fd, v4l2::VIDIOC_REQBUFS, &req);
        self.dev_fail_on_neg(&rc, &out, &done);
        let granted32 = self.dev_load("i32", &req, v4l2::REQBUFS_COUNT_AT);
        let granted = self.fresh();
        self.out.push_str(&format!("  {granted} = zext i32 {granted32} to i64\n"));
        let none = self.dev_icmp("eq", "i64", &granted, "0");
        self.dev_answer_if(&none, DEVICE_EINVAL, &out, &done);
        self.dev_store("i64", &granted, &block, v4l2::STATE_COUNT_AT);
        let count = self.dev_umin(&granted, &v4l2::BUFFERS.to_string());

        // QUERYBUF, mmap and QBUF for each buffer
        let buf = self.dev_zeroed(v4l2::BUFFER_SIZE);
        let (index, head, exit, i) = self.dev_loop_begin(&count);
        self.dev_fill(&buf, 0, 0, v4l2::BUFFER_SIZE);
        let i32v = self.fresh();
        self.out.push_str(&format!("  {i32v} = trunc i64 {i} to i32\n"));
        self.dev_store("i32", &i32v, &buf, v4l2::BUFFER_INDEX_AT);
        self.dev_store("i32", &v4l2::BUF_TYPE_CAPTURE.to_string(), &buf, v4l2::BUFFER_TYPE_AT);
        self.dev_store("i32", &v4l2::MEMORY_MMAP.to_string(), &buf, v4l2::BUFFER_MEMORY_AT);
        let rc = self.dev_ioctl(&fd, v4l2::VIDIOC_QUERYBUF, &buf);
        self.dev_fail_on_neg(&rc, &out, &done);
        let length32 = self.dev_load("i32", &buf, v4l2::BUFFER_LENGTH_AT);
        let length = self.fresh();
        self.out.push_str(&format!("  {length} = zext i32 {length32} to i64\n"));
        let offset32 = self.dev_load("i32", &buf, v4l2::BUFFER_OFFSET_AT);
        let offset = self.fresh();
        self.out.push_str(&format!("  {offset} = zext i32 {offset32} to i64\n"));
        let mapped = self.fresh();
        self.out.push_str(&format!(
            "  {mapped} = call ptr @mmap(ptr null, i64 {length}, i32 3, i32 1, i32 {fd}, i64 {offset})\n"
        ));
        let address = self.fresh();
        self.out.push_str(&format!("  {address} = ptrtoint ptr {mapped} to i64\n"));
        let failed = self.dev_icmp("eq", "i64", &address, "-1");
        let (mfail, mok) = (format!("cam.mmapfail.{n}"), format!("cam.mmapok.{n}"));
        self.dev_cond(&failed, &mfail, &mok);
        self.dev_label(&mfail);
        let reason = self.errno();
        self.dev_set(&out, &operand(&reason));
        self.dev_br(&done);
        self.dev_label(&mok);
        let slot = self.dev_bin("mul", "i64", &i, "8");
        let addr_at = self.dev_bin("add", "i64", &slot, &v4l2::STATE_ADDR_AT.to_string());
        let addr_p = self.dev_gep(&block, &addr_at);
        self.out.push_str(&format!("  store i64 {address}, ptr {addr_p}\n"));
        let len_at = self.dev_bin("add", "i64", &slot, &v4l2::STATE_LEN_AT.to_string());
        let len_p = self.dev_gep(&block, &len_at);
        self.out.push_str(&format!("  store i64 {length}, ptr {len_p}\n"));
        let rc = self.dev_ioctl(&fd, v4l2::VIDIOC_QBUF, &buf);
        self.dev_fail_on_neg(&rc, &out, &done);
        self.dev_loop_end(&index, &head, &exit, &i);

        // STREAMON
        let kind = self.fresh();
        self.hoist(format!("  {kind} = alloca i32\n"));
        self.out.push_str(&format!("  store i32 {}, ptr {kind}\n", v4l2::BUF_TYPE_CAPTURE));
        let rc = self.dev_ioctl(&fd, v4l2::VIDIOC_STREAMON, &kind);
        self.dev_fail_on_neg(&rc, &out, &done);
        self.dev_set(&out, "0");
        self.dev_br(&done);
        self.dev_label(&done);
        Ok(vec![LValue::Reg(self.dev_get(&out))])
    }

    /// `camera_next(&!Cam, into: &![byte]) -> int` (`docs/device-backends.md` §4).
    pub(crate) fn camera_next(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if !self.device_supported() {
            return Ok(vec![LValue::Reg("-1".to_string())]);
        }
        let block = self.dev_block_of_ref(&args[0]);
        let fd = self.dev_fd(&block);
        let (into, len) = (operand(&args[1]), operand(&args[2]));
        let out = self.dev_cell("-1");
        let n = self.dev_n();
        let done = format!("cam.next.done.{n}");
        let width = self.dev_load("i64", &block, v4l2::STATE_WIDTH_AT);
        let height = self.dev_load("i64", &block, v4l2::STATE_HEIGHT_AT);
        let unset = self.dev_icmp("eq", "i64", &width, "0");
        self.dev_answer_if(&unset, -1, &out, &done);

        // drain: the newest frame wins, an older one is given back
        let have = self.dev_cell("-1");
        let why = self.dev_cell("0");
        let buf = self.dev_zeroed(v4l2::BUFFER_SIZE);
        let back = self.dev_zeroed(v4l2::BUFFER_SIZE);
        let (head, body, got, drained) = (
            format!("cam.drain.{n}"),
            format!("cam.dq.{n}"),
            format!("cam.got.{n}"),
            format!("cam.drained.{n}"),
        );
        let (again, empty) = (format!("cam.again.{n}"), format!("cam.noframe.{n}"));
        self.dev_br(&head);
        self.dev_label(&head);
        self.dev_fill(&buf, 0, 0, v4l2::BUFFER_SIZE);
        self.dev_store("i32", &v4l2::BUF_TYPE_CAPTURE.to_string(), &buf, v4l2::BUFFER_TYPE_AT);
        self.dev_store("i32", &v4l2::MEMORY_MMAP.to_string(), &buf, v4l2::BUFFER_MEMORY_AT);
        let rc = self.dev_ioctl(&fd, v4l2::VIDIOC_DQBUF, &buf);
        let neg = self.dev_icmp("slt", "i32", &rc, "0");
        self.dev_cond(&neg, &empty, &body);
        self.dev_label(&empty);
        let reason = self.errno();
        self.dev_set(&why, &operand(&reason));
        self.dev_br(&drained);
        self.dev_label(&body);
        let old = self.dev_get(&have);
        let has_old = self.dev_icmp("sge", "i64", &old, "0");
        self.dev_cond(&has_old, &again, &got);
        self.dev_label(&again);
        self.dev_fill(&back, 0, 0, v4l2::BUFFER_SIZE);
        let old32 = self.fresh();
        self.out.push_str(&format!("  {old32} = trunc i64 {old} to i32\n"));
        self.dev_store("i32", &old32, &back, v4l2::BUFFER_INDEX_AT);
        self.dev_store("i32", &v4l2::BUF_TYPE_CAPTURE.to_string(), &back, v4l2::BUFFER_TYPE_AT);
        self.dev_store("i32", &v4l2::MEMORY_MMAP.to_string(), &back, v4l2::BUFFER_MEMORY_AT);
        self.dev_ioctl(&fd, v4l2::VIDIOC_QBUF, &back);
        self.dev_br(&got);
        self.dev_label(&got);
        let index32 = self.dev_load("i32", &buf, v4l2::BUFFER_INDEX_AT);
        let index = self.fresh();
        self.out.push_str(&format!("  {index} = zext i32 {index32} to i64\n"));
        self.dev_set(&have, &index);
        self.dev_br(&head);

        self.dev_label(&drained);
        let have_v = self.dev_get(&have);
        let none = self.dev_icmp("slt", "i64", &have_v, "0");
        let (nothing, convert) = (format!("cam.nothing.{n}"), format!("cam.convert.{n}"));
        self.dev_cond(&none, &nothing, &convert);
        self.dev_label(&nothing);
        let why_v = self.dev_get(&why);
        let is_again = self.dev_icmp("eq", "i64", &why_v, &DEVICE_EAGAIN.to_string());
        let answer = self.fresh();
        self.out.push_str(&format!("  {answer} = select i1 {is_again}, i64 0, i64 -1\n"));
        self.dev_set(&out, &answer);
        self.dev_br(&done);

        // the grey byte of pixel k is byte 2k of the YUYV frame
        self.dev_label(&convert);
        let pixels = self.dev_bin("mul", "i64", &width, &height);
        let count = self.dev_umin(&len, &pixels);
        let slot = self.dev_bin("mul", "i64", &have_v, "8");
        let addr_at = self.dev_bin("add", "i64", &slot, &v4l2::STATE_ADDR_AT.to_string());
        let addr_p = self.dev_gep(&block, &addr_at);
        let mapped = self.fresh();
        self.out.push_str(&format!("  {mapped} = load ptr, ptr {addr_p}\n"));
        let (index, head2, exit, k) = self.dev_loop_begin(&count);
        let src_at = self.dev_bin("mul", "i64", &k, "2");
        let src = self.dev_gep(&mapped, &src_at);
        let grey = self.fresh();
        self.out.push_str(&format!("  {grey} = load i8, ptr {src}\n"));
        let dst = self.dev_gep(&into, &k);
        self.out.push_str(&format!("  store i8 {grey}, ptr {dst}\n"));
        self.dev_loop_end(&index, &head2, &exit, &k);
        self.dev_fill(&back, 0, 0, v4l2::BUFFER_SIZE);
        let have32 = self.fresh();
        self.out.push_str(&format!("  {have32} = trunc i64 {have_v} to i32\n"));
        self.dev_store("i32", &have32, &back, v4l2::BUFFER_INDEX_AT);
        self.dev_store("i32", &v4l2::BUF_TYPE_CAPTURE.to_string(), &back, v4l2::BUFFER_TYPE_AT);
        self.dev_store("i32", &v4l2::MEMORY_MMAP.to_string(), &back, v4l2::BUFFER_MEMORY_AT);
        self.dev_ioctl(&fd, v4l2::VIDIOC_QBUF, &back);
        self.dev_set(&out, &count);
        self.dev_br(&done);
        self.dev_label(&done);
        Ok(vec![LValue::Reg(self.dev_get(&out))])
    }

    /// `camera_close(Cam) -> int` (`docs/device-backends.md` §5): the handle is the block's address.
    pub(crate) fn camera_close(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if !self.device_supported() {
            return Ok(vec![LValue::Reg("-1".to_string())]);
        }
        let block = self.dev_block(&operand(&args[0]));
        let fd = self.dev_fd(&block);
        let width = self.dev_load("i64", &block, v4l2::STATE_WIDTH_AT);
        let configured = self.dev_icmp("ne", "i64", &width, "0");
        let n = self.dev_n();
        let (stop, release) = (format!("cam.stop.{n}"), format!("cam.release.{n}"));
        self.dev_cond(&configured, &stop, &release);
        self.dev_label(&stop);
        let kind = self.fresh();
        self.hoist(format!("  {kind} = alloca i32\n"));
        self.out.push_str(&format!("  store i32 {}, ptr {kind}\n", v4l2::BUF_TYPE_CAPTURE));
        self.dev_ioctl(&fd, v4l2::VIDIOC_STREAMOFF, &kind);
        let granted = self.dev_load("i64", &block, v4l2::STATE_COUNT_AT);
        let count = self.dev_umin(&granted, &v4l2::BUFFERS.to_string());
        let (index, head, exit, i) = self.dev_loop_begin(&count);
        let slot = self.dev_bin("mul", "i64", &i, "8");
        let addr_at = self.dev_bin("add", "i64", &slot, &v4l2::STATE_ADDR_AT.to_string());
        let addr_p = self.dev_gep(&block, &addr_at);
        let len_at = self.dev_bin("add", "i64", &slot, &v4l2::STATE_LEN_AT.to_string());
        let len_p = self.dev_gep(&block, &len_at);
        let mapped = self.fresh();
        self.out.push_str(&format!("  {mapped} = load ptr, ptr {addr_p}\n"));
        let length = self.fresh();
        self.out.push_str(&format!("  {length} = load i64, ptr {len_p}\n"));
        let recorded = self.fresh();
        self.out.push_str(&format!("  {recorded} = icmp ne ptr {mapped}, null\n"));
        let (unmap, skip) = (format!("cam.unmap.{n}"), format!("cam.skip.{n}"));
        self.dev_cond(&recorded, &unmap, &skip);
        self.dev_label(&unmap);
        self.out.push_str(&format!("  call i32 @munmap(ptr {mapped}, i64 {length})\n"));
        self.dev_br(&skip);
        self.dev_label(&skip);
        self.dev_loop_end(&index, &head, &exit, &i);
        self.dev_br(&release);
        self.dev_label(&release);
        Ok(vec![self.dev_close_and_free(&fd, &block)])
    }
}
