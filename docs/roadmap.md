# 实现路线图

> 当前基线：M0-M20 已完成，M20 后完成链接器发行策略调整（默认系统链接器，见[进度报告](reports/linker-system-default-progress.md)）。语言核心（M0-M15）、可选 LLVM 与后端无关 IR（M16）、格式化器/LSP/DWARF（M17）、正确性收敛与发布门禁（M18）、真实 CLI 与用户测试（M19）、项目级诊断与开发工具（M20）均已验收；M21 按实际负载与交付需求规划。

路线图以[已实现功能参考](implemented-features.md)为基线，以[语言设计说明](language-design.md)为目标。只有代码、测试、示例和文档全部完成，里程碑才能标记为完成。

后续执行见 [M18-M21 人工交接指南](plan-m18-plus.md)。M18 于 2026-09-18 完成（H18-00..11），M19 于 2026-09-23 完成（H19-00..07），M20 于 2026-09-24 完成（H20-00..05 + Windows H20-W + macOS H20-M），证据见 [M18](reports/m18-progress.md)、[M19](reports/m19-progress.md)、[M20](reports/m20-progress.md) 进度报告。已完成标记是历史交付记录，不代表所有组合语义无缺陷；正确性审计范围见 M18 报告。

## 1. 已完成

### M0：空程序

- [x] 读取 `src/main.do`
- [x] 解析空 `main`
- [x] Cranelift 生成本机目标文件
- [x] 系统链接器生成可执行文件

### M1：整数算术

- [x] `i32` 字面量
- [x] 算术、一元负号和括号
- [x] 运算符优先级
- [x] 进程退出码
- [x] 溢出与除零陷阱

### M2：局部变量

- [x] `var` 和 `val`
- [x] 显式 `i32` 标注和局部推断
- [x] 普通与复合赋值
- [x] 块级作用域和不可变检查

### M3：控制流

- [x] `bool` 和逻辑运算
- [x] 比较和相等
- [x] 短路求值
- [x] `if/else`、`while` 和 `loop`
- [x] `break` 和 `continue`
- [x] CFG IR

### M4：函数

- [x] 多函数、参数和返回类型
- [x] 前向调用和递归
- [x] `Unit` 函数
- [x] 调用类型检查和返回路径检查

### M5：字符串和输出

- [x] UTF-8 `string`
- [x] 字符串参数、局部值和返回值
- [x] `print` 和 `println`
- [x] `{}`、`{{` 和 `}}`
- [x] 最小输出运行时
- [x] macOS ARM64 PIC 目标文件

## 2. M6：数组与 `for`（已完成）

目标：完成基础集合和遍历能力，让语言设计中的基础示例可以在单文件模式下运行。

### 类型和语法

- [x] 定长数组类型 `[T; N]`
- [x] 数组字面量 `[1, 2, 3]`
- [x] 重复初始化 `[0; 10]`
- [x] 数组下标读取和写入
- [x] 数组局部变量、函数参数和返回值 ABI
- [x] 半开范围 `start..end`
- [x] 闭合范围 `start..=end`
- [x] `for name in range`
- [x] `for name in array`

### 语义和运行时

- [x] 数组元素类型与长度检查
- [x] 下标必须是整数
- [x] 编译期已知越界检查
- [x] 运行时边界检查和稳定失败行为
- [x] `val` 数组不可修改元素
- [x] `break`、`continue` 与嵌套 `for`
- [x] 明确暂不允许数组直接格式化

### 验收标准

- [x] 新增 `examples/m6`
- [x] 能遍历数组并计算总和
- [x] 半开和闭合范围边界正确
- [x] 越界程序安全终止
- [x] 正确与错误用例均有测试
- [x] M1-M5 全部回归通过

## 3. M7：项目与模块（已完成）

目标：从“只读取一个 `src/main.do`”升级为真正的多文件项目。

### 源码发现

- [x] 递归扫描项目 `src/`
- [x] `src/*.do` 归入根模块并允许省略 `pkg`
- [x] 子目录文件必须声明与所在目录一致的 `pkg`
- [x] 检测重复函数和模块路径不一致
- [x] 根目录多文件合并；子目录每个文件对应一个模块

### 名称和可见性

- [x] `pkg`
- [x] `use module.path`
- [x] `use module.path.member`
- [x] `pub` 顶层可见性
- [x] 明确暂不支持显式模块别名
- [x] 导入冲突、未知模块和私有访问诊断
- [x] 纯函数模块允许循环导入

### 标准库

- [x] 标准库源码暂放在项目 `src/std/`
- [x] 建立 `std.math` 示例模块
- [x] 区分编译器内建函数与源码标准库
- [x] 多模块链接使用稳定内部函数编号

### 验收标准

- [x] 新增 `examples/m7`
- [x] 根模块调用 `src/std/math.do` 的公开函数
- [x] 私有函数不能跨模块访问
- [x] 路径与 `pkg` 不一致时有准确诊断
- [x] M1-M6 全部回归通过

## 4. M8：完成 MVP（已完成）

目标：补齐第一阶段基础类型、转换、诊断和命令体验，形成可发布的 MVP。

### 基础类型

- [x] `i8`、`i16`、`i64`
- [x] `u8`、`u16`、`u32`、`u64`
- [x] `f32`、`f64`
- [x] `char`
- [x] 数值字面量后缀
- [x] `as` 显式转换
- [x] 各类型的溢出、除零和比较规则
- [x] 浮点数、字符和数组元素格式化

### 字符串完善

- [x] 字符串内容相等比较
- [x] `length(string)` UTF-8 字节长度查询
- [x] 字符串切片和索引不进入 MVP
- [x] 字符串不可索引，因此不存在无效 UTF-8 边界

### 诊断

- [ ] 一次编译报告多个独立错误（延后到 MVP 之后）
- [x] 稳定诊断类别 `E0000`、`E0001`
- [x] Unicode 列号和多行 Span 展示
- [x] `--color auto|always|never`
- [x] 运行时越界、溢出和除零错误信息
- [x] 用户错误使用诊断返回，内部不变量保留为编译器错误

### 命令行和构建

- [x] `dc check <project>`
- [x] `dc build <project>`
- [x] `dc run <project>`
- [x] Debug/Release 优化级别
- [x] 目标文件和运行时目标文件使用稳定输出路径
- [x] Linux x86_64 与 macOS ARM64 CI
- [x] LLVM 后端 Debug 的 Unix DWARF 行表与函数信息（M17；Cranelift 与 Windows/PDB 仍未覆盖）

### 验收标准

- [x] 语言设计中的单文件 MVP 示例全部运行
- [x] M1-M8 能力由端到端测试构建运行
- [x] 已覆盖的错误输入不导致编译器 panic
- [x] 生成 `dc` 和兼容名称 `dolphin-compiler`
- [x] 语言规范、实现参考和实际行为一致

## 5. 第二阶段：可分发编译器（M9-M12）

第二阶段不急于扩展语言语法，目标是把已经可用的 MVP 变成能够在主流平台稳定安装、构建和发布的编译器。这里的“自包含”是指用户安装官方发行包后，执行 `dc check/build/run` 不需要另行安装 C 编译器；生成的程序是否依赖操作系统动态库，需要按目标平台分别说明。

### M9：项目清单与构建描述（已完成）

目标：引入类似 Cargo.toml 的 `dolphin.toml`，让项目入口、产物和构建选项由稳定配置描述，而不是依赖目录猜测或不断增加命令行参数。

首版配置可以从以下最小结构开始，字段名称在实现 M9 前冻结：

```toml
[package]
group = "me.foxlab"
name = "hello"
version = "0.1.0"
source = "src"

[[bin]]
name = "hello"
path = "src/main.do"

[build]
optimization = "debug"
```

`group` 是项目的命名空间，推荐使用反向域名形式；包的完整坐标为 `group:name:version`，例如 `me.foxlab:hello:0.1.0`。`group` 与 `name` 共同标识一个包，`version` 标识其发布版本。`group` 由点分隔的非空标识符组成，三个字段均不得包含冒号，`version` 首版要求使用语义化版本。M9 只解析、校验并展示该坐标，不下载依赖。

后续依赖功能采用“本地别名 = 包坐标”的形式，预留语法如下：

```toml
[dependencies]
json = "org.example:json:1.2.0"
http = "me.foxlab:http:0.4.1"
```

左侧 `json`、`http` 是当前项目使用的稳定别名，右侧是全局包坐标。M9 不实现依赖下载；M15 v2 负责精确版本、仓库地址、校验和及传递依赖，版本范围求解继续延后。

- [x] 定义 `dolphin.toml` 的最小模式、默认值和未知字段处理规则
- [x] 支持 `[package]` 中的 group、包名、版本和源码目录
- [x] 校验 `group:name:version` 坐标，并在 `dc` 项目信息中显示完整坐标
- [x] 支持 `[[bin]]` 或等价配置声明一个或多个可执行目标及入口文件
- [x] 支持 `[build]` 中的输出目录、优化级别等稳定构建选项
- [x] `dc check/build/run` 从当前目录向上查找项目清单，并允许显式指定路径
- [x] 未提供清单时继续兼容当前单文件和 `src/` 项目行为
- [x] 提供带源码位置的 TOML 配置错误、重复目标和入口不存在诊断
- [x] 暂不实现远程依赖解析，但为后续 `[dependencies] alias = "group:name:version"` 保留兼容边界

验收标准：同一项目无需额外命令行参数即可重复构建；多可执行目标可以被明确选择；M1-M8 旧示例保持兼容，并新增一个基于 `dolphin.toml` 的 M9 示例。

### M10：平台与工具链抽象

目标：移除链接流程中散落的 Unix 和 `cc` 假设，为不同目标平台建立稳定边界。

- [x] 引入 `TargetPlatform`，统一目标三元组、目标文件后缀、可执行文件后缀和 ABI
- [x] 将运行时源码、运行时符号和链接参数放入平台实现，不再由通用流程硬编码
- [x] 将“生成目标文件”和“链接可执行文件”拆成可独立测试的步骤
- [x] 增加 `dc env` 或等价诊断命令，显示宿主、目标和所选链接器
- [x] 对不支持的目标给出编译器诊断，而不是等外部工具失败
- [x] 保持现有 macOS ARM64、Linux x86_64 行为和测试全部通过

验收标准：现有程序输出不变；平台差异集中在明确模块；链接命令可在测试中检查且不真正启动链接器。

### M11：Windows x86_64 原生支持（已完成）

目标：官方支持在 64 位 Windows 上构建和运行 Dolphin 程序。

- [x] 实现 Windows 运行时，替换 `unistd.h`、`write`、POSIX 信号和 GCC constructor
- [x] 明确采用 MSVC ABI，并处理 `.obj`、`.exe`、入口符号和系统导入库
- [x] 支持 Windows 路径、带空格路径和进程退出码
- [x] 在 Windows CI 执行格式检查、测试、Release 构建和端到端示例
- [x] 为 Linux、macOS、Windows 分别记录最低支持版本与已知依赖

原验收标准要求 Windows x86_64 干净环境可检查、构建和运行 M1-M8。当前证据主要来自已激活 MSVC 的 CI 环境，不能据此认定无 SDK/开发库机器也可构建；具体依赖见[安装边界](installation.md#6-系统依赖边界)。运行时保证仅覆盖明确检测的错误，不包括所有悬垂指针或未初始化访问。

### M12：自包含工具链与发布稳定化（已完成）

原目标：官方发行包构建程序时不再调用系统 `cc`。已交付预编译 runtime 和 LLD，但链接参数探测仍可能调用 `cc`/`xcrun`；不等于完全独立于系统 CRT/SDK。

- [x] 将各受支持目标的运行时预编译为目标文件，并作为编译器资源打包
- [x] 接入可再分发链接器，优先评估 LLD，统一 ELF、Mach-O 和 COFF 链接入口
- [x] 校验内嵌资源与编译器版本、目标三元组和 ABI 一致
- [x] 缺少目标资源时给出明确错误，不静默回退到错误平台的文件
- [x] 增加 `--linker` 或受控的系统链接器回退选项，便于诊断和高级使用
- [x] 记录系统 CRT、动态库和平台 SDK 仍然存在的边界
- [x] 生成 Linux x86_64、macOS ARM64、Windows x86_64 发行包及校验和
- [x] 增加安装、升级、卸载和离线使用文档
- [x] 定义命令行兼容政策、目标支持等级和语义化版本规则
- [x] 增加冒烟测试：解压发行包后从空目录构建并运行示例
- [x] 为编译时间、链接时间、编译器体积和产物体积建立基准
- [x] 补齐许可证清单和第三方组件声明

原验收标准还要求新机器只需操作系统组件与发行包即可 `dc build`。三个目标的打包与开发 runner 冒烟已存在，发布质量门禁已在 M18 补齐，但无开发环境承诺仍未通过隔离 CRT/SDK 的验收；M21 冻结系统依赖策略并验证干净环境，当前要求以[安装说明](installation.md)为准。

注意：LLVM 是代码生成后端，LLD 是链接器。M12 不要求改用 LLVM；现有 Cranelift 后端可以继续生成目标文件，再交给 LLD 链接。

后续调整（M20 之后）：为缩减发行包体积（`rust-lld` 加 `libLLVM` 约 108 MB），默认链接器改为系统链接器（Unix `cc`，Windows `link`），发行包不再携带 `rust-lld`；原 LLD 路径保留为 `--bundled-linker`，需要本机 Rust 工具链。原 `--system-linker` 作为默认行为的显式写法继续兼容。M21 的系统 SDK/sysroot 策略与干净环境验收据此更新。实现、验证范围与未验证项见[系统链接器默认化进度报告](reports/linker-system-default-progress.md)。

## 6. 第三阶段目标：实用语言与优化（M13-M17）

第三阶段开始扩展语言表达力。具体语法必须在实现前形成设计提案；若内存模型尚未确定，不应先加入会隐藏所有权和生命周期问题的容器 API。

### M13：用户自定义数据类型（已完成）

目标：引入结构体和枚举，让类型系统能够表达复合数据和带状态的分支逻辑。**值传递、移动/复制语义不在本里程碑内决定**，统一后移到 M14 的内存模型提案，避免先实现再返工。

- [x] 结构体、字段访问、构造（不可变优先，值传递语义留待 M14）
- [x] 枚举、携带数据的枚举项
- [x] `match` 模式匹配
- [x] 基于枚举实现非泛型的错误处理样例，验证 ABI 与控制流设计

`match` 前瞻性语义（实现前据此冻结语法）：

- 表达式式 `match`，各分支可求值为同一类型；实现只在表达式位置使用，语句式 `match` 尚未实现。
- 对枚举穷尽匹配，缺失分支或冗余分支产生源码级诊断。
- 支持携带数据枚举项的解构绑定；支持通配符 `_` 分支。
- 结构体/枚举字段重复、字段不存在均为编译期错误。

验收目标：可以用结构体和枚举编写一个多模块命令行示例；无效字段、重复字段和不穷尽匹配都有源码级诊断。

### M14：手动内存管理与 C 互操作（v2，已完成）

> 实施依据：[M14 实现规格](proposal-m14-memory-model.md)。从 M13 新增指针、切片、分配与 defer；细分批次 R00–R08 见[实施指南](plan-m14-m15-rework.md)。

目标：Zig 式手动申请/释放，不引入 `'a`、借用检查、GC/RC；直接声明并调用 C 函数，完成原生链接输入。

- [x] M14-A：统一 layout/place，新增聚合参数/返回、字段写入与 `usize/isize`
- [x] M14-B：新增可空指针/切片、const、取址/解引用、内建 std.mem 身份与视图
- [x] M14-C：新增 intrinsic 显式类型实参、typed 分配/释放、Debug live 表与双 runtime
- [x] M14-D：新增 defer 语法及 CFG 清理，覆盖分支与循环出口，不引入资源 try
- [x] M14-E：`extern "C"`、C 类型映射、指针传递 extern struct、native 清单和三平台链接
- [x] M14-F：补齐 UTF-8 和新示例，核对 M13 长度 ABI/用户 std 名称迁移，完成 MEM/FFI 矩阵

验收目标：手动管理动态数据的跨函数示例、真实 C 库调用、清理顺序和错误诊断全部通过；仅对明确可检测的错误承诺运行时检测，不声称能捕获所有悬垂引用。

### M15：泛型、标准库与库包发布（v2，已完成）

> 实施依据：[M15 实现规格](proposal-m15-generics-stdlib.md)。完成新 M14 后新增泛型/方法/源码标准库、lib 和包分发；细分批次 R09–R21 见[实施指南](plan-m14-m15-rework.md)。

目标：用真实 Dolphin 泛型实现易用容器；完成“lib → `.dlib` 压缩包 → 网站发布 → 坐标依赖 → 本机构建”的完整流程。

- [x] M15-A：新建用户泛型 AST、单态化/方法模块、trait/关联类型、包身份和完整 payload
- [x] M15-B：新建 std 源码模块、Vec/String/CString、Option/Result、Iterator 和视图工具
- [x] M15-C：lib-only/lib+bin 目标、本地 path 依赖、包图与跨包可见性
- [x] M15-D：经编译验证的源码型 `.dlib`、确定性 ZIP、元数据与 C 原生文件打包
- [x] M15-E：HTTPS 精确版本消费、传递依赖、内容缓存、`dolphin.lock`、locked/offline
- [x] M15-F：条件发布协议、静态网站托管流程、跨包 C 链接与三平台发行包冒烟测试

M15-A 实现要点：用户泛型声明/类型实参语法；`(PackageId, 限定名, 实参)` 实例 key 与工作队列单态化，
递归/扩张限制与按值布局环检测；泛型 `impl` 方法与 trait 静态分派、接收者自动取址；
单 trait 约束 `C: Trait` 与关联类型 `C::Item` / `Self::Item` 解析；trait 实现签名比对（参数数量、
接收者可变性、返回类型）；字段 `pub` 可见性与默认模块私有；聚合 payload。

M15-B 实现要点：随编译器注入源码标准库单元（保留身份 `dolphin:std:<version>`、`PackageId::STD`），
`std`/`std.collections`/`std.text`/`std.ffi` 与内建 `std.mem` 共存；`use std.mem`、`use std.collections.Vec`
类型导入与最小 prelude（`Option`/`Result`/`Iterator`，可被显式定义或导入遮蔽）；`for` 迭代器协议
（`it.next()` + `Option` 解构，数组/切片经只读切片适配到 `SliceIter<T>`，范围降低为 `Range`）；
`Vec<T>`、拥有型 `String`、`CString`、UTF-8 查询与 `Result` 错误；`s.iter()` 零分配适配。
新增 API-01–07 端到端测试并扩充 `examples/m15`。

M15-C–F 实现要点：`[lib]` 目标与无入口的库检查对象；`[dependencies]` 的本地 path 依赖与 Maven 风格坐标；
包图解析（精确版本、`(group,name)` 唯一来源、冲突与依赖环诊断、别名私有环境）；
每包唯一的编译期模块前缀与 `(PackageId, 限定名, 实参)` 实例 key，跨包泛型只单态化一次；
确定性 `.dlib`（ZIP + `META-INF/dolphin-package.toml` + 规范化清单 + C 原生文件摘要）与 `dc package`/`build --lib`；
仓库 GET/条件 PUT（`file://` 与 `http(s)://`）、内容寻址缓存、`dolphin.lock`、`--locked`/`--offline` 与 `dc fetch`/`publish`。

跨包泛型实例化与包图（GEN-02/05、PKG-01/02）已在 M15-C 完成，由 `tests/packages.rs` 与 `tests/manifest.rs` 覆盖。

验收目标：GEN/API/PKG 测试矩阵全部通过，新应用只声明坐标即可下载并使用库；已锁定依赖可以离线重建。v1 消费端统一编译包中源码，不承诺 JVM 字节码或稳定 Dolphin 二进制 ABI。

延后项：版本范围求解、闭源二进制 Dolphin 包、完整仓库网站后端、自定义 allocator、网络标准库与其余完整标准库、动态多态、`?T`/`?`。`Vec<T>` 与精确版本包管理已经纳入本次验收；基础文件/进程/字节 I/O 已在 M19 落地。

### M16：优化后端（已完成）

目标：建立后端无关的类型化 IR，并引入可选 LLVM 后端，用基准数据证明其收益。

- [x] 建立与后端无关的类型化 IR 和后端接口稳定性测试
- [x] 评估并实现可选 LLVM 后端，保留 Cranelift 作为快速编译后端
- [x] 用基准数据比较 Debug 编译速度、Release 性能和二进制体积

验收目标：同一套前端测试可运行于两个后端；LLVM 后端必须在约定基准上提供可测量收益，而不是仅完成接口接入。

实现要点：

- **后端边界**：类型化 CFG IR（`dolphin-ir`）与聚合布局完全独立于后端；`dolphin-backend`
  提供 `CodegenBackend` trait，Cranelift（`dolphin-codegen-cranelift`）与 LLVM
  （`dolphin-codegen-llvm`，`inkwell`/`llvm-sys`）各自实现，驱动按 `BackendChoice` 选择。
- **后端选择**：默认 Cranelift；`--backend llvm` 或 `DOLPHIN_BACKEND=llvm` 切换。
  `dc env` 显示默认后端。未编译所选后端的构建返回明确诊断，不静默回退。
- **ABI 一致**：两个后端共用 `dolphin-ir::layout` 的字节布局与组件展开；`main` 仍为
  `i32(i32, ptr)`，sret、溢出/越界陷阱、运行时 ABI 行为一致。
- **稳定性测试**：`tests/backend.rs` 在启用 `llvm` feature 时，用同一份源码在两个
  后端构建并断言退出码与标准输出一致，并静态校验 IR 不出现后端类型名。

基准（Arch Linux x86_64，Release `dc`，`examples/m16`，`scripts/bench.py`）：

| 后端 | Debug 编译 | Release 运行 | Release 体积 |
| --- | ---: | ---: | ---: |
| Cranelift | 0.027 s | 0.230 s | 16.2 KiB |
| LLVM | 0.027 s | 0.162 s | 14.2 KiB |

LLVM 在 Release 运行上约 1.4× 于 Cranelift；Cranelift 保持快速编译。数值随机器变化，
以 `python3 scripts/bench.py` 的实测为准。

### M17：开发工具（最小版本已完成）

目标：补齐开发者体验所需的调试信息、格式化器和语言服务器。此里程碑与后端无关，但排期上跟随 M16 之后，避免前后端改造并行分散精力。

- [x] DWARF 调试信息的最小可用版本（M17）
  - IR 不携带 `Span`，而是在各基本块旁维护预计算的 `Location` 侧表（`BasicBlock.locations` 与指令一一对应），`dolphin-ir` 因此不依赖 `dolphin-source`。
  - LLVM 后端在 Debug（`optimize == false`）下设置 `Dwarf Version` / `Debug Info Version` 模块标志，为每个源码文件建立 `DIFile`，为每个用户函数建立 `DISubprogram`，并在每条指令前设置行号位置；对象文件由 `TargetMachine::write_to_file` 写出，包含 `.debug_line` 与 `.debug_info`。
  - Cranelift 后端暂不生成调试信息；调试信息要求 `--backend llvm`，且仅支持 Unix 上的 DWARF，Windows/PDB 尚未覆盖。
- [x] 格式化器的最小可用版本（M17）
  - `crates/dolphin-format`：单遍字符扫描器（`Normal/LineComment/BlockComment/String/Char`），只改空白、不重写 token。
  - 规范化 4 空格缩进（行首 `}` 退一级）、去除行尾空白、折叠连续空行、保留注释原样、统一结尾换行；未闭合块注释/字符串返回诊断。
  - `dc fmt <路径...> [--check]`：默认原地写入，`--check` 只报告并以非零退出。不依赖类型检查，因此覆盖 M1-M15 全部语法。
- [x] 语言服务器（LSP）的最小可用版本（M17）
  - `crates/dolphin-lsp`：无异步运行时的 stdio JSON-RPC（`Content-Length` 帧），由 `dc lsp` 启动。
  - 支持 `initialize`/`shutdown`/`exit`、`didOpen`/`didChange`/`didClose`、`documentSymbol`、`hover`、`definition`；未实现的方法返回 `null`，EOF/`exit` 正常退出。
  - 诊断来自词法、语法及（无 `pkg`/`use` 且含 `main` 时的）语义检查；字节偏移与 LSP UTF-16 位置精确互转。

原验收目标：调试信息可被主流调试器加载；格式化器和语言服务器对 M1-M15 的全部语法特性可用。实际交付范围以上述 MVP 条目为准：LSP 不具备项目级语义分析/跨文件导航；DWARF 自动测试目前主要检查调试段，不等于真实调试器完整验收。项目级工具、格式化保持性和调试器实测已由 M20 交付（见下文）。

## 7. M18-M21 后续路线（M18、M19、M20 已完成，M21 待实施）

详细批次只在[交接指南](plan-m18-plus.md)及相应执行合同中维护，避免多个文档的子任务状态互相矛盾。

### M18：正确性收敛与可信基线（已完成）

状态：2026-09-18 按 [H18-00 至 H18-11](plan-m18-correctness.md) 逐批完成；批次证据、验收编号与
测试名见 [M18 进度报告](reports/m18-progress.md)。

- [x] 聚合写入/求值顺序、枚举布局、双后端饱和转换正确性
- [x] 类型声明 bound、impl 头校验与不支持语法的明确拒绝
- [x] 后端无关 IR verifier 与 LLVM module verification
- [x] 包来源冲突、依赖目标加载与 profile 优先级回归
- [x] LLVM Linux CI、同提交发布门禁、真实 stderr/泄漏检查（远端 CI 配置由用户确认通过）
- [x] 当前文档、可运行示例和组合回归验收（`examples/m18` + `tests/doc_examples.rs`）

验收：按 [H18-00 至 H18-11](plan-m18-correctness.md)逐批执行；关键语义用固定期望验证两后端和 Dolphin Debug/Release，保留三平台默认回归。仅本机验证不能关闭平台验收项。不实施大型语法扩展或全量编译器重写。

### M19：真实 CLI 与用户测试（已完成）

状态：2026-09-23 按 [H19-00 至 H19-07](plan-m18-plus.md) 逐批完成；API/目标程序决策（D1–D3）、
标准库、`dc test` 与 `examples/m19` 的证据、三平台复验（Linux/Windows/macOS）与远端 CI 结果见
[M19 进度报告](reports/m19-progress.md)。未新增语言语法。

- [x] 先冻结 API、资源/错误状态、测试目标及公开行为兼容决策（H19-00；D1–D3 用户确认）
- [x] 参数/环境、dc run 参数转发、基础字节 I/O 与文件 API（H19-01..03）
- [x] 最小文本/数字处理、Result/defer 错误路径（H19-04、H19-06）
- [x] 用户 dc test 与库开发闭环（H19-05a/b/c）
- [x] Dolphin 编写的真实文件/标准输入处理工具与自动化验收（H19-07：`examples/m19` + `tests/m19_app.rs`）

验收：`examples/m19` 能接收真实输入、报告错误、自动测试并清理资源；应用逻辑由 Dolphin 源码实现，不依赖外部脚本，也不引入异步/宏/闭包。

### M20：项目级开发工具（已完成）

状态：2026-09-24 按 [H20-00 至 H20-05](plan-m18-plus.md) 逐批完成，并含 Windows（H20-W）
与 macOS（H20-M）平台复验与缺陷修复；规格、批次证据、平台缺陷与三平台/LLVM lane 结果见
[M20 进度报告](reports/m20-progress.md)与 [M20 冻结规格](proposal-m20-project-tools.md)。

- [x] 结构化诊断与有限错误恢复（DIAG-01..06）
- [x] 无构建/网络副作用的共享项目分析与未保存文件 overlay（ANALYSIS-01..06）
- [x] stdlib/依赖/lib/多 bin 项目的准确诊断、悬停和定义导航（LSP-01..07）
- [x] 格式化幂等/语义保持、项目源码发现（FMT-01..06）
- [x] 真实调试器断点/单步/调用栈与项目开发流程验收（DBG-01..04；Linux gdb 与 macOS lldb）

验收：M19 项目使用依赖与未保存文档时仍能被正确分析；调试体验以真实断点/单步/调用栈与
源码行映射的结果为准，PDB/Cranelift 完整调试不在本阶段。三平台默认 lane 与 Linux/macOS
LLVM Debug 已通过；远端 CI 未在本地复验。

### M21：规模与可靠交付（条件规划）

- [ ] 多类真实负载的可重复分阶段测量
- [ ] 根据证据选择聚合表示/重复分析/构建缓存等最小优化
- [ ] 工具链与源码包兼容身份、锁与缓存迁移规则
- [ ] 系统 SDK/sysroot 策略、可重定位与干净环境发行验收

验收：性能结论有输入、原始数据和回归预算；实际上传的发行包满足声明环境，工具链升级/离线消费行为明确。是否引入构建缓存以测量数据为准。

## 8. 暂不排期的候选能力

以下能力保留为方向，不自动纳入 M20-M21 完成条件；重新评估条件见交接指南。

### 抽象能力

- [ ] 闭包
- [ ] 异步函数和协程
- [ ] 宏或编译期元编程

### 工程能力

- [ ] 依赖版本范围约束与求解（精确版本解析已纳入 M15 v2）
- [ ] Dolphin 预编译二进制库与稳定 ABI（源码型库包已纳入 M15 v2）
- [ ] 细粒度增量编译（粗粒度构建缓存已列为 M21 的条件候选）
- [ ] 完整包仓库网站与账号服务（GET/条件 PUT 客户端和锁文件已纳入 M15 v2）

### 后端和平台

- [ ] WebAssembly 后端
- [ ] 交叉编译
- [ ] 自托管编译器
- [ ] JIT 和 REPL

## 9. 实施约束

每个后续里程碑都必须：

1. 先更新语法与语义规则。
2. 为正确程序和错误程序编写测试。
3. 保持 AST、类型化 IR 与代码生成边界。
4. 增加一个独立、可运行的 `examples/mN`；纯工具链里程碑还要增加发行包冒烟测试。
5. 更新已实现功能参考和路线图状态。
6. 运行格式、静态检查、单元测试和本机端到端测试。
7. 不提前混入下一个里程碑的大型功能。
8. 每批记录验收编号到真实测试名、后端/profile/平台及命令的映射；未执行项不得写为通过。
9. 当前与历史规格分开，公开 CLI、持久包/锁格式的变更先做兼容决策。
