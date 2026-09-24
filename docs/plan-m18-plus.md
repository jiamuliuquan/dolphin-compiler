# M18-M21 后续任务与人工交接指南

> 状态：后续计划，尚未实施。M1-M17 的完成记录保留；它们不等于所有功能组合、所有后端和所有平台均无缺陷。
>
> 当前执行入口：[M18 正确性执行合同](plan-m18-correctness.md)。建议首次只派发 `H18-00`，收到报告后再派发下一批。
>
> 本文适用于手动交接给 DeepSeek V4.1 Flash 或其他编码助手，不依赖助手记得此前聊天。真实基线是接手时的源码和已完成批次报告，不是某个模型的口头“已完成”。

## 1. 路线决策

下一阶段面向“可实际使用的本机语言工具链”，不以新增语法数量为验收标准。

| 里程碑 | 交付目标 | 用户可观察的验收结果 | 状态 |
| --- | --- | --- | --- |
| M18 | 已有语义正确、结果可信 | 组合语义反例修复，双后端回归和发布门禁持续执行 | 完成（H18-00..11，见 [M18 报告](reports/m18-progress.md)） |
| M19 | 能编写、测试真实 CLI | 一个纯 Dolphin 文件处理工具能接收输入、报告错误、自测 | 完成（H19-00..07；`examples/m19` + `dc test`；三平台与 CI 通过，见 [M19 报告](reports/m19-progress.md)） |
| M20 | 项目级开发体验 | 使用 stdlib/依赖的未保存代码有正确诊断和跨文件导航 | 待设计冻结，然后实施 |
| M21 | 规模与可靠交付 | 有性能证据、工具链兼容规则和干净环境交付验收 | 条件规划，不提前实施 |

默认顺序 M18 -> M19 -> M20 -> M21。不要同时推进异步、宏、自托管和交叉编译。后续里程碑若因真实应用证据改变，应先改合同及验收，再改代码；不能为了迁就实现困难自行删验收项。

M18 已给出小批次合同；M19/M20/M21 下文是设计输入与实施拆分，不代表 API 和持久格式已经全部冻结。每阶段的 `HNN-00` 负责补足规格及决策记录；该批完成前不得凭下面的模块名称自行实现一套 API。

## 2. 接手规则

### 2.1 文档职责与优先级

| 文档 | 用途 | 不应如何使用 |
| --- | --- | --- |
| [roadmap](roadmap.md) | 总体状态、阶段范围、执行入口 | 不能仅勾选路线图代替验收 |
| [implemented-features](implemented-features.md) | 当前能力与已知限制 | 不能把已知 bug 当作应永久保留的规范 |
| [language-design](language-design.md) | 既有语言规则及明确标注的历史设计 | 不复制历史切片/std 示例作为当前实现目标 |
| [M14 规格](proposal-m14-memory-model.md)、[M15 规格](proposal-m15-generics-stdlib.md) | 已有语义和回归约束 | 不能根据“从 M13 起步”去恢复/重做旧代码 |
| [旧 R00-R21 指南](plan-m14-m15-rework.md) | 已完成工作的历史记录 | 不再派发其中的旧执行提示词 |
| 本文与 [M18 合同](plan-m18-correctness.md) | 新批次、边界和验收 | 未来项不写成当前已实现 |

有冲突时先区分“规范与 bug”“历史与当前”“建议与冻结决定”。已有持久数据、公开 CLI、库 API 受到兼容约束；无法判断是否允许破坏时，提出一个具体问题交由用户决定，不私自引入兼容层或删除行为。

### 2.2 每次只派一个批次

1. 人工指定批次 ID，并附上前一批报告。新会话不要求读完全部历史资料，只读本批及其必需规则。
2. 助手先读现有代码和 diff，列出本批修改范围、验证点与阻塞；不得立即按文档中的旧文件名新建重复模块。
3. 已有用户改动保留。不 reset/checkout，不自动 commit/push/tag，不自动发布包或访问用户真实仓库。
4. 修 bug 必须有修复前失败、修复后正确的回归；新功能必须有正例、反例、边界和旧能力回归。
5. 默认小改动优先；只有明确存在复用需求才抽 helper，不为完成一次批次建新框架。修改公共语义时同步核查两后端。
6. 发生前置缺口或会改变公开契约时停止扩大任务，报告最小阻塞和建议；不要实现“差不多能跑”的替代版本后勾选完成。
7. 结束时输出文件、测试名、实际命令、后端/profile/平台、失败/未验证项和下一批建议。

### 2.3 不可降低的验收要求

- `cargo test --features llvm` 不等于所有测试都跑 LLVM；默认选择和显式 backend 必须核实。
- `cargo test --release` 不等于 Dolphin Release；fixture 中的 `BuildProfile` 必须显式设置。
- 两个后端相同不等于正确；还要断言固定 stdout、stderr、exit 或明确诊断。
- `dc check` 不会验证 codegen、链接或执行结果；不能替代端到端测试。
- `exit=0` 不等于无泄漏；Debug leak report 保留原退出码，必须检查 stderr。
- feature 门控后“0 tests”、未安装依赖、未运行远端 CI 均不是通过。
- “只检测明显错误”不是内存安全保证；不能把悬垂访问、未初始化读写纳入现有保证。
- 不修改测试去接受编译器错误，不删除失败例，不用 `|| true` 掩盖失败，不把未验证项写成通过。

## 3. 当前源码导航

这是 2026-09-17 审计时的真实 workspace 结构；执行时按名称定位，行号可能漂移。

| 工作范围 | 首先阅读 |
| --- | --- |
| CLI、profile、run 参数、后续 test 命令 | [src/main.rs](../src/main.rs)、[dc 入口](../src/bin/dc.rs)、[tests/cli.rs](../tests/cli.rs) |
| 构建/check 编排、后端选择、目标源码集合 | [dolphin-driver](../crates/dolphin-driver/src/lib.rs) |
| 词法、源码位置、诊断 | [dolphin-source](../crates/dolphin-source/src/)、[diagnostic.rs](../crates/dolphin-source/src/diagnostic.rs) |
| AST、parser | [dolphin-syntax](../crates/dolphin-syntax/src/) |
| 模块、类型检查、实例化、lowering | [lower.rs](../crates/dolphin-hir/src/lower.rs)、[modules.rs](../crates/dolphin-hir/src/modules.rs)、[monomorphize.rs](../crates/dolphin-hir/src/monomorphize.rs) |
| 类型化 CFG、Place、布局 | [ir.rs](../crates/dolphin-ir/src/ir.rs)、[layout.rs](../crates/dolphin-ir/src/layout.rs) |
| 两个代码生成后端 | [Cranelift](../crates/dolphin-codegen-cranelift/src/codegen.rs)、[LLVM](../crates/dolphin-codegen-llvm/src/codegen.rs) |
| 源码标准库 | [dolphin-std/src](../crates/dolphin-std/src/)；`std.mem` 是 HIR/codegen 的内建入口 |
| 运行时/平台/链接 | [runtime](../runtime/)、[dolphin-platform](../crates/dolphin-platform/)、[dolphin-linker](../crates/dolphin-linker/) |
| manifest、包图、归档、缓存、锁 | [dolphin-package/src](../crates/dolphin-package/src/) |
| LSP/formatter | [dolphin-lsp](../crates/dolphin-lsp/src/lib.rs)、[dolphin-format](../crates/dolphin-format/src/lib.rs) |
| 语言/FFI/包回归 | [tests/build.rs](../tests/build.rs)、[tests/ffi.rs](../tests/ffi.rs)、[tests/manifest.rs](../tests/manifest.rs)、[tests/packages.rs](../tests/packages.rs)、[tests/backend.rs](../tests/backend.rs) |
| CI、打包、基准 | [ci.yml](../.github/workflows/ci.yml)、[package.py](../scripts/package.py)、[bench.py](../scripts/bench.py) |

## 4. M18：正确性与可信基线

详细步骤、最小复现、固定语义、验收编号和命令见 [M18 执行合同](plan-m18-correctness.md)，不在本文维护第二份状态表。

默认派发顺序：

```text
H18-00 基线与测试准备
H18-01 聚合写入
H18-02 枚举布局
H18-03 饱和转换
H18-04 类型 bound
H18-05 impl 头
H18-06 IR/LLVM verifier
H18-07 包来源
H18-08 目标/profile
H18-09 CI/发布
H18-10 文档示例
H18-11 集成验收
```

M18 完成之前不启动 M19 的语言/runtime 改动；可以记录设计问题，但不能用“正在做标准库”跳过已知错误代码生成。

## 5. M19：真实 CLI 与用户测试

### 5.1 最终交付

实现一个 `dtext` 风格的文本统计/过滤工具，放在 `examples/m19`。名称可在 H19-00 冻结，但必须满足以下行为：

- 从文件路径或 stdin 接收文本；支持一个过滤选项和一个明确的统计输出格式。
- 支持参数转发、帮助、非法参数诊断；约定成功 0、运行/I/O 错误 1、用法错误 2。
- 支持 UTF-8；非法编码按可恢复错误处理，不把任意字节默认为合法 string。
- 空输入、无匹配、文件不存在、读写失败均有确定输出和退出码。
- 使用 Dolphin 标准库完成应用逻辑，不靠 shell/Python/Rust 子进程做真正的文件处理。
- 拆为应用与一个实际复用的库模块/包，验证 lib/path 依赖与用户测试。
- 正常和错误返回路径均关闭自有资源，Debug 不泄漏；标准输入输出句柄不被错误关闭。

统计定义必须精确：字节数还是字符数、最后一行无换行如何计数、CRLF 如何处理、空文件行数、过滤是字节子串还是 Unicode 文本规则。M19 默认选择 UTF-8 文本 + 明确的逐行/子串规则，不加入正则引擎、JSON 或网络。

**实现状态（2026-09-23）**：H19-00..07 全部完成。交付物为 `examples/m19/textstats`（lib）与
`examples/m19/dtext`（lib+bin，path 依赖 `textstats`），由两包 `dc test`、`tests/m19_app.rs` 与
发行包冒烟验收；Linux/Windows/macOS 默认后端与 Linux LLVM 通过（macOS/Windows 平台缺陷修复记录
见 M19 报告 H19-07-W/H19-07-M 节）。

### 5.2 决策批次 H19-00

前置：M18 完成。规格已产出：[M19 规格](proposal-m19-cli-stdlib.md)与 [M19 进度报告](reports/m19-progress.md)；
D1–D3 已确认（规格第 12 节），后续代码批次从 H19-01 起逐批派发。

必须冻结以下决策，每项写推荐方案、拒绝方案、理由与验收例，不能保留“任选一种”交给下一代理猜：

| 决策 | 必须回答的问题 |
| --- | --- |
| 参数/环境 API | 返回拥有字符串还是借用视图？何时释放？Windows UTF-16 如何转换？Unix 非 UTF-8 如何报错？环境缺项如何表达？ |
| 字节 I/O API | buffer 所有权、部分读写、EOF、错误码、最大读取量、stdin/out/err 是否可关闭？ |
| 文件 API | 打开模式、读写/关闭错误、路径中的 NUL 与非 UTF-8、句柄复制后的关闭责任？显式关闭后 defer 是否再次关闭？关闭失败后句柄状态怎样？ |
| 资源状态与 defer | 清理失败如何报告而不覆盖原错误？拥有变量重绑定前如何处理旧资源？defer 退出时读取最新参数，如何避免释放错对象？ |
| 错误类型 | 平台错误如何映射成稳定类别并保留 native code？如何不依赖未实现的字符串格式化能力？ |
| 本地库构建 | 当前 build --lib 会打包且拒绝 path 依赖；怎样支持开发库和测试又不悄悄破坏已文档化行为？ |
| 用户测试 | 测试文件位置、发现规则、入口/签名、库可见性、过滤语义、超时、退出码、0 测试处理？ |
| 错误处理语法 | 现有 Result/match/defer 能否完成目标？是否真的需要 match 语句分支块？ |

建议用小的 runtime 平台封装 + Dolphin 源码库实现；公共 API 的拥有/借用关系写在每个函数旁。不要把全部标准库逻辑写进 Rust 编译器或用类型名字硬编码识别业务容器。

`build --lib` 的公开行为变更、测试目标清单格式与输出路径变化须由用户确认后冻结。未获确认可以先设计内部“只验证库”的入口，但不能宣称开发构建问题已经解决。不得放宽 package/publish 对 path 依赖的限制来绕过该问题。

默认不新增 allocator 参数、隐式析构、异常、`?`、闭包或线程。不支持的路径编码要显式报错，不能静默替换字符后打开错误文件。

### 5.3 实施批次

| 编号 | 工作 | 前置 | 状态 |
| --- | --- | --- | --- |
| H19-00 | API/目标程序/兼容决策冻结 | M18 | 完成（规格与 D1–D3 已确认，见 [规格](proposal-m19-cli-stdlib.md)、[报告](reports/m19-progress.md)） |
| H19-01 | 应用参数、环境、dc run 转发 | H19-00 | 完成（`std.process` + `dc run --`，见 [报告](reports/m19-progress.md)） |
| H19-02 | 标准流与字节 I/O | H19-01 | 完成（`std.error`/`std.io` 标准流，见 [报告](reports/m19-progress.md)） |
| H19-03 | 文件操作与资源错误路径 | H19-02 | 完成（`std.fs` + `release`/`from_raw`，见 [报告](reports/m19-progress.md)） |
| H19-04 | 必要文本/数字处理 | H19-03 | 完成（`std.text.lines`/`Builder`/`parse_i64`/`parse_u64`，见 [报告](reports/m19-progress.md)） |
| H19-05 | 最小 dc test 与库开发闭环 | H19-04 | 完成（H19-05a/b/c：构建 + `tests/` 发现/harness/`std.test` + 子进程执行/超时/过滤/汇总，见 [报告](reports/m19-progress.md)） |
| H19-06 | Result/defer 组合与有限语法补齐 | H19-05 | 完成（组合回归 ERR-01..04；未新增语法，见 [报告](reports/m19-progress.md)） |
| H19-07 | 真实应用、文档、三平台验收 | H19-06 | 完成（Linux：`examples/m19` + `tests/m19_app.rs`；Windows 复验：本机默认 lane 全绿，发现并修复 4 个构建/运行时与 2 个测试/门禁缺陷；macOS 复验：默认 lane、LLVM 22 lane、发行包归档冒烟全绿，未发现产品缺陷，新增 2 个受控读写失败用例；三平台默认 lane 与远端 CI 已通过，见 [报告](reports/m19-progress.md)） |

**H19-01：参数和环境。** 保留当前无参数 main 的源码兼容，推荐通过 std API 读取进程上下文；底层 argc/argv 已存在不代表语言能直接访问。`dc run <项目> [编译选项] -- <应用参数>` 必须原样传递空参数、空格、Unicode 和以 `-` 开头的参数，不经过 shell 拼接。env 缺项和编码失败按规格返回不同结果。验收 ARGS-01：直接运行与 dc run 参数一致；ARGS-02：参数转义/Unicode；ARGS-03：无环境项与非法输入；ARGS-04：main/exit 的历史行为不变。

**H19-02：标准流。** 实现原始字节读写和 stderr 输出，区分 EOF 与错误，正确处理短读/短写和可重试中断；write-all 不得悄悄忽略未写完的字节。借用标准句柄不允许应用误释放；失败路径不泄漏缓冲。验收 IO-01：重定向 stdin/stdout/stderr；IO-02：空输入和多块读取；IO-03：受控部分写入/失败 fixture；IO-04：无效 UTF-8 字节能以 bytes 形式读取但不能无校验转 string。不要用无限阻塞的交互终端作为自动测试输入。

**H19-03：文件。** 打开、读、写、关闭及最小便捷 API；优先实现目标工具实际使用的模式，不一次补完整 fs。显式关闭能返回错误，defer 清理提供约定的 Unit 清理入口或等价方案，不能直接 `defer` 一个返回 Result 的调用后忽略现有 Unit 限制。句柄是拥有资源但不引入自动析构；复制描述符不产生第二份关闭权。验收 FS-01：空/小/多块文件；FS-02：不存在、目录当文件、无效路径；FS-03：可注入读写/关闭失败与资源清理；FS-04：空格/Unicode 路径三平台；FS-05：截断/覆盖规则；FS-06：显式关闭后退出、关闭失败后退出、拥有变量重绑定及显式转交关闭责任，均遵循冻结状态机，不重复关闭、不遗漏旧资源。这里的转交是 API 契约，不是新增隐式 move；不得把 defer 改成注册时捕获来避开问题。权限失败测试不能依赖测试进程一定不是 root，应使用受控 fixture 或平台适用条件并注明。

**H19-04：文本和数值。** 按目标程序补行遍历/分割、整数解析、增长式字符串构建；视图和拥有结果明确区分，源缓冲释放/扩容后不能继续使用旧视图。解析无效字符和整数溢出返回 Result，不走算术 trap。验收 TEXT-01：空串、边界分隔、CRLF/无末尾换行；TEXT-02：UTF-8 边界；TEXT-03：带符号/最大最小整数/溢出；TEXT-04：builder 扩容后结果正确和释放无泄漏。不为 M19 强行新增 HashMap、正则或完整 Unicode 算法。

**H19-05：用户测试。** 提供最小 `dc test` 与断言能力，利用已冻结的约定发现测试，不要求宏/闭包。推荐独立子进程隔离 trap 与超时，生成 harness 时复用模块/包可见性，不在 release 应用中混入测试入口；不能通过把所有私有项变 pub 支持测试。支持名称过滤、确定性顺序、清晰汇总；编译失败、断言失败、trap、超时分别可识别。验收 TEST-01：全部通过 exit=0；TEST-02：至少一个失败 exit 非零；TEST-03：过滤和 0 测试按冻结规则；TEST-04：死循环被终止回收，后续测试继续；TEST-05：lib-only/lib+bin/path 依赖；TEST-06：标准库或示例库的测试确实实例化公开泛型 API。Rust harness 继续负责编译器回归，不能被 dc test 替换。

H19-00 应把 H19-05 细分为 `H19-05a` 已批准的库开发/测试目标入口、`H19-05b` 发现与 harness、`H19-05c` 子进程执行和汇总，按顺序交接，每个子批次有独立正反例；全部通过才关闭 H19-05。不把三个子批次一次派给一个新会话。

**H19-06：错误处理组合。** 先用 Result/match/辅助函数写完整失败路径，确认瓶颈。只有 H19-00 已批准且目标程序确实需要时，新增有限 match 语句分支块；必须先定义 branch scope、return/break/continue、defer、分支结果与发散行为，再改 AST/lower/IR。不得顺带实现一般 block expression、if expression、`?` 或异常。无需新语法也能完成目标时，本批以组合回归完成，不为凑里程碑造语法。验收 ERR-01：早返回仍关闭已获得资源；ERR-02：错误对象/视图不悬垂；ERR-03：return 值快照和 defer 顺序保持 M14 契约；ERR-04：正常 I/O 错误不 trap。

**H19-07：最终应用。** 用新 API 完成第 5.1 节工具，禁止为了漂亮示例跳过错误处理或硬编码输入。应用有自己的 dc test；编译器集成测试实际调用命令行并断言三路结果；三平台默认后端和 Linux LLVM 均验证。标准库公开条目逐项记录拥有权/失效/错误规则；发布包可构建该项目。测试含空输入、正常 UTF-8、无末尾换行、无匹配、非法参数、缺失文件、可控读写失败、重复运行无资源累积。完成报告链接 ARGS/IO/FS/TEXT/TEST/ERR 编号和真实测试名。

## 6. M20：项目级诊断与开发工具

### 6.1 当前限制与目标

审计基线的 LSP 遇到 pkg/use 或无 main 文件会跳过语义分析；hover/definition 主要按当前文件 AST 中的名字匹配。Diagnostic 只保留渲染字符串，LSP 再解析行列。M20 要修的是这些基础，不是先增加补全菜单。

最终验收对象为 M19 的真实项目以及 lib-only/lib+多 bin/path/缓存坐标依赖 fixture；未保存文本也必须参与项目分析。

| 编号 | 工作 | 前置 | 状态 |
| --- | --- | --- | --- |
| H20-00 | 项目分析与工具协议规格 | M19 | 完成（规格见 [proposal-m20](proposal-m20-project-tools.md)；D-M20-1..4 已确认） |
| H20-01 | 结构化诊断与有限恢复 | H20-00 | 完成（实现 + Linux 默认/LLVM lane 通过；Windows/macOS 与远端 CI 未验证，见 [M20 报告](reports/m20-progress.md) H20-01 节） |
| H20-02 | 共享项目分析接口与文件 overlay | H20-01 | 完成（`dolphin-analysis` + `resolve_readonly` + side table；Linux 默认/LLVM lane 通过，Windows/macOS 与远端 CI 未验证，见 [M20 报告](reports/m20-progress.md) H20-02 节） |
| H20-03 | 项目诊断、符号绑定与定义导航 | H20-02 | 完成（项目级 LSP、overlay、跨文件/跨包定义、协议状态机；Linux 默认/LLVM lane 通过，Windows/macOS 与远端 CI 未验证，见 [M20 报告](reports/m20-progress.md) H20-03 节） |
| H20-04 | Formatter 保持性与项目发现 | H20-03 | 完成（`dc fmt` 清单发现/排除/全有或全无 + FMT-01..06；Linux 默认/LLVM lane 通过，Windows/macOS 与远端 CI 未验证，见 [M20 报告](reports/m20-progress.md) H20-04 节） |
| H20-05 | 实际调试器及整体体验验收 | H20-04 | 待实施 |

**H20-00：规格。** 新建 `docs/proposal-m20-project-tools.md`，冻结 SourceId/Span 身份、诊断数据、分析快照生命周期、依赖缺失行为、取消/版本规则、lib/bin 选择与 LSP 测试协议。不强制引入数据库式增量框架，也不要求一次拆开全部 lower；共享分析产物要能提供符号身份、类型与定义位置，而不是重新按名字猜。

状态（2026-09-24）：H20-00 完成，规格已产出（[proposal-m20](proposal-m20-project-tools.md)、[m20-progress](reports/m20-progress.md) H20-00 节）。D-M20-1..4（LSP 协议错误码与退出码、LSP message 结构化、`dc fmt` 发现与全有或全无）已由用户确认；H20-01 解阻，等待人工派发，收到批次指令前不得开始编码。

状态（2026-09-24，H20-01 完成后）：H20-01 已实现结构化 `Diagnostic`/`SourceId`/`SourceMap`、`lex_recovering`/`parse_recovering`、收集式 loader 与声明级 lowering、CLI 多诊断打印与 LSP 结构化映射；`tests/m20_diag.rs` DIAG-01..06 通过。Linux 默认 lane（396 passed）与 Linux LLVM lane（403/202 passed）全绿，clippy/fmt 通过；Windows/macOS 默认 lane 与远端 CI 未验证。`lower_sources_analysis_collecting` 暂返回 `ir::Program`，`LoweredProgram`/side table 留待 H20-02（见 [M20 报告](reports/m20-progress.md) H20-01 节“与冻结规格的差异”）。H20-02 等待人工派发。

状态（2026-09-24，H20-02 完成后）：新增 `dolphin-analysis`（`AnalysisHost`/`AnalysisSnapshot`/`SymbolIndex`/`Resolution`/`file://` URI），`resolve_readonly`（offline、只读锁、`E1001`），HIR `LoweredProgram`/`AnalysisData` side table 与 `lower_sources_analysis(_collecting)` 升级；`tests/m20_analysis.rs` ANALYSIS-01..06 通过（零网络/零写锁/零产物、overlay 影响调用方诊断、lib-only 无误报、多 bin/自定义 source、同名不同包身份、CLI 构建不变）。Linux 默认 lane（418 passed）与 Linux LLVM lane（425/208 passed）全绿，clippy/fmt 通过；Windows/macOS 默认 lane 与远端 CI 未验证。H20-03 等待人工派发。

状态（2026-09-24，H20-03 完成后）：`dc lsp [PROJECT]` 项目级 LSP 落地：`dolphin-lsp` 依赖切换为 `dolphin-analysis`，删除文本同名查找，基于 `SymbolIndex` 的 hover/definition（跨文件/跨包、局部遮蔽/参数）、按 overlay version 发布诊断、`window/showMessage` 项目诊断、URI 字典序发布与协议状态机（-32002/-32601/-32600、D-M20-2 退出码）；单文件模式新增 `analyze_single_file` 保留 M17 导航能力。`tests/support/lsp.rs` + `tests/m20_lsp.rs` LSP-01..06 与 4 个边界用例通过。Linux 默认 lane（437 passed）与 Linux LLVM lane（444/218 passed）全绿，clippy/fmt 通过；Windows/macOS 默认 lane 与远端 CI 未验证。H20-04 等待人工派发。

状态（2026-09-24，H20-04 完成后）：`dc fmt` 按 D-M20-4 落地：无路径参数从 cwd 向上发现 `dolphin.toml`，根为 `[package].source`（默认 `src`）；递归排除项目 `build.output`（含嵌套清单项目）与 `.git`、不跟随目录符号链接；显式文件精确生效；全部选中文件先在内存格式化，任一失败不写任何文件；`--check` 零写入语义不变。`tests/m20_fmt.rs` FMT-01..06 通过（幂等、token/注释保持、全有或全无、`--check` 零写入、清单发现/排除/CRLF、M1-M19 示例格式化后固定构建/运行/自测结果）。Linux 默认 lane（443 passed）与 Linux LLVM lane（450/224 passed）全绿，clippy/fmt 通过；Windows/macOS 默认 lane 与远端 CI 未验证。H20-05 等待人工派发。

**H20-01：诊断。** 数据含 code/severity/primary span/labels/notes；CLI 负责终端渲染，LSP 消费结构化字段。保留 Unicode/UTF-16 正确转换和跨文件 related locations。先做词法/语法同步点及互不依赖声明的多错误收集；出错表达式不能伪造正常类型进入 codegen，缺完整分析时声明 partial。验收 DIAG-01：CLI/LSP 相同错误身份与位置；DIAG-02：多文件两个独立错误；DIAG-03：非 BMP 字符/CRLF；DIAG-04：泛型实例链和用户类型名可读；DIAG-05：错误程序不 panic、不产生可执行产物。

**H20-02：共享分析。** 将项目配置/已解析依赖读取、源码提供器、分析快照与真正构建副作用分开。overlay 以规范化 URI/路径映射未保存文本；didClose 恢复磁盘版本或移除文档状态。分析不下载依赖、不写锁、不产出对象，不在每次 didChange 调用完整 resolve_project；缺依赖给明确诊断/操作提示，用户另行 fetch。验收 ANALYSIS-01：项目分析零网络/零写锁/零构建产物；ANALYSIS-02：未保存依赖文件影响调用方诊断；ANALYSIS-03：库无需 main；ANALYSIS-04：多 bin 与自定义 source；ANALYSIS-05：同坐标/同名不同包不混淆；ANALYSIS-06：保留 CLI 构建行为。

**H20-03：LSP。** 使用解析后的符号身份处理 hover/definition，而不是文本查找顶层同名声明。支持局部遮蔽、函数参数、跨模块 pub 类型/函数/方法和可读的缓存依赖源码；错误/未完成文档至少保留可用的语法诊断，不能把“无法分析”返回为“没有错误”。发布诊断带文档版本，旧任务完成后不得覆盖新快照；可先同步处理或简单取消，不必为此引入异步 runtime。验收 LSP-01：pkg/use 文件类型错误；LSP-02：跨文件定义；LSP-03：局部遮蔽；LSP-04：打开/变更/关闭/依赖变更；LSP-05：连续版本更新；LSP-06：未知请求的协议行为符合 LSP，已有客户端回归通过。测试通过实际 stdio JSON-RPC 会话，不仅调用内部 helper。

**H20-04：格式化。** 优先共享词法规则或以 token 验证现有扫描器，覆盖注释、字符串/字符转义、泛型、defer、FFI、新标准库语法；支持清单自定义 source，排除 target/缓存等生成目录，明确定义显式文件路径行为。先确保幂等和 token/AST 等价，再增加排版规则。验收 FMT-01：连续两次完全相同；FMT-02：去掉位置/空白后的语义 token 不变；FMT-03：错误文件不被部分覆盖；FMT-04：--check 不写文件；FMT-05：项目发现与跨平台换行策略；FMT-06：M1-M19 完整示例保持行为。

**H20-05：调试和整体验收。** 固定 Linux gdb 或 macOS lldb 的批处理脚本，实际验证多文件断点、单步、调用栈、源码行映射；段名字符串检查仅作辅助。先保证 LLVM Debug，未实现的局部变量类型/值检查明确列出，不宣称完整支持；Cranelift/PDB 不自动纳入本阶段。验收 DBG-01：工具实际加载；DBG-02：断点命中正确文件行；DBG-03：跨函数调用栈；DBG-04：优化/无调试配置边界说明。最后在编辑器或协议 fixture 中完整走一次 M19 开发流程，报告仍缺功能，而不是仅展示 initialize 成功。

## 7. M21：规模、兼容与交付

M21 是条件规划：必须用 M19/M20 的实际项目与测量来选优化，不提前承诺某个框架。可先完成测量和兼容设计，再按批准结果实施。包仓库、全局配置与版本策略的设计输入（未冻结）见 [包仓库与版本策略设计输入](design-package-registry.md)；H21-00 评估后决定是否并入 `proposal-m21-delivery.md`。

| 编号 | 工作 | 前置 | 状态 |
| --- | --- | --- | --- |
| H21-00 | 负载、预算与兼容/部署决策 | M20 | 待实施 |
| H21-01 | 可重复编译/运行性能测量 | H21-00 | 待实施 |
| H21-02 | 基于证据的最小规模优化 | H21-01 | 条件待实施 |
| H21-03 | 工具链与源码包兼容身份 | H21-00；按规格协调 H21-02 | 待实施 |
| H21-04 | 干净环境发行与迁移验收 | H21-02/03 | 待实施 |

**H21-00：冻结决策。** 新建 `docs/proposal-m21-delivery.md`。明确支持平台/最低 OS 与运行库版本；选择“要求系统 SDK/开发库并检测”或“分发可重定位 sysroot”，不能只写自包含。评估 GPL 编译器、进入用户产物的 runtime/源码标准库与第三方组件许可说明，遇到授权变更提交维护者决策，助手不擅自改 LICENSE。将编译器发行版本、语言兼容标识、stdlib/runtime ABI、归档格式、native target 限制分开说明，明确哪些真正需要独立编号，避免无必要多版本系统。

**H21-01：测量。** 至少含算术/递归、Vec/String、枚举/聚合、跨包泛型、多 bin 与 M19 应用；逐阶段记录解析/类型检查/单态化/codegen/链接耗时、峰值内存、实例数、IR 大小、产物大小与运行耗时。记录硬件、OS、工具链、后端、Dolphin profile、输入规模、冷/热状态、重复次数和中位数/离散程度；固定工作量和结果校验，避免 benchmark 被优化成空程序。PERF-01：脚本输出可比较的原始数据；PERF-02：至少两次可复现运行；PERF-03：不把单次 best time 推广为普遍 1.4x；预算/阈值在看过基线后写入规格，不拍脑袋定毫秒数。

**H21-02：选择最小优化。** 先判断瓶颈是聚合分量展开、数组动态访问、多目标重复分析、codegen 还是链接。候选是大聚合地址表示、必要的公共显式 load/store、共享分析结果、粗粒度构建缓存；只实施证据支持的项目。不要求自研 SSA 优化器或全量 query 增量数据库。若加缓存，key 至少覆盖工具链/语义身份、目标、后端、profile、源码闭包、stdlib、native 输入内容、链接选项、依赖锁和影响构建的配置；写入原子化、并发隔离、损坏可检测。PERF-04：正确性全回归；PERF-05：选定负载有可重复收益且其他预算未明显退化；CACHE-01：每类输入变更失效；CACHE-02：并发与损坏恢复；CACHE-03：缓存不可用时不返回旧错误产物。无显著收益时记录“不实施缓存”的决策也可，不虚报提速。

**H21-03：源码包兼容。** 当前 `.dlib` 是源码归档，compiler-version 完全匹配且仓库坐标不可覆盖；工具链升级可能迫使包重新发版。同一 Cargo version 的不同开发构建又未必语义一致。实现 H21-00 批准的兼容身份/寻址方案，先用 fixture 模拟旧/新工具链，不直接操作真实仓库。COMPAT-01：兼容包可消费；COMPAT-02：不兼容包明确拒绝；COMPAT-03：已有锁/归档如何迁移或明确不支持；COMPAT-04：离线缓存不会混用不同身份；COMPAT-05：静态仓库仍可部署且不可覆盖原则不弱化。不得以删版本检查解决兼容，也不增加版本范围 solver 或稳定 Dolphin 二进制 ABI。

**H21-04：交付。** 在隔离 Linux 镜像/虚拟机及对应 macOS/Windows 环境中，仅安装声明的运行/开发前提；记录实际工具是否存在，而不是改 PATH 后宣称完全无 SDK。移动解压目录、禁止使用构建机绝对路径，构建并运行 M19 和最小 C 依赖例；测新安装、升级、卸载、锁定和离线重建。DIST-01：声明环境可用；DIST-02：缺前提有可操作诊断；DIST-03：可重定位；DIST-04：上传归档就是验收归档；DIST-05：许可与第三方文件齐全；DIST-06：版本迁移说明可执行。无法验证的平台保留待验收，不用 Linux 结果代替其他平台。

## 8. 延后项与重新评估条件

| 能力 | 何时重新考虑 | 当前禁止的捷径 |
| --- | --- | --- |
| HashMap/HashSet | 真实程序需要键查找，线性容器已成为障碍 | 为标准库清单好看先实现全部集合 |
| allocator/arena | 分配成本或生命周期场景有证据 | 在没有统一约定时给所有 API 机械加 allocator 参数 |
| 函数指针/C 回调 | 封装实际 C 库需要 | 为回调先实现捕获闭包和自动析构 |
| 闭包 | 先明确函数值、捕获、逃逸和清理语义 | 借用局部变量但不规定寿命 |
| 异步/协程 | 同步 I/O 与资源模型已可靠，有网络/并发场景 | 先堆 async 语法却没有可用 runtime |
| 宏/编译期元编程 | 函数/泛型不能解决的重复模式已出现 | 为 dc test 或简单断言先造宏系统 |
| 版本范围 solver | 精确版本分发与兼容策略已稳定，有真实冲突规模 | 用宽松范围掩盖同源/同版本身份问题 |
| Wasm/交叉编译 | target data layout、sysroot、runtime 边界明确 | 仅替换 triple，仍按宿主 C long/指针宽度生成 |
| 自托管 | 语言能支撑大型工具，测量/诊断/库能力达标 | 用编译自己掩盖基础正确性和平台缺口 |
| JIT/REPL | 有交互式使用需求与持久会话语义 | 作为“第三后端”重复制造语义分歧 |

这些不是 M18-M21 完成条件；旧 M14/M15 的所有“延后项”也不自动纳入。

## 9. 可复制交接提示词

### 9.1 首次交接 H18-00

```text
请在当前 Dolphin 仓库执行 H18-00，不执行 H18-01 或后续功能。

先读 docs/plan-m18-plus.md 第 1-4、9-10 节，
再读 docs/plan-m18-correctness.md 第 1-3 节、H18-00、第 5-6 节。
文档中的 a72db41 是审计记录，不是 checkout/reset 指令。

要求：
1. 先检查真实 HEAD、git status 和已有 diff；保留所有用户改动。
2. 核对默认/LLVM 环境，重跑基线与四个独立最小复现。
3. 每个结果记录源码、命令、stdout/stderr/exit、后端和 Dolphin profile。
4. 仅按 H18-00 准备必要的最小测试驱动；不修后续四类 bug，不大改架构。
5. 创建或更新 docs/reports/m18-progress.md，真实报告未运行/受阻项。
6. 不自动提交、推送、打 tag 或发布；不恢复 M13、不重做 R00-R21。

完成后给出证据与 H18-01 的输入。不要只返回计划，也不要宣称 M18 已完成。
```

### 9.2 后续编码批次

人工替换占位符；前置报告直接提供文件路径，不要求模型记住上一会话。

```text
本次只执行批次：<H18-01 等实际 ID>。
前置批次报告：<docs/reports/... 的实际路径与章节>。

请先读 docs/plan-m18-plus.md 的接手规则、源码导航和报告要求，
再读该批所属合同的本批、前置语义、验收矩阵与命令。
M18 合同为 docs/plan-m18-correctness.md；后续阶段先读已冻结规格。

先核对当前源码与 diff，不把文档路径当作必需新建模块的指令。
修复类任务先加入并运行失败回归，再最小修复；新功能覆盖正例/反例/边界。
凡改 IR/layout/lower/runtime 同时检查两个后端，并显式测 Dolphin Debug/Release。
每个验收点必须有真实测试名、固定期望和执行结果，不能只比较两个后端一致。
有兼容变更或未批准语义决策时先问我一个具体问题，不自行选择破坏行为。
完成后更新本批状态和报告，说明实际命令、平台、未验证项与下一批输入。
不得删除/忽略失败测试，不得提前实现下一批，不自动 commit/push/tag/release。
```

### 9.3 后续设计冻结批次

```text
本次只执行 <H19-00 / H20-00 / H21-00> 设计冻结，不实现应用代码。
先读 docs/plan-m18-plus.md 对应阶段、当前源码和前置验收报告。
按该阶段决策表创建详细规格，写清唯一推荐方案、API/格式/语义、拥有权、
错误行为、兼容影响、源码入口、正反例、测试矩阵与后续批次的完成标准。
不能把未实现 API 标为当前可用，不能把“任选一种”留给后续编码助手。
涉及已有 CLI、持久格式或资源模型的破坏性变更，列出最小决策请求交给我确认。
未决事项明确标阻塞，不要擅自开始后续编码或声称整个里程碑完成。
```

## 10. 人工复核与完成标准

收到每批报告，人工至少核对以下内容再派发下一批：

1. 是否真的修改了当前任务需要的源码，而不是只改文档/示例隐藏 bug？设计批次除外。
2. 测试是否在错误实现上失败、在修复后通过？断言是否来自规范，而不是当前错误输出？
3. 是否同时验证成功、诊断、trap、stderr 与资源清理中的相关路径？
4. 是否区分 Rust profile、Dolphin profile、后端 feature 和实际后端选择？
5. 公共变更是否检查了另一个后端、lib、FFI、包图和文档？
6. 未运行平台/CI/调试器是否如实标出？是否有“0 tests 通过”的假验收？
7. 是否有超范围重构、兼容变化、用户文件覆盖或未经要求提交？

报告命名按阶段使用 `docs/reports/m18-progress.md`、`m19-progress.md`、`m20-progress.md`、`m21-progress.md`，实际执行首批时创建。内容遵循 M18 合同第 6 节模板，追加历史，不覆盖前批证据。

阶段完成需要四种证据同时成立：代码实现、自动化验收、真实示例/工具流程、当前文档。完成状态只能根据已发生的验证更新；阻塞时写下一步，不把阻塞项悄悄移到未来里程碑。
