# M18 进度报告

本文件按 [M18 执行合同](../plan-m18-correctness.md) 第 6 节模板追加批次记录，不覆盖前批证据。
每批一节；未实际运行的检查必须如实标注，不把本机结果写成三平台结论。

## H18-00 基线复核与测试矩阵准备

- 批次：H18-00
- 状态：完成（Linux x86_64 本机基线；不表示 M18 完成，也不代表 macOS/Windows 或远端 CI）
- 前置批次及报告：无

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`cab9009c612763dcab27c0892d9c301737732a6f`（`docs: plan-m18-plus`）。
- 审计提交 `a72db41f4c254a67d0be62be8adcc47598f54b34` 只作为审计记录引用；本批未执行任何
  checkout/reset，工作区始终停留在当前分支 HEAD。
- `git diff a72db41..HEAD --stat` 仅 `docs/`、`README.md` 变更；`crates/`、`src/`、`tests/` 与审计提交
  完全一致。因此第 3 节复现结论可直接对照审计记录。
- 开始时 `git status --porcelain --untracked-files=all` 为空，无用户改动需要保留。
- 本批新增且未提交的文件：`tests/support/mod.rs`、`tests/m18_harness.rs`（`??` 状态，未 commit）。
- 仓库根目录不存在 `AGENTS.md`，未自行编造规则。

### 环境

| 项 | 值 |
| --- | --- |
| 平台 | Linux x86_64（`Linux AppServer 7.2.3-zen1-2-zen`），glibc 运行时 |
| rustc | `rustc 1.97.1 (8bab26f4f 2026-07-14)` |
| cargo | `cargo 1.97.1 (c980f4866 2026-06-30)` |
| llvm-config | `22.1.8`（能链接 `--features llvm`，见下方 LLVM 门禁） |
| `DOLPHIN_BACKEND` | 未设置；`dc env` 报告默认后端 `cranelift` |
| 代理 | `HTTP_PROXY`/`HTTPS_PROXY` 已设置；`tests/packages.rs` 对回环 fixture 显式设置 `NO_PROXY=127.0.0.1,localhost`（提交 3df2725），本批测试均通过 |
| 链接器 | 默认 `rust-lld`，系统回退 `cc` |

### 修改文件与关键实现

仅新增测试基础设施，未改编译器源码：

- `tests/support/mod.rs`：共享最小驱动 `run_project(backend, profile, files, timeout) -> ProgramResult`。
  - 通过 `BuildSettings::with_backend(backend)` 显式选择 `BackendChoice`，通过参数显式传 `BuildProfile`；
    不读写 `DOLPHIN_BACKEND`，不在并行测试中修改全局环境变量。
  - 构建成功后用 `stdin=null`、`stdout/stderr=piped` 启动可执行文件；两个 reader 线程持续排空两路输出，
    主线程 `try_wait` + deadline 轮询，超时 `kill` 后 `wait` 回收，避免管道写满死锁和僵尸进程。
  - `ProgramResult` 携带 backend、profile、stdout、stderr、exit、timed_out、build_error，`context()`
    供断言失败时打印后端与 Dolphin profile。
- `tests/m18_harness.rs`：驱动自检（成功/编译失败/运行 trap/两种 profile/超时，LLVM 显式后端用例由
  `#[cfg(feature = "llvm")]` 门控）。未加入任何针对第 3 节已知缺陷的失败测试。

### 验收映射（H18-00 无编号验收点，映射到合同第 4 节操作与验收条款）

| 检查 | 测试/命令 | 后端/profile | 结果 |
| --- | --- | --- | --- |
| 驱动成功路径 | `tests/m18_harness.rs::harness_captures_success_stdout_and_exit` | Cranelift/Debug | 通过（exit=7、stdout=`ok 7\n`、stderr 空） |
| 驱动编译失败 | `harness_reports_compile_failure_without_running` | Cranelift/Debug | 通过（build_error 有诊断、未运行） |
| 驱动运行失败 | `harness_captures_runtime_trap_exit_and_stderr` | Cranelift/Debug | 通过（exit=101、stderr 含 `Dolphin runtime error`） |
| 显式 Dolphin profile | `harness_honors_dolphin_profile` | Cranelift/Debug+Release | 通过（Debug 报 leaked、Release 无泄漏报告，exit 均为 42） |
| 超时终止回收 | `harness_terminates_timeout_and_reaps_child` | Cranelift/Debug | 通过（500ms 超时被终止，耗时远小于 10s） |
| 显式 LLVM 后端 | `harness_selects_explicit_llvm_backend` | LLVM/Release | 通过（stdout=`llvm 12\n`） |

### 修复前复现结果

`dc` 由 `cargo build --bins --features llvm` 构建（Rust debug 二进制），四个程序放在
`/tmp/opencode/h18-00/repro/`。每个组合的命令形如：

```bash
target/debug/dc run --backend <cranelift|llvm> <--debug|--release> <输入>
```

3.2 的输入是项目目录 `/tmp/opencode/h18-00/repro/3.2-enum-layout`（含 `src/main.do`），其余为单文件；
单文件模式不能写 `use`，该限制已避开。全部 16 次运行的 exit 均为 0、stderr 均为空。

| 用例 | 源码 sha256（前 16 位） | cranelift Debug | cranelift Release | LLVM Debug | LLVM Release | 正确期望 |
| --- | --- | --- | --- | --- | --- | --- |
| 3.1 字段别名 | `5d907d63404ae243` | `7 2` | `7 2` | `7 2` | `7 2` | `7 9` |
| 3.2 枚举布局 | `94f906919447842c` | `12 8` | `12 8` | `12 8` | `12 8` | `16 8` |
| 3.3 浮点转整数 | `1312eac917bb49a4` | `2147483647` | `2147483647` | `-2147483648` | `1` | `2147483647` |
| 3.4 类型 bound | `80fa976a5a6b77ea` | 接受并输出 `42` | 接受并输出 `42` | 接受并输出 `42` | 接受并输出 `42` | 拒绝诊断 |

四例源码（与合同第 3 节一致）：

```dolphin
// 3.1 字段赋值丢失别名修改
struct Pair { x: i32, y: i32 }

fn mutate(p: *Pair): i32 {
    p->y = 9;
    return 7;
}

fn main() {
    var p = Pair(1, 2);
    p.x = mutate(&p);
    println("{} {}", p.x, p.y);
    return 0;
}
```

```dolphin
// 3.2 枚举布局（项目 src/main.do）
use std.mem;

enum Payload { Value(i64), Empty }

fn main() {
    println("{} {}", mem.size_of<Payload>(), mem.align_of<Payload>());
    return 0;
}
```

```dolphin
// 3.3 浮点转整数
fn convert(x: f64): i32 { return x as i32; }

fn main() {
    println("{}", convert(100000000000000000000.0));
    return 0;
}
```

```dolphin
// 3.4 类型声明 bound 被忽略
trait Mark { fn mark(self: *const Self): i32; }
struct Box<T: Mark> { value: T }

fn main() {
    val b = Box<i32>(42);
    println("{}", b.value);
    return 0;
}
```

结论：四类缺口在本机 HEAD 上全部复现，且比审计记录多测出两点——3.1 在四种组合都丢失别名修改；
3.3 的 LLVM Release `1` 是未定义行为的具体体现。复现原始 stdout/stderr/exit 保存在
`/tmp/opencode/h18-00/results/*.{stdout,stderr,exit}`（临时证据，不随仓库分发）。

#### 与入口代码的对应（供 H18-01 使用）

- 字段写入：`crates/dolphin-hir/src/lower.rs:1279` `lower_field_assignment` 发出
  `Instruction::SetField`；两个后端在发射 RHS 前先整值加载目标局部变量，再写回快照
  （Cranelift `codegen.rs:830-868`、LLVM `codegen.rs:1181-1235`），因此 RHS 通过别名写入的 `y=9`
  被旧快照覆盖。
- 索引写入：`lower.rs:1135` `lower_index_assignment` 使用 `SetIndex`/`SetIndexAt`
  （`crates/dolphin-ir/src/ir.rs:239-258`）。
- 3.2 入口：`crates/dolphin-ir/src/layout.rs` 的 `layout_of`/`enum_payload_components`。
- 3.3 入口：两个后端 cast 发射；LLVM 目前未使用饱和 intrinsic。
- 3.4 入口：`lower.rs` 的 `build_templates`，`monomorphize.rs` 的 `StructTemplate`/
  `instantiate_named`/`bind_param_bounds`。

### 基线门禁（改造前，clean HEAD；与合同 2.1 审计表逐项一致）

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace --exclude dolphin-codegen-llvm` | 229 passed；`tests/backend.rs` 为 0 项（feature 门控） |
| `cargo test --workspace --features llvm` | 233 passed；含 `tests/backend.rs` 4 项 |
| `DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test manifest --test packages` | 118 passed |
| `cargo fmt --all -- --check` | 通过 |
| `cargo clippy --workspace --all-targets --features llvm -- -D warnings` | 通过 |
| `git diff --check` | 通过，无空白错误 |

### 加入测试驱动后的门禁（同一 HEAD + 未提交新测试文件）

| 命令 | 结果 |
| --- | --- |
| `cargo test -p dolphin-compiler --test m18_harness` | 5 passed（默认 feature，仅 Cranelift） |
| `cargo test -p dolphin-compiler --features llvm --test m18_harness` | 6 passed（含 LLVM 显式后端） |
| `cargo test --workspace --exclude dolphin-codegen-llvm` | 234 passed（+5） |
| `cargo test --workspace --features llvm` | 239 passed（+6） |
| `DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test manifest --test packages --test m18_harness` | 124 passed（118+6） |
| `cargo test -p dolphin-compiler --features llvm --test backend` | 4 passed |
| `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets --features llvm -- -D warnings` / `git diff --check` | 均通过 |

原始日志在 `/tmp/opencode/h18-00/logs/`（`default-test*.log`、`llvm-test*.log`、
`llvm-env-tests*.log`、`m18-harness-*.log`、`fmt*.log`、`clippy*.log`）。

### 未运行的检查及原因

- 远端 CI、tag 与发布门禁：本批未 push/tag，且 H18-09 才负责 CI；本地不伪造远端结果。
- macOS/Windows：本机只有 Linux，未运行；相关 lane 留给 H18-09/H18-11。
- 合同 5.3 的 `dc fmt/check/run examples/m14|m15` 与发行打包：属于 H18-11 的集成验收，本批未执行。
- 四个失败本身未修复：合同规定失败回归随 H18-01/02/03/04 分别落地；本批不以此宣称 M18 通过。
- `cargo test --release` 未运行：它优化 Rust 测试程序，不改变 fixture 的 Dolphin profile；本批需要
  的 profile 覆盖已由复现矩阵和驱动的 Debug/Release 用例显式完成。

### 行为/兼容变化

无编译器行为或公开 API 变化；仅新增两个测试目标文件，默认测试总数 229→234，LLVM 233→239。
新增独立测试文件 `tests/m18_harness.rs` 尚未并入合同 5.2/CI 的显式 `--test` 列表，
H18-09 更新 CI 时必须加入；本地用 `--test m18_harness` 显式执行过。

### 剩余问题和下一批输入（H18-01）

1. 固定失败基线：PLACE-01 四组合均为 `7 2`（期望 `7 9`），复现源码同上，可用
   `tests/support/mod.rs::run_project` 直接转成失败回归。
2. H18-01 需新增 PLACE-02..08，并覆盖两后端 × 两 profile；驱动已支持显式选择，超时默认 30s。
3. 建议修复路径按合同执行：复用 `Place`/地址存储，消除整值快照写回；不接受只把整值读取移到 RHS 后的
   补丁。相关入口见上文“与入口代码的对应”。
4. 约束提醒：不要用环境变量切换后端；不要在并行测试中改 `DOLPHIN_BACKEND`；新增测试文件需进入显式
   `--test` 列表。
5. 未解决/待确认：无阻塞；H18-01 未开始，本报告不把四类缺陷记为已修复。

## H18-01 聚合写入与求值顺序

- 批次：H18-01
- 状态：完成（Linux x86_64；两后端 × Dolphin Debug/Release 本机验证；不表示其余批次或三平台）
- 前置批次及报告：H18-00，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`3d1d8487ab8c580bef91b4c3ee076ec0cd34c4c2`（`v0.2.0-M18-00`），工作区干净。
- 本批修改未提交：`crates/dolphin-hir/src/lower.rs`、`crates/dolphin-ir/src/ir.rs`、两个 codegen、
  新增 `tests/m18_place.rs`；文档更新 `language-design.md`、`implemented-features.md`、合同状态行。

### 修改文件与关键实现

- `crates/dolphin-ir/src/ir.rs`：删除 `Instruction::SetIndex`、`Instruction::SetField`；保留
  `SetFieldAt`/`SetIndexAt` 两个按 Place 地址写入的指令。
- `crates/dolphin-hir/src/lower.rs`：
  - `lower_field_assignment` 的普通局部目标改为 `PlaceKind::Local` + `SetFieldAt`，与 `p->field` 共用
    同一条地址写入路径；
  - `lower_index_assignment` 的数组目标改为 `SetIndexAt` + `PlaceKind::Index`（索引先存入临时局部，
    目标只求值一次）；
  - 复合赋值仍把“旧值读取”作为二元表达式的左操作数，在右值之前发射。
- 两个后端：删除整值加载/快照写回的 `SetField`/`SetIndex` 发射代码与字符串收集分支；`SetFieldAt`
  先在 RHS 前求地址，`SetIndexAt` 先求地址并对数组/切片做边界检查，两者都只写目标分量。
- `tests/m18_place.rs`：PLACE-01..08 固定期望回归，逐例遍历可用后端（Cranelift；`--features llvm`
  时含 LLVM）× Debug/Release。

### 验收映射

每个测试在 Cranelift+LLVM × Debug+Release 四个组合上运行并断言固定 stdout/exit/stderr；
`PLACE-01 -> tests/m18_place.rs::place_01_field_alias_keeps_rhs_side_effect`，其余依此类推：

| 验收点 | 测试名（tests/m18_place.rs） | 修复前 | 修复后（两后端 × 两 profile） |
| --- | --- | --- | --- |
| PLACE-01 | `place_01_field_alias_keeps_rhs_side_effect` | FAILED：`7 2`（期望 `7 9`） | 通过：`7 9\n`，exit 0，stderr 空 |
| PLACE-02 | `place_02_array_rhs_element_alias_preserved` | FAILED：`7 2 3`（期望 `7 20 3`） | 通过：`7 20 3\n`，exit 0 |
| PLACE-03 | `place_03_index_target_evaluated_once` | 通过（既有临时局部实现） | 通过：固定事件序列 `index 0/rhs 5/index 1/rhs 99`，`15 99 30 counter=2` |
| PLACE-04 | `place_04_rhs_target_and_other_field` | FAILED：`7 2`、`17 20` | 通过：`7 200`、`17 200`，事件序列固定 |
| PLACE-05 | `place_05_rejects_immutable_and_out_of_bounds_targets`、`place_05_dynamic_out_of_bounds_traps` | 通过（权限/静态越界） | 通过：5 类拒绝诊断 + 动态越界 exit 101、stderr 含 runtime error |
| PLACE-06 | `place_06_pointer_slice_and_aggregate_fields` | 通过（非目标路径） | 通过：`7 20 3 5 6 10 20`、`30 40 50 60` |
| PLACE-07 | `place_07_bounds_check_before_rhs` | FAILED：简单/复合都在 trap 前打印 `RHS-RAN` | 通过：stdout 无 `RHS-RAN`，exit 101 |
| PLACE-08 | `place_08_pointer_rebind_uses_pre_rhs_address`、`place_08_slice_descriptor_rebind_uses_pre_rhs_address` | 通过（`SetFieldAt`/`SetIndexAt` 原本先求地址） | 通过：`7 2 100 200`、`10 27 100 200` |

PLACE-08 的重绑定在源码中可表达：用 `mem.view<*Pair>(pp, 1)`/`mem.view<[]i32>(desc, 1)` 把指针局部或
切片描述符当作切片元素写入，无需新增左值语法；因此没有仅内部 IR 的替代回归。

### 修复前复现结果

1. 手工 `dc` 探针（修复前二进制）：PLACE-01 `7 2`；PLACE-02 `7 2 3`；PLACE-04 `7 2`、`17 20`；
   PLACE-07 先打印 `RHS-RAN` 再 exit 101。原始输出见上一批 `/tmp/opencode/h18-00/` 与本批
   `/tmp/opencode/h18-01/` 日志。
2. 永久回归在修复前代码上运行：`git stash push -- crates/` 后
   `cargo test -p dolphin-compiler --test m18_place` 结果为 `6 passed; 4 failed`，失败项正是
   PLACE-01/02/04/07，断言差异与文档期望完全一致，随后 `git stash pop` 恢复修复。

### 修复后结果

- `cargo test -p dolphin-compiler --test m18_place`：10 passed（Cranelift × Debug/Release）。
- `cargo test -p dolphin-compiler --features llvm --test m18_place`：10 passed（每例含 LLVM
  Debug/Release，即 40 次构建运行组合）。
- 全部成功例 stdout 为固定文本、stderr 为空、exit 0；PLACE-05/07 按契约 exit 101 且输出不含 RHS
  标记。IR/语言层面没有引入新左值语法，`val`、`*const`、切片只读与静态/动态越界行为不变。

### 实际运行命令与测试数量

```bash
git stash push -- crates/ && cargo test -p dolphin-compiler --test m18_place   # 预修复：6 passed; 4 failed
cargo test -p dolphin-compiler --test m18_place                               # 10 passed
cargo test -p dolphin-compiler --features llvm --test m18_place               # 10 passed
cargo test --workspace --exclude dolphin-codegen-llvm                         # 244 passed
cargo test --workspace --features llvm                                        # 249 passed
DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test manifest \
  --test packages --test m18_harness --test m18_place                         # 134 passed
cargo test -p dolphin-compiler --features llvm --test backend                 # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                          # 全部通过
```

默认测试总数 234→244，LLVM 239→249。日志在 `/tmp/opencode/h18-01/`。

### 未运行的检查及原因

- macOS/Windows：本机只有 Linux，未运行；留待 H18-09/H18-11。
- 远端 CI：本批未 push/tag；新增 `--test m18_place` 必须由 H18-09 写入 CI 显式列表。
- 合同 5.3 示例与发行打包：H18-11 范围，本批未执行。

### 行为/兼容变化

- 内部 IR 删除了两个仅由 lower 使用的指令，不构成公开 API 或持久格式变更。
- 语言行为变化仅是兑现既有按值/别名语义：字段与元素写入不再整值写回；目标地址与数组边界检查
  先于 RHS。完整对象赋值仍是按值复制，复合赋值仍读一次旧值。
- 文档同步：`language-design.md` 新增 8.1 赋值求值顺序；`implemented-features.md` 6.2/12.1 补充
  顺序说明，并把本缺陷从 1.1 已知问题移入 1.2 已修复记录。

### 剩余问题和下一批输入（H18-02）

1. H18-01 已消除整值快照写回；枚举布局批次不要重新引入按聚合整值覆盖的存储路径。
2. H18-02 输入：H18-00 复现 3.2 仍为 `12 8`（期望 `16 8`），全部四组合一致；入口
   `crates/dolphin-ir/src/layout.rs` 的 `layout_of`/`enum_payload_components`，以及两个后端枚举
   组件偏移、参数/返回/栈槽消费点。
3. H18-02 测试可复用 `tests/support/mod.rs` 与 `tests/m18_place.rs` 的矩阵写法；LAYOUT-02 需要
   `mem.alloc<Enum>` 逐元素初始化，LAYOUT-05 需要断言 Debug 无泄漏误报。
4. 无阻塞；本报告不把 H18-02..11 记为完成。

## H18-02 枚举布局与内存步长

- 批次：H18-02
- 状态：完成（Linux x86_64；两后端 × Dolphin Debug/Release 本机验证；不表示其余批次或三平台）
- 前置批次及报告：H18-01，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`6e3986538e8a1220cbc03632441db326a1786602`（`v0.2.0-M18-01`），工作区干净。
- 本批修改未提交：公共 layout、HIR 实例化检查、两个后端枚举分量偏移、测试驱动抽取与新增
  `tests/m18_layout.rs`；文档同步。

### 修改文件与关键实现

- `crates/dolphin-ir/src/layout.rs`：新增 `EnumLayout`（tag/payload offset、payload 分量数、size、align）
  与 `enum_layout_of`、`enum_payload_size_bytes`；枚举规则统一为 tag@0（i32）、payload@8、每分量 8
  字节、align=8、size=8+n*8；无 payload 时 size=4、align=4。`layout_of(Enum)` 改用该描述。
  新增 `checked_layout_of`/`checked_size_of`：以 `u64` 精确计算，供实例化检查预算，不做静默饱和。
- `crates/dolphin-hir/src/monomorphize.rs`：`instantiate_named_at` 在 `ensure_acyclic` 后调用
  `validate_aggregate_size`，尺寸超过 `MAX_AGGREGATE_BYTES` 时给出
  “type `X` is too large for the target” 诊断。
- 两个后端：`component_layout` 的枚举分支改为消费 `layout::enum_layout_of`，不再各自保留 `4 + ...`
  常量；Cranelift 删除重复的 `uniform_variant_components`。
- `tests/support/mod.rs`：把“可用后端 × Debug/Release 断言”抽成共享 helper
  （`backends`/`assert_runs`/`assert_rejected`/`assert_traps`/`assert_traps_without_stdout`）；
  `tests/m18_place.rs` 改用共享 helper，行为不变。
- `tests/m18_layout.rs`：LAYOUT-01..06 回归；`layout.rs` 内新增 4 个内部边界单测（不分配超大内存）。

### 验收映射

每个集成测试在 Cranelift+LLVM × Debug+Release 四个组合上运行并断言固定 stdout/exit/stderr：

| 验收点 | 测试名 | 修复前 | 修复后（两后端 × 两 profile） |
| --- | --- | --- | --- |
| LAYOUT-01 | `tests/m18_layout.rs::layout_01_enum_sizes_and_alignments`；`layout::tests::enum_without_payload_is_four_four`、`enum_payload_starts_at_eight_and_size_is_padded` | FAILED：`4 8 / 12 8 / 28 8` | 通过：`4 4 / 16 8 / 32 8`，并断言每个非零类型 `size % align == 0` |
| LAYOUT-02 | `layout_02_enum_slice_alloc_address_and_write` | 通过 | 通过：逐元素初始化、`&items[1]` 取址读、`items[1]` 重写，`-1 20 30`，无泄漏 |
| LAYOUT-03 | `layout_03_enum_in_struct_and_struct_in_enum`；`layout::tests::enum_in_struct_uses_aligned_field_offsets` | FAILED：`24 8 4 2.5` | 通过：`32 8 4 2.5`，字段偏移 0/8/24、尾部补齐到 32 |
| LAYOUT-04 | `layout_04_payloads_match_params_returns_and_copy` | 通过（旧步长自洽） | 通过：string/指针/结构体/Option/Result 经 match、传参、返回与 `mem.copy`，`124 30 1` |
| LAYOUT-05 | `layout_05_free_full_slice_without_mismatch`、`layout_05_dynamic_bounds_trap_in_both_profiles` | 通过 | 通过：完整切片 free 无误报且 stderr 无 leaked；动态越界两 profile 均 exit 101 |
| LAYOUT-06 | `layout_06_rejects_oversized_nested_aggregate`；既有 `tests/ffi.rs` extern struct 套件 | FAILED：静默构建、`size_of` 饱和为 `4294967295` | 通过：构建期报 “type `DoubleChunk` is too large for the target”；`enum_payload_size_is_exact_for_max_components` 证明 u32::MAX 分量也精确不回绕；extern struct 全回归通过 |

### 修复前复现结果

在 H18-01 提交（`6e39865`）的代码上先运行新增回归（未改动编译器）：

```text
test result: FAILED. 4 passed; 3 failed
layout_01: left "4 8\n12 8\n28 8\n"  right "4 4\n16 8\n32 8\n"
layout_03: left "24 8 4 2.5\n"       right "32 8 4 2.5\n"
layout_06: exit=Some(0) build_error=None（静默接受超预算类型）
```

LAYOUT-02/04/05 修复前通过，属于防止修复回退的回归；日志在
`/tmp/opencode/h18-02/pre-fix-tests.log`。

### 修复后结果

- 原审计复现 3.2 经 `dc run --backend <cranelift|llvm> <--debug|--release>`：四个组合均为 `16 8`，exit 0。
- 所有 LAYOUT 成功例 stdout 为固定文本、stderr 为空、exit 0；越界例 exit 101；超预算例为编译诊断而非
  wrap、饱和或巨量 IR。
- C extern struct 布局未受影响（`tests/ffi.rs` 在完整门禁中通过）。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m18_layout --test m18_place     # 7 + 10 passed
cargo test -p dolphin-compiler --features llvm --test m18_layout      # 7 passed（每例 4 组合）
cargo test -p dolphin-ir --lib layout                                 # 4 passed（内部边界单测）
cargo test --workspace --exclude dolphin-codegen-llvm                 # 255 passed
cargo test --workspace --features llvm                                # 260 passed
DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test manifest \
  --test packages --test m18_harness --test m18_place --test m18_layout   # 141 passed
cargo test -p dolphin-compiler --features llvm --test backend         # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                  # 全部通过
```

默认测试总数 244→255，LLVM 249→260（新增 m18_layout 7 项与 layout 单测 4 项，m18_place 仍 10 项）。
日志在 `/tmp/opencode/h18-02/`。

### 未运行的检查及原因

- macOS/Windows：本机只有 Linux，未运行；留待 H18-09/H18-11。
- 远端 CI：本批未 push/tag；新增 `--test m18_layout` 必须由 H18-09 写入 CI 显式列表。
- 合同 5.3 示例与发行打包：H18-11 范围，本批未执行。
- 布局的外部二进制兼容无意保证：本批只修复内部布局，文档明确不承诺稳定 ABI。

### 行为/兼容变化

- 枚举内存布局变化：size `12→16`（一分量 payload）、无 payload align `8→4`、payload 偏移 `4→8`；
  枚举嵌 struct 的字段偏移与切片步长随之修正。公共 API/源码语义无变化，`.dlib` 不承诺稳定 ABI。
- 超过 `MAX_AGGREGATE_BYTES` 的嵌套 struct/enum 现在在实例化时给出诊断，不再静默饱和；数组既有检查不变。
- 测试基础设施抽取无行为变化；`tests/m18_place.rs` 仍全绿。

### 剩余问题和下一批输入（H18-03）

1. H18-00 复现 3.3：Cranelift 两 profile 为 `2147483647`，LLVM Debug `-2147483648`、Release `1`；
   正确期望 `2147483647`，两后端 × 两 profile 必须一致。
2. H18-03 入口：两个 codegen 的 cast 发射路径；LLVM 优先使用 `llvm.fptosi.sat`/
   `llvm.fptoui.sat`（LLVM 22/inkwell 实际接口）。
3. 测试矩阵可直接复用 `tests/support/mod.rs` 的 `assert_runs` 等 helper；CAST-02 需要由固定期望构造
   NaN/±Inf/边界值（通过运行时参数，不只常量折叠路径）。
4. 无阻塞；本报告不把 H18-03..11 记为完成。

## H18-03 饱和数值转换

- 批次：H18-03
- 状态：完成（Linux x86_64；两后端 × Dolphin Debug/Release 本机验证；不表示其余批次或三平台）
- 前置批次及报告：H18-02，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`e9c09b6f88cf9e7c852fac934747546db79f8174`（`v0.2.0-M18-02`），工作区干净。
- 本批修改未提交：两个 codegen 的浮点转整数发射、新增 `tests/m18_cast.rs`、数值规范与已知问题文档。

### 修改文件与关键实现

- `crates/dolphin-codegen-llvm/src/codegen.rs`：浮点到整数改用 `llvm.fptosi.sat`/
  `llvm.fptoui.sat` intrinsic（`Intrinsic::find` + `get_declaration`，重载类型按“返回类型、操作数
  类型”顺序），不再发射非饱和的 `fptosi`/`fptoui`。
- `crates/dolphin-codegen-cranelift/src/codegen.rs`：新增 `emit_saturating_float_to_int`。
  Cranelift 0.134 的 x64 饱和序列只支持 32/64 位目标，直接对 I8/I16 发射
  `fcvt_to_*_sat` 会在 `emit.rs` 触发内部 `unreachable`；窄目标改为先按 I32 饱和，再按最终位宽
  （有符号用有符号比较，无符号用无符号比较）夹紧后 `ireduce`，结果仍由最终位宽决定。
- `tests/m18_cast.rs`：CAST-01 固定输出；CAST-02 在测试内生成覆盖 `f32/f64` ×
  `i8/i16/i32/i64/isize/u8/u16/u32/u64/usize` 的程序，期望由 Rust 自身的浮点到整数 `as`
  语义（截断、饱和、NaN→0）生成，不调用待测后端作为 oracle。NaN/±Inf 由
  `fdiv(0.0/0.0)`、`fdiv(±1.0/0.0)` 经函数参数在运行时构造，不只走前端常量路径。

### 验收映射

| 验收点 | 测试名（tests/m18_cast.rs） | 修复前 | 修复后（Cranelift+LLVM × Debug+Release） |
| --- | --- | --- | --- |
| CAST-01 | `cast_01_float_to_int_saturates` | FAILED（LLVM Debug `-2147483648`；Cranelift 通过） | 通过：四组合均 `2147483647\n`，exit 0，stderr 空 |
| CAST-02 | `cast_02_full_matrix_saturates_with_nan_zero` | FAILED（Cranelift x64 对窄目标 `unreachable` 内部 ICE；LLVM 未兑现饱和） | 通过：20 条输出行（2 种浮点源 × 10 个整数目标）逐值匹配 Rust `as` 期望 |

CAST-02 覆盖的每类输入：正负小数、2.5/-2.5、±0（`0.0` 与 `fneg(0.0)`）、NaN、±Inf、各目标
near-hi/over-hi/near-lo/under-lo（i64 用 `2^63-1024`、`2^63`、`-2^63`、`-2^63-2048`；u64 用
`2^63`、`2^64`；窄目标用位宽边界）、负数到无符号、`1e20`。i64/u64 结果以完整十进制文本断言，
不经退出码。

### 修复前复现结果

```text
CAST-01（--features llvm）：left "-2147483648\n"  right "2147483647\n"
CAST-02（默认 Cranelift）：panicked at cranelift-codegen-0.134.2 .../inst/emit.rs:1057:
    internal error: entered unreachable code
CAST-01 默认 Cranelift：通过（既有 fcvt_to_sint_sat）
```

日志在 `/tmp/opencode/h18-03/pre-llvm.log`、`pre-cranelift.log`。

### 修复后结果

- 原审计复现 3.3 经 `dc run --backend <cranelift|llvm> <--debug|--release>`：四个组合均
  `2147483647`，exit 0。
- 两后端、两 profile 对 CAST-01/02 固定期望全部通过；整数窄化/符号扩展、浮点算术与既有 trap
  规则由完整回归覆盖未变。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m18_cast                          # 2 passed
cargo test -p dolphin-compiler --features llvm --test m18_cast         # 2 passed（每例 4 组合）
cargo test --workspace --exclude dolphin-codegen-llvm                   # 257 passed
cargo test --workspace --features llvm                                  # 262 passed
DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test manifest \
  --test packages --test m18_harness --test m18_place --test m18_layout --test m18_cast
                                                                        # 143 passed
cargo test -p dolphin-compiler --features llvm --test backend           # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                    # 全部通过
```

默认测试总数 255→257，LLVM 260→262。日志在 `/tmp/opencode/h18-03/`。

### 未运行的检查及原因

- macOS/Windows：本机只有 Linux，未运行；窄目标饱和转换的 Cranelift 修复路径在 x64 上验证，
  其他平台同属 Cranelift 的 32/64 位饱和序列，留待 H18-09/H18-11 平台验收。
- 远端 CI：本批未 push/tag；新增 `--test m18_cast` 必须由 H18-09 写入 CI 显式列表。
- 合同 5.3 示例与发行打包：H18-11 范围，本批未执行。

### 行为/兼容变化

- LLVM 浮点到整数不再产生 poison/不确定值：越界饱和、NaN→0。此前依赖 LLVM 非饱和输出的程序
  行为改变，但旧行为不是承诺语义。
- Cranelift 侧只改变窄目标发射方式，32/64 位语义不变；同时消除了一处编译器内部 panic。
- 规范补充：`language-design.md` 第 5.1 节加入饱和表与窄目标说明；`implemented-features.md`
  第 4.6 节与 1.2 已修复记录同步。

### 剩余问题和下一批输入（H18-04）

1. H18-00 复现 3.4：`Box<i32>` 被接受并输出 `42`（四组合一致），期望拒绝 “`i32` 没有 `Mark` 实现”。
2. 入口：`crates/dolphin-hir/src/lower.rs` 的 `build_templates`；`monomorphize.rs` 的
   `StructTemplate`/`EnumTemplate`、`instantiate_named`/`instantiate_named_at`、`bind_param_bounds`。
   H18-02 已把 `validate_aggregate_size` 挂在 `instantiate_named_at` 的公共出口，bound 检查可复用
   同一入口，但要区分定义处作用域与实例化位置诊断。
3. 需要覆盖 GEN-01..05：struct/enum 正反例、签名/嵌套/跨模块/跨包、关联类型 `T::Item`、
   已有函数 bound 与 stdlib 全回归；错误要说明类型、缺失 trait 与实例化位置。
4. 测试可复用 `tests/support/mod.rs` 的 `assert_runs`/`assert_rejected`；未实例化泛型函数体是否完整
   检查不在本批承诺。
5. 无阻塞；本报告不把 H18-04..11 记为完成。

## H18-04 类型声明泛型约束

- 批次：H18-04
- 状态：完成（Linux x86_64；两后端 × Dolphin Debug/Release 本机验证；不表示其余批次或三平台）
- 前置批次及报告：H18-03，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`499f54cc9d5ee14b31e1dcc141e4f39faa22abfc`（`v0.2.0-M18-03`），工作区干净。
- 本批修改未提交：模板表/HIR 实例化约束检查、`tests/m18_bounds.rs`、`tests/manifest.rs` 跨包用例、
  `tests/support/mod.rs` 多条件断言 helper 与文档同步。

### 修改文件与关键实现

- `crates/dolphin-hir/src/lower.rs`：`StructTemplate`/`EnumTemplate` 新增 `bounds` 字段；新增
  `type_param_bounds` helper，struct/enum/function 模板统一保留“参数名 -> 限定 trait 名”的单约束。
- `crates/dolphin-hir/src/monomorphize.rs`：`bind_param_bounds` 改为直接接收模板 bounds（不再按名字
  二次查询，避免同名类型/函数命名空间混淆）；在 `instantiate_named_at` 的公共入口（占位类型登记后、
  字段/variant 解析前）调用它：先校验约束，再把 `T::Item` 绑定进实例环境；删除只查函数的
  `TemplateTables::bound_of`。递归占位、实例去重与膨胀限制保持不变。
- `tests/m18_bounds.rs`：GEN-01..05 回归；`tests/manifest.rs` 增加
  `h18_04_type_bounds_across_path_dependency`（path 依赖包提供 trait/类型，根包使用）。
- `tests/support/mod.rs`：新增 `assert_rejected_messages`（多关键字拒绝断言）。

### 验收映射

| 验收点 | 测试名 | 修复前 | 修复后（Cranelift+LLVM × Debug+Release） |
| --- | --- | --- | --- |
| GEN-01 | `tests/m18_bounds.rs::gen_01_ignored_type_bound_is_rejected` | 接受并运行（`42`） | 通过：拒绝 `type i32 cannot implement trait Mark`，exit 1 |
| GEN-02 | `gen_02_positive_hold_and_maybe`、`gen_02_negative_hold_and_maybe` | 正例通过、反例被接受 | 正例 `1 -1\n`；`Hold<Bad>`/`Maybe<Bad>` 均拒绝且诊断含类型、trait、实例化位置 |
| GEN-03 | `gen_03_positive_signature_and_nested`、`gen_03_negative_nested_and_signature`、`gen_03_cross_module_trait_identity`、`gen_03_cross_module_negative_uses_qualified_trait`、`tests/manifest.rs::h18_04_type_bounds_across_path_dependency` | 反例被接受；同包正例通过 | 签名/嵌套正例 `5 5\n`；反例拒绝；跨模块同名 trait 不误匹配（诊断使用 `util.holding.Has`）；跨 path 依赖正例 exit 42、反例拒绝 |
| GEN-04 | 既有 `m15_generic_functions_types_and_methods`、`m15_type_parameter_bounds_resolve_associated_types`、`m15b_iterator_protocol_and_rejection`、`rejects_unbounded_generic_type_expansion` 与完整门禁 | 通过 | 全量回归通过（默认 267、LLVM 272） |
| GEN-05 | `gen_05_associated_type_fields_and_payloads`、`gen_05_negative_type_and_missing_impl` | 正例报 `unknown type T::Item` | 正例 `1 -1 2\n`；`ItemBox<Good>(true)` 拒绝为 `expected i32, found bool`；`ItemBox<Bad>` 与 `ItemMaybe<Bad>` 拒绝 |

### 修复前复现结果

```text
3.4 / GEN-01：接受并输出 42（四组合一致）
GEN-02 反例：Hold<Bad>、Maybe<Bad> 接受
GEN-05 正例：error[E0001]: unknown type `T::Item`（struct/enum 字段均失败）
```

预修复 `tests/m18_bounds.rs`：`3 passed; 6 failed`；日志 `/tmp/opencode/h18-04/pre-fix.log`。

### 修复后结果

- 3.4 经 `dc run` 四组合均为 `error[E0001]: type i32 cannot implement trait Mark` 且 exit 1。
- 所有正例 stdout 固定、stderr 空、exit 0；反例诊断包含类型、缺失 trait 与实例化位置。
- 错误中的类型/trait 名按定义处限定名解析：同包显示 `Bad`/`Has`，跨包显示包前缀（当前为
  `@N.Bad`/`@N.Has`），不显示 `struct@N`；跨包可读包名属于既有显示限制，不在本批变更。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m18_bounds                          # 9 passed
cargo test -p dolphin-compiler --features llvm --test m18_bounds         # 9 passed（每例 4 组合）
cargo test -p dolphin-compiler --test manifest h18_04                    # 1 passed
cargo test --workspace --exclude dolphin-codegen-llvm                    # 267 passed
cargo test --workspace --features llvm                                   # 272 passed
DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test manifest \
  --test packages --test m18_harness --test m18_place --test m18_layout --test m18_cast \
  --test m18_bounds                                                      # 153 passed
cargo test -p dolphin-compiler --features llvm --test backend            # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                     # 全部通过
```

默认测试总数 257→267，LLVM 262→272。日志在 `/tmp/opencode/h18-04/`。

### 未运行的检查及原因

- macOS/Windows：本机只有 Linux，未运行；留待 H18-09/H18-11。
- 远端 CI：本批未 push/tag；新增 `--test m18_bounds` 必须由 H18-09 写入 CI 显式列表。
- 合同 5.3 示例与发行打包：H18-11 范围，本批未执行。
- 未实例化泛型函数体的完整检查仍是另一项前端设计，不在本批承诺；具名类型一旦被实例化即受检。

### 行为/兼容变化

- 之前被静默接受的越界类型实例化（如 `Box<i32>` 上不满足的 bound）现在给出编译诊断；这是兑现
  既有规格，不新增语法。
- 之前报 `unknown type T::Item` 的具名类型现在可正常使用关联类型字段/payload；错误信息改为含
  类型、trait 与实例化位置。
- 诊断中的跨包限定名仍使用 `@N` 包前缀（既有行为），未新增包坐标渲染。

### 剩余问题和下一批输入（H18-05）

1. H18-05 范围：`impl` 头目标实参不再解析后丢弃；当前静态发现 `discard_type_arguments`，需覆盖
   IMPL-01..04（正例 `impl<U> Box<U>` 与 `Box<T: Has>` 组合、反例头、同名 trait 方法、跨包参数化 impl）。
2. 本批已把类型模板 bound 放进 `StructTemplate`/`EnumTemplate.bounds`；H18-05 的 impl 目标校验可复用
   限定名与实例化入口，但不要在 impl 头忽略实参。
3. 无阻塞；本报告不把 H18-05..11 记为完成。

## H18-05 impl 头与不支持语法

- 批次：H18-05
- 状态：完成（Linux x86_64；两后端 × Dolphin Debug/Release 本机验证；不表示其余批次或三平台）
- 前置批次及报告：H18-04，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`bb8502592860e2aaa54de48f1f12e595b8fd6cb5`（`v0.2.0-M18-04`），工作区干净。
- 本批修改未提交：parser/AST impl 目标实参、模块解析、模板表声明处校验、方法实例化参数环境，
  以及 `tests/m18_impl.rs`、`tests/manifest.rs` 跨包用例。

### 修改文件与关键实现

- `crates/dolphin-syntax/src/{ast.rs,parser.rs}`：`ImplBlock` 新增 `trait_arguments`/`type_arguments`；
  `parse_impl` 解析并保留目标与 trait 的类型实参（删除只解析后丢弃的 `discard_type_arguments`）。
- `crates/dolphin-hir/src/modules.rs`：impl 目标/trait 的类型实参参与 `resolve_type_ref` 限定。
- `crates/dolphin-hir/src/lower.rs`：新增 `validate_impl_header`，在 `build_templates` 里对每个 impl
  在声明处校验并诊断：未知目标、blanket impl、泛型 trait 实参、impl 参数 bound、方法独立泛型参数、
  具体类型特化、impl 与声明参数数量不一致、目标实参数量/顺序/嵌套/重复。
- `crates/dolphin-hir/src/monomorphize.rs`：`instantiate_method` 用**声明处参数名**实例化目标类型
  （impl 参数允许改名），使字段环境与 H18-04 的 bound/关联类型检查在方法路径同样生效。
- `tests/m18_impl.rs`：IMPL-01..03；`tests/manifest.rs`：
  `h18_05_parameterized_impl_across_path_dependency`（依赖包内参数化固有 + trait impl）。

### 验收映射

| 验收点 | 测试名 | 修复前 | 修复后（Cranelift+LLVM × Debug+Release） |
| --- | --- | --- | --- |
| IMPL-01 | `impl_01_construct_then_call_renamed_params`、`impl_01_associated_function_first_instantiation`、`impl_01_bound_checked_through_method_instantiation` | 构造后调用因类型缓存偶然通过；关联函数首实例化报 `unknown type T`；`Box<Bad>` 经方法路径未按声明 bound 拒绝 | 三个程序分别通过：`7\n`、`7\n`、`3\n`；`Box<Bad>::make` 拒绝为 `Bad does not implement Has` |
| IMPL-02 | `impl_02_specialization_is_rejected`、`impl_02_missing_and_repeated_arguments_are_rejected`、`impl_02_reordered_and_arity_mismatch_are_rejected`、`impl_02_nested_argument_is_rejected`、`impl_02_blanket_impl_is_rejected`、`impl_02_impl_parameter_bound_is_rejected`、`impl_02_method_type_parameter_is_rejected`、`impl_02_generic_trait_argument_is_rejected`、`impl_02_unknown_target_is_rejected` | 全部静默接受 | 全部在声明处拒绝，含特化/顺序/嵌套/参数数量/blanket/bound/方法泛型/trait 实参/未知类型关键字 |
| IMPL-03 | `impl_03_same_name_method_conflict_is_rejected` | 通过（既有规则） | 通过：同名方法重复与 trait/固有冲突均报 `already defined` |
| IMPL-04 | `tests/manifest.rs::h18_05_parameterized_impl_across_path_dependency` | 未被覆盖 | 通过：依赖包内 `impl<U> Holder<U>` 与 `impl<U> Wrap for Holder<U>`，根包经 `use math.Wrap` 调用，exit 41 |

### 修复前复现结果

```text
p1/p2（impl<U> Box<U> + 取值/关联函数）：unknown type `T`（声明字段无法解析）
p3 impl Box<i32>、p4 impl<T> Box、p5 impl<A,B> Pair<B,A>、p6 impl<T> Box<Box<T>>、
p7 impl<T> Has for T、p8 impl<T: Has> Box<T>、p9 方法 <V>、p11 重复实参、p12 参数数量：
全部 dc check 通过（静默接受）
tests/m18_impl.rs 预修复：2 passed; 11 failed
```

日志在 `/tmp/opencode/h18-05/pre-fix.log` 与 `/tmp/opencode/h18-05/p*/`。

### 修复后结果

- `impl<U> Box<U>` 改名参数在“先构造再调方法”和“关联函数首次实例化”两条路径都正确；
  `Box<Good>` 经方法路径仍通过，`Box<Bad>` 经方法路径按声明 bound 拒绝（不被类型缓存掩盖）。
- 所有反例在声明处给出带 span 的诊断（例如 `impl target must use the impl type parameters in order`、
  `blanket impl is not supported`、`concrete impl targets for generic types are not supported`）。
- 既有正例（`impl Type`、`impl<T> Box<T>`、`impl<T> Iterator for SliceIter<T>`、跨包参数化 impl）
  与完整 stdlib 回归通过。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test m18_impl                            # 13 passed
cargo test -p dolphin-compiler --features llvm --test m18_impl           # 13 passed（每例 4 组合）
cargo test -p dolphin-compiler --test manifest h18_05                     # 1 passed
cargo test --workspace --exclude dolphin-codegen-llvm                     # 281 passed
cargo test --workspace --features llvm                                    # 286 passed
DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test manifest \
  --test packages --test m18_harness --test m18_place --test m18_layout --test m18_cast \
  --test m18_bounds --test m18_impl                                        # 167 passed
cargo test -p dolphin-compiler --features llvm --test backend              # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                       # 全部通过
```

默认测试总数 267→281，LLVM 272→286。日志在 `/tmp/opencode/h18-05/`。

### 未运行的检查及原因

- macOS/Windows：本机只有 Linux，未运行；留待 H18-09/H18-11。
- 远端 CI：本批未 push/tag；新增 `--test m18_impl` 必须由 H18-09 写入 CI 显式列表。
- 合同 5.3 示例与发行打包：H18-11 范围，本批未执行。
- 本批未新增“固有方法只能在类型所属包定义”的强制检查；跨包用例改为在依赖包内定义 impl，
  不依赖未冻结的跨包固有实现行为。

### 行为/兼容变化

- 之前被静默接受的 impl 头形式现在在声明处拒绝，即使 impl 为空或方法从未调用；这是兑现合同的
  拒绝要求，不新增特化/blanket impl 支持。
- `impl<U> Box<U>` 这类改名参数形式由不可用变为可用（字段按声明参数解析）。
- 方法路径现在复用 H18-04 的声明 bound 与关联类型检查，避免类型缓存掩盖错误。

### 剩余问题和下一批输入（H18-06）

1. H18-06 第一层 IR 校验（ID/索引、TypeDef 种类、普通函数 entry/跳转、类型一致、instruction/location
   数量、source 引用）与第二层 LLVM verifier 接入，需覆盖畸形 IR 单测和正常程序全回归。
2. 本批 `validate_impl_header` 只在 `build_templates` 声明处运行；H18-06 的 IR 校验不依赖它，但
   类型/Place 校验需与 impl 目标实参的既有约定保持一致。
3. 本批的诊断渲染中跨包名仍是 `@N` 包前缀（既有行为）。
4. 无阻塞；本报告不把 H18-06..11 记为完成。

## H18-06 IR 与 LLVM 校验

- 批次：H18-06
- 状态：完成（Linux x86_64；两后端 × Dolphin Debug/Release 本机验证；不表示其余批次或三平台）
- 前置批次及报告：H18-05，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`e2976decd0d612d3de2bf3c1d5299311dbb4ceda`（`v0.2.0-M18-05`），工作区干净。
- 本批修改未提交：新增 `dolphin-ir::verify` 与单测、HIR 统一出口调用、LLVM 两阶段
  `module.verify()` 与入口块 sret 槽、`tests/m18_sret.rs`、文档同步。

### 修改文件与关键实现

- `crates/dolphin-ir/src/verify.rs`（新增）：`verify_program(&Program) -> Result<(), VerifyError>`，
  不依赖任何后端。检查 ID/索引边界、`Type::Struct/Enum` 与 `TypeDef` 种类一致、按值布局自环/互环
  （灰/黑 DFS，指针/切片递归合法）、entry/跳转目标、`bool` 分支、extern 无函数体、库 `main=None`、
  块 instruction/location 数量、source/location 引用，以及指令/表达式/Place 的类型一致性；允许
  `null` 与合法只读限定（`*T -> *const T`、`[]T -> []const T`，含 HIR 初始化时写入的限定）。
- `crates/dolphin-hir/src/lower.rs`：`ProgramLowerer::lower` 构造 `ir::Program` 后统一调用校验，
  失败转成诊断；`lower`/`lower_sources`/`lower_library`（check/build/run/lib）全部覆盖。
- `crates/dolphin-codegen-llvm/src/codegen.rs`：DebugInfo finalize 后、优化前调用
  `module.verify()`；Release 在 `default<O2>` 后再次校验；错误带阶段与原始 LLVM 信息。
  聚合返回值调用的 sret 缓冲区改由 `create_entry_alloca` 在函数入口块分配（每调用点一个静态槽），
  并在重定位 builder 前后保存/恢复调试位置，避免调用指令丢失 `!dbg`。
- `tests/m18_sret.rs`（新增）：1e6 次循环调用聚合返回函数的固定回归。

### 验收映射

| 验收点 | 测试名 | 结果 |
| --- | --- | --- |
| IR-01 | `dolphin-ir` 单测：`struct_kind_mismatch_is_rejected`、`value_self_cycle_is_rejected`、`value_mutual_cycle_is_rejected`、`pointer_recursion_is_allowed`、`invalid_local_index_is_rejected`、`enum_variant_out_of_range_is_rejected`、`invalid_source_is_rejected` | 全部通过（畸形 IR 返回错误，间接递归不误报） |
| IR-02 | `invalid_block_target_is_rejected`、`branch_condition_must_be_bool`、`extern_without_body_is_allowed`、`library_without_main_is_allowed`、`call_arity_mismatch_is_rejected` | 全部通过 |
| IR-03 | `set_local_type_mismatch_is_rejected`、`return_type_mismatch_is_rejected`、`empty_return_in_non_unit_is_rejected`、`print_non_printable_is_rejected`、`call_arity_mismatch_is_rejected`；完整默认/LLVM 套件 | 畸形拒绝；正常 defer/泛型/match/切片/lib/extern 程序全部通过
| IR-04 | `location_count_mismatch_is_rejected`、`invalid_source_is_rejected` | 全部通过；块内 location 数量一一对应、合成位置允许 `line=0` |
| IR-05 | 18 个 `verify::tests` 全部返回 `Err` 或 `Ok`，无 panic；`cargo test --workspace` 默认 300 / LLVM 306 | 通过 |
| LLVM 校验 | `dolphin-codegen-llvm` 单测 `verifier_reports_stage_and_raw_error`；全部 LLVM Debug/Release 构建 | 通过：阶段文本与原始 LLVM 信息出现在诊断中，校验失败不写目标文件 |
| sret 循环 | `tests/m18_sret.rs::loop_sret_alloca_does_not_accumulate_stack` | 修复前 LLVM Debug SIGSEGV（exit 139），修复后四组合均 `499999500000\n`、exit 0 |

### 修复前复现与证据

```text
sret 循环（H18-00 静态风险，本轮实机确认）：
  1e6 次调用返回 24 字节结构体的函数，LLVM Debug：SIGSEGV（direct exit 139，栈上限 8 MiB）
  Cranelift Debug：exit 0，输出 499999500000
  生成 IR 证据：修复前 `%sret = alloca [24 x i8]` 位于循环体 bb2 内
```

LLVM verifier 接入后立即发现一个真实缺陷：入口块提升早期版本丢失调用指令的 `!dbg`
（“inlinable function call ... must have a !dbg location”），通过保存/恢复当前调试位置修复。

### 修复后结果

- LLVM Debug 生成 IR 中 `%sret` 位于入口块，循环体不再 alloca；程序 exit 0、输出固定。
- 所有正常程序（含 defer、泛型、match、lib、extern、跨包）在 check/build/run/lib 统一出口通过 IR 校验；
  畸形 IR 单测返回错误而非 panic。
- Debug/Release 两阶段 LLVM verifier 全绿，目标文件只在两次校验通过后写出。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-ir --lib verify                                  # 18 passed
cargo test -p dolphin-compiler --features llvm --lib                    # 含 LLVM verifier 单测
cargo test -p dolphin-compiler --test m18_sret                          # 1 passed
cargo test -p dolphin-compiler --features llvm --test m18_sret          # 1 passed（每例 4 组合）
cargo test --workspace --exclude dolphin-codegen-llvm                   # 300 passed
cargo test --workspace --features llvm                                  # 306 passed
DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test manifest \
  --test packages --test m18_harness --test m18_place --test m18_layout --test m18_cast \
  --test m18_bounds --test m18_impl --test m18_sret                     # 168 passed
cargo test -p dolphin-compiler --features llvm --test backend           # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                    # 全部通过
```

默认测试总数 281→300，LLVM 286→306（新增 verify 单测 18、LLVM verifier 单测 1、sret 回归 1）。
日志在 `/tmp/opencode/h18-06/`。

### 未运行的检查及原因

- macOS/Windows：本机只有 Linux，未运行；留待 H18-09/H18-11。
- 远端 CI：本批未 push/tag；新增 `--test m18_sret` 必须由 H18-09 写入 CI 显式列表。
- 合同 5.3 示例与发行打包：H18-11 范围，本批未执行。
- 无法从合法源码注入畸形 LLVM IR；LLVM 层通过单测（直接构造非法模块）与全量构建覆盖，
  不伪造“注入用户程序验证失败”的端到端用例。

### 行为/兼容变化

- 内部 IR 违反契约时现在返回诊断而不是在 codegen 中 panic/未定义行为；用户程序语义无变化。
- LLVM 聚合返回值调用在 Debug 下不再随循环累积栈；每个调用点在入口块占用固定大小的 sret 槽。
- LLVM `module.verify()` 成为写出目标文件的前置条件（Debug 优化前、Release 优化后）。

### 剩余问题和下一批输入（H18-07）

1. H18-07 范围：`resolver.rs` 的来源冲突对称检查，覆盖 PKGSRC-01..06：Path 先/Remote 先两种顺序都拒绝，
   同仓库同坐标菱形复用，不同仓库/版本冲突，locked/offline 不绕过来源检查，相同 canonical path 的别名
   与循环检测。
2. 使用隔离缓存与测试仓库，不访问真实用户仓库、不修改 `~/.dolphin`。
3. 无阻塞；本报告不把 H18-07..11 记为完成。

## H18-07 包来源身份与顺序无关性

- 批次：H18-07
- 状态：完成（Linux x86_64；Cranelift 与 LLVM 两后端本机验证；不表示其余批次或三平台）
- 前置批次及报告：H18-06，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`429ff5c64a8dedae475b36fe859796bce12ce095`（`v0.2.0-M18-05`），工作区干净。
- 本批修改未提交：`crates/dolphin-package/src/resolver.rs`、`tests/packages.rs`、
  `docs/implemented-features.md`、`docs/plan-m18-correctness.md`（状态行）。
- 本批不新增独立测试文件，`tests/packages.rs` 已在合同 5.2 的显式 `--test` 列表中；H18-09 无需为此批
  新增 CI 条目。

### 修改文件与关键实现

- `crates/dolphin-package/src/resolver.rs`：
  - 坐标依赖分支命中 `by_name` 且坐标相同时，来源判定由“仅比较 Remote 仓库”改为统一规则
    `same_remote_repository`：只有“同一仓库 ID 的 Remote”才算同源；Path/Root 与 Remote、以及不同
    仓库 ID 的 Remote 都判为来源冲突。此前 Path 先加载后遇到同坐标 Remote 会直接 `return Ok(existing)`，
    静默复用路径包。
  - `acquire_path` 命中 `by_name` 的冲突与坐标分支统一走新的 `source_conflict`，诊断列出两条请求链与
    来源描述；`describe_source` 只输出仓库 ID 或规范化路径，不含凭据。
  - 模块文档补充“Path/Root 与 Remote 永不视为同一来源，任一加载顺序都拒绝”。
- `tests/packages.rs`：新增 `h18_07_pkgsrc_01..06`；PKGSRC-01/02 用 `left`（Path→dup）与 `right`
  （Remote→dup）两个菱形分支，通过根别名顺序控制先加载哪一支，并断言诊断里出现两条不同的请求链。

### 验收映射

每个用例走真实 `dc build`（`file://` 仓库 + 隔离 `DOLPHIN_HOME`）；成败例均断言固定 exit/stderr，
不用“两后端一致”代替正确性：

| 验收点 | 测试名（tests/packages.rs） | 修复前 | 修复后 |
| --- | --- | --- | --- |
| PKGSRC-01 | `h18_07_pkgsrc_01_path_before_remote_is_rejected` | FAILED：静默成功（输出 `Built .../app`） | 拒绝 `source conflict`；`already selected from path`，链含 `left -> dup` 与 `right` |
| PKGSRC-02 | `h18_07_pkgsrc_02_remote_before_path_is_rejected` | FAILED：行为已拒绝，但诊断为旧 `dependency conflict`，无统一来源类别 | 拒绝 `source conflict`；`already selected from repository default`，链含 `right -> dup` 与 `left` |
| PKGSRC-03 | `h18_07_pkgsrc_03_same_repository_same_coordinate_is_reused` | 通过 | 通过：exit 42；锁内恰好 1 个 `[[package]]` |
| PKGSRC-04 | `h18_07_pkgsrc_04_different_repository_or_version_is_rejected` | FAILED：不同仓库行为已拒绝但诊断非统一来源类别；不同版本通过 | 通过：不同仓库拒绝 `source conflict` 且列出 `default`/`other`；不同版本拒绝 `conflict` |
| PKGSRC-05 | `h18_07_pkgsrc_05_locked_and_offline_do_not_bypass_source_check` | FAILED：`--locked` 报 `dolphin.lock is out of date` 而非来源冲突（缺口下 `--offline` 会静默改写锁） | 通过：`--locked`/`--offline` 均报 `source conflict`，锁文件字节与失败前完全一致 |
| PKGSRC-06 | `h18_07_pkgsrc_06_same_canonical_path_aliases_are_reused` | 通过 | 通过：exit 42；两个别名解析为 1 个包；循环检测由既有 `pkg06_dependency_cycle_and_version_conflict_report_chains` 继续覆盖 |

### 修复前复现结果

用 `git stash push -- crates/dolphin-package/src/resolver.rs` 临时撤下修复，运行新增回归：

```text
test result: FAILED. 2 passed; 4 failed
pkgsrc_01: expected failure but succeeded（Path 先加载后 Remote 被静默复用——本次核心缺陷）
pkgsrc_05: expected `source conflict`, got `dolphin.lock` is out of date（来源检查被绕过）
pkgsrc_02/04: 行为已拒绝，但旧诊断不含统一来源类别
pkgsrc_03/06: 通过
```

日志：`/tmp/opencode/h18-07/pre-fix-tests.log`。

### 修复后结果

- 6 个 PKGSRC 用例全部通过；PKGSRC-01/05 的缺陷消除，PKGSRC-02/04 诊断为统一的来源冲突并含两条链。
- 单仓库同坐标复用、同 canonical path 别名复用与既有循环/版本/坏摘要/离线等路径未回退
  （`tests/packages.rs` 13 项、`tests/manifest.rs` 22 项全绿）。
- 既有 `pkg06_dependency_cycle_and_version_conflict_report_chains` 仍断言版本与来源冲突含 `conflict`。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test packages h18_07                    # 6 passed
cargo test -p dolphin-compiler --test packages --test manifest            # 13 + 22 passed
cargo test --workspace --exclude dolphin-codegen-llvm                     # 306 passed
cargo test --workspace --features llvm                                    # 312 passed
DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test manifest \
  --test packages --test m18_harness --test m18_place --test m18_layout --test m18_cast \
  --test m18_bounds --test m18_impl --test m18_sret                       # 174 passed
cargo test -p dolphin-compiler --features llvm --test backend             # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                      # 全部通过
```

默认测试总数 300→306，LLVM 306→312，`DOLPHIN_BACKEND=llvm` 显式列表 168→174（新增 packages 6 项）。
日志在 `/tmp/opencode/h18-07/`。

### 未运行的检查及原因

- macOS/Windows：本机只有 Linux，未运行；留待 H18-09/H18-11。
- 远端 CI：本批未 push/tag；未新增独立测试文件，现有 `--test packages` 已覆盖本批回归。
- 合同 5.3 示例与发行打包：H18-11 范围，本批未执行。
- Dolphin profile：本批改动只影响依赖解析，与 codegen/后端/profile 无关；成功例默认 Dolphin Debug，
  解析路径不区分 Release，未伪造 Release 结论。
- 未改动 `registry.rs` 对锁来源的检查（`locked.source != "path"`）；该状态机属 `--locked` 与锁一致性
  范畴，本批范围仅为 resolver 的来源身份对称检查，未越界修改。

### 行为/兼容变化

- `(group, name)` 同坐标下，Path/Root 与 Remote（以及不同仓库 ID 的 Remote）现在一律判为来源冲突；
  此前 Path 先加载后 Remote 会静默复用路径包。这是兑现既有“同一来源”规范，不新增配置/override。
- 冲突诊断措辞统一为 `dependency source conflict` 并列出两条链；依赖诊断的机器消费者若匹配旧措辞会受影响，
  但旧措辞未在文档中承诺为稳定接口。
- 无公开 API、`.dlib` 格式或锁文件格式变化。

### 剩余问题和下一批输入（H18-08）

1. H18-08 范围：driver/CLI 的项目目标与 profile 一致性，先复现再修复第 3.5 节两问题：
   BUILD-01（path 依赖同时声明 lib+两个 bin 时不能把其 bin main 带入应用）、BUILD-02（优先级
   `显式 CLI --debug/--release > 根清单 build.optimization > Debug`，bin/lib 一致，依赖包不能覆盖根 profile）。
2. BUILD-03 需用受控泄漏/非法释放例检查 stderr/exit，不能只断言 `BuildProfile` 枚举值；BUILD-04 回归
   单文件、多 bin、纯库、native inputs 与显式 backend 优先级。
3. 本批未改 driver/CLI，`resolve_project` 的锁写入点（失败早退、不写锁）已由 PKGSRC-05 锁定；H18-08 修改
   profile 选择时不要破坏该早退行为。
4. 无阻塞；本报告不把 H18-08..11 记为完成。

## H18-08 项目目标与 profile 一致性

- 批次：H18-08
- 状态：完成（Linux x86_64；Cranelift 与 LLVM 两后端本机验证；profile 用 Debug/Release runtime 行为验证；不表示其余批次或三平台）
- 前置批次及报告：H18-07，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`ba053c63e373f5c54b475370bb816b7e69693e65`（`v0.2.0-M18-07`），工作区干净，无用户改动。
- 本批修改未提交：`crates/dolphin-driver/src/lib.rs`、`src/main.rs`、`tests/manifest.rs`、
  `tests/packages.rs`、`tests/cli.rs`、`docs/implemented-features.md`、`docs/plan-m18-correctness.md`。
- 未新增独立测试文件；`tests/cli.rs` 本就在合同 5.2 的显式 `--test` 列表中，H18-09 无需新增条目。

### 修改文件与关键实现

- `crates/dolphin-driver/src/lib.rs`：
  - `BuildProfile::from_manifest`（清单 `build.optimization`，否则 Debug）与
    `BuildProfile::resolve(manifest, explicit: Option<Self>)` 冻结优先级 `显式 > 清单 > Debug`。
  - `build_manifest_with_graph` 删除“清单 release 强制覆盖传入 profile”的分支，改为把调用方解析后的
    最终 profile 原样用于全部 bin；依赖包清单不参与。
  - `load_graph_sources`：非根包（依赖）改为排除自己的全部 `[[bin]]` 入口；此前只有根包排除，
    lib+两个 bin 的 path 依赖会把两个 `main` 作为 `@N.main` 载入并报重复定义。
- `src/main.rs`：`BuildArgs::profile()` 改为 `explicit_profile() -> Option<BuildProfile>`（区分“没传”和
  显式 Debug）；`build`/`run` 发现清单后调用 `BuildProfile::resolve` 解析一次，并把同一 profile 传给
  库构建与全部 bin；单文件模式无清单，默认 Debug。
- `tests/manifest.rs`：BUILD-01 driver 回归与 BUILD-02/03 显式 profile 的运行时（泄漏）回归。
- `tests/packages.rs`：BUILD-01 path 依赖与发布包消费一致性回归。
- `tests/cli.rs`：BUILD-02/03/04 命令行回归，含 LLVM DWARF 观察库/bin profile 一致性。

### 验收映射

| 验收点 | 测试名 | 修复前 | 修复后 |
| --- | --- | --- | --- |
| BUILD-01 | `tests/manifest.rs::h18_08_build_01_path_dependency_with_bins_loads_only_library` | FAILED：`function @1.main is already defined`（Cranelift/Debug） | 通过：两后端 × Debug/Release 均 exit 42、stderr 空；依赖 `shared.do` helper 仍生效 |
| BUILD-01 | `tests/packages.rs::h18_08_build_01_path_and_published_dependency_agree` | FAILED：path 应用重复 `@1.main`；坐标应用通过（归档已排除 bin） | 通过：path 与坐标消费均 exit 42，行为一致 |
| BUILD-02/03 | `tests/cli.rs::h18_08_build_03_explicit_debug_on_release_manifest_reports_leak` | FAILED：显式 `--debug` 被清单 release 覆盖，stderr 无泄漏报告 | 通过：`--debug` 使用 Debug runtime，stderr 含 `leaked`，exit 42 |
| BUILD-03 | `h18_08_build_03_explicit_release_on_debug_manifest_is_clean` | 通过（反向已是 Release） | 通过：`--release` 无泄漏报告，exit 42 |
| BUILD-02 | `h18_08_build_02_manifest_optimization_is_the_default` | 通过 | 通过：无显式选项时清单 release 无泄漏、清单 debug 有泄漏 |
| BUILD-02 | `h18_08_build_02_dependency_manifest_does_not_override_root_profile` | 通过（根清单只取根） | 通过：依赖清单 release 不影响根 Debug，stderr 含 `leaked` |
| BUILD-02 | `h18_08_build_02_library_and_bins_share_effective_profile`（`--features llvm`） | FAILED：`--debug` 时库对象 Debug、bin 仍 Release，bin 缺 DWARF | 通过：`--debug` 库对象与 bin 均含 `.debug_info`；`--release` 两者均无 |
| BUILD-03 | `h18_08_build_03_debug_runtime_reports_leak_and_invalid_free` | 通过 | 通过：正常 `mem.free` 无泄漏；double free exit 103、stderr 含 `invalid free` |
| BUILD-02/03 | `tests/manifest.rs::h18_08_build_03_explicit_profile_overrides_manifest_optimization` | FAILED：显式 Debug 被 release 清单覆盖（无泄漏） | 通过：两后端显式 Debug 有泄漏；显式 Release 无泄漏 |
| BUILD-04 | `h18_08_build_04_explicit_backend_beats_environment` | 通过（既有行为） | 通过：无显式选项时非法 `DOLPHIN_BACKEND` 被告警回退；`--backend cranelift` 不读取环境变量 |
| BUILD-04 | 既有单文件/多 bin/纯库/native 回归：`tests/build.rs`、`tests/manifest.rs`、`tests/ffi.rs`、`tests/cli.rs` | 通过 | 通过（完整门禁） |

### 修复前复现结果

1. 手工探针（修复前二进制）：
   - BUILD-01：path 依赖（lib+2 bins）→ `error[E0001]: function '@1.main' is already defined`；
   - BUILD-02：清单 release + `dc run --debug` → exit 42 但 stderr 无泄漏报告（实际使用 Release runtime）。
2. 永久回归在修复前代码上运行：临时撤下 driver/CLI 改动（`git stash push -- crates/dolphin-driver/src/lib.rs src/main.rs`）
   后运行新增用例，结果见 `/tmp/opencode/h18-08/logs/pre-fix-tests.log`：

```text
manifest:  0 passed; 2 failed（BUILD-01 重复 main；显式 Debug 被 release 清单覆盖）
packages:  0 passed; 1 failed（path 应用重复 @1.main）
cli:       4 passed; 1 failed（显式 --debug 无泄漏报告）
cli+llvm:  4 passed; 2 failed（上项 + 库/bin profile 不一致：bin 缺 .debug_info）
反向/守卫例（显式 Release、清单默认、非法释放、backend 优先级）修复前也通过。
依赖清单不覆盖根 profile 的守卫例单独在修复前运行：1 passed
（`cargo test -p dolphin-compiler --test cli h18_08_build_02_dependency`）。
```

### 修复后结果

- 两个缺陷消除：依赖 bin 不进入使用方，path 与发布包消费一致；profile 优先级与 bin/lib 一致兑现。
- 泄漏/非法释放用例给出固定运行时行为：Debug 泄漏报告（exit 42 保留）、double free exit 103 +
  `Dolphin runtime error: invalid free`；Release 无泄漏报告。断言基于 runtime stderr/exit，不是枚举值。
- 完整回归未回退：默认 315、LLVM 322、后端对照 4 全绿。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test manifest h18_08                       # 2 passed
cargo test -p dolphin-compiler --test packages h18_08                       # 1 passed
cargo test -p dolphin-compiler --test cli h18_08                             # 6 passed
cargo test -p dolphin-compiler --features llvm --test cli h18_08             # 7 passed（含 DWARF 一致性）
cargo test --workspace --exclude dolphin-codegen-llvm                        # 315 passed（默认）
cargo test --workspace --features llvm                                       # 322 passed
DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test cli \
  --test manifest --test packages --test m18_harness --test m18_place --test m18_layout \
  --test m18_cast --test m18_bounds --test m18_impl --test m18_sret          # 193 passed
cargo test -p dolphin-compiler --features llvm --test backend                # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                         # 全部通过
```

默认测试总数 306→315，LLVM 312→322；env-LLVM 显式列表本批加入 `--test cli`（合同 5.2 要求），
计数从上一批的 174（未含 cli）变为 193。日志在 `/tmp/opencode/h18-08/`。

### 未运行的检查及原因

- macOS/Windows：本机只有 Linux，未运行；留待 H18-09/H18-11。DWARF 一致性用例按 ELF 段名检查，
  macOS/Mach-O 由 H18-09/11 的平台 lane 覆盖。
- 远端 CI：本批未 push/tag；未新增测试文件，现有 `--test cli`/`--test manifest`/`--test packages`
  已包含本批回归。
- 合同 5.3 示例与发行打包：H18-11 范围，本批未执行。
- Release runtime 的非法释放仍为未定义行为（运行时不追踪分配），本批只在 Debug 下验证受控 double free；
  没有把 Release 的未定义行为写入规范。
- `dc package`/`dc publish` 仍固定使用 Debug 构建库（既有行为，本批未改）；合同 BUILD-02 只覆盖
  带 `--debug/--release` 的 `build`/`run`。
- 单文件模式的 `--debug/--release` 既有语义不变，由 `tests/build.rs`/`tests/cli.rs` 覆盖。

### 行为/兼容变化

- 显式 `--debug`/`--release` 现在始终优先于根清单 `build.optimization`；此前清单 release 会覆盖显式
  `--debug`。默认（不传选项）行为不变：清单 release → Release，否则 Debug。
- `build` 在 lib+bin 项目下库与 bin 使用同一最终 profile；此前清单 release + 显式 `--debug`（或未传时）
  可能让库与 bin 分属不同 profile。
- path 依赖的 `[[bin]]` 入口不再被加载；此前会与库源码一起编译并可能报重复 `main`，与发布包消费不一致。
  这兑现已文档化的“归档排除 bin”行为，不新增配置。
- `BuildProfile::from_manifest`/`resolve` 为新增公开 API；`build_manifest_with_graph` 语义改为“传入即最终”，
  公开 API 调用方传 `BuildProfile::Debug` 时不再被清单 release 覆盖。

### 剩余问题和下一批输入（H18-09）

1. H18-09 范围（CI 与发布质量门禁）：保留 Linux/macOS/Windows 默认 Cranelift lane；新增固定 LLVM 22
   的 Linux lane 并执行合同 5.2 命令（含 `--test cli`）；`tests/backend.rs` 断言 exit=25、
   stdout=`7 10 12 25\n`、stderr 空；tag 上运行质量门禁且发布依赖同提交测试与同一归档冒烟；
   M14/M15 无泄漏冒烟必须检查 stderr；核查 Darwin shared-library fixture 的 install_name 而非 ELF soname。
2. 本批新增的 CLI/profile 测试依赖本机 `dc` 二进制的实际运行；H18-10/H18-11 更新文档示例与集成验收时
   不要绕过这些回归。
3. 无阻塞；本报告不把 H18-09..11 记为完成。

## H18-09 LLVM CI 与发布质量门禁

- 批次：H18-09
- 状态：完成（配置与本机步骤已验证；远端 CI/tag 门禁待平台验收，不表示其余批次或三平台）
- 前置批次及报告：H18-08，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`f4aa9446284921e5b26e56e98acbc29ab9fcb2d8`（`v0.2.0-M18-08`），工作区干净，无用户改动。
- 本批修改未提交：`.github/workflows/ci.yml`、`tests/backend.rs`、`tests/ffi.rs`、`tests/manifest.rs`、
  `crates/dolphin-hir/src/modules.rs`（冒烟阻塞缺陷的最小修复）、`README.md`、`docs/installation.md`、
  `docs/implemented-features.md`、`docs/plan-m18-correctness.md`。
- 本批未触发任何 tag、未 push、未创建发布。

### 修改文件与关键实现

- `.github/workflows/ci.yml` 重构为三个 job：
  - `test`（三平台矩阵，PR/分支 push/tag 都运行）：保留格式检查、默认 Clippy、默认测试、Release 构建、
    `dc fmt --check`、打包与冒烟。冒烟对 M14/M15 显式把 stderr 写入文件并断言为空（仅 exit=0 不证明无泄漏），
    包消费 smokeapp 同样检查 stderr。新增加 `actions/upload-artifact@v4` 上传**已冒烟**的归档。
  - `llvm`（Ubuntu）：用 apt.llvm.org 固定安装 `llvm-22-dev` 到 `/usr/lib/llvm-22`，校验
    `llvm-config --version` 为 22.x 且 prefix 为该路径，设置 `LLVM_SYS_221_PREFIX`，然后执行合同 5.2 命令。
  - `release`（tag `v*`）：`needs: [test, llvm]`，用 `actions/download-artifact@v4` 取回 `test` 上传的归档，
    由 `softprops/action-gh-release@v2` 上传；不重建、不使用 `always()`。tag 上 `test`/`llvm` 不再被 skip，
    因此 `needs` 不会因 skip 放行。
- `tests/backend.rs`：`assert_backends_agree` 除两后端一致外，新增固定断言 exit=25、
  stdout=`7 10 12 25\n`、stderr 为空（Debug 与 Release 各跑一次）。
- `tests/ffi.rs`：共享库 fixture 按平台分支；Darwin 用 `-dynamiclib -install_name @rpath/lib<stem>.dylib`
  产出 `.dylib`，ELF 平台保留 `-shared -Wl,-soname,...` 产出 `.so`，不再把 `-soname` 传给 macOS linker。
- `crates/dolphin-hir/src/modules.rs`（冒烟发现的阻塞缺陷）：`assign_source_id` 之前只给顶层
  trait/impl 块设置 `source_id`，没有递归到 `trait.methods`/`impl.methods`；方法保留解析器默认值 0，
  被误归属到第一个源文件。现在 trait 与 impl 的方法都设置为所在源文件的 id。
- 文档：README 的 LLVM/CI 段落更新为实际三 job 结构与固定 LLVM 22 安装；installation 增加
  “LLVM 22 开发环境”一节（安装、`LLVM_SYS_221_PREFIX`、版本检查与合同 5.2 命令）。

### CI job 依赖表（CI-01）

| 事件 | `test`（3 平台矩阵） | `llvm`（Ubuntu） | `release` |
| --- | --- | --- | --- |
| PR | 运行 | 运行 | 不运行（`if` 非 tag） |
| 分支 push | 运行 | 运行 | 不运行 |
| tag `v*` push | 运行（含归档上传） | 运行 | `needs: [test, llvm]` 全部成功后运行，只下载并上传已冒烟归档 |

失败语义：`needs` 默认要求依赖成功；`test`/`llvm` 任一失败或被跳过，`release` 都会被跳过；无 `always()`。

### 验收映射

| 验收点 | 证据 | 结果 |
| --- | --- | --- |
| CI-01 | 上表 + `ci.yml` 解析检查（`python3` PyYAML：jobs `test`/`llvm`/`release`，`needs=['test','llvm']`，`if` 为 tag 前缀） | 配置已验证；远端依赖表待平台确认 |
| CI-02 | `llvm` lane 安装 LLVM 22 并校验版本；本机复跑 lane 命令：`DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm` 323 passed、显式列表 146 passed、`--test backend` 4 passed | 通过（本机 LLVM 22.1.8） |
| CI-03 | `release` 依赖 `test`+`llvm`、无 `always()`；`test` 去掉 tag 排除，tag 上不再 skip | 配置已验证 |
| CI-04 | 冒烟在 `test` 内解压发行包构建/运行 m8/m14/m15、打包并消费 `.dlib`；同一 `dist/*` 由 `upload-artifact` 上传，`release` 只下载该 artifact | 本机用带 patchelf 的隔离 venv 打包并跑完整冒烟序列通过 |
| CI-05 | `test` 保留 fmt/clippy/默认测试/Release 构建/`dc fmt --check`，三平台矩阵未删 | 配置保留，默认门禁本机通过 |
| BUILD-04（跨批） | `tests/backend.rs` 固定期望 | Debug/Release 四组合通过（exit 25、`7 10 12 25\n`、stderr 空） |
| 冒烟阻塞缺陷 | `tests/manifest.rs::h18_09_method_diagnostics_use_defining_file` | 修复前 FAILED（诊断指向 `main.do`）；修复后通过（指向 `util.do`） |

### 修复前复现结果

1. `examples/m15` 用修复前二进制构建直接 panic（冒烟在 `smoke/dc build examples/m15` 触发）：

```text
thread 'main' panicked at crates/dolphin-source/src/source.rs:44:31:
end byte index 914 is not a char boundary; it is inside '。' (bytes 913..916 of string)
```

   加临时探针后确认：`FunctionLowerer.source` 是 `examples/m15/src/main.do`，但正在 lower 的是
   `module=std.collections function=next` 的 impl 方法；实例 `source_id` 来自
   `template.function.source_id`，而 `assign_source_id` 漏设了 impl/trait 方法的 `source_id`。
2. 确定性回归：两文件项目 `main.do` + `util.do`，方法体内引用未知变量。修复前诊断错误指向
   `src/main.do:1:52`（span 被截断到第一个文件）；`git stash push -- crates/dolphin-hir/src/modules.rs`
   后运行 `cargo test -p dolphin-compiler --test manifest h18_09` = `0 passed; 1 failed`。
3. 对照：把 `main.do` 的整段中文注释换成 ASCII 后修复前也能编译，说明 panic 依赖多字节字符位置，
   属“错误归属 + 非字符边界切片”的组合，不是 m15 例子本身的问题。

### 修复后结果

- `examples/m15` 用打包后的 `dc` 构建、运行输出 `42 22 true`，Debug stderr 为空；完整冒烟序列
  （m8 exit 64；m14/m15 无泄漏；`.dlib` 打包；`fetch` + `build --locked`；smokeapp exit 42 且 stderr 空；
  `--locked --offline` 重建）全部通过。
- `h18_09_method_diagnostics_use_defining_file` 修复后通过：错误指向 `util.do:4:35`，不再出现 `main.do`。
- `tests/backend.rs` 与 `tests/ffi.rs` 改动后完整默认/LLVM 套件全绿。

### 实际运行命令与测试数量

```bash
# 本机 LLVM lane 命令（合同 5.2）
llvm-config --version                                                     # 22.1.8
cargo build --bins --features llvm
cargo clippy --workspace --all-targets --features llvm -- -D warnings     # 通过
DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm          # 323 passed
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages      # 146 passed
cargo test -p dolphin-compiler --features llvm --test backend             # 4 passed

# 默认与本地完整矩阵
cargo test --workspace --exclude dolphin-codegen-llvm                     # 316 passed (315→316)
cargo test --workspace --features llvm                                    # 323 passed (322→323)
DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test cli \
  --test manifest --test packages --test m18_harness --test m18_place --test m18_layout \
  --test m18_cast --test m18_bounds --test m18_impl --test m18_sret       # 194 passed (193→194)
cargo fmt --all -- --check && git diff --check                            # 通过

# 发行包冒烟（本机，含受限替换件）
cargo build --release --bins
PATH=<venv-with-patchelf> python3 scripts/package.py --target x86_64-unknown-linux-gnu --out-dir /tmp/...
# 解压归档后用归档内 dc + 自带 rust-lld 跑 CI 冒烟序列            # SMOKE_SEQUENCE_OK
```

默认测试总数 315→316，LLVM 322→323，env-LLVM 列表 193→194（新增 `h18_09_method_diagnostics_use_defining_file`）。
日志在 `/tmp/opencode/h18-09/`；CI YAML 用 `python3` + PyYAML 解析验证。

### 未运行的检查及原因

- 远端 CI 与 tag 发布：本机无远端权限、未 push/tag，不伪造远端结果；状态为“配置与本机步骤已验证，
  远端门禁待平台验收”。CI job 依赖表已写入本报告（CI-01）。
- macOS/Windows lane：本机只有 Linux。Darwin shared-library fixture 修正按 clang/ld64 选项规则编写，
  macOS 实跑留待平台 lane；Windows lane 配置保留。
- `patchelf` 未预装：本机用隔离 venv 的 `patchelf`（仅 `/tmp`）复现 Linux 打包与自带 rust-lld 冒烟；
  CI 的 Linux runner 需系统有 patchelf（`scripts/package.py` 现状），这不是本批新增依赖。
- 无 CRT/SDK 的干净环境验收仍属 H21；本批不宣称 runner 是干净机器。
- 未新增/删除测试文件；`tests/backend.rs`/`tests/ffi.rs`/`tests/manifest.rs` 已在 CI 显式列表中。

### 行为/兼容变化

- CI：tag 现在先跑三平台质量门禁与 LLVM lane，发布消费的是同一份已冒烟归档；分支/PR 行为保持。
- 编译器：impl/trait 方法的 `source_id` 修正后，方法内诊断、IR `Location.file` 与 DWARF 行表归属到
  定义文件，而不是第一个源文件。这是修正错误归属，不改变语言语义、IR 结构、公开 CLI 或打包格式。
- `tests/backend.rs` 新增固定期望使“两后端以相同方式错误”不再可能；`tests/ffi.rs` 的 Darwin 分支
  不再传 ELF `-soname`。

### 剩余问题和下一批输入（H18-10）

1. H18-10 范围：README、implemented-features、language-design、installation 与网站中英文核正；
   修正可疑示例（自建 `std`、`val` 调 deinit、语句 if 当表达式、`strlen` 用 `c_ulong` 等）；
   建立轻量文档 fixture/提取机制进入测试；定向核查饱和规则、泛型子集、LSP 单文件限制、DWARF 范围、
   SDK 依赖与 `.dlib` 版本政策。
2. 本批 README/installation 的 LLVM 与 CI 段落已更新；H18-10 核正时以实际 `ci.yml` 与命令为准，
   不要再写“CI 不装 LLVM/发布不依赖门禁”。
3. `crates/dolphin-source` 的 `line_column` 仍假设 span 落在字符边界；本批已消除已知的错误归属来源，
   但未加防御性截断。若后续再发现越界 span，应修根因并在该层返回诊断而不是 panic（留给发现它的批次）。
4. 无阻塞；本报告不把 H18-10..11 记为完成。

## H18-09-W Windows 平台补充验证

- 批次：H18-09 的 Windows 平台复验（补充节，不新增批次编号）
- 状态：完成（Windows 11 本机默认 Cranelift lane 与发行包冒烟通过；CRLF 格式门禁问题经用户确认后
  以 `.gitattributes` 修复并在 runner 式检出复验通过；LLVM lane 与远端 CI/tag 未验证）
- 开始 HEAD 与已有本地改动：`89052031a3e2e310a0bfe68b17a53f23aa63c4b9`（`v0.2.0-M18-09`，
  H18-09 内容已包含在此提交；上一节写作时记为未提交）。
- 前置批次及报告：H18-08、H18-09，见本文件上一节

### 环境

| 项 | 值 |
| --- | --- |
| 平台 | Windows 11 企业版 10.0.26200，x86_64（AMD Ryzen 5 5600G） |
| rustc / cargo | 1.98.1（host `x86_64-pc-windows-msvc`） |
| C 编译器 | Visual Studio 2022 Community MSVC（`build.rs` 与 ffi 测试自行定位 `vcvars64.bat`；`cl` 不在 PATH） |
| llvm-ar | scoop LLVM 23.1.1（ffi 静态库 fixture 使用） |
| llvm-config / LLVM 22 dev | 不存在（scoop LLVM 23.1.1 无 `llvm-config.exe`），无法构建 `--features llvm` |
| `DOLPHIN_BACKEND` | 未设置；`dc env` 报默认后端 `cranelift`，linker `rust-lld`、system linker `link` 回退 |
| git EOL 配置 | `core.autocrlf=true`（Git for Windows 默认，与 GitHub `windows-latest` runner 相同）；仓库原无 `.gitattributes`（复验中新增，见“行为/兼容变化”），工作树 `.do` 为 CRLF |
| 验证用 `dc` | `cargo build --release --bins`（HEAD 全部改动）与 `dist/dolphin-0.2.0-x86_64-pc-windows-msvc.zip` |

### 验收映射（Windows）

| 检查 | 命令/测试 | 后端/profile | 结果 |
| --- | --- | --- | --- |
| CI-02 默认 lane（Windows 行） | `cargo test --workspace --exclude dolphin-codegen-llvm` | 默认 Cranelift，fixture Debug | 通过：315 passed / 0 failed，exit 0 |
| CI-02 显式后端（合同 5.1） | `$env:DOLPHIN_BACKEND="cranelift"; cargo test --workspace --exclude dolphin-codegen-llvm` | Cranelift | 通过：315 passed / 0 failed |
| CI-05 格式检查（Windows 行，修复前） | `target\release\dc.exe fmt --check examples crates\dolphin-std\src` | n/a | **FAILED**：exit 1，25 个文件 `would reformat`（CRLF 检出）；LF 副本同命令 exit 0 |
| CI-05 修复后复验（runner 式检出） | 临时 clone（`core.autocrlf=true`，含已提交的 `.gitattributes`）执行同一命令 | n/a | 通过：检出为 `w/lf`、clone status 干净，exit 0 |
| CI-04 打包 | `python scripts/package.py --target x86_64-pc-windows-msvc` | n/a | 通过：`dist/dolphin-0.2.0-x86_64-pc-windows-msvc.zip`（43116195 B）+ `.sha256`（`d2f4e45f5f20af927c4c443a270e736d65e8bed4acfa1168f71c08b68c187e64`） |
| CI-04 归档冒烟（ci.yml 第 58-139 行原样脚本） | Git Bash 在干净 examples 副本 + 该 zip 上执行 | 归档内 `dc`，Debug/Release 按脚本 | 通过：`BASH_EXIT=0`；m8 release exit 64；m14 exit 0 且 stderr 空；m15 stdout `42 22 true`、exit 0 且 stderr 空；`.dlib`+`.sha256` 生成；`fetch`+`--locked` 构建 exit 42 且 stderr 空；`--locked --offline` 重建成功 |
| H18-09 回归 | `cargo test -p dolphin-compiler --test manifest h18_09` | Cranelift/Debug | 通过：`h18_09_method_diagnostics_use_defining_file` 1 passed |
| FFI Windows fixture（H18-09 改动 6 的 Windows 侧） | `cargo test -p dolphin-compiler --test ffi` | Cranelift/Debug | 通过：7 passed（`cl` 编译对象、`llvm-ar` 归档走 Windows 分支） |
| LLVM 对照 `tests/backend.rs` | `cargo test -p dolphin-compiler --test backend` | — | 0 tests（`--features llvm` 门控）；Windows lane 本就不含 LLVM，未验证 |
| 默认后端观测 | `target\release\dc.exe env` | — | `backend: cranelift (default)`；host/target 均为 `x86_64-pc-windows-msvc` |

### 修复前复现结果（CRLF 根因）

```text
CRLF 副本（与 runner core.autocrlf=true 检出等价）：
  dc fmt --check <file> -> "would reformat ..."; exit=1
LF 副本（同一内容仅换行不同）：
  dc fmt --check <file> -> exit=0
整目录（examples + crates/dolphin-std/src 的 LF 副本，53 文件）：
  dc fmt --check examples crates\dolphin-std\src -> exit=0
```

### 修复后结果

源码未修改；“CI-05 修复后复验”行是 `.gitattributes` 落地后在 runner 式全新检出上的结果，
其余行是 HEAD（H18-09）在 Windows 的实际结果。

### 实际运行命令与测试数量

```powershell
cargo fmt --all -- --check                                        # 通过
cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings  # 通过
cargo test --workspace --exclude dolphin-codegen-llvm             # 315 passed; 0 failed
$env:DOLPHIN_BACKEND="cranelift"; cargo test --workspace --exclude dolphin-codegen-llvm  # 315 passed; 0 failed
git diff --check                                                  # 通过
cargo build --release --bins                                      # 通过（Release，17.77s 增量）
target\release\dc.exe fmt --check examples crates\dolphin-std\src # exit 1（CRLF），见上表
python scripts/package.py --target x86_64-pc-windows-msvc         # zip + sha256
bash.exe -c "bash -x ci-smoke.sh"                                 # 冒烟序列 exit 0（干净 examples 副本）
cargo test -p dolphin-compiler --test manifest h18_09             # 1 passed
cargo test -p dolphin-compiler --test ffi                         # 7 passed
cargo test -p dolphin-compiler --test backend                     # 0 tests
# CRLF 修复验证（临时目录，不动本仓库）：
#   clone（core.autocrlf=true）-> 提交 .gitattributes -> 再 clone
#   git ls-files --eol examples/m14/src/main.do  =>  i/lf  w/lf  attr/text=auto eol=lf
#   dc fmt --check examples crates\dolphin-std\src  =>  exit 0
```

默认测试总数：Windows 315（Linux 同命令 316）。差值 1 为
`tests/ffi.rs::ffi06_shared_library_integration`（`#[cfg(unix)]`，tests/ffi.rs:519），
属既有平台门控，不是本批删除或跳过。日志在
`C:\Users\jiangyc\AppData\Local\Temp\opencode\h18-09-win\`（`logs\`、`smoke-root\`、`lf-check\`、
CRLF 验证用 clone），不在仓库内；仓库内新增但未跟踪的是 `.gitattributes` 与本机打包产物
`dist/dolphin-0.2.0-x86_64-pc-windows-msvc.zip`（43 MB，按需保留或删除），均未提交。

### 未运行的检查及原因

- 远端 GitHub CI（windows-latest 真实 runner、`release` job、tag）：本机无远端权限，未 push/tag，不伪造。
- LLVM 22 lane：配置只在 Ubuntu 运行；本机无 LLVM 22 开发库（`llvm-config` 缺失，scoop LLVM 为 23.1.1），
  `--features llvm`、`tests/backend.rs` 固定期望、llvm 门控的
  `h18_08_build_02_library_and_bins_share_effective_profile` 均未在 Windows 验证。
- macOS：无设备。
- `cargo test --release`（Rust profile）：与 H18-09 节相同理由，不改变 fixture 的 Dolphin profile。

### 行为/兼容变化

- 无源码、IR、公开 CLI 或归档格式变化。
- 经用户确认，仓库根新增 `.gitattributes`：`* text=auto eol=lf`（未提交）。三平台检出统一 LF；
  索引内容本来已是 LF，本次不产生任何跟踪文件的内容变更（`git status` 无批量修改，`git diff`
  仅文档）。既有 Windows 工作树仍是 CRLF，本地要得到同一行为需重新检出（CI 每次全新检出）。
- 发现一项 CI 配置级风险：`dc fmt --check` 对 CRLF 检出失败，而 GitHub `windows-latest` runner
  默认 `core.autocrlf=true`（actions/runner-images 明确按 Git for Windows 默认配置 runner）且仓库无
  `.gitattributes`；修复见下节。

### 剩余问题和下一批输入（H18-10 / H18-11）

1. **已决策并落地**：用户选择“加 `.gitattributes` 强制 LF”。仓库根新增 `.gitattributes`
   （`* text=auto eol=lf`，未提交），并在 `core.autocrlf=true` 的全新 clone 上复验：
   `examples/m14/src/main.do` 检出为 `w/lf`、clone `git status` 干净、`dc fmt --check examples
   crates\dolphin-std\src` exit 0。H18-10/H18-11 提交时需带上该文件；远端 Windows lane 的最终
   绿灯仍待在真实 runner 上确认。
2. 本机首次冒烟在“已有 `examples/*/target` 的工作树”上失败（exit 126）：Git Bash 优先执行无扩展名的
   旧 ELF 残留（`examples/m14/target/m14`），而不是新构建的 `m14.exe`。干净检出（无 `target/`）不受影响；
   H18-11 的示例/集成验证应在干净副本或先清理 `target` 后执行。
3. Windows 上 `examples/*/target` 存在跨平台残留（ELF），后续批次不要把 `target` 内容当输入或证据。
4. 其余 H18-10/H18-11 输入与上一节相同；本补充不把 H18-10..11 记为完成。

## H18-09-M macOS 平台补充验证

- 批次：H18-09 的 macOS 平台复验（补充节，不新增批次编号）
- 状态：完成（macOS 26.6.2 arm64 本机：默认 Cranelift lane、Darwin shared-library fixture、
  发行打包与归档冒烟、LLVM 22 lane 全部通过；发现并修复 macOS runner 的
  rust-lld/libLLVM 环境缺口与两个 ELF 专用 DWARF 断言；远端 CI/tag 门禁未验证）
- 前置批次及报告：H18-08、H18-09（本文件 H18-09 节）与 H18-09-W（Windows 补充节）

### 环境

| 项 | 值 |
| --- | --- |
| 平台 | macOS 26.6.2（25G83），arm64（Apple Silicon） |
| rustc / cargo | 1.98.1（host `aarch64-apple-darwin`） |
| C 工具链 | Apple clang 21.0.0（Command Line Tools）；`cc`/`ar`/`install_name_tool`/`dsymutil` 可用 |
| LLVM 22 | Homebrew `llvm@22` 22.1.8（keg-only，用户手动安装），`LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm@22` |
| `DOLPHIN_BACKEND` | 未设置；`dc env` 报默认后端 `cranelift`、linker `rust-lld`、system linker `cc` |
| 验证用 `dc` | `cargo build --release --bins`（HEAD 全部改动）与打包后的 `dist/dolphin-0.2.0-aarch64-apple-darwin.tar.gz` |

### 平台缺口与修复：macOS 上 rust-lld/rust-objcopy 无法加载 libLLVM

未处理时 macOS 上 `cargo test` 的每一次链接都失败（`tests/build.rs` 首轮即
`20 passed; 63 failed`，全部为同一诊断）：

```text
dyld[...]: Library not loaded: @rpath/libLLVM.dylib
  Referenced from: .../.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/bin/rust-lld
  Reason: tried: '.../rustlib/aarch64-apple-darwin/bin/../lib/libLLVM.dylib' (no such file), ...
```

根因：Rust 1.96+ 的 rustup `rust-lld` 动态依赖 `@rpath/libLLVM.dylib`，其 `LC_RPATH` 为
`@loader_path/../lib`（即 `rustlib/<host>/lib`）和官方构建机绝对路径，而真实库位于
`$SYSROOT/lib/libLLVM.dylib`；`rust-objcopy`（build.rs 的 strip 步骤）同样受影响，只告警不失败。
本机临时验证统一加 `DYLD_FALLBACK_LIBRARY_PATH="$(rustc --print sysroot)/lib"`。

**已按用户批准做最小 CI 修复**（不改源码）：`.github/workflows/ci.yml` 的 `test` job 增加
`if: runner.os == 'macOS'` 步骤，把该回退路径写入 `$GITHUB_ENV`，供后续测试/构建步骤使用。
发行包侧不受影响：`scripts/package.py` 已复制 `libLLVM.dylib` 并为 rust-lld 追加 `@loader_path`
rpath，冒烟已验证。

### 验收映射（macOS）

| 检查 | 命令/测试 | 后端/profile | 结果 |
| --- | --- | --- | --- |
| CI-02 默认 lane（macOS 行） | `cargo test --workspace --exclude dolphin-codegen-llvm` | Cranelift，fixture Debug/Release | 通过：316 passed / 0 failed，exit 0 |
| CI-02 显式后端（合同 5.1） | `DOLPHIN_BACKEND=cranelift cargo test --workspace --exclude dolphin-codegen-llvm` | Cranelift | 通过：316 passed / 0 failed |
| CI-05 格式检查 | `cargo fmt --all -- --check`；`./target/release/dc fmt --check examples crates/dolphin-std/src` | n/a | 均 exit 0（`.gitattributes` 保证 LF 检出） |
| CI-05 默认 Clippy | `cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings` | n/a | 通过 |
| CI-04 打包 | `python3 scripts/package.py --target aarch64-apple-darwin` | n/a | 通过：tar.gz 49 672 968 B，sha256 `0ea35d95c51b2564d212ab47e6255e9a8ccc3da3006c37571050c865348ac823` |
| CI-04 归档冒烟（ci.yml 原样脚本） | 干净 examples 副本 + 上述归档，`bash -x` 执行 | 归档内 `dc` | 通过 `SMOKE_SEQUENCE_OK`：m8 release exit 64；m14/m15 exit 0 且 stderr 空；m15 stdout `42 22 true`；`.dlib`+`.sha256` 生成；`fetch`+`--locked` 构建 exit 42 且 stderr 空；`--locked --offline` 重建成功 |
| H18-09 改动 6（Darwin fixture） | `cargo test -p dolphin-compiler --test ffi` | Cranelift/Debug | 通过：8 passed，含 `ffi06_shared_library_integration`；另用 `otool -D` 实测产物为 `@rpath/libdemo.dylib`，未传 ELF `-soname` |
| H18-09 回归 | `cargo test -p dolphin-compiler --test manifest h18_09` | Cranelift/Debug | 通过：`h18_09_method_diagnostics_use_defining_file` 1 passed |
| BUILD-04/CI-02 固定期望 | `cargo test -p dolphin-compiler --features llvm --test backend` | Cranelift+LLVM × Debug/Release | 通过：4 passed；`debug_backends_agree`/`release_backends_agree` 在 macOS 兑现 exit 25、stdout `7 10 12 25\n`、stderr 空 |
| CI-02 LLVM lane（macOS 补跑） | 见下节 | 显式 LLVM | 通过：workspace 323、显式后端列表 146、backend 4 |

### LLVM 22 lane（macOS 补跑；CI 的 LLVM lane 本身只要求 Ubuntu）

| 命令 | 结果 |
| --- | --- |
| `llvm-config --version` | 22.1.8 |
| `cargo build --bins --features llvm` | 通过 |
| `cargo clippy --workspace --all-targets --features llvm -- -D warnings` | 通过 |
| `DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm` | 323 passed / 0 failed（与 Linux 计数一致） |
| `DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm --test build --test ffi --test cli --test manifest --test packages` | 146 passed / 0 failed（与 Linux 计数一致） |
| `cargo test -p dolphin-compiler --features llvm --test backend` | 4 passed |

首次运行有两个测试失败，均为 ELF 专用断言而非编译器缺陷；**已按用户批准做最小平台感知修复**：

- `tests/backend.rs::llvm_debug_profile_emits_dwarf`：原断言可执行文件字节含 `.debug_line`
  （修复前 panicked at tests/backend.rs:164，消息为 “debug executable must contain `.debug_line`”）。
  macOS 上 Mach-O 链接器按平台惯例不把 DWARF 复制进可执行文件：实测 `program.o` 含
  `__debug_info`/`__debug_line`（8 个 `__DWARF` 段，`dwarfdump` 160 行 debug_info），且
  `dsymutil program -o program.dSYM` 能从 debug map 还原完整 dSYM（318 行 debug_line）；
  对照实验：Apple clang `cc -g` 的可执行文件同样不含 `__DWARF`。修复后 macOS 检查
  `program.o` 的 `__debug_info`/`__debug_line`，非 macOS 仍检查可执行文件的 `.debug_*`。
- `tests/cli.rs::h18_08_build_02_library_and_bins_share_effective_profile`：原断言
  `target/lib/mixed.o` 与可执行文件含 `.debug_info`（修复前 `panicked at tests/cli.rs:725:
  ... target/lib/mixed.o must be built with the Debug profile`）。macOS 的 `mixed.o` 实测含
  `__debug_info`（Debug）且 Release 不含，可执行文件按平台惯例无 DWARF。修复后 macOS 检查
  lib/bin 的对象文件（`target/lib/mixed.o`、`target/app.o`）与 `__debug_info`，非 macOS 路径不变
  （仍检查 `.debug_info` 与可执行文件）。
- 修复后复验：`--test backend` 4 passed；`--test cli h18_08_build_02...` 1 passed；上表完整
  LLVM lane 全绿。默认（无 llvm feature）路径不受影响，重跑仍 316 passed。

### 修复前复现结果（摘要）

1. 无 `DYLD_FALLBACK_LIBRARY_PATH`：`cargo test --workspace --exclude dolphin-codegen-llvm`
   在 `tests/build.rs` 得到 `20 passed; 63 failed`，诊断均为
   `error[E0000]: linking ... failed \n dyld[...]: Library not loaded: @rpath/libLLVM.dylib`。
2. `llvm_debug_profile_emits_dwarf`：panicked，消息为 “debug executable must contain `.debug_line`”；
   `h18_08_build_02_library_and_bins_share_effective_profile`：panicked，消息为
   “target/lib/mixed.o must be built with the Debug profile”。两项根因证据见上节。
3. `rust-objcopy` 失败以构建警告形式出现（build script 不因此失败）：
   `warning: stripping debug info with rust-objcopy failed: signal: 6 (SIGABRT)`。

### 修复后结果

- macOS 默认 lane 316 passed、LLVM lane 323/146/4 passed，均与 Linux 计数一致；`ffi` 在
  Darwin 上用 `.dylib` + `@rpath` install_name 通过（Windows 因 `#[cfg(unix)]` 少的那一项在
  macOS 正常执行）。
- 发行包在 macOS 解压即可用：打包脚本的 rust-lld rpath 修复 + 随附 `libLLVM.dylib` 使归档内
  `dc` 不依赖 rustup 工具链，完整冒烟序列（含 `--locked --offline`）通过。
- 两个 DWARF 断言改为平台感知后，macOS LLVM Debug 的 DWARF 证据落在对象文件与 dSYM，
  与 Apple 工具链一致；M20-05 在 macOS 用 lldb 调试时应依赖 .o/debug map 或 `dsymutil` 产物。

### 实际运行命令与测试数量

```bash
# 默认 lane（macOS；均带 DYLD_FALLBACK_LIBRARY_PATH="$(rustc --print sysroot)/lib"）
cargo fmt --all -- --check                                            # 通过
cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings  # 通过
cargo test --workspace --exclude dolphin-codegen-llvm                 # 316 passed
DOLPHIN_BACKEND=cranelift cargo test --workspace --exclude dolphin-codegen-llvm  # 316 passed
cargo build --release --bins                                          # 通过（伴随 rust-objcopy 告警，见上）
./target/release/dc fmt --check examples crates/dolphin-std/src       # exit 0
python3 scripts/package.py --target aarch64-apple-darwin --out-dir /tmp/...  # tar.gz + sha256
# 冒烟：干净副本中按 ci.yml 原样脚本执行（仅 python→临时 shim 与 DOLPHIN_HOME 隔离）
#   -> SMOKE_SEQUENCE_OK
cargo test -p dolphin-compiler --test ffi                             # 8 passed
cargo test -p dolphin-compiler --test manifest h18_09                 # 1 passed

# LLVM lane（macOS，LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm@22）
cargo build --bins --features llvm                                    # 通过
cargo clippy --workspace --all-targets --features llvm -- -D warnings # 通过
DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm      # 323 passed
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages  # 146 passed
cargo test -p dolphin-compiler --features llvm --test backend         # 4 passed
cargo fmt --all -- --check && git diff --check                        # 通过
```

日志与产物在 `/tmp/opencode/h18-09-macos/`（`logs/`、`dist/`、`smoke-root/smoke-run.log`、
`dwarf/`、`mixed/`），不在仓库内。CI YAML 用 `python3` + PyYAML（临时 venv）解析验证：
jobs `test`/`llvm`/`release`、`release.needs=['test','llvm']`、新 macOS 步骤仅在
`runner.os == 'macOS'` 时写入 `DYLD_FALLBACK_LIBRARY_PATH`。

### 本批修改文件（macOS 补充，均未提交）

- `.github/workflows/ci.yml`：macOS-only 的 rust-lld/libLLVM 回退路径步骤（用户批准）。
- `tests/backend.rs`、`tests/cli.rs`：两个 DWARF 断言平台感知（用户批准）；非 macOS 分支
  逻辑与产物不变。`cargo fmt`/clippy 全绿，默认与 LLVM lane 复验通过。
- 文档：本报告本节与 `docs/plan-m18-correctness.md` 的 H18-09 状态行。

### 未运行的检查及原因

- 远端 GitHub CI（macos-latest 真实 runner、`release` job、tag）：本机无远端权限、未 push/tag，
  新增的 macOS 步骤只在本机等价验证，不伪造 runner 结果。
- macOS 的 LLVM lane 不在 `ci.yml` 矩阵内（LLVM lane 只要求 Ubuntu）；本地补跑不改变 CI 覆盖面。
- Windows：本补充未在 Windows 运行；上一节结论不变（Windows LLVM 未验证）。
- `cargo test --release`（Rust profile）：与 H18-09 节相同理由，不改变 fixture 的 Dolphin profile。
- `dc package`/`dc publish` 的更多组合、H21 干净环境验收：不在本补充范围。

### 行为/兼容变化

- 无源码、IR、公开 CLI、`.dlib` 格式或打包行为变化；`dc`/`dolphin-compiler` 二进制不变。
- 测试变化仅限 macOS 分支的 DWARF 产物/段名选择；此前 macOS LLVM 下红的两项现在按
  Mach-O 平台事实通过，Linux/Windows 路径未改。
- CI 变化仅新增 macOS-only 环境回退步骤；三平台默认 lane、LLVM lane（Ubuntu）与 release
  依赖关系未变。

### 剩余问题和下一批输入（H18-10 / H18-11）

1. 远端 macOS lane 的最终绿灯仍需在真实 runner 上确认；本机等价验证为
   `DYLD_FALLBACK_LIBRARY_PATH=$(rustc --print sysroot)/lib` 后 316/323/146/4 全绿。
2. H18-10 文档核正时：macOS 上 LLVM Debug 的 DWARF 位于对象文件与 debug map（可用
   `dsymutil` 生成 dSYM），不要写成可执行文件内嵌；LLVM 22 安装说明可用 Homebrew
   `llvm@22` + `LLVM_SYS_221_PREFIX` 的已验证路径。
3. H18-11 集成验收：三平台默认 lane 与 Linux LLVM lane 之外，建议把本节的 macOS LLVM 命令
   作为本地可选复验；`examples/m18` 与发行包冒烟在新示例加入后需按 ci.yml 脚本重跑。
4. 无阻塞；本补充不把 H18-10/H18-11 记为完成。

### H18-09-CI 修复：LLVM lane 缺失 Polly 静态库

- 触发：GitHub Actions 的 `llvm` job 在 `cargo build --bins --features llvm` 失败：

```text
error: could not find native static library `Polly`, perhaps an -L flag is missing?
error: could not compile `llvm-sys` (lib) due to 1 previous error
```

- 根因：`llvm-sys` 221 的 `LinkingPreferences::init` 默认 `prefer-static`（`build.rs` 只认
  cargo feature `prefer-static`/`prefer-dynamic`/`force-*`，没有环境变量开关），会链接
  `llvm-config --link-static --libs` 列出的静态组件；apt.llvm.org 把 Polly 拆到单独的
  `libpolly-22-dev`，H18-09 lane 只装了 `llvm-22-dev`，因此缺 `libPolly.a`。
- 修复：`.github/workflows/ci.yml` 的安装步骤改为
  `sudo apt-get install -y llvm-22-dev libpolly-22-dev`，并在版本检查步骤增加
  `test -f "$(llvm-config --libdir)/libPolly.a"`，让环境不合格时在构建前给出明确失败；
  `docs/installation.md` 的 LLVM 22 一节同步安装包与检查命令。
- 证据与限制：apt.llvm.org 官方包列表包含 `libpolly-22-dev`（Ubuntu resolute 也有该包），
  与 CI 报错一致；本机（Arch LLVM 22.1.8）的 `llvm-config --link-static --libs` 不含 Polly、
  `libPolly.a` 也不存在，所以本地 `--features llvm` 从不触发该问题，无法在本机复现 apt 拆包
  路径。已用 PyYAML 解析验证 `ci.yml`，并确认新增的 `libPolly.a` 检查在本机会按预期报缺库
  （说明该守卫能拦截原先的失败条件）。
- 状态：配置与文档已修复；用户确认 GitHub Actions 在后续提交上通过（三平台 `test` 矩阵与 `llvm` lane；
  tag 上的 `release` 仍未触发，保持“未验证”）。
- 该修复不影响本批测试数量与其它结论。

## H18-10 当前文档与可运行示例核正

- 批次：H18-10
- 状态：完成（Linux x86_64；两后端；DOC-01..04 有自动化证据；macOS/Windows 平台 lane 待平台验收）
- 前置批次及报告：H18-09（含 -W Windows / -M macOS 平台补充），见上两节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`73d2128951ba2fe8ddc006dde5a415b2a13cfb59`（`v0.2.0-M18-09-mac`），工作区干净。
- 本批修改未提交：`tests/doc_examples.rs`（新增）、`tests/build.rs`、
  `crates/dolphin-hir/src/modules.rs`（示例暴露的缺陷修复）、网站 4 个内容文件、
  `docs/language-design.md`、`docs/implemented-features.md`、`docs/roadmap.md`、
  `README.md`、`docs/installation.md`、`.github/workflows/ci.yml`、`docs/plan-m18-correctness.md`。

### 修改文件与关键实现

- `tests/doc_examples.rs`（新增，文档 fixture/抽取机制，无 Node 依赖）：
  - 抽取当前文档：Markdown 的 ` ```dc ` 块与网站内容里的 `<pre><code>` 块（HTML 实体解码）；
    候选为含 `fn main` 的完整程序或以 `// src/` 标注的多文件块。
  - `CLASSIFICATION` 表按 `(文件, 候选序号, 首行提示)` 给每个候选明确类型：成功程序（运行并断言
    stdout/exit/stderr）、只构建、预期诊断、片段、多文件工程、历史草案、未来语法。首行提示不符即
    失败，防止文档改动静默漂移。
  - `doc_examples_build_and_run`：非 `zh-*` 的 `Run`/`ProjectRun` 用真实 `dc` 编译运行；多文件工程把
    同文件连续的 `// src/` 块组装成项目（README 与 tutorial 共 5 组）；`Project` 分类显式映射到
    `examples/m7` 与 `tests/build.rs::builds_and_runs_m7_modules`。
  - `website_translations_are_in_sync`：英文/中文网站候选逐对比较（去掉行注释与空白），防翻译漂移。
  - `current_doc_links_exist`：当前文档相对链接必须存在（历史提案的旧 `src/*.rs` 路径表已明确标注，
    不在检查范围）。
  - `fixed_defects_moved_to_history`：§1.1 不再枚举已修缺陷，§1.2 记录 H18-01..05/07/08/09 并链接 `tests/`。
- `crates/dolphin-hir/src/modules.rs`：`is_constructor_target` 在枚举项构造分支补查
  `qualify(module, prefix)`。此前类型表以「模块.类型」为键，子模块内 `Enum.Variant(...)` 被误报
  `unknown function`；网站最终工程示例暴露该缺陷。
- `tests/build.rs`：新增 `h18_10_enum_construction_in_submodule`（两后端 × Debug/Release），并引入
  `mod support;` 复用后端枚举。
- 网站示例修复（en/zh 同步，代码逐块一致）：
  - 拥有对象 `val` 却调用 `deinit`/可变方法改为 `var`：tutorial 的 concat 结果，std 的 `String::from`、
    `concat`、`Vec.clone`、CString `match` 结果与内部 NUL 片段。
  - 索引/长度参数补 `usize`：`get(0_usize)`、`substring(..., 0_usize, 2_usize)`。
  - CString 示例 `strlen` 返回类型 `c_ulong` → `usize`（C `size_t`；`c_ulong` 在 Windows 仅 32 位）。
  - `[Token; 3]`（枚举定长数组）改为 `step(token, pending)` 辅助函数按序调用，保持 13 的输出。
  - match 作为语句（不支持）改为绑定表达式的形式；match 分支里的 `if ... else` 表达式改为普通分支。
  - `src/stdlib/`（错误路径）→ `crates/dolphin-std/src/`；`match` 语句限制的表述在
    language-design/implemented-features/roadmap/网站中英同步修正。
- `docs/language-design.md`：18.3 match 示例补上 `enum Shape` 使其可独立编译；修正“作为语句时忽略
  结果”的表述。
- 显式 `--test` 列表加入 `doc_examples`：`.github/workflows/ci.yml` 的 LLVM lane、README 开发验证、
  installation 的 LLVM 环境一节；默认 `cargo test --workspace` 自动覆盖三平台。

### 验收映射

| 验收点 | 测试名 | 结果 |
| --- | --- | --- |
| DOC-01 | `tests/doc_examples.rs::current_doc_links_exist` | 通过：当前文档相对链接 0 死链；历史提案旧路径表按标注排除 |
| DOC-02 | `doc_examples_are_classified`、`doc_examples_build_and_run` | 通过：68 个候选全部有类型；每后端执行 30 个（25 单文件 Run + 5 多文件工程 ProjectRun），固定 stdout/exit 且 stderr 空 |
| DOC-03 | `website_translations_are_in_sync` | 通过：en/zh 各自 22（tutorial）/ 8（std）块去注释后逐对一致 |
| DOC-04 | `fixed_defects_moved_to_history` | 通过：§1.1 不再列已修缺陷，§1.2 覆盖 H18-01..05/07/08/09 且链接 `tests/`；本批把 H18-10 缺陷补入 |
| 定向核查 | 饱和规则（language-design §5.1、implemented-features §4.6）；泛型/impl 子集（§12、language-design §18.5）；LSP 单文件限制（§18.3）；DWARF 范围（§18.2）；SDK 依赖（installation §6）；`.dlib` 版本（installation §7） | 文档与当前实现一致，无需改动 |
| 缺陷回归 | `tests/build.rs::h18_10_enum_construction_in_submodule` | 修复前 FAILED（`unknown function Query.Found`）；修复后两后端 × Debug/Release 通过 |

### 修复前复现结果

1. 编译器缺陷（网站最终工程示例暴露）：`pkg report` 内 `Query.Found(best)` 报
   `error[E0001]: unknown function `Query.Found``。临时 `git stash push -- crates/dolphin-hir/src/modules.rs`
   后 `cargo test -p dolphin-compiler --test build h18_10_enum` = `0 passed; 1 failed`
   （`error[E0001]: unknown function `Query.Found``）。
2. 文档示例机制对修复前文本：`git stash push -- docs/website crates/dolphin-hir/src/modules.rs docs/language-design.md`
   后 `cargo test -p dolphin-compiler --test doc_examples doc_examples_build_and_run` 失败，首条为
   `en-tutorial.js [11] ... stderr=error[E0001]: nested arrays are not implemented yet`；后续还会命中
   `cannot call deinit on an immutable receiver`、`expected usize, found i32`、最终工程 `unknown function`。
3. 手工审计（单文件/工程模式，修复前 `dc`）：真正需要修复的类别为枚举定长数组（`[Token; 3]`）、
   `val` 不可变接收者（tutorial/std 共 5 处）、索引字面量类型（`get(0)`/`substring(...,0,2)` 共 2 类）、
   match 语句与分支 if 表达式（最终工程/tutorial 各 1 处）、`c_ulong`（en/zh CString 各 1 处）；
   `use mathutil`/`geom.shapes`/`report.stats` 多文件块在组装成工程后编译运行正常，单文件模式报
   `use requires a project` 属预期，不作为缺陷。

### 修复后结果

- `cargo test -p dolphin-compiler --test doc_examples`：5 passed；`--features llvm` 时同样 5 passed
  （每后端执行 30 个示例）。
- 网站 tutorial 的 4 个多文件工程与 README 的 mathutil 工程按文档块组装后编译运行，输出与文档一致：
  `min = 3`、`p = (3, 4)`、`result = 13`、`total = 255, average = 85 / best = 95`。
- CString 示例调用 libc `strlen` 返回 7（`usize` 声明），Debug 无泄漏。
- 全量回归未回退（见下）。

### 实际运行命令与测试数量

```bash
cargo test -p dolphin-compiler --test doc_examples                    # 5 passed
cargo test -p dolphin-compiler --features llvm --test doc_examples    # 5 passed
cargo test -p dolphin-compiler --test build h18_10_enum               # 1 passed
cargo test --workspace --exclude dolphin-codegen-llvm                 # 322 passed (316→322)
cargo test --workspace --features llvm                                # 329 passed (323→329)
DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test cli \
  --test manifest --test packages --test doc_examples                 # 152 passed (146→152)
DOLPHIN_BACKEND=llvm cargo test --features llvm --test build --test ffi --test cli \
  --test manifest --test packages --test doc_examples --test m18_harness --test m18_place \
  --test m18_layout --test m18_cast --test m18_bounds --test m18_impl --test m18_sret
                                                                      # 200 passed (194→200)
cargo test -p dolphin-compiler --features llvm --test backend         # 4 passed
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --features llvm \
  -- -D warnings && git diff --check                                  # 全部通过
```

默认测试总数 316→322，LLVM 323→329，合同 5.2 显式列表 146→152，带 m18 的显式列表 194→200
（新增 `tests/doc_examples.rs` 5 项与 `tests/build.rs` 1 项）。日志在 `/tmp/opencode/h18-10/`。

### 未运行的检查及原因

- macOS/Windows 平台 lane：本批未在对应平台重跑；doc_examples 只做编译/运行与固定输出断言，属平台
  无关，但仍以 H18-09 的平台 lane 与 H18-11 集成验收为准。
- 远端 CI：本机未 push/tag，不伪造远端结果。
- 历史资料不做自动化编译：`proposal-m14`/`proposal-m15`/`plan-m14-m15-rework` 的原始路径表与示例按
  合同保留为“历史草案”，只在 DOC-01 链接检查中显式排除。
- `language-design.md` 的 `fn main(args: []string)`（未来语法，正文标注当前 `main` 不能接收参数）与
  §12/§15 自建 `std` 示例（历史示例，已指向 `examples/m7`）未改造为当前可运行程序。
- 预期诊断类（`Class::Reject`）在当前文档中没有实例，分类机制保留该类型备用。

### 行为/兼容变化

- 编译器：子模块（`pkg`）内用裸名构造本模块枚举项由误报 `unknown function` 变为可用；这是修正错误
  拒绝，不改变公开语法、IR、布局或打包格式。
- 文档：修正不能编译/误导的示例与表述，不新增语言能力承诺；网站标准库路径改为实际位置。
- 测试：新增 `tests/doc_examples.rs`，默认/LLVM 测试数变化；显式 `--test` 列表已同步 CI 与文档。

### 剩余问题和下一批输入（H18-11）

1. H18-11 范围：核对 H18-00..10 报告与全部验收编号；跑第 5 节完整本机门禁；增加 `examples/m18`
   组合回归示例（泛型容器/枚举、跨函数聚合与指针别名写、defer 清理，写明 stdout/exit/stderr）；
   核查安装/打包说明、旧里程碑回归与发行包 smoke；确认无 P0/P1 已知正确性问题被无理由推迟。
2. `examples/m18` 加入后需按合同 5.3 把 `dc fmt/check/run` 命令补入文档与 CI 冒烟；本批新增的
   `doc_examples` 不覆盖 `examples/`（由 H18-11 的示例回归覆盖）。
3. 本批修复的枚举构造缺陷只覆盖子模块裸名形式；跨模块 `mod.Enum.Variant(...)` 既有路径由
   `tests/build.rs::function_named_like_enum_variant_is_not_enum_construction` 等回归继续保护。
4. 无阻塞；本报告不把 H18-11 记为完成。

## H18-11 全量集成与阶段验收

- 批次：H18-11
- 状态：完成（Linux x86_64；两后端 × Debug/Release 本机；三平台默认 Cranelift lane 与 Linux LLVM lane
  由用户确认远端 CI 通过；云 tag/`release` job 在发布 0.3.0 时最终确认）
- 前置批次及报告：H18-10，见本文件上一节

### 开始 HEAD 与已有本地改动

- 开始 HEAD：`7042b6d18bea3cfbfd88167cb73d139b4009beff`（`v0.2.0-M18-10-01`）。
- 工作区开始时有已 staged 的 `docs/reports/m18-progress.md`（H18-09-CI 修复的状态行更新），已保留。
- 本批修改：新增 `examples/m18/`（`dolphin.toml`、`src/main.do`、`README.md`）、
  `examples/README.md`、`tests/build.rs`、`.github/workflows/ci.yml`（smoke 加 m18）、
  `docs/roadmap.md`、`README.md`、`docs/implemented-features.md`、`docs/plan-m18-correctness.md`、
  `docs/reports/m18-progress.md`；并按用户批准的发布决策把编译器版本从 `0.2.0` 升到 `0.3.0`
  （`Cargo.toml`、`Cargo.lock`、`examples/m9|m15|m16` 的 lock 文件）。未 commit/push/tag。

### examples/m18（组合回归示例）

- 内容：泛型容器 `Box<T>`/泛型枚举 `Maybe<T>` 与 `match`；指针别名写
  `pair.x = mutate(&pair)`（RHS 经 `*Pair` 把 `y` 改为 9）；`Pair` 按值传参与返回（聚合 sret）；
  `Vec<i32>` + `defer numbers.deinit()`；`mem.alloc`/`mem.free` + `defer` 且切片跨函数求和。
- 不依赖网络、不读未初始化存储、不使用未实现语法；`dc fmt --check` 通过。
- 固定结果（Cranelift/LLVM × Debug/Release 一致）：stdout
  `alias = 7 9\ngeneric = 40\nshifted = 8 10\nsum = 12\n`，exit `0`，stderr 空。
- 回归测试：`tests/build.rs::h18_11_examples_m18_combination_regression`（可用后端 × Debug/Release，
  直接构建 `examples/m18` 清单并断言 stdout/exit/stderr）。
- 命令已加入文档与 CI 冒烟（合同 5.3）：`examples/README.md`、`examples/m18/README.md`、
  `.github/workflows/ci.yml` 发行包 smoke。

### 验收编号核对（H18-00..10，缺项可见）

| 编号 | 真实测试名（文件省略前缀） | H18-11 复验 |
| --- | --- | --- |
| H18-00 驱动 | `m18_harness.rs`：`harness_captures_success_stdout_and_exit`、`harness_reports_compile_failure_without_running`、`harness_captures_runtime_trap_exit_and_stderr`、`harness_honors_dolphin_profile`、`harness_terminates_timeout_and_reaps_child`、`harness_selects_explicit_llvm_backend` | 通过 |
| PLACE-01..08 | `m18_place.rs`：`place_01_field_alias_keeps_rhs_side_effect`、`place_02_array_rhs_element_alias_preserved`、`place_03_index_target_evaluated_once`、`place_04_rhs_target_and_other_field`、`place_05_rejects_immutable_and_out_of_bounds_targets`、`place_05_dynamic_out_of_bounds_traps`、`place_06_pointer_slice_and_aggregate_fields`、`place_07_bounds_check_before_rhs`、`place_08_pointer_rebind_uses_pre_rhs_address`、`place_08_slice_descriptor_rebind_uses_pre_rhs_address` | 通过 |
| LAYOUT-01..06 | `m18_layout.rs`：`layout_01_enum_sizes_and_alignments`、`layout_02_enum_slice_alloc_address_and_write`、`layout_03_enum_in_struct_and_struct_in_enum`、`layout_04_payloads_match_params_returns_and_copy`、`layout_05_free_full_slice_without_mismatch`、`layout_05_dynamic_bounds_trap_in_both_profiles`、`layout_06_rejects_oversized_nested_aggregate`；`dolphin-ir`：`enum_without_payload_is_four_four`、`enum_payload_starts_at_eight_and_size_is_padded`、`enum_payload_size_is_exact_for_max_components`、`enum_in_struct_uses_aligned_field_offsets` | 通过 |
| CAST-01..02 | `m18_cast.rs`：`cast_01_float_to_int_saturates`、`cast_02_full_matrix_saturates_with_nan_zero` | 通过 |
| GEN-01..05 | `m18_bounds.rs`：`gen_01_ignored_type_bound_is_rejected`、`gen_02_positive_hold_and_maybe`、`gen_02_negative_hold_and_maybe`、`gen_03_positive_signature_and_nested`、`gen_03_negative_nested_and_signature`、`gen_03_cross_module_trait_identity`、`gen_03_cross_module_negative_uses_qualified_trait`、`gen_05_associated_type_fields_and_payloads`、`gen_05_negative_type_and_missing_impl`；`manifest.rs::h18_04_type_bounds_across_path_dependency`；GEN-04 既有：`build.rs::m15_generic_functions_types_and_methods`、`m15_type_parameter_bounds_resolve_associated_types`、`m15b_iterator_protocol_and_rejection`、`rejects_unbounded_generic_type_expansion` | 通过 |
| IMPL-01..04 | `m18_impl.rs`：`impl_01_construct_then_call_renamed_params`、`impl_01_associated_function_first_instantiation`、`impl_01_bound_checked_through_method_instantiation`、`impl_02_specialization_is_rejected`、`impl_02_missing_and_repeated_arguments_are_rejected`、`impl_02_reordered_and_arity_mismatch_are_rejected`、`impl_02_nested_argument_is_rejected`、`impl_02_blanket_impl_is_rejected`、`impl_02_impl_parameter_bound_is_rejected`、`impl_02_method_type_parameter_is_rejected`、`impl_02_generic_trait_argument_is_rejected`、`impl_02_unknown_target_is_rejected`、`impl_03_same_name_method_conflict_is_rejected`；`manifest.rs::h18_05_parameterized_impl_across_path_dependency` | 通过 |
| IR-01..05 | `dolphin-ir::verify::tests` 18 项（`valid_minimal_program_passes`、`struct_kind_mismatch_is_rejected`、`value_self_cycle_is_rejected`、`value_mutual_cycle_is_rejected`、`pointer_recursion_is_allowed`、`invalid_local_index_is_rejected`、`invalid_block_target_is_rejected`、`location_count_mismatch_is_rejected`、`branch_condition_must_be_bool`、`set_local_type_mismatch_is_rejected`、`call_arity_mismatch_is_rejected`、`extern_without_body_is_allowed`、`library_without_main_is_allowed`、`invalid_source_is_rejected`、`enum_variant_out_of_range_is_rejected`、`return_type_mismatch_is_rejected`、`empty_return_in_non_unit_is_rejected`、`print_non_printable_is_rejected`）；`m18_sret.rs::loop_sret_alloca_does_not_accumulate_stack`；LLVM：`verifier_reports_stage_and_raw_error` | 通过 |
| PKGSRC-01..06 | `packages.rs`：`h18_07_pkgsrc_01_path_before_remote_is_rejected`、`h18_07_pkgsrc_02_remote_before_path_is_rejected`、`h18_07_pkgsrc_03_same_repository_same_coordinate_is_reused`、`h18_07_pkgsrc_04_different_repository_or_version_is_rejected`、`h18_07_pkgsrc_05_locked_and_offline_do_not_bypass_source_check`、`h18_07_pkgsrc_06_same_canonical_path_aliases_are_reused` | 通过 |
| BUILD-01 | `manifest.rs::h18_08_build_01_path_dependency_with_bins_loads_only_library`、`packages.rs::h18_08_build_01_path_and_published_dependency_agree` | 通过 |
| BUILD-02 | `cli.rs::h18_08_build_02_manifest_optimization_is_the_default`、`h18_08_build_02_dependency_manifest_does_not_override_root_profile`、`h18_08_build_02_library_and_bins_share_effective_profile`；`manifest.rs::h18_08_build_03_explicit_profile_overrides_manifest_optimization` | 通过 |
| BUILD-03 | `cli.rs::h18_08_build_03_explicit_debug_on_release_manifest_reports_leak`、`h18_08_build_03_explicit_release_on_debug_manifest_is_clean`、`h18_08_build_03_debug_runtime_reports_leak_and_invalid_free` | 通过 |
| BUILD-04 | `cli.rs::h18_08_build_04_explicit_backend_beats_environment`；既有 `build.rs`/`ffi.rs`/`manifest.rs`/`cli.rs` 全回归 | 通过 |
| CI-01..05 | `ci.yml` 三 job 依赖表（H18-09 节）；`backend.rs::debug_backends_agree`、`release_backends_agree`（固定 exit 25、`7 10 12 25\n`、stderr 空）、`typed_ir_has_no_backend_types`、`llvm_debug_profile_emits_dwarf`；`manifest.rs::h18_09_method_diagnostics_use_defining_file`；Darwin shared-library fixture 按平台选项（`ffi06_shared_library_integration`） | 用户确认远端 CI 通过；本机全绿 |
| DOC-01..04 | `doc_examples.rs`：`current_doc_links_exist`、`doc_examples_are_classified`、`doc_examples_build_and_run`、`website_translations_are_in_sync`、`fixed_defects_moved_to_history`；`build.rs::h18_10_enum_construction_in_submodule` | 通过 |
| H18-11 | `build.rs::h18_11_examples_m18_combination_regression`；§5.3 示例与发行包 smoke | 通过 |

### 实际运行命令与测试数量

```bash
# 合同 5.1 默认门禁
cargo fmt --all -- --check                                            # 通过
cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings  # 通过
DOLPHIN_BACKEND=cranelift cargo test --workspace --exclude dolphin-codegen-llvm       # 323 passed
git diff --check                                                      # 通过

# 合同 5.2 LLVM 门禁
llvm-config --version                                                 # 22.1.8
cargo clippy --workspace --all-targets --features llvm -- -D warnings # 通过
DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm      # 330 passed
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm \
  --test build --test ffi --test cli --test manifest --test packages --test doc_examples
                                                                      # 153 passed
DOLPHIN_BACKEND=llvm cargo test --features llvm --test m18_harness --test m18_place \
  --test m18_layout --test m18_cast --test m18_bounds --test m18_impl --test m18_sret
                                                                      # 48 passed
cargo test -p dolphin-compiler --features llvm --test backend          # 4 passed

# 合同 5.3 集成、示例与发行包
cargo build --release --bins                                          # 通过
./target/release/dc fmt --check examples crates/dolphin-std/src       # 通过
./target/release/dc check examples/m14                                # exit 0
./target/release/dc run examples/m14                                  # exit 0，stderr 空
./target/release/dc run examples/m15                                  # `42 22 true`，stderr 空
./target/release/dc run examples/m18                                  # 固定四行，exit 0，stderr 空
# 发行包：按 ci.yml 冒烟脚本，在解压归档（自带 rust-lld/libLLVM）上构建/运行
#   m8(64)/m14/m15/m18、打包 m15math、坐标消费 smokeapp(42)、--locked --offline 重建
#   -> SMOKE_SEQUENCE_OK（/tmp/opencode/h18-11/，本机 patchelf 隔离 venv）
```

默认 322→323（新增 `h18_11_examples_m18_combination_regression`），LLVM 329→330，
合同显式列表 152→153；`examples/m18` 已在默认/LLVM 两套矩阵与发行包冒烟中执行。

### 未运行的检查及原因

- 本机只有 Linux；三平台默认 lane（macOS ARM64 / Windows x86_64）由用户确认的 GitHub Actions 覆盖，
  不在本机重跑。macOS/Windows LLVM 不在 CI 矩阵内，H18-09-M/-W 节结论不变。
- tag 上的 `release` job：H18-09 已修 Polly 配置，用户在发布 0.3.0 时做最终确认；本机不伪造。
- 无 CRT/SDK 的干净环境验收仍属 H21。
- `cargo test --release`（Rust profile）不改变 fixture 的 Dolphin profile，未运行。

### 行为/兼容变化

- 新增示例与 CI 冒烟步骤；无编译器行为、公开 API 或持久格式变化。
- 版本号按用户批准从 `0.2.0` 升到 `0.3.0`：`Cargo.toml`（workspace）、`Cargo.lock`、
  四个示例 lock 的 `compiler-version`；`.dlib`/lock 的 `compiler-version` 精确匹配策略不变，
  `--locked` 在 m9/m15/m15math/m16 上用新编译器复验通过。示例各自的 package 版本未动（m15/m16 仍
  为 `0.2.0`，`examples/m18` 为 `0.3.0`）。
- 阶段状态更新：roadmap M18 勾选完成、README 里程碑行、implemented-features 头部与 H18-11 合同状态。

### 剩余问题和下一批输入（M19 / H19-00）

1. M18 已按合同关闭；下一阶段按 [M18-M21 计划](../plan-m18-plus.md)先执行 `H19-00` 设计冻结：
   `docs/proposal-m19-cli-stdlib.md` 必须冻结参数/环境、字节 I/O、文件、资源状态与 defer、错误类型、
   本地库构建、用户测试和错误处理语法，并给出唯一方案、拒绝方案与验收例。
2. `build --lib` 同时打包 `.dlib`、归档禁止 path 依赖的开发体验问题按合同留给 H19-00 决策；
   M18 未擅自删除打包行为或放宽 path 依赖限制。
3. 仍未承诺的边界（供 H19-00/H21 引用）：内存安全无借用/悬垂检查，trap/`_Exit` 不执行 defer；
   调试仅 LLVM Debug 的 Unix DWARF；无一般定长数组/嵌套数组；无增量编译与稳定二进制 ABI；
   `.dlib` 仅源码归档且要求编译器版本完全一致；干净环境 SDK/sysroot 策略待 H21。
4. 未发现被无理由推迟的 P0/P1 已知正确性问题：`implemented-features.md` §1.1 当前无未修复审计项，
   §1.2 记录 H18-01..10 修复并链接回归测试。
