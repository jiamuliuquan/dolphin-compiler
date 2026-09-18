//! 后端无关的代码生成接口（M16 核心边界）。
//!
//! 驱动与前端只依赖这里的 [`CodegenBackend`] trait，不依赖任何具体后端。
//! Cranelift 与 LLVM 后端各自实现该 trait，前端测试可在两个后端上复用。
//! 这也让 LLVM 的重依赖被隔离在 `dolphin-codegen-llvm` 一个 crate 内。

use std::path::Path;

use dolphin_ir::ir::Program;
use dolphin_platform::platform::TargetPlatform;
use dolphin_source::diagnostic::Diagnostic;

/// 把类型化 IR 降低为目标平台的本机目标文件。
pub trait CodegenBackend {
    /// 生成 `object` 目标文件；`optimize` 为 true 时启用速度优化。
    fn emit_program(
        &self,
        program: &Program,
        object: &Path,
        optimize: bool,
        target: &dyn TargetPlatform,
    ) -> Result<(), Diagnostic>;
}
