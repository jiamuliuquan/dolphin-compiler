//! 共享项目分析（M20/H20-02）。
//!
//! 本 crate 把“项目配置/已解析依赖读取、源码提供器、分析快照”与真正构建副作用
//! 分开：只读解析依赖（不下载、不写锁）、overlay 未保存文本、按 lib/bin 选择
//! 分析单元、合并去重诊断，并产出不可变快照与符号索引。
//!
//! 明确不做：codegen、链接、构建产物、后台线程/异步 runtime（M20 §6、§7）。

pub mod host;
pub mod index;
pub mod single;
pub mod uri;

pub use host::{AnalysisHost, AnalysisMode, AnalysisSnapshot, AnalysisUnit, UnitKind};
pub use index::{
    DefId, DefKind, Definition, DocumentSymbol, Resolution, ResolutionEntry, SymbolId, SymbolIndex,
};
pub use single::{SingleFileAnalysis, analyze_single_file};
pub use uri::{path_to_uri, uri_to_path};
