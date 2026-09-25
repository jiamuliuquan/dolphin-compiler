# 已实现功能参考

> 对应实现记录：M0-M20 已完成；批次与平台证据见 [M18](reports/m18-progress.md)、
> [M19](reports/m19-progress.md)、[M20](reports/m20-progress.md) 进度报告
>
> 编译器目录：[`仓库根目录`](../)  
> 可运行示例：[`examples`](../examples/)

本文记录当前能力、明确限制与已知缺陷；完成标记不等于所有组合都无缺陷。后续任务见 [M18-M21 交接指南](plan-m18-plus.md)（M21 待实施），历史合同与旧实施指南仅作证据保留。

## 1. 功能状态

| 里程碑 | 状态 | 能力 |
| --- | --- | --- |
| M0 | 已完成 | 空 `main`、本机目标文件和可执行文件 |
| M1 | 已完成 | `i32` 常量算术和返回值 |
| M2 | 已完成 | 局部变量、`var`、`val` 和赋值 |
| M3 | 已完成 | `bool`、比较、分支和循环 |
| M4 | 已完成 | 多函数、参数、返回类型、调用和递归 |
| M5 | 已完成 | UTF-8 字符串、格式化输出和最小运行时 |
| M6 | 已完成 | 定长数组、安全下标、范围和 `for` |
| M7 | 已完成 | 多文件项目、模块、导入和可见性 |
| M8 | 已完成 | 完整基础标量、转换、字符串比较、CLI 和运行时错误 |
| M9 | 已完成 | `dolphin.toml` 项目清单、包坐标、多可执行目标和清单查找 |
| M10 | 已完成 | `TargetPlatform` 平台抽象、`dc env`、链接命令可测试且不硬编码 `cc` |
| M11 | 已完成 | Windows x86_64 原生支持：MSVC ABI、`.obj`/`.exe`、Windows 运行时与 CI |
| M12 | 已完成 | 内嵌运行时与发行包；当时随包分发 `rust-lld`，后续调整为默认系统链接器、`--bundled-linker` 可选 |
| M13 | 已完成 | 用户自定义类型：结构体、枚举、`match` 模式匹配和跨模块类型引用 |
| M14 | 已完成 | 手动内存：`usize/isize`、统一布局、指针/切片、`std.mem` 分配与视图、`defer`、字符串字节视图、`extern "C"` 与原生链接 |
| M15 | A-F 已完成 | 用户泛型、trait/方法、源码标准库、lib/path/坐标依赖、确定性 `.dlib`、仓库/缓存/锁与发布 |
| M16 | 已完成 | 后端无关 IR、可选 LLVM、双后端对照与基准 |
| M17 | 最小版本已完成 | LLVM Unix DWARF、保守格式化器、单文件最小 LSP |
| M18 | 已完成 | 组合语义正确性、泛型约束、IR 校验、项目/包回归、LLVM CI 与发布门禁 |
| M19 | 已完成 | 真实 CLI 与用户测试：`std.process`/`std.error`/`std.io`/`std.fs`/`std.text`/`std.test`、`dc run --` 转发、`dc test` 发现/执行/汇总、`examples/m19` `dtext`（三平台默认后端 + Linux LLVM 验收） |
| M20 | 已完成 | 项目级开发工具：结构化诊断（`E0000`/`E0001`/`E0002`/`E1001`/`E1002`/`E2001`）与前端多错误收集、无副作用共享项目分析（`dolphin-analysis`）与未保存 overlay、项目级 LSP 诊断/悬停/跨文件跨包定义、`dc fmt` 清单发现与全有或全无、真实 gdb/lldb 调试验收（三平台默认后端 + Linux/macOS LLVM Debug；证据见 [M20 进度报告](reports/m20-progress.md)） |

### 1.1 当前已知问题与验证边界

M18 审计发现的入口问题已全部修复（见 1.2 节），当前没有遗留的审计缺陷；M19/M20 的三平台复验与平台缺陷修复见 [M19 进度报告](reports/m19-progress.md) 与 [M20 进度报告](reports/m20-progress.md)（Windows H20-W、macOS H20-M）。测试全绿只代表已覆盖路径，不代表所有组合都无缺陷。

### 1.2 已修复缺陷（M18）

下表保留 M18 审计缺陷的固定期望与回归测试引用。

| 缺陷 | 修复批次 | 回归测试 |
| --- | --- | --- |
| 聚合字段/元素赋值整值写回，覆盖 RHS 经别名对其他字段/元素的修改；数组越界检查晚于 RHS | H18-01 | `tests/m18_place.rs`（PLACE-01..08，两后端 × Debug/Release） |
| 枚举 size/align 与 payload 对齐不自洽：payload 从偏移 4 开始、size=4+n*8 不是 align 的整数倍；两后端各自保留 `4 + ...` 常量；嵌套聚合超预算时静默饱和 | H18-02 | `tests/m18_layout.rs`（LAYOUT-01..06）与 `dolphin-ir` 的 `layout::tests`（两后端 × Debug/Release） |
| LLVM 浮点到整数未兑现饱和政策（越界为 poison/不确定值）；Cranelift x64 对窄目标饱和转换内部 ICE | H18-03 | `tests/m18_cast.rs`（CAST-01..02，f32/f64 × 全部整数位宽 × 两后端 × Debug/Release） |
| struct/enum 类型参数的 trait bound 丢失：越界实例化被接受；`T::Item` 无法用于字段/payload | H18-04 | `tests/m18_bounds.rs`（GEN-01..05）与 `tests/manifest.rs::h18_04_type_bounds_across_path_dependency`（两后端 × Debug/Release） |
| impl 目标实参被解析后丢弃：特化、重排、重复、嵌套、漏参/多参、blanket impl、泛型 trait 实参、impl 参数 bound、方法独立泛型参数均被静默接受；`impl<U> Box<U>` 改名参数使声明字段无法解析 | H18-05 | `tests/m18_impl.rs`（IMPL-01..03）与 `tests/manifest.rs::h18_05_parameterized_impl_across_path_dependency`（两后端 × Debug/Release） |
| 依赖解析的来源冲突检查不对称：Path 先加载后遇到同坐标 Remote 时被静默复用（坐标分支直接返回已有节点），只有反序才拒绝；`--locked`/`--offline` 下该缺口会绕过来源检查并可能改写锁 | H18-07 | `tests/packages.rs` 的 `h18_07_pkgsrc_01_path_before_remote_is_rejected`、`h18_07_pkgsrc_02_remote_before_path_is_rejected`、`h18_07_pkgsrc_03_same_repository_same_coordinate_is_reused`、`h18_07_pkgsrc_04_different_repository_or_version_is_rejected`、`h18_07_pkgsrc_05_locked_and_offline_do_not_bypass_source_check`、`h18_07_pkgsrc_06_same_canonical_path_aliases_are_reused` |
| 项目构建不一致：path 依赖的 `[[bin]]` 入口被当作模块加载（lib+两个 bin 的依赖报重复 `main`，与消费发布包不一致）；根清单 `build.optimization = "release"` 覆盖显式 `--debug`；库与 bin 可能使用不同 profile | H18-08 | `tests/manifest.rs::h18_08_build_01_path_dependency_with_bins_loads_only_library`、`h18_08_build_03_explicit_profile_overrides_manifest_optimization`、`tests/packages.rs::h18_08_build_01_path_and_published_dependency_agree`、`tests/cli.rs` 的 `h18_08_build_02_manifest_optimization_is_the_default`、`h18_08_build_02_dependency_manifest_does_not_override_root_profile`、`h18_08_build_02_library_and_bins_share_effective_profile`、`h18_08_build_03_explicit_debug_on_release_manifest_reports_leak`、`h18_08_build_03_explicit_release_on_debug_manifest_is_clean`、`h18_08_build_03_debug_runtime_reports_leak_and_invalid_free`、`h18_08_build_04_explicit_backend_beats_environment` |
| trait/impl 方法的 `source_id` 未赋值（停留解析器默认 0），方法内诊断与 IR Location 被误归属到第一个源文件；stdlib 方法在多字节注释处会命中非字符边界触发编译器 panic | H18-09 | `tests/manifest.rs::h18_09_method_diagnostics_use_defining_file` |
| 子模块（`pkg`）内用裸名构造本模块枚举项（`Enum.Variant(...)` 与 `Enum.Variant`）被误报 `unknown function`：类型表以「模块.类型」为键，而构造目标判定只查未限定名 | H18-10 | `tests/build.rs::h18_10_enum_construction_in_submodule`；文档示例由 `tests/doc_examples.rs` 覆盖 |

最小源码、期望与验证环境见 [M18 合同](plan-m18-correctness.md)；CI 现状与验证边界见 [README 开发验证](../README.md#开发验证)。M17 的项目级 LSP、真实调试器流程和更完整格式化验收另见第 18 节及 M20 计划。

## 2. 构建和使用

### 2.1 构建编译器

```bash
cargo build --release
```

生成的编译器为：

```text
target/release/dc
target/release/dolphin-compiler
```

`dc` 是正式短命令，`dolphin-compiler` 保留兼容。

### 2.2 编译 Dolphin 项目

项目源码位于 `src/`，编译器会递归读取所有 `.do` 文件。入口函数 `main` 必须定义在 `src` 根目录的某个文件中：

```text
my-project/
└── src/
    ├── main.do
    └── mathutil/
        └── math.do
```

命令格式：

```text
dc check <项目目录或main.do> [--locked] [--offline] [--color auto|always|never]
dc build <项目目录或main.do> [--bin <名称>|--lib] [-o <输出文件>] [--debug|--release] [--system-linker|--bundled-linker] [--backend cranelift|llvm] [--locked] [--offline]
dc run   <项目目录或main.do> [--bin <名称>] [-o <输出文件>] [--debug|--release] [--system-linker|--bundled-linker] [--backend cranelift|llvm] [--locked] [--offline] [-- <应用参数>...]
dc test  <项目目录> [--filter <子串>] [--debug|--release] [--system-linker|--bundled-linker] [--backend cranelift|llvm] [--locked] [--offline]
dc package <项目目录> [--locked] [--offline]
dc fetch <项目目录> [--locked] [--offline]
dc publish <项目目录> [--repository <id>] [--locked] [--offline]
dc info  <项目目录>
dc env
dc fmt <文件或目录...> [--check]
dc lsp
```

CLI 使用 Clap 解析参数。`dc --help`、`dc --version` 以及 `dc <子命令> --help` 均可用；非法参数、缺失参数和冲突的 `--debug --release` 会输出标准帮助提示并返回退出码 `2`。`--color` 是全局选项，可放在子命令前后。`-o`（`--output`）与 `--bin` 仅对 `build`/`run` 生效；`--bin` 需要 `dolphin.toml` 清单，`-o` 仅在单文件模式下生效。`dc info` 只接受项目目录，`dc env` 显示宿主/目标平台、ABI 与所选链接器。

例如：

```bash
./target/release/dc build examples/m8 --release
./examples/m8/target/m8
```

M8 示例预期退出码为 64。也可以直接编译一个不使用模块声明和导入的 `.do` 文件；依赖管理通过项目清单提供，见第 17 节。`dc run ... -- <应用参数>` 自 H19-01 起原样转发参数（不经 shell，空参数/空格/Unicode/以 `-` 开头均保留，支持非法 UTF-8 字节）；`dc test`（H19-05）为库项目构建并运行用户测试：需要 `[lib]` 目标；不产出 `.dlib`、不要求可发布性，因此 path 依赖可解析；测试目标在 `target/test/<包名>-tests`。`dc build`/`run`/`package`/`check` 永不读取 `tests/`。

### 2.2a 项目清单 `dolphin.toml`

当项目根目录存在 `dolphin.toml` 时，`check`、`build`、`run` 会从给定目录（或当前目录）向上查找该清单，并以清单声明驱动构建：

```toml
[package]
group = "me.foxlab"
name = "greeter"
version = "0.1.0"
source = "src"

[lib]
path = "src/lib.do"

[[bin]]
name = "cli"
path = "src/main.do"

[[bin]]
name = "server"
path = "src/server.do"

[repositories]
default = "https://packages.example.org/dolphin"

[dependencies]
math = "org.example:mathlib:1.0.0"
codec = { coordinate = "org.example:codec:2.0.0", repository = "default" }
local = { path = "../local-lib" }

[build]
optimization = "debug"
output = "target"
```

- `[package]` 的 `group`、`name`、`version` 组成包坐标 `group:name:version`；`source` 是源码根目录（默认 `src`）。
- `[lib]` 声明库目标，`path` 必须是 `source` 的直接子文件；lib 与 `[[bin]]` 至少声明一种，可同时存在；一个包最多一个 lib。
- `[[bin]]` 声明一个或多个可执行目标，每个目标由 `name` 和入口文件 `path` 组成，共享 `source` 下的模块，各自拥有独立的 `main`。
- `[repositories]` 是仓库 ID 到基地址的映射（`http(s)://` 或 `file://`，不含查询/片段/凭据）；ID 仅字母数字下划线、首字母必须为字母、大小写不敏感。
- `[dependencies]` 左侧是当前包私有别名（不能为 `std`，不能与顶层模块重名），右侧是精确坐标字符串或 `{ coordinate, repository }` / `{ path }` 表；坐标与 path 不能同时给出。
- `[native.<target>]` 声明 `objects`/`static-libs`/`shared-libs`/`runtime-files`，路径相对清单目录解析，打包时复制进 `.dlib` 并记录摘要。
- `[build]` 的 `optimization` 取 `debug` 或 `release`，`output` 是产物目录（默认 `target`）。根清单的 `optimization` 作为 `build`/`run` 未传 `--debug`/`--release` 时的默认；profile 优先级为显式 CLI 选项 > 根清单 `optimization` > Debug，库与 bin 使用同一最终 profile，依赖包清单不参与。
- `dc info <项目>` 显示包坐标、源码目录、lib/bin 目标、直接依赖、仓库映射、优化级别与锁文件状态。
- 多目标项目运行需用 `--bin <name>` 选择目标；`build` 默认处理全部目标，可用 `--bin` 指定单个；`check` 检查全部目标，目前没有 `--bin` 选项。
- 依赖解析结果写入根目录 `dolphin.lock`，支持 `--locked` 与 `--offline`；`dc fetch` 解析/下载依赖并写锁，不生成本机代码；`dc package` 本地产出 `.dlib`，`dc publish` 才执行发布上传。

### 2.3 构建产物

编译 `examples/m5` 会生成：

```text
examples/m5/target/m5             本机可执行文件
examples/m5/target/m5.o           Dolphin 程序目标文件
examples/m5/target/m5.runtime.o   最小运行时目标文件
```

项目模式还会生成 `target/lib/<库名>.o` 与 `target/package/<库名>-<版本>.dlib`（`build --lib`/`package`），以及 `target/test/<包名>-tests[.exe]` 等测试产物（`dc test`）。当前只生成运行编译器所在平台的本机程序，不支持交叉编译。

## 3. 源文件和词法规则

源文件扩展名为 `.do`，按 UTF-8 读取。

### 3.1 注释

```dc
// 单行注释

/*
 * 多行注释
 */
```

多行注释暂不支持嵌套。未闭合的多行注释会产生带源码位置的编译错误。

### 3.2 标识符

标识符必须由 ASCII 字母或下划线开头，后续可以包含 ASCII 字母、数字和下划线：

```text
合法：value、user_name、value2、_internal
非法：2value、user-name
```

Unicode 标识符尚未实现。

### 3.3 字符串字面量

字符串字面量支持 UTF-8 内容和以下转义：

```text
\n  换行
\r  回车
\t  制表符
\\  反斜杠
\"  双引号
\u{1F600} Unicode 码点
```

字符串不能跨源码行书写。未闭合字符串、未知转义和无效 Unicode 码点会产生编译错误。

## 4. 当前类型

### 4.1 `i32`

`i32` 是有符号 32 位整数，也是没有后缀的整数字面量的默认类型。其他已实现数值类型见本节后续条目。

```dc
val inferred = 42;
var explicit: i32 = -10;
```

支持范围包括 `-2147483648` 到 `2147483647`。超出范围的字面量产生编译错误。

运行时整数加、减、乘和取负执行溢出检查；溢出、除以零、取模零和数组越界会输出统一运行时错误并以 `101` 终止。运行时错误目前不含 Dolphin 源码位置。

### 4.2 `bool`

```dc
val enabled: bool = true;
val disabled = false;
```

条件表达式必须是 `bool`。整数不能作为布尔条件使用。

### 4.3 `string`

`string` 是不可变 UTF-8 字符串：

```dc
val name: string = "海豚";
```

字符串可以：

- 保存到局部变量。
- 作为函数参数。
- 作为函数返回值。
- 通过 `print` 和 `println` 输出。

运行时使用“数据指针 + UTF-8 字节长度”表示字符串，因此字符串不依赖 NUL 结尾。字符串支持 `==`、`!=` 内容比较和 `length(value)` UTF-8 字节长度查询（M14 起统一返回 `usize`）。`value.bytes()` 返回只读字节视图 `[]const u8`，`string.from_bytes(bytes)` 校验 UTF-8 后返回 `string` 视图；非法 UTF-8 以 `104` 终止。没有内建字符串 `+` 和按字符下标；M15 源码标准库提供拥有型 `String` 与 concat 等 API，见第 16.1 节。

### 4.4 `Unit`

省略返回类型的普通函数内部视为返回 `Unit`：

```dc
fn do_nothing() {
    return;
}
```

`Unit` 不能保存到变量、参与运算或格式化输出。

### 4.5 其余整数类型

支持有符号 `i8`、`i16`、`i32`、`i64` 和无符号 `u8`、`u16`、`u32`、`u64`。无后缀整数默认是 `i32`：

```dc
val small = -8_i8;
val count: u64 = 100_u64;
val widened: i64 = small as i64;
```

后缀必须使用下划线。不同数值类型不会隐式混合。整数加减乘、取负和除零执行运行时检查；窄化转换保留低位，扩展转换按来源类型进行符号或零扩展。

### 4.6 浮点数

支持 IEEE `f32` 和 `f64`。无后缀浮点数默认是 `f64`：

```dc
val ratio = 0.5; // f64
val precise = 0.5_f64;
```

浮点数支持 `+`、`-`、`*`、`/`、比较和相等，不支持 `%`。浮点除零遵循 IEEE 规则。浮点到整数的 `as` 自 H18-03 起在两个后端都使用饱和语义：向零截断，越界与 ±Inf 饱和到目标类型极值，NaN 与 ±0 结果为 0；窄目标按最终位宽饱和。

### 4.7 `char`

`char` 表示一个 Unicode 标量值，支持比较、相等、数组、函数调用、格式化以及与整数之间的显式转换：

```dc
val symbol: char = '海';
println("{}", symbol);
```

## 5. 定长数组

### 5.1 类型与初始化

数组类型写作 `[元素类型; 长度]`。长度是类型的一部分，必须是大于零且能用 `i32` 表示的整数字面量：

```dc
val inferred = [1, 2, 3];
val explicit: [i32; 3] = [1, 2, 3];
val repeated: [bool; 4] = [false; 4];
```

- 当前只支持一维数组，元素可以是任意已实现的非 `Unit` 标量类型。
- 数组字面量必须非空，所有元素类型必须一致。
- `[value; N]` 中的 `value` 只求值一次，再复制为 `N` 个元素。
- `[i32; 2]` 与 `[i32; 3]` 是不同类型，不能互相赋值或传参。
- 嵌套定长数组与空数组字面量尚未实现；动态存储可使用 M14 切片及 M15 `Vec<T>`，不是新的定长数组语法。

### 5.2 下标访问与修改

```dc
var values = [10, 20, 30];
val first = values[0];
values[1] = 25;
values[2] += 5;
```

下标必须是 `i32`。编译器会拒绝确定越界的常量下标；动态下标在运行时检查，负数或大于等于数组长度都会触发陷阱并终止程序。当前运行时陷阱还没有友好的 Dolphin 源码位置。

通过 `val` 声明的数组不能修改元素。普通赋值支持全部数组元素类型，复合赋值支持数值元素，不限于 `i32`。

### 5.3 传参与返回

数组可以保存到局部变量、作为函数参数和返回值：

```dc
fn identity(values: [i32; 2]): [i32; 2] {
    return values;
}
```

当前数组采用按值语义，函数参数和返回值通过展开后的本机 ABI 传递。修改函数内的数组副本不会修改调用方数组。数组不能直接使用 `print`/`println` 格式化，也不能进行整体相等比较。

## 6. 变量和作用域

### 6.1 `var` 和 `val`

```dc
var count = 1;
count = 2;

val limit = 10;
limit = 20; // 编译错误
```

- `var` 可以重新赋值。
- `val` 不能重新赋值。
- 所有变量必须在声明时初始化。
- 类型可以显式标注，也可以从初始化表达式推断。
- 赋值两侧的类型必须一致。

### 6.2 赋值运算符

支持：

```text
=  +=  -=  *=  /=  %=
```

复合赋值用于数值变量、可写数值字段/元素，不限于 `i32`；操作数类型必须一致。

赋值顺序固定为：先求目标位置（含边界检查，且只求值一次），再求右值，最后只写目标子对象；复合赋值在右值前读取一次旧值。右值通过别名修改同一对象的其他字段/元素时这些修改保留，详见 [语言设计 8.1](language-design.md)。

### 6.3 块级作用域

`if` 和循环体会创建内层作用域：

```dc
if true {
    val scoped = 1;
}

return scoped; // 编译错误：未知变量
```

同一作用域不能重复声明同名变量。内层作用域可以遮蔽外层变量。函数参数视为不可变局部变量。

## 7. 表达式和运算符

### 7.1 算术

```text
+  -  *  /  %
```

支持一元负号和括号：

```dc
val result = -(1 + 2) * 3;
```

### 7.2 比较和相等

```text
<  <=  >  >=  ==  !=
```

- 大小比较支持整数、浮点数和 `char`，两侧类型必须匹配。
- 相等比较支持整数、浮点数、`char`、`bool`、string 内容及裸指针地址；不支持聚合值整体比较。
- 字符串大小比较尚未实现；内容相等使用 `==` 和 `!=`。

### 7.3 逻辑

```text
!  &&  ||
```

逻辑运算只接受 `bool`。`&&` 和 `||` 使用短路求值：

```dc
val safe = false && 1 / 0 == 0; // 不执行右侧除零表达式
```

### 7.4 优先级

从高到低：

1. 函数调用、下标和括号
2. 一元 `-`、`!`
3. `*`、`/`、`%`
4. `+`、`-`
5. `<`、`<=`、`>`、`>=`
6. `==`、`!=`
7. `&&`
8. `||`

赋值只作为语句存在，当前不是可以嵌套的表达式。

## 8. 控制流

### 8.1 `if/else`

```dc
if score >= 60 {
    return 1;
} else {
    return 0;
}
```

条件必须为 `bool`。当前 `if` 是语句，不产生值；尚不支持 `else if` 简写。

### 8.2 `while`

```dc
var i = 0;
while i < 5 {
    i += 1;
}
```

### 8.3 `loop`

```dc
loop {
    if done {
        break;
    }
}
```

### 8.4 `for`

数组遍历：

```dc
for value in [1, 2, 3] {
    println("{}", value);
}
```

整数范围遍历：

```dc
for i in 0..3 {  // 0、1、2
    println("{}", i);
}

for i in 0..=3 { // 0、1、2、3
    println("{}", i);
}
```

范围两端必须是 `i32`，并且在进入循环时各求值一次。范围只用于 `for`，当前不是可以保存或传递的普通值。循环变量不可赋值；被遍历数组也只求值一次。

### 8.5 `break` 和 `continue`

二者只能出现在循环内部。`break` 跳出当前最内层循环，`continue` 开始当前最内层循环的下一次迭代。带标签跳转尚未实现。

编译器会拒绝确定不可达的后续语句。

## 9. 函数

### 9.1 定义和调用

```dc
fn add(a: i32, b: i32): i32 {
    return a + b;
}

fn main() {
    return add(1, 2);
}
```

- 参数必须显式标注类型。
- 返回值函数必须显式标注返回类型。
- 省略返回类型表示 `Unit`。
- 调用时参数数量和类型必须完全匹配。
- 函数不需要先于调用位置定义。
- 支持直接递归和通过其他函数形成的调用。
- 支持用户泛型，见第 16 节；不支持函数重载、默认参数和可变参数。

### 9.2 返回路径

非 `Unit` 函数的所有可达路径都必须返回正确类型的值：

```dc
fn choose(flag: bool): i32 {
    if flag {
        return 1;
    }
    return 0;
}
```

缺少返回值、返回错误类型或从 `Unit` 函数返回值都会产生编译错误。

### 9.3 `main`

当前入口规则：

- 程序必须且只能定义一个 `main`。
- `main` 暂不接受参数。
- `main` 可以省略返回类型，内部按 `i32` 退出码处理。
- `main` 自然执行结束或执行 `return;` 时退出码为 `0`。
- `return expression;` 的表达式必须是 `i32`。

## 10. 输出和格式化

### 10.1 `print` 与 `println`

```dc
print("Hello, ");
println("{}!", "Dolphin");
```

- `print` 输出后不换行。
- `println` 输出后增加换行。
- `println()` 只输出换行。
- `print()` 不输出内容。
- 二者只能作为独立语句调用。

### 10.2 占位符

```dc
println("{} + {} = {}", 1, 2, 3);
```

每个 `{}` 消耗一个格式化参数。目前支持全部整数、浮点数、`char`、`bool` 和 `string`。数组整体不能格式化，但数组元素可以。占位符数量与参数数量不一致会产生编译错误。

第一个参数当前必须是字符串字面量，以下代码尚不支持：

```dc
val format = "{}";
println(format, 1);
```

字面量花括号使用双写转义：

```dc
println("{{}}"); // 输出 {}
```

未匹配的 `{` 或 `}` 会产生编译错误。

## 11. 项目与模块

### 11.1 源码路径和 `pkg`

`src` 根目录下的所有 `.do` 文件合并为根模块，并且必须省略 `pkg`：

```text
src/main.do
src/helper.do
```

子目录文件的模块路径由相对目录和文件名共同决定；文件只需声明所在目录作为 `pkg`：

```text
src/mathutil/math.do    ->  pkg mathutil;   (模块 mathutil.math)
src/net/http/client.do  ->  pkg net.http;   (模块 net.http.client)
```

模块文件名和目录名目前使用原始 UTF-8 路径段；语言标识符仍只允许 ASCII。`pkg` 与所在目录不一致或子目录文件缺少 `pkg` 会产生编译错误。

### 11.2 `use`

导入模块会以最后一个路径段作为模块名：

```dc
use mathutil.math;

fn main() {
    return math.min(1, 2);
}
```

也可以直接导入公开成员：

```dc
use mathutil.math.min;

fn main() {
    return min(1, 2);
}
```

还可以只导入包前缀（目录），再用完整模块路径访问成员：

```dc
use mathutil;

fn main() {
    return mathutil.math.min(1, 2);
}
```

局部模块函数优先于导入成员。重复导入名、导入名与本模块函数冲突、未知模块和未知成员都会产生编译错误。当前不支持 `use std.*`、显式别名或重导出；也允许直接使用完整公开路径调用。

### 11.3 `pub` 和模块依赖

顶层函数默认只在所属模块内可见，`pub fn` 可以跨模块调用：

```dc
pkg mathutil;

pub fn min(a: i32, b: i32): i32 {
    // ...
}
```

跨模块访问私有函数会在调用位置产生错误。M13 模块包含函数及用户类型定义，没有模块初始化执行顺序，因此允许循环导入；函数调用形成的递归继续使用原有递归规则。

M7 的 `src/` 子目录模块是项目内的普通源码目录，不是编译器自带依赖；`std` 命名空间自 M14 起保留给内建 `std.mem`，用户模块不得占用。`print` 和 `println` 仍是编译器内建函数，不能被用户函数覆盖。

## 12. 用户自定义类型（M13）

M13 引入结构体和枚举，M14 已补齐按值复制、参数/返回、字段写入和显式指针语义。以下按当前能力说明，不把 M13 初版限制当作当前限制。

### 12.1 结构体

结构体把同名字段聚合为一个复合值类型，采用位置构造与字段访问：

```dc
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    var p = Point(3, 4);
    println("point = ({}, {})", p.x, p.y);
}
```

- 字段在结构体内不得重名，访问不存在的字段是编译期错误。
- 结构体支持作为局部变量和 `var`/`val` 赋值；`var` 声明的结构体整体可重新赋值，字段也可单独写入；字段写入只更新该字段，右值经别名对其他字段的修改保留（H18-01）。
- 结构体字段支持基础类型、数组及其它结构体（嵌套）。

### 12.2 枚举

枚举描述一组有限的取值，枚举项可携带数据：

```dc
enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Empty,
}
```

- 同一枚举内的枚举项名不得重复。
- 枚举项携带零个或多个参数，参数类型在声明处固定。
- 枚举项通过 `Enum.Variant(...)` 构造；无参枚举项写作 `Enum.Variant`（无括号）。

### 12.3 match 模式匹配

`match` 是表达式式控制流，用于按枚举取值分派：

```dc
val area = match shape {
    Shape.Circle(r) => 3.14 * r * r,
    Shape.Rectangle(w, h) => w * h,
    Shape.Empty => 0.0,
};
```

- `match` 在表达式位置使用（如 `val x = match ...`、`return match ...`），各分支求值为同一类型；当前不支持把 `match` 作为独立语句，分支体也不能是 `println` 等语句。
- 对枚举穷尽匹配：缺失分支或冗余分支产生源码级诊断。
- 支持携带数据枚举项的解构绑定，以及通配符 `_` 分支（`_` 可作为解构绑定位忽略某个字段，也可作为整体通配分支）。
- 解构绑定 `_` 表示忽略该字段，不绑定到变量。

### 12.4 跨模块类型

结构体和枚举支持跨模块引用，规则与函数一致：

- 顶层类型默认仅在本模块内可见，`pub struct`/`pub enum` 可跨模块引用。
- 通过 `use module.path;` 导入模块后，可用 `module.TypeName` 引用其类型；也支持完整公开路径直接引用。
- 跨模块访问私有类型会产生编译期错误。

### 12.5 当前边界

- 结构体和枚举支持作为函数参数和返回值（M14-A 起按值复制完整有效值）；`var` 结构体字段可单独写入。
- 结构体/枚举支持作为局部变量、整体赋值、字段访问和 `match` 主题。
- M15 已支持 string、结构体等复合枚举 payload；枚举布局（tag/payload 偏移、size、align）自 H18-02 起统一由公共 layout 计算并已被回归覆盖。

## 13. 手动内存管理与 C 互操作（M14）

M14 采用 Zig 式显式内存模型：没有 GC、RC、自动析构、隐式 move 或借用检查；值默认复制，指针和切片只复制描述符。分配与释放显式可见，`defer` 是唯一的作用域清理语法。

### 13.1 类型、布局和可变性

| 形式 | 含义 |
| --- | --- |
| `*T` / `*const T` | 可为 null 的可写 / 只读原始指针 |
| `[]T` / `[]const T` | 可写 / 只读视图 `{ ptr, len }` |
| `string` | 合法 UTF-8 的只读字节视图，不拥有内存 |
| `usize` / `isize` | 目标指针宽度的无符号 / 有符号整数（当前目标为 64 位） |

- 结构体按字段对齐补齐；`mem.size_of<T>()` / `mem.align_of<T>()` 是编译期布局常量。
- `&var_local` 得到 `*T`，`&val_local` 和不可变参数取址得到 `*const T`；取址只作用于稳定存储，禁止对临时值取址，索引取址执行边界检查。
- `[]T` 可隐式转为 `[]const T`，`*T` 可隐式转为 `*const T`；反向转换是编译错误。
- `null` 只能用于已知指针类型的上下文；`.ptr`、`.len` 是切片只读字段，`string` 的 `.len` 是字节数。
- 结构体自引用必须经过指针；按值布局循环给出诊断。

### 13.2 `std.mem` API

`use std.mem;` 提供编译器内建入口（无需磁盘标准库）：

| API | 结果及责任 |
| --- | --- |
| `mem.alloc<T>(count): []T` | 分配未初始化连续存储；调用者写入后读取，交 `mem.free` |
| `mem.free<T>(buffer: []T)` | 只释放完整原始切片，不递归释放元素 |
| `mem.create<T>(value): *T` / `mem.destroy<T>(ptr)` | 分配/释放单个已初始化对象 |
| `mem.size_of<T>()` / `mem.align_of<T>()` | 编译期布局常量 |
| `mem.copy<T>(dst, src)` | 等长按值复制（`memmove` 语义），不分配、不深拷贝 |
| `mem.is_valid_utf8(bytes): bool` | 只校验编码，不分配、不 trap |
| `mem.view<T>(ptr, len)` / `mem.view_const<T>(ptr, len)` | 从 C 指针创建视图，不取得释放权 |
| `mem.cast_ptr<T>(ptr)` / `mem.cast_const_ptr<T>(ptr)` | 在对象指针与 `*Unit` 间显式转换并校验对齐 |

显式类型实参写作 `mem.alloc<i32>(n)`、`mem.size_of<Point>()` 等；用户泛型由 M15 引入。切片子视图写作 `buffer.slice(start, end)`，始终检查 `0 <= start <= end <= len`。

### 13.3 `defer`

`defer call_expression;` 绑定最近的词法块，块退出时按注册逆序执行。调用参数在**退出时**求值，名称绑定注册处的局部变量身份，因此读取的是最新值。自然出块、`return`、`break`、`continue` 都会清理实际退出到的作用域；`return expr` 先求值并保存返回值再清理。trap/OOM 不展开 Dolphin 栈，因此不执行 `defer`。不支持 defer 块、嵌套 defer 和资源 `try`。

### 13.4 字符串视图

`s.bytes()` 零分配返回 `[]const u8`；`string.from_bytes(bytes)` 校验 UTF-8 后返回 `string` 视图，非法 UTF-8 以 `104` 终止。字符串字面量是静态视图，永远不能 `free`；不引入字符串 `+`。

### 13.5 C 互操作与原生链接

```dc
extern struct CPoint { x: f64, y: f64 }

extern "C" {
    pub fn demo_add(a: i32, b: i32): i32;
    pub fn demo_create(): *Unit;
    pub fn demo_destroy(handle: *Unit);
    pub fn demo_translate(point: *CPoint, dx: f64): f64;
}
```

- `c_int`/`c_uint`/`c_long`/`c_ulong`/`c_char` 是平台相关内建别名（Windows `long` 为 32 位）。
- extern 函数按原 C 符号导入，不加 Dolphin mangling；`*Unit` 表示 C 的 `void*`，禁止解引用。
- 首版拒绝把 `bool`、Dolphin `char`、`string`、切片、普通结构体、枚举按值写入 extern 签名；`extern struct` 只能通过指针传给 C。
- `dolphin.toml` 的 `[native.<triple>]` 声明 `objects`、`static-libs`、`shared-libs`、`runtime-files`，路径相对包目录；`runtime-files` 会复制到可执行文件同目录。
- 缺文件、架构不符或未解析符号会得到包含文件与目标的诊断。`dc` 消费预编译 C 文件，不编译 C 源码。

### 13.6 运行时失败与检测

| 情况 | Debug | Release |
| --- | --- | --- |
| 算术 trap、切片越界、无效切片区间、`null` 视图/对齐检查 | `101` | `101` |
| 分配失败 / 分配尺寸溢出 | `102` | `102` |
| 无效释放：未知地址、已释放地址、长度不匹配 | `103` | 不保证检测 |
| UTF-8 校验失败 | `104` | `104` |
| 正常退出时泄漏 | stderr 报告，保留程序退出码 | 不追踪 |

Debug 构建链接带存活分配登记表的检测版运行时，Release 构建链接普通版；两份运行时都在构建编译器时预编译并内嵌。登记表自身申请失败同样以 `102` 结束，`free` 先查表再调用系统 `free`，不读取已释放内存。

## 14. 编译器结构

当前编译流程：

```text
UTF-8 源码
  -> 词法分析和 Token
  -> AST
  -> 函数签名收集
  -> 名称、作用域和类型检查
  -> 类型化 CFG IR
  -> CodegenBackend（Cranelift 或可选 LLVM）与本机目标文件
  -> 内嵌最小输出运行时目标文件
  -> 系统链接器 cc/link（--bundled-linker 改用 rust-lld）
  -> 本机可执行文件
```

控制流 IR 由基本块、指令和终结指令组成。每个基本块最终以跳转、条件跳转或返回结束。用户函数在生成函数体前统一声明，因此支持前向调用和递归。

### 14.1 最小运行时

运行时通过固定 ABI 提供：

```text
dolphin_print_i32
dolphin_print_i64
dolphin_print_u64
dolphin_print_f32
dolphin_print_f64
dolphin_print_char
dolphin_print_bool
dolphin_print_string
dolphin_string_equal
dolphin_alloc
dolphin_free
dolphin_copy
dolphin_is_valid_utf8
dolphin_check_utf8
dolphin_check_align
dolphin_check_view
dolphin_init_args
dolphin_arg_count
dolphin_arg
dolphin_env
dolphin_last_error_kind
dolphin_last_error_code
dolphin_stream_stdin
dolphin_stream_stdout
dolphin_stream_stderr
dolphin_stream_read
dolphin_stream_write
dolphin_stream_flush
dolphin_stream_close
dolphin_stream_is_open
dolphin_stream_open
dolphin_stream_release
dolphin_stream_from_raw
dolphin_test_fail
dolphin_runtime_finish
```

运行时使用系统 `write`（Unix）/`WriteFile`（Windows）输出，不依赖可变参数 `printf`。M12 起运行时源码由 `crates/dolphin-platform/build.rs` 调用 C/C++ 编译器预编译并内嵌，构建 Dolphin 程序时无需现场编译 runtime，链接默认使用系统链接器（Unix `cc`，Windows `link`）。`--bundled-linker` 时改用 Rust 工具链的 `rust-lld`，其链接参数仍可能在运行时通过 `cc`/`xcrun` 探测，失败后回退编译期路径；CRT、SDK 与导入库不因内嵌 runtime 而消失。实际平台依赖与干净环境验证限制见[安装说明](installation.md#6-系统依赖边界)。

## 15. 当前诊断

已实现的主要编译错误包括：

- 非法字符、字符串转义和未闭合注释。
- 语法符号缺失。
- 重复函数、重复局部变量和重复参数。
- 缺少或重复 `main`。
- 未知函数、未知变量和未知类型。
- 参数数量或类型错误。
- 对 `val` 或函数参数赋值。
- 非布尔条件。
- 循环外使用 `break` 或 `continue`。
- 非 `Unit` 函数缺少返回路径。
- 不可达语句。
- 格式串占位符和花括号错误。
- 数组元素、长度、下标类型和常量越界错误。
- 修改 `val` 数组元素。
- `for` 的遍历对象类型错误。
- `pkg` 与文件路径不一致或缺失。
- 未知导入、导入冲突和跨模块私有访问。
- 重复字段、重复枚举项、重复类型名。
- 未知字段、未知枚举项、构造参数数量或类型错误。
- `match` 缺失分支、冗余分支和解构绑定数量错误。

诊断包含稳定类别 `E0000`/`E0001`、文件路径、Unicode 字符列号、多行源码标记。CLI 支持 `--color auto|always|never`。M20 起词法/语法与声明级语义错误按同步点收集（见 18.5），函数体内仍首错即停。

## 16. 泛型、方法与 trait（M15-A，已完成）

按 [M15 实现规格](proposal-m15-generics-stdlib.md)已实现：

- **用户泛型**：`fn identity<T>(value: T): T`、`struct Pair<T>`、`enum Maybe<T>`；声明 `<T, E>`、
  类型实参 `Pair<i32>`、显式调用 `identity<i32>(x)` 与由实参推断 `identity(a)`；模板调用模板
  与嵌套泛型实例。
- **单态化**：`crates/dolphin-hir/src/monomorphize.rs` 以 `(PackageId, 限定名, 具体类型实参)` 为实例 key，登记后展开
  函数体并用工作队列迭代到不动点；每个 key 只生成一次，未实例化模板不产出符号。实例链深度上限 128、
  实例总数上限 10000，超限给出诊断而不 panic。`PackageId` 定义于 `crates/dolphin-package/src/package.rs`，`ROOT` 为当前包，
  `STD` 为源码标准库身份。
- **递归与布局**：支持普通/互递归与经指针自引用（`struct Node<T> { next: *Node<T> }`）；
  按值包含环（结构体字段、枚举 payload）在实例化后立即报 `recursive layout`。
- **方法与 trait**：`impl Type` 与 `impl<T> Box<T>` 固有方法；`trait` 声明 + `impl Trait for Type`
  静态分派；接收者 `self` / `self: *Self` / `self: *const Self`，可寻址对象自动取址。impl 目标实参在
  声明处校验（H18-05）：支持具名 struct/enum 的非泛型 impl，以及目标实参与 impl 参数按位置一一对应的
  完整泛型形式（参数可改名）；具体类型特化、重排/重复/嵌套实参、漏参/多参、blanket impl、泛型
  trait 实参、impl 参数 bound 与方法独立泛型参数都在声明处明确拒绝，即使 impl 为空或方法从未调用。
  跨包参数化 impl 与关联类型按定义包身份解析。
- **约束与关联类型**：泛型函数与具名类型（struct/enum）都支持单约束 `C: Trait`；约束在具名类型
  实例化的公共入口检查，字段、签名、嵌套类型与无 payload variant 同样受约束；`Self::Item`、`C::Item`
  与类型声明的 `T::Item` 通过 impl 的 `type Item = T;` 绑定解析；缺少约束实现、缺失 trait 成员、
  未知/重复方法、trait 实现签名（参数数量、接收者可变性、返回类型）不一致均有源码级诊断，
  错误包含类型名、缺失 trait 与实例化位置。同名 trait 方法歧义仍按重复方法拒绝。
- **字段可见性**：字段默认模块私有，跨模块读/写/位置构造须 `pub`。
- **聚合 payload 与路径**：泛型枚举可携带 `string`、结构体等复合值；支持 `mod.Type<i32>`、
  `Self::Item`、`Type::function()`。

## 16.1 源码标准库与迭代（M15-B，已完成）

- **注入方式**：`crates/dolphin-std/src/*.do` 由 `crates/dolphin-hir/src/modules.rs` 在每次构建中与用户源码一起解析，携带保留身份
  `PackageId::STD`；`std.mem` 仍是无源码的内建入口。用户模块不能占用 `std` 命名空间。
- **模块与 API**：`std`（`Option`/`Result`/`Iterator`）、`std.collections`（`Vec<T>`、`SliceIter<T>`、
  `Range`）、`std.text`（`String`、`concat`/`trim`/`substring`/`starts_with`/`ends_with`/`contains`/
  `from_utf8`、`TextError`、H19-04 的 `lines`/`Builder`/`parse_i64`/`parse_u64`/`NumberError`）、
  `std.ffi`（`CString`、`CStringError`）、`std.process`（进程参数与环境，
  H19-01）、`std.error` 与 `std.io`（错误类别与标准流字节 I/O，H19-02）、`std.fs`（文件打开与模式，
  H19-03）、`std.test`（用户测试断言，H19-05b）。
- **导入与 prelude**：`use std.mem;`、`use std.text;`、`use std.collections.Vec;` 与类型导入；
  `Option`/`Result`/`Iterator` 作为最小 prelude 自动可用，被本模块定义或显式导入时遮蔽。
- **迭代器协议**：`for x in expr` 要求 `expr` 实现 `Iterator`；数组与切片经只读切片适配到
  `SliceIter<T>`，`start..end` / `start..=end` 降低为 `Range`，`s.iter()` 零分配。`break`/`continue`
  与每轮 `defer` 走同一清理路径。
- **`Vec<T>`**：`init`/`with_capacity`/`len`/`capacity`/`push`/`reserve`/`get`/`set`/`pop`/`as_slice`/
  `as_mut_slice`/`iter`/`clone`/`clear`/`deinit`；backing 字段模块私有，扩容溢出走 102。
- **`String`/`CString`**：拥有型缓冲 + 零分配视图；`substring`/`from_utf8` 返回 `Result`，
  `CString::from` 拒绝内部 NUL，`ptr()` 交给 C，`deinit` 释放。
- **`std.process`（H19-01）**：`arg_count(): usize`、`arg(index): Result<string, ArgError>`、
  `program_name()`、`env(name): EnvLookup`。返回值都是**借用视图**，有效到进程结束，不得
  `deinit`/写入；`ArgError` 区分 `OutOfRange` 与 `NotUtf8`；`EnvLookup` 三态区分 `Found`/`Missing`/
  `NotUtf8`。Unix 直接保存字节 argv 并校验 UTF-8；Windows 用宽字符 API 转 UTF-8（含未配对代理项
  的条目为 `NotUtf8`）。`main` 仍无参数，源码兼容不变。
- **`std.error`（H19-02）**：`ErrorKind`（`Other`/`NotFound`/`PermissionDenied`/`IsADirectory`/
  `InvalidArgument`/`NotOwned`/`Closed`，判别值 `0..6` 与运行时 ABI 一致）、
  `Error::new(kind, code)`、`kind()`、`code()`、`from_last_error()`。错误是值类型，不拥有内存、
  不需要释放；`code` 保留 native errno/GetLastError，纯 API 错误为 0。
- **`std.io`（H19-02）**：`Stream`、`stdin()`/`stdout()`/`stderr()`、
  `read(self, []u8): Result<usize, Error>`（短读正常，`Ok(0)`=EOF）、`write(...): Result<usize, Error>`、
  `write_all(...): Result<bool, Error>`（循环写完整段；写入 0 或出错返回 Err）、
  `flush`/`close`/`close_abort`/`is_open`、`eprint([]const u8)`；标准流是**借用句柄**，`close` 返回
  `Err(NotOwned)` 且不影响后续写入，副本共享运行时状态、`close` 幂等。成功类返回值用 `bool`
  （`true`）而非 Unit：当前语言无 Unit 值（`Result<Unit, E>` 会得到诊断）。异常 UTF-8 字节可读入
  `[]u8`，但转 `string` 必须经 `std.text.from_utf8` 校验；`string.from_bytes` 保持 104 trap 语义。
  `release(self): usize` 显式转交自有句柄（源句柄置 0，接收者用 `from_raw` 接管）；借用/已关闭返回 0；
  `from_raw(handle)` 对非法/已关闭/借用 id 得到已关闭句柄（`is_open=false`、读写 `Err(Closed)`）。
- **`std.fs`（H19-03）**：`OpenMode{Read, Write, Append}` 与
  `open(path: string, mode): Result<Stream, Error>`。`Read` 不存在 → `NotFound`；目录 → Unix
  `IsADirectory`、Windows `InvalidArgument`；`Write` 创建/截断、`Append` 创建/追加（Unix 0644）。
  路径含内部 NUL 在调用运行时前返回 `InvalidArgument`；Windows UTF-8→UTF-16 失败同样报错。
  句柄是自有资源：`close` 幂等，重绑定前必须 `close`/`release`；Debug 运行时在退出收尾报告
  未关闭的自有流（`Dolphin: N open handle(s) not closed at exit`，退出码不变），借用标准流不计入。
- **`std.test` 用户测试（H19-05b）**：`expect(condition: bool)` 与 `fail()`；失败写 stderr
  固定文本 `Dolphin test assertion failed` 并以 `106` 退出（与 trap 一样不展开栈、不执行 defer、
  不运行 Debug 收尾报告）。不提供带消息断言；需要上下文时先 `println`。
- **`std.text` 文本/数值（H19-04）**：`lines(bytes: []const u8): Lines` 按 `\n` 切分，去掉紧邻
  `\n` 前的一个 `\r`，孤立 `\r` 保留，无 `\n` 的非空尾段算一行，空输入 0 行；产出 `[]const u8`
  **借用视图**、零分配、不校验 UTF-8。`Builder` 是**拥有型**增长缓冲：`init`/`with_capacity`/
  `append(string)`/`append_bytes([]const u8)`/`len`/`is_empty`/`view`/`consume`/`clear`/`deinit`；
  `view(): []const u8` 是借用视图，在下一次 `append*`/`consume`/`clear`/`deinit` 后失效（扩容会
  替换底层存储），构造中允许暂不完整的 UTF-8，转文本前用 `from_utf8` 校验；长度和/倍增溢出走
  102，不做算术 trap。`parse_i64`/`parse_u64` 只接受可选 `+`（`parse_i64` 还可选 `-`）与 ASCII
  数字，`NumberError{Empty, InvalidDigit, Overflow}` 在乘加前检查溢出，不触发 101。
- **所有权边界**：容器赋值/元素读取/Vec.clone 是浅复制；clear/deinit 不递归释放拥有型元素。`Vec<String>` 等需按 API 契约逐元素释放；view 不延长缓冲寿命，可写 deinit 的接收者应为 var。
- **`Unit` 约束（H19-02）**：`Unit` 没有运行时值，不能作为 struct 字段或 enum payload 按值存储；
  实例化时给出诊断，不再在 codegen 内部 panic。

## 17. lib 项目、包图与库包发布（M15-C…F，已完成）

按 [M15 实现规格](proposal-m15-generics-stdlib.md)已实现：

- **lib 目标**：`[lib].path` 指向 `[package].source` 的直接子文件；一个包最多一个 lib，可同时有多个 `[[bin]]`；
  至少声明一种目标。lib 不要求 `main`，`ir::Program.main` 在库构建中为 `None`，只产出验证目标文件
  `target/lib/<name>.o` 与库包，不链接可执行文件。`dc run` 对纯库项目明确拒绝，`--lib` 与 `--bin` 互斥。
  作为依赖被消费时只加载该包的库源码：其 `[[bin]]` 入口文件（含 `main`）始终排除，与归档已排除 bin 的发布包
  行为一致；库与 bin 共享的非入口 helper 文件仍加载。
- **依赖与包图**：`crates/dolphin-package/src/resolver.rs` 从根清单展开 path 与坐标依赖为 `PackageGraph`；规则要求一个 `(group, name)`
  只允许一个精确版本与一个来源：同坐标 Path/Root 与 Remote 永不视为同一来源，同一 Remote 的不同仓库 ID 也视为冲突，
  任一解析顺序都在加载时拒绝并列出两条请求链与来源。包依赖环显示环路径，依赖必须声明 `[lib]`。
  别名是每包私有的解析环境，应用不能直接使用未声明的传递依赖。
- **跨包身份与可见性**：每个依赖包获得唯一编译期模块前缀 `@<PackageId 序号>`（根包为空、标准库为 `std`），
  `(PackageId, 限定名, 实参)` 实例 key 保证同名定义不碰撞、同一库泛型只单态化一次；模板按定义包解析私有 helper，
  调用包不能访问其私有项。
- **确定性 `.dlib`**：`crates/dolphin-package/src/package_archive.rs` 写出 ZIP，条目按 UTF-8 路径排序、统一 `/`、DOS 时间戳
  `1980-01-01 00:00:00`、权限 `0644`、DEFLATE 级别 6；含 `META-INF/dolphin-package.toml`、规范化清单、
  完整库源码、带 SHA-256 记录的 C 原生文件与 `LICENSE`。相同输入与 dc 版本产生相同摘要。
- **安全解包**：拒绝绝对路径、盘符/UNC、反斜杠、`..`、符号链接、重复条目、大小写冲突与未允许的顶层路径；
  单包 256 MiB、单文件 128 MiB、清单 1 MiB、10000 文件上限，提取过程继续计数。
- **仓库与缓存**：`crates/dolphin-package/src/registry.rs` 支持 `file://` 与 `http(s)://`；坐标映射到
  `<base>/<group 目录>/<name>/<version>/<name>-<version>.dlib(.sha256)`；TLS 校验、30 秒超时、
  仅连接/5xx 重试最多 3 次、最多 5 次同源重定向且不允许 HTTPS 降级。`crates/dolphin-package/src/cache.rs` 在
  `DOLPHIN_HOME`（默认 `~/.dolphin`）下按内容寻址缓存归档与解包目录，并记录坐标索引。
- **锁文件与模式**：`crates/dolphin-package/src/lockfile.rs` 读写 `dolphin.lock`（整个传递闭包、排序依赖、path/远程来源与摘要）；
  `--locked` 要求与清单/仓库/编译器版本一致且不重写，`--offline` 不访问 HTTP(S)，`dc fetch` 只解析写锁。
  同坐标远程内容变化时即使不是 `--locked` 也不接受新摘要。
- **发布**：`dc publish` 以 `If-None-Match: *` 条件 PUT 上传包，再上传摘要作为完成标记；已存在同字节幂等成功，
  内容不同拒绝覆盖，半上传可补全摘要；`file://` 仓库用临时文件 + 不覆盖的原子发布；token 读取
  `DOLPHIN_REPOSITORY_<ID>_TOKEN`。
- **原生文件**：依赖的原生输入按反向拓扑顺序（使用者先于提供者，共享依赖排在全部使用者之后）去重合并；
  同名不同内容 runtime 文件报冲突；包只在其列出的目标上可消费。
- **示例**：`examples/m15` 通过 `math = { path = "mathlib" }` 演示 lib + path 依赖 + 跨包泛型。
- **`dc test` 用户测试（H19-05a/b/c）**：需要 `[lib]` 目标；发现包根 `tests/` 的**直接子文件**
  `*.do`（不递归）中的 `test_*` 函数，按函数名排序生成根模块入口，测试二进制输出到
  `target/test/<包名>-tests[.exe]`。测试文件不得声明 `pkg`、不得定义 `main`；`test_*` 必须无参数、
  无类型参数、返回 Unit（无返回类型标注）；其他函数可作为 helper。测试与 `src/*.do` 同属根模块，
  可访问根模块私有项与子模块 `pub` 项（子模块私有项不可见）。断言用 `std.test.expect/fail`。
  入口的内部调用形式 `<二进制> --dolphin-test <名称>` 不对外承诺。
  **执行与汇总（H19-05c）**：每个测试在独立子进程中运行，stdout/stderr 直接继承；固定 30 秒超时
  （超时 kill 并回收，继续后续测试）。`--filter <子串>` 按名称子串选择子集。输出固定为
  `test <name> ... ok`、`... FAILED (assertion)`（退出码 106）、`... FAILED (trap exit N)`、
  `... FAILED (timeout after 30s)`，最后一行 `N passed; M failed; K filtered out`；全部通过 0、
  任一失败 1、0 测试 `no tests found` 1、过滤无匹配 `no tests matched filter` 1、用法错误 2。
  `examples/m19` 的 `textstats`（lib）与 `dtext`（lib+bin，path 依赖）演示该闭环；因 D1
  （`dc build --lib` 仍打包且 `.dlib` 拒绝 path 依赖），`dtext` 用 `dc build --bin dtext` 构建。

## 18. 优化后端与开发工具（M16、M17、M20，已完成）

### 18.1 后端无关 IR 与 LLVM 后端（M16）

- `dolphin-backend` 定义 `CodegenBackend`；`dolphin-codegen-cranelift` 与
  `dolphin-codegen-llvm`（inkwell/LLVM）各自实现，驱动按 `BackendChoice` 选择。
- 默认 Cranelift；`--backend llvm` 或 `DOLPHIN_BACKEND=llvm` 切换；未编译所选后端时返回
  诊断，不静默回退。`dc env` 显示默认后端。
- 两后端共用 `dolphin-ir` 的布局并以一致的陷阱/ABI 行为为目标；`tests/backend.rs` 目前对一份组合源码比较两
  后端的退出码与标准输出，覆盖范围有限。基准见 `examples/m16` 与 `scripts/bench.py`。

### 18.2 调试信息（M17）

- IR 在基本块旁携带预计算的 `Location`（`BasicBlock.locations` 与指令一一对应），
  `dolphin-ir` 不依赖 `dolphin-source`。
- LLVM 后端在 Debug 配置下发射 DWARF 编译单元、子程序与行表，可被 gdb/lldb 加载；
  `tests/backend.rs` 校验 Debug 产物含 `.debug_line`/`.debug_info`，不是实际断点/单步/局部变量验收。
- Cranelift 后端暂不生成调试信息；仅 Unix DWARF，Windows/PDB 未覆盖。

### 18.3 格式化器与语言服务器（M17）

- `dc fmt`：基于字符状态机的保守空白整理（缩进、去行尾空白、折叠空行），不依赖类型检查；
  `--check` 只检查并以非零退出。
- `dc lsp`：stdio 上的最小 LSP，提供文档诊断、文档符号、悬停与跳转定义；未实现的方法返回
  `null`，EOF/`exit` 正常退出。
- LSP 对含 pkg/use 或无 main 的文件跳过语义检查；hover/definition 基于当前文档顶层声明，不支持完整作用域绑定和项目级跨文件/跨包导航。上述扩展及 formatter 保持性、调试器实测已由 M20 交付，见 18.5。

### 18.4 IR 与 LLVM 校验（H18-06）

- **后端无关 IR 校验**：`dolphin-ir::verify` 在 HIR 完成 Program 后统一运行（`lower`/`lower_sources`/
  `lower_library` 都经过同一出口，`dc check`/build/run/lib 均覆盖）。检查索引与 ID 边界、`Type::Struct`/
  `Enum` 与 `TypeDef` 种类一致、按值布局自环/互环（经指针/切片的递归合法）、普通函数 entry/跳转目标、
  `bool` 分支条件、外部函数无函数体、库 `main=None`、每个块 instruction/location 数量一致、
  source/location 引用有效，以及指令、表达式与 Place 的类型一致性（保留合法只读限定转换与 `null`）。
  错误返回后端无关的 `VerifyError`，由 HIR 转成诊断；校验是内部契约检查，不代替用户程序检查。
- **LLVM verifier**：LLVM 后端在 DebugInfo finalize 后、优化前调用 `module.verify()`，Release 在
  `default<O2>` 后再校验一次；失败返回带阶段（before/after optimization）与原始 LLVM 信息的诊断，
  不 unwrap、不写目标文件。
- **循环 sret 槽**：聚合返回值调用的 sret 缓冲区提升到函数入口块（每个调用点一个静态槽），
  避免 Debug（无优化）下循环内 alloca 随迭代累积栈；`tests/m18_sret.rs` 以 1e6 次迭代固定回归。

### 18.5 项目级诊断与开发工具（M20）

- **结构化诊断**：`Diagnostic` 在保留 `plain`/`at` 文本渲染的同时提供 `code`/`severity`/
  `labels`/`notes` 与 `with_label`/`with_note`/`with_code`；`SourceId`/`SourceFile.id`/`SourceMap`
  提供文件身份与位置换算。code 登记：`E0000`（`plain`）、`E0001`（有位置错误）、`E0002`
  （诊断数量上限）、`E1001`（依赖不可本地恢复）、`E1002`（清单/源码根加载失败）、`E2001`
  （分析内部错误）。
- **前端多错误收集**：`lex_recovering`/`parse_recovering` 在同步点恢复并收集词法/语法错误
  （每文件最多 100 条 + `E0002`）；`lower_sources_analysis_collecting` 逐声明收集语义错误；
  函数体内仍首错即停。`dc check/build/run/test` 编译前打印全部诊断并退出 1，错误程序不产出
  任何对象/可执行/`.dlib`。声明级/函数体错误消息使用可读限定名与泛型实参（无 `TypeId(n)`）。
- **共享项目分析**：`dolphin-analysis` 的 `AnalysisHost`/`AnalysisSnapshot`/`SymbolIndex` 在
  无网络、不写 `dolphin.lock`、不产生产物（允许解包已有缓存归档到 `DOLPHIN_HOME`）的前提下
  分析 lib/多 bin/path/缓存坐标依赖项目；`resolve_readonly` 对不可本地恢复的依赖返回 `E1001`
  并提示 `dc fetch`。overlay 以词法绝对路径映射未保存文本，`didClose` 恢复磁盘版本或移除
  仅存在于 overlay 的新文件；旧 version 的 `set_overlay` 被忽略，`revision` 作为发布契约。
  CLI 构建路径仍使用原有网络/写锁逻辑。
- **项目级 LSP（`dc lsp [项目目录]`）**：hover/definition 基于 `SymbolIndex` 的符号身份
  （支持参数、局部遮蔽、`for`/match 绑定、跨文件与跨包 pub 定义），不再按文本同名查找；
  诊断按 overlay version 发布，项目级诊断经 `window/showMessage`；协议按 LSP 3.17：
  `initialize` 前请求 `-32002`、未知方法 `-32601`、`shutdown` 后请求 `-32600`、未 `shutdown`
  的 `exit` 退出码 1（EOF 为 0）。无 `dolphin.toml` 时按单文件规则分析（`pkg`/`use` 或缺少
  `main` 时跳过语义检查，语义成功仍提供真实索引）。
- **`dc fmt` 项目发现**：无路径参数时从 cwd 向上发现 `dolphin.toml`，根为 `[package].source`
  （默认 `src`）；递归排除 `build.output`（默认 `target`）与 `.git`、不跟随目录符号链接；
  显式文件精确生效；全部选中文件先在内存格式化，任一失败不写任何文件（全有或全无）；
  `--check` 零写入；输出统一 LF。
- **调试器验收**：`scripts/debug_smoke.sh`（gdb）与 `scripts/debug_smoke_lldb.sh`（lldb）
  在 LLVM Debug 产物上实际验证断点命中正确文件行、单步行映射与 `bt` 调用链；Release 与
  Cranelift 明确报告无行表（DBG-04 边界）。Cranelift/PDB 调试信息不在本阶段。
- **路径身份**：分析路径、overlay 键与 LSP `file://` URI 使用绝对、纯词法规范化路径，
  不解析符号链接；路径依赖包根不再带 `fs::canonicalize` 的平台产物（Windows `\\?\` 前缀、
  macOS `/var`→`/private/var`），因此未保存依赖文本与 definition 跳转在编辑器打开的路径上
  生效。`by_path` 身份键、`PackageSource::Path` 与 `dolphin.lock` 输出不变。

## 19. 明确未实现

当前未实现的语法、工具能力或明确不采用的设计包括：

- 嵌套数组和空数组字面量。
- 资源 `try` 语法、借用检查、生命周期参数、GC/RC 与隐式析构。
- 指针算术、整数地址转换、指针悬垂检查。
- C 头文件导入、C 可变参数/回调/函数导出、C 聚合值按值 ABI、union/位域/packed。
- 自定义 allocator/arena 与参数传递。
- 多约束/泛型 trait、trait 默认方法与动态分派；`match` 分支体暂不支持语句块（`=> { ... }`）。
- 数组整体比较、直接格式化和数组方法。
- 通配符导入、导入别名和重导出；依赖版本范围求解。外部 path/精确坐标库依赖已实现。
- 三元表达式和隐式数值转换。
- `?T` 可选类型和 `?` 错误传播运算符。
- 网络与子进程标准库。
- 闭包和异常。
- 链式字段上的直接方法调用（`a.b.method()`）与 `return match ... Result.Err(...)` 的泛型构造臂
  类型推断：前者按路径解析为函数名而报未知函数，后者报无法推断类型参数；组合错误路径时先
  绑定局部变量（`val x = a.b; x.method()`、`val e = ...; return Result.Err(e)`）即可（H19-06）。
- 闭源二进制 Dolphin 库包、稳定二进制 ABI、增量编译、交叉编译。
- Cranelift 后端的调试信息、Windows/PDB 调试信息，以及函数体内的表达式级多错误收集
  （词法/语法/声明级收集已在 M20 实现，见 18.5）。

实现顺序见[路线图](roadmap.md)，逐批交接与验收见 [M18-M21 计划](plan-m18-plus.md)。
