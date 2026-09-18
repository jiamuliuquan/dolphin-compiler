//! 源码与诊断基础设施：`Span`/`SourceMap`、词法单元和词法分析。
//!
//! 这是依赖图的最底层，不依赖任何其他 Dolphin crate。

pub mod diagnostic;
pub mod lexer;
pub mod source;
pub mod token;
