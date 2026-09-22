//! H19-01 ARGS-01..04：`std.process` 参数/环境 API 与 `dc run -- <应用参数>` 转发。
//!
//! 每个用例在可用后端 × Dolphin Debug/Release 上构建，直接运行与经 `dc run` 运行
//! 都断言固定 stdout/exit；涉及泄漏检查的 Debug 用例要求 stderr 为空。

mod support;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

/// 打印进程参数与环境分类。所有输出都是固定的 `label=value` 行。
const ARGS_PROGRAM: &str = r#"
use std.process.arg;
use std.process.arg_count;
use std.process.env;
use std.process.ArgError;
use std.process.EnvLookup;

fn show_arg(index: usize): string {
    val result = arg(index);
    return match result {
        Result.Ok(text) => text,
        Result.Err(error) => match error {
            ArgError.OutOfRange => "<out-of-range>",
            ArgError.NotUtf8 => "<not-utf8>",
        },
    };
}

fn show_env(name: string): string {
    val lookup = env(name);
    return match lookup {
        EnvLookup.Found(value) => value,
        EnvLookup.Missing => "<missing>",
        EnvLookup.NotUtf8 => "<not-utf8>",
    };
}

fn main() {
    val count = arg_count();
    println("count={}", count);
    var index = 0_usize;
    while index < count {
        println("arg{}={}", index, show_arg(index));
        index += 1_usize;
    }
    println("oor={}", show_arg(99_usize));
    println("set={}", show_env("DOLPHIN_H19_SET"));
    println("missing={}", show_env("DOLPHIN_H19_MISSING"));
    println("raw={}", show_env("DOLPHIN_H19_RAW"));
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
        "dolphin-m19-args-{tag}-{}-{unique}",
        std::process::id()
    ))
}

fn probe(tag: &str, source: &str) -> Probe {
    let root = unique_dir(tag);
    let home = root.join("home");
    fs::create_dir_all(root.join("src")).expect("project src");
    fs::create_dir_all(&home).expect("isolated home");
    fs::write(
        root.join("dolphin.toml"),
        "[package]\ngroup = \"org.example\"\nname = \"m19args\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"m19args\"\npath = \"src/main.do\"\n",
    )
    .expect("manifest");
    fs::write(root.join("src/main.do"), source).expect("source");
    Probe { root, home }
}

impl Probe {
    fn executable(&self) -> PathBuf {
        let path = self.root.join("target").join("m19args");
        if cfg!(windows) {
            path.with_extension("exe")
        } else {
            path
        }
    }

    fn dc(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_dc"));
        command.env("DOLPHIN_HOME", &self.home);
        command
    }

    fn build(&self, backend: &str, release: bool) -> Output {
        let mut command = self.dc();
        command
            .arg("build")
            .arg(&self.root)
            .args(["--backend", backend]);
        if release {
            command.arg("--release");
        }
        command.output().expect("dc build should run")
    }

    fn dc_run(
        &self,
        backend: &str,
        release: bool,
        app_args: &[OsString],
        environment: &[(&str, OsString)],
    ) -> Output {
        let mut command = self.dc();
        command
            .arg("run")
            .arg(&self.root)
            .args(["--backend", backend]);
        if release {
            command.arg("--release");
        }
        for (key, value) in environment {
            command.env(key, value);
        }
        if !app_args.is_empty() {
            command.arg("--");
            command.args(app_args);
        }
        command.output().expect("dc run should run")
    }

    fn run_direct(&self, app_args: &[OsString], environment: &[(&str, OsString)]) -> Output {
        let mut command = Command::new(self.executable());
        command.args(app_args);
        for (key, value) in environment {
            command.env(key, value);
        }
        command.output().expect("program should run")
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// 完整期望输出：`count` + 每条 arg + 分类行。
fn expected(exe: &Path, app_args: &[OsString], set_value: &str, raw_label: &str) -> String {
    let mut expected = format!("count={}\n", app_args.len() + 1);
    expected.push_str(&format!("arg0={}\n", exe.display()));
    for (index, argument) in app_args.iter().enumerate() {
        expected.push_str(&format!(
            "arg{}={}\n",
            index + 1,
            argument.to_string_lossy()
        ));
    }
    expected.push_str("oor=<out-of-range>\n");
    expected.push_str(&format!("set={set_value}\n"));
    expected.push_str("missing=<missing>\n");
    expected.push_str(&format!("raw={raw_label}\n"));
    expected
}

fn assert_matches_expected(
    label: &str,
    output: &Output,
    expected_stdout: &str,
    backend: &str,
    release: bool,
) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{label} backend={backend} release={release} stderr={}",
        stderr_text(output)
    );
    assert_eq!(
        text(output),
        expected_stdout,
        "{label} backend={backend} release={release}"
    );
    assert!(
        output.stderr.is_empty(),
        "{label} backend={backend} release={release} stderr must be empty: {}",
        stderr_text(output)
    );
}

/// ARGS-01：直接运行与 `dc run --` 参数一致，且输出是固定期望。
#[test]
fn args_01_direct_and_dc_run_match() {
    let probe = probe("args01", ARGS_PROGRAM);
    let app_args: Vec<OsString> = vec!["alpha".into(), "beta".into()];
    let set_value = "h19-set-value";
    for backend in support::backends() {
        for release in [false, true] {
            let build = probe.build(backend.name(), release);
            assert!(
                build.status.success(),
                "build backend={} release={release}: {}",
                backend.name(),
                stderr_text(&build)
            );
            let expected_stdout = expected(&probe.executable(), &app_args, set_value, "<missing>");

            let environment = [("DOLPHIN_H19_SET", OsString::from(set_value))];
            let direct = probe.run_direct(&app_args, &environment);
            assert_matches_expected("direct", &direct, &expected_stdout, backend.name(), release);

            let via_dc = probe.dc_run(backend.name(), release, &app_args, &environment);
            assert_matches_expected("dc run", &via_dc, &expected_stdout, backend.name(), release);
            assert_eq!(
                text(&direct),
                text(&via_dc),
                "direct and dc run must agree (backend={} release={release})",
                backend.name()
            );
        }
    }
}

/// ARGS-02：空参数、空格、Unicode、以 `-` 开头（含 `--help`）原样转发。
#[test]
fn args_02_spaces_unicode_and_dash() {
    let probe = probe("args02", ARGS_PROGRAM);
    let app_args: Vec<OsString> = vec![
        "".into(),
        "a b".into(),
        "你好，Dolphin".into(),
        "--help".into(),
        "-x".into(),
        "tab\targ".into(),
        "quote\"q".into(),
        "rocks;rm -rf".into(),
    ];
    let set_value = "值 with space";
    for backend in support::backends() {
        for release in [false, true] {
            assert!(
                probe.build(backend.name(), release).status.success(),
                "build failed (backend={} release={release})",
                backend.name()
            );
            let expected_stdout = expected(&probe.executable(), &app_args, set_value, "<missing>");
            for (label, output) in [
                (
                    "direct",
                    probe.run_direct(&app_args, &[("DOLPHIN_H19_SET", set_value.into())]),
                ),
                (
                    "dc run",
                    probe.dc_run(
                        backend.name(),
                        release,
                        &app_args,
                        &[("DOLPHIN_H19_SET", set_value.into())],
                    ),
                ),
            ] {
                assert_matches_expected(label, &output, &expected_stdout, backend.name(), release);
            }
        }
    }
}

/// ARGS-03：越界、缺项环境、非 UTF-8 参数与环境分别返回不同结果。
#[test]
fn args_03_missing_env_and_not_utf8() {
    let probe = probe("args03", ARGS_PROGRAM);
    for backend in support::backends() {
        for release in [false, true] {
            assert!(
                probe.build(backend.name(), release).status.success(),
                "build failed (backend={} release={release})",
                backend.name()
            );

            // 无参数：越界为 OutOfRange，缺项为 Missing，未设置的 RAW 为 Missing。
            let expected_stdout = expected(&probe.executable(), &[], "<missing>", "<missing>");
            let plain = probe.run_direct(&[], &[]);
            assert_matches_expected("no args", &plain, &expected_stdout, backend.name(), release);

            // Unix：非法 UTF-8 argv 与环境值都必须显式分类，不替换字符。
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStringExt;
                let invalid = OsString::from_vec(vec![0xff, 0xfe]);
                let environment = [("DOLPHIN_H19_RAW", invalid.clone())];
                let output = probe.run_direct(std::slice::from_ref(&invalid), &environment);
                assert_eq!(
                    output.status.code(),
                    Some(0),
                    "backend={} release={release} stderr={}",
                    backend.name(),
                    stderr_text(&output)
                );
                let stdout = text(&output);
                assert!(
                    stdout.contains("arg1=<not-utf8>"),
                    "invalid argv must be classified (backend={} release={release}): {stdout}",
                    backend.name()
                );
                assert!(
                    stdout.contains("raw=<not-utf8>"),
                    "invalid env must be classified (backend={} release={release}): {stdout}",
                    backend.name()
                );

                // `dc run --` 也原样转发非法字节（OsString 不做 UTF-8 转换）。
                let via_dc =
                    probe.dc_run(backend.name(), release, std::slice::from_ref(&invalid), &[]);
                assert_eq!(
                    via_dc.status.code(),
                    Some(0),
                    "dc run invalid argv backend={} release={release}: {}",
                    backend.name(),
                    stderr_text(&via_dc)
                );
                assert!(
                    text(&via_dc).contains("arg1=<not-utf8>"),
                    "dc run must forward invalid bytes unchanged: {}",
                    text(&via_dc)
                );
            }
        }
    }
}

/// ARGS-04：`main` 签名与退出码兼容；`dc build` 不接受 `--` 之后的应用参数。
#[test]
fn args_04_main_exit_unchanged() {
    // 不使用 std.process 的程序照常构建运行。
    let plain = probe("args04-plain", "fn main() { return 7; }");
    assert!(
        plain
            .build(support::backends()[0].name(), false)
            .status
            .success()
    );
    let direct = plain.run_direct(&[], &[]);
    assert_eq!(direct.status.code(), Some(7), "{}", stderr_text(&direct));

    // `main(): i32` 的退出码经 `dc run`（带与不带 `--`）都保留。
    let exit_probe = probe("args04-exit", "fn main(): i32 { return 42; }");
    for backend in support::backends() {
        for release in [false, true] {
            assert!(
                exit_probe.build(backend.name(), release).status.success(),
                "build failed (backend={} release={release})",
                backend.name()
            );
            let with_args =
                exit_probe.dc_run(backend.name(), release, &[OsString::from("--help")], &[]);
            assert_eq!(
                with_args.status.code(),
                Some(42),
                "dc run -- must preserve exit code (backend={} release={release})",
                backend.name()
            );
            let without = exit_probe.dc_run(backend.name(), release, &[], &[]);
            assert_eq!(
                without.status.code(),
                Some(42),
                "dc run without -- must preserve exit code (backend={} release={release})",
                backend.name()
            );
        }
    }

    // 隐式 Unit 返回的 main 退出 0。
    let empty = probe("args04-empty", "fn main() { }");
    assert!(
        empty
            .build(support::backends()[0].name(), false)
            .status
            .success()
    );
    let output = empty.run_direct(&[], &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr_text(&output));

    // `dc build` 不接受 `--` 之后的应用参数（用法错误 2）。
    let mut command = exit_probe.dc();
    let usage = command
        .arg("build")
        .arg(&exit_probe.root)
        .args(["--", "extra"])
        .output()
        .expect("dc build should run");
    assert_eq!(
        usage.status.code(),
        Some(2),
        "dc build must reject trailing application arguments: {}",
        stderr_text(&usage)
    );
}
