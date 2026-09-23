# 包仓库与版本策略设计输入（未冻结）

> 状态：设计输入，2026-09-23 由维护者讨论整理；**未冻结、未实现**。M20 完成后由 H21-00
> 评估并决定是否并入 `proposal-m21-delivery.md`；在此之前不按本文改代码。
>
> 相关：[M18-M21 计划](plan-m18-plus.md) 第 7 节、[M19 规格](proposal-m19-cli-stdlib.md)、
> [安装与发行](installation.md)、[已实现功能参考](implemented-features.md)。

## 1. 背景：当前实现事实

- `.dlib` 是**源码归档**（确定性 ZIP，`format-version = 1`），消费端用自己的编译器重新编译；
  没有稳定二进制 ABI。
- 坐标固定为 `group:name:version`，只接受精确 semver（无范围、无 build metadata）；
  仓库布局固定为 `<base>/<group 路径>/<name>/<version>/<name>-<version>.dlib(.sha256)`。
- `dc publish` 用条件 PUT（`If-None-Match: *`）保证不可覆盖；`file://` 为原子不覆盖发布；
  HTTP 上传必须 HTTPS，token 取自 `DOLPHIN_REPOSITORY_<ID>_TOKEN`。
- `dc fetch`/`dc build` 把依赖下载到 `DOLPHIN_HOME`（默认 `~/.dolphin`）的**内容寻址缓存**：
  `cache/packages/sha256/<摘要>/{package.dlib,unpacked/}`，另有按仓库 URL 摘要分目录的坐标索引
  `cache/index/<base 摘要>/<group>/<name>/<version>.toml`。项目里只有 `dolphin.lock` 与
  `target/`，依赖源码不复制进项目。
- 仓库只在项目 `dolphin.toml` 的 `[repositories]` 声明；代理只通过环境变量
  （`HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY`/`NO_PROXY`）生效；没有用户级配置文件。
- 解析器以 `(group, name)` 为键，同一包在依赖图中**只允许一个版本**；第二个不同版本直接报冲突
  （`crates/dolphin-package/src/resolver.rs` 的 `by_name`）。
- `compiler-version` 由工具链自动写入归档与 `dolphin.lock`，消费端要求**完全相等**
  （`registry.rs` 的下载检查、`lockfile.rs` 的 `--locked` 检查）；工具链升级会使旧包与旧锁失效。

## 2. 目标形态

### 2.1 版本策略：精确版本 + 根直接依赖覆盖

- 所有依赖写精确版本，不做范围求解，不做同包多版本共存。
- 根项目**直接声明**的 `(group, name)` 覆盖传递依赖对同一包的任何版本请求（版本与仓库都取根声明）。
- 传递依赖之间版本不同且根未声明时：**报错**，提示用户在根清单声明该包版本，由用户自己匹配。
- 保持“每包单版本”，不引入 Cargo 式多版本共存。
- 锁文件记录覆盖后的有效版本与来源；`--locked` 对覆盖表变化敏感；`dc info` 显示被覆盖的传递请求。

### 2.2 全局配置（Maven settings 式）

新增用户级配置（建议 `$DOLPHIN_HOME/config.toml`，与缓存/测试隔离一致），至少包含：

```toml
[repositories]
official = "https://<official-base>/"

[mirrors]
official = "https://<mirror-base>/"   # 仅传输层；是否支持 "*" 通配待定

[proxy]
url = "http://<proxy>:8080"
no_proxy = "127.0.0.1,localhost,.internal"
```

- 优先级：CLI > 项目 `dolphin.toml` > 用户配置 > 内置默认。
- **canonical 身份**：lock 与缓存索引按“声明的仓库 URL”记账，镜像只改变实际请求地址；
  否则换镜像会使锁与索引失效。
- 官方默认仓库在站点上线前保持未配置；缺配置时给可操作诊断，不硬编码不存在的地址。
- 发布不经过只读镜像；镜像要可写必须显式声明。
- token 目前只走环境变量；若要写入配置文件需权限校验且不得出现在任何诊断输出中。

### 2.3 `compiler` 要求由作者声明

把“打包工具链”和“源码要求”拆开：

- `built-with`：工具链自动写入的戳，仅用于审计/诊断，不参与门禁。
- `compiler`：作者在 `dolphin.toml` 声明的要求，消费端据此判断能否编译。
- MVP 语法只支持精确值与 `>=X.Y.Z`（MSRV 式）；`^`/范围在 1.0 后再评估。
- pre-1.0 规则：0.x 的 minor 视为破坏性变更，`>=0.3.0` 默认不自动放行 0.4；1.0 之后
  `>=1.2` 这类向下兼容才有常规含义。
- 门禁位置从“归档 `compiler-version` 完全相等”改为“消费端版本满足声明要求”；
  lock 同时记录声明要求与实际编译器，`--locked` 因不满足声明而失败，而不是因工具链升级失败。
- 声明写错只会导致消费端编译失败（源码包需重新编译），不会产生二进制 ABI 静默不兼容。

### 2.4 统一缓存与项目形态

- 保持 `DOLPHIN_HOME` 全局内容寻址缓存；项目不引入 node_modules 式依赖目录，
  依赖只通过缓存 + `dolphin.lock` 参与构建。
- 实现前必须解决：缓存清理/回收（容量或最后使用时间）与**并发解包竞态**——
  `cache.rs` 的 `ensure_unpacked` 在安装前会删除已存在的目标目录，两个进程同时解包同一摘要
  可能互删。
- 兼容身份确定后，缓存需要考虑按兼容身份分命名空间，避免工具链升级后旧包混用或长期占空间。

### 2.5 站点协议（最小）

- 现有静态 GET + 条件 PUT 已满足发布/消费；站点只需正确实现这两点。
- 每包版本列表/依赖元数据接口只服务搜索与包页；在精确版本策略下不参与解析。
- 站点侧还需要所有权/命名空间、yank（不删除、只标记）、配额/限流与审计；
  `.dlib` 可携带 `[native.*]` 原生文件并被客户端链接，公共站点需要明确是否允许及其审核策略。

## 3. 实现影响面（供评估，不在本文实施）

| 范围 | 位置 | 说明 |
| --- | --- | --- |
| 覆盖表与两阶段解析 | `crates/dolphin-package/src/resolver.rs` | 先收集根直接依赖覆盖表，再解析传递依赖 |
| 用户级配置 | `crates/dolphin-package/src/{manifest,registry}.rs` | 新增配置解析与优先级；代理/镜像 |
| compiler 要求 | `crates/dolphin-package/src/{manifest,package_archive,lockfile,registry}.rs` | 字段、门禁与 lock 语义 |
| CLI 观测 | `src/main.rs`（`dc info`） | 显示覆盖与被覆盖的传递请求 |
| 缓存维护 | `crates/dolphin-package/src/cache.rs` | 回收/清理与并发安全 |
| 站点 | 仓库外 | 静态 GET、条件 PUT、可选索引、认证与安全策略 |

无 IR/lower/codegen 改动；不改现有 `dc build --lib` 打包行为与 path 依赖限制（D1）。

## 4. 待冻结决策（H21-00 输入）

1. 覆盖表范围：是否包含 path 依赖覆盖坐标依赖；覆盖是否连带仓库。
2. 传递-传递冲突行为：报错并提示根声明（当前倾向），不自动挑版本。
3. 配置文件路径/格式/优先级；镜像是否只读、是否支持通配；canonical URL 规则。
4. `compiler` 的语法与 pre-1.0 匹配规则；lock 新字段与 `--locked` 行为。
5. 缓存回收策略与并发安全；是否按兼容身份分命名空间。
6. 官方默认仓库地址、所有权/命名空间、yank 与原生文件政策。

## 5. 非目标与边界

- 不做版本范围 solver、不做同包多版本共存、不承诺稳定二进制 ABI。
- 不在本文冻结任何 API、清单字段或持久格式；冻结前不实现。
- 不改现有 CLI 行为、归档格式与 D1 限制；已有锁/归档的迁移方案由 H21-03 决定。
