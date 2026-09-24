//! H20-05 调试器验收（DBG-01..04，规格 §9）。
//!
//! 只在启用 `llvm` feature 时编译；Linux 用 gdb（`scripts/debug_smoke.sh`）、
//! macOS 用 lldb（`scripts/debug_smoke_lldb.sh`）。测试在临时目录生成两文件项目
//! （`src/math.do` 提供 `add`，`src/main.do` 调用它），用 `dc build --backend llvm`
//! 构建 Dolphin Debug 后实际加载调试器，断言断点命中文件/行、`bt` 调用链，以及
//! Release / Cranelift 无行表边界。缺少调试器时测试失败并提示
//! `DOLPHIN_SKIP_DEBUGGER=1` 显式跳过（跳过时报告必须列为未验证）。
//!
//! 断点行由 fixture 中的 `// DBG-BREAK` 标记扫描得到，不硬编码行号。

#![cfg(feature = "llvm")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

const MATH_SOURCE: &str = "\
pub fn add(a: i32, b: i32): i32 {
    val sum = a + b; // DBG-BREAK
    return sum;
}
";

const MAIN_SOURCE: &str = "\
fn main(): i32 {
    val result = add(20, 22);
    return result;
}
";

fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after Unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dolphin-m20-debug-{tag}-{}-{unique}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory");
    }
    fs::write(path, text).expect("source file should be written");
}

/// 期望断点行：`MATH_SOURCE` 中 `// DBG-BREAK` 所在行的 1-based 行号。
fn break_line() -> u32 {
    MATH_SOURCE
        .lines()
        .position(|line| line.contains("// DBG-BREAK"))
        .map(|index| index as u32 + 1)
        .expect("fixture must mark `// DBG-BREAK`")
}

fn script_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "debug_smoke_lldb.sh"
    } else {
        "debug_smoke.sh"
    }
}

fn debugger_tool() -> &'static str {
    if cfg!(target_os = "macos") {
        "lldb"
    } else {
        "gdb"
    }
}

fn debugger_present() -> bool {
    Command::new(debugger_tool())
        .arg("--version")
        .output()
        .is_ok()
}

/// §9.2：缺少调试器时必须失败并提示显式跳过；设置 `DOLPHIN_SKIP_DEBUGGER=1`
/// 时返回 `false`（测试直接返回，报告列为未验证）。
fn debugger_ready() -> bool {
    if std::env::var_os("DOLPHIN_SKIP_DEBUGGER").is_some() {
        eprintln!(
            "skipping debugger test: DOLPHIN_SKIP_DEBUGGER is set; DBG-01..04 未验证（不得记为通过）"
        );
        return false;
    }
    assert!(
        debugger_present(),
        "{} not found: install it or set DOLPHIN_SKIP_DEBUGGER=1 to skip (DBG-01..04 then 未验证)",
        debugger_tool()
    );
    true
}

struct Project {
    root: PathBuf,
}

impl Project {
    fn new(tag: &str) -> Project {
        let root = temp_dir(tag);
        write(
            &root.join("dolphin.toml"),
            "[package]\ngroup = \"g\"\nname = \"debugsmoke\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"debugsmoke\"\npath = \"src/main.do\"\n",
        );
        write(&root.join("src/math.do"), MATH_SOURCE);
        write(&root.join("src/main.do"), MAIN_SOURCE);
        Project { root }
    }

    fn build(&self, backend: &str, release: bool) -> Output {
        let mut args = vec![
            "build",
            self.root.to_str().expect("utf-8 project path"),
            "--backend",
            backend,
        ];
        if release {
            args.push("--release");
        }
        Command::new(env!("CARGO_BIN_EXE_dc"))
            .args(&args)
            .output()
            .expect("dc should run")
    }

    fn executable(&self) -> PathBuf {
        let path = self.root.join("target").join("debugsmoke");
        if cfg!(windows) {
            path.with_extension("exe")
        } else {
            path
        }
    }

    fn math_path(&self) -> PathBuf {
        self.root.join("src/math.do")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn assert_built(output: &Output, backend: &str, release: bool) {
    assert!(
        output.status.success(),
        "build failed backend={backend} release={release}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join(script_name())
}

fn run_smoke(exe: &Path, math: &Path, line: u32) -> Output {
    let script = script_path();
    Command::new("bash")
        .arg(&script)
        .arg(exe)
        .arg(math)
        .arg(line.to_string())
        .output()
        .unwrap_or_else(|error| panic!("could not run `{}`: {error}", script.display()))
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_debug_smoke(output: &Output) -> String {
    let text = stdout_text(output);
    assert_eq!(
        output.status.code(),
        Some(0),
        "debug smoke must report a conclusion (stdout={text} stderr={})",
        String::from_utf8_lossy(&output.stderr)
    );
    text
}

/// 边界：参数个数错误以固定退出码 2 报告用法，不尝试运行调试器。
#[test]
fn dbg_script_rejects_bad_arguments() {
    let output = Command::new("bash")
        .arg(script_path())
        .output()
        .expect("bash should run");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("usage:"),
        "stderr must carry usage: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// 边界：缺少调试器时脚本以非 0 退出并打印 `debugger not available`（§9.2），
/// 不把“没有行信息”伪装成通过。用绝对路径启动 bash 并把子进程 PATH 指向空目录。
#[test]
fn dbg_script_reports_missing_debugger() {
    let bash = Command::new("bash")
        .arg("-c")
        .arg("command -v bash")
        .output()
        .expect("bash should run");
    let bash = String::from_utf8_lossy(&bash.stdout).trim().to_string();
    assert!(!bash.is_empty(), "bash must be available");

    let empty_path = temp_dir("empty-path");
    let output = Command::new(&bash)
        .env("PATH", &empty_path)
        .arg(script_path())
        .arg("/nonexistent-program")
        .arg("/nonexistent.do")
        .arg("1")
        .output()
        .expect("script should run");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("debugger not available"),
        "stderr must report missing debugger: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(&empty_path).ok();
}

/// DBG-01：调试器实际加载 LLVM Debug 产物（脚本退出 0，输出含 `Breakpoint`/`stop reason`）。
#[test]
fn dbg_01_debugger_loads() {
    if !debugger_ready() {
        return;
    }
    let project = Project::new("dbg01");
    assert_built(&project.build("llvm", false), "llvm", false);
    let output = run_smoke(&project.executable(), &project.math_path(), break_line());
    let text = assert_debug_smoke(&output);
    assert!(
        text.contains("Breakpoint") || text.contains("stop reason"),
        "调试器未实际加载断点：{text}"
    );
}

/// DBG-02：多文件断点命中正确文件与行（输出含 `<math.do>:<期望行>`）。
#[test]
fn dbg_02_breakpoint_correct_file_line() {
    if !debugger_ready() {
        return;
    }
    let project = Project::new("dbg02");
    assert_built(&project.build("llvm", false), "llvm", false);
    let line = break_line();
    let output = run_smoke(&project.executable(), &project.math_path(), line);
    let text = assert_debug_smoke(&output);
    assert!(
        text.contains(&format!("math.do:{line}")),
        "断点必须命中 math.do:{line}：{text}"
    );
    assert!(
        text.contains("debug smoke: breakpoint hit"),
        "断点必须被命中：{text}"
    );
}

/// 补充（计划 §6“单步、源码行映射”）：断点后单步到下一语句，栈顶帧移动到下一行。
#[test]
fn dbg_single_step_line_mapping() {
    if !debugger_ready() {
        return;
    }
    let project = Project::new("dbgstep");
    assert_built(&project.build("llvm", false), "llvm", false);
    let line = break_line();
    let output = run_smoke(&project.executable(), &project.math_path(), line);
    let text = assert_debug_smoke(&output);
    assert!(
        text.contains(&format!("math.do:{}", line + 1)),
        "单步后应停在 math.do:{}：{text}",
        line + 1
    );
}

/// DBG-03：`bt` 含 `#0`（math 函数）与 `#1`（main）且顺序正确。
#[test]
fn dbg_03_cross_function_call_stack() {
    if !debugger_ready() {
        return;
    }
    let project = Project::new("dbg03");
    assert_built(&project.build("llvm", false), "llvm", false);
    let output = run_smoke(&project.executable(), &project.math_path(), break_line());
    let text = assert_debug_smoke(&output);
    let zero = text
        .find("#0")
        .unwrap_or_else(|| panic!("bt 缺少 #0 帧：{text}"));
    let one = text
        .find("#1")
        .unwrap_or_else(|| panic!("bt 缺少 #1 帧：{text}"));
    assert!(zero < one, "`#0` 必须在 `#1` 之前：{text}");
    let zero_line = text[zero..].lines().next().expect("frame #0 line");
    let one_line = text[one..].lines().next().expect("frame #1 line");
    assert!(zero_line.contains("add"), "`#0` 必须是 math.add：{text}");
    assert!(one_line.contains("main"), "`#1` 必须是 main：{text}");
}

/// DBG-04：Release 与 Cranelift 不产生可用行信息，脚本以文档化方式报告
/// `debug smoke: no line table (breakpoint not hit)`，不算失败（§9.1 边界）。
#[test]
fn dbg_04_release_without_debug_boundary() {
    if !debugger_ready() {
        return;
    }
    let project = Project::new("dbg04");

    assert_built(&project.build("llvm", true), "llvm", true);
    let output = run_smoke(&project.executable(), &project.math_path(), break_line());
    let text = assert_debug_smoke(&output);
    assert!(
        text.contains("debug smoke: no line table (breakpoint not hit)"),
        "LLVM Release 必须报告无行表/未命中：{text}"
    );
    assert!(
        !text.contains("debug smoke: breakpoint hit"),
        "LLVM Release 不得报告断点命中：{text}"
    );

    #[cfg(feature = "cranelift")]
    {
        assert_built(&project.build("cranelift", false), "cranelift", false);
        let output = run_smoke(&project.executable(), &project.math_path(), break_line());
        let text = assert_debug_smoke(&output);
        assert!(
            text.contains("debug smoke: no line table (breakpoint not hit)"),
            "Cranelift Debug 不在本阶段行表范围，必须报告无行表/未命中：{text}"
        );
    }
}
