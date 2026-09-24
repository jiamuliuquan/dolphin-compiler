use std::fmt;
use std::path::Path;

use crate::source::{SourceFile, SourceId, Span};

/// 每文件诊断收集上限（M20 §5.2/§5.4）。
pub const DIAGNOSTIC_LIMIT: usize = 100;

/// 追加一条诊断；达到上限时追加 `E0002` 并返回 `false`（调用方应停止收集）。
pub fn push_capped(diagnostics: &mut Vec<Diagnostic>, diagnostic: Diagnostic) -> bool {
    if diagnostics.len() >= DIAGNOSTIC_LIMIT {
        return false;
    }
    diagnostics.push(diagnostic);
    if diagnostics.len() >= DIAGNOSTIC_LIMIT {
        diagnostics.push(Diagnostic::error(
            "E0002",
            "too many errors; further diagnostics suppressed",
        ));
        return false;
    }
    true
}

/// 诊断严重级别（M20/H20-01）。
///
/// M20 只产生 `Error`；`Warning`/`Note` 变体保留给未来协议映射，
/// 在 CLI 默认输出中不得新增 warning。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// 诊断的位置标签：`labels[0]` 是 primary，其余是 related location。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub source: SourceId,
    pub span: Span,
    /// 可为空；primary label 的 message 不参与渲染。
    pub message: String,
}

/// 结构化诊断（M20/H20-01）。
///
/// 保留 `plain`/`at` 的构造签名与 `Display` 文本；新增字段只通过访问器读取。
/// 数据完全拥有自身（`String`/`Vec`），不借用 `SourceFile`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    code: Box<str>,
    severity: Severity,
    message: String,
    labels: Vec<Label>,
    notes: Vec<String>,
    /// 预渲染文本；`plain`/`at` 的输出与 M19 逐字节一致。
    rendered: String,
}

impl Diagnostic {
    pub fn plain(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code: "E0000".into(),
            severity: Severity::Error,
            rendered: format!("error[E0000]: {message}"),
            message,
            labels: Vec::new(),
            notes: Vec::new(),
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
            code: "E0001".into(),
            severity: Severity::Error,
            message: message.as_ref().to_string(),
            labels: vec![Label {
                source: source.id,
                span: Span::new(start, marked_end),
                message: String::new(),
            }],
            notes: Vec::new(),
            rendered: format!(
                "error[E0001]: {}\n --> {}:{line}:{column}\n  |\n{line} | {line_text}\n  | {marker}{continuation}",
                message.as_ref(),
                Self::display_path(&source.path)
            ),
        }
    }

    /// 无位置错误：`error[<code>]: <message>`（M20 新分类使用 `E1xxx`/`E2xxx`）。
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code: code.into(),
            severity: Severity::Error,
            rendered: format!("error[{code}]: {message}"),
            message,
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// 追加 secondary label（related location）。渲染为独立 `= note` 行。
    pub fn with_label(
        mut self,
        source: &SourceFile,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        let message = message.into();
        let start = char_boundary(&source.text, span.start);
        let end = char_boundary(&source.text, span.end.max(start));
        let (line, column) = source.line_column(start);
        let label = Label {
            source: source.id,
            span: Span::new(start, end),
            message,
        };
        let location = format!("{}:{line}:{column}", Self::display_path(&source.path));
        let note = if label.message.is_empty() {
            format!("  = note: {location}")
        } else {
            format!("  = note: {} ({location})", label.message)
        };
        self.rendered.push('\n');
        self.rendered.push_str(&note);
        self.labels.push(label);
        self
    }

    /// 追加一条无位置说明，渲染为独立 `= note` 行。
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        let note = note.into();
        self.rendered.push('\n');
        self.rendered.push_str("  = note: ");
        self.rendered.push_str(&note);
        self.notes.push(note);
        self
    }

    /// 替换诊断 code；`rendered` 首行的 `error[<code>]` 同步更新。
    pub fn with_code(mut self, code: &str) -> Self {
        if self.code.as_ref() != code {
            // `plain`/`at`/`error` 构造的 rendered 首行固定为 `error[<code>]: `。
            if let Some(rest) = self.rendered.strip_prefix("error[")
                && let Some(end) = rest.find("]: ")
            {
                self.rendered = format!("error[{code}]: {}", &rest[end + 3..]);
            }
            self.code = code.into();
        }
        self
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn labels(&self) -> &[Label] {
        &self.labels
    }

    pub fn notes(&self) -> &[String] {
        &self.notes
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn source(text: &str) -> SourceFile {
        SourceFile::new(PathBuf::from("main.do"), text.to_string())
    }

    #[test]
    fn plain_and_at_render_unchanged() {
        assert_eq!(Diagnostic::plain("boom").to_string(), "error[E0000]: boom");
        let file = source("fn main() {\n    return;\n}\n");
        let diagnostic = Diagnostic::at(&file, Span::new(16, 22), "bad");
        assert_eq!(
            diagnostic.to_string(),
            "error[E0001]: bad\n --> main.do:2:5\n  |\n2 |     return;\n  |     ^^^^^^"
        );
        assert_eq!(diagnostic.code(), "E0001");
        assert_eq!(diagnostic.message(), "bad");
        assert_eq!(diagnostic.labels().len(), 1);
        assert_eq!(diagnostic.labels()[0].source, SourceId::ANONYMOUS);
    }

    #[test]
    fn secondary_labels_and_notes_append_lines() {
        let primary = source("fn main() { return; }\n");
        let related = source("fn helper() { return; }\n");
        let diagnostic = Diagnostic::at(&primary, Span::new(3, 7), "duplicate")
            .with_label(&related, Span::new(3, 9), "previous definition")
            .with_label(&related, Span::new(3, 9), "")
            .with_note("rename one of them")
            .with_code("E2001");
        let rendered = diagnostic.to_string();
        assert_eq!(
            rendered,
            "error[E2001]: duplicate\n --> main.do:1:4\n  |\n1 | fn main() { return; }\n  |    ^^^^\n  \
             = note: previous definition (main.do:1:4)\n  = note: main.do:1:4\n  = note: rename one of them"
        );
        assert_eq!(diagnostic.code(), "E2001");
        assert_eq!(diagnostic.labels().len(), 3);
        assert_eq!(diagnostic.notes(), ["rename one of them"]);
    }

    #[test]
    fn at_snaps_mid_character_spans() {
        let file = source("fn main() { val s = \"𝄞\"; return; }\n");
        let offset = file.text.find('𝄞').unwrap() + 1;
        let diagnostic = Diagnostic::at(&file, Span::new(offset, offset), "bad");
        assert!(diagnostic.to_string().contains("main.do:1:"));
        assert_eq!(
            diagnostic.labels()[0].span.start,
            file.text.find('𝄞').unwrap()
        );
    }
}
