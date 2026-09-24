//! H18 批次共享的最小测试驱动：显式选择后端与 Dolphin profile，构建项目，
//! 限时运行可执行文件并捕获 stdout/stderr/exit。后续回归测试按需 `mod support;` 复用。

#![allow(dead_code)]

pub mod lsp;

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dolphin_compiler::{
    BackendChoice, BuildOptions, BuildProfile, BuildSettings, build_with_profile,
};

/// 默认运行超时；死循环 fixture 使用更短的显式超时。
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct ProgramResult {
    pub backend: BackendChoice,
    pub profile: BuildProfile,
    pub stdout: String,
    pub stderr: String,
    pub exit: Option<i32>,
    pub timed_out: bool,
    pub build_error: Option<String>,
}

impl ProgramResult {
    /// 断言失败信息里附上后端与 Dolphin profile（Rust profile 无法在运行时查询）。
    pub fn context(&self) -> String {
        format!(
            "backend={} profile={:?} timed_out={} exit={:?} build_error={:?}",
            self.backend.name(),
            self.profile,
            self.timed_out,
            self.exit,
            self.build_error
        )
    }

    pub fn succeeded(&self) -> bool {
        self.build_error.is_none() && !self.timed_out && self.exit == Some(0)
    }
}

/// 在临时目录写入项目文件，按显式后端/profile 构建并限时运行。
pub fn run_project(
    backend: BackendChoice,
    profile: BuildProfile,
    files: &[(&str, &str)],
    timeout: Duration,
) -> ProgramResult {
    let dir = unique_project_dir();
    for (relative, source) in files {
        let path = dir.join(relative);
        fs::create_dir_all(
            path.parent()
                .expect("project file must have a parent directory"),
        )
        .expect("source directory should be created");
        fs::write(&path, source).expect("source file should be written");
    }

    let built = build_with_profile(
        BuildOptions {
            input: dir.clone(),
            output: None,
        },
        profile,
        BuildSettings::with_backend(backend),
    );

    let artifact = match built {
        Ok(artifact) => artifact,
        Err(error) => {
            fs::remove_dir_all(&dir).ok();
            return ProgramResult {
                backend,
                profile,
                stdout: String::new(),
                stderr: String::new(),
                exit: None,
                timed_out: false,
                build_error: Some(error.to_string()),
            };
        }
    };

    let (stdout, stderr, exit, timed_out) = run_with_timeout(&artifact.executable, timeout);
    fs::remove_dir_all(&dir).ok();
    ProgramResult {
        backend,
        profile,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit,
        timed_out,
        build_error: None,
    }
}

/// 启动进程，持续排空 stdout/stderr；超时则 kill 并回收，避免管道写满死锁。
fn run_with_timeout(executable: &Path, timeout: Duration) -> (Vec<u8>, Vec<u8>, Option<i32>, bool) {
    let mut child = Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("could not run `{}`: {error}", executable.display()));

    let stdout = child.stdout.take().expect("stdout must be piped");
    let stderr = child.stderr.take().expect("stderr must be piped");
    let stdout_reader = std::thread::spawn(move || read_all(stdout));
    let stderr_reader = std::thread::spawn(move || read_all(stderr));

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("waiting for child should succeed") {
            break Some(status);
        }
        if Instant::now() >= deadline {
            timed_out = true;
            child.kill().ok();
            break child.wait().ok();
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    let stdout = stdout_reader.join().expect("stdout reader thread");
    let stderr = stderr_reader.join().expect("stderr reader thread");
    (
        stdout,
        stderr,
        status.and_then(|status| status.code()),
        timed_out,
    )
}

fn read_all(mut stream: impl Read) -> Vec<u8> {
    let mut buffer = Vec::new();
    stream.read_to_end(&mut buffer).ok();
    buffer
}

fn unique_project_dir() -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "dolphin-h18-test-{}-{unique}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

/// 当前测试构建可用的后端：默认 Cranelift；启用 `llvm` feature 时含 LLVM。
pub fn backends() -> Vec<BackendChoice> {
    #[cfg(feature = "llvm")]
    {
        vec![BackendChoice::Cranelift, BackendChoice::Llvm]
    }
    #[cfg(not(feature = "llvm"))]
    {
        vec![BackendChoice::Cranelift]
    }
}

/// 在可用后端 × Dolphin Debug/Release 上运行，断言固定 stdout/exit 且 stderr 为空。
pub fn assert_runs(files: &[(&str, &str)], expected_stdout: &str, expected_exit: i32) {
    for backend in backends() {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let result = run_project(backend, profile, files, DEFAULT_TIMEOUT);
            assert!(
                result.build_error.is_none(),
                "build failed: {}",
                result.context()
            );
            assert_eq!(result.exit, Some(expected_exit), "{}", result.context());
            assert_eq!(result.stdout, expected_stdout, "{}", result.context());
            assert!(result.stderr.is_empty(), "{}", result.context());
        }
    }
}

/// 在可用后端 × Dolphin Debug/Release 上断言构建被拒绝，诊断包含 `needle`。
pub fn assert_rejected(source: &str, needle: &str) {
    assert_rejected_messages(source, &[needle]);
}

/// 与 `assert_rejected` 相同，但要求诊断同时包含全部 `needles`。
pub fn assert_rejected_messages(source: &str, needles: &[&str]) {
    for backend in backends() {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let result = run_project(
                backend,
                profile,
                &[("src/main.do", source)],
                DEFAULT_TIMEOUT,
            );
            let error = result
                .build_error
                .clone()
                .unwrap_or_else(|| panic!("expected {needles:?} rejection: {}", result.context()));
            for needle in needles {
                assert!(
                    error.contains(needle),
                    "expected `{needle}` in diagnostic: {error}"
                );
            }
        }
    }
}

/// 断言程序在可用后端 × 两种 profile 下都以 101 trap 退出，且 stdout 不出现标记。
pub fn assert_traps_without_stdout(source: &str, forbidden_stdout: &str) {
    for backend in backends() {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let result = run_project(
                backend,
                profile,
                &[("src/main.do", source)],
                DEFAULT_TIMEOUT,
            );
            assert!(
                result.build_error.is_none(),
                "build failed: {}",
                result.context()
            );
            assert_eq!(result.exit, Some(101), "{}", result.context());
            assert!(
                !result.stdout.contains(forbidden_stdout),
                "stdout must not contain `{forbidden_stdout}`: {}",
                result.context()
            );
            assert!(
                result.stderr.contains("Dolphin runtime error"),
                "{}",
                result.context()
            );
        }
    }
}

/// 断言程序在可用后端 × 两种 profile 下都以 101 trap 退出。
pub fn assert_traps(source: &str) {
    for backend in backends() {
        for profile in [BuildProfile::Debug, BuildProfile::Release] {
            let result = run_project(
                backend,
                profile,
                &[("src/main.do", source)],
                DEFAULT_TIMEOUT,
            );
            assert!(
                result.build_error.is_none(),
                "build failed: {}",
                result.context()
            );
            assert_eq!(result.exit, Some(101), "{}", result.context());
            assert!(
                result.stderr.contains("Dolphin runtime error"),
                "{}",
                result.context()
            );
        }
    }
}
