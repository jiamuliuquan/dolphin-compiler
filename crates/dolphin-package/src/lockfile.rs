//! `dolphin.lock` 读写与一致性判断（M15-E，§8.3）。
//!
//! 锁文件记录整个传递闭包：每个包的坐标、来源（仓库基地址或 `path`）、远程摘要
//! 与排序后的依赖别名表，以及根项目的直接依赖别名。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cache::write_atomic;
use crate::package::{PackageGraph, PackageSource};
use crate::package_archive::COMPILER_VERSION;
use dolphin_source::diagnostic::Diagnostic;

pub const LOCKFILE_NAME: &str = "dolphin.lock";

/// 锁文件根结构。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lockfile {
    pub version: u32,
    #[serde(rename = "compiler-version")]
    pub compiler_version: String,
    #[serde(default)]
    pub package: Vec<LockedPackage>,
    #[serde(default, rename = "root-dependency")]
    pub root_dependency: Vec<RootDependency>,
}

/// 一个被锁定的包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedPackage {
    pub coordinate: String,
    /// 规范化仓库基地址，或 `path`。
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<LockedDependency>,
}

/// 依赖边：别名 + 坐标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedDependency {
    pub alias: String,
    pub coordinate: String,
}

/// 根项目的直接依赖（保留别名）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootDependency {
    pub alias: String,
    pub coordinate: String,
}

impl Lockfile {
    /// 从包图构造锁文件；`root` 是根清单所在目录。
    pub fn from_graph(graph: &PackageGraph, root: &Path) -> Lockfile {
        let mut package = Vec::new();
        for info in graph.packages.iter() {
            if info.id == graph.root {
                continue;
            }
            let (source, path, sha256) = match &info.source {
                PackageSource::Root => ("root".to_string(), None, None),
                PackageSource::Path(directory) => (
                    "path".to_string(),
                    Some(relative_path(root, directory)),
                    None,
                ),
                PackageSource::Remote { base, sha256, .. } => {
                    (base.clone(), None, Some(sha256.clone()))
                }
            };
            let mut dependencies: Vec<LockedDependency> = info
                .aliases
                .iter()
                .map(|(alias, target)| LockedDependency {
                    alias: alias.clone(),
                    coordinate: graph.get(*target).coordinate(),
                })
                .collect();
            dependencies.sort_by(|left, right| left.alias.cmp(&right.alias));
            package.push(LockedPackage {
                coordinate: info.coordinate(),
                source,
                path,
                sha256,
                dependencies,
            });
        }
        package.sort_by(|left, right| left.coordinate.cmp(&right.coordinate));

        let mut root_dependency: Vec<RootDependency> = graph
            .get(graph.root)
            .aliases
            .iter()
            .map(|(alias, target)| RootDependency {
                alias: alias.clone(),
                coordinate: graph.get(*target).coordinate(),
            })
            .collect();
        root_dependency.sort_by(|left, right| left.alias.cmp(&right.alias));

        Lockfile {
            version: 1,
            compiler_version: COMPILER_VERSION.to_string(),
            package,
            root_dependency,
        }
    }

    /// 锁文件是否与当前清单图、仓库映射和编译器版本一致。
    pub fn matches(&self, graph: &PackageGraph, root: &Path) -> bool {
        *self == Lockfile::from_graph(graph, root)
    }

    /// 坐标 -> 锁定条目，供远程解析使用。
    pub fn locked_packages(&self) -> BTreeMap<String, LockedPackage> {
        self.package
            .iter()
            .map(|package| (package.coordinate.clone(), package.clone()))
            .collect()
    }

    /// 根依赖别名 -> 坐标。
    pub fn root_aliases(&self) -> BTreeMap<String, String> {
        self.root_dependency
            .iter()
            .map(|dependency| (dependency.alias.clone(), dependency.coordinate.clone()))
            .collect()
    }
}

/// 读取根目录下的 `dolphin.lock`。
pub fn load(root: &Path) -> Result<Option<Lockfile>, Diagnostic> {
    let path = root.join(LOCKFILE_NAME);
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| {
        Diagnostic::plain(format!("could not read `{}`: {error}", path.display()))
    })?;
    let lock: Lockfile = toml::from_str(&text)
        .map_err(|error| Diagnostic::plain(format!("invalid `{}`: {error}", path.display())))?;
    Ok(Some(lock))
}

/// 原子写入 `dolphin.lock`。
pub fn write(root: &Path, lock: &Lockfile) -> Result<(), Diagnostic> {
    let path = root.join(LOCKFILE_NAME);
    let text = toml::to_string(lock)
        .map_err(|error| Diagnostic::plain(format!("could not serialize lockfile: {error}")))?;
    write_atomic(&path, text.as_bytes())
}

/// 计算 `target` 相对 `base` 的路径（`/` 分隔，供锁文件跨平台使用）。
fn relative_path(base: &Path, target: &Path) -> String {
    let base = normalize(base);
    let target = normalize(target);
    let mut base_components = base.components().peekable();
    let mut target_components = target.components().peekable();
    while base_components.peek().is_some() && base_components.peek() == target_components.peek() {
        base_components.next();
        target_components.next();
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in base_components {
        parts.push("..".to_string());
    }
    for component in target_components {
        parts.push(component.as_os_str().to_string_lossy().into_owned());
    }
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

fn normalize(path: &Path) -> PathBuf {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            other => cleaned.push(other.as_os_str()),
        }
    }
    cleaned
}
