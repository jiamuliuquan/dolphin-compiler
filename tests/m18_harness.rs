//! H18-00 自检：共享驱动自身的成功、失败、超时与显式后端/profile 选择。

mod support;

use std::time::{Duration, Instant};

use dolphin_compiler::{BackendChoice, BuildProfile};
use support::{DEFAULT_TIMEOUT, run_project};

const LEAK_PROGRAM: &str = r#"
use std.mem;

fn main() {
    val bytes = mem.alloc<u8>(8);
    bytes[0] = 1_u8;
    return 42;
}
"#;

#[test]
fn harness_captures_success_stdout_and_exit() {
    let result = run_project(
        BackendChoice::Cranelift,
        BuildProfile::Debug,
        &[(
            "src/main.do",
            r#"fn main() { println("ok {}", 7); return 7; }"#,
        )],
        DEFAULT_TIMEOUT,
    );
    assert!(
        result.build_error.is_none(),
        "build failed: {}",
        result.context()
    );
    assert_eq!(result.exit, Some(7), "{}", result.context());
    assert_eq!(result.stdout, "ok 7\n", "{}", result.context());
    assert!(result.stderr.is_empty(), "{}", result.context());
    assert!(!result.timed_out, "{}", result.context());
}

#[test]
fn harness_reports_compile_failure_without_running() {
    let result = run_project(
        BackendChoice::Cranelift,
        BuildProfile::Debug,
        &[("src/main.do", "fn main() { return missing_name; }")],
        DEFAULT_TIMEOUT,
    );
    assert!(result.build_error.is_some(), "{}", result.context());
    assert_eq!(result.exit, None, "{}", result.context());
    assert!(!result.timed_out, "{}", result.context());
}

#[test]
fn harness_captures_runtime_trap_exit_and_stderr() {
    let result = run_project(
        BackendChoice::Cranelift,
        BuildProfile::Debug,
        &[(
            "src/main.do",
            "fn main() { val values = [1, 2]; var index = 2; return values[index]; }",
        )],
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

#[test]
fn harness_honors_dolphin_profile() {
    let debug = run_project(
        BackendChoice::Cranelift,
        BuildProfile::Debug,
        &[("src/main.do", LEAK_PROGRAM)],
        DEFAULT_TIMEOUT,
    );
    assert_eq!(debug.exit, Some(42), "{}", debug.context());
    assert!(
        debug.stderr.contains("leaked"),
        "Debug profile must report the leak: {}",
        debug.context()
    );

    let release = run_project(
        BackendChoice::Cranelift,
        BuildProfile::Release,
        &[("src/main.do", LEAK_PROGRAM)],
        DEFAULT_TIMEOUT,
    );
    assert_eq!(release.exit, Some(42), "{}", release.context());
    assert!(
        release.stderr.is_empty(),
        "Release profile must not report leaks: {}",
        release.context()
    );
}

#[test]
fn harness_terminates_timeout_and_reaps_child() {
    let started = Instant::now();
    let result = run_project(
        BackendChoice::Cranelift,
        BuildProfile::Debug,
        &[("src/main.do", "fn main() { loop { } }")],
        Duration::from_millis(500),
    );
    assert!(
        result.build_error.is_none(),
        "build failed: {}",
        result.context()
    );
    assert!(result.timed_out, "{}", result.context());
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "timeout must terminate the child promptly: {}",
        result.context()
    );
}

#[cfg(feature = "llvm")]
#[test]
fn harness_selects_explicit_llvm_backend() {
    let result = run_project(
        BackendChoice::Llvm,
        BuildProfile::Release,
        &[(
            "src/main.do",
            r#"fn main() { println("llvm {}", 3 * 4); return 0; }"#,
        )],
        DEFAULT_TIMEOUT,
    );
    assert!(
        result.build_error.is_none(),
        "build failed: {}",
        result.context()
    );
    assert!(result.succeeded(), "{}", result.context());
    assert_eq!(result.stdout, "llvm 12\n", "{}", result.context());
}
