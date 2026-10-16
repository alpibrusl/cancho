//! `cancho_child_reaper` (`docs/processes.md` §4.11): the function a
//! released child's pid is handed to. `void *reap(void *pid)` makes one
//! `waitpid` on that pid alone, so it never takes another child's status from
//! a `child_wait`, and ends. `child_release` starts it on a detached thread.
//! `cancho-codegen-llvm` writes the same function as IR text.
//!
//! No retry on `EINTR`: the program installs no handler (a claimed signal is
//! blocked or ignored, `docs/signals.md` §5), so nothing interrupts the call.

use cancho_ir::CHILD_REAPER;
use cranelift_codegen::ir::{Function, UserFuncName};

use crate::*;

/// Declared local to this object (idempotent).
pub(crate) fn declare(module: &mut ObjectModule) -> FuncId {
    let pointer = module.isa().pointer_type();
    let mut sig = module.make_signature();
    sig.call_conv = module.isa().default_call_conv();
    sig.params.push(AbiParam::new(pointer));
    sig.returns.push(AbiParam::new(pointer));
    module
        .declare_function(CHILD_REAPER, Linkage::Local, &sig)
        .expect("the reaper is declared with one signature")
}

impl<'a> Emitter<'a> {
    /// Whether a body started the reaper, so that it is defined.
    pub(crate) fn reaper_used(&self) -> bool {
        self.module.declarations().get_name(CHILD_REAPER).is_some()
    }

    /// Define `cancho_child_reaper`.
    pub(crate) fn define_reaper(
        &mut self,
        ctx: &mut Context,
        fb_ctx: &mut FunctionBuilderContext,
    ) -> Result<(), CodegenError> {
        let pointer = self.module.isa().pointer_type();
        let mut waitpid_sig = self.module.make_signature();
        waitpid_sig.call_conv = self.module.isa().default_call_conv();
        waitpid_sig.params.push(AbiParam::new(types::I32));
        waitpid_sig.params.push(AbiParam::new(pointer));
        waitpid_sig.params.push(AbiParam::new(types::I32));
        waitpid_sig.returns.push(AbiParam::new(types::I32));
        let waitpid = self
            .module
            .declare_function("waitpid", Linkage::Import, &waitpid_sig)
            .map_err(|e| CodegenError::plain(e.to_string()))?;

        let id = declare(&mut self.module);
        ctx.clear();
        ctx.func = Function::with_name_signature(
            UserFuncName::default(),
            self.module.declarations().get_function_decl(id).signature.clone(),
        );
        {
            let mut b = FunctionBuilder::new(&mut ctx.func, fb_ctx);
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let pid = b.block_params(entry)[0];
            let pid = b.ins().ireduce(types::I32, pid);
            let slot =
                b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let status = b.ins().stack_addr(pointer, slot, 0);
            let none = b.ins().iconst(types::I32, 0);
            let f = self.module.declare_func_in_func(waitpid, b.func);
            b.ins().call(f, &[pid, status, none]);
            let null = b.ins().iconst(pointer, 0);
            b.ins().return_(&[null]);
            b.seal_all_blocks();
            b.finalize();
        }
        self.module.define_function(id, ctx).map_err(|e| CodegenError::plain(e.to_string()))
    }
}
