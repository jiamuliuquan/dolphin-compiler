use crate::diagnostic::Diagnostic;
use crate::source::{SourceFile, Span};
use crate::token::{Token, TokenKind};

pub fn lex(source: &SourceFile) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source).lex_all()
}

struct Lexer<'a> {
    source: &'a SourceFile,
    bytes: &'a [u8],
    position: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a SourceFile) -> Self {
        Self {
            source,
            bytes: source.text.as_bytes(),
            position: 0,
            tokens: Vec::new(),
        }
    }

    fn lex_all(mut self) -> Result<Vec<Token>, Diagnostic> {
        while self.position < self.bytes.len() {
            if self.bytes[self.position].is_ascii_whitespace() {
                self.position += 1;
                continue;
            }
            if self.starts_with(b"//") {
                self.skip_line_comment();
                continue;
            }
            if self.starts_with(b"/*") {
                self.skip_block_comment()?;
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
                b':' => self.single(TokenKind::Colon),
                b',' => self.single(TokenKind::Comma),
                b';' => self.single(TokenKind::Semicolon),
                b'"' => self.string()?,
                b'\'' => self.character()?,
                b'+' => self.with_equal(TokenKind::Plus, TokenKind::PlusEqual),
                b'-' => {
                    if self.starts_with(b"->") {
                        self.double(TokenKind::Arrow)
                    } else {
                        self.with_equal(TokenKind::Minus, TokenKind::MinusEqual)
                    }
                }
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
                b'&' => self.single(TokenKind::Amper),
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
                    return Err(Diagnostic::at(
                        self.source,
                        Span::new(start, start + character.len_utf8()),
                        format!("unexpected character `{character}`"),
                    ));
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
        Ok(self.tokens)
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

    fn character(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.position;
        self.position += 1;
        let value = if self.bytes.get(self.position) == Some(&b'\\') {
            self.position += 1;
            self.escape(start)?
        } else {
            let value = self.source.text[self.position..]
                .chars()
                .next()
                .ok_or_else(|| {
                    Diagnostic::at(
                        self.source,
                        Span::new(start, start + 1),
                        "unterminated character literal",
                    )
                })?;
            self.position += value.len_utf8();
            value
        };
        if self.bytes.get(self.position) != Some(&b'\'') {
            return Err(Diagnostic::at(
                self.source,
                Span::new(start, self.position),
                "character literal must contain exactly one character",
            ));
        }
        self.position += 1;
        Ok(TokenKind::Character(value))
    }

    fn string(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.position;
        self.position += 1;
        let mut value = String::new();

        while self.position < self.bytes.len() {
            match self.bytes[self.position] {
                b'"' => {
                    self.position += 1;
                    return Ok(TokenKind::String(value));
                }
                b'\n' | b'\r' => {
                    return Err(Diagnostic::at(
                        self.source,
                        Span::new(start, self.position),
                        "unterminated string literal",
                    ));
                }
                b'\\' => {
                    self.position += 1;
                    value.push(self.escape(start)?);
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

        Err(Diagnostic::at(
            self.source,
            Span::new(start, start + 1),
            "unterminated string literal",
        ))
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
            _ => Err(Diagnostic::at(
                self.source,
                Span::new(start, self.position),
                "unknown string escape",
            )),
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
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "match" => TokenKind::Match,
            "defer" => TokenKind::Defer,
            "try" => TokenKind::Try,
            value => TokenKind::Identifier(value.to_string()),
        }
    }

    fn skip_line_comment(&mut self) {
        self.position += 2;
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
}
