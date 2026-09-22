# M19 进度报告

本文件按 [M18 执行合同第 6 节模板](../plan-m18-correctness.md#6-报告与验收映射)追加批次记录。
每批一节；未实际运行的检查必须如实标注；设计冻结批次不实现代码，不代表 M19 已完成。

## H19-00 API/目标程序/兼容决策冻结

- 批次：H19-00
- 状态：完成（规格与决策请求已产出；**D1–D3 已于 2026-09-22 由用户确认**，H19-01 解阻并等待人工派发）
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

1. D1、D2、D3 已于 2026-09-22 由用户确认（见 [M19 规格](../proposal-m19-cli-stdlib.md) 第 12 节）：
   `build --lib` 行为不变 + 新增 `dc test`；测试发现/输出/退出码按 §9；M19 不新增语法并保留 defer。
   H19-01 已解阻，等待人工派发；收到批次指令前不开始编码。
2. 收到 H19-01 后的输入：规格 §3（参数/环境）、§13（runtime ABI）、§14 的 ARGS-01..04 测试计划；
   需同步改 CI 显式 `--test` 列表（新增 `tests/m19_args.rs`）。
3. H19-05 必须按规格 §9.2 拆为 H19-05a/b/c，分别交接，不合并为一次实现。
4. 本报告不把 M19 记为完成；也不把规格中任何 API 视为当前可用。

## H19-01 应用参数、环境与 dc run 转发

- 批次：H19-01
- 状态：完成（Linux x86_64；Cranelift/LLVM × Dolphin Debug/Release 本机；macOS/Windows 由远端 CI lane 覆盖，待平台确认）
- 前置批次及报告：H19-00，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`67f47c174f62932ef28357cb21e3f8400b2232e5`（`docs: M19 proposal`）。
- 工作区开始时有 H19-00 确认后的未提交文档改动（`docs/plan-m18-plus.md`、`docs/proposal-m19-cli-stdlib.md`、
  本报告），本批保留并继续修改。
- 本批修改：`runtime/unix_runtime.c`、`runtime/windows_runtime.cpp`、两个 codegen、
  `crates/dolphin-std/src/{lib.rs,process.do}`、`src/main.rs`、`tests/m19_args.rs`、
  `.github/workflows/ci.yml`、`README.md`、`docs/installation.md`、`docs/implemented-features.md`，
  以及规格/计划/本报告的状态同步。未 commit/push/tag。

### 修改文件与关键实现

- `runtime/unix_runtime.c`：新增 `dolphin_init_args`（保存原始 `argc/argv`）、`dolphin_arg_count`、
  `dolphin_arg`（校验 UTF-8，非 UTF-8 返回 NULL+`InvalidArgument`）、`dolphin_env`（`getenv` +
  UTF-8 校验，缺项 `NotFound`）、`dolphin_last_error_kind/code`；稳定错误类别 0..6。
- `runtime/windows_runtime.cpp`：`dolphin_init_args` 忽略 ANSI argv，改用
  `GetCommandLineW`+`CommandLineToArgvW` 惰性转换宽字符参数（`WC_ERR_INVALID_CHARS`，未配对代理项
  → `NotUtf8`）；`GetEnvironmentStringsW` 建立 UTF-8 环境表（名称大小写不敏感）；用
  `#pragma comment(lib, "shell32.lib")` 带上 shell32 依赖。
- 两个后端：入口 `main(i32, void*)` 在函数入口调用 `dolphin_init_args(argc, argv)`；IR/`main` 的
  Dolphin 签名与用户源码不变。Cranelift 在 `initialize_parameters` 后从 entry block 取两个 ABI 参数；
  LLVM 用 `get_nth_param(0/1)`。
- `crates/dolphin-std/src/process.do`（新单元 `std.process`）：`arg_count`、`arg`、
  `program_name`、`env`；借用视图有效到进程结束；`ArgError{OutOfRange,NotUtf8}`、
  `EnvLookup{Found,Missing,NotUtf8}`；视图经 `mem.view_const` + 运行时 UTF-8 校验；
  `std.error` 尚未引入，`env` 内部按运行时 kind 编号（`NotFound = 1`）区分缺项。
- `src/main.rs`：`Commands::Run` 改用 `RunArgs`（flatten `BuildArgs` + `#[arg(last = true)] app_args:
  Vec<OsString>`）；`run_executable` 把 `--` 之后的参数原样 `.args()`；`dc build` 不接受尾随参数。
  使用 `OsString` 使 Unix 非法 UTF-8 参数也能原样转发。
- `tests/m19_args.rs`：ARGS-01..04，每个用例在可用后端 × Debug/Release 上构建并断言固定
  stdout/stderr/exit。

### 验收映射

| 验收点 | 测试名 | 期望与结果 |
| --- | --- | --- |
| ARGS-01 | `m19_args.rs::args_01_direct_and_dc_run_match` | 直接运行与 `dc run --` 输出完全一致，且等于固定期望（`count`、`arg0..N`、`oor=<out-of-range>`、`set=...`、`missing=<missing>`、`raw=<missing>`）；4 组合通过 |
| ARGS-02 | `args_02_spaces_unicode_and_dash` | 空参数、`a b`、`你好，Dolphin`、`--help`、`-x`、制表符、引号、`rocks;rm -rf` 原样出现；固定 stdout；4 组合通过 |
| ARGS-03 | `args_03_missing_env_and_not_utf8` | 越界 `OutOfRange`；缺项 `Missing`；Unix 非法 UTF-8 argv/env 为 `NotUtf8`（不替换字符）；`dc run --` 也原样转发非法字节；4 组合通过 |
| ARGS-04 | `args_04_main_exit_unchanged` | 不用 `std.process` 的程序照常运行；`main(): i32` 经 `dc run` 带/不带 `--` 均返回 42；空返回 0；`dc build . -- extra` 用法错误 exit 2；4 组合通过 |

### 修复前复现结果

新功能先写回归：临时 `git stash push` 撤下 runtime/codegen/stdlib/CLI 改动后运行
`cargo test -p dolphin-compiler --test m19_args`，4 项全部失败：

```text
build backend=cranelift release=false: error[E0001]: unknown import `std.process.arg`
test result: FAILED. 0 passed; 4 failed
```

日志 `/tmp/opencode/h19-01/pre-fix.log`。

### 修复后结果

- `cargo test -p dolphin-compiler --test m19_args`：4 passed；`--features llvm` 同样 4 passed
  （每例 Cranelift+LLVM × Debug/Release）。
- 手工探针（两种后端、Debug/Release）：空参数/空格/Unicode/前导 `-` 保留；非法 UTF-8 argv/env
  → `<not-utf8>`；缺项 → `<missing>`；exit 码透传。
- 完整门禁与发行包冒烟见下；`examples/m14/m15/m18` 在 `dc run` 下 exit 0、stderr 空。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m19_args                          # 4 passed
cargo test -p dolphin-compiler --features llvm --test m19_args          # 4 passed
cargo test --workspace --exclude dolphin-codegen-llvm                   # 327 passed (323→327)
cargo test --workspace --features llvm                                  # 334 passed (330→334)
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages --test doc_examples --test m19_args
                                                                        # 157 passed (153→157)
cargo test -p dolphin-compiler --features llvm --test backend           # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                     # 全部通过
# §5.3 / 发行包冒烟（本机 patchelf 隔离 venv）
cargo build --release --bins
./target/release/dc fmt --check examples crates/dolphin-std/src
./target/release/dc run examples/m14|m15|m18                            # 固定输出、exit 0、stderr 空
# 解压归档后按 ci.yml 冒烟序列（m8/m14/m15/m18、打包 m15math、坐标消费、--locked --offline）
                                                                        # SMOKE_SEQUENCE_OK
```

日志与产物在 `/tmp/opencode/h19-01/`；显式 `--test m19_args` 已加入 `ci.yml` LLVM lane、
README 与 installation 的 LLVM 命令列表。

### 未运行的检查及原因

- macOS/Windows 本机未运行；新运行时函数与 Windows 宽字符路径由远端 CI 的三平台默认 lane 覆盖
  （`cargo test --workspace` 含 `m19_args`），本机不伪造。
- `dc test`（H19-05）与 `std.error` 公开类型（规格 §7，I/O/文件批次引入）未实现；`std.process`
  内部用运行时 kind 编号区分缺项，待 `std.error` 落地后统一。
- 标准流/文件 I/O（H19-02/03）未实现；tag/release 未触发。

### 行为/兼容变化

- 新增保留模块 `std.process`；`main` 源码签名与既有程序行为不变。
- `dc run` 增加 `-- 应用参数`（增量语法）；`dc build` 仍拒绝尾随参数（用法错误 2）。
- 运行时新增参数/环境函数并在入口保存 argv；Windows 运行时新增 shell32 依赖（仅
  `CommandLineToArgvW`），Unix 无新依赖。Debug 泄漏/句柄追踪不受进程生命周期缓存影响
  （缓存用 `malloc`，不走 `dolphin_alloc`）。

### 剩余问题和下一批输入（H19-02）

1. H19-02：`std.io.Stream` 与标准流字节 I/O（IO-01..04）：短读/短写/EOF、`write_all`、
   标准流不可关闭、IO-04 非法 UTF-8 可读为字节但不得无校验转 string。
2. `std.error` 公开类型与 `dolphin_last_error_kind/code` 的统一映射在 H19-02/03 引入；
   本批已提供稳定 kind 编号与运行时常量。
3. 新测试文件 `tests/m19_args.rs` 已进入 CI/文档显式列表；H19-02 新增文件同样要加入。
4. 无阻塞；本报告不把 M19 记为完成。
