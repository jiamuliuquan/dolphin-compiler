//! Dolphin 源码格式化器（M17）。
//!
//! 保守的 token 级格式化器：保留注释与空行，规范化缩进与行尾空白。因为不依赖
//! 完整类型检查，天然覆盖 M1-M15 的全部语法。

use dolphin_source::diagnostic::Diagnostic;

/// 单次扫描中已知的词法状态。
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Normal,
    LineComment,
    BlockComment,
    String,
    Char,
}

/// 格式化单个源码文件内容；成功时返回规范化后的完整文本。
pub fn format_source(source: &str) -> Result<String, Diagnostic> {
    let chars: Vec<char> = source.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut state = State::Normal;
    let mut depth = 0usize;
    let mut line_depth = 0usize;
    let mut line_started_in_block_comment = false;
    let mut pending_blank = false;

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        if c == '\n' {
            match state {
                State::String => return Err(Diagnostic::plain("unterminated string literal")),
                State::Char => return Err(Diagnostic::plain("unterminated char literal")),
                _ => {}
            }
            flush_line(
                &mut out,
                &mut pending_blank,
                &line,
                line_started_in_block_comment,
                line_depth,
            );
            line.clear();
            if state == State::LineComment {
                state = State::Normal;
            }
            line_started_in_block_comment = state == State::BlockComment;
            line_depth = depth;
            i += 1;
            continue;
        }

        match state {
            State::Normal => match c {
                '/' if chars.get(i + 1) == Some(&'/') => {
                    line.push('/');
                    line.push('/');
                    state = State::LineComment;
                    i += 2;
                }
                '/' if chars.get(i + 1) == Some(&'*') => {
                    line.push('/');
                    line.push('*');
                    state = State::BlockComment;
                    i += 2;
                }
                '"' => {
                    line.push(c);
                    state = State::String;
                    i += 1;
                }
                '\'' => {
                    line.push(c);
                    state = State::Char;
                    i += 1;
                }
                '{' => {
                    depth += 1;
                    line.push(c);
                    i += 1;
                }
                '}' => {
                    depth = depth.saturating_sub(1);
                    line.push(c);
                    i += 1;
                }
                _ => {
                    line.push(c);
                    i += 1;
                }
            },
            State::LineComment => {
                line.push(c);
                i += 1;
            }
            State::BlockComment => {
                if c == '*' && chars.get(i + 1) == Some(&'/') {
                    line.push('*');
                    line.push('/');
                    state = State::Normal;
                    i += 2;
                } else {
                    line.push(c);
                    i += 1;
                }
            }
            State::String => {
                if c == '\\' {
                    line.push(c);
                    if let Some(&next) = chars.get(i + 1)
                        && next != '\n'
                    {
                        line.push(next);
                        i += 2;
                        continue;
                    }
                    i += 1;
                } else if c == '"' {
                    line.push(c);
                    state = State::Normal;
                    i += 1;
                } else {
                    line.push(c);
                    i += 1;
                }
            }
            State::Char => {
                if c == '\\' {
                    line.push(c);
                    if let Some(&next) = chars.get(i + 1)
                        && next != '\n'
                    {
                        line.push(next);
                        i += 2;
                        continue;
                    }
                    i += 1;
                } else if c == '\'' {
                    line.push(c);
                    state = State::Normal;
                    i += 1;
                } else {
                    line.push(c);
                    i += 1;
                }
            }
        }
    }

    match state {
        State::BlockComment => return Err(Diagnostic::plain("unterminated block comment")),
        State::String => return Err(Diagnostic::plain("unterminated string literal")),
        State::Char => return Err(Diagnostic::plain("unterminated char literal")),
        _ => {}
    }

    if !line.is_empty() {
        flush_line(
            &mut out,
            &mut pending_blank,
            &line,
            line_started_in_block_comment,
            line_depth,
        );
    }

    if out.is_empty() {
        return Ok(String::new());
    }
    let mut result = out.join("\n");
    result.push('\n');
    Ok(result)
}

/// 把一行写入输出；`verbatim` 表示该行起始于块注释内部，保留其原始前导空白。
fn flush_line(
    out: &mut Vec<String>,
    pending_blank: &mut bool,
    line: &str,
    verbatim: bool,
    line_depth: usize,
) {
    let trimmed = line.trim_end();
    if trimmed.trim().is_empty() {
        if !out.is_empty() {
            *pending_blank = true;
        }
        return;
    }

    let content = if verbatim {
        trimmed.to_string()
    } else {
        let text = trimmed.trim_start();
        let indent_depth = if text.starts_with('}') {
            line_depth.saturating_sub(1)
        } else {
            line_depth
        };
        let mut rendered = "    ".repeat(indent_depth);
        rendered.push_str(text);
        rendered
    };

    if *pending_blank && !out.is_empty() {
        out.push(String::new());
    }
    *pending_blank = false;
    out.push(content);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(source: &str) -> String {
        format_source(source).expect("formatting should succeed")
    }

    #[test]
    fn already_formatted_is_unchanged() {
        let source = "fn main() {\n    val x = 1;\n}\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn nested_braces_are_reindented() {
        let source = "fn main() {\nval x = 1;\nif x > 0 {\nval y = 2;\n}\n}\n";
        let expected =
            "fn main() {\n    val x = 1;\n    if x > 0 {\n        val y = 2;\n    }\n}\n";
        assert_eq!(fmt(source), expected);
    }

    #[test]
    fn else_chain_is_dedented() {
        let source = "if a {\nb();\n} else if c {\nd();\n} else {\ne();\n}\n";
        let expected = "if a {\n    b();\n} else if c {\n    d();\n} else {\n    e();\n}\n";
        assert_eq!(fmt(source), expected);
    }

    #[test]
    fn double_slash_in_string_is_not_a_comment() {
        let source = "val url = \"http://x\";\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn char_literal_brace_does_not_change_depth() {
        let source = "fn main() {\n    val open = '{';\n    val close = '}';\n}\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn nested_braces_inside_strings_do_not_change_depth() {
        let source = "fn main() {\nprintln(\"{}\", 1);\n}\n";
        let expected = "fn main() {\n    println(\"{}\", 1);\n}\n";
        assert_eq!(fmt(source), expected);
    }

    #[test]
    fn multiline_block_comment_is_preserved_verbatim() {
        let source = "fn main() {\n    /*\n     * { }\n     */\n    val x = 1;\n}\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn trailing_whitespace_is_removed() {
        let source = "fn main() {   \n    val x = 1;  \n}   \n";
        let expected = "fn main() {\n    val x = 1;\n}\n";
        assert_eq!(fmt(source), expected);
    }

    #[test]
    fn multiple_blank_lines_collapse_to_one() {
        let source = "fn main() {\n    val x = 1;\n\n\n\n    val y = 2;\n}\n";
        let expected = "fn main() {\n    val x = 1;\n\n    val y = 2;\n}\n";
        assert_eq!(fmt(source), expected);
    }

    #[test]
    fn leading_and_trailing_blank_lines_are_dropped() {
        let source = "\n\nfn main() {}\n\n\n";
        assert_eq!(fmt(source), "fn main() {}\n");
    }

    #[test]
    fn all_whitespace_becomes_empty() {
        assert_eq!(fmt("   \n\t\n"), "");
        assert_eq!(fmt(""), "");
    }

    #[test]
    fn unterminated_block_comment_is_an_error() {
        let error = format_source("fn main() {\n    /* open\n").unwrap_err();
        assert!(error.to_string().contains("unterminated block comment"));
    }

    #[test]
    fn unterminated_string_is_an_error() {
        let error = format_source("val x = \"oops\n").unwrap_err();
        assert!(error.to_string().contains("unterminated string literal"));
    }

    #[test]
    fn unterminated_char_is_an_error() {
        let error = format_source("val x = 'oops\n").unwrap_err();
        assert!(error.to_string().contains("unterminated char literal"));
    }

    #[test]
    fn braces_in_line_comments_do_not_change_depth() {
        let source = "fn main() {\n    // 不配对的 } 与 {\n    val x = 1;\n}\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn inline_block_comment_does_not_change_depth() {
        let source = "fn main() {\n    val x = 1; /* { } */\n}\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn escaped_quote_does_not_end_string() {
        let source = "fn main() {\n    val text = \"a\\\"} // b\";\n}\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn escaped_quote_does_not_end_char() {
        let source = "fn main() {\n    val quote = '\\'';\n}\n";
        assert_eq!(fmt(source), source);
    }

    #[test]
    fn crlf_line_endings_are_normalized() {
        let source = "fn main() {\r\n    val x = 1;\r\n}\r\n";
        assert_eq!(fmt(source), "fn main() {\n    val x = 1;\n}\n");
    }

    #[test]
    fn representative_program_is_idempotent() {
        let source = "// 顶部注释。\nstruct Pair<T> {\nfirst: T,\nsecond: T,\n}\n\nenum Maybe<T> {\nJust(T),\nNothing,\n}\n\nimpl<T> Pair<T> {\nfn swapped(self): Pair<T> {\nreturn Pair<T> {\nfirst: self.second,\nsecond: self.first,\n};\n}\n}\n\nfn classify(n: i32): i32 {\nvar total = 0;\nfor i in 0..=n {\ntotal += i;\n}\nwhile total > 100 {\ntotal -= 1;\n}\nif total == 0 {\nreturn 0;\n} else if total < 10 {\nreturn 1;\n} else {\nreturn 2;\n}\n}\n\nfn main() {\n// 字符串中的 http:// 不是注释，\"{\" 也不计入深度。\nval tag = \"http://x/{}\";\nval brace = '{';\nval m = Maybe.Just(42);\nval v = match m {\nMaybe.Just(x) => x,\nMaybe.Nothing => 0,\n};\nprintln(\"{}\", tag, brace, v);\n}\n";
        let once = fmt(source);
        let twice = fmt(&once);
        assert_eq!(once, twice);
    }
}
