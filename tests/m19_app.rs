//! H19-07 目标工具 `dtext` 集成验收。
//!
//! 把 `examples/m19` 复制到临时目录，用 `dc` 在可用后端 × Dolphin Debug/Release 上
//! 构建应用（`--bin dtext`）并实际调用命令行，断言三路结果（stdout/stderr/exit）；
//! 同时运行两个示例包各自的 `dc test`。D1 交互（lib+bin 且声明 path 依赖时 plain
//! `dc build` 仍拒绝打包）也在这里固定。

mod support;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dolphin-m19-app-{tag}-{}-{unique}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("destination dir");
    for entry in fs::read_dir(source).expect("read source dir") {
        let entry = entry.expect("source entry");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

struct App {
    root: PathBuf,
    home: PathBuf,
}

impl App {
    fn new(tag: &str) -> App {
        let root = temp_dir(tag);
        let home = root.join("home");
        fs::create_dir_all(&home).expect("isolated home");
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/m19");
        copy_tree(&examples, &root.join("m19"));
        App { root, home }
    }

    fn dtext_dir(&self) -> PathBuf {
        self.root.join("m19/dtext")
    }

    fn textstats_dir(&self) -> PathBuf {
        self.root.join("m19/textstats")
    }

    fn executable(&self) -> PathBuf {
        let path = self.dtext_dir().join("target").join("dtext");
        if cfg!(windows) {
            path.with_extension("exe")
        } else {
            path
        }
    }

    fn dc(&self, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dc"));
        command.env("DOLPHIN_HOME", &self.home).args(args);
        command.output().expect("dc should run")
    }

    /// 构建应用；`plain` 为 true 时用默认（会打包库的）`dc build`。
    fn build(&self, backend: &str, release: bool, plain: bool) -> Output {
        let directory = self.dtext_dir();
        let directory = directory.to_str().expect("utf-8 path");
        let mut args = vec!["build", directory];
        if !plain {
            args.extend_from_slice(&["--bin", "dtext"]);
        }
        args.extend_from_slice(&["--backend", backend]);
        if release {
            args.push("--release");
        }
        self.dc(&args)
    }

    fn dc_test(&self, directory: &Path, backend: &str, release: bool) -> Output {
        let directory = directory.to_str().expect("utf-8 path");
        let mut args = vec!["test", directory, "--backend", backend];
        if release {
            args.push("--release");
        }
        self.dc(&args)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(self.executable())
            .args(args)
            .output()
            .expect("dtext should run")
    }

    fn run_with_stdin(&self, args: &[&str], input: &[u8]) -> Output {
        let mut child = Command::new(self.executable())
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("dtext should start");
        let mut stdin = child.stdin.take().expect("stdin should be piped");
        stdin.write_all(input).expect("stdin should accept input");
        drop(stdin);
        child.wait_with_output().expect("dtext should finish")
    }
}

impl Drop for App {
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

fn assert_built(app: &App, backend: &str, release: bool) {
    let build = app.build(backend, release, false);
    assert!(
        build.status.success(),
        "build failed backend={backend} release={release}: {}",
        stderr_text(&build)
    );
    assert!(
        app.executable().is_file(),
        "backend={backend} release={release}: missing dtext executable"
    );
}

fn assert_run(
    output: &Output,
    expected_stdout: &str,
    expected_stderr: &str,
    exit: i32,
    context: &str,
) {
    assert_eq!(
        output.status.code(),
        Some(exit),
        "{context} stdout={} stderr={}",
        stdout_text(output),
        stderr_text(output)
    );
    assert_eq!(stdout_text(output), expected_stdout, "{context}");
    assert_eq!(stderr_text(output), expected_stderr, "{context}");
}

/// 布局/D1 契约 + `--help`：plain `dc build` 仍因 path 依赖拒绝打包库；`--bin` 可用。
#[test]
fn app_01_layout_build_and_help() {
    for backend in support::backends() {
        for release in [false, true] {
            let app = App::new("build");
            let context = format!("backend={} release={release}", backend.name());

            let plain = app.build(backend.name(), release, true);
            assert_eq!(
                plain.status.code(),
                Some(1),
                "{context}: plain dc build must keep rejecting path dependencies"
            );
            assert!(
                stderr_text(&plain).contains("path dependency"),
                "{context} stderr={}",
                stderr_text(&plain)
            );

            assert_built(&app, backend.name(), release);
            let help = app.run(&["--help"]);
            assert_run(
                &help,
                "usage: dtext [--help] [--filter <text>] [<path>]\n",
                "",
                0,
                &format!("{context} help"),
            );
        }
    }
}

/// 空输入、正常 UTF-8、无末尾换行、无匹配、CRLF、Unicode、`-` 与 stdin 读取。
#[test]
fn app_02_stdin_and_line_rules() {
    for backend in support::backends() {
        for release in [false, true] {
            let app = App::new("stdin");
            let context = format!("backend={} release={release}", backend.name());
            assert_built(&app, backend.name(), release);

            for (input, args, expected) in [
                (&b""[..], vec![], "lines=0\nmatched=0\nbytes=0\n"),
                (
                    &b"alpha\nbeta\ngamma\n"[..],
                    vec!["--filter", "et"],
                    "lines=3\nmatched=1\nbytes=17\n",
                ),
                (&b"a\nb"[..], vec![], "lines=2\nmatched=2\nbytes=3\n"),
                (
                    &b"a\nb\n"[..],
                    vec!["--filter", "zzz"],
                    "lines=2\nmatched=0\nbytes=4\n",
                ),
                (
                    &b"a\r\nb\r\n"[..],
                    vec!["--filter", "a"],
                    "lines=2\nmatched=1\nbytes=6\n",
                ),
                (
                    "h\u{e9}llo\n\u{4e16}\u{754c}\n".as_bytes(),
                    vec!["--filter", "\u{4e16}\u{754c}"],
                    "lines=2\nmatched=1\nbytes=14\n",
                ),
                (&b"x\n"[..], vec!["-"], "lines=1\nmatched=1\nbytes=2\n"),
            ] {
                let output = app.run_with_stdin(&args, input);
                assert_run(
                    &output,
                    expected,
                    "",
                    0,
                    &format!("{context} args={args:?}"),
                );
            }
        }
    }
}

/// 文件输入（含空格/Unicode 路径）与重复运行无资源累积（Debug stderr 必须为空）。
#[test]
fn app_03_file_input_and_repeated_runs() {
    for backend in support::backends() {
        for release in [false, true] {
            let app = App::new("file");
            let context = format!("backend={} release={release}", backend.name());
            assert_built(&app, backend.name(), release);

            let directory = app.root.join("data dir");
            fs::create_dir_all(&directory).expect("fixture dir");
            let file = directory.join("in put.txt");
            fs::write(&file, "h\u{e9}llo\n\u{4e16}\u{754c}\n").expect("fixture");
            let file = file.to_str().expect("utf-8 path").to_string();

            let expected = "lines=2\nmatched=1\nbytes=14\n";
            for _ in 0..5 {
                let output = app.run(&[&file, "--filter", "\u{4e16}\u{754c}"]);
                assert_run(
                    &output,
                    expected,
                    "",
                    0,
                    &format!("{context} repeated file run"),
                );
            }
        }
    }
}

/// 用法错误：未知选项、缺少 `--filter` 值、多余位置参数都是 exit 2 + 固定 stderr。
#[test]
fn app_04_usage_errors() {
    for backend in support::backends() {
        for release in [false, true] {
            let app = App::new("usage");
            let context = format!("backend={} release={release}", backend.name());
            assert_built(&app, backend.name(), release);

            for (args, expected_stderr) in [
                (vec!["--bogus"], "dtext: unknown option\n"),
                (vec!["--filter"], "dtext: --filter requires a value\n"),
                (vec!["one", "two"], "dtext: too many arguments\n"),
                (vec!["-", "-"], "dtext: too many arguments\n"),
            ] {
                let output = app.run(&args);
                assert_run(
                    &output,
                    "",
                    expected_stderr,
                    2,
                    &format!("{context} args={args:?}"),
                );
            }
        }
    }
}

/// 缺失文件、目录与非法 UTF-8：exit 1 + 固定诊断，stdout 不输出统计。
#[test]
fn app_05_open_and_encoding_errors() {
    for backend in support::backends() {
        for release in [false, true] {
            let app = App::new("errors");
            let context = format!("backend={} release={release}", backend.name());
            assert_built(&app, backend.name(), release);

            let missing = app.root.join("no-such-file.txt");
            let output = app.run(&[missing.to_str().unwrap()]);
            assert_run(
                &output,
                "",
                "dtext: cannot open input (not-found)\n",
                1,
                &format!("{context} missing"),
            );

            let directory = app.root.join("a-directory");
            fs::create_dir_all(&directory).expect("fixture dir");
            let expected_kind = if cfg!(windows) { "invalid" } else { "is-dir" };
            let output = app.run(&[directory.to_str().unwrap()]);
            assert_run(
                &output,
                "",
                &format!("dtext: cannot open input ({expected_kind})\n"),
                1,
                &format!("{context} directory"),
            );

            let output = app.run_with_stdin(&[], &[0x61, 0xff, 0x62]);
            assert_run(
                &output,
                "",
                "dtext: invalid UTF-8\n",
                1,
                &format!("{context} invalid stdin"),
            );

            let bad = app.root.join("bad.txt");
            fs::write(&bad, [0x61, 0xff, 0x62]).expect("fixture");
            let output = app.run(&[bad.to_str().unwrap()]);
            assert_run(
                &output,
                "",
                "dtext: invalid UTF-8\n",
                1,
                &format!("{context} invalid file"),
            );
        }
    }
}

/// 受控写失败：stdout 指向 `/dev/full` 时返回 1 + 固定诊断，不 trap（Linux）。
#[test]
#[cfg(target_os = "linux")]
fn app_06_write_failure_not_trap() {
    for backend in support::backends() {
        for release in [false, true] {
            let app = App::new("write");
            let context = format!("backend={} release={release}", backend.name());
            assert_built(&app, backend.name(), release);

            let file = app.root.join("input.txt");
            fs::write(&file, b"a\n").expect("fixture");
            let full = fs::OpenOptions::new()
                .write(true)
                .open("/dev/full")
                .expect("/dev/full");
            let output = Command::new(app.executable())
                .arg(file.to_str().unwrap())
                .stdout(Stdio::from(full))
                .stderr(Stdio::piped())
                .output()
                .expect("dtext should run");
            assert_eq!(
                output.status.code(),
                Some(1),
                "{context} stderr={}",
                stderr_text(&output)
            );
            assert_eq!(
                stderr_text(&output),
                "dtext: cannot write output (other)\n",
                "{context}"
            );
        }
    }
}

/// 两个示例包各自的 `dc test` 全部通过（应用测试实际消费 path 依赖的公开泛型/核心 API）。
#[test]
fn app_07_dc_test_self_checks() {
    for backend in support::backends() {
        for release in [false, true] {
            let app = App::new("selftest");
            let context = format!("backend={} release={release}", backend.name());

            let textstats = app.dc_test(&app.textstats_dir(), backend.name(), release);
            assert_eq!(
                textstats.status.code(),
                Some(0),
                "{context} textstats stdout={} stderr={}",
                stdout_text(&textstats),
                stderr_text(&textstats)
            );
            assert_eq!(
                stdout_text(&textstats),
                concat!(
                    "test test_byte_count ... ok\n",
                    "test test_filter_rules ... ok\n",
                    "test test_invalid_utf8 ... ok\n",
                    "test test_line_rules ... ok\n",
                    "test test_unicode_filter ... ok\n",
                    "5 passed; 0 failed; 0 filtered out\n",
                ),
                "{context} textstats"
            );
            assert!(textstats.stderr.is_empty(), "{context} textstats stderr");

            let dtext = app.dc_test(&app.dtext_dir(), backend.name(), release);
            assert_eq!(
                dtext.status.code(),
                Some(0),
                "{context} dtext stdout={} stderr={}",
                stdout_text(&dtext),
                stderr_text(&dtext)
            );
            assert_eq!(
                stdout_text(&dtext),
                concat!(
                    "test test_dependency_analyze ... ok\n",
                    "test test_number_formatting ... ok\n",
                    "test test_parse_filter_and_path ... ok\n",
                    "test test_parse_help ... ok\n",
                    "test test_parse_usage_errors ... ok\n",
                    "5 passed; 0 failed; 0 filtered out\n",
                ),
                "{context} dtext"
            );
            assert!(dtext.stderr.is_empty(), "{context} dtext stderr");
        }
    }
}
