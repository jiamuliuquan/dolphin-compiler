# M20 进度报告

本文件按 [M18 执行合同第 6 节模板](../plan-m18-correctness.md#6-报告与验收映射)追加批次记录。
每批一节；未实际运行的检查必须如实标注；设计冻结批次不实现代码，不代表 M20 已完成。

## H20-00 项目分析与工具协议规格冻结

- 批次：H20-00
- 状态：完成（规格已产出；**D-M20-1..4 已于 2026-09-24 由用户确认，H20-01 解阻待派发**）
- 前置批次及报告：H19-07 / M19 阶段验收（见 [m19-progress](m19-progress.md) 末节，M19 已完成）

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`ac64486df70746bb66c8d0586fb7bfdb49587bcd`（`update docs`）；工作区干净。
- 本批只新增/修改文档：
  - 新增 `docs/proposal-m20-project-tools.md`（H20-00 冻结规格）；
  - 新增本报告；
  - `docs/plan-m18-plus.md` 的 H20-00 状态行与 M20 节状态说明。
- 未触碰任何源码、示例、清单、测试、构建配置；未 commit/push/tag。

### 产出与关键决策（详见规格）

| 决策表项 | 冻结选择（唯一推荐） | 规格章节 |
| --- | --- | --- |
| SourceId/Span 身份 | `Span` 保持字节半开区间；新增 `SourceId(u32)` 为加载单元内下标、`SourceFile.id` 字段、`SourceMap` 只读文件表；跨快照不稳定、不持久化 | §3 |
| 诊断数据 | `Diagnostic` 保留 `plain`/`at` 签名与 `Display` 文本，新增 `code`/`severity`/`labels`/`notes` 访问器与 `with_label`/`with_note`/`with_code`；LSP 消费结构化字段，CLI 继续渲染 | §4 |
| 错误恢复 | 新增 `lex_recovering`/`parse_recovering`（同步点、每文件 100 条上限 + `E0002`）；声明级语义校验逐声明收集（`lower_sources_analysis_collecting`），函数体仍首错即停；有诊断不 lower、不产生产物 | §5 |
| 共享项目分析 | 新增 `dolphin-analysis` crate：`AnalysisHost`/`AnalysisSnapshot`/`AnalysisUnit`/`SymbolIndex`/`Resolution`；HIR 增 `lower_sources_analysis` side table；loader 增 `SourceProvider`/`LoadedSources` | §6 |
| 依赖缺失行为 | 新增 `dolphin-package::resolver::resolve_readonly`：读锁但不写锁、offline、不下载；失败返回 `E1001` 并提示 `dc fetch`；CLI 构建路径不变 | §6.2、§6.5 |
| 取消/版本规则 | M20 同步单线程无取消令牌；`set_overlay` 拒绝旧 version；`revision` 作为未来异步的发布契约；发布诊断带文档 version | §6.5 |
| lib/bin 选择 | 有 `[lib]` 生成 Lib 单元（排除全部 bin 入口、不要求 main）；每个 `[[bin]]` 生成 Bin 单元（排除其他入口）；共享文件按 Lib > 清单顺序 Bin 查询；诊断按路径去重 | §6.4、§6.5 |
| LSP 测试协议 | 真实 `dc lsp <项目>` 子进程 stdio 会话（`tests/support/lsp.rs` + `tests/m20_lsp.rs`），10s 超时，固定握手与退出码；未知方法 `-32601`、未初始化 `-32002`、未 shutdown 的 `exit` 退出 1 | §7.2、§7.6 |
| Formatter | 清单发现 `[package].source`、排除 `build.output`/`.git`、显式路径精确生效、统一 LF、全有或全无写入、token/注释保持性 | §8 |
| 调试器 | `scripts/debug_smoke.sh`（gdb）/`debug_smoke_lldb.sh`（lldb）批处理 + `tests/m20_debug.rs`；只验 LLVM Debug；工具缺失必须显式跳过并标未验证 | §9 |

### 验收映射

设计批次无编号测试；映射到 [计划](../plan-m18-plus.md) 第 6 节与规格第 12 节：

| 检查 | 证据 | 结果 |
| --- | --- | --- |
| H20-00 要求冻结的 7 项均有唯一推荐与拒绝方案 | `docs/proposal-m20-project-tools.md` §3–§9 | 完成 |
| API/格式/语义、拥有权、错误行为 | §3–§10（§10.2 拥有权表、§10.3 错误行为表） | 完成 |
| 兼容影响列全（CLI/持久格式/Rust 库/资源模型） | §11 | 完成 |
| 源码入口与测试协议 | §10.1、§7.6、§9.2 | 完成 |
| 正反例 | 各决策末节 | 完成 |
| 测试矩阵与后续批次完成标准（DIAG/ANALYSIS/LSP/FMT/DBG） | §12 | 完成 |
| 破坏性变更最小决策请求 | §13（D-M20-1..4） | 2026-09-24 用户全部同意 |
| 未实现 API 未标为当前可用 | 页首状态声明、§11 末条、§14 第 5 条 | 完成 |
| 未决事项标阻塞 | §14（H20-01 阻塞于 D-M20-1..4） | 完成 |

### 修复前复现结果

不适用：H20-00 是设计冻结批次，无编译器/运行时改动，没有新增失败回归。

### 修复后结果

不适用。规格中所有 API 与行为均标注“未实现”，未写入 `implemented-features.md`、README 或示例。

### 实际运行命令与测试数量

- 本批只新增/修改 Markdown，未重跑测试套件。只读核对命令：
  - `git status --short`、`git rev-parse HEAD`；
  - 阅读 `crates/dolphin-lsp/src/lib.rs`、`crates/dolphin-source/src/{source,diagnostic,lexer}.rs`、
    `crates/dolphin-syntax/src/{ast,parser}.rs`、`crates/dolphin-hir/src/{modules,lower,monomorphize}.rs`、
    `crates/dolphin-package/src/{manifest,resolver,registry,cache,package}.rs`、
    `crates/dolphin-format/src/lib.rs`、`src/main.rs`、`tests/m19_app.rs`、`tests/cli.rs`、
    `.github/workflows/ci.yml`；
  - 规格中的相对链接均指向仓库内存在的文件。
- 未运行的检查：任何 M20 实现的编译/运行/协议测试；D-M20 确认后的行为冻结复核；远端 CI。

### 未运行的检查及原因

- 任何 M20 实现的编译/运行测试：本批不实现代码。
- LSP 协议会话、分析快照、formatter 保持性、调试器实测：对应 H20-01..05，尚未派发。
- 远端 CI：本批未 push/tag；纯文档改动。

### 行为/兼容变化

- 无产品行为变化。文档层面：
  - 新增 `docs/proposal-m20-project-tools.md`，明确所有 M20 API 未实现；
  - `docs/plan-m18-plus.md` H20-00 状态改为“完成（规格已产出，D-M20-1..4 待确认，H20-01 阻塞）”，
    M20 节补充状态说明；
  - 本报告新增。
- 公开行为变化（D-M20-1..4）在规格第 13 节列出，已于 2026-09-24 确认；本批仍未实现，
  不作为当前行为。

### 剩余问题和下一批输入（H20-01）

1. **H20-01 已解阻、等待人工派发**：D-M20-1（LSP 未知方法/未初始化错误码）、D-M20-2
   （未 shutdown 的 `exit` 退出码）、D-M20-3（LSP message 结构化）、D-M20-4（`dc fmt` 发现
   与全有或全无）已于 2026-09-24 由用户全部同意；收到批次指令前不开始编码。
2. 收到确认与 H20-01 派发指令后的输入：规格 §3–§5、§10、§12.1；计划第 6 节的
   “H20-01：诊断”段；新增测试文件 `tests/m20_diag.rs` 需同步加入 CI 显式 `--test` 列表。
3. H20-02 之前不得创建 `dolphin-analysis`；H20-01 只做 `dolphin-source`/`dolphin-syntax`/
   `dolphin-hir`/`dolphin-driver`/`dolphin-lsp` 的诊断数据与恢复，不实现项目分析。
4. H20-05 的 gdb/lldb 可用性需在派发前核实；缺失时该项标未验证，不用其他平台结果代替。
5. 本报告不把 M20 记为完成；也不把规格中任何 API 视为当前可用。
