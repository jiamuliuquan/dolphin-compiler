//! M16 后端接口稳定性：同一份前端 IR 在 Cranelift 与 LLVM 上产生一致行为。
//!
//! 仅在同时启用 `cranelift` 与 `llvm` feature 时编译
//! （`cargo test --features llvm`）。测试覆盖标量算术、结构体、枚举与 `match`、
//! 数组与 `for`、字符串格式化与返回值，除两后端一致性外还断言固定
//! exit=25、stdout=`7 10 12 25\n`、stderr 为空。
#![cfg(all(feature = "cranelift", feature = "llvm"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use dolphin_compiler::{
    BackendChoice, BuildOptions, BuildProfile, BuildSettings, build_with_profile,
};

const PROGRAM: &str = r#"
struct Point {
    x: i32,
    y: i32,
}

enum Shape {
    Circle(i32),
    Square(i32),
}

fn area(s: Shape): i32 {
    val result = match s {
        Shape.Circle(r) => 3 * r * r,
        Shape.Square(a) => a * a,
    };
    return result;
}

fn sum(values: [i32; 4]): i32 {
    var total = 0;
    for v in values {
        total += v;
    }
    return total;
}

fn main() {
    val p = Point(3, 4);
    val values = [1, 2, 3, 4];
    println("{} {} {} {}", p.x + p.y, sum(values), area(Shape.Circle(2)), area(Shape.Square(5)));
    return area(Shape.Square(5));
}
"#;

fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dolphin-backend-{tag}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn run_on(backend: BackendChoice, profile: BuildProfile) -> Output {
    let dir = temp_dir(backend.name());
    let source = dir.join("main.do");
    fs::write(&source, PROGRAM).expect("write source");
    let output = dir.join("program");
    let artifact = build_with_profile(
        BuildOptions {
            input: source,
            output: Some(output.clone()),
        },
        profile,
        BuildSettings::with_backend(backend),
    )
    .expect("backend should build the program");
    let result = Command::new(&artifact.executable)
        .output()
        .expect("built program should run");
    fs::remove_dir_all(&dir).ok();
    result
}

/// 两后端一致只是最低要求；这里同时钉住规范期望，避免两边以相同方式错误。
fn assert_backends_agree(profile: BuildProfile) {
    let cranelift = run_on(BackendChoice::Cranelift, profile);
    let llvm = run_on(BackendChoice::Llvm, profile);
    for (backend, output) in [("cranelift", &cranelift), ("llvm", &llvm)] {
        assert_eq!(
            output.status.code(),
            Some(25),
            "{backend} exit differs (profile={profile:?})"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "7 10 12 25\n",
            "{backend} stdout differs (profile={profile:?})"
        );
        assert!(
            output.stderr.is_empty(),
            "{backend} stderr must be empty (profile={profile:?}): {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        cranelift.status.code(),
        llvm.status.code(),
        "exit codes differ: cranelift={:?} llvm={:?}",
        cranelift.status.code(),
        llvm.status.code()
    );
    assert_eq!(
        String::from_utf8_lossy(&cranelift.stdout),
        String::from_utf8_lossy(&llvm.stdout),
        "stdout differs between backends"
    );
}

#[test]
fn debug_backends_agree() {
    assert_backends_agree(BuildProfile::Debug);
}

#[test]
fn release_backends_agree() {
    assert_backends_agree(BuildProfile::Release);
}

/// IR 层不依赖具体后端的静态检查：`dolphin-ir` 的公开类型不出现后端类型名。
#[test]
fn typed_ir_has_no_backend_types() {
    let ir_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/dolphin-ir/src/ir.rs");
    let text = fs::read_to_string(&ir_source).expect("read dolphin-ir");
    for forbidden in ["cranelift", "inkwell", "llvm"] {
        assert!(
            !text.to_ascii_lowercase().contains(forbidden),
            "backend-independent IR must not mention `{forbidden}`"
        );
    }
}

/// LLVM 后端在 Debug 配置下产出 DWARF：ELF 可执行文件内嵌 `.debug_line` 与
/// `.debug_info`；Mach-O 链接器按平台惯例不把 DWARF 复制进可执行文件（保留在
/// 对象文件与 debug map 中，`dsymutil` 可据此生成 dSYM），因此 macOS 检查对象
/// 文件的 `__debug_line`/`__debug_info` 段名。
#[test]
fn llvm_debug_profile_emits_dwarf() {
    let dir = temp_dir("dwarf");
    let source = dir.join("main.do");
    fs::write(&source, "fn main() { return 0; }\n").expect("write source");
    let output = dir.join("program");
    let artifact = build_with_profile(
        BuildOptions {
            input: source,
            output: Some(output),
        },
        BuildProfile::Debug,
        BuildSettings::with_backend(BackendChoice::Llvm),
    )
    .expect("llvm debug build should succeed");
    let (debug_artifact, sections): (PathBuf, [&[u8]; 2]) = if cfg!(target_os = "macos") {
        (
            dir.join("program.o"),
            [b"__debug_line".as_slice(), b"__debug_info".as_slice()],
        )
    } else {
        (
            artifact.executable.clone(),
            [b".debug_line".as_slice(), b".debug_info".as_slice()],
        )
    };
    let bytes = fs::read(&debug_artifact).expect("read debug artifact");
    for section in sections {
        assert!(
            bytes.windows(section.len()).any(|window| window == section),
            "{} must contain `{}`",
            debug_artifact.display(),
            String::from_utf8_lossy(section)
        );
    }
    fs::remove_dir_all(&dir).ok();
}
