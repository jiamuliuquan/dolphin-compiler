# Dolphin Compiler

Dolphin 是一个用于学习和实践编译器实现的静态类型编程语言，语法参考 Rust、Kotlin、Java 和 C。编译器使用 Rust 编写，通过默认 Cranelift 或可选 LLVM 后端生成目标代码，并链接为当前操作系统可直接运行的本机可执行文件。

项目已经完成 M0-M17，支持多文件项目、完整基础标量、函数、控制流、字符串、定长数组、模块系统、结构体/枚举与 `match`、手动内存管理与 C 互操作、用户泛型与方法/trait、源码标准库容器、基于 `.dlib` 的库包发布/消费、可选 LLVM 和最小开发工具。

下一阶段按“正确性收敛、真实 CLI、项目级工具、规模与交付”推进。逐批执行见 [M18-M21 交接指南](docs/plan-m18-plus.md)和 [M18 详细合同](docs/plan-m18-correctness.md)。这些任务尚未实施；现有语义缺口见[当前已知问题](docs/implemented-features.md#11-当前已知问题与验证边界)。

## 当前能力

| 类别 | 已实现功能 |
| --- | --- |
| 类型 | `i8/i16/i32/i64`、`u8/u16/u32/u64`、`f32/f64`、`char`、`bool`、`string`、`Unit`、裸指针/切片、`usize/isize` |
| 自定义类型 | 结构体、枚举、携带数据的枚举项、字段 `pub` 可见性、跨模块/跨包类型引用 |
| 泛型 | 泛型函数/结构体/枚举、显式与推断类型实参、泛型 `impl`、trait 与关联类型、静态分派、单态化 |
| 变量 | `var`、`val`、显式类型、局部类型推断、普通与复合赋值 |
| 表达式 | 算术、比较、相等、短路逻辑、一元运算、`as` 转换、数值后缀 |
| 控制流 | `if/else`、`loop`、`while`、`for`、范围、`break`、`continue`、`return`、`match` |
| 函数与方法 | 参数、返回类型、前向调用、递归、数组/聚合参数和返回值、方法接收者自动取址 |
| 内存 | `defer`、`std.mem` 类型化分配/释放、按值布局环检测、Debug 泄漏检测、`extern "C"` 与 C 原生链接 |
| 集合与标准库 | 源码标准库 `std.collections.Vec<T>`、`std.text.String`、`std.ffi.CString`、`Option`/`Result`/`Iterator`、切片/范围 `for` 协议 |
| 字符串 | UTF-8 字符串、内容相等、`length` 字节长度、`bytes`/`from_bytes`、格式化输出 |
| 模块 | 递归扫描 `src/`、`pkg`、模块/成员 `use`、`pub` 可见性 |
| 包管理 | `dolphin.toml` 的 `[lib]`/`[[bin]]`、本地 path 依赖、Maven 风格坐标与仓库、确定性 `.dlib`、内容寻址缓存、`dolphin.lock`、`--locked`/`--offline`、条件 PUT 发布 |
| 工具 | Clap CLI、`check/build/run/package/fetch/publish/info/env/fmt/lsp`、Debug/Release、颜色、帮助和版本 |
| 开发工具 | `dc fmt` 保守空白格式化、`dc lsp` 单文件最小诊断/符号/悬停/跳转、LLVM Debug 下的 Unix DWARF 行表与函数调试信息 |
| 后端 | 类型化 CFG IR、后端无关 `CodegenBackend` 接口、Cranelift（默认）与可选 LLVM 后端、本机目标文件、内嵌最小 C 运行时和 `rust-lld` 链接器 |

尚未实现的主要能力包括嵌套数组、通配符导入、版本范围求解、闭源二进制 Dolphin 包、动态多态、`?` 错误传播、多错误恢复，以及 Cranelift 后端的调试信息和 Windows PDB 调试信息。详细边界见[已实现功能参考](docs/implemented-features.md)和[路线图](docs/roadmap.md)。

## 快速开始

环境要求（按目标平台）：

| 平台 | 支持架构 | 构建编译器时的依赖 |
| --- | --- | --- |
| macOS | ARM64（Apple Silicon） | 系统 `cc`（Xcode Command Line Tools） |
| Linux | x86_64 | 支持 C11 的系统 `cc`（GCC 或 Clang） |
| Windows | x86_64 | MSVC 的 `cl`（Visual Studio 2022 生成工具，或 Build Tools） |

从源码构建**编译器自身**需要 Rust/Cargo 和上述 C/C++ 工具链，用于预编译并内嵌 Dolphin 运行时。官方发行包携带 `dc`、`rust-lld` 及其所需附件，使用发行包不需要 Rust/Cargo，但链接程序仍需要平台 CRT/SDK 等文件；当前尚未完成任意无开发工具链机器的可用性验收，具体见[系统依赖边界](docs/installation.md#6-系统依赖边界)。生成程序仍依赖 glibc、libSystem 或 UCRT。运行时对明确支持的溢出、除零、越界等错误检查并终止，不保证检测悬垂指针、未初始化读取等所有内存错误。

构建编译器并查看帮助：

```bash
cargo build --release --bins
./target/release/dc --help
./target/release/dc --version
```

`dc` 是正式短命令，`dolphin-compiler` 是兼容名称。Windows 下可执行文件为 `target\release\dc.exe`。

检查、构建和运行 M8 示例：

```bash
./target/release/dc check examples/m8
./target/release/dc build examples/m8 --release
./target/release/dc run examples/m8 --release
```

`run` 会返回 Dolphin 程序的退出码，因此 M8 示例正常结束时命令退出码为 `64`。

### 可选 LLVM 后端（M16）

默认后端是 Cranelift（编译快）。LLVM 后端在构建 `dc` 时用 `--features llvm` 启用，需要本机安装 LLVM 开发库（`inkwell` 通过 `llvm-config` 定位，当前对接 LLVM 22）：

```bash
cargo build --release --features llvm --bins

# 用 LLVM 后端构建同一份程序（也可用 --backend llvm）
DOLPHIN_BACKEND=llvm ./target/release/dc build examples/m16 --release
./examples/m16/target/m16

# 同一套前端测试分别跑两个后端
cargo test --features llvm
DOLPHIN_BACKEND=llvm cargo test --features llvm

# 对比编译时间、运行时间与产物体积
python3 scripts/bench.py
```

未启用 `llvm` feature 的构建在请求 LLVM 后端时返回明确诊断，不会静默回退到 Cranelift。官方发行包使用默认特性构建，只包含 Cranelift；LLVM 后端适合从源码构建（Release 性能场景），运行时需系统提供对应的 `libLLVM`。

### 开发工具（M17）

```bash
# 格式化：默认原地写回，--check 只检查并以非零退出
./target/release/dc fmt examples/m8/src
./target/release/dc fmt --check examples

# 语言服务器：stdio 上的 LSP（诊断/文档符号/悬停/跳转定义）
./target/release/dc lsp
```

调试信息由 LLVM 后端在 Debug 配置下产 DWARF（行表 + 子程序），可被 gdb/lldb 加载：

```bash
cargo build --release --features llvm --bins
DOLPHIN_BACKEND=llvm ./target/release/dc build examples/m8 --debug
gdb -batch -ex 'info line main' ./examples/m8/target/m8
```

Cranelift 后端暂不生成调试信息，Windows/PDB 也未覆盖。当前 LSP 对含 `pkg`/`use` 或没有 `main` 的文件跳过语义检查，悬停/定义主要来自当前文档的顶层声明；项目级语义分析与跨文件导航安排在 M20。格式化器目前主要整理缩进和空白，不是完整 AST 排版器。

## CLI

```text
dc check <项目目录或main.do> [--locked] [--offline] [--color auto|always|never]
dc build <项目目录或main.do> [--bin <名称>] [--lib] [-o <输出文件>] [--debug|--release] [--system-linker] [--backend cranelift|llvm] [--locked] [--offline]
dc run   <项目目录或main.do> [--bin <名称>] [-o <输出文件>] [--debug|--release] [--system-linker] [--backend cranelift|llvm] [--locked] [--offline]
dc package <项目目录> [--locked] [--offline]
dc fetch   <项目目录> [--locked] [--offline]
dc publish <项目目录> [--repository <id>] [--locked] [--offline]
dc info  <项目目录>
dc env
dc fmt   <文件或目录...> [--check]
dc lsp
```

使用 `dc <子命令> --help` 查看子命令参数。`--debug` 与 `--release` 互斥，默认使用 Debug 配置。`--color` 是全局选项，可放在子命令前后。`-o`（`--output`）与 `--bin` 仅对 `build`/`run` 生效；`--bin` 需要 `dolphin.toml` 清单，`-o` 仅在单文件模式下生效。`dc info` 只接受项目目录（不接受 `.do` 文件），`dc env` 显示宿主/目标平台、ABI、所选链接器与缓存根。

当目录中存在 `dolphin.toml` 时，`check`/`build`/`run` 会从给定目录（或当前目录）向上查找清单并按清单驱动构建，先解析 `[dependencies]`（本地 path 与 Maven 风格坐标）再编译。清单可声明 `[lib]` 库目标与一个或多个 `[[bin]]` 可执行目标；`dc build --lib`（或 `dc package`）只编译库并产出 `target/package/<name>-<version>.dlib` 与 `.dlib.sha256`，`dc build` 会同时构建库与全部 bin。多目标项目运行需用 `--bin` 选择目标，纯库项目 `run` 会明确拒绝，`dc info` 显示完整坐标、目标、依赖与锁状态。

仓库通过根清单的 `[repositories]` 配置，依赖坐标形如 `org.example:mathlib:1.0.0`。解析结果写入根目录 `dolphin.lock`（提交到版本管理）；`--locked` 要求锁与清单一致且不重写，`--offline` 不访问 HTTP(S)。`dc fetch` 只解析/下载整个闭包并写锁，`dc publish` 用 `If-None-Match: *` 条件 PUT 上传当前库（`file://` 仓库为不覆盖的原子发布），token 由 `DOLPHIN_REPOSITORY_<ID>_TOKEN` 提供。

构建项目时会在项目的 `target/` 中生成：

```text
target/<项目名>                       本机可执行文件
target/<项目名>.o                     Dolphin 程序目标文件
target/<项目名>.runtime.o             内嵌最小运行时目标文件（落盘）
target/lib/<库名>.o                   库验证目标文件（build --lib / package）
target/package/<库名>-<版本>.dlib     确定性源码型库包
target/package/<库名>-<版本>.dlib.sha256  包摘要（64 位小写十六进制加换行）
```

默认使用 Rust 工具链自带的 `rust-lld` 链接器；`--system-linker` 可回退到系统链接器（`cc`/`link`）以便诊断。

也可以直接编译不含 `pkg` 和 `use` 的单个 `.do` 文件：

```bash
./target/release/dc build examples/m5/src/main.do -o target/m5-program
./target/m5-program
```

## Dolphin 项目

每个项目使用 `src/` 作为源码根目录。编译器会递归读取 `.do` 文件；`src` 直接子文件组成根模块并省略 `pkg`，子目录文件必须声明所在目录作为包名（文件名不参与 `pkg`）：

```text
project/
└── src/
    ├── main.do
    ├── helper.do
    └── mathutil/
        └── math.do  # pkg mathutil;（模块 mathutil.math）
```

模块导入示例：

```dc
// src/main.do
use mathutil.math;
use mathutil.math.min;

fn main() {
    println("min = {}", min(8, 3));
    return math.min(12, 10);
}
```

```dc
// src/mathutil/math.do
pkg mathutil;

pub fn min(a: i32, b: i32): i32 {
    if a < b {
        return a;
    }
    return b;
}
```

## 语法预览

```dc
fn sum(values: [i32; 4]): i32 {
    var total = 0;
    for value in values {
        total += value;
    }
    return total;
}

fn main() {
    val name = "Dolphin";
    var values = [1, 2, 3, 4];
    values[1] = 10;

    val precise = 2.25_f64;
    val symbol: char = '海';
    println("{}, total = {}, value = {}, symbol = {}", name, sum(values), precise, symbol);
}
```

## 编译流程

```text
UTF-8 源文件
  -> Token
  -> AST
  -> 模块与名称解析
  -> 类型和控制流检查
  -> 类型化 CFG IR
  -> CodegenBackend（默认 Cranelift 或可选 LLVM）
  -> 本机目标文件
  -> 内嵌运行时与 rust-lld 链接器
  -> 本机可执行文件
```

## 仓库结构

编译器实现按依赖层次拆分为多个 workspace crate，`dolphin-compiler` 根包只保留薄 CLI 与兼容门面：

```text
.
├── Cargo.toml              workspace 定义与共享依赖
├── README.md
├── src/                    根包：CLI 与兼容门面
│   ├── main.rs             dc CLI 实现
│   ├── lib.rs              `dolphin_compiler::*` 兼容 re-export
│   └── bin/dc.rs           dc 可执行入口（复用 main.rs）
├── crates/
│   ├── dolphin-source      Span/SourceMap、词法、诊断
│   ├── dolphin-syntax      AST 与解析器
│   ├── dolphin-ir          类型化 CFG IR 与聚合布局
│   ├── dolphin-package     dolphin.toml、包图、仓库、缓存、锁文件
│   ├── dolphin-platform    目标平台、ABI、内嵌运行时与链接参数
│   ├── dolphin-hir         模块加载、名称解析、单态化、lowering
│   ├── dolphin-backend     后端无关的 `CodegenBackend` trait（M16 边界）
│   ├── dolphin-codegen-cranelift  Cranelift 后端
│   ├── dolphin-codegen-llvm       LLVM 后端（M16 接入点）
│   ├── dolphin-linker      目标文件与运行时链接
│   ├── dolphin-std         随编译器分发的源码标准库（数据 crate）
│   ├── dolphin-format      源码格式化器（`dc fmt`）
│   ├── dolphin-lsp         语言服务器（`dc lsp`）
│   └── dolphin-driver      build/check 编排与后端选择
├── runtime/               运行时 C 源码（dolphin-platform 的 build.rs 预编译并内嵌）
├── scripts/
│   └── package.py         发行包打包脚本
├── tests/                 端到端构建及 CLI 测试
├── docs/
│   ├── language-design.md
│   ├── compiler-implementation.md
│   ├── implemented-features.md
│   ├── installation.md    安装、升级、卸载、兼容政策与许可证
│   ├── roadmap.md
│   ├── plan-m18-plus.md    M18-M21 后续批次与人工交接提示词
│   └── plan-m18-correctness.md  M18 复现、修复合同与验收矩阵
└── examples/
    ├── README.md
    ├── m1/ ... m6/
    ├── m7/               多文件模块示例
    ├── m8/               完整 MVP 示例
    ├── m9/               dolphin.toml 清单与多可执行目标示例
    ├── m13/              结构体、枚举、match 与跨模块类型示例
    ├── m14/              手动内存、指针/切片、defer 与 C 互操作示例
    ├── m15/              泛型/标准库示例，含 path 依赖库 `mathlib/`
    └── m16/              后端基准示例（Cranelift 与 LLVM 对比）
```

## 实现进度

| 里程碑 | 状态 | 内容 |
| --- | --- | --- |
| M0-M1 | 已完成 | 本机可执行文件、`i32` 算术和返回值 |
| M2 | 已完成 | `var`、`val`、类型推断和赋值 |
| M3 | 已完成 | 布尔、比较、分支和循环 |
| M4 | 已完成 | 函数、参数、返回类型和递归 |
| M5 | 已完成 | UTF-8 字符串、格式化输出和最小运行时 |
| M6 | 已完成 | 定长数组、下标、范围和 `for` |
| M7 | 已完成 | 多文件、模块、导入和可见性 |
| M8 | 已完成 | 完整基础标量、转换、诊断和命令体验 |
| M9 | 已完成 | `dolphin.toml` 包坐标、多可执行目标、清单查找和 `dc info` |
| M10 | 已完成 | `TargetPlatform` 平台抽象、`dc env`、链接命令可测试且不硬编码 `cc` |
| M11 | 已完成 | Windows x86_64 原生支持：MSVC ABI、`.obj`/`.exe`、Windows 运行时与 CI |
| M12 | 已完成 | 自包含工具链：内嵌运行时、`rust-lld` 链接、`--system-linker` 回退、发行包与冒烟测试 |
| M13 | 已完成 | 用户自定义类型：结构体、枚举、`match` 模式匹配和跨模块类型引用 |
| M14 | 已完成 | 手动内存管理：`std.mem`、指针/切片、`defer`、字符串字节视图、`extern "C"` |
| M15 | 已完成 | 泛型、方法与 trait；源码标准库 `Vec`/`String`/`CString`、`Option`/`Result`/`Iterator`；lib 目标、path/坐标依赖、确定性 `.dlib`、仓库/缓存/锁与条件发布 |
| M16 | 已完成 | 后端无关类型化 IR、`CodegenBackend` 接口、可选 LLVM 后端、双后端稳定性测试与基准 |
| M17 | 已完成 | DWARF 调试信息（LLVM Debug）、`dc fmt` 格式化器、`dc lsp` 语言服务器 |
| M18 | 已完成 | 组合语义正确性、泛型约束、IR 校验、项目/包回归、LLVM CI 与发布门禁（证据见 [M18 进度报告](docs/reports/m18-progress.md)） |
| M19 | 待设计冻结 | 真实 CLI、参数与基础 I/O、文本处理、用户 `dc test` |
| M20 | 待设计冻结 | 结构化诊断、共享项目分析、文件 overlay、项目级 LSP 与工具验收 |
| M21 | 条件规划 | 性能测量、按证据优化、包兼容身份与干净环境交付 |

## 文档和示例

- [语言设计说明](docs/language-design.md)：语法、类型、模块和运行时规则。
- [编译器实现指南](docs/compiler-implementation.md)：历史架构建议、阶段拆分和测试策略，不是当前待办。
- [已实现功能参考](docs/implemented-features.md)：当前编译器的准确行为与限制。
- [安装与发行](docs/installation.md)：发行包获取、安装、升级、卸载、兼容政策与许可证。
- [实现路线图](docs/roadmap.md)：M0-M17 完成记录与 M18-M21 后续方向。
- [M18-M21 人工交接指南](docs/plan-m18-plus.md)：逐批任务、前置决策、验收要求和可复制执行提示词。
- [M18 正确性执行合同](docs/plan-m18-correctness.md)：12 个批次、已复现缺口、固定语义、源码入口和测试矩阵。
- [M14 实现规格](docs/proposal-m14-memory-model.md)：从 M13 新增手动内存、指针/视图、defer、C ABI 及验收矩阵。
- [M15 实现规格](docs/proposal-m15-generics-stdlib.md)：在新 M14 基础上实现泛型、容器、lib/`.dlib`、仓库和锁文件。
- [M14/M15 分步实施指南](docs/plan-m14-m15-rework.md)：已归档的 R00-R21 批次，不能据此从 M13 重做当前项目。
- [M1-M8 可运行示例](examples/README.md)：每个里程碑的源码、命令和预期结果。

## 开发验证

```bash
cargo fmt --all -- --check
cargo clippy --workspace --exclude dolphin-codegen-llvm --all-targets -- -D warnings
cargo test --workspace --exclude dolphin-codegen-llvm
cargo build --release --bins

# 可选 LLVM 后端（需要本机 LLVM 开发库）：额外覆盖 `dolphin-codegen-llvm`
cargo clippy --workspace --features llvm --all-targets -- -D warnings
DOLPHIN_BACKEND=cranelift cargo test --workspace --features llvm
DOLPHIN_BACKEND=llvm cargo test -p dolphin-compiler --features llvm --test build --test ffi --test cli --test manifest --test packages --test doc_examples --test m19_args
cargo test -p dolphin-compiler --features llvm --test backend
```

分支/PR CI 在 Linux x86_64、macOS ARM64 和 Windows x86_64 上执行格式检查、Clippy、测试、格式化器规范和 Release 构建，并解压发行包运行冒烟测试。当前默认 CI 不安装 LLVM，tag 发布任务也尚未依赖同提交的质量门禁；H18-09 负责补齐。现有冒烟运行在开发 runner 上，不是无 CRT/SDK 的干净机器证明。新测试文件须加入显式 `--test` 命令，feature 开启不等于所有 fixture 自动使用 LLVM。
