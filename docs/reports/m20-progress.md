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

## H20-01 结构化诊断与有限恢复

- 批次：H20-01
- 状态：**实现完成，Linux 本机默认 lane 与 Linux LLVM lane 通过；Windows/macOS 默认 lane 与远端 CI 未验证**
  （§12.1 完成标准要求三平台，本机只跑 Linux；未验证项见下）
- 前置批次及报告：H20-00 / 本文件上一节；规格 D-M20-1..4 已由用户确认

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`ca91fe011b0d30ab382c14912714511c3d3ae55c`（`v0.3.0-M20-00`）；工作区干净。
- 无用户未提交改动被覆盖；未 commit/push/tag。

### 本批修改范围（源码，非文档）

| 范围 | 文件 |
| --- | --- |
| `SourceId`/`SourceFile.id`/`with_id`/`SourceMap`；`Span` 增加 `Hash` | `crates/dolphin-source/src/source.rs` |
| `Severity`/`Label`/结构化 `Diagnostic` 与访问器；`plain`/`at` 渲染逐字节不变；`DIAGNOSTIC_LIMIT`/`push_capped` | `crates/dolphin-source/src/diagnostic.rs` |
| `lex_recovering`（未知字符跳过；未闭合字符串/字符跳行尾；未闭合块注释跳 EOF；100 条 + `E0002`） | `crates/dolphin-source/src/lexer.rs` |
| `parse_recovering`（块内/顶层同步点、多余 `}` 报错跳过、100 条 + `E0002`） | `crates/dolphin-syntax/src/parser.rs` |
| `LoadedSources`/`SourceProvider`/`DiskProvider`/`normalize_path`/`load_packages_collecting`/`load_packages_with_provider`；`load_packages` 改为内部走收集式 | `crates/dolphin-hir/src/modules.rs` |
| `lower_sources_analysis_collecting`/`lower_collecting`；`build_templates_collecting`；错误消息可读类型名 `MonoState::display_type` | `crates/dolphin-hir/src/lower.rs`、`crates/dolphin-hir/src/monomorphize.rs` |
| `check_source_collecting`/`check_manifest_collecting`；跨目标去重 | `crates/dolphin-driver/src/lib.rs` |
| `dc check/build/run/test` 编译前收集式检查并打印全部诊断；退出码 1 | `src/main.rs` |
| LSP 结构化诊断映射（code/severity/message/range/relatedInformation）；`analyze` 走 recovering + 收集式 lower | `crates/dolphin-lsp/src/lib.rs` |
| DIAG-01..06 集成验收 | `tests/m20_diag.rs`（新增） |
| CI/README 显式 `--test` 列表加入 `m20_diag`；根包新增 dev-dependency `serde_json` | `.github/workflows/ci.yml`、`README.md`、`Cargo.toml`、`Cargo.lock` |

### 验收映射（真实测试名与固定期望）

| 验收 | 测试（`tests/m20_diag.rs`） | 固定期望（断言内容） | 结果 |
| --- | --- | --- | --- |
| DIAG-01 | `diag_01_cli_and_lsp_same_error_identity` | CLI 首行 `error[E0001]: unknown variable \`missing\``、`--> <file>:1:20`、exit 1；LSP `code=E0001`、`message=unknown variable \`missing\``、UTF-16 range `(0,19)-(0,26)` | 通过 |
| DIAG-02 | `diag_02_two_files_independent_errors` | CLI 同时含 a.do `expected \`}\` after block` 与 b.do `unknown type \`Missing\``、exit 1；LSP 两个 URI 各 1 条同 code/message | 通过 |
| DIAG-03 | `diag_03_non_bmp_and_crlf_positions` | CLI `2:12` 与 `3:12`（CRLF、非 BMP 前后）、无 `panicked`；LSP 两条 range 起点 `(1,11)`/`(2,11)` | 通过 |
| DIAG-04 | `diag_04_generic_chain_and_user_type_names` | CLI 消息 `expected \`std.collections.Vec<std.Result<i32, myerr.error.MyError>>\`, found \`i32\``，无 `TypeId(`/`struct@`/`enum@`；LSP 单文件同格式 | 通过 |
| DIAG-05 | `diag_05_error_program_no_panic_no_artifact` | `dc build`/`dc run` exit 1、`target/` 不存在、stderr 无 `panicked` | 通过 |
| 上限 | `diag_06_error_collection_bound` | 词法 120 个非法字符 → 恰好 100 条 `E0001` + 1 条 `E0002`；语法 120 条坏语句同；声明级 120 个非法 impl 同；均无 `panicked` | 通过 |

补充单元测试（验收矩阵外的正反例/边界）：

| crate | 测试 | 断言 |
| --- | --- | --- |
| dolphin-source | `plain_and_at_render_unchanged` | `plain`/`at` 输出与 M19 逐字节一致；label[0]=primary、`ANONYMOUS` |
| dolphin-source | `secondary_labels_and_notes_append_lines` | `with_label`/`with_note`/`with_code` 的 `= note:` 行与 code 前缀替换 |
| dolphin-source | `at_snaps_mid_character_spans` | 非字符边界 span 向下吸附、不 panic |
| dolphin-source | `recovering_lexes_past_unknown_characters` 等 4 个 | 未知字符/未闭合字符串/块注释恢复；`lex` 仍首错返回 |
| dolphin-source | `source_map_resolves_ids_positions_and_offsets` / `source_map_never_guesses_anonymous_files` | 文件/位置/偏移换算；`ANONYMOUS` 不猜文件（§3.4） |
| dolphin-syntax | `recovering_collects_independent_statement_errors` | 同块两个独立语句错误都收集，函数仍解析 |
| dolphin-syntax | `recovering_syncs_to_next_top_level_item` / `recovering_reports_stray_closing_brace` | 顶层同步与多余 `}` 报错 |
| dolphin-syntax | `parse_keeps_first_error_behavior` | `parse` 首错文本不变 |
| dolphin-syntax | `recovering_never_panics_on_garbage` / `recovering_is_panic_free_on_random_input` | 固定样例 + 500 条确定性伪随机输入不 panic |
| dolphin-hir | `collecting_lowering_reports_independent_declarations` | 两个独立 impl 头错误全部返回（`Vec` 长 2） |
| dolphin-hir | `collecting_lowering_keeps_first_body_error` | 函数体仍首错即停（`Vec` 长 1） |
| dolphin-hir | `collecting_loader_keeps_source_ids_aligned` | `SourceId.0` == `sources` 下标；`asts`/`packages` 对齐 |
| dolphin-hir | `collecting_loader_collects_multiple_file_diagnostics` | 跨文件词法 + pkg 校验各一条；词法失败文件 `asts=None`，pkg 失败保留 AST |
| dolphin-hir | `load_packages_with_provider_uses_provider_text` | overlay 文本优先于磁盘（磁盘无文件） |
| dolphin-hir | `normalize_path_is_lexical_only` | `.`/`..` 词法折叠，不解析符号链接 |
| dolphin-lsp | `structured_diagnostics_use_code_message_and_utf16_range` | `code`/纯 `message`/UTF-16 range/severity/source |
| dolphin-lsp | `related_information_maps_secondary_labels` | secondary label → `relatedInformation` 的 uri/range/message |
| dolphin-lsp | `anonymous_diagnostic_without_map_entry_gets_zero_range` | §3.4 反例：`ANONYMOUS` 不猜文件，返回 `(0,0)-(0,0)` |

### 实际运行命令与结果

环境：Linux x86_64（`Linux AppServer 7.2.3-zen1-2-zen`），rustc/cargo 1.97.1，LLVM 22.1.8（`llvm-config` 在 PATH）。

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过 |
| `cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings` | 通过 |
| `cargo test --workspace --exclude dolphin-codegen-llvm` | 52 个测试二进制共 396 passed；0 failed |
| `cargo test -p dolphin-compiler --test m20_diag` | 6 passed；0 failed |
| `cargo build --bins --features llvm` | 通过 |
| `cargo clippy --workspace --all-targets --features llvm -- -D warnings` | 通过 |
| `DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm` | 54 个二进制共 403 passed；0 failed |
| `DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm --test build --test ffi --test cli --test manifest --test packages --test doc_examples --test m19_args --test m19_io --test m19_fs --test m19_text --test m19_test_cmd --test m19_errors --test m19_app --test m20_diag` | 14 个二进制共 202 passed；0 failed（含 m20_diag 6 passed） |
| `cargo test -p dolphin-compiler --features llvm --test backend` | 4 passed（`debug_backends_agree`/`release_backends_agree`/`llvm_debug_profile_emits_dwarf`/`typed_ir_has_no_backend_types`） |

补充手工核对（`target/debug/dc`，Linux）：`dc check <file>`/`dc build <project>` 的错误输出与退出码、`dc run` 拦截、`target/` 不生成等已在 `tests/m20_diag.rs` 断言中固化。

### 行为/兼容变化

- **CLI**：`dc check/build/run/test` 现在编译前收集并打印多条词法/语法/声明级诊断；首条诊断 `Display` 文本与退出码 1 不变，成功输出不变。`dc build --lib`/`--bin` 也会先检查清单中全部目标（§5.5）。
- **Rust 库 API（增量）**：`SourceId`/`SourceFile.id`/`with_id`/`SourceMap`；`Diagnostic` 新增结构化字段与 `error`/`with_label`/`with_note`/`with_code`/访问器（`plain`/`at` 签名与 `Display` 不变）；`lex_recovering`/`parse_recovering`；`LoadedSources`/`SourceProvider`/`load_packages_collecting`/`load_packages_with_provider`；`lower_sources_analysis_collecting`/`lower_collecting`；driver `check_source_collecting`/`check_manifest_collecting`。`Diagnostic` 体积增至 120 字节（`code: Box<str>` 以满足 clippy `result_large_err`）。
- **错误消息**：涉及用户类型的错误改用可读限定名与泛型实参（如 `std.collections.Vec<std.Result<i32, myerr.error.MyError>>`），不再输出 `struct@N`/`enum@N`。
- **未改变**：`dolphin.toml`/`dolphin.lock`/`.dlib` 无格式变化；`dc lsp` 协议生命周期（D-M20-1/2）与项目分析（H20-02/03）未实现；`lex`/`parse`/`load_packages`/`lower_sources`/`lower_library` 签名与首错语义不变。

### 未验证项（不得当作通过）

1. **Windows/macOS 默认 lane**：本机只跑 Linux；`diag_01..06` 未在 Windows/macOS 运行（含 Windows 盘符/反斜杠路径、CRLF 平台差异）。
2. **远端 CI**：未 push/tag，未运行 GitHub Actions。
3. **真实 stdio LSP 协议会话**：DIAG-01..05 的 LSP 断言使用 `dolphin_lsp::Server::handle` 进程内消息处理；`Content-Length` 帧与退出码协议测试属 H20-03 的 `tests/m20_lsp.rs`。
4. **H20-02/03/04/05**：`dolphin-analysis`、项目级 LSP、formatter、调试器均未实现。

### 与冻结规格的差异（需 H20-02 处理；已在 H20-02 处理）

1. `lower_sources_analysis_collecting` 本批返回 `Result<ir::Program, Vec<Diagnostic>>`，不是 §5.4 冻结的 `Result<LoweredProgram, Vec<Diagnostic>>`：`LoweredProgram`/`AnalysisData`（§6.7）依赖 `DefinitionData`/`DefKind`/`SymbolIndex`（§6.6，H20-02）。为避免提前实现下一批，本批只做诊断与恢复；H20-02 引入 side table 时升级返回类型。
2. `parse_recovering` 顶层同步点除规格列出的 `fn/struct/enum/trait/impl/use/pkg/EOF` 外，加入本语法实际顶层项起始 `extern`/`pub`（否则会跳过整个项）。
3. `DiskProvider` 按 §5.3 提供（规格只点名语义，未列类型名）；`normalize_path` 目前只用于 `exclude` 比较，不改写 `SourceFile.path`。

### 剩余问题和下一批输入（H20-02）

1. H20-02 输入：规格 §6、§10.1、§12.2；`tests/m20_analysis.rs` 需同步加入 CI/README 显式 `--test` 列表；新增 `crates/dolphin-analysis` 与 workspace 成员。
2. H20-02 需消费本批的 `LoadedSources`/`SourceProvider`/`load_packages_with_provider`，实现 overlay 与 `resolve_readonly`（`E1001`），并把 `lower_sources_analysis_collecting` 升级为 `LoweredProgram`。
3. H20-03 需把 `dc lsp [PROJECT]` 与协议行为（D-M20-1/2）、`tests/support/lsp.rs` 落地；本批刻意保留 `unknown_request_returns_null` 旧行为。
4. H20-05 前需核实 gdb/lldb 可用性；本机未检查调试器。
5. 本报告不把 M20 记为完成；H20-01 状态以三平台 CI 结果为准。

## H20-02 共享项目分析接口与文件 overlay

- 批次：H20-02
- 状态：**实现完成，Linux 本机默认 lane 与 Linux LLVM lane 通过；Windows/macOS 默认 lane 与远端 CI 未验证**
  （§12.2 完成标准含 Windows 盘符用例；本机只跑 Linux，盘符分支以平台无关单测覆盖，见下）
- 前置批次及报告：H20-01 / 本文件上一节；规格 §6、§10.1、§12.2

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`3aaf7696b9da7bd43a2754e20e3091886d531156`（`v0.3.0-M20-01`）；工作区干净。
- 无用户未提交改动被覆盖；未 commit/push/tag。

### 本批修改范围（源码，非文档）

| 范围 | 文件 |
| --- | --- |
| `LoweredProgram`/`AnalysisData`/`DefinitionData`/`DefinitionKind`；`lower_sources_analysis`；`lower_sources_analysis_collecting` 升级返回 side table；`lower_sources`/`lower_library` 只返回 `.program`；定义收集（函数/类型/trait/方法/字段/variant/类型参数） | `crates/dolphin-hir/src/lower.rs` |
| `MonoState.instance_keys`（与 `FunctionId` 对齐的实例 key） | `crates/dolphin-hir/src/monomorphize.rs` |
| `package_sources_for_graph`（从 driver 移到 HIR，构建与分析共用，避免选择规则漂移） | `crates/dolphin-hir/src/modules.rs`、`crates/dolphin-driver/src/lib.rs` |
| `resolve_readonly` + `ReadonlyRemote`（offline、只读锁、缓存恢复、`E1001`/未知仓库 `E1002`） | `crates/dolphin-package/src/resolver.rs` |
| `TypeId`/`FunctionId` 增加 `PartialOrd`/`Ord`（side table `BTreeMap` 需要；不影响布局/ABI） | `crates/dolphin-ir/src/ir.rs` |
| 新 crate `dolphin-analysis`：`host.rs`（overlay/revision/快照/单元选择/诊断合并）、`index.rs`（定义/解析/签名/文档符号）、`uri.rs`（`file://` 编解码） | `crates/dolphin-analysis/`（新增） |
| ANALYSIS-01..06 集成验收 | `tests/m20_analysis.rs`（新增） |
| CI/README 显式 `--test` 列表加入 `m20_analysis`；workspace/根包 dev-dependency 接线 | `.github/workflows/ci.yml`、`README.md`、`Cargo.toml`、`Cargo.lock` |

### 验收映射（真实测试名与固定期望）

| 验收 | 测试（`tests/m20_analysis.rs`） | 固定期望（断言内容） | 结果 |
| --- | --- | --- | --- |
| ANALYSIS-01 | `analysis_01_no_network_no_lock_write_no_artifacts` | HTTP 依赖无锁无缓存 → `partial=true`、`units=[]`、唯一项目诊断 `code=E1001` 且消息含坐标与 `run \`dc fetch\` and retry`；回环 listener `accept()` 为 `WouldBlock`（零连接）；项目文件树逐字节不变、无 `dolphin.lock`、无 `target/`；路径依赖 + `dc fetch` 锁 → 快照成功且锁/文件不变 | 通过 |
| ANALYSIS-02 | `analysis_02_unsaved_dependency_affects_caller` | overlay 依赖把 `count(): i32` 改为 `bool` → 调用方恰好 1 条 `E0001` `expected \`i32\`, found \`bool\``，primary 路径为 app 的 `main.do`；stale version 返回 `false`；`remove_overlay` 后诊断清空、revision=2 | 通过 |
| ANALYSIS-03 | `analysis_03_library_without_main` | lib-only 项目：1 个 `UnitKind::Lib`、`partial=false`、无任何诊断、`index=Some`（不误报缺 main）；bin 缺 main → `partial=true` 且项目诊断含 `program does not define \`main\`` | 通过 |
| ANALYSIS-04 | `analysis_04_multi_bin_and_custom_source` | `[package].source = "code"` + lib + 两 bin：单元顺序 `[Lib, Bin(first), Bin(second)]`；共享文件 `unit_for_path` 取 Lib；lib 排除两 bin 入口，每个 bin 排除另一个；各单元均有符号索引 | 通过 |
| ANALYSIS-05 | `analysis_05_same_name_different_package_not_confused` | 两个 path 依赖的同名模块 `mod.inner.value`/`Point`：调用点解析为不同 `DefId`（包不同）、签名分别 `fn left.mod.inner.value() -> i32` / `fn right.mod.inner.value() -> i32`、定义 source 不同；`render_type` 为 `left.mod.inner.Point` / `right.mod.inner.Point` | 通过 |
| ANALYSIS-06 | `analysis_06_cli_build_behavior_unchanged` | 快照零写入/零 `target/`；`dc check` 输出 `Checked g:app:0.1.0`；`dc build` 成功且生成 Debug 产物；`dc run` 退出码 42、stderr 空；显式 `dc build --release` 与 `dc run --release` 同样退出 42 | 通过 |

补充单元测试（验收矩阵外的正反例/边界）：

| crate | 测试 | 断言 |
| --- | --- | --- |
| dolphin-analysis | `stale_overlay_is_ignored_and_revision_advances` | 同 version/旧 version 返回 false 且 revision 不变；新 version/remove 各 +1；重复 remove 返回 false |
| dolphin-analysis | `snapshot_is_cached_per_revision` | 同 revision 返回同一 `Arc`；overlay 后 revision+1 且生成新快照 |
| dolphin-analysis | `overlay_only_new_file_participates_in_project_analysis` | 仅存在于 overlay 的 `src/helper.do` 参与项目分析并消除 `unknown function`；didClose 后从发现集合移除、诊断恢复 |
| dolphin-analysis | `missing_manifest_is_single_file_mode` | 无清单 → `AnalysisMode::SingleFile`、无单元、无诊断、非 partial |
| dolphin-analysis | `unit_selection_prefers_lib_then_manifest_order` | 单元顺序与 Lib > Bin 选择；入口排除 |
| dolphin-analysis | `decodes_percent_escapes_and_unicode` / `windows_drive_letters_map_to_backslash_paths` / `rejects_non_file_scheme_backslashes_and_bad_escapes` / `path_to_uri_round_trips_spaces_and_unicode` | 空格/中文/盘符解码、反斜杠与非 `file` scheme 拒绝、编码往返（Windows 分支以显式参数在 Linux 上断言） |
| dolphin-analysis | `resolves_parameters_locals_for_vars_and_shadowing` | 参数/`val`/`for` 变量解析到最近绑定、内层遮蔽外层、限定函数调用解析为定义 |
| dolphin-analysis | `match_bindings_resolve_to_declaration_spans` | match 绑定解析到模式内绑定 token 的真实 span；模式名解析到枚举定义 |
| dolphin-analysis | `definitions_signatures_documents_and_instances` | `struct Point`/`fn helper(i32) -> i32`/`x: i32` 签名、`document_symbols` 名称与顺序、`Pair<i32>` 实例 `render_type` |
| dolphin-analysis | `unresolved_names_stay_unresolved` | 字段访问与函数名声明位置不被猜测为定义 |
| dolphin-package | `readonly_reports_e1001_without_writing_lock` | 固定完整 `E1001` 文本；不写 `dolphin.lock` |
| dolphin-package | `readonly_resolves_path_dependencies_without_writing_lock` | 路径依赖图解析成功且不写锁 |
| dolphin-hir | `analysis_side_table_aligns_with_type_and_function_ids` | `type_names.len()==types.len()`、`function_instances.len()==functions.len()` 且逐项对应；定义含 `Pair`/`Pair.T`/`id`/`main`；`Pair<i32>` 实例 key 正确 |

### 实际运行命令与结果

环境：Linux x86_64（`Linux AppServer 7.2.3-zen1-2-zen`），rustc/cargo 1.97.1，LLVM 22.1.8（`llvm-config` 在 PATH）。

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过 |
| `cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings` | 通过 |
| `cargo test --workspace --exclude dolphin-codegen-llvm` | 55 个 test harness（含 doc-tests）共 418 passed；0 failed |
| `cargo test -p dolphin-compiler --test m20_analysis` | 6 passed；0 failed |
| `cargo build --bins --features llvm` | 通过 |
| `cargo clippy --workspace --all-targets --features llvm -- -D warnings` | 通过 |
| `DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm` | 57 个 test harness（含 doc-tests）共 425 passed；0 failed |
| `DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm --test build --test ffi --test cli --test manifest --test packages --test doc_examples --test m19_args --test m19_io --test m19_fs --test m19_text --test m19_test_cmd --test m19_errors --test m19_app --test m20_diag --test m20_analysis` | 15 个二进制共 208 passed；0 failed（含 `m20_analysis` 6 passed） |
| `cargo test -p dolphin-compiler --features llvm --test backend` | 4 passed（`debug_backends_agree`/`release_backends_agree`/`llvm_debug_profile_emits_dwarf`/`typed_ir_has_no_backend_types`） |

`TypeId`/`FunctionId` 只增加 derive（无布局/ABI 语义变化），仍按“改 IR 同时检查两个后端”执行了默认与 LLVM 两套 lane 及 `backend` 对照测试。

### 行为/兼容变化

- **新增 Rust 库 API（增量）**：
  - `dolphin-hir`：`LoweredProgram`/`AnalysisData`/`DefinitionData`/`DefinitionKind`；`lower_sources_analysis`；
    `lower_sources_analysis_collecting` 返回类型由 `Result<ir::Program, Vec<Diagnostic>>` 升级为
    `Result<LoweredProgram, Vec<Diagnostic>>`（H20-01 已声明的差异，本批按 §5.4/§6.7 补齐；
    仓库内调用方均忽略 `Ok` 值，无行为变化）；`modules::package_sources_for_graph`；
    `MonoState.instance_keys`。
  - `dolphin-package`：`resolver::resolve_readonly`。
  - `dolphin-analysis`（新 crate）：`AnalysisHost`/`AnalysisSnapshot`/`AnalysisUnit`/`AnalysisMode`/`UnitKind`；
    `SymbolIndex`/`SymbolId`/`DefId`/`DefKind`/`Definition`/`DocumentSymbol`/`Resolution`/`ResolutionEntry`；
    `uri_to_path`/`path_to_uri`；增量 helper `AnalysisHost::with_cache`/`with_cache_root`/`overlay_version`、
    `AnalysisUnit::source_id_for_path`/`source_for_path`、`AnalysisSnapshot::unit_for_path`（供 H20-03）。
  - `dolphin-ir`：`TypeId`/`FunctionId` 增加 `PartialOrd`/`Ord`。
- **CLI/持久格式**：无变化。`dc check/build/run/test/fetch/publish` 仍走现有网络/写锁逻辑（ANALYSIS-06）；
  `dolphin.toml`/`dolphin.lock`/`.dlib` 无字段或格式变化；分析路径不下载、不写锁、不产生产物。
- **未改变**：`lex`/`parse`/`load_packages`/`lower_sources`/`lower_library`/`resolve_project` 签名与语义；
  `dc lsp` 协议行为（H20-03 才改 D-M20-1/2）；`dolphin-lsp` 仍直接依赖 `dolphin-hir`（依赖切换到
  `dolphin-analysis` 属 H20-03）。

### 与冻结规格的差异

1. `dolphin-analysis` 除规格 §6.1 列出的四个 crate 外还依赖 `dolphin-ir`：冻结的 `SymbolIndex`
   签名使用 `ir::TypeId`/`ir::FunctionId`/`ir::Type`，必须直接依赖；未违反“不得依赖 driver/codegen/
   linker/platform”的禁止项。
2. `DefKind` 无 `Impl` 变体（规格 §6.6 冻结集合）；`document_symbols` 因此只返回函数/结构体/枚举/trait，
   impl 的文档符号显示留待 H20-03 按现有 LSP 行为处理（§7.5）。
3. `Resolution::Instance` 变体已实现但当前解析遍历不会产生：表达式级类型推断不在 M20 范围；
   泛型实例身份通过 `type_names`/`function_instances` 暴露。
4. 合并 AST 不保留 `uses`（`merge_units` 现状），因此 `use` 声明路径 token 本身不产生 `ResolutionEntry`；
   使用点的函数/类型/模块路径按 loader 已限定名解析（`modules::resolve_modules` 产物）。
5. `AnalysisHost::snapshot` 的单文件模式只返回 `mode=SingleFile` 空快照；打开文档的单文件语法分析
   仍由 LSP 层执行（§7.1，H20-03）。

### 未验证项（不得当作通过）

1. **Windows/macOS 默认 lane**：本机只跑 Linux；`analysis_01..06` 未在 Windows/macOS 运行。
   盘符/反斜杠 URI 分支以 `uri_to_path_impl(..., windows)` 单测在 Linux 上断言，但不等于真实平台验证。
2. **远端 CI**：未 push/tag，未运行 GitHub Actions。
3. **真实 stdio LSP 会话**：本批只提供分析 API；协议层（`tests/m20_lsp.rs`、`tests/support/lsp.rs`）属 H20-03。
4. **H20-03/04/05**：项目级 LSP、formatter、调试器均未实现。
5. 能力声明未更新：`implemented-features.md` 未新增 `dolphin-analysis`/`resolve_readonly` 条目，README 只更新了开发验证命令列表；按 §11，M20 全部批次完成前不把这些 API 写入当前能力文档。

### 剩余问题和下一批输入（H20-03）

1. H20-03 输入：规格 §7、§10.1、§12.3；`dc lsp [PROJECT]` 与 D-M20-1/2 协议行为；新增
   `tests/support/lsp.rs` 与 `tests/m20_lsp.rs` 并加入 CI/README 显式 `--test` 列表。
2. H20-03 需把 `dolphin-lsp` 依赖从 `dolphin-hir` 切换为 `dolphin-analysis`，删除文本同名查找回退，
   使用 `SymbolIndex::resolve`/`definition_of`/`document_symbols`/`render_type`；发布诊断带 overlay version
   （`AnalysisHost::overlay_version`/`AnalysisSnapshot.revision`）。
3. 单文件模式（无清单）与打开文档的单文件诊断、`window/showMessage` 项目诊断发布规则见 §7.1/§7.3/§7.4。
4. H20-05 前需核实 gdb/lldb 可用性；本机未检查调试器。
5. 本报告不把 M20 记为完成；H20-02 状态以三平台 CI 结果为准。

## H20-03 项目诊断、符号绑定与定义导航

- 批次：H20-03
- 状态：**实现完成，Linux 本机默认 lane 与 Linux LLVM lane 通过；Windows/macOS 默认 lane 与远端 CI 未验证**
  （§12.3 完成标准要求三平台，本机只跑 Linux；未验证项见下）
- 前置批次及报告：H20-02 / 本文件上一节；规格 §7、§10.1、§12.3；D-M20-1/2/3 已确认

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`47c4faece2bb6d3ff137443fc2a3874c9a445f25`（`v0.3.0-M20-02`）；工作区干净。
- 无用户未提交改动被覆盖；未 commit/push/tag。

### 本批修改范围（源码，非文档）

| 范围 | 文件 |
| --- | --- |
| 单文件 loader 公开入口 `load_single_source`（保留调用方 `SourceId`，注入内建标准库） | `crates/dolphin-hir/src/modules.rs` |
| `SingleFileAnalysis`/`analyze_single_file`（§7.1 单文件规则 + 语义成功时符号索引） | `crates/dolphin-analysis/src/single.rs`（新增）、`lib.rs` |
| `SymbolIndex::display_name`/`local_type`；参数/`val`/`var` 类型标注收集 | `crates/dolphin-analysis/src/index.rs` |
| LSP 重写：`dc lsp [PROJECT]` 项目/单文件分析、协议状态机（-32002/-32601/-32600、退出码）、按 overlay 版本发布诊断、`window/showMessage`、URI 排序、基于 `SymbolIndex` 的 hover/definition、保留 `documentSymbol` | `crates/dolphin-lsp/src/lib.rs` |
| `dolphin-lsp` 依赖从 `dolphin-hir` 切换为 `dolphin-analysis` | `crates/dolphin-lsp/Cargo.toml`、`Cargo.lock` |
| `dc lsp [PROJECT]` 参数与退出码接线 | `src/main.rs` |
| 真实 stdio JSON-RPC 会话驱动 | `tests/support/lsp.rs`（新增）、`tests/support/mod.rs` |
| LSP-01..06 与 4 个补充边界用例 | `tests/m20_lsp.rs`（新增） |
| 进程内 LSP helper 增加 `initialize` 握手（断言不变；通知在 initialize 前被忽略） | `tests/m20_diag.rs` |
| CI/README 显式 `--test` 列表加入 `m20_lsp`；README CLI 用法与 LSP 描述更新 | `.github/workflows/ci.yml`、`README.md` |

### 验收映射（真实测试名与固定期望）

| 验收 | 测试（`tests/m20_lsp.rs`） | 固定期望（断言内容） | 结果 |
| --- | --- | --- | --- |
| LSP-01 | `lsp_01_pkg_use_type_error` | 打开含 `use stats.count` 的 bin：`publishDiagnostics` 带 `version=1`、恰好 1 条 `code=E0001`、`message=expected \`bool\`, found \`i32\``、severity=1、source=dolphin、range 覆盖 `count()`（行/列由 fixture 计算并断言起止） | 通过 |
| LSP-02 | `lsp_02_cross_file_and_cross_package_definition` | `helper()` definition → 根包 `src/util.do` 的 `name_span`（0:7-0:13）；`count()` definition → path 依赖 `stats/src/lib.do`（0:7-0:12）；hover 分别为 ``fn `helper` `` 与 ``fn `stats.count` `` | 通过 |
| LSP-03 | `lsp_03_local_shadowing_and_parameters` | 内层 `x` 使用解析到内层绑定、外层 `return x` 解析到外层绑定；参数 `value` definition 指向参数 `name_span`，hover 为 ``local `value`: i32`` | 通过 |
| LSP-04 | `lsp_04_open_change_close_dependency_change` | 依赖 didOpen/overlay 改 `bool` 后调用方发布 `version=1` 的 `expected \`i32\`, found \`bool\``；didClose 先发该 URI 空诊断（无 version）再恢复调用方空诊断；一次 overlay 变化同时改变两文档诊断时按 URI 字典序升序发布 | 通过 |
| LSP-05 | `lsp_05_sequential_versions_latest_wins` | 连续 didChange 2→3 各发布对应 version 与最新文本诊断（`missing1`→`missing2`→空）；旧 version=2 再变更被忽略，`documentSymbol` 仍为 version 3 的 `[main, extra]` | 通过 |
| LSP-06 | `lsp_06_unknown_request_lifecycle_conformance` | initialize 前请求 `-32002`；未知请求 `-32601` + `Method not found`；未知通知被忽略；shutdown `result:null`；shutdown 后请求 `-32600`；已 shutdown `exit`=0、未 shutdown `exit`=1、EOF=0 | 通过 |

补充边界用例（同一文件，验收矩阵外）：

| 测试 | 固定期望 | 结果 |
| --- | --- | --- |
| `lsp_accepts_no_project_argument` | 无位置参数 `dc lsp` 可 initialize（`positionEncoding=utf-16`）并正常退出 0 | 通过 |
| `lsp_non_file_uri_stays_single_file` | `untitled:` 文档发布单文件 `E0001 unknown variable \`missing\``；definition 返回 `null`（不回退文本同名查找） | 通过 |
| `lsp_project_failure_keeps_syntax_diagnostics` | HTTP 依赖不可本地恢复：`window/showMessage` type=1 含坐标与 `run \`dc fetch\` and retry`，文档仍发布 `E0001 expected \`}\` after block`；不写 `dolphin.lock`、不产 `target/` | 通过 |
| `lsp_malformed_frame_terminates_with_error` | 超限 `Content-Length` 终止服务，stderr 含 `exceeds the 16777216 byte limit`，退出码 1 | 通过 |

补充单元测试：

| crate | 测试 | 断言 |
| --- | --- | --- |
| dolphin-analysis | `single::tests::lexical_and_syntax_errors_are_collected_without_index` | 词法/语法错误收集且无索引 |
| dolphin-analysis | `single::tests::semantic_error_keeps_stdlib_sources_without_index` | `unknown variable \`missing\``、无索引、已注入 stdlib 且 `sources[0].id` 保留 |
| dolphin-analysis | `single::tests::valid_program_yields_index_and_resolutions` | 调用解析为 `helper` 定义、`name_span` 正确、参数 `local_type=i32` |
| dolphin-analysis | `single::tests::pkg_use_and_missing_main_skip_semantics` | `pkg`/无 `main` 时无诊断且无索引 |
| dolphin-lsp | `requests_before_initialize_report_not_initialized` / `requests_after_shutdown_report_invalid_request` / `exit_without_shutdown_is_failure` | `-32002`/`-32600` 与退出码状态机 |
| dolphin-lsp | `unknown_request_returns_method_not_found` | `-32601` 且无 `result`（替换 M17 `unknown_request_returns_null`） |
| dolphin-lsp | `hover_definition_kinds_use_display_names` | struct hover ``struct `Point` ``；未解析字段访问返回 `null` |
| dolphin-lsp | `hover_local_reports_type_annotation` | 参数使用 hover ``local `value`: i32`` 与 token range |
| dolphin-lsp | `initialize_reports_capabilities` | capabilities 含 `positionEncoding=utf-16` |

### 实际运行命令与结果

环境：Linux x86_64（`Linux AppServer 7.2.3-zen1-2-zen`），rustc/cargo 1.97.1，LLVM 22.1.8（`llvm-config` 在 PATH）。

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过 |
| `cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings` | 通过 |
| `cargo test --workspace --exclude dolphin-codegen-llvm` | 56 个 test harness 共 437 passed；0 failed |
| `cargo test -p dolphin-compiler --test m20_lsp` | 10 passed；0 failed（LSP-01..06 + 4 边界） |
| `cargo test -p dolphin-analysis` | 17 passed；0 failed（含 4 个 `single` 单测） |
| `cargo test -p dolphin-lsp` | 15 passed；0 failed |
| `cargo test -p dolphin-compiler --test m20_diag` | 6 passed；0 failed（helper 握手更新，断言不变） |
| `cargo build --bins --features llvm` | 通过 |
| `cargo clippy --workspace --all-targets --features llvm -- -D warnings` | 通过 |
| `DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm` | 58 个 harness 共 444 passed；0 failed |
| `DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm --test build --test ffi --test cli --test manifest --test packages --test doc_examples --test m19_args --test m19_io --test m19_fs --test m19_text --test m19_test_cmd --test m19_errors --test m19_app --test m20_diag --test m20_analysis --test m20_lsp` | 16 个二进制共 218 passed；0 failed（含 `m20_lsp` 10 passed） |
| `cargo test -p dolphin-compiler --features llvm --test backend` | 4 passed |

另以临时项目手工核对过跨文件/跨包 definition、依赖 overlay 变更、stale version 忽略与 `documentSymbol` 回读；结果已固化为上述测试断言。

### 行为/兼容变化

- **CLI**：`dc lsp [PROJECT]` 新增可选位置参数（缺省当前目录）；无参数调用与 stdio 传输不变。退出码按 D-M20-2：已 `shutdown` 后 `exit`/EOF 为 0，未 `shutdown` 的 `exit` 为 1；未知方法由 `result:null` 改为 `-32601`，initialize 前请求为 `-32002`，shutdown 后请求为 `-32600`（D-M20-1）。
- **LSP**：`publishDiagnostics` 只针对打开文档发布，带文档 `version`；`didClose` 立即发空诊断；项目级诊断经 `window/showMessage`（type=1）；发布顺序为 URI 字典序；诊断 `message` 为纯消息文本并带 `code`/`relatedInformation`（D-M20-3，H20-01 已实现字段）。
- **导航**：hover/definition 使用 `SymbolIndex::resolve`/`definition_of` 与显示名，删除 M17 文本同名顶层查找；项目模式支持 `pkg`/`use`、lib/多 bin、path/缓存依赖源码、局部遮蔽/参数/`for`/match 绑定；无清单时按 §7.1 单文件规则分析，单文件语义成功时仍提供真实索引（M17 单文件导航不退化）。
- **Rust 库 API（增量）**：`dolphin-hir::modules::load_single_source`；`dolphin-analysis::SingleFileAnalysis`/`analyze_single_file`、`SymbolIndex::display_name`/`local_type`。`dolphin-lsp` 移除对 `dolphin-hir` 的直接依赖（§6.1）。
- **未改变**：`dolphin.toml`/`dolphin.lock`/`.dlib` 无格式变化；`dc check/build/run/test/fmt` 行为不变；`AnalysisHost`/`AnalysisSnapshot` 与 `lower_sources*` 签名不变。

### 与冻结规格的差异

1. 规格 §6.1 只要求 `dolphin-lsp` 依赖 `dolphin-analysis`；为删除文本同名回退又不让单文件 hover/definition 退化，新增
   `dolphin-analysis::analyze_single_file` 与 HIR 公开入口 `load_single_source`（规格 §6.6/§7.1 未列类型名）。
2. `SymbolIndex` 新增 `display_name`/`local_type` 访问器（§6.6 冻结方法列表外），分别供 §7.5 的 hover 显示名与局部类型标注；身份仍是 `SymbolId`。
3. 项目单元 `partial` 且 `index=None` 时，该文档的 hover/definition 回退到单文件分析（文档含 `pkg`/`use` 时按 §7.1 跳过语义）；规格未明确该回退。
4. `initialize` 之前的通知（含 didOpen）被忽略（LSP 严格行为）；§7.2 表只约束请求。因此 `tests/m20_diag.rs` 的进程内 helper 增加握手，所有断言文本不变。
5. `documentSymbol` 仍按当前文件文本解析（保留 M17 `collect_declarations`），未改用 `SymbolIndex::document_symbols`；§7.5 只要求顶层声明与现有 `kind`/`range` 取值。
6. `AnalysisHost` 单文件模式仍返回空快照（H20-02 差异 #5）；LSP 层按 §7.1 调用 `analyze_single_file`。

### 未验证项（不得当作通过）

1. **Windows/macOS 默认 lane**：本机只跑 Linux；`m20_lsp` 未在 Windows/macOS 运行。`path_to_uri`/`uri_to_path` 的盘符/反斜杠分支仅由 `dolphin-analysis` 单测在 Linux 上断言，不等于真实平台验证。
2. **远端 CI**：未 push/tag，未运行 GitHub Actions。
3. **H20-04/05**：`dc fmt` 发现与全有或全无、调试器实测未实现。
4. **能力文档**：`implemented-features.md`/`roadmap.md` 未更新（按 §11，H20-01..05 完成前不写入）；README 更新了 `dc lsp [项目目录]` 用法、测试列表与本批已实现的 LSP 描述，若需严格维持“不更新能力文档”的字面要求可在复核时回退。

### 剩余问题和下一批输入（H20-04）

1. H20-04 输入：规格 §8、§10.1、§12.4；新增 `tests/m20_fmt.rs` 并加入 CI/README 显式 `--test` 列表。
2. `dc fmt` 当前 `run_fmt` 仍是默认根 `src`、逐文件读取并写入；D-M20-4 的清单 `[package].source` 发现、排除 `build.output`/`.git`、全有或全无尚未实现。
3. README LSP 段落已按本批实现更新；复核若要求严格 §11 可回退该段与开发工具表项。
4. H20-05 前需核实 gdb/lldb 可用性；本机未检查调试器。
5. 本报告不把 M20 记为完成；H20-03 状态以三平台 CI 结果为准。
