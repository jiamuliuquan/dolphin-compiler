use std::fmt;
use std::path::Path;

use crate::source::{SourceFile, Span};

#[derive(Debug)]
pub struct Diagnostic {
    rendered: String,
}

impl Diagnostic {
    pub fn plain(message: impl Into<String>) -> Self {
        Self {
            rendered: format!("error[E0000]: {}", message.into()),
        }
    }

    /// 诊断中的源码路径：Windows 下把反斜杠统一显示为正斜杠（rustc 惯例），
    /// 保证诊断文本跨平台稳定。
    fn display_path(path: &Path) -> String {
        let text = path.to_string_lossy();
        if cfg!(windows) {
            text.replace('\\', "/")
        } else {
            text.into_owned()
        }
    }

    pub fn at(source: &SourceFile, span: Span, message: impl AsRef<str>) -> Self {
        let (line, column) = source.line_column(span.start);
        let line_text = source.line_text(line);
        let line_end = source.text[span.start.min(source.text.len())..]
            .find('\n')
            .map(|offset| span.start + offset)
            .unwrap_or(source.text.len());
        let marked_end = span
            .end
            .min(line_end)
            .max(span.start + 1)
            .min(source.text.len());
        let width = source.text[span.start.min(source.text.len())..marked_end]
            .chars()
            .count()
            .max(1);
        let marker = format!("{}{}", " ".repeat(column - 1), "^".repeat(width));
        let continuation = if span.end > line_end { "\n  | ..." } else { "" };
        Self {
            rendered: format!(
                "error[E0001]: {}\n --> {}:{line}:{column}\n  |\n{line} | {line_text}\n  | {marker}{continuation}",
                message.as_ref(),
                Self::display_path(&source.path)
            ),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.rendered)
    }
}

impl std::error::Error for Diagnostic {}
