# M15 设计提案：泛型抽象与标准库

> 状态：草案（Draft，待评审）
> 对应路线图：[roadmap.md](roadmap.md) 第 6 节「M15：泛型抽象与标准库」

本提案为 M15 里程碑冻结泛型语义、契约（方法契约）模型与标准库形态提供决策依据。在提案通过前，不进行实现；提案通过后，M14 已冻结的内存模型在泛型化后语义平滑迁移，不留半成品。

---

## 1. 背景与现状

当前编译器（M0-M14）处于**无泛型、无方法、无契约**状态：

- 类型系统仅有四种类型：具名类型 `Name`、定长数组 `Array`、动态切片 `Slice`、显式指针 `Pointer`（见 [ast.rs `TypeRefKind`](../src/ast.rs)，**不含类型参数**）。
- 函数与结构体/枚举声明**均无泛型参数列表**（见 [ast.rs `Function`](../src/ast.rs) / `StructDecl` / `EnumDecl`）。
- 不存在方法语法：所有可调用对象都是自由函数 `f(x)`，结构体字段通过 `p.x` 访问，但无法 `p.method()`。
- `for` 循环是**编译器硬编码的两条路径**：整数范围 `0..3` 与定长数组（见 [ast.rs `ForIterable`](../src/ast.rs)）。「范围」不是可保存/可传递的普通值（见 [implemented-features.md §8.4](implemented-features.md)）。
- 所谓 `std`（M7）是用户项目 `src/std/` 下的普通源码目录，**不是编译器自带依赖**；`print`/`println`/`allocate`/`free`/`length` 等是编译器内建符号，经 codegen 直接映射到运行时导出的 C 符号（`dolphin_print_*`、`dolphin_allocate` 等，见 [implemented-features.md §14.1](implemented-features.md)）。
- M14 已冻结：Zig 式值语义 + 显式指针 + 手动分配；`allocate(n) -> []u8` 为字节切片；`string` 与 `[]u8` 别名等价；可选类型 `?T`、自动扩容容器 `ArrayList`、`Allocator` 类型均明确延后到 M15（见 [proposal-m14 §9](proposal-m14-memory-model.md)）。

因此，M15 的本质是**首次为语言引入类型抽象层（泛型 + 契约 + 方法）**，并据此构建**不依赖编译器对每个具体类型硬编码**的标准库。这是第三阶段（M13-M15）的最后一块、也是抽象风险最高的一块。

---

## 2. 设计原则（已确定）

以下原则由项目负责人明确，作为本提案的不可动摇前提：

1. **易用性优先**：语言面向学习与实践，用户心智负担优先于极致性能（沿用 M14 原则 1）。
2. **零生命周期符号**：继续拒绝 Rust 式显式生命周期标注（`'a`），泛型不引入任何与生命周期绑定的符号（沿用 M14 原则 2）。
3. **单态化**：泛型通过**单态化（monomorphization）**实现——泛型函数/类型在调用点按具体类型实参展开生成代码，无运行时装箱、无类型字典。已确定。
4. **标准库不靠类型特判**：`Option<T>`、`Result<T,E>`、迭代器、集合等**泛型抽象必须用泛型 + 契约真实实现**，不得为某个具体类型（如 `[]u8`）偷偷添加专用方法或专用 `for` 路径。

### 2.1 「不靠特判」的精确界定（本提案新增，必须冻结）

验收标准「标准库不依赖编译器对每个具体类型硬编码」存在一个**物理边界**：内存分配、系统调用（文件/进程）、格式化输出（`dolphin_print_*`）**必然穿透到编译器/runtime 配合**，不可能纯用户态实现。因此需重新界定：

> **「不硬编码」指类型层**：泛型容器与抽象（`Option<T>`、`Result<T,E>`、`Vec<T>`、迭代器）必须用泛型 + 契约真写出来，不得给 `[]u8` 偷偷加 `append`、给数组偷偷加 `.map()`、给 `for` 偷偷加第三条特判路径。
>
> **内建/运行时原语保留一个最小、正交、不可再缩减的底座**（分配、I/O、OS 穿透），其余全部下沉到用户态标准库。

标准库由此分为两层，边界见 §6。

---

## 3. 泛型语义（已确定）

以下语义对应 roadmap M15 的「泛型函数和泛型数据类型」：

### 3.1 类型参数语法

- 泛型函数：`fn foo<T>(x: T): T { ... }`，类型参数声明在函数名后、参数列表前。
- 泛型结构体：`struct Vec<T> { ... }`；泛型枚举同理 `enum Result<T, E> { ... }`。
- 泛型类型实参：`Vec<i32>`、`Result<string, Error>`；类型实参在类型名后用尖括号。
- 类型参数名沿用标识符规则（ASCII，见 [implemented-features.md §3.2](implemented-features.md)）。

### 3.2 单态化实例化规则（已确定）

- 泛型函数在每个**不同具体类型实参组合**的调用点实例化为一个独立的具体函数。
- 实例化时机：**全程序收集所有调用点的类型实参，去重后统一批量生成**（见 §11.1，已确定）。
- 跨模块实例化：泛型函数可跨模块调用；全程序统一实例化 + 稳定 mangling 去重（见 §11.1，已确定）。
- 递归泛型：天然支持，无需额外实例化机制，仅需自引用字段间接性校验（见 §11.2，已确定）。
- 泛型约束：类型参数约束 `fn f<T: trait>(...)`，语法见 §4.3（已确定）。

### 3.3 与 M14 语义的平滑迁移

- `allocate(n) -> []u8` 演进为 `allocate<T>(n) -> []T`，`[]u8` 即 `allocate<u8>` 的特例（见 [proposal-m14 §12.4](proposal-m14-memory-model.md)）。
- 可选类型 `?T` 与 `Option<T>` 语义：M14 已冻结「指针默认非空、不引入 null」，M15 的 `?T` 定义为 `Option<T>` 的语法糖（见 §11.3，已确定）。
- 自动扩容容器 `ArrayList` 在泛型 + 方法落地后实现为 `Vec<T>`（见 §6.2）。

### 3.4 单态化的实现落点与 IR 表示（已确定）

**核心结论：单态化是 AST 层的预处理 pass，IR 与 codegen 层零改动。** 泛型只存在于 AST 层，进入 IR 前已全部实例化为具体类型。

#### 3.4.1 落地位置：AST → IR 之间插入「单态化 pass」

当前编译流程（见 [implemented-features.md §14](implemented-features.md)）为「AST → 函数签名收集 → 名称/类型检查 → CFG IR → 代码生成」。单态化插入在**类型检查之后、CFG IR lowering 之前**：

```
AST（含泛型）
  → 单态化 pass（收集实参 + 实例化展开，输出「无泛型的 AST」）
  → 名称/类型检查（在实例化后的 AST 上执行，复用现有逻辑）
  → CFG IR（无泛型，沿用现有 ir.rs 表示）
  → 代码生成（零改动）
```

泛型函数体是「参数化的 AST 模板」：实例化 = 把模板里的类型参数 `T` 替换为具体类型实参，产出一个普通（无泛型）的 AST 节点，随后走现有 lowering。

#### 3.4.2 泛型类型实例的标识：mangling 名复用现有 `TypeId`（方案 A，已确定）

`Vec<i32>` 与 `Vec<string>` 是两个不同的具体类型，各分配一个 `TypeId`，复用现有 `types: Vec<TypeDef>` 类型表。`type_ids` 的键从「类型名字符串」升级为「实例化后的 mangling 名」：

- `Vec<i32>` → 键 `"Vec<i32>"`（或规范 mangling 如 `"Vec$i32"`）
- `Vec<string>` → 键 `"Vec<string>"`

实例化类型在 IR 层就是普通的 `Type::Struct(TypeId)`，`Type`/`TypeDef`/`type_of`/codegen **均无需改动**（见 [ir.rs `Type`](../src/ir.rs:4)）。`type_ids: HashMap<String, TypeId>`（[lower.rs](../src/lower.rs:38)）的结构不变，只是键的内容从「裸类型名」变为「mangling 后的实例名」。

#### 3.4.3 泛型函数的实例化：每个「泛型名 + 实参」生成独立具体函数

- 泛型函数 `foo<T>` 按每个唯一类型实参组合实例化为具体函数 `foo$i32`、`foo$string`（mangling 见 §11.1）。
- 实例化结果就是普通 `Function`，`Function`/`FunctionId` 结构无需泛型字段（见 [ir.rs `Function`](../src/ir.rs:173)）。
- 全程序收集实参、去重、统一批量生成（§11.1，已确定），天然处理递归泛型（§11.2，已确定）。

#### 3.4.4 AST 侧改动（唯一需要动的地方）

- [ast.rs `TypeRefKind`](../src/ast.rs:76) 需新增泛型相关 variant（如 `TypeParam(String)`、`Generic { name, args }`），以及 `Function`/`StructDecl`/`EnumDecl` 需新增类型参数列表字段。
- `resolve_type`（[lower.rs](../src/lower.rs:2214)）在单态化后接收到的类型已是具体类型，`TypeRefKind::Name` 分支按 mangling 名查 `type_ids` 即可，逻辑不变。

#### 3.4.5 待补的连带项（实现时一并处理）

- **`allocate`/`try` 泛型化**：`allocate<T>(n) -> []T` 与 `try` 资源检查（当前硬编码 `[]u8`，见 [lower.rs `lower_try`](../src/lower.rs:815)）需随泛型同步放开到 `[]T`。这是 §3.3「平滑迁移」里表面轻、实则连带面较大的一处，实现时不可忽略。

---

## 4. 契约模型：静态方法契约（已确定）

### 4.1 术语澄清

路线图原文「trait/interface、方法和关联函数」使用了含糊的「trait/interface」写法。本提案澄清：

- **「契约」是本语言的概念名**：一组方法签名组成的**编译期行为契约**，类型去实现它。它对应 Rust 的 `trait`、Java/Kotlin 的 `interface` 在「方法契约」这一层的内涵，**三者是同一个概念在不同语言里的名字**，本语言只引入一个，不区分「trait」与「interface」两个词。
- **关键字定为 `trait`**（已确定）。
- 本提案正文统一用「契约」指代该机制，语法示例中写作 `trait`。

### 4.2 契约 = 编译期约束，不是运行时多态

本里程碑的「契约」是**静态的**：它作为泛型约束的载体，在编译期完成检查与单态化，**不引入运行时多态**、不引入虚表、不引入「一个契约变量指向不同实现类型」的异构集合。

### 4.3 契约语法（已确定）

三项语法决策均已确定：

1. **声明**：`trait 名称 { 方法签名列表 }`，方法只声明签名、不写函数体。`self` 是约定的第一参数名，代表「当前类型的值」；`self: *Self` 代表「指向当前类型的指针」。

```dc
trait Shape {
    fn area(self): f64;
    fn scale(self: *Shape, factor: f64);
}
```

2. **实现**：`impl 契约 for 类型 { 方法体 }`，一个类型可实现多个契约（多个 `impl` 块），实现块内写函数体，签名须与契约声明一致。

```dc
struct Circle { radius: f64 }

impl Shape for Circle {
    fn area(self): f64 { return 3.14 * self.radius * self.radius; }
    fn scale(self: *Circle, factor: f64) { self->radius *= factor; }
}
```

3. **约束**：`fn f<T: 契约>(...)` 表达「`T` 必须实现该契约」，约束在编译期检查，`T` 未实现契约时在调用点报错。

```dc
fn total_area<T: Shape>(xs: []T): f64 {
    var s = 0.0;
    for x in xs { s += x.area(); }
    return s;
}
```

### 4.4 `self` 传递语义（已确定）

- **读方法拿 `self`（值）**：调用时复制一份，方法内修改不影响原值，与 M14 §5.2「无移动语义、一律复制」一致。
- **改方法拿 `*Self`（显式指针）**：原地修改，与 M14 §5.3「显式指针、不做借用检查」一致。
- **不引入 `&self`/`&mut self` 借用语法**：违背 M14 原则 2「零借用符号」。

### 4.5 契约的职责

1. **泛型约束载体**：`fn f<T: trait>(...)` 表达「`T` 具备某能力」。
2. **方法组织**：契约声明方法签名，`impl` 提供实现；`x.method()` 方法调用依赖契约 + `impl` 的名称解析。
3. **标准库抽象**：`Iterator`、`Eq`、`Hash` 等用契约表达（见 §6、§7）。

### 4.6 与长远 OOP 愿景的关系（必须冻结，避免 M15 过度设计）

项目对语言有一个**更长远的面向对象愿景**（`struct`≈值类型、接口声明方法、未来引入 `class` 实现多接口、支持运行时多态）。本提案明确：

- **M15 只做契约的「静态约束」这一层地基**，它是泛型的必需品，与单态化天然契合。
- **`class`、继承、运行时多态（接口引用指向不同实现类）明确不在 M15**，留待未来单独里程碑。届时再决定是否在静态契约之上叠加运行时多态（`dyn` / 装箱）。
- 理由：运行时多态需引入装箱或引用语义，与 M14 冻结的「值语义 + 显式指针、无 GC/RC」存在张力；且 `class`/继承是多里程碑体量，不应与泛型同批引入。

---

## 5. 方法语法与 `self` 传递（已确定）

### 5.1 方法定义

- 方法在 `impl Type` 块内定义：`impl Point { fn x(self): i32 { ... } }`。
- 关联函数（无 `self`）与实例方法（带 `self`）**都写在同一个 `impl Type { }` 块内**，靠「第一参数是否名为 `self`」区分，不引入额外的 `static` 关键字。

```dc
impl Vector {
    // 关联函数（无 self）
    fn make(capacity: i32): Vector { ... }

    // 实例方法（值 self，只读）
    fn total(self): i32 { ... }

    // 实例方法（*Self 指针，原地修改）
    fn push(self: *Vector, value: u8): i32 { ... }
}
```

### 5.2 `self` 传递语义（与 M14 咬合，已确定）

M14 已冻结「值语义 + 显式指针」，方法要原地修改容器**必须拿 `*Self`**，这是 M14 指针语义的第一次真实落地场景：

- `self`（值）→ 复制调用者，方法内修改不影响原值，与 M14 §5.2「无移动语义、一律复制」一致。
- `self: *Self`（指针）→ 原地修改，与 M14 §5.3「显式指针、不做借用检查」一致。
- **不引入 `&self` 借用语法**（见 §4.4，已确定）。

### 5.3 调用语法

- 实例方法：`v.push(1)`、`s.area()` —— 由值的类型查找到对应 `impl` 中的方法。
- 关联函数：`Vector::make(4)` —— 用 `::` 调用，与实例方法的 `.` 天然区分（已确定）。

### 5.4 方法解析

- `x.foo()` 解析规则：由 `x` 的类型查找到对应 `impl` 中的 `foo`，与自由函数命名空间分离。
- 泛型方法：`impl<T> Vec<T> { fn push(self: *Vec<T>, v: T) { ... } }`。
- 契约方法：`impl Iterator for Range { fn next(self: *Self): Option<i32> { ... } }` 后，`range.next()` 可用。

---

## 6. 标准库分层与模块清单

### 6.1 分层边界（已确定）

标准库分为两层，边界是「**是否穿透编译器/runtime**」：

| 层 | 内容 | 实现方式 |
|----|------|---------|
| **内建原语（primitives）** | 内存分配 `allocate`/`free`、格式化输出 `print`/`println`、OS 穿透（文件/进程系统调用）、运行时 trap | 保留为编译器内建 + runtime 导出符号（沿用 M14 机制，见 [proposal-m14 §12.5](proposal-m14-memory-model.md)），**不下沉** |
| **用户态标准库（stdlib）** | `Option<T>`、`Result<T,E>`、迭代器、`Range`、`Vec<T>`、`string` 工具、`File`/进程高层封装 | **用 Dolphin 语言自身 + 泛型 + 契约真写**，作为源码随编译器分发或内置 |

**原则**：内建原语只保留「不可再缩减的底座」，凡是能用泛型 + 契约在用户态表达的，一律下沉。判定标准：`for x in my_custom_iter` 能跑通，即证明迭代器抽象立住了。

### 6.2 模块清单（按实现难度分三档）

标准库按「是否穿透 OS」分三档，M15 纳入前两档，第三档整体延后：

#### 6.2.1 第一档：纯用户态（不碰 OS，M15 首批核心）

| 模块 | 内容 | 依赖 |
|------|------|------|
| `Option<T>` | 可选值（`?T` 语法糖，§11.3） | 纯泛型枚举，无 OS 依赖 |
| `Result<T,E>` | 错误值（§8） | 纯泛型枚举 |
| `Iterator` 契约 | `next()` 抽象 | trait + `Option<T>` |
| `Range` | 把 `0..3` 下沉为值类型 | `Iterator` |
| `Vec<T>` | 自动扩容动态容器（M14 的 `ArrayList` 演进） | `allocate<T>` + `free` |
| 数组/`Vec` 迭代器 | 各实现 `Iterator` | `Iterator` |
| `string` 工具 | split / trim / 查找 / 拼接等 | `string`/`[]u8` 之上 |

#### 6.2.2 第二档：OS 穿透（文件 + 进程，M15 一并打通）

文件与进程同属「OS 穿透」，需 runtime 侧导出符号 + 用户态高层封装，一起做：

| 模块 | 内容 | 模型 |
|------|------|------|
| **文件 I/O** | `File` 类型 + `open`/`read`/`write`/`close`（最小可用） | **同步阻塞**（已确定） |
| **进程** | `spawn`/`wait`/`exit_code` | 同步阻塞 |

**实现要点**：

- runtime 侧在 [unix_runtime.c](../runtime/unix_runtime.c) / [windows_runtime.cpp](../runtime/windows_runtime.cpp) 各加一套 OS 系统调用导出（文件用 `open`/`read`/`write`/`close`，进程用 `spawn`/`wait`），Windows 侧走 `CreateFile`/`ReadFile`/`WriteFile`/`CloseHandle` 与 `CreateProcess`/`WaitForSingleObject`。
- 用户态在导出符号之上封装 `File` 类型与进程类型，用 `Result` 表达错误（打开失败、读取失败等）。
- **文件 I/O 作为「OS 穿透」的样板**，验证 §6.1「内建原语底座」路径跑得通；进程复用同一套模式。
- 首版**不做**：异步/非阻塞、文件缓冲层、目录遍历、文件系统元数据。

#### 6.2.3 第三档：网络（整体延后，见 §9）

网络**不在 M15**，理由：

1. **阻塞模型矛盾**：同步 socket 会卡死（`recv` 可能永久阻塞、`connect` 可能超时），而异步需要事件循环 + 协程，牵涉 roadmap 明确「暂不排期」的异步/协程（[roadmap.md:346](roadmap.md:346)）。
2. **跨平台成本最高**：Unix 的 BSD socket 与 Windows 的 Winsock2 差异是数量级的（`WSAStartup` 初始化、`SOCKET` 句柄、错误码体系均不同），每个功能都要双份 runtime 实现。
3. **协议无底洞**：DNS、TCP 重连、HTTP、TLS 会越做越深，需先画线。

### 6.3 首批核心抽象（M15 里程碑范围）

按「核心抽象优先」，落地顺序：

1. **`Option<T>`**：可选值（`?T` 语法糖，§11.3）。
2. **`Result<T, E>`**：错误值，衔接 M13 非泛型错误处理样例（§8）。
3. **`Iterator` 契约**：`next()` 抽象，统一 `Range`、数组、`Vec` 的迭代。
4. **`Range`**：把 `0..3` 下沉为可保存/传递的值类型。
5. **数组 / `Vec` 迭代**：定长数组与 `Vec<T>` 各提供迭代器。
6. **`Vec<T>`**：自动扩容容器（M14 的 `ArrayList` 演进）。
7. **`string` 工具**：常用字符串操作。
8. **文件 I/O（同步）+ 进程（同步）**：OS 穿透样板，`Result` 表达错误。

**明确不在首批**（延后，见 §9）：`HashMap`/`HashSet`（需先定 `Hash`/`Eq` 契约）、网络（socket/DNS/HTTP/TLS）、异步文件 I/O。

### 6.4 验收试金石

`for` 循环从「硬编码两条路径」重构为「对实现 `Iterator` 的类型做语法糖展开」后，以下代码必须可用：

```dc
for x in 0..3 { ... }          // Range 实现 Iterator
for v in [1, 2, 3] { ... }     // 数组迭代器
for x in my_custom_iter { ... } // 用户自定义迭代器（验收标准核心）
```

---

## 7. 迭代器与 `for` 泛型化（已确定）

- `for x in iterable` 展开为：`var it = iterable.into_iter(); loop { match it.next() { Option.Some(x) => { body }, Option.None => break } }`（§11.6 已确定）。
- `Iterator` 契约最小含 `next(self: *Self): Option<T>`；`T` 为迭代产出类型。
- 采用 `into_iter()` 分层：可迭代对象（`Range`、数组、`Vec`）各自实现 `into_iter()` 返回迭代器，容器与迭代器分离。
- 循环变量 `x` 类型由 `Iterator` 的产出类型推导。
- 与 M14 指针语义咬合：`next` 拿 `*Self` 以推进内部游标，是 M14 显式指针的又一落地场景。

---

## 8. 错误处理：`Result` 与 `?`（已确定）

- `Result<T, E>` 用泛型枚举实现，对齐 M13 已有的「基于枚举的非泛型错误处理样例」，避免两套风格打架。
- `?` 传播语法（已确定引入）：函数内 `expr?` 等价于「`Err(e)` 则提前 `return Err(e)`，`Ok` 则解包」。
- 首版 `?` **仅作用于 `Result<T,E>`**，不作用 `Option`；错误类型不自动转换，要求 `?` 所在函数返回的 `E` 与 `expr` 的 `E` 完全一致（§11.7 已确定）。

---

## 9. 演进路径（不在 M15 实现）

以下能力明确延后，避免本里程碑过度设计：

- 动态分派 / 运行时多态 / `class` 继承（含契约关键字最终命名，见 §4.6）。
- `HashMap`/`HashSet` 等哈希容器（需先定 `Hash`/`Eq` 与哈希算法）。
- **网络（socket / DNS / HTTP / TLS）**：整体延后。理由见 §6.2.3——同步 socket 会卡死、异步需事件循环+协程、跨平台成本最高、协议无底洞。留待异步/协程里程碑一并解决。
- 异步文件 I/O / 非阻塞 I/O（与网络共享同一事件循环/协程机制）。
- 文件缓冲层、目录遍历、文件系统元数据。
- 闭包与高阶函数（`for_each` 等）。
- 泛型特化（specialization）。
- 关联类型（associated types）与常量泛型（`const` 泛型参数）。
- 宏 / 编译期元编程。
- 异步 / 协程。

---

## 10. 验收标准

1. 类型参数参与名称解析、类型检查和代码生成（单态化后每个具体实例正确生成）。
2. 泛型函数/类型可跨模块使用，实例化规则一致、无重复定义冲突。
3. `Option<T>`、`Result<T,E>` 用泛型真实实现，非编译器硬编码。
4. `for` 循环重构为对 `Iterator` 契约的展开，`for x in my_custom_iter` 可用（迭代器抽象立住）。
5. 方法语法 `x.foo()` 可用，标准库 API 为链式风格；`*Self` 方法可原地修改容器。
6. 契约作为编译期约束生效，`fn f<T: 契约>` 约束错误在编译期、调用点报告。
7. 标准库不依赖编译器对每个具体类型硬编码（内建原语边界如 §6.1 冻结）。
8. `Vec<T>` 作为自动扩容容器可用，`string` 常用工具可用。
9. 文件 I/O（同步）与进程（同步）可用，错误经 `Result` 表达；OS 穿透原语路径（§6.2.2）跑通。
10. 全部现有 M0-M14 测试仍通过；新增 M15 示例（含跨模块泛型、自定义迭代器、文件 I/O）通过。

---

## 11. 决策清单（全部已确定）

以下为 M15 实现前逐一冻结的设计决策，供实现时直接查阅：

### 11.1 泛型跨模块实例化的归属（定义方 vs 调用方）— ✅ 已确定

- **决策**：**全程序统一实例化 + 稳定 mangling 去重**。已确定。

  1. **全程序处理**：编译器本就把整个项目所有源文件合并为单个 `ast::Program` 再 lowering（见 [lower.rs `lower_sources`](../src/lower.rs:18)），不存在模块间信息隔离。因此在 lowering 阶段收集「全程序所有调用点的类型实参」是免费且天然的，无需「定义方预见」。
  2. **按「泛型名 + 类型实参列表」去重**：每个唯一组合只生成一份具体函数实例。若多个模块以相同实参调用（如 `a`、`c` 都用 `identity(1)` 即 `T=i32`），因 key 相同只生成一份，天然避免重复符号——同时解决了「定义方预见」与「调用方重复」两个问题。
  3. **稳定 mangling**：实例符号名按确定规则生成（如 `identity<T=i32>` → `identity$i32`），与编译顺序无关，跨模块、跨编译单元一致。
  4. **实例化时机**：先收集完所有实例请求（`HashMap<(泛型名, 实参列表), ...>` 去重），lowering 结束后**统一批量生成**，而非「遇到调用即生成」——天然去重，且避免递归泛型下的展开顺序泥潭。

### 11.2 递归泛型（如 `Node<T> { next: Option<Node<T>> }`）— ✅ 已确定

- **决策**：**递归泛型天然支持，无需额外实例化机制**。已确定。

  1. **布局无风险**：结构体通过 `TypeId` 间接层引用类型（见 [ir.rs `Type`](../src/ir.rs:23)），自引用字段经 `Option`/指针间接持有，不会内联展开成无限嵌套布局。
  2. **实例化无风险**：实例化只在「类型实参为具体类型」时发生（§11.1），类型参数 `T` 本身不触发实例化；配合全程序去重，每个具体组合只生成一次，不存在 `Node<Node<Node<...>>>` 无限套娃。
  3. **唯一需补的校验**：自引用字段必须经 `Option`/指针间接——纯内联自引用 `struct Node<T> { next: Node<T> }` 会构成无限大小布局，报编译错。这属结构体布局的既有规则，非 M15 泛型新增。

### 11.3 `?T` 与 `Option<T>` 的关系 — ✅ 已确定

- **决策**：`?T` 定义为 `Option<T>` 的**语法糖**，二者完全等价。已确定。
  - `?T` 是 `Option<T>` 的简写（尤其 `?*Point` 表示「可空指针」这类高频场景，比 `Option<*Point>` 直观）。
  - 底层统一为泛型枚举 `Option<T>`，实现只做一份，不引入第二套可选语义。

### 11.4 `impl` 块与关联函数的语法 — ✅ 已确定

- **决策**：`impl Type { ... }` 定义方法，`impl<T> Type<T> { ... }` 定义泛型方法块；无 `self` 的函数即关联函数，通过 `Type::func()`（`::`）调用；实例方法通过 `x.func()`（`.`）调用。契约实现语法（`impl 契约 for 类型`）见 §4.3，关键字为 `trait`。已确定。

### 11.5 `self` 传递语法：`*Self` vs 引入 `&self` — ✅ 已确定

- **决策**：**沿用 `*Self`**（`fn push(self: *Vector, ...)`），**不引入** `&self` 借用语法。理由：M14 已冻结「显式指针 `*T`、不做借用检查」，引入 `&self` 会连带暗示借用语义，违背原则 2；`*Self` 与自由函数 `fn push(v: *Vector, ...)`（见 [examples/m14 vector.do](../examples/m14/src/dyn/vector.do)）完全同构，迁移零成本。已确定。

### 11.6 `for` 循环展开为 `Iterator` 的具体形态 — ✅ 已确定

- **决策**：`for x in iterable` 展开为「`var it = iterable.into_iter(); loop { match it.next() { Option.Some(x) => { body }, Option.None => break } }`」。已确定。
  - `Iterator` 契约最小含 `next(self: *Self): Option<T>`；`T` 为产出类型。
  - 采用 **`into_iter()` 分层**：可迭代对象（`Range`、数组、`Vec`）各自实现 `into_iter()` 返回迭代器，容器与迭代器分离。
  - `break`/`continue` 语义沿用现有循环规则（[implemented-features.md §8.5](implemented-features.md)）；循环变量 `x` 类型由 `Iterator` 产出类型推导。

### 11.7 `?` 传播语法与 `Result`/`Option` 的交互 — ✅ 已确定

- **决策**：**引入 `?`，首版仅作用于 `Result<T,E>`**。已确定。
  - `expr?` 等价于「`Err(e)` 则提前 `return Err(e)`，`Ok` 则解包」。
  - **不作用于 `Option`**：`Option` 的可空处理用 `match` 显式做，避免首版引入 `From`/`Into` 转换体系。
  - 错误类型收敛：首版要求 `?` 所在函数返回的 `E` 与 `expr` 的 `E` 完全一致，不做自动转换（`From`/`Into` 延后）。

### 11.8 契约与 `struct` 的定位关系（评审新增）— ✅ 已确定

- **决策**：`struct` 是**数据的容器**（纯值类型，≈ data class），契约是**行为的抽象**，二者正交、不冲突。契约服务于「对泛型参数/类型声明方法能力」，是 M15 泛型 + 迭代器抽象的前提，不因 struct 已存在而多余。已确定。
  - 未来 `class` 实现契约的愿景不在本里程碑，但契约设计不写死为「仅值类型可实现」，为未来叠加 `class`/多态预留空间。

---

## 12. 决策记录（全部已确定）

| 编号 | 议题 | 决策结论 |
|------|------|---------|
| 4.x | 契约关键字 | **`trait`** |
| 4.x | 契约声明/实现/约束语法 | `trait 名 { 方法签名 }` / `impl trait for 类型 { 方法体 }` / `<T: trait>` |
| 4.4/11.5 | `self` 传递语义 | **`self`（值）+ `*Self`（指针）**，不引入 `&self` |
| 11.4 | `impl` 块与关联函数 | `impl Type { ... }`；关联函数 `Type::func()`、实例方法 `x.func()` |
| 11.1 | 跨模块实例化归属 | **全程序统一实例化 + 稳定 mangling 去重** |
| 11.2 | 递归泛型 | **天然支持**（`TypeId` 间接 + 全程序去重），仅需补自引用字段间接性校验 |
| 11.3 | `?T` 与 `Option<T>` | **`?T` = `Option<T>` 语法糖** |
| 11.6 | `for` 展开形态 | `into_iter()` 分层 + `Iterator::next`，`Iterator` 最小含 `next(self: *Self): Option<T>` |
| 11.7 | `?` 与 `Result`/`Option` | **`?` 仅 `Result<T,E>`**，不作用 `Option`，错误类型不自动转换 |
| 11.8 | 契约与 struct 定位关系 | struct（数据容器）与契约（行为抽象）正交，不冲突 |
| 3.4 | 单态化实现落点与 IR 表示 | **AST 层预处理 pass，IR/codegen 零改动**；实例化类型用 mangling 名复用 `TypeId`（方案 A） |
| 6.2 | 标准库模块清单 | 纯用户态（Option/Result/Iterator/Range/Vec/string）+ OS 穿透（文件 I/O 同步 + 进程同步）；网络整体延后 |
