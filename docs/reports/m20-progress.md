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

### 与冻结规格的差异（需 H20-02 处理）

1. `lower_sources_analysis_collecting` 本批返回 `Result<ir::Program, Vec<Diagnostic>>`，不是 §5.4 冻结的 `Result<LoweredProgram, Vec<Diagnostic>>`：`LoweredProgram`/`AnalysisData`（§6.7）依赖 `DefinitionData`/`DefKind`/`SymbolIndex`（§6.6，H20-02）。为避免提前实现下一批，本批只做诊断与恢复；H20-02 引入 side table 时升级返回类型。
2. `parse_recovering` 顶层同步点除规格列出的 `fn/struct/enum/trait/impl/use/pkg/EOF` 外，加入本语法实际顶层项起始 `extern`/`pub`（否则会跳过整个项）。
3. `DiskProvider` 按 §5.3 提供（规格只点名语义，未列类型名）；`normalize_path` 目前只用于 `exclude` 比较，不改写 `SourceFile.path`。

### 剩余问题和下一批输入（H20-02）

1. H20-02 输入：规格 §6、§10.1、§12.2；`tests/m20_analysis.rs` 需同步加入 CI/README 显式 `--test` 列表；新增 `crates/dolphin-analysis` 与 workspace 成员。
2. H20-02 需消费本批的 `LoadedSources`/`SourceProvider`/`load_packages_with_provider`，实现 overlay 与 `resolve_readonly`（`E1001`），并把 `lower_sources_analysis_collecting` 升级为 `LoweredProgram`。
3. H20-03 需把 `dc lsp [PROJECT]` 与协议行为（D-M20-1/2）、`tests/support/lsp.rs` 落地；本批刻意保留 `unknown_request_returns_null` 旧行为。
4. H20-05 前需核实 gdb/lldb 可用性；本机未检查调试器。
5. 本报告不把 M20 记为完成；H20-01 状态以三平台 CI 结果为准。
