# M15 实现计划：泛型抽象与标准库

> 对应设计提案：[proposal-m15-generics-stdlib.md](proposal-m15-generics-stdlib.md)
> 本计划在提案通过后执行；每一步都有可验证的验收点，未通过不进入下一步。

## 实现总览

M15 的核心是「首次引入类型抽象层」。按 §3.4 已确定的方案，**单态化是 AST 层预处理 pass，IR 与 codegen 零改动**。因此实现顺序遵循：

1. 先打通「泛型语法 → AST」的词法/语法层（最小侵入）。
2. 再实现「单态化 pass」，把泛型 AST 展开成无泛型 AST。
3. 然后实现「契约 + 方法」——它们依赖泛型约束与名称解析。
4. 最后实现标准库（纯用户态 → OS 穿透）。

---

## 阶段 0：基线确认

**验证**：`cargo test` 全绿；`cargo build --release` 成功。

- 确保在改动前，现有 M0-M14 全部测试通过（[tests/build.rs](../tests/build.rs)、[tests/cli.rs](../tests/cli.rs)、[tests/manifest.rs](../tests/manifest.rs)）。
- 记录当前基线，后续每个阶段完成后都要回归这条基线。

---

## 阶段 1：泛型语法（词法 + AST + 解析）

目标：让 `fn foo<T>(x: T): T`、`struct Vec<T>`、`Vec<i32>` 能被解析进 AST，暂不产生任何代码。

### 1.1 词法新增 token

- 新增 `Less`/`Greater` 已存在（[token.rs](../src/token.rs:48)），可直接用于 `<T>` 尖括号。
- 新增 `ColonColon`（`::`，关联函数调用，§5.3）；需在 [lexer.rs](../src/lexer.rs) 把连续的 `:` 识别为 `::`（区别于单个 `Colon`）。
- 新增 `Question`（`?`，错误传播 + `?T` 语法糖，§8/§11.3）。
- 新增关键字 `trait`、`impl`（§4）。

### 1.2 AST 扩展

- [ast.rs `TypeRefKind`](../src/ast.rs:76) 新增：
  - `TypeParam(String)`：类型参数 `T`。
  - `Generic { name: String, args: Vec<TypeRef> }`：泛型实例 `Vec<i32>`。
- `Function`/`StructDecl`/`EnumDecl` 新增 `type_params: Vec<String>` 字段（类型参数列表）。
- 新增顶层 `trait` 声明与 `impl` 块的 AST 节点（契约声明 `TraitDecl`、实现块 `ImplBlock`）。

### 1.3 解析器

- `parse_function` 支持函数名后的 `<T, U>` 参数列表。
- `parse_struct`/`parse_enum` 支持 `<T>`。
- 类型引用 `parse_type` 支持 `<...>` 实参。
- 新增 `parse_trait`、`parse_impl`。

**验证**：`cargo test` 通过（新增解析测试：能 parse 出带类型参数的 AST，断言 AST 结构正确）。此阶段**不**要求能编译运行泛型程序，只要求 AST 形状正确。

---

## 阶段 2：单态化 pass（核心）

目标：实现 §3.4 的「AST 层单态化」，把泛型 AST 展开成无泛型 AST，再交给现有 lowering。

### 2.1 类型实例的标识（方案 A，§3.4.2）

- 单态化 pass 内维护「泛型类型名 + 实参 → 具体类型」的展开。
- 实例化类型用 mangling 名（如 `Vec<i32>` → 键 `Vec$i32`）进入现有 `type_ids`，复用 `TypeId`，IR 零改动。

### 2.2 泛型函数实例化（§3.4.3 / §11.1）

- 全程序收集每个泛型函数的类型实参组合（`lower_sources` 已全程序处理，见 [lower.rs](../src/lower.rs:18)）。
- 以「泛型名 + 实参列表」去重，统一批量生成具体函数实例。
- 实例符号名稳定 mangling（`identity$i32`）。

### 2.3 无约束泛型先行

- **先只做「无约束泛型」**（`fn foo<T>(x: T): T`、`struct Vec<T>`），不引入 `<T: trait>` 约束——约束依赖阶段 3 的契约，避免耦合。
- `allocate<T>(n) -> []T` 与 `try` 资源检查从硬编码 `[]u8` 放开到 `[]T`（§3.4.5，当前 [lower.rs `lower_try`](../src/lower.rs:815) 写死 `[]u8`）。

**验证**：新增测试——`fn id<T>(x: T): T { return x; }` 分别用 `i32`、`string` 调用，编译运行正确；`struct Vec<T>` 用 `Vec<i32>`、`Vec<string>` 实例化，跨模块调用无重复符号。

---

## 阶段 3：契约（trait）与方法

目标：实现 §4 契约模型 + §5 方法语法。

### 3.1 契约声明与实现

- `trait 名 { 方法签名 }` 解析 + 类型检查（方法签名不写体）。
- `impl trait for 类型 { 方法体 }`，一个类型可多 impl。
- `self`（值）+ `*Self`（指针）传递（§4.4/§11.5，已确定，不引入 `&self`）。

### 3.2 方法语法与关联函数

- `impl Type { ... }` 定义方法；关联函数（无 `self`）`Type::func()`、实例方法 `x.func()`（§11.4，已确定）。
- 方法解析：`x.foo()` 由 `x` 类型查对应 `impl`（§5.4）。

### 3.3 泛型约束

- `fn f<T: trait>(...)` 约束检查：`T` 未实现契约时在调用点报错（§4.3）。

**验证**：新增测试——定义 `trait Shape { fn area(self): f64; }`，`Circle`/`Rectangle` 实现它，`fn total_area<T: Shape>(...)` 正确静态分派；`*Self` 方法原地修改容器；约束缺失时编译报错。

---

## 阶段 4：标准库——纯用户态（第一档）

目标：实现 §6.2.1 的核心抽象，作为「泛型 + 契约立住」的证明。

### 4.1 核心抽象

1. `Option<T>`（`?T` 语法糖，§11.3）
2. `Result<T, E>`
3. `Iterator` 契约（`next(self: *Self): Option<T>`）
4. `Range`（把 `0..3` 下沉为值类型）
5. 数组 / `Vec` 迭代器
6. `Vec<T>`（自动扩容，`ArrayList` 演进）
7. `string` 工具（split/trim/查找/拼接）

### 4.2 `for` 泛型化（§7/§11.6）

- 把 `for` 从硬编码两条路径（[ast.rs `ForIterable`](../src/ast.rs:168)）重构为「对实现 `Iterator` 的类型展开」。
- 展开：`var it = iterable.into_iter(); loop { match it.next() { Some(x) => { body }, None => break } }`。

### 4.3 错误处理 `?`（§8/§11.7）

- `Result<T,E>` 用泛型枚举实现。
- `?` 仅作用于 `Result`，`expr?` = 「`Err` 提前 return，`Ok` 解包」；错误类型不自动转换。

**验证（验收试金石）**：`for x in my_custom_iter` 能跑通（用户自定义迭代器）；`Vec<i32>.push/pop` 链式可用；`?` 传播在错误路径正确提前返回。新增 `examples/m15` 示例。

---

## 阶段 5：标准库——OS 穿透（第二档）

目标：实现 §6.2.2 文件 I/O（同步）+ 进程（同步）。

### 5.1 runtime 侧导出符号

- [unix_runtime.c](../runtime/unix_runtime.c)：文件 `open`/`read`/`write`/`close`，进程 `spawn`/`wait`。
- [windows_runtime.cpp](../runtime/windows_runtime.cpp)：`CreateFile`/`ReadFile`/`WriteFile`/`CloseHandle`，`CreateProcess`/`WaitForSingleObject`。

### 5.2 用户态高层封装

- `File` 类型 + `open`/`read`/`write`/`close`，同步阻塞。
- 进程 `spawn`/`wait`/`exit_code`。
- 错误经 `Result` 表达。

**验证**：文件读写往返正确、进程启动并取得退出码，错误路径（打开失败）返回 `Err`。Windows 与 Unix 双平台冒烟测试通过。

---

## 阶段 6：文档对齐与收尾

- 更新 [implemented-features.md](../docs/implemented-features.md)：M15 状态置为已完成，补充泛型/契约/方法/标准库章节。
- 更新 [roadmap.md](../docs/roadmap.md)：M15 复选框勾选。
- 新增 `examples/m15` 示例（跨模块泛型、自定义迭代器、文件 I/O）。

**验证**：`cargo test` 全绿；`cargo build --release` 成功；`dc run examples/m15` 通过。

---

## 阶段依赖关系

```
阶段 0 基线
  └─ 阶段 1 泛型语法（词法/AST/解析）
       └─ 阶段 2 单态化 pass（核心）
            ├─ 阶段 3 契约 + 方法（依赖泛型约束）
            │    └─ 阶段 4 纯用户态标准库（依赖契约/方法）
            │         └─ 阶段 5 OS 穿透（文件+进程）
            └──────────────────────────────┘
                        └─ 阶段 6 文档收尾
```

## 风险点

1. **阶段 2 单态化是最大风险**：类型表键结构升级（§3.4.2）、全程序收集实参、稳定 mangling 去重，是本里程碑的硬骨头。建议在阶段 2 单独充分测试后再进入阶段 3。
2. **阶段 3 的 `*Self` 方法解析**：方法查找与自由函数命名空间分离，可能牵动现有 [lower.rs `lower_expr`](../src/lower.rs) 的 `Call` 分支。
3. **阶段 5 跨平台**：文件/进程需 Windows/Unix 双份 runtime 实现，需双平台冒烟测试（现有 CI 已覆盖双平台）。
