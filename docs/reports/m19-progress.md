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

## H19-02 标准流与字节 I/O

- 批次：H19-02
- 状态：完成（Linux x86_64；Cranelift/LLVM × Dolphin Debug/Release 本机；macOS/Windows 由远端 CI lane 覆盖，待平台确认）
- 前置批次及报告：H19-01，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`8c83e530642e0d2f68a7436c4a2a533123004e14`（`v0.3.0-M19-01`），工作区干净。
- 本批修改：`crates/dolphin-std/src/{lib.rs,error.do,io.do,process.do}`、
  `crates/dolphin-hir/src/monomorphize.rs`、`crates/dolphin-codegen-llvm/src/codegen.rs`、
  `runtime/{unix_runtime.c,windows_runtime.cpp}`、`tests/{m19_io.rs,build.rs}`、
  `.github/workflows/ci.yml`、`README.md`、`docs/installation.md`、`docs/implemented-features.md`，
  以及规格/计划/本报告的状态同步。未 commit/push/tag。

### 决策与阻塞处理

- 规格原写 `Result<(), Error>`，但当前语言不能表达 Unit 值：`Result<(), E>` 不解析；
  `Result<Unit, E>` 能通过类型检查，但 `Result.Ok(unit())` 在 Cranelift codegen 内部 panic
  （`Unit has no runtime value`），LLVM 路径同样无法保证。经用户 2026-09-22 确认：
  **unit-like 返回值改为 `Result<bool, Error>`（`true` 成功）**，并把该 ICE 在本批修成诊断。
  规格第 4.1/16 节已同步记录，未新增语法。

### 修改文件与关键实现

- `crates/dolphin-std/src/error.do`（新单元 `std.error`）：`ErrorKind`（0..6 判别值与运行时一致）、
  `Error::new/kind/code`、`from_last_error()`；`code` 保留 native errno/GetLastError。
- `crates/dolphin-std/src/io.do`（新单元 `std.io`）：`Stream`、`stdin/stdout/stderr`、
  `read/write/write_all/flush/close/close_abort/is_open/eprint`。标准流是借用句柄，`close` 返回
  `Err(NotOwned)` 且不影响后续写入；`close_abort` 对借用/已关闭句柄无操作，失败写 stderr 固定前缀。
  `release`/`from_raw` 与 `std.fs.open` 留到 H19-03。
- `crates/dolphin-std/src/process.do`：改用 `std.error.from_last_error()` 与 `ErrorKind` 判断缺项，
  去掉与 `std.error` 重复的 `dolphin_last_error_kind` extern 声明。
- 运行时：新增 `dolphin_stream_stdin/stdout/stderr/read/write/flush/close/is_open` 与共享句柄状态
  （open/owned/native；`close` 幂等、借用返回 `NotOwned`）；Unix 读写重试 `EINTR`、错误 errno 映射到
  稳定 kind；Windows 用 `ReadFile`/`WriteFile`/`FlushFileBuffers`/`CloseHandle` 与 `GetLastError` 映射。
- `crates/dolphin-hir/src/monomorphize.rs`：`instantiate_named_at` 新增 `validate_runtime_fields`，
  `Unit` 不能作为 struct 字段或 enum payload（诊断而非 codegen panic）。
- `crates/dolphin-codegen-llvm/src/codegen.rs`：`declare_user_functions` 按符号名复用已有声明，
  修复多个模块声明同一 `extern "C"` 符号时被重命名为 `<symbol>.1` 导致的链接失败（Cranelift 原本正常）。
- `tests/m19_io.rs`：IO-01..04；`tests/build.rs`：两个编译器缺陷回归。

### 验收映射

| 验收点 | 测试名 | 期望与结果 |
| --- | --- | --- |
| IO-01 | `m19_io.rs::io_01_redirect_stdio` | stdin 读 5 字节、stdout/stderr 重定向输出、`flush=true`；对借用 stdout `close` 返回 `closed=false` 且之后仍能写入；固定 stdout/stderr、exit 0；4 组合通过 |
| IO-02 | `io_02_empty_and_chunked_read` | 空输入立即 `total=0 checksum=0`；100 KiB+37 字节多块读取 `total` 与校验和固定；4 组合通过 |
| IO-03 | `io_03_partial_write_and_failure` | `write_all` 输出 262144 字节模式，长度与校验和固定；Linux 上将 stdout 指向 `/dev/full` 时 `write_all` 返回 Err（写 stderr 标记、exit 3），不是 trap；4 组合通过（`/dev/full` 子用例 Linux-only） |
| IO-04 | `io_04_invalid_utf8_bytes_not_string` | 合法 UTF-8 → `utf8=ok`；非法字节经 `from_utf8` → `utf8=err`（可恢复）；`string.from_bytes` 保持 exit 104 + `invalid UTF-8`；4 组合通过 |
| 编译器缺陷 1 | `build.rs::h19_02_unit_aggregate_payload_is_rejected` | 修复前 codegen panic；修复后 enum payload 与 struct 字段两种用例都在两后端 × Debug/Release 得到含 `Unit` 的诊断 |
| 编译器缺陷 2 | `build.rs::h19_02_duplicate_extern_declarations_share_symbol` | 修复前 Cranelift 通过、LLVM 链接失败（`dolphin_last_error_code.1`）；修复后两后端 × Debug/Release 均 exit 0 |

### 修复前复现结果

1. `std.io` 新功能：临时 `git stash push` 撤下 H19-02 源码后
   `cargo test -p dolphin-compiler --test m19_io` = `0 passed; 4 failed`，均为
   `unknown import std.io.stdin/stdout`（日志 `/tmp/opencode/h19-02/pre-fix-io.log`）。
2. Unit payload：`h19_02_unit_aggregate_payload_is_rejected` 修复前在
   `crates/dolphin-codegen-cranelift/src/codegen.rs:2091` panic（`Unit has no runtime value`）。
3. 重复 extern：`h19_02_duplicate_extern_declarations_share_symbol` 修复前 LLVM 链接报
   `undefined symbol: dolphin_last_error_code.1`（Cranelift 通过）。

### 修复后结果

- `cargo test -p dolphin-compiler --test m19_io`：4 passed；`--features llvm` 同样 4 passed
  （每例 Cranelift+LLVM × Debug/Release）。
- 手工探针：`read`/`write_all`/`eprint` 正常；借用 `close` 返回 NotOwned 且后续写入成功；
  `/dev/full` 失败可恢复；非法 UTF-8 经 `from_utf8` 返回错误、经 `from_bytes` trap 104。
- 完整门禁与发行包冒烟全绿（见下）。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m19_io                            # 4 passed
cargo test -p dolphin-compiler --features llvm --test m19_io            # 4 passed
cargo test -p dolphin-compiler --test build h19_02                      # 2 passed
cargo test -p dolphin-compiler --features llvm --test build h19_02      # 2 passed
cargo test --workspace --exclude dolphin-codegen-llvm                   # 333 passed (327→333)
cargo test --workspace --features llvm                                  # 340 passed (334→340)
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages --test doc_examples \
  --test m19_args --test m19_io                                         # 163 passed (157→163)
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

日志与产物在 `/tmp/opencode/h19-02/`；`--test m19_io` 已加入 `ci.yml` LLVM lane、README 与
installation 的显式列表。

### 未运行的检查及原因

- macOS/Windows 本机未运行；流运行时与 Windows 句柄路径由远端 CI 三平台默认 lane
  （`cargo test --workspace` 含 `m19_io`）覆盖，本机不伪造。
- IO-03 的部分写入子用例只在 Linux 用 `/dev/full` 验证失败路径；其他平台由大段 `write_all`
  与固定输出覆盖，未伪造跨平台受控短写。
- `std.fs.open`、`release`/`from_raw`、文件句柄 Debug 未关闭报告属 H19-03；`dc test` 属 H19-05；
  文本 builder/整数解析属 H19-04。
- tag/release 未触发。

### 行为/兼容变化

- 新增保留模块 `std.error`、`std.io`；`std.process` 缺项判断改走 `std.error`（行为不变）。
- `Unit` 作为 struct 字段/enum payload 现在在实例化时报诊断；此前该输入会在 codegen panic
  （Cranelift）或链接/未定义行为（LLVM）。合法程序不受影响。
- LLVM 后端对同一 `extern "C"` 符号的多次声明现在复用同一个导入声明；此前链接失败。
- 成功类 I/O 返回值采用 `Result<bool, Error>`（用户确认的规格修订），未新增语法。

### 剩余问题和下一批输入（H19-03）

1. H19-03：`std.fs`（`open(path, OpenMode.Read/Write/Append)`）、`release`/`from_raw` 显式转交、
   Debug 未关闭自有流报告；FS-01..06（含 `/dev/full` 类受控失败、Unicode 路径、截断/追加、
   重绑定与转交状态机）。
2. 运行时已有共享句柄状态与 errno→kind 映射，H19-03 只需增加 `dolphin_stream_open` 与自有句柄注册；
   不要改动借用标准流语义。
3. 新测试文件 `tests/m19_io.rs` 已进入显式列表；H19-03 新增文件同样要加入。
4. 无阻塞；本报告不把 M19 记为完成。

## H19-03 文件操作与资源错误路径

- 批次：H19-03
- 状态：完成（Linux x86_64；Cranelift/LLVM × Dolphin Debug/Release 本机；macOS/Windows 由远端 CI lane 覆盖，待平台确认）
- 前置批次及报告：H19-02，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`1d888811edcf5edcaabee40029e4c046dcdff4a9`（`v0.3.0-M19-02`），工作区干净。
- 本批修改：`runtime/{unix_runtime.c,windows_runtime.cpp}`、
  `crates/dolphin-std/src/{lib.rs,io.do,fs.do}`、`tests/m19_fs.rs`、
  `.github/workflows/ci.yml`、`README.md`、`docs/installation.md`、`docs/implemented-features.md`，
  以及规格/计划/本报告的状态同步。未 commit/push/tag。

### 修改文件与关键实现

- 运行时（Unix/Windows）：
  - 句柄状态扩展为登记表（`next` 链）；自有文件流在 `open` 时分配并登记，借用标准流不登记；
    状态不释放，过期句柄最多得到 `Closed`，不会访问悬垂内存。
  - `dolphin_stream_open(path, len, mode, out)`：mode 0=Read、1=Write（创建/截断）、2=Append
    （创建/追加）；路径先查内部 NUL；Unix `open(2)` + `fstat` 识别目录（`IsADirectory`），
    Windows `CreateFileW` + 文件属性识别目录（`InvalidArgument`），Windows 路径按
    `WC_ERR_INVALID_CHARS` 转 UTF-16。
  - `dolphin_stream_release` / `dolphin_stream_from_raw`：基于登记表的安全转交/接管；未知、已关闭、
    借用 id 返回 0。
  - Debug `dolphin_runtime_finish` 不再在无分配泄漏时提前返回；除分配泄漏外还报告未关闭的自有流
    （`Dolphin: N open handle(s) not closed at exit`），退出码不变；Release 不追踪。
- `std.io`：新增 `release(self: *Self): usize`（成功时源句柄置 0）与 `from_raw(handle): Stream`
  （非法/已关闭/借用 id 得到已关闭句柄）；`close` 保持幂等。
- `std.fs`（新单元）：`OpenMode{Read,Write,Append}` 与 `open`；NUL 路径在调用运行时前报
  `InvalidArgument`；返回自有 `Stream`。
- `tests/m19_fs.rs`：FS-01..06，argv 传路径、直接运行、断言固定 stdout/stderr/exit。

### 验收映射

| 验收点 | 测试名 | 期望与结果 |
| --- | --- | --- |
| FS-01 | `m19_fs.rs::fs_01_empty_small_multi_chunk` | 空文件 `total=0 checksum=0 open=true`；小文件与 100 KiB+37 字节多块文件字节数/校验和固定；4 组合通过 |
| FS-02 | `fs_02_missing_dir_and_bad_path` | 不存在 → `kind=not-found`；目录 → Unix `is-dir` / Windows `invalid`（三种模式）；合法文件成功且 Debug 无句柄报告；含 NUL 路径 → `kind=invalid code=0`；4 组合通过 |
| FS-03 | `fs_03_injected_read_write_close_failure` | 对只写流 `read`、对只读流 `write` 均返回 Err；重复 `close` 均成功且 `open-after=false`；`from_raw(0)` 读写返回 `Err(Closed)`；Linux `/dev/full` 写入失败 exit 2（非 trap）；4 组合通过 |
| FS-04 | `fs_04_space_and_unicode_path` | 完整路径含空格与中文（`数据 dir/文件 name.txt`）可打开并读出固定标签；4 组合通过 |
| FS-05 | `fs_05_truncate_and_append` | `Write` 把 10 字节文件截断为 `xy`，`Append` 追加为 `xyz`；`Append` 可创建新文件；4 组合通过 |
| FS-06 | `fs_06_state_machine_close_rebind_handoff` | `release`+`from_raw` 转交：`released=true first-open=false second-open=true` 且 Debug stderr 空；未关闭就重绑定：Debug 报告 1 个未关闭句柄、Release 不报告、exit 均为 0；显式关闭后退出无报告；4 组合通过 |

### 修复前复现结果

新功能先写回归：临时 `git stash push` 撤下 H19-03 的运行时/`std.io`/`std.fs` 改动后运行
`cargo test -p dolphin-compiler --test m19_fs`，6 项全部失败，均为
`unknown import std.fs.open`（日志 `/tmp/opencode/h19-03/pre-fix-fs.log`）。

### 修复后结果

- `cargo test -p dolphin-compiler --test m19_fs`：6 passed；`--features llvm` 同样 6 passed
  （每例 Cranelift+LLVM × Debug/Release）。
- 手工探针：读取/缺失/目录/NUL 分类正确；`.release`+`from_raw` 状态机符合规格；未关闭重绑定在
  Debug 输出 `Dolphin: 1 open handle(s) not closed at exit`、Release 为空。
- 完整门禁与发行包冒烟全绿（见下）。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m19_fs                              # 6 passed
cargo test -p dolphin-compiler --features llvm --test m19_fs              # 6 passed
cargo test --workspace --exclude dolphin-codegen-llvm                     # 339 passed (333→339)
cargo test --workspace --features llvm                                    # 346 passed (340→346)
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages --test doc_examples \
  --test m19_args --test m19_io --test m19_fs                             # 169 passed (163→169)
cargo test -p dolphin-compiler --features llvm --test backend             # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                       # 全部通过
# §5.3 / 发行包冒烟（本机 patchelf 隔离 venv）
cargo build --release --bins
./target/release/dc fmt --check examples crates/dolphin-std/src
./target/release/dc run examples/m14|m15|m18                              # 固定输出、exit 0、stderr 空
# 解压归档后按 ci.yml 冒烟序列（m8/m14/m15/m18、打包 m15math、坐标消费、--locked --offline）
                                                                          # SMOKE_SEQUENCE_OK
```

日志与产物在 `/tmp/opencode/h19-03/`；`--test m19_fs` 已加入 `ci.yml` LLVM lane、README 与
installation 的显式列表。

### 未运行的检查及原因

- macOS/Windows 本机未运行；`CreateFileW` 路径、目录识别与 Debug 句柄报告由远端 CI 三平台默认 lane
  （`cargo test --workspace` 含 `m19_fs`）覆盖，本机不伪造。
- 受控 `close(2)` 失败没有可移植注入方式（普通文件关闭不会失败）；用模式不匹配读写、Linux
  `/dev/full` 写入失败、幂等重复关闭与 invalid 句柄覆盖，未伪造真实关闭失败（规格 §16 已注明）。
- `dc test`（H19-05）、文本 builder/整数解析（H19-04）、真实 `dtext` 应用（H19-07）未实施。
- tag/release 未触发。

### 行为/兼容变化

- 新增保留模块 `std.fs`；`std.io` 增加 `release`/`from_raw`（此前规格列出、实现留到本批）。
- Debug 运行时新增未关闭自有流报告；不影响退出码，正常清理程序 stderr 仍为空。Release 不追踪。
- 目录打开在 Unix/Windows 分别归入 `IsADirectory`/`InvalidArgument`；NUL 路径显式拒绝。无破坏性变更。

### 剩余问题和下一批输入（H19-04）

1. H19-04：文本与数值（TEXT-01..04）：行遍历/分割、`std.text.Builder`（增长式字符串，扩容后旧视图
   失效）、`parse_i64`/`parse_u64`（空串/非法/溢出返回 `Result`，不走算术 trap）；不为 M19 加
   HashMap/正则。
2. 目标工具 `dtext` 的行/CRLF/无末尾换行规则见规格第 11 节；H19-04 的 Builder 是应用累积跨块行的
   基础，需先定义 view 失效规则。
3. 新测试文件 `tests/m19_fs.rs` 已进入显式列表；H19-04 新增文件同样要加入。
4. 无阻塞；本报告不把 M19 记为完成。

## H19-04 必要文本/数字处理

- 批次：H19-04
- 状态：完成（Linux x86_64；Cranelift/LLVM × Dolphin Debug/Release 本机；macOS/Windows 由远端 CI lane 覆盖，待平台确认）
- 前置批次及报告：H19-03，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`21c3e35`（`v0.3.0-M19-03`），工作区干净。
- 本批修改：`crates/dolphin-std/src/text.do`、`tests/m19_text.rs`（新）、
  `.github/workflows/ci.yml`、`README.md`、`docs/installation.md`、`docs/implemented-features.md`、
  `docs/proposal-m19-cli-stdlib.md`（新增 §11.1 与状态）、`docs/plan-m18-plus.md`、本报告。
  未 commit/push/tag。

### 修改文件与关键实现

- `crates/dolphin-std/src/text.do`（纯增量，既有 `String`/`concat`/`substring`/`from_utf8`
  等行为不变）：
  - `Lines` + `lines(bytes: []const u8): Lines`：按 `\n` 切分，行内容去掉紧邻 `\n` 前的一个
    `\r`（孤立 `\r` 保留），无 `\n` 的非空尾段算一行，空输入 0 行；`Iterator.Item = []const u8`
    是借用视图，零分配、不做 UTF-8 校验（行边界是 ASCII 字节）。
  - `Builder`（增长式字节缓冲）：`init`/`with_capacity`/`append(string)`/`append_bytes`/`len`/
    `is_empty`/`view(): []const u8`/`consume(count)`/`clear`/`deinit`。`view()` 是借用视图，在下一次
    `append*`/`consume`/`clear`/`deinit` 后失效（扩容替换底层存储）；构造中允许暂不完整的 UTF-8。
    长度和/倍增溢出按 `Vec` 的既有模式走 102 分配失败通道，不做算术 trap。
  - `NumberError{Empty, InvalidDigit, Overflow}`、`parse_i64`（可选 `-`/`+`）、`parse_u64`（可选 `+`）：
    只接受 ASCII 数字，溢出在乘加前检查并返回 `Overflow`；负数直接按 i64 累积，使
    `-9223372036854775808` 可表示且不触发 101。
- `tests/m19_text.rs`：TEXT-01..04，每个用例在可用后端 × Dolphin Debug/Release 上构建，
  断言固定 stdout、stderr 为空（Debug 同时覆盖 Builder 释放无泄漏与无句柄报告）、exit 0。

### 验收映射

| 验收点 | 测试名 | 期望与结果 |
| --- | --- | --- |
| TEXT-01 | `m19_text.rs::text_01_lines_crlf_no_final_newline` | 空串 0 行；`"a"`/`"a\n"` 1 行；`"a\n\n"` 2 行（含空行）；`"a\r\nb"` 去 CRLF；`"a\rb"` 保留孤立 `\r`；`"a\r\r\n"` 只去一个 `\r`；`"x\ny"` 尾行无换行仍计数；固定 stdout；4 组合通过 |
| TEXT-02 | `text_02_utf8_boundaries` | `aé日` 字节长 6；合法区间切出 `é`/`日`；结束/开始/中间落在续字节 → `InvalidBoundary`；越界/逆序 → `OutOfBounds`；空区间成功；多字节行遍历保留原字节；`from_utf8` 对 `61 FF 62` 返回 Err；4 组合通过 |
| TEXT-03 | `text_03_parse_signed_extremes_overflow` | i64 min/max 精确值；`+42`/`-0`；超界、超长、空串、仅符号、尾随/前导空白、`0x10`、`1_000` 全部返回对应 `NumberError`；u64 max 精确值、`+1` 成功、`-1` 非法；exit 0（无 101 trap）；4 组合通过 |
| TEXT-04 | `text_04_builder_grow_and_deinit` | 2000 字节经多次扩容后 `from_utf8` 成功、长度/校验和固定；`consume(10)` 后首字节与长度固定；超量 `consume` 清空；清空后可复用；`with_capacity(4)` 扩容后内容 `abcdef`；跨两次 `append_bytes` 的 `é`（`C3`+`A9`）校验成功；Debug stderr 为空（deinit 无泄漏）；4 组合通过 |

### 修复前复现结果

新功能先写回归：`git stash push -- crates/dolphin-std/src/text.do` 撤下实现后运行
`cargo test -p dolphin-compiler --test m19_text`，4 项全部失败，分别为
`unknown import std.text.lines/Builder/NumberError`（日志 `/tmp/opencode/h19-04/pre-fix.log`）。

### 修复后结果

- `cargo test -p dolphin-compiler --test m19_text`：4 passed；`--features llvm` 同样 4 passed
  （每例 Cranelift+LLVM × Debug/Release，stderr 为空）。
- 手工探针（默认后端）：行切分、极值解析、Builder 扩容/consume/clear、跨块 UTF-8 校验结果
  与固定期望一致；`-9223372036854775808` 输出精确，无 101/104。
- 完整门禁与发行包冒烟全绿（见下）。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m19_text                            # 4 passed
cargo test -p dolphin-compiler --features llvm --test m19_text            # 4 passed
cargo test --workspace --exclude dolphin-codegen-llvm                     # 343 passed (339→343)
cargo test --workspace --features llvm                                    # 350 passed (346→350)
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages --test doc_examples \
  --test m19_args --test m19_io --test m19_fs --test m19_text             # 173 passed (169→173)
cargo test -p dolphin-compiler --features llvm --test backend             # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                       # 全部通过
# §5.3 / 发行包冒烟（本机 patchelf 隔离 venv）
cargo build --release --bins
./target/release/dc fmt --check examples crates/dolphin-std/src
./target/release/dc run examples/m14|m15|m18                              # 固定输出、exit 0、stderr 空
python3 scripts/package.py --target x86_64-unknown-linux-gnu --out-dir /tmp/opencode/h19-04/dist
# 解压归档后按 ci.yml 冒烟序列（m8/m14/m15/m18、打包 m15math、坐标消费、--locked --offline）
                                                                          # SMOKE_SEQUENCE_OK
```

日志与产物在 `/tmp/opencode/h19-04/`；`--test m19_text` 已加入 `ci.yml` LLVM lane、README 与
installation 的显式列表。

### 未运行的检查及原因

- macOS/Windows 本机未运行；本批是纯 Dolphin 源码标准库与测试改动，无平台分支/运行时改动，
  由远端 CI 三平台默认 lane（`cargo test --workspace` 含 `m19_text`）覆盖，本机不伪造。
- 无受控“视图失效后使用旧视图”测试：语言无 use-after-free 检测，直接读旧视图是未定义行为；
  规格只冻结契约并测试扩容后新视图正确（TEXT-04），不把未定义行为纳入保证。
- `dc test`（H19-05）、真实 `dtext` 应用（H19-07）未实施；`Builder.consume` 的 O(n) 前缀搬移
  在 H19-07 应用循环中按“每块一次 consume”使用，未在本批做性能测量。
- tag/release 未触发。

### 行为/兼容变化

- `std.text` 纯增量：新增 `Lines`/`lines`/`Builder`/`NumberError`/`parse_i64`/`parse_u64`；
  既有 `String`/`CString`/`concat`/`trim`/`substring`/`from_utf8`/查询函数行为不变。
- 无 IR/layout/lower/runtime 改动；两个后端使用同一份源码标准库，TEXT-01..04 均在
  Cranelift 与 LLVM × Debug/Release 上通过。

### 剩余问题和下一批输入（H19-05）

1. H19-05 必须按规格 §9.2 拆为 H19-05a（`dc test` 命令与 `target/test/` 产物）、H19-05b
   （`tests/*.do` + `test_*` 发现与 harness、可见性）、H19-05c（子进程执行、超时、汇总），
   分别交接，不合并为一次实现。
2. 本批已提供应用累积跨块行的 Builder 与行规则；H19-07 的 `dtext` 使用 `Builder.view()` +
   `consume` 处理完整行，并在 `from_utf8` 校验失败时报 `dtext: invalid UTF-8`（退出 1）。
3. 新测试文件 `tests/m19_text.rs` 已进入显式列表；H19-05 新增文件同样要加入。
4. 无阻塞；本报告不把 M19 记为完成。

## H19-05a `dc test` 测试目标入口

- 批次：H19-05a（H19-05 的第一子批次；b/c 未实施，H19-05 不关闭）
- 状态：完成（Linux x86_64；Cranelift/LLVM × Dolphin Debug/Release 本机；macOS/Windows 由远端 CI lane 覆盖，待平台确认）
- 前置批次及报告：H19-04，见本文件上一节；子批次拆分依据规格 §9.2 与计划第 5.3 节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`d252afc`（`v0.3.0-M19-04`），工作区干净。
- 本批修改：`crates/dolphin-hir/src/modules.rs`、`crates/dolphin-driver/src/lib.rs`、`src/main.rs`、
  `tests/m19_test_cmd.rs`（新）、`.github/workflows/ci.yml`、`README.md`、`docs/installation.md`、
  `docs/implemented-features.md`、`docs/proposal-m19-cli-stdlib.md`、`docs/plan-m18-plus.md`、本报告。
  未 commit/push/tag。

### 修改文件与关键实现

- `crates/dolphin-hir/src/modules.rs`：`PackageSources` 新增 `extra: Vec<ExtraSource>`，把不参与
  磁盘发现的源码按**根模块文件**注入（不得声明 `pkg`，可访问根模块私有项与子模块 `pub` 项）；
  根包在磁盘源码被排除后仍有 `extra` 时不再报“没有 `.do` 文件”。`load_sources` 传空。
- `crates/dolphin-driver/src/lib.rs`：新增 `build_tests`（仅路径依赖）与 `build_tests_with_graph`
  （CLI 用完整包图）。需要 `[lib]` 目标；排除全部 `[[bin]]` 入口；生成入口写到
  `target/test/<包名>-tests.entry.do` 并作为根模块注入；产出
  `target/test/<包名>-tests[.exe]` 与 `.o`；**不调用打包路径**（无 `.dlib`、不要求可发布性）。
  `load_graph_sources` 改为委托新的 `load_graph_sources_with_extra`。
- `src/main.rs`：新增 `dc test <项目目录>` 子命令，编译选项与 build/run 一致
  （`--debug`/`--release`、`--system-linker`、`--backend`、`--locked`/`--offline`）；缺清单、
  bin-only、编译失败均 exit 1，clap 用法错误 exit 2。本批生成入口为占位
  `fn main(): i32 { return 0; }`；发现（b）与执行/汇总（c）未实现，命令按冻结的 0 测试规则
  输出 `no tests found` 并 exit 1。`BuildArgs` 与 `TestArgs` 共享 `explicit_profile`/
  `compile_settings`/`resolve_options` 三个 helper。
- `tests/m19_test_cmd.rs`：TEST-05a 构建侧、根模块契约与反例。

### 验收映射

| 验收点 | 测试名 | 期望与结果 |
| --- | --- | --- |
| TEST-05a（构建侧） | `m19_test_cmd.rs::test_05a_lib_only_bin_and_path_dep_targets` | lib-only、lib+bin、path 依赖三种项目在可用后端 × Debug/Release 上均 exit 1 + 固定 `no tests found`、stderr 空；`target/test/<名>-tests` 与 `.o` 存在；无 `target/package`、全项目无 `.dlib`；占位产物运行 exit 0 且 stderr 空；4 组合通过 |
| 根模块契约 | `test_05a_generated_entry_is_root_module` | 驱动 API 注入调用私有 `secret()` 与 `pub value()` 的入口，产物 exit 8，证明生成入口与 `src/*.do` 同属根模块；4 组合通过 |
| 反例/用法 | `test_05a_errors_usage_and_no_packaging` | 库源码错误 exit 1 + `unknown variable` 诊断、无产物；无清单 exit 1；bin-only exit 1 + `requires a library target`；`--locked` 无锁 exit 1；未知参数 exit 2；`dc check`/`dc build --lib` 忽略含非法文件的 `tests/`；`dc test` 可构建 path 依赖而 `dc package` 仍拒绝 path dependency（D1 不变） |

### 修复前复现结果

新功能先写回归：`git stash push` 撤下 HIR/driver/CLI 三个文件后
`cargo test -p dolphin-compiler --test m19_test_cmd` 编译失败
`error[E0432]: unresolved import dolphin_compiler::build_tests`；同一状态下
`./target/debug/dc test <项目>` 输出 `error: unrecognized subcommand 'test'` 并 exit 2
（日志 `/tmp/opencode/h19-05/pre-fix.log`、`pre-fix-cli.err`）。

### 修复后结果

- `cargo test -p dolphin-compiler --test m19_test_cmd`：3 passed；`--features llvm` 同样 3 passed
  （每例 Cranelift+LLVM × Debug/Release）。
- 手工探针：lib-only/lib+bin/path 依赖均生成 `target/test/<名>-tests`；产物 exit 0、stderr 空；
  `dc package` 对 path 依赖仍报 `cannot publish: dependency ... is a path dependency`。
- 完整门禁与发行包冒烟全绿（见下）。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m19_test_cmd                        # 3 passed
cargo test -p dolphin-compiler --features llvm --test m19_test_cmd        # 3 passed
cargo test --workspace --exclude dolphin-codegen-llvm                     # 346 passed (343→346)
cargo test --workspace --features llvm                                    # 353 passed (350→353)
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages --test doc_examples \
  --test m19_args --test m19_io --test m19_fs --test m19_text --test m19_test_cmd
                                                                          # 176 passed (173→176)
cargo test -p dolphin-compiler --features llvm --test backend             # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                       # 全部通过
# §5.3 / 发行包冒烟（本机 patchelf 隔离 venv）
cargo build --release --bins
./target/release/dc fmt --check examples crates/dolphin-std/src
./target/release/dc run examples/m14|m15|m18                              # 固定输出、exit 0、stderr 空
python3 scripts/package.py --target x86_64-unknown-linux-gnu --out-dir /tmp/opencode/h19-05/dist
# 解压归档后按 ci.yml 冒烟序列（m8/m14/m15/m18、打包 m15math、坐标消费、--locked --offline）
                                                                          # SMOKE_SEQUENCE_OK
# 发行包内 dc test 冒烟：lib 项目 -> target/test/archtest-tests，no tests found，exit 1
```

日志与产物在 `/tmp/opencode/h19-05/`；`--test m19_test_cmd` 已加入 `ci.yml` LLVM lane、README 与
installation 的显式列表。

### 未运行的检查及原因

- macOS/Windows 本机未运行；本批改动为跨平台路径/模块加载与 CLI，由远端 CI 三平台默认 lane
  （`cargo test --workspace` 含 `m19_test_cmd`）覆盖，本机不伪造。
- 测试发现（`tests/*.do`、`test_*`、可见性）、`std.test.expect/fail`、`--filter`、30s 超时、
  子进程隔离与汇总均未实现（H19-05b/c）；TEST-01..04、TEST-06 因此未运行。
- 生成入口当前是占位 `main`，没有真实测试代码，因此本批未覆盖“测试 trap 后继续/超时回收”等
  运行期路径；Debug 只验证占位产物 stderr 为空。
- bin-only 项目被明确拒绝（`dc test` 需要 `[lib]`，避免与生成入口的 `main` 冲突）；规格
  TEST-05 只要求 lib-only/lib+bin/path 依赖，此限制已写入规格 §9.2 与 implemented-features。
- tag/release 未触发。

### 行为/兼容变化

- 新增 `dc test` 子命令（H19-05a 仅构建侧）；`dc build`/`run`/`check`/`package`/`publish` 行为不变，
  且都不读取 `tests/`。`dc build --lib` 仍产出验证目标文件与 `.dlib`（D1 不变）。
- `PackageSources` 新增 `extra` 字段（dolphin-hir 公共结构；当前唯一构造方是 driver，已同步）。
- 生成的测试入口与目标文件写入 `target/test/`，不写入用户 `src/`。

### 剩余问题和下一批输入（H19-05b）

1. H19-05b：发现 `tests/` 直接子文件 `*.do`（不递归）、`test_*`（无参数/无类型参数/返回 Unit）、
   拒绝测试文件中的 `main`、生成按全限定名排序的 harness 入口（内部 `--dolphin-test <名称>` 分发）、
   测试文件按根模块编译并保持可见性（不把私有项改 `pub`）。生成入口应替换本批占位实现。
2. H19-05c：子进程执行（30s 超时 kill 并继续）、退出码分类（assertion 106/trap/timeout）、
   `--filter`、固定汇总格式与 0 测试/无匹配 exit 1。b 不得提前实现 c 的运行与汇总。
3. `std.test`（`expect`/`fail` + 运行时 `dolphin_test_fail`）随 b 引入；§13 ABI 表已冻结。
4. `tests/m19_test_cmd.rs` 已进入显式列表；b/c 扩展同一文件时保持 TEST-01..06 计划测试名。
5. 无阻塞；H19-05 未关闭，本报告不把 M19 记为完成。

## H19-05b 测试发现与 harness 生成

- 批次：H19-05b（H19-05 的第二子批次；c 未实施，H19-05 不关闭）
- 状态：完成（Linux x86_64；Cranelift/LLVM × Dolphin Debug/Release 本机；macOS/Windows 由远端 CI lane 覆盖，待平台确认）
- 前置批次及报告：H19-05a，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`082e90a`（`v0.3.0-M19-05a`），工作区干净。
- 本批修改：`crates/dolphin-driver/src/lib.rs`、`crates/dolphin-std/src/lib.rs`、
  `crates/dolphin-std/src/test.do`（新）、`runtime/unix_runtime.c`、`runtime/windows_runtime.cpp`、
  `src/main.rs`、`tests/m19_test_cmd.rs`、`docs/proposal-m19-cli-stdlib.md`、
  `docs/implemented-features.md`、`docs/plan-m18-plus.md`、本报告。未 commit/push/tag。

### 决策与阻塞处理

- 规格 §9.2 把“子进程执行”整体划给 H19-05c，b 的命令在发现 N>0 个测试时没有冻结输出。
  经用户 2026-09-22 确认采用**严格拆分**：0 测试 → `no tests found`；N>0 → 固定临时输出
  `built N tests (execution lands in H19-05c)`；两者都 exit 1，未执行前绝不报成功。b 的验收
  直接运行产物内部 `--dolphin-test` 接口。

### 修改文件与关键实现

- `crates/dolphin-driver/src/lib.rs`：
  - `TestFunction`/`TestTarget` 与 `discover_tests`：读包根 `tests/` 的**直接子文件** `*.do`
    （不递归），逐文件 lex/parse；拒绝 `pkg`、拒绝 `main`、要求 `test_*` 无参数、无类型参数、
    无返回类型标注、非 extern；其余函数作为 helper；按函数名排序，同名测试在发现阶段报错。
  - `generate_test_harness`：按名称排序生成入口，按内部参数 `--dolphin-test <名称>` 分发；
    参数缺失/错误返回 2、未知名称返回 3（不对外承诺）；0 测试生成占位 `main`（保持 H19-05a
    的构建行为）。测试文件（含只定义 helper 的文件）与入口一起作为根模块源码注入，保持
    根模块私有项与子模块 `pub` 项的可见性。
  - `build_test_target[_with_graph]` 返回发现的测试与产物；`build_test_sources_with_graph`
    为公共构建实现，H19-05a 的 `build_tests*` 保持兼容。
- `crates/dolphin-std/src/test.do`（新 `std.test`）与 `dolphin-std/src/lib.rs` 注册：
  `expect(condition: bool)`、`fail()`；失败调用运行时 `dolphin_test_fail`。
- `runtime/{unix_runtime.c,windows_runtime.cpp}`：新增 `DOLPHIN_EXIT_TEST 106` 与
  `dolphin_test_fail`：写 stderr 固定文本 `Dolphin test assertion failed`，`_Exit`/`ExitProcess(106)`；
  与 trap 一致不展开 Dolphin 栈、不执行 defer、不运行 Debug 收尾报告。
- `src/main.rs`：`dc test` 改用发现 + harness；按用户确认的临时输出/退出码。
- `tests/m19_test_cmd.rs`：新增 3 个 TEST-05b 用例。

### 验收映射

| 验收点 | 测试名 | 期望与结果 |
| --- | --- | --- |
| 发现/排序/helper | `m19_test_cmd.rs::test_05b_discovery_direct_children_order_and_helpers` | `tests/` 直接子文件的 `test_*` 按名排序为 `test_alpha/test_beta/test_zeta`（跨文件），`tests/nested/ignored.do` 与 `notes.txt` 不发现，`helper_from_b` 不算测试；CLI 固定 `built 3 tests (execution lands in H19-05c)`、stderr 空、exit 1、产物存在；可用后端 × Debug/Release 通过 |
| harness/断言 | `test_05b_harness_dispatch_and_assertions` | `--dolphin-test test_pass`→0 且 stderr 空；`test_helper`（跨文件 helper + 私有 `secret()`）→0；`test_expect_false`/`test_fail`→106 且 stderr 恰为 `Dolphin test assertion failed\n`；未知名→3；无参数/缺名称/错误 flag→2；4 组合通过 |
| 可见性/反例 | `test_05b_visibility_and_rejections` | 子模块 `pub fn shown` 可见且 `test_ok`→0；子模块私有 `hidden` → `is private`；测试文件 `pkg`/`main`/带参数/类型参数/返回类型/extern/语法错误/同名测试分别以固定诊断拒绝且不产出二进制；4 组合通过（反例覆盖前端诊断） |

### 修复前复现结果

新功能先写回归：`git stash push -u` 撤下 driver/stdlib/runtime/CLI 改动后
`cargo test -p dolphin-compiler --test m19_test_cmd` 编译失败
`error[E0432]: unresolved import dolphin_compiler::build_test_target`；同一状态下
`dc test <含 tests/ 的项目>` 只输出 `no tests found`（发现缺失），exit 1
（日志 `/tmp/opencode/h19-05b/pre-fix.log`、`pre-fix-cli.*`）。

### 修复后结果

- `cargo test -p dolphin-compiler --test m19_test_cmd`：6 passed（3 个 H19-05a + 3 个 H19-05b）；
  `--features llvm` 同样 6 passed。
- 手工探针：`dc test` → `built 3 tests (execution lands in H19-05c)` exit 1；
  `test_pass`→0、`test_expect_false`→106 + 固定 stderr、未知→3、无参数→2；发行包内 `dc test`
  同样可用。
- 完整门禁与发行包冒烟全绿（见下）。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m19_test_cmd                        # 6 passed
cargo test -p dolphin-compiler --features llvm --test m19_test_cmd        # 6 passed
cargo test --workspace --exclude dolphin-codegen-llvm                     # 349 passed (346→349)
cargo test --workspace --features llvm                                    # 356 passed (353→356)
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages --test doc_examples \
  --test m19_args --test m19_io --test m19_fs --test m19_text --test m19_test_cmd
                                                                          # 179 passed (176→179)
cargo test -p dolphin-compiler --features llvm --test backend             # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                       # 全部通过
# §5.3 / 发行包冒烟（本机 patchelf 隔离 venv）
cargo build --release --bins
./target/release/dc fmt --check examples crates/dolphin-std/src
./target/release/dc run examples/m14|m15|m18                              # 固定输出、exit 0、stderr 空
python3 scripts/package.py --target x86_64-unknown-linux-gnu --out-dir /tmp/opencode/h19-05b/dist
# 解压归档后按 ci.yml 冒烟序列（m8/m14/m15/m18、打包 m15math、坐标消费、--locked --offline）
                                                                          # SMOKE_SEQUENCE_OK
# 发行包 dc test 冒烟：4 个测试、test_pass=0、test_fail=106 + 固定文本
```

日志与产物在 `/tmp/opencode/h19-05b/`；测试文件列表无新增（`m19_test_cmd` 已在 CI/README/
installation 显式列表中）。

### 未运行的检查及原因

- macOS/Windows 本机未运行；`dolphin_test_fail` 的 Windows `ExitProcess` 路径由远端 CI 三平台
  默认 lane（`cargo test --workspace` 含 `m19_test_cmd`）覆盖，本机不伪造。
- 子进程执行/汇总/`--filter`/30s 超时/固定 `test <name> ...` 输出（H19-05c）未实现；因此
  TEST-01..04、TEST-06 的端到端验收未运行，`dc test` 对 N>0 以 1 退出且不宣称通过。
- 断言失败不执行 `defer`、不运行 Debug 收尾报告：由 `_Exit(106)` 继承 trap 语义，本批未单独
  构造“失败前申请资源”的观测用例（属 ERR-01/ERR-04 范围）。
- 测试之间的共享状态隔离、trap/超时分类未验证（c）。
- tag/release 未触发。

### 行为/兼容变化

- 新增保留模块 `std.test`（`expect`/`fail`）；运行时新增 `dolphin_test_fail` 与退出码 `106`，
  `101`–`104` 语义不变。
- `dc test` 现在读取 `tests/` 并生成 harness；`dc build`/`run`/`check`/`package` 仍永不读取
  `tests/`（H19-05a 回归继续覆盖）。`dc test` 对 N>0 个测试的临时输出/退出码见上，c 会替换为
  运行与汇总。
- `build_tests*`（H19-05a 公共 API）保持兼容；新增 `build_test_target*`、`discover_tests`、
  `generate_test_harness`、`TestFunction`、`TestTarget`。

### 剩余问题和下一批输入（H19-05c）

1. H19-05c：对每个发现到的测试启动独立子进程
   `<二进制> --dolphin-test <名称>`，30s 超时 kill 并继续；按退出码分类固定输出
   `test <name> ... ok` / `FAILED (assertion)` / `FAILED (trap exit N)` / `FAILED (timeout after 30s)`；
   `--filter <子串>` 与 `no tests matched filter`；固定汇总 `N passed; M failed; K filtered out`；
   全部通过 0、任一失败 1、0 测试/无匹配 1、用法错误 2；子进程 stdout/stderr 继承。
2. c 必须替换本批的临时 `built N tests ...` 输出，并保持 `dc test` 的发现/harness/可见性行为不变。
3. `tests/m19_test_cmd.rs` 扩展时保持 TEST-01..06 计划测试名；本批已提供
   `discover_tests`/`TestTarget.tests` 供 runner 复用，不需要重新解析测试文件。
4. 无阻塞；H19-05 未关闭，本报告不把 M19 记为完成。

## H19-05c 子进程执行与汇总

- 批次：H19-05c（H19-05 的第三子批次；完成后 H19-05 关闭，M19 仍未完成）
- 状态：完成（Linux x86_64；Cranelift/LLVM × Dolphin Debug/Release 本机；macOS/Windows 由远端 CI lane 覆盖，待平台确认）
- 前置批次及报告：H19-05b，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`79f1f38`（`v0.3.0-M19-05b`），工作区干净。
- 本批修改：`src/main.rs`、`tests/m19_test_cmd.rs`、`docs/proposal-m19-cli-stdlib.md`、
  `docs/implemented-features.md`、`docs/plan-m18-plus.md`、本报告。未 commit/push/tag。

### 修改文件与关键实现

- `src/main.rs`：
  - `dc test` 新增 `--filter <子串>`（按测试名单子串选择；在完整 harness 上运行，K=未选中数）。
  - `run_test_case`：对每个测试启动独立子进程 `<二进制> --dolphin-test <名称>`，子进程
    stdout/stderr 直接继承；`TEST_TIMEOUT = 30s`，超时 `kill` + `wait` 回收后继续。
  - 退出码分类：0→`ok`；106→`FAILED (assertion)`；其余（无正常退出码的信号终止统一按 1）→
    `FAILED (trap exit N)`；超时→`FAILED (timeout after 30s)`。
  - 固定输出与汇总：`test <name> ... ok|FAILED (...)` 与 `N passed; M failed; K filtered out`；
    全部通过 0、任一失败 1；0 测试 `no tests found`、过滤无匹配 `no tests matched filter`，均 1；
    编译失败 1、用法错误 2（clap）。
- `tests/m19_test_cmd.rs`：新增冻结名 TEST-01..06；H19-05b 的三个用例的 CLI 断言从临时
  `built N tests (execution lands in H19-05c)`（b 的用户确认中间行为）更新为最终 runner 输出，
  发现/harness/可见性断言保持不变；TEST-01 增加测试内 `println` 验证子进程 stdout 继承。

### 验收映射

| 验收点 | 测试名 | 期望与结果 |
| --- | --- | --- |
| TEST-01 | `m19_test_cmd.rs::test_01_all_pass` | 两个测试逐行 `ok`，测试内 `println("marker-from-test")` 原样出现在两行之间（stdout 继承、不被吞），汇总 `2 passed; 0 failed; 0 filtered out`，exit 0，stderr 空；4 组合通过 |
| TEST-02 | `test_02_failure_nonzero` | `test_assert`→`FAILED (assertion)`、`test_pass` 在失败后仍执行→`ok`、`test_trap`（i32 溢出）→`FAILED (trap exit 101)`，汇总 `1 passed; 2 failed; 0 filtered out`，exit 1，stderr 含断言与 trap 固定文本；4 组合通过 |
| TEST-03 | `test_03_filter_and_zero` | 无过滤按名排序 3 个 `ok`；`--filter beta` 只跑 `test_beta` 且 `1 passed; 0 failed; 2 filtered out` exit 0；`--filter zzz` → `no tests matched filter` exit 1；无 `tests/` → `no tests found` exit 1；4 组合通过 |
| TEST-04 | `test_04_timeout_kills_and_continues` | 死循环 `test_a_loop` 固定 30s 被 kill 回收→`FAILED (timeout after 30s)`，后续 `test_b_after` 继续→`ok`，汇总 `1 passed; 1 failed; 0 filtered out`，exit 1；4 组合通过（本机 Debug 2 组合 60s、LLVM 4 组合 120s） |
| TEST-05 | `test_05_lib_only_bin_and_path_dep` | lib-only、lib+bin、path 依赖三种项目各运行 1 个测试→`ok` + 汇总 + exit 0、stderr 空；4 组合 × 3 项目通过 |
| TEST-06 | `test_06_stdlib_generic_instantiation` | 测试内实际实例化 `Vec<i32>`（push/len/get/deinit）与 `Option<i32>` match，通过→exit 0；4 组合通过 |
| 回归 | H19-05a/b 的 6 个既有用例 | a 的 0 测试构建/无打包/用法反例不变；b 的发现/排序/harness 分发（含 106/3/2）/可见性/拒绝断言不变，仅 CLI 观测值更新为 runner 输出 |

### 修复前复现结果

新功能先写回归：`git stash push -- src/main.rs` 撤下 runner 后
`cargo test -p dolphin-compiler --test m19_test_cmd` = 3 passed; 9 failed：TEST-01..06 与
更新后的 b CLI 断言全部失败（`--filter` 为未知参数 exit 2、`dc test` 仍输出 b 的临时
`built N tests ...`），H19-05a 的 3 个用例仍通过（日志 `/tmp/opencode/h19-05c/pre-fix.log`）。

### 修复后结果

- `cargo test -p dolphin-compiler --test m19_test_cmd`：12 passed（默认 60.3s，LLVM 120.4s）。
- 手工探针：pass/assertion/trap 分类、`--filter` 子集与汇总、0 测试、无匹配、30s 超时 kill 后
  继续，均与固定期望一致；发行包内 `dc test` 同样输出与退出码正确。
- 完整门禁与发行包冒烟全绿（见下）。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m19_test_cmd                        # 12 passed（60.3s）
cargo test -p dolphin-compiler --features llvm --test m19_test_cmd        # 12 passed（120.4s）
cargo test --workspace --exclude dolphin-codegen-llvm                     # 355 passed (349→355)
cargo test --workspace --features llvm                                    # 362 passed (356→362)
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages --test doc_examples \
  --test m19_args --test m19_io --test m19_fs --test m19_text --test m19_test_cmd
                                                                          # 185 passed (179→185)
cargo test -p dolphin-compiler --features llvm --test backend             # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                       # 全部通过
# §5.3 / 发行包冒烟（本机 patchelf 隔离 venv）
cargo build --release --bins
./target/release/dc fmt --check examples crates/dolphin-std/src
./target/release/dc run examples/m14|m15|m18                              # 固定输出、exit 0、stderr 空
python3 scripts/package.py --target x86_64-unknown-linux-gnu --out-dir /tmp/opencode/h19-05c/dist
# 解压归档后按 ci.yml 冒烟序列（m8/m14/m15/m18、打包 m15math、坐标消费、--locked --offline）
                                                                          # SMOKE_SEQUENCE_OK
# 发行包 dc test 冒烟：assertion=FAILED (assertion)、trap=FAILED (trap exit 101)、
# --filter pass 只跑 1 个测试并汇总 1 passed; 0 failed; 2 filtered out
```

日志与产物在 `/tmp/opencode/h19-05c/`；测试文件列表无新增（`m19_test_cmd` 已在 CI/README/
installation 显式列表中）。

### 未运行的检查及原因

- macOS/Windows 本机未运行；Windows 的 `TerminateProcess`/无退出码归类由远端 CI 三平台默认
  lane（`cargo test --workspace` 含 `m19_test_cmd`）覆盖，本机不伪造；`kill` 语义差异不额外断言。
- `--system-linker` 与 `dc test` 的组合未单独运行（选项经 `compile_settings` 与 build 共用，
  链接器行为已由既有构建测试覆盖）。
- 0 测试 + `--filter` 组合未单独断言（先判 0 测试，输出 `no tests found`）。
- 并行执行、随机顺序、重试、输出格式 JSON 等均不在规格内，未实现。
- tag/release 未触发。

### 行为/兼容变化

- `dc test` 现在实际运行用户测试：全部通过 exit 0、任一失败 exit 1，并按规格逐行输出与汇总。
  H19-05b 的用户确认中间行为（`built N tests (execution lands in H19-05c)`，exit 1）被最终行为
  替换；`docs/proposal-m19-cli-stdlib.md` §9.2 与 implemented-features 已同步，b 测试断言已更新。
- 新增 `--filter <子串>`；`dc build`/`run`/`check`/`package` 行为不变且仍不读取 `tests/`。
- 断言失败（106）/trap/超时都以非零退出并在 stderr 保留子进程诊断，不被 runner 吞掉。

### 剩余问题和下一批输入（H19-06）

1. H19-06：Result/defer 组合与有限语法补齐（ERR-01..04 → `tests/m19_errors.rs`）；先用既有
   `Result`/`match`/helper/`defer` 写完整失败路径，只有证明阻塞才按规格 §10.2 回到用户重新冻结，
   不得自行新增语法。
2. H19-05 已关闭（a/b/c 全部通过）；H19-07 的真实应用需使用本批的 `dc test` 与 `std.test`。
3. 本批未改动 IR/layout/lower/runtime；`dc test` 的 runner 属 CLI/driver 层。
4. 无阻塞；M19 未完成，本报告不把 M19 或 H19-07 记为完成。

## H19-06 Result/defer 组合与有限语法补齐

- 批次：H19-06（组合回归；未新增语法，D3 保持）
- 状态：完成（Linux x86_64；Cranelift/LLVM × Dolphin Debug/Release 本机；macOS/Windows 由远端 CI lane 覆盖，待平台确认）
- 前置批次及报告：H19-05c，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`f4bd056`（`v0.3.0-M19-05c`），工作区干净。
- 本批修改：`tests/m19_errors.rs`（新）、`.github/workflows/ci.yml`、`README.md`、
  `docs/installation.md`、`docs/implemented-features.md`、`docs/proposal-m19-cli-stdlib.md`、
  `docs/plan-m18-plus.md`、本报告。**无编译器/运行时/标准库源码改动**。未 commit/push/tag。

### 决策与结论（D3 复核）

- 用既有 `Result` + `match` + helper + `defer` 写完了四类失败路径（资源清理、错误值/视图寿命、
  返回快照/清理顺序、I/O 错误处理），**无需新增语法**：不加 match 语句块、`?`、if/block 表达式或异常。
- 组合中发现两处表达限制，均可用局部绑定绕过，不阻塞目标程序，也不构成本批要改的语义决策：
  1. 字段访问结果上直接调用方法（`report.error.code()`）被按路径解析为函数名而报未知函数；
     先 `val reported = report.error;` 再 `reported.code()`。
  2. `return match ... { Result.Err(error) => Result.Err(error), ... }` 的 Err 构造臂报
     `cannot infer type argument T`；先绑定错误再 `return Result.Err(error)`。
  两条已记入 implemented-features 的当前限制。

### 修改文件与关键实现

- `tests/m19_errors.rs`：ERR-01..04，四个 Dolphin 程序 + 每例在可用后端 × Debug/Release 上
  `dc build` 并以 argv 运行，断言固定 stdout/stderr/exit；Debug 的 stderr 为空同时覆盖无分配泄漏
  与无未关闭句柄报告。

### 验收映射

| 验收点 | 测试名 | 期望与结果 |
| --- | --- | --- |
| ERR-01 | `m19_errors.rs::err_01_early_return_cleans_resources` | 空文件读后早返回 exit 4、非空 exit 0、缺失文件 exit 2；三条路径 stdout 分别为 `clean 1`/`clean 1`/空，Debug stderr 空（defer 释放缓冲并关闭句柄）；4 组合通过 |
| ERR-02 | `err_02_error_views_not_dangling` | 失败 `open` 的 `Error` 经结构体与 helper 传递后仍可读（`kind=not-found has-code=true`、`again=not-found`）；`Builder` 视图在其存活期内经 `from_utf8` 使用（`prefix=view`、`whole=view-after`）；Builder deinit 无泄漏；exit 0；4 组合通过 |
| ERR-03 | `err_03_return_snapshot_and_defer_order` | 固定 stdout `clean 1`（内层清理）→`value=42`（返回快照在释放缓冲前取得）→`clean 3`→`clean 2`（外层逆序、退出时读 tag 最新值）；exit 0、Debug stderr 空；4 组合通过 |
| ERR-04 | `err_04_io_error_not_trap` | 缺失文件→`missing=not-found`、对只写流 `read`→`read-on-write-err=true`、含 NUL 路径→`nul=invalid`；全部走 `Result`，exit 0（不是 101/104）、stderr 空；4 组合通过 |

### 修复前复现结果

- 本批不修生产代码，没有“修复前失败”的源码回归；四个验收用例是既有语义的新覆盖，首次运行即通过。
- 为证明 ERR-01 的泄漏断言有效，手工探针在 `src/main.do` 中故意不注册 `defer`：
  Debug 产物输出 `Dolphin: 1 open handle(s) not closed at exit`、exit 0——即测试的
  `stderr.is_empty()` 会捕获清理失效（探针在 `/tmp/opencode/h19-06/leak/`，未提交）。
- 编写失败路径时最初尝试 `report.error.code()` 与 `return match ... Result.Err(...)`，分别得到
  `unknown function report.error.code` 与 `cannot infer type argument T`；按上节局部绑定改写后通过。

### 修复后结果

- `cargo test -p dolphin-compiler --test m19_errors`：4 passed；`--features llvm` 同样 4 passed。
- 手工探针：四类程序的 stdout/stderr/exit 与固定期望一致；无 101/104/句柄报告。
- 完整门禁与发行包冒烟全绿（见下）。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m19_errors                          # 4 passed
cargo test -p dolphin-compiler --features llvm --test m19_errors          # 4 passed
cargo test --workspace --exclude dolphin-codegen-llvm                     # 359 passed (355→359)
cargo test --workspace --features llvm                                    # 366 passed (362→366)
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages --test doc_examples \
  --test m19_args --test m19_io --test m19_fs --test m19_text --test m19_test_cmd --test m19_errors
                                                                          # 189 passed (185→189)
cargo test -p dolphin-compiler --features llvm --test backend             # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                       # 全部通过
# §5.3 / 发行包冒烟（本机 patchelf 隔离 venv）
cargo build --release --bins
./target/release/dc fmt --check examples crates/dolphin-std/src
./target/release/dc run examples/m14|m15|m18                              # 固定输出、exit 0、stderr 空
python3 scripts/package.py --target x86_64-unknown-linux-gnu --out-dir /tmp/opencode/h19-06/dist
# 解压归档后按 ci.yml 冒烟序列（m8/m14/m15/m18、打包 m15math、坐标消费、--locked --offline）
                                                                          # SMOKE_SEQUENCE_OK
```

日志与产物在 `/tmp/opencode/h19-06/`；`--test m19_errors` 已加入 `ci.yml` LLVM lane、README 与
installation 的显式列表。

### 未运行的检查及原因

- macOS/Windows 本机未运行；四个用例只用既有跨平台 `std.fs`/`std.io`/`std.text`/`defer` 组合，
  由远端 CI 三平台默认 lane（`cargo test --workspace` 含 `m19_errors`）覆盖，本机不伪造。
- `/dev/full` 等受控写失败不重复覆盖（FS-03 已有）；权限失败需要非 root 受控 fixture，未新增。
- 未测试“defer 中再注册 defer/跳转”等规格明确不支持的写法。
- tag/release 未触发。

### 行为/兼容变化

- 无：本批不改编译器、运行时、标准库或 CLI；新增的只是回归测试与文档中的当前限制说明。
- 目标程序的失败路径已证明可用既有语法表达；D3 的“不新增语法”结论在本批得到验证。

### 剩余问题和下一批输入（H19-07）

1. H19-07：用已实现的 `std.process`/`std.io`/`std.fs`/`std.text`/`std.test` 完成
   `examples/m19/textstats`（lib）与 `examples/m19/dtext`（bin）及两包 `tests/*.do`；
   编译器集成测试实际调用命令行并断言三路结果；三平台默认后端与 Linux LLVM 均验证。
2. 测试矩阵要求：空输入、正常 UTF-8、无末尾换行、无匹配、非法参数、缺失文件、可控读写失败、
   重复运行无资源累积；标准库公开条目逐项记录拥有权/失效/错误规则；发布包可构建该项目。
3. 失败路径写法沿用本批模式（先绑定错误/视图再使用）；`return match` 与链式方法调用的限制见上。
4. 无阻塞；M19 未完成，本报告不把 M19 或 H19-07 记为完成。

## H19-07 真实应用、文档与（Linux）验收

- 批次：H19-07（M19 最后一个实施批次；macOS/Windows 验收由用户在后续会话中另行执行）
- 状态：完成（Linux x86_64：Cranelift/LLVM × Dolphin Debug/Release 本机全绿；macOS/Windows 未运行，见“未运行的检查”）
- 前置批次及报告：H19-06，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`9ed21b1`（`v0.3.0-M19-06`），工作区干净。
- 本批修改：`examples/m19/**`（新：`textstats` lib + `dtext` lib+bin + 两包 `tests/*.do` +
  `README.md`）、`tests/m19_app.rs`（新）、`.github/workflows/ci.yml`（显式列表 + 冒烟）、
  `README.md`、`docs/installation.md`、`docs/implemented-features.md`、
  `docs/proposal-m19-cli-stdlib.md`、`docs/plan-m18-plus.md`、`examples/README.md`、本报告。
  **无编译器/运行时/标准库源码改动**。未 commit/push/tag。

### 决策与阻塞处理

- 规格 §11 写 `dtext` 是 `[[bin]]` 且两包各自有 `tests/*.do`，计划要求“应用有自己的 dc test”；
  但 `dc test` 需要 `[lib]`，且 D1 下 lib+bin 且声明 path 依赖的包 plain `dc build` 会在打包库时
  拒绝 path 依赖（已实测）。经用户 2026-09-22 确认采用 **lib+bin**：应用逻辑放 `src/app.do`，
  `src/main.do` 只是入口；文档与冒烟用 `dc build --bin dtext` / `dc run --bin dtext`，
  并在规格 §11、examples/m19/README 与 implemented-features 记录 D1 交互。

### 修改文件与关键实现

- `examples/m19/textstats`（`[lib]`）：`Stats`（私有字段 + `empty`/`lines`/`matched`/`bytes`）
  与 `analyze(input: []const u8, filter: string): Result<Stats, TextError>`；复用 `std.text.lines`
  的冻结行规则，空过滤匹配所有行，非法 UTF-8 返回 `TextError.InvalidUtf8`；纯函数零分配。
- `examples/m19/dtext`：`src/app.do`（`[lib]`）实现 `parse_args_from`（纯函数，`dc test` 直接覆盖）、
  `parse_process_args`（把 argv 复制为 `[]const string`）、`run()`（stdin/`-`/文件输入、Builder
  累积整输入、`textstats.analyze`、固定三行输出、退出码 0/1/2）、诊断与数字格式化；
  `src/main.do` 仅 `return run();`。
- `examples/m19/*/tests/*.do`：textstats 5 个核心用例（行/CRLF/无末尾换行/过滤/字节数/非法 UTF-8）；
  dtext 5 个用例（参数解析、通过 path 依赖调用 `textstats.analyze`、数字格式化）。
- `tests/m19_app.rs`：把 `examples/m19` 复制到临时目录，用 `dc` 在可用后端 × Debug/Release 上
  `dc build --bin dtext` 并实际调用命令行断言三路结果；同时运行两包 `dc test`。

### 验收映射（链接 ARGS/IO/FS/TEXT/TEST/ERR）

| 验收点 | 真实测试名 | 期望与结果 |
| --- | --- | --- |
| 布局/D1 + `--help` | `m19_app.rs::app_01_layout_build_and_help` | plain `dc build` exit 1 且 stderr 含 `path dependency`（D1 不变）；`--bin dtext` 构建成功；`--help` 输出固定用法 exit 0、stderr 空；4 组合通过 |
| ARGS/IO/FS/TEXT 组合（stdin） | `app_02_stdin_and_line_rules` | 空输入 `lines=0 matched=0 bytes=0`；正常 UTF-8 `--filter et`→`3/1/17`；无末尾换行 `a\nb`→`2/2/3`；无匹配 `zzz`→`2/0/4`；CRLF→`2/1/6`；Unicode 过滤→`2/1/14`；`-`→`1/1/2`；全部 exit 0、stderr 空；4 组合通过 |
| FS/TEXT 文件输入 + 重复运行 | `app_03_file_input_and_repeated_runs` | 空格/中文路径文件读取固定输出；同一 Debug 产物连续运行 5 次 stderr 全空（无泄漏、无未关闭句柄）；4 组合通过 |
| ARGS（用法错误） | `app_04_usage_errors` | 未知选项/缺少 `--filter` 值/多余位置参数（含 `- -`）→ exit 2 + 固定 stderr、stdout 空；4 组合通过 |
| FS/ERR（打开与编码错误） | `app_05_open_and_encoding_errors` | 缺失文件→`dtext: cannot open input (not-found)` exit 1；目录→Unix `(is-dir)`/Windows `(invalid)`；stdin 与文件的非法 UTF-8→`dtext: invalid UTF-8` exit 1；stdout 不输出统计；4 组合通过 |
| IO（受控写失败） | `app_06_write_failure_not_trap`（Linux） | stdout 指向 `/dev/full`→exit 1 + `dtext: cannot write output (other)`，不 trap；4 组合通过 |
| TEST（应用自测） | `app_07_dc_test_self_checks` | `dc test textstats` 与 `dc test dtext` 各 5 个用例全 `ok`、固定 `5 passed; 0 failed; 0 filtered out`、exit 0、stderr 空；4 组合通过 |
| 既有回归 | `m19_args`/`m19_io`/`m19_fs`/`m19_text`/`m19_test_cmd`/`m19_errors` | ARGS-01..04、IO-01..04、FS-01..06、TEXT-01..04、TEST-01..06、ERR-01..04 全部继续通过（见下方命令） |

固定诊断文本（`dtext`，均写 stderr 并带换行）：`dtext: unknown option`、
`dtext: --filter requires a value`、`dtext: too many arguments`、`dtext: invalid UTF-8 argument`、
`dtext: invalid UTF-8`、`dtext: cannot open input (<kind>)`、`dtext: cannot read input (<kind>)`、
`dtext: cannot write output (<kind>)`；`<kind>` ∈ not-found/permission/is-dir/invalid/not-owned/closed/other。
`--help` 固定 stdout 为 `usage: dtext [--help] [--filter <text>] [<path>]`。

### 修复前复现结果

- 本批不修生产代码；新示例与集成测试首次运行即通过（无“修复前失败”）。规格/D1 冲突与用户
  决策见上节；`dc build --bin dtext` 与 plain `dc build` 的差异由 `app_01` 固定为回归。

### 修复后结果

- `cargo test -p dolphin-compiler --test m19_app`：7 passed；`--features llvm` 同样 7 passed。
- 手工探针：`--help`、空/正常/无末尾换行/无匹配/CRLF/Unicode/`-`、用法错误、缺失文件/目录/
  非法 UTF-8/受控写失败、重复运行均与固定期望一致。
- 完整门禁与发行包冒烟全绿（含归档 `dc build --bin dtext` + 运行 + `dc test` 两包）。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m19_app                            # 7 passed
cargo test -p dolphin-compiler --features llvm --test m19_app            # 7 passed
cargo test --workspace --exclude dolphin-codegen-llvm                     # 366 passed (359→366)
cargo test --workspace --features llvm                                    # 373 passed (366→373)
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages --test doc_examples \
  --test m19_args --test m19_io --test m19_fs --test m19_text --test m19_test_cmd \
  --test m19_errors --test m19_app                                        # 196 passed (189→196)
cargo test -p dolphin-compiler --features llvm --test backend             # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                       # 全部通过
# §5.3 / 发行包冒烟（本机 patchelf 隔离 venv）
cargo build --release --bins
./target/release/dc fmt --check examples crates/dolphin-std/src
./target/release/dc run examples/m14|m15|m18                              # 固定输出、exit 0、stderr 空
python3 scripts/package.py --target x86_64-unknown-linux-gnu --out-dir /tmp/opencode/h19-07/dist
# 解压归档：m8/m14/m15/m18、m19 plain build 拒绝 path 依赖 + --bin 构建运行、打包 m15math、
# 坐标消费、--locked --offline                                             # SMOKE_SEQUENCE_OK
# 归档 dc test：textstats 5 passed、dtext 5 passed（隔离 DOLPHIN_HOME）
```

日志与产物在 `/tmp/opencode/h19-07/`；`--test m19_app` 已加入 `ci.yml` LLVM lane、README 与
installation 的显式列表；`ci.yml` 冒烟新增 M19 构建/运行步骤。

### 未运行的检查及原因

- **macOS/Windows 本机未运行**：本机只有 Linux；用户已说明会在后续会话单独验证。三平台默认
  后端（Cranelift）与路径含空格/Unicode、Windows 目录错误类别 `(invalid)` 等断言已写好但未
  在真实平台执行，未伪造。
- macOS/Windows 上的 LLVM 未运行（本机 Linux LLVM 已验证）。
- 受控读失败没有可移植注入方式（同 FS-03 说明）：应用只在 open 成功后读，无法在普通文件上
  稳定注入 EIO；open/write 失败与 FS-03 的模式不匹配读失败覆盖了错误路径代码。
- `--system-linker` 与 `dc test` 组合未单独运行（选项与 build 共用）。
- tag/release 未触发。

### 行为/兼容变化

- 无编译器/运行时/标准库行为变化；新增示例与集成测试、CI 显式列表与冒烟步骤、文档。
- 规格 §11 补充实现状态：`dtext` 为 lib+bin 布局，plain `dc build` 在 D1 下仍拒绝 path 依赖，
  应用用 `--bin` 构建；`examples/m19/README.md`、examples 索引与 implemented-features 已同步。
- M19 验收：ARGS/IO/FS/TEXT/TEST/ERR 均有真实测试名与固定期望；H19-01..07 全部完成，
  M19 待三平台验收后再由用户确认阶段完成。

### 剩余问题和下一批输入

1. 用户在 macOS/Windows 上按本报告命令复验 `tests/m19_app.rs`（三平台默认 Cranelift ×
   Debug/Release）与 examples/m19 构建运行；重点确认：Windows 目录错误类别 `(invalid)`、
   路径含空格/Unicode、`/dev/full` 用例在非 Linux 上按 `cfg(target_os = "linux")` 跳过。
2. 三平台通过后 M19 阶段完成，下一阶段为 M20（先做 H20-00 设计冻结，不提前实现）。
3. 本报告不把 M19 阶段记为完成；Windows 已由下方补充节复验，macOS 结果未出前保持
   “Linux + Windows 已验收”。

## H19-07-W Windows 平台复验与缺陷修复

- 批次：H19-07 的 Windows 平台复验（补充节，不新增批次编号；含为通过复验所需的缺陷修复）
- 状态：完成（Windows 10 企业版 25H2 本机：默认 Cranelift lane、`tests/m19_app.rs`、
  examples/m19 构建运行、fmt/clippy/Release 构建全绿；发现并修复 4 个 Windows 构建/运行时缺陷
  与 2 个 Windows 测试/门禁缺陷；LLVM lane、发行包冒烟、macOS 与远端 CI 未验证）
- 前置批次及报告：H19-07，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`77c5717`（`v0.3.0-M19-07`），工作区干净。
- 本批修改：`runtime/windows_runtime.cpp`、`crates/dolphin-platform/build.rs`、
  `tests/m19_io.rs`、`tests/m19_test_cmd.rs`，以及本报告与计划状态同步。未 commit/push/tag。

### 环境

| 项 | 值 |
| --- | --- |
| 平台 | Windows 10 企业版 25H2（build 26200），x86_64 |
| rustc / cargo | 1.98.1（host `x86_64-pc-windows-msvc`） |
| C 编译器 | Visual Studio 2022 Community MSVC 19.44.35228（`cl` 不在 PATH；`build.rs` 自定位 `vcvars64.bat`，复验命令经 vcvars64 激活） |
| 系统代码页 | 936（中文 GBK）；仓库源码为 UTF-8 无 BOM |
| LLVM 22 dev | 不存在（无 `llvm-config`），未构建 `--features llvm` |
| git EOL | `core.autocrlf=true` + 仓库 `.gitattributes`（`eol=lf`），工作树 LF |
| 验证用 `dc` | `cargo build --release --bins`（HEAD + 本节修复） |

### 发现与修复

H19-07 报告的“macOS/Windows 未运行”在本机复验时暴露 6 个 Windows 专属缺陷；1–4 使 M19
在 Windows 上不可构建或不可运行，5–6 使 Windows lane 的门禁/测试失败。均为平台实现缺陷，
不涉及规格决策；Unix 源码与语义未改。

| # | 位置 | 根因（修复前） | 修复 | 引入批次 |
| --- | --- | --- | --- | --- |
| 1 | `runtime/windows_runtime.cpp` | `dolphin_stream_state`/`dolphin_owned_streams` 被放在 `#ifdef DOLPHIN_DEBUG_RUNTIME` 内；`build.rs` 无条件编译 Debug/Release 两份 runtime 对象，Release 分支缺少定义 → `cl` 失败 | 结构/登记表移到 `#ifdef` 之前（与 `unix_runtime.c` 布局一致） | H19-03 |
| 2 | `crates/dolphin-platform/build.rs` | `cl` 未加 `/utf-8`，MSVC 按系统代码页 936 解析 UTF-8 源码，中文注释吞掉后续字节 | `cl` 调用（直接与 `.bat` 回退两处）加 `/utf-8` | H19-01（该文件首次引入非 ASCII 注释） |
| 3 | `runtime/windows_runtime.cpp` | `dolphin_stream_release`/`dolphin_stream_from_raw` 缺 `extern "C"`；Windows 为 C++ 编译单元产生名称修饰，用户程序链接 `undefined symbol` | 两个定义加 `extern "C"` | H19-03 |
| 4 | `runtime/windows_runtime.cpp` | `dolphin_stream_read` 未把匿名管道写端关闭后的 `ERROR_BROKEN_PIPE`（109）按 EOF 处理；stdin 第二次读失败 | 该错误码返回 `Ok(0)`，与 `Ok(0)=EOF` 契约一致 | H19-02 |
| 5 | `tests/m19_io.rs` | `IO_WRITE_FAILURE_PROGRAM` 只在 `cfg(target_os = "linux")` 子用例使用，非 Linux 为 dead_code → `clippy --all-targets -- -D warnings` 失败 | 常量加 `#[cfg(target_os = "linux")]` | H19-02 |
| 6 | `tests/m19_test_cmd.rs` | TEST-05a 硬编码测试目标文件后缀 `.o`，Windows MSVC 产物为 `.obj` | 按 `cfg!(windows)` 选择后缀 | H19-05a |

### 修复前复现结果

```text
# 缺陷 1+2：Windows 任何 cargo build 都在 build.rs 编译 runtime 失败
error: failed to run custom build command for `dolphin-platform`
  thread 'main' panicked at crates\dolphin-platform\build.rs:228:9:
  `cl` failed to compile the Dolphin runtime
# 手工 cl（无 /utf-8，代码页 936）从 windows_runtime.cpp:324 起语法错误雪崩；
# 加 /utf-8 后 Debug 对象通过、Release 仍报 `dolphin_stream_state` 未定义（两缺陷独立）。

# 缺陷 3：修复 1/2 后 m19_app 6 项全失败
rust-lld: error: undefined symbol: dolphin_stream_release
rust-lld: error: undefined symbol: dolphin_stream_from_raw

# 缺陷 4：修复 3 后 m19_app 4 passed; 2 failed（app_02/app_05）
dtext: cannot read input (other)
# 探针：管道第二次 ReadFile 返回 ok=0 err=109 (ERROR_BROKEN_PIPE)

# 缺陷 5：cargo test 编译警告（clippy -D warnings 会失败）
warning: constant `IO_WRITE_FAILURE_PROGRAM` is never used

# 缺陷 6：m19_test_cmd 11 passed; 1 failed
dir=lib-only ... missing test object `...\target\test\alpha-tests.o`
```

修复前输出为本会话观测，未单独归档；最终证据日志在
`C:\Users\jiangyc\AppData\Local\Temp\opencode\h19-07-win\`（`workspace-test.log`、`m19_app.log`、
`smoke.log`），不在仓库内。

### 修复后结果

| 检查 | 结果 |
| --- | --- |
| `tests/m19_app.rs` | 6 passed（`app_06_write_failure_not_trap` 为 `#[cfg(target_os = "linux")]`，按冻结预期跳过）；Windows 目录错误类别 `(invalid)`、空格/Unicode 路径、stdin 管道读取均在用例内实际断言通过 |
| M19 其余套件 | `m19_args` 4、`m19_io` 4、`m19_fs` 6、`m19_text` 4、`m19_errors` 4、`m19_test_cmd` 12 全通过 |
| 完整默认 lane | `cargo test --workspace --exclude dolphin-codegen-llvm` = 364 passed; 0 failed（Linux 366；差值 2 为 `app_06` 与 `ffi06_shared_library_integration` 的既有平台门控） |
| 门禁 | `cargo fmt --all -- --check`、`cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings`、`cargo build --release --bins`、`git diff --check` 全部通过 |
| 手工冒烟（release `dc`，临时副本） | plain `dc build` 拒绝 path 依赖（exit 1、含 `path dependency`）；`--bin dtext` Debug/Release 构建成功；stdin `--filter a` → `lines=2 matched=1 bytes=4`、stderr 空、exit 0；`--help` 固定用法；目录输入 → `dtext: cannot open input (invalid)`；空格+中文路径文件 → `lines=2 matched=1 bytes=14`；`dc test textstats`/`dtext` 各 5 passed、exit 0 |

### 实际运行命令与测试数量

```powershell
# 均经 vcvars64 激活 MSVC
cargo build --bins                                                # 修复前失败；修复后通过
cargo test -p dolphin-compiler --test m19_app                     # 6 passed
cargo test -p dolphin-compiler --test m19_args --test m19_io --test m19_fs `
  --test m19_text --test m19_errors                               # 22 passed
cargo test -p dolphin-compiler --test m19_test_cmd                # 12 passed (61.0s)
cargo test --workspace --exclude dolphin-codegen-llvm             # 364 passed
cargo fmt --all -- --check                                        # 通过
cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings  # 通过
cargo build --release --bins                                      # 通过
.\target\release\dc.exe fmt --check examples crates/dolphin-std/src  # 通过
# examples/m19 手工冒烟见上表（脚本与日志在 h19-07-win\smoke.log）
```

### 未运行的检查及原因

- LLVM lane：本机无 LLVM 22 开发环境（`llvm-config` 缺失），`--features llvm`、`tests/backend.rs`
  与 LLVM × M19 组合未在 Windows 验证。
- 发行包（`scripts/package.py` 的 Windows zip）与 `ci.yml` 归档冒烟序列：本机复验聚焦 M19
  默认 lane，未运行；Windows 归档的 M19 冒烟仍待在 runner 或后续补充节确认。
- 远端 CI/tag：本机无 push/tag 权限，不伪造。
- macOS：无设备，未验证。
- 缺陷 5 的“修复前 clippy 失败”未单独跑一次 clippy 复现；`cargo test` 编译已直接输出该
  dead_code 警告，修复后 clippy 全绿。

### 行为/兼容变化

- Windows：修复 1 使 `cargo build` 恢复可用（H19-03 起 Windows 全平台构建失败）；修复 3 使
  `std.io.release`/`from_raw` 在 Windows 可链接；修复 4 使 Windows 管道 stdin 读取在 EOF 返回
  `Ok(0)`；修复 2 使中文代码页环境可构建；修复 5/6 使 Windows 的 clippy 门禁与 TEST-05a 断言成立。
- Unix：未触碰 `unix_runtime.c`；测试改动仅为平台门控/后缀选择，Unix 行为与断言不变。
- 无 API、CLI、IR、归档格式变化；所有修复只影响此前在 Windows 上失败的路径。

### 剩余问题和下一批输入

1. macOS 复验仍未运行（H19-07 剩余问题 1 的 macOS 部分）；本批未改 Unix 运行时，预计不受影响，
   但仍需 macOS 本机或 CI 确认。
2. 建议在 Windows 上补跑 `python scripts/package.py --target x86_64-pc-windows-msvc` 与
   `ci.yml` 归档冒烟（含 M19 步骤），或由远端 Windows runner 覆盖。
3. 本批修复未 commit/push/tag。缺陷 1 与代码页无关，H19-03..07 的 Windows 构建在本地必然失败；
   若远端 Windows lane 在此期间运行过，应同样失败。建议核对远端 lane 状态，不要把“由远端 CI
   lane 覆盖”当作已通过。
4. Windows 已复验；M19 阶段待 macOS 结果后由用户确认完成。

## H19-07-M macOS 平台复验

- 批次：H19-07 的 macOS 平台复验（补充节，不新增批次编号；含 2 个 macOS 受控读写失败用例新增）
- 状态：完成（macOS 26.6.2 arm64 本机：默认 Cranelift lane、M19 定向套件、`examples/m19` 手工冒烟、
  LLVM 22 lane、aarch64 发行包与归档冒烟全部通过；**未发现 macOS 编译器/运行时/标准库缺陷**；
  M18 已记录的 rust-lld/libLLVM 环境前提再次复现，见下；远端 CI/tag 未运行）
- 前置批次及报告：H19-07-W，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`36f0042`（`v0.3.0-M19-07-win`），工作区干净。
- 本批修改：`tests/m19_app.rs`（新增 2 个非 Linux Unix 受控读写失败用例）、`docs/plan-m18-plus.md`、
  本报告。**无编译器/运行时/标准库/示例/CI 源码改动**。未 commit/push/tag。

### 环境

| 项 | 值 |
| --- | --- |
| 平台 | macOS 26.6.2（25G83），arm64（Apple Silicon） |
| rustc / cargo | 1.98.1（host `aarch64-apple-darwin`） |
| C 编译器 | Apple clang 21.0.0（`/Library/Developer/CommandLineTools`） |
| LLVM 22 dev | `/opt/homebrew/opt/llvm@22`（22.1.8，`libPolly.a` 存在）；默认 lane 不依赖 |
| 验证用 `dc` | `cargo build --release --bins`（HEAD + 本节测试改动）与打包后的
  `dolphin-0.3.0-aarch64-apple-darwin.tar.gz` |

### 环境前提：rust-lld/libLLVM（M18 已记录，无新增源码改动）

不带 dyld 回退直接运行时，`dc` 链接每个 Dolphin 程序都失败：

```text
error[E0000]: linking `<...>/dtext` failed
dyld[...]: Library not loaded: @rpath/libLLVM.dylib
  Referenced from: .../.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/bin/rust-lld
```

根因与 H18-09-M 节相同：Rust 1.96+ 的 rustup `rust-lld` 动态依赖 `@rpath/libLLVM.dylib`，其 rpath
指向官方构建机路径，而库实际在 `$(rustc --print sysroot)/lib`。按 H18-09-M 已批准的本机做法，
本节所有命令均带（CI 的 macOS 步骤等价处理）：

```bash
export DYLD_FALLBACK_LIBRARY_PATH="$(rustc --print sysroot)/lib"
```

发行包不受影响：`scripts/package.py` 复制 `libLLVM.dylib` 并给 `rust-lld` 追加 `@loader_path`
rpath；归档冒烟用 `env -u DYLD_FALLBACK_LIBRARY_PATH ./dc env` 证明不依赖 rustup 工具链。

### 修改文件与关键实现

- `tests/m19_app.rs`（仅测试，无产品代码）：
  - `app_06b_write_failure_readonly_stdout`（`#[cfg(all(unix, not(target_os = "linux")))]`）：
    stdout 绑定只读 fd，`write_all` 失败 → exit 1、stderr `dtext: cannot write output (other)\n`、
    不 trap。Linux 已有 `/dev/full` 用例，排除 Linux 保持既有矩阵计数不变。
  - `app_06c_read_failure_writeonly_stdin`（同 cfg）：stdin 绑定只写 fd，`read` 失败 → exit 1、
    stdout 空、stderr `dtext: cannot read input (other)\n`、不 trap。这是“可控读失败”的第一个可移植
    注入（H19-07 Linux 节曾如实标注该路径无注入方式）。
  - 两个用例都在可用后端 × Dolphin Debug/Release 上 `dc build --bin dtext` 后运行，断言三路结果。

### 验收映射（macOS）

| 验收点 | 真实测试名/命令 | 后端/profile | 结果 |
| --- | --- | --- | --- |
| 布局/D1 + `--help` | `m19_app.rs::app_01_layout_build_and_help` | Cranelift × Debug/Release | 4 组合通过 |
| stdin 行规则/IO/ARGS/TEXT | `app_02_stdin_and_line_rules` | Cranelift × Debug/Release | 4 组合通过（空输入、正常 UTF-8、无末尾换行、无匹配、CRLF、Unicode、`-`） |
| FS/TEXT 文件 + 重复运行 | `app_03_file_input_and_repeated_runs` | Cranelift × Debug/Release | 4 组合通过（空格/中文路径；Debug 连跑 5 次 stderr 空） |
| 用法错误 | `app_04_usage_errors` | Cranelift × Debug/Release | 4 组合通过（exit 2 + 固定 stderr） |
| 打开/编码错误 | `app_05_open_and_encoding_errors` | Cranelift × Debug/Release | 4 组合通过（缺失 `not-found`、目录 `is-dir`、非法 UTF-8） |
| 可控写失败（Linux `/dev/full`） | `app_06_write_failure_not_trap` | — | macOS 按 cfg 跳过；由 `app_06b` 覆盖 |
| **可控写失败（macOS 新增）** | `app_06b_write_failure_readonly_stdout` | Cranelift × Debug/Release | 4 组合通过 |
| **可控读失败（macOS 新增）** | `app_06c_read_failure_writeonly_stdin` | Cranelift × Debug/Release | 4 组合通过 |
| 应用自测 | `app_07_dc_test_self_checks` | Cranelift × Debug/Release | 4 组合通过（两包各 `5 passed; 0 failed; 0 filtered out`） |
| ARGS-01..04 | `m19_args.rs` 4 项 | Cranelift × Debug/Release | 4 passed |
| IO-01..04 | `m19_io.rs` 4 项 | Cranelift × Debug/Release | 4 passed（IO-03 的 `/dev/full` 子用例 Linux-only） |
| FS-01..06 | `m19_fs.rs` 6 项 | Cranelift × Debug/Release | 6 passed（FS-03 的 `/dev/full` 子用例 Linux-only） |
| TEXT-01..04 | `m19_text.rs` 4 项 | Cranelift × Debug/Release | 4 passed |
| TEST-01..06 | `m19_test_cmd.rs` 12 项 | Cranelift × Debug/Release | 12 passed（60.3s，含 30s 超时用例） |
| ERR-01..04 | `m19_errors.rs` 4 项 | Cranelift × Debug/Release | 4 passed |
| 默认 lane 全量 | `cargo test --workspace --exclude dolphin-codegen-llvm` | Cranelift | 367 passed / 0 failed |
| LLVM lane 显式列表 | `DOLPHIN_BACKEND=llvm ... --features llvm` | Cranelift+LLVM × Debug/Release | 197 passed / 0 failed（`app_06` 除外） |
| LLVM lane 全量 | `DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm` | 两后端 | 374 passed / 0 failed |
| 后端一致性 | `cargo test -p dolphin-compiler --features llvm --test backend` | Cranelift+LLVM × Debug/Release | 4 passed（含 macOS DWARF 平台感知断言） |
| 发行包 | `scripts/package.py --target aarch64-apple-darwin` + 归档冒烟 | 归档 `dc`（默认） | tar.gz + sha256；`SMOKE_SEQUENCE_OK` |

### 修复前复现结果

- 无生产缺陷可修。第一次（不带环境变量）运行 `cargo test -p dolphin-compiler --test m19_app`：
  `0 passed; 6 failed`，全部为上面的 rust-lld/libLLVM 链接诊断；带已批准的环境变量后同一命令全绿，
  这是环境前提而非 macOS 语义缺陷。
- 新增用例首次运行：`app_06c` 的 fixture 用 `.write(true)` 打开不存在的文件，测试自身报
  `Os { code: 2, kind: NotFound }`；加 `.create(true).truncate(true)` 后通过（缺陷在测试代码，不在产品）。
  `app_06b` 首次即通过。
- 负向探针（Python `os.open` 以只读/只写 fd 作为 stdout/stdin）确认失败路径由受控 I/O 失败触发：
  read-only stdout → `dtext: cannot write output (other)` exit 1；write-only stdin →
  `dtext: cannot read input (other)` exit 1、stdout 空。

### 修复后结果

- `cargo test -p dolphin-compiler --test m19_app`：8 passed（默认）；`--features llvm` 同样 8 passed
  （每例 Cranelift+LLVM × Debug/Release）。
- 手工冒烟（release `dc`，临时副本）：66/66 断言通过——plain `dc build` 拒绝 path 依赖（D1）、
  `--bin` Debug/Release 构建、`--help`、stdin 全部行规则、空格+中文路径连跑 5 次 stderr 空、
  4 类用法错误、缺失/目录/非法 UTF-8、两包 `dc test` 各 5 passed。
- 发行包归档冒烟通过：m8（exit 64）、m14/m15/m18 固定输出且 stderr 空、m19 plain 拒绝 path 依赖 +
  `--bin` 构建运行固定输出、两包 `dc test` 5+5、m15math 打包、坐标消费与 `--locked --offline` 重建。
- fmt/clippy（默认与 `--features llvm`）/`git diff --check` 全部通过；无警告（带环境变量时
  `rust-objcopy` strip 也不告警）。

### 实际运行命令与测试数量

```bash
# 所有命令均带 export DYLD_FALLBACK_LIBRARY_PATH="$(rustc --print sysroot)/lib"
cargo build --bins                                                      # 通过
cargo test -p dolphin-compiler --test m19_app                           # 8 passed
cargo test -p dolphin-compiler --features llvm --test m19_app           # 8 passed
cargo test -p dolphin-compiler --test m19_args --test m19_io --test m19_fs \
  --test m19_text --test m19_errors --test m19_test_cmd                 # 4+4+6+4+4+12 passed
cargo test --workspace --exclude dolphin-codegen-llvm                   # 367 passed
cargo fmt --all -- --check                                              # 通过
cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings  # 通过
cargo build --release --bins                                            # 通过
./target/release/dc fmt --check examples crates/dolphin-std/src         # exit 0
cargo test -p dolphin-compiler --features llvm --test backend           # 4 passed
cargo clippy --workspace --all-targets --features llvm -- -D warnings   # 通过
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages --test doc_examples \
  --test m19_args --test m19_io --test m19_fs --test m19_text --test m19_test_cmd \
  --test m19_errors --test m19_app                                      # 197 passed
DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm        # 374 passed
python3 scripts/package.py --target aarch64-apple-darwin --out-dir /tmp/opencode/h19-07-mac/dist
#   tar.gz sha256 3958736335daaf10625bea97e39dd0ed6e499f6437db95036deeb70b1cbe2ffc
# 解压归档后按 ci.yml 冒烟序列（m8/m14/m15/m18/m19、m15math 打包、坐标消费、--locked --offline）
#   + m19 两包 dc test 5+5                                              # SMOKE_SEQUENCE_OK
```

日志与产物在 `/tmp/opencode/h19-07-mac/`（`m19_app.log`、`m19_suites.log`、`workspace-test*.log`、
`llvm-*.log`、`package.log`、`smoke*.log`、`archive-smoke.log`、`dist/`），不在仓库内。
新增用例属于已在 CI/README/installation 显式列表中的 `tests/m19_app.rs`，无需改 `--test` 列表。

### 未运行的检查及原因

- 远端 CI/tag：本机无 push/tag，未运行。
- Windows zip 归档冒烟不在本机范围（H19-07-W 已注明仍待 runner/后续补充）。
- LLVM lane：macOS 的 LLVM lane 不在 `ci.yml` 矩阵内（CI 的 LLVM lane 只要求 Ubuntu），
  本机用 `/opt/homebrew/opt/llvm@22` 补跑不改变 CI 覆盖面；`llvm-config` 不在 PATH，按前缀显式指定。
- 受控 `close(2)` 失败与 `EINTR` 注入仍无可移植方式（同 FS-03/规格 §16）。
- `/dev/full` 子用例（`app_06`、IO-03、FS-03 的对应子用例）在 macOS 按冻结预期跳过；
  macOS 的写失败由 `app_06b`、读失败由 `app_06c` 实际覆盖。
- SIGPIPE（下游提前关闭管道）行为未测试：规格未要求，Linux 行为相同。
- `--system-linker` 与 `dc test` 组合未单独运行（选项与 build 共用，链接器行为已由既有测试覆盖）。

### 行为/兼容变化

- 无产品行为变化；仅新增 2 个 `#[cfg(all(unix, not(target_os = "linux")))]` 测试，Linux 与 Windows 的
  测试矩阵计数不变（新用例在两个平台不编译）。
- macOS 复验未发现需要修改 `runtime/unix_runtime.c`、标准库、CLI 或链接器的缺陷；
  H19-07-W 的 Windows 修复在 macOS 无回归（Unix 运行时未被触碰）。
- `tests/m19_app.rs` 的新用例进入默认与 LLVM lane：macOS 默认 lane 365→367、LLVM 全量 372→374、
  LLVM 显式列表 195→197。

### 剩余问题和下一批输入

1. Linux、Windows、macOS 三平台 H19-07 复验均已完成（Linux/Windows 证据见前两节）；M19 阶段待用户
   确认完成后进入 M20。
2. 下一阶段 M20：先执行 H20-00 设计冻结（新建 `docs/proposal-m20-project-tools.md`），
   不提前实现项目分析或 LSP 代码。
3. macOS 本机开发若不带 `DYLD_FALLBACK_LIBRARY_PATH`，`dc` 链接仍会失败（M18 已记录、CI 已处理）；
   发行包不受影响。该环境项不是 M19 的产品缺陷，未在本批改产品代码。
4. 本报告不把 M19 阶段记为完成；macOS 结果已出，等待用户确认。

## M19 阶段验收与收尾（2026-09-23）

- 范围：H19-00..07 全部批次 + H19-07-W（Windows 复验/修复）+ H19-07-M（macOS 复验）
- 状态：**M19 完成**。代码实现、自动化验收、真实示例/工具流程、当前文档四种证据同时成立；
  三平台（Linux/Windows/macOS）默认后端与 Linux LLVM 已由用户本机复验，远端 GitHub CI 由用户确认通过。
- 收尾时 HEAD：`195e5b6`（`update gitignore`）；平台提交：`77c5717`（M19-07）、
  `36f0042`（M19-07-win）、`e28e8d6`（M19-07-mac）。本收尾只更新状态/文档，未 commit/push/tag。

### 阶段证据（四种）

| 证据 | 内容 |
| --- | --- |
| 代码实现 | `std.process`/`std.error`/`std.io`/`std.fs`/`std.text`（`lines`/`Builder`/`parse_i64`/`parse_u64`）/`std.test`；`dc run --` 转发；`dc test`（构建 + `tests/` 发现/harness + 子进程执行/超时/分类/过滤/汇总）；`examples/m19/textstats`（lib）与 `examples/m19/dtext`（lib+bin，path 依赖） |
| 自动化验收 | `tests/m19_args.rs`(ARGS-01..04)、`m19_io.rs`(IO-01..04)、`m19_fs.rs`(FS-01..06)、`m19_text.rs`(TEXT-01..04)、`m19_test_cmd.rs`(TEST-01..06，12 项)、`m19_errors.rs`(ERR-01..04)、`m19_app.rs`(7 项)；两示例包各自 `dc test` 5 项 |
| 真实示例/工具流程 | `dtext` 实际接收 stdin/文件、输出固定三行、错误诊断与退出码 0/1/2；发行包归档冒烟含 m19 构建/运行与两包 `dc test`（`SMOKE_SEQUENCE_OK`，Linux/macOS；Windows 由 CI） |
| 当前文档 | 本报告各批节；[规格](proposal-m19-cli-stdlib.md) 状态与 §11/§16；[路线图](../roadmap.md) M19 完成；[README](../README.md) 里程碑行；[implemented-features](../implemented-features.md) 头部/§1/§16.1/§17/§19；[交接指南](../plan-m18-plus.md) 状态表与 §5；[examples/m19/README](../../examples/m19/README.md) 与 examples 索引 |

### 平台与 CI

| 平台 | 默认后端（Cranelift × Debug/Release） | LLVM | 结论 |
| --- | --- | --- | --- |
| Linux x86_64 | 全量 `cargo test --workspace` 366 passed；`m19_app` 7 passed | 全量 373 passed、显式列表 196 passed、backend 4 passed | 通过（H19-07 节） |
| Windows x86_64 | `m19_app` 6 passed（`app_06` Linux-only 跳过）、其余 M19 套件与默认 lane 364 passed | 本机无 LLVM 22 未运行（CI 矩阵亦只要求 Linux LLVM） | 通过（H19-07-W 节，修复 4 个构建/运行时 + 2 个测试/门禁缺陷） |
| macOS arm64 | `m19_app` 8 passed、默认 lane 367 passed | LLVM 22 lane 374 passed、显式列表 197 passed、backend 4 passed | 通过（H19-07-M 节，无产品缺陷；新增受控读写失败用例） |
| 远端 CI | 用户确认 GitHub Actions 三平台质量门禁与归档冒烟通过 | 同上（Linux LLVM lane） | 通过（用户确认；本机不访问远端） |

### 收尾复核（本机在最终 HEAD 上重跑，仅 Linux）

```bash
cargo test -p dolphin-compiler --test m19_app                            # 7 passed
cargo test -p dolphin-compiler --features llvm --test m19_app            # 7 passed
cargo test --workspace --exclude dolphin-codegen-llvm                    # 366 passed
```

### 边界与未验证项

- 三平台结果来自用户本机会话与远端 CI；本收尾未在 Windows/macOS 重跑，也未访问远端仓库。
- Windows zip 归档冒烟的最终确认由 CI 承担（H19-07-W 本机未跑 `scripts/package.py --target
  x86_64-pc-windows-msvc`）。
- macOS 本机开发需 `DYLD_FALLBACK_LIBRARY_PATH`（M18 已记录、CI 已处理）；发行包不受影响。
- 受控 `close(2)` 失败、`EINTR` 注入、SIGPIPE 行为仍无可移植/规格要求的覆盖；`--system-linker`
  与 `dc test` 组合未单独运行。
- M19 明确不做：allocator 参数、隐式析构、异常、`?`、闭包、线程/异步、宏、HashMap、正则、
  JSON、网络、完整 Unicode 算法、稳定二进制 ABI（规格 §1 非目标）。

### 下一阶段

1. M20 从 H20-00 开始：新建 `docs/proposal-m20-project-tools.md`，冻结结构化诊断、共享项目分析
   快照、overlay 生命周期、lib/bin 选择与 LSP 测试协议；该批完成前不实现项目分析或 LSP 代码。
2. M19 的 `examples/m19` 与 `dc test` 是 M20 的验收对象（H20-04 的 FMT-06、H20-05 的整体流程）。
3. M21 仍为条件规划，需 M19/M20 的真实项目与测量后再选优化。
