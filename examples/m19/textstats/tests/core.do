// textstats 自测（`dc test`）：行/CRLF/无末尾换行、过滤、字节数与非法 UTF-8。

use std.mem;
use std.test.expect;

fn lines_of(input: string, filter: string): usize {
    val bytes = input.bytes();
    val result = analyze(bytes, filter);
    val stats = match result {
        Result.Ok(value) => value,
        Result.Err(error) => Stats::empty(),
    };
    return stats.lines();
}

fn matched_of(input: string, filter: string): usize {
    val bytes = input.bytes();
    val result = analyze(bytes, filter);
    val stats = match result {
        Result.Ok(value) => value,
        Result.Err(error) => Stats::empty(),
    };
    return stats.matched();
}

fn bytes_of(input: string): usize {
    val raw = input.bytes();
    val result = analyze(raw, "");
    val stats = match result {
        Result.Ok(value) => value,
        Result.Err(error) => Stats::empty(),
    };
    return stats.bytes();
}

fn test_line_rules() {
    expect(lines_of("", "") == 0_usize);
    expect(lines_of("a", "") == 1_usize);
    expect(lines_of("a\n", "") == 1_usize);
    expect(lines_of("a\n\n", "") == 2_usize);
    expect(lines_of("a\r\nb", "") == 2_usize);
    expect(lines_of("a\rb", "") == 1_usize);
    expect(lines_of("\n", "") == 1_usize);
}

fn test_filter_rules() {
    expect(matched_of("a\nb\nc", "") == 3_usize);
    expect(matched_of("a\nb\nc", "b") == 1_usize);
    expect(matched_of("alpha\nbeta", "a") == 2_usize);
    expect(matched_of("alpha\nbeta", "zzz") == 0_usize);
    expect(matched_of("a\r\nb", "a") == 1_usize);
    expect(matched_of("a\r\nb", "a\r") == 0_usize);
    expect(matched_of("a\r", "a\r") == 1_usize);
    expect(matched_of("", "x") == 0_usize);
}

fn test_unicode_filter() {
    expect(matched_of("你好世界\nhello", "世界") == 1_usize);
    expect(matched_of("你好世界\nhello", "世") == 1_usize);
    expect(matched_of("你好世界\nhello", "xyz") == 0_usize);
}

fn test_byte_count() {
    expect(bytes_of("") == 0_usize);
    expect(bytes_of("a\nb") == 3_usize);
    expect(bytes_of("你好\n") == 7_usize);
}

fn test_invalid_utf8() {
    val raw = mem.alloc<u8>(2_usize);
    defer mem.free<u8>(raw);
    raw[0] = 97_u8;
    raw[1] = 255_u8;
    val without_filter = analyze(raw, "");
    expect(without_filter.is_err());
    val with_filter = analyze(raw, "a");
    expect(with_filter.is_err());
}
