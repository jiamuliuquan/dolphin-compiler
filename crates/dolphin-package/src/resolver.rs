//! 依赖解析：把根清单展开为 `PackageGraph`（M15-C，M15-E 扩展远程）。
//!
//! 规则（M15 规格 §8.2）：
//! - 包以完整坐标为节点、依赖为边，精确版本只做图遍历；
//! - 一个 `(group, name)` 只允许一个精确版本与一个来源，冲突时显示两条依赖链；
//!   Path/Root 与 Remote 永不视为同一来源，同一 Remote 的不同仓库也视为冲突，
//!   任一加载顺序都在解析时拒绝，不依赖遍历顺序；
//! - 检测包依赖环并显示环路径；
//! - 依赖必须声明 `[lib]`；
//! - 返回确定性顺序的图，供构建层消费；网络逻辑不在 lower/codegen 内。

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::manifest::{Dependency, DependencyKind, Manifest, parse_coordinate};
use crate::package::{PackageGraph, PackageId, PackageInfo, PackageSource};
use dolphin_source::diagnostic::Diagnostic;

/// 解析选项（`--locked` / `--offline`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResolveOptions {
    pub offline: bool,
    pub locked: bool,
}

/// 远程包获取结果。
pub struct AcquiredPackage {
    /// 解包后的包根目录。
    pub root: PathBuf,
    /// 归档 SHA-256。
    pub sha256: String,
    /// 规范化仓库基地址。
    pub base: String,
}

/// 远程包获取：由 `registry` 实现；测试可注入假实现。
pub trait RemoteSource {
    /// 下载、校验并解包给定坐标。
    fn acquire(
        &mut self,
        coordinate: &str,
        repository: &str,
    ) -> Result<AcquiredPackage, Diagnostic>;
}

/// 解析根清单为包图（路径依赖 + 注入的远程源）。
pub fn resolve(
    manifest: &Manifest,
    options: ResolveOptions,
    remote: &mut dyn RemoteSource,
) -> Result<PackageGraph, Diagnostic> {
    // 选项在构造远程源时已生效；解析器本身只做图遍历。
    let _ = options;
    let mut resolver = Resolver {
        packages: Vec::new(),
        state: HashMap::new(),
        by_name: BTreeMap::new(),
        by_path: BTreeMap::new(),
        chains: Vec::new(),
        remote,
    };
    resolver.install(manifest.clone(), PackageSource::Root, Vec::new())?;
    resolver.register_root()?;
    resolver.load(PackageId::ROOT)?;
    resolver.finish()
}

/// 只解析路径依赖（远程依赖报错）；供不涉及仓库的测试与早期批次使用。
pub fn resolve_paths_only(manifest: &Manifest) -> Result<PackageGraph, Diagnostic> {
    resolve(manifest, ResolveOptions::default(), &mut NoRemote)
}

struct NoRemote;

impl RemoteSource for NoRemote {
    fn acquire(
        &mut self,
        coordinate: &str,
        _repository: &str,
    ) -> Result<AcquiredPackage, Diagnostic> {
        Err(Diagnostic::plain(format!(
            "remote dependency `{coordinate}` cannot be resolved: repositories are not configured for this build"
        )))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Loading,
    Done,
}

struct Resolver<'a> {
    packages: Vec<PackageInfo>,
    state: HashMap<PackageId, State>,
    by_name: BTreeMap<(String, String), PackageId>,
    by_path: BTreeMap<PathBuf, PackageId>,
    /// 每个包从根出发的坐标链，用于冲突/环诊断。
    chains: Vec<Vec<String>>,
    remote: &'a mut dyn RemoteSource,
}

/// 已选来源是否为同一仓库的 Remote；Path/Root 及不同仓库都返回 `false`。
fn same_remote_repository(source: &PackageSource, repository: &str) -> bool {
    matches!(
        source,
        PackageSource::Remote {
            repository: existing,
            ..
        } if existing == repository
    )
}

/// 来源描述：仓库 ID 或规范化路径，不含凭据。
fn describe_source(source: &PackageSource) -> String {
    match source {
        PackageSource::Root => "the root project".to_string(),
        PackageSource::Path(path) => format!("path `{}`", path.display()),
        PackageSource::Remote { repository, .. } => format!("repository `{repository}`"),
    }
}

impl Resolver<'_> {
    /// 登记一个包；`chain` 是导致该包的依赖链（不含自身）。
    fn install(
        &mut self,
        manifest: Manifest,
        source: PackageSource,
        chain: Vec<String>,
    ) -> Result<PackageId, Diagnostic> {
        let id = PackageId(self.packages.len() as u32);
        let mut chain = chain;
        chain.push(manifest.package.coordinate());
        let prefix = id.prefix();
        let root = manifest.root.clone();
        let source_root = manifest.package.source.clone();
        let package = PackageInfo {
            id,
            manifest,
            root,
            source_root,
            source,
            prefix,
            aliases: BTreeMap::new(),
        };
        self.packages.push(package);
        self.chains.push(chain);
        Ok(id)
    }

    fn chain_of(&self, id: PackageId) -> String {
        self.chains[id.0 as usize].join(" -> ")
    }

    /// 把根包登记进 `by_name`/`by_path`，使依赖图中的同坐标/同路径包能被识别为冲突。
    fn register_root(&mut self) -> Result<(), Diagnostic> {
        let manifest = &self.packages[PackageId::ROOT.0 as usize].manifest;
        let (group, name, _) = parse_coordinate(&manifest.package.coordinate())?;
        self.by_name.insert((group, name), PackageId::ROOT);
        if let Ok(canonical) = std::fs::canonicalize(&manifest.root) {
            self.by_path.insert(canonical, PackageId::ROOT);
        }
        Ok(())
    }

    fn load(&mut self, id: PackageId) -> Result<(), Diagnostic> {
        self.state.insert(id, State::Loading);
        let dependencies = self.packages[id.0 as usize].manifest.dependencies.clone();
        for dependency in &dependencies {
            let dep_id = self.acquire_dependency(id, dependency)?;
            self.packages[id.0 as usize]
                .aliases
                .insert(dependency.alias.clone(), dep_id);
        }
        self.state.insert(id, State::Done);
        Ok(())
    }

    fn acquire_dependency(
        &mut self,
        parent: PackageId,
        dependency: &Dependency,
    ) -> Result<PackageId, Diagnostic> {
        match &dependency.kind {
            DependencyKind::Path(path) => self.acquire_path(parent, dependency, path),
            DependencyKind::Coordinate {
                coordinate,
                repository,
            } => {
                let (group, name, version) = parse_coordinate(coordinate)?;
                if let Some(existing) = self.by_name.get(&(group.clone(), name.clone())).copied() {
                    let existing_info = &self.packages[existing.0 as usize];
                    let existing_coordinate = existing_info.coordinate();
                    if existing_coordinate != *coordinate {
                        return Err(self.conflict(parent, coordinate, &existing_coordinate));
                    }
                    // 规范要求同一 `(group, name)` 且坐标相同时只能有一个来源：
                    // Path/Root 与 Remote 永不视为同一来源，同一 Remote 的不同仓库
                    // 也视为冲突。两个解析顺序都必须拒绝，不能靠遍历顺序。
                    if !same_remote_repository(&existing_info.source, repository) {
                        let incoming =
                            format!("coordinate `{coordinate}` from repository `{repository}`");
                        return Err(self.source_conflict(parent, &incoming, existing));
                    }
                    if let State::Loading = self.state[&existing] {
                        return Err(self.cycle(parent, existing));
                    }
                    return Ok(existing);
                }
                let chain = self.chains[parent.0 as usize].clone();
                let acquired = self.remote.acquire(coordinate, repository)?;
                let manifest = crate::manifest::load(&acquired.root)?;
                let actual = manifest.package.coordinate();
                if actual != *coordinate {
                    return Err(Diagnostic::plain(format!(
                        "downloaded package at `{}` declares coordinate `{actual}`, expected `{coordinate}`",
                        acquired.root.display()
                    )));
                }
                if manifest.lib.is_none() {
                    return Err(Diagnostic::plain(format!(
                        "dependency `{coordinate}` must declare a `[lib]` target"
                    )));
                }
                let id = self.install(
                    manifest,
                    PackageSource::Remote {
                        repository: repository.clone(),
                        base: acquired.base,
                        sha256: acquired.sha256,
                    },
                    chain,
                )?;
                self.by_name.insert((group, name), id);
                let _ = version;
                self.load(id)?;
                Ok(id)
            }
        }
    }

    fn acquire_path(
        &mut self,
        parent: PackageId,
        dependency: &Dependency,
        path: &Path,
    ) -> Result<PackageId, Diagnostic> {
        let canonical = std::fs::canonicalize(path).map_err(|error| {
            Diagnostic::plain(format!(
                "path dependency `{}` of `{}` could not be resolved: {error}",
                dependency.alias,
                self.packages[parent.0 as usize].name()
            ))
        })?;
        if let Some(existing) = self.by_path.get(&canonical).copied() {
            if let State::Loading = self.state[&existing] {
                return Err(self.cycle(parent, existing));
            }
            return Ok(existing);
        }
        let manifest = crate::manifest::load(&canonical)?;
        let key = (
            manifest.package.group.clone(),
            manifest.package.name.clone(),
        );
        if let Some(existing) = self.by_name.get(&key).copied() {
            let incoming = format!("path `{}`", canonical.display());
            return Err(self.source_conflict(parent, &incoming, existing));
        }
        if manifest.lib.is_none() {
            return Err(Diagnostic::plain(format!(
                "dependency `{}` from `{}` must declare a `[lib]` target",
                manifest.package.coordinate(),
                self.chain_of(parent)
            )));
        }
        let chain = self.chains[parent.0 as usize].clone();
        let id = self.install(manifest, PackageSource::Path(canonical.clone()), chain)?;
        self.by_name.insert(key, id);
        self.by_path.insert(canonical, id);
        self.load(id)?;
        Ok(id)
    }

    fn conflict(&self, parent: PackageId, wanted: &str, existing: &str) -> Diagnostic {
        Diagnostic::plain(format!(
            "dependency conflict for `{wanted}`:\n  {wanted}\n    required via {}\n  {existing}\n    already selected via {}\nA build allows exactly one version and one source per (group, name).",
            self.chain_of(parent),
            self.chain_of(self.by_name_of(existing)),
        ))
    }

    /// 同一 `(group, name)` 且版本相同时来源必须一致；诊断列出两条请求链与来源，
    /// 便于定位冲突。来源描述只含仓库 ID 或规范化路径，不含任何凭据。
    fn source_conflict(
        &self,
        parent: PackageId,
        incoming: &str,
        existing: PackageId,
    ) -> Diagnostic {
        let existing_info = &self.packages[existing.0 as usize];
        Diagnostic::plain(format!(
            "dependency source conflict for `{}`:\n  {incoming} requested via {}\n  {} already selected from {} via {}\nA build allows exactly one version and one source per (group, name).",
            existing_info.coordinate(),
            self.chain_of(parent),
            existing_info.coordinate(),
            describe_source(&existing_info.source),
            self.chain_of(existing),
        ))
    }

    fn by_name_of(&self, coordinate: &str) -> PackageId {
        let (group, name, _) = parse_coordinate(coordinate).expect("coordinate already validated");
        self.by_name[&(group, name)]
    }

    fn cycle(&self, parent: PackageId, existing: PackageId) -> Diagnostic {
        Diagnostic::plain(format!(
            "dependency cycle detected:\n  {} -> {}\n  {}",
            self.chain_of(parent),
            self.packages[existing.0 as usize].coordinate(),
            self.chain_of(existing),
        ))
    }

    /// 反向拓扑顺序：依赖者先于被依赖者，共享依赖排在全部使用者之后。
    fn finish(self) -> Result<PackageGraph, Diagnostic> {
        let root = PackageId::ROOT;
        let mut order = Vec::with_capacity(self.packages.len());
        // `users[p]` 是依赖 p 的包数量。
        let mut users = vec![0usize; self.packages.len()];
        for package in &self.packages {
            for dependency in package.aliases.values() {
                users[dependency.0 as usize] += 1;
            }
        }
        let mut ready: Vec<PackageId> = (0..self.packages.len())
            .map(|id| PackageId(id as u32))
            .filter(|id| users[id.0 as usize] == 0)
            .collect();
        while !ready.is_empty() {
            ready.sort_by(|left, right| {
                self.packages[left.0 as usize]
                    .coordinate()
                    .cmp(&self.packages[right.0 as usize].coordinate())
            });
            let next = ready.remove(0);
            order.push(next);
            let dependencies: Vec<PackageId> = self.packages[next.0 as usize]
                .aliases
                .values()
                .copied()
                .collect();
            for dependency in dependencies {
                users[dependency.0 as usize] -= 1;
                if users[dependency.0 as usize] == 0 {
                    ready.push(dependency);
                }
            }
        }
        if order.len() != self.packages.len() {
            return Err(Diagnostic::plain(
                "dependency graph contains a cycle that could not be ordered",
            ));
        }
        Ok(PackageGraph {
            root,
            packages: self.packages,
            order,
        })
    }
}
