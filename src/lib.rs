mod ast;
mod codegen;
mod diagnostic;
mod ir;
mod lexer;
mod linker;
mod lower;
mod manifest;
mod modules;
mod parser;
mod platform;
mod source;
mod token;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub use diagnostic::Diagnostic;
pub use linker::LinkerChoice;
pub use manifest::{BinTarget, BuildConfig, Manifest, Package};
pub use platform::{Abi, TargetPlatform};
use source::SourceFile;

/// 宿主目标平台（M10 起宿主即目标）。
pub fn host_platform() -> Result<Box<dyn TargetPlatform>, Diagnostic> {
    platform::host()
}

/// 读取并解析项目根目录下的 `dolphin.toml`。
pub fn load_manifest(root: &Path) -> Result<Manifest, Diagnostic> {
    manifest::load(root)
}

#[derive(Debug)]
pub struct BuildOptions {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
}

/// 构建时使用的链接器选择（M12）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuildSettings {
    pub linker: LinkerChoice,
}

impl BuildSettings {
    pub fn with_linker(linker: LinkerChoice) -> Self {
        Self { linker }
    }
}

#[derive(Debug)]
pub struct BuildArtifact {
    pub executable: PathBuf,
    pub object: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildProfile {
    Debug,
    Release,
}

pub fn build(options: BuildOptions) -> Result<BuildArtifact, Diagnostic> {
    build_with_profile(options, BuildProfile::Debug, BuildSettings::default())
}

pub fn check(input: &Path) -> Result<(), Diagnostic> {
    compile_frontend(input).map(|_| ())
}

/// 构建清单中声明的可执行目标。
///
/// `bin` 为 `None` 时构建全部目标；为 `Some(name)` 时只构建指定目标。
pub fn build_manifest(
    manifest: &Manifest,
    bin: Option<&str>,
    profile: BuildProfile,
    settings: BuildSettings,
) -> Result<Vec<BuildArtifact>, Diagnostic> {
    let targets = select_targets(manifest, bin)?;
    let profile = if manifest.build.optimization == "release" {
        BuildProfile::Release
    } else {
        profile
    };
    let mut artifacts = Vec::with_capacity(targets.len());
    for target in targets {
        artifacts.push(build_bin(manifest, target, profile, settings)?);
    }
    Ok(artifacts)
}

/// 检查清单中声明的全部可执行目标。
pub fn check_manifest(manifest: &Manifest) -> Result<(), Diagnostic> {
    let targets = select_targets(manifest, None)?;
    for target in targets {
        compile_bin(manifest, target)?;
    }
    Ok(())
}

fn select_targets<'a>(
    manifest: &'a Manifest,
    bin: Option<&str>,
) -> Result<Vec<&'a BinTarget>, Diagnostic> {
    if let Some(name) = bin {
        manifest
            .bins
            .iter()
            .find(|target| target.name == name)
            .map(|target| vec![target])
            .ok_or_else(|| {
                Diagnostic::plain(format!(
                    "unknown binary target `{name}` (available: {})",
                    manifest
                        .bins
                        .iter()
                        .map(|target| target.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })
    } else {
        Ok(manifest.bins.iter().collect())
    }
}

fn build_bin(
    manifest: &Manifest,
    target: &BinTarget,
    profile: BuildProfile,
    settings: BuildSettings,
) -> Result<BuildArtifact, Diagnostic> {
    let platform = platform::host()?;
    let base = manifest.build.output.join(&target.name);
    let output = with_executable_suffix(&base, platform.as_ref());
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            Diagnostic::plain(format!(
                "could not create output directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    let program = compile_bin(manifest, target)?;
    let object = object_path(&base, platform.object_suffix());
    codegen::emit_program_optimized(
        &program,
        &object,
        profile == BuildProfile::Release,
        platform.as_ref(),
    )?;
    linker::link(platform.as_ref(), &object, &output, settings.linker)?;
    Ok(BuildArtifact {
        executable: output,
        object,
    })
}

/// 编译单个可执行目标：从清单源码根加载模块，排除其他目标的入口文件。
fn compile_bin(manifest: &Manifest, target: &BinTarget) -> Result<ir::Program, Diagnostic> {
    if !target.path.is_file() {
        return Err(Diagnostic::plain(format!(
            "binary target `{}` entry `{}` does not exist",
            target.name,
            target.path.display()
        )));
    }
    let exclude: HashSet<PathBuf> = manifest
        .bins
        .iter()
        .filter(|other| other.name != target.name)
        .map(|other| other.path.clone())
        .collect();
    let loaded = modules::load_sources(&manifest.package.source, &exclude)?;
    lower::lower_sources(&loaded.sources, &loaded.program)
}

pub fn build_with_profile(
    options: BuildOptions,
    profile: BuildProfile,
    settings: BuildSettings,
) -> Result<BuildArtifact, Diagnostic> {
    let platform = platform::host()?;
    let default_output = default_output(&options.input)?;
    let base = options.output.unwrap_or(default_output);
    let output = with_executable_suffix(&base, platform.as_ref());
    let program = compile_frontend(&options.input)?;

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| {
            Diagnostic::plain(format!(
                "could not create output directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }

    let object = object_path(&base, platform.object_suffix());
    codegen::emit_program_optimized(
        &program,
        &object,
        profile == BuildProfile::Release,
        platform.as_ref(),
    )?;
    linker::link(platform.as_ref(), &object, &output, settings.linker)?;

    Ok(BuildArtifact {
        executable: output,
        object,
    })
}

/// 从 `start` 目录（或其任意祖先目录）查找 `dolphin.toml`。
///
/// 返回清单；未找到时返回 `None`，调用方回退到旧行为。
pub fn discover_manifest(start: &Path) -> Result<Option<Manifest>, Diagnostic> {
    let start = std::path::absolute(start).unwrap_or_else(|_| start.to_path_buf());
    let mut current = if start.is_dir() {
        start
    } else {
        start
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };
    loop {
        let candidate = current.join("dolphin.toml");
        if candidate.is_file() {
            return manifest::load(&current).map(Some);
        }
        if !current.pop() {
            return Ok(None);
        }
    }
}

fn compile_frontend(input: &Path) -> Result<ir::Program, Diagnostic> {
    if input.is_dir() {
        let loaded = modules::load_project(input)?;
        lower::lower_sources(&loaded.sources, &loaded.program)
    } else {
        let source_text = fs::read_to_string(input).map_err(|error| {
            Diagnostic::plain(format!("could not read `{}`: {error}", input.display()))
        })?;
        let source = SourceFile::new(input.to_path_buf(), source_text);
        let tokens = lexer::lex(&source)?;
        let program = parser::parse(&source, tokens)?;
        if let Some(package) = &program.package {
            return Err(Diagnostic::at(
                &source,
                package.span,
                "`pkg` requires compiling a project directory",
            ));
        }
        if let Some(import) = program.uses.first() {
            return Err(Diagnostic::at(
                &source,
                import.span,
                "`use` requires compiling a project directory",
            ));
        }
        lower::lower(&source, &program)
    }
}

fn default_output(input: &Path) -> Result<PathBuf, Diagnostic> {
    if input.is_dir() {
        let project_name = input
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("main");
        let output = input.join("target").join(project_name);
        Ok(output)
    } else if input.is_file() {
        let stem = input
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("main");
        let parent = input.parent().unwrap_or_else(|| Path::new("."));
        Ok(parent.join("target").join(stem))
    } else {
        Err(Diagnostic::plain(format!(
            "input `{}` does not exist",
            input.display()
        )))
    }
}

fn object_path(output: &Path, suffix: &str) -> PathBuf {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    output.with_file_name(name)
}

/// 按平台规则补全可执行文件后缀（Windows 为 `.exe`；已带后缀时保持原样）。
fn with_executable_suffix(base: &Path, platform: &dyn TargetPlatform) -> PathBuf {
    let suffix = platform.executable_suffix();
    if suffix.is_empty() {
        return base.to_path_buf();
    }
    let expected = format!(".{suffix}");
    let name = base.file_name().unwrap_or_default().to_os_string();
    if name.to_string_lossy().ends_with(&expected) {
        return base.to_path_buf();
    }
    let mut with_suffix = name;
    with_suffix.push(&expected);
    base.with_file_name(with_suffix)
}
