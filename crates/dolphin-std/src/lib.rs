//! 随编译器分发的 Dolphin 源码标准库（数据 crate）。
//!
//! 这些 `.do` 文件不是预编译产物，而是作为源码注入到每次构建的前端，因此本
//! crate 只暴露字符串常量，不依赖编译器任何阶段。它与编译器锁版本：`.dlib`
//! 消费端要求 `compiler-version` 完全一致。

/// 标准库单元：`(限定模块名, 展示路径, 源码)`。
///
/// 限定模块名会与 `PackageId::STD` 组合成 `dolphin:std:<version>` 身份。
pub const UNITS: &[(&str, &str, &str)] = &[
    ("std", "<std>/std.do", include_str!("std.do")),
    (
        "std.collections",
        "<std>/collections.do",
        include_str!("collections.do"),
    ),
    ("std.text", "<std>/text.do", include_str!("text.do")),
    ("std.ffi", "<std>/ffi.do", include_str!("ffi.do")),
];
