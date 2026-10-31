//! The pieces the camera, microphone and speaker verbs share (`docs/device-backends.md`), LLVM: the state block,
//! struct field access, the `ioctl` call and the labelled control flow the walks are written in. Every number
//! is `cancho_ir::device`'s. Only Linux x86_64 is measured, so only there do the verbs touch a device.

use crate::*;
use target_lexicon::{Architecture, OperatingSystem};

impl FuncEmitter<'_> {
    /// Whether the V4L2 and ALSA numbers are measured for the target being compiled for.
    pub(crate) fn device_supported(&self) -> bool {
        self.triple.operating_system == OperatingSystem::Linux
            && self.triple.architecture == Architecture::X86_64
    }

    /// A number for labels, unique within the function.
    pub(crate) fn dev_n(&mut self) -> String {
        self.fresh().trim_start_matches('%').to_string()
    }

    pub(crate) fn dev_label(&mut self, name: &str) {
        self.out.push_str(&format!("{name}:\n"));
    }

    pub(crate) fn dev_br(&mut self, name: &str) {
        self.out.push_str(&format!("  br label %{name}\n"));
    }

    pub(crate) fn dev_cond(&mut self, cond: &str, yes: &str, no: &str) {
        self.out.push_str(&format!("  br i1 {cond}, label %{yes}, label %{no}\n"));
    }

    /// An `i64` cell in the entry block holding `value` from here on.
    pub(crate) fn dev_cell(&mut self, value: &str) -> String {
        let cell = self.fresh();
        self.hoist(format!("  {cell} = alloca i64\n"));
        self.out.push_str(&format!("  store i64 {value}, ptr {cell}\n"));
        cell
    }

    pub(crate) fn dev_get(&mut self, cell: &str) -> String {
        let v = self.fresh();
        self.out.push_str(&format!("  {v} = load i64, ptr {cell}\n"));
        v
    }

    pub(crate) fn dev_set(&mut self, cell: &str, value: &str) {
        self.out.push_str(&format!("  store i64 {value}, ptr {cell}\n"));
    }

    /// `base + at` (`at` an operand: a constant or a register).
    pub(crate) fn dev_gep(&mut self, base: &str, at: &str) -> String {
        let p = self.fresh();
        self.out.push_str(&format!("  {p} = getelementptr inbounds i8, ptr {base}, i64 {at}\n"));
        p
    }

    pub(crate) fn dev_load(&mut self, ty: &str, base: &str, at: i32) -> String {
        let p = self.dev_gep(base, &at.to_string());
        let v = self.fresh();
        self.out.push_str(&format!("  {v} = load {ty}, ptr {p}\n"));
        v
    }

    pub(crate) fn dev_store(&mut self, ty: &str, value: &str, base: &str, at: i32) {
        let p = self.dev_gep(base, &at.to_string());
        self.out.push_str(&format!("  store {ty} {value}, ptr {p}\n"));
    }

    /// A zeroed, 8-aligned struct of `size` bytes, its `alloca` in the entry block and the zeroing here.
    pub(crate) fn dev_zeroed(&mut self, size: usize) -> String {
        let p = self.fresh();
        self.hoist(format!("  {p} = alloca i8, i64 {size}, align 8\n"));
        self.dev_fill(&p, 0, 0, size);
        p
    }

    /// `memset(base + at, byte, size)`.
    pub(crate) fn dev_fill(&mut self, base: &str, at: i32, byte: i32, size: usize) {
        let p = self.dev_gep(base, &at.to_string());
        self.out.push_str(&format!("  call ptr @memset(ptr {p}, i32 {byte}, i64 {size})\n"));
    }

    /// The state block a handle cell (a `&!Handle`) points to.
    pub(crate) fn dev_block_of_ref(&mut self, reference: &LValue) -> String {
        let value = self.fresh();
        self.out.push_str(&format!("  {value} = load i64, ptr {}\n", operand(reference)));
        self.dev_block(&value)
    }

    /// The state block a handle value is.
    pub(crate) fn dev_block(&mut self, value: &str) -> String {
        let p = self.fresh();
        self.out.push_str(&format!("  {p} = inttoptr i64 {value} to ptr\n"));
        p
    }

    /// The descriptor in cell 0 of the block, narrowed for libc.
    pub(crate) fn dev_fd(&mut self, block: &str) -> String {
        let fd64 = self.dev_load("i64", block, 0);
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = trunc i64 {fd64} to i32\n"));
        fd
    }

    /// `ioctl(fd, request, arg)`, the `i32` result. Linux's `ioctl` reads the request as an `unsigned long`, so
    /// it is passed as a zero-extended `i64`: the call-site type says so whatever the shared declaration is.
    pub(crate) fn dev_ioctl(&mut self, fd: &str, request: i64, arg: &str) -> String {
        let rc = self.fresh();
        self.out.push_str(&format!(
            "  {rc} = call i32 (i32, i64, ...) @ioctl(i32 {fd}, i64 {request}, ptr {arg})\n"
        ));
        rc
    }

    /// After a call whose `i32` result is `rc`: when negative, store its `errno` in `out` and go to `done`.
    pub(crate) fn dev_fail_on_neg(&mut self, rc: &str, out: &str, done: &str) {
        let n = self.dev_n();
        let (fail, ok) = (format!("dev.fail.{n}"), format!("dev.ok.{n}"));
        let neg = self.fresh();
        self.out.push_str(&format!("  {neg} = icmp slt i32 {rc}, 0\n"));
        self.dev_cond(&neg, &fail, &ok);
        self.dev_label(&fail);
        let reason = self.errno();
        self.dev_set(out, &operand(&reason));
        self.dev_br(done);
        self.dev_label(&ok);
    }

    /// `if cond { out = value; goto done }`.
    pub(crate) fn dev_answer_if(&mut self, cond: &str, value: i64, out: &str, done: &str) {
        let n = self.dev_n();
        let (yes, no) = (format!("dev.yes.{n}"), format!("dev.no.{n}"));
        self.dev_cond(cond, &yes, &no);
        self.dev_label(&yes);
        self.dev_set(out, &value.to_string());
        self.dev_br(done);
        self.dev_label(&no);
    }

    pub(crate) fn dev_icmp(&mut self, cmp: &str, ty: &str, a: &str, b: &str) -> String {
        let r = self.fresh();
        self.out.push_str(&format!("  {r} = icmp {cmp} {ty} {a}, {b}\n"));
        r
    }

    pub(crate) fn dev_bin(&mut self, op: &str, ty: &str, a: &str, b: &str) -> String {
        let r = self.fresh();
        self.out.push_str(&format!("  {r} = {op} {ty} {a}, {b}\n"));
        r
    }

    /// Unsigned `min` of two `i64`s.
    pub(crate) fn dev_umin(&mut self, a: &str, b: &str) -> String {
        let lt = self.dev_icmp("ult", "i64", a, b);
        let r = self.fresh();
        self.out.push_str(&format!("  {r} = select i1 {lt}, i64 {a}, i64 {b}\n"));
        r
    }

    /// `close(fd)`, `free(block)`, the close's result widened: what every `*_close` ends with.
    pub(crate) fn dev_close_and_free(&mut self, fd: &str, block: &str) -> LValue {
        let closed = self.fresh();
        self.out.push_str(&format!("  {closed} = call i32 @close(i32 {fd})\n"));
        self.out.push_str(&format!("  call void @free(ptr {block})\n"));
        let wide = self.fresh();
        self.out.push_str(&format!("  {wide} = sext i32 {closed} to i64\n"));
        LValue::Reg(wide)
    }

    /// Open a counted loop `for i in 0..bound` (an `i64` operand): answers the index cell and the labels
    /// `(index, body, exit)`; the caller writes the body, then calls [`Self::dev_loop_end`].
    pub(crate) fn dev_loop_begin(&mut self, bound: &str) -> (String, String, String, String) {
        let n = self.dev_n();
        let (head, body, exit) =
            (format!("dev.head.{n}"), format!("dev.body.{n}"), format!("dev.exit.{n}"));
        let index = self.dev_cell("0");
        self.dev_br(&head);
        self.dev_label(&head);
        let i = self.dev_get(&index);
        let more = self.dev_icmp("slt", "i64", &i, bound);
        self.dev_cond(&more, &body, &exit);
        self.dev_label(&body);
        (index, head, exit, i)
    }

    pub(crate) fn dev_loop_end(&mut self, index: &str, head: &str, exit: &str, i: &str) {
        let next = self.dev_bin("add", "i64", i, "1");
        self.dev_set(index, &next);
        self.dev_br(head);
        self.dev_label(exit);
    }
}
