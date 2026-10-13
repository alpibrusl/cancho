//! The per-thread stack of region chunks (`docs/region-chunk-cache.md` §3):
//! `region` takes its chunk from `cancho_region_take` and gives it back to
//! `cancho_region_give`, which clears what the region used and keeps the
//! chunk for the thread's next region, up to
//! `cancho_ir::REGION_CACHE_SLOTS`. `cancho-codegen-llvm`'s own
//! `region_cache.rs` writes the same four functions as IR text.
//!
//! A thread's stack is found through a `pthread` key, made by `main` before
//! any cancho code runs; the key's destructor frees a thread's chunks when
//! it ends. The key is kept plus one in a zeroed 8-byte global, so zero means
//! "no key" (a failed `pthread_key_create` leaves every region as it was,
//! `malloc` in and `free` out). `pthread_key_t` is 4 bytes on glibc and 8 on
//! Darwin; it crosses here as a full register, of which the callee reads its
//! width, as `memchr`'s byte does (`body/memory.rs`).

use cancho_ir::{
    REGION_CACHE_BYTES, REGION_CACHE_GLOBAL, REGION_CACHE_SLOTS, REGION_DROP, REGION_GIVE,
    REGION_INIT, REGION_TAKE,
};
use cranelift_codegen::ir::{Function, UserFuncName};

use crate::*;

/// The signatures of the four functions, by name.
fn signature(module: &ObjectModule, name: &str) -> cranelift_codegen::ir::Signature {
    let pointer = module.isa().pointer_type();
    let mut sig = module.make_signature();
    sig.call_conv = module.isa().default_call_conv();
    match name {
        REGION_TAKE => sig.returns.push(AbiParam::new(pointer)),
        REGION_GIVE => {
            sig.params.push(AbiParam::new(pointer));
            sig.params.push(AbiParam::new(pointer));
        }
        REGION_DROP => sig.params.push(AbiParam::new(pointer)),
        _ => {}
    }
    sig
}

/// One of the four, declared local to this object (idempotent).
fn declare(module: &mut ObjectModule, name: &str) -> FuncId {
    let sig = signature(module, name);
    module
        .declare_function(name, Linkage::Local, &sig)
        .expect("the region runtime's own functions are declared consistently")
}

/// A libc import, as `BodyEmitter::libc_fn` declares one.
fn libc(
    module: &mut ObjectModule,
    name: &str,
    params: &[types::Type],
    returns: &[types::Type],
) -> Result<FuncId, CodegenError> {
    let mut sig = module.make_signature();
    sig.call_conv = module.isa().default_call_conv();
    sig.params.extend(params.iter().map(|p| AbiParam::new(*p)));
    sig.returns.extend(returns.iter().map(|r| AbiParam::new(*r)));
    module
        .declare_function(name, Linkage::Import, &sig)
        .map_err(|e| CodegenError::plain(e.to_string()))
}

impl<'a, 'f> BodyEmitter<'a, 'f> {
    /// A region's chunk: the thread's last one given back, or a fresh
    /// `malloc`; null if there is no memory, which the caller traps on.
    pub(crate) fn region_take(&mut self) -> Value {
        let id = declare(self.module, REGION_TAKE);
        let f = self.module.declare_func_in_func(id, self.builder.func);
        let call = self.builder.ins().call(f, &[]);
        self.builder.inst_results(call)[0]
    }

    /// The end of a region whose chunk is `base` and whose bump pointer is `end`.
    pub(crate) fn region_give(&mut self, base: Value, end: Value) {
        let id = declare(self.module, REGION_GIVE);
        let f = self.module.declare_func_in_func(id, self.builder.func);
        self.builder.ins().call(f, &[base, end]);
    }
}

impl<'a> Emitter<'a> {
    /// Whether a body called the runtime, so that `main` makes the key and
    /// the four functions are defined.
    pub(crate) fn regions_used(&self) -> bool {
        self.module.declarations().get_name(REGION_TAKE).is_some()
    }

    /// Define the key's global and the four functions.
    pub(crate) fn define_region_cache(
        &mut self,
        ctx: &mut Context,
        fb_ctx: &mut FunctionBuilderContext,
    ) -> Result<(), CodegenError> {
        let pointer = self.module.isa().pointer_type();
        let global = self
            .module
            .declare_data(REGION_CACHE_GLOBAL, Linkage::Local, true, false)
            .map_err(|e| CodegenError::plain(e.to_string()))?;
        let mut description = DataDescription::new();
        description.define_zeroinit(8);
        description.set_align(WORD_ALIGN);
        self.module
            .define_data(global, &description)
            .map_err(|e| CodegenError::plain(e.to_string()))?;

        let malloc = libc(&mut self.module, "malloc", &[pointer], &[pointer])?;
        let free = libc(&mut self.module, "free", &[pointer], &[])?;
        let memset = libc(&mut self.module, "memset", &[pointer, types::I32, pointer], &[pointer])?;
        let getspecific = libc(&mut self.module, "pthread_getspecific", &[types::I64], &[pointer])?;
        let setspecific =
            libc(&mut self.module, "pthread_setspecific", &[types::I64, pointer], &[types::I32])?;
        let key_create =
            libc(&mut self.module, "pthread_key_create", &[pointer, pointer], &[types::I32])?;

        // take
        self.define(REGION_TAKE, ctx, fb_ctx, |module, b| {
            let entry = b.create_block();
            let (lookup, count, pop, fresh) =
                (b.create_block(), b.create_block(), b.create_block(), b.create_block());
            b.switch_to_block(entry);
            let k1 = load_key(module, b, global, pointer);
            b.ins().brif(k1, lookup, &[], fresh, &[]);

            b.switch_to_block(lookup);
            let key = b.ins().iadd_imm(k1, -1);
            let c = call1(module, b, getspecific, &[key]);
            b.ins().brif(c, count, &[], fresh, &[]);

            b.switch_to_block(count);
            let n = b.ins().load(types::I64, MemFlags::trusted(), c, 0);
            b.ins().brif(n, pop, &[], fresh, &[]);

            b.switch_to_block(pop);
            let m = b.ins().iadd_imm(n, -1);
            b.ins().store(MemFlags::trusted(), m, c, 0);
            let offset = b.ins().ishl_imm(n, 3);
            let at = b.ins().iadd(c, offset);
            let p = b.ins().load(pointer, MemFlags::trusted(), at, 0);
            b.ins().return_(&[p]);

            b.switch_to_block(fresh);
            let size = b.ins().iconst(pointer, ARENA_CHUNK);
            let q = call1(module, b, malloc, &[size]);
            b.ins().return_(&[q]);
        })?;

        // give
        self.define(REGION_GIVE, ctx, fb_ctx, |module, b| {
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            let blocks: Vec<_> = (0..8).map(|_| b.create_block()).collect();
            let [lookup, make, made_ok, unmade, have, push, keep, release] = blocks[..] else {
                unreachable!("eight blocks were made")
            };
            b.append_block_param(push, pointer);

            b.switch_to_block(entry);
            let base = b.block_params(entry)[0];
            let end = b.block_params(entry)[1];
            // Every byte the region handed out, cleared before the chunk is
            // kept or freed (`docs/region-chunk-cache.md` §3.1).
            let used = b.ins().isub(end, base);
            let zero = b.ins().iconst(types::I32, 0);
            call0(module, b, memset, &[base, zero, used]);
            let k1 = load_key(module, b, global, pointer);
            b.ins().brif(k1, lookup, &[], release, &[]);

            b.switch_to_block(lookup);
            let key = b.ins().iadd_imm(k1, -1);
            let c = call1(module, b, getspecific, &[key]);
            b.ins().brif(c, have, &[], make, &[]);

            b.switch_to_block(make);
            let bytes = b.ins().iconst(pointer, REGION_CACHE_BYTES);
            let made = call1(module, b, malloc, &[bytes]);
            b.ins().brif(made, made_ok, &[], release, &[]);

            b.switch_to_block(made_ok);
            let none = b.ins().iconst(types::I64, 0);
            b.ins().store(MemFlags::trusted(), none, made, 0);
            let status = call1(module, b, setspecific, &[key, made]);
            b.ins().brif(status, unmade, &[], push, &[made.into()]);

            b.switch_to_block(unmade);
            call0(module, b, free, &[made]);
            b.ins().jump(release, &[]);

            b.switch_to_block(have);
            b.ins().jump(push, &[c.into()]);

            b.switch_to_block(push);
            let s = b.block_params(push)[0];
            let n = b.ins().load(types::I64, MemFlags::trusted(), s, 0);
            let full = b.ins().icmp_imm(IntCC::SignedGreaterThanOrEqual, n, REGION_CACHE_SLOTS);
            b.ins().brif(full, release, &[], keep, &[]);

            b.switch_to_block(keep);
            let n1 = b.ins().iadd_imm(n, 1);
            let offset = b.ins().ishl_imm(n1, 3);
            let at = b.ins().iadd(s, offset);
            b.ins().store(MemFlags::trusted(), base, at, 0);
            b.ins().store(MemFlags::trusted(), n1, s, 0);
            b.ins().return_(&[]);

            b.switch_to_block(release);
            call0(module, b, free, &[base]);
            b.ins().return_(&[]);
        })?;

        // drop: the key's destructor
        self.define(REGION_DROP, ctx, fb_ctx, |module, b| {
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            let (head, body, done) = (b.create_block(), b.create_block(), b.create_block());
            b.append_block_param(head, types::I64);

            b.switch_to_block(entry);
            let s = b.block_params(entry)[0];
            let n = b.ins().load(types::I64, MemFlags::trusted(), s, 0);
            let one = b.ins().iconst(types::I64, 1);
            b.ins().jump(head, &[one.into()]);

            b.switch_to_block(head);
            let i = b.block_params(head)[0];
            let more = b.ins().icmp(IntCC::SignedLessThanOrEqual, i, n);
            b.ins().brif(more, body, &[], done, &[]);

            b.switch_to_block(body);
            let offset = b.ins().ishl_imm(i, 3);
            let at = b.ins().iadd(s, offset);
            let p = b.ins().load(pointer, MemFlags::trusted(), at, 0);
            call0(module, b, free, &[p]);
            let next = b.ins().iadd_imm(i, 1);
            b.ins().jump(head, &[next.into()]);

            b.switch_to_block(done);
            call0(module, b, free, &[s]);
            b.ins().return_(&[]);
        })?;

        // init: the key, plus one into the global, when `pthread_key_create` succeeds
        self.define(REGION_INIT, ctx, fb_ctx, |module, b| {
            let entry = b.create_block();
            let (set, done) = (b.create_block(), b.create_block());
            b.switch_to_block(entry);
            let slot =
                b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let zero = b.ins().iconst(types::I64, 0);
            b.ins().stack_store(zero, slot, 0);
            let at = b.ins().stack_addr(pointer, slot, 0);
            let drop_id = declare(module, REGION_DROP);
            let drop_ref = module.declare_func_in_func(drop_id, b.func);
            let destructor = b.ins().func_addr(pointer, drop_ref);
            let status = call1(module, b, key_create, &[at, destructor]);
            b.ins().brif(status, done, &[], set, &[]);

            b.switch_to_block(set);
            let key = b.ins().stack_load(types::I64, slot, 0);
            let plus = b.ins().iadd_imm(key, 1);
            let g = module.declare_data_in_func(global, b.func);
            let address = b.ins().global_value(pointer, g);
            b.ins().store(MemFlags::trusted(), plus, address, 0);
            b.ins().jump(done, &[]);

            b.switch_to_block(done);
            b.ins().return_(&[]);
        })?;
        Ok(())
    }

    /// Build and define one of the four, its body written by `body`.
    fn define(
        &mut self,
        name: &str,
        ctx: &mut Context,
        fb_ctx: &mut FunctionBuilderContext,
        body: impl FnOnce(&mut ObjectModule, &mut FunctionBuilder),
    ) -> Result<(), CodegenError> {
        let id = declare(&mut self.module, name);
        ctx.clear();
        ctx.func =
            Function::with_name_signature(UserFuncName::default(), signature(&self.module, name));
        {
            let mut builder = FunctionBuilder::new(&mut ctx.func, fb_ctx);
            body(&mut self.module, &mut builder);
            builder.seal_all_blocks();
            builder.finalize();
        }
        self.module.define_function(id, ctx).map_err(|e| CodegenError::plain(e.to_string()))
    }

    /// `main`'s first call: make the key.
    pub(crate) fn call_region_init(&mut self, builder: &mut FunctionBuilder) {
        let id = declare(&mut self.module, REGION_INIT);
        let f = self.module.declare_func_in_func(id, builder.func);
        builder.ins().call(f, &[]);
    }
}

/// The key plus one, from the global.
fn load_key(
    module: &mut ObjectModule,
    b: &mut FunctionBuilder,
    global: DataId,
    pointer: types::Type,
) -> Value {
    let g = module.declare_data_in_func(global, b.func);
    let address = b.ins().global_value(pointer, g);
    b.ins().load(types::I64, MemFlags::trusted(), address, 0)
}

/// A call with one result.
fn call1(module: &mut ObjectModule, b: &mut FunctionBuilder, id: FuncId, args: &[Value]) -> Value {
    let f = module.declare_func_in_func(id, b.func);
    let call = b.ins().call(f, args);
    b.inst_results(call)[0]
}

/// A call whose result, if any, is not wanted.
fn call0(module: &mut ObjectModule, b: &mut FunctionBuilder, id: FuncId, args: &[Value]) {
    let f = module.declare_func_in_func(id, b.func);
    b.ins().call(f, args);
}
