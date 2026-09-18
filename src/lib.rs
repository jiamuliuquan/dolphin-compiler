//! `dolphin-compiler` 兼容门面。
//!
//! 真正的实现分散在 `crates/` 下的各 workspace crate；这里只做转发，保持
//! `dolphin_compiler::*` 这一公开路径与既有测试、脚本兼容。

pub use dolphin_driver::*;
