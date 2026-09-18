//! 前端中段：模块加载、名称解析、泛型单态化与降低到类型化 IR。

pub mod lower;
pub mod modules;
pub mod monomorphize;
