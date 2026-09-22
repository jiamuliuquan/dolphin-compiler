// dtext 自测（`dc test`）：参数解析（纯函数）与通过 path 依赖调用 textstats 公开 API，
// 以及输出数字格式化。真正的进程级三路结果由 Rust 集成测试 `tests/m19_app.rs` 覆盖。

use std.mem;
use std.test.expect;
use std.text;
use std.text.Builder;
use textstats;

fn options_of(result: Result<Options, UsageError>): Options {
    return match result {
        Result.Ok(value) => value,
        Result.Err(error) => Options(false, "", "", false),
    };
}

fn usage_error_of(result: Result<Options, UsageError>): UsageError {
    return match result {
        Result.Err(item) => item,
        Result.Ok(value) => UsageError.InvalidUtf8,
    };
}

/// 0=UnknownOption、1=MissingFilterValue、2=TooManyArguments、3=InvalidUtf8。
fn usage_code(error: UsageError): i32 {
    return match error {
        UsageError.UnknownOption => 0_i32,
        UsageError.MissingFilterValue => 1_i32,
        UsageError.TooManyArguments => 2_i32,
        UsageError.InvalidUtf8 => 3_i32,
    };
}

fn parse_one(first: string): Result<Options, UsageError> {
    val args = mem.alloc<string>(1_usize);
    defer mem.free<string>(args);
    args[0] = first;
    return parse_args_from(args);
}

fn parse_two(first: string, second: string): Result<Options, UsageError> {
    val args = mem.alloc<string>(2_usize);
    defer mem.free<string>(args);
    args[0] = first;
    args[1] = second;
    return parse_args_from(args);
}

fn test_parse_help() {
    val parsed = parse_two("--help", "--bogus");
    expect(parsed.is_ok());
    val options = options_of(parsed);
    expect(options.help);
}

fn test_parse_filter_and_path() {
    val filtered = parse_two("--filter", "needle");
    expect(filtered.is_ok());
    val with_filter = options_of(filtered);
    expect(!with_filter.help);
    expect(with_filter.filter == "needle");
    expect(!with_filter.has_path);

    val path = parse_one("file.txt");
    expect(path.is_ok());
    val with_path = options_of(path);
    expect(with_path.has_path);
    expect(with_path.path == "file.txt");

    val dash = parse_one("-");
    expect(dash.is_ok());
    val dash_options = options_of(dash);
    expect(dash_options.has_path);
    expect(dash_options.path == "-");
}

fn test_parse_usage_errors() {
    expect(usage_code(usage_error_of(parse_one("--bogus"))) == 0_i32);
    expect(usage_code(usage_error_of(parse_one("--filter"))) == 1_i32);
    expect(usage_code(usage_error_of(parse_two("a", "b"))) == 2_i32);
    expect(usage_code(usage_error_of(parse_two("file", "--filter"))) == 1_i32);
    expect(usage_code(usage_error_of(parse_two("-", "-"))) == 2_i32);
}

fn test_dependency_analyze() {
    val input = "a\r\nb\n";
    val result = textstats.analyze(input.bytes(), "b");
    expect(result.is_ok());
    val stats = match result {
        Result.Ok(value) => value,
        Result.Err(error) => textstats.Stats::empty(),
    };
    expect(stats.lines() == 2_usize);
    expect(stats.matched() == 1_usize);
    expect(stats.bytes() == 5_usize);
}

fn test_number_formatting() {
    var out = Builder::init();
    defer out.deinit();
    append_line(&out, "lines=", 0_usize);
    append_line(&out, "matched=", 123_usize);
    append_line(&out, "bytes=", 105000_usize);
    val view = match text.from_utf8(out.view()) {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    expect(view == "lines=0\nmatched=123\nbytes=105000\n");
}
