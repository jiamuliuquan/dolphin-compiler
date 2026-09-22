//! 构建脚本：在编译编译器时，把运行时 C 源码预编译为目标文件。
//!
//! 产物写入 `OUT_DIR/dolphin_runtime_object_debug` 与
//! `OUT_DIR/dolphin_runtime_object_release`（统一文件名，无平台后缀），
//! 由 `platform.rs` 通过 `include_bytes!` 嵌入：Debug profile 链接检测版（含存活
//! 分配登记表与泄漏报告），Release profile 链接普通版（M14-C/R04）。
//!
//! 同时探测 Unix 目标链接所需的参数并生成 `OUT_DIR/link_args.rs`，作为运行时
//! 探测失败时的自包含回退。

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../runtime/unix_runtime.c");
    println!("cargo:rerun-if-changed=../../runtime/windows_runtime.cpp");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"));
    let target = env::var("TARGET").expect("TARGET is set by Cargo");

    if target.contains("windows") {
        compile_windows(&out_dir, &manifest_dir);
    } else {
        compile_unix(&out_dir, &manifest_dir);
    }

    probe_unix_link_args(&target, &out_dir);
}

/// 探测 Unix 目标（Linux ELF / macOS Mach-O）链接所需的参数，生成
/// `OUT_DIR/link_args.rs` 供 crate `include!`，使 `rust-lld` 无需系统 `cc`
/// 即可链接出可执行文件。
///
/// Windows 的 COFF 由 `rust-lld -flavor link` 自动解析默认库，无需这些参数；
/// Linux 的 GNU flavor 需显式提供 CRT 启动对象、`-L` 与 `-dynamic-linker`；
/// macOS 的 darwin flavor 需 `-arch`、`-platform_version` 与 `-syslibroot`。
/// 这些值在构建编译器时用系统 `cc`/`xcrun` 探测（最终用户构建 Dolphin 程序时
/// 不再需要 C 编译器）。
fn probe_unix_link_args(target: &str, out_dir: &Path) {
    let mut prefix: Vec<String> = Vec::new();
    let mut lib: Vec<String> = Vec::new();
    let mut suffix: Vec<String> = Vec::new();

    if target.contains("linux") {
        let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
        if let Some(linker) = print_file_name(&cc, "ld-linux-x86-64.so.2") {
            prefix.push("-dynamic-linker".to_string());
            prefix.push(linker.display().to_string());
        }
        if let Some(libc) = print_file_name(&cc, "libc.so")
            && let Some(dir) = libc.parent()
        {
            prefix.push("-L".to_string());
            prefix.push(dir.display().to_string());
        }
        for object in ["Scrt1.o", "crti.o", "crtbeginS.o"] {
            if let Some(path) = print_file_name(&cc, object) {
                prefix.push(path.display().to_string());
            }
        }
        lib.push("-lc".to_string());
        for object in ["crtendS.o", "crtn.o"] {
            if let Some(path) = print_file_name(&cc, object) {
                suffix.push(path.display().to_string());
            }
        }
    } else if target.contains("darwin") {
        prefix.push("-arch".to_string());
        prefix.push(macos_arch(target).to_string());
        if let Some(sdk) = xcrun("--show-sdk-version") {
            prefix.push("-platform_version".to_string());
            prefix.push("macos".to_string());
            prefix.push(sdk.clone());
            prefix.push(sdk);
        }
        if let Some(sdk_path) = xcrun("--show-sdk-path") {
            prefix.push("-syslibroot".to_string());
            prefix.push(sdk_path);
        }
        lib.push("-lSystem".to_string());
    }

    let contents = format!(
        "pub const LINK_PREFIX: &[&str] = &{};\npub const LINK_LIB: &[&str] = &{};\npub const LINK_SUFFIX: &[&str] = &{};\n",
        rust_literal(&prefix),
        rust_literal(&lib),
        rust_literal(&suffix)
    );
    std::fs::write(out_dir.join("link_args.rs"), contents).expect("failed to write link_args.rs");
}

/// 从 Rust target 三元组推导 macOS 的 `-arch` 值（`aarch64` → `arm64`）。
fn macos_arch(target: &str) -> &'static str {
    if target.contains("aarch64") {
        "arm64"
    } else if target.contains("x86_64") {
        "x86_64"
    } else {
        "arm64"
    }
}

/// 用 `xcrun` 探测 macOS SDK 信息（SDK 路径、SDK 版本）。
fn xcrun(arg: &str) -> Option<String> {
    let output = Command::new("xcrun").arg(arg).output().ok()?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

/// 把 `&[String]` 渲染为 Rust 字符串字面量数组源码。
fn rust_literal(items: &[String]) -> String {
    let inner = items
        .iter()
        .map(|item| format!("{item:?},"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("[{inner}]")
}

/// 用 `cc -print-file-name=<name>` 探测系统对象的绝对路径；找不到时返回 `None`。
///
/// 返回前做路径规范化（`cc` 常返回含 `../../` 的路径，`rust-lld` 不会自行
/// 规范化，导致 `-L` 或对象路径失效）。
fn print_file_name(cc: &str, name: &str) -> Option<PathBuf> {
    let output = Command::new(cc)
        .arg(format!("-print-file-name={name}"))
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() || path == name {
        return None;
    }
    let path = PathBuf::from(path);
    // 规范化（解析 `..`/`.`）：`cc` 常返回含 `../../` 的路径，`rust-lld` 不会
    // 自行规范化，导致 `-L` 或对象路径失效。探测目标真实存在，canonicalize 应成功。
    std::fs::canonicalize(&path).ok()
}

/// Unix 系（macOS / Linux）：用系统 `cc` 编译 `unix_runtime.c`。
///
/// M14-C/R04 起编译两份运行时：Debug 版定义 `DOLPHIN_DEBUG_RUNTIME`（含存活
/// 分配登记表与泄漏报告），Release 版不含检测。两份都在这里预编译并内嵌，
/// 由 `BuildProfile` 决定链接哪一份。
fn compile_unix(out_dir: &Path, manifest_dir: &Path) {
    let source = manifest_dir.join("../../runtime/unix_runtime.c");
    for (debug, name) in [
        (true, "dolphin_runtime_object_debug"),
        (false, "dolphin_runtime_object_release"),
    ] {
        let object = out_dir.join(name);
        let mut command = Command::new("cc");
        command.arg("-std=c11").arg("-c").arg("-o").arg(&object);
        if debug {
            command.arg("-DDOLPHIN_DEBUG_RUNTIME=1");
        }
        let status = command
            .arg(&source)
            .status()
            .expect("failed to run `cc` to build the Dolphin runtime");
        assert!(
            status.success(),
            "`cc` failed to compile the Dolphin runtime"
        );
    }
}

/// Windows：用 MSVC `cl` 编译 `windows_runtime.cpp` 的 Debug / Release 两份。
///
/// `cl` 通常不在普通 PowerShell 的 PATH 中，需要通过 vcvars 环境激活。
/// 已激活 MSVC 环境时直接调用 `cl`；否则写一个临时 `.bat` 先 `call vcvars64.bat`
/// 再执行 `cl`。使用 `.bat` 而不是 `cmd /c` 拼接字符串，避免 Windows 命令行
/// 参数引号转义破坏带空格的 vcvars 路径。
fn compile_windows(out_dir: &Path, manifest_dir: &Path) {
    let source = manifest_dir.join("../../runtime/windows_runtime.cpp");
    let use_cl = cl_in_path();
    let vcvars = if use_cl {
        None
    } else {
        Some(locate_vcvars64())
    };

    for (debug, name) in [
        (true, "dolphin_runtime_object_debug"),
        (false, "dolphin_runtime_object_release"),
    ] {
        // MSVC `cl /Fo` 会自动补 `.obj` 后缀；先让 cl 生成 `.obj`，再重命名为
        // 无后缀的统一名字，使 `include_bytes!` 路径全平台一致。
        let object = out_dir.join(format!("{name}.obj"));
        let final_object = out_dir.join(name);
        let debug_flag = debug.then_some("/DDOLPHIN_DEBUG_RUNTIME=1");
        let success = if use_cl {
            let mut command = Command::new("cl");
            command
                .arg("/nologo")
                // 源码是 UTF-8（含中文注释）；不加 `/utf-8` 时 MSVC 按系统代码页
                // 解析，在中文 Windows（936）上会吞掉后续字节导致编译失败。
                .arg("/utf-8")
                .arg("/TP")
                .arg("/c")
                .arg(format!("/Fo{}", object.display()));
            if let Some(flag) = debug_flag {
                command.arg(flag);
            }
            command
                .arg(&source)
                .status()
                .expect("failed to run `cl` to build the Dolphin runtime")
                .success()
        } else {
            let script = out_dir.join(format!("build_runtime_{name}.bat"));
            let debug_flag = debug_flag
                .map(|flag| format!(" {flag}"))
                .unwrap_or_default();
            let contents = format!(
                "@echo off\r\ncall \"{}\" >nul\r\ncl /nologo /utf-8 /TP /c /Fo\"{}\"{} \"{}\"\r\n",
                vcvars.as_ref().unwrap().display(),
                object.display(),
                debug_flag,
                source.display()
            );
            std::fs::write(&script, contents).expect("failed to write runtime build script");
            Command::new(&script)
                .status()
                .expect("failed to run the runtime build script")
                .success()
        };
        assert!(success, "`cl` failed to compile the Dolphin runtime");
        std::fs::rename(&object, &final_object)
            .expect("failed to rename the compiled runtime object");
    }
}

/// 定位 `vcvars64.bat`：优先用微软官方的 `vswhere.exe` 查询 VS 安装路径，
/// 再回退到常见安装目录（含 Program Files 与 Program Files (x86)）逐版本探测。
///
/// GitHub Actions 的 `windows-latest` 镜像把 Visual Studio Enterprise 2022 装在
/// `C:\Program Files\Microsoft Visual Studio\2022\Enterprise`；本地开发机可能是
/// Community/BuildTools 或其他盘符。`vswhere` 是 VS 自带的定位工具，最可靠。
fn locate_vcvars64() -> PathBuf {
    // 1. 用 vswhere 查询带 C++ 工具集的 VS 安装路径。
    for vswhere in [
        r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe",
        r"C:\Program Files\Microsoft Visual Studio\Installer\vswhere.exe",
    ] {
        if let Ok(installation) = Command::new(vswhere)
            .args([
                "-latest",
                "-products",
                "*",
                "-requires",
                "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                "-property",
                "installationPath",
            ])
            .output()
        {
            let path = String::from_utf8_lossy(&installation.stdout)
                .trim()
                .to_string();
            if !path.is_empty() {
                let vcvars = Path::new(&path)
                    .join("VC")
                    .join("Auxiliary")
                    .join("Build")
                    .join("vcvars64.bat");
                if vcvars.exists() {
                    return vcvars;
                }
            }
        }
    }

    // 2. 回退：逐版本/版本探测常见安装目录。
    let roots = ["ProgramFiles", "ProgramFiles(x86)"]
        .into_iter()
        .filter_map(|key| env::var(key).ok())
        .map(|root| PathBuf::from(root).join("Microsoft Visual Studio"));

    for root in roots {
        for year in ["2022", "2019"] {
            for edition in ["Community", "Professional", "Enterprise", "BuildTools"] {
                let vcvars = root
                    .join(year)
                    .join(edition)
                    .join("VC")
                    .join("Auxiliary")
                    .join("Build")
                    .join("vcvars64.bat");
                if vcvars.exists() {
                    return vcvars;
                }
            }
        }
    }

    // 诊断：打印环境与已探测的路径，便于定位 CI 上找不到 VS 的原因。
    eprintln!("[build.rs] cl_in_path={}", cl_in_path());
    eprintln!(
        "[build.rs] ProgramFiles={:?}",
        env::var("ProgramFiles").ok()
    );
    eprintln!(
        "[build.rs] ProgramFiles(x86)={:?}",
        env::var("ProgramFiles(x86)").ok()
    );
    eprintln!("[build.rs] PATH={:?}", env::var("PATH").ok());

    panic!(
        "`cl` not found in PATH and no vcvars64.bat located; \
         run this build from an MSVC developer prompt (M12 requires a C compiler \
         only to build the compiler itself)"
    );
}

fn cl_in_path() -> bool {
    if let Ok(path) = env::var("PATH") {
        return env::split_paths(&path).any(|dir| dir.join("cl.exe").is_file());
    }
    false
}
