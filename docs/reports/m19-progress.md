# M19 进度报告

本文件按 [M18 执行合同第 6 节模板](../plan-m18-correctness.md#6-报告与验收映射)追加批次记录。
每批一节；未实际运行的检查必须如实标注；设计冻结批次不实现代码，不代表 M19 已完成。

## H19-00 API/目标程序/兼容决策冻结

- 批次：H19-00
- 状态：完成（规格与决策请求已产出；**D1–D3 待用户确认，确认前 H19-01 及之后阻塞**，见本文末）
- 前置批次及报告：H18-11（M18 阶段验收，见 [m18-progress](m18-progress.md) 末节）

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`b7d79b40988708646a816d836a0508b03dec2821`（`v0.3.0`；H18-11 的 `examples/m18` 与
  版本号改动已由用户提交）。
- 本批只新增两份文档（`docs/proposal-m19-cli-stdlib.md`、本文件）并更新
  `docs/plan-m18-plus.md` 的 H19-00 状态行；未触碰任何源码、示例或构建配置。
- 未 commit/push/tag。

### 产出与关键决策（详见规格）

| 决策表项 | 冻结选择（唯一推荐） | 规格章节 |
| --- | --- | --- |
| 参数/环境 API | 新增 `std.process`：`arg_count`/`arg(i): Result<string, ArgError>`/`program_name`/`env(name): EnvLookup`；全部是**借用视图**（进程生命周期），不引入拥有型 args；Windows 首次访问惰性转 UTF-8，非法 UTF-16 → `NotUtf8`；Unix 非 UTF-8 → `NotUtf8`；缺项与编码失败用 `EnvLookup` 三态区分 | §3 |
| 字节 I/O API | 新增 `std.io.Stream`；`read` 短读/`Ok(0)=EOF`，`write` 短写 + `write_all` 循环，运行时内部重试 `EINTR`；不提供 `read_all`；标准流是借用句柄，`close` 返回 `NotOwned` 且不触碰 OS 句柄 | §4 |
| 文件 API | 新增 `std.fs.open(path: string, OpenMode.Read/Write/Append)`，与 `std.io.Stream` 共用句柄；`Write` 截断、`Append` 追加；路径 NUL/非法编码返回 `InvalidArgument`；Unix 非 UTF-8 路径 M19 不可表达且不静默替换 | §5 |
| 资源状态与 defer | 保留 M14 defer 语义（注册处 lowering、退出时读最新值），禁止改成注册时捕获；句柄状态堆上共享，`close` 幂等、`release`/`from_raw` 显式转交；重绑定前必须先 close/release，Debug 报告未关闭自有流；清理失败只写 stderr、保留原退出码 | §6 |
| 错误类型 | 新增 `std.error`：稳定 `ErrorKind`（0..6，含 `NotOwned`/`Closed`）+ native `code`；平台 errno/GetLastError 映射表冻结；不提供字符串消息/Display | §7 |
| 本地库构建 | **不改** `build --lib` 打包行为，不放宽 `.dlib` path 依赖限制；新增 `dc test` 提供库开发/测试闭环（纯增量） | §8（D1） |
| 用户测试 | `tests/*.do` 直接子文件 + `test_*` 自动发现（不新增 `[[test]]` 清单字段），按根模块编译保留可见性；断言 `std.test.expect(bool)`；子进程隔离 + 30s 超时；`target/test/<包名>-tests`；固定输出与汇总；断言失败退出 `106`；0 测试/过滤无匹配退出 1 | §9（D2） |
| 错误处理语法 | M19 **不新增语法**（无 match 语句块/`?`/if 表达式）；若 H19-06 证明阻塞则停止并回到用户重新冻结 | §10（D3） |

另冻结：`dc run ... -- <应用参数>` 原样转发（不经 shell）；目标程序 `dtext` 的用法、CRLF/无末尾换行/空输入/过滤子串与三行统计输出、退出码 0/1/2；运行时 C ABI 函数表（Unix+Windows 必须同实现）；
测试矩阵按 ARGS/IO/FS/TEXT/TEST/ERR 编号映射到 `tests/m19_*.rs` 计划测试名，并要求 H19-05 拆分为 a/b/c。

### 验收映射

设计批次无编号测试；映射到计划第 5.2 节决策表与 H19-01..07 验收编号：

| 检查 | 证据 | 结果 |
| --- | --- | --- |
| 决策表 8 项均有唯一推荐、拒绝方案、理由、正反例 | `docs/proposal-m19-cli-stdlib.md` §3–§10 | 完成 |
| 每个公共 API 标注拥有权/借用/失效规则 | §3–§7 每个 API 段落 | 完成 |
| 错误行为与退出码冻结（101–104 不变，新增 106） | §6.3、§7、§9.1 | 完成 |
| 兼容影响列全（CLI/清单/持久格式/资源模型/保留名） | §15 | 完成 |
| 源码入口与 runtime ABI 表 | §13 | 完成 |
| 测试矩阵与新测试文件计划 | §14 | 完成 |
| 破坏性变更列出最小决策请求 | §12（D1–D3） | 待用户确认 |
| 未实现 API 未标为可用 | 规格页首状态声明 + §15 末条 | 完成 |

### 修复前复现结果

不适用：H19-00 是设计冻结批次，无编译器/运行时改动；本批没有新增失败回归。

### 修复后结果

不适用。规格中所有 API 均标注“未实现”，未写入 `implemented-features.md` 的当前能力。

### 实际运行命令与测试数量

- 本批只新增/修改 Markdown，无源码改动，未重跑测试套件；最后一次全量门禁与数量见
  [H18-11 节](m18-progress.md)（默认 323、LLVM 330、显式列表 153、m18 48、backend 4 全通过）。
- 已做的只读检查：`git status`/`git diff`、规格中相对链接均指向存在文件。

### 未运行的检查及原因

- 任何 M19 实现的编译/运行测试：本批不实现代码。
- D1–D3 确认后的行为冻结复核：等待用户决定。
- 远端 CI：本批未 push/tag；纯文档改动。

### 行为/兼容变化

- 无运行时行为变化。文档层面：
  - `docs/proposal-m19-cli-stdlib.md` 明确所有 M19 API 未实现；
  - `docs/plan-m18-plus.md` 的 H19-00 状态从“待实施”改为“规格已产出，待确认 D1–D3”。
- D1 若选择“保持 `build --lib` 不变”，则公开 CLI/归档行为零变化；若用户要求改变，需要单独的
  兼容性迁移方案（本批未提出实现）。

### 剩余问题和下一批输入（H19-01）

1. **阻塞项**：D1（`build --lib` 保持打包 + 新增 `dc test`）、D2（测试发现/输出/退出码约定）、
   D3（不新增语法 + 保留 defer 语义）需用户确认。未确认前 H19-01 不开始。
2. 确认后 H19-01 的输入：规格 §3（参数/环境）、§13（runtime ABI）、§14 的 ARGS-01..04 测试计划；
   需要同步改 CI 显式 `--test` 列表（新增 `tests/m19_args.rs`）。
3. H19-05 必须按规格 §9.2 拆为 H19-05a/b/c，分别交接，不合并为一次实现。
4. 本报告不把 M19 记为完成；也不把规格中任何 API 视为当前可用。
