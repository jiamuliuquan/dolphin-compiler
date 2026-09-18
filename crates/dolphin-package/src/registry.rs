//! Maven 风格仓库协议与远程消费（M15-E，§8.1）。
//!
//! 仓库是通过 HTTPS/HTTP GET 下载静态文件的网站；本模块也支持 `file://` 供离线
//! fixture 与本地发布。坐标 `org.example:mathlib:1.0.0` 的固定路径：
//!
//! ```text
//! <base>/org/example/mathlib/1.0.0/mathlib-1.0.0.dlib
//! <base>/org/example/mathlib/1.0.0/mathlib-1.0.0.dlib.sha256
//! ```
//!
//! 下载使用 TLS 校验、固定 30 秒超时、最多 3 次仅针对连接故障/5xx 的重试；
//! 404、摘要错误、解析错误不重试。GET 最多跟随 5 次同源重定向，不允许 HTTPS
//! 降级；凭据不会发往另一 origin。

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use url::Url;

use crate::cache::{Cache, IndexEntry};
use crate::lockfile::{LockedPackage, Lockfile};
use crate::manifest::parse_coordinate;
use crate::package_archive::{self, COMPILER_VERSION, FORMAT_VERSION, PackageMetadata, sha256_hex};
use crate::resolver::{AcquiredPackage, RemoteSource, ResolveOptions};
use dolphin_source::diagnostic::Diagnostic;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REDIRECTS: usize = 5;
const MAX_ATTEMPTS: usize = 3;

/// 仓库客户端：解析仓库地址、下载、校验并写入内容缓存。
pub struct Registry {
    agent: ureq::Agent,
    offline: bool,
    repositories: BTreeMap<String, String>,
    locked: BTreeMap<String, LockedPackage>,
    cache: Cache,
}

impl Registry {
    /// 用根项目仓库映射、解析选项与（可选的）已有锁文件构造客户端。
    pub fn new(
        repositories: &BTreeMap<String, String>,
        options: ResolveOptions,
        cache: Cache,
        lock: Option<&Lockfile>,
    ) -> Registry {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .http_status_as_error(false)
            // 重定向由本模块手动处理，以便约束同源与不降级。
            .max_redirects(0)
            .build();
        Registry {
            agent: ureq::Agent::new_with_config(config),
            offline: options.offline,
            repositories: repositories.clone(),
            locked: lock.map(Lockfile::locked_packages).unwrap_or_default(),
            cache,
        }
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// 解析仓库 ID 到规范化基地址。
    fn base_for(&self, repository: &str) -> Result<String, Diagnostic> {
        let id = repository.to_ascii_lowercase();
        self.repositories.get(&id).cloned().ok_or_else(|| {
            Diagnostic::plain(format!(
                "unknown repository `{repository}`; declare it under `[repositories]` in the root `dolphin.toml`"
            ))
        })
    }

    /// 从缓存恢复一个已锁定的摘要。
    fn restore_from_cache(
        &self,
        coordinate: &str,
        base: &str,
        expected: &str,
    ) -> Result<AcquiredPackage, Diagnostic> {
        let bytes = self.cache.read_archive(expected).ok_or_else(|| {
            Diagnostic::plain(format!(
                "package `{coordinate}` is not present in the local cache (expected sha256 {expected}); run without `--offline` to download it"
            ))
        })?;
        let actual = sha256_hex(&bytes);
        if actual != expected {
            return Err(Diagnostic::plain(format!(
                "cached archive for `{coordinate}` has sha256 {actual}, expected {expected}; the cache entry is corrupt"
            )));
        }
        let metadata = self.validate_metadata(coordinate, &bytes)?;
        let directory = self.cache.ensure_unpacked(expected, &bytes)?;
        if let Ok(manifest) = crate::manifest::load(&directory)
            && manifest.coordinate() != metadata.coordinate
        {
            return Err(Diagnostic::plain(format!(
                "cached package for `{coordinate}` declares `{}`",
                manifest.coordinate()
            )));
        }
        Ok(AcquiredPackage {
            root: directory,
            sha256: expected.to_string(),
            base: base.to_string(),
        })
    }

    /// 校验元数据的格式、源码类型与编译器版本（§7.2）。
    fn validate_metadata(
        &self,
        coordinate: &str,
        bytes: &[u8],
    ) -> Result<PackageMetadata, Diagnostic> {
        let metadata = package_archive::read_metadata(bytes)?;
        if metadata.coordinate != coordinate {
            return Err(Diagnostic::plain(format!(
                "package `{coordinate}` declares coordinate `{}`",
                metadata.coordinate
            )));
        }
        if metadata.format_version != FORMAT_VERSION {
            return Err(Diagnostic::plain(format!(
                "package `{coordinate}` uses archive format-version {}, this dc supports {FORMAT_VERSION}",
                metadata.format_version
            )));
        }
        if metadata.kind != "source" {
            return Err(Diagnostic::plain(format!(
                "package `{coordinate}` has kind `{}`, expected `source`",
                metadata.kind
            )));
        }
        if metadata.compiler_version != COMPILER_VERSION {
            return Err(Diagnostic::plain(format!(
                "package `{coordinate}` was built with dc {version}, but this is dc {COMPILER_VERSION}; versions must match exactly",
                version = metadata.compiler_version
            )));
        }
        Ok(metadata)
    }

    /// 读取 `.dlib` 与 `.sha256` 的 URL。
    fn artifact_urls(
        &self,
        base: &str,
        group: &str,
        name: &str,
        version: &str,
    ) -> Result<(Url, Url), Diagnostic> {
        let with_slash = format!("{}/", base.trim_end_matches('/'));
        let root = Url::parse(&with_slash).map_err(|error| {
            Diagnostic::plain(format!("invalid repository URL `{base}`: {error}"))
        })?;
        let group_path = group.replace('.', "/");
        let file = format!("{name}-{version}.dlib");
        let relative = format!("{group_path}/{name}/{version}/{file}");
        let dlib = root.join(&relative).map_err(|error| {
            Diagnostic::plain(format!(
                "could not build package URL under `{base}`: {error}"
            ))
        })?;
        let sha = root.join(&format!("{relative}.sha256")).map_err(|error| {
            Diagnostic::plain(format!(
                "could not build checksum URL under `{base}`: {error}"
            ))
        })?;
        Ok((dlib, sha))
    }

    /// 读取 URL 字节：`file://` 直接读盘，`http(s)://` 走 GET。404 返回 `None`。
    fn read_url(&self, url: &Url) -> Result<Option<Vec<u8>>, Diagnostic> {
        match url.scheme() {
            "file" => {
                let path = url.to_file_path().map_err(|_| {
                    Diagnostic::plain(format!("invalid file repository URL `{url}`"))
                })?;
                if !path.is_file() {
                    return Ok(None);
                }
                let length = std::fs::metadata(&path)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                if length > package_archive_limit() {
                    return Err(Diagnostic::plain(format!(
                        "repository file `{}` exceeds the {} byte limit",
                        path.display(),
                        package_archive_limit()
                    )));
                }
                let bytes = std::fs::read(&path).map_err(|error| {
                    Diagnostic::plain(format!("could not read `{}`: {error}", path.display()))
                })?;
                Ok(Some(bytes))
            }
            "http" | "https" => {
                if self.offline {
                    return Err(Diagnostic::plain(format!(
                        "offline mode forbids network access to `{url}`"
                    )));
                }
                self.http_get(url)
            }
            scheme => Err(Diagnostic::plain(format!(
                "unsupported repository scheme `{scheme}`"
            ))),
        }
    }

    /// HTTP GET：同源重定向、不降级、固定超时、连接/5xx 重试。
    fn http_get(&self, url: &Url) -> Result<Option<Vec<u8>>, Diagnostic> {
        let original = url.clone();
        let mut current = url.clone();
        for redirects in 0..=MAX_REDIRECTS {
            let mut response = self.send_with_retry(&current)?;
            let status = response.status().as_u16();
            match status {
                200..=299 => {
                    let bytes = response
                        .body_mut()
                        .with_config()
                        .limit(package_archive_limit())
                        .read_to_vec()
                        .map_err(|error| {
                            Diagnostic::plain(format!("could not read `{current}`: {error}"))
                        })?;
                    return Ok(Some(bytes));
                }
                301 | 302 | 303 | 307 | 308 => {
                    if redirects == MAX_REDIRECTS {
                        return Err(Diagnostic::plain(format!(
                            "too many redirects while fetching `{original}`"
                        )));
                    }
                    let location = response
                        .headers()
                        .get("location")
                        .and_then(|value| value.to_str().ok())
                        .ok_or_else(|| {
                            Diagnostic::plain(format!("redirect from `{current}` has no Location"))
                        })?;
                    let next = current.join(location).map_err(|error| {
                        Diagnostic::plain(format!("invalid redirect target `{location}`: {error}"))
                    })?;
                    if !same_origin(&original, &next) {
                        return Err(Diagnostic::plain(format!(
                            "refusing cross-origin redirect from `{current}` to `{next}`"
                        )));
                    }
                    if original.scheme() == "https" && next.scheme() != "https" {
                        return Err(Diagnostic::plain(format!(
                            "refusing HTTPS downgrade redirect to `{next}`"
                        )));
                    }
                    current = next;
                }
                404 => return Ok(None),
                other => {
                    return Err(Diagnostic::plain(format!(
                        "GET `{current}` failed with HTTP status {other}"
                    )));
                }
            }
        }
        Err(Diagnostic::plain(format!(
            "too many redirects while fetching `{original}`"
        )))
    }

    fn send_with_retry(&self, url: &Url) -> Result<ureq::http::Response<ureq::Body>, Diagnostic> {
        let mut last = None;
        for attempt in 0..MAX_ATTEMPTS {
            match self.agent.get(url.as_str()).call() {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if (500..600).contains(&status) && attempt + 1 < MAX_ATTEMPTS {
                        last = Some(format!("HTTP status {status}"));
                        continue;
                    }
                    if (500..600).contains(&status) {
                        return Err(Diagnostic::plain(format!(
                            "GET `{url}` failed with HTTP status {status} after {MAX_ATTEMPTS} attempts"
                        )));
                    }
                    return Ok(response);
                }
                Err(error) => {
                    if attempt + 1 < MAX_ATTEMPTS {
                        last = Some(error.to_string());
                        continue;
                    }
                    return Err(Diagnostic::plain(format!("could not GET `{url}`: {error}")));
                }
            }
        }
        Err(Diagnostic::plain(format!(
            "could not GET `{url}`: {}",
            last.unwrap_or_default()
        )))
    }

    /// 读取并严格校验 `.sha256` 文件：64 位小写十六进制摘要（加可选换行）。
    fn fetch_checksum(&self, url: &Url) -> Result<Option<String>, Diagnostic> {
        let Some(bytes) = self.read_url(url)? else {
            return Ok(None);
        };
        let text = String::from_utf8(bytes)
            .map_err(|_| Diagnostic::plain(format!("checksum file `{url}` is not valid UTF-8")))?;
        let trimmed = text.strip_suffix('\n').unwrap_or(&text);
        if trimmed.len() != 64
            || !trimmed
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Diagnostic::plain(format!(
                "checksum file `{url}` must contain a 64-character lowercase hex digest"
            )));
        }
        Ok(Some(trimmed.to_string()))
    }
}

fn package_archive_limit() -> u64 {
    // 单包下载上限 256 MiB（§7.2）。
    256 * 1024 * 1024
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

impl RemoteSource for Registry {
    fn acquire(
        &mut self,
        coordinate: &str,
        repository: &str,
    ) -> Result<AcquiredPackage, Diagnostic> {
        let (group, name, version) = parse_coordinate(coordinate)?;
        let base = self.base_for(repository)?;
        let file_repository = base.starts_with("file://");
        let locked = self.locked.get(coordinate).cloned();
        if let Some(locked) = &locked
            && locked.source != "path"
            && locked.source != base
        {
            return Err(Diagnostic::plain(format!(
                "package `{coordinate}` is locked to source `{}` but repository `{repository}` resolves to `{base}`; a build allows exactly one source per package",
                locked.source
            )));
        }

        if self.offline && !file_repository {
            let expected = locked
                .as_ref()
                .and_then(|locked| locked.sha256.clone())
                .or_else(|| {
                    self.cache
                        .read_index(&base, &group, &name, &version)
                        .map(|entry| entry.sha256)
                })
                .ok_or_else(|| {
                    Diagnostic::plain(format!(
                        "cannot resolve `{coordinate}` offline: it is not in the cache index"
                    ))
                })?;
            return self.restore_from_cache(coordinate, &base, &expected);
        }

        let (dlib_url, sha_url) = self.artifact_urls(&base, &group, &name, &version)?;
        let expected = match locked.as_ref().and_then(|locked| locked.sha256.clone()) {
            Some(locked_sha) => {
                // 锁内已有摘要：远程即使变化也不自动接受，要求发布新版本。
                if let Some(remote) = self.fetch_checksum(&sha_url)?
                    && remote != locked_sha
                {
                    return Err(Diagnostic::plain(format!(
                        "package `{coordinate}` changed content for the same version: repository reports {remote}, lock records {locked_sha}; publish a new version instead"
                    )));
                }
                locked_sha
            }
            None => self.fetch_checksum(&sha_url)?.ok_or_else(|| {
                Diagnostic::plain(format!(
                    "repository `{base}` does not provide a checksum file for `{coordinate}`"
                ))
            })?,
        };

        let bytes = match self.cache.read_archive(&expected) {
            Some(cached) => {
                if sha256_hex(&cached) != expected {
                    return Err(Diagnostic::plain(format!(
                        "cached archive for `{coordinate}` does not match sha256 {expected}"
                    )));
                }
                cached
            }
            None => {
                let Some(data) = self.read_url(&dlib_url)? else {
                    return Err(Diagnostic::plain(format!(
                        "package `{coordinate}` was not found at `{dlib_url}`"
                    )));
                };
                let actual = sha256_hex(&data);
                if actual != expected {
                    return Err(Diagnostic::plain(format!(
                        "checksum mismatch for `{coordinate}`: expected {expected}, got {actual}"
                    )));
                }
                self.cache.store_archive(&expected, &data)?;
                data
            }
        };

        self.validate_metadata(coordinate, &bytes)?;
        if !file_repository {
            self.cache.write_index(
                &base,
                &group,
                &name,
                &version,
                &IndexEntry {
                    coordinate: coordinate.to_string(),
                    source: base.clone(),
                    sha256: expected.clone(),
                },
            )?;
        }
        let directory = self.cache.ensure_unpacked(&expected, &bytes)?;
        Ok(AcquiredPackage {
            root: directory,
            sha256: expected,
            base,
        })
    }
}

/// 发布结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    /// 本次上传成功。
    Uploaded,
    /// 仓库中已存在且内容相同（幂等）。
    AlreadyPresent,
    /// 包已存在但摘要缺失，本次补传摘要（半上传恢复）。
    CompletedChecksum,
}

impl Registry {
    /// 发布库包：先条件创建 `.dlib`，再上传摘要作为完成标记（§9.2）。
    pub fn publish(
        &self,
        coordinate: &str,
        repository: &str,
        package: &[u8],
    ) -> Result<PublishOutcome, Diagnostic> {
        let base = self.base_for(repository)?;
        if base.starts_with("http") && self.offline {
            return Err(Diagnostic::plain(format!(
                "`--offline` forbids uploading to `{base}`; use a `file://` repository"
            )));
        }
        let (group, name, version) = parse_coordinate(coordinate)?;
        let (dlib_url, sha_url) = self.artifact_urls(&base, &group, &name, &version)?;
        let digest = sha256_hex(package);
        let checksum = format!("{digest}\n");
        let token = repository_token(repository);

        let outcome = match dlib_url.scheme() {
            "file" => publish_file(&dlib_url, &sha_url, package, checksum.as_bytes())?,
            "http" | "https" => self.publish_http(
                &dlib_url,
                &sha_url,
                package,
                checksum.as_bytes(),
                token.as_deref(),
            )?,
            scheme => {
                return Err(Diagnostic::plain(format!(
                    "unsupported publish scheme `{scheme}`"
                )));
            }
        };
        Ok(outcome)
    }

    fn publish_http(
        &self,
        dlib_url: &Url,
        sha_url: &Url,
        package: &[u8],
        checksum: &[u8],
        token: Option<&str>,
    ) -> Result<PublishOutcome, Diagnostic> {
        let dlib_status = self.conditional_put(dlib_url, package, token)?;
        let package_outcome = match dlib_status {
            PutStatus::Created => PublishOutcome::Uploaded,
            PutStatus::Exists => {
                // 条件创建失败：已有内容必须字节相同，否则拒绝覆盖。
                match self.read_url(dlib_url)? {
                    Some(existing) if existing == package => PublishOutcome::AlreadyPresent,
                    Some(_) => {
                        return Err(Diagnostic::plain(format!(
                            "repository already contains a different `{dlib_url}`; refusing to overwrite"
                        )));
                    }
                    None => {
                        return Err(Diagnostic::plain(format!(
                            "repository reported `{dlib_url}` exists but it could not be read"
                        )));
                    }
                }
            }
        };
        // 摘要作为完成标记；包已存在而摘要缺失时补传。
        match self.conditional_put(sha_url, checksum, token)? {
            PutStatus::Created => {
                if package_outcome == PublishOutcome::AlreadyPresent {
                    Ok(PublishOutcome::CompletedChecksum)
                } else {
                    Ok(PublishOutcome::Uploaded)
                }
            }
            PutStatus::Exists => match self.read_url(sha_url)? {
                Some(existing) if existing == checksum => Ok(package_outcome),
                Some(_) => Err(Diagnostic::plain(format!(
                    "repository already contains a different `{sha_url}`; refusing to overwrite"
                ))),
                None => Err(Diagnostic::plain(format!(
                    "repository reported `{sha_url}` exists but it could not be read"
                ))),
            },
        }
    }

    /// 条件创建 PUT：`If-None-Match: *`，不跟随重定向。
    fn conditional_put(
        &self,
        url: &Url,
        body: &[u8],
        token: Option<&str>,
    ) -> Result<PutStatus, Diagnostic> {
        if token.is_some() && url.scheme() != "https" {
            return Err(Diagnostic::plain(format!(
                "refusing to send repository credentials over `{}://`; use HTTPS",
                url.scheme()
            )));
        }
        let mut request = self
            .agent
            .put(url.as_str())
            .header("If-None-Match", "*")
            .header("Content-Type", "application/octet-stream");
        if let Some(token) = token {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
        let response = request
            .send(body)
            .map_err(|error| Diagnostic::plain(format!("could not PUT `{url}`: {error}")))?;
        match response.status().as_u16() {
            200 | 201 | 204 => Ok(PutStatus::Created),
            412 => Ok(PutStatus::Exists),
            405 | 501 => Err(Diagnostic::plain(format!(
                "repository `{url}` does not support conditional PUT (`If-None-Match`); upload the package manually and retry"
            ))),
            409 => Err(Diagnostic::plain(format!(
                "repository `{url}` rejected the upload with a conflict"
            ))),
            other => Err(Diagnostic::plain(format!(
                "PUT `{url}` failed with HTTP status {other}"
            ))),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PutStatus {
    Created,
    Exists,
}

/// `file://` 仓库的原子、不覆盖发布。
fn publish_file(
    dlib_url: &Url,
    sha_url: &Url,
    package: &[u8],
    checksum: &[u8],
) -> Result<PublishOutcome, Diagnostic> {
    let dlib = file_path(dlib_url)?;
    let sha = file_path(sha_url)?;
    let package_fresh = write_new_file(&dlib, package)?;
    let checksum_fresh = write_new_file(&sha, checksum)?;
    Ok(match (package_fresh, checksum_fresh) {
        (true, true) => PublishOutcome::Uploaded,
        (false, true) => PublishOutcome::CompletedChecksum,
        (false, false) => PublishOutcome::AlreadyPresent,
        (true, false) => PublishOutcome::Uploaded,
    })
}

fn file_path(url: &Url) -> Result<PathBuf, Diagnostic> {
    url.to_file_path()
        .map_err(|_| Diagnostic::plain(format!("invalid file repository URL `{url}`")))
}

/// 不覆盖地创建文件；已存在且字节相同返回 false，内容不同报错。
///
/// 使用 `create_new` 原子创建，避免「先检查后写入」的竞态导致覆盖已有包。
fn write_new_file(path: &std::path::Path, bytes: &[u8]) -> Result<bool, Diagnostic> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            Diagnostic::plain(format!("could not create `{}`: {error}", parent.display()))
        })?;
    }
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            if let Err(error) = file.write_all(bytes) {
                drop(file);
                let _ = std::fs::remove_file(path);
                return Err(Diagnostic::plain(format!(
                    "could not write `{}`: {error}",
                    path.display()
                )));
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = std::fs::read(path).map_err(|error| {
                Diagnostic::plain(format!("could not read `{}`: {error}", path.display()))
            })?;
            if existing == bytes {
                Ok(false)
            } else {
                Err(Diagnostic::plain(format!(
                    "repository already contains a different `{}`; refusing to overwrite",
                    path.display()
                )))
            }
        }
        Err(error) => Err(Diagnostic::plain(format!(
            "could not create `{}`: {error}",
            path.display()
        ))),
    }
}

/// `DOLPHIN_REPOSITORY_<ID>_TOKEN`（ID 大写）。
fn repository_token(repository: &str) -> Option<String> {
    let key = format!(
        "DOLPHIN_REPOSITORY_{}_TOKEN",
        repository.to_ascii_uppercase()
    );
    std::env::var(key).ok().filter(|token| !token.is_empty())
}
