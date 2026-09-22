//! H19-06 ERR-01..04：用既有 `Result`/`match`/helper/`defer` 组合出完整失败路径。
//!
//! 本批不新增语法（D3 已冻结）：四个用例验证错误路径上的资源清理、错误值/视图寿命、
//! M14 defer/返回快照契约与 I/O 错误不 trap。每个用例在可用后端 × Dolphin
//! Debug/Release 上构建并以 argv 运行，断言固定 stdout/stderr/exit；Debug 下 stderr
//! 为空同时覆盖无分配泄漏与无未关闭句柄报告。

mod support;

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const ERR_01_PROGRAM: &str = r#"
use std.fs.open;
use std.fs.OpenMode;
use std.io.stdin;
use std.mem;
use std.process.arg;

fn note(tag: i32) {
    println("clean {}", tag);
}

fn read_first(path: string): i32 {
    val opened = open(path, OpenMode.Read);
    if opened.is_err() {
        return 2;
    }
    var stream = match opened {
        Result.Ok(value) => value,
        Result.Err(error) => stdin(),
    };
    defer stream.close_abort();
    val buffer = mem.alloc<u8>(8_usize);
    defer mem.free<u8>(buffer);
    defer note(1);
    val result = stream.read(buffer);
    if result.is_err() {
        return 3;
    }
    val count = match result {
        Result.Ok(value) => value,
        Result.Err(error) => 0_usize,
    };
    if count == 0_usize {
        return 4;
    }
    return 0;
}

fn main(): i32 {
    val p = arg(1_usize);
    if p.is_err() {
        return 9;
    }
    val path = match p {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    return read_first(path);
}
"#;

const ERR_02_PROGRAM: &str = r#"
use std.error.Error;
use std.error.ErrorKind;
use std.fs.open;
use std.fs.OpenMode;
use std.io.stdin;
use std.process.arg;
use std.text;
use std.text.Builder;

struct Report {
    error: Error,
    label: string,
}

fn classify(error: Error): string {
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

fn make_report(error: Error): Report {
    return Report(error, classify(error));
}

fn main(): i32 {
    val p = arg(1_usize);
    if p.is_err() {
        return 9;
    }
    val path = match p {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    var failure = Error::new(ErrorKind.Other, 0_i32);
    val opened = open(path, OpenMode.Read);
    if opened.is_err() {
        failure = match opened {
            Result.Err(error) => error,
            Result.Ok(value) => failure,
        };
    } else {
        var stream = match opened {
            Result.Ok(value) => value,
            Result.Err(error) => stdin(),
        };
        stream.close_abort();
    }
    val report = make_report(failure);
    // 方法调用需要局部接收者（字段访问后直接 `.method()` 目前不解析）。
    val reported = report.error;
    println("kind={} has-code={}", report.label, reported.code() != 0_i32);

    val second = open(path, OpenMode.Read);
    val again = match second {
        Result.Ok(value) => Error::new(ErrorKind.Other, 0_i32),
        Result.Err(error) => error,
    };
    println("again={}", classify(again));

    var builder = Builder::init();
    defer builder.deinit();
    builder.append("view");
    val prefix = match text.from_utf8(builder.view()) {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    println("prefix={}", prefix);
    builder.append("-after");
    val whole = match text.from_utf8(builder.view()) {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    println("whole={}", whole);
    return 0;
}
"#;

const ERR_03_PROGRAM: &str = r#"
use std.error.Error;
use std.error.ErrorKind;
use std.fs.open;
use std.fs.OpenMode;
use std.io.stdin;
use std.mem;
use std.process.arg;

fn note(tag: i32) {
    println("clean {}", tag);
}

fn acquire(path: string): Result<i32, Error> {
    val opened = open(path, OpenMode.Read);
    if opened.is_err() {
        // `return match` 的 Err 构造臂缺少期望类型，先绑定错误再直接 return。
        val error = match opened {
            Result.Err(item) => item,
            Result.Ok(value) => Error::new(ErrorKind.Other, 0_i32),
        };
        return Result.Err(error);
    }
    var stream = match opened {
        Result.Ok(value) => value,
        Result.Err(error) => stdin(),
    };
    defer stream.close_abort();
    val buffer = mem.alloc<u8>(1_usize);
    defer mem.free<u8>(buffer);
    defer note(1);
    buffer[0] = 42_u8;
    return Result.Ok(buffer[0] as i32);
}

fn main(): i32 {
    var tag = 1_i32;
    defer note(tag);
    defer note(3);
    tag = 2_i32;
    val p = arg(1_usize);
    if p.is_err() {
        return 9;
    }
    val path = match p {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    val result = acquire(path);
    if result.is_err() {
        return 1;
    }
    val value = match result {
        Result.Ok(item) => item,
        Result.Err(error) => 0,
    };
    println("value={}", value);
    return 0;
}
"#;

const ERR_04_PROGRAM: &str = r#"
use std.error.Error;
use std.error.ErrorKind;
use std.fs.open;
use std.fs.OpenMode;
use std.io.stdin;
use std.mem;
use std.process.arg;

fn classify(error: Error): string {
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

fn probe_missing(path: string) {
    val opened = open(path, OpenMode.Read);
    val label = match opened {
        Result.Ok(value) => "unexpected",
        Result.Err(error) => classify(error),
    };
    println("missing={}", label);
}

fn probe_read_on_write(path: string) {
    val opened = open(path, OpenMode.Write);
    if opened.is_err() {
        println("read-on-write=open-err");
        return;
    }
    var stream = match opened {
        Result.Ok(value) => value,
        Result.Err(error) => stdin(),
    };
    defer stream.close_abort();
    val buffer = mem.alloc<u8>(4_usize);
    defer mem.free<u8>(buffer);
    val result = stream.read(buffer);
    println("read-on-write-err={}", result.is_err());
}

fn probe_nul() {
    val opened = open("bad\u{0}path", OpenMode.Read);
    val label = match opened {
        Result.Ok(value) => "unexpected",
        Result.Err(error) => classify(error),
    };
    println("nul={}", label);
}

fn main(): i32 {
    val missing = arg(1_usize);
    val writable = arg(2_usize);
    if missing.is_err() || writable.is_err() {
        return 9;
    }
    val missing_path = match missing {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    val writable_path = match writable {
        Result.Ok(value) => value,
        Result.Err(error) => "",
    };
    probe_missing(missing_path);
    probe_read_on_write(writable_path);
    probe_nul();
    return 0;
}
"#;

struct Probe {
    root: PathBuf,
    home: PathBuf,
}

fn unique_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "dolphin-m19-errors-{tag}-{}-{unique}",
        std::process::id()
    ))
}

fn new_probe(tag: &str, source: &str) -> Probe {
    let root = unique_dir(tag);
    let home = root.join("home");
    fs::create_dir_all(root.join("src")).expect("project src");
    fs::create_dir_all(&home).expect("isolated home");
    fs::write(
        root.join("dolphin.toml"),
        "[package]\ngroup = \"org.example\"\nname = \"m19err\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"m19err\"\npath = \"src/main.do\"\n",
    )
    .expect("manifest");
    fs::write(root.join("src/main.do"), source).expect("source");
    Probe { root, home }
}

impl Probe {
    fn executable(&self) -> PathBuf {
        let path = self.root.join("target").join("m19err");
        if cfg!(windows) {
            path.with_extension("exe")
        } else {
            path
        }
    }

    fn fixture(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).expect("fixture dir");
        path
    }

    fn build(&self, backend: &str, release: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dc"));
        command
            .env("DOLPHIN_HOME", &self.home)
            .arg("build")
            .arg(&self.root)
            .args(["--backend", backend]);
        if release {
            command.arg("--release");
        }
        command.output().expect("dc build should run")
    }

    fn run(&self, args: &[OsString]) -> Output {
        Command::new(self.executable())
            .args(args)
            .output()
            .expect("program should run")
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn run_cases(probe: &Probe, args: &[OsString]) -> Vec<(String, bool, Output)> {
    let mut outputs = Vec::new();
    for backend in support::backends() {
        for release in [false, true] {
            let build = probe.build(backend.name(), release);
            assert!(
                build.status.success(),
                "build failed backend={} release={release}: {}",
                backend.name(),
                stderr_text(&build)
            );
            let output = probe.run(args);
            outputs.push((backend.name().to_string(), release, output));
        }
    }
    outputs
}

/// ERR-01：成功与早返回路径都执行 defer 清理（Debug 无泄漏/无未关闭句柄）。
#[test]
fn err_01_early_return_cleans_resources() {
    let probe = new_probe("err01", ERR_01_PROGRAM);
    let empty = probe.fixture("data/empty.txt");
    fs::write(&empty, b"").unwrap();
    let small = probe.fixture("data/small.txt");
    fs::write(&small, b"hello\n").unwrap();
    let missing = probe.root.join("data/missing.txt");

    for (path, expected_exit, expected_stdout) in [
        (empty, 4, "clean 1\n"),
        (small, 0, "clean 1\n"),
        (missing, 2, ""),
    ] {
        for (backend, release, output) in run_cases(&probe, &[path.into_os_string()]) {
            assert_eq!(
                output.status.code(),
                Some(expected_exit),
                "backend={backend} release={release} stderr={}",
                stderr_text(&output)
            );
            assert_eq!(
                stdout_text(&output),
                expected_stdout,
                "backend={backend} release={release}"
            );
            assert!(
                output.stderr.is_empty(),
                "backend={backend} release={release} stderr={}",
                stderr_text(&output)
            );
        }
    }
}

/// ERR-02：错误值跨函数传递后仍可读取；合法视图在源缓冲存活期内使用，Builder 释放无泄漏。
#[test]
fn err_02_error_views_not_dangling() {
    let probe = new_probe("err02", ERR_02_PROGRAM);
    let missing = probe.root.join("data/missing.txt");
    let expected = concat!(
        "kind=not-found has-code=true\n",
        "again=not-found\n",
        "prefix=view\n",
        "whole=view-after\n",
    );
    for (backend, release, output) in run_cases(&probe, &[missing.into_os_string()]) {
        assert_eq!(
            output.status.code(),
            Some(0),
            "backend={backend} release={release} stderr={}",
            stderr_text(&output)
        );
        assert_eq!(
            stdout_text(&output),
            expected,
            "backend={backend} release={release}"
        );
        assert!(
            output.stderr.is_empty(),
            "backend={backend} release={release} stderr={}",
            stderr_text(&output)
        );
    }
}

/// ERR-03：`return` 先取快照再清理，内层清理先于外层，defer 实参读退出时最新值。
#[test]
fn err_03_return_snapshot_and_defer_order() {
    let probe = new_probe("err03", ERR_03_PROGRAM);
    let file = probe.fixture("data/input.txt");
    fs::write(&file, b"x").unwrap();
    let expected = "clean 1\nvalue=42\nclean 3\nclean 2\n";
    for (backend, release, output) in run_cases(&probe, &[file.into_os_string()]) {
        assert_eq!(
            output.status.code(),
            Some(0),
            "backend={backend} release={release} stderr={}",
            stderr_text(&output)
        );
        assert_eq!(
            stdout_text(&output),
            expected,
            "backend={backend} release={release}"
        );
        assert!(
            output.stderr.is_empty(),
            "backend={backend} release={release} stderr={}",
            stderr_text(&output)
        );
    }
}

/// ERR-04：缺失文件、模式不匹配的读、NUL 路径都返回 `Result`，不触发 101/104。
#[test]
fn err_04_io_error_not_trap() {
    let probe = new_probe("err04", ERR_04_PROGRAM);
    let missing = probe.root.join("data/missing.txt");
    let writable = probe.fixture("data/target.txt");
    fs::write(&writable, b"0123456789").unwrap();
    let expected = "missing=not-found\nread-on-write-err=true\nnul=invalid\n";
    for (backend, release, output) in run_cases(
        &probe,
        &[missing.into_os_string(), writable.into_os_string()],
    ) {
        assert_eq!(
            output.status.code(),
            Some(0),
            "backend={backend} release={release} stderr={}",
            stderr_text(&output)
        );
        assert_eq!(
            stdout_text(&output),
            expected,
            "backend={backend} release={release}"
        );
        assert!(
            output.stderr.is_empty(),
            "backend={backend} release={release} stderr={}",
            stderr_text(&output)
        );
    }
}
