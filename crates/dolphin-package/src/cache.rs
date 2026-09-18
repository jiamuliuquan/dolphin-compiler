//! 内容寻址包缓存（M15-E，§8.4）。
//!
//! ```text
//! <home>/cache/packages/sha256/<digest>/package.dlib
//! <home>/cache/packages/sha256/<digest>/unpacked/...
//! <home>/cache/index/<仓库规范 URL 的 SHA-256>/<group 目录>/<name>/<version>.toml
//! ```
//!
//! 缓存根优先 `DOLPHIN_HOME`，默认用户主目录 `.dolphin`。归档先写临时文件，
//! 校验后再原子改名；坐标索引记录 `coordinate/source/sha256`。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::package_archive;
use dolphin_source::diagnostic::Diagnostic;

/// 坐标索引条目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexEntry {
    pub coordinate: String,
    pub source: String,
    pub sha256: String,
}

/// 内容寻址缓存。
#[derive(Debug, Clone)]
pub struct Cache {
    home: PathBuf,
}

impl Cache {
    /// 按 `DOLPHIN_HOME` 或用户主目录 `.dolphin` 建立缓存。
    pub fn from_env() -> Cache {
        if let Some(home) = std::env::var_os("DOLPHIN_HOME")
            && !home.is_empty()
        {
            return Cache {
                home: PathBuf::from(home),
            };
        }
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".dolphin");
        Cache { home }
    }

    pub fn at(home: PathBuf) -> Cache {
        Cache { home }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn archive_path(&self, sha256: &str) -> PathBuf {
        self.home
            .join("cache")
            .join("packages")
            .join("sha256")
            .join(sha256)
            .join("package.dlib")
    }

    pub fn unpacked_dir(&self, sha256: &str) -> PathBuf {
        self.home
            .join("cache")
            .join("packages")
            .join("sha256")
            .join(sha256)
            .join("unpacked")
    }

    /// 仓库索引路径：`cache/index/<base 摘要>/<group 目录>/<name>/<version>.toml`。
    pub fn index_path(&self, base: &str, group: &str, name: &str, version: &str) -> PathBuf {
        let mut path = self
            .home
            .join("cache")
            .join("index")
            .join(package_archive::sha256_hex(base.as_bytes()));
        for segment in group.split('.') {
            path.push(segment);
        }
        path.push(name);
        path.push(format!("{version}.toml"));
        path
    }

    /// 读取坐标索引（已校验过的归档摘要）。
    pub fn read_index(
        &self,
        base: &str,
        group: &str,
        name: &str,
        version: &str,
    ) -> Option<IndexEntry> {
        let path = self.index_path(base, group, name, version);
        let text = fs::read_to_string(path).ok()?;
        toml::from_str(&text).ok()
    }

    /// 原子写入坐标索引。
    pub fn write_index(
        &self,
        base: &str,
        group: &str,
        name: &str,
        version: &str,
        entry: &IndexEntry,
    ) -> Result<(), Diagnostic> {
        let path = self.index_path(base, group, name, version);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                Diagnostic::plain(format!("could not create `{}`: {error}", parent.display()))
            })?;
        }
        let text = toml::to_string(entry).map_err(|error| {
            Diagnostic::plain(format!("could not serialize cache index: {error}"))
        })?;
        write_atomic(&path, text.as_bytes())
    }

    /// 归档是否已在缓存中，并返回其字节（校验摘要由调用方负责）。
    pub fn read_archive(&self, sha256: &str) -> Option<Vec<u8>> {
        fs::read(self.archive_path(sha256)).ok()
    }

    /// 原子写入归档；已存在同摘要文件时视为成功（内容寻址）。
    pub fn store_archive(&self, sha256: &str, bytes: &[u8]) -> Result<(), Diagnostic> {
        let path = self.archive_path(sha256);
        if path.is_file() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                Diagnostic::plain(format!("could not create `{}`: {error}", parent.display()))
            })?;
        }
        write_atomic(&path, bytes)
    }

    /// 确保归档已解包，返回解包目录。
    ///
    /// 解包先写入同级的临时目录，全部成功后再原子改名到最终位置；缺失清单标记
    /// 或解包中断时不会留下可被误用的半成品。
    pub fn ensure_unpacked(&self, sha256: &str, bytes: &[u8]) -> Result<PathBuf, Diagnostic> {
        let directory = self.unpacked_dir(sha256);
        if directory.join("dolphin.toml").is_file() {
            return Ok(directory);
        }
        let parent = directory.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| {
            Diagnostic::plain(format!("could not create `{}`: {error}", parent.display()))
        })?;
        let temp = parent.join(format!(
            ".unpack-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&temp);
        if let Err(error) = package_archive::extract(bytes, &temp) {
            let _ = fs::remove_dir_all(&temp);
            return Err(error);
        }
        // 目标可能残留半成品（例如旧版本写入的目录），改名到已存在目录会失败。
        if directory.exists() {
            let _ = fs::remove_dir_all(&directory);
        }
        fs::rename(&temp, &directory).map_err(|error| {
            let _ = fs::remove_dir_all(&temp);
            Diagnostic::plain(format!(
                "could not install unpacked cache `{}`: {error}",
                directory.display()
            ))
        })?;
        Ok(directory)
    }

    /// 已知坐标到摘要的映射（用于离线解析）。
    pub fn index_snapshot(&self, base: &str) -> BTreeMap<String, IndexEntry> {
        let root = self
            .home
            .join("cache")
            .join("index")
            .join(package_archive::sha256_hex(base.as_bytes()));
        let mut entries = BTreeMap::new();
        collect_entries(&root, &mut entries);
        entries
    }
}

fn collect_entries(directory: &Path, entries: &mut BTreeMap<String, IndexEntry>) {
    let Ok(read) = fs::read_dir(directory) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_entries(&path, entries);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "toml")
            && let Ok(text) = fs::read_to_string(&path)
            && let Ok(parsed) = toml::from_str::<IndexEntry>(&text)
        {
            entries.insert(parsed.coordinate.clone(), parsed);
        }
    }
}

/// 写临时文件后原子改名，失败不留下可被误用的半成品。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Diagnostic> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        Diagnostic::plain(format!("could not create `{}`: {error}", parent.display()))
    })?;
    let temp = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("write"),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    fs::write(&temp, bytes).map_err(|error| {
        Diagnostic::plain(format!("could not write `{}`: {error}", temp.display()))
    })?;
    if let Err(error) = fs::rename(&temp, path) {
        // Windows 上目标存在时 rename 失败；仅当已有文件内容完全一致（内容寻址）
        // 时才视为成功，避免把旧内容误当成本次写入的结果。
        let matches_existing = fs::read(path)
            .map(|existing| existing == bytes)
            .unwrap_or(false);
        let _ = fs::remove_file(&temp);
        if !matches_existing {
            return Err(Diagnostic::plain(format!(
                "could not move `{}` into place: {error}",
                temp.display()
            )));
        }
    }
    Ok(())
}
