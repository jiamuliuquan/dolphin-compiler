//! 包身份与包图（M15）。
//!
//! 泛型实例与定义身份必须包含包坐标，避免同名定义跨包碰撞（M15 规格 §2.2）。
//! v1 消费端统一编译源码，包图由 `resolver.rs` 建立；本模块定义稳定身份类型、
//! 包来源与解析后的 `PackageGraph`。
//!
//! 每个包在编译期拥有唯一的模块前缀：
//!
//! - 根包：空前缀，沿用 M13/M14 的限定名（如 `util.numbers.Pair`）；
//! - 源码标准库：`std` 前缀；
//! - 依赖包：`@<PackageId 序号>`，该前缀含有标识符不允许的字符，保证不会与
//!   任何用户模块名碰撞（GEN-05）。
//!
//! `PackageId` 本身包含在 `GenericKey` 中，同一库泛型被多个包以不同别名使用时
//! 仍只生成一份实例（GEN-02）。

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::manifest::Manifest;

/// 包身份。`ROOT` 是当前用户包，`STD` 是内建/源码标准库，其余为依赖包。
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct PackageId(pub u32);

impl PackageId {
    /// 当前被编译的根包。
    pub const ROOT: Self = Self(0);
    /// 随编译器分发的标准库保留身份。
    ///
    /// 使用哨兵值而不是 `1`，避免与包图中按 `0,1,2,...` 分配的依赖包冲突。
    pub const STD: Self = Self(u32::MAX);

    /// 编译期模块前缀；根包为空。
    pub fn prefix(self) -> String {
        match self {
            Self::ROOT => String::new(),
            Self::STD => "std".to_string(),
            Self(id) => format!("@{id}"),
        }
    }
}

impl std::fmt::Display for PackageId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::ROOT => formatter.write_str("<root>"),
            Self::STD => formatter.write_str("std"),
            Self(id) => write!(formatter, "package#{id}"),
        }
    }
}

/// 依赖包的来源。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackageSource {
    /// 根项目。
    Root,
    /// 本地路径依赖；保存规范化后的项目根目录。
    Path(PathBuf),
    /// 远程坐标依赖；保存仓库 ID、规范化基地址与归档摘要。
    Remote {
        repository: String,
        base: String,
        sha256: String,
    },
}

impl PackageSource {
    /// 锁文件中的来源描述。
    pub fn lock_source(&self) -> String {
        match self {
            PackageSource::Root => "root".to_string(),
            PackageSource::Path(_) => "path".to_string(),
            PackageSource::Remote { base, .. } => base.clone(),
        }
    }
}

/// 解析后的包：身份、来源、清单、源码根与别名环境。
#[derive(Clone, Debug)]
pub struct PackageInfo {
    pub id: PackageId,
    pub manifest: Manifest,
    /// 包根目录（路径依赖的项目根；远程包的解包目录）。
    pub root: PathBuf,
    /// 源码根目录（`[package].source`）。
    pub source_root: PathBuf,
    pub source: PackageSource,
    /// 编译期模块前缀。
    pub prefix: String,
    /// 该包自己的依赖别名环境：别名 -> 依赖包身份。
    pub aliases: BTreeMap<String, PackageId>,
}

impl PackageInfo {
    pub fn coordinate(&self) -> String {
        self.manifest.package.coordinate()
    }

    pub fn name(&self) -> &str {
        &self.manifest.package.name
    }
}

/// 依赖图：节点是包，边是依赖。
#[derive(Clone, Debug)]
pub struct PackageGraph {
    pub root: PackageId,
    /// 按 `PackageId` 索引。
    pub packages: Vec<PackageInfo>,
    /// 确定性拓扑顺序：被依赖者先于依赖者。
    pub order: Vec<PackageId>,
}

impl PackageGraph {
    pub fn get(&self, id: PackageId) -> &PackageInfo {
        &self.packages[id.0 as usize]
    }

    pub fn iter(&self) -> impl Iterator<Item = &PackageInfo> {
        self.packages.iter()
    }

    /// 按顶层顺序（依赖在先）遍历，用于链接原生输入与打包。
    pub fn order_iter(&self) -> impl Iterator<Item = &PackageInfo> {
        self.order.iter().map(|id| self.get(*id))
    }

    /// 包图中是否存在该 `(group, name)`。
    pub fn find_by_name(&self, group: &str, name: &str) -> Option<&PackageInfo> {
        self.packages.iter().find(|package| {
            package.manifest.package.group == group && package.manifest.package.name == name
        })
    }
}

/// 从全限定定义名推导其所属模块（限定名前缀；根模块为空）。
///
/// 名称由 `modules.rs` 以 `module.path.Name` 形式生成，因此包内模块归属可以从
/// 限定名稳定还原。
pub fn module_of(qualified_name: &str) -> &str {
    match qualified_name.rsplit_once('.') {
        Some((module, _)) => module,
        None => "",
    }
}

/// 把包前缀与相对模块名拼成全局限定模块名。
pub fn qualify_module(prefix: &str, module: &str) -> String {
    match (prefix.is_empty(), module.is_empty()) {
        (true, _) => module.to_string(),
        (false, true) => prefix.to_string(),
        (false, false) => format!("{prefix}.{module}"),
    }
}
