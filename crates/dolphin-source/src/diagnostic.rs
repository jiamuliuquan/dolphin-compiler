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
        // 诊断 span 始终吸附到字符边界，避免任何上游错误 span 触发切片 panic。
        let start = char_boundary(&source.text, span.start);
        let (line, column) = source.line_column(start);
        let line_text = source.line_text(line);
        let line_end = source.text[start..]
            .find('\n')
            .map(|offset| start + offset)
            .unwrap_or(source.text.len());
        let marked_end = char_boundary(
            &source.text,
            span.end.min(line_end).max(start + 1).min(source.text.len()),
        );
        let width = source.text[start..marked_end].chars().count().max(1);
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

/// 将字节偏移向下吸附到最近的 UTF-8 字符边界（并夹在本文件长度内）。
fn char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.rendered)
    }
}

impl std::error::Error for Diagnostic {}
