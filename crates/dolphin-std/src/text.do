pkg std.text;

// 拥有型 UTF-8 字符串与零分配视图工具（M15-B/R14）。
// `string` 是只读视图；`String` 是可 deinit 的拥有型缓冲。

use std.Iterator;
use std.Option;
use std.mem;

pub enum TextError {
    InvalidUtf8,
    InvalidBoundary,
    OutOfBounds,
}

// 数值解析错误（M19/H19-04）：解析失败返回 Result，不触发算术 trap（101）。
pub enum NumberError {
    Empty,
    InvalidDigit,
    Overflow,
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

// 行遍历（M19/H19-04）：按 `\n` 切分字节，零分配。
//
// 规则（与目标工具 `dtext` 的冻结定义一致）：
// - 以 `\n` 分隔；行内容去掉紧邻 `\n` 前的一个 `\r`（CRLF），孤立 `\r` 保留；
// - 最后一段无 `\n` 时仅在非空时作为独立一行；空输入 0 行。
// `Lines` 产出的 `[]const u8` 是**借用视图**，有效到源字节失效；不做 UTF-8
// 校验（行边界是 ASCII 字节），需要文本时用 `from_utf8` 校验。
pub struct Lines {
    storage: []const u8,
    index: usize,
}

impl Lines {
    pub fn init(storage: []const u8): Lines {
        return Lines(storage, 0_usize);
    }
}

impl Iterator for Lines {
    type Item = []const u8;

    fn next(self: *Self): Option<[]const u8> {
        if self->index >= self->storage.len {
            return Option.None;
        }
        val storage = self->storage;
        val start = self->index;
        var end = start;
        while end < storage.len && storage[end] != 10_u8 {
            end += 1_usize;
        }
        if end < storage.len {
            var content_end = end;
            if content_end > start && storage[content_end - 1_usize] == 13_u8 {
                content_end -= 1_usize;
            }
            self->index = end + 1_usize;
            return Option.Some(storage.slice(start, content_end));
        }
        self->index = storage.len;
        return Option.Some(storage.slice(start, storage.len));
    }
}

pub fn lines(bytes: []const u8): Lines {
    return Lines::init(bytes);
}

// 增长式字节缓冲（M19/H19-04）。
//
// `Builder` 是**拥有型**缓冲：`deinit` 释放；`view()` 返回**借用视图**，
// 在下一次 `append`/`append_bytes`/`consume`/`clear`/`deinit` 之后失效
// （扩容会替换底层存储）。构造中的内容可能是暂不完整的 UTF-8 序列，
// 因此 `view()` 返回原始字节；需要文本时用 `from_utf8` 校验。
pub struct Builder {
    bytes: []u8,
    used: usize,
}

impl Builder {
    pub fn init(): Builder {
        return Builder(mem.alloc<u8>(0_usize), 0_usize);
    }

    pub fn with_capacity(capacity: usize): Builder {
        return Builder(mem.alloc<u8>(capacity), 0_usize);
    }

    pub fn len(self: *const Self): usize {
        return self->used;
    }

    pub fn is_empty(self: *const Self): bool {
        return self->used == 0_usize;
    }

    pub fn append(self: *Self, s: string) {
        self.append_bytes(s.bytes());
    }

    pub fn append_bytes(self: *Self, bytes: []const u8) {
        // 长度和溢出时走分配失败通道（102），不做算术 trap。
        if bytes.len > 18446744073709551615_usize - self->used {
            val overflow = mem.alloc<u8>(bytes.len);
            mem.free<u8>(overflow);
            return;
        }
        val required = self->used + bytes.len;
        if required > self->bytes.len {
            self.grow(required);
        }
        if bytes.len > 0_usize {
            val storage = self->bytes;
            val target = storage.slice(self->used, required);
            mem.copy<u8>(target, bytes);
        }
        self->used = required;
    }

    fn grow(self: *Self, required: usize) {
        val current = self->bytes.len;
        var next = 64_usize;
        if current >= 64_usize {
            if current > 9223372036854775807_usize {
                val overflow = mem.alloc<u8>(current);
                mem.free<u8>(overflow);
                return;
            }
            next = current * 2_usize;
        }
        if next < required {
            next = required;
        }
        var replacement = mem.alloc<u8>(next);
        val previous = self->bytes;
        val count = self->used;
        if count > 0_usize {
            val target = replacement.slice(0_usize, count);
            val source = previous.slice(0_usize, count);
            mem.copy<u8>(target, source);
        }
        mem.free<u8>(previous);
        self->bytes = replacement;
    }

    pub fn view(self: *const Self): []const u8 {
        val storage = self->bytes;
        return storage.slice(0_usize, self->used);
    }

    // 丢弃前 count 字节并保留剩余；count >= len 时清空。视图失效。
    pub fn consume(self: *Self, count: usize) {
        if count >= self->used {
            self->used = 0_usize;
            return;
        }
        val remaining = self->used - count;
        val storage = self->bytes;
        val source = storage.slice(count, self->used);
        val target = storage.slice(0_usize, remaining);
        mem.copy<u8>(target, source);
        self->used = remaining;
    }

    pub fn clear(self: *Self) {
        self->used = 0_usize;
    }

    pub fn deinit(self: *Self) {
        mem.free<u8>(self->bytes);
        self->bytes = mem.alloc<u8>(0_usize);
        self->used = 0_usize;
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

// 整数解析（M19/H19-04）：只接受可选符号 + ASCII 数字，无空白/下划线/进制前缀；
// 溢出在乘加之前检查，绝不触发算术 trap。`parse_i64` 接受一个可选 `-`/`+`；
// `parse_u64` 只接受可选 `+`。`""` → `Empty`，仅符号或无数字 → `InvalidDigit`。
pub fn parse_i64(s: string): Result<i64, NumberError> {
    val bytes = s.bytes();
    if bytes.len == 0_usize {
        return Result.Err(NumberError.Empty);
    }
    var negative = false;
    var index = 0_usize;
    if bytes[0] == 45_u8 {
        negative = true;
        index = 1_usize;
    } else {
        if bytes[0] == 43_u8 {
            index = 1_usize;
        }
    }
    if index >= bytes.len {
        return Result.Err(NumberError.InvalidDigit);
    }
    var value = 0_i64;
    while index < bytes.len {
        val byte = bytes[index];
        if byte < 48_u8 || byte > 57_u8 {
            return Result.Err(NumberError.InvalidDigit);
        }
        val digit = (byte - 48_u8) as i64;
        if negative {
            // 负数按 i64 直接累积，使 `-9223372036854775808` 可表示。
            if value < (-9223372036854775808_i64 + digit) / 10_i64 {
                return Result.Err(NumberError.Overflow);
            }
            value = value * 10_i64 - digit;
        } else {
            if value > (9223372036854775807_i64 - digit) / 10_i64 {
                return Result.Err(NumberError.Overflow);
            }
            value = value * 10_i64 + digit;
        }
        index += 1_usize;
    }
    return Result.Ok(value);
}

pub fn parse_u64(s: string): Result<u64, NumberError> {
    val bytes = s.bytes();
    if bytes.len == 0_usize {
        return Result.Err(NumberError.Empty);
    }
    var index = 0_usize;
    if bytes[0] == 43_u8 {
        index = 1_usize;
    }
    if index >= bytes.len {
        return Result.Err(NumberError.InvalidDigit);
    }
    var value = 0_u64;
    while index < bytes.len {
        val byte = bytes[index];
        if byte < 48_u8 || byte > 57_u8 {
            return Result.Err(NumberError.InvalidDigit);
        }
        val digit = (byte - 48_u8) as u64;
        if value > (18446744073709551615_u64 - digit) / 10_u64 {
            return Result.Err(NumberError.Overflow);
        }
        value = value * 10_u64 + digit;
        index += 1_usize;
    }
    return Result.Ok(value);
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
