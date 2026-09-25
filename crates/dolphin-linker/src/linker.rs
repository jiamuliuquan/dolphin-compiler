//! 链接流程：把目标文件与内嵌运行时组合成可执行文件。
//!
//! M12 起运行时不再是构建时用系统 C 编译器现场编译，而是在编译编译器时
//! （`build.rs`）预编译并内嵌。这里把内嵌的运行时目标文件字节落盘，再执行
//! 平台的链接命令（默认系统链接器 `cc`/`link`，`--bundled-linker` 可改用
//! Rust 工具链自带的 `rust-lld`）。
//!
//! 「生成目标文件」与「链接可执行文件」仍拆成两个可独立测试的步骤。

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use dolphin_platform::platform::{NativeInputs, TargetPlatform};
use dolphin_source::diagnostic::Diagnostic;

/// 链接器选择：默认系统链接器（`cc`/`link`），或显式使用 `rust-lld`。
///
/// 发行包不再携带 `rust-lld`，默认系统链接器让发行包从约 114 MB 降到约 9 MB；
/// `--bundled-linker` 保留给装有 Rust 工具链的开发/诊断场景。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LinkerChoice {
    /// 默认：平台系统链接器（Unix `cc`，Windows `link`）。
    #[default]
    System,
    /// `--bundled-linker`：Rust 工具链自带的 `rust-lld`。
    Bundled,
}

/// 把用户目标文件、运行时与原生输入链接成可执行文件。
///
/// 先落盘内嵌的运行时目标文件，再执行平台的链接命令。`debug` 决定使用带存活
/// 分配登记表的检测版运行时（Debug）还是普通版（Release）。`native` 是清单声明的
/// 用户 object/静态库/共享库；`runtime_files` 会复制到可执行文件同目录。
pub fn link(
    platform: &dyn TargetPlatform,
    object: &Path,
    output: &Path,
    linker: LinkerChoice,
    debug: bool,
    native: &NativeInputs,
) -> Result<(), Diagnostic> {
    validate_native_inputs(platform, native)?;
    let runtime = runtime_object_path(object, platform.object_suffix());
    write_runtime(&runtime, platform.runtime_object_bytes(debug))?;
    let command = match linker {
        LinkerChoice::System => platform.system_link_command(object, &runtime, native, output),
        LinkerChoice::Bundled => platform.link_command(object, &runtime, native, output),
    };
    let result = run_link_command(&command, output);
    // 运行时目标文件是本次链接的临时产物，链接结束后清理，避免污染输出目录。
    let _ = fs::remove_file(&runtime);
    result?;
    copy_runtime_files(native, output)?;
    Ok(())
}

/// 校验原生输入文件存在且类型匹配，给出包含文件与目标的诊断（FFI-05）。
fn validate_native_inputs(
    platform: &dyn TargetPlatform,
    native: &NativeInputs,
) -> Result<(), Diagnostic> {
    let triple = platform.triple();
    let check = |paths: &[PathBuf], kind: &str| -> Result<(), Diagnostic> {
        for path in paths {
            if !path.is_file() {
                return Err(Diagnostic::plain(format!(
                    "native {kind} `{}` for target `{triple}` does not exist or is not a file",
                    path.display()
                )));
            }
        }
        Ok(())
    };
    check(&native.objects, "object")?;
    check(&native.static_libs, "static library")?;
    check(&native.shared_libs, "shared library")?;
    check(&native.runtime_files, "runtime file")?;
    Ok(())
}

/// 把 `runtime-files` 复制到可执行文件同目录（Windows DLL、随程序分发的 `.so/.dylib`）。
fn copy_runtime_files(native: &NativeInputs, output: &Path) -> Result<(), Diagnostic> {
    if native.runtime_files.is_empty() {
        return Ok(());
    }
    let directory = output.parent().unwrap_or_else(|| Path::new("."));
    for source in &native.runtime_files {
        let file_name = source.file_name().ok_or_else(|| {
            Diagnostic::plain(format!("invalid runtime file `{}`", source.display()))
        })?;
        let destination = directory.join(file_name);
        // 源与目标可能是同一文件（相对/绝对路径混用）；比较规范化路径避免自拷贝截断。
        if destination.exists()
            && let (Ok(source_real), Ok(destination_real)) =
                (fs::canonicalize(source), fs::canonicalize(&destination))
            && source_real == destination_real
        {
            continue;
        }
        fs::copy(source, &destination).map_err(|error| {
            Diagnostic::plain(format!(
                "could not copy runtime file `{}` to `{}`: {error}",
                source.display(),
                destination.display()
            ))
        })?;
    }
    Ok(())
}

fn write_runtime(path: &Path, bytes: &[u8]) -> Result<(), Diagnostic> {
    fs::write(path, bytes).map_err(|error| {
        Diagnostic::plain(format!(
            "could not write runtime object `{}`: {error}",
            path.display()
        ))
    })
}

/// 执行链接命令。参数列表由平台提供，第一个元素是工具名。
fn run_link_command(command: &[OsString], output: &Path) -> Result<(), Diagnostic> {
    run_command(command, &format!("linking `{}` failed", output.display()))
}

/// 执行一个完整工具命令（第一个参数是工具名），失败时带上工具输出。
///
/// Unix 工具（`cc`）的报错在 stderr，MSVC 工具（`link`）在 stdout，
/// 因此两个流都合并进诊断。
fn run_command(command: &[OsString], failure: &str) -> Result<(), Diagnostic> {
    let (tool, args) = command
        .split_first()
        .ok_or_else(|| Diagnostic::plain("tool command is empty"))?;
    let resolved = resolve_tool(tool);
    let result = Command::new(&resolved)
        .args(args)
        .output()
        .map_err(|error| {
            Diagnostic::plain(format!(
                "could not start `{}` for the Dolphin linker: {error}",
                resolved.to_string_lossy()
            ))
        })?;
    if result.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        let mut details = stdout.trim().to_string();
        if !stderr.trim().is_empty() {
            if !details.is_empty() {
                details.push('\n');
            }
            details.push_str(stderr.trim());
        }
        Err(Diagnostic::plain(format!("{failure}\n{details}")))
    }
}

/// 解析工具名：`--bundled-linker` 的 `rust-lld` 需要定位到完整路径，Windows 的
/// 系统链接器 `link` 需要避开 PATH 上的同名工具；Unix 的 `cc` 保持原样交给
/// PATH 解析。
///
/// `rust-lld` 查找顺序：可执行文件同目录（用户自行放置 `rust-lld` 时）→
/// `DOLPHIN_LLD` 环境变量（build.rs 编译期注入的 Rust 工具链路径）→ PATH →
/// 用 `rustc --print sysroot` 动态查询。最终回退到原名，由 `Command` 报错。
///
/// Windows 上 Git for Windows / MSYS2 的 `usr\bin\link.exe` 是 GNU coreutils 的
/// 硬链接工具，与 MSVC 链接器同名；在 Git Bash 里运行 `dc` 时它常排在 PATH 前面，
/// 直接按名字执行会报 `link: extra operand`（CI 冒烟即因此失败）。因此 `link`
/// 先尝试解析到 MSVC 工具链的完整路径。
fn resolve_tool(tool: &OsString) -> OsString {
    if tool == "rust-lld" {
        return resolve_rust_lld(tool);
    }
    #[cfg(windows)]
    if tool == "link"
        && let Some(path) = resolve_msvc_link()
    {
        return path;
    }
    tool.clone()
}

/// 解析 `rust-lld`；找不到时回退原名，由 `Command` 报错。
fn resolve_rust_lld(tool: &OsString) -> OsString {
    // 1. 可执行文件同目录（用户把 `rust-lld` 与 `dc` 放在一起时）。
    if let Some(path) = find_next_to_executable() {
        return path;
    }
    // 2. 编译期注入的路径（开发/`cargo test` 环境：dc 同目录无 rust-lld）。
    if let Some(path) = option_env!("DOLPHIN_LLD")
        && Path::new(path).exists()
    {
        return path.into();
    }
    // 3. PATH。
    if let Some(path) = find_in_path("rust-lld") {
        return path;
    }
    // 4. rustc sysroot。
    if let Some(path) = find_via_rustc() {
        return path;
    }
    tool.clone()
}

/// 在 `dc` 可执行文件所在目录查找 `rust-lld`（用户手动放置的兼容路径）。
fn find_next_to_executable() -> Option<OsString> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if cfg!(windows) {
        "rust-lld.exe"
    } else {
        "rust-lld"
    };
    let candidate = dir.join(name);
    candidate.is_file().then(|| candidate.into_os_string())
}

fn find_in_path(name: &str) -> Option<OsString> {
    let exe_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .map(|dir| dir.join(&exe_name))
        .find(|path| path.is_file())
        .map(|path| path.into_os_string())
}

fn find_via_rustc() -> Option<OsString> {
    let output = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let exe_name = if cfg!(windows) {
        "rust-lld.exe"
    } else {
        "rust-lld"
    };
    let host = rustc_host()?;
    let candidate = Path::new(&sysroot)
        .join("lib")
        .join("rustlib")
        .join(host)
        .join("bin")
        .join(exe_name);
    candidate.is_file().then(|| candidate.into_os_string())
}

fn rustc_host() -> Option<String> {
    let output = Command::new("rustc").args(["-vV"]).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(|host| host.trim().to_string())
}

/// 解析 Windows 系统链接器 `link.exe`，避开 PATH 上同名的 GNU coreutils `link`。
///
/// 优先用 `VCToolsInstallDir`（MSVC 开发环境激活时由 vcvars 设置）定位
/// `<dir>\bin\Host<x>\x64\link.exe`；否则扫描 PATH，只接受 MSVC 工具链路径。
/// 都找不到时返回 `None`，调用方保持原名交给 `Command` 报错。
#[cfg(windows)]
fn resolve_msvc_link() -> Option<OsString> {
    find_msvc_link_in_vc_tools().or_else(find_msvc_link_in_path)
}

/// 从 `VCToolsInstallDir` 定位 MSVC 链接器（目标固定为 x64，与
/// [`WindowsPlatform`](dolphin_platform::platform::WindowsPlatform) 一致）。
#[cfg(windows)]
fn find_msvc_link_in_vc_tools() -> Option<OsString> {
    let tools = env::var_os("VCToolsInstallDir")?;
    let mut hosts: Vec<PathBuf> = fs::read_dir(Path::new(&tools).join("bin"))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    // `Hostx64` 优先于 `Hostx86`（32 位宿主工具集），字典序恰好满足。
    hosts.sort();
    hosts
        .into_iter()
        .map(|host| host.join("x64").join("link.exe"))
        .find(|candidate| candidate.is_file())
        .map(PathBuf::into_os_string)
}

#[cfg(windows)]
fn find_msvc_link_in_path() -> Option<OsString> {
    env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .map(|dir| dir.join("link.exe"))
        .find(|path| path.is_file() && is_msvc_link_path(path))
        .map(PathBuf::into_os_string)
}

/// MSVC 工具链路径特征（如 `...\VC\Tools\MSVC\<版本>\bin\Hostx64\x64`），
/// 用于排除 Git for Windows / MSYS2 的 coreutils `link.exe`。
#[cfg(windows)]
fn is_msvc_link_path(path: &Path) -> bool {
    path.to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
        .contains("\\vc\\")
}

/// 运行时目标文件与用户目标文件放在同一目录、同一基名，扩展名取平台规则。
fn runtime_object_path(object: &Path, object_suffix: &str) -> PathBuf {
    let stem = object.file_stem().unwrap_or_default();
    let mut name = stem.to_os_string();
    name.push(".runtime.");
    name.push(object_suffix);
    object.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_lld_tools_are_returned_unchanged() {
        // Unix 系统链接器 `cc` 原样返回，不做路径解析。
        assert_eq!(resolve_tool(&OsString::from("cc")), OsString::from("cc"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_link_resolves_to_msvc_toolchain() {
        // CI 的 `cargo test` 步骤已激活 MSVC 环境（`VCToolsInstallDir`）；此时
        // `link` 必须解析到 MSVC 的 `link.exe`，而不是 Git Bash 的 coreutils `link`。
        if env::var_os("VCToolsInstallDir").is_none() {
            return;
        }
        let resolved = resolve_tool(&OsString::from("link"));
        let text = resolved
            .to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase();
        assert!(
            text.ends_with("\\link.exe"),
            "unexpected `link` resolution: `{text}`"
        );
        assert!(
            text.contains("\\vc\\"),
            "`link` should come from the MSVC toolchain: `{text}`"
        );
    }

    #[cfg(windows)]
    #[test]
    fn coreutils_link_is_not_mistaken_for_msvc() {
        assert!(!is_msvc_link_path(Path::new(
            r"C:\Program Files\Git\usr\bin\link.exe"
        )));
        assert!(is_msvc_link_path(Path::new(
            r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\link.exe"
        )));
    }

    #[test]
    fn rust_lld_resolves_to_an_existing_path() {
        // 开发/CI 环境有 Rust 工具链，rust-lld 应能解析到真实存在的路径。
        let resolved = resolve_tool(&OsString::from("rust-lld"));
        // 至少应解析到 rust-lld 或包含其名称的可执行路径，而非原样返回。
        let text = resolved.to_string_lossy();
        assert!(
            text.contains("rust-lld"),
            "rust-lld should resolve to a path containing its name, got `{text}`"
        );
    }
}
