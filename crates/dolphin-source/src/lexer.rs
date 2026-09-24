use crate::diagnostic::{Diagnostic, push_capped};
use crate::source::{SourceFile, Span};
use crate::token::{Token, TokenKind};

/// 首错即停的词法分析（行为与 M19 一致）。
pub fn lex(source: &SourceFile) -> Result<Vec<Token>, Diagnostic> {
    let (tokens, diagnostics) = lex_recovering(source);
    match diagnostics.into_iter().next() {
        Some(diagnostic) => Err(diagnostic),
        None => Ok(tokens),
    }
}

/// 收集式词法分析（M20/H20-01）：未知字符跳过该字符继续；未闭合字符串/字符
/// 在起始引号报错并跳到行尾继续；未闭合块注释在 `/*` 报错并跳到 EOF。
/// 返回值总是以 `Eof` 结尾，可安全交给 parser。每文件最多收集 100 条。
pub fn lex_recovering(source: &SourceFile) -> (Vec<Token>, Vec<Diagnostic>) {
    Lexer::new(source).lex_all()
}

struct Lexer<'a> {
    source: &'a SourceFile,
    bytes: &'a [u8],
    position: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
    capped: bool,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a SourceFile) -> Self {
        Self {
            source,
            bytes: source.text.as_bytes(),
            position: 0,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
            capped: false,
        }
    }

    /// 收集一条诊断；达到上限后置 `capped` 并停止继续扫描。
    fn report(&mut self, diagnostic: Diagnostic) {
        if !push_capped(&mut self.diagnostics, diagnostic) {
            self.capped = true;
        }
    }

    fn lex_all(mut self) -> (Vec<Token>, Vec<Diagnostic>) {
        while self.position < self.bytes.len() && !self.capped {
            if self.bytes[self.position].is_ascii_whitespace() {
                self.position += 1;
                continue;
            }
            if self.starts_with(b"//") {
                self.skip_line_comment();
                continue;
            }
            if self.starts_with(b"/*") {
                if let Err(diagnostic) = self.skip_block_comment() {
                    self.report(diagnostic);
                }
                continue;
            }

            let start = self.position;
            let kind = match self.bytes[self.position] {
                b'(' => self.single(TokenKind::LeftParen),
                b')' => self.single(TokenKind::RightParen),
                b'{' => self.single(TokenKind::LeftBrace),
                b'}' => self.single(TokenKind::RightBrace),
                b'[' => self.single(TokenKind::LeftBracket),
                b']' => self.single(TokenKind::RightBracket),
                b':' if self.starts_with(b"::") => self.double(TokenKind::ColonColon),
                b':' => self.single(TokenKind::Colon),
                b',' => self.single(TokenKind::Comma),
                b';' => self.single(TokenKind::Semicolon),
                b'"' => match self.string() {
                    Some(kind) => kind,
                    None => continue,
                },
                b'\'' => match self.character() {
                    Some(kind) => kind,
                    None => continue,
                },
                b'+' => self.with_equal(TokenKind::Plus, TokenKind::PlusEqual),
                b'-' if self.starts_with(b"->") => self.double(TokenKind::Arrow),
                b'-' => self.with_equal(TokenKind::Minus, TokenKind::MinusEqual),
                b'*' => self.with_equal(TokenKind::Star, TokenKind::StarEqual),
                b'/' => self.with_equal(TokenKind::Slash, TokenKind::SlashEqual),
                b'%' => self.with_equal(TokenKind::Percent, TokenKind::PercentEqual),
                b'!' => self.with_equal(TokenKind::Bang, TokenKind::BangEqual),
                b'=' => {
                    if self.starts_with(b"=>") {
                        self.double(TokenKind::FatArrow)
                    } else {
                        self.with_equal(TokenKind::Equal, TokenKind::EqualEqual)
                    }
                }
                b'<' => self.with_equal(TokenKind::Less, TokenKind::LessEqual),
                b'>' => self.with_equal(TokenKind::Greater, TokenKind::GreaterEqual),
                b'&' if self.starts_with(b"&&") => self.double(TokenKind::AndAnd),
                b'&' => self.single(TokenKind::Ampersand),
                b'|' if self.starts_with(b"||") => self.double(TokenKind::OrOr),
                b'.' if self.starts_with(b"..=") => self.triple(TokenKind::DotDotEqual),
                b'.' if self.starts_with(b"..") => self.double(TokenKind::DotDot),
                b'.' => self.single(TokenKind::Dot),
                b'0'..=b'9' => self.number(),
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.identifier(),
                _ => {
                    let character = self.source.text[start..]
                        .chars()
                        .next()
                        .expect("position is inside source");
                    self.position += character.len_utf8();
                    self.report(Diagnostic::at(
                        self.source,
                        Span::new(start, self.position),
                        format!("unexpected character `{character}`"),
                    ));
                    continue;
                }
            };
            self.tokens.push(Token {
                kind,
                span: Span::new(start, self.position),
            });
        }

        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.position, self.position),
        });
        (self.tokens, self.diagnostics)
    }

    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.position += 1;
        kind
    }

    fn double(&mut self, kind: TokenKind) -> TokenKind {
        self.position += 2;
        kind
    }

    fn triple(&mut self, kind: TokenKind) -> TokenKind {
        self.position += 3;
        kind
    }

    fn with_equal(&mut self, single: TokenKind, equal: TokenKind) -> TokenKind {
        self.position += 1;
        if self.bytes.get(self.position) == Some(&b'=') {
            self.position += 1;
            equal
        } else {
            single
        }
    }

    fn number(&mut self) -> TokenKind {
        let start = self.position;
        while self
            .bytes
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            self.position += 1;
        }
        if self.bytes.get(self.position) == Some(&b'.')
            && self.bytes.get(self.position + 1) != Some(&b'.')
        {
            self.position += 1;
            while self
                .bytes
                .get(self.position)
                .is_some_and(u8::is_ascii_digit)
            {
                self.position += 1;
            }
        }
        if matches!(self.bytes.get(self.position), Some(b'e' | b'E')) {
            self.position += 1;
            if matches!(self.bytes.get(self.position), Some(b'+' | b'-')) {
                self.position += 1;
            }
            while self
                .bytes
                .get(self.position)
                .is_some_and(u8::is_ascii_digit)
            {
                self.position += 1;
            }
        }
        if self.bytes.get(self.position) == Some(&b'_') {
            self.position += 1;
            while self
                .bytes
                .get(self.position)
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
            {
                self.position += 1;
            }
        }
        TokenKind::Number(self.source.text[start..self.position].to_string())
    }

    fn character(&mut self) -> Option<TokenKind> {
        let start = self.position;
        self.position += 1;
        let value = if self.bytes.get(self.position) == Some(&b'\\') {
            self.position += 1;
            match self.escape(start) {
                Ok(value) => value,
                Err(diagnostic) => {
                    self.report(diagnostic);
                    self.skip_to_line_end();
                    return None;
                }
            }
        } else {
            let Some(value) = self.source.text[self.position..].chars().next() else {
                self.report(Diagnostic::at(
                    self.source,
                    Span::new(start, start + 1),
                    "unterminated character literal",
                ));
                return None;
            };
            // 未转义的 `'` 与裸换行必须拒绝：与字符串字面量、格式化器保持一致。
            if matches!(value, '\'' | '\n' | '\r') {
                self.report(Diagnostic::at(
                    self.source,
                    Span::new(start, self.position + value.len_utf8()),
                    "character literal must contain exactly one character",
                ));
                if value == '\'' {
                    // 跳过这个未转义引号，下一轮从后继内容继续。
                    self.position += 1;
                } else {
                    self.skip_to_line_end();
                }
                return None;
            }
            self.position += value.len_utf8();
            value
        };
        if self.bytes.get(self.position) != Some(&b'\'') {
            self.report(Diagnostic::at(
                self.source,
                Span::new(start, self.position),
                "character literal must contain exactly one character",
            ));
            self.skip_to_line_end();
            return None;
        }
        self.position += 1;
        Some(TokenKind::Character(value))
    }

    fn string(&mut self) -> Option<TokenKind> {
        let start = self.position;
        self.position += 1;
        let mut value = String::new();

        while self.position < self.bytes.len() {
            match self.bytes[self.position] {
                b'"' => {
                    self.position += 1;
                    return Some(TokenKind::String(value));
                }
                b'\n' | b'\r' => {
                    self.report(Diagnostic::at(
                        self.source,
                        Span::new(start, self.position),
                        "unterminated string literal",
                    ));
                    return None;
                }
                b'\\' => {
                    self.position += 1;
                    match self.escape(start) {
                        Ok(character) => value.push(character),
                        Err(diagnostic) => {
                            self.report(diagnostic);
                            self.skip_to_line_end();
                            return None;
                        }
                    }
                }
                _ => {
                    let character = self.source.text[self.position..]
                        .chars()
                        .next()
                        .expect("position is inside source");
                    value.push(character);
                    self.position += character.len_utf8();
                }
            }
        }

        self.report(Diagnostic::at(
            self.source,
            Span::new(start, start + 1),
            "unterminated string literal",
        ));
        None
    }

    fn escape(&mut self, string_start: usize) -> Result<char, Diagnostic> {
        let start = self.position.saturating_sub(1);
        let byte = self.bytes.get(self.position).copied().ok_or_else(|| {
            Diagnostic::at(
                self.source,
                Span::new(string_start, string_start + 1),
                "unterminated string literal",
            )
        })?;
        self.position += 1;
        match byte {
            b'n' => Ok('\n'),
            b'r' => Ok('\r'),
            b't' => Ok('\t'),
            b'\\' => Ok('\\'),
            b'"' => Ok('"'),
            b'\'' => Ok('\''),
            b'u' => self.unicode_escape(start),
            _ => {
                // 非 ASCII 转义字符：把扫描位置推进到该字符末尾，保证诊断 span
                // 结束在字符边界上（否则渲染诊断时会切片 panic）。
                if byte >= 0x80
                    && let Some(character) = self.source.text[self.position - 1..].chars().next()
                {
                    self.position = self.position - 1 + character.len_utf8();
                }
                Err(Diagnostic::at(
                    self.source,
                    Span::new(start, self.position),
                    "unknown string escape",
                ))
            }
        }
    }

    fn unicode_escape(&mut self, start: usize) -> Result<char, Diagnostic> {
        if self.bytes.get(self.position) != Some(&b'{') {
            return Err(Diagnostic::at(
                self.source,
                Span::new(start, self.position),
                "expected `{` after `\\u`",
            ));
        }
        self.position += 1;
        let digits_start = self.position;
        while self
            .bytes
            .get(self.position)
            .is_some_and(u8::is_ascii_hexdigit)
        {
            self.position += 1;
        }
        if digits_start == self.position || self.bytes.get(self.position) != Some(&b'}') {
            return Err(Diagnostic::at(
                self.source,
                Span::new(start, self.position),
                "invalid Unicode escape",
            ));
        }
        let digits = &self.source.text[digits_start..self.position];
        self.position += 1;
        let codepoint = u32::from_str_radix(digits, 16).ok();
        codepoint.and_then(char::from_u32).ok_or_else(|| {
            Diagnostic::at(
                self.source,
                Span::new(start, self.position),
                "invalid Unicode code point",
            )
        })
    }

    fn identifier(&mut self) -> TokenKind {
        let start = self.position;
        while self
            .bytes
            .get(self.position)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            self.position += 1;
        }
        match &self.source.text[start..self.position] {
            "_" => TokenKind::Underscore,
            "fn" => TokenKind::Fn,
            "pub" => TokenKind::Pub,
            "pkg" => TokenKind::Pkg,
            "use" => TokenKind::Use,
            "return" => TokenKind::Return,
            "var" => TokenKind::Var,
            "val" => TokenKind::Val,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "loop" => TokenKind::Loop,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "as" => TokenKind::As,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "defer" => TokenKind::Defer,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "match" => TokenKind::Match,
            "extern" => TokenKind::Extern,
            "trait" => TokenKind::Trait,
            "impl" => TokenKind::Impl,
            value => TokenKind::Identifier(value.to_string()),
        }
    }

    fn skip_line_comment(&mut self) {
        self.position += 2;
        while self.position < self.bytes.len() && self.bytes[self.position] != b'\n' {
            self.position += 1;
        }
    }

    /// 跳过当前行剩余内容（不消费换行），用于字面量恢复。
    fn skip_to_line_end(&mut self) {
        while self.position < self.bytes.len() && self.bytes[self.position] != b'\n' {
            self.position += 1;
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), Diagnostic> {
        let start = self.position;
        self.position += 2;
        while self.position < self.bytes.len() {
            if self.starts_with(b"*/") {
                self.position += 2;
                return Ok(());
            }
            self.position += 1;
        }
        Err(Diagnostic::at(
            self.source,
            Span::new(start, start + 2),
            "unterminated block comment",
        ))
    }

    fn starts_with(&self, expected: &[u8]) -> bool {
        self.bytes[self.position..].starts_with(expected)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn lexes_m3_program() {
        let source = SourceFile::new(
            PathBuf::from("main.do"),
            "fn main() { var i = 0; while i < 3 && true { i += 1; } return i; }".to_string(),
        );
        let tokens = lex(&source).expect("lexing should succeed");
        assert!(matches!(tokens[0].kind, TokenKind::Fn));
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::While))
        );
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::AndAnd))
        );
        assert!(matches!(tokens.last().unwrap().kind, TokenKind::Eof));
    }

    #[test]
    fn decodes_utf8_strings_and_escapes() {
        let source = SourceFile::new(
            PathBuf::from("main.do"),
            "fn main() { println(\"你好\\n{}\", 1); }".to_string(),
        );
        let tokens = lex(&source).expect("lexing should succeed");
        assert!(
            tokens.iter().any(
                |token| matches!(&token.kind, TokenKind::String(value) if value == "你好\n{}")
            )
        );
    }

    #[test]
    fn skips_comments() {
        let source = SourceFile::new(
            PathBuf::from("main.do"),
            "// line\nfn /* block */ main() {}".to_string(),
        );
        let tokens = lex(&source).expect("lexing should succeed");
        assert_eq!(tokens.len(), 7);
    }

    #[test]
    fn recovering_lexes_past_unknown_characters() {
        let source = SourceFile::new(
            PathBuf::from("main.do"),
            "fn main() { val § = 1; }".to_string(),
        );
        let (tokens, diagnostics) = lex_recovering(&source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code(), "E0001");
        assert!(
            diagnostics[0]
                .message()
                .contains("unexpected character `§`")
        );
        // 未知字符跳过该字符后，后续 token 继续。
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::Equal))
        );
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::Number(_)))
        );
        assert!(matches!(tokens.last().unwrap().kind, TokenKind::Eof));
    }

    #[test]
    fn recovering_reports_unterminated_string_and_continues() {
        let source = SourceFile::new(
            PathBuf::from("main.do"),
            "fn main() { val s = \"abc\n    val t = 1; }".to_string(),
        );
        let (tokens, diagnostics) = lex_recovering(&source);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message().contains("unterminated string"));
        // 字符串后的下一行仍被词法分析。
        assert!(
            tokens
                .iter()
                .any(|token| matches!(&token.kind, TokenKind::Identifier(name) if name == "t"))
        );
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::Semicolon))
        );
    }

    #[test]
    fn recovering_reports_unterminated_block_comment_at_eof() {
        let source = SourceFile::new(PathBuf::from("main.do"), "fn main() {} /*".to_string());
        let (tokens, diagnostics) = lex_recovering(&source);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message()
                .contains("unterminated block comment")
        );
        assert!(matches!(tokens.last().unwrap().kind, TokenKind::Eof));
    }

    #[test]
    fn lex_keeps_first_error_behavior() {
        let source = SourceFile::new(PathBuf::from("main.do"), "fn main() { § }".to_string());
        let error = lex(&source).expect_err("lexing should fail");
        assert!(error.message().contains("unexpected character `§`"));
        assert_eq!(error.code(), "E0001");
    }
}
