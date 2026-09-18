# M15 实现规格：泛型、优雅标准库与库包发布（M13 基线）

> 状态：目标规格 v2 已按 R09–R21 实现并验收（M15-A…F 全部完成）；源码标准库、lib 包图、确定性 `.dlib`、仓库/缓存/锁与发布均已落地。
>
> 历史前置：当时先按 [M14 实现规格](proposal-m14-memory-model.md)完成内存、布局、清理与 C ABI。该阶段已完成，不是要求当前执行者重做 M14。
>
> 目标：写 lib → 构建 `.dlib` 压缩包 → 发布到网站 → 用坐标声明依赖 → 构建本机程序。
>
> 本文保留 M15 v2 的语义与验收合同；旧批次见已归档的[分步实施指南](plan-m14-m15-rework.md)。当前后续任务见 [M18-M21 交接指南](plan-m18-plus.md)，不得直接执行本篇从 M13 起步的历史指令。

## 1. 实现原则与边界

历史执行背景：当次重做从 M13 提交 `9e6f4c6` 开始，旧 M14/M15 当时已清理，按 v2 目标逐步新增。下文的“已有”“待新建”及旧 `src/` 路径仅描述这个历史起点；当前已完成 M17 并采用 `crates/` workspace，不能因旧路径不存在而创建第二套模块。本文示例仍须结合当前功能参考和测试核对；已知语义缺口由 M18 修复，不因历史完成标记而忽略。

### 1.1 必须交付的三个闭环

1. **语言闭环**：泛型函数/类型、方法、静态 trait、关联类型可以跨包工作。
2. **API 闭环**：`Vec<T>`、拥有型 `String`、字符串视图、迭代器与 `Result` 有统一、可预测的手动释放规则。
3. **分发闭环**：lib 不需要 main；库作者产出单个 `.dlib`；应用声明 Maven 风格坐标，工具自动下载、校验、解析传递依赖并构建。

不能仅完成泛型语法就将 M15 标为完成，也不能因为文件 I/O 或网络标准库尚未完成而阻塞包管理。**dc 是 Rust 程序，下载依赖使用 Rust HTTP 客户端，不依赖 Dolphin 自己先实现 HTTP/TLS。**

**小上下文执行方式**：先读第 1、10 节选定阶段，再按需加载：A→第 2、3 节；B→第 3–5 节及 M14 的内存契约；C→第 6 节和第 2 节包身份规则；D→第 7 节；E→第 8 节；F→第 9 节与跨平台验收。第 11 节按验收编号取用，引用到其他规则时补读；不要一次给模型派发整个 M15。

### 1.2 M13 当时的基础与待新增模块（历史）

| 入口 | 当前状态与实现任务 |
| --- | --- |
| `src/monomorphize.rs`（待新建） | 从零实现模板、作用域推断、工作队列、稳定身份和递归检测；不恢复历史一次扫描实现 |
| `src/methods.rs`（待新建）；[ast.rs](../src/ast.rs)、[parser.rs](../src/parser.rs)（已有） | 新增 trait/impl/方法/关联类型声明、接收者、签名检查和解析；M13 无这些 AST 节点 |
| [lower.rs](../src/lower.rs)、[ir.rs](../src/ir.rs)、[codegen.rs](../src/codegen.rs) | 复用具体类型 IR；补齐任意聚合值作为枚举 payload、切片元素与函数结果 |
| `src/stdlib/*.do`（待新建） | 在 M14 内建 std.mem 身份上新建 Option/Result/Iterator 与 Dolphin 源码容器/字符串库；M13 无内置 std.do |
| [manifest.rs](../src/manifest.rs) | 当前要求至少一个 bin，依赖只是保留字段；新增 lib、有效依赖与仓库配置 |
| [modules.rs](../src/modules.rs)、[lib.rs](../src/lib.rs)、[main.rs](../src/main.rs) | 从单包流程扩展为包图、库构建和 CLI |
| [tests/build.rs](../tests/build.rs)、[tests/cli.rs](../tests/cli.rs)、[tests/manifest.rs](../tests/manifest.rs) | 复用现有测试驱动，增加包管理 fixture |

以下是必须避免的历史设计误区，不代表当前源码仍有相应实现：

- “泛型仅替换 AST，IR/codegen 保证零改动”：可以复用具体类型 IR，但名称解析、布局、聚合值、符号与构建流程都可能需要修改。
- “TypeId 或 Option 可以打断递归内存布局”：TypeId 是编译器内部索引，Option 是内联枚举，均不等于运行时指针。
- “只扫描一遍原程序即可收集所有泛型实例”：实例化函数体还会请求新的实例，需要工作队列到不动点。
- “Vec 必须把 T 擦除成 []u8”：前置 M14 完成后提供 typed alloc/typed slice，`Vec<T>` 可直接持有 `[]T`。
- “需找回已清理的实现才能开工”：现有 M13 前端/后端和 v2 规格就是起点，新增所需能力，不恢复旧 API 兼容层。

## 2. 泛型：单态化，但实现必须完整

### 2.1 冻结语法

```dc
fn identity<T>(value: T): T { return value; }
struct Pair<T> { first: T, second: T }
enum Option<T> { Some(T), None }
enum Result<T, E> { Ok(T), Err(E) }

fn main() {
    val a = identity<i32>(3);
    val b = identity(a);              // 从变量类型推断，不限于字面量
    val p = Pair<i32>(a, b);
    val empty: Option<i32> = Option.None;
}
```

- 声明 `<T, E>`；类型实参 `Vec<i32>`；函数显式调用 `identity<i32>(x)`。
- 推断先统一参数类型与实参类型，再用期望返回类型补齐；冲突或仍未确定则报错并建议显式实参。不用猜测 `None` / `Err` 中缺失的类型参数。
- 支持变量、字段、调用结果、嵌套泛型参数和跨模块/跨包推断；词法作用域中的同名变量不可污染外层推断。
- 表达式中的 `<` 仍可表示比较：完整类型实参列表后接 `(`、关联函数 `::` 或枚举成员 `.` 时识别为泛型路径；不满足完整形态时回退比较解析。必须支持 `a < b`、`f<Vec<i32>>(x)`、`Vec<i32>::init()`、`Option<i32>.None`，嵌套类型的 `>>` 按类型上下文解析。
- 泛型是编译期机制，无装箱、类型字典、虚表或隐藏堆分配。
- 任意已支持的可布局类型都可作类型参数，包括 string、切片、结构体、枚举；必须扩展 M13 的 enum payload 表示以承载 string 等复合值，不能以基线限制回避 Result/String API。
- 首版不引入 `?T` 和 `?` 错误传播。写 `Option<T>` 与显式 `match`；C 空指针继续用 M14 的 nullable raw pointer。

### 2.2 实例化算法

顺序如下，不要求机械重写为另一套编译器架构：

```text
加载根包、依赖包、标准库源码
  → 包/模块名称解析与定义身份分配
  → 收集普通签名、泛型模板、trait/impl 表
  → 对具体入口和调用推断类型，生成实例请求
  → 工作队列：实例化、检查、发现新请求，直到队列为空
  → 具体类型布局、方法/迭代语法降低、具体类型 CFG IR
  → Cranelift 目标文件 + C 原生依赖链接
```

实例 key 必须包含：`(PackageId, ModuleId, DefinitionId, concrete_type_args)`。`PackageId` 包含规范化包坐标；同名函数、不同包、不同类型不能碰撞。实参类型用结构化类型身份，不能直接用源码展示字符串拼接去重。

实现状态至少有 `Queued / InProgress / Done`：先登记再展开函数体，递归调用已有实例时引用预声明符号；新实例入队。按稳定顺序处理队列和输出，不能依赖 HashMap 的随机迭代顺序生成符号。

- 同一个 key 全程序生成一次，多个模块或包使用同一个库泛型不会重复定义。
- 符号编码使用可逆的长度前缀编码，或规范序列化后使用稳定摘要并检测碰撞；不使用 Rust 默认 HashHasher 的结果作为发行符号。
- 支持普通递归和互递归。对 `f<T>` 无止境请求 `f<Pair<T>>` 这类扩张递归，限制实例链深度为 128、单次构建实例总数为 10000；超限给出实例链诊断，不允许栈溢出/panic。
- 模板内引用按**定义包**解析，不能被调用包的同名私有函数劫持。
- 模板声明时检查语法、类型参数、可独立解析的名字和 trait 声明；依赖具体 T 的操作在实例化时检查，错误同时显示定义位置和调用链。
- 不承诺所有未实例化泛型函数都已经完整类型验证；lib 打包也遵循这一边界，不能宣称源码通过打包就证明所有 T 均可用。

类型相关操作的边界：`twice<T>` 中的 `value + value` 可在实例化后按普通标量运算规则验证，`twice<i32>` 合法，`twice<string>` 因语言不支持字符串加法而失败；这不引入运算符重载。泛型体通过 T 调用契约方法则必须声明对应 trait 约束，不能仅凭实例恰巧有同名方法放行。未使用泛型体中的未知自由函数名称应在模板检查时即报错。

### 2.3 递归布局验收

```dc
struct Node<T> { value: T, next: *Node<T> }        // 合法，指针固定大小
struct Bad<T> { next: Bad<T> }                    // 非法，无限大小
struct AlsoBad<T> { next: Option<AlsoBad<T>> }     // 同样非法，Option 内联 payload
```

类型布局检测按“按值包含”建图，结构体字段、数组元素和枚举 payload 都是边；原始指针不是展开边。检测直接和间接环，打印字段路径。

## 3. 方法与静态 trait

### 3.1 接收者规则

```dc
struct Point { x: i32, y: i32 }

impl Point {
    pub fn origin(): Point { return Point(0, 0); }
    pub fn sum(self): i32 { return self.x + self.y; }
    pub fn set_x(self: *Self, value: i32) { self->x = value; }
    pub fn read_x(self: *const Self): i32 { return self->x; }
}
```

| 声明 | 调用行为 |
| --- | --- |
| 无 self | 关联函数 `Point::origin()` |
| `self`（省略类型） | `Self` 按值复制，不隐式分配 |
| `self: *Self` | `p.set_x(3)` 对可写且可寻址的 p 自动取地址；临时值和 val 对象拒绝 |
| `self: *const Self` | 对可寻址对象自动取只读地址；不复制大对象 |

自动取址仅是**方法接收者糖**，等价于显式传 `&p`，不引入借用检查或自动生命周期延长。接收者本来就是指针时使用该地址，最多一层解引用；表达式只求值一次。

指针字段保持 `p->x`，方法统一 `p.method()`。`impl<T> Vec<T>` 必须是真正的参数化 impl，而不是把 `Vec<T>` 当成一个字符串名字。

### 3.2 trait 与关联类型

```dc
trait Iterator {
    type Item;
    fn next(self: *Self): Option<Self::Item>;
}

struct Counter { current: i32, end: i32 }

impl Iterator for Counter {
    type Item = i32;
    fn next(self: *Self): Option<i32> {
        if self->current >= self->end { return Option.None; }
        val value = self->current;
        self->current += 1;
        return Option.Some(value);
    }
}
```

- trait 只有签名和关联类型，无默认方法、动态分派或运行时 trait 对象。
- 泛型约束首版每参数一个，例如 `fn count<I: Iterator>(iter: I): usize`；多个约束、trait 继承和泛型 trait 延后。
- `Self` 是实现类型，不是 trait 名。`Self::Item` 和受约束参数的 `I::Item` 必须能解析。
- impl 必须提供全部方法和关联类型，参数数量、类型、接收者可变性、返回值须一致；缺失/重复/多余绑定都有诊断。
- 同一具体“trait + 类型”只能有一个 impl；首版禁止 blanket impl 和重叠泛型 impl。固有方法只能在类型所属包定义；trait impl 要求 trait 或类型至少一个属于当前包。
- 固有方法优先；trait 方法只有 trait 被 `use` 引入或来自类型参数约束时参与解析。同名 trait 方法存在歧义则报错，不随机选择。
- 固有方法默认私有，`pub` 对外；trait 方法按 trait 的可见性提供实现，不能把公开契约方法降为私有。

“禁止 blanket impl”指拒绝 `impl<T> Trait for T` 这种任意接收者实现；允许标准库需要的 `impl<T> Iterator for SliceIter<T>`，只要不存在重叠。普通泛型 impl 及其方法的类型参数不能在方法提升时清空。首版方法可使用所属 impl 的 T；方法另声明独立泛型参数的语法明确延后，避免模型自行实现半套泛型方法。

## 4. 标准库：少量原语，真实 Dolphin 容器

### 4.1 模块与 API 风格

M15 新建随编译器分发的 Dolphin 源码标准库，沿用 M14 为内建 std.mem 建立的保留身份 `dolphin:std:<compiler-version>`，一次构建只注入一次。源码按以下模块组织：

| 模块 | 首版内容 |
| --- | --- |
| `std` | 新增公开 Option、Result、Iterator，支持 `use std;` |
| `std.mem` | M14 分配、释放、布局、内存视图与复制原语 |
| `std.collections` | `Vec<T>` 与迭代器 |
| `std.text` | 拥有型 String、UTF-8 查询、concat |
| `std.ffi` | CString 与 C 字符串转换 |

最小 prelude 仅提供 `Option`、`Result`、`Iterator` 三个名字，它们分别指向 std 中的同一定义；不另造内建枚举。允许用户模块中的显式定义或导入遮蔽 prelude，编译器展开 for 时仍通过 std 的定义身份引用协议。其他标准库类型/函数必须显式 use。

M13 的 `src/std/` 是用户模块；M14 首次引入内建 std.mem 时负责迁移仓库示例。M15 扩展同一个内建 std 身份，不重新创建第二个 std 或再次迁移已完成的示例。其他项目与保留名字冲突时给出诊断，不能静默覆盖用户源码。

API 命名统一：

- `init` / `deinit`：建立 / 释放拥有型资源；deinit 不自动调用。
- `view` / `as_slice` / `iter`：零分配借用视图，不得释放，失效条件写在文档中。
- `clone`：显式分配并复制；不使用含糊的 `copy` 表示深拷贝资源图。
- `len` / `is_empty` / `get`：查询不分配。
- `push` / `reserve`：允许扩容，必须明示可能使已有视图失效。
- 不强求所有方法链式调用；修改方法返回 Unit，避免“返回容器副本”造成两份释放责任。
- 错误用 `Result<T,E>` 与 `match`；内存不足仍沿用 M14 的确定性 102，不再另造可恢复 OOM 体系。

编译器只保留内存、布局、视图、UTF-8 校验、现有输出、C ABI 等必要底座。Option、Result、Vec、String、Iterator 的业务逻辑写 Dolphin 源码，禁止按类型名在 codegen 内实现容器。

### 4.2 `Vec<T>`：必须纳入本次 M15

内部表示：`storage: []T`（长度是容量）、`used: usize`（已初始化元素数量）。切片仍只有 ptr/len 两个字段，不给所有切片增加 cap。

| API | 语义 |
| --- | --- |
| `Vec<T>::init(): Vec<T>` | 空容器，零分配 |
| `Vec<T>::with_capacity(n: usize): Vec<T>` | 显式预分配 n 个 T 的未初始化存储 |
| `v.len(): usize` / `v.capacity(): usize` / `v.is_empty(): bool` | 不分配 |
| `v.push(value: T)` | 按值复制到末尾；必要时扩容 |
| `v.reserve(additional: usize)` | 确保容量至少为 `used + additional`，检查溢出 |
| `v.get(index: usize): Option<T>` | 返回元素副本，越界返回 None |
| `v.set(index: usize, value: T)` | 替换元素，越界 101；不自动销毁旧元素 |
| `v.pop(): Option<T>` | 移除并返回副本，空容器返回 None |
| `v.as_slice(): []const T` | 只读的已初始化区域，不含容量尾部 |
| `v.as_mut_slice(): []T` | 可写的已初始化区域，需要可写 receiver |
| `v.iter(): SliceIter<T>` | 零分配只读迭代器，按值产出 T |
| `v.clone(): Vec<T>` | 分配独立 backing buffer，逐元素复制 T；不递归 clone 元素资源 |
| `v.clear()` | used 置零，不释放容量，不自动销毁元素 |
| `v.deinit()` | 释放 backing buffer，将本对象重置为空 |

接收者统一规定：`push/reserve/set/pop/as_mut_slice/clear/deinit` 用 `self: *Self`；其他实例方法用 `self: *const Self`。关联函数没有 self。String/CString 的 deinit 同样用可写接收者，view/ptr/clone 用只读接收者。

扩容算法固定为：初次容量至少 4；后续 `max(required, old_capacity * 2)`，乘法/加法溢出时报 102。分配新 `[]T`，只复制 `used` 个已初始化元素，释放旧完整 storage，然后替换描述符。没有 realloc 或 reinterpret cast 的前置要求。

先计算 checked `required = used + additional`，若 required 不超过现有容量立即返回，不能为 `reserve(0)` 无故分配。查询、get、as_slice、iter、clone 采用 `self: *const Self`；push/reserve/set/pop/as_mut_slice/clear/deinit 采用 `self: *Self`。push/reserve/set/clear/deinit 返回 Unit；pop 返回 Option，as_mut_slice 返回可写切片。

```dc
use std.collections.Vec;

fn main() {
    var numbers = Vec<i32>::init();
    defer numbers.deinit();
    numbers.push(10);
    numbers.push(20);
    for value in numbers.iter() { println("{}", value); }
}
```

生命周期契约：

- reserve/push 导致重分配、deinit 都会使旧切片、元素指针和迭代器失效；迭代中禁止修改容器。
- `var other = numbers` 只复制描述符，**不是独立容器**；不能对两个副本各自 deinit，也不能一方扩容后继续使用另一方。需要独立容器用 clone。
- deinit 对**同一个已重置对象**重复调用是无操作；不能据此声称容器的浅拷贝也可重复释放。
- `Vec<String>` 的 deinit 只释放 Vec 存储。调用者先遍历元素显式 deinit，再释放 Vec。pop 后资源责任由调用者按约定接管；clear/set 前须处理将被丢弃的元素资源。
- 容器 backing 字段必须私有。M15 增加结构体字段 `pub` 可见性：默认模块私有，私有字段禁止外部位置构造/访问；普通所有字段公开的数据类型仍可位置构造。impl 使用类型定义模块权限。

已有 M13 跨模块位置构造的数据类型需为原本公开的字段补 `pub`；这是明确的语法迁移，应更新对应正例，不能静默让旧调用失去能力。销毁 `Vec<String>` 时通过 `var item = ...` 得到可写局部副本后 deinit，或对可写元素 place 调用 deinit；普通 `for` 的迭代变量不可重赋，不要直接在不可写迭代变量上调用指针 receiver。

### 4.3 String 与 CString

`string` 是 M14 的 UTF-8 只读视图；`std.text.String` 是可释放的拥有型 UTF-8 缓冲。两者不能混为一个可 free 的 fat pointer。

| API | 分配 / 返回责任 |
| --- | --- |
| `String::from(s: string): String` | 复制并拥有新字节，调用者 deinit |
| `text.concat(a: string, b: string): String` | 一次分配拼接，调用者 deinit |
| `owned.view(): string` | 零分配视图，owned 释放后失效 |
| `owned.clone(): String` / `owned.deinit()` | 显式独立复制 / 释放并重置 |
| `text.trim(s: string): string` | 去除首尾 ASCII 空白（09–0D、20），返回原缓冲视图 |
| `text.substring(s, start: usize, end: usize): Result<string, TextError>` | 半开字节区间；检查范围与 UTF-8 边界，无分配 |
| `text.starts_with/ends_with/contains(s, part): bool` | 内容查询，无分配 |
| `text.from_utf8(bytes: []const u8): Result<string, TextError>` | 校验后返回视图，不复制；失败返回错误，不调用 trap 型转换 |
| `CString::from(s: string): Result<CString, CStringError>` | 复制并追加 NUL，内部含 NUL 返回错误 |
| `cstring.ptr(): *const c_char` / `cstring.deinit()` | C 只读字符串指针 / 释放并重置；C 不得保存到 deinit 之后 |

`TextError` 至少区分 `InvalidUtf8 / InvalidBoundary / OutOfBounds`，`CStringError` 至少有 `InteriorNul`。前置 M14 实现的 `string.from_bytes` 是 trap 型低层入口；高层 Result 转换先调用 `mem.is_valid_utf8`，仅验证成功后建立视图。

```dc
use std.text;

fn main() {
    var greeting = text.concat("hello, ", "Dolphin");
    defer greeting.deinit();
    println("{}", greeting.view());
}
```

首版 String 不提供会改变字节的原地 append；需要构建复杂字符串可先使用 Vec<u8>，验证 UTF-8 后显式复制成 String。避免在首批 API 中同时引入多种视图失效规则。

String/CString 的 view/ptr/clone 使用只读指针 receiver，deinit 使用可写指针 receiver；deinit 将同一对象重置为空，其他浅拷贝仍不能再读/销毁。CString 至少保留原始 `[]u8` 用于 free；导出指针通过 `*const Unit` 中转为 `*const c_char`，不把 C 指针反过来当成释放依据。CString 的长度加一、concat 的长度和均需 checked，溢出按 M14 的 102 处理。

直接调用常见 C 接口的完整目标例子：

```dc
use std.ffi.CString;

extern "C" { fn puts(text: *const c_char): c_int; }

fn main() {
    var text = match CString::from("hello from Dolphin") {
        Result.Ok(value) => value,
        Result.Err(error) => { return 1; },
    };
    defer text.deinit();
    puts(text.ptr());
    return 0;
}
```

## 5. 迭代器：统一协议，限定语法糖范围

首版选择最小可实现规则：**`for x in iterator_value` 要求表达式类型实现 `Iterator`。** M13 只有数组/范围 for；M15 新增协议路径，不引入隐式 into_iter 查找或仅凭同名 next 方法的鸭子类型协议。

```text
for value in expression { body }
=> 在循环外建立隐藏可写变量 it = expression（只求值一次）
   loop {
       match it.next() {
           Option.Some(value) => { body },
           Option.None => break
       }
   }
```

- Vec 用 `v.iter()`，切片用 `s.iter()`；`SliceIter<T>` 是标准库类型，持有 `[]const T` 和当前索引，next 按值复制 T。
- `s.iter()` 可作为切片的固定适配 intrinsic 降低为标准库构造调用；它不实现循环或容器算法。
- 保留旧 `for x in array_expr`：数组表达式求值一次，隐藏数组局部存储覆盖整个循环，通过只读切片适配到 SliceIter；不能对临时数组创建立即悬垂的视图。
- 保留 `for i in start..end` / `..=`：降低为标准库 `Range`（i32 端点）/ 对应闭区间状态，端点求值一次。闭区间到 i32 最大值时不能多加一次导致溢出。
- 范围首版仍是 for 专属语法；普通范围值显式用 `Range::exclusive(start, end)` / `Range::inclusive(start, end)`，Range 为 i32 迭代器，避免要求常量泛型或多 trait 约束。
- 循环体 defer 每轮清理，break/continue 不跳过 M14 约定的清理；隐藏 iterator 本身不自动 deinit。拥有资源的用户迭代器由用户在外层显式管理。

编译器允许上述数组/切片/范围**语法适配**，但 next 的业务实现必须来自同一 Iterator 协议；不能再为 Vec 或特定元素类型写专用循环后端。

## 6. lib 项目与清单

### 6.1 库项目

```toml
# mathlib/dolphin.toml
[package]
group = "org.example"
name = "mathlib"
version = "1.0.0"
source = "src"

[lib]
path = "src/lib.do"
```

```dc
// mathlib/src/lib.do：根模块，无 pkg
pub fn twice<T>(value: T): T { return value + value; }
pub fn add(a: i32, b: i32): i32 { return a + b; }
```

规则：

1. 一个包最多一个 `[lib]`，可以同时有多个 `[[bin]]`；至少声明一种目标。
2. lib 不需要 main，根模块中叫 main 的普通函数也不会变成入口。构建 lib 不生成启动入口，也不链接可执行文件。
3. `[lib].path` 必须位于 `[package].source` 内且是其直接子文件；它标记根模块入口，其他根目录 `.do` 按既有规则组成根模块。
4. 同包 bin 入口单独选入编译单元；lib 扫描排除所有 `[[bin]].path`，bin 扫描排除其他 bin 入口，防止多个 main 混入。共享函数放到其他源码文件；禁止库引用被排除的 bin 专属定义。
5. 库的 API 是根模块及子模块的 pub 定义，跨包只允许公开访问。lib 和同包 bin 使用同一 PackageId，定义只能装入一次。
6. 对外签名不能泄露该包私有类型。泛型实现可以调用私有 helper，消费端实例化必须保留定义包解析和隐私规则。

### 6.2 应用与依赖

```toml
[package]
group = "org.example"
name = "app"
version = "0.1.0"

[[bin]]
name = "app"
path = "src/main.do"

[repositories]
default = "https://packages.example.org/dolphin"

[dependencies]
math = "org.example:mathlib:1.0.0"
codec = { coordinate = "org.example:codec:2.0.0", repository = "default" }
# 开发期可以替换 math 为：math = { path = "../mathlib" }
```

```dc
use math.add;
use math.twice;

fn main() {
    return add(twice<i32>(20), 2); // 42
}
```

- 左边是本包使用的别名，右边是发布坐标；保留 M9 已预留的字符串简写。
- 依赖别名是合法语言标识符，不允许 `std`，不能与本包顶层模块重名。`use math.add` 指向该依赖根模块；`use math.algorithms.sort` 指向其内部公开模块。
- 包坐标与源码 `pkg` 分离。库内部仍写相对目录 `pkg algorithms;`，无需把 group 或消费端别名写入源码。
- 别名是每包私有的解析环境；依赖包自己的别名不受根应用覆盖。应用不能直接 use 未声明的传递依赖。
- 同一包的多个别名可以指向同一个 PackageId，仅加载一次；同名类型来自不同包必须保持不同身份。
- 无清单的旧单文件/目录模式保留，但不支持远程依赖、lib 和发布。

## 7. `.dlib` 包：类似 JAR 的使用体验

### 7.1 冻结首版分发策略

**扩展名使用 `.dlib`，格式是 ZIP。** 借鉴 JAR 的“单文件库产物 + 坐标分发”，不产生 JVM `.class`，不引入虚拟机。Dolphin 最终仍生成本机代码。

为控制首版实现成本，v1 采用**源码模板分发 + 消费端统一编译**：

- `dc build --lib` 做库模式编译验证：普通函数完成类型检查和当前目标的对象代码生成；未实例化泛型按 §2.2 检查模板，已用实例正常编译。
- 本地生成的验证 object 放在 `target/lib/<name>.o`（Windows 为 `.obj`）；可包含外部引用，不做可执行链接。
- `.dlib` 打包规范化清单、完整库源码和明确声明的 C 原生文件；**v1 不把 Dolphin object 当作消费端链接输入**。消费端读取源码，在目标平台与应用一起单态化并生成本机代码。
- 这使未知 T 的泛型可跨包使用，也避免首版同时设计稳定二进制 ABI、序列化 AST/IR、跨目标机器码和重复实例去重。
- 这是“经编译验证的源码型库包”，不是隐藏源码的二进制库。闭源 Dolphin 二进制包、预编译缓存与稳定 ABI 明确延后；不得把源码压缩包宣传成一次编译跨平台运行的机器码。

**v1 没有第二条 Dolphin 二进制消费路径。** 以后再加目标专属预编译组件时须升级包格式，不能让实现模型自行任选源码/IR/object 混搭方案。

### 7.2 包内结构

```text
mathlib-1.0.0.dlib                # ZIP，根目录不额外嵌套 mathlib-1.0.0/
├── META-INF/
│   └── dolphin-package.toml     # 格式、坐标、编译器兼容、目标与原生文件摘要
├── dolphin.toml                # 规范化发布清单
├── src/
│   ├── lib.do
│   └── algorithms/sort.do
├── native/                     # 仅声明了 C 原生输入时存在
│   └── <target>/...
└── LICENSE                     # 若项目存在则附带
```

元数据示例：

```toml
format-version = 1
coordinate = "org.example:mathlib:1.0.0"
compiler-version = "0.1.0"      # 由实际 dc 版本生成，不要求项目版本相同
kind = "source"
targets = []                    # 空表示纯 Dolphin；native 包填有完整输入的目标列表

# 有 native 文件时，每个文件一个记录：
# [[native-files]]
# path = "native/x86_64-pc-windows-msvc/demo.lib"
# sha256 = "<64位小写十六进制摘要>"
```

兼容策略：首版要求消费端 dc 版本与 `compiler-version` **完全相同**；不匹配给出发布方版本和当前版本。以后放宽到语言版本范围另行设计，不承诺当前不稳定语言的跨版本兼容。原生目标不匹配同样在链接前诊断。

规范化规则：

1. 打包全部库源码，含泛型依赖的私有 helper；排除 bin 入口、target、.git 和未声明的文件，不使用“整个项目目录直接 zip”。
2. 发布清单只保留 package、lib、精确 dependencies、native。源码根统一为 `src`，lib/native 路径重写为包内相对路径；不保留 build.output、bin、绝对路径或发布凭据。
3. 路径依赖不能原样发布；必须先将该依赖发布并把清单改为精确坐标。dc package 对残留 path 依赖给出具体条目诊断。
4. ZIP 条目按 UTF-8 路径排序，统一 `/`、DOS 时间戳 `1980-01-01 00:00:00`、普通文件权限 `0644`、DEFLATE 压缩级别 6，不嵌入绝对路径或当前时间。相同输入和 dc 版本生成相同 SHA-256。
5. 解包拒绝绝对路径、盘符/UNC、反斜杠、`..`、符号链接、重复条目、Windows 大小写冲突以及未在格式内允许的顶层路径。先检查条目与总解压大小上限再提取：单包下载/解压上限均为 256 MiB，单文件 128 MiB，单个清单/元数据 1 MiB，文件数 10000。提取过程继续计数，不能只相信 ZIP 头部声明。
6. 元数据与清单坐标必须一致；压缩包内容不能覆盖缓存外文件。校验完成后原子地写入缓存，失败不留下可被误用的半包。

### 7.3 C 库随包分发

原生声明沿用 M14 的 `[native.<target>]`。预编译 C 的 `.o/.obj`、`.a/.lib`、`.so/.dylib` 与 DLL 按目标带入包；只复制清单明确列出的文件并记录摘要。未分发的系统依赖必须由原生库说明，不能偷偷引用发布者本机目录。

- 依赖图中每个包的原生输入只加入一次。
- C 符号保持原始名字；两个库提供相同符号时报告链接冲突，不靠 Dolphin 包别名改写 C 符号。
- 按确定性拓扑顺序提供静态库（使用者先于提供者，多个就绪节点按坐标排序），同包沿用声明顺序；菱形图的共享依赖必须排在全部使用者后面，不能简单按首次 DFS 到达顺序追加。检测不到的原生循环依赖由链接诊断暴露，首版不提供任意 linker flags 绕过模型。
- runtime-files 复制到 bin 目录；不同包同名不同内容文件冲突时报错，同名同摘要可去重。
- 纯 Dolphin 包可跨目标消费；带原生文件的包只能在列出的目标上消费。发布多目标原生包由作者分别产出文件，不意味着 dc 已支持交叉编译。

## 8. Maven 风格仓库、解析与锁文件

### 8.1 远程仓库协议

仓库就是能通过 HTTPS GET 下载文件的网站，无需首版搭建账号系统或数据库服务。仓库 ID 在根项目 `[repositories]` 配置，URL 规范化为不含尾部 `/` 的基地址。

坐标 `org.example:mathlib:1.0.0` 的路径固定为：

```text
<base>/org/example/mathlib/1.0.0/mathlib-1.0.0.dlib
<base>/org/example/mathlib/1.0.0/mathlib-1.0.0.dlib.sha256
```

- group 点分段变目录，name/version 保持合法校验后的路径段。name 沿用现有标识符约束；version 使用无 build metadata 的精确 SemVer，可有 prerelease。不允许路径分隔符、冒号或目录穿越。
- `.sha256` 格式固定为 64 位小写十六进制摘要加换行，只包含包字节的摘要。
- v1 不需要版本索引，不支持 `latest`、`^1.2`、版本范围、SNAPSHOT 或“选最高版本”。
- 字符串依赖默认使用 repository `default`；该 ID 缺失就报错，不内置一个尚不存在的中央仓库。
- 根项目仓库映射用于整个构建。包的依赖条目可以记录 repository ID，但不得携带自己的任意 URL；未知 ID 必须由根项目配置。repository ID 仅允许 ASCII 字母、数字、下划线且首位为字母，大小写不敏感，解析后统一小写并拒绝重复。
- 下载使用 TLS 校验、固定 30 秒请求超时、最多 3 次仅针对连接故障/5xx 的重试。404、摘要错误、解析错误不重试；失败显示坐标与 URL，日志不能泄漏凭据。
- GET 最多跟随 5 次同源重定向，不允许 HTTPS 降级；publish PUT 不跟随重定向。认证头不能发往另一 origin。仓库 URL 不允许嵌入用户名、密码、query 或 fragment。
- 生产远程源用 HTTPS；`http://127.0.0.1` / `http://localhost` 仅供本机开发和自动化测试。`file://` 路径仓库用于离线 fixture 和本地发布，必须用 URI 解析处理 Windows 路径。

### 8.2 依赖解析算法

以包完整坐标为节点、依赖为边，精确版本只做图遍历，不需要 SAT 求解器。

1. 从根清单读取直接依赖，校验别名、坐标、仓库 ID；按别名排序遍历，确保稳定行为。
2. 路径依赖读取其真实清单，取得坐标，以规范化绝对目录去重；远程依赖下载 / 使用校验过的缓存包。
3. 读取每个依赖包内规范化清单，递归解析传递依赖。
4. 一个 `(group, name)` 在一个构建中只允许**一个精确版本与一个来源**；不同版本或不同来源均报冲突并显示两条依赖链，不使用 Maven 的 nearest-wins，也不静默替换。
5. 检测包依赖环并显示环路径。包内纯函数模块的循环导入仍按已有规则处理，不能与包图环混淆。
6. 依赖必须声明 lib；引用只有 bin 的包报错。路径依赖也要执行同样规则。
7. 返回排序稳定的 PackageGraph、各包别名表、源码与 native 输入；构建层消费该结果，不在 lower/codegen 里下载网络资源。

### 8.3 锁文件

根目录生成并提交 `dolphin.lock`。它记录整个传递闭包，而不是只记录根项目直接依赖。

```toml
version = 1
compiler-version = "0.1.0"

[[package]]
coordinate = "org.example:mathlib:1.0.0"
source = "https://packages.example.org/dolphin"
sha256 = "<64位小写十六进制摘要>"
dependencies = []

[[root-dependency]]
alias = "math"
coordinate = "org.example:mathlib:1.0.0"
```

每个 package 的 dependencies 是排序后的 `{ alias, coordinate }` 数组；package 按坐标排序，根依赖单独保存别名。source 记录规范化仓库基地址；同坐标多来源既已禁止，无需再引入含糊的来源优先级。

路径依赖记录 `source = "path"` 和相对根清单的 `path`，不记录包摘要；路径源码修改会影响构建，锁文件只锁定身份与依赖图，不宣称锁住本地源码内容。

| 命令模式 | 行为 |
| --- | --- |
| 普通 check/build/run/fetch | 无锁则解析并写锁；清单图变化则更新；锁内未变的远程节点始终按已有摘要校验 |
| `--locked` | 必须有与清单、仓库映射、编译器版本一致的锁，禁止改写；仍可下载缺失缓存 |
| `--offline` | 不访问 HTTP(S)，使用本地缓存/路径/file 仓库；缺包报错，可在信息完整时生成锁 |
| `--locked --offline` | 不更新锁且不联网，用于可复现离线构建 |

同坐标远程文件变化时，即使不是 `--locked`，也不能自动接受新摘要覆盖旧锁；报“同一版本内容改变”，要求发布新版本。库包里不包含依赖源码或库作者 lock；消费端根 lock 才是最终解析结果。

### 8.4 缓存

缓存根：优先 `DOLPHIN_HOME`，默认用户主目录 `.dolphin`。

```text
<home>/cache/packages/sha256/<digest>/package.dlib
<home>/cache/packages/sha256/<digest>/unpacked/...
```

缓存按内容寻址；坐标映射必须同时带规范化来源。下载先到临时文件，校验摘要/结构后再原子改名；并发构建用每摘要锁或等价原子操作，不共享可被写坏的半成品。使用缓存构建前校验包文件摘要；解包目录依据可信包恢复，不将可变解包内容当作可信源。

坐标到摘要的索引固定写 `<home>/cache/index/<仓库规范URL的SHA-256>/<group目录>/<name>/<version>.toml`，记录 coordinate/source/sha256。首次无锁的 offline 解析只能使用已经校验过的索引与归档；锁存在时其摘要优先。索引/归档均以临时文件原子落盘，源码解包缓存不存放编译产物；产物写根项目 target。

远程源码诊断显示包坐标和包内路径，不只显示长缓存绝对路径。网络失败不能静默改用同名、不同来源的缓存。

## 9. CLI 与发布的完整使用流程

### 9.1 命令表

沿用现有 `<项目目录或文件>` 输入习惯；新命令的项目路径默认当前目录。

| 命令 | 精确定义 |
| --- | --- |
| `dc check <project>` | 解析依赖，检查该包 lib 和所有 bin；不产出发布包 |
| `dc build <project> --lib` | 仅编译验证 lib 并生成 `.dlib`；没有 lib 报错 |
| `dc build <project>` | 构建 lib（若有）与所有 bin；`--bin name` 只选该 bin，依赖 lib 作为输入 |
| `dc run <project> --bin name` | 运行 bin；只有 lib 时给出“库不可直接运行”诊断 |
| `dc package <project>` | 等价于 `build --lib` 的打包入口，先验证，不接受跳过检查选项 |
| `dc fetch <project>` | 解析/下载整个依赖闭包并写锁，不生成本机代码 |
| `dc publish <project> --repository <id>` | package 后上传包与摘要到配置的仓库，不覆盖已发布内容 |
| `dc info <project>` | 显示坐标、lib/bin 目标、直接依赖和缓存/锁状态；不隐式下载 |

`--lib` 与 `--bin` 互斥；`run --lib` 拒绝。`--locked`、`--offline` 对 check/build/run/package/fetch/publish 的依赖解析均有效；publish 若目标是 HTTP(S)，`--offline` 应拒绝上传，file 仓库可用。

默认产物 `<build.output>/package/<name>-<version>.dlib` 及 `.dlib.sha256`；本地验证 object 在 `<build.output>/lib/`。无清单单文件模式的 `-o` 保持现有意义，不用它指定远程包名。

### 9.2 发布协议

- **静态网站发布**：作者先 `dc package`，将两个产物放到 §8.1 的固定目录即可；可用静态托管网站或对象存储，消费者只需要 GET。
- **CLI 发布**：仓库提供相同路径的 HTTP PUT；dc 以 `If-None-Match: *` 上传包，再上传摘要作为完成标记。服务端必须支持条件创建；不支持时给出协议不支持诊断，不能退化为覆盖 PUT。
- 已存在且字节摘要相同视为幂等成功；内容不同拒绝。若首次上传包成功而摘要失败，重试读取并校验已有包后补传摘要。下载方只有包与摘要都存在才接受该版本。
- file 仓库使用临时文件 + 不覆盖的原子发布实现相同语义。
- 可选 Bearer token 从 `DOLPHIN_REPOSITORY_<ID>_TOKEN` 读取，ID 转大写；repository ID 仅允许 ASCII 字母、数字、下划线且首位为字母。token 不写入清单、lock 或包。
- `publish` 只发布当前库，不递归上传依赖；发布前用干净的解析缓存验证精确坐标依赖可从目标清单所引用仓库获取，避免靠本地 path/旧缓存掩盖缺失依赖。

首版不实现仓库网站后端、账号注册、搜索、评分和私服管理。协议与客户端必须落地；静态 GET 托管已足够实现“发布到某个网站再像 Maven 一样使用”。

### 9.3 最小端到端操作

```text
# 库作者：项目已按 §6.1 建立，repositories.default 已配置
dc check mathlib
dc build mathlib --lib
dc publish mathlib --repository default

# 应用作者：按 §6.2 声明 math = "org.example:mathlib:1.0.0"
dc fetch app
dc run app --bin app

# CI：先允许下载补齐缓存，之后断网重建
dc build app --locked
dc build app --locked --offline
```

验收时 `app` 应返回 42。仓库使用本机 fixture 服务，所有示例域名都是占位符，不要求访问一个真实中央服务。

## 10. 从 M13 经 M14 开始的实施计划

每阶段开工先看依赖和目标，只完成这一阶段。一个模型调用不要求包揽整份文档。

| 阶段 | 前置 | 具体交付 | 验收门槛 |
| --- | --- | --- | --- |
| M15-A 泛型与方法 | M14-F | 新增用户泛型 AST/检查、单态化模块、trait/impl/方法模块、字段可见性、聚合 payload | GEN-01/03/04/06 与同包嵌套模板测试通过；跨包验收在 C |
| M15-B 标准库 | A | 新建 std 源码模块、Option/Result、Vec、String/CString、Iterator/Range 和 prelude | API 测试组通过，容器逻辑在 Dolphin 源码 |
| M15-C 本地 lib 与包图 | A；集成示例依赖 B | manifest/lib 模式、path dependency、包别名、跨包 pub/泛型；重构构建目标选择 | GEN-02/05、PKG-01/02 通过；两个本地 lib + 一个 app 可构建 |
| M15-D 确定性打包 | C | ZIP 格式、元数据、规范化清单、native 文件、dc package/build --lib | 包内容与摘要测试，解包后可被另一项目编译 |
| M15-E 远程消费 | D | 精确版本解析、仓库 GET、缓存、lock、fetch、locked/offline | 冷缓存下载到运行、断网重建、冲突/损坏反例全部通过 |
| M15-F 发布与收尾 | B、E、M14-E | 条件 PUT/file 发布、静态网站样例、C 包消费、三平台 CI、文档迁移 | 发布者→仓库→新应用完整流程通过，才能标 M15 完成 |

建议新增文件职责（可按代码风格微调，不得把网络逻辑放进 lower）：

| 文件 / 模块 | 单一职责 |
| --- | --- |
| `src/monomorphize.rs` | 泛型模板、实例请求队列、去重和实例展开 |
| `src/methods.rs` | 方法/trait 声明解析、签名/关联类型检查、接收者降低 |
| `src/package.rs` | PackageId、PackageGraph、依赖描述与包来源 |
| `src/resolver.rs` | 精确版本图遍历、冲突/循环、别名环境 |
| `src/registry.rs` | GET/PUT/file 仓库协议，不处理语言 AST |
| `src/package_archive.rs` | 确定性 ZIP、格式/路径/摘要校验 |
| `src/lockfile.rs` | lock 读写、规范序列化与一致性判断 |
| `src/stdlib/*.do` | 真正的容器、字符串、迭代器实现 |

阶段 C 先交付 lib 模式、验证 object 和本地包图；阶段 D 再接入 `.dlib` 归档并完成 `build --lib` / `package` 的最终 CLI 合同。不能因阶段 C 尚无 ZIP 就回头要求提前完成 D，也不能把阶段 C 的 object 宣称为完整库包。

Rust 依赖优先选成熟的 HTTP/TLS、ZIP、SHA-256、SemVer 库；不要手写密码算法或 ZIP 解析器。实际选型写入 Cargo.toml/Cargo.lock，并更新发行许可证清单。

## 11. 验收矩阵

### 11.1 语言与标准库

| 编号 | 用例 | 预期 |
| --- | --- | --- |
| GEN-01 | identity 接收字面量、变量、字段、调用结果、嵌套泛型 | 推断一致，错误实参有位置诊断 |
| GEN-02 | 两个包调用同一个库泛型同一实参；模板调用另一模板 | 所需实例全部生成且去重，无重复符号 |
| GEN-03 | 普通递归、互递归、扩张递归、间接按值布局环 | 合法程序运行；非法程序显示调用/字段链，不 panic |
| GEN-04 | 泛型 impl、关联类型、缺方法、错 receiver、重复 impl | 静态分派正确，错误调用点报告 |
| GEN-05 | 两包同名类型/函数；泛型访问定义包私有 helper | 不碰撞、不被调用包名字劫持，不泄露私有访问 |
| GEN-06 | Option.None 的已知/未知期望类型；Result<string, Error> | 已知可编译，未知诊断；聚合 payload 完整可用 |
| API-01 | Vec<i32>、Vec<Point> push 扩容/get/set/pop/clone | 元素与容量正确，不依赖类型擦除 |
| API-02 | Vec 分配失败、零容量、reserve 溢出、deinit 两次同对象 | 102 或正常重置；Debug 无意外泄漏 |
| API-03 | Vec<String> 显式逐元素 deinit，再 deinit 容器 | 无泄漏，不存在隐式递归析构 |
| API-04 | const receiver、val 对象调用修改方法、私有 backing 字段 | 可读可写边界符合 §3/§4 |
| API-05 | UTF-8 concat/clone/substring/trim、非法边界、CString 内部 NUL | 正确内容、零分配查询、明确 Result 错误 |
| API-06 | 用户自定义 Iterator；只同名 next 未实现 trait | 前者运行，后者编译拒绝 |
| API-07 | 数组临时值、空迭代、范围边界、循环提前退出与 defer | 无悬垂适配；每轮和外层清理顺序正确 |

### 11.2 lib、仓库与发布

| 编号 | 用例 | 预期 |
| --- | --- | --- |
| PKG-01 | lib-only、bin-only、lib+多bin；run lib | 合法目标正确构建，纯库 run 明确拒绝 |
| PKG-02 | path 依赖、同名模块、私有成员访问、重复别名 | 正确解析或明确冲突诊断 |
| PKG-03 | 相同输入连续 package；改变私有 helper | 前者包字节与摘要一致，后者摘要变化 |
| PKG-04 | 打包 bin/target/未声明文件、残留 path 依赖 | 无关文件排除；path 依赖报错 |
| PKG-05 | 发布到本机仓库，清空消费端缓存后只写坐标构建 | 自动下载到运行结果 42；没有本地源码路径依赖 |
| PKG-06 | A→B→C、菱形相同版本、不同版本冲突、依赖环 | 完整 lock、共享节点去重；冲突显示两条链/环 |
| PKG-07 | 正确 lock 离线重建、缺缓存、改依赖后 locked | 可重建或明确缺包/锁过期诊断，无暗中联网 |
| PKG-08 | 下载中断、404、坏摘要、半上传版本、并发下载 | 不接受半包、不污染有效缓存 |
| PKG-09 | ZIP 路径穿越、重复条目、解压超限、大小写冲突 | 提取前拒绝，不写缓存外文件 |
| PKG-10 | 包格式/编译器版本/原生目标不兼容 | 在 codegen/link 前报告版本或目标 |
| PKG-11 | C 库包被两个依赖引用；native 同名 DLL 冲突 | C 输入去重；不同内容冲突明确报错 |
| PKG-12 | 相同版本重复 publish；不同字节；上传中断后重试 | 幂等成功、拒绝覆盖、可补全摘要 |
| PKG-13 | 同坐标远程文件被替换，已有 lock；不同仓库同坐标 | 不接受新摘要，来源冲突不自动选一个 |
| PKG-14 | 空格/中文目录、Windows file URI、本机 HTTP fixture | 三平台路径与网络处理正确 |
| PKG-15 | 解压 dc 发行包，在新目录消费已发布纯 Dolphin lib | 不需要 Rust/C 编译工具链现场编译运行时或第三方源码 |

HTTP 测试使用本机隔离端口和临时 `DOLPHIN_HOME`；通过请求计数证明 offline 零请求，不能依靠开发者全局缓存通过测试。网络标准库、公共网站可用性不属于测试前提。

阶段验收通过后，完整执行：

```text
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release --bins
```

新增 `examples/m15`（容器、跨包泛型）、库发布 fixture 和发行包冒烟用例。完成后同步 README、已实现功能参考、语言设计、安装发行说明与路线图；只根据真实测试证据勾选完成。

## 12. 明确延后与 M13 基线衔接

**延后**：动态 trait/class/继承、闭包、常量泛型、多 trait 约束、泛型特化、`?T`/`?`、HashMap/HashSet、自定义 allocator、完整文件/进程 API、网络/异步标准库、版本范围求解、闭源二进制 Dolphin 包、增量编译、中央仓库网站后端。M13 无 shell `run(cmd)` 内建，本阶段也不为包管理新增它。

**需要扩展的 M13 基础**：普通函数/类型 AST 增加用户泛型；数组/范围 for 接入 Iterator；module 解析增加包身份；要求 main 的 bin 流程增加独立 lib 模式；原跨模块公开数据字段显式补 pub。前置 M14 的内存 API、defer 和 std 身份直接复用。

**不做历史兼容**：不找回已清理的 M14/M15 模块/测试，不新增全局 allocate/free 或资源 try，不恢复隐式字符串分配和 duck-typed for。已存在的 M0-M13 正例/负例按明确新增语义逐项调整并记录原因，不能靠删除测试宣称通过。

### 历史任务模板（不可用于当前任务）

以下只保留当次 M13 起步的执行记录，不可复制到当前项目。请使用 [M18-M21 交接指南](plan-m18-plus.md)第 9 节的新提示词。

```text
[历史归档，禁止作为当前执行提示词；不得回退源码或重做已完成阶段]
从 M13 基线按 docs/proposal-m14-memory-model.md 与 docs/proposal-m15-generics-stdlib.md 的 v2 规格逐阶段实现。
本轮只完成阶段 <阶段编号>，先确认其前置阶段的验收结果。
先阅读对应文档的阶段表和“小上下文执行方式”，按路由加载相关章节，不要求一次读完两份长文档。
当前源码基线是 9e6f4c6；旧 M14/M15 已清理，待新增模块不是缺失依赖，不恢复旧实现或兼容层。
逐条完成该阶段对应验收编号，运行相关测试，必要时迁移旧测试并说明语义变化。
不要顺手引入“明确延后”的能力，也不要仅凭代码存在就标记阶段完成。
交付：修改文件、通过/失败的测试命令与结果、尚未完成的验收项、下一阶段入口。
```
