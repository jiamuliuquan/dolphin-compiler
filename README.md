# Dolphin Compiler

Dolphin 是一个用于学习和实践编译器实现的静态类型编程语言，语法参考 Rust、Kotlin、Java 和 C。编译器使用 Rust 编写，通过 Cranelift 生成目标代码，并链接为当前操作系统可直接运行的本机可执行文件。

项目已经完成 M0-M8 MVP，支持多文件项目、完整基础标量、函数、控制流、字符串、定长数组、模块系统以及 `dc check/build/run` 命令。

## 当前能力

| 类别 | 已实现功能 |
| --- | --- |
| 类型 | `i8/i16/i32/i64`、`u8/u16/u32/u64`、`f32/f64`、`char`、`bool`、`string`、`Unit` |
| 变量 | `var`、`val`、显式类型、局部类型推断、普通与复合赋值 |
| 表达式 | 算术、比较、相等、短路逻辑、一元运算、`as` 转换、数值后缀 |
| 控制流 | `if/else`、`loop`、`while`、`for`、范围、`break`、`continue`、`return` |
| 函数 | 参数、返回类型、前向调用、递归、数组参数和返回值 |
| 集合 | 一维定长数组、字面量、重复初始化、安全下标和按值语义 |
| 字符串 | UTF-8 字符串、内容相等、`length` 字节长度、格式化输出 |
| 模块 | 递归扫描 `src/`、`pkg`、模块/成员 `use`、`pub` 可见性 |
| 工具 | Clap CLI、`check/build/run/info/env`、Debug/Release、颜色、帮助和版本 |
| 后端 | 类型化 CFG IR、Cranelift、本机目标文件、内嵌最小 C 运行时和 `rust-lld` 链接器 |

尚未实现的主要能力包括嵌套数组、切片、通配符导入、外部依赖、多错误恢复和 DWARF 源码调试信息。详细边界见[已实现功能参考](docs/implemented-features.md)和[路线图](docs/roadmap.md)。

## 快速开始

环境要求（按目标平台）：

| 平台 | 最低支持版本 | 构建编译器时的依赖 |
| --- | --- | --- |
| macOS | ARM64（Apple Silicon） | 系统 `cc`（Xcode Command Line Tools） |
| Linux | x86_64 | 支持 C11 的系统 `cc`（GCC 或 Clang） |
| Windows | x86_64 | MSVC 的 `cl`（Visual Studio 2022 生成工具，或 Build Tools） |

构建 Dolphin **程序**时需要 Rust 和 Cargo（编译器自身是 Rust 程序，且需要 Rust 工具链自带的 `rust-lld` 链接器）。构建**编译器自身**时才需要上述 C 编译器，用于把运行时源码预编译为目标文件并内嵌到编译器二进制中；最终用户使用发行包构建 Dolphin 程序时不再需要安装 C/C++ 工具链。生成的程序在 Linux 上仍动态链接系统 glibc、在 macOS 上动态链接 libSystem、在 Windows 上动态链接 UCRT，这些是操作系统自带组件，不属于 C/C++ 开发工具链。运行时错误（溢出、除零、数组越界）由平台原生机制捕获并稳定终止，不会产生未定义行为。

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

## CLI

```text
dc check <项目目录或main.do> [--color auto|always|never]
dc build <项目目录或main.do> [--bin <名称>] [-o <输出文件>] [--debug|--release] [--system-linker]
dc run   <项目目录或main.do> [--bin <名称>] [-o <输出文件>] [--debug|--release] [--system-linker]
dc info  <项目目录>
dc env
```

使用 `dc <子命令> --help` 查看子命令参数。`--debug` 与 `--release` 互斥，默认使用 Debug 配置。`--color` 是全局选项，可放在子命令前后。`-o`（`--output`）与 `--bin` 仅对 `build`/`run` 生效；`--bin` 需要 `dolphin.toml` 清单，`-o` 仅在单文件模式下生效。`dc info` 只接受项目目录（不接受 `.do` 文件），`dc env` 显示宿主/目标平台、ABI 与所选链接器。

当目录中存在 `dolphin.toml` 时，`check`/`build`/`run` 会从给定目录（或当前目录）向上查找清单并按清单驱动构建。清单声明包坐标与一个或多个 `[[bin]]` 可执行目标；多目标项目运行需用 `--bin` 选择目标，`dc info` 显示完整坐标。

构建项目时会在项目的 `target/` 中生成：

```text
target/<项目名>             本机可执行文件
target/<项目名>.o           Dolphin 程序目标文件
target/<项目名>.runtime.o   内嵌最小运行时目标文件（落盘）
```

默认使用 Rust 工具链自带的 `rust-lld` 链接器；`--system-linker` 可回退到系统链接器（`cc`/`link`）以便诊断。

也可以直接编译不含 `pkg` 和 `use` 的单个 `.do` 文件：

```bash
./target/release/dc build examples/m5/src/main.do -o target/m5-program
./target/m5-program
```

## Dolphin 项目

每个项目使用 `src/` 作为源码根目录。编译器会递归读取 `.do` 文件；`src` 直接子文件组成根模块并省略 `pkg`，子目录文件必须声明与路径一致的包名：

```text
project/
└── src/
    ├── main.do
    ├── helper.do
    └── std/
        └── math.do  # pkg std.math;
```

模块导入示例：

```dc
// src/main.do
use std.math;
use std.math.min;

fn main() {
    println("min = {}", min(8, 3));
    return math.min(12, 10);
}
```

```dc
// src/std/math.do
pkg std.math;

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
  -> Cranelift IR
  -> 本机目标文件
  -> 内嵌运行时与 rust-lld 链接器
  -> 本机可执行文件
```

## 仓库结构

```text
.
├── Cargo.toml
├── build.rs               构建脚本：预编译运行时、探测链接参数
├── README.md
├── src/                   编译器实现和 dc CLI
│   ├── manifest.rs        dolphin.toml 清单解析与校验
│   └── bin/dc.rs          dc 可执行入口（复用 main.rs）
├── runtime/               运行时 C 源码（build.rs 预编译并内嵌）
├── scripts/
│   └── package.py         发行包打包脚本
├── tests/                 端到端构建及 CLI 测试
├── docs/
│   ├── language-design.md
│   ├── compiler-implementation.md
│   ├── implemented-features.md
│   ├── installation.md    安装、升级、卸载、兼容政策与许可证
│   └── roadmap.md
└── examples/
    ├── README.md
    ├── m1/ ... m6/
    ├── m7/               多文件模块示例
    ├── m8/               完整 MVP 示例
    └── m9/               dolphin.toml 清单与多可执行目标示例
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
| M13-M17 | 目标阶段 | 数据类型、内存模型、泛型标准库、可选 LLVM 后端和开发工具 |

## 文档和示例

- [语言设计说明](docs/language-design.md)：语法、类型、模块和运行时规则。
- [编译器实现指南](docs/compiler-implementation.md)：架构、核心数据结构和测试策略。
- [已实现功能参考](docs/implemented-features.md)：当前编译器的准确行为与限制。
- [安装与发行](docs/installation.md)：发行包获取、安装、升级、卸载、兼容政策与许可证。
- [实现路线图](docs/roadmap.md)：MVP 完成状态、M9-M12 可执行计划和 M13-M17 目标。
- [M1-M8 可运行示例](examples/README.md)：每个里程碑的源码、命令和预期结果。

## 开发验证

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release --bins
```

CI 在 Linux x86_64、macOS ARM64 和 Windows x86_64 上执行格式检查、Clippy、测试、Release 构建，并打包自包含发行包（含 `rust-lld`）后做冒烟测试（解压发行包、从空目录构建并运行示例）。
