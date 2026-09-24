use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn merge(self, other: Self) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
    }
}

/// 加载单元内源码文件的稳定身份（M20/H20-01）。
///
/// `SourceId.0` 等于该文件在加载单元 `sources` 向量中的下标；跨快照不稳定、
/// 不持久化。未进入任何加载单元的独立 `SourceFile` 使用 [`SourceId::ANONYMOUS`]。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(pub u32);

impl SourceId {
    /// 未绑定到任何加载单元的源码（单文件工具、测试临时文件）。
    pub const ANONYMOUS: SourceId = SourceId(u32::MAX);
}

#[derive(Debug)]
pub struct SourceFile {
    /// 加载单元内的文件身份；`SourceFile::new` 构造时为 `ANONYMOUS`。
    pub id: SourceId,
    pub path: PathBuf,
    pub text: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(path: PathBuf, text: String) -> Self {
        Self::with_id(SourceId::ANONYMOUS, path, text)
    }

    /// 带显式 `SourceId` 构造（收集式 loader 按文件顺序分配）。
    pub fn with_id(id: SourceId, path: PathBuf, text: String) -> Self {
        let mut line_starts = vec![0];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index + 1);
            }
        }
        Self {
            id,
            path,
            text,
            line_starts,
        }
    }

    pub fn line_column(&self, offset: usize) -> (usize, usize) {
        let line = self.line_starts.partition_point(|start| *start <= offset) - 1;
        let line_start = self.line_starts[line];
        let column = self.text[line_start..offset.min(self.text.len())]
            .chars()
            .count();
        (line + 1, column + 1)
    }

    /// 行数（1-based 行号的合法范围为 `1..=line_count`）。
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// 指定行（1-based）首字节在源码中的偏移。
    pub fn line_start(&self, line: usize) -> usize {
        self.line_starts[line - 1]
    }

    pub fn line_text(&self, line: usize) -> &str {
        let start = self.line_starts[line - 1];
        let end = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.text.len());
        self.text[start..end].trim_end_matches(['\r', '\n'])
    }
}

/// 只读文件表，供诊断渲染与 LSP 位置换算（M20/H20-01）。
///
/// `SourceId::ANONYMOUS` 不参与查找：调用方不得为匿名诊断猜测文件。
pub struct SourceMap<'a> {
    files: &'a [SourceFile],
}

impl<'a> SourceMap<'a> {
    pub fn new(files: &'a [SourceFile]) -> Self {
        Self { files }
    }

    pub fn file(&self, id: SourceId) -> Option<&'a SourceFile> {
        if id == SourceId::ANONYMOUS {
            return None;
        }
        self.files.iter().find(|file| file.id == id)
    }

    pub fn path(&self, id: SourceId) -> Option<&'a Path> {
        self.file(id).map(|file| file.path.as_path())
    }

    /// 字节偏移 -> 1-based `(line, column)`；列按 Unicode 字符计数。
    pub fn position(&self, id: SourceId, offset: usize) -> Option<(usize, usize)> {
        self.file(id).map(|file| file.line_column(offset))
    }

    /// 1-based `(line, column)`（字符列）-> 字节偏移；越界返回 `None`。
    pub fn offset(&self, id: SourceId, line: usize, column: usize) -> Option<usize> {
        let file = self.file(id)?;
        if line == 0 || line > file.line_count() {
            return None;
        }
        let mut byte = file.line_start(line);
        let mut remaining = column.saturating_sub(1);
        for character in file.line_text(line).chars() {
            if remaining == 0 {
                break;
            }
            byte += character.len_utf8();
            remaining -= 1;
        }
        Some(byte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_map_resolves_ids_positions_and_offsets() {
        let first = SourceFile::with_id(
            SourceId(0),
            PathBuf::from("a.do"),
            "fn a() {}\nval s = \"𝄞\";\n".to_string(),
        );
        let second = SourceFile::with_id(
            SourceId(1),
            PathBuf::from("b.do"),
            "fn b() {}\n".to_string(),
        );
        let files = vec![first, second];
        let map = SourceMap::new(&files);

        assert_eq!(map.path(SourceId(1)), Some(Path::new("b.do")));
        assert_eq!(map.position(SourceId(1), 0), Some((1, 1)));
        assert_eq!(map.position(SourceId(1), 3), Some((1, 4)));
        // 1-based 字符列：`𝄞` 是单个字符但 4 字节（第 10 列起于字节 19，第 11 列起于 23）。
        assert_eq!(map.offset(SourceId(0), 2, 9), Some(18));
        assert_eq!(map.offset(SourceId(0), 2, 10), Some(19));
        assert_eq!(map.offset(SourceId(0), 2, 11), Some(23));
        assert_eq!(map.offset(SourceId(0), 9, 1), None);
        assert_eq!(map.offset(SourceId(0), 2, 0), Some(10));
    }

    #[test]
    fn source_map_never_guesses_anonymous_files() {
        let anonymous = SourceFile::new(PathBuf::from("anon.do"), "fn a() {}\n".to_string());
        let files = vec![anonymous];
        let map = SourceMap::new(&files);
        assert!(map.file(SourceId::ANONYMOUS).is_none());
        assert_eq!(map.position(SourceId::ANONYMOUS, 0), None);
        assert_eq!(map.path(SourceId(0)), None);
    }
}
