// M19 目标工具 `dtext` 的应用逻辑（H19-07）。
//
// `src/main.do` 只是入口；参数解析、输入读取、统计输出与退出码都在这里，失败路径
// 全部用 `Result`/`match`/`defer`（H19-06 模式）：用法错误 2、I/O 或编码错误 1。
//
// 用法（冻结）：
//   dtext [--help] [--filter <text>] [<path>]
// 无 path 或 path == "-" 读 stdin；输出固定三行 `lines=`/`matched=`/`bytes=`。
// 输入累积到 `Builder` 后交给 `textstats.analyze`（整缓冲校验 UTF-8，错误时不输出统计）。

use std.error.Error;
use std.error.ErrorKind;
use std.fs.open;
use std.fs.OpenMode;
use std.io.Stream;
use std.io.eprint;
use std.io.stdin;
use std.io.stdout;
use std.mem;
use std.process.arg;
use std.process.arg_count;
use std.text.Builder;
use textstats;

struct Options {
    help: bool,
    filter: string,
    path: string,
    has_path: bool,
}

enum UsageError {
    UnknownOption,
    MissingFilterValue,
    TooManyArguments,
    InvalidUtf8,
}

/// `-` 是 stdin 占位符；只有长度 >= 2 且以 `-` 开头才算选项。
fn is_option(text: string): bool {
    val bytes = text.bytes();
    if bytes.len < 2_usize {
        return false;
    }
    return bytes[0_usize] == 45_u8;
}

/// 解析应用参数（不含程序名）。纯函数，供 `dc test` 直接覆盖。
pub fn parse_args_from(args: []const string): Result<Options, UsageError> {
    var options = Options(false, "", "", false);
    var index = 0_usize;
    while index < args.len {
        val text = args[index];
        if text == "--help" {
            options.help = true;
            return Result.Ok(options);
        } else {
            if text == "--filter" {
                if index + 1_usize >= args.len {
                    return Result.Err(UsageError.MissingFilterValue);
                }
                options.filter = args[index + 1_usize];
                index += 2_usize;
            } else {
                if is_option(text) {
                    return Result.Err(UsageError.UnknownOption);
                }
                if options.has_path {
                    return Result.Err(UsageError.TooManyArguments);
                }
                options.path = text;
                options.has_path = true;
                index += 1_usize;
            }
        }
    }
    return Result.Ok(options);
}

/// 把进程参数（跳过 arg(0)）复制成借用视图数组；非 UTF-8 参数返回 `InvalidUtf8`。
fn parse_process_args(): Result<Options, UsageError> {
    val count = arg_count();
    var total = 0_usize;
    if count > 0_usize {
        total = count - 1_usize;
    }
    val args = mem.alloc<string>(total);
    defer mem.free<string>(args);
    var index = 0_usize;
    while index < total {
        val current = arg(index + 1_usize);
        if current.is_err() {
            return Result.Err(UsageError.InvalidUtf8);
        }
        args[index] = match current {
            Result.Ok(value) => value,
            Result.Err(error) => "",
        };
        index += 1_usize;
    }
    return parse_args_from(args);
}

fn kind_label(error: Error): string {
    return match error.kind() {
        ErrorKind.NotFound => "not-found",
        ErrorKind.PermissionDenied => "permission",
        ErrorKind.IsADirectory => "is-dir",
        ErrorKind.InvalidArgument => "invalid",
        ErrorKind.NotOwned => "not-owned",
        ErrorKind.Closed => "closed",
        ErrorKind.Other => "other",
    };
}

fn report_io(prefix: string, error: Error) {
    val opening = " (";
    val closing = ")\n";
    val label = kind_label(error);
    eprint(prefix.bytes());
    eprint(opening.bytes());
    eprint(label.bytes());
    eprint(closing.bytes());
}

fn report_usage(error: UsageError) {
    val message = match error {
        UsageError.UnknownOption => "dtext: unknown option\n",
        UsageError.MissingFilterValue => "dtext: --filter requires a value\n",
        UsageError.TooManyArguments => "dtext: too many arguments\n",
        UsageError.InvalidUtf8 => "dtext: invalid UTF-8 argument\n",
    };
    eprint(message.bytes());
}

fn report_invalid_utf8() {
    val message = "dtext: invalid UTF-8\n";
    eprint(message.bytes());
}

fn print_usage() {
    println("usage: dtext [--help] [--filter <text>] [<path>]");
}

/// 把输入读完并追加到 `out`；失败写 stderr 并返回 1。
fn read_all(stream: Stream, out: *Builder): i32 {
    val buffer = mem.alloc<u8>(4096_usize);
    defer mem.free<u8>(buffer);
    var running = true;
    while running {
        val result = stream.read(buffer);
        if result.is_err() {
            val error = match result {
                Result.Ok(value) => Error::new(ErrorKind.Other, 0_i32),
                Result.Err(item) => item,
            };
            report_io("dtext: cannot read input", error);
            return 1;
        }
        val count = match result {
            Result.Ok(value) => value,
            Result.Err(item) => 0_usize,
        };
        if count == 0_usize {
            running = false;
        } else {
            out.append_bytes(buffer.slice(0_usize, count));
        }
    }
    return 0;
}

fn append_usize(out: *Builder, value: usize) {
    if value == 0_usize {
        out.append("0");
        return;
    }
    val digits = mem.alloc<u8>(32_usize);
    defer mem.free<u8>(digits);
    var count = 0_usize;
    var current = value;
    while current > 0_usize {
        val digit = (current % 10_usize) as u8;
        digits[count] = 48_u8 + digit;
        count += 1_usize;
        current = current / 10_usize;
    }
    while count > 0_usize {
        count -= 1_usize;
        out.append_bytes(digits.slice(count, count + 1_usize));
    }
}

fn append_line(out: *Builder, label: string, value: usize) {
    out.append(label);
    append_usize(out, value);
    out.append("\n");
}

/// 固定三行统计输出；stdout 写失败返回 1。
fn emit_stats(stats: textstats.Stats): i32 {
    var out = Builder::init();
    defer out.deinit();
    append_line(&out, "lines=", stats.lines());
    append_line(&out, "matched=", stats.matched());
    append_line(&out, "bytes=", stats.bytes());
    val stream = stdout();
    val written = stream.write_all(out.view());
    if written.is_err() {
        val error = match written {
            Result.Ok(value) => Error::new(ErrorKind.Other, 0_i32),
            Result.Err(item) => item,
        };
        report_io("dtext: cannot write output", error);
        return 1;
    }
    return 0;
}

/// 应用入口：返回进程退出码（0 成功、1 I/O/编码错误、2 用法错误）。
pub fn run(): i32 {
    val parsed = parse_process_args();
    if parsed.is_err() {
        val error = match parsed {
            Result.Err(item) => item,
            Result.Ok(value) => UsageError.InvalidUtf8,
        };
        report_usage(error);
        return 2;
    }
    val options = match parsed {
        Result.Ok(value) => value,
        Result.Err(error) => Options(false, "", "", false),
    };
    if options.help {
        print_usage();
        return 0;
    }
    var stream = stdin();
    defer stream.close_abort();
    if options.has_path {
        if options.path != "-" {
            val opened = open(options.path, OpenMode.Read);
            if opened.is_err() {
                val error = match opened {
                    Result.Ok(value) => Error::new(ErrorKind.Other, 0_i32),
                    Result.Err(item) => item,
                };
                report_io("dtext: cannot open input", error);
                return 1;
            }
            stream = match opened {
                Result.Ok(value) => value,
                Result.Err(item) => stdin(),
            };
        }
    }
    var input = Builder::init();
    defer input.deinit();
    val status = read_all(stream, &input);
    if status != 0 {
        return status;
    }
    val analyzed = textstats.analyze(input.view(), options.filter);
    if analyzed.is_err() {
        report_invalid_utf8();
        return 1;
    }
    val stats = match analyzed {
        Result.Ok(value) => value,
        Result.Err(item) => textstats.Stats::empty(),
    };
    return emit_stats(stats);
}
