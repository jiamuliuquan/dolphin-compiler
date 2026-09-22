// M19 目标工具核心：文本行统计与过滤（H19-07）。
//
// 纯函数、零分配：输入是完整字节缓冲；行/CRLF/无末尾换行规则与规格 §11 冻结一致，
// 复用 `std.text.lines`。输入必须是合法 UTF-8，否则返回 `TextError.InvalidUtf8`。
// `filter` 为空字符串表示匹配所有行（与 CLI 的“无过滤/空子串”一致）。

use std.text;
use std.text.TextError;

pub struct Stats {
    lines: usize,
    matched: usize,
    bytes: usize,
}

impl Stats {
    pub fn empty(): Stats {
        return Stats(0_usize, 0_usize, 0_usize);
    }

    pub fn lines(self: *const Self): usize {
        return self->lines;
    }

    pub fn matched(self: *const Self): usize {
        return self->matched;
    }

    pub fn bytes(self: *const Self): usize {
        return self->bytes;
    }
}

pub fn analyze(input: []const u8, filter: string): Result<Stats, TextError> {
    val has_filter = filter.bytes().len > 0_usize;
    var lines = 0_usize;
    var matched = 0_usize;
    for line in text.lines(input) {
        val validated = text.from_utf8(line);
        if validated.is_err() {
            return Result.Err(TextError.InvalidUtf8);
        }
        val content = match validated {
            Result.Ok(value) => value,
            Result.Err(error) => "",
        };
        lines += 1_usize;
        if !has_filter {
            matched += 1_usize;
        } else {
            if text.contains(content, filter) {
                matched += 1_usize;
            }
        }
    }
    return Result.Ok(Stats(lines, matched, input.len));
}
