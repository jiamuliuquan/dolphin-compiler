//! H19-04 TEXT-01..04：行遍历/CRLF 规则、UTF-8 边界、整数解析与增长式 Builder。
//!
//! 期望值来自 M19 规格冻结规则（`docs/proposal-m19-cli-stdlib.md` 第 11 节与
//! [M18-M21 计划] H19-04），由人工推导后固定写死；每个用例在可用后端 ×
//! Dolphin Debug/Release 上构建并断言固定 stdout/stderr/exit（Debug 的
//! stderr 为空同时覆盖 Builder 释放无泄漏、无未关闭句柄报告）。

mod support;

use support::assert_runs;

const TEXT_01_PROGRAM: &str = r#"
use std.text;
use std.text.lines;

fn sum(bytes: []const u8): usize {
    var total = 0_usize;
    var index = 0_usize;
    while index < bytes.len {
        total += bytes[index] as usize;
        index += 1_usize;
    }
    return total;
}

fn report(label: string, input: string) {
    var count = 0_usize;
    println("case={}", label);
    for line in lines(input.bytes()) {
        println("line len={} sum={}", line.len, sum(line));
        count += 1_usize;
    }
    println("count={}", count);
}

fn main() {
    report("empty", "");
    report("single", "a");
    report("newline", "a\n");
    report("double", "a\n\n");
    report("crlf", "a\r\nb");
    report("lone-cr", "a\rb");
    report("bare-lf", "\n");
    report("double-cr", "a\r\r\n");
    report("no-final", "x\ny");
    return 0;
}
"#;

const TEXT_02_PROGRAM: &str = r#"
use std.mem;
use std.text;
use std.text.TextError;
use std.text.lines;

fn label(error: TextError): string {
    return match error {
        TextError.InvalidUtf8 => "utf8",
        TextError.InvalidBoundary => "boundary",
        TextError.OutOfBounds => "bounds",
    };
}

fn show(result: Result<string, TextError>): string {
    return match result {
        Result.Ok(value) => value,
        Result.Err(error) => label(error),
    };
}

fn sum(bytes: []const u8): usize {
    var total = 0_usize;
    var index = 0_usize;
    while index < bytes.len {
        total += bytes[index] as usize;
        index += 1_usize;
    }
    return total;
}

fn main() {
    val s = "a\u{e9}\u{65e5}";
    println("bytes={}", s.bytes().len);
    println("whole={}", show(text.substring(s, 0_usize, 6_usize)));
    println("e-acute={}", show(text.substring(s, 1_usize, 3_usize)));
    println("kanji={}", show(text.substring(s, 3_usize, 6_usize)));
    println("end-inside={}", show(text.substring(s, 0_usize, 2_usize)));
    println("start-inside={}", show(text.substring(s, 2_usize, 6_usize)));
    println("pair-inside={}", show(text.substring(s, 1_usize, 2_usize)));
    println("past-end={}", show(text.substring(s, 0_usize, 7_usize)));
    println("reversed={}", show(text.substring(s, 4_usize, 3_usize)));
    println("empty={}", show(text.substring(s, 0_usize, 0_usize)));

    val input = "\u{e9}\n\u{65e5}\r\nx";
    var count = 0_usize;
    for line in lines(input.bytes()) {
        println("line len={} sum={}", line.len, sum(line));
        count += 1_usize;
    }
    println("lines={}", count);

    val raw = mem.alloc<u8>(3_usize);
    defer mem.free(raw);
    raw[0] = 97_u8;
    raw[1] = 255_u8;
    raw[2] = 98_u8;
    val invalid = text.from_utf8(raw);
    println("invalid={}", invalid.is_err());
    return 0;
}
"#;

const TEXT_03_PROGRAM: &str = r#"
use std.text.NumberError;
use std.text.parse_i64;
use std.text.parse_u64;

fn label(error: NumberError): string {
    return match error {
        NumberError.Empty => "empty",
        NumberError.InvalidDigit => "invalid",
        NumberError.Overflow => "overflow",
    };
}

fn check_i(input: string) {
    val result = parse_i64(input);
    if result.is_ok() {
        val value = match result {
            Result.Ok(item) => item,
            Result.Err(error) => 0_i64,
        };
        println("i64[{}]=ok {}", input, value);
    } else {
        val name = match result {
            Result.Err(item) => label(item),
            Result.Ok(value) => "ok",
        };
        println("i64[{}]=err {}", input, name);
    }
}

fn check_u(input: string) {
    val result = parse_u64(input);
    if result.is_ok() {
        val value = match result {
            Result.Ok(item) => item,
            Result.Err(error) => 0_u64,
        };
        println("u64[{}]=ok {}", input, value);
    } else {
        val name = match result {
            Result.Err(item) => label(item),
            Result.Ok(value) => "ok",
        };
        println("u64[{}]=err {}", input, name);
    }
}

fn main() {
    check_i("0");
    check_i("-0");
    check_i("42");
    check_i("+42");
    check_i("-42");
    check_i("9223372036854775807");
    check_i("-9223372036854775808");
    check_i("9223372036854775808");
    check_i("-9223372036854775809");
    check_i("99999999999999999999");
    check_i("");
    check_i("-");
    check_i("+");
    check_i("12a");
    check_i(" 1");
    check_i("1 ");
    check_i("0x10");
    check_i("1_000");
    check_u("0");
    check_u("18446744073709551615");
    check_u("18446744073709551616");
    check_u("-1");
    check_u("+1");
    check_u("");
    check_u("abc");
    return 0;
}
"#;

const TEXT_04_PROGRAM: &str = r#"
use std.mem;
use std.text;
use std.text.Builder;

fn sum(bytes: []const u8): usize {
    var total = 0_usize;
    var index = 0_usize;
    while index < bytes.len {
        total += bytes[index] as usize;
        index += 1_usize;
    }
    return total;
}

fn main() {
    var builder = Builder::init();
    defer builder.deinit();
    var step = 0_usize;
    while step < 200_usize {
        builder.append("0123456789");
        step += 1_usize;
    }
    val grown = builder.view();
    val grown_text = text.from_utf8(grown);
    println("grown-ok={} len={} sum={}", grown_text.is_ok(), builder.len(), sum(grown));
    builder.consume(10_usize);
    val after = builder.view();
    println("after-consume len={} first={}", builder.len(), after[0_usize]);
    builder.consume(100000_usize);
    println("over-consume empty={} len={}", builder.is_empty(), builder.len());
    builder.append("xy");
    println("reuse len={}", builder.len());
    builder.clear();
    println("after-clear empty={} len={}", builder.is_empty(), builder.len());

    var small = Builder::with_capacity(4_usize);
    defer small.deinit();
    small.append("ab");
    val extra = "cdef";
    small.append_bytes(extra.bytes());
    val small_text = match text.from_utf8(small.view()) {
        Result.Ok(value) => value,
        Result.Err(error) => "<bad>",
    };
    println("small={}", small_text);

    val lead = mem.alloc<u8>(1_usize);
    defer mem.free(lead);
    lead[0] = 195_u8;
    val trail = mem.alloc<u8>(1_usize);
    defer mem.free(trail);
    trail[0] = 169_u8;
    var split = Builder::init();
    defer split.deinit();
    split.append_bytes(lead);
    split.append_bytes(trail);
    val split_text = text.from_utf8(split.view());
    println("split-ok={} split-len={}", split_text.is_ok(), split.len());
    return 0;
}
"#;

/// TEXT-01：空串、`\n`/`\n\n` 边界、CRLF、孤立 `\r`、无末尾换行。
#[test]
fn text_01_lines_crlf_no_final_newline() {
    let expected = concat!(
        "case=empty\n",
        "count=0\n",
        "case=single\n",
        "line len=1 sum=97\n",
        "count=1\n",
        "case=newline\n",
        "line len=1 sum=97\n",
        "count=1\n",
        "case=double\n",
        "line len=1 sum=97\n",
        "line len=0 sum=0\n",
        "count=2\n",
        "case=crlf\n",
        "line len=1 sum=97\n",
        "line len=1 sum=98\n",
        "count=2\n",
        "case=lone-cr\n",
        "line len=3 sum=208\n",
        "count=1\n",
        "case=bare-lf\n",
        "line len=0 sum=0\n",
        "count=1\n",
        "case=double-cr\n",
        "line len=2 sum=110\n",
        "count=1\n",
        "case=no-final\n",
        "line len=1 sum=120\n",
        "line len=1 sum=121\n",
        "count=2\n",
    );
    assert_runs(&[("src/main.do", TEXT_01_PROGRAM)], expected, 0);
}

/// TEXT-02：多字节字符边界、`substring` 三类错误、`from_utf8` 非法字节、行遍历保字节。
#[test]
fn text_02_utf8_boundaries() {
    let expected = concat!(
        "bytes=6\n",
        "whole=a\u{e9}\u{65e5}\n",
        "e-acute=\u{e9}\n",
        "kanji=\u{65e5}\n",
        "end-inside=boundary\n",
        "start-inside=boundary\n",
        "pair-inside=boundary\n",
        "past-end=bounds\n",
        "reversed=bounds\n",
        "empty=\n",
        "line len=2 sum=364\n",
        "line len=3 sum=546\n",
        "line len=1 sum=120\n",
        "lines=3\n",
        "invalid=true\n",
    );
    assert_runs(&[("src/main.do", TEXT_02_PROGRAM)], expected, 0);
}

/// TEXT-03：`parse_i64`/`parse_u64` 的符号、极值、溢出与非法输入都返回 `Result`，不 trap。
#[test]
fn text_03_parse_signed_extremes_overflow() {
    let expected = concat!(
        "i64[0]=ok 0\n",
        "i64[-0]=ok 0\n",
        "i64[42]=ok 42\n",
        "i64[+42]=ok 42\n",
        "i64[-42]=ok -42\n",
        "i64[9223372036854775807]=ok 9223372036854775807\n",
        "i64[-9223372036854775808]=ok -9223372036854775808\n",
        "i64[9223372036854775808]=err overflow\n",
        "i64[-9223372036854775809]=err overflow\n",
        "i64[99999999999999999999]=err overflow\n",
        "i64[]=err empty\n",
        "i64[-]=err invalid\n",
        "i64[+]=err invalid\n",
        "i64[12a]=err invalid\n",
        "i64[ 1]=err invalid\n",
        "i64[1 ]=err invalid\n",
        "i64[0x10]=err invalid\n",
        "i64[1_000]=err invalid\n",
        "u64[0]=ok 0\n",
        "u64[18446744073709551615]=ok 18446744073709551615\n",
        "u64[18446744073709551616]=err overflow\n",
        "u64[-1]=err invalid\n",
        "u64[+1]=ok 1\n",
        "u64[]=err empty\n",
        "u64[abc]=err invalid\n",
    );
    assert_runs(&[("src/main.do", TEXT_03_PROGRAM)], expected, 0);
}

/// TEXT-04：Builder 多次扩容后内容正确、`consume`/`clear`/复用、跨 append 的 UTF-8 边界、deinit 无泄漏。
#[test]
fn text_04_builder_grow_and_deinit() {
    let expected = concat!(
        "grown-ok=true len=2000 sum=105000\n",
        "after-consume len=1990 first=48\n",
        "over-consume empty=true len=0\n",
        "reuse len=2\n",
        "after-clear empty=true len=0\n",
        "small=abcdef\n",
        "split-ok=true split-len=2\n",
    );
    assert_runs(&[("src/main.do", TEXT_04_PROGRAM)], expected, 0);
}
