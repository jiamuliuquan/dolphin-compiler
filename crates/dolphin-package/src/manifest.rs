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

use dolphin_source::diagnostic::Diagnostic;

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

/// 库目标：根模块入口文件（M15-C）。
///
/// 一个包最多一个 `[lib]`。它指向 `[package].source` 的直接子文件，标记根模块
/// 入口；其余根目录 `.do` 仍按既有规则合并进根模块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibTarget {
    pub path: PathBuf,
}

/// 依赖来源（M15-C）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyKind {
    /// 远程坐标 `group:name:version`，来自某个仓库 ID。
    Coordinate {
        coordinate: String,
        repository: String,
    },
    /// 本地路径依赖；路径相对清单所在目录解析为绝对路径。
    Path(PathBuf),
}

/// 一条 `[dependencies]` 条目：别名 + 来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub alias: String,
    pub kind: DependencyKind,
}

impl DependencyKind {
    /// 坐标字符串（路径依赖返回 `None`）。
    pub fn coordinate(&self) -> Option<&str> {
        match self {
            DependencyKind::Coordinate { coordinate, .. } => Some(coordinate),
            DependencyKind::Path(_) => None,
        }
    }

    pub fn repository(&self) -> Option<&str> {
        match self {
            DependencyKind::Coordinate { repository, .. } => Some(repository),
            DependencyKind::Path(_) => None,
        }
    }
}

/// 构建选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildConfig {
    pub optimization: String,
    pub output: PathBuf,
}

/// 某个目标三元组的原生链接输入（M14-E）。
///
/// 路径相对清单所在目录解析为绝对路径；同一 C 实现只在 `objects` 或
/// `static_libs` 中选一种。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeSpec {
    pub objects: Vec<PathBuf>,
    pub static_libs: Vec<PathBuf>,
    pub shared_libs: Vec<PathBuf>,
    pub runtime_files: Vec<PathBuf>,
}

/// 解析并校验后的项目清单。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub package: Package,
    /// 库目标；与 `bins` 至少声明一种。
    pub lib: Option<LibTarget>,
    pub bins: Vec<BinTarget>,
    pub build: BuildConfig,
    /// 按目标三元组声明的原生链接输入。
    pub native: Vec<(String, NativeSpec)>,
    /// `[dependencies]` 条目，按别名排序。
    pub dependencies: Vec<Dependency>,
    /// `[repositories]`：仓库 ID（小写）到规范化基地址。
    pub repositories: std::collections::BTreeMap<String, String>,
    /// 清单文件所在目录（项目根）。
    pub root: PathBuf,
    /// 清单文件路径（诊断用）。
    pub path: PathBuf,
}

impl Manifest {
    /// 返回指定目标三元组的原生链接输入。
    pub fn native_for(&self, triple: &str) -> Option<&NativeSpec> {
        self.native
            .iter()
            .find(|(key, _)| key == triple)
            .map(|(_, spec)| spec)
    }

    pub fn coordinate(&self) -> String {
        self.package.coordinate()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    #[serde(default)]
    package: Option<RawPackage>,
    #[serde(default)]
    lib: Option<RawLib>,
    #[serde(default)]
    bin: Vec<RawBin>,
    #[serde(default)]
    build: Option<RawBuild>,
    #[serde(default)]
    native: std::collections::BTreeMap<String, RawNative>,
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, toml::Value>,
    #[serde(default)]
    repositories: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLib {
    path: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawNative {
    #[serde(default)]
    objects: Vec<String>,
    #[serde(default)]
    static_libs: Vec<String>,
    #[serde(default)]
    shared_libs: Vec<String>,
    #[serde(default)]
    runtime_files: Vec<String>,
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

    if raw.lib.is_none() && raw.bin.is_empty() {
        return Err(Diagnostic::plain(format!(
            "`{}` must declare a `[lib]` target or at least one `[[bin]]` target",
            path.display()
        )));
    }

    let lib = match &raw.lib {
        Some(raw_lib) => {
            let lib_path = root.join(&raw_lib.path);
            // `[lib].path` 必须是 `[package].source` 的直接子文件。
            let parent = lib_path.parent().unwrap_or_else(|| Path::new(""));
            if parent != source {
                return Err(Diagnostic::plain(format!(
                    "`{}.lib.path` `{}` must be a direct child of the source directory `{}`",
                    path.display(),
                    lib_path.display(),
                    source.display()
                )));
            }
            Some(LibTarget { path: lib_path })
        }
        None => None,
    };

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

    let mut native = Vec::with_capacity(raw.native.len());
    for (triple, spec) in raw.native {
        if triple.is_empty() {
            return Err(Diagnostic::plain(format!(
                "`{}` has a `[native]` entry with an empty target triple",
                path.display()
            )));
        }
        let resolve = |paths: Vec<String>| -> Vec<PathBuf> {
            paths.into_iter().map(|entry| root.join(entry)).collect()
        };
        native.push((
            triple,
            NativeSpec {
                objects: resolve(spec.objects),
                static_libs: resolve(spec.static_libs),
                shared_libs: resolve(spec.shared_libs),
                runtime_files: resolve(spec.runtime_files),
            },
        ));
    }

    let dependencies = parse_dependencies(path, &root, raw.dependencies)?;
    let repositories = parse_repositories(path, raw.repositories)?;

    Ok(Manifest {
        package: Package {
            group,
            name,
            version,
            source,
        },
        lib,
        bins,
        build: BuildConfig {
            optimization,
            output,
        },
        native,
        dependencies,
        repositories,
        root: root.to_path_buf(),
        path: path.to_path_buf(),
    })
}

/// 解析 `[dependencies]`：字符串简写或 `{ coordinate, repository }` / `{ path }`。
fn parse_dependencies(
    path: &Path,
    root: &Path,
    raw: std::collections::BTreeMap<String, toml::Value>,
) -> Result<Vec<Dependency>, Diagnostic> {
    let mut dependencies = Vec::with_capacity(raw.len());
    for (alias, value) in raw {
        if !is_valid_alias(&alias) {
            return Err(Diagnostic::plain(format!(
                "invalid dependency alias `{alias}` in `{}`: expected an identifier other than `std`",
                path.display()
            )));
        }
        let kind = match value {
            toml::Value::String(coordinate) => {
                validate_coordinate(&coordinate)?;
                DependencyKind::Coordinate {
                    coordinate,
                    repository: "default".to_string(),
                }
            }
            toml::Value::Table(table) => {
                let coordinate = table.get("coordinate").and_then(toml::Value::as_str);
                let dependency_path = table.get("path").and_then(toml::Value::as_str);
                let repository = table.get("repository").and_then(toml::Value::as_str);
                for key in table.keys() {
                    if !matches!(key.as_str(), "coordinate" | "repository" | "path") {
                        return Err(Diagnostic::plain(format!(
                            "unknown key `{key}` in dependency `{alias}` of `{}`",
                            path.display()
                        )));
                    }
                }
                match (coordinate, dependency_path) {
                    (Some(_), Some(_)) => {
                        return Err(Diagnostic::plain(format!(
                            "dependency `{alias}` in `{}` cannot set both `coordinate` and `path`",
                            path.display()
                        )));
                    }
                    (Some(coordinate), None) => {
                        validate_coordinate(coordinate)?;
                        DependencyKind::Coordinate {
                            coordinate: coordinate.to_string(),
                            repository: repository.unwrap_or("default").to_string(),
                        }
                    }
                    (None, Some(dependency_path)) => {
                        if repository.is_some() {
                            return Err(Diagnostic::plain(format!(
                                "path dependency `{alias}` in `{}` cannot set `repository`",
                                path.display()
                            )));
                        }
                        DependencyKind::Path(root.join(dependency_path))
                    }
                    (None, None) => {
                        return Err(Diagnostic::plain(format!(
                            "dependency `{alias}` in `{}` must set `coordinate` or `path`",
                            path.display()
                        )));
                    }
                }
            }
            other => {
                return Err(Diagnostic::plain(format!(
                    "dependency `{alias}` in `{}` must be a coordinate string or a table, found {}",
                    path.display(),
                    other.type_str()
                )));
            }
        };
        dependencies.push(Dependency { alias, kind });
    }
    dependencies.sort_by(|left, right| left.alias.cmp(&right.alias));
    Ok(dependencies)
}

/// 解析并规范 `[repositories]`：ID 小写、基地址去掉尾部 `/`。
fn parse_repositories(
    path: &Path,
    raw: std::collections::BTreeMap<String, String>,
) -> Result<std::collections::BTreeMap<String, String>, Diagnostic> {
    let mut repositories = std::collections::BTreeMap::new();
    for (id, url) in raw {
        if !is_valid_repository_id(&id) {
            return Err(Diagnostic::plain(format!(
                "invalid repository id `{id}` in `{}`: expected ASCII letters, digits, or underscores starting with a letter",
                path.display()
            )));
        }
        let id = id.to_ascii_lowercase();
        let base = validate_repository_url(&url)?;
        if repositories.insert(id.clone(), base).is_some() {
            return Err(Diagnostic::plain(format!(
                "duplicate repository id `{id}` in `{}` (ids are case-insensitive)",
                path.display()
            )));
        }
    }
    Ok(repositories)
}

/// 解析 `group:name:version` 并校验每段。
pub fn parse_coordinate(coordinate: &str) -> Result<(String, String, String), Diagnostic> {
    let mut parts = coordinate.split(':');
    let (Some(group), Some(name), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(Diagnostic::plain(format!(
            "invalid coordinate `{coordinate}`: expected `group:name:version`"
        )));
    };
    for segment in group.split('.') {
        if segment.is_empty() || !segment.chars().all(is_identifier_char) {
            return Err(Diagnostic::plain(format!(
                "invalid coordinate `{coordinate}`: expected dot-separated identifiers in the group"
            )));
        }
    }
    if name.is_empty()
        || !name.chars().next().is_some_and(is_identifier_start)
        || !name.chars().all(is_identifier_char)
    {
        return Err(Diagnostic::plain(format!(
            "invalid coordinate `{coordinate}`: expected a non-empty identifier as the package name"
        )));
    }
    if !is_semver(version) || version.contains('+') {
        return Err(Diagnostic::plain(format!(
            "invalid coordinate `{coordinate}`: expected an exact semantic version without build metadata"
        )));
    }
    Ok((group.to_string(), name.to_string(), version.to_string()))
}

fn validate_coordinate(coordinate: &str) -> Result<(), Diagnostic> {
    parse_coordinate(coordinate).map(|_| ())
}

/// 依赖别名是合法标识符，且不能占用保留的 `std`。
pub fn is_valid_alias(alias: &str) -> bool {
    alias != "std"
        && !alias.is_empty()
        && alias.chars().next().is_some_and(is_identifier_start)
        && alias.chars().all(is_identifier_char)
}

fn is_valid_repository_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic())
        && id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// 校验并规范化仓库基地址：`http(s)://` 或 `file://`，不含凭据/query/fragment。
pub fn validate_repository_url(url: &str) -> Result<String, Diagnostic> {
    let base = url.trim_end_matches('/');
    if base.is_empty() {
        return Err(Diagnostic::plain("repository URL must not be empty"));
    }
    let (scheme, rest) = base.split_once("://").ok_or_else(|| {
        Diagnostic::plain(format!(
            "invalid repository URL `{url}`: expected an `http://`, `https://`, or `file://` base"
        ))
    })?;
    if !matches!(scheme, "http" | "https" | "file") {
        return Err(Diagnostic::plain(format!(
            "invalid repository URL `{url}`: unsupported scheme `{scheme}`"
        )));
    }
    if rest.is_empty() {
        return Err(Diagnostic::plain(format!(
            "invalid repository URL `{url}`: missing host or path"
        )));
    }
    if rest.contains('?') || rest.contains('#') {
        return Err(Diagnostic::plain(format!(
            "invalid repository URL `{url}`: must not contain a query or fragment"
        )));
    }
    if let Some(authority) = rest.split('/').next()
        && authority.contains('@')
    {
        return Err(Diagnostic::plain(format!(
            "invalid repository URL `{url}`: must not embed credentials"
        )));
    }
    Ok(base.to_string())
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

/// 语义化版本校验：`major.minor.patch`（数字），可选 `-预发布` 与 `+构建` 元数据。
///
/// 预发布/构建段只允许 ASCII 字母数字与 `-`，并以 `.` 分隔且不含空标识符。
/// 这样可确保版本字符串不可能包含路径分隔符、`..`、`:` 等，避免它被拼进
/// 仓库 URL 或缓存路径时产生路径穿越。
fn is_semver(value: &str) -> bool {
    let (without_build, build) = match value.split_once('+') {
        Some((left, right)) => (left, Some(right)),
        None => (value, None),
    };
    if build.is_some_and(|metadata| metadata.contains('+')) {
        return false;
    }
    let (core, pre_release) = match without_build.split_once('-') {
        Some((left, right)) => (left, Some(right)),
        None => (without_build, None),
    };
    if !is_semver_core(core) {
        return false;
    }
    if pre_release.is_some_and(|pre| !is_semver_identifiers(pre)) {
        return false;
    }
    if build.is_some_and(|metadata| !is_semver_identifiers(metadata)) {
        return false;
    }
    true
}

/// `major.minor.patch`：恰好三段且全为非空数字。
fn is_semver_core(core: &str) -> bool {
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// 点分隔的标识符列表；标识符非空且仅含 `[0-9A-Za-z-]`。
fn is_semver_identifiers(value: &str) -> bool {
    value.split('.').all(|identifier| {
        !identifier.is_empty()
            && identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
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
    fn rejects_version_with_path_separators() {
        for version in [
            "1.0.0-/../../evil",
            "1.0.0-a/../b",
            "1.0.0-..",
            "1.0.0-",
            "1.0.0+../x",
            "1.0.0-a+b+c",
        ] {
            assert!(
                parse_coordinate(&format!("g:n:{version}")).is_err(),
                "version `{version}` must be rejected"
            );
        }
        assert!(parse_coordinate("g:n:1.0.0-alpha.1").is_ok());
        assert!(parse_coordinate("g:n:0.1.0").is_ok());
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

    #[test]
    fn parses_native_inputs_for_target() {
        let (manifest, root) = parse_ok(
            r#"
            [package]
            group = "me.foxlab"
            name = "ffiapp"
            version = "0.1.0"

            [[bin]]
            name = "ffiapp"
            path = "src/main.do"

            [native.x86_64-unknown-linux-gnu]
            objects = ["native/demo.o"]
            static-libs = ["native/libdemo.a"]
            shared-libs = ["native/libshared.so"]
            runtime-files = ["native/libshared.so"]
            "#,
        );
        let spec = manifest
            .native_for("x86_64-unknown-linux-gnu")
            .expect("native spec should be present");
        assert_eq!(spec.objects, vec![root.join("native/demo.o")]);
        assert_eq!(spec.static_libs, vec![root.join("native/libdemo.a")]);
        assert_eq!(spec.shared_libs, vec![root.join("native/libshared.so")]);
        assert_eq!(spec.runtime_files, vec![root.join("native/libshared.so")]);
        assert!(manifest.native_for("aarch64-apple-darwin").is_none());
    }

    #[test]
    fn rejects_unknown_native_field() {
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

            [native.x86_64-unknown-linux-gnu]
            bogus = ["x"]
            "#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("dolphin.toml"));
    }

    #[test]
    fn parses_lib_only_manifest() {
        let (manifest, root) = parse_ok(
            r#"
            [package]
            group = "org.example"
            name = "mathlib"
            version = "1.0.0"

            [lib]
            path = "src/lib.do"
            "#,
        );
        assert!(manifest.bins.is_empty());
        assert_eq!(
            manifest.lib.as_ref().map(|lib| lib.path.clone()),
            Some(root.join("src").join("lib.do"))
        );
    }

    #[test]
    fn rejects_lib_outside_source_directory() {
        let error = parse(
            Path::new("/proj/dolphin.toml"),
            r#"
            [package]
            group = "g"
            name = "n"
            version = "0.1.0"

            [lib]
            path = "other/lib.do"
            "#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("direct child"));
    }

    #[test]
    fn requires_at_least_one_target() {
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
        assert!(error.to_string().contains("[lib]"));
        assert!(error.to_string().contains("[[bin]]"));
    }

    #[test]
    fn parses_dependencies_and_repositories() {
        let (manifest, root) = parse_ok(
            r#"
            [package]
            group = "org.example"
            name = "app"
            version = "0.1.0"

            [[bin]]
            name = "app"
            path = "src/main.do"

            [repositories]
            default = "https://packages.example.org/dolphin/"

            [dependencies]
            math = "org.example:mathlib:1.0.0"
            codec = { coordinate = "org.example:codec:2.0.0", repository = "default" }
            local = { path = "../local-lib" }
            "#,
        );
        assert_eq!(
            manifest.repositories.get("default").map(String::as_str),
            Some("https://packages.example.org/dolphin")
        );
        let names: Vec<&str> = manifest
            .dependencies
            .iter()
            .map(|dependency| dependency.alias.as_str())
            .collect();
        assert_eq!(names, vec!["codec", "local", "math"]);
        let math = manifest
            .dependencies
            .iter()
            .find(|dependency| dependency.alias == "math")
            .expect("math dependency should parse");
        assert_eq!(math.kind.coordinate(), Some("org.example:mathlib:1.0.0"));
        assert_eq!(math.kind.repository(), Some("default"));
        let local = manifest
            .dependencies
            .iter()
            .find(|dependency| dependency.alias == "local")
            .expect("local dependency should parse");
        assert_eq!(local.kind, DependencyKind::Path(root.join("../local-lib")));
    }

    #[test]
    fn rejects_reserved_or_invalid_dependency_alias() {
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

            [dependencies]
            std = "g:other:1.0.0"
            "#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("alias"));
    }

    #[test]
    fn rejects_invalid_repository_url_and_id() {
        let bad_url = parse(
            Path::new("/proj/dolphin.toml"),
            r#"
            [package]
            group = "g"
            name = "n"
            version = "0.1.0"

            [[bin]]
            name = "n"
            path = "src/main.do"

            [repositories]
            default = "ftp://example.org/repo"
            "#,
        )
        .unwrap_err();
        assert!(bad_url.to_string().contains("scheme"));

        let bad_id = parse(
            Path::new("/proj/dolphin.toml"),
            r#"
            [package]
            group = "g"
            name = "n"
            version = "0.1.0"

            [[bin]]
            name = "n"
            path = "src/main.do"

            [repositories]
            "2bad" = "https://example.org/repo"
            "#,
        )
        .unwrap_err();
        assert!(bad_id.to_string().contains("repository id"));
    }

    #[test]
    fn dependency_table_rejects_both_coordinate_and_path() {
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

            [dependencies]
            both = { coordinate = "g:o:1.0.0", path = "../o" }
            "#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("both"));
    }
}
