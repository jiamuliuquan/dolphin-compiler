//! `.dlib` 库包：确定性 ZIP 写入与安全解包（M15-D）。
//!
//! v1 是「经编译验证的源码型库包」：归档携带规范化清单、完整库源码和明确声明的
//! C 原生文件，消费端在目标平台与应用一起单态化并生成本机代码（§7.1）。
//!
//! 确定性要求（§7.2）：条目按 UTF-8 路径排序、统一 `/`、DOS 时间戳
//! `1980-01-01 00:00:00`、普通文件权限 `0644`、DEFLATE 级别 6、不嵌入绝对路径或
//! 当前时间。相同输入与 dc 版本产生相同 SHA-256。

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::manifest::{DependencyKind, Manifest};
use dolphin_source::diagnostic::Diagnostic;

/// 归档格式版本。
pub const FORMAT_VERSION: u32 = 1;
/// 编译器版本；消费端要求与包内 `compiler-version` 完全相同（§7.2）。
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 单包下载/解压上限：256 MiB。
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
/// 单文件上限：128 MiB。
const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
/// 单个清单/元数据上限：1 MiB。
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
/// 文件数上限。
const MAX_FILES: usize = 10_000;

pub const METADATA_PATH: &str = "META-INF/dolphin-package.toml";
pub const MANIFEST_PATH: &str = "dolphin.toml";

/// `META-INF/dolphin-package.toml`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageMetadata {
    #[serde(rename = "format-version")]
    pub format_version: u32,
    pub coordinate: String,
    #[serde(rename = "compiler-version")]
    pub compiler_version: String,
    /// `source` 表示源码型库包。
    pub kind: String,
    /// 携带原生文件的目标三元组；纯 Dolphin 包为空。
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default, rename = "native-files")]
    pub native_files: Vec<NativeFileRecord>,
}

/// 原生文件摘要记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeFileRecord {
    pub path: String,
    pub sha256: String,
}

/// 一条待写入的归档条目。
struct Entry {
    path: String,
    bytes: Vec<u8>,
}

/// 从库清单与包图构造 `.dlib` 字节（确定性）。
pub fn build(
    manifest: &Manifest,
    _graph: &crate::package::PackageGraph,
) -> Result<Vec<u8>, Diagnostic> {
    let lib = manifest.lib.as_ref().ok_or_else(|| {
        Diagnostic::plain(format!(
            "`{}` does not declare a `[lib]` target and cannot be packaged",
            manifest.path.display()
        ))
    })?;
    if !lib.path.is_file() {
        return Err(Diagnostic::plain(format!(
            "library entry `{}` does not exist",
            lib.path.display()
        )));
    }

    let mut entries: Vec<Entry> = Vec::new();
    let mut native_files: Vec<NativeFileRecord> = Vec::new();
    let mut targets: BTreeSet<String> = BTreeSet::new();
    let mut normalized_native: NormalizedNative = Vec::new();

    // 1. 库源码：`src/` 下的全部 `.do`，排除 bin 入口。
    let bin_paths = bin_paths_relative_to_source(manifest);
    let source_root = &manifest.package.source;
    let mut sources = Vec::new();
    discover_do_files(source_root, &mut sources)?;
    sources.sort();
    if sources.is_empty() {
        return Err(Diagnostic::plain(format!(
            "source directory `{}` does not contain any `.do` files",
            source_root.display()
        )));
    }
    for path in &sources {
        let relative = path.strip_prefix(source_root).map_err(|_| {
            Diagnostic::plain(format!(
                "source `{}` is outside the source root",
                path.display()
            ))
        })?;
        let relative_text = path_to_slash(relative);
        if bin_paths.contains(&relative_text) {
            continue;
        }
        entries.push(Entry {
            path: format!("src/{relative_text}"),
            bytes: read_bounded(path, MAX_FILE_BYTES)?,
        });
    }

    // 2. 原生文件：按目标复制到 `native/<target>/<file>`。
    for (triple, spec) in &manifest.native {
        let copy = |paths: &[PathBuf],
                    entries: &mut Vec<Entry>,
                    native_files: &mut Vec<NativeFileRecord>| {
            let mut result = Vec::new();
            for path in paths {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| {
                        Diagnostic::plain(format!("invalid native file `{}`", path.display()))
                    })?;
                let archive_path = format!("native/{triple}/{name}");
                let bytes = read_bounded(path, MAX_FILE_BYTES)?;
                let sha256 = sha256_hex(&bytes);
                native_files.push(NativeFileRecord {
                    path: archive_path.clone(),
                    sha256,
                });
                entries.push(Entry {
                    path: archive_path.clone(),
                    bytes,
                });
                result.push(archive_path);
            }
            Ok::<_, Diagnostic>(result)
        };
        let objects = copy(&spec.objects, &mut entries, &mut native_files)?;
        let static_libs = copy(&spec.static_libs, &mut entries, &mut native_files)?;
        let shared_libs = copy(&spec.shared_libs, &mut entries, &mut native_files)?;
        let runtime_files = copy(&spec.runtime_files, &mut entries, &mut native_files)?;
        if !objects.is_empty()
            || !static_libs.is_empty()
            || !shared_libs.is_empty()
            || !runtime_files.is_empty()
        {
            targets.insert(triple.clone());
        }
        normalized_native.push((
            triple.clone(),
            (objects, static_libs, shared_libs, runtime_files),
        ));
    }

    // 3. 规范化清单：只保留 package、lib、精确 dependencies、native。
    let normalized = normalized_manifest(
        manifest,
        lib.path.strip_prefix(source_root).ok(),
        &normalized_native,
    )?;
    entries.push(Entry {
        path: MANIFEST_PATH.to_string(),
        bytes: normalized.into_bytes(),
    });

    // 4. 元数据。
    let metadata = PackageMetadata {
        format_version: FORMAT_VERSION,
        coordinate: manifest.coordinate(),
        compiler_version: COMPILER_VERSION.to_string(),
        kind: "source".to_string(),
        targets: targets.into_iter().collect(),
        native_files,
    };
    let metadata_text = toml::to_string(&metadata).map_err(|error| {
        Diagnostic::plain(format!("could not serialize package metadata: {error}"))
    })?;
    entries.push(Entry {
        path: METADATA_PATH.to_string(),
        bytes: metadata_text.into_bytes(),
    });

    // 5. LICENSE（若项目存在）。
    let license = manifest.root.join("LICENSE");
    if license.is_file() {
        entries.push(Entry {
            path: "LICENSE".to_string(),
            bytes: read_bounded(&license, MAX_FILE_BYTES)?,
        });
    }

    write_zip(&mut entries)
}

/// 规范化清单的 native 表：`(target, (objects, static_libs, shared_libs, runtime_files))`。
type NormalizedNative = Vec<(String, (Vec<String>, Vec<String>, Vec<String>, Vec<String>))>;

fn normalized_manifest(
    manifest: &Manifest,
    lib_relative: Option<&Path>,
    native: &NormalizedNative,
) -> Result<String, Diagnostic> {
    let mut output = String::new();
    output.push_str("[package]\n");
    output.push_str(&format!(
        "group = {}\n",
        toml_string(&manifest.package.group)
    ));
    output.push_str(&format!("name = {}\n", toml_string(&manifest.package.name)));
    output.push_str(&format!(
        "version = {}\n",
        toml_string(&manifest.package.version)
    ));
    output.push_str("source = \"src\"\n\n");

    let lib_relative = lib_relative
        .ok_or_else(|| Diagnostic::plain("library entry must live inside the source directory"))?;
    output.push_str("[lib]\n");
    output.push_str(&format!(
        "path = {}\n\n",
        toml_string(&format!("src/{}", path_to_slash(lib_relative)))
    ));

    let mut exact = Vec::new();
    for dependency in &manifest.dependencies {
        match &dependency.kind {
            DependencyKind::Path(_) => {
                return Err(Diagnostic::plain(format!(
                    "cannot publish: dependency `{}` is a path dependency; publish it first and replace it with an exact coordinate",
                    dependency.alias
                )));
            }
            DependencyKind::Coordinate {
                coordinate,
                repository,
            } => {
                if repository == "default" {
                    exact.push(format!(
                        "{} = {}\n",
                        dependency.alias,
                        toml_string(coordinate)
                    ));
                } else {
                    exact.push(format!(
                        "{} = {{ coordinate = {}, repository = {} }}\n",
                        dependency.alias,
                        toml_string(coordinate),
                        toml_string(repository)
                    ));
                }
            }
        }
    }
    if !exact.is_empty() {
        output.push_str("[dependencies]\n");
        for line in exact {
            output.push_str(&line);
        }
        output.push('\n');
    }

    for (triple, (objects, static_libs, shared_libs, runtime_files)) in native {
        output.push_str(&format!("[native.{triple}]\n"));
        for (key, values) in [
            ("objects", objects),
            ("static-libs", static_libs),
            ("shared-libs", shared_libs),
            ("runtime-files", runtime_files),
        ] {
            if values.is_empty() {
                continue;
            }
            let joined = values
                .iter()
                .map(|value| toml_string(value))
                .collect::<Vec<_>>()
                .join(", ");
            output.push_str(&format!("{key} = [{joined}]\n"));
        }
        output.push('\n');
    }

    Ok(output)
}

/// 收集源文件相对源码根、并排除的 bin 入口（相对路径，`/` 分隔）。
fn bin_paths_relative_to_source(manifest: &Manifest) -> HashSet<String> {
    let source = &manifest.package.source;
    manifest
        .bins
        .iter()
        .filter_map(|bin| bin.path.strip_prefix(source).ok())
        .map(path_to_slash)
        .collect()
}

fn discover_do_files(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), Diagnostic> {
    let entries = fs::read_dir(directory).map_err(|error| {
        Diagnostic::plain(format!(
            "could not read directory `{}`: {error}",
            directory.display()
        ))
    })?;
    for entry in entries {
        let entry = entry
            .map_err(|error| Diagnostic::plain(format!("could not read source entry: {error}")))?;
        let file_type = entry.file_type().map_err(|error| {
            Diagnostic::plain(format!(
                "could not inspect `{}`: {error}",
                entry.path().display()
            ))
        })?;
        if file_type.is_dir() {
            discover_do_files(&entry.path(), paths)?;
        } else if file_type.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "do")
        {
            paths.push(entry.path());
        }
    }
    Ok(())
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, Diagnostic> {
    let metadata = fs::metadata(path).map_err(|error| {
        Diagnostic::plain(format!("could not read `{}`: {error}", path.display()))
    })?;
    if metadata.len() > limit {
        return Err(Diagnostic::plain(format!(
            "file `{}` exceeds the {} byte limit",
            path.display(),
            limit
        )));
    }
    fs::read(path)
        .map_err(|error| Diagnostic::plain(format!("could not read `{}`: {error}", path.display())))
}

/// 按确定性规则写出 ZIP。
fn write_zip(entries: &mut [Entry]) -> Result<Vec<u8>, Diagnostic> {
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    for pair in entries.windows(2) {
        if pair[0].path == pair[1].path {
            return Err(Diagnostic::plain(format!(
                "duplicate archive entry `{}`",
                pair[0].path
            )));
        }
    }
    let mut buffer = Vec::new();
    {
        let mut writer = ZipWriter::new(Cursor::new(&mut buffer));
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6))
            .last_modified_time(DateTime::default())
            .unix_permissions(0o644)
            .large_file(false);
        for entry in entries.iter() {
            writer
                .start_file(entry.path.as_str(), options)
                .map_err(|error| {
                    Diagnostic::plain(format!("could not write archive entry: {error}"))
                })?;
            writer.write_all(&entry.bytes).map_err(|error| {
                Diagnostic::plain(format!("could not write archive entry: {error}"))
            })?;
        }
        writer
            .finish()
            .map_err(|error| Diagnostic::plain(format!("could not finish archive: {error}")))?;
    }
    Ok(buffer)
}

/// SHA-256 小写十六进制摘要。
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut text = String::with_capacity(64);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// 解析元数据但不解包。
pub fn read_metadata(bytes: &[u8]) -> Result<PackageMetadata, Diagnostic> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| Diagnostic::plain(format!("invalid `.dlib` archive: {error}")))?;
    let mut text = String::new();
    {
        let mut entry = archive.by_name(METADATA_PATH).map_err(|error| {
            Diagnostic::plain(format!("`.dlib` is missing `{METADATA_PATH}`: {error}"))
        })?;
        if entry.size() > MAX_METADATA_BYTES {
            return Err(Diagnostic::plain(format!(
                "package metadata exceeds the {MAX_METADATA_BYTES} byte limit"
            )));
        }
        // 不信任 ZIP 头部声明的大小：按字节上限读取，防止解压炸弹。
        entry
            .by_ref()
            .take(MAX_METADATA_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|error| {
                Diagnostic::plain(format!("could not read package metadata: {error}"))
            })?;
        if text.len() as u64 > MAX_METADATA_BYTES {
            return Err(Diagnostic::plain(format!(
                "package metadata exceeds the {MAX_METADATA_BYTES} byte limit"
            )));
        }
    }
    toml::from_str(&text)
        .map_err(|error| Diagnostic::plain(format!("invalid package metadata: {error}")))
}

/// 校验并解包到 `destination`，返回元数据。
pub fn extract(bytes: &[u8], destination: &Path) -> Result<PackageMetadata, Diagnostic> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| Diagnostic::plain(format!("invalid `.dlib` archive: {error}")))?;
    if archive.len() > MAX_FILES {
        return Err(Diagnostic::plain(format!(
            "package contains more than {MAX_FILES} files"
        )));
    }

    // 第一遍：校验全部条目名、重复与大小写冲突、大小上限（不信任 ZIP 头部声明）。
    let mut seen: HashSet<String> = HashSet::new();
    let mut folded: HashSet<String> = HashSet::new();
    let mut total: u64 = 0;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| Diagnostic::plain(format!("invalid archive entry: {error}")))?;
        let name = entry.name().to_string();
        let is_dir = entry.is_dir();
        validate_entry_name(&name, is_dir)?;
        if !seen.insert(name.clone()) {
            return Err(Diagnostic::plain(format!(
                "duplicate archive entry `{name}`"
            )));
        }
        if !folded.insert(name.to_lowercase()) {
            return Err(Diagnostic::plain(format!(
                "archive entry `{name}` collides with another entry when case is folded"
            )));
        }
        if is_symlink(&entry) {
            return Err(Diagnostic::plain(format!(
                "archive entry `{name}` is a symbolic link, which is not allowed"
            )));
        }
        if !is_dir {
            if entry.size() > MAX_FILE_BYTES {
                return Err(Diagnostic::plain(format!(
                    "archive entry `{name}` exceeds the {MAX_FILE_BYTES} byte limit"
                )));
            }
            total = total
                .checked_add(entry.size())
                .ok_or_else(|| Diagnostic::plain("archive uncompressed size overflow"))?;
            if total > MAX_TOTAL_BYTES {
                return Err(Diagnostic::plain(format!(
                    "archive exceeds the {MAX_TOTAL_BYTES} byte total limit"
                )));
            }
        }
    }
    drop(archive);

    let archive_metadata = read_metadata(bytes)?;

    // 第二遍：提取，提取过程继续计数。
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| Diagnostic::plain(format!("invalid `.dlib` archive: {error}")))?;
    fs::create_dir_all(destination).map_err(|error| {
        Diagnostic::plain(format!(
            "could not create `{}`: {error}",
            destination.display()
        ))
    })?;
    let mut extracted: u64 = 0;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| Diagnostic::plain(format!("invalid archive entry: {error}")))?;
        let name = entry.name().to_string();
        let target = destination.join(&name);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|error| {
                Diagnostic::plain(format!("could not create `{}`: {error}", target.display()))
            })?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                Diagnostic::plain(format!("could not create `{}`: {error}", parent.display()))
            })?;
        }
        let mut file = fs::File::create(&target).map_err(|error| {
            Diagnostic::plain(format!("could not create `{}`: {error}", target.display()))
        })?;
        let mut chunk = [0u8; 64 * 1024];
        let mut file_extracted: u64 = 0;
        loop {
            let read = entry.read(&mut chunk).map_err(|error| {
                Diagnostic::plain(format!("could not read archive entry `{name}`: {error}"))
            })?;
            if read == 0 {
                break;
            }
            file_extracted += read as u64;
            if file_extracted > MAX_FILE_BYTES {
                return Err(Diagnostic::plain(format!(
                    "archive entry `{name}` exceeds the {MAX_FILE_BYTES} byte limit during extraction"
                )));
            }
            extracted += read as u64;
            if extracted > MAX_TOTAL_BYTES {
                return Err(Diagnostic::plain(format!(
                    "archive exceeds the {MAX_TOTAL_BYTES} byte total limit during extraction"
                )));
            }
            file.write_all(&chunk[..read]).map_err(|error| {
                Diagnostic::plain(format!("could not write `{}`: {error}", target.display()))
            })?;
        }
    }
    drop(archive);
    Ok(archive_metadata)
}

fn is_symlink<R: Read>(entry: &zip::read::ZipFile<'_, R>) -> bool {
    match entry.unix_mode() {
        Some(mode) => mode & 0o170_000 == 0o120_000,
        None => false,
    }
}

/// 拒绝绝对路径、盘符/UNC、反斜杠、`..`、空段与未允许的顶层路径。
fn validate_entry_name(name: &str, is_dir: bool) -> Result<(), Diagnostic> {
    if name.is_empty() {
        return Err(Diagnostic::plain(
            "archive contains an entry with an empty name",
        ));
    }
    if name.contains('\\') {
        return Err(Diagnostic::plain(format!(
            "archive entry `{name}` uses a backslash path separator"
        )));
    }
    if name.starts_with('/') || name.contains(':') {
        return Err(Diagnostic::plain(format!(
            "archive entry `{name}` is an absolute or drive-qualified path"
        )));
    }
    if name.chars().any(|ch| ch.is_control()) {
        return Err(Diagnostic::plain(format!(
            "archive entry `{name}` contains control characters"
        )));
    }
    let trimmed = name.trim_end_matches('/');
    for component in trimmed.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(Diagnostic::plain(format!(
                "archive entry `{name}` contains an unsafe path component"
            )));
        }
    }
    if !is_dir {
        let first = trimmed.split('/').next().unwrap_or("");
        let allowed = matches!(first, "src" | "native" | "META-INF")
            || matches!(trimmed, MANIFEST_PATH | "LICENSE");
        if !allowed {
            return Err(Diagnostic::plain(format!(
                "archive entry `{name}` is outside the allowed package layout"
            )));
        }
    }
    Ok(())
}

fn path_to_slash(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn toml_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "dolphin-archive-{tag}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn raw_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut writer = ZipWriter::new(Cursor::new(&mut buffer));
            let options = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .last_modified_time(DateTime::default())
                .unix_permissions(0o644);
            for (name, bytes) in entries {
                writer.start_file(*name, options).expect("start file");
                writer.write_all(bytes).expect("write");
            }
            writer.finish().expect("finish");
        }
        buffer
    }

    #[test]
    fn rejects_path_traversal_and_absolute_entries() {
        for name in [
            "../evil.txt",
            "src/../../evil.txt",
            "/abs.txt",
            "C:/evil.txt",
        ] {
            let bytes = raw_zip(&[(name, b"x")]);
            let destination = temp_dir("traversal");
            let error = extract(&bytes, &destination).unwrap_err();
            assert!(
                error.to_string().contains("unsafe")
                    || error.to_string().contains("absolute")
                    || error.to_string().contains("allowed"),
                "unexpected error for `{name}`: {error}"
            );
            let _ = fs::remove_dir_all(&destination);
        }
    }

    #[test]
    fn rejects_case_folded_entries() {
        let folded = raw_zip(&[("src/a.do", b"a"), ("src/A.DO", b"b")]);
        let destination = temp_dir("folded");
        let error = extract(&folded, &destination).unwrap_err();
        assert!(error.to_string().contains("case"), "got: {error}");
        let _ = fs::remove_dir_all(&destination);
    }

    #[test]
    fn rejects_disallowed_top_level() {
        let bytes = raw_zip(&[("secrets.txt", b"x")]);
        let destination = temp_dir("top");
        let error = extract(&bytes, &destination).unwrap_err();
        assert!(error.to_string().contains("allowed"), "got: {error}");
        let _ = fs::remove_dir_all(&destination);
    }

    #[test]
    fn metadata_roundtrips() {
        let metadata = PackageMetadata {
            format_version: FORMAT_VERSION,
            coordinate: "org.example:mathlib:1.0.0".to_string(),
            compiler_version: COMPILER_VERSION.to_string(),
            kind: "source".to_string(),
            targets: vec!["x86_64-unknown-linux-gnu".to_string()],
            native_files: vec![NativeFileRecord {
                path: "native/x86_64-unknown-linux-gnu/libdemo.a".to_string(),
                sha256: "00".repeat(32),
            }],
        };
        let text = toml::to_string(&metadata).expect("serialize");
        assert!(text.contains("format-version = 1"));
        let parsed: PackageMetadata = toml::from_str(&text).expect("deserialize");
        assert_eq!(parsed, metadata);
    }
}
