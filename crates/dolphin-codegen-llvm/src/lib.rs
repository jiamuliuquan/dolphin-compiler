//! LLVM 代码生成后端（M16）。
//!
//! 这是 M16 的落点：在此 crate 内接入 `inkwell`/`llvm-sys`，把类型化 IR 降低为
//! LLVM IR，复用驱动现有的 `rust-lld` 链接流程。LLVM 的构建期依赖只允许出现在
//! 本 crate，不得泄漏到 `dolphin-backend` 或前端。
mod codegen;

use std::path::Path;

use dolphin_backend::CodegenBackend;
use dolphin_ir::ir::Program;
use dolphin_platform::platform::TargetPlatform;
use dolphin_source::diagnostic::Diagnostic;

/// LLVM 代码生成后端。
pub struct LlvmBackend;

impl CodegenBackend for LlvmBackend {
    fn emit_program(
        &self,
        program: &Program,
        object: &Path,
        optimize: bool,
        target: &dyn TargetPlatform,
    ) -> Result<(), Diagnostic> {
        codegen::emit_program_optimized(program, object, optimize, target)
    }
}
