//! 单文件分析（M20/H20-03，§7.1）。
//!
//! 无 `dolphin.toml` 的项目根按现有单文件规则分析：词法/语法错误全部收集；
//! `pkg`/`use` 或缺少 `main` 时跳过语义检查（与 M17 LSP 行为一致，避免误报）；
//! 语义成功时注入内建标准库并构建符号索引，供 hover/definition 使用而不做文本同名猜测。

use std::sync::Arc;

use dolphin_hir::lower;
use dolphin_hir::modules;
use dolphin_source::diagnostic::Diagnostic;
use dolphin_source::lexer::lex_recovering;
use dolphin_source::source::SourceFile;
use dolphin_syntax::parser::parse_recovering;

use crate::index::SymbolIndex;

/// 单文件分析结果：诊断与可选符号索引。
pub struct SingleFileAnalysis {
    /// 词法/语法/语义诊断；语义成功且跳过检查时为空。
    pub diagnostics: Vec<Diagnostic>,
    /// 语义成功时可用；`pkg`/`use` 或缺少 `main` 时为 `None`。
    pub index: Option<Arc<SymbolIndex>>,
    /// 用户文件（`sources[0]`，保留传入的 `SourceId`）与注入的内建标准库源码。
    pub sources: Vec<SourceFile>,
}

/// 按 §7.1 的单文件规则分析一个打开文档。
pub fn analyze_single_file(source: &SourceFile) -> SingleFileAnalysis {
    let (tokens, lex_diagnostics) = lex_recovering(source);
    if !lex_diagnostics.is_empty() {
        return failed(lex_diagnostics, source);
    }
    let (program, parse_diagnostics) = parse_recovering(source, tokens);
    if !parse_diagnostics.is_empty() {
        return failed(parse_diagnostics, source);
    }
    // `pkg`/`use` 文档属于项目分析；库文件或尚未写完 `main` 的文档不做语义检查。
    if program.package.is_some() || !program.uses.is_empty() {
        return skipped(source);
    }
    if !program
        .functions
        .iter()
        .any(|function| function.name == "main")
    {
        return skipped(source);
    }
    let loaded = match modules::load_single_source(source, &program) {
        Ok(loaded) => loaded,
        Err(error) => return failed(vec![error], source),
    };
    match lower::lower_sources_analysis_collecting(
        &loaded.sources,
        &loaded.program,
        &loaded.packages,
        true,
    ) {
        Ok(lowered) => {
            let index = SymbolIndex::new(&lowered.analysis, &loaded.program, &loaded.sources, None);
            SingleFileAnalysis {
                diagnostics: Vec::new(),
                index: Some(Arc::new(index)),
                sources: loaded.sources,
            }
        }
        Err(diagnostics) => SingleFileAnalysis {
            diagnostics,
            index: None,
            sources: loaded.sources,
        },
    }
}

fn failed(diagnostics: Vec<Diagnostic>, source: &SourceFile) -> SingleFileAnalysis {
    SingleFileAnalysis {
        diagnostics,
        index: None,
        sources: vec![copy_source(source)],
    }
}

fn skipped(source: &SourceFile) -> SingleFileAnalysis {
    SingleFileAnalysis {
        diagnostics: Vec::new(),
        index: None,
        sources: vec![copy_source(source)],
    }
}

fn copy_source(source: &SourceFile) -> SourceFile {
    SourceFile::with_id(source.id, source.path.clone(), source.text.clone())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use dolphin_source::source::{SourceFile, SourceId, Span};

    use super::*;

    fn analyze(text: &str) -> SingleFileAnalysis {
        let source = SourceFile::with_id(
            SourceId(0),
            PathBuf::from("/single/main.do"),
            text.to_string(),
        );
        analyze_single_file(&source)
    }

    #[test]
    fn lexical_and_syntax_errors_are_collected_without_index() {
        let lexical = analyze("fn main() { § return 0; }\n");
        assert_eq!(lexical.diagnostics.len(), 1);
        assert_eq!(lexical.diagnostics[0].code(), "E0001");
        assert!(lexical.index.is_none());

        let syntax = analyze("fn main() { return 0;\n");
        assert_eq!(syntax.diagnostics.len(), 1);
        assert_eq!(syntax.diagnostics[0].code(), "E0001");
        assert!(syntax.index.is_none());
    }

    #[test]
    fn semantic_error_keeps_stdlib_sources_without_index() {
        let analysis = analyze("fn main() { return missing; }\n");
        assert_eq!(analysis.diagnostics.len(), 1);
        assert_eq!(analysis.diagnostics[0].code(), "E0001");
        assert_eq!(
            analysis.diagnostics[0].message(),
            "unknown variable `missing`"
        );
        assert!(analysis.index.is_none());
        assert!(analysis.sources.len() > 1, "语义阶段必须注入内建标准库源码");
        assert_eq!(analysis.sources[0].id, SourceId(0));
    }

    #[test]
    fn valid_program_yields_index_and_resolutions() {
        let text =
            "fn helper(value: i32): i32 { return value; }\nfn main() { return helper(1); }\n";
        let analysis = analyze(text);
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        let index = analysis.index.as_ref().expect("index");
        let call = text.find("helper(1)").unwrap();
        let resolution = index.resolve(SourceId(0), call);
        let crate::index::Resolution::Def(def) = resolution else {
            panic!("helper call must resolve to a definition: {resolution:?}");
        };
        assert_eq!(def.qualified, "helper");
        let definition = index
            .definition_of(&crate::index::SymbolId::Def(def))
            .expect("definition");
        assert_eq!(
            &text[definition.name_span.start..definition.name_span.end],
            "helper"
        );
        assert_eq!(
            index.local_type(SourceId(0), Span::new(10, 15)),
            Some("i32")
        );
    }

    #[test]
    fn pkg_use_and_missing_main_skip_semantics() {
        let pkg = analyze("pkg inner;\npub fn value(): i32 { return 1; }\n");
        assert!(pkg.diagnostics.is_empty());
        assert!(pkg.index.is_none());

        let library = analyze("pub fn value(): i32 { return 1; }\n");
        assert!(library.diagnostics.is_empty());
        assert!(library.index.is_none());
    }
}
