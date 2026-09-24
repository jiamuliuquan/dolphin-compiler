# 语言设计说明

> 状态：设计草案（Draft）  
> 源文件扩展名：`.do`

本文档描述目标语言规范，其中包含尚未实现的设计。当前编译器的真实能力以[已实现功能参考](implemented-features.md)为准，后续顺序以[实现路线图](roadmap.md)为准。

> 当前进度：已实现到 M20。M14/M15 的内存模型、泛型与源码标准库、M16 的可选 LLVM 后端、M17 的格式化器/LSP/DWARF、M18 的正确性收敛、M19 的参数环境/标准流/文件 I/O/文本处理/`dc test` 用户测试、M20 的项目级诊断/共享分析/overlay/跨文件导航/格式化发现与调试器实测均已完成。Cranelift 调试信息、Windows/PDB 和完整变量调试尚未实现。本文保留早期阶段设计，不应把“第一阶段”限制或历史示例当作当前能力清单；准确能力见[已实现功能参考](implemented-features.md)，下一步见 [M21 规划](plan-m18-plus.md)。

本文档描述一门面向学习和实践的静态类型编程语言。该语言参考 Rust、Kotlin、Java 和 C 等语言的部分语法，目标是将源代码编译为可由操作系统直接运行的本机可执行文件。

当前阶段优先保证规则简单、行为明确和编译器易于维护，不追求一次性提供完整的应用开发生态。

## 1. 设计目标

- 语法简洁，常用代码容易阅读。
- 静态类型，并支持局部变量类型推断。
- 默认提供明确、安全的行为，尽量在编译期发现错误。
- 支持函数、数组、控制流、模块和基础标准库。
- 编译为本机目标文件，并链接为可直接运行的二进制文件。
- 编译器结构清晰，方便逐步增加语言特性和新的代码生成后端。

暂不支持类、继承、闭包、异步、异常、宏和垃圾回收；泛型、指针 `null` 与 `Option<T>` 已经实现。

## 2. 编译器实现建议

编译器推荐使用 **Rust** 实现，并在第一阶段使用 **Cranelift** 生成本机代码。

Rust 适合实现词法分析器、语法树、类型检查器和代码生成器，能够兼顾执行效率、类型安全和长期维护成本。Cranelift 的接口比 LLVM 更容易接入，适合尽快打通完整编译流程；语言稳定并产生更高优化需求后，可以再增加 LLVM 后端。

以上是最初的后端选型理由；M16 已实现可选 LLVM 后端，Cranelift 仍为默认后端。下列流程保留第一阶段的设计视角。

建议的编译流程：

```text
源文件
  -> 词法分析（Token）
  -> 语法分析（AST）
  -> 模块和名称解析
  -> 类型检查
  -> 中间表示（IR）
  -> Cranelift 代码生成
  -> 目标文件
  -> 系统链接器
  -> 本机可执行文件
```

标准库中与输入输出、字符串和进程有关的底层功能，可以实现为一个小型运行时库，并在链接阶段自动加入最终程序。最终用户不需要单独安装解释器。

## 3. 源文件和注释

源文件使用 UTF-8 编码，扩展名为 `.do`。

```dc
// 单行注释

/*
 * 多行注释
 */
```

第一阶段的多行注释不支持嵌套。字符串中的 `//` 和 `/*` 不会被识别为注释。

标识符由英文字母或下划线开头，后面可以包含英文字母、数字和下划线。关键字不能作为标识符使用。第一阶段暂不支持 Unicode 标识符。

```text
合法：name、user_name、value2、_internal
非法：2value、user-name、fn
```

## 4. 程序入口

可执行程序必须包含且只能包含一个 `main` 函数：

```dc
fn main() {
    println("Hello world!");
}
```

入口函数 `main` 不接受参数；应用参数通过源码标准库 `std.process` 读取（M19）。省略返回类型时，编译器仍将 `main` 按进程入口处理：自然执行结束或 `return;` 的退出码为 `0`，也可以返回一个 `i32` 作为退出码：

```dc
fn main() {
    return 0;
}
```

读取进程参数不需要修改 `main` 签名：

```dc
use std.process.arg;
use std.process.arg_count;

fn print_arguments() {
    var index = 1_usize;
    while index < arg_count() {
        val item = arg(index);
        if item.is_ok() {
            val text = match item {
                Result.Ok(value) => value,
                Result.Err(error) => "",
            };
            println("arg {} = {}", index, text);
        }
        index += 1_usize;
    }
}
```

## 5. 基本类型

### 5.1 整数

```text
i8   i16   i32   i64
u8   u16   u32   u64
```

没有类型标注的整数字面量默认推断为 `i32`：

```dc
val count = 10;          // i32
val size: i64 = 10;
val explicit = 10_i64;  // i64
```

整数不进行可能丢失数据的隐式窄化转换：

```dc
val large: i64 = 10;
val small: i32 = large;        // 编译错误
val small: i32 = large as i32; // 显式转换
```

整数字面量超出后缀类型范围时产生编译错误。运行时整数溢出和除以零会终止程序并输出统一运行时错误；整数窄化转换保留低位。浮点到整数的 `as` 采用饱和转换，规则如下：

| 输入 | 整数结果 |
| --- | --- |
| 可表示有限数 | 向零截断 |
| 超过上界、正无穷 | 目标类型最大值 |
| 低于下界、负无穷 | 有符号目标最小值；无符号目标为 0 |
| NaN | 0 |
| 正零、负零 | 0 |

窄整数目标（如 `i8`/`u8`）按最终位宽饱和，例如 `300.0_f64 as u8 == 255_u8`。后续可以为运行时错误加入源码位置。

### 5.2 浮点数

```text
f32   f64
```

没有类型标注的浮点字面量默认推断为 `f64`：

```dc
val ratio = 0.5;          // f64
val ratio32 = 0.5_f32;    // f32
```

### 5.3 布尔值

布尔类型为 `bool`，值只有 `true` 和 `false`。

```dc
val enabled: bool = true;
```

条件表达式必须是 `bool`，整数不能隐式作为条件：

```dc
if enabled {
    println("enabled");
}

if 1 { } // 编译错误
```

### 5.4 字符和字符串

`char` 表示一个 Unicode 字符，`string` 表示不可变的 UTF-8 字符串。

```dc
val initial: char = 'D';
val message: string = "Hello world!";
```

字符串使用 `==`、`!=` 按 UTF-8 字节内容比较。`length(message)` 返回 UTF-8 字节数。MVP 不提供字符串索引或切片，以免引入落在码点中间的无效边界。

第一阶段至少支持以下转义字符：

```text
\n  换行
\r  回车
\t  制表符
\\  反斜杠
\"  双引号
\'  单引号
\u{1F600} Unicode 码点
```

## 6. 变量和常量

使用 `var` 定义可变变量，使用 `val` 定义不可变变量：

```dc
var count = 1;
count = 2;

val limit = 10;
limit = 20; // 编译错误
```

变量可以显式标注类型，也可以由初始化表达式推断类型：

```dc
var count: i32 = 1;
val name = "knift";
```

第一阶段要求所有变量在声明时初始化，不允许仅声明而不赋值：

```dc
var count: i32; // 编译错误
```

同一作用域内不能重复定义同名变量。内层作用域允许遮蔽外层变量；编译器当前没有警告机制，遮蔽会静默生效：

```dc
val value = 1;

if true {
    val value = 2; // 允许，当前无警告
}
```

`val` 表示整个值不可变。通过 `val` 声明的数组不能修改元素。

## 7. 数组和切片

定长数组类型写作 `[元素类型; 长度]`：

```dc
var numbers: [i32; 2] = [1, 2];
numbers[0] = 10;
```

编译器可以从数组字面量推断类型和长度：

```dc
val numbers = [1, 2, 3]; // [i32; 3]
val zeros = [0; 10];     // 10 个 i32
```

数组下标必须是整数。编译器会在编译期拒绝已知越界的常量下标，动态下标越界会在运行时以固定文本终止程序；运行时错误暂不带源码位置。

当前支持任意可布局的非 `Unit` 元素（含结构体/枚举）的一维非空数组，并采用按值传参和返回语义。嵌套数组、数组整体比较和直接格式化留待后续版本。

历史草案曾将切片写作 `[T]`；M14 已实现的当前语法为 `[]T`，只读形式为 `[]const T`。切片是对连续数组数据的非拥有视图，这里的“借用”不表示存在借用检查器。以下仅示意函数签名，省略函数体：

```dc
fn sum(values: []i32): i32 {
    // 函数体略
}
```

## 8. 运算符和表达式

支持以下基础运算符：

| 类别 | 运算符 |
| --- | --- |
| 算术 | `+` `-` `*` `/` `%` |
| 比较 | `<` `<=` `>` `>=` |
| 相等 | `==` `!=` |
| 逻辑 | `!` `&&` `||` |
| 赋值 | `=` `+=` `-=` `*=` `/=` `%=` |
| 转换 | `as` |

`&&` 和 `||` 使用短路求值。函数参数和表达式操作数按从左到右的顺序求值。

运算符优先级从高到低为：

1. 调用、下标和分组：`()`、`[]`
2. 一元运算：`!`、一元 `-`
3. 乘除和取余：`*`、`/`、`%`
4. 加减：`+`、`-`
5. 比较：`<`、`<=`、`>`、`>=`
6. 相等：`==`、`!=`
7. 逻辑与：`&&`
8. 逻辑或：`||`
9. 赋值：`=`、`+=`、`-=`、`*=`、`/=`、`%=`

条件选择使用 `if/else`；三元表达式 `?:` 未实现。不支持 `++` 和 `--`，使用复合赋值可以避免前置、后置自增的求值歧义：

```dc
i += 1;
```

### 8.1 赋值求值顺序

赋值只作为语句存在，不是可嵌套的表达式。目标位置与右值的求值顺序固定为：

1. 简单赋值先求目标位置（`name`、`name[index]`、`name.field`、`name->field`），包含索引/指针求值和必要的边界检查；有副作用的目标表达式只求值一次。
2. 再求右值。
3. 最后只写入目标子对象。右值执行期间通过别名对该对象其他字段/元素的修改保留，不会被整值写回覆盖。

复合赋值按同样顺序，但在求右值之前先从目标位置读取一次旧值，再计算 `旧值 op 右值`，最后只写回目标。指针、索引和切片描述符在求右值前选定；右值通过别名重绑定目标指针或切片描述符，不改变已经选定的地址。整体对象赋值（`name = value`）仍是按值复制。

```dc
struct Pair { x: i32, y: i32 }

fn mutate(p: *Pair): i32 {
    p->y = 9;
    return 7;
}

fn main() {
    var p = Pair(1, 2);
    p.x = mutate(&p);
    println("{} {}", p.x, p.y); // 7 9
}
```

## 9. 控制流

### 9.1 条件语句

`if` 的条件不强制添加括号；括号仍可作为普通的表达式分组使用。

```dc
if score >= 60 {
    println("passed");
} else {
    println("failed");
}
```

### 9.2 `loop` 循环

`loop` 创建无限循环，通过 `break` 退出：

```dc
var i = 0;

loop {
    if i >= 5 {
        break;
    }

    println("i: {}", i);
    i += 1;
}
```

### 9.3 `while` 循环

```dc
var i = 0;

while i < 5 {
    println("i: {}", i);
    i += 1;
}
```

### 9.4 `for` 循环和范围

半开范围 `start..end` 不包含结束值，闭合范围 `start..=end` 包含结束值：

```dc
for i in 0..5 {
    // 依次得到 0、1、2、3、4
}

for i in 0..=5 {
    // 依次得到 0、1、2、3、4、5
}
```

数组遍历：

```dc
val numbers = [1, 2, 3];

for number in numbers {
    println("{}", number);
}
```

`break` 和 `continue` 只能出现在循环中。第一阶段不支持带标签的循环跳转。

## 10. 函数

使用 `fn` 定义函数，参数必须标注类型：

```dc
fn add(a: i32, b: i32): i32 {
    return a + b;
}
```

无返回值函数省略返回类型：

```dc
fn greet(name: string) {
    println("Hello, {}", name);
}
```

有返回类型的函数必须保证所有可达路径都返回相应类型的值：

```dc
fn max(a: i32, b: i32): i32 {
    if a > b {
        return a;
    }

    return b;
}
```

不支持函数重载、默认参数和可变参数；泛型函数见 18.5。同一模块内不能定义两个同名函数。

## 11. 格式化输出

标准库提供 `print` 和 `println`：

```dc
print("Hello");
println(" world!");
println("Hello, my name is {}", "knift");
println("{} + {} = {}", 1, 2, 3);
```

每个 `{}` 对应一个格式化参数。占位符数量和参数数量不一致时产生编译错误：

```dc
println("{} {}", 1); // 编译错误：缺少参数
println("{}", 1, 2); // 编译错误：参数过多
```

使用 `{{` 和 `}}` 输出字面量花括号：

```dc
println("{{}}"); // 输出 {}
```

第一阶段支持格式化整数、浮点数、布尔值、字符和字符串。数组格式化可以在后续版本中增加。

## 12. 模块、导入和可见性

> 历史示例说明：本节保留早期自建 `src/std/` 和 `use std.math` 示例，用于解释模块路径规则，不可直接作为当前用户项目模板。M14 起 `std` 是编译器保留身份，用户模块不得占用；当前工程可将这些示例中的 `std` 一致改为 `mathutil`，参见已迁移的 [`examples/m7`](../examples/m7/)。其余设计规则不因这次名称迁移而改变。

每个项目使用 `src/` 作为源码根目录。`src` 只用于组织项目，不属于模块名称。

```text
project/
└── src/
    ├── main.do
    ├── helper.do
    ├── std/
    │   └── math.do
    └── app/
        └── service.do
```

直接位于 `src/` 下的 `.do` 文件属于项目根模块，可以省略 `pkg`：

```dc
// src/main.do
use std.math;

fn main() {
    println("{}", math.min(1, 2));
}
```

`src/main.do` 和 `src/helper.do` 位于同一个根模块。根模块中的声明默认可以互相访问，但仍不能重复定义同名符号。

`src` 子目录中的源码必须在第一条有效语句中声明 `pkg`。`pkg` 只需与文件相对于 `src` 的目录路径一致，文件名不参与声明；文件自身的模块名由「目录路径 + 文件名」推导：目录分隔符替换为 `.`，再追加去掉 `.do` 的文件名。

```text
src/std/math.do        -> pkg std;        (模块 std.math)
src/app/service.do     -> pkg app;        (模块 app.service)
src/net/http/client.do -> pkg net.http;   (模块 net.http.client)
```

例如 `src/std/math.do` 必须以以下声明开头：

```dc
pkg std;
```

如果子目录文件省略 `pkg`，或者声明的目录与所在目录不一致，编译器必须报错。`src` 本身永远不会出现在模块名中。

导入整个模块后，通过模块名访问公开成员：

```dc
use std.math;

val result = math.min(1, 2);
```

也可以精确导入成员：

```dc
use std.math.min;

val result = min(1, 2);
```

还可以只导入包前缀（目录），再用完整模块路径访问成员：

```dc
use std;

val result = std.math.min(1, 2);
```

后续可以增加显式的批量导入：

```dc
use std.math.{min, max, abs};
```

第一阶段不支持 `use std.*` 通配符导入，以避免名称冲突和符号来源不明确。

默认声明仅在当前模块内可见，添加 `pub` 后可以被其他模块访问：

```dc
fn internal_helper() {
    // 仅当前模块可见
}

pub fn min(a: i32, b: i32): i32 {
    if a < b {
        return a;
    }
    return b;
}
```

导入不存在或非公开的名称、重复导入同名符号时，编译器给出明确错误。模块间允许循环导入；模块没有初始化顺序，循环导入不会引入未定义行为。

## 13. 语句和分号

变量声明、赋值、函数调用、`return`、`break` 和 `continue` 等简单语句以分号结束：

```dc
val value = add(1, 2);
println("{}", value);
return value;
```

函数、`if`、`loop`、`while` 和 `for` 的代码块结尾不需要分号：

```dc
if value > 0 {
    println("positive");
}
```

## 14. 错误处理和运行时行为

编译器错误至少应包含：

- 文件路径、行号和列号。
- 出错的源码片段。
- 简短、明确的错误原因。
- 在可以准确判断时给出修复建议。

例如：

```text
error: cannot assign to immutable variable `limit`
 --> app/main.do:8:5
  |
8 |     limit = 20;
  |     ^^^^^ `limit` was declared with `val`
```

第一阶段以下情况会安全终止程序，而不是继续执行未定义行为：

- 整数除以零。
- 整数运行时溢出。
- 数组下标越界。
- 标准库无法完成的不可恢复操作。

语言不提供隐式 `null`；裸指针的 `null` 只能与指针类型比较。`Option<T>` 表达“可能没有值”，`Result<T, E>` 表达可恢复错误，二者作为最小 prelude 自动可用，也可 `use std.Option;` / `use std.Result;` 显式导入。

## 15. 完整示例

以下保留 M7 时期自建 `std` 的历史示例，不是当前可直接运行的工程。M14 起 `std` 已保留；当前可运行工程已迁移为 `mathutil`，见 [`examples/m7`](../examples/m7/)。

应用入口 `src/main.do` 位于源码根目录，因此不需要 `pkg`：

```dc
use std.math;

fn main() {
    val a = 1;
    val b = 2;

    println("min({}, {}) = {}", a, b, math.min(a, b));
    println("max({}, {}) = {}", a, b, math.max(a, b));
}
```

标准库模块 `src/std/math.do` 位于子目录，因此必须声明 `pkg std;`：

```dc
pkg std;

pub fn min(a: i32, b: i32): i32 {
    if a < b {
        return a;
    }
    return b;
}

pub fn max(a: i32, b: i32): i32 {
    if a > b {
        return a;
    }
    return b;
}
```

## 16. 第一阶段实现范围

第一阶段（MVP）包括：

- 单行和多行注释。
- 基础数值类型、`bool`、`char` 和 `string`。
- `var`、`val` 和局部类型推断。
- 定长数组和下标访问。
- 算术、比较、逻辑、条件和赋值表达式。
- `if`、`loop`、`while`、`for`、`break` 和 `continue`。
- 函数定义、调用和 `return`。
- `pkg`、`use` 和 `pub`。
- `main` 程序入口。
- `print` 和 `println`。
- 生成并链接本机可执行文件。

第一阶段的历史架构、里程碑和验收标准参见[编译器实现指南](compiler-implementation.md)，不是当前待办。最初的总体实现顺序为：

1. 词法分析器和带源码位置的 Token。
2. 表达式、语句、函数和模块的语法分析器。
3. AST、作用域、名称解析和错误诊断。
4. 基础类型检查和类型推断。
5. 控制流检查和返回路径检查。
6. 自定义中间表示。
7. Cranelift 代码生成和系统链接。
8. 最小运行时与标准库。
9. 端到端编译测试和错误信息测试。

## 17. 第二阶段：工具链与分发（M9-M12）

第二阶段（M9-M12）不扩展语言语法，目标是把已可用的 MVP 变成能够在主流平台稳定安装、构建和发布的编译器。它属于**工程与分发**范畴，不属于语言设计，因此本语言设计文档不描述其语法细节；完整规划与验收标准见[实现路线图](roadmap.md)，实际能力见[已实现功能参考](implemented-features.md)。

四个里程碑的主题：

- **M9** 项目清单 `dolphin.toml`：以包坐标、源码目录、多可执行目标和构建选项稳定描述项目。
- **M10** 平台与工具链抽象：`TargetPlatform` 统一目标三元组、目标文件后缀、可执行文件后缀与 ABI，新增 `dc env`。
- **M11** Windows x86_64 原生支持：MSVC ABI、`.obj`/`.exe`、Windows 运行时与 CI。
- **M12** 自包含工具链与发布：内嵌预编译运行时、接入 `rust-lld` 链接、`--system-linker` 回退、三平台发行包与冒烟测试。

## 18. 第三阶段语言草案

> 当前源码已实现到 M20。18.1-18.3 的结构体、枚举与 match、18.4 的内存/C 互操作、18.5 的用户泛型、源码标准库、lib 项目与库包分发（M15-A–F）均已实现；M20 的项目级诊断/共享分析/LSP/格式化发现与调试器实测见[已实现功能参考](implemented-features.md) 18.5。当前行为见[已实现功能参考](implemented-features.md)，[M14](proposal-m14-memory-model.md) / [M15](proposal-m15-generics-stdlib.md) v2 保留为已完成规格；下一步见 [M21 规划](plan-m18-plus.md)。

### 18.1 结构体（M13 已实现）

结构体把同名字段聚合为一个复合值类型，采用位置构造与字段访问。历史上 M13 只支持局部值和整体赋值，不支持 struct/enum 函数参数/返回及字段写入；这些限制已在 M14 v2 解除，按值复制、字段写入和显式指针语义均已实现。

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

- 字段在结构体内不得重名。
- 访问不存在的字段是编译期错误。
- 结构体字段与既有类型系统一致，支持基础类型、数组及其它结构体。
- 历史分工：M13 未决定结构体跨函数传递时的复制/移动行为；M14 已确定并实现按值复制，不引入隐式 move。

### 18.2 枚举（M13 已实现）

枚举描述一组有限的取值，枚举项可携带数据：

```dc
enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    None,
}
```

- 同一枚举内的枚举项名不得重复。
- 枚举项可携带零个或多个参数，参数类型在枚举声明处固定。
- 枚举项通过 `Shape.Circle(...)` 构造；无参枚举项写作 `Shape.None`。
- 枚举的穷尽性与解构语义由 `match`（见 18.3）定义。

### 18.3 match 模式匹配（M13 已实现）

`match` 是表达式式控制流，用于按枚举取值分派：

- `match` 在表达式位置使用（如 `val x = match ...`、`return match ...`），各分支求值为同一类型；当前实现不支持把 `match` 作为独立语句，分支体也不能是 `println` 等语句。
- 对枚举穷尽匹配：缺失分支或冗余分支产生源码级诊断。
- 支持携带数据枚举项的解构绑定，以及通配符 `_` 分支（`_` 也可作为解构绑定位忽略单个字段）。

```dc
enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    None,
}

fn main() {
    val shape = Shape.Circle(2.0);
    val area = match shape {
        Shape.Circle(r) => 3.14 * r * r,
        Shape.Rectangle(w, h) => w * h,
        Shape.None => 0.0,
    };
    println("{}", area);
}
```

### 18.4 内存与 C 互操作（M14 v2 已实现）

采用 Zig 式手动申请/释放：`std.mem`、原始指针、切片与块级 `defer`。不引入 GC、RC、隐式 move、自动析构或 Rust 式生命周期参数。只读指针/切片约束通过该视图的写入权限，不构成借用检查。

当前 `string` 是只读 UTF-8 视图，拥有型动态字符串在 M15 用 `String` 表达；继续禁止字符串 `+`，拼接使用显式分配 API。C 函数通过 `extern "C"` 声明按目标 ABI 调用。M14 已实现 intrinsic 类型实参及 std.mem 入口，不依赖 M15 用户泛型/方法系统。精确语法、历史基线衔接和验收见 [M14 实现规格](proposal-m14-memory-model.md)。

### 18.5 泛型、标准库与库包（M15 v2：A–F 已实现）

泛型使用工作队列单态化，trait 只提供静态约束。具名类型（`struct`/`enum`）的类型参数可带单 trait 约束，约束在实例化时检查并可用 `T::Item` 关联类型作为字段或 payload 类型（H18-04）。impl 目标实参在声明处校验：支持具名类型的非泛型 impl 与目标实参与 impl 参数按位置一一对应的泛型 impl（参数可改名），其余形式明确拒绝（H18-05）。`Vec`、`String`、`Option`/`Result`/`Iterator` 等标准库已作为随编译器分发的 Dolphin 源码实现，拥有型资源显式 `deinit`，视图默认零分配。lib 项目已能生成经编译验证的源码型 `.dlib` ZIP，按 `group:name:version` 发布/消费，支持传递依赖、校验和、锁文件及离线构建。完整规格见 [M15 实现规格](proposal-m15-generics-stdlib.md)。

## 19. 后续候选能力

以下能力仍需逐项设计，执行顺序见 [M18-M21 计划](plan-m18-plus.md)：

- 闭包、动态多态和异步。
- 依赖版本范围求解、闭源二进制库与稳定 ABI。
- 完整仓库网站后端。
- Cranelift 调试信息、Windows/PDB 与完整变量调试；LLVM 后端 Debug 下的 Unix 最小 DWARF（编译单元、函数与行表）已在 M17 实现。
- WebAssembly 等额外后端；可选 LLVM 后端已在 M16 实现。

新增特性前应先明确语法、类型规则、运行时行为、错误信息以及与既有特性的交互，避免仅根据示例代码确定语言语义。
