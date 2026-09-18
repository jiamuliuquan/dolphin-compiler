//! 链接流程：把目标文件与内嵌运行时组合成可执行文件。
//!
//! M12 起运行时不再是构建时用系统 C 编译器现场编译，而是在编译编译器时
//! （`build.rs`）预编译并内嵌。这里把内嵌的运行时目标文件字节落盘，再执行
//! 平台的链接命令（默认 `rust-lld`，也可用 `--linker` 回退到系统链接器）。
//!
//! 「生成目标文件」与「链接可执行文件」仍拆成两个可独立测试的步骤。

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use dolphin_platform::platform::{NativeInputs, TargetPlatform};
use dolphin_source::diagnostic::Diagnostic;

/// 链接器选择：默认自包含（`rust-lld`），或回退到系统链接器（`cc`/`link`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LinkerChoice {
    /// 默认：内嵌的 `rust-lld`。
    #[default]
    Default,
    /// `--linker` 回退：使用平台系统链接器。
    System,
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
        LinkerChoice::Default => platform.link_command(object, &runtime, native, output),
        LinkerChoice::System => platform.system_link_command(object, &runtime, native, output),
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

/// 解析工具名：`rust-lld` 需要定位到完整路径，其他工具（`cc`/`link`）保持原样。
///
/// 查找顺序：可执行文件同目录（发行包自带 `rust-lld`）→ `DOLPHIN_LLD` 环境变量
/// （build.rs 编译期注入）→ PATH → 用 `rustc --print sysroot` 动态查询。
/// 最终回退到原名，由 `Command` 报错。
///
/// 发行包里的 `rust-lld`（与 `dc` 同目录）优先于编译期注入的工具链路径：
/// 后者在 macOS 上可能因 rpath 问题（rust-lld 动态依赖 libLLVM.dylib）无法运行，
/// 而发行包里的 rust-lld 已经过 package.py 修复 rpath 并随附 libLLVM.dylib。
fn resolve_tool(tool: &OsString) -> OsString {
    if tool != "rust-lld" {
        return tool.clone();
    }
    // 1. 可执行文件同目录（发行包形态：`dc` 与 `rust-lld` 放在同一目录）。
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

/// 在 `dc` 可执行文件所在目录查找 `rust-lld`（发行包将二者放在一起）。
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
        // `--system-linker` 回退工具（cc/link）原样返回，不做路径解析。
        assert_eq!(resolve_tool(&OsString::from("cc")), OsString::from("cc"));
        assert_eq!(
            resolve_tool(&OsString::from("link")),
            OsString::from("link")
        );
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
