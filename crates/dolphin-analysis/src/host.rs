//! 分析宿主、快照与 overlay（M20/H20-02，§6.3–§6.5）。
//!
//! `AnalysisHost` 拥有未保存文本 overlay 与 revision；`snapshot()` 返回不可变
//! `Arc<AnalysisSnapshot>`。构建快照时只读解析依赖、不下载、不写锁、不产出构建
//! 产物；overlay 文本通过 `SourceProvider` 进入收集式 loader。

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dolphin_hir::lower;
use dolphin_hir::modules::{self, DiskProvider, SourceProvider};
use dolphin_package::cache::Cache;
use dolphin_package::manifest;
use dolphin_package::package::{PackageGraph, PackageId};
use dolphin_package::resolver;
use dolphin_source::diagnostic::Diagnostic;
use dolphin_source::source::{SourceFile, SourceId, Span};

use crate::index::SymbolIndex;

/// 分析模式：项目（找到清单）或单文件（无清单）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisMode {
    Project,
    SingleFile,
}

/// 分析单元种类；顺序与 §6.4 一致：Lib 在前，Bin 按清单顺序。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitKind {
    Lib,
    Bin(String),
}

/// 一个编译单元的分析结果（诊断与可选符号索引）。
pub struct AnalysisUnit {
    pub kind: UnitKind,
    pub sources: Vec<SourceFile>,
    pub packages: Vec<PackageId>,
    pub diagnostics: Vec<Diagnostic>,
    pub partial: bool,
    pub index: Option<Arc<SymbolIndex>>,
}

impl AnalysisUnit {
    /// 该单元内某路径对应的 `SourceId`（按词法规范化路径比较）。
    pub fn source_id_for_path(&self, path: &Path) -> Option<SourceId> {
        let path = modules::normalize_path(path);
        self.sources
            .iter()
            .find(|source| modules::normalize_path(&source.path) == path)
            .map(|source| source.id)
    }

    pub fn source_for_path(&self, path: &Path) -> Option<&SourceFile> {
        let path = modules::normalize_path(path);
        self.sources
            .iter()
            .find(|source| modules::normalize_path(&source.path) == path)
    }
}

/// 不可变分析快照；所有数据拥有自身，不借用 `AnalysisHost`。
pub struct AnalysisSnapshot {
    pub revision: u64,
    pub root: PathBuf,
    pub mode: AnalysisMode,
    /// 只读依赖解析成功时的包图。
    pub graph: Option<Arc<PackageGraph>>,
    pub units: Vec<AnalysisUnit>,
    /// 文件级诊断：跨单元按 `(路径, span, code, message)` 去重、按 `(路径, start, code)` 排序。
    pub diagnostics: Vec<Diagnostic>,
    /// 无 primary label 的项目级诊断（清单/依赖）。
    pub project_diagnostics: Vec<Diagnostic>,
    pub partial: bool,
}

impl AnalysisSnapshot {
    /// 查询包含该文件的单元：Lib > Bin（按清单顺序），与 §6.4 规则 3 一致。
    pub fn unit_for_path(&self, path: &Path) -> Option<&AnalysisUnit> {
        let path = modules::normalize_path(path);
        self.units.iter().find(|unit| {
            unit.sources
                .iter()
                .any(|source| modules::normalize_path(&source.path) == path)
        })
    }
}

struct Overlay {
    version: u64,
    text: String,
}

/// 分析宿主：项目根、overlay、revision 与缓存快照。
pub struct AnalysisHost {
    root: PathBuf,
    overlays: BTreeMap<PathBuf, Overlay>,
    revision: u64,
    cached: Option<Arc<AnalysisSnapshot>>,
    cache: Cache,
}

impl AnalysisHost {
    pub fn new(root: PathBuf) -> Self {
        Self::with_cache(root, Cache::from_env())
    }

    /// 显式指定缓存根（测试隔离；生产使用 `DOLPHIN_HOME`）。
    pub fn with_cache(root: PathBuf, cache: Cache) -> Self {
        AnalysisHost {
            root,
            overlays: BTreeMap::new(),
            revision: 0,
            cached: None,
            cache,
        }
    }

    /// 与 [`Self::with_cache`] 相同，但直接给缓存根目录。
    pub fn with_cache_root(root: PathBuf, cache_root: PathBuf) -> Self {
        Self::with_cache(root, Cache::at(cache_root))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 记录未保存文本；同一路径已有 >= version 的 overlay 时忽略并返回 `false`。
    pub fn set_overlay(&mut self, path: PathBuf, version: u64, text: String) -> bool {
        let path = self.normalize_key(&path);
        if let Some(existing) = self.overlays.get(&path)
            && existing.version >= version
        {
            return false;
        }
        self.overlays.insert(path, Overlay { version, text });
        self.revision += 1;
        self.cached = None;
        true
    }

    /// `didClose`：移除 overlay；成功移除时 revision 自增。
    pub fn remove_overlay(&mut self, path: &Path) -> bool {
        let path = self.normalize_key(path);
        if self.overlays.remove(&path).is_none() {
            return false;
        }
        self.revision += 1;
        self.cached = None;
        true
    }

    /// 该路径当前 overlay 的 version（未打开返回 `None`）。
    pub fn overlay_version(&self, path: &Path) -> Option<u64> {
        self.overlays
            .get(&self.normalize_key(path))
            .map(|overlay| overlay.version)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// 按当前 overlays 构建（或返回缓存的）快照；同一 revision 返回同一 `Arc`。
    pub fn snapshot(&mut self) -> Arc<AnalysisSnapshot> {
        if let Some(cached) = &self.cached {
            return cached.clone();
        }
        let snapshot = Arc::new(self.build());
        self.cached = Some(snapshot.clone());
        snapshot
    }

    fn normalize_key(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            modules::normalize_path(path)
        } else {
            modules::normalize_path(&self.root.join(path))
        }
    }

    fn build(&self) -> AnalysisSnapshot {
        let manifest_path = self.root.join("dolphin.toml");
        if !manifest_path.is_file() {
            return AnalysisSnapshot {
                revision: self.revision,
                root: self.root.clone(),
                mode: AnalysisMode::SingleFile,
                graph: None,
                units: Vec::new(),
                diagnostics: Vec::new(),
                project_diagnostics: Vec::new(),
                partial: false,
            };
        }
        let manifest = match manifest::load(&self.root) {
            Ok(manifest) => manifest,
            Err(error) => return self.project_failure(Diagnostic::error("E1002", error.message())),
        };
        let graph = match resolver::resolve_readonly(&manifest, &self.cache) {
            Ok(graph) => graph,
            Err(error) => {
                let diagnostic = if matches!(error.code(), "E1001" | "E1002") {
                    error
                } else {
                    Diagnostic::error("E1002", error.message())
                };
                return self.project_failure(diagnostic);
            }
        };
        let graph = Arc::new(graph);
        let provider = OverlayProvider {
            overlays: &self.overlays,
        };
        let mut units = Vec::new();
        let mut project_diagnostics = Vec::new();
        if let Some(lib) = &manifest.lib {
            if !lib.path.is_file() {
                project_diagnostics.push(Diagnostic::plain(format!(
                    "library entry `{}` does not exist",
                    lib.path.display()
                )));
            } else {
                let exclude: HashSet<PathBuf> =
                    manifest.bins.iter().map(|bin| bin.path.clone()).collect();
                units.push(self.build_unit(&graph, UnitKind::Lib, exclude, false, &provider));
            }
        }
        for target in &manifest.bins {
            if !target.path.is_file() {
                project_diagnostics.push(Diagnostic::plain(format!(
                    "binary target `{}` entry `{}` does not exist",
                    target.name,
                    target.path.display()
                )));
                continue;
            }
            let exclude: HashSet<PathBuf> = manifest
                .bins
                .iter()
                .filter(|other| other.name != target.name)
                .map(|other| other.path.clone())
                .collect();
            units.push(self.build_unit(
                &graph,
                UnitKind::Bin(target.name.clone()),
                exclude,
                true,
                &provider,
            ));
        }

        let mut merged: Vec<(PathBuf, Span, String, Diagnostic)> = Vec::new();
        let mut seen: HashSet<(PathBuf, usize, usize, String, String)> = HashSet::new();
        for unit in &units {
            for diagnostic in &unit.diagnostics {
                let Some(label) = diagnostic.labels().first() else {
                    project_diagnostics.push(diagnostic.clone());
                    continue;
                };
                let Some(source) = unit.sources.get(label.source.0 as usize) else {
                    project_diagnostics.push(diagnostic.clone());
                    continue;
                };
                let path = modules::normalize_path(&source.path);
                let key = (
                    path.clone(),
                    label.span.start,
                    label.span.end,
                    diagnostic.code().to_string(),
                    diagnostic.message().to_string(),
                );
                if seen.insert(key) {
                    merged.push((
                        path,
                        label.span,
                        diagnostic.code().to_string(),
                        diagnostic.clone(),
                    ));
                }
            }
        }
        merged.sort_by(|left, right| {
            (&left.0, left.1.start, &left.2).cmp(&(&right.0, right.1.start, &right.2))
        });
        let diagnostics = merged
            .into_iter()
            .map(|(_, _, _, diagnostic)| diagnostic)
            .collect();
        let partial = units.iter().any(|unit| unit.partial) || !project_diagnostics.is_empty();

        AnalysisSnapshot {
            revision: self.revision,
            root: self.root.clone(),
            mode: AnalysisMode::Project,
            graph: Some(graph),
            units,
            diagnostics,
            project_diagnostics,
            partial,
        }
    }

    fn project_failure(&self, diagnostic: Diagnostic) -> AnalysisSnapshot {
        AnalysisSnapshot {
            revision: self.revision,
            root: self.root.clone(),
            mode: AnalysisMode::Project,
            graph: None,
            units: Vec::new(),
            diagnostics: Vec::new(),
            project_diagnostics: vec![diagnostic],
            partial: true,
        }
    }

    fn build_unit(
        &self,
        graph: &PackageGraph,
        kind: UnitKind,
        exclude: HashSet<PathBuf>,
        require_main: bool,
        provider: &dyn SourceProvider,
    ) -> AnalysisUnit {
        let packages = modules::package_sources_for_graph(graph, exclude, Vec::new());
        let loaded = modules::load_packages_with_provider(&packages, provider);
        let mut diagnostics = loaded.diagnostics;
        let mut partial = !diagnostics.is_empty();
        let mut index = None;
        if !partial {
            match loaded.program.as_ref() {
                Some(program) => {
                    match lower::lower_sources_analysis_collecting(
                        &loaded.sources,
                        program,
                        &loaded.packages,
                        require_main,
                    ) {
                        Ok(lowered) => {
                            index = Some(Arc::new(SymbolIndex::new(
                                &lowered.analysis,
                                program,
                                &loaded.sources,
                                Some(graph),
                            )));
                        }
                        Err(errors) => {
                            diagnostics = errors;
                            partial = true;
                        }
                    }
                }
                None => {
                    diagnostics = vec![Diagnostic::plain(
                        "internal error: collecting loader produced no merged program",
                    )];
                    partial = true;
                }
            }
        }
        AnalysisUnit {
            kind,
            sources: loaded.sources,
            packages: loaded.packages,
            diagnostics,
            partial,
            index,
        }
    }
}

/// overlay 优先的源码提供器：磁盘发现 + overlay-only 新文件。
struct OverlayProvider<'a> {
    overlays: &'a BTreeMap<PathBuf, Overlay>,
}

impl SourceProvider for OverlayProvider<'_> {
    fn discover(&self, source_root: &Path) -> Result<Vec<PathBuf>, String> {
        let root = modules::normalize_path(source_root);
        let mut paths = DiskProvider.discover(source_root)?;
        let mut known: HashSet<PathBuf> = paths
            .iter()
            .map(|path| modules::normalize_path(path))
            .collect();
        for path in self.overlays.keys() {
            if !path.is_absolute() || !path.starts_with(&root) {
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "do") {
                continue;
            }
            if known.insert(path.clone()) {
                paths.push(path.clone());
            }
        }
        paths.sort();
        Ok(paths)
    }

    fn read(&self, path: &Path) -> Result<String, String> {
        if let Some(overlay) = self.overlays.get(&modules::normalize_path(path)) {
            return Ok(overlay.text.clone());
        }
        fs::read_to_string(path).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "dolphin-analysis-host-{tag}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(path, text).expect("write");
    }

    fn host(root: &Path) -> AnalysisHost {
        AnalysisHost::with_cache(root.to_path_buf(), Cache::at(root.join(".cache")))
    }

    #[test]
    fn stale_overlay_is_ignored_and_revision_advances() {
        let root = temp_dir("stale");
        let mut host = host(&root);
        assert_eq!(host.revision(), 0);
        assert!(host.set_overlay(PathBuf::from("src/a.do"), 2, "fn a() {}\n".into()));
        assert_eq!(host.revision(), 1);
        assert!(!host.set_overlay(PathBuf::from("src/a.do"), 2, "fn b() {}\n".into()));
        assert!(!host.set_overlay(PathBuf::from("src/a.do"), 1, "fn c() {}\n".into()));
        assert_eq!(host.revision(), 1);
        assert_eq!(host.overlay_version(Path::new("src/a.do")), Some(2));
        assert!(host.set_overlay(PathBuf::from("src/a.do"), 3, "fn d() {}\n".into()));
        assert_eq!(host.revision(), 2);
        assert!(host.remove_overlay(Path::new("src/a.do")));
        assert_eq!(host.revision(), 3);
        assert!(!host.remove_overlay(Path::new("src/a.do")));
        assert_eq!(host.revision(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn snapshot_is_cached_per_revision() {
        let root = temp_dir("cache");
        write(&root.join("src/main.do"), "fn main() { return 0; }\n");
        write(
            &root.join("dolphin.toml"),
            "[package]\ngroup = \"g\"\nname = \"p\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"p\"\npath = \"src/main.do\"\n",
        );
        let mut host = host(&root);
        let first = host.snapshot();
        let second = host.snapshot();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.mode, AnalysisMode::Project);
        assert!(!first.partial, "{:?}", first.project_diagnostics);
        assert_eq!(first.units.len(), 1);
        assert_eq!(first.units[0].kind, UnitKind::Bin("p".into()));
        assert!(first.units[0].index.is_some());
        host.set_overlay(
            root.join("src/main.do"),
            1,
            "fn main() { return 1; }\n".into(),
        );
        let third = host.snapshot();
        assert!(!Arc::ptr_eq(&first, &third));
        assert_eq!(third.revision, first.revision + 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn overlay_only_new_file_participates_in_project_analysis() {
        let root = temp_dir("overlay-new");
        write(
            &root.join("src/main.do"),
            "fn main() { return helper(); }\n",
        );
        write(
            &root.join("dolphin.toml"),
            "[package]\ngroup = \"g\"\nname = \"p\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"p\"\npath = \"src/main.do\"\n",
        );
        let mut host = host(&root);
        let baseline = host.snapshot();
        assert!(baseline.partial);
        assert!(
            baseline
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message().contains("unknown function `helper`")),
            "{:?}",
            baseline.diagnostics
        );
        // overlay-only 新文件位于源码根下：按相对路径推导模块并参与分析。
        let helper = root.join("src/helper.do");
        assert!(host.set_overlay(
            helper.clone(),
            1,
            "pub fn helper(): i32 { return 1; }\n".into()
        ));
        let snapshot = host.snapshot();
        assert!(!snapshot.partial, "{:?}", snapshot.diagnostics);
        assert!(
            snapshot.diagnostics.is_empty(),
            "{:?}",
            snapshot.diagnostics
        );
        let unit = snapshot.unit_for_path(&root.join("src/main.do")).unwrap();
        assert!(unit.source_id_for_path(&helper).is_some());
        // didClose：overlay-only 文件从发现集合移除，诊断恢复。
        assert!(host.remove_overlay(&helper));
        let closed = host.snapshot();
        assert!(closed.partial);
        assert!(
            closed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message().contains("unknown function `helper`")),
            "{:?}",
            closed.diagnostics
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_manifest_is_single_file_mode() {
        let root = temp_dir("single");
        let mut host = host(&root);
        let snapshot = host.snapshot();
        assert_eq!(snapshot.mode, AnalysisMode::SingleFile);
        assert!(snapshot.units.is_empty());
        assert!(snapshot.diagnostics.is_empty());
        assert!(!snapshot.partial);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unit_selection_prefers_lib_then_manifest_order() {
        let root = temp_dir("units");
        write(
            &root.join("src/lib.do"),
            "pub fn value(): i32 { return 1; }\n",
        );
        write(
            &root.join("src/shared.do"),
            "pub fn shared(): i32 { return 2; }\n",
        );
        write(&root.join("src/a.do"), "fn main() { return shared(); }\n");
        write(&root.join("src/b.do"), "fn main() { return shared(); }\n");
        write(
            &root.join("dolphin.toml"),
            "[package]\ngroup = \"g\"\nname = \"p\"\nversion = \"0.1.0\"\n\n[lib]\npath = \"src/lib.do\"\n\n[[bin]]\nname = \"b\"\npath = \"src/b.do\"\n\n[[bin]]\nname = \"a\"\npath = \"src/a.do\"\n",
        );
        let mut host = host(&root);
        let snapshot = host.snapshot();
        let kinds: Vec<&UnitKind> = snapshot.units.iter().map(|unit| &unit.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &UnitKind::Lib,
                &UnitKind::Bin("b".into()),
                &UnitKind::Bin("a".into())
            ]
        );
        let shared = root.join("src/shared.do");
        let unit = snapshot.unit_for_path(&shared).expect("shared file unit");
        assert_eq!(unit.kind, UnitKind::Lib);
        // 每个单元排除其他入口：lib 不含 main，bin 不含另一个 bin。
        let names: Vec<&str> = snapshot
            .units
            .iter()
            .map(|unit| {
                if unit.source_id_for_path(&root.join("src/a.do")).is_some() {
                    "a"
                } else if unit.source_id_for_path(&root.join("src/b.do")).is_some() {
                    "b"
                } else {
                    "lib"
                }
            })
            .collect();
        assert_eq!(names, vec!["lib", "b", "a"]);
        fs::remove_dir_all(root).unwrap();
    }
}
