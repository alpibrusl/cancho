//! Cranelift lowering and native object emission.
//!
//! Cranelift rather than LLVM for M0 (#3): trivial to embed, no C++ build
//! dependency, and compile speed suits the feedback loop. LLVM arrives later
//! for release-quality codegen — the plan is to ship both, as rustc does.
//!
//! Everything this module can reject, `lex-sys-ir` has already rejected, so an
//! error here is the environment's (an unsupported host) or the compiler's own:
//! a Cranelift verifier complaint or a broken invariant, which the CLI reports
//! as an `internal` refusal at the function it happened in
//! (`docs/internal-errors.md`).

use std::fmt;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    AbiParam, InstBuilder, MemFlags, StackSlotData, StackSlotKind, TrapCode, Value, types,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::{Context, isa};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use lex_sys_ir::{
    Arm, BinOp, Builtin, Callee, Expr, Func, FuncId as IrFuncId, Place, Program, Slot, Stmt,
    TypeInfo, terminates,
};
use lex_sys_types::{DefId, Type};
use target_lexicon::Triple;

mod abi;
mod body;
mod emit;
mod layout;

use abi::*;
use body::*;
use emit::*;
pub use layout::*;

/// Code generation failed (`docs/internal-errors.md`).
///
/// Every one of these used to be a bug in the compiler, not in the
/// program -- true of Cranelift, which has no environment to fail in.
/// `lex-sys-codegen-llvm` shells out to a real external tool, so one
/// failure mode here genuinely is not a bug: `clang` not being on the
/// host at all. `environment` is `true` for exactly that case and
/// `false` for everything else, so the CLI can tell "lex-sys has a bug"
/// from "the host is missing a tool" apart rather than reporting both as
/// `internal` (`docs/llvm-backend.md`). `function` is what makes the
/// `internal` case reportable -- the CLI turns it into a refusal located
/// at the function's declaration -- and it is `None` only for a failure
/// outside any one function, such as defining a data object, finishing
/// the module, or (always, since no function caused it) an
/// `environment` failure.
#[derive(Debug)]
pub struct CodegenError {
    /// The index into `Program::funcs` of the function being generated.
    pub function: Option<usize>,
    /// Cranelift's own text, or the panic's: what a bug report needs.
    pub message: String,
    /// `true` when the host's environment is what failed (a missing
    /// tool), not lex-sys's own code generation.
    pub environment: bool,
}

impl CodegenError {
    fn plain(message: impl Into<String>) -> CodegenError {
        CodegenError { function: None, message: message.into(), environment: false }
    }
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CodegenError {}

impl From<String> for CodegenError {
    fn from(s: String) -> Self {
        CodegenError::plain(s)
    }
}

/// The text a panic carried, which is a `&str` or a `String` for every
/// `panic!`, `unreachable!` and `expect` in this crate.
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a panic with no message".to_owned()
    }
}

/// The host triple this compiler emits for. M0 is host-only; cross-compilation
/// is not a milestone yet.
pub fn host_triple() -> Triple {
    Triple::host()
}

/// Compile a program to the bytes of a native object file for the host.
///
/// `entry` names the lex-sys function that becomes the process entry point: a C
/// `main` is synthesised around it, truncating its `int` result to the `int`
/// the platform's exit status is.
pub fn compile_object(program: &Program, entry: &str) -> Result<Vec<u8>, CodegenError> {
    compile_object_for(program, entry, host_triple())
}

/// Compile a program to the bytes of an object file for an explicit target.
///
/// M0 builds only for the host — there is no cross-compilation story and no
/// driver flag for one. This exists so the object *format's* conventions can be
/// tested from any host: symbol mangling differs between ELF and Mach-O, and
/// getting it wrong is a link failure nobody on Linux will ever see.
pub fn compile_object_for(
    program: &Program,
    entry: &str,
    triple: Triple,
) -> Result<Vec<u8>, CodegenError> {
    let mut flags = settings::builder();
    // Position-independent code: both default targets link PIE by default.
    flags.set("is_pic", "true").map_err(|e| CodegenError::plain(e.to_string()))?;
    // Deterministic output matters more here than the last few percent (#1).
    flags.set("opt_level", "speed").map_err(|e| CodegenError::plain(e.to_string()))?;
    let flags = settings::Flags::new(flags);

    let isa = isa::lookup(triple.clone())
        .map_err(|e| CodegenError::plain(format!("unsupported host `{triple}`: {e}")))?
        .finish(flags)
        .map_err(|e| CodegenError::plain(e.to_string()))?;

    let builder = ObjectBuilder::new(isa, "lex-sys", default_libcall_names())
        .map_err(|e| CodegenError::plain(e.to_string()))?;
    let mut module = ObjectModule::new(builder);

    let mut emitter = Emitter { module, program };
    emitter.emit(entry)?;
    module = emitter.module;

    module.finish().emit().map_err(|e| CodegenError::plain(e.to_string()))
}

#[cfg(test)]
mod tests;
