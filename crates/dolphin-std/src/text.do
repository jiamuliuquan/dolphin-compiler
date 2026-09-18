pkg std.text;

// 拥有型 UTF-8 字符串与零分配视图工具（M15-B/R14）。
// `string` 是只读视图；`String` 是可 deinit 的拥有型缓冲。

use std.mem;

pub enum TextError {
    InvalidUtf8,
    InvalidBoundary,
    OutOfBounds,
}

pub struct String {
    bytes: []u8,
}

impl String {
    pub fn from(s: string): String {
        val count = length(s);
        var buffer = mem.alloc<u8>(count);
        if count > 0_usize {
            mem.copy<u8>(buffer, s.bytes());
        }
        return String(buffer);
    }

    pub fn view(self: *const Self): string {
        return string.from_bytes(self->bytes);
    }

    pub fn clone(self: *const Self): String {
        return String::from(self.view());
    }

    pub fn deinit(self: *Self) {
        mem.free<u8>(self->bytes);
        self->bytes = mem.alloc<u8>(0_usize);
    }
}

pub fn concat(a: string, b: string): String {
    val left = length(a);
    val right = length(b);
    val total = left + right;
    if total < left {
        val overflow = mem.alloc<u8>(right);
        mem.free<u8>(overflow);
        return String(mem.alloc<u8>(0_usize));
    }
    var buffer = mem.alloc<u8>(total);
    if left > 0_usize {
        val target = buffer.slice(0_usize, left);
        mem.copy<u8>(target, a.bytes());
    }
    if right > 0_usize {
        val target = buffer.slice(left, total);
        mem.copy<u8>(target, b.bytes());
    }
    return String(buffer);
}

pub fn trim(s: string): string {
    val bytes = s.bytes();
    val count = bytes.len;
    var start = 0_usize;
    var end = count;
    while start < end && is_ascii_space(bytes[start]) {
        start = start + 1_usize;
    }
    while end > start && is_ascii_space(bytes[end - 1_usize]) {
        end = end - 1_usize;
    }
    val trimmed = bytes.slice(start, end);
    return string.from_bytes(trimmed);
}

pub fn substring(s: string, start: usize, end: usize): Result<string, TextError> {
    val bytes = s.bytes();
    val count = bytes.len;
    if start > end || end > count {
        return Result.Err(TextError.OutOfBounds);
    }
    if !is_boundary(bytes, start, count) || !is_boundary(bytes, end, count) {
        return Result.Err(TextError.InvalidBoundary);
    }
    return Result.Ok(string.from_bytes(bytes.slice(start, end)));
}

pub fn from_utf8(bytes: []const u8): Result<string, TextError> {
    if !mem.is_valid_utf8(bytes) {
        return Result.Err(TextError.InvalidUtf8);
    }
    return Result.Ok(string.from_bytes(bytes));
}

pub fn starts_with(s: string, part: string): bool {
    val source = s.bytes();
    val needle = part.bytes();
    if needle.len > source.len {
        return false;
    }
    var index = 0_usize;
    while index < needle.len {
        if source[index] != needle[index] {
            return false;
        }
        index = index + 1_usize;
    }
    return true;
}

pub fn ends_with(s: string, part: string): bool {
    val source = s.bytes();
    val needle = part.bytes();
    if needle.len > source.len {
        return false;
    }
    var offset = source.len - needle.len;
    var index = 0_usize;
    while index < needle.len {
        if source[offset + index] != needle[index] {
            return false;
        }
        index = index + 1_usize;
    }
    return true;
}

pub fn contains(s: string, part: string): bool {
    val source = s.bytes();
    val needle = part.bytes();
    if needle.len == 0_usize || needle.len > source.len {
        return false;
    }
    var start = 0_usize;
    while start + needle.len <= source.len {
        if region_equal(source, start, needle) {
            return true;
        }
        start = start + 1_usize;
    }
    return false;
}

fn region_equal(source: []const u8, start: usize, needle: []const u8): bool {
    var index = 0_usize;
    while index < needle.len {
        if source[start + index] != needle[index] {
            return false;
        }
        index = index + 1_usize;
    }
    return true;
}

fn is_ascii_space(byte: u8): bool {
    return byte == 32_u8 || (byte >= 9_u8 && byte <= 13_u8);
}

fn is_continuation(byte: u8): bool {
    return byte >= 128_u8 && byte <= 191_u8;
}

fn is_boundary(bytes: []const u8, index: usize, count: usize): bool {
    if index == 0_usize || index == count {
        return true;
    }
    return !is_continuation(bytes[index]);
}
