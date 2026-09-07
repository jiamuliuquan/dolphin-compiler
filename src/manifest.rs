//! `dolphin.toml` 项目清单：解析、校验与包坐标。
//!
//! M9 引入清单，让项目入口、产物和构建选项由稳定配置描述。
//! 首版字段（在实现 M9 时冻结）：
//!
//! ```toml
//! [package]
//! group = "me.foxlab"
//! name = "hello"
//! version = "0.1.0"
//! source = "src"
//!
//! [[bin]]
//! name = "hello"
//! path = "src/main.do"
//!
//! [build]
//! optimization = "debug"
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml::Spanned;

use crate::diagnostic::Diagnostic;

/// 包的完整坐标：`group:name:version`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub group: String,
    pub name: String,
    pub version: String,
    pub source: PathBuf,
}

impl Package {
    pub fn coordinate(&self) -> String {
        format!("{}:{}:{}", self.group, self.name, self.version)
    }
}

/// 一个可执行目标：入口文件 + 产物名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinTarget {
    pub name: String,
    pub path: PathBuf,
}

/// 构建选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildConfig {
    pub optimization: String,
    pub output: PathBuf,
}

/// 解析并校验后的项目清单。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub package: Package,
    pub bins: Vec<BinTarget>,
    pub build: BuildConfig,
    /// 清单文件所在目录（项目根）。
    pub root: PathBuf,
}

/// `[dependencies]` 预留边界：M9 仅解析保留，不下载或解析依赖。
///
/// 允许 `alias = "group:name:version"` 形式写入，但暂不生效。
#[derive(Debug, Default, Deserialize)]
#[serde(transparent)]
struct RawDependencies {
    #[allow(dead_code)]
    entries: std::collections::BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    #[serde(default)]
    package: Option<RawPackage>,
    #[serde(default)]
    bin: Vec<RawBin>,
    #[serde(default)]
    build: Option<RawBuild>,
    #[serde(default)]
    #[allow(dead_code)]
    dependencies: RawDependencies,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPackage {
    group: Spanned<String>,
    name: Spanned<String>,
    version: Spanned<String>,
    #[serde(default)]
    source: Option<Spanned<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBin {
    name: Spanned<String>,
    path: Spanned<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBuild {
    #[serde(default)]
    optimization: Option<Spanned<String>>,
    #[serde(default)]
    output: Option<Spanned<String>>,
}

/// 读取并解析项目根目录下的 `dolphin.toml`。
pub fn load(root: &Path) -> Result<Manifest, Diagnostic> {
    let root = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
    let path = root.join("dolphin.toml");
    let text = fs::read_to_string(&path).map_err(|error| {
        Diagnostic::plain(format!("could not read `{}`: {error}", path.display()))
    })?;
    parse(&path, &text)
}

/// 解析 `dolphin.toml` 文本。
pub fn parse(path: &Path, text: &str) -> Result<Manifest, Diagnostic> {
    let raw: RawManifest = toml::from_str(text).map_err(|error| toml_error(path, text, &error))?;

    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let root = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());

    let raw_package = raw.package.ok_or_else(|| {
        Diagnostic::plain(format!(
            "`{}` is missing a `[package]` table",
            path.display()
        ))
    })?;

    let group = validate_group(&raw_package.group)?;
    let name = validate_name(&raw_package.name)?;
    let version = validate_version(&raw_package.version)?;

    let source = raw_package
        .source
        .map(|source| source.into_inner())
        .unwrap_or_else(|| "src".to_string());
    let source = root.join(&source);

    if raw.bin.is_empty() {
        return Err(Diagnostic::plain(format!(
            "`{}` must declare at least one `[[bin]]` target",
            path.display()
        )));
    }

    let mut bins = Vec::with_capacity(raw.bin.len());
    let mut seen = std::collections::HashSet::new();
    for bin in &raw.bin {
        let bin_name = validate_name(&bin.name)?;
        if !seen.insert(bin_name.clone()) {
            return Err(Diagnostic::plain(format!(
                "duplicate binary target `{bin_name}` in `{}`",
                path.display()
            )));
        }
        let bin_path = root.join(bin.path.get_ref());
        bins.push(BinTarget {
            name: bin_name,
            path: bin_path,
        });
    }

    let optimization = raw
        .build
        .as_ref()
        .and_then(|build| build.optimization.as_ref())
        .map(|optimization| optimization.get_ref().clone())
        .unwrap_or_else(|| "debug".to_string());
    if optimization != "debug" && optimization != "release" {
        return Err(Diagnostic::plain(format!(
            "invalid `build.optimization` `{optimization}`: expected `debug` or `release`"
        )));
    }

    let output = raw
        .build
        .as_ref()
        .and_then(|build| build.output.as_ref())
        .map(|output| output.get_ref().clone())
        .unwrap_or_else(|| "target".to_string());
    let output = root.join(&output);

    Ok(Manifest {
        package: Package {
            group,
            name,
            version,
            source,
        },
        bins,
        build: BuildConfig {
            optimization,
            output,
        },
        root: root.to_path_buf(),
    })
}

/// 将 `toml` 解析错误转为带源码位置的诊断。
fn toml_error(path: &Path, text: &str, error: &toml::de::Error) -> Diagnostic {
    let message = error.message();
    if let Some(span) = error.span() {
        let line = text[..span.start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let column = text[..span.start]
            .rsplit_once('\n')
            .map(|(_, rest)| rest.chars().count() + 1)
            .unwrap_or(span.start + 1);
        Diagnostic::plain(format!(
            "invalid `dolphin.toml` at {}:{line}:{column}: {message}",
            path.display()
        ))
    } else {
        Diagnostic::plain(format!(
            "invalid `dolphin.toml` in `{}`: {message}",
            path.display()
        ))
    }
}

fn validate_group(value: &Spanned<String>) -> Result<String, Diagnostic> {
    let group = value.get_ref();
    if group.is_empty() {
        return Err(Diagnostic::plain("`package.group` must not be empty"));
    }
    for segment in group.split('.') {
        if segment.is_empty() || !segment.chars().all(is_identifier_char) {
            return Err(Diagnostic::plain(format!(
                "invalid `package.group` `{group}`: expected dot-separated identifiers"
            )));
        }
    }
    Ok(group.clone())
}

fn validate_name(value: &Spanned<String>) -> Result<String, Diagnostic> {
    let name = value.get_ref();
    if name.is_empty()
        || !name.chars().next().is_some_and(is_identifier_start)
        || !name.chars().all(is_identifier_char)
    {
        return Err(Diagnostic::plain(format!(
            "invalid name `{name}`: expected a non-empty identifier"
        )));
    }
    Ok(name.clone())
}

fn validate_version(value: &Spanned<String>) -> Result<String, Diagnostic> {
    let version = value.get_ref();
    if version.contains(':') {
        return Err(Diagnostic::plain(format!(
            "invalid `package.version` `{version}`: must not contain `:`"
        )));
    }
    if !is_semver(version) {
        return Err(Diagnostic::plain(format!(
            "invalid `package.version` `{version}`: expected semantic version like `0.1.0`"
        )));
    }
    Ok(version.clone())
}

fn is_identifier_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// 最小语义化版本校验：`major.minor.patch`，每段为非负整数，可带可选预发布/构建元数据。
fn is_semver(value: &str) -> bool {
    let core = value.split(['-', '+']).next().unwrap_or(value);
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts
        .iter()
        .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 解析并返回清单与解析过程使用的绝对项目根（跨平台路径规则一致）。
    fn parse_ok(text: &str) -> (Manifest, PathBuf) {
        let root = std::path::absolute(Path::new("/proj")).expect("root should be absolute");
        let manifest = parse(&root.join("dolphin.toml"), text).expect("manifest should parse");
        (manifest, root)
    }

    #[test]
    fn parses_minimal_manifest_with_defaults() {
        let (manifest, root) = parse_ok(
            r#"
            [package]
            group = "me.foxlab"
            name = "hello"
            version = "0.1.0"

            [[bin]]
            name = "hello"
            path = "src/main.do"
            "#,
        );
        assert_eq!(manifest.package.coordinate(), "me.foxlab:hello:0.1.0");
        assert_eq!(manifest.package.source, root.join("src"));
        assert_eq!(manifest.bins.len(), 1);
        assert_eq!(manifest.bins[0].name, "hello");
        assert_eq!(manifest.bins[0].path, root.join("src").join("main.do"));
        assert_eq!(manifest.build.optimization, "debug");
        assert_eq!(manifest.build.output, root.join("target"));
    }

    #[test]
    fn parses_explicit_source_build_and_multiple_bins() {
        let (manifest, root) = parse_ok(
            r#"
            [package]
            group = "org.example"
            name = "app"
            version = "1.2.3"
            source = "source"

            [[bin]]
            name = "cli"
            path = "source/cli.do"

            [[bin]]
            name = "server"
            path = "source/server.do"

            [build]
            optimization = "release"
            output = "dist"
            "#,
        );
        assert_eq!(manifest.package.coordinate(), "org.example:app:1.2.3");
        assert_eq!(manifest.bins.len(), 2);
        assert_eq!(manifest.build.optimization, "release");
        assert_eq!(manifest.build.output, root.join("dist"));
    }

    #[test]
    fn rejects_missing_package() {
        let error = parse(Path::new("/proj/dolphin.toml"), "").unwrap_err();
        assert!(error.to_string().contains("[package]"));
    }

    #[test]
    fn rejects_empty_bins() {
        let error = parse(
            Path::new("/proj/dolphin.toml"),
            r#"
            [package]
            group = "g"
            name = "n"
            version = "0.1.0"
            "#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("[[bin]]"));
    }

    #[test]
    fn rejects_invalid_group() {
        let error = parse(
            Path::new("/proj/dolphin.toml"),
            r#"
            [package]
            group = "bad..group"
            name = "n"
            version = "0.1.0"

            [[bin]]
            name = "n"
            path = "src/main.do"
            "#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("package.group"));
    }

    #[test]
    fn rejects_invalid_version() {
        let error = parse(
            Path::new("/proj/dolphin.toml"),
            r#"
            [package]
            group = "g"
            name = "n"
            version = "not-a-version"

            [[bin]]
            name = "n"
            path = "src/main.do"
            "#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("package.version"));
    }

    #[test]
    fn rejects_duplicate_bins() {
        let error = parse(
            Path::new("/proj/dolphin.toml"),
            r#"
            [package]
            group = "g"
            name = "n"
            version = "0.1.0"

            [[bin]]
            name = "n"
            path = "src/a.do"

            [[bin]]
            name = "n"
            path = "src/b.do"
            "#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("duplicate"));
    }

    #[test]
    fn rejects_unknown_field() {
        let error = parse(
            Path::new("/proj/dolphin.toml"),
            r#"
            [package]
            group = "g"
            name = "n"
            version = "0.1.0"
            bogus = true

            [[bin]]
            name = "n"
            path = "src/main.do"
            "#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("dolphin.toml"));
    }

    #[test]
    fn rejects_invalid_optimization() {
        let error = parse(
            Path::new("/proj/dolphin.toml"),
            r#"
            [package]
            group = "g"
            name = "n"
            version = "0.1.0"

            [[bin]]
            name = "n"
            path = "src/main.do"

            [build]
            optimization = "fast"
            "#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("optimization"));
    }
}
