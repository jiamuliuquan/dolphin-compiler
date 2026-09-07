//! 目标平台与工具链抽象（M10 引入，M11 加入 Windows，M12 内嵌运行时与 LLD）。
//!
//! 把链接流程中散落的 Unix/`cc` 假设集中到 `TargetPlatform`，为不同目标平台
//! 建立稳定边界：目标三元组、目标文件后缀、可执行文件后缀、ABI、运行时目标
//! 文件与链接参数都由平台实现提供，通用流程不再硬编码。
//!
//! M12 起运行时不再由构建时用系统 C 编译器现场编译，而是在编译编译器时
//! （`build.rs`）预编译成目标文件并以 [`include_bytes!`] 内嵌。链接默认使用
//! 可再分发的 `rust-lld`，统一 ELF、Mach-O 与 COFF；系统链接器仅作为
//! `--linker` 的受控回退保留。
//!
//! 当前只支持宿主即目标（交叉编译延后），但对未知宿主三元组会在编译器侧
//! 给出诊断，而不是等外部链接器失败。

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use target_lexicon::{Architecture, OperatingSystem, Triple};

use crate::diagnostic::Diagnostic;

/// 目标 ABI：描述函数调用约定与对象文件布局的关键差异。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Abi {
    /// Apple 平台（macOS / iOS）的 Aarch64 调用约定。
    AppleAarch64,
    /// System V AMD64 调用约定（Linux、BSD 等 ELF x86_64 平台）。
    SystemV,
    /// MSVC x64 调用约定（Windows，`.obj`/`.exe`，C 符号不带前导下划线）。
    MsVc,
}

impl Abi {
    pub fn name(self) -> &'static str {
        match self {
            Abi::AppleAarch64 => "apple-aarch64",
            Abi::SystemV => "system-v",
            Abi::MsVc => "msvc-x64",
        }
    }
}

/// 目标平台：统一三元组、目标文件后缀、可执行文件后缀与 ABI，
/// 并提供内嵌运行时目标文件与链接命令。
pub trait TargetPlatform {
    /// 目标三元组，供 Cranelift 后端选择 ISA 与代码模型。
    fn triple(&self) -> Triple;

    /// 目标文件（object file）后缀，不含点，例如 `"o"`。
    fn object_suffix(&self) -> &'static str;

    /// 可执行文件后缀，不含点；Unix 系为空字符串。
    fn executable_suffix(&self) -> &'static str;

    /// 目标 ABI。
    fn abi(&self) -> Abi;

    /// 内嵌的运行时目标文件字节（M12 起由 `build.rs` 预编译）。
    fn runtime_object_bytes(&self) -> &'static [u8];

    /// 运行时 C 源码，仅用于一致性校验与符号测试，不再参与构建。
    fn runtime_source(&self) -> &'static str;

    /// 目标平台上的 C 符号名。
    ///
    /// 当前 Unix 系与 MSVC x64 都保持原名（MSVC x64 的 C 符号不带前导下划线，
    /// 下划线装饰只存在于 MSVC x86）。跨越 C 边界的符号（`main` 和运行时函数）
    /// 必须经过它，保证代码生成与平台 C 工具链的命名规则一致。
    fn c_symbol(&self, name: &str) -> String;

    /// `rust-lld` 的目标 flavor：`gnu`（ELF）、`darwin`（Mach-O）、`link`（COFF）。
    fn link_flavor(&self) -> &'static str;

    /// 默认（自包含）链接器名称，用于 `dc env` 显示，M12 起为 `rust-lld`。
    fn linker_name(&self) -> &'static str;

    /// 系统链接器名称，用于 `--linker` 回退，例如 `"cc"` 或 `"link"`。
    fn system_linker_name(&self) -> &'static str;

    /// 生成把 `object`（及 `runtime` 运行时目标文件）链接成 `output` 的默认
    /// 自包含链接命令参数列表（不含 argv[0]，以 `rust-lld` 开头）。
    ///
    /// 返回 `Vec<OsString>` 便于测试直接断言，真正执行由 `linker` 模块负责。
    fn link_command(&self, object: &Path, runtime: &Path, output: &Path) -> Vec<OsString>;

    /// 生成使用系统链接器的回退命令参数列表（`--linker` 场景，不含 argv[0]）。
    ///
    /// 返回 `Vec<OsString>` 便于测试直接断言，真正执行由 `linker` 模块负责。
    fn system_link_command(&self, object: &Path, runtime: &Path, output: &Path) -> Vec<OsString>;
}

/// Unix 系平台（macOS 与 Linux 共用运行时与自包含 LLD 链接器）。
///
/// 运行时目标文件由 `build.rs` 用系统 `cc` 预编译并内嵌（M12）。
#[derive(Debug, Clone)]
pub struct UnixPlatform {
    triple: Triple,
}

impl TargetPlatform for UnixPlatform {
    fn triple(&self) -> Triple {
        self.triple.clone()
    }

    fn object_suffix(&self) -> &'static str {
        "o"
    }

    fn executable_suffix(&self) -> &'static str {
        ""
    }

    fn abi(&self) -> Abi {
        if self.triple.operating_system.is_like_darwin() {
            Abi::AppleAarch64
        } else {
            Abi::SystemV
        }
    }

    fn runtime_object_bytes(&self) -> &'static [u8] {
        include_bytes!(concat!(env!("OUT_DIR"), "/dolphin_runtime_object"))
    }

    fn runtime_source(&self) -> &'static str {
        include_str!("../runtime/unix_runtime.c")
    }

    fn c_symbol(&self, name: &str) -> String {
        name.to_string()
    }

    fn link_flavor(&self) -> &'static str {
        if self.triple.operating_system.is_like_darwin() {
            "darwin"
        } else {
            "gnu"
        }
    }

    fn linker_name(&self) -> &'static str {
        "rust-lld"
    }

    fn system_linker_name(&self) -> &'static str {
        "cc"
    }

    fn link_command(&self, object: &Path, runtime: &Path, output: &Path) -> Vec<OsString> {
        let mut command = vec![
            self.linker_name().into(),
            OsString::from("-flavor"),
            self.link_flavor().into(),
        ];
        // Linux ELF / macOS Mach-O：`rust-lld` 需显式提供平台相关参数（CRT 对象、
        // 架构、平台版本、动态链接器、系统库等）。Windows 的 COFF 由
        // `-flavor link` 自动处理（常量均为空）。
        //
        // 若在编译编译器时把平台相关的绝对路径/版本固化成常量，跨机器分发后会
        // 因目标机的工具链不同而链接失败：
        //   - Linux：CRT 对象（`crtbeginS.o` 等）位于 GCC 版本目录
        //     （如 `/usr/lib/gcc/x86_64-linux-gnu/<ver>/`）。
        //   - macOS：`-syslibroot` 固化了 Xcode/CLT 的 SDK 路径，`-platform_version`
        //     固化了 SDK 版本号。
        // 因此 Linux 与 macOS 都改为运行时现查（Linux 用 `cc -print-file-name`，
        // macOS 用 `xcrun`），探测失败再回退到 build.rs 编译期固化的 `link_args.rs`
        // 常量，保持自包含能力（最终用户不装 C 编译器/Xcode 时仍能靠常量工作）。
        if self.triple.operating_system == OperatingSystem::Linux {
            if let Some((prefix, lib, suffix)) = probe_linux_link_args_runtime() {
                command.extend(prefix.into_iter());
                command.push(object.as_os_str().to_owned());
                command.push(runtime.as_os_str().to_owned());
                command.extend(lib.into_iter());
                command.extend(suffix.into_iter());
                command.push(OsString::from("-o"));
                command.push(output.as_os_str().to_owned());
                return command;
            }
        } else if self.triple.operating_system.is_like_darwin() {
            if let Some((prefix, lib, suffix)) = probe_darwin_link_args_runtime(self) {
                command.extend(prefix.into_iter());
                command.push(object.as_os_str().to_owned());
                command.push(runtime.as_os_str().to_owned());
                command.extend(lib.into_iter());
                command.extend(suffix.into_iter());
                command.push(OsString::from("-o"));
                command.push(output.as_os_str().to_owned());
                return command;
            }
        }
        command.extend(link_args::LINK_PREFIX.iter().map(OsString::from));
        command.push(object.as_os_str().to_owned());
        command.push(runtime.as_os_str().to_owned());
        command.extend(link_args::LINK_LIB.iter().map(OsString::from));
        command.extend(link_args::LINK_SUFFIX.iter().map(OsString::from));
        command.push(OsString::from("-o"));
        command.push(output.as_os_str().to_owned());
        command
    }

    fn system_link_command(&self, object: &Path, runtime: &Path, output: &Path) -> Vec<OsString> {
        vec![
            self.system_linker_name().into(),
            object.as_os_str().to_owned(),
            runtime.as_os_str().to_owned(),
            OsString::from("-o"),
            output.as_os_str().to_owned(),
        ]
    }
}

/// Windows x86_64 平台（M11 引入，M12 内嵌运行时与 LLD）。
///
/// 采用 MSVC x64 ABI 与 COFF 对象格式：目标文件 `.obj`、可执行文件 `.exe`，
/// C 符号不带前导下划线（x64 约定）。运行时由 `build.rs` 用 MSVC `cl` 预编译
/// 并内嵌；链接默认使用 `rust-lld -flavor link`，`link` 作为 `--linker` 回退。
#[derive(Debug, Clone)]
pub struct WindowsPlatform {
    triple: Triple,
}

impl TargetPlatform for WindowsPlatform {
    fn triple(&self) -> Triple {
        self.triple.clone()
    }

    fn object_suffix(&self) -> &'static str {
        "obj"
    }

    fn executable_suffix(&self) -> &'static str {
        "exe"
    }

    fn abi(&self) -> Abi {
        Abi::MsVc
    }

    fn runtime_object_bytes(&self) -> &'static [u8] {
        include_bytes!(concat!(env!("OUT_DIR"), "/dolphin_runtime_object"))
    }

    fn runtime_source(&self) -> &'static str {
        include_str!("../runtime/windows_runtime.cpp")
    }

    fn c_symbol(&self, name: &str) -> String {
        // MSVC x64 的 C 符号不带前导下划线（x86 才带），`main` 就是 CRT 查找的
        // 入口符号；运行时 C 函数同样以原名导出。
        name.to_string()
    }

    fn link_flavor(&self) -> &'static str {
        "link"
    }

    fn linker_name(&self) -> &'static str {
        "rust-lld"
    }

    fn system_linker_name(&self) -> &'static str {
        "link"
    }

    fn link_command(&self, object: &Path, runtime: &Path, output: &Path) -> Vec<OsString> {
        let mut out = OsString::from("/OUT:");
        out.push(output);
        vec![
            self.linker_name().into(),
            OsString::from("-flavor"),
            self.link_flavor().into(),
            out,
            object.as_os_str().to_owned(),
            runtime.as_os_str().to_owned(),
            OsString::from("/SUBSYSTEM:CONSOLE"),
        ]
    }

    fn system_link_command(&self, object: &Path, runtime: &Path, output: &Path) -> Vec<OsString> {
        let mut out = OsString::from("/OUT:");
        out.push(output);
        vec![
            self.system_linker_name().into(),
            OsString::from("/nologo"),
            out,
            object.as_os_str().to_owned(),
            runtime.as_os_str().to_owned(),
            OsString::from("/SUBSYSTEM:CONSOLE"),
        ]
    }
}

/// 从宿主三元组派生目标平台。
///
/// 当前支持 macOS ARM64、Linux x86_64 与 Windows x86_64（宿主即目标）。
/// 未知平台返回编译器诊断，而不是等外部工具失败——这正是 M10 要求的
/// 「对不支持的目标给出编译器诊断」。
pub fn host() -> Result<Box<dyn TargetPlatform>, Diagnostic> {
    let triple = Triple::host();
    match (&triple.architecture, &triple.operating_system) {
        (Architecture::Aarch64(_), os) if os.is_like_darwin() => {
            Ok(Box::new(UnixPlatform { triple }))
        }
        (Architecture::X86_64, OperatingSystem::Linux) => Ok(Box::new(UnixPlatform { triple })),
        (Architecture::X86_64, OperatingSystem::Windows) => {
            Ok(Box::new(WindowsPlatform { triple }))
        }
        _ => Err(Diagnostic::plain(format!(
            "unsupported target platform `{triple}`: supports macOS ARM64, Linux x86_64, and Windows x86_64"
        ))),
    }
}

/// 运行时探测 Linux ELF 链接参数（`rust-lld` GNU flavor 所需）。
///
/// 与 `build.rs::probe_unix_link_args` 逻辑一致，但在**每次 `dc build` 时**用本机
/// `cc -print-file-name=<name>` 现查 CRT 对象、libc 目录与动态链接器路径，避免
/// 编译期固化的 GCC 版本目录（如 `/usr/lib/gcc/x86_64-linux-gnu/13/`）在跨机器
/// 分发后失效。返回 `(prefix, lib, suffix)` 三段参数；任一关键对象（`Scrt1.o`）
/// 探测不到则返回 `None`，调用方回退到编译期常量。
///
/// 注意：这要求目标机装有 `cc`（C 编译器）。自包含发行（无 C 编译器）场景会
/// 探测失败并自动回退到 `link_args.rs` 常量。
fn probe_linux_link_args_runtime() -> Option<(Vec<OsString>, Vec<OsString>, Vec<OsString>)> {
    let cc = env_cc();
    let mut prefix: Vec<OsString> = Vec::new();
    let mut lib: Vec<OsString> = Vec::new();
    let mut suffix: Vec<OsString> = Vec::new();

    if let Some(linker) = print_file_name(&cc, "ld-linux-x86-64.so.2") {
        prefix.push(OsString::from("-dynamic-linker"));
        prefix.push(linker.into_os_string());
    }
    if let Some(libc) = print_file_name(&cc, "libc.so")
        && let Some(dir) = libc.parent()
    {
        prefix.push(OsString::from("-L"));
        prefix.push(dir.into());
    }
    // `Scrt1.o` 是 CRT 入口对象，缺失说明没有可用工具链，整个探测视为失败。
    let mut objects = Vec::new();
    for object in ["Scrt1.o", "crti.o", "crtbeginS.o"] {
        objects.push(print_file_name(&cc, object)?);
    }
    prefix.extend(objects.into_iter().map(PathBuf::into_os_string));
    lib.push(OsString::from("-lc"));
    for object in ["crtendS.o", "crtn.o"] {
        suffix.push(print_file_name(&cc, object)?.into_os_string());
    }
    Some((prefix, lib, suffix))
}

/// 运行时探测 macOS Mach-O 链接参数（`rust-lld` darwin flavor 所需）。
///
/// 与 `build.rs::probe_unix_link_args` 的 darwin 分支逻辑一致，但在**每次
/// `dc build` 时**用本机 `xcrun --show-sdk-path` / `--show-sdk-version` 现查 SDK
/// 路径与版本号，避免编译期固化的 `-syslibroot` 路径和 `-platform_version` 版本
/// 号在跨机器（不同 Xcode / Command Line Tools 安装位置与版本）分发后失效。
///
/// SDK 路径与版本号任一探测不到则返回 `None`，调用方回退到编译期常量。
///
/// 注意：这要求目标机装有 `xcrun`（Xcode 或 Command Line Tools）。自包含发行
/// （无 Xcode）场景会探测失败并自动回退到 `link_args.rs` 常量。
fn probe_darwin_link_args_runtime(
    platform: &UnixPlatform,
) -> Option<(Vec<OsString>, Vec<OsString>, Vec<OsString>)> {
    let sdk = xcrun("--show-sdk-version")?;
    let sdk_path = xcrun("--show-sdk-path")?;

    let mut prefix: Vec<OsString> = Vec::new();
    prefix.push(OsString::from("-arch"));
    prefix.push(OsString::from(macos_arch(platform)));
    prefix.push(OsString::from("-platform_version"));
    prefix.push(OsString::from("macos"));
    prefix.push(OsString::from(sdk.clone()));
    prefix.push(OsString::from(sdk));
    prefix.push(OsString::from("-syslibroot"));
    prefix.push(OsString::from(sdk_path));

    let lib: Vec<OsString> = vec![OsString::from("-lSystem")];
    let suffix: Vec<OsString> = Vec::new();
    Some((prefix, lib, suffix))
}

/// 从 Rust target 三元组推导 macOS 的 `-arch` 值（`aarch64` → `arm64`）。
fn macos_arch(platform: &UnixPlatform) -> &'static str {
    let triple = platform.triple().to_string();
    if triple.contains("aarch64") {
        "arm64"
    } else if triple.contains("x86_64") {
        "x86_64"
    } else {
        "arm64"
    }
}

/// 用 `xcrun` 探测 macOS SDK 信息（SDK 路径、SDK 版本）。
fn xcrun(arg: &str) -> Option<String> {
    let output = Command::new("xcrun").arg(arg).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

/// 选择 `cc` 命令：优先 `CC` 环境变量，回退 `cc`。
fn env_cc() -> String {
    std::env::var("CC").unwrap_or_else(|_| "cc".to_string())
}/// 用 `cc -print-file-name=<name>` 探测系统对象路径；找不到时返回 `None`。
///
/// 返回前做路径规范化（`cc` 常返回含 `../../` 的路径，`rust-lld` 不会自行
/// 规范化，导致 `-L` 或对象路径失效）。
fn print_file_name(cc: &str, name: &str) -> Option<PathBuf> {
    let output = Command::new(cc)
        .arg(format!("-print-file-name={name}"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() || path == name {
        return None;
    }
    let path = PathBuf::from(path);
    std::fs::canonicalize(&path).ok()
}

/// `build.rs` 生成的链接参数常量（`LINK_PREFIX` / `LINK_SUFFIX`）。
///
/// Linux ELF 链接所需的 CRT 对象、库搜索路径与动态链接器；其他平台为空数组。
/// 作为运行时探测失败时的回退，保证自包含发行（无 C 编译器）仍可链接。
mod link_args {
    include!(concat!(env!("OUT_DIR"), "/link_args.rs"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn triple(target: &str) -> Triple {
        target.parse().expect("test triple should parse")
    }

    fn unix_platform(triple: Triple) -> UnixPlatform {
        UnixPlatform { triple }
    }

    fn windows_platform() -> WindowsPlatform {
        WindowsPlatform {
            triple: triple("x86_64-pc-windows-msvc"),
        }
    }

    #[test]
    fn host_platform_is_supported() {
        // 本机（macOS ARM64、Linux x86_64 或 Windows x86_64）必须能派生平台。
        let platform = host().expect("host platform should be supported");
        assert!(!platform.triple().to_string().is_empty());
    }

    #[test]
    fn unix_platform_suffixes_and_abi() {
        let platform = unix_platform(triple("x86_64-unknown-linux-gnu"));
        assert_eq!(platform.object_suffix(), "o");
        assert_eq!(platform.executable_suffix(), "");
        assert_eq!(platform.abi(), Abi::SystemV);

        let darwin = unix_platform(triple("aarch64-apple-darwin"));
        assert_eq!(darwin.abi(), Abi::AppleAarch64);
    }

    #[test]
    fn windows_platform_suffixes_and_abi() {
        let platform = windows_platform();
        assert_eq!(platform.object_suffix(), "obj");
        assert_eq!(platform.executable_suffix(), "exe");
        assert_eq!(platform.abi(), Abi::MsVc);
    }

    #[test]
    fn c_symbol_mangling_follows_platform() {
        let unix = unix_platform(triple("x86_64-unknown-linux-gnu"));
        assert_eq!(unix.c_symbol("main"), "main");
        assert_eq!(unix.c_symbol("dolphin_print_i32"), "dolphin_print_i32");

        // MSVC x64 与 Unix 一样保持 C 符号原名（无下划线装饰）。
        let windows = windows_platform();
        assert_eq!(windows.c_symbol("main"), "main");
        assert_eq!(windows.c_symbol("dolphin_print_i32"), "dolphin_print_i32");
    }

    #[test]
    fn unix_link_command_uses_rust_lld_gnu_flavor() {
        let platform = unix_platform(triple("x86_64-unknown-linux-gnu"));
        let object = Path::new("/tmp/app.o");
        let runtime = Path::new("/tmp/app.runtime.o");
        let output = Path::new("/tmp/app");

        let command = platform.link_command(object, runtime, output);
        // 前三个元素固定；其后可能带 build.rs 探测注入的平台参数（仅对应平台编译）。
        assert_eq!(command[0], OsString::from("rust-lld"));
        assert_eq!(command[1], OsString::from("-flavor"));
        assert_eq!(command[2], OsString::from("gnu"));
        // 对象与运行时按顺序出现，且以 `-o <output>` 结尾。
        let object_idx = command
            .iter()
            .position(|arg| arg == &OsString::from("/tmp/app.o"))
            .expect("object should appear");
        let runtime_idx = command
            .iter()
            .position(|arg| arg == &OsString::from("/tmp/app.runtime.o"))
            .expect("runtime should appear");
        assert!(object_idx < runtime_idx);
        let last = &command[command.len() - 2..];
        assert_eq!(last[0], OsString::from("-o"));
        assert_eq!(last[1], OsString::from("/tmp/app"));
    }

    #[test]
    fn darwin_link_command_uses_darwin_flavor() {
        let platform = unix_platform(triple("aarch64-apple-darwin"));
        let object = Path::new("/tmp/app.o");
        let runtime = Path::new("/tmp/app.runtime.o");
        let output = Path::new("/tmp/app");

        let command = platform.link_command(object, runtime, output);
        assert_eq!(command[0], OsString::from("rust-lld"));
        assert_eq!(command[1], OsString::from("-flavor"));
        assert_eq!(command[2], OsString::from("darwin"));
        // 对象与运行时按顺序出现，且以 `-o <output>` 结尾。
        let object_idx = command
            .iter()
            .position(|arg| arg == &OsString::from("/tmp/app.o"))
            .expect("object should appear");
        let runtime_idx = command
            .iter()
            .position(|arg| arg == &OsString::from("/tmp/app.runtime.o"))
            .expect("runtime should appear");
        assert!(object_idx < runtime_idx);
        let last = &command[command.len() - 2..];
        assert_eq!(last[0], OsString::from("-o"));
        assert_eq!(last[1], OsString::from("/tmp/app"));
    }

    #[test]
    fn unix_system_link_command_uses_cc() {
        let platform = unix_platform(triple("x86_64-unknown-linux-gnu"));
        let object = Path::new("/tmp/app.o");
        let runtime = Path::new("/tmp/app.runtime.o");
        let output = Path::new("/tmp/app");

        let command = platform.system_link_command(object, runtime, output);
        assert_eq!(
            command,
            vec![
                OsString::from("cc"),
                OsString::from("/tmp/app.o"),
                OsString::from("/tmp/app.runtime.o"),
                OsString::from("-o"),
                OsString::from("/tmp/app"),
            ]
        );
    }

    #[test]
    fn windows_link_command_uses_rust_lld_link_flavor() {
        let platform = windows_platform();
        let object = Path::new("C:\\temp\\app.obj");
        let runtime = Path::new("C:\\temp\\app.runtime.obj");
        let output = Path::new("C:\\temp\\app.exe");

        let command = platform.link_command(object, runtime, output);
        assert_eq!(
            command,
            vec![
                OsString::from("rust-lld"),
                OsString::from("-flavor"),
                OsString::from("link"),
                OsString::from("/OUT:C:\\temp\\app.exe"),
                OsString::from("C:\\temp\\app.obj"),
                OsString::from("C:\\temp\\app.runtime.obj"),
                OsString::from("/SUBSYSTEM:CONSOLE"),
            ]
        );
    }

    #[test]
    fn windows_system_link_command_uses_link() {
        let platform = windows_platform();
        let object = Path::new("C:\\temp\\app.obj");
        let runtime = Path::new("C:\\temp\\app.runtime.obj");
        let output = Path::new("C:\\temp\\app.exe");

        let command = platform.system_link_command(object, runtime, output);
        assert_eq!(
            command,
            vec![
                OsString::from("link"),
                OsString::from("/nologo"),
                OsString::from("/OUT:C:\\temp\\app.exe"),
                OsString::from("C:\\temp\\app.obj"),
                OsString::from("C:\\temp\\app.runtime.obj"),
                OsString::from("/SUBSYSTEM:CONSOLE"),
            ]
        );
    }

    #[test]
    fn link_flavor_matches_platform() {
        assert_eq!(
            unix_platform(triple("x86_64-unknown-linux-gnu")).link_flavor(),
            "gnu"
        );
        assert_eq!(
            unix_platform(triple("aarch64-apple-darwin")).link_flavor(),
            "darwin"
        );
        assert_eq!(windows_platform().link_flavor(), "link");
    }

    #[test]
    fn embedded_runtime_object_is_non_empty() {
        let platform = host().expect("host platform should be supported");
        assert!(
            !platform.runtime_object_bytes().is_empty(),
            "embedded runtime object should be non-empty"
        );
    }

    #[test]
    fn runtime_source_contains_required_symbols() {
        let platform = host().expect("host platform should be supported");
        let source = platform.runtime_source();
        for symbol in [
            "dolphin_print_i32",
            "dolphin_print_string",
            "dolphin_print_bool",
            "dolphin_print_f32",
            "dolphin_print_f64",
            "dolphin_print_char",
            "dolphin_print_i64",
            "dolphin_print_u64",
            "dolphin_string_equal",
        ] {
            assert!(source.contains(symbol), "missing runtime symbol {symbol}");
        }
    }
}
